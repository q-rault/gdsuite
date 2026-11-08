// The fixtures stand in for the delegated tools with `.cmd` scripts. Other platforms
// are covered end to end by .github/workflows/e2e.yml.
#![cfg(windows)]

use std::{
    collections::HashMap,
    fs,
    io::{Read, Write},
    net::TcpListener,
    path::{Path, PathBuf},
    sync::Arc,
    thread,
};

use assert_cmd::Command;
use predicates::prelude::*;
use tempfile::TempDir;
use zip::{ZipWriter, write::SimpleFileOptions};

#[test]
fn fmt_defaults_to_dot() {
    let fixture = Fixture::new();
    fixture.seed_project();
    fixture.seed_installed_toolchain();

    fixture.cmd().arg("fmt").assert().success();

    assert_eq!(fixture.read_log("gdtidy-fmt-args.txt"), "fmt .");
}

#[test]
fn fmt_forwards_args_and_keeps_explicit_targets() {
    let fixture = Fixture::new();
    fixture.seed_project();
    fixture.seed_installed_toolchain();

    fixture
        .cmd()
        .args(["fmt", "--", "--check", "src", "test"])
        .assert()
        .success();

    assert_eq!(
        fixture.read_log("gdtidy-fmt-args.txt"),
        "fmt --check src test"
    );
}

#[test]
fn check_fails_on_files_gdtidy_would_reformat() {
    let fixture = Fixture::new();
    fixture.seed_project();
    fixture.seed_installed_toolchain();
    fixture.write_flag("fmt-fail.flag");

    fixture
        .cmd()
        .arg("check")
        .assert()
        .failure()
        .stdout(predicate::str::contains(
            "test/sample_a.gd: would be reformatted",
        ))
        .stdout(predicate::str::contains(
            "Found formatting issues in 2 files",
        ));

    assert_eq!(
        fixture.read_log("gdtidy-fmt-args.txt"),
        "fmt --check --format json ."
    );
}

#[test]
fn a_file_with_a_syntax_error_does_not_fail_the_format_check() {
    let fixture = Fixture::new();
    fixture.seed_project();
    fixture.seed_installed_toolchain();
    fixture.write_flag("fmt-skip.flag");

    fixture.cmd().arg("check").assert().success();
}

#[test]
fn lint_runs_gdtidy_with_its_json_report() {
    let fixture = Fixture::new();
    fixture.seed_project();
    fixture.seed_installed_toolchain();

    fixture
        .cmd()
        .arg("lint")
        .assert()
        .success()
        .stderr(predicate::str::contains("0 lint problems in 1 files"));

    assert_eq!(fixture.read_log("gdtidy-args.txt"), "lint --format json");
}

#[test]
fn lint_forwards_args_to_gdtidy() {
    let fixture = Fixture::new();
    fixture.seed_project();
    fixture.seed_installed_toolchain();

    fixture
        .cmd()
        .args(["lint", "--", "--fix", "src"])
        .assert()
        .success();

    assert_eq!(
        fixture.read_log("gdtidy-args.txt"),
        "lint --format json --fix src"
    );
}

#[test]
fn lint_prints_findings_and_fails() {
    let fixture = Fixture::new();
    fixture.seed_project();
    fixture.seed_installed_toolchain();
    fixture.write_flag("lint-fail.flag");

    fixture
        .cmd()
        .arg("lint")
        .assert()
        .failure()
        .code(12)
        .stdout(predicate::str::contains(
            "test/sample.gd:3:6: warning[function-name]: Function name",
        ));
}

#[test]
fn a_file_with_a_syntax_error_does_not_fail_lint() {
    let fixture = Fixture::new();
    fixture.seed_project();
    fixture.seed_installed_toolchain();
    fixture.write_flag("lint-skip.flag");

    fixture
        .cmd()
        .arg("lint")
        .assert()
        .success()
        .stderr(predicate::str::contains(
            "test/broken.gd:2:1: skipped: Expected end of statement.",
        ))
        .stderr(predicate::str::contains("which typecheck reports"));
}

#[test]
fn lint_warns_about_a_gdlintrc_gdtidy_does_not_read() {
    let fixture = Fixture::new();
    fixture.seed_project();
    fixture.seed_installed_toolchain();
    fs::write(fixture.root.join("gdlintrc"), "max-line-length: 130\n").unwrap();

    fixture
        .cmd()
        .arg("lint")
        .assert()
        .success()
        .stderr(predicate::str::contains("is not read by gdtidy"));
}

