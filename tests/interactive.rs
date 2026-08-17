use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;

fn cmd() -> Command {
    let mut cmd = Command::cargo_bin("journal").unwrap();
    cmd.env_remove("JOURNAL_FILE")
        .env_remove("XDG_DATA_HOME")
        .env("XDG_CONFIG_HOME", tempfile::tempdir().unwrap().keep());
    cmd
}

fn run(input: &str) -> String {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("journal.txt");
    cmd()
        .args(["--interactive", "--file"])
        .arg(&path)
        .write_stdin(input)
        .assert()
        .success()
        .stdout(predicate::str::is_empty())
        .stderr(predicate::str::contains("Tags (space-separated, optional):"));
    fs::read_to_string(path).unwrap()
}

#[test]
fn timestamp_choices_control_the_exact_stored_header_shape() {
    let cases = [
        ("F", r"^\[\d{4}-\d{2}-\d{2} \d{2}:\d{2}:\d{2}\]"),
        ("full", r"^\[\d{4}-\d{2}-\d{2} \d{2}:\d{2}:\d{2}\]"),
        ("y", r"^\[\d{4}\]"),
        ("YEAR", r"^\[\d{4}\]"),
        ("d", r"^\[\d{2}-\d{2}\]"),
        ("date", r"^\[\d{2}-\d{2}\]"),
        ("month-day", r"^\[\d{2}-\d{2}\]"),
        ("t", r"^\[\d{2}:\d{2}:\d{2}\]"),
        ("time", r"^\[\d{2}:\d{2}:\d{2}\]"),
        ("", r"^\[\d{4}-\d{2}-\d{2} \d{2}:\d{2}:\d{2}\]"),
    ];
    for (choice, pattern) in cases {
        let contents = run(&format!("\n{choice}\nbody"));
        assert!(predicate::str::is_match(pattern).unwrap().eval(&contents), "{contents:?}");
    }
}

#[test]
fn tags_are_normalized_on_the_final_line_after_a_multiline_body() {
    let contents = run("bare @ready\nyear\nfirst line\nsecond line\n");
    assert!(contents.contains("first line\nsecond line\n@bare @ready\n\n"));
}

#[test]
fn empty_body_and_empty_tags_are_valid() {
    let contents = run("\ntime\n");
    assert!(predicate::str::is_match(r"^\[\d{2}:\d{2}:\d{2}\]\n\n$")
        .unwrap()
        .eval(&contents));
}

#[test]
fn invalid_timestamp_prints_an_explanation_and_reprompts() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("journal.txt");
    cmd()
        .args(["-i", "-f"])
        .arg(&path)
        .write_stdin("\ninvalid\nt\nbody")
        .assert()
        .success()
        .stderr(
            predicate::str::contains("Invalid timestamp choice")
                .and(predicate::str::contains("Timestamp [F/full").count(2)),
        );
}

#[test]
fn eof_during_tags_or_timestamp_aborts_without_writing() {
    for input in ["", "tags\n"] {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("journal.txt");
        cmd()
            .args(["-i", "-f"])
            .arg(&path)
            .write_stdin(input)
            .assert()
            .failure()
            .code(1);
        assert!(!path.exists());
    }
}

#[test]
fn only_file_and_verbose_flags_compose_with_interactive() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("journal.txt");
    cmd()
        .args(["-i", "-v", "-f"])
        .arg(&path)
        .write_stdin("\n\nbody")
        .assert()
        .success()
        .stderr(predicate::str::contains("acquiring lock"));

    for args in [
        vec!["-i", "text"],
        vec!["-i", "-t", "tag"],
        vec!["-i", "-s", "query"],
        vec!["-i", "-1"],
        vec!["-i", "--all-tags"],
        vec!["-i", "--human"],
        vec!["-i", "--no-headers"],
        vec!["-i", "--color"],
    ] {
        cmd().args(args).assert().failure().code(2);
    }
}
