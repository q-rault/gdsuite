use std::{
    env, fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, anyhow};

use crate::{
    config::{
        GdhealthConfig, GdhealthPolicyConfig, GdhealthProjectConfig, GdhealthThresholdsConfig,
        GdsuiteConfig, GdtestConfig, Manifest, Versions,
    },
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

#[derive(Debug, Default, Clone)]
pub struct AddonInstallReport {
    pub installed_files: usize,
    pub skipped_modified: Vec<PathBuf>,
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
    let gdtest = merged_gdtest_config(context, config)?;
    fs::write(&context.gdtest_config_path, toml::to_string(&gdtest)?)
        .with_context(|| format!("failed to write {}", context.gdtest_config_path.display()))?;

    let gdhealth = merged_gdhealth_config(context, config)?;
    fs::write(&context.gdhealth_config_path, toml::to_string(&gdhealth)?)
        .with_context(|| format!("failed to write {}", context.gdhealth_config_path.display()))?;
    Ok(())
}

pub fn product_config_warnings(
    context: &ProjectContext,
    config: &GdsuiteConfig,
) -> Result<Vec<String>> {
    let mut warnings = Vec::new();

    let gdtest = missing_gdtest_suite_settings(context, config)?;
    if !gdtest.is_empty() {
        warnings.push(format!(
            "gdtest config drift from gdsuite.toml: {}; run `gdsuite migrate` or `gdsuite setup --sync-product-configs` to re-render the canonical subset",
            gdtest.join(", ")
        ));
    }

    let gdhealth = missing_gdhealth_settings(context, config)?;
    if !gdhealth.is_empty() {
        warnings.push(format!(
            "gdhealth config drift from gdsuite.toml: {}; run `gdsuite migrate` or `gdsuite setup --sync-product-configs` to re-render the canonical subset",
            gdhealth.join(", ")
        ));
    }

    Ok(warnings)
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

pub fn find_python_in_venv(context: &ProjectContext) -> Option<PathBuf> {
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

fn load_product_config_or_render(
    path: &Path,
    fallback: impl FnOnce() -> String,
) -> Result<toml::Value> {
    if path.exists() {
        let body = fs::read_to_string(path)
            .with_context(|| format!("failed to read {}", path.display()))?;
        return body
            .parse::<toml::Value>()
            .with_context(|| format!("failed to parse {}", path.display()));
    }

    fallback()
        .parse::<toml::Value>()
        .with_context(|| format!("failed to render fallback {}", path.display()))
}

fn render_gdtest_config(config: &GdsuiteConfig) -> String {
    let mut lines = vec![
        format!(
            "display_mode = {}",
            toml_string(&config.gdtest.display_mode)
        ),
        format!("harness = {}", toml_string(&config.gdtest.harness)),
        format!("patterns = {}", toml_array(config.gdtest.patterns.clone())),
        format!(
            "coverage_exclude = {}",
            toml_array(config.gdtest.coverage_exclude.clone())
        ),
    ];

    for (name, profile) in &config.gdtest.profile {
        lines.push(String::new());
        lines.push(format!("[profile.{name}]"));
        if let Some(reporter) = &profile.reporter {
            lines.push(format!("reporter = {}", toml_string(reporter)));
        }
        if let Some(fail_fast) = profile.fail_fast {
            lines.push(format!("fail_fast = {fail_fast}"));
        }
    }

    lines.join("\n") + "\n"
}

pub fn merged_gdtest_config(
    context: &ProjectContext,
    config: &GdsuiteConfig,
) -> Result<toml::Value> {
    let mut doc = load_product_config_or_render(&context.gdtest_config_path, || {
        render_gdtest_config(config)
    })?;

    let table = doc
        .as_table_mut()
        .ok_or_else(|| anyhow!("gdtest config root must be a TOML table"))?;
    table.insert(
        "display_mode".to_string(),
        toml::Value::String(config.gdtest.display_mode.clone()),
    );
    table.insert(
        "harness".to_string(),
        toml::Value::String(config.gdtest.harness.clone()),
    );
    table.insert(
        "patterns".to_string(),
        string_array_value(&config.gdtest.patterns),
    );
    table.insert(
        "coverage_exclude".to_string(),
        string_array_value(&config.gdtest.coverage_exclude),
    );
    table.remove("editor_plugin_dir");
    table.remove("install_editor");

    let profile_table = table
        .entry("profile")
        .or_insert_with(|| toml::Value::Table(toml::map::Map::new()))
        .as_table_mut()
        .ok_or_else(|| anyhow!("gdtest profile section must be a TOML table"))?;
    for (name, profile) in &config.gdtest.profile {
        let profile_doc = profile_table
            .entry(name.clone())
            .or_insert_with(|| toml::Value::Table(toml::map::Map::new()))
            .as_table_mut()
            .ok_or_else(|| anyhow!("gdtest profile entry must be a TOML table"))?;
        if let Some(reporter) = &profile.reporter {
            profile_doc.insert(
                "reporter".to_string(),
                toml::Value::String(reporter.clone()),
            );
        }
        if let Some(fail_fast) = profile.fail_fast {
            profile_doc.insert("fail_fast".to_string(), toml::Value::Boolean(fail_fast));
        }
    }

    Ok(doc)
}

pub fn missing_gdtest_suite_settings(
    context: &ProjectContext,
    config: &GdsuiteConfig,
) -> Result<Vec<String>> {
    if !context.gdtest_config_path.exists() {
        return Ok(Vec::new());
    }

    let body = fs::read_to_string(&context.gdtest_config_path)
        .with_context(|| format!("failed to read {}", context.gdtest_config_path.display()))?;
    let doc = body
        .parse::<toml::Value>()
        .with_context(|| format!("failed to parse {}", context.gdtest_config_path.display()))?;

    let expected: toml::Value = render_gdtest_config(config).parse()?;
    Ok(diff_owned_keys(
        &doc,
        &expected,
        &gdtest_owned_paths(config),
    ))
}

pub(crate) fn render_gdhealth_config(config: &GdsuiteConfig) -> String {
    let lines = vec![
        "[project]".to_string(),
        format!("root = {}", toml_string(&config.gdhealth.project.root)),
        String::new(),
        "[discovery]".to_string(),
        format!(
            "include = {}",
            toml_array(config.gdhealth.discovery.include.clone())
        ),
        format!(
            "exclude = {}",
            toml_array(config.gdhealth.discovery.exclude.clone())
        ),
        String::new(),
        "[groups]".to_string(),
        render_group_lines(&config.gdhealth.groups),
    ];

    let mut body = lines[..lines.len() - 1].join("\n");
    if !body.ends_with('\n') {
        body.push('\n');
    }
    body.push_str(&lines.last().cloned().unwrap_or_default());
    body.push('\n');
    append_gdhealth_thresholds_and_policy(&mut body, &config.gdhealth);
    body
}

pub fn merged_gdhealth_config(
    context: &ProjectContext,
    config: &GdsuiteConfig,
) -> Result<toml::Value> {
    let mut doc = load_product_config_or_render(&context.gdhealth_config_path, || {
        render_gdhealth_config(config)
    })?;

    let table = doc
        .as_table_mut()
        .ok_or_else(|| anyhow!("gdhealth config root must be a TOML table"))?;
    replace_gdhealth_owned_sections(table, &config.gdhealth, false);

    Ok(doc)
}

pub fn merged_gdhealth_overlay_config(
    context: &ProjectContext,
    config: &GdsuiteConfig,
) -> Result<toml::Value> {
    let mut doc = merged_gdhealth_config(context, config)?;
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
    if !excludes
        .iter()
        .any(|value| value.as_str() == Some(".gdsuite/**"))
    {
        excludes.push(toml::Value::String(".gdsuite/**".to_string()));
    }
    Ok(doc)
}

pub fn missing_gdhealth_settings(
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
    let expected: toml::Value = render_gdhealth_config(config).parse()?;
    Ok(diff_owned_keys(
        &doc,
        &expected,
        &gdhealth_owned_paths(config),
    ))
}

fn toml_array(values: Vec<String>) -> String {
    let values = values
        .into_iter()
        .map(|value| format!("\"{}\"", value.replace('\\', "\\\\")))
        .collect::<Vec<_>>();
    format!("[{}]", values.join(", "))
}

fn toml_string(value: &str) -> String {
    format!("\"{}\"", value.replace('\\', "\\\\"))
}

fn string_array_value(values: &[String]) -> toml::Value {
    toml::Value::Array(
        values
            .iter()
            .cloned()
            .map(toml::Value::String)
            .collect::<Vec<_>>(),
    )
}

fn render_group_lines(groups: &std::collections::BTreeMap<String, Vec<String>>) -> String {
    groups
        .iter()
        .map(|(name, patterns)| format!("{name} = {}", toml_array(patterns.clone())))
        .collect::<Vec<_>>()
        .join("\n")
}

fn append_gdhealth_thresholds_and_policy(body: &mut String, config: &GdhealthConfig) {
    if has_thresholds(&config.thresholds) {
        body.push_str("\n[thresholds]\n");
        append_optional_threshold(body, "file_lines_warn", config.thresholds.file_lines_warn);
        append_optional_threshold(
            body,
            "file_lines_critical",
            config.thresholds.file_lines_critical,
        );
        append_optional_threshold(
            body,
            "function_lines_warn",
            config.thresholds.function_lines_warn,
        );
        append_optional_threshold(
            body,
            "function_lines_critical",
            config.thresholds.function_lines_critical,
        );
        append_optional_threshold(body, "cyclomatic_warn", config.thresholds.cyclomatic_warn);
        append_optional_threshold(
            body,
            "cyclomatic_critical",
            config.thresholds.cyclomatic_critical,
        );
        append_optional_threshold(body, "nesting_warn", config.thresholds.nesting_warn);
        append_optional_threshold(body, "nesting_critical", config.thresholds.nesting_critical);
        append_optional_threshold(body, "parameters_warn", config.thresholds.parameters_warn);
    }

    if !config.policy.allow_cycles.is_empty() || !config.policy.forbidden_dependencies.is_empty() {
        body.push_str("\n[policy]\n");
        body.push_str(&format!(
            "allow_cycles = {}\n",
            toml_array(config.policy.allow_cycles.clone())
        ));
        for rule in &config.policy.forbidden_dependencies {
            body.push_str("\n[[policy.forbidden_dependencies]]\n");
            body.push_str(&format!("from = {}\n", toml_string(&rule.from)));
            body.push_str(&format!("to = {}\n", toml_string(&rule.to)));
            body.push_str(&format!("severity = {}\n", toml_string(&rule.severity)));
        }
    }
}

fn append_optional_threshold(body: &mut String, key: &str, value: Option<usize>) {
    if let Some(value) = value {
        body.push_str(&format!("{key} = {value}\n"));
    }
}

fn has_thresholds(config: &GdhealthThresholdsConfig) -> bool {
    config.file_lines_warn.is_some()
        || config.file_lines_critical.is_some()
        || config.function_lines_warn.is_some()
        || config.function_lines_critical.is_some()
        || config.cyclomatic_warn.is_some()
        || config.cyclomatic_critical.is_some()
        || config.nesting_warn.is_some()
        || config.nesting_critical.is_some()
        || config.parameters_warn.is_some()
}

fn gdtest_owned_paths(config: &GdsuiteConfig) -> Vec<String> {
    let mut keys = vec![
        "display_mode".to_string(),
        "harness".to_string(),
        "patterns".to_string(),
        "coverage_exclude".to_string(),
    ];
    for (name, profile) in &config.gdtest.profile {
        if profile.reporter.is_some() {
            keys.push(format!("profile.{name}.reporter"));
        }
        if profile.fail_fast.is_some() {
            keys.push(format!("profile.{name}.fail_fast"));
        }
    }
    keys
}

fn gdhealth_owned_paths(config: &GdsuiteConfig) -> Vec<String> {
    let mut keys = vec![
        "project.root".to_string(),
        "discovery.include".to_string(),
        "discovery.exclude".to_string(),
    ];
    for name in config.gdhealth.groups.keys() {
        keys.push(format!("groups.{name}"));
    }
    for name in [
        "file_lines_warn",
        "file_lines_critical",
        "function_lines_warn",
        "function_lines_critical",
        "cyclomatic_warn",
        "cyclomatic_critical",
        "nesting_warn",
        "nesting_critical",
        "parameters_warn",
    ] {
        keys.push(format!("thresholds.{name}"));
    }
    keys.push("policy.allow_cycles".to_string());
    if !config.gdhealth.policy.forbidden_dependencies.is_empty() {
        keys.push("policy.forbidden_dependencies".to_string());
    }
    keys
}

fn diff_owned_keys(actual: &toml::Value, expected: &toml::Value, keys: &[String]) -> Vec<String> {
    keys.iter()
        .filter(|path| value_at_path(actual, path) != value_at_path(expected, path))
        .cloned()
        .collect()
}

fn value_at_path<'a>(value: &'a toml::Value, path: &str) -> Option<&'a toml::Value> {
    let mut current = value;
    for segment in path.split('.') {
        current = current.get(segment)?;
    }
    Some(current)
}

fn replace_gdhealth_owned_sections(
    table: &mut toml::map::Map<String, toml::Value>,
    config: &GdhealthConfig,
    include_runtime_overlay: bool,
) {
    let mut project = toml::map::Map::new();
    project.insert(
        "root".to_string(),
        toml::Value::String(config.project.root.clone()),
    );
    table.insert("project".to_string(), toml::Value::Table(project));

    let mut discovery = toml::map::Map::new();
    discovery.insert(
        "include".to_string(),
        string_array_value(&config.discovery.include),
    );
    let mut exclude = config.discovery.exclude.clone();
    if include_runtime_overlay && !exclude.iter().any(|pattern| pattern == ".gdsuite/**") {
        exclude.push(".gdsuite/**".to_string());
    }
    discovery.insert("exclude".to_string(), string_array_value(&exclude));
    table.insert("discovery".to_string(), toml::Value::Table(discovery));

    let groups = table
        .entry("groups")
        .or_insert_with(|| toml::Value::Table(toml::map::Map::new()))
        .as_table_mut()
        .expect("groups must be a TOML table");
    for (name, patterns) in &config.groups {
        groups.insert(name.clone(), string_array_value(patterns));
    }

    let mut thresholds = toml::map::Map::new();
    insert_optional_usize(
        &mut thresholds,
        "file_lines_warn",
        config.thresholds.file_lines_warn,
    );
    insert_optional_usize(
        &mut thresholds,
        "file_lines_critical",
        config.thresholds.file_lines_critical,
    );
    insert_optional_usize(
        &mut thresholds,
        "function_lines_warn",
        config.thresholds.function_lines_warn,
    );
    insert_optional_usize(
        &mut thresholds,
        "function_lines_critical",
        config.thresholds.function_lines_critical,
    );
    insert_optional_usize(
        &mut thresholds,
        "cyclomatic_warn",
        config.thresholds.cyclomatic_warn,
    );
    insert_optional_usize(
        &mut thresholds,
        "cyclomatic_critical",
        config.thresholds.cyclomatic_critical,
    );
    insert_optional_usize(
        &mut thresholds,
        "nesting_warn",
        config.thresholds.nesting_warn,
    );
    insert_optional_usize(
        &mut thresholds,
        "nesting_critical",
        config.thresholds.nesting_critical,
    );
    insert_optional_usize(
        &mut thresholds,
        "parameters_warn",
        config.thresholds.parameters_warn,
    );
    if thresholds.is_empty() {
        table.remove("thresholds");
    } else {
        table.insert("thresholds".to_string(), toml::Value::Table(thresholds));
    }

    let mut policy = toml::map::Map::new();
    policy.insert(
        "allow_cycles".to_string(),
        string_array_value(&config.policy.allow_cycles),
    );
    if config.policy.forbidden_dependencies.is_empty() {
        policy.remove("forbidden_dependencies");
    } else {
        let rules = config
            .policy
            .forbidden_dependencies
            .iter()
            .map(|rule| {
                let mut doc = toml::map::Map::new();
                doc.insert("from".to_string(), toml::Value::String(rule.from.clone()));
                doc.insert("to".to_string(), toml::Value::String(rule.to.clone()));
                doc.insert(
                    "severity".to_string(),
                    toml::Value::String(rule.severity.clone()),
                );
                toml::Value::Table(doc)
            })
            .collect::<Vec<_>>();
        policy.insert(
            "forbidden_dependencies".to_string(),
            toml::Value::Array(rules),
        );
    }
    table.insert("policy".to_string(), toml::Value::Table(policy));
}

fn insert_optional_usize(
    table: &mut toml::map::Map<String, toml::Value>,
    key: &str,
    value: Option<usize>,
) {
    if let Some(value) = value {
        table.insert(key.to_string(), toml::Value::Integer(value as i64));
    }
}

pub fn import_product_configs_into_canonical(
    context: &ProjectContext,
    config: &mut GdsuiteConfig,
) -> Result<()> {
    if context.gdtest_config_path.exists() {
        let body = fs::read_to_string(&context.gdtest_config_path)
            .with_context(|| format!("failed to read {}", context.gdtest_config_path.display()))?;
        let value: toml::Value = body
            .parse()
            .with_context(|| format!("failed to parse {}", context.gdtest_config_path.display()))?;
        config.gdtest = import_gdtest_config(value, &config.gdtest);
    }

    if context.gdhealth_config_path.exists() {
        let body = fs::read_to_string(&context.gdhealth_config_path).with_context(|| {
            format!("failed to read {}", context.gdhealth_config_path.display())
        })?;
        let value: toml::Value = body.parse().with_context(|| {
            format!("failed to parse {}", context.gdhealth_config_path.display())
        })?;
        config.gdhealth = import_gdhealth_config(value, &config.gdhealth);
    }

    Ok(())
}

fn import_gdtest_config(value: toml::Value, defaults: &GdtestConfig) -> GdtestConfig {
    let mut config = defaults.clone();
    if let Some(display_mode) = value.get("display_mode").and_then(|value| value.as_str()) {
        config.display_mode = display_mode.to_string();
    }
    if let Some(harness) = value.get("harness").and_then(|value| value.as_str()) {
        config.harness = harness.to_string();
    }
    if let Some(editor_plugin_dir) = value
        .get("editor_plugin_dir")
        .and_then(|value| value.as_str())
    {
        config.editor_plugin_dir = editor_plugin_dir.to_string();
    }
    if let Some(install_editor) = value
        .get("install_editor")
        .and_then(|value| value.as_bool())
    {
        config.install_editor = install_editor;
    }
    if let Some(patterns) = array_of_strings(value.get("patterns")) {
        config.patterns = patterns;
    }
    if let Some(exclude) = array_of_strings(value.get("coverage_exclude")) {
        config.coverage_exclude = exclude;
    }
    if let Some(profile) = value.get("profile").and_then(|value| value.as_table()) {
        for (name, profile_value) in profile {
            let entry = config.profile.entry(name.clone()).or_default();
            if let Some(reporter) = profile_value
                .get("reporter")
                .and_then(|value| value.as_str())
            {
                entry.reporter = Some(reporter.to_string());
            }
            if let Some(fail_fast) = profile_value
                .get("fail_fast")
                .and_then(|value| value.as_bool())
            {
                entry.fail_fast = Some(fail_fast);
            }
        }
    }
    config
}

fn import_gdhealth_config(value: toml::Value, defaults: &GdhealthConfig) -> GdhealthConfig {
    let mut config = defaults.clone();
    if let Some(project) = value.get("project").and_then(|value| value.as_table())
        && let Some(root) = project.get("root").and_then(|value| value.as_str())
    {
        config.project = GdhealthProjectConfig {
            root: root.to_string(),
        };
    }
    if let Some(discovery) = value.get("discovery").and_then(|value| value.as_table()) {
        let mut imported = config.discovery.clone();
        if let Some(include) = array_of_strings(discovery.get("include")) {
            imported.include = include;
        }
        if let Some(exclude) = array_of_strings(discovery.get("exclude")) {
            imported.exclude = exclude
                .into_iter()
                .filter(|pattern| pattern != ".gdsuite/**")
                .collect();
        }
        config.discovery = imported;
    }
    if let Some(groups) = value.get("groups").and_then(|value| value.as_table()) {
        config.groups = groups
            .iter()
            .filter_map(|(name, value)| {
                array_of_strings(Some(value)).map(|patterns| (name.clone(), patterns))
            })
            .collect();
    }
    if let Some(thresholds) = value.get("thresholds").and_then(|value| value.as_table()) {
        config.thresholds = GdhealthThresholdsConfig {
            file_lines_warn: usize_value(thresholds.get("file_lines_warn")),
            file_lines_critical: usize_value(thresholds.get("file_lines_critical")),
            function_lines_warn: usize_value(thresholds.get("function_lines_warn")),
            function_lines_critical: usize_value(thresholds.get("function_lines_critical")),
            cyclomatic_warn: usize_value(thresholds.get("cyclomatic_warn")),
            cyclomatic_critical: usize_value(thresholds.get("cyclomatic_critical")),
            nesting_warn: usize_value(thresholds.get("nesting_warn")),
            nesting_critical: usize_value(thresholds.get("nesting_critical")),
            parameters_warn: usize_value(thresholds.get("parameters_warn")),
        };
    }
    if let Some(policy) = value.get("policy").and_then(|value| value.as_table()) {
        let allow_cycles = array_of_strings(policy.get("allow_cycles")).unwrap_or_default();
        let forbidden_dependencies = policy
            .get("forbidden_dependencies")
            .and_then(|value| value.as_array())
            .map(|values| {
                values
                    .iter()
                    .filter_map(|value| {
                        let table = value.as_table()?;
                        Some(crate::config::GdhealthForbiddenDependency {
                            from: table.get("from")?.as_str()?.to_string(),
                            to: table.get("to")?.as_str()?.to_string(),
                            severity: table.get("severity")?.as_str()?.to_string(),
                        })
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        config.policy = GdhealthPolicyConfig {
            allow_cycles,
            forbidden_dependencies,
        };
    }
    config
}

fn array_of_strings(value: Option<&toml::Value>) -> Option<Vec<String>> {
    value.and_then(|value| {
        value.as_array().map(|values| {
            values
                .iter()
                .filter_map(|value| value.as_str().map(ToString::to_string))
                .collect::<Vec<_>>()
        })
    })
}

fn usize_value(value: Option<&toml::Value>) -> Option<usize> {
    value
        .and_then(|value| value.as_integer())
        .map(|value| value as usize)
}

pub fn install_gdtest_addons(
    context: &ProjectContext,
    config: &GdsuiteConfig,
) -> Result<AddonInstallReport> {
    let source_root = context.root.join(&config.tools.gdtest_source);
    let runtime_source = source_root.join("addons").join("gdtest");
    if !runtime_source.exists() {
        return Err(anyhow!(
            "gdtest runtime addon source missing at {}",
            runtime_source.display()
        ));
    }

    let mut report = AddonInstallReport::default();
    let runtime_target_dir = harness_parent_dir(&config.gdtest.harness);
    let runtime_res_dir = res_dir_from_rel_path(&runtime_target_dir);
    install_addon_dir(
        &runtime_source,
        &context.root.join(&runtime_target_dir),
        &mut report,
        |relative, contents| rewrite_runtime_asset(relative, contents, &runtime_res_dir),
    )?;

    if config.gdtest.install_editor {
        let editor_source = source_root.join("addons").join("gdtest_editor");
        if !editor_source.exists() {
            return Err(anyhow!(
                "gdtest editor addon source missing at {}",
                editor_source.display()
            ));
        }
        let editor_res_dir = res_dir_from_rel_path(Path::new(&config.gdtest.editor_plugin_dir));
        install_addon_dir(
            &editor_source,
            &context.root.join(&config.gdtest.editor_plugin_dir),
            &mut report,
            |relative, contents| rewrite_editor_asset(relative, contents, &editor_res_dir),
        )?;
    }

    Ok(report)
}

fn install_addon_dir(
    source_dir: &Path,
    target_dir: &Path,
    report: &mut AddonInstallReport,
    rewrite: impl Fn(&Path, &str) -> String + Copy,
) -> Result<()> {
    for entry in fs::read_dir(source_dir)
        .with_context(|| format!("failed to read addon source {}", source_dir.display()))?
    {
        let entry = entry?;
        let source_path = entry.path();
        let relative = source_path.strip_prefix(source_dir).unwrap();
        let target_path = target_dir.join(relative);

        if entry.file_type()?.is_dir() {
            fs::create_dir_all(&target_path)
                .with_context(|| format!("failed to create {}", target_path.display()))?;
            install_addon_dir(&source_path, &target_path, report, rewrite)?;
            continue;
        }

        let contents = fs::read_to_string(&source_path)
            .with_context(|| format!("failed to read {}", source_path.display()))?;
        let rewritten = rewrite(relative, &contents);
        if let Some(parent) = target_path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("failed to create {}", parent.display()))?;
        }

        if target_path.exists() {
            let current = fs::read_to_string(&target_path)
                .with_context(|| format!("failed to read {}", target_path.display()))?;
            if current == rewritten {
                continue;
            }
            report.skipped_modified.push(target_path);
            continue;
        }

        fs::write(&target_path, rewritten)
            .with_context(|| format!("failed to write {}", target_path.display()))?;
        report.installed_files += 1;
    }

    Ok(())
}

fn rewrite_runtime_asset(relative: &Path, contents: &str, runtime_res_dir: &str) -> String {
    let runtime_prefix = join_res_path(runtime_res_dir, "gdunit4_chain.gd");
    let compat_prefix = join_res_path(runtime_res_dir, "gdunit4_compat.gd");
    match relative.to_string_lossy().replace('\\', "/").as_str() {
        "gdtest_worker.gd" => contents
            .replace("res://addons/gdtest/gdunit4_chain.gd", &runtime_prefix)
            .replace("res://addons/gdtest/gdunit4_compat.gd", &compat_prefix),
        "gdunit4_compat.gd" => {
            contents.replace("res://addons/gdtest/gdunit4_chain.gd", &runtime_prefix)
        }
        _ => contents.to_string(),
    }
}

fn rewrite_editor_asset(relative: &Path, contents: &str, editor_res_dir: &str) -> String {
    match relative.to_string_lossy().replace('\\', "/").as_str() {
        "plugin.gd" => contents.replace(
            "res://addons/gdtest_editor/test_panel.gd",
            &join_res_path(editor_res_dir, "test_panel.gd"),
        ),
        _ => contents.to_string(),
    }
}

fn harness_parent_dir(harness: &str) -> PathBuf {
    Path::new(harness)
        .parent()
        .map(Path::to_path_buf)
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or_else(|| PathBuf::from("."))
}

fn res_dir_from_rel_path(path: &Path) -> String {
    let normalized = path.display().to_string().replace('\\', "/");
    if normalized == "." || normalized.is_empty() {
        "res://".to_string()
    } else {
        format!("res://{}", normalized.trim_end_matches('/'))
    }
}

fn join_res_path(base: &str, file: &str) -> String {
    if base.ends_with('/') {
        format!("{base}{file}")
    } else {
        format!("{base}/{file}")
    }
}
