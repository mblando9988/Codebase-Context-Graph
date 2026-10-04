use crate::config;
use crate::symbols::SYMBOL_TYPES;
use rusqlite::types::Value as SqlValue;
use rusqlite::{Connection, OptionalExtension, params_from_iter};
use serde_json::{Value, json};
use std::io::{self, BufRead, Write};
use std::path::PathBuf;

/// Reads one JSON request per line on stdin and answers one JSON line on stdout.
/// This is a plain line protocol, not MCP.
pub fn run(project_root: &PathBuf) -> Result<(), Box<dyn std::error::Error>> {
    eprintln!("Codebase Context Graph query server starting (JSON lines on stdin/stdout)");
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

fn handle_tool(db: &Connection, line: &str) -> Value {
    let request: Value = match serde_json::from_str(line) {
        Ok(v) => v,
        Err(e) => return json!({"error": format!("Parse error: {}", e)}),
    };

    let method = request.get("method").and_then(|v| v.as_str()).unwrap_or("");
    let params = request.get("params").cloned().unwrap_or(json!({}));

    let result = match method {
        "get_overview" => get_overview(db),
        "get_module_map" => get_module_map(db),
        "get_file_structure" => get_file_structure(db, &params),
        "find_hubs" => find_hubs(db, &params),
        "search_symbols" => search_symbols(db, &params),
        "get_node_detail" => get_node_detail(db, &params),
        _ => Ok(json!({"error": format!("Unknown method: {}", method)})),
    };
    result.unwrap_or_else(|e| json!({"error": e.to_string()}))
}

type Reply = Result<Value, rusqlite::Error>;

/// Every cell is rendered according to its actual SQLite type, so a numeric column can
/// never make a row silently disappear.
fn cell(value: SqlValue) -> String {
    match value {
        SqlValue::Null => "-".to_string(),
        SqlValue::Integer(i) => i.to_string(),
        SqlValue::Real(f) => f.to_string(),
        SqlValue::Text(s) => s,
        SqlValue::Blob(_) => "<blob>".to_string(),
    }
}

fn query_rows(db: &Connection, sql: &str, params: &[SqlValue]) -> Result<Vec<Vec<String>>, rusqlite::Error> {
    let mut stmt = db.prepare(sql)?;
    let columns = stmt.column_count();
    let rows = stmt.query_map(params_from_iter(params.iter()), |row| {
        (0..columns).map(|i| row.get::<_, SqlValue>(i).map(cell)).collect()
    })?;
    rows.collect()
}

fn count(db: &Connection, sql: &str) -> Result<i64, rusqlite::Error> {
    db.query_row(sql, [], |r| r.get(0))
}

fn str_param<'a>(params: &'a Value, key: &str) -> Option<&'a str> {
    params.get(key).and_then(|v| v.as_str())
}

fn limit_param(params: &Value, default: i64) -> i64 {
    params
        .get("limit")
        .and_then(|v| v.as_i64())
        .unwrap_or(default)
        .clamp(1, 500)
}

fn in_list(types: &[&str]) -> String {
    types.iter().map(|t| format!("'{t}'")).collect::<Vec<_>>().join(",")
}

fn get_overview(db: &Connection) -> Reply {
    let nodes = |types: &str| count(db, &format!("SELECT COUNT(*) FROM nodes WHERE type IN ({types})"));
    let edges = |kind: &str| count(db, &format!("SELECT COUNT(*) FROM edges WHERE type = '{kind}'"));

    let mut rows = vec![
        vec!["files".to_string(), count(db, "SELECT COUNT(*) FROM file_manifest")?.to_string()],
        vec![
            "covered_files".to_string(),
            count(db, "SELECT COUNT(*) FROM file_manifest WHERE indexed_by IS NOT NULL")?.to_string(),
        ],
        vec!["modules".to_string(), nodes("'MODULE'")?.to_string()],
        vec!["functions".to_string(), nodes("'FUNCTION'")?.to_string()],
        vec!["methods".to_string(), nodes("'METHOD'")?.to_string()],
        vec![
            "types".to_string(),
            nodes(&in_list(&["CLASS", "INTERFACE", "STRUCT", "ENUM", "TRAIT", "TYPE", "TYPE_ALIAS"]))?.to_string(),
        ],
        vec!["calls".to_string(), edges("CALLS")?.to_string()],
        vec!["references".to_string(), edges("REFERENCES")?.to_string()],
        vec!["external_packages".to_string(), nodes("'EXTERNAL'")?.to_string()],
    ];
    for run in query_rows(
        db,
        "SELECT indexer, root, status, documents, message FROM index_runs ORDER BY id",
        &[],
    )? {
        let root = if run[1].is_empty() { ".".to_string() } else { run[1].clone() };
        let detail = if run[4].is_empty() {
            format!("{}, {} files", run[2], run[3])
        } else {
            format!("{}: {}", run[2], run[4])
        };
        rows.push(vec![format!("indexer:{}@{}", run[0], root), detail]);
    }
    Ok(format_toon("overview", &["metric", "value"], &rows))
}

