//! The GDSuite addon for the Godot editor, built into gdsuite and installed in
//! projects as `addons/gdsuite/`. Its source is `editor/addons/gdsuite/`.

use std::{
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};

macro_rules! addon_file {
    ($name:literal) => {
        (
            concat!("addons/gdsuite/", $name),
            include_str!(concat!("../editor/addons/gdsuite/", $name)),
        )
    };
}

/// `(path in the project, contents)` of every file of the addon.
const FILES: &[(&str, &str)] = &[
    addon_file!("LICENSE"),
    addon_file!("plugin.cfg"),
    addon_file!("plugin.gd"),
    addon_file!("plugin.gd.uid"),
    addon_file!("problem.gd"),
    addon_file!("problem.gd.uid"),
    addon_file!("column_resizer.gd"),
    addon_file!("column_resizer.gd.uid"),
    addon_file!("markers.gd"),
    addon_file!("markers.gd.uid"),
    addon_file!("problems_dock.gd"),
    addon_file!("problems_dock.gd.uid"),
    addon_file!("runner.gd"),
    addon_file!("runner.gd.uid"),
    addon_file!("settings.gd"),
    addon_file!("settings.gd.uid"),
    addon_file!("tests_dock.gd"),
    addon_file!("tests_dock.gd.uid"),
    addon_file!("test_result.gd"),
    addon_file!("test_result.gd.uid"),
    addon_file!("tests_watch.gd"),
    addon_file!("tests_watch.gd.uid"),
    addon_file!("tests_gutter.gd"),
    addon_file!("tests_gutter.gd.uid"),
    addon_file!("tests_menu.gd"),
    addon_file!("tests_menu.gd.uid"),
    addon_file!("ui.gd"),
    addon_file!("ui.gd.uid"),
];

/// Where gdsuite keeps a copy of each addon file as it wrote it, to tell a
/// file someone changed from one an older gdsuite wrote.
const WRITTEN_COPY_DIR: &str = ".gdsuite/state/editor-addon";

/// What `install` did.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Installed {
    /// Files written: 0 when the project has this version already.
    pub written: usize,
    /// Files that differ from what gdsuite wrote, kept as they are.
    pub kept: Vec<PathBuf>,
}

/// Writes the addon's files that are missing, or that an older gdsuite wrote
/// and nobody changed since. A file someone changed is kept.
pub fn install(project_root: &Path) -> Result<Installed> {
    let mut installed = Installed::default();
    for (relative, contents) in FILES {
        let path = project_root.join(relative);
        let written_copy = project_root.join(WRITTEN_COPY_DIR).join(relative);
        match fs::read_to_string(&path) {
            Ok(existing) if existing == *contents => {
                if !written_copy.exists() {
                    write(&written_copy, contents)?;
                }
                continue;
            }
            Ok(existing) => {
                let as_written = fs::read_to_string(&written_copy).ok();
                if as_written.as_deref() != Some(existing.as_str()) {
                    installed.kept.push(path);
                    continue;
                }
            }
            Err(_) => {}
        }
        write(&path, contents)?;
        write(&written_copy, contents)?;
        installed.written += 1;
    }
    Ok(installed)
}

fn write(path: &Path, contents: &str) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    fs::write(path, contents).with_context(|| format!("failed to write {}", path.display()))
}

/// Whether the project has the addon, in any version.
pub fn is_installed(project_root: &Path) -> bool {
    project_root.join("addons/gdsuite/plugin.cfg").exists()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn installs_once_then_changes_nothing() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(install(dir.path()).unwrap().written, FILES.len());
        assert_eq!(install(dir.path()).unwrap(), Installed::default());
        assert!(is_installed(dir.path()));
    }

    #[test]
    fn replaces_what_an_older_gdsuite_wrote_but_keeps_edits() {
        let dir = tempfile::tempdir().unwrap();
        install(dir.path()).unwrap();
        let plugin = dir.path().join("addons/gdsuite/plugin.gd");
        let runner = dir.path().join("addons/gdsuite/runner.gd");
        // As an older gdsuite would have written it, and recorded it.
        fs::write(&plugin, "old").unwrap();
        fs::write(
            dir.path()
                .join(WRITTEN_COPY_DIR)
                .join("addons/gdsuite/plugin.gd"),
            "old",
        )
        .unwrap();
        // Changed by hand.
        fs::write(&runner, "mine").unwrap();

        let installed = install(dir.path()).unwrap();

        assert_eq!(installed.written, 1);
        assert_eq!(installed.kept, vec![runner.clone()]);
        assert_ne!(fs::read_to_string(&plugin).unwrap(), "old");
        assert_eq!(fs::read_to_string(&runner).unwrap(), "mine");
    }
}
