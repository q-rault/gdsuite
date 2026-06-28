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
use flate2::{Compression, write::GzEncoder};
use predicates::prelude::*;
use tar::Builder;
use tempfile::TempDir;
use zip::{ZipWriter, write::SimpleFileOptions};

#[test]
fn fmt_defaults_to_dot() {
    let fixture = Fixture::new();
    fixture.seed_project();
    fixture.seed_installed_toolchain();

    fixture.cmd().arg("fmt").assert().success();

    assert_eq!(fixture.read_log("gdformat-args.txt"), ".");
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

    assert_eq!(fixture.read_log("gdformat-args.txt"), "--check src test");
}

#[test]
fn lint_defaults_to_dot() {
    let fixture = Fixture::new();
    fixture.seed_project();
    fixture.seed_installed_toolchain();

    fixture.cmd().arg("lint").assert().success();

    assert_eq!(fixture.read_log("gdlint-args.txt"), ".");
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
        .stdout(predicate::str::contains("\"file\": \"test/sample.gd\""));

    assert_eq!(fixture.read_log("gdlint-args.txt"), "--verbose .");
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
    assert!(!fixture.root.join(".gdsuite/state/gdhealth.gdsuite.toml").exists());
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

    assert_eq!(fixture.read_log("gdhealth-args.txt"), "scan --fail-on critical");
}

#[test]
fn typecheck_defaults_to_dot_and_does_not_generate_checker_script() {
    let fixture = Fixture::new();
    fixture.seed_project();
    fixture.seed_installed_toolchain();

    fixture.cmd().arg("typecheck").assert().success();

    assert_eq!(fixture.read_log("gdtypecheck-args.txt"), ".");
    assert!(!fixture.root.join(".gdsuite/state/typecheck_checker.gd").exists());
}

#[test]
fn check_runs_configured_steps_in_order_and_stops_on_failure() {
    let fixture = Fixture::new();
    fixture.seed_project();
    fixture.seed_installed_toolchain();
    fixture.write_flag("typecheck-fail.flag");

    fixture.cmd().arg("check").assert().failure().code(12);

    assert_eq!(
        fixture.read_log("step-order.txt"),
        "fmt\nlint\ntypecheck"
    );
    assert!(!fixture.root.join("logs/gdtest-args.txt").exists());
    assert!(!fixture.root.join("logs/gdhealth-args.txt").exists());
}

