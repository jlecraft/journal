use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;

fn cmd() -> Command {
    let mut cmd = Command::cargo_bin("journal").unwrap();
    cmd.env_remove("JOURNAL_FILE")
        .env_remove("XDG_DATA_HOME")
        .env("XDG_CONFIG_HOME", tempfile::tempdir().unwrap().keep());
    cmd
}

/// Writes an executable shell script standing in for `$EDITOR`, so tests
/// stay non-interactive instead of launching a real editor against no
/// controlling terminal (which just hangs).
fn fake_editor(dir: &std::path::Path, name: &str, body: &str) -> PathBuf {
    let path = dir.join(name);
    fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
    path
}

#[test]
fn opens_the_real_journal_file_directly_with_no_seeded_content() {
    let dir = tempfile::tempdir().unwrap();
    let journal_path = dir.path().join("journal.txt");
    let marker = dir.path().join("seeded.txt");
    let editor = fake_editor(
        dir.path(),
        "capture-editor.sh",
        &format!(r#"cp "$1" "{}""#, marker.display()),
    );

    cmd()
        .env("EDITOR", &editor)
        .arg("-f")
        .arg(&journal_path)
        .assert()
        .success();

    // Nothing is seeded into the buffer -- a brand new journal is opened
    // completely empty, no timestamp, no blank cursor line, no tags.
    assert_eq!(fs::read_to_string(&marker).unwrap(), "");
}

#[test]
fn changes_saved_by_the_editor_land_directly_in_the_journal_file() {
    let dir = tempfile::tempdir().unwrap();
    let journal_path = dir.path().join("journal.txt");
    let editor = fake_editor(
        dir.path(),
        "append-editor.sh",
        r#"printf '[2026-07-28 14:03:00]\nentry body from editor @demo\n\n' >> "$1""#,
    );

    cmd()
        .env("EDITOR", &editor)
        .arg("-f")
        .arg(&journal_path)
        .assert()
        .success();

    let contents = fs::read_to_string(&journal_path).unwrap();
    assert_eq!(
        contents,
        "[2026-07-28 14:03:00]\nentry body from editor @demo\n\n"
    );
}

#[test]
fn existing_journal_content_is_visible_to_the_editor_and_preserved() {
    let dir = tempfile::tempdir().unwrap();
    let journal_path = dir.path().join("journal.txt");
    fs::write(&journal_path, "[2026-01-01 00:00:00]\nold entry\n\n").unwrap();
    let editor = fake_editor(
        dir.path(),
        "append-editor.sh",
        r#"printf '[2026-07-28 14:03:00]\nnew entry\n\n' >> "$1""#,
    );

    cmd()
        .env("EDITOR", &editor)
        .arg("-f")
        .arg(&journal_path)
        .assert()
        .success();

    let contents = fs::read_to_string(&journal_path).unwrap();
    assert_eq!(
        contents,
        "[2026-01-01 00:00:00]\nold entry\n\n[2026-07-28 14:03:00]\nnew entry\n\n"
    );
}

#[test]
fn editor_quitting_without_saving_leaves_journal_untouched() {
    let dir = tempfile::tempdir().unwrap();
    let journal_path = dir.path().join("journal.txt");
    fs::write(&journal_path, "[2026-01-01 00:00:00]\nunchanged\n\n").unwrap();
    let editor = fake_editor(dir.path(), "noop-editor.sh", "exit 0"); // never touches the file

    cmd()
        .env("EDITOR", &editor)
        .arg("-f")
        .arg(&journal_path)
        .assert()
        .success();

    assert_eq!(
        fs::read_to_string(&journal_path).unwrap(),
        "[2026-01-01 00:00:00]\nunchanged\n\n"
    );
}

#[test]
fn editor_failure_reports_an_error() {
    let dir = tempfile::tempdir().unwrap();
    let journal_path = dir.path().join("journal.txt");
    fs::write(&journal_path, "[2026-01-01 00:00:00]\nunchanged\n\n").unwrap();
    let editor = fake_editor(dir.path(), "failing-editor.sh", "exit 1");

    cmd()
        .env("EDITOR", &editor)
        .arg("-f")
        .arg(&journal_path)
        .assert()
        .failure()
        .code(1)
        .stderr(predicate::str::contains("non-zero status"));

    assert_eq!(
        fs::read_to_string(&journal_path).unwrap(),
        "[2026-01-01 00:00:00]\nunchanged\n\n"
    );
}

#[test]
fn opening_the_editor_on_a_missing_file_creates_it_first() {
    let dir = tempfile::tempdir().unwrap();
    let journal_path = dir.path().join("journal.txt");
    assert!(!journal_path.exists());
    let editor = fake_editor(dir.path(), "noop-editor.sh", "exit 0");

    cmd()
        .env("EDITOR", &editor)
        .arg("-f")
        .arg(&journal_path)
        .assert()
        .success();

    assert!(journal_path.exists());
    assert_eq!(fs::read_to_string(&journal_path).unwrap(), "");
}

#[test]
fn dash_t_flag_without_entry_text_is_a_usage_error() {
    let dir = tempfile::tempdir().unwrap();
    let journal_path = dir.path().join("journal.txt");
    cmd()
        .arg("-f")
        .arg(&journal_path)
        .args(["-t", "bp"])
        .assert()
        .failure()
        .code(2);
}

#[test]
fn editor_session_does_not_leave_the_lock_held() {
    let dir = tempfile::tempdir().unwrap();
    let journal_path = dir.path().join("journal.txt");
    let editor = fake_editor(
        dir.path(),
        "append-editor.sh",
        r#"printf '[2026-07-28 14:03:00]\nentry one\n\n' >> "$1""#,
    );

    cmd()
        .env("EDITOR", &editor)
        .arg("-f")
        .arg(&journal_path)
        .assert()
        .success();

    // A subsequent, unrelated append must succeed promptly rather than
    // blocking on a lock the editor session failed to release.
    cmd()
        .arg("-f")
        .arg(&journal_path)
        .arg("entry two")
        .assert()
        .success();

    let contents = fs::read_to_string(&journal_path).unwrap();
    assert!(contents.contains("entry one"));
    assert!(contents.contains("entry two"));
}
