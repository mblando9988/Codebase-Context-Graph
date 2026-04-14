use crate::parser::{self, Community, GraphEdge, GraphNode};
use std::path::PathBuf;

pub struct IndexStats {
    pub files: usize,
    pub functions: usize,
    pub classes: usize,
    pub variables: usize,
    pub edges: usize,
    pub communities: usize,
}

pub fn init_project(project_root: &PathBuf) -> Result<(), Box<dyn std::error::Error>> {
    let context_dir = crate::config::context_dir(project_root);
    std::fs::create_dir_all(&context_dir)?;

    let db_path = crate::config::database_path(project_root);
    let db = crate::db::open_database(&db_path)?;
    drop(db);

    crate::config::ensure_project_config(project_root)?;

    println!("Initialized: {}", context_dir.display());
    Ok(())
}

pub fn index_project(
    project_root: &PathBuf,
    advanced: bool,
) -> Result<IndexStats, Box<dyn std::error::Error>> {
    let config = crate::config::load_project_config(project_root)?;
    let manifest = crate::scanner::scan_project(project_root, &config.ignore_patterns)?;

    println!("Parsing {} files...", manifest.len());

    let mut all_nodes: Vec<GraphNode> = Vec::new();
    let mut all_edges: Vec<GraphEdge> = Vec::new();
    let mut all_communities: Vec<Community> = Vec::new();
    let mut parse_errors = Vec::new();

    for (idx, item) in manifest.iter().enumerate() {
        if idx % 100 == 0 {
            println!("  {}/{} files", idx, manifest.len());
        }

        let graph = crate::parser::parse_file(item, &mut parse_errors);
        all_nodes.extend(graph.nodes);
        all_edges.extend(graph.edges);
    }

    println!("Enriching graph...");
    parser::enrich_graph(&mut all_nodes, &all_edges, &mut all_communities);

    if advanced {
        println!("Running advanced analysis...");
    }

    println!("Writing database...");
    let db_path = crate::config::database_path(project_root);
    let db = crate::db::open_database(&db_path)?;

    let hashes: Vec<crate::db::FileHash> = manifest
        .iter()
        .map(|item| crate::db::FileHash {
            file_path: item.file_path.clone(),
            content_hash: item.content_hash.clone(),
            last_indexed: chrono::Utc::now().to_rfc3339(),
        })
        .collect();

    crate::db::replace_all(
        &db,
        &manifest,
        &all_nodes
            .iter()
            .map(|n| crate::db::Node {
                id: n.id.clone(),
                node_type: n.node_type.clone(),
                name: n.name.clone(),
                file_path: n.file_path.clone(),
                start_line: n.start_line,
                end_line: n.end_line,
                language: n.language.clone(),
                metadata: n.metadata.clone(),
            })
            .collect::<Vec<_>>(),
        &all_edges
            .iter()
            .map(|e| crate::db::Edge {
                id: None,
                source_id: e.source_id.clone(),
                target_id: e.target_id.clone(),
                edge_type: e.edge_type.clone(),
                metadata: e.metadata.clone(),
            })
            .collect::<Vec<_>>(),
        &all_communities
            .iter()
            .map(|c| crate::db::Community {
                id: c.id,
                label: c.label.clone(),
                node_ids: c.node_ids.clone(),
            })
            .collect::<Vec<_>>(),
        &hashes,
    )?;
    drop(db);

    let json_path = crate::config::graph_json_path(project_root);
    let json_payload = serde_json::json!({
        "version": "1.0",
        "generatedAt": chrono::Utc::now().to_rfc3339(),
        "analysisMode": if advanced { "advanced" } else { "standard" },
        "nodes": all_nodes.iter().map(|n| crate::db::Node {
            id: n.id.clone(),
            node_type: n.node_type.clone(),
            name: n.name.clone(),
            file_path: n.file_path.clone(),
            start_line: n.start_line,
            end_line: n.end_line,
            language: n.language.clone(),
            metadata: n.metadata.clone(),
        }).collect::<Vec<_>>(),
        "edges": all_edges.iter().map(|e| crate::db::Edge {
            id: None,
            source_id: e.source_id.clone(),
            target_id: e.target_id.clone(),
            edge_type: e.edge_type.clone(),
            metadata: e.metadata.clone(),
        }).collect::<Vec<_>>(),
        "communities": all_communities.iter().map(|c| crate::db::Community {
            id: c.id,
            label: c.label.clone(),
            node_ids: c.node_ids.clone(),
        }).collect::<Vec<_>>(),
        "fileHashes": hashes,
    });
    std::fs::write(&json_path, serde_json::to_string_pretty(&json_payload)?)?;

    println!(
        "Done: {} files, {} nodes, {} edges",
        manifest.len(),
        all_nodes.len(),
        all_edges.len()
    );

    Ok(IndexStats {
        files: manifest.len(),
        functions: all_nodes
            .iter()
            .filter(|n| n.node_type == "FUNCTION")
            .count(),
        classes: all_nodes.iter().filter(|n| n.node_type == "CLASS").count(),
        variables: all_nodes
            .iter()
            .filter(|n| n.node_type == "VARIABLE")
            .count(),
        edges: all_edges.len(),
        communities: all_communities.len(),
    })
}

pub fn watch_project(project_root: &PathBuf) -> Result<(), Box<dyn std::error::Error>> {
    println!("Watching {} for changes...", project_root.display());
    println!("Press Ctrl+C to stop.");
    println!("(Watcher implementation requires notify crate - running single index instead)");
    index_project(project_root, false)?;
    Ok(())
}

pub fn smoke_test(project_root: &PathBuf) -> Result<(), Box<dyn std::error::Error>> {
    let db_path = crate::config::database_path(project_root);

    if !db_path.exists() {
        return Err("Database not found. Run 'index' first.".into());
    }

    let db = crate::db::open_database(&db_path)?;

    let count: i64 = db.query_row("SELECT COUNT(*) FROM nodes", [], |r| r.get(0))?;
    if count == 0 {
        return Err("Database is empty. Run 'index' first.".into());
    }

    println!("Database OK: {} nodes", count);
    Ok(())
}
