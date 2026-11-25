//! The GDSuite addon for the Godot editor, built into gdsuite and installed in
//! projects as `addons/gdsuite/`. Its source is `editor/addons/gdsuite/`.

use std::{
    collections::BTreeMap,
    fs,
    io::ErrorKind,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};
use sha2::{Digest, Sha256};

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
    addon_file!("details_pane.gd"),
    addon_file!("details_pane.gd.uid"),
    addon_file!("column_resizer.gd"),
    addon_file!("column_resizer.gd.uid"),
    addon_file!("folding.gd"),
    addon_file!("folding.gd.uid"),
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
    addon_file!("checker.gd"),
    addon_file!("checker.gd.uid"),
    addon_file!("dependency_graph.gd"),
    addon_file!("dependency_graph.gd.uid"),
    addon_file!("problem_store.gd"),
    addon_file!("problem_store.gd.uid"),
    addon_file!("script_edits.gd"),
    addon_file!("script_edits.gd.uid"),
    addon_file!("settings_menu.gd"),
    addon_file!("settings_menu.gd.uid"),
    addon_file!("ui.gd"),
    addon_file!("ui.gd.uid"),
];

const DIR: &str = "addons/gdsuite/";

/// The hash of each file as gdsuite wrote it, in `sha256sum` format. It tells
/// a file someone changed from one another gdsuite wrote, and is part of the
/// addon so that it is committed, cloned and checked out with the files.
const RECORD_PATH: &str = "addons/gdsuite/files.sha256";
const RECORD_HEADER: &str =
    "# Written by gdsuite, to update the files nobody changed: commit it with them.\n";

/// The files of the releases that kept no record.
const RELEASED: &str = include_str!("editor_addon_released.sha256");

/// Where those releases kept a copy of each file they wrote, for one machine
/// only: `.gdsuite/` isn't committed.
const OLD_COPY_DIR: &str = ".gdsuite/state/editor-addon";

/// What `install` did.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Installed {
    /// Files written: 0 when the project has this version already.
    pub written: usize,
    /// Files someone changed, kept as they are.
    pub kept: Vec<Kept>,
}

/// A file of the addon someone changed.
#[derive(Debug, PartialEq, Eq)]
pub struct Kept {
    pub path: PathBuf,
    /// Whether it isn't known to be this version with changes, so the project
    /// may mix two versions of the addon.
    pub other_version: bool,
}

impl Kept {
    pub fn message(&self) -> String {
        let path = self.path.display();
        if self.other_version {
            format!(
                "kept {path}, which was changed and may be from another version of the GDSuite addon, so the addon may not work; delete it to get this version"
            )
        } else {
            format!("kept {path}, which was changed; delete it to get gdsuite's")
        }
    }
}

enum State {
    Missing,
    Current,
    /// As another gdsuite wrote it.
    Outdated,
    Changed(Kept),
}

/// Writes the addon's files that are missing, or that another gdsuite wrote
/// and nobody changed since. A file someone changed is kept.
pub fn install(project_root: &Path) -> Result<Installed> {
    let recorded = read_record(project_root);
    let mut record = recorded.clone();
    let mut installed = Installed::default();
    for (relative, contents) in FILES {
        match file_state(project_root, relative, contents, &recorded)? {
            State::Changed(kept) => {
                installed.kept.push(kept);
                continue;
            }
            State::Current => {}
            State::Missing | State::Outdated => {
                write(&project_root.join(relative), contents)?;
                installed.written += 1;
            }
        }
        record.insert(name(relative).to_string(), hash(contents));
    }
    if record != recorded {
        write_record(project_root, &record)?;
    }
    Ok(installed)
}

/// The files someone changed, which `install` keeps.
pub fn changed_files(project_root: &Path) -> Result<Vec<Kept>> {
    let recorded = read_record(project_root);
    let mut changed = Vec::new();
    for (relative, contents) in FILES {
        if let State::Changed(kept) = file_state(project_root, relative, contents, &recorded)? {
            changed.push(kept);
        }
    }
    Ok(changed)
}

