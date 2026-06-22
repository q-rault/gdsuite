use std::path::PathBuf;

use clap::{ArgAction, Args, Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(
    name = "gdsuite",
    version,
    about = "Project-local Godot tooling wrapper"
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
    Setup,
    Doctor,
    Fmt(FmtArgs),
    Lint(LintArgs),
    Typecheck,
    Test(ForwardArgs),
    Health(ForwardArgs),
    Check,
    Ci,
}

#[derive(Debug, Args)]
pub struct FmtArgs {
    #[arg(long)]
    pub check: bool,
}

#[derive(Debug, Args)]
pub struct LintArgs {
    #[arg(long)]
    pub fix: bool,
    #[arg(long)]
    pub unsafe_fixes: bool,
    #[arg(long, default_value = "text", value_parser = ["json", "text"])]
    pub report: String,
}

#[derive(Debug, Args)]
pub struct ForwardArgs {
    #[arg(last = true, action = ArgAction::Append)]
    pub args: Vec<String>,
}