#[test]
fn lint_report_json_wraps_native_output() {
    let fixture = Fixture::new();
    fixture.seed_project();
    fixture.seed_installed_toolchain();
    fixture.write_flag("lint-fail.flag");

    fixture
        .cmd()
        .args(["lint", "--report", "json"])
        .assert()
        .failure()
        .code(12)
        .stdout(predicate::str::contains("\"findings\""))
        .stdout(predicate::str::contains("\"file\": \"test/sample.gd\""))
        .stdout(predicate::str::contains("\"rule\": \"function-name\""));

    assert_eq!(fixture.read_log("gdtidy-args.txt"), "lint --format json");
}

#[test]
fn test_defaults_to_configured_target() {
    let fixture = Fixture::new();
    fixture.seed_project();
    fixture.seed_installed_toolchain();

    fixture.cmd().arg("test").assert().success();

    assert_eq!(fixture.read_log("gdtest-args.txt"), "test/");
}

#[test]
fn test_forwards_args_exactly() {
    let fixture = Fixture::new();
    fixture.seed_project();
    fixture.seed_installed_toolchain();

    fixture
        .cmd()
        .args(["test", "--", "--filter", "combat"])
        .assert()
        .success();

    assert_eq!(fixture.read_log("gdtest-args.txt"), "--filter combat");
}

#[test]
fn health_defaults_to_configured_subcommand() {
    let fixture = Fixture::new();
    fixture.seed_project();
    fixture.seed_installed_toolchain();

    fixture.cmd().arg("health").assert().success();

    assert_eq!(fixture.read_log("gdhealth-args.txt"), "health");
    assert!(
        !fixture
            .root
            .join(".gdsuite/state/gdhealth.gdsuite.toml")
            .exists()
    );
}

#[test]
fn health_forwards_args_exactly() {
    let fixture = Fixture::new();
    fixture.seed_project();
    fixture.seed_installed_toolchain();

    fixture
        .cmd()
        .args(["health", "--", "scan", "--fail-on", "critical"])
        .assert()
        .success();

    assert_eq!(
        fixture.read_log("gdhealth-args.txt"),
        "scan --fail-on critical"
    );
}

#[test]
fn typecheck_defaults_to_dot_and_does_not_generate_checker_script() {
    let fixture = Fixture::new();
    fixture.seed_project();
    fixture.seed_installed_toolchain();

    fixture.cmd().arg("typecheck").assert().success();

    assert_eq!(fixture.read_log("gdtypecheck-args.txt"), ".");
    assert!(
        !fixture
            .root
            .join(".gdsuite/state/typecheck_checker.gd")
            .exists()
    );
}

#[test]
fn check_runs_configured_steps_in_order_and_stops_on_failure() {
    let fixture = Fixture::new();
    fixture.seed_project();
    fixture.seed_installed_toolchain();
    fixture.write_flag("typecheck-fail.flag");

    fixture
        .cmd()
        .arg("check")
        .assert()
        .failure()
        .code(12)
        .stdout(predicate::str::contains("Found 1 error."));

    assert_eq!(fixture.read_log("gdtypecheck-args.txt"), "--no-warnings .");
    assert_eq!(fixture.read_log("step-order.txt"), "fmt\nlint\ntypecheck");
    assert!(!fixture.root.join("logs/gdtest-args.txt").exists());
    assert!(!fixture.root.join("logs/gdhealth-args.txt").exists());
}

#[test]
fn delegated_tools_receive_the_godot_chosen_at_setup() {
    let fixture = Fixture::new();
    fixture.seed_project();
    fixture.seed_installed_toolchain();

    fixture
        .cmd()
        .env_remove("GODOT_EXE")
        .arg("typecheck")
        .assert()
        .success();

    assert!(
        fixture
            .read_log("gdtypecheck-godot.txt")
            .contains("godot.cmd")
    );
}

