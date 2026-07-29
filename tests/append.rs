use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;

fn cmd() -> Command {
    let mut cmd = Command::cargo_bin("journal").unwrap();
    // Isolate from the real environment/XDG default in every test.
    cmd.env_remove("JOURNAL_FILE").env_remove("XDG_DATA_HOME");
    cmd
}

#[test]
fn appends_entry_with_inline_trailing_tags_via_dash_f() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("journal.txt");

    cmd()
        .arg("-f")
        .arg(&path)
        .arg("124/80/55 @bp @health")
        .assert()
        .success();

    let contents = fs::read_to_string(&path).unwrap();
    assert!(contents.contains("@bp @health"));
    assert!(contents.contains("124/80/55"));
    assert!(!contents.contains("124/80/55 @bp")); // tags hoisted off the body
    assert!(contents.ends_with("\n\n"));
}

#[test]
fn appends_entry_with_dash_t_flag_and_merges_with_inline_tags() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("journal.txt");

    cmd()
        .arg("-f")
        .arg(&path)
        .args(["-t", "@bp @extra"])
        .arg("124/80/55 @bp @health")
        .assert()
        .success();

    let contents = fs::read_to_string(&path).unwrap();
    // @bp appears once despite being in both sources (dedup).
    assert_eq!(contents.matches("@bp").count(), 1);
    assert!(contents.contains("@health"));
    assert!(contents.contains("@extra"));
}

#[test]
fn appends_multiple_entries_sequentially_without_clobbering() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("journal.txt");

    cmd().arg("-f").arg(&path).arg("first entry").assert().success();
    cmd().arg("-f").arg(&path).arg("second entry").assert().success();

    let contents = fs::read_to_string(&path).unwrap();
    assert!(contents.contains("first entry"));
    assert!(contents.contains("second entry"));
    assert!(contents.find("first entry").unwrap() < contents.find("second entry").unwrap());
}

#[test]
fn resolves_journal_file_env_var_when_no_dash_f() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("via-env.txt");

    cmd()
        .env("JOURNAL_FILE", &path)
        .arg("entry via JOURNAL_FILE")
        .assert()
        .success();

    assert!(fs::read_to_string(&path)
        .unwrap()
        .contains("entry via JOURNAL_FILE"));
}

#[test]
fn creates_journal_file_and_parent_dirs_via_xdg_default() {
    let dir = tempfile::tempdir().unwrap();

    cmd()
        .env("XDG_DATA_HOME", dir.path())
        .arg("entry via xdg default")
        .assert()
        .success();

    let expected = dir.path().join("journal").join("journal.txt");
    assert!(fs::read_to_string(&expected)
        .unwrap()
        .contains("entry via xdg default"));
}

#[test]
fn no_args_fails_clearly_instead_of_panicking() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("journal.txt");

    cmd()
        .arg("-f")
        .arg(&path)
        .assert()
        .failure()
        .code(1)
        .stderr(predicate::str::contains("editor mode"));
}

#[test]
fn dash_f_errors_when_parent_directory_missing() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("no-such-subdir").join("journal.txt");

    cmd()
        .arg("-f")
        .arg(&path)
        .arg("some entry")
        .assert()
        .failure()
        .code(1);
}
