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
        .stdout(predicate::str::contains(
            "[gdsuite:setup] step  installing local toolchain",
        ))
        .stdout(predicate::str::contains("[gdsuite:setup] ok    setup complete"));

    assert!(fixture.root.join(".gdsuite/state/manifest.json").exists());
    assert!(fixture.root.join("gdsuite.toml").exists());
    assert!(fixture.root.join("gdtest.toml").exists());
    assert!(fixture.root.join("gdhealth.toml").exists());
    assert!(fixture.root.join("addons/gdtest/gdtest_worker.gd").exists());
    assert!(fixture.root.join("addons/gdtest_editor/plugin.gd").exists());
}

#[test]
fn init_generates_project_local_toolchain_and_configs() {
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
            "[gdsuite:init] step  installing local toolchain",
        ))
        .stdout(predicate::str::contains(
            "[gdsuite:init] step  refreshing Godot class cache",
        ))
        .stdout(predicate::str::contains(
            "[gdsuite:init] ok    initialization complete",
        ));

    assert!(fixture.root.join(".gdsuite/state/manifest.json").exists());
    assert!(fixture.root.join("gdsuite.toml").exists());
    assert!(fixture.root.join("gdtest.toml").exists());
    assert!(fixture.root.join("gdhealth.toml").exists());
    assert!(fixture.root.join("addons/gdtest/gdtest_worker.gd").exists());
    assert!(fixture.root.join("addons/gdtest_editor/plugin.gd").exists());

    let config = fs::read_to_string(fixture.root.join("gdsuite.toml")).unwrap();
    assert!(config.contains("[reports]"));
    assert!(config.contains("[health]"));

    let gdtest = fs::read_to_string(fixture.root.join("gdtest.toml")).unwrap();
    assert!(gdtest.contains("patterns = [\"test/**/*.gd\", \"**/*.test.gd\"]"));
    assert!(gdtest.contains("[profile.ci]"));
    assert!(gdtest.contains("reporter = \"junit\""));

    let gdhealth = fs::read_to_string(fixture.root.join("gdhealth.toml")).unwrap();
    assert!(gdhealth.contains("[discovery]"));
    assert!(gdhealth.contains("include = [\"**/*.gd\"]"));
    assert!(gdhealth.contains("[groups]"));
}

#[test]
fn init_builds_godot_class_cache_after_installing_gdtest_addon() {
    let fixture = Fixture::new();
    fixture.seed_project();
    fixture.seed_tool_sources();
    fixture.seed_path_tools();

    fixture.cmd().arg("init").assert().success();

    assert!(
        fixture
            .root
            .join(".godot/global_script_class_cache.cfg")
            .exists()
    );
    let godot_args = fs::read_to_string(fixture.root.join("mock-state/godot-args.txt")).unwrap();
    assert!(godot_args.contains("--import"));
}

#[test]
fn init_accepts_godot_exe_as_command_name_from_env() {
    let fixture = Fixture::new();
    fixture.seed_project();
    fixture.seed_tool_sources();
    fixture.seed_path_tools();

    fixture
        .cmd()
        .env("GODOT_EXE", "godot")
        .arg("init")
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "[gdsuite:init] step  installing local toolchain",
        ))
        .stdout(predicate::str::contains(
            "[gdsuite:init] ok    initialization complete",
        ));

    assert!(fixture.root.join(".gdsuite/state/manifest.json").exists());
}

#[test]
fn setup_supports_crate_sources() {
    let fixture = Fixture::new();
    fixture.seed_project();
    fixture.seed_path_tools();

    let server = TestHttpServer::new(HashMap::from([
        (
            "/gdtest/0.4.0/download".to_string(),
            build_tar_gz(&[
                (
                    "gdtest-0.4.0/addons/gdtest/gdtest_worker.gd",
                    "const HARNESS_VERSION = \"0.4.0\"\nload(\"res://addons/gdtest/gdunit4_chain.gd\")\n",
                ),
                ("gdtest-0.4.0/addons/gdtest/gdunit4_chain.gd", ""),
                (
                    "gdtest-0.4.0/addons/gdtest_editor/plugin.gd",
                    "_panel = null\n",
                ),
            ]),
        ),
        (
            "/gdhealth/0.7.1/download".to_string(),
            build_tar_gz(&[("gdhealth-0.7.1/README.md", "gdhealth\n")]),
        ),
    ]));

    fs::write(
        fixture.root.join("gdsuite.toml"),
        fixture.remote_gdsuite_config("crate:gdtest@0.4.0", "crate:gdhealth@0.7.1"),
    )
    .unwrap();

    fixture
        .cmd()
        .env("GDSUITE_CRATES_DOWNLOAD_BASE_URL", server.url())
        .arg("setup")
        .assert()
        .success()
        .stdout(predicate::str::contains("[gdsuite:setup] ok    setup complete"));

    assert!(fixture.root.join("addons/gdtest/gdtest_worker.gd").exists());
    assert!(fixture.root.join("addons/gdtest_editor/plugin.gd").exists());
    let installation =
        fs::read_to_string(fixture.root.join(".gdsuite/state/installation.json")).unwrap();
    assert!(installation.contains("\"crate\": \"gdtest\""));
    assert!(installation.contains("\"version\": \"0.4.0\""));
    assert!(installation.contains("\"crate\": \"gdhealth\""));
    assert!(installation.contains("\"version\": \"0.7.1\""));
}

#[test]
fn setup_supports_github_release_archives() {
    let fixture = Fixture::new();
    fixture.seed_project();
    fixture.seed_path_tools();

    let server = TestHttpServer::new(HashMap::from([
        (
            "/gdtest.tar.gz".to_string(),
            build_tar_gz(&[
                (
                    "gdtest-release/gdtest.cmd",
                    "@echo off\n@if \"%1\"==\"--version\" echo gdtest 0.4.0\n",
                ),
                (
                    "gdtest-release/addons/gdtest/gdtest_worker.gd",
                    "const HARNESS_VERSION = \"0.4.0\"\nload(\"res://addons/gdtest/gdunit4_chain.gd\")\n",
                ),
                ("gdtest-release/addons/gdtest/gdunit4_chain.gd", ""),
                (
                    "gdtest-release/addons/gdtest_editor/plugin.gd",
                    "_panel = null\n",
                ),
            ]),
        ),
        (
            "/gdhealth.tar.gz".to_string(),
            build_tar_gz(&[(
                "gdhealth-release/gdhealth.cmd",
                "@echo off\n@if \"%1\"==\"--help\" echo gdhealth help\n@if \"%1\"==\"--version\" echo gdhealth 0.4.0\n@if \"%1\"==\"--schema-version\" echo 1\n@if \"%1\"==\"schema-version\" echo 1\n",
            )]),
        ),
    ]));

    fs::write(
        fixture.root.join("gdsuite.toml"),
        fixture.remote_gdsuite_config(
            &format!("github:{}/gdtest.tar.gz", server.url()),
            &format!("github:{}/gdhealth.tar.gz", server.url()),
        ),
    )
    .unwrap();

    fixture.cmd().arg("setup").assert().success();

    assert!(
        fixture
            .root
            .join(".gdsuite/tools/gdtest/bin/gdtest.cmd")
            .exists()
    );
    assert!(
        fixture
            .root
            .join(".gdsuite/tools/gdhealth/bin/gdhealth.cmd")
            .exists()
    );
    assert!(fixture.root.join("addons/gdtest/gdtest_worker.gd").exists());
}

