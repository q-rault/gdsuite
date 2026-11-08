use std::{fs, path::Path};

use anyhow::{Context, Result, anyhow};
use globwalk::GlobWalkerBuilder;

use crate::{
    config::{self, GdsuiteConfig},
    tooling::{self, ProjectContext},
};

pub fn migrate_project(context: &ProjectContext, cli_godot: Option<&Path>) -> Result<()> {
    context.ensure_dirs()?;
    let (config, warnings) = migrate_suite_config(context, cli_godot)?;
    let tool_sections = collect_tool_sections(context)?;
    rewrite_entrypoints(context)?;
    for warning in warnings {
        println!("warning: {warning}");
    }
    // Written before converting gdlintrc, which gdtidy adds to gdsuite.toml.
    config.write_with_sections(&context.config_path, &tool_sections.text)?;
    for moved in &tool_sections.moved {
        fs::remove_file(context.root.join(moved.file))
            .with_context(|| format!("failed to remove {}", moved.file))?;
        println!("moved {} into gdsuite.toml {}", moved.file, moved.sections);
    }
    for (file, sections) in &tool_sections.kept_apart {
        println!(
            "warning: gdsuite.toml already has {sections}, so {file} is ignored; \
             keep these settings in one file and delete the other"
        );
    }
    convert_tool_configs(context)?;
    println!("migrate complete");
    Ok(())
}

/// A tool's own configuration file, and the `gdsuite.toml` sections that
/// hold the same settings.
struct ToolFile {
    file: &'static str,
    /// The section its settings go under; `None` for gdtidy.toml, whose
    /// `[lint]` and `[fmt]` already are gdsuite.toml's sections.
    section: Option<&'static str>,
    sections: &'static str,
}

const TOOL_FILES: &[ToolFile] = &[
    ToolFile {
        file: "gdtest.toml",
        section: Some("test"),
        sections: "[test]",
    },
    ToolFile {
        file: "gdhealth.toml",
        section: Some("health"),
        sections: "[health]",
    },
    ToolFile {
        file: "gdtidy.toml",
        section: None,
        sections: "[lint] and [fmt]",
    },
];

#[derive(Default)]
struct ToolSections {
    /// The tool sections to write after gdsuite's own.
    text: String,
    /// Tool files whose settings moved into `text`.
    moved: Vec<&'static ToolFile>,
    /// Tool files left in place because gdsuite.toml already has their
    /// sections: (file, sections).
    kept_apart: Vec<(&'static str, &'static str)>,
}

/// The tool sections gdsuite.toml keeps, then the tool files that move into
/// it, so a project ends with its settings in one file, as `vp migrate`
/// does for Vite+. Sections in gdsuite 0.1's shape are dropped (see the
/// migration warnings).
fn collect_tool_sections(context: &ProjectContext) -> Result<ToolSections> {
    let existing: toml::Table = fs::read_to_string(&context.config_path)
        .ok()
        .and_then(|body| body.parse().ok())
        .unwrap_or_default();
    let mut result = ToolSections::default();
    let mut kept = toml::Table::new();
    for name in config::TOOL_SECTIONS {
        if let Some(value) = existing.get(*name)
            && !config::is_legacy_tool_section(name, value)
        {
            kept.insert(name.to_string(), value.clone());
        }
    }
    if !kept.is_empty() {
        result
            .text
            .push_str(&toml::to_string(&kept).context("failed to write the tool sections")?);
    }

    for tool in TOOL_FILES {
        let path = context.root.join(tool.file);
        let Ok(text) = fs::read_to_string(&path) else {
            continue;
        };
        let has_section = match tool.section {
            Some(section) => kept.contains_key(section),
            None => kept.contains_key("lint") || kept.contains_key("fmt"),
        };
        if has_section {
            result.kept_apart.push((tool.file, tool.sections));
            continue;
        }
        let value: toml::Table = text
            .parse()
            .with_context(|| format!("failed to parse {}", path.display()))?;
        let section_text = match tool.section {
            Some(section) => nest_under(&text, &value, section).map_or_else(
                // A file the line-based nesting cannot handle keeps its
                // values, without its comments.
                || {
                    let mut wrapped = toml::Table::new();
                    wrapped.insert(section.to_string(), toml::Value::Table(value.clone()));
                    toml::to_string(&wrapped)
                },
                Ok,
            )?,
            None => text.clone(),
        };
        if !result.text.is_empty() {
            result.text.push('\n');
        }
        result.text.push_str(section_text.trim_end());
        result.text.push('\n');
        result.moved.push(tool);
    }
    Ok(result)
}

/// A tool file's text with every key under `[section]`: its top-level keys
/// get the section header, and each table header gets the section as
/// prefix (`[profile.ci]` becomes `[test.profile.ci]`), so comments stay.
/// `None` when the result does not hold exactly the file's values.
fn nest_under(text: &str, value: &toml::Table, section: &str) -> Option<String> {
    let mut nested = format!("[{section}]\n");
    for line in text.lines() {
        let indent = &line[..line.len() - line.trim_start().len()];
        let trimmed = line.trim_start();
        if let Some(rest) = trimmed.strip_prefix("[[") {
            nested.push_str(&format!("{indent}[[{section}.{}", rest.trim_start()));
        } else if let Some(rest) = trimmed.strip_prefix('[') {
            nested.push_str(&format!("{indent}[{section}.{}", rest.trim_start()));
        } else {
            nested.push_str(line);
        }
        nested.push('\n');
    }
    // A file with only tables needs no bare `[section]` header.
    let starts_with_table = text
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty() && !line.starts_with('#'))
        .is_some_and(|line| line.starts_with('['));
    if starts_with_table {
        nested = nested.split_once('\n').map(|(_, rest)| rest.to_string())?;
    }
    let parsed: toml::Table = nested.parse().ok()?;
    (parsed.len() == 1 && parsed.get(section) == Some(&toml::Value::Table(value.clone())))
        .then_some(nested)
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

    if table
        .get("lint")
        .is_some_and(|lint| config::is_legacy_tool_section("lint", lint))
    {
        warnings.push(
            "[lint] in gdsuite 0.1's format is dropped: [lint] now holds gdtidy's settings; `gdsuite setup` converts a gdlintrc into it.".to_string(),
        );
    }
    if table.contains_key("gdtest") {
        warnings.push(
            "[gdtest] is dropped: gdtest's settings go in [test], with the keys of gdtest.toml."
                .to_string(),
        );
    }
    if table.contains_key("gdhealth") {
        warnings.push(
            "[gdhealth] is dropped: gdhealth's settings go in [health], with the sections of gdhealth.toml.".to_string(),
        );
    }
    if table
        .get("health")
        .is_some_and(|health| config::is_legacy_tool_section("health", health))
    {
        warnings.push(
            "[health] in gdsuite 0.1's format is dropped: [health] now holds gdhealth's settings, with the sections of gdhealth.toml.".to_string(),
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

/// Convert gdlintrc and gdformatrc into gdsuite.toml when gdtidy is
/// installed; otherwise say that setup will do it, since it installs gdtidy
/// first.
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
                "warning: {} is not read by gdtidy; run `gdsuite setup`, which installs gdtidy and converts it into gdsuite.toml",
                path.display()
            );
        }
        return Ok(());
    };
    if let Some(conversion) = tooling::convert_tool_configs(context, &gdtidy)? {
        println!(
            "converted {} into {}",
            conversion.files.join(" and "),
            conversion.target
        );
        for note in conversion.notes {
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
