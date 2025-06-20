use std::fs;
use std::path::Path;
use anyhow::Result;
use serde_json::Value;
use crate::error::SpineError;

#[derive(Debug, Clone)]
pub struct PackageInfo {
    pub name: String,
    pub version: String,
    pub dependencies: Vec<String>,
    pub dev_dependencies: Vec<String>,
}

pub fn get_package_name(package_json_path: &Path) -> Result<String> {
    let content = fs::read_to_string(package_json_path)?;
    let json: Value = serde_json::from_str(&content)?;
    
    json.get("name")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .ok_or_else(|| SpineError::PackageJson("No name field found".to_string()).into())
}

pub fn get_package_version(package_json_path: &Path) -> Result<String> {
    let content = fs::read_to_string(package_json_path)?;
    let json: Value = serde_json::from_str(&content)?;
    
    json.get("version")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .ok_or_else(|| SpineError::PackageJson("No version field found".to_string()).into())
}

pub fn parse_package_json(package_json_path: &Path) -> Result<PackageInfo> {
    let content = fs::read_to_string(package_json_path)?;
    let json: Value = serde_json::from_str(&content)?;

    let name = json.get("name")
        .and_then(|v| v.as_str())
        .ok_or_else(|| SpineError::PackageJson("No name field found".to_string()))?
        .to_string();

    let version = json.get("version")
        .and_then(|v| v.as_str())
        .ok_or_else(|| SpineError::PackageJson("No version field found".to_string()))?
        .to_string();

    let dependencies = extract_dependencies(&json, "dependencies");
    let dev_dependencies = extract_dependencies(&json, "devDependencies");

    Ok(PackageInfo {
        name,
        version,
        dependencies,
        dev_dependencies,
    })
}

fn extract_dependencies(json: &Value, field: &str) -> Vec<String> {
    json.get(field)
        .and_then(|deps| deps.as_object())
        .map(|deps| deps.keys().cloned().collect())
        .unwrap_or_default()
}

pub fn validate_package_path(path: &Path) -> Result<bool> {
    if !path.exists() {
        return Ok(false);
    }

    let package_json = path.join("package.json");
    if !package_json.exists() {
        return Ok(false);
    }

    parse_package_json(&package_json).map(|_| true)
}