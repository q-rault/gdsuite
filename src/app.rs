use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    time::Instant,
};

use anyhow::{Context, Result};
use clap::Parser;
use serde::{Deserialize, Serialize};

use crate::{
    cli::{
        Cli, Commands, DoctorArgs, FmtArgs, ForwardReportArgs, LintArgs, OutputArgs, OutputModeArg,
        ReportArgs, RunArgs, SetupArgs,
    },
    config::{CompatibilityVerdict, GdsuiteConfig, Manifest},
    fixers, migrate,
    output::{CommandScope, OutputContext, OutputMode},
    process,
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
    tooling::load_dotenv_if_present(&context)?;

    match cli.command {
        Commands::Init(args) => init(&context, cli.godot_exe.as_deref(), args),
        Commands::Migrate => migrate_project(&context, cli.godot_exe.as_deref()),
        Commands::Setup(args) => setup(&context, cli.godot_exe.as_deref(), args),
        Commands::Doctor(args) => doctor(&context, cli.godot_exe.as_deref(), args),
        Commands::Fmt(args) => fmt(&context, args),
        Commands::Lint(args) => lint(&context, args),
        Commands::Typecheck => typecheck(&context),
        Commands::Test(args) => test(&context, args),
        Commands::Health(args) => health(&context, args),
        Commands::Check(args) => check(&context, args),
        Commands::Report(args) => report(&context, args),
    }
}

fn init(context: &ProjectContext, cli_godot: Option<&Path>, args: OutputArgs) -> Result<()> {
    let output = OutputContext::new(resolve_output_mode(args.output), CommandScope::Init);
    setup_impl(
        context,
        cli_godot,
        SetupArgs {
            output: args,
            sync_product_configs: false,
        },
        true,
        &output,
    )?;
    finish_mutating_command(&output, "initialization complete");
    Ok(())
}

fn migrate_project(context: &ProjectContext, cli_godot: Option<&Path>) -> Result<()> {
    migrate::migrate_project(context, cli_godot)?;
    println!("migrate complete");
    Ok(())
}

fn setup(context: &ProjectContext, cli_godot: Option<&Path>, args: SetupArgs) -> Result<()> {
    let output = OutputContext::new(resolve_output_mode(args.output.output), CommandScope::Setup);
    setup_impl(context, cli_godot, args, false, &output)?;
    finish_mutating_command(&output, "setup complete");
    Ok(())
}

fn setup_impl(
    context: &ProjectContext,
    cli_godot: Option<&Path>,
    args: SetupArgs,
    import_product_configs: bool,
    output: &OutputContext,
) -> Result<()> {
    output.step("resolving project setup");
    context.ensure_dirs()?;
    output.step("resolving Godot executable");
    let godot_path = if context.config_path.exists() {
        let existing = tooling::load_config(context)?;
        tooling::resolve_godot(context, cli_godot, Some(&existing))?
    } else {
        tooling::resolve_godot(context, cli_godot, None)?
    };

    output.step("writing gdsuite config");
    GdsuiteConfig::write_if_missing(&context.config_path, Some(&godot_path))?;
    let mut config = tooling::load_config(context)?;
    if import_product_configs {
        output.step("importing delegated product configs");
        tooling::import_product_configs_into_canonical(context, &mut config)?;
        config.write(&context.config_path)?;
    }
    if args.sync_product_configs {
        output.warn(
            "`--sync-product-configs` is deprecated and has no effect; gdsuite no longer manages gdtest.toml or gdhealth.toml",
        );
    }
    output.step("installing local toolchain");
    let install = tooling::install_toolchain(context, &config, &godot_path)
        .map_err(|error| command_error(EXIT_SETUP_ERROR, error.to_string()))?;
    output.step("saving tool manifest");
    tooling::save_manifest(context, &install.manifest)?;
    output.step("installing gdtest addons");
    let addon_report = tooling::install_gdtest_addons(context, &config)
        .map_err(|error| command_error(EXIT_SETUP_ERROR, error.to_string()))?;
    for skipped in &addon_report.skipped_modified {
        output.warn(format!(
            "gdtest addon install skipped modified file {}",
            skipped.display()
        ));
    }
    output.step("refreshing Godot class cache");
    tooling::ensure_gdtest_class_cache(context, &config, &godot_path, &addon_report)
        .map_err(|error| command_error(EXIT_SETUP_ERROR, error.to_string()))?;
    output.step("verifying installed tools");
    let toolchain = tooling::resolve_toolchain(context, Some(&install.manifest))
        .map_err(|error| command_error(EXIT_TOOLCHAIN_ERROR, error.to_string()))?;
    tooling::verify_toolchain(context, &toolchain)
        .map_err(|error| command_error(EXIT_TOOLCHAIN_ERROR, error.to_string()))?;
    output.step("recording installation state");
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
            output.warn(&tool.reason);
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
        if output.mode() != OutputMode::Json {
            output.status("error");
        }
        return Err(command_error(EXIT_TOOLCHAIN_ERROR, errors.join("; ")));
    }
    Ok(())
}

