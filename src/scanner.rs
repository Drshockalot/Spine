use std::process::Command;
use anyhow::Result;
use crate::config::Config;
use crate::workspace::WorkspaceManager;
use crate::platform::Platform;

pub struct Scanner;

impl Scanner {
    pub fn scan_packages(add_packages: bool, search_path: Option<&str>) -> Result<()> {
        println!("Scanning for packages...");
        
        let packages = WorkspaceManager::scan_for_packages(search_path)?;
        
        if packages.is_empty() {
            println!("No packages found in the specified directory.");
            return Ok(());
        }

        println!("Found {} package(s):", packages.len());
        
        // Load workspace config if available
        let workspace_config = WorkspaceManager::load_workspace_config()?.unwrap_or_default();
        let filtered_packages = WorkspaceManager::filter_packages_by_workspace_config(&packages, &workspace_config);
        
        for package in &packages {
            let included = filtered_packages.iter().any(|p| p.name == package.name);
            let dist_indicator = if package.is_dist { " (dist)" } else { "" };
            let status = if included { "✓" } else { "○" };
            
            println!("  {} {} (v{}) -> {}{}", 
                status, 
                package.name, 
                package.version, 
                package.path.display(),
                dist_indicator
            );
        }

        if add_packages {
            println!("\nAdding packages to configuration...");
            let mut config = Config::load_or_create()?;
            let mut added_count = 0;
            
            for package in filtered_packages {
                match config.add_link(package.name.clone(), package.path.to_string_lossy().to_string()) {
                    Ok(_) => {
                        println!("✓ Added: {}", package.name);
                        added_count += 1;
                    }
                    Err(e) => {
                        println!("✗ Failed to add {}: {}", package.name, e);
                    }
                }
            }
            
            if added_count > 0 {
                config.save()?;
                println!("\nAdded {} package(s) to configuration.", added_count);
            }
        } else {
            println!("\nUse --add to automatically add discovered packages to your configuration.");
            println!("Create a .spine.toml file to configure auto-link patterns.");
        }

        Ok(())
    }

    pub fn sync_links() -> Result<()> {
        println!("Enforcing Spine configuration as authority for package links...");
        
        let mut config = Config::load_or_create()?;
        
        if config.links.is_empty() {
            println!("No packages configured to sync.");
            return Ok(());
        }

        let current_dir = std::env::current_dir()?;
        
        // Check which configured packages should be linked to current project
        let mut packages_to_restore = Vec::new();
        let mut packages_already_linked = Vec::new();
        let mut packages_not_configured_here = Vec::new();
        
        for (package_name, package_link) in &config.links {
            // Check if this package should be linked to the current project according to config
            let should_be_linked = package_link.linked_projects.contains(&current_dir);
            
            if should_be_linked {
                // Check if it's actually linked
                let is_actually_linked = crate::config::Config::is_package_linked_in_project_static(package_name, &current_dir);
                
                if is_actually_linked {
                    packages_already_linked.push(package_name.clone());
                } else {
                    packages_to_restore.push(package_name.clone());
                }
            } else {
                packages_not_configured_here.push(package_name.clone());
            }
        }
        
        // Report current state
        println!("📊 Current state analysis:");
        println!("  ✅ Already linked as configured: {}", packages_already_linked.len());
        println!("  🔗 Need to restore links: {}", packages_to_restore.len());
        println!("  📦 Not configured for this project: {}", packages_not_configured_here.len());
        
        if packages_to_restore.is_empty() {
            println!("\n✅ All configured packages are properly linked.");
            return Ok(());
        }
        
        // Restore links that should exist according to configuration
        println!("\n🔧 Restoring package links according to Spine configuration...");
        let mut restored_count = 0;
        let mut failed_packages = Vec::new();
        
        for package_name in &packages_to_restore {
            let package_link = config.links.get(package_name).unwrap();
            
            print!("  🔗 Restoring link for {}... ", package_name);
            
            match crate::npm::NpmManager::npm_link_static(&package_link.path) {
                Ok(_) => {
                    // Verify the link was actually created
                    if crate::config::Config::is_package_linked_in_project_static(package_name, &current_dir) {
                        restored_count += 1;
                        println!("✅ Success");
                    } else {
                        println!("❌ Failed (verification failed)");
                        failed_packages.push(package_name.clone());
                    }
                }
                Err(e) => {
                    println!("❌ Failed ({})", e);
                    failed_packages.push(package_name.clone());
                }
            }
        }
        
        // Summary
        println!("\n📊 Sync Summary:");
        println!("  ✅ Successfully restored: {}", restored_count);
        if !failed_packages.is_empty() {
            println!("  ❌ Failed to restore: {}", failed_packages.len());
            for package in &failed_packages {
                println!("    • {}", package);
            }
        }
        
        if restored_count > 0 {
            println!("\n✨ Spine configuration has been enforced. {} package(s) restored.", restored_count);
        }
        
        Ok(())
    }

    pub fn open_config_editor() -> Result<()> {
        let config_path = Config::config_path()?;
        
        if !config_path.exists() {
            println!("Configuration file doesn't exist yet. Creating it...");
            let config = Config::default();
            config.save()?;
        }

        // Try common editors in order of preference
        let editors = [
            std::env::var("EDITOR").unwrap_or_default(),
            "code".to_string(),      // VS Code
            "subl".to_string(),      // Sublime Text
            "atom".to_string(),      // Atom
            "nano".to_string(),      // Nano
            "vim".to_string(),       // Vim
            "vi".to_string(),        // Vi
        ];

        for editor in &editors {
            if editor.is_empty() {
                continue;
            }

            let result = Command::new(editor)
                .arg(&config_path)
                .status();

            match result {
                Ok(status) => {
                    if status.success() {
                        println!("Configuration file opened in {}.", editor);
                        return Ok(());
                    }
                }
                Err(_) => continue, // Try next editor
            }
        }

        // Fallback: try opening with system default
        // Use cross-platform file opening
        match Platform::open_file_with_default_app(&config_path) {
            Ok(status) if status.success() => {
                println!("Configuration file opened with system default application.");
                return Ok(());
            }
            Ok(_) => {
                println!("Failed to open configuration file with default application.");
            }
            Err(e) => {
                println!("Error opening configuration file: {}", e);
            }
        }

        // If all else fails, just show the path
        println!("Could not open editor automatically.");
        println!("Please manually edit: {}", config_path.display());
        
        Ok(())
    }

    pub fn suggest_packages() -> Result<()> {
        println!("Analyzing current project dependencies...");
        
        let suggested = WorkspaceManager::suggest_packages_for_current_project()?;
        
        if suggested.is_empty() {
            println!("No local packages found that match your project's dependencies.");
            println!("Run 'spine scan' to see all available local packages.");
            return Ok(());
        }

        println!("Found {} local package(s) that match your project dependencies:", suggested.len());
        
        for package in &suggested {
            let dist_indicator = if package.is_dist { " (dist)" } else { "" };
            println!("  {} (v{}) -> {}{}", 
                package.name, 
                package.version, 
                package.path.display(),
                dist_indicator
            );
        }

        println!("\nUse 'spine link <package-name>' to link individual packages,");
        println!("or 'spine scan --add' to add all discovered packages to your configuration.");

        Ok(())
    }
}