#[test]
fn setup_supports_github_release_repo_with_auth_token() {
    let fixture = Fixture::new();
    fixture.seed_project();
    fixture.seed_path_tools();

    let server = TestHttpServer::new(HashMap::from([
        (
            "/repos/q-rault/gdtest/releases/tags/v0.1.1".to_string(),
            TestRoute::auth(
                br#"{"assets":[{"name":"gdtest-v0.1.1-x86_64-pc-windows-msvc.zip","url":"__BASE_URL__/api/assets/gdtest"}]}"#.to_vec(),
                "Bearer secret-token",
            ),
        ),
        (
            "/repos/q-rault/gdhealth/releases/tags/v0.1.1".to_string(),
            TestRoute::auth(
                br#"{"assets":[{"name":"gdhealth-v0.1.1-x86_64-pc-windows-msvc.zip","url":"__BASE_URL__/api/assets/gdhealth"}]}"#.to_vec(),
                "Bearer secret-token",
            ),
        ),
        (
            "/api/assets/gdtest".to_string(),
            TestRoute::auth(
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
                        "gdtest-v0.1.1-x86_64-pc-windows-msvc/addons/gdtest_editor/plugin.gd",
                        "_panel = null\n",
                    ),
                ]),
                "Bearer secret-token",
            ),
        ),
        (
            "/api/assets/gdhealth".to_string(),
            TestRoute::auth(
                build_zip(&[(
                    "gdhealth-v0.1.1-x86_64-pc-windows-msvc/gdhealth.cmd",
                    "@echo off\r\n@if \"%1\"==\"--version\" echo gdhealth 0.1.1\r\n@if \"%1\"==\"--schema-version\" echo 1\r\n@if \"%1\"==\"schema-version\" echo 1\r\n",
                )]),
                "Bearer secret-token",
            ),
        ),
    ]));

    fixture
        .cmd()
        .env("GDSUITE_GITHUB_API_BASE_URL", server.url())
        .env("GDSUITE_GITHUB_TOKEN", "secret-token")
        .arg("setup")
        .assert()
        .success()
        .stdout(predicate::str::contains("[gdsuite:setup] ok    setup complete"));

    assert!(
        fixture
            .root
            .join(".gdsuite/tools/gdtest/bin/gdtest.cmd")
            .exists()
    );
    assert!(
        fixture
            .root
            .join(".gdsuite/tools/gdhealth/bin/gdhealth.cmd")
            .exists()
    );
}

#[test]
fn setup_loads_github_token_from_dotenv() {
    let fixture = Fixture::new();
    fixture.seed_project();
    fixture.seed_path_tools();

    let server = TestHttpServer::new(HashMap::from([
        (
            "/repos/q-rault/gdtest/releases/tags/v0.1.1".to_string(),
            TestRoute::auth(
                br#"{"assets":[{"name":"gdtest-v0.1.1-x86_64-pc-windows-msvc.zip","url":"__BASE_URL__/api/assets/gdtest"}]}"#.to_vec(),
                "Bearer secret-token",
            ),
        ),
        (
            "/repos/q-rault/gdhealth/releases/tags/v0.1.1".to_string(),
            TestRoute::auth(
                br#"{"assets":[{"name":"gdhealth-v0.1.1-x86_64-pc-windows-msvc.zip","url":"__BASE_URL__/api/assets/gdhealth"}]}"#.to_vec(),
                "Bearer secret-token",
            ),
        ),
        (
            "/api/assets/gdtest".to_string(),
            TestRoute::auth(
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
                        "gdtest-v0.1.1-x86_64-pc-windows-msvc/addons/gdtest_editor/plugin.gd",
                        "_panel = null\n",
                    ),
                ]),
                "Bearer secret-token",
            ),
        ),
        (
            "/api/assets/gdhealth".to_string(),
            TestRoute::auth(
                build_zip(&[(
                    "gdhealth-v0.1.1-x86_64-pc-windows-msvc/gdhealth.cmd",
                    "@echo off\r\n@if \"%1\"==\"--version\" echo gdhealth 0.1.1\r\n@if \"%1\"==\"--schema-version\" echo 1\r\n@if \"%1\"==\"schema-version\" echo 1\r\n",
                )]),
                "Bearer secret-token",
            ),
        ),
    ]));

    fs::write(
        fixture.root.join(".env"),
        "GDSUITE_GITHUB_TOKEN=secret-token\n",
    )
    .unwrap();

    fixture
        .cmd()
        .env_remove("GDSUITE_GITHUB_TOKEN")
        .env("GDSUITE_GITHUB_API_BASE_URL", server.url())
        .arg("setup")
        .assert()
        .success()
        .stdout(predicate::str::contains("[gdsuite:setup] ok    setup complete"));

    assert!(
        fixture
            .root
            .join(".gdsuite/tools/gdtest/bin/gdtest.cmd")
            .exists()
    );
}

#[test]
fn setup_supports_non_sibling_local_paths() {
    let fixture = Fixture::new();
    fixture.seed_project();
    fixture.seed_path_tools();

    let gdtest_root = fixture.root.join("vendor").join("tools").join("gdtest-src");
    let gdhealth_root = fixture
        .root
        .join("external")
        .join("delegates")
        .join("gdhealth-src");
    fixture.seed_tool_sources_at(&gdtest_root, &gdhealth_root);

    fs::write(
        fixture.root.join("gdsuite.toml"),
        fixture.remote_gdsuite_config(
            "path:vendor/tools/gdtest-src",
            "external/delegates/gdhealth-src",
        ),
    )
    .unwrap();

    fixture
        .cmd()
        .arg("setup")
        .assert()
        .success()
        .stdout(predicate::str::contains("[gdsuite:setup] ok    setup complete"));

    assert!(
        fixture
            .root
            .join(".gdsuite/tools/gdtest/bin/gdtest.cmd")
            .exists()
    );
    assert!(
        fixture
            .root
            .join(".gdsuite/tools/gdhealth/bin/gdhealth.cmd")
            .exists()
    );
    assert!(fixture.root.join("addons/gdtest/gdtest_worker.gd").exists());
    assert!(fixture.root.join("addons/gdtest_editor/plugin.gd").exists());
}

