use anyhow::Result;
use indicatif::{ProgressBar, ProgressStyle};
use serde_json;
use std::fs;
use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::Duration;
use crate::angular::{AngularBuildManager, AngularWorkspace};
use crate::config::Config;
use crate::error::SpineError;
use crate::platform::Platform;

pub struct AngularCliIntegration {
    workspace: AngularWorkspace,
    config: Config,
    workspace_root: PathBuf,
}

impl AngularCliIntegration {
    pub fn new(config: Config, workspace_root: PathBuf) -> Result<Self> {
        let workspace = AngularBuildManager::detect_angular_workspace(&workspace_root)?
            .ok_or_else(|| SpineError::angular_workspace_not_found(&workspace_root.display().to_string()))?;

        Ok(Self {
            workspace,
            config,
            workspace_root,
        })
    }

    pub fn generate_with_lib_context(
        &self,
        schematic: &str,
        name: &str,
        lib: Option<&str>,
        args: Vec<String>,
    ) -> Result<()> {
        let mut cmd = Platform::ng_command();
        cmd.arg("generate")
           .arg(schematic)
           .arg(name)
           .current_dir(&self.workspace_root);

        // If library is specified, add project context
        if let Some(library) = lib {
            // Validate the library exists and is linked
            self.validate_library_exists(library)?;
            
            // Resolve library to actual project name
            let project_name = self.resolve_library_project_name(library)?;
            cmd.args(&["--project", &project_name]);

            // Add context-aware arguments based on library analysis
            if schematic == "component" {
                self.add_component_context(&mut cmd, library)?;
            } else if schematic == "service" {
                self.add_service_context(&mut cmd, library)?;
            }

            println!("🎯 Generating {} '{}' in library '{}'", schematic, name, library);
        } else {
            println!("🎯 Generating {} '{}'", schematic, name);
        }

        // Add user-provided arguments
        cmd.args(args);

        // Execute with enhanced output
        self.execute_with_context(cmd, lib)
    }

    fn validate_library_exists(&self, lib: &str) -> Result<()> {
        if !self.config.links.contains_key(lib) {
            let available: Vec<String> = self.config.links.keys().cloned().collect();
            return Err(SpineError::package_not_found_with_suggestions(lib, &available).into());
        }

        // Check if library exists in Angular workspace
        let library_exists = self.workspace.projects
            .iter()
            .any(|(name, project)| name == lib && project.project_type == "library");

        if !library_exists {
            let available_libs: Vec<String> = self.workspace.projects
                .iter()
                .filter(|(_, project)| project.project_type == "library")
                .map(|(name, _)| name.clone())
                .collect();
            
            let suggestion = if available_libs.is_empty() {
                "No libraries found in Angular workspace. Create one with 'ng generate library <name>'.".to_string()
            } else {
                format!("Available libraries in workspace: {}", available_libs.join(", "))
            };
            
            return Err(SpineError::AngularWorkspace {
                message: format!("Library '{}' not found in Angular workspace", lib),
                suggestion,
            }.into());
        }

        Ok(())
    }

    fn resolve_library_project_name(&self, lib: &str) -> Result<String> {
        // For now, assume library name matches project name
        // This could be enhanced to handle more complex mappings
        Ok(lib.to_string())
    }

    fn add_component_context(&self, cmd: &mut Command, library: &str) -> Result<()> {
        // Check if library uses standalone components
        if self.uses_standalone_components(library)? {
            cmd.arg("--standalone");
            println!("  📦 Using standalone component");
        }

        // Detect and use library's style extension
        if let Some(style_ext) = self.detect_style_extension(library)? {
            cmd.args(&["--style", &style_ext]);
            println!("  🎨 Using {} styles", style_ext);
        }

        // Add change detection strategy for better performance
        cmd.args(&["--change-detection", "OnPush"]);

        Ok(())
    }

    fn add_service_context(&self, _cmd: &mut Command, library: &str) -> Result<()> {
        // Check if library has a public API file for service exports
        let lib_path = self.get_library_source_path(library)?;
        let public_api_path = lib_path.join("public-api.ts");
        
        if public_api_path.exists() {
            println!("  📤 Remember to export service in public-api.ts");
        }

        Ok(())
    }

    fn uses_standalone_components(&self, lib: &str) -> Result<bool> {
        let lib_path = self.get_library_source_path(lib)?;
        let package_json_path = lib_path.join("package.json");

        if package_json_path.exists() {
            let content = fs::read_to_string(&package_json_path)?;
            let package_json: serde_json::Value = serde_json::from_str(&content)?;

            // Check Angular version - standalone available in v14+
            if let Some(ng_version) = package_json.get("peerDependencies")
                .and_then(|deps| deps.get("@angular/core"))
                .and_then(|v| v.as_str()) {
                
                return Ok(self.is_angular_version_14_plus(ng_version));
            }
        }

        // Also check for existing standalone components in the library
        self.has_existing_standalone_components(lib)
    }

