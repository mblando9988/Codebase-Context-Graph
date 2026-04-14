use crate::config;
use std::io::{self, BufRead, Write};
use std::path::PathBuf;

pub fn run(project_root: &PathBuf) -> Result<(), Box<dyn std::error::Error>> {
    eprintln!("Codebase Context Graph MCP server starting");
    eprintln!("Project root: {}", project_root.display());

    let db_path = config::database_path(project_root);

    if !db_path.exists() {
        eprintln!("Error: Database not found at {}", db_path.display());
        eprintln!("Run 'codebase-context-graph index' first.");
        return Err("Database not found".into());
    }

    let db = crate::db::open_database(&db_path)?;

    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut stdout = stdout.lock();

    for line in stdin.lock().lines() {
        let line = match line {
            Ok(l) => l,
            Err(_) => break,
        };

        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        let result = handle_tool(&db, &line);
        let response = serde_json::to_string(&result)?;
        writeln!(stdout, "{}", response)?;
        stdout.flush()?;
    }

    Ok(())
}

fn handle_tool(db: &rusqlite::Connection, line: &str) -> serde_json::Value {
    let request: serde_json::Value = match serde_json::from_str(line) {
        Ok(v) => v,
        Err(e) => return serde_json::json!({"error": format!("Parse error: {}", e)}),
    };

    let method = request.get("method").and_then(|v| v.as_str()).unwrap_or("");
    let params = request
        .get("params")
        .cloned()
        .unwrap_or(serde_json::json!({}));

    match method {
        "get_overview" => get_overview(db),
        "get_module_map" => get_module_map(db),
        "get_file_structure" => get_file_structure(db, &params),
        "find_hubs" => find_hubs(db, &params),
        "search_symbols" => search_symbols(db, &params),
        "get_node_detail" => get_node_detail(db, &params),
        _ => serde_json::json!({"error": format!("Unknown method: {}", method)}),
    }
}

fn get_overview(db: &rusqlite::Connection) -> serde_json::Value {
    let files: i64 = db
        .query_row("SELECT COUNT(*) FROM file_manifest", [], |r| r.get(0))
        .unwrap_or(0);
    let modules: i64 = db
        .query_row(
            "SELECT COUNT(*) FROM nodes WHERE type = 'MODULE'",
            [],
            |r| r.get(0),
        )
        .unwrap_or(0);
    let functions: i64 = db
        .query_row(
            "SELECT COUNT(*) FROM nodes WHERE type = 'FUNCTION'",
            [],
            |r| r.get(0),
        )
        .unwrap_or(0);
    let classes: i64 = db
        .query_row("SELECT COUNT(*) FROM nodes WHERE type = 'CLASS'", [], |r| {
            r.get(0)
        })
        .unwrap_or(0);

    format_toon(
        "overview",
        &["metric", "value"],
        &[
            vec!["files".to_string(), files.to_string()],
            vec!["modules".to_string(), modules.to_string()],
            vec!["functions".to_string(), functions.to_string()],
            vec!["classes".to_string(), classes.to_string()],
        ],
    )
}

fn get_module_map(db: &rusqlite::Connection) -> serde_json::Value {
    let mut stmt = db
        .prepare("SELECT id, name FROM nodes WHERE type = 'MODULE' ORDER BY name")
        .unwrap();
    let rows: Vec<Vec<String>> = stmt
        .query_map([], |r| Ok(vec![r.get::<_, String>(0)?, r.get(1)?]))
        .unwrap()
        .filter_map(Result::ok)
        .collect();

    format_toon("modules", &["id", "name"], &rows)
}

fn get_file_structure(db: &rusqlite::Connection, params: &serde_json::Value) -> serde_json::Value {
    let file_path = params
        .get("file_path")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    let mut stmt = db.prepare("SELECT type, name, start_line, end_line FROM nodes WHERE file_path = ? ORDER BY start_line, name").unwrap();
    let rows: Vec<Vec<String>> = stmt
        .query_map([file_path], |r| {
            Ok(vec![
                r.get::<_, String>(0)?,
                r.get(1)?,
                r.get::<_, i64>(2)?.to_string(),
                r.get::<_, i64>(3)?.to_string(),
            ])
        })
        .unwrap()
        .filter_map(Result::ok)
        .collect();

    format_toon(
        "file_structure",
        &["type", "name", "start_line", "end_line"],
        &rows,
    )
}

fn find_hubs(db: &rusqlite::Connection, params: &serde_json::Value) -> serde_json::Value {
    let limit = params.get("limit").and_then(|v| v.as_i64()).unwrap_or(10) as usize;

    let mut stmt = db.prepare(
        "SELECT id, type, name, json_extract(metadata, '$.hubScore') AS hub_score FROM nodes ORDER BY hub_score DESC LIMIT ?"
    ).unwrap();

    let rows: Vec<Vec<String>> = stmt
        .query_map([limit as i64], |r| {
            Ok(vec![
                r.get(0)?,
                r.get(1)?,
                r.get(2)?,
                r.get::<_, String>(3)?,
            ])
        })
        .unwrap()
        .filter_map(Result::ok)
        .collect();

    format_toon("hubs", &["id", "type", "name", "hub_score"], &rows)
}

fn search_symbols(db: &rusqlite::Connection, params: &serde_json::Value) -> serde_json::Value {
    let query = params
        .get("query")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_lowercase();
    let limit = params.get("limit").and_then(|v| v.as_i64()).unwrap_or(20) as usize;

    let mut stmt = db.prepare(
        "SELECT id, type, name, file_path, start_line FROM nodes WHERE type IN ('FUNCTION', 'CLASS', 'VARIABLE') ORDER BY name LIMIT ?"
    ).unwrap();

    let rows: Vec<Vec<String>> = stmt
        .query_map([limit as i64], |r| {
            Ok(vec![
                r.get(0)?,
                r.get(1)?,
                r.get(2)?,
                r.get(3)?,
                r.get::<_, i64>(4)?.to_string(),
            ])
        })
        .unwrap()
        .filter_map(Result::ok)
        .filter(|r| r[2].to_lowercase().contains(&query))
        .take(limit)
        .collect();

    format_toon(
        "symbols",
        &["id", "type", "name", "file_path", "line"],
        &rows,
    )
}

fn get_node_detail(db: &rusqlite::Connection, params: &serde_json::Value) -> serde_json::Value {
    let node_id = params.get("node_id").and_then(|v| v.as_str()).unwrap_or("");

    db.query_row(
        "SELECT id, type, name, file_path, start_line, end_line, metadata FROM nodes WHERE id = ?",
        [node_id],
        |r| {
            Ok(serde_json::json!({
                "@id": r.get::<_, String>(0)?,
                "@type": format!("code:{}", r.get::<_, String>(1)?),
                "name": r.get::<_, String>(2)?,
                "filePath": r.get::<_, String>(3)?,
                "startLine": r.get::<_, i64>(4)?,
                "endLine": r.get::<_, i64>(5)?,
                "metadata": r.get::<_, String>(6)?,
            }))
        },
    )
    .unwrap_or(serde_json::json!({"error": "not found"}))
}

fn format_toon(label: &str, headers: &[&str], rows: &[Vec<String>]) -> serde_json::Value {
    let mut lines = vec![format!("{} [{}]", label, rows.len())];
    lines.push(format!("  {}", headers.join(" ")));
    for row in rows {
        lines.push(format!("  {}", row.join(" ")));
    }

    serde_json::json!({ "content": lines.join("\n") })
}
