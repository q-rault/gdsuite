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
        .with_context(|| format!("failed to execute {}", command_path.display()))?;

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
            "{} failed with exit code {:?}\nstdout:\n{}\nstderr:\n{}",
            command_path.display(),
            output.status.code(),
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
        .with_context(|| format!("failed to execute {}", command_path.display()))?;

    if !status.success() {
        return Err(anyhow!(
            "{} failed with exit code {:?}",
            command_path.display(),
            status.code()
        ));
    }

    Ok(())
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

fn build_bare_command(command_path: &Path) -> Command {
    if is_script(command_path) {
        let mut command = Command::new("cmd");
        command.arg("/C").arg(command_path);
        return command;
    }

    Command::new(command_path)
}

fn normalize_windows_path(path: &Path) -> PathBuf {
    let text = path.display().to_string();
    if let Some(stripped) = text.strip_prefix(r"\\?\") {
        return PathBuf::from(stripped);
    }
    path.to_path_buf()
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