    fn detect_style_extension(&self, lib: &str) -> Result<Option<String>> {
        let lib_path = self.get_library_source_path(lib)?;
        
        // Look for existing component files to detect style preference
        let component_files = self.find_component_files(&lib_path)?;
        
        for file in component_files {
            if file.ends_with(".component.scss") {
                return Ok(Some("scss".to_string()));
            } else if file.ends_with(".component.sass") {
                return Ok(Some("sass".to_string()));
            } else if file.ends_with(".component.less") {
                return Ok(Some("less".to_string()));
            }
        }

        // Check angular.json for default style extension
        if let Some(project) = self.workspace.projects.get(lib) {
            if let Some(architect) = &project.architect {
                if let Some(build_config) = architect.get("build") {
                    if let Some(style_ext) = build_config.options.get("styleExt") {
                        if let Some(ext) = style_ext.as_str() {
                            return Ok(Some(ext.to_string()));
                        }
                    }
                }
            }
        }

        Ok(Some("css".to_string()))
    }

    fn get_library_source_path(&self, lib: &str) -> Result<PathBuf> {
        if let Some(project) = self.workspace.projects.get(lib) {
            let source_root = if let Some(src_root) = &project.source_root {
                src_root.clone()
            } else {
                format!("{}/src", project.root)
            };
            Ok(self.workspace_root.join(source_root))
        } else {
            Err(SpineError::PackageNotFound(format!("Library '{}' not found in workspace", lib)).into())
        }
    }

    fn find_component_files(&self, lib_path: &PathBuf) -> Result<Vec<String>> {
        let mut component_files = Vec::new();
        
        if let Ok(entries) = fs::read_dir(lib_path) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_file() {
                    if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                        if name.contains(".component.") {
                            component_files.push(name.to_string());
                        }
                    }
                } else if path.is_dir() {
                    // Recursively search subdirectories
                    if let Ok(mut sub_files) = self.find_component_files(&path) {
                        component_files.append(&mut sub_files);
                    }
                }
            }
        }
        
        Ok(component_files)
    }

    fn is_angular_version_14_plus(&self, version_spec: &str) -> bool {
        // Parse version specification (e.g., "^17.0.0", ">=14.0.0")
        let version_num = version_spec
            .chars()
            .filter(|c| c.is_ascii_digit() || *c == '.')
            .collect::<String>();
            
        if let Some(major_version) = version_num.split('.').next() {
            if let Ok(major) = major_version.parse::<u32>() {
                return major >= 14;
            }
        }
        
        false
    }

    fn has_existing_standalone_components(&self, lib: &str) -> Result<bool> {
        let lib_path = self.get_library_source_path(lib)?;
        let component_files = self.find_component_files(&lib_path)?;
        
        for file in component_files {
            let file_path = lib_path.join(&file);
            if let Ok(content) = fs::read_to_string(&file_path) {
                if content.contains("standalone: true") {
                    return Ok(true);
                }
            }
        }
        
        Ok(false)
    }

    fn execute_with_context(&self, mut cmd: Command, lib: Option<&str>) -> Result<()> {
        // Add environment variables for better Angular CLI experience
        cmd.env("NG_CLI_ANALYTICS", "false"); // Disable analytics prompts
        
        if let Some(library) = lib {
            cmd.env("SPINE_TARGET_LIBRARY", library);
        }

        // Create progress spinner for generation
        let spinner = ProgressBar::new_spinner();
        spinner.set_style(
            ProgressStyle::default_spinner()
                .tick_strings(&["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"])
                .template("{spinner:.blue} {msg}")
                .unwrap()
        );
        
        if let Some(library) = lib {
            spinner.set_message(format!("Generating in library '{}'...", library));
        } else {
            spinner.set_message("Generating...");
        }
        spinner.enable_steady_tick(Duration::from_millis(100));

        let status = cmd.status()?;
        
        if status.success() {
            spinner.finish_with_message("✅ Generation completed successfully");
            
            if let Some(library) = lib {
                println!("💡 Next steps:");
                println!("  • Check the generated files in projects/{}", library);
                println!("  • Update public-api.ts if needed");
                println!("  • Run 'spine build {}' to build the library", library);
            }
        } else {
            spinner.finish_with_message("❌ Generation failed");
            return Err(SpineError::Config("Angular CLI command failed".to_string()).into());
        }

        Ok(())
    }
}

pub struct NgProxy {
    spine_config: Config,
    workspace_root: PathBuf,
}

impl NgProxy {
    pub fn new(config: Config, workspace_root: PathBuf) -> Self {
        Self {
            spine_config: config,
            workspace_root,
        }
    }

    pub fn proxy_command(&self, args: Vec<String>) -> Result<()> {
        if args.is_empty() {
            return Err(SpineError::Config("No Angular CLI command provided".to_string()).into());
        }

        println!("🔄 Proxying Angular CLI command with Spine enhancements...");
        
        let enhanced_args = self.enhance_ng_command(args)?;
        
        let mut cmd = Platform::ng_command();
        cmd.args(enhanced_args)
           .current_dir(&self.workspace_root)
           .env("NG_CLI_ANALYTICS", "false");

        let status = cmd.status()?;
        
        if !status.success() {
            return Err(SpineError::Config("Angular CLI command failed".to_string()).into());
        }

        Ok(())
    }

