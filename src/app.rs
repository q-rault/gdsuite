use std::{
    fs,
    path::{Path, PathBuf},
    time::Instant,
};

use anyhow::{Context, Result};
use clap::Parser;
use serde::{Deserialize, Serialize};

use crate::{
    cli::{Cli, Commands, FmtArgs, ForwardReportArgs, LintArgs, ReportArgs, RunArgs, SetupArgs},
    config::{CompatibilityVerdict, GdsuiteConfig, Manifest},
    fixers, migrate, process,
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
        Commands::Migrate => migrate_project(&context, cli.godot_exe.as_deref()),
        Commands::Setup(args) => setup(&context, cli.godot_exe.as_deref(), args),
        Commands::Doctor => doctor(&context, cli.godot_exe.as_deref()),
        Commands::Fmt(args) => fmt(&context, args),
        Commands::Lint(args) => lint(&context, args),
        Commands::Typecheck => typecheck(&context),
        Commands::Test(args) => test(&context, args),
        Commands::Health(args) => health(&context, args),
        Commands::Check(args) => check(&context, args),
        Commands::Report(args) => report(&context, args),
    }
}

fn init(context: &ProjectContext, cli_godot: Option<&Path>) -> Result<()> {
    setup_impl(
        context,
        cli_godot,
        SetupArgs {
            sync_product_configs: false,
        },
    )?;
    println!("init complete");
    Ok(())
}

fn migrate_project(context: &ProjectContext, cli_godot: Option<&Path>) -> Result<()> {
    migrate::migrate_project(context, cli_godot)?;
    println!("migrate complete");
    Ok(())
}

fn setup(context: &ProjectContext, cli_godot: Option<&Path>, args: SetupArgs) -> Result<()> {
    setup_impl(context, cli_godot, args)?;
    println!("setup complete");
    Ok(())
}

fn setup_impl(context: &ProjectContext, cli_godot: Option<&Path>, args: SetupArgs) -> Result<()> {
    context.ensure_dirs()?;
    let godot_path = if context.config_path.exists() {
        let existing = tooling::load_config(context)?;
        tooling::resolve_godot(context, cli_godot, Some(&existing))?
    } else {
        tooling::resolve_godot(context, cli_godot, None)?
    };

    GdsuiteConfig::write_if_missing(&context.config_path, Some(&godot_path))?;
    let config = tooling::load_config(context)?;
    tooling::generate_external_configs(context, &config)?;
    if args.sync_product_configs {
        tooling::sync_product_configs(context, &config)?;
    }
    let install = tooling::install_toolchain(context, &config, &godot_path)
        .map_err(|error| command_error(EXIT_SETUP_ERROR, error.to_string()))?;
    tooling::save_manifest(context, &install.manifest)?;
    let addon_report = tooling::install_gdtest_addons(context, &config)
        .map_err(|error| command_error(EXIT_SETUP_ERROR, error.to_string()))?;
    for skipped in addon_report.skipped_modified {
        println!(
            "warning=gdtest addon install skipped modified file {}",
            skipped.display()
        );
    }
    let toolchain = tooling::resolve_toolchain(context, Some(&install.manifest))
        .map_err(|error| command_error(EXIT_TOOLCHAIN_ERROR, error.to_string()))?;
    tooling::verify_toolchain(context, &toolchain)
        .map_err(|error| command_error(EXIT_TOOLCHAIN_ERROR, error.to_string()))?;
    let installation =
        tooling::probe_installation_state(context, &config, &install.manifest, &install.requested)
            .map_err(|error| command_error(EXIT_TOOLCHAIN_ERROR, error.to_string()))?;
    tooling::save_installation_state(context, &installation)
        .map_err(|error| command_error(EXIT_INTERNAL_ERROR, error.to_string()))?;
    let compatibility = tooling::compatibility_report(&installation);
    for tool in [
        &compatibility.gdtoolkit,
        &compatibility.gdtest,
        &compatibility.gdhealth,
    ] {
        if tool.verdict == CompatibilityVerdict::Warning {
            println!("warning={}", tool.reason);
        }
    }
    if tooling::report_status(&compatibility) == CompatibilityVerdict::Error {
        let errors = [
            &compatibility.gdtoolkit,
            &compatibility.gdtest,
            &compatibility.gdhealth,
        ]
        .into_iter()
        .filter(|tool| tool.verdict == CompatibilityVerdict::Error)
        .map(|tool| tool.reason.clone())
        .collect::<Vec<_>>();
        return Err(command_error(EXIT_TOOLCHAIN_ERROR, errors.join("; ")));
    }
    Ok(())
}

