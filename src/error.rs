use thiserror::Error;

#[derive(Error, Debug)]
pub enum SpineError {
    #[error("Configuration error: {0}")]
    Config(String),

    #[error("Package.json parsing error: {0}")]
    PackageJson(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("TOML parsing error: {0}")]
    TomlParsing(#[from] toml::de::Error),

    #[error("JSON parsing error: {0}")]
    JsonParsing(#[from] serde_json::Error),

    #[error("Invalid path: {0}")]
    InvalidPath(String),

    #[error("Package not found: {0}")]
    PackageNotFound(String),

    #[error("Package not found: '{package}'\n💡 {suggestion}")]
    PackageNotFoundWithSuggestion { package: String, suggestion: String },

    #[error("Angular workspace error: {message}\n💡 {suggestion}")]
    AngularWorkspace { message: String, suggestion: String },

    #[error("Command failed: {command}\n❌ {error}\n💡 {suggestion}")]
    CommandFailed { command: String, error: String, suggestion: String },
}

impl SpineError {
    pub fn package_not_found_with_suggestions(package: &str, available_packages: &[String]) -> Self {
        let suggestion = if available_packages.is_empty() {
            "No packages are currently configured. Use 'spine add <package> <path>' to add one.".to_string()
        } else {
            let similar = find_similar_names(package, available_packages);
            if similar.is_empty() {
                format!("Available packages: {}", available_packages.join(", "))
            } else {
                format!("Did you mean '{}'? Available: {}", similar[0], available_packages.join(", "))
            }
        };

        SpineError::PackageNotFoundWithSuggestion {
            package: package.to_string(),
            suggestion,
        }
    }

    pub fn angular_workspace_not_found(current_dir: &str) -> Self {
        SpineError::AngularWorkspace {
            message: format!("No angular.json found in {}", current_dir),
            suggestion: "Make sure you're in an Angular project root directory, or run 'ng new' to create a new project.".to_string(),
        }
    }

    pub fn command_failed_with_suggestion(command: &str, error: &str) -> Self {
        let suggestion = match command {
            cmd if cmd.contains("ng") => "Make sure Angular CLI is installed: npm install -g @angular/cli".to_string(),
            cmd if cmd.contains("npm") => "Make sure you're in a directory with package.json".to_string(),
            _ => "Check that all required tools are installed and accessible".to_string(),
        };

        SpineError::CommandFailed {
            command: command.to_string(),
            error: error.to_string(),
            suggestion,
        }
    }
}

// Simple string similarity algorithm (Levenshtein distance)
fn find_similar_names(target: &str, candidates: &[String]) -> Vec<String> {
    let mut similar: Vec<(String, usize)> = candidates
        .iter()
        .map(|candidate| {
            let distance = levenshtein_distance(target, candidate);
            (candidate.clone(), distance)
        })
        .filter(|(_, distance)| *distance <= 3) // Only consider if distance <= 3
        .collect();
    
    similar.sort_by_key(|(_, distance)| *distance);
    similar.into_iter().take(3).map(|(name, _)| name).collect()
}

fn levenshtein_distance(s1: &str, s2: &str) -> usize {
    let len1 = s1.len();
    let len2 = s2.len();
    let mut matrix = vec![vec![0; len2 + 1]; len1 + 1];

    for i in 1..=len1 {
        matrix[i][0] = i;
    }
    for j in 1..=len2 {
        matrix[0][j] = j;
    }

    for i in 1..=len1 {
        for j in 1..=len2 {
            let cost = if s1.chars().nth(i - 1) == s2.chars().nth(j - 1) { 0 } else { 1 };
            matrix[i][j] = std::cmp::min(
                std::cmp::min(matrix[i - 1][j] + 1, matrix[i][j - 1] + 1),
                matrix[i - 1][j - 1] + cost,
            );
        }
    }

    matrix[len1][len2]
}