#[test]
fn doctor_uses_the_godot_remembered_from_setup() {
    let fixture = Fixture::new();
    fixture.seed_project();
    fixture.seed_installed_toolchain();
    let custom = fixture.root.join("custom-godot");
    fs::create_dir_all(&custom).unwrap();
    fs::copy(
        fixture.root.join("mock-bin/godot.cmd"),
        custom.join("godot.cmd"),
    )
    .unwrap();
    let manifest_path = fixture.root.join(".gdsuite/state/manifest.json");
    let manifest = fs::read_to_string(&manifest_path).unwrap();
    assert!(manifest.contains("mock-bin"), "{manifest}");
    fs::write(&manifest_path, manifest.replace("mock-bin", "custom-godot")).unwrap();

    fixture
        .cmd()
        .env_remove("GODOT_EXE")
        .args(["doctor", "--output", "json"])
        .assert()
        .stdout(predicate::str::contains("custom-godot"));
}

#[test]
fn old_schema_is_rejected_with_migration_guidance() {
    let fixture = Fixture::new();
    fixture.seed_project();
    fixture.seed_installed_toolchain();
    fs::write(
        fixture.root.join("gdsuite.toml"),
        fixture.legacy_gdsuite_config(),
    )
    .unwrap();

    fixture
        .cmd()
        .arg("test")
        .assert()
        .failure()
        .code(2)
        .stderr(predicate::str::contains("Run `gdsuite migrate`"))
        .stderr(predicate::str::contains("[lint] is no longer supported"));
}

#[test]
fn migrate_rewrites_legacy_config_to_orchestration_only_schema() {
    let fixture = Fixture::new();
    fixture.seed_project();
    fs::write(
        fixture.root.join("gdsuite.toml"),
        fixture.legacy_gdsuite_config(),
    )
    .unwrap();

    fixture
        .cmd()
        .arg("migrate")
        .assert()
        .success()
        .stdout(predicate::str::contains("Moved out of suite contract"))
        .stdout(predicate::str::contains("migrate complete"));

    let config = fs::read_to_string(fixture.root.join("gdsuite.toml")).unwrap();
    assert!(config.contains("[tools]"));
    assert!(config.contains("gdtypecheck_source"));
    assert!(config.contains("[check]"));
    assert!(!config.contains("[lint]"));
    assert!(!config.contains("[health]"));
    assert!(!config.contains("[gdtest]"));
    assert!(!config.contains("[gdhealth]"));
}

#[test]
fn init_writes_orchestration_only_config_and_installs_toolchain() {
    let fixture = Fixture::new();
    fixture.seed_project();
    fixture.seed_tool_sources();
    fixture.seed_path_tools();

    fixture
        .cmd()
        .arg("init")
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "[gdsuite:init] ok    initialization complete",
        ));

    let config = fs::read_to_string(fixture.root.join("gdsuite.toml")).unwrap();
    assert!(config.contains("gdtypecheck_source"));
    assert!(
        !config.contains("godot_exe"),
        "a machine's Godot path must stay out of the committed config:\n{config}"
    );
    let gitignore = fs::read_to_string(fixture.root.join(".gitignore")).unwrap();
    assert!(
        gitignore.lines().any(|line| line == ".gdsuite/"),
        "{gitignore}"
    );
    // gdtidy skips addons itself and needs no gdtoolkit configuration.
    assert!(!fixture.root.join(".gdformatrc").exists());
    assert!(!fixture.root.join(".gdlintrc").exists());
    assert!(!fixture.root.join(".gdsuite/python").exists());
    assert!(!config.contains("gdtoolkit"), "{config}");
    assert!(!fixture.root.join("gdtidy.toml").exists());
    assert!(config.contains("[check]"));
    assert!(!config.contains("[lint]"));
    assert!(!config.contains("[health]"));
    assert!(!config.contains("[gdtest]"));
    assert!(!config.contains("[gdhealth]"));
}

struct Fixture {
    _tempdir: TempDir,
    root: PathBuf,
    release_server: TestHttpServer,
}

