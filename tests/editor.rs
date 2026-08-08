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
fn commits_new_entry_when_editor_saves_changes() {
    let dir = tempfile::tempdir().unwrap();
    let journal_path = dir.path().join("journal.txt");
    let editor = fake_editor(
        dir.path(),
        "append-editor.sh",
        r#"printf '\nentry body from editor @demo\n' >> "$1""#,
    );

    cmd()
        .env("EDITOR", &editor)
        .arg("-f")
        .arg(&journal_path)
        .assert()
        .success();

    let contents = fs::read_to_string(&journal_path).unwrap();
    assert!(
        predicate::str::is_match(r"^\[\d{4}-\d{2}-\d{2} \d{2}:\d{2}:\d{2}\]\n")
            .unwrap()
            .eval(&contents)
    );
    assert!(contents.contains("entry body from editor @demo"));
    assert!(contents.ends_with("entry body from editor @demo\n\n"));
}

#[test]
fn dash_t_flag_seeds_a_tags_line_into_the_editor_buffer() {
    let dir = tempfile::tempdir().unwrap();
    let journal_path = dir.path().join("journal.txt");
    let marker = dir.path().join("seeded.txt");
    // This editor never touches "$1" -- it just captures what journal
    // seeded the buffer with, so the mtime check treats the session as
    // "no changes" and nothing is persisted to the real journal file.
    let editor = fake_editor(
        dir.path(),
        "capture-editor.sh",
        &format!(r#"cp "$1" "{}""#, marker.display()),
    );

    cmd()
        .env("EDITOR", &editor)
        .arg("-f")
        .arg(&journal_path)
        .args(["-t", "beer @store"])
        .assert()
        .success();

    let seeded = fs::read_to_string(&marker).unwrap();
    // Timestamp alone on line 1, a blank line for the body, then the
    // -t tags line -- bare "beer" auto-prefixed with @.
    assert!(
        predicate::str::is_match(r"^\[\d{4}-\d{2}-\d{2} \d{2}:\d{2}:\d{2}\]\n\n@beer @store$")
            .unwrap()
            .eval(&seeded)
    );
}

#[test]
fn dash_t_tags_seeded_into_the_editor_are_persisted_on_save() {
    let dir = tempfile::tempdir().unwrap();
    let journal_path = dir.path().join("journal.txt");
    let editor = fake_editor(
        dir.path(),
        "append-editor.sh",
        r#"sed -i '2s/^$/entry body from editor/' "$1""#,
    );

    cmd()
        .env("EDITOR", &editor)
        .arg("-f")
        .arg(&journal_path)
        .args(["-t", "beer @store"])
        .assert()
        .success();

    let contents = fs::read_to_string(&journal_path).unwrap();
    assert!(contents.ends_with("entry body from editor\n@beer @store\n\n"));
}

#[test]
fn basic_entry_seeds_a_blank_line_after_the_timestamp_for_the_cursor() {
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

    let seeded = fs::read_to_string(&marker).unwrap();
    assert!(
        predicate::str::is_match(r"^\[\d{4}-\d{2}-\d{2} \d{2}:\d{2}:\d{2}\]\n\n$")
            .unwrap()
            .eval(&seeded)
    );
}

#[test]
fn editor_args_from_config_position_the_cursor_via_the_line_placeholder() {
    let dir = tempfile::tempdir().unwrap();
    let journal_path = dir.path().join("journal.txt");
    // A new (empty) journal: line 1 is the seeded timestamp, so the
    // blank cursor line -- where {line} should resolve to -- is line 2.
    let config_home = dir.path().join("config-home");
    fs::create_dir_all(config_home.join("journal")).unwrap();
    fs::write(
        config_home.join("journal").join("config.toml"),
        "[editor]\nargs = \"+{line}\"\n",
    )
    .unwrap();

    let marker = dir.path().join("argv.txt");
    let editor = fake_editor(
        dir.path(),
        "capture-args-editor.sh",
        &format!(r#"printf '%s\n' "$@" > "{}""#, marker.display()),
    );

    cmd()
        .env("EDITOR", &editor)
        .env("XDG_CONFIG_HOME", &config_home)
        .arg("-f")
        .arg(&journal_path)
        .assert()
        .success();

    let argv: Vec<String> = fs::read_to_string(&marker)
        .unwrap()
        .lines()
        .map(str::to_string)
        .collect();
    assert!(argv.contains(&"+2".to_string()));
}

#[test]
fn cursor_positioning_args_precede_editors_own_args() {
    // Regression guard: vim-likes run `+cmd`/`-c cmd` arguments in the
    // order given on the command line. If journal's own cursor-position
    // arg landed *after* extra commands baked into $EDITOR itself, those
    // commands would run before the cursor is repositioned and act on
    // the wrong line. Caught manually with a real vim invocation whose
    // $EDITOR also carried its own `-c` flags; this pins the fix.
    let dir = tempfile::tempdir().unwrap();
    let journal_path = dir.path().join("journal.txt");
    let config_home = dir.path().join("config-home");
    fs::create_dir_all(config_home.join("journal")).unwrap();
    fs::write(
        config_home.join("journal").join("config.toml"),
        "[editor]\nargs = \"+{line}\"\n",
    )
    .unwrap();

    let marker = dir.path().join("argv.txt");
    let editor = fake_editor(
        dir.path(),
        "capture-args-editor.sh",
        &format!(r#"printf '%s\n' "$@" > "{}""#, marker.display()),
    );

    cmd()
        .env("EDITOR", format!("{} --an-editor-flag", editor.display()))
        .env("XDG_CONFIG_HOME", &config_home)
        .arg("-f")
        .arg(&journal_path)
        .assert()
        .success();

    let argv: Vec<String> = fs::read_to_string(&marker)
        .unwrap()
        .lines()
        .map(str::to_string)
        .collect();
    let cursor_pos = argv.iter().position(|a| a == "+2").unwrap();
    let flag_pos = argv.iter().position(|a| a == "--an-editor-flag").unwrap();
    assert!(cursor_pos < flag_pos);
}

#[test]
fn preserves_existing_entries_above_the_new_one() {
    let dir = tempfile::tempdir().unwrap();
    let journal_path = dir.path().join("journal.txt");
    fs::write(&journal_path, "[2026-01-01 00:00:00] @old\nold entry\n\n").unwrap();
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
    assert!(contents.starts_with("[2026-01-01 00:00:00] @old\nold entry\n\n"));
    assert!(contents.contains("new entry text"));
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
fn editor_failure_leaves_journal_untouched_and_reports_error() {
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
fn sigint_during_editor_removes_the_temp_edit_buffer() {
    let dir = tempfile::tempdir().unwrap();
    let journal_path = dir.path().join("journal.txt");
    // Never touches "$1" -- just gives us a window to send SIGINT before
    // the (fake) editor would otherwise exit on its own.
    let editor = fake_editor(dir.path(), "slow-editor.sh", "sleep 5");

    // assert_cmd::Command has no public `spawn` (it wants `.assert()`
    // instead), so this test needs a real std::process::Child to signal
    // mid-run -- build one directly against the built binary's path.
    let mut child = std::process::Command::new(assert_cmd::cargo::cargo_bin("journal"))
        .env_remove("JOURNAL_FILE")
        .env_remove("XDG_DATA_HOME")
        .env_remove("XDG_CONFIG_HOME")
        .env("EDITOR", &editor)
        .arg("-f")
        .arg(&journal_path)
        .spawn()
        .unwrap();

    // Give journal time to seed the temp file and launch the editor.
    std::thread::sleep(std::time::Duration::from_millis(300));

    std::process::Command::new("kill")
        .args(["-INT", &child.id().to_string()])
        .status()
        .unwrap();

    let status = child.wait().unwrap();
    assert_eq!(status.code(), Some(130));

    let leaked: Vec<_> = fs::read_dir(dir.path())
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_name().to_string_lossy().starts_with(".journal-edit-"))
        .collect();
    assert!(leaked.is_empty(), "leaked temp file(s): {leaked:?}");
    assert!(!journal_path.exists());
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
