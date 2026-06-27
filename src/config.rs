use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    sync::OnceLock,
};

use anyhow::{Context, Result};
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
    pub fix: FixConfig,
    #[serde(default)]
    pub lint: LintConfig,
    #[serde(default)]
    pub typecheck: TypecheckConfig,
    #[serde(default)]
    pub reports: ReportsConfig,
    #[serde(default)]
    pub health: HealthConfig,
    #[serde(default)]
    pub gdtest: GdtestConfig,
    #[serde(default)]
    pub gdhealth: GdhealthConfig,
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
    #[serde(default = "default_scripts")]
    pub include: Vec<String>,
    #[serde(default = "default_exclude")]
    pub exclude: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TypecheckConfig {
    #[serde(default = "default_scripts")]
    pub include: Vec<String>,
    #[serde(default = "default_exclude")]
    pub exclude: Vec<String>,
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
    #[serde(default = "default_health_command")]
    pub command: String,
    #[serde(default = "default_health_fail_on")]
    pub fail_on: String,
    #[serde(default = "default_health_bundle_dir")]
    pub bundle_dir: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GdtestConfig {
    #[serde(default = "default_gdtest_display_mode")]
    pub display_mode: String,
    #[serde(default = "default_gdtest_harness")]
    pub harness: String,
    #[serde(default = "default_gdtest_editor_plugin_dir")]
    pub editor_plugin_dir: String,
    #[serde(default = "default_gdtest_install_editor")]
    pub install_editor: bool,
    #[serde(default = "default_gdtest_patterns")]
    pub patterns: Vec<String>,
    #[serde(default = "default_exclude")]
    pub coverage_exclude: Vec<String>,
    #[serde(default)]
    pub profile: BTreeMap<String, GdtestProfileConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GdtestProfileConfig {
    pub reporter: Option<String>,
    pub fail_fast: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GdhealthConfig {
    #[serde(default)]
    pub project: GdhealthProjectConfig,
    #[serde(default)]
    pub discovery: GdhealthDiscoveryConfig,
    #[serde(default = "default_gdhealth_groups")]
    pub groups: BTreeMap<String, Vec<String>>,
    #[serde(default)]
    pub thresholds: GdhealthThresholdsConfig,
    #[serde(default)]
    pub policy: GdhealthPolicyConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GdhealthProjectConfig {
    #[serde(default = "default_root")]
    pub root: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GdhealthDiscoveryConfig {
    #[serde(default = "default_gdhealth_include")]
    pub include: Vec<String>,
    #[serde(default = "default_gdhealth_exclude")]
    pub exclude: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GdhealthThresholdsConfig {
    pub file_lines_warn: Option<usize>,
    pub file_lines_critical: Option<usize>,
    pub function_lines_warn: Option<usize>,
    pub function_lines_critical: Option<usize>,
    pub cyclomatic_warn: Option<usize>,
    pub cyclomatic_critical: Option<usize>,
    pub nesting_warn: Option<usize>,
    pub nesting_critical: Option<usize>,
    pub parameters_warn: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GdhealthPolicyConfig {
    #[serde(default)]
    pub allow_cycles: Vec<String>,
    #[serde(default)]
    pub forbidden_dependencies: Vec<GdhealthForbiddenDependency>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GdhealthForbiddenDependency {
    pub from: String,
    pub to: String,
    pub severity: String,
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
            include: default_scripts(),
            exclude: default_exclude(),
        }
    }
}

impl Default for TypecheckConfig {
    fn default() -> Self {
        Self {
            include: default_scripts(),
            exclude: default_exclude(),
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

impl Default for HealthConfig {
    fn default() -> Self {
        Self {
            command: default_health_command(),
            fail_on: default_health_fail_on(),
            bundle_dir: default_health_bundle_dir(),
        }
    }
}

impl Default for GdtestConfig {
    fn default() -> Self {
        Self {
            display_mode: default_gdtest_display_mode(),
            harness: default_gdtest_harness(),
            editor_plugin_dir: default_gdtest_editor_plugin_dir(),
            install_editor: default_gdtest_install_editor(),
            patterns: default_gdtest_patterns(),
            coverage_exclude: default_exclude(),
            profile: default_gdtest_profiles(),
        }
    }
}

impl Default for GdhealthConfig {
    fn default() -> Self {
        Self {
            project: GdhealthProjectConfig::default(),
            discovery: GdhealthDiscoveryConfig::default(),
            groups: default_gdhealth_groups(),
            thresholds: GdhealthThresholdsConfig::default(),
            policy: GdhealthPolicyConfig::default(),
        }
    }
}

impl Default for GdhealthProjectConfig {
    fn default() -> Self {
        Self {
            root: default_root(),
        }
    }
}

impl Default for GdhealthDiscoveryConfig {
    fn default() -> Self {
        Self {
            include: default_gdhealth_include(),
            exclude: default_gdhealth_exclude(),
        }
    }
}

impl GdsuiteConfig {
    pub fn load(path: &Path) -> Result<Self> {
        let text = fs::read_to_string(path)
            .with_context(|| format!("failed to read config {}", path.display()))?;
        let mut config: Self = toml::from_str(&text)
            .with_context(|| format!("failed to parse config {}", path.display()))?;
        config.ensure_gdtest_profile_defaults();
        Ok(config)
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

    fn ensure_gdtest_profile_defaults(&mut self) {
        for (name, defaults) in default_gdtest_profiles() {
            let profile = self.gdtest.profile.entry(name).or_default();
            if profile.reporter.is_none() {
                profile.reporter = defaults.reporter;
            }
            if profile.fail_fast.is_none() {
                profile.fail_fast = defaults.fail_fast;
            }
        }
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

fn default_gdhealth_exclude() -> Vec<String> {
    vec![
        "addons/**".to_string(),
        ".godot/**".to_string(),
        "reports/**".to_string(),
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

fn default_report_format() -> String {
    "text".to_string()
}

fn default_artifacts_dir() -> PathBuf {
    PathBuf::from("reports/gdsuite")
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

fn default_gdtest_display_mode() -> String {
    "headless".to_string()
}

fn default_gdtest_harness() -> String {
    "addons/gdtest/gdtest_worker.gd".to_string()
}

fn default_gdtest_editor_plugin_dir() -> String {
    "addons/gdtest_editor".to_string()
}

fn default_gdtest_install_editor() -> bool {
    true
}

fn default_gdtest_patterns() -> Vec<String> {
    vec!["test/**/*.gd".to_string(), "**/*.test.gd".to_string()]
}

fn default_gdtest_profiles() -> BTreeMap<String, GdtestProfileConfig> {
    BTreeMap::from([(
        "ci".to_string(),
        GdtestProfileConfig {
            reporter: Some("junit".to_string()),
            fail_fast: Some(true),
        },
    )])
}

fn default_gdhealth_include() -> Vec<String> {
    vec!["**/*.gd".to_string()]
}

fn default_gdhealth_groups() -> BTreeMap<String, Vec<String>> {
    BTreeMap::from([
        (
            "client".to_string(),
            vec!["client/**/*.gd".to_string(), "client/**".to_string()],
        ),
        (
            "server".to_string(),
            vec!["server/**/*.gd".to_string(), "server/**".to_string()],
        ),
        (
            "test".to_string(),
            vec!["test/**/*.gd".to_string(), "test/**".to_string()],
        ),
        (
            "root_gameplay".to_string(),
            vec![
                "*.gd".to_string(),
                "!test/**".to_string(),
                "!server/**".to_string(),
                "!client/**".to_string(),
            ],
        ),
    ])
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

[commands]
default_test_target = "test/"
default_health_subcommand = "health"

[fix]
safe_rules = ["format", "trailing-whitespace", "final-newline"]
unsafe_rules = ["class-definitions-order"]

[lint]
line_length = 100
include = ["*.gd", "client/**/*.gd", "server/**/*.gd", "test/**/*.gd"]
exclude = ["addons/**", ".godot/**", "reports/**", ".gdsuite/**"]

[typecheck]
include = ["*.gd", "client/**/*.gd", "server/**/*.gd", "test/**/*.gd"]
exclude = ["addons/**", ".godot/**", "reports/**", ".gdsuite/**"]

[reports]
default = "text"
artifacts_dir = "reports/gdsuite"

[health]
command = "health"
fail_on = "critical"
bundle_dir = "reports/code-health"

[gdtest]
display_mode = "headless"
harness = "addons/gdtest/gdtest_worker.gd"
editor_plugin_dir = "addons/gdtest_editor"
install_editor = true
patterns = ["test/**/*.gd", "**/*.test.gd"]
coverage_exclude = ["addons/**", ".godot/**", "reports/**", ".gdsuite/**"]

[gdtest.profile.ci]
reporter = "junit"
fail_fast = true

[gdhealth.project]
root = "."

[gdhealth.discovery]
include = ["**/*.gd"]
exclude = ["addons/**", ".godot/**", "reports/**"]

[gdhealth.groups]
client = ["client/**/*.gd", "client/**"]
server = ["server/**/*.gd", "server/**"]
test = ["test/**/*.gd", "test/**"]
root_gameplay = ["*.gd", "!test/**", "!server/**", "!client/**"]
"#,
        release
            .tools
            .gdtoolkit
            .acquire
            .gdtoolkit_spec()
            .expect("gdtoolkit release manifest must use pip"),
        release.tools.gdtest.acquire.render_requested(),
        release.tools.gdhealth.acquire.render_requested(),
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

    let mut body = format!(
        r#"[project]
root = "{}"
{godot_exe}[tools]
gdtoolkit = "{}"
gdtest_source = "{}"
gdhealth_source = "{}"

[commands]
default_test_target = "{}"
default_health_subcommand = "{}"

[fix]
safe_rules = {}
unsafe_rules = {}

[lint]
line_length = {}
include = {}
exclude = {}

[typecheck]
include = {}
exclude = {}
"#,
        escape_string(&config.project.root),
        escape_string(&config.tools.gdtoolkit),
        escape_string(&config.tools.gdtest_source),
        escape_string(&config.tools.gdhealth_source),
        escape_string(&config.commands.default_test_target),
        escape_string(&config.commands.default_health_subcommand),
        render_string_array(&config.fix.safe_rules),
        render_string_array(&config.fix.unsafe_rules),
        config.lint.line_length,
        render_string_array(&config.lint.include),
        render_string_array(&config.lint.exclude),
        render_string_array(&config.typecheck.include),
        render_string_array(&config.typecheck.exclude),
    );

    body.push_str(&format!(
        r#"
[reports]
default = "{}"
artifacts_dir = "{}"

[health]
command = "{}"
fail_on = "{}"
bundle_dir = "{}"

[gdtest]
display_mode = "{}"
harness = "{}"
editor_plugin_dir = "{}"
install_editor = {}
patterns = {}
coverage_exclude = {}
"#,
        escape_string(&config.reports.default),
        escape_path(&config.reports.artifacts_dir),
        escape_string(&config.health.command),
        escape_string(&config.health.fail_on),
        escape_path(&config.health.bundle_dir),
        escape_string(&config.gdtest.display_mode),
        escape_string(&config.gdtest.harness),
        escape_string(&config.gdtest.editor_plugin_dir),
        config.gdtest.install_editor,
        render_string_array(&config.gdtest.patterns),
        render_string_array(&config.gdtest.coverage_exclude),
    ));

    for (name, profile) in &config.gdtest.profile {
        body.push_str(&format!("\n[gdtest.profile.{name}]\n"));
        if let Some(reporter) = &profile.reporter {
            body.push_str(&format!("reporter = \"{}\"\n", escape_string(reporter)));
        }
        if let Some(fail_fast) = profile.fail_fast {
            body.push_str(&format!("fail_fast = {fail_fast}\n"));
        }
    }

    body.push_str(&format!(
        r#"
[gdhealth.project]
root = "{}"

[gdhealth.discovery]
include = {}
exclude = {}

[gdhealth.groups]
"#,
        escape_string(&config.gdhealth.project.root),
        render_string_array(&config.gdhealth.discovery.include),
        render_string_array(&config.gdhealth.discovery.exclude),
    ));

    for (name, patterns) in &config.gdhealth.groups {
        body.push_str(&format!("{name} = {}\n", render_string_array(patterns)));
    }

    if has_thresholds(&config.gdhealth.thresholds) {
        body.push_str("\n[gdhealth.thresholds]\n");
        append_optional_usize(
            &mut body,
            "file_lines_warn",
            config.gdhealth.thresholds.file_lines_warn,
        );
        append_optional_usize(
            &mut body,
            "file_lines_critical",
            config.gdhealth.thresholds.file_lines_critical,
        );
        append_optional_usize(
            &mut body,
            "function_lines_warn",
            config.gdhealth.thresholds.function_lines_warn,
        );
        append_optional_usize(
            &mut body,
            "function_lines_critical",
            config.gdhealth.thresholds.function_lines_critical,
        );
        append_optional_usize(
            &mut body,
            "cyclomatic_warn",
            config.gdhealth.thresholds.cyclomatic_warn,
        );
        append_optional_usize(
            &mut body,
            "cyclomatic_critical",
            config.gdhealth.thresholds.cyclomatic_critical,
        );
        append_optional_usize(
            &mut body,
            "nesting_warn",
            config.gdhealth.thresholds.nesting_warn,
        );
        append_optional_usize(
            &mut body,
            "nesting_critical",
            config.gdhealth.thresholds.nesting_critical,
        );
        append_optional_usize(
            &mut body,
            "parameters_warn",
            config.gdhealth.thresholds.parameters_warn,
        );
    }

    if !config.gdhealth.policy.allow_cycles.is_empty()
        || !config.gdhealth.policy.forbidden_dependencies.is_empty()
    {
        body.push_str("\n[gdhealth.policy]\n");
        body.push_str(&format!(
            "allow_cycles = {}\n",
            render_string_array(&config.gdhealth.policy.allow_cycles)
        ));
        for rule in &config.gdhealth.policy.forbidden_dependencies {
            body.push_str("\n[[gdhealth.policy.forbidden_dependencies]]\n");
            body.push_str(&format!("from = \"{}\"\n", escape_string(&rule.from)));
            body.push_str(&format!("to = \"{}\"\n", escape_string(&rule.to)));
            body.push_str(&format!(
                "severity = \"{}\"\n",
                escape_string(&rule.severity)
            ));
        }
    }

    body
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

fn append_optional_usize(body: &mut String, key: &str, value: Option<usize>) {
    if let Some(value) = value {
        body.push_str(&format!("{key} = {value}\n"));
    }
}

fn render_string_array(values: &[String]) -> String {
    let values = values
        .iter()
        .map(|value| format!("\"{}\"", escape_string(value)))
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