impl Fixture {
    fn new() -> Self {
        let tempdir = tempfile::tempdir().unwrap();
        let root = tempdir.path().join("project");
        fs::create_dir_all(&root).unwrap();
        let release_server = TestHttpServer::new(HashMap::from([
            (
                "/q-rault/gdtest/releases/download/v0.2.1/gdtest-v0.2.1-x86_64-pc-windows-msvc.zip"
                    .to_string(),
                build_zip(&[
                    (
                        "gdtest-v0.2.1-x86_64-pc-windows-msvc/gdtest.cmd",
                        "@echo off\r\n@if \"%1\"==\"--version\" echo gdtest 0.2.1\r\n",
                    ),
                    (
                        "gdtest-v0.2.1-x86_64-pc-windows-msvc/addons/gdtest/gdtest_worker.gd",
                        "const HARNESS_VERSION = \"0.2.1\"\nload(\"res://addons/gdtest/gdunit4_chain.gd\")\n",
                    ),
                    (
                        "gdtest-v0.2.1-x86_64-pc-windows-msvc/addons/gdtest/gdunit4_chain.gd",
                        "",
                    ),
                    (
                        "gdtest-v0.2.1-x86_64-pc-windows-msvc/addons/gdtest/gdt.gd",
                        "",
                    ),
                    (
                        "gdtest-v0.2.1-x86_64-pc-windows-msvc/addons/gdtest_editor/plugin.gd",
                        "_panel = null\n",
                    ),
                ]),
            ),
            (
                "/q-rault/gdhealth/releases/download/v0.3.0/gdhealth-v0.3.0-x86_64-pc-windows-msvc.zip"
                    .to_string(),
                build_zip(&[(
                    "gdhealth-v0.3.0-x86_64-pc-windows-msvc/gdhealth.cmd",
                    "@echo off\r\n@if \"%1\"==\"--help\" echo gdhealth help\r\n@if \"%1\"==\"--version\" echo gdhealth 0.3.0\r\n@if \"%1\"==\"--schema-version\" echo 1\r\n@if \"%1\"==\"schema-version\" echo 1\r\n",
                )]),
            ),
            (
                "/q-rault/gdtidy/releases/download/v0.2.0/gdtidy-v0.2.0-x86_64-pc-windows-msvc.zip"
                    .to_string(),
                build_zip(&[(
                    "gdtidy-v0.2.0-x86_64-pc-windows-msvc/gdtidy.cmd",
                    &GDTIDY_CMD.replace('\n', "\r\n"),
                )]),
            ),
            (
                "/q-rault/gdtypecheck/releases/download/v0.2.1/gdtypecheck-v0.2.1-x86_64-pc-windows-msvc.zip"
                    .to_string(),
                build_zip(&[
                    (
                        "gdtypecheck-v0.2.1-x86_64-pc-windows-msvc/gdtypecheck.cmd",
                        "@echo off\r\n@if \"%1\"==\"--version\" echo gdtypecheck 0.2.1\r\n",
                    ),
                    (
                        "gdtypecheck-v0.2.1-x86_64-pc-windows-msvc/README.md",
                        "gdtypecheck\n",
                    ),
                ]),
            ),
        ]));

        Self {
            _tempdir: tempdir,
            root,
            release_server,
        }
    }

    fn cmd(&self) -> Command {
        let mut command = Command::cargo_bin("gdsuite").unwrap();
        command
            .current_dir(&self.root)
            .env("PATH", self.path_env())
            .env(
                "GDSUITE_CRATES_DOWNLOAD_BASE_URL",
                self.release_server.url(),
            )
            .env("GDSUITE_GITHUB_RELEASE_BASE_URL", self.release_server.url())
            // The mock server's archives are not the real releases the bundled
            // manifest has checksums for.
            .env("GDSUITE_SKIP_RELEASE_CHECKSUMS", "1");
        command
    }

    fn path_env(&self) -> String {
        let mut parts = vec![self.root.join("mock-bin").display().to_string()];
        if let Some(path) = std::env::var_os("PATH") {
            parts.push(path.to_string_lossy().into_owned());
        }
        parts.join(";")
    }

    fn seed_project(&self) {
        fs::create_dir_all(self.root.join("test")).unwrap();
        fs::create_dir_all(self.root.join("logs")).unwrap();
        fs::create_dir_all(self.root.join("mock-state")).unwrap();
        fs::write(
            self.root.join("project.godot"),
            "[application]\nconfig/name=\"demo\"\n",
        )
        .unwrap();
    }

