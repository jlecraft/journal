use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
use tempfile::TempDir;

fn cmd() -> Command {
    let mut cmd = Command::cargo_bin("journal").unwrap();
    cmd.env_remove("JOURNAL_FILE")
        .env_remove("XDG_DATA_HOME")
        .env("XDG_CONFIG_HOME", tempfile::tempdir().unwrap().keep());
    cmd
}

/// A journal file pre-populated directly (not via the CLI) so tests
/// control exact entry content/order.
fn fixture() -> (TempDir, std::path::PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("journal.txt");
    fs::write(
        &path,
        "[2026-07-01 08:00:00]\n124/80/55 @bp @health\n\n\
         [2026-07-02 09:00:00]\nlistened to @radio\n\n\
         [2026-07-03 10:00:00]\nno tags here\n\n\
         [2026-07-04 11:00:00]\nfollow-up @bp reading, also @radio again\n\n",
    )
    .unwrap();
    (dir, path)
}

#[test]
fn all_tags_prints_unique_sorted_bare_tags_with_right_justified_counts() {
    let (_dir, path) = fixture();
    cmd()
        .arg("-f")
        .arg(&path)
        .arg("--all-tags")
        .assert()
        .success()
        .stdout("2 bp\n1 health\n2 radio\n");
}

#[test]
fn all_tags_pads_counts_to_the_widest_count_width() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("journal.txt");
    let mut contents = String::new();
    for i in 0..10 {
        contents.push_str(&format!("[2026-07-01 08:00:{i:02}]\n@common entry\n\n"));
    }
    contents.push_str("[2026-07-01 08:01:00]\n@rare entry\n\n");
    fs::write(&path, contents).unwrap();

    cmd()
        .arg("-f")
        .arg(&path)
        .arg("--all-tags")
        .assert()
        .success()
        .stdout("10 common\n 1 rare\n");
}

#[test]
fn all_tags_on_empty_journal_prints_nothing_and_succeeds() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("journal.txt");
    cmd()
        .arg("-f")
        .arg(&path)
        .arg("--all-tags")
        .assert()
        .success()
        .stdout(predicate::str::is_empty());
}

#[test]
fn all_tags_combined_with_search_is_a_usage_error() {
    let (_dir, path) = fixture();
    cmd()
        .arg("-f")
        .arg(&path)
        .arg("--all-tags")
        .args(["-s", "foo"])
        .assert()
        .failure()
        .code(2);
}

#[test]
fn all_tags_combined_with_entry_text_is_a_usage_error() {
    let (_dir, path) = fixture();
    cmd()
        .arg("-f")
        .arg(&path)
        .arg("--all-tags")
        .arg("some entry text")
        .assert()
        .failure()
        .code(2);
}

#[test]
fn all_tags_combined_with_dash_n_is_a_usage_error() {
    let (_dir, path) = fixture();
    cmd()
        .arg("-f")
        .arg(&path)
        .arg("--all-tags")
        .arg("-3")
        .assert()
        .failure()
        .code(2);
}
