pub fn to_toon(label: &str, headers: &[&str], rows: &[Vec<String>]) -> String {
    let mut lines = vec![format!("{} [{}]", label, rows.len())];
    lines.push(format!("  {}", headers.join(" ")));
    for row in rows {
        lines.push(format!("  {}", row.join(" ")));
    }
    lines.join("\n")
}

pub fn format_json_ld(value: &serde_json::Value) -> String {
    serde_json::to_string_pretty(value).unwrap_or_default()
}
