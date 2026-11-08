use std::{fs, path::Path};

use anyhow::{Context, Result, anyhow};
use globwalk::GlobWalkerBuilder;

use crate::{
    config::GdsuiteConfig,
    tooling::{self, ProjectContext},
};

pub fn migrate_project(context: &ProjectContext, cli_godot: Option<&Path>) -> Result<()> {
    context.ensure_dirs()?;
    let (config, warnings) = migrate_suite_config(context, cli_godot)?;
    rewrite_entrypoints(context)?;
    for warning in warnings {
        println!("warning: {warning}");
    }
    convert_tool_configs(context)?;
    println!("migrate complete");
    config.write(&context.config_path)?;
    Ok(())
}

fn migrate_suite_config(
    context: &ProjectContext,
    cli_godot: Option<&Path>,
) -> Result<(GdsuiteConfig, Vec<String>)> {
    let body = if context.config_path.exists() {
        fs::read_to_string(&context.config_path)
            .with_context(|| format!("failed to read {}", context.config_path.display()))?
    } else {
        String::new()
    };
    let value = if body.trim().is_empty() {
        toml::Value::Table(Default::default())
    } else {
        body.parse()
            .with_context(|| format!("failed to parse {}", context.config_path.display()))?
    };

    let mut config = GdsuiteConfig::default();
    let warnings = collect_migration_warnings(&value);

    if let Some(project) = value.get("project").and_then(|value| value.as_table()) {
        if let Some(root) = project.get("root").and_then(|value| value.as_str()) {
            config.project.root = root.to_string();
        }
        if let Some(godot_exe) = project.get("godot_exe").and_then(|value| value.as_str()) {
            config.project.godot_exe = Some(godot_exe.into());
        }
    }
    if let Some(tools) = value.get("tools").and_then(|value| value.as_table()) {
        if let Some(source) = tools.get("gdtest_source").and_then(|value| value.as_str()) {
            config.tools.gdtest_source = source.into();
        }
        if let Some(source) = tools
            .get("gdhealth_source")
            .and_then(|value| value.as_str())
        {
            config.tools.gdhealth_source = source.into();
        }
        if let Some(source) = tools
            .get("gdtypecheck_source")
            .and_then(|value| value.as_str())
        {
            config.tools.gdtypecheck_source = source.into();
        }
        if let Some(source) = tools.get("gdtidy_source").and_then(|value| value.as_str()) {
            config.tools.gdtidy_source = source.into();
        }
    }
    if let Some(commands) = value.get("commands").and_then(|value| value.as_table()) {
        if let Some(target) = commands
            .get("default_test_target")
            .and_then(|value| value.as_str())
        {
            config.commands.default_test_target = target.to_string();
        }
        if let Some(subcommand) = commands
            .get("default_health_subcommand")
            .and_then(|value| value.as_str())
        {
            config.commands.default_health_subcommand = subcommand.to_string();
        }
    }
    if let Some(reports) = value.get("reports").and_then(|value| value.as_table()) {
        if let Some(default) = reports.get("default").and_then(|value| value.as_str()) {
            config.reports.default = default.to_string();
        }
        if let Some(dir) = reports
            .get("artifacts_dir")
            .and_then(|value| value.as_str())
        {
            config.reports.artifacts_dir = dir.into();
        }
    }
    if let Some(check) = value.get("check").and_then(|value| value.as_table())
        && let Some(steps) = check.get("steps")
    {
        config.check.steps = steps.clone().try_into().map_err(|_| {
            anyhow!(
                "failed to parse [check].steps in {}",
                context.config_path.display()
            )
        })?;
    }

    if let Some(path) = cli_godot {
        config.project.godot_exe = Some(path.to_path_buf());
    }

    Ok((config, warnings))
}

fn collect_migration_warnings(value: &toml::Value) -> Vec<String> {
    let mut warnings = Vec::new();
    let Some(table) = value.as_table() else {
        return warnings;
    };

    if table.contains_key("lint") {
        warnings.push(
            "Moved out of suite contract: create `gdtidy.toml` with your lint rules.".to_string(),
        );
    }
    if table.contains_key("gdtest") {
        warnings.push(
            "Moved out of suite contract: create `gdtest.toml` with your gdtest settings."
                .to_string(),
        );
    }
    if table.contains_key("gdhealth") {
        warnings.push(
            "Moved out of suite contract: create `gdhealth.toml` with your gdhealth settings."
                .to_string(),
        );
    }
    if table.contains_key("health") {
        warnings.push(
            "Moved out of suite contract: remove suite-owned health settings and use gdhealth native config or explicit CLI args.".to_string(),
        );
    }
    if table.contains_key("fix") {
        warnings
            .push("Moved out of suite contract: suite-owned lint fixers were removed.".to_string());
    }
    if table.contains_key("typecheck") {
        warnings.push(
            "Moved out of suite contract: install and configure the delegated typecheck tool directly.".to_string(),
        );
    }
    if table
        .get("tools")
        .and_then(|tools| tools.as_table())
        .is_some_and(|tools| tools.contains_key("gdtoolkit"))
    {
        warnings.push(
            "gdsuite no longer installs gdtoolkit: gdtidy formats and lints, so `gdtoolkit` is dropped from [tools]. The `.gdsuite/python` folder can be deleted.".to_string(),
        );
    }
    warnings
}

