use std::{
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, anyhow};
use clap::Parser;
use serde::Serialize;

use crate::{
    cli::{Cli, Commands, FmtArgs, LintArgs},
    config::{GdsuiteConfig, Manifest},
    fixers, process,
    tooling::{self, ProjectContext},
};

const TYPECHECK_SCRIPT_NAME: &str = "typecheck_checker.gd";

pub const EXIT_USAGE_ERROR: u8 = 2;
pub const EXIT_SETUP_ERROR: u8 = 10;
pub const EXIT_TOOLCHAIN_ERROR: u8 = 11;
pub const EXIT_VALIDATION_ERROR: u8 = 12;
pub const EXIT_INTERNAL_ERROR: u8 = 70;

#[derive(Debug, thiserror::Error)]
#[error("{message}")]
pub struct CommandError {
    pub code: u8,
    pub message: String,
}

pub fn run() -> Result<()> {
    let cli = Cli::parse();
    let root = fs::canonicalize(&cli.project_root)
        .with_context(|| format!("invalid project root {}", cli.project_root.display()))?;
    let context = ProjectContext::new(root);

    match cli.command {
        Commands::Setup => setup(&context, cli.godot_exe.as_deref()),
        Commands::Doctor => doctor(&context, cli.godot_exe.as_deref()),
        Commands::Fmt(args) => fmt(&context, args),
        Commands::Lint(args) => lint(&context, args),
        Commands::Typecheck => typecheck(&context),
        Commands::Test(args) => test(&context, args.args),
        Commands::Health(args) => health(&context, args.args),
        Commands::Check => check(&context),
        Commands::Ci => ci(&context),
    }
}

fn setup(context: &ProjectContext, cli_godot: Option<&Path>) -> Result<()> {
    context.ensure_dirs()?;
    let godot_path = if context.config_path.exists() {
        let existing = tooling::load_config(context)?;
        tooling::resolve_godot(context, cli_godot, Some(&existing))?
    } else {
        tooling::resolve_godot(context, cli_godot, None)?
    };

    GdsuiteConfig::write_if_missing(&context.config_path, Some(&godot_path))?;
    tooling::generate_external_configs(context)?;
    let config = tooling::load_config(context)?;
    let manifest = tooling::install_toolchain(context, &config, &godot_path)
        .map_err(|error| command_error(EXIT_SETUP_ERROR, error.to_string()))?;
    tooling::save_manifest(context, &manifest)?;
    tooling::save_versions(context, &config)?;
    run_gdtest_init(context, &manifest)?;
    let toolchain = tooling::resolve_toolchain(context, Some(&manifest))
        .map_err(|error| command_error(EXIT_TOOLCHAIN_ERROR, error.to_string()))?;
    tooling::verify_toolchain(context, &toolchain)
        .map_err(|error| command_error(EXIT_TOOLCHAIN_ERROR, error.to_string()))?;

    println!("setup complete");
    Ok(())
}

fn doctor(context: &ProjectContext, cli_godot: Option<&Path>) -> Result<()> {
    let config = tooling::load_config(context)
        .map_err(|error| command_error(EXIT_VALIDATION_ERROR, error.to_string()))?;
    let manifest = tooling::load_manifest(context)
        .map_err(|error| command_error(EXIT_VALIDATION_ERROR, error.to_string()))?;
    let toolchain = tooling::resolve_toolchain(context, Some(&manifest))
        .map_err(|error| command_error(EXIT_VALIDATION_ERROR, error.to_string()))?;
    let godot = tooling::resolve_godot(context, cli_godot, Some(&config))
        .map_err(|error| command_error(EXIT_VALIDATION_ERROR, error.to_string()))?;

    println!("project_root={}", context.root.display());
    println!("config={}", context.config_path.display());
    println!("gdtest_config={}", context.gdtest_config_path.display());
    println!("gdhealth_config={}", context.gdhealth_config_path.display());
    println!("manifest={}", context.manifest_path.display());
    println!("godot={}", godot.display());
    println!("python={}", toolchain.python.display());
    println!("gdformat={}", toolchain.gdformat.display());
    println!("gdlint={}", toolchain.gdlint.display());
    println!("gdtest={}", toolchain.gdtest.display());
    println!("gdhealth={}", toolchain.gdhealth.display());

    print_version("godot", &godot, &["--version"])?;
    print_version("python", &toolchain.python, &["--version"])?;
    print_version("gdtest", &toolchain.gdtest, &["--version"])?;
    print_version("gdhealth", &toolchain.gdhealth, &["--help"])?;

    println!("status=ok");
    Ok(())
}

