use std::{
    fs,
    path::{Path, PathBuf},
};

use assert_cmd::Command;
use predicates::prelude::*;
use tempfile::TempDir;

#[test]
fn setup_generates_project_local_toolchain_and_configs() {
    let fixture = Fixture::new();
    fixture.seed_project();
    fixture.seed_tool_sources();
    fixture.seed_path_tools();

    fixture
        .cmd()
        .arg("setup")
        .assert()
        .success()
        .stdout(predicate::str::contains("setup complete"));

    assert!(fixture.root.join(".gdsuite/state/manifest.json").exists());
    assert!(fixture.root.join("gdsuite.toml").exists());
    assert!(fixture.root.join("gdtest.toml").exists());
    assert!(fixture.root.join("gdhealth.toml").exists());
}

#[test]
fn doctor_reports_resolved_paths() {
    let fixture = Fixture::new();
    fixture.seed_project();
    fixture.seed_installed_toolchain();

    fixture
        .cmd()
        .arg("doctor")
        .assert()
        .success()
        .stdout(predicate::str::contains("status=ok"))
        .stdout(predicate::str::contains("gdformat="))
        .stdout(predicate::str::contains("gdhealth="));
}

#[test]
fn fmt_and_lint_fix_normalize_files() {
    let fixture = Fixture::new();
    fixture.seed_project();
    fixture.seed_installed_toolchain();

    fs::write(
        fixture.root.join("test/sample.gd"),
        "class_name Foo   \r\nextends Node   ",
    )
    .unwrap();

    fixture.cmd().args(["lint", "--fix"]).assert().success();

    let updated = fs::read_to_string(fixture.root.join("test/sample.gd")).unwrap();
    assert_eq!(updated, "class_name Foo\r\nextends Node\r\n");
}

#[test]
fn lint_with_unsafe_fixes_reorders_supported_declarations() {
    let fixture = Fixture::new();
    fixture.seed_project();
    fixture.seed_installed_toolchain();

    fs::write(
        fixture.root.join("test/order.gd"),
        "var value = 1\nextends Node\nclass_name Example\n",
    )
    .unwrap();

    fixture
        .cmd()
        .args(["lint", "--fix", "--unsafe-fixes"])
        .assert()
        .success();

    let updated = fs::read_to_string(fixture.root.join("test/order.gd")).unwrap();
    assert_eq!(updated, "class_name Example\nextends Node\nvar value = 1\n");
}

#[test]
fn lint_report_json_outputs_suite_owned_json() {
    let fixture = Fixture::new();
    fixture.seed_project();
    fixture.seed_installed_toolchain();

    fs::write(fixture.root.join("test/sample.gd"), "extends Node\n").unwrap();

    fixture
        .cmd()
        .args(["lint", "--report", "json"])
        .assert()
        .success()
        .stdout(predicate::str::contains("\"status\": \"ok\""))
        .stdout(predicate::str::contains(
            "\"summary\": \"Success: no problems found\"",
        ))
        .stdout(predicate::str::contains("\"findings\": []"));

    let gdlint_args = fs::read_to_string(fixture.root.join("logs/gdlint-args.txt")).unwrap();
    assert!(gdlint_args.contains("--verbose"));
}

#[test]
fn lint_report_json_includes_parsed_findings_on_failure() {
    let fixture = Fixture::new();
    fixture.seed_project();
    fixture.seed_installed_toolchain();

    fs::write(fixture.root.join("test/sample.gd"), "extends Node\n").unwrap();
    fs::write(fixture.root.join("mock-state/lint-fail.flag"), "").unwrap();

    fixture
        .cmd()
        .args(["lint", "--report", "json"])
        .assert()
        .failure()
        .stdout(predicate::str::contains("\"status\": \"failed\""))
        .stdout(predicate::str::contains("\"file\": \"test/sample.gd\""))
        .stdout(predicate::str::contains("\"rule\": \"function-name\""))
        .stderr(predicate::str::contains("lint failed with exit code"));
}

