use std::path::Path;
use anyhow::Result;
use crate::config::Config;
use crate::error::SpineError;
use crate::platform::Platform;

pub struct NpmManager;

impl NpmManager {
    pub fn link_all(config: &mut Config) -> Result<()> {
        if config.links.is_empty() {
            println!("No packages configured to link.");
            return Ok(());
        }

        println!("Linking all configured packages...");
        let mut success_count = 0;
        let mut failed_packages = Vec::new();
        let current_dir = std::env::current_dir()?;

        let package_names: Vec<String> = config.links.keys().cloned().collect();
        
        for name in package_names {
            let link = config.links.get(&name).unwrap().clone();
            match Self::npm_link(&link.path) {
                Ok(_) => {
                    // Verify the link was actually created
                    if crate::config::Config::is_package_linked_in_project_static(&name, &current_dir) {
                        config.add_linked_project(&name, current_dir.clone())?;
                        println!("✓ Linked: {} -> {}", name, link.path.display());
                        success_count += 1;
                    } else {
                        println!("⚠️  Link command succeeded but verification failed for: {}", name);
                        failed_packages.push(name);
                    }
                }
                Err(e) => {
                    println!("✗ Failed to link {}: {}", name, e);
                    failed_packages.push(name);
                }
            }
        }

        println!("\nSummary: {} successful, {} failed", success_count, failed_packages.len());
        if !failed_packages.is_empty() {
            println!("Failed packages: {}", failed_packages.join(", "));
        }

        Ok(())
    }

    pub fn link_package(config: &mut Config, package_name: &str) -> Result<()> {
        let link = config.links.get(package_name)
            .ok_or_else(|| {
                let available: Vec<String> = config.links.keys().cloned().collect();
                SpineError::package_not_found_with_suggestions(package_name, &available)
            })?
            .clone();

        println!("Linking package: {} -> {}", package_name, link.path.display());
        
        Self::npm_link(&link.path)?;
        
        // Verify the link was actually created
        let current_dir = std::env::current_dir()?;
        if crate::config::Config::is_package_linked_in_project_static(package_name, &current_dir) {
            config.add_linked_project(package_name, current_dir)?;
            println!("✓ Successfully linked: {}", package_name);
        } else {
            println!("⚠️  Link command completed but symlink verification failed for: {}", package_name);
            return Err(SpineError::Config("Link verification failed".to_string()).into());
        }
        
        Ok(())
    }

    pub fn unlink_package(config: &mut Config, package_name: &str) -> Result<()> {
        println!("Unlinking package: {}", package_name);
        
        let output = Platform::npm_command()
            .args(&["unlink", package_name])
            .output()
            .map_err(|e| SpineError::Io(e))?;

        if output.status.success() {
            let current_dir = std::env::current_dir()?;
            
            // Verify the link was actually removed
            if !crate::config::Config::is_package_linked_in_project_static(package_name, &current_dir) {
                config.remove_linked_project(package_name, &current_dir)?;
                println!("✓ Successfully unlinked: {}", package_name);
            } else {
                println!("⚠️  Unlink command completed but symlink still exists for: {}", package_name);
                // Still remove from config since npm unlink succeeded
                config.remove_linked_project(package_name, &current_dir)?;
            }
        } else {
            let error_msg = String::from_utf8_lossy(&output.stderr);
            return Err(SpineError::Config(format!("npm unlink failed: {}", error_msg)).into());
        }

        Ok(())
    }

    pub fn unlink_all(config: &mut Config) -> Result<()> {
        println!("Unlinking all packages from current project...");
        
        let current_dir = std::env::current_dir()?;
        
        // Get packages that are actually linked to the current project
        let linked_packages = Self::get_linked_packages()?;
        
        if linked_packages.is_empty() {
            println!("No packages currently linked in this project.");
            return Ok(());
        }
        
        println!("Found {} linked package(s) to unlink:", linked_packages.len());
        
        let mut success_count = 0;
        let mut failed_packages = Vec::new();
        
        for package_name in &linked_packages {
            // Only unlink if it's in our configuration (managed by Spine)
            if config.links.contains_key(package_name) {
                print!("  🔗 Unlinking {}... ", package_name);
                
                let output = Platform::npm_command()
                    .args(&["unlink", package_name])
                    .output()
                    .map_err(|e| crate::error::SpineError::Io(e))?;

                if output.status.success() {
                    // Remove from linked projects for this package
                    config.remove_linked_project(package_name, &current_dir)?;
                    success_count += 1;
                    println!("✅ Success");
                } else {
                    let error_msg = String::from_utf8_lossy(&output.stderr);
                    failed_packages.push((package_name.clone(), error_msg.to_string()));
                    println!("❌ Failed");
                }
            } else {
                println!("  ⚠️  Skipping {} (not managed by Spine)", package_name);
            }
        }
        
        // Summary
        println!("\n📊 Unlink Summary:");
        println!("  ✅ Successfully unlinked: {}", success_count);
        
        if !failed_packages.is_empty() {
            println!("  ❌ Failed to unlink: {}", failed_packages.len());
            for (package, error) in &failed_packages {
                println!("    • {}: {}", package, error.trim());
            }
        }
        
        if success_count > 0 {
            println!("\n✨ All managed packages have been unlinked from the current project.");
        }
        
        Ok(())
    }

