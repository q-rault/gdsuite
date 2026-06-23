use std::{
    env, fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, anyhow};

use crate::{
    config::{GdsuiteConfig, Manifest, ProfileConfig, Versions},
    process,
};

pub const GD_DIR: &str = ".gdsuite";

#[derive(Debug, Clone)]
pub struct ProjectContext {
    pub root: PathBuf,
    pub gdsuite_dir: PathBuf,
    pub state_dir: PathBuf,
    pub config_path: PathBuf,
    pub gdtest_config_path: PathBuf,
    pub gdhealth_config_path: PathBuf,
    pub manifest_path: PathBuf,
    pub versions_path: PathBuf,
}

#[derive(Debug, Clone)]
pub struct Toolchain {
    pub godot_exe: PathBuf,
    pub python: PathBuf,
    pub gdformat: PathBuf,
    pub gdlint: PathBuf,
    pub gdtest: PathBuf,
    pub gdhealth: PathBuf,
}

impl ProjectContext {
    pub fn new(root: PathBuf) -> Self {
        let gdsuite_dir = root.join(GD_DIR);
        let state_dir = gdsuite_dir.join("state");

        Self {
            config_path: root.join("gdsuite.toml"),
            gdtest_config_path: root.join("gdtest.toml"),
            gdhealth_config_path: root.join("gdhealth.toml"),
            manifest_path: state_dir.join("manifest.json"),
            versions_path: state_dir.join("versions.json"),
            root,
            gdsuite_dir,
            state_dir,
        }
    }

    pub fn ensure_dirs(&self) -> Result<()> {
        fs::create_dir_all(self.gdsuite_dir.join("bin"))?;
        fs::create_dir_all(self.gdsuite_dir.join("python"))?;
        fs::create_dir_all(self.gdsuite_dir.join("tools"))?;
        fs::create_dir_all(&self.state_dir)?;
        Ok(())
    }
}

pub fn load_config(context: &ProjectContext) -> Result<GdsuiteConfig> {
    GdsuiteConfig::load(&context.config_path)
}

pub fn load_manifest(context: &ProjectContext) -> Result<Manifest> {
    Manifest::load(&context.manifest_path)
}

pub fn save_manifest(context: &ProjectContext, manifest: &Manifest) -> Result<()> {
    manifest.write(&context.manifest_path)
}

pub fn save_versions(context: &ProjectContext, config: &GdsuiteConfig) -> Result<()> {
    Versions {
        gdtoolkit: config.tools.gdtoolkit.clone(),
        gdtest_source: config.tools.gdtest_source.clone(),
        gdhealth_source: config.tools.gdhealth_source.clone(),
    }
    .write(&context.versions_path)
}

pub fn resolve_godot(
    context: &ProjectContext,
    cli_override: Option<&Path>,
    config: Option<&GdsuiteConfig>,
) -> Result<PathBuf> {
    let mut candidates = Vec::new();

    if let Some(path) = cli_override {
        candidates.push(path.to_path_buf());
    }
    if let Some(path) = env::var_os("GODOT_EXE") {
        candidates.push(PathBuf::from(path));
    }
    if let Some(config) = config
        && let Some(path) = &config.project.godot_exe
    {
        candidates.push(path.clone());
    }
    if let Ok(path) = which::which("godot") {
        candidates.push(path);
    }
    if let Ok(path) = which::which("godot4") {
        candidates.push(path);
    }
    candidates.push(PathBuf::from(r"C:\Users\me\tools\Godot.exe"));
    candidates.push(PathBuf::from(r"C:\Program Files\Godot\Godot.exe"));
    candidates.push(PathBuf::from(r"C:\Program Files\Godot_v4\Godot.exe"));

    for candidate in candidates {
        for path in process::candidate_paths(candidate) {
            if path.exists() {
                return Ok(path);
            }
        }
    }

    Err(anyhow!(
        "could not resolve Godot executable for {}",
        context.root.display()
    ))
}

pub fn resolve_toolchain(
    context: &ProjectContext,
    manifest: Option<&Manifest>,
) -> Result<Toolchain> {
    let manifest = manifest.cloned().unwrap_or_default();

    Ok(Toolchain {
        godot_exe: manifest
            .godot_exe
            .ok_or_else(|| anyhow!("missing Godot path in manifest"))?,
        python: manifest
            .python
            .or_else(|| find_python_in_venv(context))
            .ok_or_else(|| anyhow!("missing project Python in manifest"))?,
        gdformat: manifest
            .gdformat
            .or_else(|| find_binary(context, &["gdformat"]))
            .ok_or_else(|| anyhow!("missing gdformat in manifest"))?,
        gdlint: manifest
            .gdlint
            .or_else(|| find_binary(context, &["gdlint"]))
            .ok_or_else(|| anyhow!("missing gdlint in manifest"))?,
        gdtest: manifest
            .gdtest
            .or_else(|| find_binary(context, &["gdtest"]))
            .ok_or_else(|| anyhow!("missing gdtest in manifest"))?,
        gdhealth: manifest
            .gdhealth
            .or_else(|| find_binary(context, &["gdhealth"]))
            .ok_or_else(|| anyhow!("missing gdhealth in manifest"))?,
    })
}