#[test]
fn delegated_commands_forward_arguments() {
    let fixture = Fixture::new();
    fixture.seed_project();
    fixture.seed_installed_toolchain();

    fixture
        .cmd()
        .args(["test", "--", "--filter", "combat"])
        .assert()
        .success();
    fixture
        .cmd()
        .args(["health", "--", "metrics", "--format", "json"])
        .assert()
        .success();

    let gdtest_args = fs::read_to_string(fixture.root.join("logs/gdtest-args.txt")).unwrap();
    let gdhealth_args = fs::read_to_string(fixture.root.join("logs/gdhealth-args.txt")).unwrap();
    assert!(gdtest_args.contains("--filter"));
    assert!(gdhealth_args.contains("metrics"));
}

#[test]
fn check_reports_aggregate_failure() {
    let fixture = Fixture::new();
    fixture.seed_project();
    fixture.seed_installed_toolchain();
    fs::write(fixture.root.join("mock-state/lint-fail.flag"), "").unwrap();

    fixture
        .cmd()
        .arg("check")
        .assert()
        .failure()
        .stdout(predicate::str::contains("lint:"))
        .stderr(predicate::str::contains("one or more checks failed"));
}

#[test]
fn typecheck_succeeds_and_generates_checker_script() {
    let fixture = Fixture::new();
    fixture.seed_project();
    fixture.seed_installed_toolchain();

    fixture.cmd().arg("typecheck").assert().success();

    assert!(
        fixture
            .root
            .join(".gdsuite/state/typecheck_checker.gd")
            .exists()
    );
}

#[test]
fn typecheck_fails_when_godot_exits_nonzero() {
    let fixture = Fixture::new();
    fixture.seed_project();
    fixture.seed_installed_toolchain();
    fs::write(fixture.root.join("mock-state/typecheck-fail"), "").unwrap();

    fixture.cmd().arg("typecheck").assert().failure();
}

#[test]
fn typecheck_checker_script_is_overwritten_on_rerun() {
    let fixture = Fixture::new();
    fixture.seed_project();
    fixture.seed_installed_toolchain();

    fixture.cmd().arg("typecheck").assert().success();
    let first =
        fs::read_to_string(fixture.root.join(".gdsuite/state/typecheck_checker.gd")).unwrap();
    fixture.cmd().arg("typecheck").assert().success();
    let second =
        fs::read_to_string(fixture.root.join(".gdsuite/state/typecheck_checker.gd")).unwrap();
    assert_eq!(first, second);
}

#[test]
fn check_includes_typecheck_in_failure_summary() {
    let fixture = Fixture::new();
    fixture.seed_project();
    fixture.seed_installed_toolchain();
    fs::write(fixture.root.join("mock-state/typecheck-fail"), "").unwrap();

    fixture
        .cmd()
        .arg("check")
        .assert()
        .failure()
        .stdout(predicate::str::contains("typecheck:"))
        .stderr(predicate::str::contains("one or more checks failed"));
}

#[test]
fn ci_passes_full_pipeline() {
    let fixture = Fixture::new();
    fixture.seed_project();
    fixture.seed_installed_toolchain();

    fixture
        .cmd()
        .arg("ci")
        .assert()
        .success()
        .stdout(predicate::str::contains("ci passed"));
}

#[test]
fn ci_reports_aggregate_failure() {
    let fixture = Fixture::new();
    fixture.seed_project();
    fixture.seed_installed_toolchain();
    fs::write(fixture.root.join("mock-state/lint-fail.flag"), "").unwrap();

    fixture
        .cmd()
        .arg("ci")
        .assert()
        .failure()
        .stdout(predicate::str::contains("lint:"))
        .stderr(predicate::str::contains("one or more checks failed"));
}