#[test]
fn setup_supports_git_sources_with_subdir() {
    let fixture = Fixture::new();
    fixture.seed_project();
    fixture.seed_path_tools();

    let gdtest_root = fixture.root.join("vendor").join("gdtest-src");
    let gdhealth_repo = fixture.root.join("vendor").join("gdhealth-repo");
    let gdhealth_root = gdhealth_repo.join("tools").join("gdhealth");
    fixture.seed_tool_sources_at(&gdtest_root, &gdhealth_root);
    fs::write(
        gdhealth_root.join("Cargo.toml"),
        "[package]\nname = \"gdhealth\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
    )
    .unwrap();
    fs::create_dir_all(gdhealth_root.join("src")).unwrap();
    fs::write(gdhealth_root.join("src/main.rs"), "fn main() {}\n").unwrap();

    Command::new("git")
        .args(["init"])
        .current_dir(&gdhealth_repo)
        .assert()
        .success();
    Command::new("git")
        .args(["config", "user.email", "tests@example.com"])
        .current_dir(&gdhealth_repo)
        .assert()
        .success();
    Command::new("git")
        .args(["config", "user.name", "Tests"])
        .current_dir(&gdhealth_repo)
        .assert()
        .success();
    Command::new("git")
        .args(["add", "."])
        .current_dir(&gdhealth_repo)
        .assert()
        .success();
    Command::new("git")
        .args(["commit", "-m", "seed gdhealth repo"])
        .current_dir(&gdhealth_repo)
        .assert()
        .success();
    let head = std::process::Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(&gdhealth_repo)
        .output()
        .unwrap();
    let rev = String::from_utf8(head.stdout).unwrap().trim().to_string();

    let repo_url = format!(
        "file:///{}",
        gdhealth_repo.display().to_string().replace('\\', "/")
    );
    fs::write(
        fixture.root.join("gdsuite.toml"),
        fixture.remote_gdsuite_config(
            "path:vendor/gdtest-src",
            &format!("git:{repo_url}?rev={rev}&subdir=tools/gdhealth"),
        ),
    )
    .unwrap();

    fixture.cmd().arg("setup").assert().success();

    assert!(
        fixture
            .root
            .join(".gdsuite/tools/gdhealth/bin/gdhealth.cmd")
            .exists()
    );
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
        .stdout(predicate::str::contains("[gdsuite:doctor] status ok"))
        .stdout(predicate::str::contains("[gdsuite:doctor] info  gdformat"))
        .stdout(predicate::str::contains("[gdsuite:doctor] info  gdhealth"));
}

#[test]
fn doctor_warns_when_gdhealth_config_drifts_from_canonical_sections() {
    let fixture = Fixture::new();
    fixture.seed_project();
    fixture.seed_installed_toolchain();
    fs::write(
        fixture.root.join("gdhealth.toml"),
        "[project]\nroot = \"src\"\n",
    )
    .unwrap();

    fixture
        .cmd()
        .arg("doctor")
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "[gdsuite:doctor] warn  gdhealth config drift from gdsuite.toml:",
        ))
        .stdout(predicate::str::contains("project.root"))
        .stdout(predicate::str::contains("[gdsuite:doctor] status warning"));
}

#[test]
fn doctor_warns_when_gdtest_config_misses_suite_defaults() {
    let fixture = Fixture::new();
    fixture.seed_project();
    fixture.seed_installed_toolchain();
    fs::write(
        fixture.root.join("gdtest.toml"),
        "patterns = [\"test/**/*.gd\"]\n",
    )
    .unwrap();

    fixture
        .cmd()
        .arg("doctor")
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "[gdsuite:doctor] warn  gdtest config drift from gdsuite.toml:",
        ))
        .stdout(predicate::str::contains("profile.ci.reporter"))
        .stdout(predicate::str::contains("[gdsuite:doctor] status warning"));
}

#[test]
fn init_plain_output_uses_stable_prefixed_steps() {
    let fixture = Fixture::new();
    fixture.seed_project();
    fixture.seed_tool_sources();
    fixture.seed_path_tools();

    fixture
        .cmd()
        .args(["init", "--output", "plain"])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "[gdsuite:init] step=installing local toolchain",
        ))
        .stdout(predicate::str::contains("[gdsuite:init] status=ok"))
        .stdout(predicate::str::contains(
            "[gdsuite:init] summary=initialization complete",
        ));
}

#[test]
fn doctor_plain_output_keeps_prefixed_key_value_lines() {
    let fixture = Fixture::new();
    fixture.seed_project();
    fixture.seed_installed_toolchain();

    fixture
        .cmd()
        .args(["doctor", "--output", "plain"])
        .assert()
        .success()
        .stdout(predicate::str::contains("[gdsuite:doctor] project_root="))
        .stdout(predicate::str::contains("[gdsuite:doctor] gdtest_compat=ok"))
        .stdout(predicate::str::contains("[gdsuite:doctor] status=ok"));
}

