//! Code health findings, as gdsuite shows them: one shape with lint and the
//! type check, so that editors list them all alike.

use std::{collections::BTreeSet, fs, path::Path};

use anyhow::{Context, Result, anyhow};
use serde::{Deserialize, Serialize};

use crate::process;

/// A gdhealth finding, in the shape of a `lint --report json` finding.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HealthFinding {
    pub file: String,
    pub line: usize,
    pub column: usize,
    /// `error` for what gdhealth calls critical, else `warning`.
    pub severity: String,
    pub message: String,
    pub rule: String,
    pub fixable: bool,
    /// The other scripts it is about: a cycle's, or the one depended on.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub related: Vec<String>,
    /// What gdhealth suggests to do about it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub suggestion: Option<String>,
    /// Compared with an older revision: `new`, `worse` or `existing`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub change: Option<String>,
}

impl HealthFinding {
    /// The finding on one line, as `gdsuite lint` prints its own.
    pub fn line_text(&self) -> String {
        format!(
            "{}:{}:{}: {}[{}]: {}",
            self.file, self.line, self.column, self.severity, self.rule, self.message
        )
    }
}

/// What `gdsuite health --quick --report json` prints.
#[derive(Debug, Serialize)]
pub struct QuickReport {
    status: &'static str,
    success: bool,
    pub summary: String,
    pub findings: Vec<HealthFinding>,
    graph: Graph,
}

#[derive(Debug, Serialize)]
struct Graph {
    scripts: Vec<Script>,
    /// `[from, to, line]`: `from` uses `to`.
    edges: Vec<(String, String, usize)>,
    cycles: Vec<Vec<String>>,
}

#[derive(Debug, Serialize)]
struct Script {
    path: String,
    group: Option<String>,
    lines: usize,
}

// gdhealth's reports; only the fields gdsuite uses.

#[derive(Debug, Deserialize)]
struct Report {
    #[serde(default)]
    policy_findings: Vec<Finding>,
}

#[derive(Debug, Deserialize)]
struct Finding {
    rule_id: String,
    severity: String,
    message: String,
    file: Option<String>,
    line: Option<usize>,
    #[serde(default)]
    related_files: Vec<String>,
    suggested_fix: Option<Suggestion>,
    change: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "kind")]
enum Suggestion {
    BreakCycle {
        cut_edges: Vec<(String, String)>,
    },
    SplitFile {
        proposed_groups: Vec<Vec<String>>,
    },
    /// One a newer gdhealth makes.
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Deserialize)]
struct GraphReport {
    files: Vec<GraphFile>,
    edges: Vec<GraphEdge>,
    cycles: Vec<Vec<String>>,
}

#[derive(Debug, Deserialize)]
struct GraphFile {
    path: String,
    group: Option<String>,
    line_count: usize,
}

#[derive(Debug, Deserialize)]
struct GraphEdge {
    from: String,
    to: String,
    line: usize,
}

/// Runs gdhealth without Git history, which takes most of its time and no
/// finding needs, and without writing the project's report.
pub fn quick(gdhealth: &Path, root: &Path) -> Result<QuickReport> {
    let run = |args: &[&str]| -> Result<String> {
        let args = args.iter().map(ToString::to_string).collect::<Vec<_>>();
        let output = process::run(gdhealth, &args, root)?;
        if !output.status.success() {
            return Err(anyhow!(
                "gdhealth {}: {}",
                process::describe_exit(output.status.code()),
                output.stderr.trim()
            ));
        }
        Ok(output.stdout)
    };

    // gdhealth prints this report to a file only.
    let path = std::env::temp_dir().join(format!("gdsuite-health-{}.json", std::process::id()));
    let shown = path.display().to_string();
    let ran = run(&["health", "--no-git", "--format", "json", "--output", &shown]);
    let report = ran
        .and_then(|_| fs::read_to_string(&path).with_context(|| format!("failed to read {shown}")));
    let _ = fs::remove_file(&path);
    let report: Report =
        serde_json::from_str(&report?).context("failed to read gdhealth's report")?;
    let graph: GraphReport = serde_json::from_str(&run(&["graph", "--format", "json"])?)
        .context("failed to read gdhealth's graph")?;

    let findings = findings(report);
    let errors = findings.iter().filter(|f| f.severity == "error").count();
    let summary = format!(
        "{errors} critical, {} other code health findings in {} scripts",
        findings.len() - errors,
        graph.files.len()
    );
    // A script that loads itself is no loop between scripts. One that uses
    // another in several places is one arrow, from the first.
    let edges: BTreeSet<_> = graph
        .edges
        .into_iter()
        .filter(|edge| edge.from != edge.to)
        .map(|edge| (edge.from, edge.to, edge.line))
        .collect();
    let mut edges: Vec<_> = edges.into_iter().collect();
    edges.dedup_by(|a, b| a.0 == b.0 && a.1 == b.1);
    Ok(QuickReport {
        status: "ok",
        success: true,
        summary,
        findings,
        graph: Graph {
            scripts: graph
                .files
                .into_iter()
                .map(|file| Script {
                    path: file.path,
                    group: file.group,
                    lines: file.line_count,
                })
                .collect(),
            edges,
            cycles: graph.cycles.into_iter().filter(|c| c.len() > 1).collect(),
        },
    })
}

/// The findings of the `health.json` gdhealth wrote at `path`; none when it
/// can't be read.
pub fn findings_of_report(path: &Path) -> Vec<HealthFinding> {
    fs::read_to_string(path)
        .ok()
        .and_then(|text| serde_json::from_str::<Report>(&text).ok())
        .map(findings)
        .unwrap_or_default()
}