    pub fn show_status(config: &Config) -> Result<()> {
        println!("NPM Link Status for current project:");
        
        if !Self::is_npm_project()? {
            println!("⚠ Warning: Current directory is not an npm project (no package.json found)");
            return Ok(());
        }

        let linked_packages = Self::get_linked_packages()?;
        
        if linked_packages.is_empty() {
            println!("No packages currently linked in this project.");
            return Ok(());
        }

        println!("\nCurrently linked packages:");
        for package in &linked_packages {
            let status = if config.links.contains_key(package) {
                "✓ (managed by Spine)"
            } else {
                "○ (not in Spine config)"
            };
            println!("  {} {}", package, status);
        }

        if !config.links.is_empty() {
            println!("\nSpine configured packages:");
            for (name, link) in &config.links {
                let linked_status = if linked_packages.contains(name) {
                    "✓ linked"
                } else {
                    "○ not linked"
                };
                println!("  {} -> {} [{}]", name, link.path.display(), linked_status);
            }
        }

        Ok(())
    }

    pub fn verify_links(config: &mut Config) -> Result<()> {
        println!("Verifying package links...");
        
        let removed_links = config.verify_and_clean_links()?;
        
        if removed_links.is_empty() {
            println!("✓ All links are valid.");
        } else {
            println!("Cleaned up {} broken link(s):", removed_links.len());
            for link in &removed_links {
                println!("  ✗ Removed: {}", link);
            }
            config.save()?;
            println!("\nConfiguration updated.");
        }
        
        Ok(())
    }

    fn npm_link(package_path: &Path) -> Result<()> {
        Self::npm_link_static(package_path)
    }

    pub fn npm_link_static(package_path: &Path) -> Result<()> {
        let output = Platform::npm_command()
            .args(&["link", &package_path.to_string_lossy()])
            .output()
            .map_err(|e| SpineError::Io(e))?;

        if !output.status.success() {
            let error_msg = String::from_utf8_lossy(&output.stderr);
            return Err(SpineError::Config(format!("npm link failed: {}", error_msg)).into());
        }

        Ok(())
    }

    fn is_npm_project() -> Result<bool> {
        Ok(Path::new("package.json").exists())
    }

    fn get_linked_packages() -> Result<Vec<String>> {
        if !std::path::Path::new("node_modules").exists() {
            return Ok(Vec::new());
        }

        let mut packages = Vec::new();
        let node_modules = std::path::Path::new("node_modules");
        
        // Scan for direct symlinks
        for entry in std::fs::read_dir(node_modules).map_err(|e| SpineError::Io(e))? {
            let entry = entry.map_err(|e| SpineError::Io(e))?;
            let path = entry.path();
            
            if path.is_symlink() {
                if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                    // Verify symlink target exists and is valid
                    if Self::is_valid_symlink(&path) {
                        packages.push(name.to_string());
                    }
                }
            }
            
            // Handle scoped packages (@scope/package)
            if path.is_dir() && entry.file_name().to_string_lossy().starts_with('@') {
                if let Ok(scope_entries) = std::fs::read_dir(&path) {
                    for scope_entry in scope_entries.flatten() {
                        let scope_path = scope_entry.path();
                        
                        if scope_path.is_symlink() {
                            if let Some(scope_name) = scope_path.file_name().and_then(|n| n.to_str()) {
                                if Self::is_valid_symlink(&scope_path) {
                                    let full_name = format!("{}/{}", entry.file_name().to_string_lossy(), scope_name);
                                    packages.push(full_name);
                                }
                            }
                        }
                    }
                }
            }
        }
        
