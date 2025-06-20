use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use clap::CommandFactory;
use crate::error::SpineError;
use crate::platform::Platform;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackageLink {
    pub name: String,
    pub path: PathBuf,
    pub version: Option<String>,
    #[serde(default)]
    pub linked_projects: Vec<PathBuf>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Config {
    pub links: HashMap<String, PackageLink>,
    #[serde(default)]
    pub completion: CompletionConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CompletionConfig {
    pub auto_regenerate: bool,
    pub shell: Option<String>,
    pub script_path: Option<PathBuf>,
}

impl Config {
    pub fn config_path() -> Result<PathBuf> {
        let config_dir = dirs::config_dir()
            .ok_or_else(|| SpineError::Config("Could not find config directory".to_string()))?;
        
        let spine_dir = config_dir.join("spine");
        if !spine_dir.exists() {
            fs::create_dir_all(&spine_dir)?;
        }
        
        Ok(spine_dir.join("config.toml"))
    }

    pub fn load_or_create() -> Result<Self> {
        let config_path = Self::config_path()?;
        
        if config_path.exists() {
            Self::load()
        } else {
            let config = Self::default();
            config.save()?;
            Ok(config)
        }
    }

    pub fn load() -> Result<Self> {
        let config_path = Self::config_path()?;
        let content = fs::read_to_string(&config_path)?;
        let config: Config = toml::from_str(&content)?;
        Ok(config)
    }

    pub fn save(&self) -> Result<()> {
        let config_path = Self::config_path()?;
        let content = toml::to_string_pretty(self)?;
        fs::write(&config_path, content)?;
        Ok(())
    }

    pub fn add_link(&mut self, name: String, path: String) -> Result<()> {
        let path_buf = PathBuf::from(&path);
        
        if !path_buf.exists() {
            return Err(SpineError::InvalidPath(format!("Path does not exist: {}", path)).into());
        }

        let package_json_path = path_buf.join("package.json");
        let version = if package_json_path.exists() {
            crate::package::get_package_version(&package_json_path).ok()
        } else {
            None
        };

        let link = PackageLink {
            name: name.clone(),
            path: path_buf,
            version,
            linked_projects: Vec::new(),
        };

        self.links.insert(name, link);
        
        // Auto-regenerate completion if enabled
        if self.completion.auto_regenerate {
            if let Err(e) = self.regenerate_completion() {
                eprintln!("Warning: Failed to regenerate completion: {}", e);
            }
        }
        
        Ok(())
    }

    pub fn remove_link(&mut self, name: &str) -> Result<()> {
        if self.links.remove(name).is_none() {
            return Err(SpineError::PackageNotFound(name.to_string()).into());
        }
        
        // Auto-regenerate completion if enabled
        if self.completion.auto_regenerate {
            if let Err(e) = self.regenerate_completion() {
                eprintln!("Warning: Failed to regenerate completion: {}", e);
            }
        }
        
        Ok(())
    }

    pub fn list_links(&self) {
        if self.links.is_empty() {
            println!("No package links configured.");
            return;
        }

        println!("Package Links:");
        
        // Sort packages alphabetically by name
        let mut sorted_links: Vec<_> = self.links.values().collect();
        sorted_links.sort_by(|a, b| a.name.cmp(&b.name));
        
        for link in sorted_links {
            let version_str = link.version.as_deref().unwrap_or("unknown");
            println!("  {} (v{}) -> {}", link.name, version_str, link.path.display());
            
            if !link.linked_projects.is_empty() {
                println!("    Linked to {} project(s):", link.linked_projects.len());
                for project in &link.linked_projects {
                    println!("      {}", project.display());
                }
            }
        }
    }

    pub fn add_linked_project(&mut self, package_name: &str, project_path: PathBuf) -> Result<()> {
        let link = self.links.get_mut(package_name)
            .ok_or_else(|| SpineError::PackageNotFound(package_name.to_string()))?;
        
        let canonical_path = project_path.canonicalize()
            .unwrap_or(project_path);
        
        if !link.linked_projects.contains(&canonical_path) {
            link.linked_projects.push(canonical_path);
        }
        
        Ok(())
    }

    pub fn remove_linked_project(&mut self, package_name: &str, project_path: &PathBuf) -> Result<()> {
        let link = self.links.get_mut(package_name)
            .ok_or_else(|| SpineError::PackageNotFound(package_name.to_string()))?;
        
        let canonical_path = project_path.canonicalize()
            .unwrap_or_else(|_| project_path.clone());
        
        link.linked_projects.retain(|p| p != &canonical_path);
        
        Ok(())
    }

    pub fn verify_and_clean_links(&mut self) -> Result<Vec<String>> {
        let mut removed_links = Vec::new();
        let package_names: Vec<String> = self.links.keys().cloned().collect();
        
        for package_name in package_names {
            let mut valid_projects = Vec::new();
            let linked_projects = self.links.get(&package_name).unwrap().linked_projects.clone();
            
            for project_path in &linked_projects {
                if Self::is_package_linked_in_project_static(&package_name, project_path) {
                    valid_projects.push(project_path.clone());
                } else {
                    removed_links.push(format!("{} from {}", package_name, project_path.display()));
                }
            }
            
            if let Some(link) = self.links.get_mut(&package_name) {
                link.linked_projects = valid_projects;
            }
        }
        
        Ok(removed_links)
    }

    fn is_package_linked_in_project(&self, package_name: &str, project_path: &PathBuf) -> bool {
        Self::is_package_linked_in_project_static(package_name, project_path)
    }

    pub fn is_package_linked_in_project_static(package_name: &str, project_path: &PathBuf) -> bool {
        let node_modules = project_path.join("node_modules");
        if !node_modules.exists() {
            return false;
        }
        
        let package_path = if package_name.starts_with('@') {
            let parts: Vec<&str> = package_name.splitn(2, '/').collect();
            if parts.len() == 2 {
                node_modules.join(parts[0]).join(parts[1])
            } else {
                node_modules.join(package_name)
            }
        } else {
            node_modules.join(package_name)
        };
        
        // Check if it's a valid symlink pointing to an existing target
        package_path.is_symlink() && 
        package_path.read_link().is_ok() && 
        package_path.exists()
    }

    pub fn sync_with_filesystem(&mut self) -> Result<SyncReport> {
        let mut report = SyncReport::new();
        let current_dir = std::env::current_dir()?;
        
        // Check all configured packages for invalid links
        for (package_name, package_link) in &mut self.links {
            let mut valid_projects = Vec::new();
            
            for project_path in &package_link.linked_projects {
                let is_actually_linked = Self::is_package_linked_in_project_static(package_name, project_path);
                
                if is_actually_linked {
                    valid_projects.push(project_path.clone());
                } else {
                    report.removed_invalid_links.push(format!("{} from {}", package_name, project_path.display()));
                }
            }
            
            package_link.linked_projects = valid_projects;
            
            // Check if package is linked to current project but not in config
            if Self::is_package_linked_in_project_static(package_name, &current_dir) {
                if !package_link.linked_projects.contains(&current_dir) {
                    package_link.linked_projects.push(current_dir.clone());
                    report.added_missing_links.push(format!("{} to {}", package_name, current_dir.display()));
                }
            }
        }
        
        // Detect packages linked but not in config
        if let Ok(linked_packages) = crate::npm::NpmManager::get_linked_packages_static() {
            for package_name in linked_packages {
                if !self.links.contains_key(&package_name) {
                    report.untracked_links.push(package_name);
                }
            }
        }
        
        Ok(report)
    }

    pub fn get_links(&self) -> Vec<&PackageLink> {
        self.links.values().collect()
    }
    
    pub fn enable_auto_completion(&mut self, shell: Option<String>, script_path: Option<PathBuf>) -> Result<()> {
        self.completion.auto_regenerate = true;
        
        // Detect shell if not provided
        let detected_shell = shell.or_else(|| Platform::detect_current_shell());
        self.completion.shell = detected_shell.clone();
        
        // Set default script path if not provided
        if script_path.is_none() && detected_shell.is_some() {
            self.completion.script_path = Self::get_default_completion_path(&detected_shell.as_ref().unwrap());
        } else {
            self.completion.script_path = script_path;
        }
        
        // Initial generation
        self.regenerate_completion()?;
        self.save()?;
        
        if let Some(shell) = &self.completion.shell {
            if let Some(path) = &self.completion.script_path {
                println!("Auto-completion enabled for {} shell", shell);
                println!("Completion script: {}", path.display());
                println!("Add this to your shell config:");
                match shell.as_str() {
                    "bash" => println!("  echo 'source {}' >> ~/.bashrc", path.display()),
                    "zsh" => println!("  echo 'source {}' >> ~/.zshrc", path.display()),
                    "fish" => println!("  # Fish completion is automatically loaded from: {}", path.display()),
                    _ => println!("  source {}", path.display()),
                }
            }
        }
        
        Ok(())
    }
    
    pub fn disable_auto_completion(&mut self) -> Result<()> {
        self.completion.auto_regenerate = false;
        self.save()?;
        println!("Auto-completion disabled");
        Ok(())
    }
    
    fn regenerate_completion(&self) -> Result<()> {
        if !self.completion.auto_regenerate {
            return Ok(());
        }
        
        let shell = self.completion.shell.as_ref()
            .ok_or_else(|| SpineError::Config("No shell configured for auto-completion".to_string()))?;
        
        let script_path = self.completion.script_path.as_ref()
            .ok_or_else(|| SpineError::Config("No script path configured for auto-completion".to_string()))?;
        
        // Ensure parent directory exists
        if let Some(parent) = script_path.parent() {
            fs::create_dir_all(parent)?;
        }
        
        // Generate completion script
        let shell_enum = match shell.as_str() {
            "bash" => clap_complete::Shell::Bash,
            "zsh" => clap_complete::Shell::Zsh,
            "fish" => clap_complete::Shell::Fish,
            "powershell" => clap_complete::Shell::PowerShell,
            "elvish" => clap_complete::Shell::Elvish,
            _ => return Err(SpineError::Config(format!("Unsupported shell: {}", shell)).into()),
        };
        
        let mut cmd = crate::cli::Cli::command();
        let mut output = Vec::new();
        crate::completion::generate_completions(shell_enum, &mut cmd, "spine", &mut output);
        
        fs::write(script_path, output)?;
        
        Ok(())
    }
    
    // Moved to platform.rs - use Platform::detect_current_shell() instead
    
    fn get_default_completion_path(shell: &str) -> Option<PathBuf> {
        let home_dir = dirs::home_dir()?;
        Platform::get_completion_script_path(shell, &home_dir)
    }
}

#[derive(Debug)]
pub struct SyncReport {
    pub removed_invalid_links: Vec<String>,
    pub added_missing_links: Vec<String>,
    pub untracked_links: Vec<String>,
}

impl SyncReport {
    pub fn new() -> Self {
        Self {
            removed_invalid_links: Vec::new(),
            added_missing_links: Vec::new(),
            untracked_links: Vec::new(),
        }
    }
}