    fn seed_tool_sources(&self) {
        let gdtest_root = self.root.join("../gdtest");
        let gdhealth_root = self.root.join("../gdhealth/tools/gdhealth");
        fs::create_dir_all(gdtest_root.join("addons/gdtest")).unwrap();
        fs::create_dir_all(gdtest_root.join("addons/gdtest_editor")).unwrap();
        fs::write(
            gdtest_root.join("addons/gdtest/gdtest_worker.gd"),
            "const HARNESS_VERSION = \"0.2.1\"\nload(\"res://addons/gdtest/gdunit4_chain.gd\")\nload(\"res://addons/gdtest/gdunit4_compat.gd\")\n",
        )
        .unwrap();
        fs::write(gdtest_root.join("addons/gdtest/gdunit4_chain.gd"), "").unwrap();
        fs::write(gdtest_root.join("addons/gdtest/gdunit4_compat.gd"), "").unwrap();
        fs::write(gdtest_root.join("addons/gdtest/gdt.gd"), "").unwrap();
        fs::write(
            gdtest_root.join("addons/gdtest_editor/plugin.gd"),
            "_panel = null\n",
        )
        .unwrap();
        fs::create_dir_all(gdhealth_root).unwrap();
    }

    fn seed_installed_toolchain(&self) {
        let gdsuite_dir = self.root.join(".gdsuite");
        fs::create_dir_all(gdsuite_dir.join("tools/gdtest/bin")).unwrap();
        fs::create_dir_all(gdsuite_dir.join("tools/gdhealth/bin")).unwrap();
        fs::create_dir_all(gdsuite_dir.join("tools/gdtypecheck/bin")).unwrap();
        fs::create_dir_all(gdsuite_dir.join("tools/gdtidy/bin")).unwrap();
        fs::create_dir_all(gdsuite_dir.join("state")).unwrap();
        self.seed_path_tools();
        write_cmd(&gdsuite_dir.join("tools/gdtidy/bin/gdtidy.cmd"), GDTIDY_CMD);
        write_cmd(
            &gdsuite_dir.join("tools/gdtest/bin/gdtest.cmd"),
            r#"@echo off
if "%1"=="--version" (
  echo gdtest 0.2.1
  exit /b 0
)
echo %*> "%CD%\logs\gdtest-args.txt"
>> "%CD%\logs\step-order.txt" echo test
if exist "%CD%\mock-state\test-fail.flag" exit /b 1
echo gdtest ok
"#,
        );
        write_cmd(
            &gdsuite_dir.join("tools/gdhealth/bin/gdhealth.cmd"),
            r#"@echo off
if "%1"=="--version" (
  echo gdhealth 0.3.0
  exit /b 0
)
if "%1"=="--schema-version" (
  echo 1
  exit /b 0
)
if "%1"=="schema-version" (
  echo 1
  exit /b 0
)
echo %*> "%CD%\logs\gdhealth-args.txt"
>> "%CD%\logs\step-order.txt" echo health
if exist "%CD%\mock-state\health-fail.flag" (
  mkdir "%CD%\reports\code-health" 2>nul
  > "%CD%\reports\code-health\summary.txt" echo health summary
  exit /b 1
)
"#,
        );
        write_cmd(
            &gdsuite_dir.join("tools/gdtypecheck/bin/gdtypecheck.cmd"),
            r#"@echo off
if "%1"=="--version" (
  echo gdtypecheck 0.2.1
  exit /b 0
)
echo %*> "%CD%\logs\gdtypecheck-args.txt"
echo %GODOT_EXE%> "%CD%\logs\gdtypecheck-godot.txt"
>> "%CD%\logs\step-order.txt" echo typecheck
if exist "%CD%\mock-state\typecheck-fail.flag" (
  echo test/sample.gd:41: error[gdscript]: type error
  echo gdtypecheck: 1 error^(s^), 0 warning^(s^) ^(1 scripts^)
  exit /b 1
)
echo gdtypecheck ok
"#,
        );
        fs::write(
            self.root.join("gdsuite.toml"),
            self.default_gdsuite_config(),
        )
        .unwrap();
        fs::write(
            gdsuite_dir.join("state/manifest.json"),
            format!(
                r#"{{
  "godot_exe": "{}",
  "gdtidy": "{}",
  "gdtest": "{}",
  "gdhealth": "{}",
  "gdtypecheck": "{}"
}}"#,
                escape(self.root.join("mock-bin/godot.cmd")),
                escape(gdsuite_dir.join("tools/gdtidy/bin/gdtidy.cmd")),
                escape(gdsuite_dir.join("tools/gdtest/bin/gdtest.cmd")),
                escape(gdsuite_dir.join("tools/gdhealth/bin/gdhealth.cmd")),
                escape(gdsuite_dir.join("tools/gdtypecheck/bin/gdtypecheck.cmd")),
            ),
        )
        .unwrap();
        fs::create_dir_all(self.root.join("addons/gdtest")).unwrap();
        fs::write(
            self.root.join("addons/gdtest/gdtest_worker.gd"),
            "const HARNESS_VERSION = \"0.2.1\"\n",
        )
        .unwrap();
    }

    fn seed_path_tools(&self) {
        let bin = self.root.join("mock-bin");
        fs::create_dir_all(&bin).unwrap();
        write_cmd(
            &bin.join("godot.cmd"),
            r#"@echo off
if "%1"=="--version" (
  echo Godot 4.4.1
  exit /b 0
)
if "%1"=="--headless" (
  mkdir ".godot" 2>nul
  > ".godot\global_script_class_cache.cfg" echo imported
  exit /b 0
)
echo Godot 4.4.1
"#,
        );
        write_cmd(
            &bin.join("cargo.cmd"),
            r#"@echo off
set ROOT=
set VERSION=0.1.0
:scan
if "%1"=="" goto found
if "%1"=="--version" (
  set VERSION=%2
  shift
)
if "%1"=="--root" (
  set ROOT=%2
  shift
)
shift
goto scan
:found
if not "%ROOT%"=="" (
  mkdir "%ROOT%\bin" 2>nul
  > "%ROOT%\bin\gdtest.cmd" echo @echo off
  >> "%ROOT%\bin\gdtest.cmd" echo @if "%%1"=="--version" echo gdtest 0.2.1
  > "%ROOT%\bin\gdhealth.cmd" echo @echo off
  >> "%ROOT%\bin\gdhealth.cmd" echo @if "%%1"=="--version" echo gdhealth 0.3.0
  >> "%ROOT%\bin\gdhealth.cmd" echo @if "%%1"=="--help" echo gdhealth help
  >> "%ROOT%\bin\gdhealth.cmd" echo @if "%%1"=="--schema-version" echo 1
  >> "%ROOT%\bin\gdhealth.cmd" echo @if "%%1"=="schema-version" echo 1
  > "%ROOT%\bin\gdtypecheck.cmd" echo @echo off
  >> "%ROOT%\bin\gdtypecheck.cmd" echo @if "%%1"=="--version" echo gdtypecheck %VERSION%
)
"#,
        );
    }

    fn default_gdsuite_config(&self) -> &'static str {
        r#"[project]