#[test]
fn old_schema_is_rejected_with_migration_guidance() {
    let fixture = Fixture::new();
    fixture.seed_project();
    fixture.seed_installed_toolchain();
    fs::write(fixture.root.join("gdsuite.toml"), fixture.legacy_gdsuite_config()).unwrap();

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
    fs::write(fixture.root.join("gdsuite.toml"), fixture.legacy_gdsuite_config()).unwrap();

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
        .stdout(predicate::str::contains("[gdsuite:init] ok    initialization complete"));

    let config = fs::read_to_string(fixture.root.join("gdsuite.toml")).unwrap();
    assert!(config.contains("gdtypecheck_source"));
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
                "/gdtypecheck/0.1.0/download".to_string(),
                build_tar_gz(&[("gdtypecheck-0.1.0/README.md", "gdtypecheck\n")]),
            ),
            (
                "/q-rault/gdtest/releases/download/v0.1.1/gdtest-v0.1.1-x86_64-pc-windows-msvc.zip"
                    .to_string(),
                build_zip(&[
                    (
                        "gdtest-v0.1.1-x86_64-pc-windows-msvc/gdtest.cmd",
                        "@echo off\r\n@if \"%1\"==\"--version\" echo gdtest 0.1.1\r\n",
                    ),
                    (
                        "gdtest-v0.1.1-x86_64-pc-windows-msvc/addons/gdtest/gdtest_worker.gd",
                        "const HARNESS_VERSION = \"0.1.1\"\nload(\"res://addons/gdtest/gdunit4_chain.gd\")\n",
                    ),
                    (
                        "gdtest-v0.1.1-x86_64-pc-windows-msvc/addons/gdtest/gdunit4_chain.gd",
                        "",
                    ),
                    (
                        "gdtest-v0.1.1-x86_64-pc-windows-msvc/addons/gdtest/gdt.gd",
                        "",
                    ),
                    (
                        "gdtest-v0.1.1-x86_64-pc-windows-msvc/addons/gdtest_editor/plugin.gd",
                        "_panel = null\n",
                    ),
                ]),
            ),
            (
                "/q-rault/gdhealth/releases/download/v0.1.1/gdhealth-v0.1.1-x86_64-pc-windows-msvc.zip"
                    .to_string(),
                build_zip(&[(
                    "gdhealth-v0.1.1-x86_64-pc-windows-msvc/gdhealth.cmd",
                    "@echo off\r\n@if \"%1\"==\"--help\" echo gdhealth help\r\n@if \"%1\"==\"--version\" echo gdhealth 0.1.1\r\n@if \"%1\"==\"--schema-version\" echo 1\r\n@if \"%1\"==\"schema-version\" echo 1\r\n",
                )]),
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
            .env("GDSUITE_GITHUB_RELEASE_BASE_URL", self.release_server.url());
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
            "const HARNESS_VERSION = \"0.1.1\"\nload(\"res://addons/gdtest/gdunit4_chain.gd\")\nload(\"res://addons/gdtest/gdunit4_compat.gd\")\n",
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
        fs::create_dir_all(gdsuite_dir.join("python/Scripts")).unwrap();
        fs::create_dir_all(gdsuite_dir.join("tools/gdtest/bin")).unwrap();
        fs::create_dir_all(gdsuite_dir.join("tools/gdhealth/bin")).unwrap();
        fs::create_dir_all(gdsuite_dir.join("tools/gdtypecheck/bin")).unwrap();
        fs::create_dir_all(gdsuite_dir.join("state")).unwrap();
        self.seed_path_tools();
        write_cmd(
            &gdsuite_dir.join("python/Scripts/python.cmd"),
            "@echo off\r\nif \"%1\"==\"--version\" echo Python 3.12.0\r\nif \"%1\"==\"-m\" if \"%2\"==\"pip\" if \"%3\"==\"show\" if \"%4\"==\"gdtoolkit\" echo Version: 4.5.0\r\n",
        );
        write_cmd(
            &gdsuite_dir.join("python/Scripts/gdformat.cmd"),
            r#"@echo off
echo %*> "%CD%\logs\gdformat-args.txt"
>> "%CD%\logs\step-order.txt" echo fmt
if exist "%CD%\mock-state\fmt-fail.flag" (
  echo test/sample_a.gd
  echo test/sample_b.gd
  exit /b 1
)
echo gdformat ok
"#,
        );
        write_cmd(
            &gdsuite_dir.join("python/Scripts/gdlint.cmd"),
            r#"@echo off
echo %*> "%CD%\logs\gdlint-args.txt"
>> "%CD%\logs\step-order.txt" echo lint
if exist "%CD%\mock-state\lint-fail.flag" (
  echo test/sample.gd:3: Error: Function name "BadName" is not valid ^(function-name^)
  >&2 echo Failure: 1 problem found
  exit /b 1
)
echo Success: no problems found
"#,
        );
        write_cmd(
            &gdsuite_dir.join("tools/gdtest/bin/gdtest.cmd"),
            r#"@echo off
if "%1"=="--version" (
  echo gdtest 0.1.1
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
  echo gdhealth 0.1.1
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
  echo gdtypecheck 0.1.0
  exit /b 0
)
echo %*> "%CD%\logs\gdtypecheck-args.txt"
>> "%CD%\logs\step-order.txt" echo typecheck
if exist "%CD%\mock-state\typecheck-fail.flag" (
  >&2 echo test/sample.gd:41 type error
  exit /b 1
)
echo gdtypecheck ok
"#,
        );
        fs::write(self.root.join("gdsuite.toml"), self.default_gdsuite_config()).unwrap();
        fs::write(
            gdsuite_dir.join("state/manifest.json"),
            format!(
                r#"{{
  "godot_exe": "{}",
  "python": "{}",
  "gdformat": "{}",
  "gdlint": "{}",
  "gdtest": "{}",
  "gdhealth": "{}",
  "gdtypecheck": "{}"
}}"#,
                escape(self.root.join("mock-bin/godot.cmd")),
                escape(gdsuite_dir.join("python/Scripts/python.cmd")),
                escape(gdsuite_dir.join("python/Scripts/gdformat.cmd")),
                escape(gdsuite_dir.join("python/Scripts/gdlint.cmd")),
                escape(gdsuite_dir.join("tools/gdtest/bin/gdtest.cmd")),
                escape(gdsuite_dir.join("tools/gdhealth/bin/gdhealth.cmd")),
                escape(gdsuite_dir.join("tools/gdtypecheck/bin/gdtypecheck.cmd")),
            ),
        )
        .unwrap();
        fs::create_dir_all(self.root.join("addons/gdtest")).unwrap();
        fs::write(
            self.root.join("addons/gdtest/gdtest_worker.gd"),
            "const HARNESS_VERSION = \"0.1.1\"\n",
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
            &bin.join("python.cmd"),
            r#"@echo off
if "%1"=="--version" (
  echo Python 3.12.0
  exit /b 0
)
if "%1"=="-m" if "%2"=="venv" (
  mkdir "%3\Scripts" 2>nul
  > "%3\Scripts\python.cmd" echo @echo off
  >> "%3\Scripts\python.cmd" echo @if "%%1"=="--version" echo Python 3.12.0
  >> "%3\Scripts\python.cmd" echo @if "%%1"=="-m" if "%%2"=="pip" if "%%3"=="install" echo pip ok
  >> "%3\Scripts\python.cmd" echo @if "%%1"=="-m" if "%%2"=="pip" if "%%3"=="show" if "%%4"=="gdtoolkit" echo Version: 4.5.0
  > "%3\Scripts\gdformat.cmd" echo @echo off
  >> "%3\Scripts\gdformat.cmd" echo @if "%%1"=="--help" echo gdformat help
  > "%3\Scripts\gdlint.cmd" echo @echo off
  >> "%3\Scripts\gdlint.cmd" echo @if "%%1"=="--help" echo gdlint help
  exit /b 0
)
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
  >> "%ROOT%\bin\gdtest.cmd" echo @if "%%1"=="--version" echo gdtest 0.1.1
  > "%ROOT%\bin\gdhealth.cmd" echo @echo off
  >> "%ROOT%\bin\gdhealth.cmd" echo @if "%%1"=="--version" echo gdhealth 0.1.1
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
gdtoolkit = "==4.5.0"
gdtest_source = "github:q-rault/gdtest@v0.1.1"
gdhealth_source = "github:q-rault/gdhealth@v0.1.1"
gdtypecheck_source = "crate:gdtypecheck@0.1.0"

[commands]
default_test_target = "test/"
default_health_subcommand = "health"

[reports]
default = "text"
artifacts_dir = "reports/gdsuite"

[check]
steps = ["fmt", "lint", "typecheck", "test", "health"]
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

fn build_tar_gz(entries: &[(&str, &str)]) -> Vec<u8> {
    let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
    {
        let mut archive = Builder::new(&mut encoder);
        for (path, contents) in entries {
            let mut header = tar::Header::new_gnu();
            header.set_path(path).unwrap();
            header.set_mode(0o644);
            header.set_size(contents.len() as u64);
            header.set_cksum();
            archive.append(&header, contents.as_bytes()).unwrap();
        }
        archive.finish().unwrap();
    }
    encoder.finish().unwrap()
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