pub fn install_toolchain(
    context: &ProjectContext,
    config: &GdsuiteConfig,
    godot_exe: &Path,
) -> Result<Manifest> {
    let python = ensure_venv(context)?;
    install_gdtoolkit(&python, &config.tools.gdtoolkit, context)?;
    let gdtest = cargo_install(context, &config.tools.gdtest_source, "gdtest")?;
    let gdhealth = cargo_install(context, &config.tools.gdhealth_source, "gdhealth")?;
    let gdformat =
        find_binary(context, &["gdformat"]).ok_or_else(|| anyhow!("gdformat install missing"))?;
    let gdlint =
        find_binary(context, &["gdlint"]).ok_or_else(|| anyhow!("gdlint install missing"))?;

    Ok(Manifest {
        godot_exe: Some(godot_exe.to_path_buf()),
        python: Some(python),
        gdformat: Some(gdformat),
        gdlint: Some(gdlint),
        gdtest: Some(gdtest),
        gdhealth: Some(gdhealth),
    })
}

pub fn generate_external_configs(context: &ProjectContext, config: &GdsuiteConfig) -> Result<()> {
    write_if_missing(&context.gdtest_config_path, &render_gdtest_config(config))?;
    write_if_missing(
        &context.gdhealth_config_path,
        &render_gdhealth_config(config),
    )?;
    Ok(())
}

pub fn sync_product_configs(context: &ProjectContext, config: &GdsuiteConfig) -> Result<()> {
    let merged = merged_gdhealth_config(context, config)?;
    fs::write(&context.gdhealth_config_path, toml::to_string(&merged)?)
        .with_context(|| format!("failed to write {}", context.gdhealth_config_path.display()))?;
    Ok(())
}

pub fn product_config_warnings(
    context: &ProjectContext,
    config: &GdsuiteConfig,
) -> Result<Vec<String>> {
    let missing = missing_gdhealth_excludes(context, config)?;
    if missing.is_empty() {
        return Ok(Vec::new());
    }

    Ok(vec![format!(
        "gdhealth config missing suite excludes: {}; run `gdsuite setup --sync-product-configs` to persist the suite-safe merge",
        missing.join(", ")
    )])
}

pub fn verify_toolchain(context: &ProjectContext, toolchain: &Toolchain) -> Result<()> {
    process::run_checked(
        &toolchain.godot_exe,
        &["--version".to_string()],
        &context.root,
    )?;
    process::run_checked(&toolchain.python, &["--version".to_string()], &context.root)?;
    process::run_checked(&toolchain.gdformat, &["--help".to_string()], &context.root)?;
    process::run_checked(&toolchain.gdlint, &["--help".to_string()], &context.root)?;
    process::run_checked(&toolchain.gdtest, &["--version".to_string()], &context.root)?;
    process::run_checked(&toolchain.gdhealth, &["--help".to_string()], &context.root)?;
    Ok(())
}

pub fn find_binary(context: &ProjectContext, names: &[&str]) -> Option<PathBuf> {
    let roots = [
        context.gdsuite_dir.join("python").join("Scripts"),
        context.gdsuite_dir.join("python").join("bin"),
        context.gdsuite_dir.join("tools").join("gdtest").join("bin"),
        context
            .gdsuite_dir
            .join("tools")
            .join("gdhealth")
            .join("bin"),
        context.gdsuite_dir.join("bin"),
    ];

    for root in roots {
        for name in names {
            for candidate in process::candidate_paths(root.join(name)) {
                if candidate.exists() {
                    return Some(candidate);
                }
            }
        }
    }

    None
}

fn ensure_venv(context: &ProjectContext) -> Result<PathBuf> {
    if let Some(path) = find_python_in_venv(context) {
        return Ok(path);
    }

    let python = which::which("python")
        .or_else(|_| which::which("py"))
        .context("python is required for setup")?;
    process::run_checked(
        &python,
        &[
            "-m".to_string(),
            "venv".to_string(),
            context.gdsuite_dir.join("python").display().to_string(),
        ],
        &context.root,
    )?;

    find_python_in_venv(context).ok_or_else(|| anyhow!("venv bootstrap did not produce python"))
}

fn install_gdtoolkit(python: &Path, version: &str, context: &ProjectContext) -> Result<()> {
    let package = format!("gdtoolkit=={version}");
    let args = vec![
        "-m".to_string(),
        "pip".to_string(),
        "install".to_string(),
        package,
    ];
    process::run_checked(python, &args, &context.root)?;
    Ok(())
}

