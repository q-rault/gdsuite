use std::{
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Result, anyhow};
use globwalk::GlobWalkerBuilder;

use crate::{config::GdsuiteConfig, process, tooling::ProjectContext};

#[derive(Debug, Clone)]
pub struct FixReport {
    pub changed_files: Vec<PathBuf>,
    pub skipped: Vec<String>,
}

pub fn discover_script_files(
    context: &ProjectContext,
    config: &GdsuiteConfig,
) -> Result<Vec<PathBuf>> {
    discover_files(context, &config.lint.include, &config.lint.exclude)
}

pub fn discover_typecheck_files(
    context: &ProjectContext,
    config: &GdsuiteConfig,
) -> Result<Vec<PathBuf>> {
    discover_files(context, &config.typecheck.include, &config.typecheck.exclude)
}

fn discover_files(
    context: &ProjectContext,
    include: &[String],
    exclude: &[String],
) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();

    for pattern in include {
        let walker = GlobWalkerBuilder::from_patterns(&context.root, &[pattern.as_str()])
            .file_type(globwalk::FileType::FILE)
            .build()?;

        for entry in walker {
            let entry = entry?;
            let relative = entry.path().strip_prefix(&context.root)?.to_path_buf();
            if is_excluded(&relative, exclude) {
                continue;
            }
            if !files.iter().any(|path| path == &relative) {
                files.push(relative);
            }
        }
    }

    files.sort();
    Ok(files)
}

pub fn apply_safe_fixes(
    context: &ProjectContext,
    config: &GdsuiteConfig,
    gdformat: &Path,
) -> Result<FixReport> {
    let files = discover_script_files(context, config)?;
    if !files.is_empty() {
        let mut args = vec!["-w".to_string()];
        args.extend(files.iter().map(|path| path.display().to_string()));
        process::run_checked(gdformat, &args, &context.root)?;
    }

    let mut changed_files = Vec::new();
    for relative in files {
        let full_path = context.root.join(&relative);
        if normalize_whitespace(&full_path)? {
            changed_files.push(relative);
        }
    }

    Ok(FixReport {
        changed_files,
        skipped: Vec::new(),
    })
}

pub fn apply_unsafe_fixes(context: &ProjectContext, config: &GdsuiteConfig) -> Result<FixReport> {
    let files = discover_script_files(context, config)?;
    let mut changed_files = Vec::new();
    let mut skipped = Vec::new();

    for relative in files {
        let full_path = context.root.join(&relative);
        match reorder_class_definitions(&full_path)? {
            UnsafeRewrite::Changed => changed_files.push(relative),
            UnsafeRewrite::Skipped(reason) => {
                skipped.push(format!("{}: {}", relative.display(), reason));
            }
            UnsafeRewrite::NotApplicable => {}
        }
    }

    Ok(FixReport {
        changed_files,
        skipped,
    })
}

fn normalize_whitespace(path: &Path) -> Result<bool> {
    let original = fs::read_to_string(path)?;
    let newline = if original.contains("\r\n") {
        "\r\n"
    } else {
        "\n"
    };
    let had_final_newline = original.ends_with('\n');
    let lines = original
        .lines()
        .map(|line| line.trim_end().to_string())
        .collect::<Vec<_>>();
    let mut updated = lines.join(newline);
    if !lines.is_empty() || had_final_newline {
        updated.push_str(newline);
    }

    if updated != original {
        fs::write(path, updated)?;
        return Ok(true);
    }

    Ok(false)
}

enum UnsafeRewrite {
    Changed,
    Skipped(String),
    NotApplicable,
}

