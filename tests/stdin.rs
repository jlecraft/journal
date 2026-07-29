use assert_cmd::Command;
use std::fs;

fn cmd() -> Command {
    let mut cmd = Command::cargo_bin("journal").unwrap();
    cmd.env_remove("JOURNAL_FILE").env_remove("XDG_DATA_HOME");
    cmd
}

#[test]
fn dash_reads_entry_text_from_stdin() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("journal.txt");

    cmd()
        .arg("-f")
        .arg(&path)
        .arg("-")
        .write_stdin("124/80/55 @bp @health")
        .assert()
        .success();

    let contents = fs::read_to_string(&path).unwrap();
    assert!(contents.contains("@bp @health"));
    assert!(contents.contains("124/80/55"));
    assert!(!contents.contains("124/80/55 @bp")); // tags still hoisted off the body
}

#[test]
fn dash_stdin_combines_with_dash_t_flag() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("journal.txt");

    cmd()
        .arg("-f")
        .arg(&path)
        .args(["-t", "@fromflag"])
        .arg("-")
        .write_stdin("piped body @inline")
        .assert()
        .success();

    let contents = fs::read_to_string(&path).unwrap();
    assert!(contents.contains("@inline"));
    assert!(contents.contains("@fromflag"));
    assert!(contents.contains("piped body"));
}

#[test]
fn literal_dash_text_without_stdin_input_appends_empty_body() {
    // Piping nothing (stdin closed/empty) is valid: an empty entry body,
    // not an error -- consistent with how e.g. `cat -` behaves on an
    // empty pipe.
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("journal.txt");

    cmd()
        .arg("-f")
        .arg(&path)
        .arg("-")
        .write_stdin("")
        .assert()
        .success();

    let contents = fs::read_to_string(&path).unwrap();
    assert!(contents.starts_with('['));
    assert!(contents.ends_with("]\n\n"));
}
