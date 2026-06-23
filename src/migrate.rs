use std::{fs, path::Path};

use anyhow::{Context, Result, anyhow};
use globwalk::GlobWalkerBuilder;

use crate::{
    config::GdsuiteConfig,
    tooling::{self, ProjectContext},
};

pub fn migrate_project(context: &ProjectContext, cli_godot: Option<&Path>) -> Result<()> {
    context.ensure_dirs()?;
    let config = migrate_suite_config(context, cli_godot)?;
    tooling::sync_product_configs(context, &config)?;
    rewrite_entrypoints(context)?;
    Ok(())
}

fn migrate_suite_config(
    context: &ProjectContext,
    cli_godot: Option<&Path>,
) -> Result<GdsuiteConfig> {
    let mut config = load_migratable_suite_config(context)?;
    tooling::import_product_configs_into_canonical(context, &mut config)?;

    if let Some(path) = cli_godot {
        config.project.godot_exe = Some(path.to_path_buf());
    } else if config.project.godot_exe.is_none()
        && let Ok(path) = tooling::resolve_godot(context, None, Some(&config))
    {
        config.project.godot_exe = Some(path);
    }

    config.write(&context.config_path)?;
    Ok(config)
}

fn load_migratable_suite_config(context: &ProjectContext) -> Result<GdsuiteConfig> {
    if !context.config_path.exists() {
        return Ok(GdsuiteConfig::default());
    }

    match tooling::load_config(context) {
        Ok(config) => Ok(config),
        Err(_) => load_legacy_suite_config(context),
    }
}

fn load_legacy_suite_config(context: &ProjectContext) -> Result<GdsuiteConfig> {
    let body = fs::read_to_string(&context.config_path)
        .with_context(|| format!("failed to read {}", context.config_path.display()))?;
    let value: toml::Value = body
        .parse()
        .with_context(|| format!("failed to parse {}", context.config_path.display()))?;
    if value.get("paths").is_none() {
        return Err(anyhow!(
            "failed to parse config {}; run `gdsuite migrate` after fixing the file",
            context.config_path.display()
        ));
    }

    let mut config = GdsuiteConfig::default();
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
        if let Some(source) = tools.get("gdhealth_source").and_then(|value| value.as_str()) {
            config.tools.gdhealth_source = source.into();
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
    if let Some(fix) = value.get("fix").and_then(|value| value.as_table()) {
        if let Some(rules) = string_array(fix.get("safe_rules")) {
            config.fix.safe_rules = rules;
        }
        if let Some(rules) = string_array(fix.get("unsafe_rules")) {
            config.fix.unsafe_rules = rules;
        }
    }
    if let Some(lint) = value.get("lint").and_then(|value| value.as_table()) {
        if let Some(line_length) = lint.get("line_length").and_then(|value| value.as_integer()) {
            config.lint.line_length = line_length as usize;
        }
    }
    if let Some(reports) = value.get("reports").and_then(|value| value.as_table()) {
        if let Some(default) = reports.get("default").and_then(|value| value.as_str()) {
            config.reports.default = default.to_string();
        }
        if let Some(dir) = reports.get("artifacts_dir").and_then(|value| value.as_str()) {
            config.reports.artifacts_dir = dir.into();
        }
    }
    if let Some(health) = value.get("health").and_then(|value| value.as_table()) {
        if let Some(enabled) = health.get("enabled").and_then(|value| value.as_bool()) {
            config.health.enabled = enabled;
        }
        if let Some(command) = health.get("command").and_then(|value| value.as_str()) {
            config.health.command = command.to_string();
        }
        if let Some(fail_on) = health.get("fail_on").and_then(|value| value.as_str()) {
            config.health.fail_on = fail_on.to_string();
        }
        if let Some(bundle_dir) = health.get("bundle_dir").and_then(|value| value.as_str()) {
            config.health.bundle_dir = bundle_dir.into();
        }
    }
    if let Some(profiles) = value.get("profiles").and_then(|value| value.as_table()) {
        config.profiles.entries.clear();
        for (name, profile) in profiles {
            let Some(table) = profile.as_table() else {
                continue;
            };
            let pipeline = string_array(table.get("pipeline")).unwrap_or_default();
            let health_fail_on = table
                .get("health_fail_on")
                .and_then(|value| value.as_str())
                .map(ToString::to_string);
            config.profiles.entries.insert(
                name.clone(),
                crate::config::ProfileConfig {
                    pipeline,
                    health_fail_on,
                },
            );
        }
    }

    if let Some(paths) = value.get("paths").and_then(|value| value.as_table()) {
        let scripts = string_array(paths.get("scripts")).unwrap_or_else(|| config.lint.include.clone());
        let exclude = string_array(paths.get("exclude")).unwrap_or_else(|| config.lint.exclude.clone());
        config.lint.include = scripts.clone();
        config.lint.exclude = exclude.clone();
        config.typecheck.include = scripts.clone();
        config.typecheck.exclude = exclude.clone();
        config.gdtest.patterns = scripts
            .iter()
            .filter(|pattern| pattern.contains("test"))
            .cloned()
            .collect::<Vec<_>>();
        if config.gdtest.patterns.is_empty() {
            config.gdtest.patterns = vec!["test/**/*.gd".to_string(), "**/*.test.gd".to_string()];
        }
        config.gdtest.coverage_exclude = exclude.clone();
        config.gdhealth.discovery.include = scripts;
        if config.gdhealth.discovery.include.is_empty() {
            config.gdhealth.discovery.include = vec!["**/*.gd".to_string()];
        }
        config.gdhealth.discovery.exclude = exclude
            .into_iter()
            .filter(|pattern| pattern != ".gdsuite/**")
            .collect();
    }

    Ok(config)
}

fn string_array(value: Option<&toml::Value>) -> Option<Vec<String>> {
    value.and_then(|value| {
        value.as_array().map(|values| {
            values
                .iter()
                .filter_map(|value| value.as_str().map(ToString::to_string))
                .collect::<Vec<_>>()
        })
    })
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
