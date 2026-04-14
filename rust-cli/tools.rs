pub fn create_tool_handlers(
    project_root: std::path::PathBuf,
    db_path: std::path::PathBuf,
) -> ToolHandlers {
    ToolHandlers {
        project_root,
        db_path,
    }
}

pub struct ToolHandlers {
    project_root: std::path::PathBuf,
    db_path: std::path::PathBuf,
}

impl ToolHandlers {
    pub fn get_overview(&self) -> Result<String, Box<dyn std::error::Error>> {
        let db = crate::db::open_database(&self.db_path)?;
        let files: i64 = db.query_row("SELECT COUNT(*) FROM file_manifest", [], |r| r.get(0))?;
        let functions: i64 = db.query_row(
            "SELECT COUNT(*) FROM nodes WHERE type = 'FUNCTION'",
            [],
            |r| r.get(0),
        )?;
        let classes: i64 =
            db.query_row("SELECT COUNT(*) FROM nodes WHERE type = 'CLASS'", [], |r| {
                r.get(0)
            })?;
        Ok(crate::serialization::to_toon(
            "overview",
            &["metric", "value"],
            &[
                vec!["files".to_string(), files.to_string()],
                vec!["functions".to_string(), functions.to_string()],
                vec!["classes".to_string(), classes.to_string()],
            ],
        ))
    }

    pub fn close(&self) {}
}
