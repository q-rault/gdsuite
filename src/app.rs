use std::{
    fs,
    path::{Path, PathBuf},
    time::Instant,
};

use anyhow::{Context, Result, anyhow};
use clap::Parser;
use serde::Serialize;

use crate::{
    cli::{Cli, Commands, FmtArgs, ForwardReportArgs, LintArgs, ReportArgs, RunArgs},
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
        Commands::Init => init(&context, cli.godot_exe.as_deref()),
        Commands::Setup => setup(&context, cli.godot_exe.as_deref()),
        Commands::Doctor => doctor(&context, cli.godot_exe.as_deref()),
        Commands::Fmt(args) => fmt(&context, args),
        Commands::Lint(args) => lint(&context, args),
        Commands::Typecheck => typecheck(&context),
        Commands::Test(args) => test(&context, args.args),
        Commands::Health(args) => health(&context, args),
        Commands::Check(args) => check(&context, args),
        Commands::Ci(args) => ci(&context, args),
        Commands::Report(args) => report(&context, args),
    }
}

fn init(context: &ProjectContext, cli_godot: Option<&Path>) -> Result<()> {
    setup_impl(context, cli_godot)?;
    println!("init complete");
    Ok(())
}

fn setup(context: &ProjectContext, cli_godot: Option<&Path>) -> Result<()> {
    setup_impl(context, cli_godot)?;
    println!("setup complete");
    Ok(())
}

fn setup_impl(context: &ProjectContext, cli_godot: Option<&Path>) -> Result<()> {
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
    Ok(())
}