fn get_module_map(db: &Connection) -> Reply {
    let rows = query_rows(
        db,
        "SELECT m.id, m.name, COALESCE(json_extract(m.metadata, '$.files'), 0),
                COALESCE((SELECT group_concat(t.name, ',')
                            FROM edges e JOIN nodes t ON t.id = e.target_id
                           WHERE e.source_id = m.id AND e.type = 'DEPENDS_ON'), '-')
           FROM nodes m WHERE m.type = 'MODULE' ORDER BY m.name",
        &[],
    )?;
    Ok(format_toon("modules", &["id", "name", "files", "depends_on"], &rows))
}

fn get_file_structure(db: &Connection, params: &Value) -> Reply {
    let path = str_param(params, "file_path")
        .unwrap_or("")
        .trim_start_matches("./")
        .replace('\\', "/");
    let rows = query_rows(
        db,
        "SELECT type, name, start_line, end_line,
                CASE WHEN type = 'FILE'
                     THEN CASE WHEN json_extract(metadata, '$.covered') = 0
                               THEN '(no semantic index)' ELSE '' END
                     ELSE COALESCE(json_extract(metadata, '$.signature'), '') END
           FROM nodes WHERE file_path = ?1
          ORDER BY CASE WHEN type = 'FILE' THEN 0 ELSE 1 END, start_line, end_line DESC, id",
        &[SqlValue::Text(path)],
    )?;
    Ok(format_toon(
        "file_structure",
        &["type", "name", "start_line", "end_line", "signature"],
        &rows,
    ))
}

fn find_hubs(db: &Connection, params: &Value) -> Reply {
    let types = match str_param(params, "type") {
        Some(t) if SYMBOL_TYPES.contains(&t) || matches!(t, "FILE" | "MODULE" | "EXTERNAL") => in_list(&[t]),
        Some(t) => return Ok(json!({"error": format!("Unknown type: {}", t)})),
        None => in_list(SYMBOL_TYPES),
    };
    let rows = query_rows(
        db,
        &format!(
            "SELECT id, type, name, fan_in FROM (
                 SELECT id, type, name, metadata, COALESCE(json_extract(metadata, '$.fanIn'), 0) AS fan_in
                   FROM nodes WHERE type IN ({types}))
              WHERE fan_in > 0 AND json_extract(metadata, '$.duplicateOf') IS NULL
              ORDER BY fan_in DESC, name, id LIMIT ?1"
        ),
        &[SqlValue::Integer(limit_param(params, 10))],
    )?;
    Ok(format_toon("hubs", &["id", "type", "name", "hub_score"], &rows))
}

fn escape_like(text: &str) -> String {
    text.replace('\\', "\\\\").replace('%', "\\%").replace('_', "\\_")
}