/// Compares a file with this gdsuite's, then with what is known of those other
/// gdsuites wrote.
fn file_state(
    project_root: &Path,
    relative: &str,
    contents: &str,
    recorded: &BTreeMap<String, String>,
) -> Result<State> {
    let path = project_root.join(relative);
    let existing = match fs::read(&path) {
        Ok(bytes) => hash(&String::from_utf8_lossy(&bytes)),
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(State::Missing),
        Err(error) => {
            return Err(error).with_context(|| format!("failed to read {}", path.display()));
        }
    };
    let current = hash(contents);
    if existing == current {
        return Ok(State::Current);
    }
    let name = name(relative);
    let written = recorded.get(name).cloned().or_else(|| {
        let copy = fs::read_to_string(project_root.join(OLD_COPY_DIR).join(relative));
        copy.ok().map(|copy| hash(&copy))
    });
    if written.as_ref() == Some(&existing)
        || record_lines(RELEASED).any(|line| line == (existing.as_str(), name))
    {
        return Ok(State::Outdated);
    }
    Ok(State::Changed(Kept {
        path,
        other_version: written != Some(current),
    }))
}

/// A file's name in the record: its path in the addon.
fn name(relative: &str) -> &str {
    relative.strip_prefix(DIR).unwrap_or(relative)
}

/// The SHA-256 of `text` with LF line endings: Git may check it out with CRLF.
fn hash(text: &str) -> String {
    format!("{:x}", Sha256::digest(text.replace("\r\n", "\n")))
}

/// `(hash, name)` of each line of a record.
fn record_lines(text: &str) -> impl Iterator<Item = (&str, &str)> {
    text.lines()
        .filter(|line| !line.starts_with('#'))
        .filter_map(|line| line.split_once("  "))
}

/// `name -> hash`, empty when the project has no record.
fn read_record(project_root: &Path) -> BTreeMap<String, String> {
    let text = fs::read_to_string(project_root.join(RECORD_PATH)).unwrap_or_default();
    record_lines(&text)
        .map(|(hash, name)| (name.to_string(), hash.to_string()))
        .collect()
}

