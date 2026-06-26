use std::path::PathBuf;

use clap::{ArgAction, Args, Parser, Subcommand};

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
    #[command(about = "Bootstrap the local gdsuite toolchain and product configs")]
    Init,
    #[command(about = "Migrate an existing project to the suite-first config model")]
    Migrate,
    #[command(about = "Install or repair the local gdsuite toolchain and product configs")]
    Setup(SetupArgs),
    #[command(about = "Print resolved tool paths, versions, and setup status")]
    Doctor,
    #[command(about = "Run gdformat on discovered GDScript files")]
    Fmt(FmtArgs),
    #[command(about = "Run gdlint and optional suite-owned fixers")]
    Lint(LintArgs),
    #[command(about = "Run a Godot headless load() pass over discovered scripts")]
    Typecheck,
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
pub struct FmtArgs {
    #[arg(long)]
    pub check: bool,
}

#[derive(Debug, Args)]
pub struct SetupArgs {
    #[arg(
        long,
        help = "Merge safe suite-owned defaults into existing product configs"
    )]
    pub sync_product_configs: bool,
}

#[derive(Debug, Args)]
pub struct LintArgs {
    #[arg(long)]
    pub fix: bool,
    #[arg(long)]
    pub unsafe_fixes: bool,
    #[arg(long, value_parser = ["json", "text"])]
    pub report: Option<String>,
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