root = "."

[tools]
gdtest_source = "github:q-rault/gdtest@v0.1.1"
gdhealth_source = "github:q-rault/gdhealth@v0.1.1"
gdtypecheck_source = "github:q-rault/gdtypecheck@v0.1.1"

[commands]
default_test_target = "test/"
default_health_subcommand = "health"

[reports]
default = "text"
artifacts_dir = "reports/gdsuite"

[check]
steps = ["fmt", "lint", "typecheck", "health"]
"#
    }

    fn legacy_gdsuite_config(&self) -> &'static str {
        r#"[project]
root = "."

[tools]
gdtoolkit = "==4.5.0"
gdtest_source = "github:q-rault/gdtest@v0.1.1"
gdhealth_source = "github:q-rault/gdhealth@v0.1.1"

[commands]
default_test_target = "test/"
default_health_subcommand = "health"

[fix]
safe_rules = ["format"]

[lint]
line_length = 100
include = ["**/*.gd"]
exclude = ["addons/**"]

[typecheck]
include = ["**/*.gd"]
exclude = ["addons/**"]

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

[gdhealth.project]
root = "."
"#
    }

    fn write_flag(&self, name: &str) {
        fs::write(self.root.join("mock-state").join(name), "").unwrap();
    }

    fn read_log(&self, name: &str) -> String {
        fs::read_to_string(self.root.join("logs").join(name))
            .unwrap()
            .replace("\r\n", "\n")
            .trim()
            .to_string()
    }
}

struct TestHttpServer {
    url: String,
    _handle: thread::JoinHandle<()>,
}

