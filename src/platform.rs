use std::process::Command;

/// Cross-platform utilities for command execution and path handling
pub struct Platform;

impl Platform {
    /// Get the correct command name for the current platform
    /// On Windows, adds .cmd extension for npm, ng, etc.
    #[cfg(target_os = "windows")]
    pub fn get_command_name(base_name: &str) -> String {
        match base_name {
            "npm" | "ng" | "npx" => format!("{}.cmd", base_name),
            _ => base_name.to_string(),
        }
    }

    #[cfg(not(target_os = "windows"))]
    pub fn get_command_name(base_name: &str) -> String {
        base_name.to_string()
    }

    /// Create a platform-appropriate Command for npm
    pub fn npm_command() -> Command {
        Command::new(Self::get_command_name("npm"))
    }

    /// Create a platform-appropriate Command for Angular CLI
    pub fn ng_command() -> Command {
        Command::new(Self::get_command_name("ng"))
    }

    /// Detect the current shell in a cross-platform way
    pub fn detect_current_shell() -> Option<String> {
        #[cfg(target_os = "windows")]
        {
            // Check for PowerShell first (both Windows PowerShell and PowerShell Core)
            if std::env::var("PSModulePath").is_ok() {
                return Some("powershell".to_string());
            }
            
            // Check COMSPEC for cmd.exe
            if let Ok(shell_path) = std::env::var("COMSPEC") {
                if shell_path.to_lowercase().contains("cmd") {
                    return Some("cmd".to_string());
                }
            }
            
            // Default to PowerShell on Windows
            return Some("powershell".to_string());
        }

        #[cfg(not(target_os = "windows"))]
        {
            // Check SHELL environment variable
            if let Ok(shell_path) = std::env::var("SHELL") {
                if let Some(shell_name) = std::path::Path::new(&shell_path).file_name() {
                    if let Some(shell_str) = shell_name.to_str() {
                        match shell_str {
                            "bash" => return Some("bash".to_string()),
                            "zsh" => return Some("zsh".to_string()),
                            "fish" => return Some("fish".to_string()),
                            _ => {}
                        }
                    }
                }
            }
            None
        }
    }

    /// Open a file with the default system application
    pub fn open_file_with_default_app(file_path: &std::path::Path) -> std::io::Result<std::process::ExitStatus> {
        #[cfg(target_os = "windows")]
        {
            Command::new("cmd")
                .args(&["/c", "start", ""])
                .arg(file_path)
                .status()
        }

        #[cfg(target_os = "macos")]
        {
            Command::new("open")
                .arg(file_path)
                .status()
        }

        #[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
        {
            Command::new("xdg-open")
                .arg(file_path)
                .status()
        }
    }

    /// Get platform-appropriate completion script path
    pub fn get_completion_script_path(shell: &str, home_dir: &std::path::Path) -> Option<std::path::PathBuf> {
        match shell {
            "bash" => Some(home_dir.join(".spine_completion.bash")),
            "zsh" => Some(home_dir.join(".spine_completion.zsh")),
            "fish" => {
                #[cfg(not(target_os = "windows"))]
                {
                    if let Some(config_dir) = dirs::config_dir() {
                        Some(config_dir.join("fish/completions/spine.fish"))
                    } else {
                        Some(home_dir.join(".config/fish/completions/spine.fish"))
                    }
                }
                #[cfg(target_os = "windows")]
                {
                    // Fish is rarely used on Windows, skip
                    None
                }
            },
            "powershell" | "cmd" => Some(home_dir.join("spine_completion.ps1")),
            _ => Some(home_dir.join(format!(".spine_completion.{}", shell))),
        }
    }
}