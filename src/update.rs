//! `gdsuite update`: bring the `[tools]` section of a project set up by
//! gdsuite 0.4 or earlier up to date, so it uses the tools built into gdsuite.
//!
//! A release pin (`github:q-rault/gdtest@v0.2.1`) or crates.io source
//! (`crate:gdtest@0.2.1`) of one of gdsuite's own tools is removed: both were
//! ways to get the tool gdsuite now has built in. Any other source (a local
//! folder, a Git repository) is the project's choice and is left as it is.
//!
//! The file is edited line by line, so its comments and layout are kept.

use crate::config::{ReleaseManifest, TOOL_NAMES};
use crate::tooling::own_release_prefix;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PinChange {
    /// The source was removed: the tool built into gdsuite runs instead.
    Unpinned { tool: String, from: String },
    /// A source gdsuite does not manage, kept as it is.
    Kept { tool: String, source: String },
}

/// Rewrites the `[tools]` section of `text`. Returns the new text and what
/// changed for each tool `[tools]` names.
pub fn refresh_tool_pins(text: &str, release: &ReleaseManifest) -> (String, Vec<PinChange>) {
    let lines: Vec<&str> = text.split_inclusive('\n').collect();
    let mut out: Vec<String> = Vec::with_capacity(lines.len());
    let mut changes = Vec::new();

    let mut index = 0;
    while index < lines.len() {
        let line = lines[index];
        if table_header(line) != Some("tools") {
            out.push(line.to_string());
            index += 1;
            continue;
        }

        // The [tools] section runs until the next table header.
        let mut end = index + 1;
        while end < lines.len() && table_header(lines[end]).is_none() {
            end += 1;
        }

        let mut body = Vec::new();
        for &body_line in &lines[index + 1..end] {
            match rewrite_source_line(body_line, release) {
                Some((replacement, change)) => {
                    changes.push(change);
                    if let Some(replacement) = replacement {
                        body.push(replacement);
                    }
                }
                None => body.push(body_line.to_string()),
            }
        }

        let has_keys = body.iter().any(|line| {
            let trimmed = line.trim();
            !trimmed.is_empty() && !trimmed.starts_with('#')
        });
        if has_keys {
            out.push(line.to_string());
            out.extend(body);
        } else if out.last().is_some_and(|last| !last.trim().is_empty())
            && body.last().is_some_and(|last| last.trim().is_empty())
        {
            // Nothing is left to pin, so the section goes. Keep one blank
            // line between the sections around it, as the file had.
            out.push("\n".to_string());
        }
        index = end;
    }

    let mut text = out.concat();
    // A dropped last section leaves blank lines at the end of the file.
    while text.ends_with("\n\n") {
        text.pop();
    }
    (text, changes)
}

fn table_header(line: &str) -> Option<&str> {
    let trimmed = line.trim();
    let inner = trimmed.strip_prefix('[')?.split(']').next()?;
    if trimmed.starts_with("[[") {
        return Some("");
    }
    Some(inner.trim())
}

/// `None` when the line is not a tool source. Otherwise the replacement line
/// (`None` to remove it) and what changed.
fn rewrite_source_line(
    line: &str,
    release: &ReleaseManifest,
) -> Option<(Option<String>, PinChange)> {
    let (key, value) = line.split_once('=')?;
    let tool = key.trim().strip_suffix("_source")?;
    if !TOOL_NAMES.contains(&tool) {
        return None;
    }
    let source = value.trim().trim_matches('"').to_string();
    release.tool(tool)?;
    let ours = source.starts_with(&own_release_prefix(tool))
        || source.starts_with(&format!("crate:{tool}@"));
    let tool = tool.to_string();
    if ours {
        return Some((None, PinChange::Unpinned { tool, from: source }));
    }
    Some((Some(line.to_string()), PinChange::Kept { tool, source }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn release() -> &'static ReleaseManifest {
        ReleaseManifest::bundled()
    }

    #[test]
    fn release_pins_are_removed_with_their_section() {
        let text = "[project]\nroot = \".\"\n\n[tools]\ngdtest_source = \"github:q-rault/gdtest@v0.1.1\"\ngdtidy_source = \"github:q-rault/gdtidy@v0.1.0\"\n\n[check]\nsteps = [\"fmt\"]\n";
        let (updated, changes) = refresh_tool_pins(text, release());
        assert_eq!(
            updated,
            "[project]\nroot = \".\"\n\n[check]\nsteps = [\"fmt\"]\n"
        );
        assert_eq!(
            changes,
            vec![
                PinChange::Unpinned {
                    tool: "gdtest".into(),
                    from: "github:q-rault/gdtest@v0.1.1".into()
                },
                PinChange::Unpinned {
                    tool: "gdtidy".into(),
                    from: "github:q-rault/gdtidy@v0.1.0".into()
                },
            ]
        );
    }

    #[test]
    fn crate_sources_are_removed_and_others_are_kept() {
        let text = "# comment kept\n[tools]\n# why we build gdtest\ngdtest_source = \"crate:gdtest@0.1.0\"\ngdhealth_source = \"path:../gdhealth\"\n";
        let (updated, changes) = refresh_tool_pins(text, release());
        assert_eq!(
            updated,
            "# comment kept\n[tools]\n# why we build gdtest\ngdhealth_source = \"path:../gdhealth\"\n"
        );
        assert_eq!(
            changes,
            vec![
                PinChange::Unpinned {
                    tool: "gdtest".into(),
                    from: "crate:gdtest@0.1.0".into()
                },
                PinChange::Kept {
                    tool: "gdhealth".into(),
                    source: "path:../gdhealth".into()
                },
            ]
        );
    }

    #[test]
    fn sections_stay_apart_when_tools_had_no_blank_line_before_it() {
        // As gdsuite 0.4 wrote it.
        let text = "[project]\nroot = \".\"\n[tools]\ngdtest_source = \"github:q-rault/gdtest@v0.2.2\"\n\n[commands]\n";
        let (updated, _) = refresh_tool_pins(text, release());
        assert_eq!(updated, "[project]\nroot = \".\"\n\n[commands]\n");
    }

    #[test]
    fn a_file_without_tools_is_unchanged() {
        let text = "[project]\nroot = \".\"\n";
        assert_eq!(
            refresh_tool_pins(text, release()),
            (text.to_string(), vec![])
        );
    }

    #[test]
    fn other_keys_in_tools_are_kept() {
        let text =
            "[tools]\ngdtest_source = \"github:q-rault/gdtest@v0.1.1\"\nsomething_else = 1\n";
        let (updated, _) = refresh_tool_pins(text, release());
        assert_eq!(updated, "[tools]\nsomething_else = 1\n");
    }
}