fn doctor(context: &ProjectContext, cli_godot: Option<&Path>) -> Result<()> {
    let config = load_config_or_error(context)?;
    let manifest = load_manifest_or_error(context, EXIT_VALIDATION_ERROR)?;
    let toolchain = resolve_toolchain_or_error(context, Some(&manifest), EXIT_VALIDATION_ERROR)?;
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

fn health(context: &ProjectContext, args: ForwardReportArgs) -> Result<()> {
    if args.report == "json" {
        let config = load_config_or_error(context)?;
        let step = run_health_step(context, &config, args.args)?;
        let report = SuiteReport::from_steps("health", context, vec![step]);
        print_and_store_report(context, &config, "health", &report)?;
        if report.status == "failed" {
            return Err(command_error(
                EXIT_VALIDATION_ERROR,
                "health failed".to_string(),
            ));
        }
        return Ok(());
    }

    let forwarded = args.args;
    let config = load_config_or_error(context)?;
    let toolchain = load_toolchain_or_error(context)?;
    let mut args = if forwarded.is_empty() {
        vec![config.health.command.clone()]
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

fn check(context: &ProjectContext, args: RunArgs) -> Result<()> {
    let config = load_config_or_error(context)?;
    let steps = vec![
        run_fmt_step(context, true)?,
        run_lint_step(context, &config)?,
        run_typecheck_step(context)?,
    ];
    finish_pipeline(context, &config, "check", args.report, steps)
}

fn ci(context: &ProjectContext, args: RunArgs) -> Result<()> {
    let config = load_config_or_error(context)?;
    let mut steps = vec![
        run_fmt_step(context, true)?,
        run_lint_step(context, &config)?,
        run_typecheck_step(context)?,
        run_test_step(context)?,
    ];
    if config.health.enabled {
        steps.push(run_health_step(context, &config, Vec::new())?);
    }
    finish_pipeline(context, &config, "ci", args.report, steps)
}

fn report(context: &ProjectContext, args: ReportArgs) -> Result<()> {
    let config = load_config_or_error(context)?;
    let path = report_path(context, &config, &args.name);
    if !path.exists() {
        return Err(command_error(
            EXIT_USAGE_ERROR,
            format!(
                "report artifact not found at {}; run `gdsuite check --report json`, `gdsuite ci --report json`, or `gdsuite health --report json` first",
                path.display()
            ),
        ));
    }
    let body = fs::read_to_string(&path)
        .with_context(|| format!("failed to read report {}", path.display()))
        .map_err(|error| command_error(EXIT_USAGE_ERROR, error.to_string()))?;
    println!("{body}");
    Ok(())
}

fn load_config_or_error(context: &ProjectContext) -> Result<GdsuiteConfig> {
    if !context.config_path.exists() {
        return Err(command_error(
            EXIT_USAGE_ERROR,
            format!(
                "missing {}; run `gdsuite init` first to bootstrap this project",
                context.config_path.display()
            ),
        ));
    }

    tooling::load_config(context)
        .map_err(|error| command_error(EXIT_USAGE_ERROR, error.to_string()))
}

fn load_toolchain_or_error(context: &ProjectContext) -> Result<tooling::Toolchain> {
    let manifest = load_manifest_or_error(context, EXIT_USAGE_ERROR)?;
    resolve_toolchain_or_error(context, Some(&manifest), EXIT_TOOLCHAIN_ERROR)
}

fn load_manifest_or_error(context: &ProjectContext, code: u8) -> Result<Manifest> {
    if !context.manifest_path.exists() {
        return Err(command_error(
            code,
            format!(
                "missing {}; run `gdsuite init` or `gdsuite setup` to install the local toolchain",
                context.manifest_path.display()
            ),
        ));
    }

    tooling::load_manifest(context).map_err(|error| command_error(code, error.to_string()))
}

fn resolve_toolchain_or_error(
    context: &ProjectContext,
    manifest: Option<&Manifest>,
    code: u8,
) -> Result<tooling::Toolchain> {
    tooling::resolve_toolchain(context, Some(&manifest.cloned().unwrap_or_default())).map_err(
        |error| {
            command_error(
                code,
                format!(
                    "{}; run `gdsuite setup` to repair the local toolchain",
                    error
                ),
            )
        },
    )
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

fn run_fmt_step(context: &ProjectContext, check: bool) -> Result<StepReport> {
    let started = Instant::now();
    match fmt(context, FmtArgs { check }) {
        Ok(()) => Ok(StepReport::ok(
            "fmt",
            started.elapsed().as_millis(),
            "format check passed",
        )),
        Err(error) => Ok(StepReport::failed(
            "fmt",
            started.elapsed().as_millis(),
            display_error(&error),
        )),
    }
}

fn run_lint_step(context: &ProjectContext, config: &GdsuiteConfig) -> Result<StepReport> {
    let started = Instant::now();
    let toolchain = load_toolchain_or_error(context)?;
    let cmd_args = lint_args(context, config, "json")?;
    let output = process::run(&toolchain.gdlint, &cmd_args, &context.root)
        .map_err(|error| command_error(EXIT_TOOLCHAIN_ERROR, error.to_string()))?;
    let lint_report = JsonLintReport::from_output(&output);
    let findings = lint_report
        .findings
        .iter()
        .map(serde_json::to_value)
        .collect::<serde_json::Result<Vec<_>>>()?;

    Ok(StepReport {
        name: "lint".to_string(),
        status: if output.status.success() {
            "ok".to_string()
        } else {
            "failed".to_string()
        },
        duration_ms: started.elapsed().as_millis(),
        summary: lint_report
            .summary
            .unwrap_or_else(|| "lint completed".to_string()),
        findings,
        artifacts: Vec::new(),
    })
}

fn run_typecheck_step(context: &ProjectContext) -> Result<StepReport> {
    let started = Instant::now();
    match typecheck(context) {
        Ok(()) => Ok(StepReport::ok(
            "typecheck",
            started.elapsed().as_millis(),
            "typecheck passed",
        )),
        Err(error) => Ok(StepReport::failed(
            "typecheck",
            started.elapsed().as_millis(),
            display_error(&error),
        )),
    }
}

fn run_test_step(context: &ProjectContext) -> Result<StepReport> {
    let started = Instant::now();
    match test(context, Vec::new()) {
        Ok(()) => Ok(StepReport::ok(
            "test",
            started.elapsed().as_millis(),
            "tests passed",
        )),
        Err(error) => Ok(StepReport::failed(
            "test",
            started.elapsed().as_millis(),
            display_error(&error),
        )),
    }
}

fn run_health_step(
    context: &ProjectContext,
    config: &GdsuiteConfig,
    forwarded: Vec<String>,
) -> Result<StepReport> {
    let started = Instant::now();
    let toolchain = load_toolchain_or_error(context)?;
    let mut args = if forwarded.is_empty() {
        vec![config.health.command.clone()]
    } else {
        forwarded
    };
    if args.first().map(|value| value == "--").unwrap_or(false) {
        args.remove(0);
    }

    let output = process::run(&toolchain.gdhealth, &args, &context.root)
        .map_err(|error| command_error(EXIT_TOOLCHAIN_ERROR, error.to_string()))?;
    let bundle_dir = context.root.join(&config.health.bundle_dir);
    let mut artifacts = Vec::new();

    for (name, path) in [
        ("summary", bundle_dir.join("summary.txt")),
        ("health_json", bundle_dir.join("health.json")),
    ] {
        if path.exists() {
            artifacts.push(ArtifactRef {
                name: name.to_string(),
                path: path.display().to_string(),
            });
        }
    }

    let summary = if output.status.success() {
        if artifacts.is_empty() {
            "health passed".to_string()
        } else {
            format!("health passed; artifacts in {}", bundle_dir.display())
        }
    } else if !output.stderr.trim().is_empty() {
        output.stderr.trim().to_string()
    } else if !output.stdout.trim().is_empty() {
        output.stdout.trim().to_string()
    } else {
        "health failed".to_string()
    };

    Ok(StepReport {
        name: "health".to_string(),
        status: if output.status.success() {
            "ok".to_string()
        } else {
            "failed".to_string()
        },
        duration_ms: started.elapsed().as_millis(),
        summary,
        findings: Vec::new(),
        artifacts,
    })
}

fn finish_pipeline(
    context: &ProjectContext,
    config: &GdsuiteConfig,
    command: &str,
    report_mode: String,
    steps: Vec<StepReport>,
) -> Result<()> {
    let report = SuiteReport::from_steps(command, context, steps);
    if report_mode == "json" {
        print_and_store_report(context, config, command, &report)?;
    } else if report.status == "ok" {
        println!("{command} passed");
    } else {
        for step in report.steps.iter().filter(|step| step.status == "failed") {
            println!("{}: {}", step.name, step.summary);
        }
    }

    if report.status == "failed" {
        return Err(command_error(
            EXIT_VALIDATION_ERROR,
            "one or more checks failed".to_string(),
        ));
    }
    Ok(())
}

fn print_and_store_report(
    context: &ProjectContext,
    config: &GdsuiteConfig,
    command: &str,
    report: &SuiteReport,
) -> Result<()> {
    let json = serde_json::to_string_pretty(report)?;
    println!("{json}");
    write_report_artifacts(context, config, command, &json)?;
    Ok(())
}

fn write_report_artifacts(
    context: &ProjectContext,
    config: &GdsuiteConfig,
    command: &str,
    json: &str,
) -> Result<()> {
    let dir = context.root.join(&config.reports.artifacts_dir);
    fs::create_dir_all(&dir)
        .with_context(|| format!("failed to create report directory {}", dir.display()))?;
    fs::write(dir.join(format!("{command}.json")), json)
        .with_context(|| format!("failed to write {} report", command))?;
    fs::write(dir.join("latest.json"), json)
        .with_context(|| format!("failed to write latest report in {}", dir.display()))?;
    Ok(())
}

fn report_path(context: &ProjectContext, config: &GdsuiteConfig, name: &str) -> PathBuf {
    let dir = context.root.join(&config.reports.artifacts_dir);
    if name == "latest" {
        return dir.join("latest.json");
    }
    dir.join(format!("{name}.json"))
}

#[derive(Debug, Serialize)]
struct SuiteReport {
    status: String,
    command: String,
    project_root: String,
    duration_ms: u128,
    steps: Vec<StepReport>,
}

impl SuiteReport {
    fn from_steps(command: &str, context: &ProjectContext, steps: Vec<StepReport>) -> Self {
        let duration_ms = steps.iter().map(|step| step.duration_ms).sum();
        let status = if steps.iter().any(|step| step.status == "failed") {
            "failed"
        } else {
            "ok"
        };

        Self {
            status: status.to_string(),
            command: command.to_string(),
            project_root: context.root.display().to_string(),
            duration_ms,
            steps,
        }
    }
}

#[derive(Debug, Serialize)]
struct StepReport {
    name: String,
    status: String,
    duration_ms: u128,
    summary: String,
    findings: Vec<serde_json::Value>,
    artifacts: Vec<ArtifactRef>,
}

impl StepReport {
    fn ok(name: &str, duration_ms: u128, summary: impl Into<String>) -> Self {
        Self {
            name: name.to_string(),
            status: "ok".to_string(),
            duration_ms,
            summary: summary.into(),
            findings: Vec::new(),
            artifacts: Vec::new(),
        }
    }

    fn failed(name: &str, duration_ms: u128, summary: impl Into<String>) -> Self {
        Self {
            name: name.to_string(),
            status: "failed".to_string(),
            duration_ms,
            summary: summary.into(),
            findings: Vec::new(),
            artifacts: Vec::new(),
        }
    }
}

#[derive(Debug, Serialize)]
struct ArtifactRef {
    name: String,
    path: String,
}
