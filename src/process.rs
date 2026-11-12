use std::{
    env,
    path::{Path, PathBuf},
    process::{Command, ExitStatus, Stdio},
    sync::OnceLock,
};

use anyhow::{Context, Result, anyhow};

static GODOT_EXE: OnceLock<PathBuf> = OnceLock::new();

/// The Godot that delegated tools should use, unless `GODOT_EXE` is already set.
pub fn set_godot_exe(path: &Path) {
    let _ = GODOT_EXE.set(path.to_path_buf());
}

pub struct ProcessOutput {
    pub status: ExitStatus,
    pub stdout: String,
    pub stderr: String,
}

pub fn run(command_path: &Path, args: &[String], cwd: &Path) -> Result<ProcessOutput> {
    let normalized_path = normalize_windows_path(command_path);
    let mut command = build_command(&normalized_path);
    command.args(args).current_dir(cwd);
    let output = command
        .output()
        .with_context(|| format!("failed to execute {}", shown(command_path)))?;

    Ok(ProcessOutput {
        status: output.status,
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    })
}

pub fn run_checked(command_path: &Path, args: &[String], cwd: &Path) -> Result<ProcessOutput> {
    let output = run(command_path, args, cwd)?;
    if !output.status.success() {
        return Err(anyhow!(
            "{} {}\nstdout:\n{}\nstderr:\n{}",
            shown(command_path),
            describe_exit(output.status.code()),
            output.stdout,
            output.stderr
        ));
    }
    Ok(output)
}

pub fn run_checked_streaming(command_path: &Path, args: &[String], cwd: &Path) -> Result<()> {
    let normalized_path = normalize_windows_path(command_path);
    let mut command = build_command(&normalized_path);
    let status = command
        .args(args)
        .current_dir(cwd)
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .with_context(|| format!("failed to execute {}", shown(command_path)))?;

    if !status.success() {
        return Err(anyhow!(
            "{} {}",
            shown(command_path),
            describe_exit(status.code())
        ));
    }

    Ok(())
}

/// How messages name a program: a built-in tool by its name (`gdtest`), any
/// other by its path.
fn shown(command_path: &Path) -> String {
    match bundled_tool_name(command_path) {
        Some(name) => name.to_string(),
        None => command_path.display().to_string(),
    }
}

/// How a failed process ended, as in "failed with exit code 1".
pub fn describe_exit(code: Option<i32>) -> String {
    match code {
        Some(code) => format!("failed with exit code {code}"),
        None => "was stopped by a signal".to_string(),
    }
}

fn build_command(command_path: &Path) -> Command {
    let mut command = build_bare_command(command_path);
    if env::var_os("GODOT_EXE").is_none()
        && let Some(godot) = GODOT_EXE.get()
    {
        command.env("GODOT_EXE", godot);
    }
    command
}

/// Where a tool built into gdsuite "is": not a file, a name that
/// `build_command` runs as `gdsuite tool <name>`.
const BUNDLED_PREFIX: &str = "gdsuite tool ";

/// The path standing for the built-in tool `name`.
pub fn bundled_tool_path(name: &str) -> PathBuf {
    PathBuf::from(format!("{BUNDLED_PREFIX}{name}"))
}

/// The tool name when `path` stands for a built-in tool.
pub fn bundled_tool_name(path: &Path) -> Option<&str> {
    path.to_str()?.strip_prefix(BUNDLED_PREFIX)
}

fn build_bare_command(command_path: &Path) -> Command {
    if let Some(name) = bundled_tool_name(command_path) {
        let gdsuite = env::current_exe().unwrap_or_else(|_| PathBuf::from("gdsuite"));
        let mut command = Command::new(gdsuite);
        command.arg(crate::bundled::TOOL_SUBCOMMAND).arg(name);
        return command;
    }
    if is_script(command_path) {
        let mut command = Command::new("cmd");
        command.arg("/C").arg(command_path);
        return command;
    }

    Command::new(command_path)
}

/// Removes the `\\\\?\\` prefix of canonicalized Windows paths, which is noise in
/// messages and which tools such as Python's `venv` can fail on.
pub fn normalize_windows_path(path: &Path) -> PathBuf {
    let text = path.display().to_string();
    match text.strip_prefix(r"\\?\") {
        Some(stripped) if !stripped.starts_with("UNC") => PathBuf::from(stripped),
        _ => path.to_path_buf(),
    }
}

fn is_script(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|value| value.to_str()),
        Some("cmd" | "bat")
    )
}

pub fn candidate_paths(base: PathBuf) -> Vec<PathBuf> {
    let mut paths = vec![base.clone()];
    if base.extension().is_none() {
        paths.push(base.with_extension("exe"));
        paths.push(base.with_extension("cmd"));
        paths.push(base.with_extension("bat"));
    }
    paths
}