#[test]
fn check_does_not_run_test_or_health() {
    let fixture = Fixture::new();
    fixture.seed_project();
    fixture.seed_installed_toolchain();

    fixture.cmd().arg("check").assert().success();

    assert!(
        !fixture.root.join("logs/gdtest-args.txt").exists(),
        "check should not invoke gdtest"
    );
    assert!(
        !fixture.root.join("logs/gdhealth-args.txt").exists(),
        "check should not invoke gdhealth"
    );
}

#[test]
fn doctor_fails_with_corrupted_manifest() {
    let fixture = Fixture::new();
    fixture.seed_project();
    fixture.seed_installed_toolchain();
    fs::write(
        fixture.root.join(".gdsuite/state/manifest.json"),
        "{ not valid json",
    )
    .unwrap();

    fixture.cmd().arg("doctor").assert().failure();
}

#[test]
fn commands_fail_with_invalid_gdsuite_toml() {
    let fixture = Fixture::new();
    fixture.seed_project();
    fixture.seed_installed_toolchain();
    fs::write(fixture.root.join("gdsuite.toml"), "[ not valid toml").unwrap();

    fixture.cmd().args(["lint"]).assert().failure();
}

#[test]
fn setup_is_rerunnable_with_partial_state() {
    let fixture = Fixture::new();
    fixture.seed_project();
    fixture.seed_tool_sources();
    fixture.seed_path_tools();
    fs::create_dir_all(fixture.root.join(".gdsuite/state")).unwrap();

    fixture
        .cmd()
        .arg("setup")
        .assert()
        .success()
        .stdout(predicate::str::contains("setup complete"));
}

