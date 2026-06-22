use std::{
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

        let mut config = GdsuiteConfig::default();
        config.project.godot_exe = godot_exe.map(Path::to_path_buf);
        fs::write(path, toml::to_string_pretty(&config)?)
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
