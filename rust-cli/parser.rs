use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tree_sitter::Parser;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphNode {
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
pub struct GraphEdge {
    pub source_id: String,
    pub target_id: String,
    #[serde(rename = "type")]
    pub edge_type: String,
    pub metadata: serde_json::Value,
}

#[derive(Debug)]
pub struct Graph {
    pub nodes: Vec<GraphNode>,
    pub edges: Vec<GraphEdge>,
}

#[derive(Debug)]
pub struct ParseError {
    pub file_path: String,
    pub language: String,
    pub error: String,
}

#[derive(Debug)]
pub struct Import {
    pub import_id: String,
    pub source: String,
    pub line: usize,
}

#[derive(Debug)]
pub struct Function {
    pub id: String,
    pub name: String,
    pub file_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Community {
    pub id: i64,
    pub label: String,
    pub node_ids: Vec<String>,
}

pub fn parse_file(
    item: &crate::scanner::FileManifest,
    parse_errors: &mut Vec<ParseError>,
) -> Graph {
    let mut parser = Parser::new();

    let language = match item.language.as_str() {
        "javascript" => tree_sitter_javascript::LANGUAGE.into(),
        "typescript" => tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
        "python" => tree_sitter_python::LANGUAGE.into(),
        "bash" => tree_sitter_bash::LANGUAGE.into(),
        "rust" => tree_sitter_rust::LANGUAGE.into(),
        _ => {
            return Graph {
                nodes: vec![],
                edges: vec![],
            }
        }
    };

    if parser.set_language(&language).is_err() {
        return Graph {
            nodes: vec![],
            edges: vec![],
        };
    }

    let file_id = format!("file:{}", item.file_path);
    let module_path = std::path::Path::new(&item.file_path)
        .parent()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|| ".".to_string());
    let module_id = format!("module:{}", module_path);

    let mut nodes = Vec::new();
    let mut edges = Vec::new();

    let line_count = item.content.lines().count() as i64;

    nodes.push(GraphNode {
        id: file_id.clone(),
        node_type: "FILE".to_string(),
        name: std::path::Path::new(&item.file_path)
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string(),
        file_path: Some(item.file_path.clone()),
        start_line: Some(1),
        end_line: Some(line_count),
        language: Some(item.language.clone()),
        metadata: serde_json::json!({ "hash": item.content_hash, "size": item.size }),
    });

    nodes.push(GraphNode {
        id: module_id.clone(),
        node_type: "MODULE".to_string(),
        name: module_path.clone(),
        file_path: Some(item.file_path.clone()),
        start_line: Some(1),
        end_line: Some(line_count),
        language: Some(item.language.clone()),
        metadata: serde_json::json!({ "path": module_path }),
    });

    edges.push(GraphEdge {
        source_id: module_id,
        target_id: file_id,
        edge_type: "CONTAINS".to_string(),
        metadata: serde_json::json!({}),
    });

    match parser.parse(&item.content, None) {
        Some(tree) => {
            walk_tree(
                tree.root_node(),
                &item.content,
                &item,
                &mut nodes,
                &mut edges,
            );
            Graph { nodes, edges }
        }
        None => {
            parse_errors.push(ParseError {
                file_path: item.file_path.clone(),
                language: item.language.clone(),
                error: "Parse failed".to_string(),
            });
            Graph { nodes, edges }
        }
    }
}

fn walk_tree(
    node: tree_sitter::Node,
    source: &str,
    item: &crate::scanner::FileManifest,
    nodes: &mut Vec<GraphNode>,
    edges: &mut Vec<GraphEdge>,
) {
    let mut cursor = node.walk();

    for child in node.children(&mut cursor) {
        let kind = child.kind();

        if kind == "function_declaration"
            || kind == "generator_function_declaration"
            || kind == "function_definition"
        {
            handle_function(child, source, item, nodes, edges, false);
        } else if kind == "method_definition" {
            handle_function(child, source, item, nodes, edges, true);
        } else if kind == "class_declaration" || kind == "class_definition" {
            handle_class(child, source, item, nodes, edges);
        }
    }
}

fn handle_function(
    node: tree_sitter::Node,
    source: &str,
    item: &crate::scanner::FileManifest,
    nodes: &mut Vec<GraphNode>,
    edges: &mut Vec<GraphEdge>,
    is_method: bool,
) {
    let name = node
        .child_by_field_name("name")
        .map(|n| source[n.byte_range()].to_string())
        .unwrap_or_else(|| format!("anonymous_{}", node.start_position().row + 1));

    let function_id = format!(
        "function:{}:{}:{}:{}",
        item.file_path,
        name,
        node.start_position().row + 1,
        node.start_position().column + 1
    );

    let complexity = count_complexity(node);

    nodes.push(GraphNode {
        id: function_id.clone(),
        node_type: "FUNCTION".to_string(),
        name: name.clone(),
        file_path: Some(item.file_path.clone()),
        start_line: Some((node.start_position().row + 1) as i64),
        end_line: Some((node.end_position().row + 1) as i64),
        language: Some(item.language.clone()),
        metadata: serde_json::json!({ "method": is_method, "complexity": complexity }),
    });

    edges.push(GraphEdge {
        source_id: format!("file:{}", item.file_path),
        target_id: function_id,
        edge_type: "DEFINES".to_string(),
        metadata: serde_json::json!({}),
    });
}

fn handle_class(
    node: tree_sitter::Node,
    source: &str,
    item: &crate::scanner::FileManifest,
    nodes: &mut Vec<GraphNode>,
    edges: &mut Vec<GraphEdge>,
) {
    let name = node
        .child_by_field_name("name")
        .map(|n| source[n.byte_range()].to_string())
        .unwrap_or_else(|| format!("class_{}", node.start_position().row + 1));

    let class_id = format!(
        "class:{}:{}:{}",
        item.file_path,
        name,
        node.start_position().row + 1
    );

    nodes.push(GraphNode {
        id: class_id.clone(),
        node_type: "CLASS".to_string(),
        name: name.clone(),
        file_path: Some(item.file_path.clone()),
        start_line: Some((node.start_position().row + 1) as i64),
        end_line: Some((node.end_position().row + 1) as i64),
        language: Some(item.language.clone()),
        metadata: serde_json::json!({}),
    });

    edges.push(GraphEdge {
        source_id: format!("file:{}", item.file_path),
        target_id: class_id,
        edge_type: "DEFINES".to_string(),
        metadata: serde_json::json!({}),
    });
}

fn count_complexity(node: tree_sitter::Node) -> i64 {
    let mut complexity = 1i64;

    fn traverse(n: tree_sitter::Node, count: &mut i64) {
        match n.kind() {
            "if_statement"
            | "for_statement"
            | "while_statement"
            | "switch_statement"
            | "case_statement"
            | "try_statement"
            | "catch_clause"
            | "conditional_expression" => {
                *count += 1;
            }
            _ => {}
        }

        let mut cursor = n.walk();
        for child in n.children(&mut cursor) {
            traverse(child, count);
        }
    }

    traverse(node, &mut complexity);
    complexity
}

pub fn enrich_graph(
    nodes: &mut Vec<GraphNode>,
    _edges: &[GraphEdge],
    communities: &mut Vec<Community>,
) {
    let mut edge_counts: HashMap<String, i64> = HashMap::new();

    for node in nodes.iter_mut() {
        let score = edge_counts.get(&node.id).copied().unwrap_or(0);
        node.metadata["hubScore"] = serde_json::json!(score);

        if node.node_type == "FUNCTION" {
            let haystack = format!(
                "{} {}",
                node.name.to_lowercase(),
                node.metadata
                    .get("signature")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
            );

            let mut concerns = Vec::new();
            if haystack.contains("log") || haystack.contains("debug") {
                concerns.push("logging");
            }
            if haystack.contains("auth") || haystack.contains("token") {
                concerns.push("auth");
            }
            if haystack.contains("error") || haystack.contains("exception") {
                concerns.push("error");
            }
            if !concerns.is_empty() {
                node.metadata["concerns"] = serde_json::json!(concerns);
            }
        }
    }

    let module_nodes: Vec<_> = nodes.iter().filter(|n| n.node_type == "MODULE").collect();

    for (idx, module) in module_nodes.iter().enumerate() {
        communities.push(Community {
            id: (idx + 1) as i64,
            label: module.name.clone(),
            node_ids: vec![module.id.clone()],
        });
    }
}