fn search_symbols(db: &Connection, params: &Value) -> Reply {
    let query = str_param(params, "query").unwrap_or("").trim();
    let types = match str_param(params, "type") {
        Some(t) if SYMBOL_TYPES.contains(&t) => in_list(&[t]),
        Some(t) => return Ok(json!({"error": format!("Unknown type: {}", t)})),
        None => in_list(SYMBOL_TYPES),
    };
    // The name filter is part of the query, so the limit applies to matches only.
    let rows = query_rows(
        db,
        &format!(
            "SELECT id, type, name, file_path, start_line FROM nodes
              WHERE type IN ({types})
                AND json_extract(metadata, '$.duplicateOf') IS NULL
                AND (name LIKE ?1 ESCAPE '\\'
                     OR COALESCE(json_extract(metadata, '$.qualified'), '') LIKE ?1 ESCAPE '\\')
              ORDER BY (lower(name) = lower(?2)) DESC,
                       COALESCE(json_extract(metadata, '$.hubScore'), 0) DESC, name, id
              LIMIT ?3"
        ),
        &[
            SqlValue::Text(format!("%{}%", escape_like(query))),
            SqlValue::Text(query.to_string()),
            SqlValue::Integer(limit_param(params, 20)),
        ],
    )?;
    Ok(format_toon("symbols", &["id", "type", "name", "file_path", "line"], &rows))
}

fn edge_list(db: &Connection, node_id: &str, outgoing: bool) -> Result<(Vec<Value>, i64), rusqlite::Error> {
    let (own, other) = if outgoing { ("source_id", "target_id") } else { ("target_id", "source_id") };
    let id = SqlValue::Text(node_id.to_string());
    let rows = query_rows(
        db,
        &format!(
            "SELECT e.type, o.id, o.name, o.type, COALESCE(json_extract(e.metadata, '$.count'), 1)
               FROM edges e JOIN nodes o ON o.id = e.{other}
              WHERE e.{own} = ?1
              ORDER BY e.type, COALESCE(json_extract(e.metadata, '$.count'), 1) DESC, o.name LIMIT 50"
        ),
        std::slice::from_ref(&id),
    )?;
    let total = db.query_row(
        &format!("SELECT COUNT(*) FROM edges WHERE {own} = ?1"),
        [node_id],
        |r| r.get(0),
    )?;
    let list = rows
        .into_iter()
        .map(|r| {
            json!({
                "type": r[0], "id": r[1], "name": r[2], "nodeType": r[3],
                "count": r[4].parse::<i64>().unwrap_or(1)
            })
        })
        .collect();
    Ok((list, total))
}

fn get_node_detail(db: &Connection, params: &Value) -> Reply {
    let Some(node_id) = str_param(params, "node_id") else {
        return Ok(json!({"error": "node_id is required"}));
    };

    let node = db
        .query_row(
            "SELECT id, type, name, file_path, start_line, end_line, language, metadata
               FROM nodes WHERE id = ?1",
            [node_id],
            |r| {
                Ok(json!({
                    "@id": r.get::<_, String>(0)?,
                    "@type": format!("code:{}", r.get::<_, String>(1)?),
                    "name": r.get::<_, String>(2)?,
                    "filePath": r.get::<_, Option<String>>(3)?,
                    "startLine": r.get::<_, Option<i64>>(4)?,
                    "endLine": r.get::<_, Option<i64>>(5)?,
                    "language": r.get::<_, Option<String>>(6)?,
                    "metadata": serde_json::from_str::<Value>(&r.get::<_, String>(7)?).unwrap_or(Value::Null),
                }))
            },
        )
        .optional()?;
    let Some(mut node) = node else {
        return Ok(json!({"error": "not found"}));
    };

    let (outgoing, outgoing_total) = edge_list(db, node_id, true)?;
    let (incoming, incoming_total) = edge_list(db, node_id, false)?;
    node["outgoing"] = Value::Array(outgoing);
    node["outgoingTotal"] = json!(outgoing_total);
    node["incoming"] = Value::Array(incoming);
    node["incomingTotal"] = json!(incoming_total);
    Ok(node)
}