#[test]
fn doctor_json_output_emits_structured_report() {
    let fixture = Fixture::new();
    fixture.seed_project();
    fixture.seed_installed_toolchain();

    fixture
        .cmd()
        .args(["doctor", "--output", "json"])
        .assert()
        .success()
        .stdout(predicate::str::contains("\"command\": \"doctor\""))
        .stdout(predicate::str::contains("\"status\": \"ok\""))
        .stdout(predicate::str::contains("\"compatibility\""));
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
fn delegated_test_uses_default_target_without_forwarded_args() {
    let fixture = Fixture::new();
    fixture.seed_project();
    fixture.seed_installed_toolchain();

    fixture.cmd().arg("test").assert().success();

    let gdtest_args = fs::read_to_string(fixture.root.join("logs/gdtest-args.txt")).unwrap();
    assert!(gdtest_args.contains("test/"));
}

#[test]
fn test_report_json_emits_suite_envelope_and_writes_artifact() {
    let fixture = Fixture::new();
    fixture.seed_project();
    fixture.seed_installed_toolchain();

    fixture
        .cmd()
        .args(["test", "--report", "json"])
        .assert()
        .success()
        .stdout(predicate::str::contains("\"command\": \"test\""))
        .stdout(predicate::str::contains("\"name\": \"test\""))
        .stdout(predicate::str::contains("\"status\": \"ok\""));

    assert!(fixture.root.join("reports/gdsuite/test.json").exists());
    assert!(fixture.root.join("reports/gdsuite/latest.json").exists());
}

#[test]
fn delegated_health_uses_default_subcommand_without_forwarded_args() {
    let fixture = Fixture::new();
    fixture.seed_project();
    fixture.seed_installed_toolchain();

    fixture.cmd().arg("health").assert().success();

    let gdhealth_args = fs::read_to_string(fixture.root.join("logs/gdhealth-args.txt")).unwrap();
    assert!(gdhealth_args.contains("health"));
    assert!(gdhealth_args.contains("--fail-on"));
    assert!(gdhealth_args.contains("critical"));
}

#[test]
fn test_uses_built_in_defaults_when_gdsuite_config_is_missing() {
    let fixture = Fixture::new();
    fixture.seed_project();
    fixture.seed_installed_toolchain();
    fs::remove_file(fixture.root.join("gdsuite.toml")).unwrap();

    fixture.cmd().arg("test").assert().success();

    let gdtest_args = fs::read_to_string(fixture.root.join("logs/gdtest-args.txt")).unwrap();
    assert!(gdtest_args.contains("test/"));
}

#[test]
fn health_uses_built_in_defaults_when_gdsuite_config_is_missing() {
    let fixture = Fixture::new();
    fixture.seed_project();
    fixture.seed_installed_toolchain();
    fs::remove_file(fixture.root.join("gdsuite.toml")).unwrap();

    fixture.cmd().arg("health").assert().success();

    let gdhealth_args = fs::read_to_string(fixture.root.join("logs/gdhealth-args.txt")).unwrap();
    assert!(gdhealth_args.contains("health"));
    assert!(gdhealth_args.contains("--fail-on"));
    assert!(gdhealth_args.contains("critical"));
}

#[test]
fn health_uses_overlay_config_that_enforces_suite_excludes() {
    let fixture = Fixture::new();
    fixture.seed_project();
    fixture.seed_installed_toolchain();
    fs::write(
        fixture.root.join("gdhealth.toml"),
        "[discovery]\ninclude = [\"**/*.gd\"]\nexclude = [\"addons/**\", \".godot/**\", \"reports/**\"]\n\n[groups]\nlegacy = [\"*.gd\"]\n",
    )
    .unwrap();

    fixture.cmd().arg("health").assert().success();

    let gdhealth_args = fs::read_to_string(fixture.root.join("logs/gdhealth-args.txt")).unwrap();
    assert!(gdhealth_args.contains("--config"));
    assert!(gdhealth_args.contains(".gdsuite\\state\\gdhealth.gdsuite.toml"));

    let overlay =
        fs::read_to_string(fixture.root.join(".gdsuite/state/gdhealth.gdsuite.toml")).unwrap();
    assert!(overlay.contains("\".gdsuite/**\""));
    assert!(overlay.contains("legacy = [\"*.gd\"]"));
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
fn check_report_json_emits_suite_envelope_and_writes_artifact() {
    let fixture = Fixture::new();
    fixture.seed_project();
    fixture.seed_installed_toolchain();

    fixture
        .cmd()
        .args(["check", "--report", "json"])
        .assert()
        .success()
        .stdout(predicate::str::contains("\"command\": \"check\""))
        .stdout(predicate::str::contains("\"status\": \"ok\""))
        .stdout(predicate::str::contains("\"name\": \"fmt\""))
        .stdout(predicate::str::contains("\"name\": \"lint\""))
        .stdout(predicate::str::contains("\"name\": \"typecheck\""));

    assert!(fixture.root.join("reports/gdsuite/check.json").exists());
    assert!(fixture.root.join("reports/gdsuite/latest.json").exists());
}

#[test]
fn check_uses_built_in_defaults_when_gdsuite_config_is_missing() {
    let fixture = Fixture::new();
    fixture.seed_project();
    fixture.seed_installed_toolchain();
    fs::remove_file(fixture.root.join("gdsuite.toml")).unwrap();

    fixture.cmd().arg("check").assert().success();
}

#[test]
fn check_uses_configured_default_report_mode_when_flag_is_omitted() {
    let fixture = Fixture::new();
    fixture.seed_project();
    fixture.seed_installed_toolchain();
    fs::write(
        fixture.root.join("gdsuite.toml"),
        fixture
            .read_gdsuite_config()
            .replace("default = \"text\"", "default = \"json\""),
    )
    .unwrap();

    fixture
        .cmd()
        .arg("check")
        .assert()
        .success()
        .stdout(predicate::str::contains("\"command\": \"check\""))
        .stdout(predicate::str::contains("\"status\": \"ok\""));

    assert!(fixture.root.join("reports/gdsuite/check.json").exists());
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
fn health_report_json_references_bundle_artifacts_when_present() {
    let fixture = Fixture::new();
    fixture.seed_project();
    fixture.seed_installed_toolchain();
    fs::create_dir_all(fixture.root.join("reports/code-health")).unwrap();
    fs::write(
        fixture.root.join("reports/code-health/summary.txt"),
        "health summary",
    )
    .unwrap();
    fs::write(
        fixture.root.join("reports/code-health/health.json"),
        "{\"status\":\"ok\"}",
    )
    .unwrap();

    fixture
        .cmd()
        .args(["health", "--report", "json"])
        .assert()
        .success()
        .stdout(predicate::str::contains("\"command\": \"health\""))
        .stdout(predicate::str::contains("\"name\": \"summary\""))
        .stdout(predicate::str::contains("\"name\": \"health_json\""));
}

#[test]
fn health_report_json_writes_artifact_even_when_health_fails() {
    let fixture = Fixture::new();
    fixture.seed_project();
    fixture.seed_installed_toolchain();
    fs::write(fixture.root.join("mock-state/health-fail.flag"), "").unwrap();

    fixture
        .cmd()
        .args(["health", "--report", "json"])
        .assert()
        .failure()
        .stdout(predicate::str::contains("\"command\": \"health\""))
        .stdout(predicate::str::contains("\"status\": \"failed\""))
        .stdout(predicate::str::contains("\"name\": \"health_json\""))
        .stderr(predicate::str::contains("health failed"));

    assert!(fixture.root.join("reports/gdsuite/health.json").exists());
    assert!(fixture.root.join("reports/gdsuite/latest.json").exists());
}

#[test]
fn report_reads_latest_saved_report() {
    let fixture = Fixture::new();
    fixture.seed_project();
    fixture.seed_installed_toolchain();

    fixture
        .cmd()
        .args(["check", "--report", "json"])
        .assert()
        .success();

    fixture
        .cmd()
        .arg("report")
        .assert()
        .success()
        .stdout(predicate::str::contains("\"command\": \"check\""));
}

#[test]
fn report_ci_aggregates_failed_health_report_artifact() {
    let fixture = Fixture::new();
    fixture.seed_project();
    fixture.seed_installed_toolchain();
    fs::write(fixture.root.join("mock-state/health-fail.flag"), "").unwrap();

    fixture
        .cmd()
        .args(["check", "--report", "json"])
        .assert()
        .success();
    fixture
        .cmd()
        .args(["test", "--report", "json"])
        .assert()
        .success();
    fixture
        .cmd()
        .args(["health", "--report", "json"])
        .assert()
        .failure();

    fixture
        .cmd()
        .args(["report", "ci"])
        .assert()
        .success()
        .stdout(predicate::str::contains("\"command\": \"ci\""))
        .stdout(predicate::str::contains("\"status\": \"failed\""))
        .stdout(predicate::str::contains("\"name\": \"health\""));
}

#[test]
fn report_ci_aggregates_explicit_suite_step_artifacts() {
    let fixture = Fixture::new();
    fixture.seed_project();
    fixture.seed_installed_toolchain();

    fixture
        .cmd()
        .args(["check", "--report", "json"])
        .assert()
        .success();
    fixture
        .cmd()
        .args(["test", "--report", "json"])
        .assert()
        .success();
    fixture
        .cmd()
        .args(["health", "--report", "json"])
        .assert()
        .success();

    fixture
        .cmd()
        .args(["report", "ci"])
        .assert()
        .success()
        .stdout(predicate::str::contains("\"command\": \"ci\""))
        .stdout(predicate::str::contains("\"name\": \"fmt\""))
        .stdout(predicate::str::contains("\"name\": \"test\""))
        .stdout(predicate::str::contains("\"name\": \"health\""));

    assert!(fixture.root.join("reports/gdsuite/ci.json").exists());
    assert!(fixture.root.join("reports/gdsuite/latest.json").exists());
}

#[test]
fn report_ci_fails_with_actionable_guidance_when_artifacts_are_missing() {
    let fixture = Fixture::new();
    fixture.seed_project();
    fixture.seed_installed_toolchain();

    fixture
        .cmd()
        .args(["check", "--report", "json"])
        .assert()
        .success();

    fixture
        .cmd()
        .args(["report", "ci"])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "run `gdsuite check --report json`",
        ))
        .stderr(predicate::str::contains("before `gdsuite report ci`"));
}

#[test]
fn report_fails_with_actionable_guidance_when_no_report_exists() {
    let fixture = Fixture::new();
    fixture.seed_project();
    fixture.seed_installed_toolchain();

    fixture
        .cmd()
        .arg("report")
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "run `gdsuite check --report json`",
        ));
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
fn doctor_succeeds_with_corrupted_manifest() {
    let fixture = Fixture::new();
    fixture.seed_project();
    fixture.seed_installed_toolchain();
    fs::write(
        fixture.root.join(".gdsuite/state/manifest.json"),
        "{ not valid json",
    )
    .unwrap();

    // Doctor is resilient to a corrupted manifest: it falls back to path-based
    // tool discovery and exits 0 so the user can still read the diagnosis.
    fixture
        .cmd()
        .arg("doctor")
        .assert()
        .success()
        .stdout(predicate::str::contains("[gdsuite:doctor] status ok"));
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
fn commands_fail_with_invalid_report_mode_in_config() {
    let fixture = Fixture::new();
    fixture.seed_project();
    fixture.seed_installed_toolchain();
    fs::write(
        fixture.root.join("gdsuite.toml"),
        fixture
            .read_gdsuite_config()
            .replace("default = \"text\"", "default = \"yaml\""),
    )
    .unwrap();

    fixture
        .cmd()
        .arg("check")
        .assert()
        .failure()
        .stderr(predicate::str::contains("unsupported report mode `yaml`"));
}

#[test]
fn commands_fail_with_init_guidance_when_project_is_not_bootstrapped() {
    let fixture = Fixture::new();
    fixture.seed_project();

    fixture
        .cmd()
        .arg("check")
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "run `gdsuite init` or `gdsuite setup`",
        ));
}

