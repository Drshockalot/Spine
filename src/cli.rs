use anyhow::Result;
use clap::{CommandFactory, Parser, Subcommand, ValueHint};
use clap_complete;
use std::io;
use std::path::PathBuf;
use crate::config::Config;
use crate::completion;
use crate::npm::NpmManager;
use crate::scanner::Scanner;
use crate::tui::TuiApp;

#[derive(Parser)]
#[command(name = "spine")]
#[command(about = "A modern replacement for npm link with interactive configuration management")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Commands>,
}

#[derive(Subcommand)]
pub enum Commands {
    #[command(about = "Launch interactive configuration interface")]
    Interactive,
    #[command(about = "List current package links")]
    List,
    #[command(about = "Add a new package link")]
    Add {
        #[arg(help = "Package name (auto-detected from package.json if not provided)")]
        package: Option<String>,
        #[arg(help = "Local path to package (defaults to current directory)")]
        path: Option<String>,
    },
    #[command(about = "Remove a package link")]
    Remove {
        #[arg(help = "Package name", value_hint = ValueHint::Other)]
        package: String,
    },
    #[command(about = "Link all configured packages to current project")]
    LinkAll,
    #[command(about = "Link specific package to current project")]
    Link {
        #[arg(help = "Package name", value_hint = ValueHint::Other)]
        package: String,
    },
    #[command(about = "Show npm link status for current project")]
    Status {
        #[arg(long, help = "Show detailed information including versions and paths")]
        detailed: bool,
        #[arg(long, help = "Check health of all links (broken symlinks, missing packages)")]
        health: bool,
        #[arg(long, help = "Output in JSON format for scripts/CI")]
        json: bool,
    },
    #[command(about = "Unlink specific package from current project")]
    Unlink {
        #[arg(help = "Package name", value_hint = ValueHint::Other)]
        package: String,
    },
    #[command(about = "Unlink all packages from current project")]
    UnlinkAll,
    #[command(about = "Verify and clean up broken package links")]
    Verify,
    #[command(about = "Scan for local packages in workspace")]
    Scan {
        #[arg(long, help = "Automatically add discovered packages")]
        add: bool,
        #[arg(long, help = "Search path (defaults to current directory)")]
        path: Option<String>,
    },
    #[command(about = "Restore package links according to Spine configuration (useful after npm install)")]
    Sync,
    #[command(about = "Open configuration file in editor")]
    ConfigEdit,
    #[command(about = "Build Angular libraries")]
    Build {
        #[arg(help = "Library name to build (optional)")]
        library: Option<String>,
        #[arg(long, help = "Build all linked libraries")]
        all: bool,
        #[arg(long, help = "Watch mode for continuous rebuilding")]
        watch: bool,
        #[arg(long, help = "Build only affected libraries")]
        affected: bool,
    },
    #[command(about = "Generate shell completion scripts")]
    GenerateCompletion {
        #[arg(help = "Shell to generate completions for")]
        shell: clap_complete::Shell,
    },
    #[command(about = "Enable automatic completion script regeneration")]
    EnableAutoCompletion {
        #[arg(long, help = "Shell to generate completions for (auto-detected if not specified)")]
        shell: Option<String>,
        #[arg(long, help = "Path to save completion script (uses default if not specified)")]
        path: Option<String>,
    },
    #[command(about = "Disable automatic completion script regeneration")]
    DisableAutoCompletion,
    #[command(about = "Angular CLI integration commands")]
    Ng {
        #[command(subcommand)]
        command: NgCommands,
    },
    #[command(about = "Proxy Angular CLI commands with Spine enhancements")]
    NgProxy {
        #[arg(trailing_var_arg = true, help = "Angular CLI command and arguments")]
        args: Vec<String>,
    },
    #[command(about = "Start development server with automatic library rebuilding")]
    Serve {
        #[arg(long, help = "Enable automatic library rebuilding")]
        with_libs: bool,
        #[arg(long, help = "Port for development server")]
        port: Option<u16>,
        #[arg(long, help = "Enable Hot Module Replacement")]
        hmr: bool,
        #[arg(help = "Application project to serve (auto-detected if not specified)")]
        project: Option<String>,
    },
    #[command(about = "Debug Angular workspace and library detection")]
    Debug {
        #[arg(long, help = "Show detailed Angular workspace information")]
        workspace: bool,
        #[arg(long, help = "Show library matching details")]
        libs: bool,
    },
    #[command(about = "Build and publish a package to npm")]
    Publish {
        #[arg(help = "Package name to build and publish")]
        package: String,
        #[arg(long, help = "Skip build step and publish directly")]
        skip_build: bool,
        #[arg(long, help = "Dry run - show what would be published without actually publishing")]
        dry_run: bool,
    },
    #[command(hide = true)]
    ListPackagesForCompletion,
    