fn cargo_install(context: &ProjectContext, source: &Path, name: &str) -> Result<PathBuf> {
    let cargo = which::which("cargo").context("cargo is required for setup")?;
    let source_root = context.root.join(source);
    if !source_root.exists() {
        return Err(anyhow!(
            "tool source does not exist: {}",
            source_root.display()
        ));
    }

    let root = context.gdsuite_dir.join("tools").join(name);
    let package_root = resolve_package_root(&source_root, name)?;

    let args = vec![
        "install".to_string(),
        "--path".to_string(),
        package_root.display().to_string(),
        "--root".to_string(),
        root.display().to_string(),
    ];
    process::run_checked(&cargo, &args, &context.root)?;

    for candidate in process::candidate_paths(root.join("bin").join(name)) {
        if candidate.exists() {
            return Ok(candidate);
        }
    }

    Err(anyhow!("cargo install did not produce {}", name))
}

fn resolve_package_root(source_root: &Path, name: &str) -> Result<PathBuf> {
    let manifest_path = source_root.join("Cargo.toml");
    if !manifest_path.exists() {
        return Ok(source_root.to_path_buf());
    }

    let text = fs::read_to_string(&manifest_path)
        .with_context(|| format!("failed to read {}", manifest_path.display()))?;
    let manifest: toml::Value = toml::from_str(&text)
        .with_context(|| format!("failed to parse {}", manifest_path.display()))?;

    if manifest.get("package").is_some() {
        return Ok(source_root.to_path_buf());
    }

    let members = manifest
        .get("workspace")
        .and_then(|w| w.get("members"))
        .and_then(|m| m.as_array())
        .ok_or_else(|| anyhow!("workspace at {} has no members", source_root.display()))?;

    for member in members {
        let member_path = member
            .as_str()
            .ok_or_else(|| anyhow!("invalid workspace member"))?;
        let candidate = source_root.join(member_path);
        let member_manifest = candidate.join("Cargo.toml");
        if !member_manifest.exists() {
            continue;
        }
        let member_text = fs::read_to_string(&member_manifest)?;
        let member_toml: toml::Value = toml::from_str(&member_text)?;
        let package_name = member_toml
            .get("package")
            .and_then(|p| p.get("name"))
            .and_then(|n| n.as_str());
        if package_name == Some(name) {
            return Ok(candidate);
        }
    }

    Err(anyhow!(
        "could not find package '{}' in workspace at {}",
        name,
        source_root.display()
    ))
}

fn find_python_in_venv(context: &ProjectContext) -> Option<PathBuf> {
    let bases = [
        context
            .gdsuite_dir
            .join("python")
            .join("Scripts")
            .join("python"),
        context
            .gdsuite_dir
            .join("python")
            .join("bin")
            .join("python"),
    ];

    for base in bases {
        for path in process::candidate_paths(base) {
            if path.exists() {
                return Some(path);
            }
        }
    }

    None
}

fn write_if_missing(path: &Path, contents: &str) -> Result<()> {
    if path.exists() {
        return Ok(());
    }
    fs::write(path, contents).with_context(|| format!("failed to write {}", path.display()))
}

fn render_gdtest_config(config: &GdsuiteConfig) -> String {
    let mut lines = vec![
        "project_path = \".\"".to_string(),
        "display_mode = \"headless\"".to_string(),
        format!("patterns = {}", toml_array(test_patterns(config))),
        format!(
            "coverage_exclude = {}",
            toml_array(config.paths.exclude.clone())
        ),
        String::new(),
        "[profile.ci]".to_string(),
        "reporter = \"junit\"".to_string(),
        "fail_fast = true".to_string(),
    ];

    if let Some(godot_exe) = &config.project.godot_exe {
        lines.insert(
            1,
            format!(
                "godot_exe = \"{}\"",
                godot_exe.display().to_string().replace('\\', "\\\\")
            ),
        );
    }

    if let Some(strict) = config.profiles.entries.get("strict") {
        append_gdtest_profile(&mut lines, "strict", strict);
    }
    if let Some(local) = config.profiles.entries.get("local")
        && local.pipeline.iter().any(|step| step == "test")
    {
        append_gdtest_profile(&mut lines, "local", local);
    }

    lines.join("\n") + "\n"
}

fn append_gdtest_profile(lines: &mut Vec<String>, name: &str, profile: &ProfileConfig) {
    if !profile.pipeline.iter().any(|step| step == "test") {
        return;
    }

    lines.push(String::new());
    lines.push(format!("[profile.{name}]"));
    if profile.health_fail_on.as_deref() == Some("warning") {
        lines.push("fail_fast = true".to_string());
    }
}