fn fmt(context: &ProjectContext, args: FmtArgs) -> Result<()> {
    let config = load_config_or_error(context)?;
    let toolchain = load_toolchain_or_error(context)?;
    let files = fixers::discover_script_files(context, &config)?;

    let mut cmd_args = Vec::new();
    if args.check {
        cmd_args.push("--check".to_string());
    }
    cmd_args.extend(files.iter().map(|path| path.display().to_string()));
    process::run_checked(&toolchain.gdformat, &cmd_args, &context.root)
        .map_err(|error| command_error(EXIT_TOOLCHAIN_ERROR, error.to_string()))?;
    Ok(())
}

fn lint(context: &ProjectContext, args: LintArgs) -> Result<()> {
    let config = load_config_or_error(context)?;
    let toolchain = load_toolchain_or_error(context)?;

    if args.fix {
        let safe_report = fixers::apply_safe_fixes(context, &config, &toolchain.gdformat)?;
        if args.unsafe_fixes {
            let unsafe_report = fixers::apply_unsafe_fixes(context, &config)?;
            for skipped in unsafe_report.skipped {
                println!("skipped: {skipped}");
            }
            println!("unsafe_changed={}", unsafe_report.changed_files.len());
        }
        println!("safe_changed={}", safe_report.changed_files.len());
    }

    let cmd_args = lint_args(context, &config, &args.report)?;
    let output = process::run(&toolchain.gdlint, &cmd_args, &context.root)
        .map_err(|error| command_error(EXIT_TOOLCHAIN_ERROR, error.to_string()))?;
    print_lint_output(&output, &args.report)?;
    if !output.status.success() {
        return Err(command_error(
            EXIT_VALIDATION_ERROR,
            format!("lint failed with exit code {:?}", output.status.code()),
        ));
    }
    Ok(())
}

fn test(context: &ProjectContext, forwarded: Vec<String>) -> Result<()> {
    let config = load_config_or_error(context)?;
    let toolchain = load_toolchain_or_error(context)?;
    let mut args = if forwarded.is_empty() {
        vec![config.commands.default_test_target]
    } else {
        forwarded
    };
    if args.first().map(|value| value == "--").unwrap_or(false) {
        args.remove(0);
    }
    process::run_checked(&toolchain.gdtest, &args, &context.root)
        .map_err(|error| command_error(EXIT_TOOLCHAIN_ERROR, error.to_string()))?;
    Ok(())
}

fn health(context: &ProjectContext, forwarded: Vec<String>) -> Result<()> {
    let config = load_config_or_error(context)?;
    let toolchain = load_toolchain_or_error(context)?;
    let mut args = if forwarded.is_empty() {
        vec![config.commands.default_health_subcommand]
    } else {
        forwarded
    };
    if args.first().map(|value| value == "--").unwrap_or(false) {
        args.remove(0);
    }
    process::run_checked(&toolchain.gdhealth, &args, &context.root)
        .map_err(|error| command_error(EXIT_TOOLCHAIN_ERROR, error.to_string()))?;
    Ok(())
}

fn typecheck(context: &ProjectContext) -> Result<()> {
    let config = load_config_or_error(context)?;
    let toolchain = load_toolchain_or_error(context)?;
    let files = fixers::discover_script_files(context, &config)?;
    let checker_path = context.state_dir.join(TYPECHECK_SCRIPT_NAME);
    write_typecheck_script(&checker_path, &files)?;
    let args = vec![
        "--headless".to_string(),
        "--script".to_string(),
        format!("res://.gdsuite/state/{TYPECHECK_SCRIPT_NAME}"),
    ];
    process::run_checked(&toolchain.godot_exe, &args, &context.root)
        .map_err(|error| command_error(EXIT_VALIDATION_ERROR, error.to_string()))?;
    Ok(())
}