fn findings(report: Report) -> Vec<HealthFinding> {
    report
        .policy_findings
        .into_iter()
        .filter_map(convert)
        .collect()
}

/// gdhealth's rules for what lint checks already: one script's length
/// (gdtidy's `max-file-lines`) and a function's parameters
/// (`function-arguments-number`). A limit that needs only the file is
/// lint's, which reports it as you type and can be silenced on its line;
/// code health keeps what needs the whole project or its history.
const LINT_OWNED_RULES: &[&str] = &["file_lines_threshold", "parameters_threshold"];

fn convert(finding: Finding) -> Option<HealthFinding> {
    if LINT_OWNED_RULES.contains(&finding.rule_id.as_str()) {
        return None;
    }
    let file = finding.file?;
    if finding.rule_id == "cycle_detected"
        && finding.related_files.iter().all(|related| *related == file)
    {
        return None;
    }
    Some(HealthFinding {
        line: finding.line.unwrap_or(1),
        column: 1,
        severity: if finding.severity == "critical" {
            "error".to_string()
        } else {
            "warning".to_string()
        },
        message: message(&finding.message, &file),
        rule: finding
            .rule_id
            .trim_end_matches("_threshold")
            .replace('_', "-"),
        fixable: false,
        related: finding
            .related_files
            .into_iter()
            .filter(|related| *related != file)
            .collect(),
        suggestion: finding.suggested_fix.and_then(suggestion),
        change: finding.change,
        file,
    })
}

/// gdhealth's message without the script it starts with, which is said
/// already: `a.gd::_ready spans 90 lines` is `_ready() spans 90 lines`.
fn message(message: &str, file: &str) -> String {
    let Some(rest) = message.strip_prefix(file) else {
        return message.to_string();
    };
    match rest.strip_prefix("::") {
        Some(rest) => match rest.split_once(' ') {
            Some((function, rest)) => format!("{function}() {rest}"),
            None => rest.to_string(),
        },
        None => format!("This script{rest}"),
    }
}

fn suggestion(suggestion: Suggestion) -> Option<String> {
    match suggestion {
        Suggestion::BreakCycle { cut_edges } if !cut_edges.is_empty() => Some(format!(
            "Breaking one dependency ends the cycle: {}.",
            cut_edges
                .iter()
                .map(|(from, to)| format!("{from} → {to}"))
                .collect::<Vec<_>>()
                .join(", or ")
        )),
        Suggestion::SplitFile { proposed_groups } if !proposed_groups.is_empty() => Some(format!(
            "Could be split into {}: {}.",
            proposed_groups.len(),
            proposed_groups
                .iter()
                .map(|group| group.join(", "))
                .collect::<Vec<_>>()
                .join(" | ")
        )),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read(json: &str) -> Vec<HealthFinding> {
        findings(serde_json::from_str(json).unwrap())
    }

    #[test]
    fn findings_read_like_lint_findings() {
        let found = read(
            r#"{"policy_findings": [
                {"rule_id": "function_lines_threshold", "severity": "critical",
                 "message": "a/b.gd::_ready spans 90 lines (warn 40, critical 80)",
                 "file": "a/b.gd", "line": 12, "related_files": ["a/b.gd"]},
                {"rule_id": "split-file-candidate", "severity": "info",
                 "message": "a/b.gd (568 lines) could split into 2 cohesive groups (modularity 0.40)",
                 "file": "a/b.gd", "line": null, "related_files": ["a/b.gd"],
                 "suggested_fix": {"kind": "SplitFile", "proposed_groups": [["f", "g"], ["h"]],
                                   "modularity": 0.4}},
                {"rule_id": "file_lines_threshold", "severity": "warning",
                 "message": "a/b.gd has 568 lines (warn 500, critical 1000)",
                 "file": "a/b.gd", "line": null, "related_files": ["a/b.gd"]},
                {"rule_id": "parameters_threshold", "severity": "warning",
                 "message": "a/b.gd::_ready has 6 parameters (warn 5)",
                 "file": "a/b.gd", "line": 12, "related_files": ["a/b.gd"]}]}"#,
        );
        // Lint reports a script's length and a function's parameters.
        assert_eq!(found.len(), 2);
        assert_eq!(
            found[0].line_text(),
            "a/b.gd:12:1: error[function-lines]: _ready() spans 90 lines (warn 40, critical 80)"
        );
        assert!(found[0].related.is_empty());
        assert_eq!(
            found[1].line_text(),
            "a/b.gd:1:1: warning[split-file-candidate]: This script (568 lines) could split into 2 cohesive groups (modularity 0.40)"
        );
        assert_eq!(
            found[1].suggestion.as_deref(),
            Some("Could be split into 2: f, g | h.")
        );
    }

    #[test]
    fn a_cycle_names_its_scripts_and_a_script_loading_itself_is_none() {
        let found = read(
            r#"{"policy_findings": [
                {"rule_id": "cycle_detected", "severity": "info",
                 "message": "dependency cycle detected: a.gd -> b.gd",
                 "file": "a.gd", "line": null, "related_files": ["a.gd", "b.gd"],
                 "suggested_fix": {"kind": "BreakCycle", "cut_edges": [["b.gd", "a.gd"]]}},
                {"rule_id": "cycle_detected", "severity": "info",
                 "message": "dependency cycle detected: c.gd",
                 "file": "c.gd", "line": null, "related_files": ["c.gd"]}]}"#,
        );
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].rule, "cycle-detected");
        assert_eq!(found[0].severity, "warning");
        assert_eq!(found[0].related, ["b.gd"]);
        assert_eq!(
            found[0].suggestion.as_deref(),
            Some("Breaking one dependency ends the cycle: b.gd → a.gd.")
        );
    }
}