pub(crate) fn render_gdhealth_config(config: &GdsuiteConfig) -> String {
    let lines = vec![
        "[project]".to_string(),
        "root = \".\"".to_string(),
        String::new(),
        "[discovery]".to_string(),
        format!("include = {}", toml_array(config.paths.scripts.clone())),
        format!("exclude = {}", toml_array(config.paths.exclude.clone())),
        String::new(),
        "[groups]".to_string(),
        format!("client = {}", toml_array(group_patterns(config, "client"))),
        format!("server = {}", toml_array(group_patterns(config, "server"))),
        format!("test = {}", toml_array(group_patterns(config, "test"))),
        format!(
            "root_gameplay = {}",
            toml_array(root_gameplay_patterns(config))
        ),
    ];

    lines.join("\n") + "\n"
}

pub fn merged_gdhealth_config(
    context: &ProjectContext,
    config: &GdsuiteConfig,
) -> Result<toml::Value> {
    let mut doc = if context.gdhealth_config_path.exists() {
        let body = fs::read_to_string(&context.gdhealth_config_path).with_context(|| {
            format!("failed to read {}", context.gdhealth_config_path.display())
        })?;
        body.parse::<toml::Value>().with_context(|| {
            format!("failed to parse {}", context.gdhealth_config_path.display())
        })?
    } else {
        render_gdhealth_config(config)
            .parse::<toml::Value>()
            .context("failed to render fallback gdhealth config")?
    };

    let table = doc
        .as_table_mut()
        .ok_or_else(|| anyhow!("gdhealth config root must be a TOML table"))?;
    let discovery = table
        .entry("discovery")
        .or_insert_with(|| toml::Value::Table(toml::map::Map::new()))
        .as_table_mut()
        .ok_or_else(|| anyhow!("gdhealth discovery section must be a TOML table"))?;
    let excludes = discovery
        .entry("exclude")
        .or_insert_with(|| toml::Value::Array(Vec::new()))
        .as_array_mut()
        .ok_or_else(|| anyhow!("gdhealth discovery.exclude must be an array"))?;

    for pattern in &config.paths.exclude {
        if !excludes
            .iter()
            .any(|value| value.as_str() == Some(pattern.as_str()))
        {
            excludes.push(toml::Value::String(pattern.clone()));
        }
    }

    Ok(doc)
}

pub fn missing_gdhealth_excludes(
    context: &ProjectContext,
    config: &GdsuiteConfig,
) -> Result<Vec<String>> {
    if !context.gdhealth_config_path.exists() {
        return Ok(Vec::new());
    }

    let body = fs::read_to_string(&context.gdhealth_config_path)
        .with_context(|| format!("failed to read {}", context.gdhealth_config_path.display()))?;
    let doc = body
        .parse::<toml::Value>()
        .with_context(|| format!("failed to parse {}", context.gdhealth_config_path.display()))?;
    let existing = doc
        .get("discovery")
        .and_then(|value| value.get("exclude"))
        .and_then(|value| value.as_array())
        .map(|values| {
            values
                .iter()
                .filter_map(|value| value.as_str())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    Ok(config
        .paths
        .exclude
        .iter()
        .filter(|pattern| !existing.iter().any(|value| value == &pattern.as_str()))
        .cloned()
        .collect())
}

fn test_patterns(config: &GdsuiteConfig) -> Vec<String> {
    let patterns = config
        .paths
        .scripts
        .iter()
        .filter(|pattern| pattern.contains("test"))
        .cloned()
        .collect::<Vec<_>>();
    if patterns.is_empty() {
        vec!["test/**/*.gd".to_string(), "**/*.test.gd".to_string()]
    } else {
        patterns
    }
}

fn group_patterns(config: &GdsuiteConfig, segment: &str) -> Vec<String> {
    let patterns = config
        .paths
        .scripts
        .iter()
        .filter(|pattern| pattern.contains(segment))
        .cloned()
        .collect::<Vec<_>>();
    if patterns.is_empty() {
        vec![format!("{segment}/**/*.gd"), format!("{segment}/**")]
    } else {
        patterns
    }
}

fn root_gameplay_patterns(config: &GdsuiteConfig) -> Vec<String> {
    let mut patterns = config
        .paths
        .scripts
        .iter()
        .filter(|pattern| {
            !pattern.contains("client") && !pattern.contains("server") && !pattern.contains("test")
        })
        .cloned()
        .collect::<Vec<_>>();

    if patterns.is_empty() {
        patterns.push("*.gd".to_string());
    }

    patterns.extend(
        config
            .paths
            .exclude
            .iter()
            .map(|pattern| format!("!{pattern}")),
    );
    patterns
}

fn toml_array(values: Vec<String>) -> String {
    let values = values
        .into_iter()
        .map(|value| format!("\"{}\"", value.replace('\\', "\\\\")))
        .collect::<Vec<_>>();
    format!("[{}]", values.join(", "))
}