#[test]
fn commands_fail_with_setup_guidance_when_manifest_is_missing() {
    let fixture = Fixture::new();
    fixture.seed_project();
    fixture.seed_installed_toolchain();
    fs::remove_file(fixture.root.join(".gdsuite/state/manifest.json")).unwrap();

    fixture
        .cmd()
        .arg("check")
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "run `gdsuite init` or `gdsuite setup`",
        ));
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
        .stdout(predicate::str::contains("[gdsuite:setup] ok    setup complete"));
}

#[test]
fn init_is_safe_when_existing_gdtest_addon_files_are_modified() {
    let fixture = Fixture::new();
    fixture.seed_project();
    fixture.seed_tool_sources();
    fixture.seed_path_tools();
    fs::create_dir_all(fixture.root.join("addons/gdtest")).unwrap();
    fs::write(
        fixture.root.join("addons/gdtest/README.md"),
        "locally modified addon readme\n",
    )
    .unwrap();

    fixture
        .cmd()
        .arg("init")
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "[gdsuite:init] warn  gdtest addon install skipped modified file",
        ));

    assert_eq!(
        fs::read_to_string(fixture.root.join("addons/gdtest/README.md")).unwrap(),
        "locally modified addon readme\n"
    );
    assert!(fixture.root.join("addons/gdtest/gdtest_worker.gd").exists());
}

#[test]
fn init_installs_gdtest_addon_into_custom_harness_directory() {
    let fixture = Fixture::new();
    fixture.seed_project();
    fixture.seed_tool_sources();
    fixture.seed_path_tools();
    fs::write(
        fixture.root.join("gdsuite.toml"),
        fixture.default_gdsuite_config().replace(
            "harness = \"addons/gdtest/gdtest_worker.gd\"",
            "harness = \"vendor/gdtest/gdtest_worker.gd\"",
        ),
    )
    .unwrap();

    fixture.cmd().arg("init").assert().success();

    assert!(fixture.root.join("vendor/gdtest/gdtest_worker.gd").exists());
    let worker = fs::read_to_string(fixture.root.join("vendor/gdtest/gdtest_worker.gd")).unwrap();
    assert!(worker.contains("res://vendor/gdtest/gdunit4_chain.gd"));
    let gdtest = fs::read_to_string(fixture.root.join("gdtest.toml")).unwrap();
    assert!(gdtest.contains("harness = \"vendor/gdtest/gdtest_worker.gd\""));
}

#[test]
fn setup_sync_product_configs_persists_suite_excludes_into_existing_gdhealth_config() {
    let fixture = Fixture::new();
    fixture.seed_project();
    fixture.seed_tool_sources();
    fixture.seed_path_tools();
    fs::write(
        fixture.root.join("gdhealth.toml"),
        "[discovery]\ninclude = [\"**/*.gd\"]\nexclude = [\"addons/**\", \".godot/**\", \"reports/**\"]\n\n[groups]\nlegacy = [\"*.gd\"]\n",
    )
    .unwrap();

    fixture
        .cmd()
        .args(["setup", "--sync-product-configs"])
        .assert()
        .success()
        .stdout(predicate::str::contains("[gdsuite:setup] ok    setup complete"));

    let gdhealth = fs::read_to_string(fixture.root.join("gdhealth.toml")).unwrap();
    assert!(!gdhealth.contains("\".gdsuite/**\""));
    assert!(gdhealth.contains("legacy = [\"*.gd\"]"));
}

