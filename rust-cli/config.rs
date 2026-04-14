use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

pub fn default_ignore_patterns() -> Vec<String> {
    vec![
        "node_modules/**".to_string(),
        ".git/**".to_string(),
        "dist/**".to_string(),
        "build/**".to_string(),
        "vendor/**".to_string(),
        "venv/**".to_string(),
        ".venv/**".to_string(),
        "env/**".to_string(),
        ".env/**".to_string(),
        "site-packages/**".to_string(),
        "__pycache__/**".to_string(),
        ".codebase-context/**".to_string(),
        ".pytest_cache/**".to_string(),
        ".ruff_cache/**".to_string(),
        "logs/**".to_string(),
        "tmp/**".to_string(),
        "archive/**".to_string(),
        "checkpoints/**".to_string(),
        "data/**".to_string(),
        "db/**".to_string(),
        "*.min.js".to_string(),
        "*.map".to_string(),
        "*.lock".to_string(),
        "*.log".to_string(),
    ]
}

pub fn detect_language(file_path: &str) -> Option<String> {
    let ext = Path::new(file_path).extension()?.to_str()?;
    match ext {
        "js" | "cjs" | "mjs" | "jsx" => Some("javascript".to_string()),
        "ts" | "tsx" => Some("typescript".to_string()),
        "py" => Some("python".to_string()),
        "rs" => Some("rust".to_string()),
        "sh" | "bash" => Some("bash".to_string()),
        _ => None,
    }
}

pub fn context_dir(project_root: &Path) -> PathBuf {
    project_root.join(".codebase-context")
}

pub fn config_path(project_root: &Path) -> PathBuf {
    context_dir(project_root).join("config.json")
}

pub fn database_path(project_root: &Path) -> PathBuf {
    context_dir(project_root).join("graph.db")
}

pub fn graph_json_path(project_root: &Path) -> PathBuf {
    context_dir(project_root).join("graph.json")
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub version: String,
    pub project_name: String,
    pub languages: Vec<String>,
    pub ignore_patterns: Vec<String>,
    #[serde(default)]
    pub analysis_mode: Option<String>,
}

pub fn ensure_project_config(project_root: &Path) -> Result<Config, Box<dyn std::error::Error>> {
    let context = context_dir(project_root);
    fs::create_dir_all(&context)?;

    let config = Config {
        version: "1.0".to_string(),
        project_name: project_root
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string(),
        languages: vec![],
        ignore_patterns: default_ignore_patterns(),
        analysis_mode: Some("standard".to_string()),
    };

    save_project_config(project_root, &config)?;
    Ok(config)
}

pub fn load_project_config(project_root: &Path) -> Result<Config, Box<dyn std::error::Error>> {
    let path = config_path(project_root);
    if !path.exists() {
        return ensure_project_config(project_root);
    }
    let content = fs::read_to_string(&path)?;
    let config: Config = serde_json::from_str(&content)?;
    Ok(config)
}

pub fn save_project_config(
    project_root: &Path,
    config: &Config,
) -> Result<(), Box<dyn std::error::Error>> {
    let path = config_path(project_root);
    let content = serde_json::to_string_pretty(config)?;
    fs::write(&path, &content)?;
    Ok(())
}
