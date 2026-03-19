//! Read and modify Cargo.toml files for Axum <-> Typeway migration.

use std::path::Path;

use anyhow::{Context, Result};
use toml_edit::{DocumentMut, InlineTable, Item, Value};

/// Update Cargo.toml for Axum -> Typeway migration.
///
/// Adds `typeway` dependency (with features), comments out `axum` if present.
/// Shared dependencies (`tower-http`, `tokio`, `serde`, `serde_json`) are kept.
/// Returns the modified TOML string.
pub fn update_cargo_for_typeway(cargo_path: &Path) -> Result<String> {
    let content =
        std::fs::read_to_string(cargo_path).context("failed to read Cargo.toml")?;

    let mut doc: DocumentMut = content
        .parse()
        .context("failed to parse Cargo.toml as TOML")?;

    let deps = ensure_dependencies_table(&mut doc);

    // Comment out axum if present.
    if let Some(axum_item) = deps.remove("axum") {
        let comment = format!(
            "# Removed by typeway-migrate: axum = {}\n",
            item_to_inline_string(&axum_item),
        );
        // Insert the comment as a decor prefix on the typeway entry we're about to add.
        add_typeway_dep(deps, &comment);
    } else {
        add_typeway_dep(deps, "");
    }

    // Also comment out axum-extra if present.
    if let Some(extra_item) = deps.remove("axum-extra") {
        let comment = format!(
            "# Removed by typeway-migrate: axum-extra = {}\n",
            item_to_inline_string(&extra_item),
        );
        prepend_comment_to_key(deps, "typeway", &comment);
    }

    Ok(doc.to_string())
}

/// Update Cargo.toml for Typeway -> Axum migration.
///
/// Adds `axum` and `axum-extra` dependencies, comments out `typeway` if present.
/// Returns the modified TOML string.
pub fn update_cargo_for_axum(cargo_path: &Path) -> Result<String> {
    let content =
        std::fs::read_to_string(cargo_path).context("failed to read Cargo.toml")?;

    let mut doc: DocumentMut = content
        .parse()
        .context("failed to parse Cargo.toml as TOML")?;

    let deps = ensure_dependencies_table(&mut doc);

    // Comment out typeway if present.
    let typeway_comment = if let Some(typeway_item) = deps.remove("typeway") {
        format!(
            "# Removed by typeway-migrate: typeway = {}\n",
            item_to_inline_string(&typeway_item),
        )
    } else {
        String::new()
    };

    // Add axum.
    if !deps.contains_key("axum") {
        let mut decor = toml_edit::Key::new("axum");
        if !typeway_comment.is_empty() {
            decor.leaf_decor_mut().set_prefix(typeway_comment);
        }
        deps.insert_formatted(
            &decor,
            Item::Value(Value::String(toml_edit::Formatted::new(
                "0.8".to_string(),
            ))),
        );
    }

    // Add axum-extra.
    if !deps.contains_key("axum-extra") {
        deps.insert(
            "axum-extra",
            Item::Value(Value::String(toml_edit::Formatted::new(
                "0.10".to_string(),
            ))),
        );
    }

    Ok(doc.to_string())
}

/// Ensure the `[dependencies]` table exists and return a mutable reference to it.
fn ensure_dependencies_table(doc: &mut DocumentMut) -> &mut toml_edit::Table {
    if !doc.contains_table("dependencies") {
        doc.insert("dependencies", Item::Table(toml_edit::Table::new()));
    }
    doc["dependencies"]
        .as_table_mut()
        .expect("[dependencies] should be a table")
}

/// Add `typeway = { version = "0.1", features = ["full"] }` to the deps table.
fn add_typeway_dep(deps: &mut toml_edit::Table, prefix_comment: &str) {
    if deps.contains_key("typeway") {
        return;
    }

    let mut inline = InlineTable::new();
    inline.insert(
        "version",
        Value::String(toml_edit::Formatted::new("0.1".to_string())),
    );
    let mut features_arr = toml_edit::Array::new();
    features_arr.push("full");
    inline.insert("features", Value::Array(features_arr));

    let mut key = toml_edit::Key::new("typeway");
    if !prefix_comment.is_empty() {
        key.leaf_decor_mut().set_prefix(prefix_comment);
    }
    deps.insert_formatted(&key, Item::Value(Value::InlineTable(inline)));
}

