use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;

fn cmd() -> Command {
    let mut cmd = Command::cargo_bin("journal").unwrap();
    cmd.env_remove("JOURNAL_FILE").env_remove("XDG_DATA_HOME");
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
fn commits_new_entry_when_editor_saves_changes() {
    let dir = tempfile::tempdir().unwrap();
    let journal_path = dir.path().join("journal.txt");
    let editor = fake_editor(
        dir.path(),
        "append-editor.sh",
        r#"printf ' @demo\nentry body from editor\n' >> "$1""#,
    );

    cmd()
        .env("EDITOR", &editor)
        .arg("-f")
        .arg(&journal_path)
        .assert()
        .success();

    let contents = fs::read_to_string(&journal_path).unwrap();
    assert!(
        predicate::str::is_match(r"^\[\d{4}-\d{2}-\d{2}\.\d{2}:\d{2}:\d{2}\] @demo\n")
            .unwrap()
            .eval(&contents)
    );
    assert!(contents.contains("entry body from editor"));
    assert!(contents.ends_with("entry body from editor\n\n"));
}

#[test]
fn preserves_existing_entries_above_the_new_one() {
    let dir = tempfile::tempdir().unwrap();
    let journal_path = dir.path().join("journal.txt");
    fs::write(&journal_path, "[2026-01-01.00:00:00] @old\nold entry\n\n").unwrap();
    let editor = fake_editor(
        dir.path(),
        "append-editor.sh",
        r#"printf ' new entry text\n' >> "$1""#,
    );

    cmd()
        .env("EDITOR", &editor)
        .arg("-f")
        .arg(&journal_path)
        .assert()
        .success();

    let contents = fs::read_to_string(&journal_path).unwrap();
    assert!(contents.starts_with("[2026-01-01.00:00:00] @old\nold entry\n\n"));
    assert!(contents.contains("new entry text"));
}

#[test]
fn editor_quitting_without_saving_leaves_journal_untouched() {
    let dir = tempfile::tempdir().unwrap();
    let journal_path = dir.path().join("journal.txt");
    fs::write(&journal_path, "[2026-01-01.00:00:00]\nunchanged\n\n").unwrap();
    let editor = fake_editor(dir.path(), "noop-editor.sh", "exit 0"); // never touches the file

    cmd()
        .env("EDITOR", &editor)
        .arg("-f")
        .arg(&journal_path)
        .assert()
        .success();

    assert_eq!(
        fs::read_to_string(&journal_path).unwrap(),
        "[2026-01-01.00:00:00]\nunchanged\n\n"
    );
}

#[test]
fn editor_failure_leaves_journal_untouched_and_reports_error() {
    let dir = tempfile::tempdir().unwrap();
    let journal_path = dir.path().join("journal.txt");
    fs::write(&journal_path, "[2026-01-01.00:00:00]\nunchanged\n\n").unwrap();
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
        "[2026-01-01.00:00:00]\nunchanged\n\n"
    );
}

#[test]
fn editor_session_does_not_leave_the_lock_held() {
    let dir = tempfile::tempdir().unwrap();
    let journal_path = dir.path().join("journal.txt");
    let editor = fake_editor(
        dir.path(),
        "append-editor.sh",
        r#"printf ' entry one\n' >> "$1""#,
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
