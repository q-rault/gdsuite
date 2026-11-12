use std::path::PathBuf;

use clap::{ArgAction, Args, Parser, Subcommand, ValueEnum};

#[derive(Debug, Parser)]
#[command(
    name = "gdsuite",
    version,
    about = "Godot developer platform entrypoint"
)]
pub struct Cli {
    #[arg(long, global = true, default_value = ".")]
    pub project_root: PathBuf,
    #[arg(long, global = true)]
    pub godot_exe: Option<PathBuf>,
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Debug, Subcommand)]
pub enum Commands {
    #[command(about = "Bootstrap the local gdsuite toolchain and write suite config")]
    Init(OutputArgs),
    #[command(about = "Rewrite gdsuite.toml to the orchestration-only schema")]
    Migrate,
    #[command(about = "Install or repair the local gdsuite toolchain")]
    Setup(SetupArgs),
    #[command(
        about = "Move the tools to the versions this gdsuite was tested with, then install them",
        long_about = "Move the tools to the versions this gdsuite was tested with, then install them.\n\nRelease pins of gdsuite's tools are removed from [tools] in gdsuite.toml, so the tools follow gdsuite from then on. crates.io sources move to the tested version. Other sources (a folder, a Git repository, another release) are kept."
    )]
    Update(OutputArgs),
    #[command(about = "Print resolved tool paths, versions, and setup status")]
    Doctor(DoctorArgs),
    #[command(about = "Format GDScript with `gdtidy fmt` (arguments are forwarded)")]
    Fmt(ForwardArgs),
    #[command(about = "Lint GDScript with gdtidy; arguments after -- go to `gdtidy lint`")]
    Lint(LintArgs),
    #[command(about = "Forward arguments to gdtypecheck")]
    Typecheck(ForwardArgs),
    #[command(
        about = "Forward arguments to project-local gdtest",
        long_about = "Forward arguments to project-local gdtest.\n\nUse `gdsuite test` for normal suite-owned test execution. Use `gdsuite test -- ...` when you need direct gdtest flags or subcommands."
    )]
    Test(ForwardReportArgs),
    #[command(
        about = "Forward arguments to project-local gdhealth",
        long_about = "Forward arguments to project-local gdhealth.\n\nUse `gdsuite health` for the standard suite-owned health entrypoint. Use `gdsuite health -- ...` when you need direct gdhealth subcommands or flags."
    )]
    Health(ForwardReportArgs),
    #[command(about = "Run the default fast local validation pipeline")]
    Check(RunArgs),
    #[command(about = "Print a saved suite JSON report artifact")]
    Report(ReportArgs),
}

#[derive(Debug, Args)]
pub struct SetupArgs {
    #[command(flatten)]
    pub output: OutputArgs,
    #[arg(
        long,
        help = "Deprecated; delegated product config syncing is no longer part of the suite contract"
    )]
    pub sync_product_configs: bool,
}

#[derive(Debug, Clone, Args)]
pub struct OutputArgs {
    #[arg(long, value_enum, default_value = "pretty")]
    pub output: OutputModeArg,
}

#[derive(Debug, Clone, Args)]
pub struct DoctorArgs {
    #[command(flatten)]
    pub output: OutputArgs,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum OutputModeArg {
    Pretty,
    Plain,
    Json,
}

#[derive(Debug, Args)]
pub struct LintArgs {
    #[arg(long, value_parser = ["json", "text"])]
    pub report: Option<String>,
    #[arg(last = true, action = ArgAction::Append)]
    pub args: Vec<String>,
}

#[derive(Debug, Args)]
pub struct ForwardArgs {
    #[arg(last = true, action = ArgAction::Append)]
    pub args: Vec<String>,
}

#[derive(Debug, Args)]
pub struct ForwardReportArgs {
    #[arg(long, value_parser = ["json", "text"])]
    pub report: Option<String>,
    #[arg(last = true, action = ArgAction::Append)]
    pub args: Vec<String>,
}

#[derive(Debug, Args)]
pub struct RunArgs {
    #[arg(long, value_parser = ["json", "text"])]
    pub report: Option<String>,
}

#[derive(Debug, Args)]
pub struct ReportArgs {
    #[arg(default_value = "latest")]
    #[arg(help = "Saved report name or `ci` to aggregate check/test/health artifacts")]
    pub name: String,
}
