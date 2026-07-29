use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
use tempfile::TempDir;

fn cmd() -> Command {
    let mut cmd = Command::cargo_bin("journal").unwrap();
    cmd.env_remove("JOURNAL_FILE")
        .env_remove("XDG_DATA_HOME")
        .env_remove("XDG_CONFIG_HOME");
    cmd
}

/// A journal file pre-populated directly (not via the CLI) so tests
/// control exact entry content/order.
fn fixture() -> (TempDir, std::path::PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("journal.txt");
    fs::write(
        &path,
        "[2026-07-01.08:00:00]\nfirst\n\n\
         [2026-07-02.09:00:00]\nsecond\n\n\
         [2026-07-03.10:00:00]\nthird\n\n\
         [2026-07-04.11:00:00]\nfourth\n\n\
         [2026-07-05.12:00:00]\nfifth\n\n",
    )
    .unwrap();
    (dir, path)
}

#[test]
fn dash_n_shows_the_last_n_entries_in_chronological_order() {
    let (_dir, path) = fixture();
    let out = cmd()
        .arg("-f")
        .arg(&path)
        .arg("-3")
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let s = String::from_utf8(out).unwrap();
    assert!(!s.contains("first"));
    assert!(!s.contains("second"));
    assert!(s.contains("third"));
    assert!(s.contains("fourth"));
    assert!(s.contains("fifth"));
    // Chronological order preserved: "third" appears before "fifth".
    assert!(s.find("third").unwrap() < s.find("fifth").unwrap());
}

#[test]
fn dash_n_larger_than_entry_count_shows_everything() {
    let (_dir, path) = fixture();
    cmd()
        .arg("-f")
        .arg(&path)
        .arg("-100")
        .assert()
        .success()
        .stdout(predicate::str::contains("first").and(predicate::str::contains("fifth")));
}

#[test]
fn dash_n_on_empty_journal_prints_nothing_and_succeeds() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("journal.txt");
    cmd()
        .arg("-f")
        .arg(&path)
        .arg("-3")
        .assert()
        .success()
        .stdout(predicate::str::is_empty());
}

#[test]
fn dash_zero_is_a_usage_error() {
    let (_dir, path) = fixture();
    cmd().arg("-f").arg(&path).arg("-0").assert().failure().code(2);
}

#[test]
fn dash_n_combined_with_search_is_a_usage_error() {
    let (_dir, path) = fixture();
    cmd()
        .arg("-f")
        .arg(&path)
        .arg("-3")
        .args(["-s", "foo"])
        .assert()
        .failure()
        .code(2);
}

#[test]
fn dash_n_combined_with_entry_text_is_a_usage_error() {
    let (_dir, path) = fixture();
    cmd()
        .arg("-f")
        .arg(&path)
        .arg("-3")
        .arg("some entry text")
        .assert()
        .failure()
        .code(2);
}

#[test]
fn limits_value_of_negative_three_is_not_mistaken_for_dash_n() {
    // Regression guard: `--limit -3` must pass "-3" through as --limit's
    // value, not be intercepted as the last-N flag. clap itself rejects
    // this since --limit expects a usize, giving a usage error either way,
    // but it must be *that* error, not the -N-combined-with-search one.
    let (_dir, path) = fixture();
    cmd()
        .arg("-f")
        .arg(&path)
        .args(["-s", "foo", "--limit", "-3"])
        .assert()
        .failure()
        .code(2);
}
