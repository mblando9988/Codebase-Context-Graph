use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

pub struct FileManifest {
    pub file_path: String,
    pub absolute_path: PathBuf,
    pub language: String,
    pub size: u64,
    pub content_hash: String,
    pub content: String,
}

pub fn scan_project(
    project_root: &Path,
    ignore_patterns: &[String],
) -> Result<Vec<FileManifest>, Box<dyn std::error::Error>> {
    let mut manifest = Vec::new();
    let ignore_dirs: Vec<&str> = vec![
        ".git",
        "node_modules",
        "dist",
        "build",
        "vendor",
        "venv",
        ".venv",
        "env",
        ".env",
        "site-packages",
        "__pycache__",
        ".codebase-context",
        ".pytest_cache",
        ".ruff_cache",
        "logs",
        "tmp",
        "archive",
        "checkpoints",
        "data",
        "db",
        "target",
    ];

    for entry in WalkDir::new(project_root).follow_links(false) {
        let entry = entry?;
        let path = entry.path();

        if !entry.file_type().is_file() {
            continue;
        }

        let relative = path.strip_prefix(project_root).unwrap_or(path);
        let relative_str = relative.to_string_lossy().replace('\\', "/");

        if should_ignore(&relative_str, ignore_patterns, &ignore_dirs) {
            continue;
        }

        let language = match crate::config::detect_language(&relative_str) {
            Some(lang) => lang,
            None => continue,
        };

        let content = match fs::read(path) {
            Ok(c) => c,
            Err(_) => continue,
        };

        if looks_binary(&content) {
            continue;
        }

        let hash = format!("{:x}", Sha256::digest(&content));

        manifest.push(FileManifest {
            file_path: relative_str,
            absolute_path: path.to_path_buf(),
            language,
            size: content.len() as u64,
            content_hash: hash,
            content: String::from_utf8_lossy(&content).into_owned(),
        });
    }

    manifest.sort_by(|a, b| a.file_path.cmp(&b.file_path));
    Ok(manifest)
}

fn should_ignore(path: &str, patterns: &[String], ignore_dirs: &[&str]) -> bool {
    let segments: Vec<&str> = path.split('/').collect();

    if segments.iter().any(|s| ignore_dirs.contains(s)) {
        return true;
    }

    for pattern in patterns {
        if pattern.starts_with("*.") {
            let ext = &pattern[1..];
            if path.ends_with(ext) {
                return true;
            }
        } else if pattern.ends_with("/**") {
            let prefix = &pattern[..pattern.len() - 3];
            if path == prefix || path.starts_with(&format!("{}/", prefix)) {
                return true;
            }
        } else if path == pattern {
            return true;
        }
    }

    false
}

fn looks_binary(content: &[u8]) -> bool {
    let limit = std::cmp::min(content.len(), 1024);
    let mut suspicious = 0;

    for &byte in &content[..limit] {
        if byte == 0 {
            return true;
        }
        if (byte < 7 || byte > 14) && byte < 32 && byte != 9 {
            suspicious += 1;
        }
    }

    suspicious > 16
}
