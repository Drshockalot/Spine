use std::fs;
use std::path::{Path, PathBuf};
use anyhow::Result;
use serde::{Deserialize, Serialize};
use crate::package;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct WorkspaceConfig {
    #[serde(default)]
    pub auto_link: AutoLinkConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AutoLinkConfig {
    #[serde(default)]
    pub patterns: Vec<String>,
    #[serde(default)]
    pub exclude: Vec<String>,
    #[serde(default)]
    pub enabled: bool,
}

#[derive(Debug, Clone)]
pub struct DiscoveredPackage {
    pub name: String,
    pub path: PathBuf,
    pub version: String,
    pub is_dist: bool,
}

pub struct WorkspaceManager;

impl WorkspaceManager {
    pub fn workspace_config_path() -> PathBuf {
        PathBuf::from(".spine.toml")
    }

    pub fn load_workspace_config() -> Result<Option<WorkspaceConfig>> {
        let config_path = Self::workspace_config_path();
        if !config_path.exists() {
            return Ok(None);
        }

        let content = fs::read_to_string(&config_path)?;
        let config: WorkspaceConfig = toml::from_str(&content)?;
        Ok(Some(config))
    }

    pub fn save_workspace_config(config: &WorkspaceConfig) -> Result<()> {
        let config_path = Self::workspace_config_path();
        let content = toml::to_string_pretty(config)?;
        fs::write(&config_path, content)?;
        Ok(())
    }

    pub fn scan_for_packages(search_path: Option<&str>) -> Result<Vec<DiscoveredPackage>> {
        let search_dir = match search_path {
            Some(path) => PathBuf::from(path),
            None => std::env::current_dir()?,
        };

        let mut packages = Vec::new();
        
        // First, try to detect if this is an Angular workspace
        if let Ok(Some(angular_workspace)) = crate::angular::AngularBuildManager::detect_angular_workspace(&search_dir) {
            println!("🅰️  Angular workspace detected at: {}", search_dir.display());
            Self::scan_angular_workspace(&search_dir, &angular_workspace, &mut packages)?;
        } else {
            // Fallback to regular directory scanning
            println!("📁 Scanning directory for packages: {}", search_dir.display());
            Self::scan_directory(&search_dir, &mut packages)?;
        }
        
        // Sort by name for consistent output
        packages.sort_by(|a, b| a.name.cmp(&b.name));
        
        Ok(packages)
    }

    fn scan_angular_workspace(
        workspace_root: &Path, 
        angular_workspace: &crate::angular::AngularWorkspace, 
        packages: &mut Vec<DiscoveredPackage>
    ) -> Result<()> {
        let dist_dir = workspace_root.join("dist");
        
        // First, scan for built libraries in dist/ folder
        if dist_dir.exists() {
            println!("📦 Scanning dist/ folder for built libraries...");
            
            // Get all library projects from angular.json
            let library_projects: Vec<_> = angular_workspace.projects
                .iter()
                .filter(|(_, project)| project.project_type == "library")
                .collect();
            
            if !library_projects.is_empty() {
                println!("🔍 Found {} library project(s) in angular.json:", library_projects.len());
                for (lib_name, _) in &library_projects {
                    println!("    • {}", lib_name);
                }
            }
            
            // Scan for built libraries in dist/LIBRARY_NAME
            for (lib_name, _) in &library_projects {
                let lib_dist_path = dist_dir.join(lib_name);
                let package_json_path = lib_dist_path.join("package.json");
                
                if package_json_path.exists() {
                    if let Ok(package_info) = package::parse_package_json(&package_json_path) {
                        println!("    ✅ Found built library: {} at {}", package_info.name, lib_dist_path.display());
                        packages.push(DiscoveredPackage {
                            name: package_info.name,
                            path: lib_dist_path,
                            version: package_info.version,
                            is_dist: true,
                        });
                    }
                } else {
                    println!("    ⚠️  Library '{}' not built yet (no package.json in {})", lib_name, lib_dist_path.display());
                    println!("       Run 'ng build {}' to build this library", lib_name);
                }
            }
        } else {
            println!("📦 No dist/ folder found. Libraries need to be built first.");
            let library_projects: Vec<_> = angular_workspace.projects
                .iter()
                .filter(|(_, project)| project.project_type == "library")
                .map(|(name, _)| name)
                .collect();
            
            if !library_projects.is_empty() {
                println!("💡 Found {} library project(s) that can be built:", library_projects.len());
                for lib_name in &library_projects {
                    println!("    • {} (run 'ng build {}' to build)", lib_name, lib_name);
                }
            }
        }
        
        Ok(())
    }

    fn scan_directory(dir: &Path, packages: &mut Vec<DiscoveredPackage>) -> Result<()> {
        if !dir.is_dir() {
            return Ok(());
        }

        // Skip node_modules and other common directories to avoid
        if let Some(dir_name) = dir.file_name() {
            if dir_name == "node_modules" || dir_name == ".git" || dir_name == "target" {
                return Ok(());
            }
        }

        // Check if this directory contains a package.json
        let package_json_path = dir.join("package.json");
        if package_json_path.exists() {
            if let Ok(package_info) = package::parse_package_json(&package_json_path) {
                let is_dist = dir.file_name()
                    .and_then(|n| n.to_str())
                    .map(|n| n == "dist" || n.contains("dist"))
                    .unwrap_or(false);

                packages.push(DiscoveredPackage {
                    name: package_info.name,
                    path: dir.to_path_buf(),
                    version: package_info.version,
                    is_dist,
                });
            }
        }

        // Recursively scan subdirectories (up to reasonable depth)
        if let Ok(entries) = fs::read_dir(dir) {
            for entry in entries.flatten() {
                if entry.path().is_dir() {
                    // Limit recursion depth to avoid scanning too deep
                    if Self::get_depth(&entry.path()) < 6 {
                        Self::scan_directory(&entry.path(), packages)?;
                    }
                }
            }
        }

        Ok(())
    }

    fn get_depth(path: &Path) -> usize {
        path.components().count()
    }

    fn scan_directory_shallow(dir: &Path, packages: &mut Vec<DiscoveredPackage>) -> Result<()> {
        if !dir.is_dir() {
            return Ok(());
        }

        // Skip node_modules and other common directories
        if let Some(dir_name) = dir.file_name() {
            if dir_name == "node_modules" || dir_name == ".git" || dir_name == "target" || dir_name == "dist" {
                return Ok(());
            }
        }

        // Check if this directory contains a package.json (but skip if already in packages)
        let package_json_path = dir.join("package.json");
        if package_json_path.exists() {
            if let Ok(package_info) = package::parse_package_json(&package_json_path) {
                // Only add if not already found (avoid duplicates)
                if !packages.iter().any(|p| p.name == package_info.name) {
                    let is_dist = dir.file_name()
                        .and_then(|n| n.to_str())
                        .map(|n| n == "dist" || n.contains("dist"))
                        .unwrap_or(false);

                    packages.push(DiscoveredPackage {
                        name: package_info.name,
                        path: dir.to_path_buf(),
                        version: package_info.version,
                        is_dist,
                    });
                }
            }
        }

        // Scan subdirectories but only go one level deep to avoid deep recursion in workspace
        if let Ok(entries) = fs::read_dir(dir) {
            for entry in entries.flatten() {
                if entry.path().is_dir() {
                    // Only scan first level subdirectories
                    let package_json_subpath = entry.path().join("package.json");
                    if package_json_subpath.exists() {
                        if let Ok(package_info) = package::parse_package_json(&package_json_subpath) {
                            // Only add if not already found (avoid duplicates)
                            if !packages.iter().any(|p| p.name == package_info.name) {
                                let is_dist = entry.path().file_name()
                                    .and_then(|n| n.to_str())
                                    .map(|n| n == "dist" || n.contains("dist"))
                                    .unwrap_or(false);

                                packages.push(DiscoveredPackage {
                                    name: package_info.name,
                                    path: entry.path(),
                                    version: package_info.version,
                                    is_dist,
                                });
                            }
                        }
                    }
                }
            }
        }

        Ok(())
    }

    pub fn filter_packages_by_workspace_config<'a>(
        packages: &'a [DiscoveredPackage],
        workspace_config: &WorkspaceConfig,
    ) -> Vec<&'a DiscoveredPackage> {
        if !workspace_config.auto_link.enabled {
            return packages.iter().collect();
        }

        packages
            .iter()
            .filter(|pkg| {
                // Check exclude patterns first
                if workspace_config.auto_link.exclude.iter().any(|pattern| {
                    Self::matches_pattern(&pkg.name, pattern)
                }) {
                    return false;
                }

                // If no include patterns, include all (except excluded)
                if workspace_config.auto_link.patterns.is_empty() {
                    return true;
                }

                // Check include patterns
                workspace_config.auto_link.patterns.iter().any(|pattern| {
                    Self::matches_pattern(&pkg.name, pattern)
                })
            })
            .collect()
    }

    fn matches_pattern(name: &str, pattern: &str) -> bool {
        // Simple glob-style pattern matching
        if pattern.ends_with('*') {
            let prefix = &pattern[..pattern.len() - 1];
            name.starts_with(prefix)
        } else if pattern.starts_with('*') {
            let suffix = &pattern[1..];
            name.ends_with(suffix)
        } else {
            name == pattern
        }
    }

    pub fn suggest_packages_for_current_project() -> Result<Vec<DiscoveredPackage>> {
        let current_dir = std::env::current_dir()?;
        let package_json_path = current_dir.join("package.json");
        
        if !package_json_path.exists() {
            return Ok(Vec::new());
        }

        // Parse current project's dependencies
        let project_info = package::parse_package_json(&package_json_path)?;
        let all_deps: std::collections::HashSet<String> = project_info.dependencies
            .iter()
            .chain(project_info.dev_dependencies.iter())
            .cloned()
            .collect();

        // Scan for packages and filter by current project's dependencies
        let discovered = Self::scan_for_packages(None)?;
        let suggested = discovered
            .into_iter()
            .filter(|pkg| all_deps.contains(&pkg.name))
            .collect();

        Ok(suggested)
    }
}