/// Prepend a comment string to the decor prefix of an existing key in the table.
fn prepend_comment_to_key(deps: &mut toml_edit::Table, key_name: &str, comment: &str) {
    if let Some((_key, item)) = deps.get_key_value_mut(key_name) {
        let existing = item
            .as_value()
            .map(|v| v.decor().prefix().map(|p| p.as_str().unwrap_or("")).unwrap_or(""))
            .unwrap_or("");
        let new_prefix = format!("{}{}", comment, existing);
        if let Some(val) = item.as_value_mut() {
            val.decor_mut().set_prefix(new_prefix);
        }
    }
}

/// Serialize a TOML `Item` back to a compact inline string for use in comments.
fn item_to_inline_string(item: &Item) -> String {
    match item {
        Item::Value(v) => v.to_string().trim().to_string(),
        _ => item.to_string().trim().to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    fn write_temp_cargo(content: &str) -> NamedTempFile {
        let mut f = NamedTempFile::new().unwrap();
        f.write_all(content.as_bytes()).unwrap();
        f
    }

    #[test]
    fn adds_typeway_and_comments_out_axum() {
        let input = r#"[package]
name = "my-app"
version = "0.1.0"

[dependencies]
axum = "0.8"
tokio = { version = "1", features = ["full"] }
serde = { version = "1", features = ["derive"] }
"#;
        let f = write_temp_cargo(input);
        let result = update_cargo_for_typeway(f.path()).unwrap();

        assert!(
            result.contains("typeway"),
            "should contain typeway dep, got:\n{result}"
        );
        assert!(
            result.contains("# Removed by typeway-migrate: axum"),
            "should contain commented-out axum, got:\n{result}"
        );
        assert!(
            !result.contains("\naxum ="),
            "should not have active axum dep, got:\n{result}"
        );
        assert!(
            result.contains("tokio"),
            "should keep tokio, got:\n{result}"
        );
        assert!(
            result.contains("serde"),
            "should keep serde, got:\n{result}"
        );
    }

    #[test]
    fn adds_axum_and_comments_out_typeway() {
        let input = r#"[package]
name = "my-app"
version = "0.1.0"

[dependencies]
typeway = { version = "0.1", features = ["full"] }
tokio = { version = "1", features = ["full"] }
"#;
        let f = write_temp_cargo(input);
        let result = update_cargo_for_axum(f.path()).unwrap();

        assert!(
            result.contains("axum"),
            "should contain axum dep, got:\n{result}"
        );
        assert!(
            result.contains("axum-extra"),
            "should contain axum-extra dep, got:\n{result}"
        );
        assert!(
            result.contains("# Removed by typeway-migrate: typeway"),
            "should contain commented-out typeway, got:\n{result}"
        );
        assert!(
            result.contains("tokio"),
            "should keep tokio, got:\n{result}"
        );
    }

    #[test]
    fn idempotent_when_target_dep_already_present() {
        let input = r#"[package]
name = "my-app"

[dependencies]
typeway = { version = "0.1", features = ["full"] }
"#;
        let f = write_temp_cargo(input);
        let result = update_cargo_for_typeway(f.path()).unwrap();

        // Should not duplicate typeway.
        let count = result.matches("typeway").count();
        // One in [dependencies] key usage.
        assert!(
            count >= 1,
            "should have typeway at least once, got:\n{result}"
        );
    }

    #[test]
    fn creates_dependencies_table_if_missing() {
        let input = r#"[package]
name = "my-app"
"#;
        let f = write_temp_cargo(input);
        let result = update_cargo_for_typeway(f.path()).unwrap();

        assert!(
            result.contains("[dependencies]"),
            "should create [dependencies], got:\n{result}"
        );
        assert!(
            result.contains("typeway"),
            "should contain typeway, got:\n{result}"
        );
    }
}
