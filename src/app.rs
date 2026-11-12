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
    bundled,
    cli::{
        Cli, Commands, DoctorArgs, ForwardArgs, ForwardReportArgs, LintArgs, OutputArgs,
        OutputModeArg, ReportArgs, RunArgs, SetupArgs,
    },
    config::{CheckStep, CompatibilityVerdict, GdsuiteConfig, Manifest},
    migrate,
    output::{CommandScope, OutputContext, OutputMode},
    process,
    tooling::{self, ProjectContext},
    update::{self, PinChange},
};

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
    let context = ProjectContext::new(process::normalize_windows_path(&root));
    tooling::load_dotenv_if_present(&context)?;

    match cli.command {
        Commands::Init(args) => init(&context, cli.godot_exe.as_deref(), args),
        Commands::Migrate => migrate_project(&context, cli.godot_exe.as_deref()),
        Commands::Setup(args) => setup(&context, cli.godot_exe.as_deref(), args),
        Commands::Update(args) => update_tools(&context, cli.godot_exe.as_deref(), args),
        Commands::Doctor(args) => doctor(&context, cli.godot_exe.as_deref(), args),
        Commands::Fmt(args) => fmt(&context, args),
        Commands::Lint(args) => lint(&context, args),
        Commands::Typecheck(args) => typecheck(&context, args),
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
        &output,
    )?;
    finish_mutating_command(&output, "initialization complete");
    Ok(())
}

fn migrate_project(context: &ProjectContext, cli_godot: Option<&Path>) -> Result<()> {
    migrate::migrate_project(context, cli_godot)
}

fn setup(context: &ProjectContext, cli_godot: Option<&Path>, args: SetupArgs) -> Result<()> {
    let output = OutputContext::new(resolve_output_mode(args.output.output), CommandScope::Setup);
    setup_impl(context, cli_godot, args, &output)?;
    finish_mutating_command(&output, "setup complete");
    Ok(())
}

fn update_tools(
    context: &ProjectContext,
    cli_godot: Option<&Path>,
    args: OutputArgs,
) -> Result<()> {
    load_config_or_error(context)?;
    let output = OutputContext::new(resolve_output_mode(args.output), CommandScope::Update);
    let text = fs::read_to_string(&context.config_path)
        .with_context(|| format!("failed to read {}", context.config_path.display()))?;
    let release = tooling::release_manifest();
    let (updated, changes) = update::refresh_tool_pins(&text, release);
    for change in &changes {
        match change {
            PinChange::Unpinned { tool, from } => {
                let tested = release
                    .tool(tool)
                    .map(|entry| format!("v{}", entry.version()))
                    .unwrap_or_default();
                if *from == tested {
                    output.step(format!(
                        "{tool}: removed the pin to {from}, the version gdsuite installs; it now follows gdsuite"
                    ));
                } else {
                    output.step(format!(
                        "{tool}: removed the pin to {from}, it now follows gdsuite ({from} -> {tested})"
                    ));
                }
            }
            PinChange::CrateBumped { tool, from, to } => {
                output.step(format!("{tool}: crates.io version {from} -> {to}"));
            }
            PinChange::Kept { tool, source } => {
                output.info(format!(
                    "{tool}: kept {source}, which gdsuite does not manage"
                ));
            }
        }
    }
    if changes.is_empty() {
        output.info("every tool follows gdsuite already");
    }
    if updated != text {
        fs::write(&context.config_path, updated)
            .with_context(|| format!("failed to write {}", context.config_path.display()))?;
    }
    setup_impl(
        context,
        cli_godot,
        SetupArgs {
            output: args,
            sync_product_configs: false,
        },
        &output,
    )?;
    finish_mutating_command(&output, "update complete");
    Ok(())
}

