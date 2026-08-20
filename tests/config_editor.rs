use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

fn cmd(config_home: &Path) -> Command {
    let mut cmd = Command::cargo_bin("journal").unwrap();
    cmd.env("XDG_CONFIG_HOME", config_home)
        .env_remove("JOURNAL_FILE")
        .env_remove("XDG_DATA_HOME");
    cmd
}

fn fake_editor(dir: &Path, name: &str, body: &str) -> PathBuf {
    let path = dir.join(name);
    fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
    path
}

#[test]
fn config_opens_user_xdg_path_and_seeds_commented_defaults() {
    let dir = tempfile::tempdir().unwrap();
    let marker = dir.path().join("opened-path");
    let editor = fake_editor(
        dir.path(),
        "capture-editor.sh",
        &format!(r#"printf '%s' "$1" > "{}""#, marker.display()),
    );

    cmd(dir.path())
        .env("EDITOR", editor)
        .arg("--config")
        .assert()
        .success();

    let config_path = dir.path().join("journal/config.toml");
    assert_eq!(
        fs::read_to_string(marker).unwrap(),
        config_path.display().to_string()
    );
    assert_eq!(
        fs::read_to_string(config_path).unwrap(),
        journal::config::DEFAULT_CONFIG_TEMPLATE
    );
}

#[test]
fn config_preserves_existing_file_and_editor_changes_persist() {
    let dir = tempfile::tempdir().unwrap();
    let config_dir = dir.path().join("journal");
    fs::create_dir_all(&config_dir).unwrap();
    let config_path = config_dir.join("config.toml");
    fs::write(&config_path, "# existing\n").unwrap();
    let editor = fake_editor(
        dir.path(),
        "append-editor.sh",
        r#"printf '# edited\n' >> "$1""#,
    );

    cmd(dir.path())
        .env("EDITOR", editor)
        .arg("--config")
        .assert()
        .success();

    assert_eq!(
        fs::read_to_string(config_path).unwrap(),
        "# existing\n# edited\n"
    );
}

#[test]
fn config_does_not_seed_an_existing_empty_file() {
    let dir = tempfile::tempdir().unwrap();
    let config_dir = dir.path().join("journal");
    fs::create_dir_all(&config_dir).unwrap();
    let config_path = config_dir.join("config.toml");
    fs::write(&config_path, "").unwrap();
    let marker = dir.path().join("contents-before-edit");
    let editor = fake_editor(
        dir.path(),
        "capture-editor.sh",
        &format!(r#"cp "$1" "{}""#, marker.display()),
    );

    cmd(dir.path())
        .env("EDITOR", editor)
        .arg("--config")
        .assert()
        .success();

    assert_eq!(fs::read_to_string(marker).unwrap(), "");
}

#[test]
fn config_can_open_malformed_toml_for_repair() {
    let dir = tempfile::tempdir().unwrap();
    let config_dir = dir.path().join("journal");
    fs::create_dir_all(&config_dir).unwrap();
    fs::write(config_dir.join("config.toml"), "not valid [[[").unwrap();
    let editor = fake_editor(dir.path(), "noop-editor.sh", "exit 0");

    cmd(dir.path())
        .env("EDITOR", editor)
        .arg("--config")
        .assert()
        .success();
}

#[test]
fn config_rejects_other_modes_but_allows_verbose() {
    let dir = tempfile::tempdir().unwrap();
    let editor = fake_editor(dir.path(), "noop-editor.sh", "exit 0");

    cmd(dir.path())
        .env("EDITOR", &editor)
        .args(["--config", "entry"])
        .assert()
        .code(2)
        .stderr(predicate::str::contains(
            "--config can only be combined with -v/--verbose",
        ));

    cmd(dir.path())
        .env("EDITOR", editor)
        .args(["--config", "--verbose"])
        .assert()
        .success()
        .stderr(predicate::str::contains("using config file"));
}

#[test]
fn config_reports_editor_failure() {
    let dir = tempfile::tempdir().unwrap();
    let editor = fake_editor(dir.path(), "failing-editor.sh", "exit 1");

    cmd(dir.path())
        .env("EDITOR", editor)
        .arg("--config")
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "editor exited with a non-zero status",
        ));
}