/// Convert gdlintrc and gdformatrc into gdtidy.toml when gdtidy is installed;
/// otherwise say that setup will do it, since it installs gdtidy first.
fn convert_tool_configs(context: &ProjectContext) -> Result<()> {
    let unconverted = tooling::unconverted_configs(context);
    if unconverted.is_empty() {
        return Ok(());
    }
    let gdtidy = tooling::load_manifest(context)
        .ok()
        .and_then(|manifest| manifest.gdtidy)
        .or_else(|| tooling::find_binary(context, &["gdtidy"]));
    let Some(gdtidy) = gdtidy else {
        for path in unconverted {
            println!(
                "warning: {} is not read by gdtidy; run `gdsuite setup`, which installs gdtidy and converts it into gdtidy.toml",
                path.display()
            );
        }
        return Ok(());
    };
    if let Some((files, notes)) = tooling::convert_tool_configs(context, &gdtidy)? {
        println!("converted {} into gdtidy.toml", files.join(" and "));
        for note in notes {
            println!("gdtidy migrate: {note}");
        }
    }
    Ok(())
}

fn rewrite_entrypoints(context: &ProjectContext) -> Result<()> {
    rewrite_globbed_files(
        context,
        &[".github/workflows/**/*.yml", ".github/workflows/**/*.yaml"],
    )?;
    rewrite_globbed_files(context, &["README.md", "docs/**/*.md"])?;
    Ok(())
}

fn rewrite_globbed_files(context: &ProjectContext, patterns: &[&str]) -> Result<()> {
    let walker = GlobWalkerBuilder::from_patterns(&context.root, patterns)
        .follow_links(false)
        .build()
        .context("failed to build migration file search")?;

    for entry in walker.filter_map(Result::ok) {
        let path = entry.path();
        let original = fs::read_to_string(path)
            .with_context(|| format!("failed to read {}", path.display()))?;
        let updated = if path
            .extension()
            .and_then(|ext| ext.to_str())
            .is_some_and(|ext| ext.eq_ignore_ascii_case("md"))
        {
            rewrite_markdown_entrypoints(&original)
        } else {
            rewrite_command_lines(&original)
        };
        if updated != original {
            fs::write(path, updated)
                .with_context(|| format!("failed to write {}", path.display()))?;
        }
    }

    Ok(())
}

fn rewrite_markdown_entrypoints(contents: &str) -> String {
    let mut output = Vec::new();
    let mut in_code_block = false;

    for line in contents.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("```") {
            in_code_block = !in_code_block;
            output.push(line.to_string());
            continue;
        }

        let rewritten = if in_code_block || looks_like_command_line(trimmed) {
            rewrite_command_lines(line)
        } else {
            rewrite_inline_code_spans(line)
        };
        output.push(rewritten);
    }

    let mut body = output.join("\n");
    if contents.ends_with('\n') {
        body.push('\n');
    }
    body
}

/// A tool command gdsuite now runs, and how to call it through gdsuite.
struct CommandRewrite {
    source: &'static str,
    target: &'static str,
    /// What goes after `--` before the original arguments, since
    /// `gdsuite health -- ...` replaces gdhealth's subcommand.
    forwarded_prefix: &'static str,
}

const COMMAND_REWRITES: &[CommandRewrite] = &[
    CommandRewrite {
        source: "gdhealth health",
        target: "gdsuite health",
        forwarded_prefix: "health ",
    },
    CommandRewrite {
        source: "gdtest",
        target: "gdsuite test",
        forwarded_prefix: "",
    },
];