fn setup_impl(
    context: &ProjectContext,
    cli_godot: Option<&Path>,
    args: SetupArgs,
    output: &OutputContext,
) -> Result<()> {
    output.step("resolving project setup");
    context.ensure_dirs()?;
    output.step("resolving Godot executable");
    let existing = if context.config_path.exists() {
        Some(tooling::load_config(context)?)
    } else {
        None
    };
    let godot_path = tooling::resolve_godot(context, cli_godot, existing.as_ref())?;

    // This file is meant to be committed, so a machine's Godot path stays out of it.
    // The manifest in .gdsuite/ remembers it instead.
    output.step("writing gdsuite config");
    GdsuiteConfig::write_if_missing(&context.config_path, None)?;
    tooling::ensure_gitignored(context)?;
    let config = tooling::load_config(context)?;
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
    if let Some(upgrade) = &addon_report.upgrade {
        let backup = if upgrade.replaced.is_empty() {
            String::new()
        } else {
            format!(
                "; the {} replaced file(s) are backed up in {}",
                upgrade.replaced.len(),
                upgrade.backup_dir.display()
            )
        };
        output.warn(format!(
            "upgraded the gdtest addons from harness {} to {}{backup}",
            upgrade.from_version, upgrade.to_version
        ));
    }
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
    if let Some(conversion) = tooling::convert_tool_configs(context, &toolchain.gdtidy)
        .map_err(|error| command_error(EXIT_SETUP_ERROR, error.to_string()))?
    {
        output.step(format!(
            "converted {} into {}",
            conversion.files.join(" and "),
            conversion.target
        ));
        for note in conversion.notes {
            output.info(format!("gdtidy migrate: {note}"));
        }
    }
    output.step("recording installation state");
    let installation =
        tooling::probe_installation_state(context, &config, &install.manifest, &install.requested)
            .map_err(|error| command_error(EXIT_TOOLCHAIN_ERROR, error.to_string()))?;
    tooling::save_installation_state(context, &installation)
        .map_err(|error| command_error(EXIT_INTERNAL_ERROR, error.to_string()))?;
    let compatibility = tooling::compatibility_report(&installation);
    for tool in [
        &compatibility.gdtest,
        &compatibility.gdhealth,
        &compatibility.gdtypecheck,
        &compatibility.gdtidy,
    ] {
        if tool.verdict == CompatibilityVerdict::Warning {
            output.warn(&tool.reason);
        }
    }
    if tooling::report_status(&compatibility) == CompatibilityVerdict::Error {
        let errors = [
            &compatibility.gdtest,
            &compatibility.gdhealth,
            &compatibility.gdtypecheck,
            &compatibility.gdtidy,
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

    let gdtidy = manifest
        .as_ref()
        .and_then(|m| m.gdtidy.clone())
        .or_else(|| tooling::find_binary(context, &["gdtidy"]));
    tool_reports.insert(
        "gdtidy".to_string(),
        match gdtidy {
            Some(path) => {
                DoctorToolReport::ok(Some(path.clone()), probe_version(&path, &["--version"]))
            }
            None => {
                missing.push("gdtidy".to_string());
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
                DoctorToolReport::ok(Some(path.clone()), probe_version(&path, &["--version"]))
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

    let gdtypecheck = manifest
        .as_ref()
        .and_then(|m| m.gdtypecheck.clone())
        .or_else(|| tooling::find_binary(context, &["gdtypecheck"]));
    tool_reports.insert(
        "gdtypecheck".to_string(),
        match gdtypecheck {
            Some(path) => {
                DoctorToolReport::ok(Some(path.clone()), probe_version(&path, &["--version"]))
            }
            None => {
                missing.push("gdtypecheck".to_string());
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
                "gdtest".to_string(),
                DoctorCompatibilityReport::from_tool(&compatibility.gdtest),
            );
            compatibility_reports.insert(
                "gdhealth".to_string(),
                DoctorCompatibilityReport::from_tool(&compatibility.gdhealth),
            );
            compatibility_reports.insert(
                "gdtypecheck".to_string(),
                DoctorCompatibilityReport::from_tool(&compatibility.gdtypecheck),
            );
            compatibility_reports.insert(
                "gdtidy".to_string(),
                DoctorCompatibilityReport::from_tool(&compatibility.gdtidy),
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

    for drift in tooling::installed_drift(context, &config) {
        warnings.push(drift.message());
        if status == CompatibilityVerdict::Ok {
            status = CompatibilityVerdict::Warning;
        }
    }
    // A pin is the project's choice, so it is mentioned without changing the status.
    for (tool, pinned, tested) in tooling::outdated_pins(&config) {
        warnings.push(format!(
            "{tool} is pinned to {pinned} in gdsuite.toml, and this gdsuite was tested with {tested}; run `gdsuite update` to follow gdsuite"
        ));
    }

    let report = DoctorReport {
        command: "doctor".to_string(),
        status: verdict_label(&status).to_string(),
        paths: DoctorPaths::from_context(context),
        settings: tooling::settings_sources(context)
            .into_iter()
            .map(|(name, source)| (name.to_string(), source))
            .collect(),
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

fn fmt(context: &ProjectContext, args: ForwardArgs) -> Result<()> {
    let toolchain = load_toolchain_or_error(context)?;
    warn_unconverted_configs(context);
    let mut cmd_args = vec!["fmt".to_string()];
    cmd_args.extend(default_or_dot_or_append_target_for_flags(args.args));
    process::run_checked_streaming(&toolchain.gdtidy, &cmd_args, &context.root)
        .map_err(|error| command_error(EXIT_TOOLCHAIN_ERROR, error.to_string()))?;
    Ok(())
}

fn lint(context: &ProjectContext, args: LintArgs) -> Result<()> {
    let config = load_runtime_config(context)?;
    let toolchain = load_toolchain_or_error(context)?;
    let report_mode = resolve_report_mode(&config, args.report.as_deref())?;
    warn_unconverted_configs(context);
    let output = process::run(&toolchain.gdtidy, &lint_args(args.args), &context.root)
        .map_err(|error| command_error(EXIT_TOOLCHAIN_ERROR, error.to_string()))?;
    let report = JsonLintReport::from_output(&output);
    print_lint_report(&report, &report_mode)?;
    if !report.success {
        // The summary is already printed with the report.
        return Err(command_error(
            EXIT_VALIDATION_ERROR,
            "lint failed".to_string(),
        ));
    }
    Ok(())
}

/// gdtidy reads neither gdlintrc nor gdformatrc; until they are converted,
/// their settings are lost.
fn warn_unconverted_configs(context: &ProjectContext) {
    for path in tooling::unconverted_configs(context) {
        eprintln!(
            "warning: {} is not read by gdtidy; run `gdsuite setup` or `gdsuite migrate` to convert it into gdsuite.toml",
            path.display()
        );
    }
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
    let args = default_test_args(&config, args.args);
    process::run_checked_streaming(&toolchain.gdtest, &args, &context.root)
        .map_err(|error| command_error(EXIT_TOOLCHAIN_ERROR, error.to_string()))?;
    Ok(())
}

fn health(context: &ProjectContext, args: ForwardReportArgs) -> Result<()> {
    let config = load_runtime_config(context)?;
    let report_mode = resolve_report_mode(&config, args.report.as_deref())?;
    if report_mode == "json" {
        let step = run_health_step(context, args.args)?;
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

    let step = run_health_step(context, args.args)?;
    if step.status == "failed" {
        return Err(command_error(EXIT_VALIDATION_ERROR, step.summary));
    }
    // gdhealth writes its bundle quietly, so say where it went.
    println!("{}", step.summary);
    Ok(())
}

fn typecheck(context: &ProjectContext, args: ForwardArgs) -> Result<()> {
    let toolchain = load_toolchain_or_error(context)?;
    let cmd_args = default_or_dot(args.args);
    process::run_checked_streaming(&toolchain.gdtypecheck, &cmd_args, &context.root)
        .map_err(|error| command_error(EXIT_VALIDATION_ERROR, error.to_string()))?;
    Ok(())
}

fn check(context: &ProjectContext, args: RunArgs) -> Result<()> {
    let config = load_runtime_config(context)?;
    let report_mode = resolve_report_mode(&config, args.report.as_deref())?;
    let steps = run_check_steps(context, &config)?;
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

/// Combines the saved check, test and health reports into one, with one entry
/// per step. `check.json` is required; `test.json` and `health.json` are
/// required unless the check already ran that step. A step that ran in both
/// keeps the result of its own command, which ran with that command's options.
fn aggregate_ci_report(context: &ProjectContext, config: &GdsuiteConfig) -> Result<SuiteReport> {
    let check = load_saved_suite_report(context, config, "check")?;
    let mut reports = vec![check];
    for name in ["test", "health"] {
        let in_check = reports[0].steps.iter().any(|step| step.name == name);
        if in_check && !report_path(context, config, name).exists() {
            continue;
        }
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

    let mut steps: Vec<StepReport> = Vec::new();
    for step in reports.into_iter().flat_map(|report| report.steps) {
        match steps.iter_mut().find(|existing| existing.name == step.name) {
            Some(existing) => *existing = step,
            None => steps.push(step),
        }
    }
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
    let toolchain = resolve_toolchain_or_error(context, Some(&manifest), EXIT_TOOLCHAIN_ERROR)?;
    process::set_godot_exe(&toolchain.godot_exe);
    if process::bundled_tool_name(&toolchain.gdtest).is_some() {
        // Keep the gdtest the editor panel runs the one built into this
        // gdsuite. Best effort: a running panel can hold the file.
        let _ = bundled::refresh_gdtest_launcher(&tooling::gdtest_launcher_path(context));
    }
    // After updating gdsuite, or editing [tools], the installed tools lag
    // behind until `gdsuite setup` runs: say so instead of running them silently.
    if let Ok(config) = load_runtime_config(context) {
        for drift in tooling::installed_drift(context, &config) {
            eprintln!("warning: {}", drift.message());
        }
    }
    Ok(toolchain)
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

/// `gdtidy lint` with its JSON report, which gdsuite renders itself. Arguments
/// after `--` (paths, `--fix`) are passed on; without paths gdtidy lints `.`.
fn lint_args(args: Vec<String>) -> Vec<String> {
    let mut cmd_args = vec![
        "lint".to_string(),
        "--format".to_string(),
        "json".to_string(),
    ];
    cmd_args.extend(strip_passthrough_marker(args));
    cmd_args
}

fn strip_passthrough_marker(mut args: Vec<String>) -> Vec<String> {
    if args.first().is_some_and(|value| value == "--") {
        args.remove(0);
    }
    args
}

fn default_or_dot(args: Vec<String>) -> Vec<String> {
    let mut args = strip_passthrough_marker(args);
    if args.is_empty() {
        args.push(".".to_string());
    }
    args
}

fn default_or_dot_or_append_target_for_flags(args: Vec<String>) -> Vec<String> {
    let mut args = strip_passthrough_marker(args);
    if args.is_empty() || args.iter().all(|value| value.starts_with('-')) {
        args.push(".".to_string());
    }
    args
}

fn default_test_args(config: &GdsuiteConfig, args: Vec<String>) -> Vec<String> {
    let mut args = strip_passthrough_marker(args);
    if args.is_empty() {
        args.push(config.commands.default_test_target.clone());
    }
    args
}

fn default_health_args(config: &GdsuiteConfig, args: Vec<String>) -> Vec<String> {
    let mut args = strip_passthrough_marker(args);
    if args.is_empty() {
        args.push(config.commands.default_health_subcommand.clone());
    }
    args
}

fn print_lint_report(report: &JsonLintReport, mode: &str) -> Result<()> {
    if mode == "json" {
        println!("{}", serde_json::to_string_pretty(report)?);
        return Ok(());
    }
    for finding in &report.findings {
        println!(
            "{}:{}:{}: {}[{}]: {}",
            finding.file,
            finding.line,
            finding.column,
            finding.severity,
            finding.rule.as_deref().unwrap_or("lint"),
            finding.message
        );
    }
    for skipped in &report.skipped {
        eprintln!(
            "{}:{}:{}: skipped: {}",
            skipped.file, skipped.line, skipped.column, skipped.message
        );
    }
    for line in &report.stderr {
        eprintln!("{line}");
    }
    if let Some(summary) = &report.summary {
        eprintln!("{summary}");
    }
    Ok(())
}

/// What `gdtidy lint --format json` reported, as gdsuite shows it.
#[derive(Debug, Serialize)]
struct JsonLintReport {
    status: &'static str,
    success: bool,
    exit_code: Option<i32>,
    summary: Option<String>,
    findings: Vec<JsonLintFinding>,
    /// Files gdtidy could not check. Syntax errors are Godot's to report, so
    /// they are left to typecheck and do not fail lint.
    skipped: Vec<JsonLintSkipped>,
    stderr: Vec<String>,
}

#[derive(Debug, Serialize)]
struct JsonLintFinding {
    file: String,
    line: usize,
    column: usize,
    severity: String,
    message: String,
    rule: Option<String>,
    /// `gdtidy lint --fix` can fix it.
    fixable: bool,
}

#[derive(Debug, Serialize)]
struct JsonLintSkipped {
    file: String,
    line: usize,
    column: usize,
    reason: String,
    message: String,
}

/// gdtidy's JSON report (version 1); only the fields gdsuite uses.
#[derive(Debug, Deserialize)]
struct GdtidyReport {
    files: usize,
    #[serde(default)]
    fixed: usize,
    diagnostics: Vec<GdtidyDiagnostic>,
    skipped: Vec<GdtidySkipped>,
}

#[derive(Debug, Deserialize)]
struct GdtidyDiagnostic {
    path: String,
    line: usize,
    column: usize,
    severity: String,
    rule: String,
    message: String,
    #[serde(default)]
    fixable: bool,
}

#[derive(Debug, Deserialize)]
struct GdtidySkipped {
    path: String,
    line: usize,
    column: usize,
    reason: String,
    message: String,
}

impl JsonLintReport {
    fn from_output(output: &process::ProcessOutput) -> Self {
        let stderr = output
            .stderr
            .lines()
            .filter(|line| !line.trim().is_empty())
            .map(ToString::to_string)
            .collect::<Vec<_>>();
        let Ok(report) = serde_json::from_str::<GdtidyReport>(&output.stdout) else {
            // No report: gdtidy could not run, such as with an invalid gdtidy.toml.
            return Self {
                status: "failed",
                success: false,
                exit_code: output.status.code(),
                summary: stderr.last().cloned().or_else(|| {
                    Some(format!(
                        "gdtidy {}",
                        process::describe_exit(output.status.code())
                    ))
                }),
                findings: Vec::new(),
                skipped: Vec::new(),
                stderr,
            };
        };

        let findings = report
            .diagnostics
            .into_iter()
            .map(|d| JsonLintFinding {
                file: d.path,
                line: d.line,
                column: d.column,
                severity: d.severity,
                message: d.message,
                rule: Some(d.rule),
                fixable: d.fixable,
            })
            .collect::<Vec<_>>();
        let skipped = report
            .skipped
            .into_iter()
            .map(|s| JsonLintSkipped {
                file: s.path,
                line: s.line,
                column: s.column,
                reason: s.reason,
                message: s.message,
            })
            .collect::<Vec<_>>();
        let success = findings.is_empty();
        let problems = if findings.len() == 1 {
            "problem"
        } else {
            "problems"
        };
        let mut summary = format!(
            "{} lint {problems} in {} files",
            findings.len(),
            report.files
        );
        if report.fixed > 0 {
            summary += &format!(", {} fixed", report.fixed);
        }
        if !skipped.is_empty() {
            summary += &format!(
                "; {} skipped with a syntax error, which typecheck reports",
                skipped.len()
            );
        }

        Self {
            status: if success { "ok" } else { "failed" },
            success,
            exit_code: output.status.code(),
            summary: Some(summary),
            findings,
            skipped,
            stderr,
        }
    }
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
    output.kv("manifest", &report.paths.manifest);
    output.kv("installation", &report.paths.installation);
    output.blank();

    for (name, label) in [
        ("test", "[test]"),
        ("health", "[health]"),
        ("fmt_lint", "[fmt]/[lint]"),
    ] {
        if let Some(source) = report.settings.get(name) {
            output.kv(label, source);
        }
    }
    output.blank();

    for name in ["godot", "gdtidy", "gdtest", "gdhealth", "gdtypecheck"] {
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
        for name in ["gdtest", "gdhealth", "gdtypecheck", "gdtidy"] {
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
    output.kv("manifest", &report.paths.manifest);
    output.kv("installation", &report.paths.installation);
    for (name, source) in &report.settings {
        output.kv(&format!("{name}_settings"), source);
    }

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
    /// Where each tool reads its settings: a gdsuite.toml section, its own
    /// file, or "defaults".
    settings: BTreeMap<String, String>,
    tools: BTreeMap<String, DoctorToolReport>,
    compatibility: BTreeMap<String, DoctorCompatibilityReport>,
    warnings: Vec<String>,
    summary: String,
}

#[derive(Debug, Serialize)]
struct DoctorPaths {
    project_root: String,
    config: String,
    manifest: String,
    installation: String,
}

impl DoctorPaths {
    fn from_context(context: &ProjectContext) -> Self {
        Self {
            project_root: context.root.display().to_string(),
            config: context.config_path.display().to_string(),
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
) -> Result<Vec<CheckStepResult>> {
    let mut steps = Vec::new();

    for step in &config.check.steps {
        let result = match step {
            CheckStep::Fmt => run_fmt_step(context)?,
            CheckStep::Lint => run_lint_step(context)?,
            CheckStep::Typecheck => run_typecheck_step(context)?,
            CheckStep::Test => run_test_step(context, config)?,
            CheckStep::Health => run_health_check_step(context, config)?,
        };
        let failed = result.report.status == "failed";
        steps.push(result);
        if failed {
            break;
        }
    }

    Ok(steps)
}

fn run_fmt_step(context: &ProjectContext) -> Result<CheckStepResult> {
    let started = Instant::now();
    let toolchain = load_toolchain_or_error(context)?;
    warn_unconverted_configs(context);
    let cmd_args = ["fmt", "--check", "--format", "json", "."].map(String::from);
    let output = process::run(&toolchain.gdtidy, &cmd_args, &context.root)
        .map_err(|error| command_error(EXIT_TOOLCHAIN_ERROR, error.to_string()))?;
    let duration_ms = started.elapsed().as_millis();
    let diagnostics = fmt_diagnostics(&output);

    if diagnostics.is_empty() {
        return Ok(CheckStepResult::passed(
            StepReport::ok("fmt", duration_ms, "format check passed"),
            "Formatting check passed",
            "fmt",
            duration_ms,
        ));
    }

    Ok(CheckStepResult::failed(
        StepReport::failed("fmt", duration_ms, "formatting issues found"),
        "Formatting issues found",
        "fmt",
        duration_ms,
        diagnostics.clone(),
        format!(
            "{}. Run `gdsuite fmt` to format the target.",
            format_formatting_summary(&diagnostics)
        ),
    ))
}

fn run_lint_step(context: &ProjectContext) -> Result<CheckStepResult> {
    let started = Instant::now();
    let toolchain = load_toolchain_or_error(context)?;
    warn_unconverted_configs(context);
    let output = process::run(&toolchain.gdtidy, &lint_args(Vec::new()), &context.root)
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
        status: if lint_report.success {
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

    if lint_report.success {
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

fn run_typecheck_step(context: &ProjectContext) -> Result<CheckStepResult> {
    let started = Instant::now();
    let toolchain = load_toolchain_or_error(context)?;
    // Warnings would not be shown here, and reporting them takes a second Godot run.
    let args = vec!["--no-warnings".to_string(), ".".to_string()];
    let output = process::run(&toolchain.gdtypecheck, &args, &context.root)
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

    let diagnostics = collect_nonempty_lines(&output.stdout, &output.stderr);

    Ok(CheckStepResult::failed(
        StepReport::failed("typecheck", duration_ms, "typecheck failed"),
        "GDScript type check failed",
        "typecheck",
        duration_ms,
        diagnostics.clone(),
        typecheck_failure_summary(&diagnostics),
    ))
}

fn run_test_step(context: &ProjectContext, config: &GdsuiteConfig) -> Result<CheckStepResult> {
    let report =
        run_test_step_with_args(context, vec![config.commands.default_test_target.clone()])?;
    let duration_ms = report.duration_ms;
    if report.status == "ok" {
        return Ok(CheckStepResult::passed(
            report,
            "Tests passed",
            "test",
            duration_ms,
        ));
    }
    Ok(CheckStepResult::failed(
        report,
        "Tests failed",
        "test",
        duration_ms,
        Vec::new(),
        "Fix the reported test failure and rerun `gdsuite check`.".to_string(),
    ))
}

fn run_health_check_step(
    context: &ProjectContext,
    config: &GdsuiteConfig,
) -> Result<CheckStepResult> {
    let report = run_health_step(
        context,
        vec![config.commands.default_health_subcommand.clone()],
    )?;
    let duration_ms = report.duration_ms;
    if report.status == "ok" {
        return Ok(CheckStepResult::passed(
            report,
            "Health checks passed",
            "health",
            duration_ms,
        ));
    }
    Ok(CheckStepResult::failed(
        report.clone(),
        "Health checks failed",
        "health",
        duration_ms,
        Vec::new(),
        report.summary,
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

fn run_health_step(context: &ProjectContext, forwarded: Vec<String>) -> Result<StepReport> {
    let started = Instant::now();
    let toolchain = load_toolchain_or_error(context)?;
    let args = default_health_args(&load_runtime_config(context)?, forwarded);

    let output = process::run(&toolchain.gdhealth, &args, &context.root)
        .map_err(|error| command_error(EXIT_TOOLCHAIN_ERROR, error.to_string()))?;
    let bundle_dir = context.root.join(tooling::GDHEALTH_ARTIFACT_DIR);
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

/// What fails the format check, from `gdtidy fmt --check --format json`: the
/// files that would change, and files gdtidy could not format safely. Files
/// with a syntax error are left to typecheck, as in lint. Output that is not
/// gdtidy's report is shown as it is.
fn fmt_diagnostics(output: &process::ProcessOutput) -> Vec<String> {
    #[derive(serde::Deserialize)]
    struct Report {
        changed: Vec<String>,
        skipped: Vec<Skipped>,
    }
    #[derive(serde::Deserialize)]
    struct Skipped {
        path: String,
        reason: String,
        message: String,
    }
    let Ok(report) = serde_json::from_str::<Report>(&output.stdout) else {
        if output.status.success() {
            return Vec::new();
        }
        return collect_nonempty_lines(&output.stdout, &output.stderr);
    };
    report
        .changed
        .into_iter()
        .map(|path| format!("{path}: would be reformatted"))
        .chain(
            report
                .skipped
                .into_iter()
                .filter(|s| s.reason != "syntax-error")
                .map(|s| format!("{}: {}", s.path, s.message)),
        )
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
        .map(|finding| {
            format!(
                "{}:{}:{} [{}] {}",
                finding.file,
                finding.line,
                finding.column,
                finding.rule.as_deref().unwrap_or("lint"),
                finding.message
            )
        })
        .collect::<Vec<_>>();
    diagnostics.extend(report.stderr.iter().cloned());
    diagnostics
}

fn lint_failure_summary(report: &JsonLintReport) -> String {
    if report.findings.is_empty() {
        return format!(
            "{}. Fix the gdtidy setup and rerun `gdsuite check`.",
            report.summary.as_deref().unwrap_or("gdtidy failed")
        );
    }
    let count = report.findings.len();
    let noun = if count == 1 { "problem" } else { "problems" };
    let fixable = report.findings.iter().filter(|f| f.fixable).count();
    if fixable == 0 {
        format!("Found {count} lint {noun}. Fix them and rerun `gdsuite check`.")
    } else if fixable == count {
        format!(
            "Found {count} lint {noun}. Run `gdsuite lint -- --fix` to fix them, and rerun `gdsuite check`."
        )
    } else {
        format!(
            "Found {count} lint {noun}, {fixable} fixable with `gdsuite lint -- --fix`. Fix the others and rerun `gdsuite check`."
        )
    }
}

fn typecheck_failure_summary(diagnostics: &[String]) -> String {
    let count = diagnostics
        .iter()
        .filter(|line| line.contains(": error["))
        .count();
    match count {
        0 => "gdtypecheck could not run. See the message above.".to_string(),
        1 => "Found 1 error. Fix it and rerun `gdsuite check`.".to_string(),
        _ => format!("Found {count} errors. Fix them and rerun `gdsuite check`."),
    }
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