struct Fixture {
    _tempdir: TempDir,
    root: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let tempdir = TempDir::new().unwrap();
        let root = tempdir.path().join("project");
        fs::create_dir_all(&root).unwrap();
        Self {
            _tempdir: tempdir,
            root,
        }
    }

    fn cmd(&self) -> Command {
        let mut command = Command::cargo_bin("gdsuite").unwrap();
        command.current_dir(&self.root).env("PATH", self.path_env());
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
        fs::create_dir_all(self.root.join("client")).unwrap();
        fs::create_dir_all(self.root.join("server")).unwrap();
        fs::create_dir_all(self.root.join("logs")).unwrap();
        fs::create_dir_all(self.root.join("mock-state")).unwrap();
        fs::write(
            self.root.join("project.godot"),
            "[application]\nconfig/name=\"demo\"\n",
        )
        .unwrap();
    }

    fn seed_tool_sources(&self) {
        fs::create_dir_all(self.root.join("../gdtest")).unwrap();
        fs::create_dir_all(self.root.join("../gdhealth/tools/gdhealth")).unwrap();
    }

    fn seed_path_tools(&self) {
        let bin = self.root.join("mock-bin");
        fs::create_dir_all(&bin).unwrap();
        write_cmd(
            &bin.join("godot.cmd"),
            r#"@echo off
if "%1"=="--version" goto :version
if "%1"=="--headless" goto :headless
echo Godot 4.4.1
exit /b 0
:version
echo Godot 4.4.1
exit /b 0
:headless
if exist "mock-state\typecheck-fail" exit /b 1
echo typecheck: ok
exit /b 0
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
  >> "%3\Scripts\python.cmd" echo @if "%%1"=="-m" if "%%2"=="pip" echo pip ok
  > "%3\Scripts\gdformat.cmd" echo @echo gdformat ok
  > "%3\Scripts\gdlint.cmd" echo @echo gdlint ok
  exit /b 0
)
if "%1"=="-m" if "%2"=="pip" (
  echo pip install %3 %4
  exit /b 0
)
"#,
        );
        write_cmd(
            &bin.join("cargo.cmd"),
            r#"@echo off
mkdir "%5\bin" 2>nul
if "%5" neq "" (
  > "%5\bin\gdtest.cmd" echo @echo off
  >> "%5\bin\gdtest.cmd" echo @if "%%1"=="--version" echo gdtest 0.1.0
  >> "%5\bin\gdtest.cmd" echo @if "%%1"=="init" echo init^> "%CD%\logs\gdtest-init.txt"
  >> "%5\bin\gdtest.cmd" echo @if not "%%1"=="init" if not "%%1"=="--version" echo %%*^> "%CD%\logs\gdtest-args.txt"
  > "%5\bin\gdhealth.cmd" echo @echo off
  >> "%5\bin\gdhealth.cmd" echo @if "%%1"=="--version" echo gdhealth 0.1.0
  >> "%5\bin\gdhealth.cmd" echo @if not "%%1"=="--version" echo %%*^> "%CD%\logs\gdhealth-args.txt"
)
"#,
        );
    }

    fn seed_installed_toolchain(&self) {
        let gdsuite_dir = self.root.join(".gdsuite");
        fs::create_dir_all(gdsuite_dir.join("python/Scripts")).unwrap();
        fs::create_dir_all(gdsuite_dir.join("tools/gdtest/bin")).unwrap();
        fs::create_dir_all(gdsuite_dir.join("tools/gdhealth/bin")).unwrap();
        fs::create_dir_all(gdsuite_dir.join("state")).unwrap();
        fs::create_dir_all(self.root.join("logs")).unwrap();
        fs::create_dir_all(self.root.join("mock-state")).unwrap();
        write_cmd(
            &gdsuite_dir.join("python/Scripts/python.cmd"),
            "@echo Python 3.12.0",
        );
        write_cmd(
            &gdsuite_dir.join("python/Scripts/gdformat.cmd"),
            "@echo gdformat ok",
        );
        write_cmd(
            &gdsuite_dir.join("python/Scripts/gdlint.cmd"),
            r#"@echo off
echo %*> "%CD%\logs\gdlint-args.txt"
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
  echo gdtest 0.1.0
  exit /b 0
)
if "%1"=="init" (
  echo init>"%CD%\logs\gdtest-init.txt"
  exit /b 0
)
echo %*> "%CD%\logs\gdtest-args.txt"
"#,
        );
        write_cmd(
            &gdsuite_dir.join("tools/gdhealth/bin/gdhealth.cmd"),
            r#"@echo off
if "%1"=="--version" (
  echo gdhealth 0.1.0
  exit /b 0
)
echo %*> "%CD%\logs\gdhealth-args.txt"
"#,
        );
        fs::write(
            self.root.join("gdsuite.toml"),
            r#"[project]
root = "."

[tools]
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
"#,
        )
        .unwrap();
        fs::write(
            gdsuite_dir.join("state/manifest.json"),
            format!(
                r#"{{
  "godot_exe": "{}",
  "python": "{}",
  "gdformat": "{}",
  "gdlint": "{}",
  "gdtest": "{}",
  "gdhealth": "{}"
}}"#,
                escape(self.root.join("mock-bin/godot.cmd")),
                escape(gdsuite_dir.join("python/Scripts/python.cmd")),
                escape(gdsuite_dir.join("python/Scripts/gdformat.cmd")),
                escape(gdsuite_dir.join("python/Scripts/gdlint.cmd")),
                escape(gdsuite_dir.join("tools/gdtest/bin/gdtest.cmd")),
                escape(gdsuite_dir.join("tools/gdhealth/bin/gdhealth.cmd"))
            ),
        )
        .unwrap();
        fs::write(self.root.join("gdtest.toml"), "test_dir = \"test/\"\n").unwrap();
        fs::write(self.root.join("gdhealth.toml"), "[groups]\n").unwrap();
        self.seed_path_tools();
    }
}

fn write_cmd(path: &Path, body: &str) {
    fs::write(path, body).unwrap();
}

fn escape(path: PathBuf) -> String {
    path.display().to_string().replace('\\', "\\\\")
}