fn doctor(context: &ProjectContext, cli_godot: Option<&Path>) -> Result<()> {
    let config = load_config_or_error(context)?;
    let manifest = if context.manifest_path.exists() {
        tooling::load_manifest(context).ok()
    } else {
        None
    };

    println!("project_root={}", context.root.display());
    println!("config={}", context.config_path.display());
    println!("gdtest_config={}", context.gdtest_config_path.display());
    println!("gdhealth_config={}", context.gdhealth_config_path.display());
    println!("manifest={}", context.manifest_path.display());
    println!("installation={}", context.installation_path.display());

    let mut missing: Vec<String> = Vec::new();

    match tooling::resolve_godot(context, cli_godot, Some(&config)) {
        Ok(path) => {
            println!("godot={}", path.display());
            print_version_or_warn("godot", &path, &["--version"]);
        }
        Err(_) => {
            println!(
                "godot=not found; install Godot and add to PATH, or set GODOT_EXE, or set project.godot_exe in gdsuite.toml"
            );
            missing.push("godot".to_string());
        }
    }

    let python = manifest
        .as_ref()
        .and_then(|m| m.python.clone())
        .or_else(|| tooling::find_python_in_venv(context));
    match python {
        Some(path) => {
            println!("python={}", path.display());
            print_version_or_warn("python", &path, &["--version"]);
        }
        None => {
            println!("python=not found; run `gdsuite setup` to install the toolchain");
            missing.push("python".to_string());
        }
    }

    let gdformat = manifest
        .as_ref()
        .and_then(|m| m.gdformat.clone())
        .or_else(|| tooling::find_binary(context, &["gdformat"]));
    match gdformat {
        Some(path) => {
            println!("gdformat={}", path.display());
            print_version_or_warn("gdformat", &path, &["--version"]);
        }
        None => {
            println!("gdformat=not found; run `gdsuite setup` to install the toolchain");
            missing.push("gdformat".to_string());
        }
    }

    let gdlint = manifest
        .as_ref()
        .and_then(|m| m.gdlint.clone())
        .or_else(|| tooling::find_binary(context, &["gdlint"]));
    match gdlint {
        Some(path) => {
            println!("gdlint={}", path.display());
            print_version_or_warn("gdlint", &path, &["--version"]);
        }
        None => {
            println!("gdlint=not found; run `gdsuite setup` to install the toolchain");
            missing.push("gdlint".to_string());
        }
    }

    let gdtest = manifest
        .as_ref()
        .and_then(|m| m.gdtest.clone())
        .or_else(|| tooling::find_binary(context, &["gdtest"]));
    match gdtest {
        Some(path) => {
            println!("gdtest={}", path.display());
            print_version_or_warn("gdtest", &path, &["--version"]);
        }
        None => {
            println!("gdtest=not found; run `gdsuite setup` to install the toolchain");
            missing.push("gdtest".to_string());
        }
    }

    let gdhealth = manifest
        .as_ref()
        .and_then(|m| m.gdhealth.clone())
        .or_else(|| tooling::find_binary(context, &["gdhealth"]));
    match gdhealth {
        Some(path) => {
            println!("gdhealth={}", path.display());
            print_version_or_warn("gdhealth", &path, &["--help"]);
        }
        None => {
            println!("gdhealth=not found; run `gdsuite setup` to install the toolchain");
            missing.push("gdhealth".to_string());
        }
    }

    let mut status = if missing.is_empty() {
        CompatibilityVerdict::Ok
    } else {
        CompatibilityVerdict::Error
    };

    if context.versions_path.exists() && !context.installation_path.exists() {
        println!(
            "warning=legacy state file {} is stale; run `gdsuite setup` to regenerate installation.json",
            context.versions_path.display()
        );
        if status == CompatibilityVerdict::Ok {
            status = CompatibilityVerdict::Warning;
        }
    }

    if let Some(manifest) = manifest.as_ref()
        && missing.is_empty()
    {
        let requested = tooling::install_plan(&config);
        if let Ok(installation) =
            tooling::probe_installation_state(context, &config, manifest, &requested)
        {
            let compatibility = tooling::compatibility_report(&installation);
            print_tool_compatibility(&compatibility.gdtoolkit);
            print_tool_compatibility(&compatibility.gdtest);
            print_tool_compatibility(&compatibility.gdhealth);
            status = match (status, tooling::report_status(&compatibility)) {
                (CompatibilityVerdict::Error, _) | (_, CompatibilityVerdict::Error) => {
                    CompatibilityVerdict::Error
                }
                (CompatibilityVerdict::Warning, _) | (_, CompatibilityVerdict::Warning) => {
                    CompatibilityVerdict::Warning
                }
                _ => CompatibilityVerdict::Ok,
            };
        }
    }

    for warning in tooling::product_config_warnings(context, &config)? {
        println!("warning={warning}");
        if status == CompatibilityVerdict::Ok {
            status = CompatibilityVerdict::Warning;
        }
    }

    match status {
        CompatibilityVerdict::Ok => {
            println!("status=ok");
            Ok(())
        }
        CompatibilityVerdict::Warning => {
            println!("status=warning");
            Ok(())
        }
        CompatibilityVerdict::Error => {
            println!("status=error");
            Err(command_error(
                EXIT_TOOLCHAIN_ERROR,
                if missing.is_empty() {
                    "toolchain compatibility check failed".to_string()
                } else {
                    format!(
                        "missing tools: {}; run `gdsuite setup` to install the local toolchain",
                        missing.join(", ")
                    )
                },
            ))
        }
    }
}