    fn enhance_ng_command(&self, args: Vec<String>) -> Result<Vec<String>> {
        let mut enhanced = args.clone();
        
        match args[0].as_str() {
            "build" => {
                enhanced = self.enhance_build_command(args)?;
            }
            "test" => {
                enhanced = self.enhance_test_command(args)?;
            }
            "serve" => {
                enhanced = self.enhance_serve_command(args)?;
            }
            "generate" => {
                enhanced = self.enhance_generate_command(args)?;
            }
            _ => {
                println!("  📝 Passing through command as-is");
            }
        }
        
        Ok(enhanced)
    }

    fn enhance_build_command(&self, args: Vec<String>) -> Result<Vec<String>> {
        let mut enhanced = args;
        
        if enhanced.len() > 1 {
            let target = &enhanced[1];
            if self.spine_config.links.contains_key(target) {
                println!("  🔗 Building linked library: {}", target);
                
                // Add production configuration for linked libraries if not specified
                if !enhanced.iter().any(|arg| arg == "--configuration") {
                    enhanced.push("--configuration".to_string());
                    enhanced.push("production".to_string());
                    println!("  ⚙️  Using production configuration");
                }
                
                // Add source map for development debugging
                if !enhanced.iter().any(|arg| arg == "--source-map") {
                    enhanced.push("--source-map".to_string());
                    println!("  🗺️  Enabled source maps for debugging");
                }
            }
        }
        
        Ok(enhanced)
    }

    fn enhance_test_command(&self, args: Vec<String>) -> Result<Vec<String>> {
        let mut enhanced = args;
        
        if enhanced.len() > 1 {
            let target = &enhanced[1];
            if self.spine_config.links.contains_key(target) {
                println!("  🧪 Testing linked library: {}", target);
                
                // Add code coverage for linked libraries
                if !enhanced.iter().any(|arg| arg == "--code-coverage") {
                    enhanced.push("--code-coverage".to_string());
                    println!("  📊 Enabled code coverage");
                }
            }
        }
        
        Ok(enhanced)
    }

    fn enhance_serve_command(&self, args: Vec<String>) -> Result<Vec<String>> {
        let mut enhanced = args;
        
        // Auto-enable useful development options
        if !enhanced.iter().any(|arg| arg == "--host") {
            enhanced.push("--host".to_string());
            enhanced.push("0.0.0.0".to_string());
            println!("  🌐 Enabled network access (host: 0.0.0.0)");
        }
        
        if !enhanced.iter().any(|arg| arg == "--live-reload") {
            enhanced.push("--live-reload".to_string());
            println!("  🔄 Enabled live reload");
        }

        // Enable HMR if there are linked libraries
        if !self.spine_config.links.is_empty() && !enhanced.iter().any(|arg| arg == "--hmr") {
            enhanced.push("--hmr".to_string());
            println!("  🔥 Enabled HMR for {} linked libraries", self.spine_config.links.len());
        }
        
        Ok(enhanced)
    }

    fn enhance_generate_command(&self, args: Vec<String>) -> Result<Vec<String>> {
        let enhanced = args;
        println!("  🎯 Use 'spine ng generate' for enhanced library context");
        Ok(enhanced)
    }
}

pub struct LibraryWatchServer {
    workspace_root: PathBuf,
    linked_libraries: Vec<LibraryWatchInfo>,
    app_project: String,
    processes: Vec<Child>,
}

#[derive(Debug, Clone)]
struct LibraryWatchInfo {
    library_name: String,
    workspace_root: PathBuf,
    package_name: String,
}

// Helper function to get packages linked to a specific project
fn get_linked_packages_for_project(config: &Config, project_path: &PathBuf) -> Result<Vec<String>> {
    let mut linked_packages = Vec::new();
    let project_canonical = project_path.canonicalize()?;
    
    for (package_name, package_link) in &config.links {
        // Check if this package is linked to the current project
        for linked_project in &package_link.linked_projects {
            if let Ok(linked_canonical) = linked_project.canonicalize() {
                if linked_canonical == project_canonical {
                    linked_packages.push(package_name.clone());
                    break;
                }
            }
        }
    }
    
    Ok(linked_packages)
}

impl LibraryWatchServer {
    fn get_linked_packages_for_project(config: &Config, project_path: &PathBuf) -> Result<Vec<String>> {
        let linked_packages = get_linked_packages_for_project(config, project_path)?;
        
        // Only show debug info if there are linked packages
        if !linked_packages.is_empty() {
            println!("🔗 Found {} packages linked to current project:", linked_packages.len());
            for pkg in &linked_packages {
                println!("  • {}", pkg);
            }
        }
        
        Ok(linked_packages)
    }

    fn get_configured_port(&self) -> Option<u16> {
        // Try to read port from angular.json for the app project
        let angular_json_path = self.workspace_root.join("angular.json");
        
        if let Ok(content) = std::fs::read_to_string(&angular_json_path) {
            if let Ok(workspace_config) = serde_json::from_str::<serde_json::Value>(&content) {
                // Navigate to projects -> app_project -> architect -> serve -> options -> port
                let port = workspace_config
                    .get("projects")
                    .and_then(|projects| projects.get(&self.app_project))
                    .and_then(|project| project.get("architect"))
                    .and_then(|architect| architect.get("serve"))
                    .and_then(|serve| serve.get("options"))
                    .and_then(|options| options.get("port"))
                    .and_then(|port| port.as_u64())
                    .and_then(|port| u16::try_from(port).ok());
                
                if let Some(p) = port {
                    println!("📡 Using port {} from angular.json", p);
                    return Some(p);
                }
                
                // Also check configurations -> development -> port (for newer Angular CLI)
                let dev_port = workspace_config
                    .get("projects")
                    .and_then(|projects| projects.get(&self.app_project))
                    .and_then(|project| project.get("architect"))
                    .and_then(|architect| architect.get("serve"))
                    .and_then(|serve| serve.get("configurations"))
                    .and_then(|configs| configs.get("development"))
                    .and_then(|dev| dev.get("port"))
                    .and_then(|port| port.as_u64())
                    .and_then(|port| u16::try_from(port).ok());
                    
                if let Some(p) = dev_port {
                    println!("📡 Using port {} from angular.json (development config)", p);
                    return Some(p);
                }
            }
        }
        
        println!("📡 No port configured in angular.json, using default 4200");
        None
    }