    // Command aliases for better UX
    #[command(about = "Alias for 'serve'")]
    S {
        #[arg(long, help = "Enable automatic library rebuilding")]
        with_libs: bool,
        #[arg(long, help = "Port for development server")]
        port: Option<u16>,
        #[arg(long, help = "Enable Hot Module Replacement")]
        hmr: bool,
        #[arg(help = "Application project to serve (auto-detected if not specified)")]
        project: Option<String>,
    },
    #[command(about = "Alias for 'list'")]
    L,
    #[command(about = "Alias for 'add' with smart defaults")]
    A {
        #[arg(help = "Package name (auto-detected if not provided)")]
        package: Option<String>,
        #[arg(help = "Local path to package (defaults to current directory)")]
        path: Option<String>,
    },
    #[command(about = "Alias for 'ng generate'")]
    G {
        #[arg(help = "Schematic type (component, service, pipe, etc.)")]
        schematic: String,
        #[arg(help = "Name of the generated item")]
        name: String,
        #[arg(long, help = "Target library for generation")]
        lib: Option<String>,
        #[arg(trailing_var_arg = true, help = "Additional Angular CLI arguments")]
        args: Vec<String>,
    },
}

#[derive(Subcommand)]
pub enum NgCommands {
    #[command(about = "Generate Angular schematics with library context")]
    Generate {
        #[arg(help = "Schematic type (component, service, pipe, etc.)")]
        schematic: String,
        #[arg(help = "Name of the generated item")]
        name: String,
        #[arg(long, help = "Target library for generation")]
        lib: Option<String>,
        #[arg(trailing_var_arg = true, help = "Additional Angular CLI arguments")]
        args: Vec<String>,
    },
}

