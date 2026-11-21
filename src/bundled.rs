//! The tools built into gdsuite.
//!
//! gdsuite runs each tool as its own process, as it did when the tools were
//! separate programs: `gdsuite tool <name> [ARGS]...` runs the tool built in.
//! Their output, exit codes and `std::process::exit` calls stay their own.

use std::{
    ffi::OsString,
    fs,
    path::{Path, PathBuf},
    process::ExitCode,
};

use anyhow::{Context, Result};

/// The hidden subcommand that runs a built-in tool.
pub const TOOL_SUBCOMMAND: &str = "tool";

/// Runs `gdsuite tool <name> [ARGS]...` when `args` is that command line, or
/// when gdsuite was started under a tool's name (`gdtest`, `gdtidy.exe`).
/// `None` when `args` is an ordinary gdsuite command line.
pub fn dispatch(args: &[OsString]) -> Option<ExitCode> {
    let invoked_as = args
        .first()
        .and_then(|program| Path::new(program).file_stem())
        .and_then(|stem| stem.to_str())
        .filter(|stem| is_tool(stem));
    if let Some(name) = invoked_as {
        return Some(run_tool(name, args.iter().skip(1).cloned()));
    }

    if args.get(1).is_some_and(|arg| arg == TOOL_SUBCOMMAND) {
        let Some(name) = args.get(2).and_then(|name| name.to_str()) else {
            eprintln!(
                "error: `gdsuite tool` needs a tool name: gdtest, gdhealth, gdtypecheck or gdtidy"
            );
            return Some(ExitCode::from(2));
        };
        if !is_tool(name) {
            eprintln!(
                "error: no tool named {name}; the tools are gdtest, gdhealth, gdtypecheck and gdtidy"
            );
            return Some(ExitCode::from(2));
        }
        return Some(run_tool(name, args.iter().skip(3).cloned()));
    }
    None
}

fn is_tool(name: &str) -> bool {
    matches!(name, "gdtest" | "gdhealth" | "gdtypecheck" | "gdtidy")
}

/// Runs a built-in tool with `args` (without the program name) and returns
/// its exit code.
fn run_tool(name: &str, args: impl Iterator<Item = OsString>) -> ExitCode {
    let argv = std::iter::once(OsString::from(name)).chain(args);
    match name {
        "gdtidy" => gdtidy::run(argv),
        "gdtypecheck" => gdtypecheck::run(argv),
        "gdtest" => match gdtest::run(argv) {
            Ok(()) => ExitCode::SUCCESS,
            Err(error) => {
                // As gdtest's own `main` reports an error.
                eprintln!("Error: {error:?}");
                ExitCode::FAILURE
            }
        },
        "gdhealth" => match gdhealth::run_from(argv) {
            Ok(()) => ExitCode::SUCCESS,
            Err(error) => {
                eprintln!("{error}");
                ExitCode::FAILURE
            }
        },
        _ => unreachable!("dispatch checks the tool name"),
    }
}

/// Writes the gdtest addons built into gdsuite under `dir`
/// (`dir/addons/gdtest/`, `dir/addons/gdtest_editor/`), where the addon
/// installer copies them from, as from a downloaded release.
pub fn write_gdtest_addons(dir: &Path) -> Result<PathBuf> {
    if dir.exists() {
        fs::remove_dir_all(dir).with_context(|| format!("failed to clear {}", dir.display()))?;
    }
    for (relative, contents) in gdtest::addon_files() {
        let path = dir.join(relative);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("failed to create {}", parent.display()))?;
        }
        fs::write(&path, contents)
            .with_context(|| format!("failed to write {}", path.display()))?;
    }
    Ok(dir.to_path_buf())
}

/// Makes `launcher` this gdsuite: a hard link, or a copy where links don't
/// work. Named `gdtest`, it runs gdtest (see `dispatch`), for the gdtest
/// panel in Godot; named `gdsuite`, it is gdsuite, for the GDSuite addon.
/// Replaced when gdsuite changed since.
pub fn refresh_link(launcher: &Path) -> Result<()> {
    let gdsuite = std::env::current_exe().context("failed to find the gdsuite program")?;
    refresh_link_to(launcher, &gdsuite)
}

/// Makes `launcher` the program at `gdsuite`. The editor starts several
/// gdsuite processes at once, and each may refresh the same launcher: the new
/// one is made under a name of its own, then renamed over the old one in one
/// step. Replacing the launcher in place let one process copy gdsuite onto a
/// link another had just made to gdsuite itself, which emptied both.
fn refresh_link_to(launcher: &Path, gdsuite: &Path) -> Result<()> {
    static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    if is_current(launcher, gdsuite) {
        return Ok(());
    }
    let parent = launcher.parent().unwrap_or(Path::new("."));
    fs::create_dir_all(parent).with_context(|| format!("failed to create {}", parent.display()))?;
    let name = launcher.file_name().unwrap_or_default().to_string_lossy();
    let next = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let temp = parent.join(format!(".{name}.{}.{next}.tmp", std::process::id()));
    let _ = fs::remove_file(&temp);
    if fs::hard_link(gdsuite, &temp).is_err() {
        fs::copy(gdsuite, &temp).with_context(|| format!("failed to write {}", temp.display()))?;
    }
    let renamed = fs::rename(&temp, launcher);
    // When `launcher` already is `gdsuite` (another process got there
    // first), the rename leaves `temp` in place.
    let _ = fs::remove_file(&temp);
    renamed.with_context(|| format!("failed to replace {}", launcher.display()))
}

/// Whether `launcher` is this gdsuite: same size, and not older.
fn is_current(launcher: &Path, gdsuite: &Path) -> bool {
    let (Ok(launcher), Ok(gdsuite)) = (fs::metadata(launcher), fs::metadata(gdsuite)) else {
        return false;
    };
    let not_older = match (launcher.modified(), gdsuite.modified()) {
        (Ok(launcher), Ok(gdsuite)) => launcher >= gdsuite,
        _ => false,
    };
    launcher.len() == gdsuite.len() && not_older
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn launchers_refreshed_at_once_stay_whole() {
        let dir = tempfile::tempdir().unwrap();
        let gdsuite = dir.path().join("gdsuite-new");
        fs::write(&gdsuite, vec![7u8; 64 * 1024]).unwrap();
        let launcher = dir.path().join("bin").join("gdsuite");
        fs::create_dir_all(launcher.parent().unwrap()).unwrap();
        for _ in 0..20 {
            // An older gdsuite, as after an update.
            let _ = fs::remove_file(&launcher);
            fs::write(&launcher, b"old").unwrap();
            let threads: Vec<_> = (0..8)
                .map(|_| {
                    let (launcher, gdsuite) = (launcher.clone(), gdsuite.clone());
                    std::thread::spawn(move || refresh_link_to(&launcher, &gdsuite))
                })
                .collect();
            for thread in threads {
                thread.join().unwrap().unwrap();
            }
            assert_eq!(fs::read(&launcher).unwrap().len(), 64 * 1024);
            assert_eq!(fs::read(&gdsuite).unwrap().len(), 64 * 1024);
        }
        let leftovers: Vec<_> = fs::read_dir(launcher.parent().unwrap())
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        assert_eq!(leftovers, vec![std::ffi::OsString::from("gdsuite")]);
    }
}