#[test]
fn setup_sync_product_configs_persists_suite_defaults_into_existing_gdtest_config() {
    let fixture = Fixture::new();
    fixture.seed_project();
    fixture.seed_tool_sources();
    fixture.seed_path_tools();
    fs::write(
        fixture.root.join("gdtest.toml"),
        "patterns = [\"spec/**/*.gd\"]\n[profile.ci]\nreporter = \"text\"\n",
    )
    .unwrap();

    fixture
        .cmd()
        .args(["setup", "--sync-product-configs"])
        .assert()
        .success();

    let gdtest = fs::read_to_string(fixture.root.join("gdtest.toml")).unwrap();
    assert!(gdtest.contains("reporter = \"junit\""));
    assert!(gdtest.contains("fail_fast = true"));
    assert!(gdtest.contains("patterns = [\"test/**/*.gd\", \"**/*.test.gd\"]"));
}

#[test]
fn migrate_syncs_suite_configs_and_rewrites_obvious_entrypoints() {
    let fixture = Fixture::new();
    fixture.seed_project();
    fs::create_dir_all(fixture.root.join(".github/workflows")).unwrap();
    fs::create_dir_all(fixture.root.join("docs")).unwrap();
    fs::write(
        fixture.root.join("gdtest.toml"),
        "patterns = [\"spec/**/*.gd\"]\n",
    )
    .unwrap();
    fs::write(
        fixture.root.join("gdhealth.toml"),
        "[discovery]\nexclude = [\"addons/**\"]\n",
    )
    .unwrap();
    fs::write(
        fixture.root.join(".github/workflows/ci.yml"),
        "steps:\n  - run: gdtest --filter combat\n  - run: gdhealth health --fail-on critical\n",
    )
    .unwrap();
    fs::write(
        fixture.root.join("README.md"),
        "Run `gdtest` and `gdhealth health`.\nReference: https://github.com/q-rault/gdtest\nAddon path: addons/gdtest/\n",
    )
    .unwrap();

    fixture.cmd().arg("migrate").assert().success();

    let gdsuite = fs::read_to_string(fixture.root.join("gdsuite.toml")).unwrap();
    assert!(gdsuite.contains("[reports]"));

    let gdtest = fs::read_to_string(fixture.root.join("gdtest.toml")).unwrap();
    assert!(gdtest.contains("patterns = [\"spec/**/*.gd\"]"));
    assert!(gdtest.contains("reporter = \"junit\""));

    let gdhealth = fs::read_to_string(fixture.root.join("gdhealth.toml")).unwrap();
    assert!(!gdhealth.contains("\".gdsuite/**\""));

    let workflow = fs::read_to_string(fixture.root.join(".github/workflows/ci.yml")).unwrap();
    assert!(workflow.contains("gdsuite test --filter combat"));
    assert!(workflow.contains("gdsuite health --fail-on critical"));

    let readme = fs::read_to_string(fixture.root.join("README.md")).unwrap();
    assert!(readme.contains("`gdsuite test`"));
    assert!(readme.contains("`gdsuite health`"));
    assert!(readme.contains("https://github.com/q-rault/gdtest"));
    assert!(readme.contains("addons/gdtest/"));
}

#[test]
fn migrate_is_idempotent() {
    let fixture = Fixture::new();
    fixture.seed_project();
    fs::write(
        fixture.root.join("gdhealth.toml"),
        "[discovery]\nexclude = [\"addons/**\"]\n",
    )
    .unwrap();

    fixture.cmd().arg("migrate").assert().success();
    let first_gdsuite = fs::read_to_string(fixture.root.join("gdsuite.toml")).unwrap();
    let first_gdtest = fs::read_to_string(fixture.root.join("gdtest.toml")).unwrap();
    let first_gdhealth = fs::read_to_string(fixture.root.join("gdhealth.toml")).unwrap();

    fixture.cmd().arg("migrate").assert().success();
    let second_gdsuite = fs::read_to_string(fixture.root.join("gdsuite.toml")).unwrap();
    let second_gdtest = fs::read_to_string(fixture.root.join("gdtest.toml")).unwrap();
    let second_gdhealth = fs::read_to_string(fixture.root.join("gdhealth.toml")).unwrap();

    assert_eq!(first_gdsuite, second_gdsuite);
    assert_eq!(first_gdtest, second_gdtest);
    assert_eq!(first_gdhealth, second_gdhealth);
}

#[test]
fn migrate_then_setup_sync_produces_no_doctor_drift_warnings() {
    let fixture = Fixture::new();
    fixture.seed_project();
    fixture.seed_tool_sources();
    fixture.seed_path_tools();
    fs::write(
        fixture.root.join("gdtest.toml"),
        "patterns = [\"spec/**/*.gd\"]\n[profile.ci]\nreporter = \"text\"\n",
    )
    .unwrap();
    fs::write(
        fixture.root.join("gdhealth.toml"),
        "[project]\nroot = \".\"\n\n[discovery]\ninclude = [\"**/*.gd\"]\nexclude = [\"addons/**\"]\n\n[groups]\nlegacy = [\"*.gd\"]\n",
    )
    .unwrap();

    fixture.cmd().arg("migrate").assert().success();
    fixture
        .cmd()
        .args(["setup", "--sync-product-configs"])
        .assert()
        .success();
    fixture
        .cmd()
        .arg("doctor")
        .assert()
        .success()
        .stdout(predicate::str::contains("drift from gdsuite.toml").not());
}

#[test]
fn existing_product_configs_are_not_overwritten() {
    let fixture = Fixture::new();
    fixture.seed_project();
    fixture.seed_tool_sources();
    fixture.seed_path_tools();
    fs::write(fixture.root.join("gdtest.toml"), "reporter = \"json\"\n").unwrap();
    fs::write(
        fixture.root.join("gdhealth.toml"),
        "[groups]\ncustom = [\"mods/**/*.gd\"]\n",
    )
    .unwrap();

    fixture.cmd().arg("init").assert().success();

    let gdtest = fs::read_to_string(fixture.root.join("gdtest.toml")).unwrap();
    let gdhealth = fs::read_to_string(fixture.root.join("gdhealth.toml")).unwrap();
    assert_eq!(gdtest, "reporter = \"json\"\n");
    assert_eq!(gdhealth, "[groups]\ncustom = [\"mods/**/*.gd\"]\n");
}

#[derive(Clone)]
struct TestRoute {
    body: Vec<u8>,
    expected_auth: Option<String>,
}