impl TestHttpServer {
    fn new<T: Into<TestRoute>>(routes: HashMap<String, T>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let base_url = format!("http://{}", address);
        let routes = Arc::new(
            routes
                .into_iter()
                .map(|(path, route)| (path, route.into()))
                .collect::<HashMap<_, _>>(),
        );
        let handle = thread::spawn(move || {
            for stream in listener.incoming().take(16) {
                let mut stream = match stream {
                    Ok(stream) => stream,
                    Err(_) => continue,
                };
                let mut buffer = [0_u8; 4096];
                let bytes = match stream.read(&mut buffer) {
                    Ok(bytes) => bytes,
                    Err(_) => continue,
                };
                let request = String::from_utf8_lossy(&buffer[..bytes]);
                let path = request
                    .lines()
                    .next()
                    .and_then(|line| line.split_whitespace().nth(1))
                    .unwrap_or("/");
                let body = routes
                    .get(path)
                    .map(|route| route.body.clone())
                    .unwrap_or_else(|| b"not found".to_vec());
                let header = format!(
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                );
                let _ = stream.write_all(header.as_bytes());
                let response_body = if let Ok(text) = String::from_utf8(body.clone()) {
                    text.replace("__BASE_URL__", &base_url).into_bytes()
                } else {
                    body
                };
                let _ = stream.write_all(&response_body);
            }
        });

        Self {
            url: format!("http://{}", address),
            _handle: handle,
        }
    }

    fn url(&self) -> String {
        self.url.clone()
    }
}

struct TestRoute {
    body: Vec<u8>,
}

impl From<Vec<u8>> for TestRoute {
    fn from(body: Vec<u8>) -> Self {
        Self { body }
    }
}

fn build_zip(entries: &[(&str, &str)]) -> Vec<u8> {
    let mut buffer = std::io::Cursor::new(Vec::new());
    {
        let mut archive = ZipWriter::new(&mut buffer);
        let options = SimpleFileOptions::default();
        for (path, contents) in entries {
            archive.start_file(path, options).unwrap();
            archive.write_all(contents.as_bytes()).unwrap();
        }
        archive.finish().unwrap();
    }
    buffer.into_inner()
}

fn write_cmd(path: &Path, body: &str) {
    fs::write(path, body).unwrap();
}

fn escape(path: PathBuf) -> String {
    path.display().to_string().replace('\\', "\\\\")
}

#[test]
fn init_rejects_a_release_archive_whose_checksum_does_not_match() {
    let fixture = Fixture::new();
    fixture.seed_project();
    fixture.seed_tool_sources();
    fixture.seed_path_tools();

    fixture
        .cmd()
        .env_remove("GDSUITE_SKIP_RELEASE_CHECKSUMS")
        .arg("init")
        .assert()
        .failure()
        .stderr(predicate::str::contains("checksum mismatch"));
}

#[test]
fn setup_extends_an_existing_gitignore_once() {
    let fixture = Fixture::new();
    fixture.seed_project();
    fixture.seed_tool_sources();
    fixture.seed_path_tools();
    fs::write(fixture.root.join(".gitignore"), "/.godot\nmy-secrets.txt").unwrap();

    fixture.cmd().arg("init").assert().success();
    fixture.cmd().arg("setup").assert().success();

    let gitignore = fs::read_to_string(fixture.root.join(".gitignore")).unwrap();
    assert!(
        gitignore.starts_with("/.godot\nmy-secrets.txt\n"),
        "{gitignore}"
    );
    assert_eq!(gitignore.matches(".gdsuite/").count(), 1, "{gitignore}");
    assert_eq!(gitignore.matches(".godot").count(), 1, "{gitignore}");
}

#[test]
fn setup_converts_an_existing_gdlintrc_and_keeps_it() {
    let fixture = Fixture::new();
    fixture.seed_project();
    fixture.seed_tool_sources();
    fixture.seed_path_tools();
    fs::write(fixture.root.join("gdlintrc"), "custom: true\n").unwrap();

    fixture
        .cmd()
        .arg("init")
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "converted gdlintrc into gdtidy.toml",
        ))
        .stdout(predicate::str::contains("gdtidy migrate: unused-argument"));

    assert!(
        fixture
            .read_log("gdtidy-migrate-args.txt")
            .ends_with("gdlintrc")
    );
    assert!(fixture.root.join("gdtidy.toml").exists());
    assert_eq!(
        fs::read_to_string(fixture.root.join("gdlintrc")).unwrap(),
        "custom: true\n"
    );
    assert!(!fixture.root.join(".gdlintrc").exists());

    // Once converted, setup leaves gdtidy.toml alone.
    fs::write(fixture.root.join("gdtidy.toml"), "[lint]\n# edited\n").unwrap();
    fixture.cmd().arg("setup").assert().success();
    assert_eq!(
        fs::read_to_string(fixture.root.join("gdtidy.toml")).unwrap(),
        "[lint]\n# edited\n"
    );
}

