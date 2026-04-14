use rusqlite::params;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Node {
    pub id: String,
    #[serde(rename = "type")]
    pub node_type: String,
    pub name: String,
    pub file_path: Option<String>,
    pub start_line: Option<i64>,
    pub end_line: Option<i64>,
    pub language: Option<String>,
    pub metadata: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Edge {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<i64>,
    pub source_id: String,
    pub target_id: String,
    #[serde(rename = "type")]
    pub edge_type: String,
    pub metadata: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Community {
    pub id: i64,
    pub label: String,
    pub node_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileHash {
    pub file_path: String,
    pub content_hash: String,
    pub last_indexed: String,
}

pub struct FileManifestEntry {
    pub file_path: String,
    pub language: String,
    pub size: i64,
    pub content_hash: String,
}

pub fn open_database(db_path: &PathBuf) -> Result<rusqlite::Connection, rusqlite::Error> {
    if let Some(parent) = db_path.parent() {
        std::fs::create_dir_all(parent).ok();
    }
    let db = rusqlite::Connection::open(db_path)?;

    db.execute_batch(
        "
        PRAGMA journal_mode = WAL;
        PRAGMA foreign_keys = ON;
        
        CREATE TABLE IF NOT EXISTS file_manifest (
            file_path TEXT PRIMARY KEY,
            language TEXT NOT NULL,
            size INTEGER NOT NULL,
            content_hash TEXT NOT NULL
        );
        
        CREATE TABLE IF NOT EXISTS nodes (
            id TEXT PRIMARY KEY,
            type TEXT NOT NULL,
            name TEXT NOT NULL,
            file_path TEXT,
            start_line INTEGER,
            end_line INTEGER,
            language TEXT,
            metadata TEXT
        );
        
        CREATE TABLE IF NOT EXISTS edges (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            source_id TEXT NOT NULL REFERENCES nodes(id),
            target_id TEXT NOT NULL REFERENCES nodes(id),
            type TEXT NOT NULL,
            metadata TEXT
        );
        
        CREATE TABLE IF NOT EXISTS communities (
            id INTEGER PRIMARY KEY,
            label TEXT NOT NULL,
            node_ids TEXT NOT NULL
        );
        
        CREATE TABLE IF NOT EXISTS file_hashes (
            file_path TEXT PRIMARY KEY,
            content_hash TEXT NOT NULL,
            last_indexed TEXT NOT NULL
        );
        
        CREATE INDEX IF NOT EXISTS idx_edges_source ON edges(source_id);
        CREATE INDEX IF NOT EXISTS idx_edges_target ON edges(target_id);
        CREATE INDEX IF NOT EXISTS idx_edges_type ON edges(type);
        CREATE INDEX IF NOT EXISTS idx_nodes_type ON nodes(type);
        CREATE INDEX IF NOT EXISTS idx_nodes_file ON nodes(file_path);
    ",
    )?;

    Ok(db)
}

pub fn replace_all(
    db: &rusqlite::Connection,
    manifest: &[crate::scanner::FileManifest],
    nodes: &[Node],
    edges: &[Edge],
    communities: &[Community],
    hashes: &[FileHash],
) -> Result<(), rusqlite::Error> {
    db.execute("BEGIN", [])?;

    db.execute("DELETE FROM edges", [])?;
    db.execute("DELETE FROM nodes", [])?;
    db.execute("DELETE FROM communities", [])?;
    db.execute("DELETE FROM file_hashes", [])?;
    db.execute("DELETE FROM file_manifest", [])?;

    let mut node_stmt = db.prepare_cached("INSERT OR REPLACE INTO nodes (id, type, name, file_path, start_line, end_line, language, metadata) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)")?;
    for node in nodes {
        node_stmt.execute(params![
            node.id,
            node.node_type,
            node.name,
            node.file_path,
            node.start_line,
            node.end_line,
            node.language,
            node.metadata.to_string()
        ])?;
    }

    let mut edge_stmt = db.prepare_cached(
        "INSERT INTO edges (source_id, target_id, type, metadata) VALUES (?1, ?2, ?3, ?4)",
    )?;
    for edge in edges {
        edge_stmt.execute(params![
            edge.source_id,
            edge.target_id,
            edge.edge_type,
            edge.metadata.to_string()
        ])?;
    }

    let mut comm_stmt = db.prepare_cached(
        "INSERT OR REPLACE INTO communities (id, label, node_ids) VALUES (?1, ?2, ?3)",
    )?;
    for comm in communities {
        comm_stmt.execute(params![
            comm.id,
            comm.label,
            serde_json::to_string(&comm.node_ids).unwrap_or_else(|_| "[]".to_string())
        ])?;
    }

    let mut hash_stmt = db.prepare_cached("INSERT OR REPLACE INTO file_hashes (file_path, content_hash, last_indexed) VALUES (?1, ?2, ?3)")?;
    for hash in hashes {
        hash_stmt.execute(params![
            &hash.file_path,
            &hash.content_hash,
            &hash.last_indexed
        ])?;
    }

    let mut manifest_stmt = db.prepare_cached("INSERT INTO file_manifest (file_path, language, size, content_hash) VALUES (?1, ?2, ?3, ?4)")?;
    for item in manifest {
        manifest_stmt.execute(params![
            &item.file_path,
            &item.language,
            item.size as i64,
            &item.content_hash
        ])?;
    }

    db.execute("COMMIT", [])?;
    Ok(())
}