fn reorder_class_definitions(path: &Path) -> Result<UnsafeRewrite> {
    let original = fs::read_to_string(path)?;
    if !original.contains("class_name") && !original.contains("extends") {
        return Ok(UnsafeRewrite::NotApplicable);
    }

    let newline = if original.contains("\r\n") {
        "\r\n"
    } else {
        "\n"
    };
    let lines = original.lines().map(str::to_string).collect::<Vec<_>>();
    let mut segments = Vec::new();
    let mut index = 0;

    while index < lines.len() {
        let current = lines[index].trim_start();

        if current.is_empty() {
            segments.push(Segment::blank(lines[index].clone()));
            index += 1;
            continue;
        }

        if current.starts_with('#') {
            segments.push(Segment::comment(lines[index].clone()));
            index += 1;
            continue;
        }

        let kind = classify(current)
            .ok_or_else(|| anyhow!("unsupported top-level construct for unsafe fixer"))?;

        let mut raw = vec![lines[index].clone()];
        index += 1;
        while index < lines.len() && lines[index].trim().is_empty() {
            raw.push(lines[index].clone());
            index += 1;
        }
        segments.push(Segment::decl(kind, raw));
    }

    if !segments
        .iter()
        .any(|segment| matches!(segment.kind, SegmentKind::Decl(_)))
    {
        return Ok(UnsafeRewrite::NotApplicable);
    }

    let mut ordered = Vec::new();
    let mut decls = segments
        .iter()
        .filter_map(|segment| match &segment.kind {
            SegmentKind::Decl(kind) => Some((kind.rank(), segment.raw.clone())),
            _ => None,
        })
        .collect::<Vec<_>>();
    if decls.is_empty() {
        return Ok(UnsafeRewrite::NotApplicable);
    }
    decls.sort_by_key(|item| item.0);

    for (_, raw) in decls {
        ordered.extend(raw);
    }

    let reordered = ordered.join(newline) + newline;
    if reordered == original {
        return Ok(UnsafeRewrite::NotApplicable);
    }
    if reordered.contains("func ") {
        return Ok(UnsafeRewrite::Skipped(
            "functions are not supported by this fixer".to_string(),
        ));
    }

    fs::write(path, reordered)?;
    Ok(UnsafeRewrite::Changed)
}

fn is_excluded(path: &Path, patterns: &[String]) -> bool {
    let text = path.to_string_lossy().replace('\\', "/");
    patterns.iter().any(|pattern| {
        let normalized = pattern.replace('\\', "/").replace("**", "");
        let prefix = normalized.trim_end_matches('*').trim_end_matches('/');
        !prefix.is_empty() && text.starts_with(prefix)
    })
}

fn classify(line: &str) -> Option<DeclKind> {
    if line.starts_with("class_name ") {
        return Some(DeclKind::ClassName);
    }
    if line.starts_with("extends ") {
        return Some(DeclKind::Extends);
    }
    if line.starts_with("const ") {
        return Some(DeclKind::Const);
    }
    if line.starts_with("enum ") {
        return Some(DeclKind::Enum);
    }
    if line.starts_with("signal ") {
        return Some(DeclKind::Signal);
    }
    if line.starts_with("@export ") || line.starts_with("@onready ") || line.starts_with("var ") {
        return Some(DeclKind::Var);
    }
    None
}

#[derive(Clone)]
struct Segment {
    kind: SegmentKind,
    raw: Vec<String>,
}

impl Segment {
    fn blank(line: String) -> Self {
        Self {
            kind: SegmentKind::Blank,
            raw: vec![line],
        }
    }

    fn comment(line: String) -> Self {
        Self {
            kind: SegmentKind::Comment,
            raw: vec![line],
        }
    }

    fn decl(kind: DeclKind, raw: Vec<String>) -> Self {
        Self {
            kind: SegmentKind::Decl(kind),
            raw,
        }
    }
}

#[derive(Clone)]
enum SegmentKind {
    Blank,
    Comment,
    Decl(DeclKind),
}

#[derive(Clone, Copy)]
enum DeclKind {
    ClassName,
    Extends,
    Const,
    Enum,
    Signal,
    Var,
}

impl DeclKind {
    fn rank(self) -> usize {
        match self {
            DeclKind::ClassName => 0,
            DeclKind::Extends => 1,
            DeclKind::Const => 2,
            DeclKind::Enum => 3,
            DeclKind::Signal => 4,
            DeclKind::Var => 5,
        }
    }
}