/// Rewrites commands in a prose line's code spans. A span that is only a
/// tool's name (`` `gdtest` ``) names the tool, as in "put `gdtest` on PATH",
/// so it is left alone.
fn rewrite_inline_code_spans(line: &str) -> String {
    let pieces: Vec<&str> = line.split('`').collect();
    pieces
        .iter()
        .enumerate()
        .map(|(index, piece)| {
            let is_span = index % 2 == 1 && index + 1 < pieces.len();
            if is_span && piece.trim() != "gdtest" {
                rewrite_command_lines(piece)
            } else {
                (*piece).to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("`")
}

/// Rewrites the tool commands in a shell line. Only a command in command
/// position is rewritten (not `cargo install gdtest` or `/path/to/gdtest`),
/// and its arguments go after `--`, which gdsuite needs to forward them.
fn rewrite_command_lines(line: &str) -> String {
    let mut output = String::with_capacity(line.len());
    let mut index = 0usize;

    while index < line.len() {
        let rest = &line[index..];
        let rewrite = COMMAND_REWRITES.iter().find(|rewrite| {
            rest.starts_with(rewrite.source)
                && is_command_right_boundary(rest[rewrite.source.len()..].chars().next())
        });
        if let Some(rewrite) = rewrite.filter(|_| at_command_position(&line[..index])) {
            let after = &rest[rewrite.source.len()..];
            let raw_args = &after[..command_args_len(after)];
            let args = raw_args.trim();
            output.push_str(rewrite.target);
            if !args.is_empty() {
                output.push_str(" -- ");
                output.push_str(rewrite.forwarded_prefix);
                output.push_str(args);
            }
            output.push_str(&raw_args[raw_args.trim_end().len()..]);
            index += rewrite.source.len() + raw_args.len();
        } else {
            let ch = rest.chars().next().unwrap();
            output.push(ch);
            index += ch.len_utf8();
        }
    }

    output
}

/// Whether a command starting after `before` is the one a shell runs.
fn at_command_position(before: &str) -> bool {
    let trimmed = before.trim_end();
    if trimmed.len() == before.len() && !before.is_empty() {
        // Glued to what comes before, as in `my-gdtest` or `bin/gdtest`.
        return before.ends_with(['(', ';', '|', '&']);
    }
    trimmed.is_empty()
        || ["$", "run:", "&&", "||", ";", "|", "("]
            .iter()
            .any(|marker| trimmed.ends_with(marker))
}

/// Where a command's arguments end: at the next shell operator, the end of
/// an inline code span, or a comment.
fn command_args_len(after: &str) -> usize {
    ["&&", "||", ";", "|", "`", ")", " #"]
        .iter()
        .filter_map(|terminator| after.find(terminator))
        .min()
        .unwrap_or(after.len())
}

fn looks_like_command_line(trimmed: &str) -> bool {
    trimmed.starts_with("gdtest ")
        || trimmed == "gdtest"
        || trimmed.starts_with("gdhealth ")
        || trimmed.starts_with("- run:")
        || trimmed.starts_with("run:")
        || trimmed.starts_with("$ ")
}

fn is_command_right_boundary(ch: Option<char>) -> bool {
    ch.is_none_or(|byte| !is_command_name_char(byte))
}

fn is_command_name_char(byte: char) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, '/' | '_' | '-' | '.')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn commands_get_their_arguments_after_a_double_dash() {
        assert_eq!(
            rewrite_command_lines("gdtest test/cases/"),
            "gdsuite test -- test/cases/"
        );
        assert_eq!(rewrite_command_lines("gdtest"), "gdsuite test");
        assert_eq!(
            rewrite_command_lines("  gdtest --help  "),
            "  gdsuite test -- --help  "
        );
        assert_eq!(rewrite_command_lines("gdhealth health"), "gdsuite health");
        assert_eq!(
            rewrite_command_lines("gdhealth health --root ."),
            "gdsuite health -- health --root ."
        );
    }

    #[test]
    fn commands_are_rewritten_in_any_command_position() {
        assert_eq!(
            rewrite_command_lines("      - run: gdtest test/ && gdhealth health"),
            "      - run: gdsuite test -- test/ && gdsuite health"
        );
        assert_eq!(
            rewrite_command_lines("$ gdtest a.gd | tee out.txt # run one file"),
            "$ gdsuite test -- a.gd | tee out.txt # run one file"
        );
        assert_eq!(
            rewrite_command_lines("(gdtest a.gd)"),
            "(gdsuite test -- a.gd)"
        );
    }

    #[test]
    fn a_tool_name_that_is_not_a_command_is_kept() {
        for line in [
            r#"ln -sf "/path/to/gdtest" "$HOME/.local/bin/gdtest""#,
            "cargo install gdtest",
            "export GDTEST_BIN=gdtest",
            "echo run gdtest-analysis",
            "gdhealth metrics",
        ] {
            assert_eq!(rewrite_command_lines(line), line);
        }
    }

    #[test]
    fn markdown_rewrites_code_blocks_and_command_spans_but_not_prose() {
        let before = "\
Put `gdtest` on PATH, then run `gdtest test/cases/` or `gdhealth health`.

```bash
gdtest --help
cargo install gdtest
```
";
        let after = "\
Put `gdtest` on PATH, then run `gdsuite test -- test/cases/` or `gdsuite health`.

```bash
gdsuite test -- --help
cargo install gdtest
```
";
        assert_eq!(rewrite_markdown_entrypoints(before), after);
        // Running migrate again changes nothing.
        assert_eq!(rewrite_markdown_entrypoints(after), after);
    }
}