fn format_toon(label: &str, headers: &[&str], rows: &[Vec<String>]) -> Value {
    let mut lines = vec![format!("{} [{}]", label, rows.len())];
    lines.push(format!("  {}", headers.join(" ")));
    for row in rows {
        lines.push(format!("  {}", row.join(" ")));
    }

    json!({ "content": lines.join("\n") })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{init_schema, replace_all};
    use crate::graph::{Edge, FileRecord, Graph, Node};
    use crate::scanner::SourceFile;

    fn node(id: &str, node_type: &str, name: &str, file: Option<&str>, metadata: Value) -> Node {
        Node {
            id: id.to_string(),
            node_type: node_type.to_string(),
            name: name.to_string(),
            file_path: file.map(String::from),
            start_line: file.map(|_| 3),
            end_line: file.map(|_| 9),
            language: file.map(|_| "python".to_string()),
            metadata,
        }
    }

    fn edge(source: &str, target: &str, kind: &str, count: i64) -> Edge {
        Edge {
            source_id: source.to_string(),
            target_id: target.to_string(),
            edge_type: kind.to_string(),
            metadata: json!({ "count": count }),
        }
    }

    fn record(path: &str, covered: bool) -> FileRecord {
        FileRecord {
            file: SourceFile {
                path: path.to_string(),
                language: "python".to_string(),
                size: 1,
                hash: "h".to_string(),
                lines: 10,
            },
            indexed_by: covered.then(|| "scip-python 0.6.6".to_string()),
        }
    }

    /// 30 functions that sort before `parse_config`, so a LIMIT applied before the name
    /// filter would never reach it.
    fn fixture() -> Connection {
        let db = Connection::open_in_memory().unwrap();
        init_schema(&db).unwrap();
        let mut nodes = vec![
            node("file:src/a.py", "FILE", "a.py", Some("src/a.py"), json!({"covered": true})),
            node("file:src/b.py", "FILE", "b.py", Some("src/b.py"), json!({"covered": false})),
            node("module:src", "MODULE", "src", None, json!({"files": 2})),
            node("module:api", "MODULE", "api", None, json!({"files": 1})),
            node(
                "py:parse_config",
                "FUNCTION",
                "parse_config",
                Some("src/a.py"),
                json!({"fanIn": 5, "hubScore": 5, "signature": "def parse_config() -> dict:"}),
            ),
            node(
                "py:Chart#render",
                "METHOD",
                "render",
                Some("src/a.py"),
                json!({"qualified": "Chart.render", "fanIn": 2, "hubScore": 2}),
            ),
            node(
                "py:Chart#render~2",
                "METHOD",
                "render",
                Some("src/b.py"),
                json!({"qualified": "Chart.render", "duplicateOf": "py:Chart#render", "fanIn": 9}),
            ),
        ];
        for i in 0..30 {
            nodes.push(node(
                &format!("py:a_func_{i:02}"),
                "FUNCTION",
                &format!("a_func_{i:02}"),
                Some("src/a.py"),
                json!({"fanIn": 0, "hubScore": 0}),
            ));
        }
        let edges = vec![
            edge("py:a_func_01", "py:parse_config", "CALLS", 2),
            edge("py:a_func_02", "py:parse_config", "CALLS", 1),
            edge("module:src", "module:api", "DEPENDS_ON", 3),
            edge("file:src/a.py", "py:parse_config", "DEFINES", 1),
        ];
        let files = vec![record("src/a.py", true), record("src/b.py", false)];
        replace_all(&db, &Graph { nodes, edges }, &files, &[], &[]).unwrap();
        db
    }

    fn content(db: &Connection, request: Value) -> String {
        let reply = handle_tool(db, &request.to_string());
        reply["content"].as_str().map(String::from).unwrap_or_else(|| reply.to_string())
    }

    #[test]
    fn search_applies_the_limit_to_matches_not_to_the_table() {
        let db = fixture();
        let found = content(&db, json!({"method": "search_symbols", "params": {"query": "parse"}}));
        assert!(found.starts_with("symbols [1]"), "{found}");
        assert!(found.contains("py:parse_config FUNCTION parse_config src/a.py 3"), "{found}");

        let capped = content(&db, json!({"method": "search_symbols", "params": {"query": "a_func", "limit": 5}}));
        assert!(capped.starts_with("symbols [5]"), "{capped}");
    }

    #[test]
    fn search_is_case_insensitive_matches_qualified_names_and_hides_duplicates() {
        let db = fixture();
        let by_qualified = content(&db, json!({"method": "search_symbols", "params": {"query": "chart.RENDER"}}));
        assert!(by_qualified.starts_with("symbols [1]"), "{by_qualified}");
        assert!(by_qualified.contains("py:Chart#render METHOD"));
        assert!(!by_qualified.contains("~2"), "duplicate definitions are not listed");
    }

    #[test]
    fn search_treats_like_wildcards_literally_and_can_filter_by_type() {
        let db = fixture();
        assert!(content(&db, json!({"method": "search_symbols", "params": {"query": "%"}})).starts_with("symbols [0]"));
        assert!(content(&db, json!({"method": "search_symbols", "params": {"query": "a_func_0_"}})).starts_with("symbols [0]"));
        let methods = content(&db, json!({"method": "search_symbols", "params": {"query": "r", "type": "METHOD"}}));
        assert!(methods.starts_with("symbols [1]"), "{methods}");
        assert!(content(&db, json!({"method": "search_symbols", "params": {"type": "BOGUS"}})).contains("Unknown type"));
    }

    #[test]
    fn find_hubs_ranks_by_how_many_nodes_depend_on_each_one() {
        let db = fixture();
        let hubs = content(&db, json!({"method": "find_hubs", "params": {"limit": 5}}));
        let rows: Vec<&str> = hubs.lines().skip(2).collect();
        assert_eq!(rows.len(), 2, "nodes nobody depends on are not hubs: {hubs}");
        assert!(rows[0].contains("py:parse_config FUNCTION parse_config 5"), "{hubs}");
        assert!(rows[1].contains("py:Chart#render METHOD render 2"), "{hubs}");
    }

    #[test]
    fn node_detail_returns_metadata_as_json_and_the_edges_around_the_node() {
        let db = fixture();
        let reply = handle_tool(&db, &json!({"method": "get_node_detail", "params": {"node_id": "py:parse_config"}}).to_string());
        assert_eq!(reply["@type"], "code:FUNCTION");
        assert_eq!(reply["metadata"]["signature"], "def parse_config() -> dict:");
        assert_eq!(reply["incomingTotal"], 3);
        let callers: Vec<&Value> = reply["incoming"].as_array().unwrap().iter().filter(|e| e["type"] == "CALLS").collect();
        assert_eq!(callers.len(), 2);
        assert_eq!(callers[0]["id"], "py:a_func_01", "highest count first");
        assert_eq!(callers[0]["count"], 2);

        let module = handle_tool(&db, &json!({"method": "get_node_detail", "params": {"node_id": "module:src"}}).to_string());
        assert_eq!(module["filePath"], Value::Null, "directories have no file");
        assert_eq!(module["outgoing"][0]["id"], "module:api");

        let missing = handle_tool(&db, &json!({"method": "get_node_detail", "params": {"node_id": "nope"}}).to_string());
        assert_eq!(missing["error"], "not found");
        let none = handle_tool(&db, &json!({"method": "get_node_detail"}).to_string());
        assert!(none["error"].as_str().unwrap().contains("node_id"));
    }

    #[test]
    fn file_structure_lists_the_file_first_and_flags_files_without_semantic_data() {
        let db = fixture();
        let a = content(&db, json!({"method": "get_file_structure", "params": {"file_path": "./src/a.py"}}));
        let rows: Vec<&str> = a.lines().skip(2).collect();
        assert!(rows[0].starts_with("  FILE a.py"), "{a}");
        assert!(a.contains("def parse_config() -> dict:"));

        let b = content(&db, json!({"method": "get_file_structure", "params": {"file_path": "src/b.py"}}));
        assert!(b.contains("FILE b.py 3 9 (no semantic index)") || b.contains("(no semantic index)"), "{b}");
    }

    #[test]
    fn module_map_shows_which_modules_depend_on_which() {
        let db = fixture();
        let map = content(&db, json!({"method": "get_module_map"}));
        assert!(map.contains("module:api api 1 -"), "{map}");
        assert!(map.contains("module:src src 2 api"), "{map}");
    }

    #[test]
    fn overview_reports_coverage() {
        let db = fixture();
        let overview = content(&db, json!({"method": "get_overview"}));
        assert!(overview.contains("  files 2"), "{overview}");
        assert!(overview.contains("  covered_files 1"), "{overview}");
        assert!(overview.contains("  modules 2"), "{overview}");
        assert!(overview.contains("  functions 31"), "{overview}");
    }

    #[test]
    fn bad_requests_get_an_error_reply_instead_of_a_crash() {
        let db = fixture();
        assert!(handle_tool(&db, "{not json")["error"].as_str().unwrap().contains("Parse error"));
        assert!(handle_tool(&db, r#"{"method":"initialize"}"#)["error"].as_str().unwrap().contains("Unknown method"));
    }
}
