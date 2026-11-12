//! The GDSuite addon for the Godot editor, built into gdsuite and installed in
//! projects as `addons/gdsuite/`. Its source is `editor/addons/gdsuite/`.

use std::{fs, path::Path};

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
    addon_file!("markers.gd"),
    addon_file!("markers.gd.uid"),
    addon_file!("problems_dock.gd"),
    addon_file!("problems_dock.gd.uid"),
    addon_file!("runner.gd"),
    addon_file!("runner.gd.uid"),
    addon_file!("settings.gd"),
    addon_file!("settings.gd.uid"),
];

/// Writes the addon's files that are missing or differ from this gdsuite's.
/// Returns how many it wrote: 0 when the project has this version already.
pub fn install(project_root: &Path) -> Result<usize> {
    let mut written = 0;
    for (relative, contents) in FILES {
        let path = project_root.join(relative);
        if fs::read_to_string(&path).is_ok_and(|existing| existing == *contents) {
            continue;
        }
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("failed to create {}", parent.display()))?;
        }
        fs::write(&path, contents)
            .with_context(|| format!("failed to write {}", path.display()))?;
        written += 1;
    }
    Ok(written)
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
        assert_eq!(install(dir.path()).unwrap(), FILES.len());
        assert_eq!(install(dir.path()).unwrap(), 0);
        assert!(is_installed(dir.path()));

        fs::write(dir.path().join("addons/gdsuite/plugin.gd"), "edited").unwrap();
        assert_eq!(install(dir.path()).unwrap(), 1);
    }
}
