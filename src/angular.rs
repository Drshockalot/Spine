use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Instant;
use crate::config::Config;
use crate::error::SpineError;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AngularWorkspace {
    pub version: u8,
    pub projects: HashMap<String, AngularProject>,
    #[serde(rename = "defaultProject")]
    pub default_project: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AngularProject {
    pub root: String,
    #[serde(rename = "sourceRoot")]
    pub source_root: Option<String>,
    #[serde(rename = "projectType")]
    pub project_type: String,
    pub architect: Option<HashMap<String, AngularArchitect>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AngularArchitect {
    pub builder: String,
    pub options: serde_json::Value,
    pub configurations: Option<HashMap<String, serde_json::Value>>,
}

#[derive(Debug, Clone)]
pub struct BuildResult {
    pub library: String,
    pub success: bool,
    pub duration: std::time::Duration,
    pub output: String,
    pub error: Option<String>,
}

pub struct AngularBuildManager {
    workspace: Option<AngularWorkspace>,
    workspace_root: PathBuf,
    config: Config,
}

impl AngularBuildManager {
    pub fn new(config: Config) -> Result<Self> {
        let workspace_root = std::env::current_dir()?;
        let workspace = Self::detect_angular_workspace(&workspace_root)?;
        
        Ok(Self {
            workspace,
            workspace_root,
            config,
        })
    }

    pub fn new_from_linked_package(config: Config, package_name: &str) -> Result<Self> {
        // Try to find the Angular workspace that contains this package
        if let Some(package_link) = config.links.get(package_name) {
            let workspace_root = Self::find_workspace_root_for_package(&package_link.path)?;
            let workspace = Self::detect_angular_workspace(&workspace_root)?;
            
            Ok(Self {
                workspace,
                workspace_root,
                config,
            })
        } else {
            // Fallback to current directory
            Self::new(config)
        }
    }

    pub fn find_workspace_root_for_package(package_path: &PathBuf) -> Result<PathBuf> {
        let mut current_path = package_path.clone();
        
        // Walk up the directory tree looking for angular.json
        loop {
            // Check if this is a dist directory (built output)
            if current_path.file_name().and_then(|n| n.to_str()) == Some("dist") {
                // Go up one more level from dist directory
                if let Some(parent) = current_path.parent() {
                    current_path = parent.to_path_buf();
                }
            }
            
            // Check for angular.json in current directory
            let angular_json = current_path.join("angular.json");
            if angular_json.exists() {
                return Ok(current_path);
            }
            
            // Move up one directory
            match current_path.parent() {
                Some(parent) => current_path = parent.to_path_buf(),
                None => break,
            }
        }
        
        // If we can't find a workspace, return the original path's parent
        Ok(package_path.parent()
            .unwrap_or(package_path)
            .to_path_buf())
    }

    pub fn detect_angular_workspace(root: &Path) -> Result<Option<AngularWorkspace>> {
        let angular_json_path = root.join("angular.json");
        
        if !angular_json_path.exists() {
            return Ok(None);
        }

        let content = fs::read_to_string(&angular_json_path)?;
        let workspace: AngularWorkspace = serde_json::from_str(&content)
            .map_err(|e| SpineError::Config(format!("Invalid angular.json: {}", e)))?;

        Ok(Some(workspace))
    }

    pub fn get_library_projects(&self) -> Vec<String> {
        match &self.workspace {
            Some(workspace) => {
                workspace.projects
                    .iter()
                    .filter(|(_, project)| project.project_type == "library")
                    .map(|(name, _)| name.clone())
                    .collect()
            }
            None => Vec::new(),
        }
    }

    pub fn get_linked_libraries(&self) -> Vec<String> {
        let library_projects = self.get_library_projects();
        let linked_packages: HashSet<String> = self.config.links.keys().cloned().collect();
        
        library_projects
            .into_iter()
            .filter(|lib| linked_packages.contains(lib))
            .collect()
    }

    pub fn resolve_package_to_library_name(&self, package_name: &str) -> Option<String> {
        // First, check if the package name directly matches a library in the workspace
        if self.library_exists(package_name) {
            return Some(package_name.to_string());
        }

        // If not, try to find the library by analyzing the package path
        if let Some(package_link) = self.config.links.get(package_name) {
            if let Some(workspace) = &self.workspace {
                // Check if this package path corresponds to a built library
                for (lib_name, project) in &workspace.projects {
                    if project.project_type == "library" {
                        // Check if the package path looks like it could be the dist output for this library
                        let lib_root = self.workspace_root.join(&project.root);
                        let potential_dist_path = self.workspace_root.join("dist").join(lib_name);
                        
                        // Compare paths (handle symlinks and canonicalization)
                        if let (Ok(package_canonical), Ok(dist_canonical)) = (
                            package_link.path.canonicalize(),
                            potential_dist_path.canonicalize()
                        ) {
                            if package_canonical == dist_canonical {
                                return Some(lib_name.clone());
                            }
                        }
                        
                        // Also check if the package path is within the library source directory
                        if package_link.path.starts_with(&lib_root) {
                            return Some(lib_name.clone());
                        }
                    }
                }
            }
        }

        // If we can't resolve it, return the original package name
        Some(package_name.to_string())
    }

    pub fn build_library(&self, library: &str, watch: bool) -> Result<BuildResult> {
        let start_time = Instant::now();
        
        // Resolve package name to actual library name in workspace
        let actual_library_name = self.resolve_package_to_library_name(library)
            .ok_or_else(|| SpineError::PackageNotFound(format!("Could not resolve package '{}' to a library in the workspace", library)))?;
        
        // Validate library exists in workspace
        if !self.library_exists(&actual_library_name) {
            return Err(SpineError::PackageNotFound(format!("Library '{}' not found in Angular workspace", actual_library_name)).into());
        }

        println!("Building library: {}{}", actual_library_name, if watch { " (watch mode)" } else { "" });

        let mut cmd = Command::new("ng");
        cmd.arg("build")
           .arg(&actual_library_name)
           .current_dir(&self.workspace_root);

        if watch {
            cmd.arg("--watch");
        }

        // Add common Angular library build options
        cmd.args(&["--configuration", "production"]);

        let output = if watch {
            // For watch mode, we need to handle it differently
            self.run_watch_command(cmd, &actual_library_name)?
        } else {
            let result = cmd.output()?;
            let stdout = String::from_utf8_lossy(&result.stdout).to_string();
            let stderr = String::from_utf8_lossy(&result.stderr).to_string();
            
            if result.status.success() {
                println!("✅ Successfully built {}", actual_library_name);
                BuildResult {
                    library: actual_library_name.to_string(),
                    success: true,
                    duration: start_time.elapsed(),
                    output: stdout,
                    error: None,
                }
            } else {
                println!("❌ Failed to build {}", actual_library_name);
                eprintln!("Error: {}", stderr);
                BuildResult {
                    library: actual_library_name.to_string(),
                    success: false,
                    duration: start_time.elapsed(),
                    output: stdout,
                    error: Some(stderr),
                }
            }
        };

        Ok(output)
    }

    pub fn build_all_libraries(&self) -> Result<Vec<BuildResult>> {
        let libraries = self.get_linked_libraries();
        
        if libraries.is_empty() {
            println!("No linked libraries found to build");
            return Ok(Vec::new());
        }

        println!("Building {} linked libraries...", libraries.len());
        let mut results = Vec::new();

        for library in libraries {
            let result = self.build_library(&library, false)?;
            results.push(result);
        }

        // Summary
        let successful = results.iter().filter(|r| r.success).count();
        let failed = results.len() - successful;
        
        println!("\n📊 Build Summary:");
        println!("  ✅ Successful: {}", successful);
        if failed > 0 {
            println!("  ❌ Failed: {}", failed);
        }

        Ok(results)
    }

    pub fn build_affected_libraries(&self) -> Result<Vec<BuildResult>> {
        println!("Detecting affected libraries...");
        
        let affected_libs = self.detect_affected_libraries()?;
        
        if affected_libs.is_empty() {
            println!("No affected libraries detected");
            return Ok(Vec::new());
        }

        println!("Found {} affected libraries: {}", affected_libs.len(), affected_libs.join(", "));
        let mut results = Vec::new();

        for library in affected_libs {
            let result = self.build_library(&library, false)?;
            results.push(result);
        }

        Ok(results)
    }

    fn detect_affected_libraries(&self) -> Result<Vec<String>> {
        // Check if git is available and we're in a git repository
        let git_check = Command::new("git")
            .args(&["rev-parse", "--git-dir"])
            .current_dir(&self.workspace_root)
            .output();

        if git_check.is_err() {
            // Fallback: build all linked libraries
            println!("Git not available, falling back to building all linked libraries");
            return Ok(self.get_linked_libraries());
        }

        // Get changed files since last commit
        let output = Command::new("git")
            .args(&["diff", "--name-only", "HEAD~1..HEAD"])
            .current_dir(&self.workspace_root)
            .output()?;

        let changed_files: HashSet<String> = String::from_utf8_lossy(&output.stdout)
            .lines()
            .map(|s| s.to_string())
            .collect();

        if changed_files.is_empty() {
            // Check staged files if no committed changes
            let staged_output = Command::new("git")
                .args(&["diff", "--name-only", "--cached"])
                .current_dir(&self.workspace_root)
                .output()?;

            let staged_files: HashSet<String> = String::from_utf8_lossy(&staged_output.stdout)
                .lines()
                .map(|s| s.to_string())
                .collect();

            if staged_files.is_empty() {
                // Check working directory changes
                let working_output = Command::new("git")
                    .args(&["diff", "--name-only"])
                    .current_dir(&self.workspace_root)
                    .output()?;

                return Ok(self.get_affected_from_files(&String::from_utf8_lossy(&working_output.stdout)));
            } else {
                return Ok(self.get_affected_from_files(&staged_files.iter().cloned().collect::<Vec<_>>().join("\n")));
            }
        }

        Ok(self.get_affected_from_files(&changed_files.iter().cloned().collect::<Vec<_>>().join("\n")))
    }

    fn get_affected_from_files(&self, files_content: &str) -> Vec<String> {
        let changed_files: HashSet<String> = files_content
            .lines()
            .map(|s| s.to_string())
            .collect();

        let _library_projects = self.get_library_projects();
        let linked_libraries = self.get_linked_libraries();
        let mut affected = HashSet::new();

        // Check each linked library
        for library in &linked_libraries {
            if let Some(workspace) = &self.workspace {
                if let Some(project) = workspace.projects.get(library) {
                    let lib_root = &project.root;
                    
                    // Check if any changed files are in this library's directory
                    for file in &changed_files {
                        if file.starts_with(lib_root) {
                            affected.insert(library.clone());
                            break;
                        }
                    }
                }
            }
        }

        // Also check for dependency changes that might affect libraries
        for file in &changed_files {
            if file == "package.json" || file == "package-lock.json" || file.ends_with("/package.json") {
                // If package.json changed, potentially all libraries are affected
                affected.extend(linked_libraries.iter().cloned());
                break;
            }
        }

        affected.into_iter().collect()
    }

    fn run_watch_command(&self, mut cmd: Command, library: &str) -> Result<BuildResult> {
        println!("🔄 Starting watch mode for {}...", library);
        println!("Press Ctrl+C to stop watching");

        cmd.stdout(Stdio::inherit())
           .stderr(Stdio::inherit())
           .stdin(Stdio::null());

        let start_time = Instant::now();
        let status = cmd.status()?;

        Ok(BuildResult {
            library: library.to_string(),
            success: status.success(),
            duration: start_time.elapsed(),
            output: "Watch mode completed".to_string(),
            error: if status.success() { None } else { Some("Watch mode terminated with error".to_string()) },
        })
    }

    fn library_exists(&self, library: &str) -> bool {
        match &self.workspace {
            Some(workspace) => {
                workspace.projects.get(library)
                    .map(|p| p.project_type == "library")
                    .unwrap_or(false)
            }
            None => false,
        }
    }

    pub fn get_build_dependencies(&self, library: &str) -> Result<Vec<String>> {
        // Read the library's package.json to get dependencies
        let lib_path = self.get_library_path(library)?;
        let package_json_path = lib_path.join("package.json");
        
        if !package_json_path.exists() {
            return Ok(Vec::new());
        }

        let content = fs::read_to_string(&package_json_path)?;
        let package_json: serde_json::Value = serde_json::from_str(&content)?;
        
        let mut deps = Vec::new();
        
        // Check dependencies and peerDependencies
        if let Some(dependencies) = package_json.get("dependencies").and_then(|d| d.as_object()) {
            for (dep_name, _) in dependencies {
                if self.library_exists(dep_name) {
                    deps.push(dep_name.clone());
                }
            }
        }
        
        if let Some(peer_deps) = package_json.get("peerDependencies").and_then(|d| d.as_object()) {
            for (dep_name, _) in peer_deps {
                if self.library_exists(dep_name) {
                    deps.push(dep_name.clone());
                }
            }
        }

        Ok(deps)
    }

    fn get_library_path(&self, library: &str) -> Result<PathBuf> {
        match &self.workspace {
            Some(workspace) => {
                if let Some(project) = workspace.projects.get(library) {
                    Ok(self.workspace_root.join(&project.root))
                } else {
                    Err(SpineError::PackageNotFound(format!("Library '{}' not found", library)).into())
                }
            }
            None => Err(SpineError::Config("No Angular workspace detected".to_string()).into()),
        }
    }

    pub fn show_build_status(&self) -> Result<()> {
        let _workspace = self.workspace.as_ref()
            .ok_or_else(|| SpineError::Config("No Angular workspace detected".to_string()))?;

        println!("🏗️  Angular Build Status");
        println!("========================");
        
        let library_projects = self.get_library_projects();
        let linked_libraries = self.get_linked_libraries();
        
        println!("📚 Total libraries in workspace: {}", library_projects.len());
        println!("🔗 Linked libraries: {}", linked_libraries.len());
        
        if !linked_libraries.is_empty() {
            println!("\n🔗 Linked Libraries:");
            for lib in &linked_libraries {
                let deps = self.get_build_dependencies(lib).unwrap_or_default();
                if deps.is_empty() {
                    println!("  📦 {}", lib);
                } else {
                    println!("  📦 {} (depends on: {})", lib, deps.join(", "));
                }
            }
        }

        let unlinked: Vec<_> = library_projects
            .iter()
            .filter(|lib| !linked_libraries.contains(lib))
            .collect();

        if !unlinked.is_empty() {
            println!("\n📚 Unlinked Libraries:");
            for lib in unlinked {
                println!("  📖 {}", lib);
            }
        }

        Ok(())
    }
}

pub fn build_command(library: Option<String>, all: bool, watch: bool, affected: bool) -> Result<()> {
    let config = Config::load_or_create()?;
    
    // If we're building a specific library, try to find its workspace
    let build_manager = if let Some(ref lib_name) = library {
        // Try to create build manager from the linked package's workspace
        match AngularBuildManager::new_from_linked_package(config.clone(), lib_name) {
            Ok(manager) if manager.workspace.is_some() => manager,
            _ => {
                // Fallback to current directory
                let manager = AngularBuildManager::new(config)?;
                if manager.workspace.is_none() {
                    return Err(SpineError::Config(
                        format!("No Angular workspace detected for library '{}'. Make sure you're in an Angular project directory with angular.json, or that the package is linked to a path within an Angular workspace.", lib_name)
                    ).into());
                }
                manager
            }
        }
    } else {
        // For --all or --affected, use current directory
        let manager = AngularBuildManager::new(config)?;
        if manager.workspace.is_none() {
            return Err(SpineError::Config("No Angular workspace detected. Make sure you're in an Angular project directory with angular.json".to_string()).into());
        }
        manager
    };

    match (library, all, affected) {
        (Some(lib), false, false) => {
            build_manager.build_library(&lib, watch)?;
        }
        (None, true, false) => {
            if watch {
                return Err(SpineError::Config("Watch mode is not supported with --all. Use individual library builds for watch mode.".to_string()).into());
            }
            build_manager.build_all_libraries()?;
        }
        (None, false, true) => {
            if watch {
                return Err(SpineError::Config("Watch mode is not supported with --affected. Use individual library builds for watch mode.".to_string()).into());
            }
            build_manager.build_affected_libraries()?;
        }
        (None, false, false) => {
            // Show status if no specific action requested
            build_manager.show_build_status()?;
        }
        _ => {
            return Err(SpineError::Config("Invalid combination of build options".to_string()).into());
        }
    }

    Ok(())
}

pub fn publish_command(config: &Config, package_name: &str, skip_build: bool, dry_run: bool) -> Result<()> {
    // Verify the package exists in config
    let package_link = config.links.get(package_name)
        .ok_or_else(|| SpineError::PackageNotFound(format!("Package '{}' not found in Spine configuration. Use 'spine add' to add it first.", package_name)))?;

    // Create build manager to find the workspace for this package
    let build_manager = AngularBuildManager::new_from_linked_package(config.clone(), package_name)?;
    
    if build_manager.workspace.is_none() {
        return Err(SpineError::Config(
            format!("No Angular workspace detected for package '{}'. Make sure the package is in an Angular workspace.", package_name)
        ).into());
    }

    // Resolve package name to library name
    let library_name = build_manager.resolve_package_to_library_name(package_name)
        .ok_or_else(|| SpineError::PackageNotFound(format!("Could not resolve package '{}' to a library in the workspace", package_name)))?;

    // Step 1: Build the package (unless skipped)
    if !skip_build {
        println!("📦 Building package: {}", library_name);
        let build_result = build_manager.build_library(&library_name, false)?;
        
        if !build_result.success {
            return Err(SpineError::Config(
                format!("Build failed for package '{}'. Cannot proceed with publishing.", package_name)
            ).into());
        }
        
        println!("✅ Build completed successfully");
    } else {
        println!("⏭️  Skipping build step");
    }

    // Step 2: Find the built package directory
    let publish_dir = find_publish_directory(&build_manager, &library_name, &package_link.path)?;
    
    println!("📂 Publishing from directory: {}", publish_dir.display());

    // Verify package.json exists in publish directory
    let package_json_path = publish_dir.join("package.json");
    if !package_json_path.exists() {
        return Err(SpineError::Config(
            format!("No package.json found in publish directory: {}", publish_dir.display())
        ).into());
    }

    // Step 3: Run npm publish
    let mut cmd = Command::new("npm");
    cmd.arg("publish")
       .current_dir(&publish_dir);

    if dry_run {
        cmd.arg("--dry-run");
        println!("🔍 Running npm publish --dry-run");
    } else {
        println!("🚀 Publishing package to npm");
    }

    let output = cmd.output()?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    if output.status.success() {
        if dry_run {
            println!("✅ Dry run completed successfully");
            println!("📄 Package would be published with the following details:");
        } else {
            println!("✅ Package published successfully!");
        }
        
        if !stdout.is_empty() {
            println!("{}", stdout);
        }
    } else {
        println!("❌ npm publish failed");
        if !stderr.is_empty() {
            eprintln!("Error: {}", stderr);
        }
        if !stdout.is_empty() {
            println!("Output: {}", stdout);
        }
        return Err(SpineError::Config("npm publish command failed".to_string()).into());
    }

    Ok(())
}

fn find_publish_directory(build_manager: &AngularBuildManager, library_name: &str, package_path: &PathBuf) -> Result<PathBuf> {
    // First, try to use the package path directly if it contains a package.json
    if package_path.join("package.json").exists() {
        return Ok(package_path.clone());
    }

    // If not, try to find the dist output directory
    let workspace_root = &build_manager.workspace_root;
    
    // Common Angular dist patterns
    let possible_dist_paths = vec![
        workspace_root.join("dist").join(library_name),
        workspace_root.join("dist").join("libs").join(library_name),
        workspace_root.join("projects").join(library_name).join("dist"),
    ];

    for dist_path in possible_dist_paths {
        if dist_path.exists() && dist_path.join("package.json").exists() {
            return Ok(dist_path);
        }
    }

    // If we still can't find it, try to get the library's architect build output path
    if let Some(workspace) = &build_manager.workspace {
        if let Some(project) = workspace.projects.get(library_name) {
            if let Some(architect) = &project.architect {
                if let Some(build_config) = architect.get("build") {
                    if let Some(options) = build_config.options.as_object() {
                        if let Some(output_path) = options.get("outputPath").and_then(|v| v.as_str()) {
                            let full_output_path = workspace_root.join(output_path);
                            if full_output_path.exists() && full_output_path.join("package.json").exists() {
                                return Ok(full_output_path);
                            }
                        }
                    }
                }
            }
        }
    }

    Err(SpineError::Config(
        format!("Could not find built package directory for '{}'. Make sure the package has been built.", library_name)
    ).into())
}