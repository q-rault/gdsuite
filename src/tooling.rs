use std::{
    env, fs, io,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, anyhow};
use flate2::read::GzDecoder;
use reqwest::blocking::Client;
use reqwest::header::{AUTHORIZATION, HeaderMap, HeaderValue, USER_AGENT};
use serde::Deserialize;
use tar::Archive;
use zip::ZipArchive;

use crate::{
    config::{
        CompatibilityVerdict, GdhealthConfig, GdhealthPolicyConfig, GdhealthProjectConfig,
        GdhealthThresholdsConfig, GdsuiteConfig, GdtestConfig, InstalledContractsState,
        InstalledState, InstalledToolState, Manifest, ReleaseManifest, ToolAcquireSpec,
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
    pub installation_path: PathBuf,
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

#[derive(Debug, Clone)]
pub struct InstallPlan {
    pub gdtoolkit: ToolAcquireSpec,
    pub gdtest: ToolAcquireSpec,
    pub gdhealth: ToolAcquireSpec,
}

#[derive(Debug, Clone)]
pub struct InstallOutcome {
    pub manifest: Manifest,
    pub requested: InstallPlan,
}

#[derive(Debug, Clone)]
pub struct CompatibilityReport {
    pub gdtoolkit: ToolCompatibility,
    pub gdtest: ToolCompatibility,
    pub gdhealth: ToolCompatibility,
}

#[derive(Debug, Clone)]
pub struct ToolCompatibility {
    pub name: &'static str,
    pub requested: String,
    pub resolved_version: Option<String>,
    pub verdict: CompatibilityVerdict,
    pub reason: String,
    pub extra: Vec<(String, String)>,
}

#[derive(Debug, Clone)]
struct ProbedToolchain {
    gdtoolkit_version: Option<String>,
    gdtest_version: Option<String>,
    gdtest_harness_version: Option<String>,
    gdhealth_version: Option<String>,
    gdhealth_schema_version: Option<u32>,
}

#[derive(Debug, Default, Clone)]
pub struct AddonInstallReport {
    pub installed_files: usize,
    pub skipped_modified: Vec<PathBuf>,
}

#[derive(Debug, Deserialize)]
struct GithubReleaseResponse {
    assets: Vec<GithubReleaseAsset>,
}

#[derive(Debug, Deserialize)]
struct GithubReleaseAsset {
    name: String,
    url: String,
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
            installation_path: state_dir.join("installation.json"),
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

pub fn load_dotenv_if_present(context: &ProjectContext) -> Result<()> {
    let path = context.root.join(".env");
    if !path.exists() {
        return Ok(());
    }

    let body =
        fs::read_to_string(&path).with_context(|| format!("failed to read {}", path.display()))?;
    for (index, raw_line) in body.lines().enumerate() {
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        let line = line.strip_prefix("export ").unwrap_or(line).trim();
        let Some((key, raw_value)) = line.split_once('=') else {
            return Err(anyhow!(
                "invalid .env entry at {}:{}",
                path.display(),
                index + 1
            ));
        };

        let key = key.trim();
        if key.is_empty() {
            return Err(anyhow!(
                "invalid .env entry at {}:{}",
                path.display(),
                index + 1
            ));
        }
        if env::var_os(key).is_some() {
            continue;
        }

        let value = parse_dotenv_value(raw_value.trim()).ok_or_else(|| {
            anyhow!("invalid .env value at {}:{}", path.display(), index + 1)
        })?;
        // Safe here because keys/values come from a user-controlled env file in the project root.
        unsafe {
            env::set_var(key, value);
        }
    }

    Ok(())
}

fn parse_dotenv_value(raw: &str) -> Option<String> {
    if raw.len() >= 2 && raw.starts_with('"') && raw.ends_with('"') {
        let inner = &raw[1..raw.len() - 1];
        return Some(
            inner
                .replace("\\n", "\n")
                .replace("\\r", "\r")
                .replace("\\t", "\t")
                .replace("\\\"", "\"")
                .replace("\\\\", "\\"),
        );
    }

    if raw.len() >= 2 && raw.starts_with('\'') && raw.ends_with('\'') {
        return Some(raw[1..raw.len() - 1].to_string());
    }

    let value = raw
        .split_once('#')
        .map(|(head, _)| head)
        .unwrap_or(raw)
        .trim()
        .to_string();
    Some(value)
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

pub fn save_installation_state(context: &ProjectContext, state: &InstalledState) -> Result<()> {
    state.write(&context.installation_path)
}

pub fn release_manifest() -> &'static ReleaseManifest {
    ReleaseManifest::bundled()
}

pub fn install_plan(config: &GdsuiteConfig) -> InstallPlan {
    let release = release_manifest();
    InstallPlan {
        gdtoolkit: if config.tools.gdtoolkit == default_gdtoolkit_requested() {
            release.tools.gdtoolkit.acquire.clone()
        } else {
            ToolAcquireSpec::Pip {
                package: "gdtoolkit".to_string(),
                spec: config.tools.gdtoolkit.clone(),
            }
        },
        gdtest: if config.tools.gdtest_source == default_gdtest_requested() {
            release.tools.gdtest.acquire.clone()
        } else {
            parse_tool_source_spec(&config.tools.gdtest_source, "gdtest")
        },
        gdhealth: if config.tools.gdhealth_source == default_gdhealth_requested() {
            release.tools.gdhealth.acquire.clone()
        } else {
            parse_tool_source_spec(&config.tools.gdhealth_source, "gdhealth")
        },
    }
}

fn default_gdtoolkit_requested() -> String {
    release_manifest()
        .tools
        .gdtoolkit
        .acquire
        .gdtoolkit_spec()
        .expect("gdtoolkit release manifest must use pip")
        .to_string()
}

fn default_gdtest_requested() -> String {
    release_manifest().tools.gdtest.acquire.render_requested()
}

fn default_gdhealth_requested() -> String {
    release_manifest().tools.gdhealth.acquire.render_requested()
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
        if let Some(path) = resolve_command_candidate(&candidate) {
            return Ok(path);
        }
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

fn resolve_command_candidate(candidate: &Path) -> Option<PathBuf> {
    if candidate.is_absolute() || candidate.parent().is_some() {
        return None;
    }

    let command = candidate.to_str()?;
    which::which(command).ok()
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
) -> Result<InstallOutcome> {
    let requested = install_plan(config);
    let python = ensure_venv(context)?;
    install_gdtoolkit(&python, &requested.gdtoolkit, context)?;
    let gdtest = install_tool(context, &requested.gdtest, "gdtest")?;
    let gdhealth = install_tool(context, &requested.gdhealth, "gdhealth")?;
    let gdformat =
        find_binary(context, &["gdformat"]).ok_or_else(|| anyhow!("gdformat install missing"))?;
    let gdlint =
        find_binary(context, &["gdlint"]).ok_or_else(|| anyhow!("gdlint install missing"))?;

    Ok(InstallOutcome {
        manifest: Manifest {
            godot_exe: Some(godot_exe.to_path_buf()),
            python: Some(python),
            gdformat: Some(gdformat),
            gdlint: Some(gdlint),
            gdtest: Some(gdtest),
            gdhealth: Some(gdhealth),
        },
        requested,
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

fn install_gdtoolkit(
    python: &Path,
    requested: &ToolAcquireSpec,
    context: &ProjectContext,
) -> Result<()> {
    let ToolAcquireSpec::Pip { package, spec } = requested else {
        return Err(anyhow!("gdtoolkit must use a pip acquisition spec"));
    };
    let package = format!("{package}{spec}");
    let args = vec![
        "-m".to_string(),
        "pip".to_string(),
        "install".to_string(),
        package,
    ];
    process::run_checked(python, &args, &context.root)?;
    Ok(())
}

fn install_tool(
    context: &ProjectContext,
    requested: &ToolAcquireSpec,
    name: &str,
) -> Result<PathBuf> {
    let cargo = which::which("cargo").context("cargo is required for setup")?;
    let root = context.gdsuite_dir.join("tools").join(name);
    fs::create_dir_all(&root)
        .with_context(|| format!("failed to create tool root {}", root.display()))?;
    match requested {
        ToolAcquireSpec::Path { path } => {
            let package_root = resolve_package_root(&context.root.join(path), name)?;
            let args = vec![
                "install".to_string(),
                "--path".to_string(),
                package_root.display().to_string(),
                "--root".to_string(),
                root.display().to_string(),
            ];
            process::run_checked(&cargo, &args, &context.root)?;
        }
        ToolAcquireSpec::Crate {
            crate_name,
            version,
        } => {
            let source_root = tool_source_cache_dir(context, name);
            reset_dir(&source_root)?;
            let archive_path = download_file(
                &crate_download_url(crate_name, version),
                &tool_download_cache_dir(context, name)
                    .join(format!("{crate_name}-{version}.crate")),
            )?;
            unpack_archive(&archive_path, &source_root)?;

            let args = vec![
                "install".to_string(),
                crate_name.clone(),
                "--version".to_string(),
                version.clone(),
                "--root".to_string(),
                root.display().to_string(),
            ];
            process::run_checked(&cargo, &args, &context.root)?;
        }
        ToolAcquireSpec::Git { url, rev, subdir } => {
            let source_root = tool_source_cache_dir(context, name);
            if source_root.exists() {
                fs::remove_dir_all(&source_root)
                    .with_context(|| format!("failed to remove {}", source_root.display()))?;
            }
            clone_git_source(url, rev, &source_root)?;
            let package_root = subdir
                .as_ref()
                .map(|value| source_root.join(value))
                .unwrap_or_else(|| source_root.clone());
            if !package_root.exists() {
                return Err(anyhow!(
                    "git source subdir does not exist for {}: {}",
                    name,
                    package_root.display()
                ));
            }
            let package_root = resolve_package_root(&package_root, name)?;
            let args = vec![
                "install".to_string(),
                "--path".to_string(),
                package_root.display().to_string(),
                "--root".to_string(),
                root.display().to_string(),
            ];
            process::run_checked(&cargo, &args, &context.root)?;
        }
        ToolAcquireSpec::GithubRelease { url } => {
            let download_name = file_name_from_url(url).unwrap_or_else(|| name.to_string());
            let archive_path = download_file(
                url,
                &tool_download_cache_dir(context, name).join(download_name),
            )?;
            if is_archive_path(&archive_path) {
                install_binary_archive(&archive_path, &root, name)?;
            } else {
                install_downloaded_binary(&archive_path, &root, name)?;
            }
        }
        ToolAcquireSpec::GithubReleaseRepo { owner, repo, tag } => {
            let (target, extension) = current_release_asset_target()?;
            let asset_name = format!("{name}-{tag}-{target}.{extension}");
            let archive_path = if github_token().is_some() {
                download_github_release_asset(
                    owner,
                    repo,
                    tag,
                    &asset_name,
                    &tool_download_cache_dir(context, name).join(&asset_name),
                )?
            } else {
                let url = github_release_asset_url(owner, repo, tag, name)?;
                let download_name = file_name_from_url(&url).unwrap_or_else(|| name.to_string());
                download_file(&url, &tool_download_cache_dir(context, name).join(download_name))?
            };
            install_binary_archive(&archive_path, &root, name)?;
        }
        ToolAcquireSpec::Pip { .. } => {
            return Err(anyhow!("{name} cannot use a pip acquisition spec"));
        }
    }

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

fn parse_tool_source_spec(source: &str, default_name: &str) -> ToolAcquireSpec {
    if let Some(rest) = source.strip_prefix("crate:")
        && let Some((name, version)) = rest.split_once('@')
        && !name.is_empty()
        && !version.is_empty()
    {
        return ToolAcquireSpec::Crate {
            crate_name: name.to_string(),
            version: version.to_string(),
        };
    }

    if let Some(url) = source.strip_prefix("github:")
        && !url.is_empty()
    {
        if !url.starts_with("http://")
            && !url.starts_with("https://")
            && let Some((repo_path, tag)) = url.rsplit_once('@')
            && let Some((owner, repo)) = repo_path.split_once('/')
            && !owner.is_empty()
            && !repo.is_empty()
            && !tag.is_empty()
        {
            return ToolAcquireSpec::GithubReleaseRepo {
                owner: owner.to_string(),
                repo: repo.to_string(),
                tag: tag.to_string(),
            };
        }

        return ToolAcquireSpec::GithubRelease {
            url: url.to_string(),
        };
    }

    if let Some(rest) = source.strip_prefix("git:") {
        let (url, query) = rest.split_once('?').unwrap_or((rest, ""));
        let mut rev = None;
        let mut subdir = None;
        for pair in query.split('&').filter(|segment| !segment.is_empty()) {
            if let Some((key, value)) = pair.split_once('=') {
                match key {
                    "rev" => rev = Some(value.to_string()),
                    "subdir" => subdir = Some(value.to_string()),
                    _ => {}
                }
            }
        }
        if !url.is_empty()
            && let Some(rev) = rev
        {
            return ToolAcquireSpec::Git {
                url: url.to_string(),
                rev,
                subdir,
            };
        }
    }

    if let Some(path) = source.strip_prefix("path:")
        && !path.is_empty()
    {
        return ToolAcquireSpec::Path {
            path: path.to_string(),
        };
    }

    if source.contains("://") {
        return ToolAcquireSpec::Git {
            url: source.to_string(),
            rev: "HEAD".to_string(),
            subdir: Some(default_name.to_string()),
        };
    }

    ToolAcquireSpec::Path {
        path: source.to_string(),
    }
}

fn resolved_source_root(context: &ProjectContext, source: &str, name: &str) -> Result<PathBuf> {
    match parse_tool_source_spec(source, name) {
        ToolAcquireSpec::Path { path } => Ok(context.root.join(path)),
        ToolAcquireSpec::Crate { .. }
        | ToolAcquireSpec::GithubRelease { .. }
        | ToolAcquireSpec::GithubReleaseRepo { .. }
        | ToolAcquireSpec::Git { .. } => {
            let source_root = tool_source_cache_dir(context, name);
            if !source_root.exists() {
                return Err(anyhow!(
                    "tool source cache missing for {}; run `gdsuite setup` to install the toolchain",
                    name
                ));
            }
            normalize_extracted_root(&source_root)
        }
        ToolAcquireSpec::Pip { .. } => Err(anyhow!("pip source is not valid for {}", name)),
    }
}

fn tool_source_cache_dir(context: &ProjectContext, name: &str) -> PathBuf {
    context.gdsuite_dir.join("tools").join(name).join("source")
}

fn tool_download_cache_dir(context: &ProjectContext, name: &str) -> PathBuf {
    context
        .gdsuite_dir
        .join("tools")
        .join(name)
        .join("downloads")
}

fn crate_download_url(name: &str, version: &str) -> String {
    if let Ok(base) = env::var("GDSUITE_CRATES_DOWNLOAD_BASE_URL") {
        let base = base.trim_end_matches('/');
        return format!("{base}/{name}/{version}/download");
    }

    format!("https://crates.io/api/v1/crates/{name}/{version}/download")
}

fn download_file(url: &str, destination: &Path) -> Result<PathBuf> {
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create download dir {}", parent.display()))?;
    }

    let client = http_client()?;
    let mut response = client
        .get(url)
        .send()
        .with_context(|| format!("failed to download {url}"))?
        .error_for_status()
        .with_context(|| format!("failed to download {url}"))?;
    let mut file = fs::File::create(destination)
        .with_context(|| format!("failed to create {}", destination.display()))?;
    io::copy(&mut response, &mut file)
        .with_context(|| format!("failed to write {}", destination.display()))?;
    Ok(destination.to_path_buf())
}

fn download_github_release_asset(
    owner: &str,
    repo: &str,
    tag: &str,
    asset_name: &str,
    destination: &Path,
) -> Result<PathBuf> {
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create download dir {}", parent.display()))?;
    }

    let client = http_client()?;
    let release_url = format!(
        "{}/repos/{owner}/{repo}/releases/tags/{tag}",
        github_api_base_url()
    );
    let release_body = client
        .get(&release_url)
        .header("Accept", "application/vnd.github+json")
        .send()
        .with_context(|| format!("failed to query GitHub release metadata at {release_url}"))?
        .error_for_status()
        .with_context(|| format!("failed to query GitHub release metadata at {release_url}"))?
        .text()
        .with_context(|| format!("failed to read GitHub release metadata from {release_url}"))?;
    let release: GithubReleaseResponse = serde_json::from_str(&release_body)
        .with_context(|| format!("failed to parse GitHub release metadata from {release_url}"))?;

    let asset = release
        .assets
        .into_iter()
        .find(|asset| asset.name == asset_name)
        .ok_or_else(|| {
            anyhow!(
                "GitHub release {owner}/{repo}@{tag} did not contain expected asset {asset_name}"
            )
        })?;

    let mut response = client
        .get(&asset.url)
        .header("Accept", "application/octet-stream")
        .send()
        .with_context(|| format!("failed to download GitHub release asset {}", asset.name))?
        .error_for_status()
        .with_context(|| format!("failed to download GitHub release asset {}", asset.name))?;
    let mut file = fs::File::create(destination)
        .with_context(|| format!("failed to create {}", destination.display()))?;
    io::copy(&mut response, &mut file)
        .with_context(|| format!("failed to write {}", destination.display()))?;
    Ok(destination.to_path_buf())
}

fn http_client() -> Result<Client> {
    let mut headers = HeaderMap::new();
    headers.insert(
        USER_AGENT,
        HeaderValue::from_static(concat!(env!("CARGO_PKG_NAME"), "/", env!("CARGO_PKG_VERSION"))),
    );
    if let Some(token) = github_token() {
        let value = HeaderValue::from_str(&format!("Bearer {token}"))
            .context("GDSUITE_GITHUB_TOKEN contains invalid header characters")?;
        headers.insert(AUTHORIZATION, value);
    }

    Client::builder()
        .default_headers(headers)
        .redirect(reqwest::redirect::Policy::limited(10))
        .build()
        .context("failed to initialize HTTP client")
}

fn github_token() -> Option<String> {
    let token = env::var("GDSUITE_GITHUB_TOKEN").ok()?;
    let token = token.trim();
    if token.is_empty() {
        None
    } else {
        Some(token.to_string())
    }
}

fn clone_git_source(url: &str, rev: &str, destination: &Path) -> Result<()> {
    let git = which::which("git").context("git is required for git-based setup sources")?;
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }

    let clone_args = vec![
        "clone".to_string(),
        url.to_string(),
        command_path(destination),
    ];
    process::run_checked(&git, &clone_args, Path::new("."))?;

    let fetch_args = vec![
        "-C".to_string(),
        command_path(destination),
        "fetch".to_string(),
        "--depth".to_string(),
        "1".to_string(),
        "origin".to_string(),
        rev.to_string(),
    ];
    process::run_checked(&git, &fetch_args, Path::new("."))?;

    let checkout_args = vec![
        "-C".to_string(),
        command_path(destination),
        "checkout".to_string(),
        "FETCH_HEAD".to_string(),
    ];
    process::run_checked(&git, &checkout_args, Path::new("."))?;

    Ok(())
}

fn install_binary_archive(archive_path: &Path, root: &Path, name: &str) -> Result<()> {
    let source_root = root.join("source");
    reset_dir(&source_root)?;
    unpack_archive(archive_path, &source_root)?;
    let extracted_root = normalize_extracted_root(&source_root)?;

    if extracted_root.join("Cargo.toml").exists() {
        let package_root = resolve_package_root(&extracted_root, name)?;
        let cargo = which::which("cargo").context("cargo is required for setup")?;
        let args = vec![
            "install".to_string(),
            "--path".to_string(),
            package_root.display().to_string(),
            "--root".to_string(),
            root.display().to_string(),
        ];
        process::run_checked(&cargo, &args, root.parent().unwrap_or(root))?;
        return Ok(());
    }

    let binary = find_extracted_binary(&extracted_root, name)?;
    install_binary_at_path(&binary, root, name)
}

fn install_downloaded_binary(path: &Path, root: &Path, name: &str) -> Result<()> {
    install_binary_at_path(path, root, name)
}

fn install_binary_at_path(path: &Path, root: &Path, name: &str) -> Result<()> {
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .map(|value| format!(".{value}"))
        .unwrap_or_default();
    let target = root.join("bin").join(format!("{name}{extension}"));
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    fs::copy(path, &target)
        .with_context(|| format!("failed to copy {} to {}", path.display(), target.display()))?;
    make_executable(&target)?;
    Ok(())
}

fn unpack_archive(archive_path: &Path, destination: &Path) -> Result<()> {
    fs::create_dir_all(destination)
        .with_context(|| format!("failed to create {}", destination.display()))?;
    let file = fs::File::open(archive_path)
        .with_context(|| format!("failed to open {}", archive_path.display()))?;
    let file_name = archive_path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or_default();

    if file_name.ends_with(".zip") {
        let mut archive = ZipArchive::new(file)
            .with_context(|| format!("failed to read zip archive {}", archive_path.display()))?;
        archive
            .extract(destination)
            .with_context(|| format!("failed to extract {}", archive_path.display()))?;
        return Ok(());
    }

    if file_name.ends_with(".tar.gz")
        || file_name.ends_with(".tgz")
        || file_name.ends_with(".crate")
    {
        let reader = GzDecoder::new(file);
        let mut archive = Archive::new(reader);
        archive
            .unpack(destination)
            .with_context(|| format!("failed to extract {}", archive_path.display()))?;
        return Ok(());
    }

    Err(anyhow!(
        "unsupported archive format for {}",
        archive_path.display()
    ))
}

fn normalize_extracted_root(source_root: &Path) -> Result<PathBuf> {
    let children = fs::read_dir(source_root)
        .with_context(|| format!("failed to read {}", source_root.display()))?
        .filter_map(|entry| entry.ok())
        .collect::<Vec<_>>();
    if children.len() == 1 {
        let child = children[0].path();
        if child.is_dir() {
            return Ok(child);
        }
    }
    Ok(source_root.to_path_buf())
}

fn find_extracted_binary(root: &Path, name: &str) -> Result<PathBuf> {
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in
            fs::read_dir(&dir).with_context(|| format!("failed to read {}", dir.display()))?
        {
            let entry = entry?;
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }

            let stem = path.file_stem().and_then(|value| value.to_str());
            let extension = path.extension().and_then(|value| value.to_str());
            let is_binary_name =
                matches!(extension, Some("exe" | "cmd" | "bat")) || extension.is_none();
            if stem == Some(name) && is_binary_name {
                return Ok(path);
            }
        }
    }

    Err(anyhow!(
        "archive did not contain an executable or cargo package for {}",
        name
    ))
}

fn is_archive_path(path: &Path) -> bool {
    let name = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or_default();
    name.ends_with(".zip")
        || name.ends_with(".tar.gz")
        || name.ends_with(".tgz")
        || name.ends_with(".crate")
}

fn file_name_from_url(url: &str) -> Option<String> {
    let clean = url.split('?').next().unwrap_or(url).trim_end_matches('/');
    clean.rsplit('/').next().map(ToString::to_string)
}

fn github_release_asset_url(
    owner: &str,
    repo: &str,
    tag: &str,
    binary_name: &str,
) -> Result<String> {
    let (target, extension) = current_release_asset_target()?;
    let base = github_release_base_url();
    Ok(format!(
        "{base}/{owner}/{repo}/releases/download/{tag}/{binary_name}-{tag}-{target}.{extension}"
    ))
}

fn github_release_base_url() -> String {
    env::var("GDSUITE_GITHUB_RELEASE_BASE_URL")
        .unwrap_or_else(|_| "https://github.com".to_string())
        .trim_end_matches('/')
        .to_string()
}

fn github_api_base_url() -> String {
    env::var("GDSUITE_GITHUB_API_BASE_URL")
        .unwrap_or_else(|_| "https://api.github.com".to_string())
        .trim_end_matches('/')
        .to_string()
}

fn current_release_asset_target() -> Result<(&'static str, &'static str)> {
    match (env::consts::OS, env::consts::ARCH) {
        ("windows", "x86_64") => Ok(("x86_64-pc-windows-msvc", "zip")),
        ("macos", "x86_64") => Ok(("x86_64-apple-darwin", "tar.gz")),
        ("macos", "aarch64") => Ok(("aarch64-apple-darwin", "tar.gz")),
        ("linux", "x86_64") => Ok(("x86_64-unknown-linux-musl", "tar.gz")),
        (os, arch) => Err(anyhow!(
            "unsupported platform for GitHub release installs: {arch}-{os}"
        )),
    }
}

fn command_path(path: &Path) -> String {
    #[cfg(windows)]
    {
        let text = path.display().to_string();
        text.strip_prefix(r"\\?\").unwrap_or(&text).to_string()
    }

    #[cfg(not(windows))]
    {
        path.display().to_string()
    }
}

fn reset_dir(path: &Path) -> Result<()> {
    if path.exists() {
        fs::remove_dir_all(path).with_context(|| format!("failed to remove {}", path.display()))?;
    }
    fs::create_dir_all(path).with_context(|| format!("failed to create {}", path.display()))?;
    Ok(())
}

fn make_executable(path: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        let mut permissions = fs::metadata(path)?.permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(path, permissions)?;
    }

    #[cfg(not(unix))]
    let _ = path;

    Ok(())
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

pub fn probe_installation_state(
    context: &ProjectContext,
    config: &GdsuiteConfig,
    manifest: &Manifest,
    requested: &InstallPlan,
) -> Result<InstalledState> {
    let toolchain = resolve_toolchain(context, Some(manifest))?;
    let probed = probe_toolchain(context, config, &toolchain)?;

    Ok(InstalledState {
        suite_version: release_manifest().suite.version.clone(),
        manifest_version: release_manifest().suite.manifest_version,
        tools: crate::config::InstalledToolsState {
            gdtoolkit: InstalledToolState {
                requested: requested.gdtoolkit.clone(),
                resolved_version: probed.gdtoolkit_version.clone(),
            },
            gdtest: InstalledToolState {
                requested: requested.gdtest.clone(),
                resolved_version: probed.gdtest_version.clone(),
            },
            gdhealth: InstalledToolState {
                requested: requested.gdhealth.clone(),
                resolved_version: probed.gdhealth_version.clone(),
            },
        },
        contracts: InstalledContractsState {
            gdtest_harness_version: probed.gdtest_harness_version,
            gdhealth_schema_version: probed.gdhealth_schema_version,
        },
    })
}

pub fn compatibility_report(state: &InstalledState) -> CompatibilityReport {
    let release = release_manifest();

    let gdtoolkit = match &state.tools.gdtoolkit.resolved_version {
        Some(version) => evaluate_versioned_tool(
            "gdtoolkit",
            &state.tools.gdtoolkit.requested,
            version,
            &release.tools.gdtoolkit.acquire,
            &release.tools.gdtoolkit.tested,
            Vec::new(),
        ),
        None => ToolCompatibility {
            name: "gdtoolkit",
            requested: state.tools.gdtoolkit.requested.render_requested(),
            resolved_version: None,
            verdict: CompatibilityVerdict::Error,
            reason: "gdtoolkit is not installed in the project venv".to_string(),
            extra: Vec::new(),
        },
    };

    let gdtest = match (
        &state.tools.gdtest.resolved_version,
        &state.contracts.gdtest_harness_version,
    ) {
        (Some(version), Some(harness))
            if normalize_version(version) == normalize_version(harness) =>
        {
            let mut report = evaluate_versioned_tool(
                "gdtest",
                &state.tools.gdtest.requested,
                version,
                &release.tools.gdtest.acquire,
                &release.tools.gdtest.tested,
                vec![("gdtest_harness".to_string(), harness.clone())],
            );
            if normalize_version(harness)
                != normalize_spec(&release.contracts.gdtest.required_harness)
            {
                report.verdict = CompatibilityVerdict::Warning;
                report.reason = format!(
                    "gdtest harness {} is outside the suite-tested harness {}",
                    harness,
                    normalize_spec(&release.contracts.gdtest.required_harness)
                );
            }
            report
        }
        (Some(version), Some(harness)) => ToolCompatibility {
            name: "gdtest",
            requested: state.tools.gdtest.requested.render_requested(),
            resolved_version: Some(version.clone()),
            verdict: CompatibilityVerdict::Error,
            reason: format!("gdtest CLI {version} is incompatible with harness {harness}"),
            extra: vec![("gdtest_harness".to_string(), harness.clone())],
        },
        (Some(version), None) => ToolCompatibility {
            name: "gdtest",
            requested: state.tools.gdtest.requested.render_requested(),
            resolved_version: Some(version.clone()),
            verdict: CompatibilityVerdict::Error,
            reason: "gdtest harness version could not be determined".to_string(),
            extra: Vec::new(),
        },
        (None, _) => ToolCompatibility {
            name: "gdtest",
            requested: state.tools.gdtest.requested.render_requested(),
            resolved_version: None,
            verdict: CompatibilityVerdict::Error,
            reason: "gdtest version probe failed".to_string(),
            extra: Vec::new(),
        },
    };

    let gdhealth = match (
        &state.tools.gdhealth.resolved_version,
        state.contracts.gdhealth_schema_version,
    ) {
        (Some(version), Some(schema))
            if schema == release.contracts.gdhealth.required_schema_version =>
        {
            evaluate_versioned_tool(
                "gdhealth",
                &state.tools.gdhealth.requested,
                version,
                &release.tools.gdhealth.acquire,
                &release.tools.gdhealth.tested,
                vec![("gdhealth_schema".to_string(), schema.to_string())],
            )
        }
        (Some(version), Some(schema)) => ToolCompatibility {
            name: "gdhealth",
            requested: state.tools.gdhealth.requested.render_requested(),
            resolved_version: Some(version.clone()),
            verdict: CompatibilityVerdict::Error,
            reason: format!(
                "gdhealth schema {} is incompatible with required schema {}",
                schema, release.contracts.gdhealth.required_schema_version
            ),
            extra: vec![("gdhealth_schema".to_string(), schema.to_string())],
        },
        (Some(version), None) => ToolCompatibility {
            name: "gdhealth",
            requested: state.tools.gdhealth.requested.render_requested(),
            resolved_version: Some(version.clone()),
            verdict: CompatibilityVerdict::Error,
            reason: "gdhealth schema version could not be determined".to_string(),
            extra: Vec::new(),
        },
        (None, _) => ToolCompatibility {
            name: "gdhealth",
            requested: state.tools.gdhealth.requested.render_requested(),
            resolved_version: None,
            verdict: CompatibilityVerdict::Error,
            reason: "gdhealth version probe failed".to_string(),
            extra: Vec::new(),
        },
    };

    CompatibilityReport {
        gdtoolkit,
        gdtest,
        gdhealth,
    }
}

pub fn report_status(report: &CompatibilityReport) -> CompatibilityVerdict {
    let verdicts = [
        &report.gdtoolkit.verdict,
        &report.gdtest.verdict,
        &report.gdhealth.verdict,
    ];
    if verdicts
        .iter()
        .any(|verdict| **verdict == CompatibilityVerdict::Error)
    {
        CompatibilityVerdict::Error
    } else if verdicts
        .iter()
        .any(|verdict| **verdict == CompatibilityVerdict::Warning)
    {
        CompatibilityVerdict::Warning
    } else {
        CompatibilityVerdict::Ok
    }
}

fn evaluate_versioned_tool(
    name: &'static str,
    requested: &ToolAcquireSpec,
    version: &str,
    release_requested: &ToolAcquireSpec,
    tested_spec: &str,
    extra: Vec<(String, String)>,
) -> ToolCompatibility {
    let requested_rendered = requested.render_requested();
    let tested_version = normalize_spec(tested_spec);
    let is_override = requested_rendered != release_requested.render_requested();
    let version_matches = normalize_version(version) == tested_version;

    let (verdict, reason) = if is_override {
        (
            CompatibilityVerdict::Warning,
            format!(
                "{name} was installed from `{}` and is outside the suite-tested set",
                requested_rendered
            ),
        )
    } else if !version_matches {
        (
            CompatibilityVerdict::Warning,
            format!(
                "{name} {} is outside the suite-tested version {}",
                version, tested_version
            ),
        )
    } else {
        (
            CompatibilityVerdict::Ok,
            format!("{name} matches the suite-tested contract"),
        )
    };

    ToolCompatibility {
        name,
        requested: requested_rendered,
        resolved_version: Some(version.to_string()),
        verdict,
        reason,
        extra,
    }
}

fn probe_toolchain(
    context: &ProjectContext,
    config: &GdsuiteConfig,
    toolchain: &Toolchain,
) -> Result<ProbedToolchain> {
    Ok(ProbedToolchain {
        gdtoolkit_version: probe_pip_package_version(&toolchain.python, "gdtoolkit", context),
        gdtest_version: probe_command_version(&toolchain.gdtest, &["--version"], context),
        gdtest_harness_version: probe_gdtest_harness_version(context, config),
        gdhealth_version: probe_command_version(&toolchain.gdhealth, &["--version"], context),
        gdhealth_schema_version: probe_gdhealth_schema_version(&toolchain.gdhealth, context),
    })
}

fn probe_pip_package_version(
    python: &Path,
    package: &str,
    context: &ProjectContext,
) -> Option<String> {
    let args = vec![
        "-m".to_string(),
        "pip".to_string(),
        "show".to_string(),
        package.to_string(),
    ];
    let output = process::run(python, &args, &context.root).ok()?;
    if !output.status.success() {
        return None;
    }
    output.stdout.lines().find_map(|line| {
        line.strip_prefix("Version:")
            .map(|value| value.trim().to_string())
    })
}

fn probe_command_version(path: &Path, args: &[&str], context: &ProjectContext) -> Option<String> {
    let args = args
        .iter()
        .map(|value| value.to_string())
        .collect::<Vec<_>>();
    let output = process::run(path, &args, &context.root).ok()?;
    if !output.status.success() {
        return None;
    }
    output
        .stdout
        .lines()
        .find_map(|line| extract_version_token(line.trim()))
}

fn extract_version_token(line: &str) -> Option<String> {
    line.split_whitespace()
        .find(|token| token.chars().next().is_some_and(|c| c.is_ascii_digit()))
        .map(|value| value.trim_start_matches('v').to_string())
        .or_else(|| {
            line.split_whitespace()
                .last()
                .map(|value| value.trim_start_matches('v').to_string())
        })
}

fn probe_gdtest_harness_version(
    context: &ProjectContext,
    config: &GdsuiteConfig,
) -> Option<String> {
    let harness_path = context.root.join(&config.gdtest.harness);
    let body = fs::read_to_string(harness_path).ok()?;
    extract_quoted_assignment(&body, "HARNESS_VERSION")
}

fn probe_gdhealth_schema_version(path: &Path, context: &ProjectContext) -> Option<u32> {
    for args in [["--schema-version"], ["schema-version"]] {
        let args = args
            .iter()
            .map(|value| value.to_string())
            .collect::<Vec<_>>();
        let output = process::run(path, &args, &context.root).ok()?;
        if !output.status.success() {
            continue;
        }
        if let Some(value) = output
            .stdout
            .lines()
            .find_map(|line| line.trim().parse::<u32>().ok())
        {
            return Some(value);
        }
    }
    None
}

fn extract_quoted_assignment(body: &str, name: &str) -> Option<String> {
    for line in body.lines() {
        let line = line.trim();
        if !line.contains(name) {
            continue;
        }
        let start = line.find('"')?;
        let end = line[start + 1..].find('"')?;
        return Some(line[start + 1..start + 1 + end].to_string());
    }
    None
}

fn normalize_spec(spec: &str) -> String {
    spec.trim_start_matches('=').trim().to_string()
}

fn normalize_version(version: &str) -> String {
    version.trim().trim_start_matches('v').to_string()
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

    if config.policy.allow_cycles.is_empty() && config.policy.forbidden_dependencies.is_empty() {
        table.remove("policy");
    } else {
        let mut policy = toml::map::Map::new();
        policy.insert(
            "allow_cycles".to_string(),
            string_array_value(&config.policy.allow_cycles),
        );
        if !config.policy.forbidden_dependencies.is_empty() {
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
    let source_root = resolved_source_root(context, &config.tools.gdtest_source, "gdtest")?;
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::ToolAcquireSpec;

    #[test]
    fn test_parse_github_release_repo_spec() {
        let spec = parse_tool_source_spec("github:q-rault/gdtest@v0.1.0", "gdtest");
        assert!(
            matches!(spec, ToolAcquireSpec::GithubReleaseRepo { ref owner, ref repo, ref tag }
                if owner == "q-rault" && repo == "gdtest" && tag == "v0.1.0")
        );
    }

    #[test]
    fn test_parse_github_release_url_still_works() {
        let spec = parse_tool_source_spec("github:https://example.com/asset.tar.gz", "gdtest");
        assert!(matches!(spec, ToolAcquireSpec::GithubRelease { ref url }
                if url == "https://example.com/asset.tar.gz"));
    }

    #[test]
    fn test_current_target_is_known_platform() {
        let known = [
            ("x86_64-pc-windows-msvc", "zip"),
            ("x86_64-apple-darwin", "tar.gz"),
            ("aarch64-apple-darwin", "tar.gz"),
            ("x86_64-unknown-linux-musl", "tar.gz"),
        ];
        let result = current_release_asset_target();
        assert!(
            result.is_ok(),
            "current platform is not supported: {}-{}",
            std::env::consts::ARCH,
            std::env::consts::OS
        );
        let (target, ext) = result.unwrap();
        assert!(
            known.contains(&(target, ext)),
            "unexpected target/ext pair: {target}/{ext}"
        );
    }

    #[test]
    fn test_github_release_asset_url_format() {
        // Use a known base to avoid flakiness from GDSUITE_GITHUB_RELEASE_BASE_URL env var
        // SAFETY: test-only, single-threaded context
        unsafe { std::env::remove_var("GDSUITE_GITHUB_RELEASE_BASE_URL") };
        let url = github_release_asset_url("q-rault", "gdtest", "v0.1.0", "gdtest").unwrap();
        assert!(
            url.starts_with(
                "https://github.com/q-rault/gdtest/releases/download/v0.1.0/gdtest-v0.1.0-"
            ),
            "unexpected URL prefix: {url}"
        );
        assert!(
            url.ends_with(".zip") || url.ends_with(".tar.gz"),
            "unexpected URL suffix: {url}"
        );
    }
}