        packages.sort();
        packages.dedup();
        Ok(packages)
    }

    fn is_valid_symlink(path: &std::path::Path) -> bool {
        // Check if symlink target exists and is readable
        path.read_link().is_ok() && path.exists()
    }

    pub fn get_linked_packages_static() -> Result<Vec<String>> {
        Self::get_linked_packages()
    }

    pub fn show_enhanced_status(config: &Config, detailed: bool, health: bool, json: bool) -> Result<()> {
        let current_dir = std::env::current_dir()?;
        
        if json {
            Self::show_status_json(config, detailed, health, &current_dir)
        } else if health {
            Self::show_health_status(config, detailed, &current_dir)
        } else if detailed {
            Self::show_detailed_status(config, &current_dir)
        } else {
            Self::show_status(config)
        }
    }

    fn show_status_json(config: &Config, detailed: bool, health: bool, current_dir: &std::path::PathBuf) -> Result<()> {
        let mut status = serde_json::Map::new();
        status.insert("current_directory".to_string(), serde_json::Value::String(current_dir.display().to_string()));
        status.insert("total_packages".to_string(), serde_json::Value::Number(config.links.len().into()));
        
        let mut packages = serde_json::Map::new();
        
        for (name, link) in &config.links {
            let mut package_info = serde_json::Map::new();
            package_info.insert("path".to_string(), serde_json::Value::String(link.path.display().to_string()));
            
            if let Some(version) = &link.version {
                package_info.insert("version".to_string(), serde_json::Value::String(version.clone()));
            }
            
            let is_linked = link.linked_projects.iter().any(|p| p == current_dir);
            package_info.insert("linked_to_current".to_string(), serde_json::Value::Bool(is_linked));
            
            if detailed || health {
                let path_exists = link.path.exists();
                package_info.insert("path_exists".to_string(), serde_json::Value::Bool(path_exists));
                
                if health {
                    let package_json_exists = link.path.join("package.json").exists();
                    package_info.insert("package_json_exists".to_string(), serde_json::Value::Bool(package_json_exists));
                    
                    // Check for version mismatch
                    if let Some(current_version) = &link.version {
                        if let Ok(actual_version) = crate::package::get_package_version(&link.path.join("package.json")) {
                            let version_matches = current_version == &actual_version;
                            package_info.insert("version_matches".to_string(), serde_json::Value::Bool(version_matches));
                            if !version_matches {
                                package_info.insert("actual_version".to_string(), serde_json::Value::String(actual_version));
                            }
                        }
                    }
                }
            }
            
            packages.insert(name.clone(), serde_json::Value::Object(package_info));
        }
        
        status.insert("packages".to_string(), serde_json::Value::Object(packages));
        
        println!("{}", serde_json::to_string_pretty(&status)?);
        Ok(())
    }

    fn show_health_status(config: &Config, detailed: bool, current_dir: &std::path::PathBuf) -> Result<()> {
        println!("🏥 Package Health Check");
        println!("=====================");
        
        let mut healthy = 0;
        let mut issues = 0;
        
        for (name, link) in &config.links {
            let is_linked = link.linked_projects.iter().any(|p| p == current_dir);
            let path_exists = link.path.exists();
            let package_json_exists = link.path.join("package.json").exists();
            
            let mut warnings = Vec::new();
            let mut errors = Vec::new();
            
            if !path_exists {
                errors.push("Path does not exist");
            } else if !package_json_exists {
                errors.push("Missing package.json");
            }
            
            // Check version mismatch
            if let Some(stored_version) = &link.version {
                if let Ok(actual_version) = crate::package::get_package_version(&link.path.join("package.json")) {
                    if stored_version != &actual_version {
                        warnings.push(format!("Version mismatch: stored '{}', actual '{}'", stored_version, actual_version));
                    }
                }
            }
            
            if errors.is_empty() && warnings.is_empty() {
                print!("✅ {}", name);
                if is_linked {
                    print!(" (linked)");
                }
                println!();
                healthy += 1;
            } else {
                issues += 1;
                if !errors.is_empty() {
                    print!("❌ {}", name);
                    for error in &errors {
                        print!(" - {}", error);
                    }
                    println!();
                } else {
                    print!("⚠️  {}", name);
                    for warning in &warnings {
                        print!(" - {}", warning);
                    }
                    println!();
                }
                
                if detailed {
                    println!("   Path: {}", link.path.display());
                    if let Some(version) = &link.version {
                        println!("   Stored version: {}", version);
                    }
                }
            }
        }
        
        println!("\n📊 Summary: {} healthy, {} with issues", healthy, issues);
        Ok(())
    }

    fn show_detailed_status(config: &Config, current_dir: &std::path::PathBuf) -> Result<()> {
        println!("📋 Detailed Package Status");
        println!("=========================");
        
        if config.links.is_empty() {
            println!("No packages configured.");
            return Ok(());
        }
        
        for (name, link) in &config.links {
            let is_linked = link.linked_projects.iter().any(|p| p == current_dir);
            
            println!("\n📦 {}", name);
            println!("   Path: {}", link.path.display());
            
            if let Some(version) = &link.version {
                print!("   Version: {}", version);
                
                // Check for version changes
                if let Ok(actual_version) = crate::package::get_package_version(&link.path.join("package.json")) {
                    if version != &actual_version {
                        print!(" ⚠️  (actual: {})", actual_version);
                    }
                }
                println!();
            }
            
            if is_linked {
                println!("   Status: ✅ Linked to current project");
            } else {
                println!("   Status: ⭕ Not linked to current project");
            }
            
            if !link.linked_projects.is_empty() {
                println!("   Linked projects:");
                for project in &link.linked_projects {
                    println!("     • {}", project.display());
                }
            }
            
            // Check path health
            if !link.path.exists() {
                println!("   ❌ Path does not exist");
            } else if !link.path.join("package.json").exists() {
                println!("   ⚠️  No package.json found");
            }
        }
        
        Ok(())
    }
}