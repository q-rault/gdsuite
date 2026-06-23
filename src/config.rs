use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GdsuiteConfig {
    #[serde(default)]
    pub project: ProjectConfig,
    #[serde(default)]
    pub tools: ToolsConfig,
    #[serde(default)]
    pub paths: PathsConfig,
    #[serde(default)]
    pub commands: CommandsConfig,
    #[serde(default)]
    pub fix: FixConfig,
    #[serde(default)]
    pub lint: LintConfig,
    #[serde(default)]
    pub profiles: ProfilesConfig,
    #[serde(default)]
    pub reports: ReportsConfig,
    #[serde(default)]
    pub health: HealthConfig,
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
    pub gdtest_source: PathBuf,
    #[serde(default = "default_gdhealth_source")]
    pub gdhealth_source: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PathsConfig {
    #[serde(default = "default_scripts")]
    pub scripts: Vec<String>,
    #[serde(default = "default_exclude")]
    pub exclude: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandsConfig {
    #[serde(default = "default_test_target")]
    pub default_test_target: String,
    #[serde(default = "default_health_subcommand")]
    pub default_health_subcommand: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FixConfig {
    #[serde(default = "default_safe_rules")]
    pub safe_rules: Vec<String>,
    #[serde(default = "default_unsafe_rules")]
    pub unsafe_rules: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LintConfig {
    #[serde(default = "default_line_length")]
    pub line_length: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfilesConfig {
    #[serde(flatten, default = "default_profiles")]
    pub entries: BTreeMap<String, ProfileConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfileConfig {
    #[serde(default)]
    pub pipeline: Vec<String>,
    pub health_fail_on: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReportsConfig {
    #[serde(default = "default_report_format")]
    pub default: String,
    #[serde(default = "default_artifacts_dir")]
    pub artifacts_dir: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthConfig {
    #[serde(default = "default_health_enabled")]
    pub enabled: bool,
    #[serde(default = "default_health_command")]
    pub command: String,
    #[serde(default = "default_health_fail_on")]
    pub fail_on: String,
    #[serde(default = "default_health_bundle_dir")]
    pub bundle_dir: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Manifest {
    pub godot_exe: Option<PathBuf>,
    pub python: Option<PathBuf>,
    pub gdformat: Option<PathBuf>,
    pub gdlint: Option<PathBuf>,
    pub gdtest: Option<PathBuf>,
    pub gdhealth: Option<PathBuf>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Versions {
    pub gdtoolkit: String,
    pub gdtest_source: PathBuf,
    pub gdhealth_source: PathBuf,
}

impl Default for GdsuiteConfig {
    fn default() -> Self {
        Self {
            project: ProjectConfig::default(),
            tools: ToolsConfig::default(),
            paths: PathsConfig::default(),
            commands: CommandsConfig::default(),
            fix: FixConfig::default(),
            lint: LintConfig::default(),
            profiles: ProfilesConfig::default(),
            reports: ReportsConfig::default(),
            health: HealthConfig::default(),
        }
    }
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
        }
    }
}

impl Default for PathsConfig {
    fn default() -> Self {
        Self {
            scripts: default_scripts(),
            exclude: default_exclude(),
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

impl Default for FixConfig {
    fn default() -> Self {
        Self {
            safe_rules: default_safe_rules(),
            unsafe_rules: default_unsafe_rules(),
        }
    }
}

impl Default for LintConfig {
    fn default() -> Self {
        Self {
            line_length: default_line_length(),
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

impl Default for ProfilesConfig {
    fn default() -> Self {
        Self {
            entries: default_profiles(),
        }
    }
}

impl Default for ProfileConfig {
    fn default() -> Self {
        Self {
            pipeline: Vec::new(),
            health_fail_on: None,
        }
    }
}

impl Default for HealthConfig {
    fn default() -> Self {
        Self {
            enabled: default_health_enabled(),
            command: default_health_command(),
            fail_on: default_health_fail_on(),
            bundle_dir: default_health_bundle_dir(),
        }
    }
}

impl GdsuiteConfig {
    pub fn load(path: &Path) -> Result<Self> {
        let text = fs::read_to_string(path)
            .with_context(|| format!("failed to read config {}", path.display()))?;
        toml::from_str(&text).with_context(|| format!("failed to parse config {}", path.display()))
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

impl Versions {
    pub fn write(&self, path: &Path) -> Result<()> {
        fs::write(path, serde_json::to_string_pretty(self)?)
            .with_context(|| format!("failed to write versions {}", path.display()))
    }
}

fn default_root() -> String {
    ".".to_string()
}

fn default_gdtoolkit_version() -> String {
    "4.*".to_string()
}

fn default_gdtest_source() -> PathBuf {
    PathBuf::from("../gdtest")
}

fn default_gdhealth_source() -> PathBuf {
    PathBuf::from("../gdhealth/tools/gdhealth")
}

fn default_scripts() -> Vec<String> {
    vec![
        "*.gd".to_string(),
        "client/**/*.gd".to_string(),
        "server/**/*.gd".to_string(),
        "test/**/*.gd".to_string(),
    ]
}

fn default_exclude() -> Vec<String> {
    vec![
        "addons/**".to_string(),
        ".godot/**".to_string(),
        "reports/**".to_string(),
        ".gdsuite/**".to_string(),
    ]
}

fn default_test_target() -> String {
    "test/".to_string()
}

fn default_health_subcommand() -> String {
    "health".to_string()
}

fn default_safe_rules() -> Vec<String> {
    vec![
        "format".to_string(),
        "trailing-whitespace".to_string(),
        "final-newline".to_string(),
    ]
}

fn default_unsafe_rules() -> Vec<String> {
    vec!["class-definitions-order".to_string()]
}

fn default_line_length() -> usize {
    100
}

fn default_profiles() -> BTreeMap<String, ProfileConfig> {
    BTreeMap::from([
        (
            "local".to_string(),
            ProfileConfig {
                pipeline: vec![
                    "fmt".to_string(),
                    "lint".to_string(),
                    "typecheck".to_string(),
                ],
                health_fail_on: None,
            },
        ),
        (
            "ci".to_string(),
            ProfileConfig {
                pipeline: vec![
                    "fmt".to_string(),
                    "lint".to_string(),
                    "typecheck".to_string(),
                    "test".to_string(),
                    "health".to_string(),
                ],
                health_fail_on: None,
            },
        ),
        (
            "strict".to_string(),
            ProfileConfig {
                pipeline: vec![
                    "fmt".to_string(),
                    "lint".to_string(),
                    "typecheck".to_string(),
                    "test".to_string(),
                    "health".to_string(),
                ],
                health_fail_on: Some("warning".to_string()),
            },
        ),
    ])
}

fn default_report_format() -> String {
    "text".to_string()
}

fn default_artifacts_dir() -> PathBuf {
    PathBuf::from("reports/gdsuite")
}

fn default_health_enabled() -> bool {
    true
}

fn default_health_command() -> String {
    "health".to_string()
}

fn default_health_fail_on() -> String {
    "critical".to_string()
}

fn default_health_bundle_dir() -> PathBuf {
    PathBuf::from("reports/code-health")
}

fn default_config_template(godot_exe: Option<&Path>) -> String {
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

gdtoolkit = "4.*"
gdtest_source = "../gdtest"
gdhealth_source = "../gdhealth/tools/gdhealth"

[paths]
scripts = ["*.gd", "client/**/*.gd", "server/**/*.gd", "test/**/*.gd"]
exclude = ["addons/**", ".godot/**", "reports/**", ".gdsuite/**"]

[commands]
default_test_target = "test/"
default_health_subcommand = "health"

[fix]
safe_rules = ["format", "trailing-whitespace", "final-newline"]
unsafe_rules = ["class-definitions-order"]

[lint]
line_length = 100

[profiles.local]
pipeline = ["fmt", "lint", "typecheck"]

[profiles.ci]
pipeline = ["fmt", "lint", "typecheck", "test", "health"]

[profiles.strict]
pipeline = ["fmt", "lint", "typecheck", "test", "health"]
health_fail_on = "warning"

[reports]
default = "text"
artifacts_dir = "reports/gdsuite"

[health]
enabled = true
command = "health"
fail_on = "critical"
bundle_dir = "reports/code-health"
"#
    )
}