fn doctor(context: &ProjectContext, cli_godot: Option<&Path>, args: DoctorArgs) -> Result<()> {
    let output = OutputContext::new(
        resolve_output_mode(args.output.output),
        CommandScope::Doctor,
    );
    let config = load_config_or_error(context)?;
    let manifest = if context.manifest_path.exists() {
        tooling::load_manifest(context).ok()
    } else {
        None
    };

    let mut missing: Vec<String> = Vec::new();
    let mut warnings = Vec::new();
    let mut tool_reports = BTreeMap::new();

    let godot = match tooling::resolve_godot(context, cli_godot, Some(&config)) {
        Ok(path) => {
            let version = probe_version(&path, &["--version"]);
            DoctorToolReport::ok(Some(path), version)
        }
        Err(_) => {
            missing.push("godot".to_string());
            DoctorToolReport::error(
                None,
                None,
                "not found; install Godot and add to PATH, or set GODOT_EXE, or set project.godot_exe in gdsuite.toml",
            )
        }
    };
    tool_reports.insert("godot".to_string(), godot);

    let python = manifest
        .as_ref()
        .and_then(|m| m.python.clone())
        .or_else(|| tooling::find_python_in_venv(context));
    tool_reports.insert(
        "python".to_string(),
        match python {
            Some(path) => {
                DoctorToolReport::ok(Some(path.clone()), probe_version(&path, &["--version"]))
            }
            None => {
                missing.push("python".to_string());
                DoctorToolReport::error(
                    None,
                    None,
                    "not found; run `gdsuite setup` to install the toolchain",
                )
            }
        },
    );

    let gdformat = manifest
        .as_ref()
        .and_then(|m| m.gdformat.clone())
        .or_else(|| tooling::find_binary(context, &["gdformat"]));
    tool_reports.insert(
        "gdformat".to_string(),
        match gdformat {
            Some(path) => {
                DoctorToolReport::ok(Some(path.clone()), probe_version(&path, &["--version"]))
            }
            None => {
                missing.push("gdformat".to_string());
                DoctorToolReport::error(
                    None,
                    None,
                    "not found; run `gdsuite setup` to install the toolchain",
                )
            }
        },
    );

    let gdlint = manifest
        .as_ref()
        .and_then(|m| m.gdlint.clone())
        .or_else(|| tooling::find_binary(context, &["gdlint"]));
    tool_reports.insert(
        "gdlint".to_string(),
        match gdlint {
            Some(path) => {
                DoctorToolReport::ok(Some(path.clone()), probe_version(&path, &["--version"]))
            }
            None => {
                missing.push("gdlint".to_string());
                DoctorToolReport::error(
                    None,
                    None,
                    "not found; run `gdsuite setup` to install the toolchain",
                )
            }
        },
    );

    let gdtest = manifest
        .as_ref()
        .and_then(|m| m.gdtest.clone())
        .or_else(|| tooling::find_binary(context, &["gdtest"]));
    tool_reports.insert(
        "gdtest".to_string(),
        match gdtest {
            Some(path) => {
                DoctorToolReport::ok(Some(path.clone()), probe_version(&path, &["--version"]))
            }
            None => {
                missing.push("gdtest".to_string());
                DoctorToolReport::error(
                    None,
                    None,
                    "not found; run `gdsuite setup` to install the toolchain",
                )
            }
        },
    );

    let gdhealth = manifest
        .as_ref()
        .and_then(|m| m.gdhealth.clone())
        .or_else(|| tooling::find_binary(context, &["gdhealth"]));
    tool_reports.insert(
        "gdhealth".to_string(),
        match gdhealth {
            Some(path) => {
                DoctorToolReport::ok(Some(path.clone()), probe_version(&path, &["--help"]))
            }
            None => {
                missing.push("gdhealth".to_string());
                DoctorToolReport::error(
                    None,
                    None,
                    "not found; run `gdsuite setup` to install the toolchain",
                )
            }
        },
    );

    let mut status = if missing.is_empty() {
        CompatibilityVerdict::Ok
    } else {
        CompatibilityVerdict::Error
    };

    if context.versions_path.exists() && !context.installation_path.exists() {
        warnings.push(format!(
            "legacy state file {} is stale; run `gdsuite setup` to regenerate installation.json",
            context.versions_path.display()
        ));
        if status == CompatibilityVerdict::Ok {
            status = CompatibilityVerdict::Warning;
        }
    }

    let mut compatibility_reports = BTreeMap::new();
    if let Some(manifest) = manifest.as_ref()
        && missing.is_empty()
    {
        let requested = tooling::install_plan(&config);
        if let Ok(installation) =
            tooling::probe_installation_state(context, &config, manifest, &requested)
        {
            let compatibility = tooling::compatibility_report(&installation);
            compatibility_reports.insert(
                "gdtoolkit".to_string(),
                DoctorCompatibilityReport::from_tool(&compatibility.gdtoolkit),
            );
            compatibility_reports.insert(
                "gdtest".to_string(),
                DoctorCompatibilityReport::from_tool(&compatibility.gdtest),
            );
            compatibility_reports.insert(
                "gdhealth".to_string(),
                DoctorCompatibilityReport::from_tool(&compatibility.gdhealth),
            );
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

    let report = DoctorReport {
        command: "doctor".to_string(),
        status: verdict_label(&status).to_string(),
        paths: DoctorPaths::from_context(context),
        tools: tool_reports,
        compatibility: compatibility_reports,
        warnings,
        summary: doctor_summary(&status),
    };

    match output.mode() {
        OutputMode::Pretty => render_doctor_pretty(&output, &report),
        OutputMode::Plain => render_doctor_plain(&output, &report),
        OutputMode::Json => println!("{}", serde_json::to_string_pretty(&report)?),
    }

    match status {
        CompatibilityVerdict::Ok | CompatibilityVerdict::Warning => Ok(()),
        CompatibilityVerdict::Error => Err(command_error(
            EXIT_TOOLCHAIN_ERROR,
            if missing.is_empty() {
                "toolchain compatibility check failed".to_string()
            } else {
                format!(
                    "missing tools: {}; run `gdsuite setup` to install the local toolchain",
                    missing.join(", ")
                )
            },
        )),
    }
}

fn resolve_output_mode(mode: OutputModeArg) -> OutputMode {
    match mode {
        OutputModeArg::Pretty => OutputMode::Pretty,
        OutputModeArg::Plain => OutputMode::Plain,
        OutputModeArg::Json => OutputMode::Json,
    }
}

fn finish_mutating_command(output: &OutputContext, summary: &str) {
    match output.mode() {
        OutputMode::Pretty => output.ok(summary),
        OutputMode::Plain => {
            output.status("ok");
            output.summary(summary);
        }
        OutputMode::Json => {}
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
    process::run_checked_streaming(&toolchain.gdtest, &args, &context.root)
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

    let step = run_health_step(context, &config, None, args.args)?;
    if step.status == "failed" {
        return Err(command_error(EXIT_VALIDATION_ERROR, step.summary));
    }
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
    let steps = run_check_steps(context, &config, args.fix)?;
    finish_check_pipeline(context, &config, report_mode, steps)
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

fn probe_version(path: &Path, args: &[&str]) -> Option<String> {
    let args_owned: Vec<String> = args.iter().map(|v| v.to_string()).collect();
    process::run_checked(path, &args_owned, PathBuf::from(".").as_path())
        .ok()
        .and_then(|output| output.stdout.trim().lines().next().map(ToString::to_string))
}

fn render_doctor_pretty(output: &OutputContext, report: &DoctorReport) {
    output.kv("project root", &report.paths.project_root);
    output.kv("config", &report.paths.config);
    output.kv("gdtest config", &report.paths.gdtest_config);
    output.kv("gdhealth config", &report.paths.gdhealth_config);
    output.kv("manifest", &report.paths.manifest);
    output.kv("installation", &report.paths.installation);
    output.blank();

    for name in [
        "godot", "python", "gdformat", "gdlint", "gdtest", "gdhealth",
    ] {
        if let Some(tool) = report.tools.get(name) {
            match tool.status.as_str() {
                "ok" => {
                    output.kv(name, tool.path.as_deref().unwrap_or("unknown"));
                    if let Some(version) = tool.version.as_deref() {
                        output.kv("version", version);
                    }
                }
                _ => output.warn(format!(
                    "{} {}",
                    name,
                    tool.summary.as_deref().unwrap_or("unavailable")
                )),
            }
        }
    }

    if !report.compatibility.is_empty() {
        output.blank();
        for name in ["gdtoolkit", "gdtest", "gdhealth"] {
            if let Some(tool) = report.compatibility.get(name) {
                let installed = tool.installed.as_deref().unwrap_or("unknown");
                let message = format!(
                    "{name:<13} requested {}, installed {}",
                    tool.requested, installed
                );
                match tool.status.as_str() {
                    "ok" => output.ok(message),
                    "warning" => output.warn(message),
                    _ => output.error(message),
                }
            }
        }
    }

    if !report.warnings.is_empty() {
        output.blank();
        for warning in &report.warnings {
            output.warn(warning);
        }
    }

    output.blank();
    output.status(&report.status);
}

fn render_doctor_plain(output: &OutputContext, report: &DoctorReport) {
    output.kv("project_root", &report.paths.project_root);
    output.kv("config", &report.paths.config);
    output.kv("gdtest_config", &report.paths.gdtest_config);
    output.kv("gdhealth_config", &report.paths.gdhealth_config);
    output.kv("manifest", &report.paths.manifest);
    output.kv("installation", &report.paths.installation);

    for (name, tool) in &report.tools {
        if let Some(path) = tool.path.as_deref() {
            output.kv(name, path);
        } else if let Some(summary) = tool.summary.as_deref() {
            output.kv(name, summary);
        }
        output.kv(
            &format!("{name}_version"),
            tool.version.as_deref().unwrap_or("unavailable"),
        );
    }

    for (name, tool) in &report.compatibility {
        output.kv(&format!("{name}_requested"), &tool.requested);
        output.kv(
            &format!("{name}_installed"),
            tool.installed.as_deref().unwrap_or("unknown"),
        );
        output.kv(&format!("{name}_compat"), &tool.status);
        output.kv(&format!("{name}_reason"), &tool.reason);
        for (key, value) in &tool.extra {
            output.kv(key, value);
        }
    }

    for warning in &report.warnings {
        output.warn(warning);
    }

    output.status(&report.status);
    output.summary(&report.summary);
}

fn verdict_label(verdict: &CompatibilityVerdict) -> &'static str {
    match verdict {
        CompatibilityVerdict::Ok => "ok",
        CompatibilityVerdict::Warning => "warning",
        CompatibilityVerdict::Error => "error",
    }
}

fn doctor_summary(status: &CompatibilityVerdict) -> String {
    match status {
        CompatibilityVerdict::Ok => "toolchain healthy".to_string(),
        CompatibilityVerdict::Warning => "toolchain healthy with warnings".to_string(),
        CompatibilityVerdict::Error => "toolchain check failed".to_string(),
    }
}

fn command_error(code: u8, message: String) -> anyhow::Error {
    CommandError { code, message }.into()
}

#[derive(Debug, Serialize)]
struct DoctorReport {
    command: String,
    status: String,
    paths: DoctorPaths,
    tools: BTreeMap<String, DoctorToolReport>,
    compatibility: BTreeMap<String, DoctorCompatibilityReport>,
    warnings: Vec<String>,
    summary: String,
}

#[derive(Debug, Serialize)]
struct DoctorPaths {
    project_root: String,
    config: String,
    gdtest_config: String,
    gdhealth_config: String,
    manifest: String,
    installation: String,
}

impl DoctorPaths {
    fn from_context(context: &ProjectContext) -> Self {
        Self {
            project_root: context.root.display().to_string(),
            config: context.config_path.display().to_string(),
            gdtest_config: context.gdtest_config_path.display().to_string(),
            gdhealth_config: context.gdhealth_config_path.display().to_string(),
            manifest: context.manifest_path.display().to_string(),
            installation: context.installation_path.display().to_string(),
        }
    }
}

#[derive(Debug, Serialize)]
struct DoctorToolReport {
    path: Option<String>,
    version: Option<String>,
    status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    summary: Option<String>,
}

impl DoctorToolReport {
    fn ok(path: Option<PathBuf>, version: Option<String>) -> Self {
        Self {
            path: path.map(|value| value.display().to_string()),
            version,
            status: "ok".to_string(),
            summary: None,
        }
    }

    fn error(path: Option<PathBuf>, version: Option<String>, summary: &str) -> Self {
        Self {
            path: path.map(|value| value.display().to_string()),
            version,
            status: "error".to_string(),
            summary: Some(summary.to_string()),
        }
    }
}

#[derive(Debug, Serialize)]
struct DoctorCompatibilityReport {
    requested: String,
    installed: Option<String>,
    status: String,
    reason: String,
    #[serde(flatten)]
    extra: BTreeMap<String, String>,
}

impl DoctorCompatibilityReport {
    fn from_tool(tool: &tooling::ToolCompatibility) -> Self {
        Self {
            requested: tool.requested.clone(),
            installed: tool.resolved_version.clone(),
            status: verdict_label(&tool.verdict).to_string(),
            reason: tool.reason.clone(),
            extra: tool.extra.iter().cloned().collect(),
        }
    }
}

fn display_error(error: &anyhow::Error) -> String {
    error
        .downcast_ref::<CommandError>()
        .map(|value| value.message.clone())
        .unwrap_or_else(|| error.to_string())
}

#[derive(Debug, Clone)]
struct CheckStepResult {
    report: StepReport,
    success_message: String,
    failure_message: String,
    diagnostics: Vec<String>,
    failure_summary: String,
}

impl CheckStepResult {
    fn passed(report: StepReport, label: &str, tool: &str, duration_ms: u128) -> Self {
        Self {
            report,
            success_message: format!("{label} ({tool}, {duration_ms}ms)"),
            failure_message: String::new(),
            diagnostics: Vec::new(),
            failure_summary: String::new(),
        }
    }

    fn failed(
        report: StepReport,
        label: &str,
        tool: &str,
        duration_ms: u128,
        diagnostics: Vec<String>,
        failure_summary: String,
    ) -> Self {
        Self {
            report,
            success_message: String::new(),
            failure_message: format!("{label} ({tool}, {duration_ms}ms)"),
            diagnostics,
            failure_summary,
        }
    }
}

fn run_check_steps(
    context: &ProjectContext,
    config: &GdsuiteConfig,
    fix: bool,
) -> Result<Vec<CheckStepResult>> {
    let mut steps = Vec::new();

    let fmt = run_fmt_step(context, config, !fix)?;
    let failed = fmt.report.status == "failed";
    steps.push(fmt);
    if failed {
        return Ok(steps);
    }

    let lint = run_lint_step(context, config, fix)?;
    let failed = lint.report.status == "failed";
    steps.push(lint);
    if failed {
        return Ok(steps);
    }

    let typecheck = run_typecheck_step(context, config)?;
    steps.push(typecheck);

    Ok(steps)
}

fn run_fmt_step(
    context: &ProjectContext,
    config: &GdsuiteConfig,
    check: bool,
) -> Result<CheckStepResult> {
    let started = Instant::now();
    let toolchain = load_toolchain_or_error(context)?;
    let files = fixers::discover_typecheck_files(context, config)?;
    let mut cmd_args = Vec::new();
    if check {
        cmd_args.push("--check".to_string());
    }
    cmd_args.extend(files.iter().map(|path| path.display().to_string()));
    let output = process::run(&toolchain.gdformat, &cmd_args, &context.root)
        .map_err(|error| command_error(EXIT_TOOLCHAIN_ERROR, error.to_string()))?;
    let duration_ms = started.elapsed().as_millis();

    if output.status.success() {
        return Ok(CheckStepResult::passed(
            StepReport::ok("fmt", duration_ms, "format check passed"),
            "Formatting check passed",
            "fmt",
            duration_ms,
        ));
    }

    let diagnostics = collect_nonempty_lines(&output.stdout, &output.stderr);
    Ok(CheckStepResult::failed(
        StepReport::failed("fmt", duration_ms, "formatting issues found"),
        "Formatting issues found",
        "fmt",
        duration_ms,
        diagnostics.clone(),
        format!(
            "{}. Run `gdsuite check --fix` to apply fixes.",
            format_formatting_summary(&diagnostics)
        ),
    ))
}

fn run_lint_step(
    context: &ProjectContext,
    config: &GdsuiteConfig,
    fix: bool,
) -> Result<CheckStepResult> {
    let started = Instant::now();
    let toolchain = load_toolchain_or_error(context)?;
    if fix {
        fixers::apply_safe_fixes(context, config, &toolchain.gdformat)
            .map_err(|error| command_error(EXIT_TOOLCHAIN_ERROR, error.to_string()))?;
    }
    let cmd_args = lint_args(context, config, "json")?;
    let output = process::run(&toolchain.gdlint, &cmd_args, &context.root)
        .map_err(|error| command_error(EXIT_TOOLCHAIN_ERROR, error.to_string()))?;
    let lint_report = JsonLintReport::from_output(&output);
    let findings = lint_report
        .findings
        .iter()
        .map(serde_json::to_value)
        .collect::<serde_json::Result<Vec<_>>>()?;

    let duration_ms = started.elapsed().as_millis();
    let report = StepReport {
        name: "lint".to_string(),
        status: if output.status.success() {
            "ok".to_string()
        } else {
            "failed".to_string()
        },
        duration_ms,
        summary: lint_report
            .summary
            .clone()
            .unwrap_or_else(|| "lint completed".to_string()),
        findings,
        artifacts: Vec::new(),
    };

    if output.status.success() {
        return Ok(CheckStepResult::passed(
            report,
            "Lint check passed",
            "lint",
            duration_ms,
        ));
    }

    Ok(CheckStepResult::failed(
        report,
        "Lint check failed",
        "lint",
        duration_ms,
        lint_diagnostics(&lint_report),
        lint_failure_summary(&lint_report),
    ))
}

fn run_typecheck_step(context: &ProjectContext, config: &GdsuiteConfig) -> Result<CheckStepResult> {
    let started = Instant::now();
    let toolchain = load_toolchain_or_error(context)?;
    let files = fixers::discover_script_files(context, config)?;
    let checker_path = context.state_dir.join(TYPECHECK_SCRIPT_NAME);
    write_typecheck_script(&checker_path, &files)?;
    let args = vec![
        "--headless".to_string(),
        "--script".to_string(),
        format!("res://.gdsuite/state/{TYPECHECK_SCRIPT_NAME}"),
    ];
    let output = process::run(&toolchain.godot_exe, &args, &context.root)
        .map_err(|error| command_error(EXIT_TOOLCHAIN_ERROR, error.to_string()))?;
    let duration_ms = started.elapsed().as_millis();

    if output.status.success() {
        return Ok(CheckStepResult::passed(
            StepReport::ok("typecheck", duration_ms, "typecheck passed"),
            "GDScript type check passed",
            "typecheck",
            duration_ms,
        ));
    }

    let diagnostics = collect_nonempty_lines(&output.stdout, &output.stderr)
        .into_iter()
        .filter(|line| line != "typecheck: ok")
        .collect::<Vec<_>>();

    Ok(CheckStepResult::failed(
        StepReport::failed("typecheck", duration_ms, "typecheck failed"),
        "GDScript type check failed",
        "typecheck",
        duration_ms,
        diagnostics.clone(),
        typecheck_failure_summary(&diagnostics),
    ))
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
    let summary_path = bundle_dir.join("summary.txt");
    let mut artifacts = Vec::new();

    for (name, path) in [
        ("summary", summary_path.clone()),
        ("health_json", bundle_dir.join("health.json")),
    ] {
        if path.exists() {
            artifacts.push(ArtifactRef {
                name: name.to_string(),
                path: path.display().to_string(),
            });
        }
    }

    let bundle_summary = read_trimmed_file_if_exists(&summary_path);
    let summary = if output.status.success() {
        if artifacts.is_empty() {
            "health passed".to_string()
        } else {
            format!("health passed; artifacts in {}", bundle_dir.display())
        }
    } else if let Some(bundle_summary) = bundle_summary {
        bundle_summary
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

fn read_trimmed_file_if_exists(path: &Path) -> Option<String> {
    let body = fs::read_to_string(path).ok()?;
    let trimmed = body.trim();
    if trimmed.is_empty() {
        return None;
    }
    Some(trimmed.to_string())
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
    let doc = tooling::merged_gdhealth_overlay_config(config)?;
    let overlay_path = context.state_dir.join("gdhealth.gdsuite.toml");
    fs::write(&overlay_path, toml::to_string(&doc)?)
        .with_context(|| format!("failed to write {}", overlay_path.display()))?;
    Ok(overlay_path)
}

fn finish_check_pipeline(
    context: &ProjectContext,
    config: &GdsuiteConfig,
    report_mode: String,
    steps: Vec<CheckStepResult>,
) -> Result<()> {
    let report = SuiteReport::from_steps(
        "check",
        context,
        steps.iter().map(|step| step.report.clone()).collect(),
    );
    if report_mode == "json" {
        print_and_store_report(context, config, "check", &report)?;
    } else {
        render_check_text_results(&steps);
    }

    if report.status == "failed" {
        return Err(command_error(
            EXIT_VALIDATION_ERROR,
            "check failed".to_string(),
        ));
    }
    Ok(())
}

fn render_check_text_results(steps: &[CheckStepResult]) {
    for step in steps {
        if step.report.status == "ok" {
            println!("pass: {}", step.success_message);
            continue;
        }

        println!("error: {}", step.failure_message);
        for line in &step.diagnostics {
            println!("{line}");
        }
        println!();
        println!("{}", step.failure_summary);
        break;
    }
}

fn collect_nonempty_lines(stdout: &str, stderr: &str) -> Vec<String> {
    stdout
        .lines()
        .chain(stderr.lines())
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(ToString::to_string)
        .collect()
}

fn format_formatting_summary(diagnostics: &[String]) -> String {
    let count = diagnostics.len();
    if count == 1 {
        "Found formatting issues in 1 file".to_string()
    } else {
        format!("Found formatting issues in {count} files")
    }
}

fn lint_diagnostics(report: &JsonLintReport) -> Vec<String> {
    let mut diagnostics = report
        .findings
        .iter()
        .map(|finding| format!("{}:{} {}", finding.file, finding.line, finding.message))
        .collect::<Vec<_>>();
    diagnostics.extend(report.raw_stdout.iter().cloned());
    diagnostics.extend(report.stderr.iter().cloned());
    diagnostics
}

fn lint_failure_summary(report: &JsonLintReport) -> String {
    let count = report.findings.len();
    let noun = if count == 1 { "error" } else { "errors" };
    format!("Found {count} lint {noun}. Fix the reported issue and rerun `gdsuite check`.")
}

fn typecheck_failure_summary(diagnostics: &[String]) -> String {
    let count = diagnostics.len();
    let noun = if count == 1 { "error" } else { "errors" };
    format!("Found {count} type {noun}. Fix the reported issue and rerun `gdsuite check`.")
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
