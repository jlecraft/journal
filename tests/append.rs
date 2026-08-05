use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;

fn cmd() -> Command {
    let mut cmd = Command::cargo_bin("journal").unwrap();
    // Isolate from the real environment/XDG default in every test.
    cmd.env_remove("JOURNAL_FILE")
        .env_remove("XDG_DATA_HOME")
        .env_remove("XDG_CONFIG_HOME");
    cmd
}

#[test]
fn inline_tags_stay_wherever_they_were_typed() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("journal.txt");

    cmd()
        .arg("-f")
        .arg(&path)
        .arg("124/80/55 @bp @health")
        .assert()
        .success();

    let contents = fs::read_to_string(&path).unwrap();
    // No hoisting: the tags stay exactly where they were typed, trailing
    // the body text on the same line, and the header line stays bare.
    assert!(contents.starts_with("[") && contents.contains("]\n124/80/55 @bp @health\n\n"));
    assert!(contents.ends_with("\n\n"));
}

#[test]
fn dash_t_appends_a_tags_line_and_prefixes_bare_words() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("journal.txt");

    cmd()
        .arg("-f")
        .arg(&path)
        .args(["-t", "beer @store"])
        .arg("124/80/55 @bp")
        .assert()
        .success();

    let contents = fs::read_to_string(&path).unwrap();
    // The -t tags land on their own line, after the body; the bare
    // "beer" is auto-prefixed, and the inline "@bp" is untouched.
    assert!(contents.contains("124/80/55 @bp\n@beer @store\n\n"));
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
fn verbose_flag_prints_diagnostics_to_stderr_only() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("journal.txt");

    cmd()
        .arg("-f")
        .arg(&path)
        .arg("-v")
        .arg("some entry")
        .assert()
        .success()
        .stdout(predicate::str::is_empty())
        .stderr(predicate::str::contains("using journal file"));
}

#[test]
fn without_verbose_flag_stderr_is_silent() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("journal.txt");

    cmd()
        .arg("-f")
        .arg(&path)
        .arg("some entry")
        .assert()
        .success()
        .stderr(predicate::str::is_empty());
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