fn fmt(context: &ProjectContext, args: FmtArgs) -> Result<()> {
    let config = load_config_or_error(context)?;
    fmt_with_config(context, &config, args)
}

fn fmt_with_config(context: &ProjectContext, config: &GdsuiteConfig, args: FmtArgs) -> Result<()> {
    let toolchain = load_toolchain_or_error(context)?;
    let files = fixers::discover_typecheck_files(context, config)?;

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
    let report_mode = resolve_report_mode(&config, args.report.as_deref())?;

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

    let cmd_args = lint_args(context, &config, &report_mode)?;
    let output = process::run(&toolchain.gdlint, &cmd_args, &context.root)
        .map_err(|error| command_error(EXIT_TOOLCHAIN_ERROR, error.to_string()))?;
    print_lint_output(&output, &report_mode)?;
    if !output.status.success() {
        return Err(command_error(
            EXIT_VALIDATION_ERROR,
            format!("lint failed with exit code {:?}", output.status.code()),
        ));
    }
    Ok(())
}

fn test(context: &ProjectContext, args: ForwardReportArgs) -> Result<()> {
    let config = load_runtime_config(context)?;
    let report_mode = resolve_report_mode(&config, args.report.as_deref())?;
    if report_mode == "json" {
        let step = run_test_step_with_args(context, args.args)?;
        let report = SuiteReport::from_steps("test", context, vec![step]);
        print_and_store_report(context, &config, "test", &report)?;
        if report.status == "failed" {
            return Err(command_error(
                EXIT_VALIDATION_ERROR,
                "test failed".to_string(),
            ));
        }
        return Ok(());
    }

    let toolchain = load_toolchain_or_error(context)?;
    let mut args = if args.args.is_empty() {
        vec![config.commands.default_test_target]
    } else {
        args.args
    };
    if args.first().map(|value| value == "--").unwrap_or(false) {
        args.remove(0);
    }
    process::run_checked(&toolchain.gdtest, &args, &context.root)
        .map_err(|error| command_error(EXIT_TOOLCHAIN_ERROR, error.to_string()))?;
    Ok(())
}