#[test]
fn setup_converts_a_gdformatrc_that_gdtidy_toml_has_no_section_for() {
    let fixture = Fixture::new();
    fixture.seed_project();
    fixture.seed_tool_sources();
    fixture.seed_path_tools();
    // A project set up by an earlier gdsuite: lint converted, gdformatrc not.
    fs::write(fixture.root.join("gdtidy.toml"), "[lint]\n").unwrap();
    fs::write(
        fixture.root.join(".gdformatrc"),
        "excluded_directories: !!set\n  addons: null\n",
    )
    .unwrap();

    fixture
        .cmd()
        .arg("init")
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "converted .gdformatrc into gdtidy.toml",
        ));

    assert!(
        fixture
            .read_log("gdtidy-migrate-args.txt")
            .ends_with(".gdformatrc")
    );
    assert!(fixture.root.join(".gdformatrc").exists());
}

#[test]
fn migrate_converts_gdlintrc_once_gdtidy_is_installed() {
    let fixture = Fixture::new();
    fixture.seed_project();
    fs::write(
        fixture.root.join("gdlintrc"),
        "max-line-length: 130
",
    )
    .unwrap();
    fs::write(
        fixture.root.join("gdsuite.toml"),
        fixture.default_gdsuite_config(),
    )
    .unwrap();

    // Without gdtidy, migrate says setup will do it.
    fixture
        .cmd()
        .arg("migrate")
        .assert()
        .success()
        .stdout(predicate::str::contains("run `gdsuite setup`"));
    assert!(!fixture.root.join("gdtidy.toml").exists());

    fixture.seed_installed_toolchain();
    fixture
        .cmd()
        .arg("migrate")
        .assert()
        .success()
        .stdout(predicate::str::contains("into gdtidy.toml"));
    assert!(fixture.root.join("gdtidy.toml").exists());
    assert!(fixture.root.join("gdlintrc").exists());
}

/// Stands in for gdtidy: `--version`, `migrate` (writes gdtidy.toml), `fmt`,
/// and `lint --format json`, whose reports depend on flags in mock-state.
const GDTIDY_CMD: &str = r#"@echo off
if "%1"=="--version" (
  echo gdtidy 0.2.0
  exit /b 0
)
if "%1"=="migrate" (
  echo %*> "%CD%\logs\gdtidy-migrate-args.txt"
  > "%CD%\gdtidy.toml" echo [lint]
  >> "%CD%\gdtidy.toml" echo [fmt]
  >&2 echo gdtidy: unused-argument: no longer gdtidy's
  >&2 echo gdtidy: wrote gdtidy.toml from gdlintrc
  exit /b 0
)
if "%1"=="fmt" (
  echo %*> "%CD%\logs\gdtidy-fmt-args.txt"
  >> "%CD%\logs\step-order.txt" echo fmt
  if exist "%CD%\mock-state\fmt-fail.flag" (
    echo {"version":1,"files":2,"changed":["test/sample_a.gd","test/sample_b.gd"],"skipped":[]}
    exit /b 1
  )
  if exist "%CD%\mock-state\fmt-skip.flag" (
    echo {"version":1,"files":2,"changed":[],"skipped":[{"path":"test/broken.gd","line":2,"column":1,"reason":"syntax-error","message":"Expected end of statement."}]}
    exit /b 2
  )
  echo {"version":1,"files":1,"changed":[],"skipped":[]}
  exit /b 0
)
echo %*> "%CD%\logs\gdtidy-args.txt"
>> "%CD%\logs\step-order.txt" echo lint
if exist "%CD%\mock-state\lint-fail.flag" (
  echo {"version":1,"files":1,"fixed":0,"diagnostics":[{"path":"test/sample.gd","line":3,"column":6,"end_line":3,"end_column":13,"severity":"warning","rule":"function-name","concern":"naming:function","fixable":false,"message":"Function name \"BadName\" is not valid"}],"skipped":[]}
  exit /b 1
)
if exist "%CD%\mock-state\lint-skip.flag" (
  echo {"version":1,"files":2,"fixed":0,"diagnostics":[],"skipped":[{"path":"test/broken.gd","line":2,"column":1,"reason":"syntax-error","message":"Expected end of statement."}]}
  exit /b 2
)
echo {"version":1,"files":1,"fixed":0,"diagnostics":[],"skipped":[]}
"#;
