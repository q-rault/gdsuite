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
    #[command(about = "Add GDSuite to a project: write gdsuite.toml and install the test addons")]
    Init(OutputArgs),
    #[command(about = "Rewrite gdsuite.toml to the orchestration-only schema")]
    Migrate,
    #[command(about = "Install or repair what the project needs, such as after cloning it")]
    Setup(SetupArgs),
    #[command(
        about = "Bring a project set up by gdsuite 0.4 or earlier up to date",
        long_about = "Bring a project set up by gdsuite 0.4 or earlier up to date: use the tools built into this gdsuite.\n\nRelease pins of gdsuite's tools are removed from [tools] in gdsuite.toml, and the tools older versions downloaded are removed from .gdsuite/. crates.io sources move to the built-in version. Other sources (a folder, a Git repository, another release) are kept."
    )]
    Update(OutputArgs),
    #[command(about = "Print resolved tool paths, versions, and setup status")]
    Doctor(DoctorArgs),
    #[command(about = "Format GDScript with `gdtidy fmt` (arguments are forwarded)")]
    Fmt(FmtArgs),
    #[command(about = "Lint GDScript with gdtidy; arguments after -- go to `gdtidy lint`")]
    Lint(LintArgs),
    #[command(about = "Type check the project with gdtypecheck (arguments are forwarded)")]
    Typecheck(ForwardReportArgs),
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
    #[command(
        about = "Read or change gdsuite.toml, keeping its comments",
        long_about = "Read or change gdsuite.toml, keeping its comments and layout.

Keys are dotted: `lint.disable`, `editor.format_on_save`, `check.steps`. Values are TOML (`true`, `100`, `[\"fmt\"]`); anything else is a string."
    )]
    Config(ConfigArgs),
}

#[derive(Debug, Args)]
pub struct FmtArgs {
    /// Format this file's text and print the result, instead of formatting
    /// files in place. Editors use it to format unsaved text with the
    /// project's settings.
    #[arg(long, value_name = "FILE")]
    pub input: Option<PathBuf>,
    #[arg(last = true, action = ArgAction::Append)]
    pub args: Vec<String>,
}

#[derive(Debug, Args)]
pub struct ConfigArgs {
    #[command(subcommand)]
    pub action: ConfigAction,
}

#[derive(Debug, Subcommand)]
pub enum ConfigAction {
    /// Print a value as JSON, `null` when it is not set
    Get { key: String },
    /// Set a value
    Set { key: String, value: String },
    /// Remove a value
    Unset { key: String },
    /// Add a value to a list, once
    Add { key: String, value: String },
    /// Remove a value from a list
    Remove { key: String, value: String },
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