fn health(context: &ProjectContext, args: ForwardReportArgs) -> Result<()> {
    let config = load_runtime_config(context)?;
    let report_mode = resolve_report_mode(&config, args.report.as_deref())?;
    if report_mode == "json" {
        let step = run_health_step(context, &config, None, args.args)?;
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
    let toolchain = load_toolchain_or_error(context)?;
    let args = if forwarded.is_empty() {
        build_default_health_args(context, &config, None)?
    } else {
        prepare_health_args(context, &config, forwarded)?
    };
    process::run_checked(&toolchain.gdhealth, &args, &context.root)
        .map_err(|error| command_error(EXIT_TOOLCHAIN_ERROR, error.to_string()))?;
    Ok(())
}

fn typecheck(context: &ProjectContext) -> Result<()> {
    let config = load_config_or_error(context)?;
    typecheck_with_config(context, &config)
}

fn typecheck_with_config(context: &ProjectContext, config: &GdsuiteConfig) -> Result<()> {
    let toolchain = load_toolchain_or_error(context)?;
    let files = fixers::discover_script_files(context, config)?;
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
    let config = load_runtime_config(context)?;
    let report_mode = resolve_report_mode(&config, args.report.as_deref())?;
    let steps = vec![
        run_fmt_step(context, &config, true)?,
        run_lint_step(context, &config)?,
        run_typecheck_step(context, &config)?,
    ];
    finish_pipeline(context, &config, "check", report_mode, steps)
}

fn report(context: &ProjectContext, args: ReportArgs) -> Result<()> {
    let config = load_config_or_error(context)?;
    if args.name == "ci" {
        let report = aggregate_ci_report(context, &config)?;
        print_and_store_report(context, &config, "ci", &report)?;
        return Ok(());
    }

    let path = report_path(context, &config, &args.name);
    if !path.exists() {
        return Err(command_error(
            EXIT_USAGE_ERROR,
            format!(
                "report artifact not found at {}; run `gdsuite check --report json`, `gdsuite test --report json`, `gdsuite health --report json`, or `gdsuite report ci` first",
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

fn aggregate_ci_report(context: &ProjectContext, config: &GdsuiteConfig) -> Result<SuiteReport> {
    let mut reports = Vec::new();
    for name in ["check", "test", "health"] {
        reports.push(load_saved_suite_report(context, config, name)?);
    }

    let expected_root = context.root.display().to_string();
    for report in &reports {
        if report.project_root != expected_root {
            return Err(command_error(
                EXIT_USAGE_ERROR,
                format!(
                    "report `{}` was generated for {}; expected {}",
                    report.command, report.project_root, expected_root
                ),
            ));
        }
    }

    let steps = reports
        .into_iter()
        .flat_map(|report| report.steps)
        .collect::<Vec<_>>();
    Ok(SuiteReport::from_steps("ci", context, steps))
}

fn load_saved_suite_report(
    context: &ProjectContext,
    config: &GdsuiteConfig,
    name: &str,
) -> Result<SuiteReport> {
    let path = report_path(context, config, name);
    if !path.exists() {
        return Err(command_error(
            EXIT_USAGE_ERROR,
            format!(
                "missing {} report artifact at {}; run `gdsuite check --report json`, `gdsuite test --report json`, and `gdsuite health --report json` before `gdsuite report ci`",
                name,
                path.display()
            ),
        ));
    }

    let body = fs::read_to_string(&path)
        .with_context(|| format!("failed to read report {}", path.display()))
        .map_err(|error| command_error(EXIT_USAGE_ERROR, error.to_string()))?;
    serde_json::from_str(&body).map_err(|error| {
        command_error(
            EXIT_USAGE_ERROR,
            format!("failed to parse report {}: {}", path.display(), error),
        )
    })
}

fn load_runtime_config(context: &ProjectContext) -> Result<GdsuiteConfig> {
    if context.config_path.exists() {
        tooling::load_config(context)
            .map_err(|error| command_error(EXIT_USAGE_ERROR, error.to_string()))
    } else {
        Ok(GdsuiteConfig::default())
    }
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

fn resolve_report_mode(config: &GdsuiteConfig, cli_report: Option<&str>) -> Result<String> {
    let report = cli_report.unwrap_or(&config.reports.default);
    match report {
        "text" | "json" => Ok(report.to_string()),
        other => Err(command_error(
            EXIT_USAGE_ERROR,
            format!(
                "unsupported report mode `{other}` in gdsuite config; expected `text` or `json`"
            ),
        )),
    }
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

fn print_version_or_warn(name: &str, path: &Path, args: &[&str]) {
    let args_owned: Vec<String> = args.iter().map(|v| v.to_string()).collect();
    match process::run_checked(path, &args_owned, PathBuf::from(".").as_path()) {
        Ok(output) => {
            let version = output.stdout.trim().lines().next().unwrap_or("unknown");
            println!("{name}_version={version}");
        }
        Err(_) => println!("{name}_version=unavailable"),
    }
}

fn print_tool_compatibility(tool: &tooling::ToolCompatibility) {
    println!("{}_requested={}", tool.name, tool.requested);
    println!(
        "{}_installed={}",
        tool.name,
        tool.resolved_version.as_deref().unwrap_or("unknown")
    );
    for (key, value) in &tool.extra {
        println!("{key}={value}");
    }
    let verdict = match tool.verdict {
        CompatibilityVerdict::Ok => "ok",
        CompatibilityVerdict::Warning => "warning",
        CompatibilityVerdict::Error => "error",
    };
    println!("{}_compat={}", tool.name, verdict);
    println!("{}={}", verdict, tool.reason);
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

fn run_fmt_step(
    context: &ProjectContext,
    config: &GdsuiteConfig,
    check: bool,
) -> Result<StepReport> {
    let started = Instant::now();
    match fmt_with_config(context, config, FmtArgs { check }) {
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

fn run_typecheck_step(context: &ProjectContext, config: &GdsuiteConfig) -> Result<StepReport> {
    let started = Instant::now();
    match typecheck_with_config(context, config) {
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

fn run_test_step_with_args(context: &ProjectContext, forwarded: Vec<String>) -> Result<StepReport> {
    let started = Instant::now();
    match test(
        context,
        ForwardReportArgs {
            report: Some("text".to_string()),
            args: forwarded,
        },
    ) {
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
    fail_on_override: Option<&str>,
    forwarded: Vec<String>,
) -> Result<StepReport> {
    let started = Instant::now();
    let toolchain = load_toolchain_or_error(context)?;
    let args = if forwarded.is_empty() {
        build_default_health_args(context, config, fail_on_override)?
    } else {
        prepare_health_args(context, config, forwarded)?
    };

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

fn build_default_health_args(
    context: &ProjectContext,
    config: &GdsuiteConfig,
    fail_on_override: Option<&str>,
) -> Result<Vec<String>> {
    let fail_on = fail_on_override.unwrap_or(&config.health.fail_on);
    prepare_health_args(
        context,
        config,
        vec![
            config.health.command.clone(),
            "--fail-on".to_string(),
            fail_on.to_string(),
        ],
    )
}

fn prepare_health_args(
    context: &ProjectContext,
    config: &GdsuiteConfig,
    forwarded: Vec<String>,
) -> Result<Vec<String>> {
    if forwarded.iter().any(|arg| arg == "--config") {
        return Ok(forwarded);
    }

    let overlay_path = write_health_overlay_config(context, config)?;
    let mut args = vec!["--config".to_string(), overlay_path.display().to_string()];
    args.extend(forwarded);
    Ok(args)
}

fn write_health_overlay_config(
    context: &ProjectContext,
    config: &GdsuiteConfig,
) -> Result<PathBuf> {
    let doc = tooling::merged_gdhealth_overlay_config(context, config)?;
    let overlay_path = context.state_dir.join("gdhealth.gdsuite.toml");
    fs::write(&overlay_path, toml::to_string(&doc)?)
        .with_context(|| format!("failed to write {}", overlay_path.display()))?;
    Ok(overlay_path)
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

#[derive(Debug, Clone, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ArtifactRef {
    name: String,
    path: String,
}