fn write_record(project_root: &Path, record: &BTreeMap<String, String>) -> Result<()> {
    let mut text = RECORD_HEADER.to_string();
    for (name, hash) in record {
        text.push_str(&format!("{hash}  {name}\n"));
    }
    write(&project_root.join(RECORD_PATH), &text)
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

    /// plugin.cfg as gdsuite 0.7.1 to 0.7.4 wrote it.
    fn plugin_cfg(version: &str) -> String {
        format!(
            "[plugin]

name=\"GDSuite\"
description=\"Check, problems, formatting and tests from GDSuite, in the editor.\"
author=\"Quentin Rault\"
version=\"{version}\"
script=\"plugin.gd\"
"
        )
    }

    /// A project as `git clone` gives it: the addon, without `.gdsuite/`.
    fn cloned() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        install(dir.path()).unwrap();
        let _ = fs::remove_dir_all(dir.path().join(".gdsuite"));
        dir
    }

    /// The same, with plugin.cfg from a release that kept no record.
    fn cloned_before_records(cfg: &str) -> tempfile::TempDir {
        let dir = cloned();
        let _ = fs::remove_file(dir.path().join("addons/gdsuite/files.sha256"));
        fs::write(dir.path().join("addons/gdsuite/plugin.cfg"), cfg).unwrap();
        dir
    }

    /// The copy gdsuite 0.7 kept of plugin.cfg.
    fn old_copy(root: &Path, cfg: &str) {
        let copy = ".gdsuite/state/editor-addon/addons/gdsuite/plugin.cfg";
        write(&root.join(copy), cfg).unwrap();
    }

    /// Makes `file` as another gdsuite wrote it, and recorded it.
    fn as_another_gdsuite(root: &Path, file: &str, text: &str) {
        fs::write(root.join("addons/gdsuite").join(file), text).unwrap();
        let mut record = read_record(root);
        record.insert(file.to_string(), hash(text));
        write_record(root, &record).unwrap();
    }

    fn read(root: &Path, file: &str) -> String {
        fs::read_to_string(root.join("addons/gdsuite").join(file)).unwrap()
    }

    #[test]
    fn updates_a_fresh_clone_made_before_records() {
        let dir = cloned_before_records(&plugin_cfg("0.7.2"));

        let installed = install(dir.path()).unwrap();

        assert_eq!((installed.written, installed.kept.len()), (1, 0));
        assert_ne!(read(dir.path(), "plugin.cfg"), plugin_cfg("0.7.2"));
    }

    #[test]
    fn updates_a_pulled_release_the_old_copies_are_not_of() {
        let dir = cloned_before_records(&plugin_cfg("0.7.2"));
        old_copy(dir.path(), &plugin_cfg("0.7.1"));

        let installed = install(dir.path()).unwrap();

        assert_eq!((installed.written, installed.kept.len()), (1, 0));
    }

    #[test]
    fn refreshes_what_it_knows_of_files_already_up_to_date() {
        // 0.7.3 ran here, git pull brought this version and gdsuite ran, then
        // a checkout went back to 0.7.2.
        let dir = cloned();
        let _ = fs::remove_file(dir.path().join(RECORD_PATH));
        old_copy(dir.path(), &plugin_cfg("0.7.3"));
        assert_eq!(install(dir.path()).unwrap(), Installed::default());
        fs::write(
            dir.path().join("addons/gdsuite/plugin.cfg"),
            plugin_cfg("0.7.2"),
        )
        .unwrap();

        let installed = install(dir.path()).unwrap();

        assert_eq!((installed.written, installed.kept.len()), (1, 0));
    }

    #[test]
    fn line_endings_are_not_changes() {
        let dir = cloned_before_records(&plugin_cfg("0.7.2"));
        for entry in fs::read_dir(dir.path().join("addons/gdsuite")).unwrap() {
            let path = entry.unwrap().path();
            let text = fs::read_to_string(&path).unwrap();
            fs::write(&path, text.replace('\n', "\r\n")).unwrap();
        }

        let installed = install(dir.path()).unwrap();

        // plugin.cfg, and a file that is this version already isn't rewritten.
        assert_eq!((installed.written, installed.kept.len()), (1, 0));
        assert!(read(dir.path(), "plugin.gd").contains("\r\n"));
    }

    #[test]
    fn updates_and_downgrades_a_fresh_clone_by_its_record() {
        // Neither is in the released files: only the record knows them.
        for text in ["# older\n", "# newer\n"] {
            let dir = cloned();
            as_another_gdsuite(dir.path(), "plugin.gd", text);

            let installed = install(dir.path()).unwrap();

            assert_eq!((installed.written, installed.kept.len()), (1, 0));
            assert_eq!(install(dir.path()).unwrap(), Installed::default());
        }
    }

    #[test]
    fn writes_a_stale_or_conflicted_record_anew() {
        let dir = cloned();
        let current = read_record(dir.path());
        for record in [
            format!("{}  plugin.gd\n", hash("# older\n")),
            "<<<<<<< HEAD\n".to_string(),
        ] {
            fs::write(dir.path().join(RECORD_PATH), record).unwrap();

            assert_eq!(install(dir.path()).unwrap(), Installed::default());
            assert_eq!(read_record(dir.path()), current);
        }
    }

    #[test]
    fn keeps_edits_and_says_which_may_be_of_another_version() {
        let dir = cloned();
        let root = dir.path();
        fs::write(root.join("addons/gdsuite/LICENSE"), [0xff, 0xfe]).unwrap();
        as_another_gdsuite(root, "runner.gd", "# older\n");
        fs::write(root.join("addons/gdsuite/runner.gd"), "# older, mine\n").unwrap();
        fs::write(root.join("addons/gdsuite/ui.gd"), "mine").unwrap();

        for _ in 0..2 {
            let installed = install(root).unwrap();

            assert_eq!(installed.written, 0);
            let kept: Vec<(PathBuf, bool)> = installed
                .kept
                .iter()
                .map(|kept| (kept.path.clone(), kept.other_version))
                .collect();
            assert_eq!(
                kept,
                vec![
                    (root.join("addons/gdsuite/LICENSE"), false),
                    (root.join("addons/gdsuite/runner.gd"), true),
                    (root.join("addons/gdsuite/ui.gd"), false),
                ]
            );
            assert_eq!(changed_files(root).unwrap(), installed.kept);
            assert_eq!(read(root, "runner.gd"), "# older, mine\n");
            let license = fs::read(root.join("addons/gdsuite/LICENSE")).unwrap();
            assert_eq!(license, [0xff, 0xfe]);
        }
        // The change undone, the file is updated again.
        fs::write(root.join("addons/gdsuite/runner.gd"), "# older\n").unwrap();
        assert_eq!(install(root).unwrap().written, 1);
    }

    #[test]
    fn keeps_edits_made_before_records() {
        let mine = plugin_cfg("0.7.2") + "; mine\n";
        let dir = cloned_before_records(&mine);
        old_copy(dir.path(), &plugin_cfg("0.7.2"));

        let installed = install(dir.path()).unwrap();

        assert_eq!((installed.written, installed.kept.len()), (0, 1));
        assert!(installed.kept[0].other_version);
        assert_eq!(read(dir.path(), "plugin.cfg"), mine);
    }
}
