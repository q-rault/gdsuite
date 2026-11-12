//! `gdsuite config`: read and change `gdsuite.toml` from scripts and editors,
//! keeping the file's comments and layout.
//!
//! Keys are dotted paths of tables: `lint.disable`, `editor.format_on_save`.
//! A value is parsed as TOML (`true`, `100`, `["fmt"]`); anything that isn't
//! TOML is a string, so `gdsuite config add lint.disable max-returns` works
//! without quotes.

use std::fs;

use anyhow::{Context, Result, anyhow, bail};
use toml_edit::{DocumentMut, Item, Table, Value};

use crate::{cli::ConfigAction, config::GdsuiteConfig, tooling::ProjectContext};

pub fn run(context: &ProjectContext, action: ConfigAction) -> Result<()> {
    if let ConfigAction::Get { key } = &action {
        println!("{}", serde_json::to_string(&get(context, key)?)?);
        return Ok(());
    }

    let before = if context.config_path.exists() {
        fs::read_to_string(&context.config_path)
            .with_context(|| format!("failed to read {}", context.config_path.display()))?
    } else {
        String::new()
    };
    let after = edit(&before, &action)?;
    if after == before {
        return Ok(());
    }
    fs::write(&context.config_path, &after)
        .with_context(|| format!("failed to write {}", context.config_path.display()))?;
    // Never leave a file gdsuite can't read.
    if let Err(error) = GdsuiteConfig::load(&context.config_path) {
        if before.is_empty() {
            let _ = fs::remove_file(&context.config_path);
        } else {
            let _ = fs::write(&context.config_path, &before);
        }
        return Err(error.context("the change was undone"));
    }
    Ok(())
}

/// The value at `key`, as JSON; `null` when it isn't set or there is no
/// gdsuite.toml.
fn get(context: &ProjectContext, key: &str) -> Result<serde_json::Value> {
    if !context.config_path.exists() {
        return Ok(serde_json::Value::Null);
    }
    let text = fs::read_to_string(&context.config_path)
        .with_context(|| format!("failed to read {}", context.config_path.display()))?;
    let mut value: toml::Value = toml::from_str(&text)
        .with_context(|| format!("failed to parse {}", context.config_path.display()))?;
    for part in parts(key)? {
        match value.get(part) {
            Some(inner) => value = inner.clone(),
            None => return Ok(serde_json::Value::Null),
        }
    }
    Ok(serde_json::to_value(value)?)
}

/// `text` with `action` applied.
fn edit(text: &str, action: &ConfigAction) -> Result<String> {
    let mut document: DocumentMut = text.parse().context("failed to parse gdsuite.toml")?;
    match action {
        ConfigAction::Get { .. } => unreachable!("get changes nothing"),
        ConfigAction::Set { key, value } => {
            let (table, name) = table_for(&mut document, key, true)?;
            let table = table.expect("created");
            table.insert(name, Item::Value(parse_value(value)));
        }
        ConfigAction::Unset { key } => {
            if let (Some(table), name) = table_for(&mut document, key, false)? {
                table.remove(name);
            }
        }
        ConfigAction::Add { key, value } => {
            let (table, name) = table_for(&mut document, key, true)?;
            let table = table.expect("created");
            let item = table
                .entry(name)
                .or_insert(Item::Value(Value::Array(Default::default())));
            let array = item
                .as_array_mut()
                .ok_or_else(|| anyhow!("{key} is not a list"))?;
            let value = parse_value(value);
            if !array.iter().any(|existing| same(existing, &value)) {
                array.push(value);
            }
        }
        ConfigAction::Remove { key, value } => {
            if let (Some(table), name) = table_for(&mut document, key, false)?
                && let Some(item) = table.get_mut(name)
            {
                let array = item
                    .as_array_mut()
                    .ok_or_else(|| anyhow!("{key} is not a list"))?;
                let value = parse_value(value);
                array.retain(|existing| !same(existing, &value));
            }
        }
    }
    Ok(document.to_string())
}

fn parts(key: &str) -> Result<Vec<&str>> {
    let parts: Vec<&str> = key.split('.').collect();
    if parts.iter().any(|part| part.is_empty()) {
        bail!("invalid key `{key}`: use dotted names such as `lint.disable`");
    }
    Ok(parts)
}

/// The table that holds `key`'s last part, and that part. With `create`,
/// missing tables are added; otherwise a missing table is `None`.
fn table_for<'a, 'k>(
    document: &'a mut DocumentMut,
    key: &'k str,
    create: bool,
) -> Result<(Option<&'a mut Table>, &'k str)> {
    let parts = parts(key)?;
    let (name, tables) = parts.split_last().expect("split gives one part at least");
    let mut table = document.as_table_mut();
    for part in tables {
        if !table.contains_key(part) {
            if !create {
                return Ok((None, name));
            }
            table.insert(part, Item::Table(Table::new()));
        }
        table = table
            .get_mut(part)
            .and_then(Item::as_table_mut)
            .ok_or_else(|| anyhow!("`{part}` in `{key}` is not a table"))?;
    }
    Ok((Some(table), name))
}

/// `raw` as TOML, or as a string when it isn't TOML.
fn parse_value(raw: &str) -> Value {
    let mut value = raw.parse::<Value>().unwrap_or_else(|_| Value::from(raw));
    // Spacing comes from where the value goes, not from `raw`.
    value.decor_mut().clear();
    value
}

fn same(a: &Value, b: &Value) -> bool {
    a.to_string().trim() == b.to_string().trim()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn set(key: &str, value: &str) -> ConfigAction {
        ConfigAction::Set {
            key: key.into(),
            value: value.into(),
        }
    }

    #[test]
    fn set_keeps_comments_and_adds_missing_tables() {
        let text = "# The project's checks.\n[check]\nsteps = [\"fmt\"] # fast\n";
        let edited = edit(text, &set("editor.format_on_save", "true")).unwrap();
        assert_eq!(
            edited,
            "# The project's checks.\n[check]\nsteps = [\"fmt\"] # fast\n\n[editor]\nformat_on_save = true\n"
        );
    }

    #[test]
    fn add_and_remove_change_a_list_once() {
        let add = ConfigAction::Add {
            key: "lint.disable".into(),
            value: "max-returns".into(),
        };
        let once = edit("", &add).unwrap();
        assert_eq!(once, "[lint]\ndisable = [\"max-returns\"]\n");
        assert_eq!(edit(&once, &add).unwrap(), once);

        let remove = ConfigAction::Remove {
            key: "lint.disable".into(),
            value: "max-returns".into(),
        };
        assert_eq!(edit(&once, &remove).unwrap(), "[lint]\ndisable = []\n");
    }

    #[test]
    fn unset_of_a_missing_key_changes_nothing() {
        let text = "[check]\nsteps = [\"fmt\"]\n";
        let unset = ConfigAction::Unset {
            key: "editor.format_on_save".into(),
        };
        assert_eq!(edit(text, &unset).unwrap(), text);
    }

    #[test]
    fn values_that_are_not_toml_are_strings() {
        assert_eq!(
            edit("", &set("commands.default_test_target", "test/unit")).unwrap(),
            "[commands]\ndefault_test_target = \"test/unit\"\n"
        );
        assert_eq!(
            edit("", &set("check.steps", "[\"fmt\", \"lint\"]")).unwrap(),
            "[check]\nsteps = [\"fmt\", \"lint\"]\n"
        );
    }
}