fn write_typecheck_script(path: &Path, files: &[PathBuf]) -> Result<()> {
    let mut lines = vec![
        "extends SceneTree".to_string(),
        String::new(),
        "func _init():".to_string(),
        "\tvar files = [".to_string(),
    ];
    for file in files {
        let res_path = format!("res://{}", file.display().to_string().replace('\\', "/"));
        lines.push(format!("\t\t\"{res_path}\","));
    }
    lines.extend([
        "\t]".to_string(),
        "\tvar errors = []".to_string(),
        "\tfor f in files:".to_string(),
        "\t\tif load(f) == null:".to_string(),
        "\t\t\terrors.append(f)".to_string(),
        "\tif errors.size() > 0:".to_string(),
        "\t\tfor e in errors:".to_string(),
        "\t\t\tprinterr(\"typecheck: \" + e)".to_string(),
        "\t\tquit(1)".to_string(),
        "\telse:".to_string(),
        "\t\tprint(\"typecheck: ok\")".to_string(),
        "\t\tquit(0)".to_string(),
    ]);
    fs::write(path, lines.join("\n") + "\n")
        .with_context(|| format!("failed to write typecheck script {}", path.display()))
}

fn check(context: &ProjectContext) -> Result<()> {
    let mut failures = Vec::new();

    if let Err(error) = fmt(context, FmtArgs { check: true }) {
        failures.push(format!("fmt: {}", display_error(&error)));
    }
    if let Err(error) = lint(
        context,
        LintArgs {
            fix: false,
            unsafe_fixes: false,
            report: "text".to_string(),
        },
    ) {
        failures.push(format!("lint: {}", display_error(&error)));
    }
    if let Err(error) = typecheck(context) {
        failures.push(format!("typecheck: {}", display_error(&error)));
    }

    if !failures.is_empty() {
        for failure in failures {
            println!("{failure}");
        }
        return Err(command_error(
            EXIT_VALIDATION_ERROR,
            "one or more checks failed".to_string(),
        ));
    }

    println!("check passed");
    Ok(())
}

fn ci(context: &ProjectContext) -> Result<()> {
    let mut failures = Vec::new();

    if let Err(error) = fmt(context, FmtArgs { check: true }) {
        failures.push(format!("fmt: {}", display_error(&error)));
    }
    if let Err(error) = lint(
        context,
        LintArgs {
            fix: false,
            unsafe_fixes: false,
            report: "text".to_string(),
        },
    ) {
        failures.push(format!("lint: {}", display_error(&error)));
    }
    if let Err(error) = typecheck(context) {
        failures.push(format!("typecheck: {}", display_error(&error)));
    }
    if let Err(error) = test(context, Vec::new()) {
        failures.push(format!("test: {}", display_error(&error)));
    }
    if let Err(error) = health(context, Vec::new()) {
        failures.push(format!("health: {}", display_error(&error)));
    }

    if !failures.is_empty() {
        for failure in failures {
            println!("{failure}");
        }
        return Err(command_error(
            EXIT_VALIDATION_ERROR,
            "one or more checks failed".to_string(),
        ));
    }

    println!("ci passed");
    Ok(())
}

fn load_config_or_error(context: &ProjectContext) -> Result<GdsuiteConfig> {
    tooling::load_config(context)
        .map_err(|error| command_error(EXIT_USAGE_ERROR, error.to_string()))
}

fn load_toolchain_or_error(context: &ProjectContext) -> Result<tooling::Toolchain> {
    let manifest = tooling::load_manifest(context)
        .map_err(|error| command_error(EXIT_USAGE_ERROR, error.to_string()))?;
    tooling::resolve_toolchain(context, Some(&manifest))
        .map_err(|error| command_error(EXIT_TOOLCHAIN_ERROR, error.to_string()))
}