    pub fn new(config: &Config, workspace_root: PathBuf) -> Result<Self> {
        // First try current directory for workspace
        let mut detected_workspace_root = workspace_root.clone();
        let mut workspace = AngularBuildManager::detect_angular_workspace(&workspace_root)?;
        
        // If no workspace in current directory, try to find workspace from linked packages
        if workspace.is_none() && !config.links.is_empty() {
            println!("🔍 No Angular workspace in current directory, searching from linked packages...");
            
            // Try to find workspace from any linked package
            for (package_name, package_link) in &config.links {
                match AngularBuildManager::find_workspace_root_for_package(&package_link.path) {
                    Ok(found_workspace_root) => {
                        if let Ok(Some(found_workspace)) = AngularBuildManager::detect_angular_workspace(&found_workspace_root) {
                            println!("✅ Found Angular workspace from package '{}': {}", package_name, found_workspace_root.display());
                            detected_workspace_root = found_workspace_root;
                            workspace = Some(found_workspace);
                            break;
                        }
                    }
                    Err(_) => continue,
                }
            }
        }
        
        let workspace = workspace
            .ok_or_else(|| SpineError::Config("No Angular workspace detected in current directory or linked package paths".to_string()))?;

        // Get only packages that are actually linked to this project
        let linked_package_names = Self::get_linked_packages_for_project(config, &detected_workspace_root)?;
        
        // Get linked libraries - handle both local and cross-workspace libraries
        let mut linked_libraries = Vec::new();
        
        for package_name in &linked_package_names {
            if let Some(package_link) = config.links.get(package_name) {
                // First try to find library in current workspace
                let mut _found_in_current_workspace = false;
                
                // Try direct name match first
                if workspace.projects
                    .get(package_name)
                    .map(|p| p.project_type == "library")
                    .unwrap_or(false) {
                    linked_libraries.push(LibraryWatchInfo {
                        library_name: package_name.clone(),
                        workspace_root: detected_workspace_root.clone(),
                        package_name: package_name.clone(),
                    });
                    _found_in_current_workspace = true;
                    continue;
                }
                
                // Try to resolve package to library name in current workspace
                for (lib_name, project) in &workspace.projects {
                    if project.project_type == "library" {
                        // Check if the package path corresponds to this library's dist output
                        let potential_dist_path = detected_workspace_root.join("dist").join(lib_name);
                        
                        // Compare paths (handle symlinks and canonicalization)
                        if let (Ok(package_canonical), Ok(dist_canonical)) = (
                            package_link.path.canonicalize(),
                            potential_dist_path.canonicalize()
                        ) {
                            if package_canonical == dist_canonical {
                                linked_libraries.push(LibraryWatchInfo {
                                    library_name: lib_name.clone(),
                                    workspace_root: detected_workspace_root.clone(),
                                    package_name: package_name.clone(),
                                });
                                println!("🔗 Mapped package '{}' -> workspace library '{}'", package_name, lib_name);
                                _found_in_current_workspace = true;
                                break;
                            }
                        }
                        
                        // Also check if package path is within library source directory
                        let lib_root = detected_workspace_root.join(&project.root);
                        if package_link.path.starts_with(&lib_root) {
                            linked_libraries.push(LibraryWatchInfo {
                                library_name: lib_name.clone(),
                                workspace_root: detected_workspace_root.clone(),
                                package_name: package_name.clone(),
                            });
                            println!("🔗 Mapped package '{}' -> workspace library '{}'", package_name, lib_name);
                            _found_in_current_workspace = true;
                            break;
                        }
                    }
                }
                
                // If not found in current workspace, try to find the library's own workspace
                if !_found_in_current_workspace {
                    match AngularBuildManager::find_workspace_root_for_package(&package_link.path) {
                        Ok(lib_workspace_root) => {
                            if let Ok(Some(lib_workspace)) = AngularBuildManager::detect_angular_workspace(&lib_workspace_root) {
                                // Look for library in its own workspace
                                for (lib_name, project) in &lib_workspace.projects {
                                    if project.project_type == "library" {
                                        // Check if the package path corresponds to this library's dist output
                                        let potential_dist_path = lib_workspace_root.join("dist").join(lib_name);
                                        
                                        if let (Ok(package_canonical), Ok(dist_canonical)) = (
                                            package_link.path.canonicalize(),
                                            potential_dist_path.canonicalize()
                                        ) {
                                            if package_canonical == dist_canonical {
                                                linked_libraries.push(LibraryWatchInfo {
                                                    library_name: lib_name.clone(),
                                                    workspace_root: lib_workspace_root.clone(),
                                                    package_name: package_name.clone(),
                                                });
                                                println!("🔗 Mapped cross-workspace package '{}' -> library '{}' in {}", 
                                                         package_name, lib_name, lib_workspace_root.display());
                                                break;
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        Err(_) => {
                            println!("⚠️  Could not find workspace for package '{}'", package_name);
                        }
                    }
                }
            }
        }

        // Find the default application project
        let app_project = workspace.default_project
            .or_else(|| {
                workspace.projects
                    .iter()
                    .find(|(_, project)| project.project_type == "application")
                    .map(|(name, _)| name.clone())
            })
            .ok_or_else(|| SpineError::Config("No application project found in workspace".to_string()))?;

        Ok(Self {
            workspace_root: detected_workspace_root,
            linked_libraries,
            app_project,
            processes: Vec::new(),
        })
    }

    pub fn serve_with_libraries(&mut self, port: Option<u16>, hmr: bool) -> Result<()> {
        // Get port from angular.json if not specified
        let port = port.unwrap_or_else(|| self.get_configured_port().unwrap_or(4200));
        
        // Create main progress spinner
        let main_spinner = ProgressBar::new_spinner();
        main_spinner.set_style(
            ProgressStyle::default_spinner()
                .tick_strings(&["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"])
                .template("{spinner:.blue} {msg}")
                .unwrap()
        );
        
        main_spinner.set_message("🚀 Initializing development server...");
        main_spinner.enable_steady_tick(Duration::from_millis(100));
        
        // Check for linked libraries
        if self.linked_libraries.is_empty() {
            main_spinner.finish_with_message("⚠️  No linked libraries found - running regular serve");
            println!("💡 This could mean:");
            println!("   • No packages are linked to this project");
            println!("   • Package names don't match Angular library names");
            println!("   • Libraries aren't marked as 'library' type in angular.json");
            return Ok(());
        }
        
        main_spinner.set_message(format!("📚 Found {} linked libraries", self.linked_libraries.len()));
        thread::sleep(Duration::from_millis(500));
        
        // Show library details (briefly)
        for lib_info in &self.linked_libraries {
            main_spinner.set_message(format!("🔗 {}", lib_info.package_name));
            thread::sleep(Duration::from_millis(200));
        }

        // 1. Start library watchers
        main_spinner.set_message("🔧 Starting library watchers...");
        self.start_library_watchers()?;
        thread::sleep(Duration::from_millis(500));

        // 2. Wait for initial library builds to complete
        main_spinner.finish_with_message("✅ Library watchers started");
        
        if !self.linked_libraries.is_empty() {
            self.wait_for_initial_builds()?;
        }

        // 3. Start the main application server
        let app_spinner = ProgressBar::new_spinner();
        app_spinner.set_style(
            ProgressStyle::default_spinner()
                .tick_strings(&["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"])
                .template("{spinner:.green} {msg}")
                .unwrap()
        );
        app_spinner.set_message(format!("🌐 Starting application server on port {}...", port));
        app_spinner.enable_steady_tick(Duration::from_millis(100));
        
        self.start_app_server(port, hmr)?;
        
        app_spinner.finish_with_message(format!("✅ Development server running at http://localhost:{}", port));
        
        // 4. Monitor and coordinate rebuilds
        self.coordinate_rebuilds()
    }

    fn start_library_watchers(&mut self) -> Result<()> {
        for lib_info in &self.linked_libraries {
            let mut cmd = Platform::ng_command();
            cmd.args(&["build", &lib_info.library_name, "--watch"])
               .current_dir(&lib_info.workspace_root)
               .stdout(Stdio::piped())
               .stderr(Stdio::piped())
               .env("NG_CLI_ANALYTICS", "false");

            let child = cmd.spawn()
                .map_err(|e| SpineError::Config(format!("Failed to start library watcher for {}: {}", lib_info.library_name, e)))?;
            
            self.processes.push(child);
        }

        Ok(())
    }

    fn wait_for_initial_builds(&mut self) -> Result<()> {
        let total_libraries = self.linked_libraries.len();
        
        // Create progress bar for library builds
        let pb = ProgressBar::new(total_libraries as u64);
        pb.set_style(
            ProgressStyle::default_bar()
                .template("{spinner:.green} [{elapsed_precise}] {bar:30.cyan/blue} {pos}/{len} {msg}")
                .unwrap()
                .progress_chars("█▉▊▋▌▍▎▏  ")
        );
        pb.set_message("Building libraries...");
        
        let mut completed_libraries = std::collections::HashSet::new();
        
        // Set up channel for build completion events
        let (tx, rx) = mpsc::channel();
        
        // Monitor each library build process for completion
        for (index, process) in self.processes.iter_mut().enumerate() {
            if index < self.linked_libraries.len() {
                let lib_name = self.linked_libraries[index].library_name.clone();
                let tx_clone = tx.clone();
                
                // Monitor stdout for initial build completion (suppress most output)
                if let Some(stdout) = process.stdout.take() {
                    thread::spawn(move || {
                        let reader = BufReader::new(stdout);
                        for line in reader.lines() {
                            if let Ok(line) = line {
                                // Only show important lines, suppress verbose output
                                if line.contains("Error") || line.contains("ERROR") || line.contains("Failed") {
                                    eprintln!("  [{}] {}", lib_name, line);
                                }
                                
                                // Check for build completion patterns
                                if line.contains("✓ Built") || 
                                   line.contains("Build complete") ||
                                   line.contains("Compilation complete") ||
                                   line.contains("webpack compiled") {
                                    let _ = tx_clone.send(LibraryBuildEvent::Complete(lib_name.clone()));
                                } else if line.contains("Build failed") || 
                                         line.contains("✖ Failed") ||
                                         line.contains("ERROR") {
                                    let _ = tx_clone.send(LibraryBuildEvent::Failed(lib_name.clone()));
                                }
                            }
                        }
                    });
                }
            }
        }
        
        // Wait for all libraries to complete their initial build
        let timeout = Duration::from_secs(120); // 2 minute timeout
        let start_time = std::time::Instant::now();
        
        while completed_libraries.len() < total_libraries {
            if start_time.elapsed() > timeout {
                pb.finish_with_message("❌ Timeout waiting for library builds");
                return Err(SpineError::Config("Timeout waiting for library builds to complete".to_string()).into());
            }
            
            // Check for build events with timeout
            match rx.recv_timeout(Duration::from_millis(100)) {
                Ok(LibraryBuildEvent::Complete(lib_name)) => {
                    if completed_libraries.insert(lib_name.clone()) {
                        pb.inc(1);
                        pb.set_message(format!("Built: {}", lib_name));
                    }
                }
                Ok(LibraryBuildEvent::Failed(lib_name)) => {
                    pb.finish_with_message(format!("❌ Library '{}' build failed", lib_name));
                    return Err(SpineError::Config(format!("Library '{}' build failed", lib_name)).into());
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    // Continue waiting
                }
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    break;
                }
            }
        }
        
        if completed_libraries.len() == total_libraries {
            pb.finish_with_message(format!("🎉 All {} library builds completed!", total_libraries));
        } else {
            pb.finish_with_message(format!("⚠️  Only {}/{} libraries completed", completed_libraries.len(), total_libraries));
        }
        
        Ok(())
    }

    fn start_app_server(&mut self, port: u16, hmr: bool) -> Result<()> {
        let mut cmd = Platform::ng_command();
        cmd.args(&["serve", &self.app_project])
           .args(&["--port", &port.to_string()])
           .args(&["--host", "0.0.0.0"])
           .args(&["--live-reload", "true"])
           .current_dir(&self.workspace_root)
           .env("NG_CLI_ANALYTICS", "false");

        if hmr {
            cmd.arg("--hmr");
        }

        let child = cmd.spawn()
            .map_err(|e| SpineError::Config(format!("Failed to start application server: {}", e)))?;
        
        self.processes.push(child);
        
        Ok(())
    }

    fn coordinate_rebuilds(&mut self) -> Result<()> {
        // Create a final spinner for the monitoring phase
        let monitor_spinner = ProgressBar::new_spinner();
        monitor_spinner.set_style(
            ProgressStyle::default_spinner()
                .tick_strings(&["🔄", "🔃", "🔄", "🔃"])
                .template("{spinner} {msg}")
                .unwrap()
        );
        monitor_spinner.set_message("Monitoring library and app servers (Press Ctrl+C to stop)");
        monitor_spinner.enable_steady_tick(Duration::from_millis(800));
        
        // Wait indefinitely (until user interrupts)
        loop {
            thread::sleep(Duration::from_secs(1));
            
            // Check if any processes have terminated
            let mut all_running = true;
            for process in &mut self.processes {
                match process.try_wait() {
                    Ok(Some(status)) => {
                        if !status.success() {
                            monitor_spinner.finish_with_message("⚠️  A process has terminated with error");
                            return Ok(());
                        }
                        all_running = false;
                    }
                    Ok(None) => {
                        // Process is still running
                    }
                    Err(_) => {
                        all_running = false;
                    }
                }
            }
            
            if !all_running {
                monitor_spinner.finish_with_message("⚠️  Some processes have stopped");
                break;
            }
        }

        Ok(())
    }
}

impl Drop for LibraryWatchServer {
    fn drop(&mut self) {
        println!("🛑 Stopping all development servers...");
        for process in &mut self.processes {
            let _ = process.kill();
        }
    }
}

#[derive(Debug)]
enum LibraryBuildEvent {
    Complete(String),
    Failed(String),
}

// CLI command implementations
pub fn ng_generate_command(
    schematic: &str,
    name: &str,
    lib: Option<&str>,
    args: Vec<String>,
) -> Result<()> {
    let config = Config::load_or_create()?;
    let workspace_root = std::env::current_dir()?;
    
    // Auto-detect library if not provided and we're in a library directory
    let detected_lib = if lib.is_none() {
        detect_current_library(&workspace_root, &config)?
    } else {
        lib.map(|s| s.to_string())
    };
    
    let integration = AngularCliIntegration::new(config, workspace_root)?;
    integration.generate_with_lib_context(schematic, name, detected_lib.as_deref(), args)
}

fn detect_current_library(current_dir: &std::path::PathBuf, config: &Config) -> Result<Option<String>> {
    // Check if we're in a library source directory by looking for project structure
    let mut dir = current_dir.clone();
    
    // Walk up directories looking for angular.json (workspace root)
    while let Some(parent) = dir.parent() {
        let angular_json = parent.join("angular.json");
        if angular_json.exists() {
            // Found workspace root, now check if current path is within a library
            if let Ok(Some(workspace)) = AngularBuildManager::detect_angular_workspace(&parent.to_path_buf()) {
                for (lib_name, project) in &workspace.projects {
                    if project.project_type == "library" {
                        let lib_path = parent.join(&project.root);
                        if current_dir.starts_with(&lib_path) {
                            // Check if this library is linked in Spine config
                            if config.links.contains_key(lib_name) {
                                println!("📚 Auto-detected library: {}", lib_name);
                                return Ok(Some(lib_name.clone()));
                            }
                        }
                    }
                }
            }
            break;
        }
        dir = parent.to_path_buf();
    }
    
    Ok(None)
}

pub fn ng_proxy_command(args: Vec<String>) -> Result<()> {
    let config = Config::load_or_create()?;
    let workspace_root = std::env::current_dir()?;
    
    let proxy = NgProxy::new(config, workspace_root);
    proxy.proxy_command(args)
}

pub fn serve_with_libs_command(port: Option<u16>, hmr: bool, project: Option<&str>) -> Result<()> {
    let config = Config::load_or_create()?;
    let workspace_root = std::env::current_dir()?;
    
    let mut server = LibraryWatchServer::new(&config, workspace_root)?;
    
    // Override app project if specified
    if let Some(proj) = project {
        server.app_project = proj.to_string();
    }
    
    server.serve_with_libraries(port, hmr)
}

pub fn debug_command(show_workspace: bool, show_libs: bool) -> Result<()> {
    let config = Config::load_or_create()?;
    let workspace_root = std::env::current_dir()?;
    
    println!("🔍 Spine Angular Debug Information");
    println!("==================================");
    
    // Show Spine linked packages with linked project info
    println!("\n📦 Spine Linked Packages:");
    if config.links.is_empty() {
        println!("  (No packages linked in Spine)");
    } else {
        for (name, link) in &config.links {
            println!("  • {} -> {}", name, link.path.display());
            if !link.linked_projects.is_empty() {
                println!("    🔗 Linked to {} project(s):", link.linked_projects.len());
                for project in &link.linked_projects {
                    println!("      • {}", project.display());
                }
            }
        }
    }
    
    // Use the same intelligent workspace detection as serve/build commands
    println!("\n🏗️  Smart Workspace Detection:");
    
    // Get only packages linked to current project (like serve command does)
    let linked_package_names = get_linked_packages_for_project(&config, &workspace_root)?;
    
    // First try current directory for workspace
    let mut detected_workspace_root = workspace_root.clone();
    let mut workspace = AngularBuildManager::detect_angular_workspace(&workspace_root)?;
    
    // If no workspace in current directory, try to find workspace from linked packages
    if workspace.is_none() && !config.links.is_empty() {
        println!("  🔍 No Angular workspace in current directory, searching from linked packages...");
        
        // Try to find workspace from any linked package
        for (package_name, package_link) in &config.links {
            match AngularBuildManager::find_workspace_root_for_package(&package_link.path) {
                Ok(found_workspace_root) => {
                    if let Ok(Some(found_workspace)) = AngularBuildManager::detect_angular_workspace(&found_workspace_root) {
                        println!("  ✅ Found Angular workspace from package '{}': {}", package_name, found_workspace_root.display());
                        detected_workspace_root = found_workspace_root;
                        workspace = Some(found_workspace);
                        break;
                    }
                }
                Err(_) => continue,
            }
        }
    }
    
    match workspace {
        Some(workspace) => {
            println!("  ✅ Angular workspace detected");
            println!("  📁 Workspace root: {}", detected_workspace_root.display());
            println!("  🎯 Default project: {}", workspace.default_project.as_deref().unwrap_or("(none)"));
            
            if show_workspace {
                println!("\n📋 All Projects in Workspace:");
                for (name, project) in &workspace.projects {
                    println!("  • {} ({})", name, project.project_type);
                    println!("    📂 Root: {}", project.root);
                    if let Some(src) = &project.source_root {
                        println!("    📄 Source: {}", src);
                    }
                }
            }
            
            // Smart library matching (same logic as serve command)
            println!("\n🔗 Smart Library Matching Analysis:");
            let library_projects: Vec<_> = workspace.projects
                .iter()
                .filter(|(_, project)| project.project_type == "library")
                .collect();
                
            println!("  📚 Libraries in workspace: {}", library_projects.len());
            for (name, _) in &library_projects {
                println!("    • {}", name);
            }
            
            println!("  🎯 Packages linked to current project: {}", linked_package_names.len());
            for pkg in &linked_package_names {
                println!("    • {}", pkg);
            }
            
            // Cross-workspace library detection
            println!("\n🔍 Cross-Workspace Library Detection:");
            let mut local_matches = Vec::new();
            let mut cross_workspace_matches: Vec<(String, String, std::path::PathBuf)> = Vec::new();
            let mut unmatched = Vec::new();
            
            for package_name in &linked_package_names {
                if let Some(package_link) = config.links.get(package_name) {
                    let mut found_match = false;
                    
                    // First try to find library in current workspace
                    if workspace.projects
                        .get(package_name)
                        .map(|p| p.project_type == "library")
                        .unwrap_or(false) {
                        local_matches.push(package_name);
                        println!("    ✅ {} (local workspace library)", package_name);
                        found_match = true;
                    } else {
                        // Try to resolve package to library name in current workspace
                        for (lib_name, project) in &workspace.projects {
                            if project.project_type == "library" {
                                let potential_dist_path = detected_workspace_root.join("dist").join(lib_name);
                                
                                if let (Ok(package_canonical), Ok(dist_canonical)) = (
                                    package_link.path.canonicalize(),
                                    potential_dist_path.canonicalize()
                                ) {
                                    if package_canonical == dist_canonical {
                                        local_matches.push(package_name);
                                        println!("    ✅ {} -> {} (local workspace library via dist mapping)", package_name, lib_name);
                                        found_match = true;
                                        break;
                                    }
                                }
                            }
                        }
                    }
                    
                    // If not found locally, try cross-workspace detection
                    if !found_match {
                        match AngularBuildManager::find_workspace_root_for_package(&package_link.path) {
                            Ok(lib_workspace_root) => {
                                if let Ok(Some(lib_workspace)) = AngularBuildManager::detect_angular_workspace(&lib_workspace_root) {
                                    for (lib_name, project) in &lib_workspace.projects {
                                        if project.project_type == "library" {
                                            let potential_dist_path = lib_workspace_root.join("dist").join(lib_name);
                                            
                                            if let (Ok(package_canonical), Ok(dist_canonical)) = (
                                                package_link.path.canonicalize(),
                                                potential_dist_path.canonicalize()
                                            ) {
                                                if package_canonical == dist_canonical {
                                                    cross_workspace_matches.push((package_name.to_string(), lib_name.to_string(), lib_workspace_root.clone()));
                                                    println!("    🔗 {} -> {} (cross-workspace library in {})", 
                                                             package_name, lib_name, lib_workspace_root.display());
                                                    found_match = true;
                                                    break;
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                            Err(_) => {}
                        }
                    }
                    
                    if !found_match {
                        unmatched.push(package_name);
                        println!("    ❌ {} (no matching workspace library found)", package_name);
                    }
                }
            }
            
            println!("\n📊 Smart Matching Summary:");
            println!("  ✅ Local workspace matches: {}", local_matches.len());
            println!("  🔗 Cross-workspace matches: {}", cross_workspace_matches.len());
            println!("  ❌ Unmatched packages: {}", unmatched.len());
            
            if show_libs && (!cross_workspace_matches.is_empty() || !unmatched.is_empty()) {
                if !cross_workspace_matches.is_empty() {
                    println!("\n🌐 Cross-Workspace Details:");
                    for (package_name, lib_name, workspace_root) in cross_workspace_matches {
                        println!("  📦 {} -> {}", package_name, lib_name);
                        println!("    🏠 Workspace: {}", workspace_root.display());
                        if let Some(link) = config.links.get(&package_name) {
                            println!("    📂 Package path: {}", link.path.display());
                        }
                    }
                }
                
                if !unmatched.is_empty() {
                    println!("\n💡 Suggestions for unmatched packages:");
                    for package in &unmatched {
                        if let Some(link) = config.links.get(*package) {
                            println!("  📦 {}", package);
                            println!("    🔗 Linked to: {}", link.path.display());
                            
                            // Try to find similar library names
                            let similar: Vec<_> = library_projects
                                .iter()
                                .filter(|(lib_name, _)| {
                                    lib_name.contains(package.as_str()) || package.contains(lib_name.as_str())
                                })
                                .collect();
                                
                            if !similar.is_empty() {
                                println!("    🔍 Similar workspace libraries:");
                                for (lib_name, _) in similar {
                                    println!("      • {}", lib_name);
                                }
                            }
                            
                            // Check if package path leads to a different workspace
                            match AngularBuildManager::find_workspace_root_for_package(&link.path) {
                                Ok(package_workspace_root) => {
                                    if package_workspace_root != detected_workspace_root {
                                        println!("    🏠 Package belongs to different workspace: {}", package_workspace_root.display());
                                    }
                                }
                                Err(_) => {
                                    println!("    ⚠️  Package path doesn't lead to an Angular workspace");
                                }
                            }
                        }
                    }
                }
            }
            
        }
        None => {
            println!("  ❌ No Angular workspace detected in current directory or linked package paths");
            println!("  📁 Current directory: {}", workspace_root.display());
            
            if !config.links.is_empty() {
                println!("  🔍 Checking individual package workspaces:");
                for (package_name, package_link) in &config.links {
                    match AngularBuildManager::find_workspace_root_for_package(&package_link.path) {
                        Ok(package_workspace_root) => {
                            println!("    📦 {} -> workspace at {}", package_name, package_workspace_root.display());
                        }
                        Err(_) => {
                            println!("    📦 {} -> no workspace found", package_name);
                        }
                    }
                }
            }
            
            println!("  💡 Make sure you're in an Angular project root directory, or run 'ng new' to create a new project.");
        }
    }
    
    Ok(())
}