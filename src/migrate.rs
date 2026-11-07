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
    let warnings = collect_migration_warnings(context, &value);

    if let Some(project) = value.get("project").and_then(|value| value.as_table()) {
        if let Some(root) = project.get("root").and_then(|value| value.as_str()) {
            config.project.root = root.to_string();
        }
        if let Some(godot_exe) = project.get("godot_exe").and_then(|value| value.as_str()) {
            config.project.godot_exe = Some(godot_exe.into());
        }
    }
    if let Some(tools) = value.get("tools").and_then(|value| value.as_table()) {
        if let Some(version) = tools.get("gdtoolkit").and_then(|value| value.as_str()) {
            config.tools.gdtoolkit = version.to_string();
        }
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
    } else if config.project.godot_exe.is_none()
        && let Ok(path) = tooling::resolve_godot(context, None, Some(&config))
    {
        config.project.godot_exe = Some(path);
    }

    Ok((config, warnings))
}

fn collect_migration_warnings(context: &ProjectContext, value: &toml::Value) -> Vec<String> {
    let mut warnings = Vec::new();
    let Some(table) = value.as_table() else {
        return warnings;
    };

    if table.contains_key("lint") {
        warnings.push(
            "Moved out of suite contract: create `gdlintrc` with your lint rules.".to_string(),
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
    if !path_exists_any(context, &["gdformatrc", ".gdformatrc"]) {
        warnings.push(
            "Moved out of suite contract: create `gdformatrc` if you need formatter settings."
                .to_string(),
        );
    }
    warnings
}

fn path_exists_any(context: &ProjectContext, paths: &[&str]) -> bool {
    paths.iter().any(|path| context.root.join(path).exists())
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

fn rewrite_inline_code_spans(line: &str) -> String {
    line.replace("`gdhealth health`", "`gdsuite health`")
        .replace("`gdtest`", "`gdsuite test`")
}

fn rewrite_command_lines(line: &str) -> String {
    let after_gdhealth = rewrite_command(line, &["gdhealth", "health"], &["gdsuite", "health"]);
    rewrite_command(&after_gdhealth, &["gdtest"], &["gdsuite", "test"])
}

fn rewrite_command(contents: &str, source: &[&str], target: &[&str]) -> String {
    let source_text = source.join(" ");
    let target_text = target.join(" ");
    let mut output = String::with_capacity(contents.len());
    let mut index = 0usize;

    while index < contents.len() {
        if contents[index..].starts_with(&source_text)
            && command_boundary(contents, index, source_text.len())
        {
            output.push_str(&target_text);
            index += source_text.len();
        } else {
            let ch = contents[index..].chars().next().unwrap();
            output.push(ch);
            index += ch.len_utf8();
        }
    }

    output
}

fn looks_like_command_line(trimmed: &str) -> bool {
    trimmed.starts_with("gdtest ")
        || trimmed == "gdtest"
        || trimmed.starts_with("gdhealth ")
        || trimmed.starts_with("- run:")
        || trimmed.starts_with("run:")
        || trimmed.starts_with("$ ")
}

fn command_boundary(text: &str, start: usize, len: usize) -> bool {
    let prev = text[..start].chars().next_back();
    let next = text[start + len..].chars().next();
    is_command_left_boundary(prev) && is_command_right_boundary(next)
}

fn is_command_left_boundary(ch: Option<char>) -> bool {
    ch.is_none_or(|byte| !is_command_name_char(byte))
}

fn is_command_right_boundary(ch: Option<char>) -> bool {
    ch.is_none_or(|byte| !is_command_name_char(byte))
}

fn is_command_name_char(byte: char) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, '/' | '_' | '-' | '.')
}
