use std::{
    fs,
    path::{Path, PathBuf},
    sync::OnceLock,
};

use anyhow::{Context, Result, anyhow};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GdsuiteConfig {
    #[serde(default)]
    pub project: ProjectConfig,
    #[serde(default)]
    pub tools: ToolsConfig,
    #[serde(default)]
    pub commands: CommandsConfig,
    #[serde(default)]
    pub reports: ReportsConfig,
    #[serde(default)]
    pub check: CheckConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectConfig {
    #[serde(default = "default_root")]
    pub root: String,
    pub godot_exe: Option<PathBuf>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolsConfig {
    #[serde(default = "default_gdtoolkit_version")]
    pub gdtoolkit: String,
    #[serde(default = "default_gdtest_source")]
    pub gdtest_source: String,
    #[serde(default = "default_gdhealth_source")]
    pub gdhealth_source: String,
    #[serde(default = "default_gdtypecheck_source")]
    pub gdtypecheck_source: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandsConfig {
    #[serde(default = "default_test_target")]
    pub default_test_target: String,
    #[serde(default = "default_health_subcommand")]
    pub default_health_subcommand: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReportsConfig {
    #[serde(default = "default_report_format")]
    pub default: String,
    #[serde(default = "default_artifacts_dir")]
    pub artifacts_dir: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckConfig {
    #[serde(default = "default_check_steps")]
    pub steps: Vec<CheckStep>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum CheckStep {
    Fmt,
    Lint,
    Typecheck,
    Test,
    Health,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Manifest {
    pub godot_exe: Option<PathBuf>,
    pub python: Option<PathBuf>,
    pub gdformat: Option<PathBuf>,
    pub gdlint: Option<PathBuf>,
    pub gdtest: Option<PathBuf>,
    pub gdhealth: Option<PathBuf>,
    pub gdtypecheck: Option<PathBuf>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReleaseManifest {
    pub suite: ReleaseSuite,
    pub tools: ReleaseTools,
    pub contracts: ReleaseContracts,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReleaseSuite {
    pub version: String,
    pub manifest_version: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReleaseTools {
    pub gdtoolkit: ReleaseTool,
    pub gdtest: ReleaseTool,
    pub gdhealth: ReleaseTool,
    pub gdtypecheck: ReleaseTool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReleaseTool {
    pub acquire: ToolAcquireSpec,
    pub tested: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReleaseContracts {
    pub gdtest: GdtestContract,
    pub gdhealth: GdhealthContract,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GdtestContract {
    pub required_harness: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GdhealthContract {
    pub required_schema_version: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ToolAcquireSpec {
    Pip {
        package: String,
        spec: String,
    },
    Crate {
        #[serde(rename = "crate")]
        crate_name: String,
        version: String,
    },
    Path {
        path: String,
    },
    Git {
        url: String,
        rev: String,
        subdir: Option<String>,
    },
    GithubRelease {
        url: String,
    },
    GithubReleaseRepo {
        owner: String,
        repo: String,
        tag: String,
    },
}

#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolPolicy {
    pub tested: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstalledState {
    pub suite_version: String,
    pub manifest_version: u32,
    pub tools: InstalledToolsState,
    pub contracts: InstalledContractsState,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstalledToolsState {
    pub gdtoolkit: InstalledToolState,
    pub gdtest: InstalledToolState,
    pub gdhealth: InstalledToolState,
    pub gdtypecheck: InstalledToolState,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstalledToolState {
    pub requested: ToolAcquireSpec,
    pub resolved_version: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct InstalledContractsState {
    pub gdtest_harness_version: Option<String>,
    pub gdhealth_schema_version: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum CompatibilityVerdict {
    Ok,
    Warning,
    Error,
}

impl Default for ProjectConfig {
    fn default() -> Self {
        Self {
            root: default_root(),
            godot_exe: None,
        }
    }
}

impl Default for ToolsConfig {
    fn default() -> Self {
        Self {
            gdtoolkit: default_gdtoolkit_version(),
            gdtest_source: default_gdtest_source(),
            gdhealth_source: default_gdhealth_source(),
            gdtypecheck_source: default_gdtypecheck_source(),
        }
    }
}

impl Default for CommandsConfig {
    fn default() -> Self {
        Self {
            default_test_target: default_test_target(),
            default_health_subcommand: default_health_subcommand(),
        }
    }
}

impl Default for ReportsConfig {
    fn default() -> Self {
        Self {
            default: default_report_format(),
            artifacts_dir: default_artifacts_dir(),
        }
    }
}

impl Default for CheckConfig {
    fn default() -> Self {
        Self {
            steps: default_check_steps(),
        }
    }
}

impl GdsuiteConfig {
    pub fn load(path: &Path) -> Result<Self> {
        let text = fs::read_to_string(path)
            .with_context(|| format!("failed to read config {}", path.display()))?;
        let value: toml::Value = toml::from_str(&text)
            .with_context(|| format!("failed to parse config {}", path.display()))?;
        reject_legacy_sections(path, &value)?;
        value
            .try_into()
            .with_context(|| format!("failed to parse config {}", path.display()))
    }

    pub fn write(&self, path: &Path) -> Result<()> {
        fs::write(path, render_gdsuite_config(self))
            .with_context(|| format!("failed to write config {}", path.display()))
    }

    pub fn write_if_missing(path: &Path, godot_exe: Option<&Path>) -> Result<()> {
        if path.exists() {
            return Ok(());
        }

        fs::write(path, default_config_template(godot_exe))
            .with_context(|| format!("failed to write config {}", path.display()))
    }
}

impl Manifest {
    pub fn load(path: &Path) -> Result<Self> {
        let text = fs::read_to_string(path)
            .with_context(|| format!("failed to read manifest {}", path.display()))?;
        serde_json::from_str(&text)
            .with_context(|| format!("failed to parse manifest {}", path.display()))
    }

    pub fn write(&self, path: &Path) -> Result<()> {
        fs::write(path, serde_json::to_string_pretty(self)?)
            .with_context(|| format!("failed to write manifest {}", path.display()))
    }
}

impl ReleaseManifest {
    pub fn bundled() -> &'static Self {
        static RELEASE_MANIFEST: OnceLock<ReleaseManifest> = OnceLock::new();
        RELEASE_MANIFEST.get_or_init(|| {
            toml::from_str(include_str!("../.gdsuite-release.toml"))
                .expect("bundled release manifest must parse")
        })
    }
}

impl InstalledState {
    #[allow(dead_code)]
    pub fn load(path: &Path) -> Result<Self> {
        let text = fs::read_to_string(path)
            .with_context(|| format!("failed to read installation state {}", path.display()))?;
        serde_json::from_str(&text)
            .with_context(|| format!("failed to parse installation state {}", path.display()))
    }

    pub fn write(&self, path: &Path) -> Result<()> {
        fs::write(path, serde_json::to_string_pretty(self)?)
            .with_context(|| format!("failed to write installation state {}", path.display()))
    }
}

impl ToolAcquireSpec {
    pub fn gdtoolkit_spec(&self) -> Option<&str> {
        match self {
            Self::Pip { spec, .. } => Some(spec),
            _ => None,
        }
    }

    pub fn render_requested(&self) -> String {
        match self {
            Self::Pip { spec, .. } => spec.clone(),
            Self::Crate {
                crate_name,
                version,
            } => format!("crate:{crate_name}@{version}"),
            Self::Path { path } => format!("path:{path}"),
            Self::Git { url, rev, subdir } => {
                if let Some(subdir) = subdir {
                    format!("git:{url}?rev={rev}&subdir={subdir}")
                } else {
                    format!("git:{url}?rev={rev}")
                }
            }
            Self::GithubRelease { url } => format!("github:{url}"),
            Self::GithubReleaseRepo { owner, repo, tag } => {
                format!("github:{owner}/{repo}@{tag}")
            }
        }
    }
}

fn reject_legacy_sections(path: &Path, value: &toml::Value) -> Result<()> {
    let Some(table) = value.as_table() else {
        return Ok(());
    };

    let legacy = [
        (
            "lint",
            "create `gdlintrc` with your native gdlint settings",
        ),
        (
            "health",
            "remove suite-owned health settings; use gdhealth CLI args or `gdhealth.toml`",
        ),
        (
            "gdtest",
            "create `gdtest.toml` with your native gdtest settings",
        ),
        (
            "gdhealth",
            "create `gdhealth.toml` with your native gdhealth settings",
        ),
        (
            "typecheck",
            "configure the delegated typecheck tool directly; gdsuite no longer owns typecheck config",
        ),
        (
            "fix",
            "remove suite-owned fixers; use the delegated tool behavior instead",
        ),
    ]
    .into_iter()
    .filter(|(key, _)| table.contains_key(*key))
    .collect::<Vec<_>>();

    if legacy.is_empty() {
        return Ok(());
    }

    let guidance = legacy
        .into_iter()
        .map(|(key, message)| format!("[{key}] is no longer supported: {message}"))
        .collect::<Vec<_>>()
        .join("; ");
    Err(anyhow!(
        "unsupported legacy sections in {}: {}. Run `gdsuite migrate` to rewrite the file.",
        path.display(),
        guidance
    ))
}

fn default_root() -> String {
    ".".to_string()
}

fn default_gdtoolkit_version() -> String {
    ReleaseManifest::bundled()
        .tools
        .gdtoolkit
        .acquire
        .gdtoolkit_spec()
        .expect("gdtoolkit release manifest must use pip")
        .to_string()
}

fn default_gdtest_source() -> String {
    ReleaseManifest::bundled()
        .tools
        .gdtest
        .acquire
        .render_requested()
}

fn default_gdhealth_source() -> String {
    ReleaseManifest::bundled()
        .tools
        .gdhealth
        .acquire
        .render_requested()
}

fn default_gdtypecheck_source() -> String {
    ReleaseManifest::bundled()
        .tools
        .gdtypecheck
        .acquire
        .render_requested()
}

fn default_test_target() -> String {
    "test/".to_string()
}

fn default_health_subcommand() -> String {
    "health".to_string()
}

fn default_report_format() -> String {
    "text".to_string()
}

fn default_artifacts_dir() -> PathBuf {
    PathBuf::from("reports/gdsuite")
}

fn default_check_steps() -> Vec<CheckStep> {
    vec![
        CheckStep::Fmt,
        CheckStep::Lint,
        CheckStep::Typecheck,
        CheckStep::Health,
    ]
}

fn default_config_template(godot_exe: Option<&Path>) -> String {
    let release = ReleaseManifest::bundled();
    let godot_exe = godot_exe
        .map(|path| {
            format!(
                "godot_exe = \"{}\"\n",
                path.display().to_string().replace('\\', "\\\\")
            )
        })
        .unwrap_or_default();

    format!(
        r#"[project]
root = "."
{godot_exe}[tools]
gdtoolkit = "{}"
gdtest_source = "{}"
gdhealth_source = "{}"
gdtypecheck_source = "{}"

[commands]
default_test_target = "test/"
default_health_subcommand = "health"

[reports]
default = "text"
artifacts_dir = "reports/gdsuite"

[check]
steps = ["fmt", "lint", "typecheck", "health"]
"#,
        release
            .tools
            .gdtoolkit
            .acquire
            .gdtoolkit_spec()
            .expect("gdtoolkit release manifest must use pip"),
        release.tools.gdtest.acquire.render_requested(),
        release.tools.gdhealth.acquire.render_requested(),
        release.tools.gdtypecheck.acquire.render_requested(),
    )
}

fn render_gdsuite_config(config: &GdsuiteConfig) -> String {
    let godot_exe = config
        .project
        .godot_exe
        .as_ref()
        .map(|path| {
            format!(
                "godot_exe = \"{}\"\n",
                path.display().to_string().replace('\\', "\\\\")
            )
        })
        .unwrap_or_default();

    format!(
        r#"[project]
root = "{}"
{godot_exe}[tools]
gdtoolkit = "{}"
gdtest_source = "{}"
gdhealth_source = "{}"
gdtypecheck_source = "{}"

[commands]
default_test_target = "{}"
default_health_subcommand = "{}"

[reports]
default = "{}"
artifacts_dir = "{}"

[check]
steps = {}
"#,
        escape_string(&config.project.root),
        escape_string(&config.tools.gdtoolkit),
        escape_string(&config.tools.gdtest_source),
        escape_string(&config.tools.gdhealth_source),
        escape_string(&config.tools.gdtypecheck_source),
        escape_string(&config.commands.default_test_target),
        escape_string(&config.commands.default_health_subcommand),
        escape_string(&config.reports.default),
        escape_path(&config.reports.artifacts_dir),
        render_check_steps(&config.check.steps),
    )
}

fn render_check_steps(steps: &[CheckStep]) -> String {
        let values = steps
        .iter()
        .map(|step| match step {
            CheckStep::Fmt => "\"fmt\"",
            CheckStep::Lint => "\"lint\"",
            CheckStep::Typecheck => "\"typecheck\"",
            CheckStep::Test => "\"test\"",
            CheckStep::Health => "\"health\"",
        })
        .collect::<Vec<_>>();
    format!("[{}]", values.join(", "))
}

fn escape_string(value: &str) -> String {
    value.replace('\\', "\\\\")
}

fn escape_path(path: &Path) -> String {
    escape_string(&path.display().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_github_release_repo_render_requested() {
        let spec = ToolAcquireSpec::GithubReleaseRepo {
            owner: "q-rault".to_string(),
            repo: "gdtest".to_string(),
            tag: "v0.1.0".to_string(),
        };
        assert_eq!(spec.render_requested(), "github:q-rault/gdtest@v0.1.0");
    }
}