struct Fixture {
    _tempdir: TempDir,
    root: PathBuf,
    release_server: TestHttpServer,
}

impl Fixture {
    fn new() -> Self {
        let tempdir = TempDir::new().unwrap();
        let root = tempdir.path().join("project");
        fs::create_dir_all(&root).unwrap();
        let release_server = TestHttpServer::new(HashMap::from([
            (
                "/gdtest/0.1.1/download".to_string(),
                build_tar_gz(&[
                    (
                        "gdtest-0.1.1/addons/gdtest/gdtest_worker.gd",
                        "const HARNESS_VERSION = \"0.1.1\"\nload(\"res://addons/gdtest/gdunit4_chain.gd\")\nload(\"res://addons/gdtest/gdunit4_compat.gd\")\n",
                    ),
                    (
                        "gdtest-0.1.1/addons/gdtest/gdunit4_compat.gd",
                        "const GDUNIT4_CHAIN := preload(\"res://addons/gdtest/gdunit4_chain.gd\")\n",
                    ),
                    ("gdtest-0.1.1/addons/gdtest/gdunit4_chain.gd", ""),
                    ("gdtest-0.1.1/addons/gdtest/gdt.gd", ""),
                    ("gdtest-0.1.1/addons/gdtest/gd_test.gd", ""),
                    ("gdtest-0.1.1/addons/gdtest/scene_runner.gd", ""),
                    ("gdtest-0.1.1/addons/gdtest/README.md", "runtime readme\n"),
                    ("gdtest-0.1.1/addons/gdtest/LICENSE", "license\n"),
                    (
                        "gdtest-0.1.1/addons/gdtest_editor/plugin.gd",
                        "_panel = preload(\"res://addons/gdtest_editor/test_panel.gd\").new()\n",
                    ),
                    ("gdtest-0.1.1/addons/gdtest_editor/test_panel.gd", ""),
                    ("gdtest-0.1.1/addons/gdtest_editor/plugin.cfg", ""),
                    (
                        "gdtest-0.1.1/addons/gdtest_editor/README.md",
                        "editor readme\n",
                    ),
                    ("gdtest-0.1.1/addons/gdtest_editor/LICENSE", "license\n"),
                ]),
            ),
            (
                "/gdhealth/0.1.1/download".to_string(),
                build_tar_gz(&[("gdhealth-0.1.1/README.md", "gdhealth\n")]),
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
                        "const HARNESS_VERSION = \"0.1.1\"\nload(\"res://addons/gdtest/gdunit4_chain.gd\")\nload(\"res://addons/gdtest/gdunit4_compat.gd\")\n",
                    ),
                    (
                        "gdtest-v0.1.1-x86_64-pc-windows-msvc/addons/gdtest/gdunit4_compat.gd",
                        "const GDUNIT4_CHAIN := preload(\"res://addons/gdtest/gdunit4_chain.gd\")\n",
                    ),
                    (
                        "gdtest-v0.1.1-x86_64-pc-windows-msvc/addons/gdtest/gdunit4_chain.gd",
                        "",
                    ),
                    ("gdtest-v0.1.1-x86_64-pc-windows-msvc/addons/gdtest/gdt.gd", ""),
                    (
                        "gdtest-v0.1.1-x86_64-pc-windows-msvc/addons/gdtest/gd_test.gd",
                        "",
                    ),
                    (
                        "gdtest-v0.1.1-x86_64-pc-windows-msvc/addons/gdtest/scene_runner.gd",
                        "",
                    ),
                    (
                        "gdtest-v0.1.1-x86_64-pc-windows-msvc/addons/gdtest/README.md",
                        "runtime readme\n",
                    ),
                    (
                        "gdtest-v0.1.1-x86_64-pc-windows-msvc/addons/gdtest/LICENSE",
                        "license\n",
                    ),
                    (
                        "gdtest-v0.1.1-x86_64-pc-windows-msvc/addons/gdtest_editor/plugin.gd",
                        "_panel = preload(\"res://addons/gdtest_editor/test_panel.gd\").new()\n",
                    ),
                    (
                        "gdtest-v0.1.1-x86_64-pc-windows-msvc/addons/gdtest_editor/test_panel.gd",
                        "",
                    ),
                    (
                        "gdtest-v0.1.1-x86_64-pc-windows-msvc/addons/gdtest_editor/plugin.cfg",
                        "",
                    ),
                    (
                        "gdtest-v0.1.1-x86_64-pc-windows-msvc/addons/gdtest_editor/README.md",
                        "editor readme\n",
                    ),
                    (
                        "gdtest-v0.1.1-x86_64-pc-windows-msvc/addons/gdtest_editor/LICENSE",
                        "license\n",
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
        let gdtest_root = self.root.join("../gdtest");
        let gdhealth_root = self.root.join("../gdhealth/tools/gdhealth");
        self.seed_tool_sources_at(&gdtest_root, &gdhealth_root);
    }

    fn seed_tool_sources_at(&self, gdtest_root: &Path, gdhealth_root: &Path) {
        fs::create_dir_all(gdtest_root.join("addons/gdtest")).unwrap();
        fs::create_dir_all(gdtest_root.join("addons/gdtest_editor")).unwrap();
        fs::write(
            gdtest_root.join("addons/gdtest/gdtest_worker.gd"),
            "const HARNESS_VERSION = \"0.1.1\"\nload(\"res://addons/gdtest/gdunit4_chain.gd\")\nload(\"res://addons/gdtest/gdunit4_compat.gd\")\n",
        )
        .unwrap();
        fs::write(
            gdtest_root.join("addons/gdtest/gdunit4_compat.gd"),
            "const GDUNIT4_CHAIN := preload(\"res://addons/gdtest/gdunit4_chain.gd\")\n",
        )
        .unwrap();
        fs::write(gdtest_root.join("addons/gdtest/gdunit4_chain.gd"), "").unwrap();
        fs::write(gdtest_root.join("addons/gdtest/gdt.gd"), "").unwrap();
        fs::write(gdtest_root.join("addons/gdtest/gd_test.gd"), "").unwrap();
        fs::write(gdtest_root.join("addons/gdtest/scene_runner.gd"), "").unwrap();
        fs::write(
            gdtest_root.join("addons/gdtest/README.md"),
            "runtime readme\n",
        )
        .unwrap();
        fs::write(gdtest_root.join("addons/gdtest/LICENSE"), "license\n").unwrap();
        fs::write(
            gdtest_root.join("addons/gdtest_editor/plugin.gd"),
            "_panel = preload(\"res://addons/gdtest_editor/test_panel.gd\").new()\n",
        )
        .unwrap();
        fs::write(gdtest_root.join("addons/gdtest_editor/test_panel.gd"), "").unwrap();
        fs::write(gdtest_root.join("addons/gdtest_editor/plugin.cfg"), "").unwrap();
        fs::write(
            gdtest_root.join("addons/gdtest_editor/README.md"),
            "editor readme\n",
        )
        .unwrap();
        fs::write(
            gdtest_root.join("addons/gdtest_editor/LICENSE"),
            "license\n",
        )
        .unwrap();
        fs::create_dir_all(gdhealth_root).unwrap();
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
echo %*> "%CD%\mock-state\godot-args.txt"
echo %* | findstr /C:"--import" >nul
if not errorlevel 1 (
  mkdir ".godot" 2>nul
  > ".godot\global_script_class_cache.cfg" echo imported
)
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
  >> "%3\Scripts\python.cmd" echo @if "%%1"=="-m" if "%%2"=="pip" if "%%3"=="install" echo pip ok
  >> "%3\Scripts\python.cmd" echo @if "%%1"=="-m" if "%%2"=="pip" if "%%3"=="show" if "%%4"=="gdtoolkit" echo Version: 4.5.0
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
set ROOT=
set VERSION=0.1.1
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
  >> "%ROOT%\bin\gdtest.cmd" echo @if "%%1"=="--version" echo gdtest %VERSION%
  >> "%ROOT%\bin\gdtest.cmd" echo @if "%%1"=="init" echo init^> "%CD%\logs\gdtest-init.txt"
  >> "%ROOT%\bin\gdtest.cmd" echo @if not "%%1"=="init" if not "%%1"=="--version" echo %%*^> "%CD%\logs\gdtest-args.txt"
  > "%ROOT%\bin\gdhealth.cmd" echo @echo off
  >> "%ROOT%\bin\gdhealth.cmd" echo @if "%%1"=="--version" echo gdhealth %VERSION%
  >> "%ROOT%\bin\gdhealth.cmd" echo @if "%%1"=="--schema-version" echo 1
  >> "%ROOT%\bin\gdhealth.cmd" echo @if "%%1"=="schema-version" echo 1
  >> "%ROOT%\bin\gdhealth.cmd" echo @if not "%%1"=="--version" echo %%*^> "%CD%\logs\gdhealth-args.txt"
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
            "@echo off\r\nif \"%1\"==\"--version\" echo Python 3.12.0\r\nif \"%1\"==\"-m\" if \"%2\"==\"pip\" if \"%3\"==\"show\" if \"%4\"==\"gdtoolkit\" echo Version: 4.5.0\r\nif \"%1\"==\"-m\" if \"%2\"==\"pip\" if \"%3\"==\"install\" echo pip ok\r\n",
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
  echo gdtest 0.1.1
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
if exist "%CD%\mock-state\health-fail.flag" (
  mkdir "%CD%\reports\code-health" 2>nul
  > "%CD%\reports\code-health\summary.txt" echo health summary
  > "%CD%\reports\code-health\health.json" echo {"status":"failed"}
  >&2 echo policy threshold triggered
  exit /b 1
)
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
        fs::write(
            self.root.join("gdtest.toml"),
            "display_mode = \"headless\"\nharness = \"addons/gdtest/gdtest_worker.gd\"\npatterns = [\"test/**/*.gd\", \"**/*.test.gd\"]\ncoverage_exclude = [\"addons/**\", \".godot/**\", \"reports/**\", \".gdsuite/**\"]\n\n[profile.ci]\nreporter = \"junit\"\nfail_fast = true\n",
        )
        .unwrap();
        fs::write(
            self.root.join("gdhealth.toml"),
            "[project]\nroot = \".\"\n\n[discovery]\ninclude = [\"**/*.gd\"]\nexclude = [\"addons/**\", \".godot/**\", \"reports/**\"]\n\n[groups]\nclient = [\"client/**/*.gd\", \"client/**\"]\nserver = [\"server/**/*.gd\", \"server/**\"]\ntest = [\"test/**/*.gd\", \"test/**\"]\nroot_gameplay = [\"*.gd\", \"!test/**\", \"!server/**\", \"!client/**\"]\n",
        )
        .unwrap();
        fs::create_dir_all(self.root.join("addons/gdtest")).unwrap();
        fs::write(
            self.root.join("addons/gdtest/gdtest_worker.gd"),
            "const HARNESS_VERSION = \"0.1.1\"\n",
        )
        .unwrap();
        self.seed_path_tools();
    }

    fn default_gdsuite_config(&self) -> &'static str {
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
"#
    }

    fn read_gdsuite_config(&self) -> String {
        fs::read_to_string(self.root.join("gdsuite.toml")).unwrap()
    }

    fn remote_gdsuite_config(&self, gdtest_source: &str, gdhealth_source: &str) -> String {
        self.default_gdsuite_config()
            .replace(
                "gdtest_source = \"github:q-rault/gdtest@v0.1.1\"",
                &format!("gdtest_source = \"{gdtest_source}\""),
            )
            .replace(
                "gdhealth_source = \"github:q-rault/gdhealth@v0.1.1\"",
                &format!("gdhealth_source = \"{gdhealth_source}\""),
            )
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
            for stream in listener.incoming().take(8) {
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
                if let Some(route) = routes.get(path) {
                    let auth = request.lines().find_map(|line| {
                        let (name, value) = line.split_once(':')?;
                        if name.eq_ignore_ascii_case("authorization") {
                            Some(value.trim())
                        } else {
                            None
                        }
                    });
                    if let Some(expected_auth) = route.expected_auth.as_deref()
                        && auth != Some(expected_auth)
                    {
                        let body = b"unauthorized";
                        let header = format!(
                            "HTTP/1.1 401 Unauthorized\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                            body.len()
                        );
                        let _ = stream.write_all(header.as_bytes());
                        let _ = stream.write_all(body);
                        continue;
                    }

                    let body = if let Ok(text) = String::from_utf8(route.body.clone()) {
                        text.replace("__BASE_URL__", &base_url).into_bytes()
                    } else {
                        route.body.clone()
                    };
                    let header = format!(
                        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                        body.len()
                    );
                    let _ = stream.write_all(header.as_bytes());
                    let _ = stream.write_all(&body);
                } else {
                    let body = b"not found";
                    let header = format!(
                        "HTTP/1.1 404 Not Found\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                        body.len()
                    );
                    let _ = stream.write_all(header.as_bytes());
                    let _ = stream.write_all(body);
                }
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

impl TestRoute {
    fn auth(body: Vec<u8>, expected_auth: &str) -> Self {
        Self {
            body,
            expected_auth: Some(expected_auth.to_string()),
        }
    }
}

impl From<Vec<u8>> for TestRoute {
    fn from(body: Vec<u8>) -> Self {
        Self {
            body,
            expected_auth: None,
        }
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