fn lint_args(
    context: &ProjectContext,
    config: &GdsuiteConfig,
    report: &str,
) -> Result<Vec<String>> {
    let mut args = Vec::new();
    if report == "json" {
        args.push("--verbose".to_string());
    }
    for file in fixers::discover_script_files(context, config)? {
        args.push(file.display().to_string());
    }
    Ok(args)
}

fn print_lint_output(output: &process::ProcessOutput, report: &str) -> Result<()> {
    if report == "json" {
        let report = JsonLintReport::from_output(output);
        println!("{}", serde_json::to_string_pretty(&report)?);
        return Ok(());
    }

    if !output.stdout.is_empty() {
        print!("{}", output.stdout);
    }
    if !output.stderr.is_empty() {
        eprint!("{}", output.stderr);
    }
    Ok(())
}

#[derive(Debug, Serialize)]
struct JsonLintReport {
    status: &'static str,
    success: bool,
    exit_code: Option<i32>,
    summary: Option<String>,
    findings: Vec<JsonLintFinding>,
    stderr: Vec<String>,
    raw_stdout: Vec<String>,
}

#[derive(Debug, Serialize)]
struct JsonLintFinding {
    file: String,
    line: usize,
    severity: String,
    message: String,
    rule: Option<String>,
}

impl JsonLintReport {
    fn from_output(output: &process::ProcessOutput) -> Self {
        let mut findings = Vec::new();
        let mut raw_stdout = Vec::new();

        for line in output.stdout.lines().filter(|line| !line.trim().is_empty()) {
            if let Some(finding) = parse_lint_finding(line) {
                findings.push(finding);
            } else {
                raw_stdout.push(line.to_string());
            }
        }

        let stderr = output
            .stderr
            .lines()
            .filter(|line| !line.trim().is_empty())
            .map(ToString::to_string)
            .collect::<Vec<_>>();

        let summary = raw_stdout
            .iter()
            .find(|line| {
                line.starts_with("Success: ")
                    || line.starts_with("Failure: ")
                    || line.starts_with("Cannot open file ")
            })
            .cloned()
            .or_else(|| stderr.last().cloned());

        Self {
            status: if output.status.success() {
                "ok"
            } else {
                "failed"
            },
            success: output.status.success(),
            exit_code: output.status.code(),
            summary,
            findings,
            stderr,
            raw_stdout,
        }
    }
}

fn parse_lint_finding(line: &str) -> Option<JsonLintFinding> {
    let (prefix, remainder) = line.split_once(": Error: ")?;
    let (file, line_number) = prefix.rsplit_once(':')?;
    let line = line_number.parse().ok()?;
    let parsed_rule = remainder
        .rsplit_once(" (")
        .and_then(|(message, suffix)| suffix.strip_suffix(')').map(|rule| (message, rule)));
    let (message, _) = parsed_rule.unwrap_or((remainder, ""));

    Some(JsonLintFinding {
        file: file.to_string(),
        line,
        severity: "error".to_string(),
        message: message.to_string(),
        rule: parsed_rule.map(|(_, value)| value.to_string()),
    })
}

fn run_gdtest_init(context: &ProjectContext, manifest: &Manifest) -> Result<()> {
    let gdtest = manifest
        .gdtest
        .as_ref()
        .ok_or_else(|| anyhow!("missing gdtest path in manifest"))?;
    let args = vec!["init".to_string()];
    process::run_checked(gdtest, &args, &context.root)?;
    Ok(())
}

fn print_version(name: &str, path: &Path, args: &[&str]) -> Result<()> {
    let args = args
        .iter()
        .map(|value| value.to_string())
        .collect::<Vec<_>>();
    let output = process::run_checked(path, &args, PathBuf::from(".").as_path())?;
    let version = output.stdout.trim().lines().next().unwrap_or("unknown");
    println!("{name}_version={version}");
    Ok(())
}

fn command_error(code: u8, message: String) -> anyhow::Error {
    CommandError { code, message }.into()
}

fn display_error(error: &anyhow::Error) -> String {
    error
        .downcast_ref::<CommandError>()
        .map(|value| value.message.clone())
        .unwrap_or_else(|| error.to_string())
}
