use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
use tempfile::TempDir;

fn cmd() -> Command {
    let mut cmd = Command::cargo_bin("journal").unwrap();
    cmd.env_remove("JOURNAL_FILE").env_remove("XDG_DATA_HOME");
    cmd
}

/// A journal file pre-populated directly (not via the CLI) so tests
/// control exact entry content/order without depending on Milestone 3.
fn fixture() -> (TempDir, std::path::PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("journal.txt");
    fs::write(
        &path,
        "[2026-07-01.08:00:00] @bp @health\n\
         124/80/55\n\n\
         [2026-07-02.09:00:00] @bph\n\
         unrelated tag that looks similar\n\n\
         [2026-07-03.10:00:00]\n\
         reading about linux kernel internals\n\n\
         [2026-07-04.11:00:00]\n\
         the weather this month\n\n\
         [2026-07-05.12:00:00] @sleep\n\
         slept 7 hours, felt great\n\n",
    )
    .unwrap();
    (dir, path)
}

#[test]
fn or_mode_is_default_and_matches_any_term() {
    let (_dir, path) = fixture();
    let out = cmd()
        .arg("-f")
        .arg(&path)
        .args(["-s", "kernel weather"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let s = String::from_utf8(out).unwrap();
    assert!(s.contains("linux kernel"));
    assert!(s.contains("this month"));
    assert!(!s.contains("124/80/55"));
}

#[test]
fn and_mode_requires_all_terms_present() {
    let (_dir, path) = fixture();
    cmd()
        .arg("-f")
        .arg(&path)
        .args(["-s", "kernel weather", "-a"])
        .assert()
        .failure()
        .code(1); // no single entry contains both terms
}

#[test]
fn tag_term_full_word_match_excludes_lookalike_tag() {
    let (_dir, path) = fixture();
    let out = cmd()
        .arg("-f")
        .arg(&path)
        .args(["-s", "@bp"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let s = String::from_utf8(out).unwrap();
    assert!(s.contains("124/80/55"));
    assert!(!s.contains("unrelated tag"));
}

#[test]
fn plus_joins_words_into_multiword_term() {
    let (_dir, path) = fixture();
    cmd()
        .arg("-f")
        .arg(&path)
        .args(["-s", "linux+kernel"])
        .assert()
        .success()
        .stdout(predicate::str::contains("linux kernel internals"));
}

#[test]
fn short_substring_matches_broadly() {
    let (_dir, path) = fixture();
    let out = cmd()
        .arg("-f")
        .arg(&path)
        .args(["-s", "th"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let s = String::from_utf8(out).unwrap();
    assert!(s.contains("this month")); // "th" inside "this"/"month"
}

#[test]
fn limit_caps_result_count() {
    let (_dir, path) = fixture();
    // "e" matches nearly every entry; --limit 1 should cap output to one.
    let out = cmd()
        .arg("-f")
        .arg(&path)
        .args(["-s", "e", "--limit", "1"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let s = String::from_utf8(out).unwrap();
    assert_eq!(s.matches('[').count(), 1); // one entry's worth of output
}

#[test]
fn no_matches_exits_one_with_empty_stdout() {
    let (_dir, path) = fixture();
    cmd()
        .arg("-f")
        .arg(&path)
        .args(["-s", "@nonexistent"])
        .assert()
        .failure()
        .code(1)
        .stdout(predicate::str::is_empty());
}

#[test]
fn matches_are_separated_by_a_blank_line() {
    let (_dir, path) = fixture();
    let out = cmd()
        .arg("-f")
        .arg(&path)
        .args(["-s", "@bp @sleep"]) // OR mode: matches both the @bp and @sleep entries
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let s = String::from_utf8(out).unwrap();
    assert!(s.contains("124/80/55\n\n[2026-07-05"));
}

#[test]
fn search_and_positional_text_conflict_is_a_usage_error() {
    let (_dir, path) = fixture();
    cmd()
        .arg("-f")
        .arg(&path)
        .arg("some text")
        .args(["-s", "foo"])
        .assert()
        .failure()
        .code(2);
}

#[test]
fn all_flag_without_search_is_a_usage_error() {
    let (_dir, path) = fixture();
    cmd()
        .arg("-f")
        .arg(&path)
        .arg("-a")
        .arg("some entry")
        .assert()
        .failure()
        .code(2);
}
