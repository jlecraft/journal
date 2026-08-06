use assert_cmd::Command;
use std::fs;

fn cmd() -> Command {
    let mut cmd = Command::cargo_bin("journal").unwrap();
    cmd.env_remove("JOURNAL_FILE")
        .env_remove("XDG_DATA_HOME")
        .env("XDG_CONFIG_HOME", tempfile::tempdir().unwrap().keep());
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
    // Piped text is treated the same as text passed as an argument: tags
    // stay exactly where they were typed, no hoisting.
    assert!(contents.contains("]\n124/80/55 @bp @health\n\n"));
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
    assert!(contents.contains("piped body @inline\n@fromflag\n\n"));
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