impl Cli {
    pub fn run(&self) -> Result<()> {
        let mut config = Config::load_or_create()?;

        match &self.command {
            Some(Commands::Interactive) | None => {
                let mut app = TuiApp::new(config)?;
                app.run()?;
            }
            Some(Commands::List) => {
                config.list_links();
            }
            Some(Commands::Add { package, path }) => {
                let (detected_package, detected_path) = Self::detect_package_info(package, path)?;
                config.add_link(detected_package.clone(), detected_path.clone())?;
                config.save()?;
                println!("Added link: {} -> {}", detected_package, detected_path);
            }
            Some(Commands::Remove { package }) => {
                config.remove_link(package)?;
                config.save()?;
                println!("Removed link: {}", package);
            }
            Some(Commands::LinkAll) => {
                NpmManager::link_all(&mut config)?;
                config.save()?;
            }
            Some(Commands::Link { package }) => {
                NpmManager::link_package(&mut config, package)?;
                config.save()?;
            }
            Some(Commands::Status { detailed, health, json }) => {
                NpmManager::show_enhanced_status(&config, *detailed, *health, *json)?;
            }
            Some(Commands::Unlink { package }) => {
                NpmManager::unlink_package(&mut config, package)?;
                config.save()?;
            }
            Some(Commands::UnlinkAll) => {
                NpmManager::unlink_all(&mut config)?;
                config.save()?;
            }
            Some(Commands::Verify) => {
                NpmManager::verify_links(&mut config)?;
            }
            Some(Commands::Scan { add, path }) => {
                Scanner::scan_packages(*add, path.as_deref())?;
            }
            Some(Commands::Sync) => {
                Scanner::sync_links()?;
            }
            Some(Commands::ConfigEdit) => {
                Scanner::open_config_editor()?;
            }
            Some(Commands::Build { library, all, watch, affected }) => {
                crate::angular::build_command(library.clone(), *all, *watch, *affected)?;
            }
            Some(Commands::GenerateCompletion { shell }) => {
                Self::generate_completion(*shell)?;
            }
            Some(Commands::EnableAutoCompletion { shell, path }) => {
                let script_path = path.as_ref().map(|p| PathBuf::from(p));
                config.enable_auto_completion(shell.clone(), script_path)?;
            }
            Some(Commands::DisableAutoCompletion) => {
                config.disable_auto_completion()?;
            }
            Some(Commands::Ng { command }) => {
                match command {
                    NgCommands::Generate { schematic, name, lib, args } => {
                        crate::angular_cli::ng_generate_command(
                            schematic,
                            name,
                            lib.as_deref(),
                            args.clone()
                        )?;
                    }
                }
            }
            Some(Commands::NgProxy { args }) => {
                crate::angular_cli::ng_proxy_command(args.clone())?;
            }
            Some(Commands::Serve { with_libs, port, hmr, project }) => {
                if *with_libs {
                    crate::angular_cli::serve_with_libs_command(*port, *hmr, project.as_deref())?;
                } else {
                    // Regular serve command - just proxy to Angular CLI
                    let mut args = vec!["serve".to_string()];
                    if let Some(p) = port {
                        args.extend(vec!["--port".to_string(), p.to_string()]);
                    }
                    if *hmr {
                        args.push("--hmr".to_string());
                    }
                    if let Some(proj) = project {
                        args.push(proj.clone());
                    }
                    crate::angular_cli::ng_proxy_command(args)?;
                }
            }
            Some(Commands::Debug { workspace, libs }) => {
                crate::angular_cli::debug_command(*workspace, *libs)?;
            }
            Some(Commands::Publish { package, skip_build, dry_run }) => {
                crate::angular::publish_command(&config, package, *skip_build, *dry_run)?;
            }
            Some(Commands::ListPackagesForCompletion) => {
                completion::list_packages_for_completion()?;
            }
            
            // Handle aliases
            Some(Commands::S { with_libs, port, hmr, project }) => {
                if *with_libs {
                    crate::angular_cli::serve_with_libs_command(*port, *hmr, project.as_deref())?;
                } else {
                    let mut args = vec!["serve".to_string()];
                    if let Some(p) = port {
                        args.extend(vec!["--port".to_string(), p.to_string()]);
                    }
                    if *hmr {
                        args.push("--hmr".to_string());
                    }
                    if let Some(proj) = project {
                        args.push(proj.clone());
                    }
                    crate::angular_cli::ng_proxy_command(args)?;
                }
            }
            Some(Commands::L) => {
                config.list_links();
            }
            Some(Commands::A { package, path }) => {
                let (detected_package, detected_path) = Self::detect_package_info(package, path)?;
                config.add_link(detected_package.clone(), detected_path.clone())?;
                config.save()?;
                println!("Added link: {} -> {}", detected_package, detected_path);
            }
            Some(Commands::G { schematic, name, lib, args }) => {
                crate::angular_cli::ng_generate_command(
                    schematic,
                    name,
                    lib.as_deref(),
                    args.clone()
                )?;
            }
        }

        Ok(())
    }

    fn detect_package_info(package: &Option<String>, path: &Option<String>) -> Result<(String, String)> {
        let detected_path = path.as_deref().unwrap_or(".").to_string();
        let path_buf = std::path::PathBuf::from(&detected_path);
        
        // Ensure the path exists
        if !path_buf.exists() {
            return Err(anyhow::anyhow!("Path does not exist: {}", detected_path));
        }
        
        // Try to detect package name from package.json if not provided
        let detected_package = if let Some(pkg) = package {
            pkg.clone()
        } else {
            // Look for package.json in the specified path
            let package_json_path = path_buf.join("package.json");
            if package_json_path.exists() {
                match crate::package::get_package_name(&package_json_path) {
                    Ok(name) => {
                        println!("📦 Auto-detected package name: {}", name);
                        name
                    }
                    Err(_) => {
                        return Err(anyhow::anyhow!(
                            "Could not detect package name from package.json. Please provide package name explicitly."
                        ));
                    }
                }
            } else {
                return Err(anyhow::anyhow!(
                    "No package.json found in {}. Please provide package name explicitly or ensure you're in a package directory.",
                    detected_path
                ));
            }
        };
        
        // Convert to absolute path for consistency
        let absolute_path = path_buf.canonicalize()
            .map_err(|_| anyhow::anyhow!("Could not resolve absolute path for: {}", detected_path))?
            .to_string_lossy()
            .to_string();
        
        Ok((detected_package, absolute_path))
    }

    fn generate_completion(shell: clap_complete::Shell) -> Result<()> {
        let mut cmd = Self::command();
        completion::generate_completions(
            shell,
            &mut cmd,
            "spine",
            &mut io::stdout(),
        );
        Ok(())
    }
}