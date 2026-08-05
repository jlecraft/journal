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
/// control exact entry content/order without depending on Milestone 3.
/// Tags now live inline in the body rather than on the header line (§2).
fn fixture() -> (TempDir, std::path::PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("journal.txt");
    fs::write(
        &path,
        "[2026-07-01.08:00:00]\n\
         124/80/55 @bp @health\n\n\
         [2026-07-02.09:00:00]\n\
         unrelated tag that looks similar @bph\n\n\
         [2026-07-03.10:00:00]\n\
         reading about linux kernel internals\n\n\
         [2026-07-04.11:00:00]\n\
         the weather this month\n\n\
         [2026-07-05.12:00:00]\n\
         slept 7 hours, felt great @sleep\n\n",
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
    assert_eq!(s.matches("### 2026-").count(), 1); // one entry's worth of output
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
    assert!(s.contains("124/80/55 @bp @health\n\n### 2026-07-05"));
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
fn bare_word_and_at_prefixed_term_both_find_an_inline_tag() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("journal.txt");
    fs::write(
        &path,
        "[2026-07-06.13:00:00]\nmy @blood_pressure was 117/75/50\n\n",
    )
    .unwrap();

    for term in ["blood_pressure", "@blood_pressure"] {
        cmd()
            .arg("-f")
            .arg(&path)
            .args(["-s", term])
            .assert()
            .success()
            .stdout(predicate::str::contains("117/75/50"));
    }
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

/// A journal file with one multi-line entry, for exercising
/// `-L/--lines-only` where line-vs-whole-entry matching actually differs.
fn multiline_fixture() -> (TempDir, std::path::PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("journal.txt");
    fs::write(
        &path,
        "[2026-07-10.08:00:00]\n\
         fm radio broadcast\n\
         unrelated line about weather\n\
         reading linux kernel internals\n\n",
    )
    .unwrap();
    (dir, path)
}

#[test]
fn lines_only_prints_header_then_only_matching_lines() {
    let (_dir, path) = multiline_fixture();
    let out = cmd()
        .arg("-f")
        .arg(&path)
        .args(["-s", "radio kernel"])
        .arg("-L")
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let s = String::from_utf8(out).unwrap();
    assert!(s.starts_with("### 2026-07-10 08:00:00 ("));
    assert!(s.contains("fm radio broadcast"));
    assert!(s.contains("reading linux kernel internals"));
    assert!(!s.contains("unrelated line about weather"));
}

#[test]
fn lines_only_with_all_flag_requires_both_terms_on_the_same_line() {
    let (_dir, path) = multiline_fixture();
    // "fm" and "kernel" each appear somewhere in the entry (so whole-entry
    // --all would match it) but never on the same line, so -L must find
    // no qualifying line and report no match.
    cmd()
        .arg("-f")
        .arg(&path)
        .args(["-s", "fm kernel"])
        .args(["-a", "-L"])
        .assert()
        .failure()
        .code(1)
        .stdout(predicate::str::is_empty());
}

#[test]
fn lines_only_with_all_flag_shows_the_line_containing_both_terms() {
    let (_dir, path) = multiline_fixture();
    let out = cmd()
        .arg("-f")
        .arg(&path)
        .args(["-s", "radio broadcast"])
        .args(["-a", "-L"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let s = String::from_utf8(out).unwrap();
    assert!(s.contains("fm radio broadcast"));
    assert!(!s.contains("unrelated line about weather"));
    assert!(!s.contains("reading linux kernel internals"));
}

#[test]
fn lines_only_without_search_is_a_usage_error() {
    let (_dir, path) = fixture();
    cmd()
        .arg("-f")
        .arg(&path)
        .arg("-L")
        .arg("some entry")
        .assert()
        .failure()
        .code(2);
}

#[test]
fn lines_only_output_has_no_color_codes_when_stdout_is_not_a_terminal() {
    let (_dir, path) = multiline_fixture();
    let out = cmd()
        .arg("-f")
        .arg(&path)
        .args(["-s", "kernel"])
        .arg("-L")
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let s = String::from_utf8(out).unwrap();
    assert!(s.contains("reading linux kernel internals"));
    assert!(!s.contains('\x1b'));
}

#[test]
fn search_output_has_no_color_codes_when_stdout_is_not_a_terminal() {
    // assert_cmd always captures output through a pipe, so stdout is never
    // a TTY here -- this exercises the auto/pipe-safe gating in
    // run_search directly, the same condition that keeps a Markdown
    // renderer like `bat` from seeing raw ANSI escapes mixed into its input.
    let (_dir, path) = fixture();
    let out = cmd()
        .arg("-f")
        .arg(&path)
        .args(["-s", "kernel"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let s = String::from_utf8(out).unwrap();
    assert!(s.contains("linux kernel"));
    assert!(!s.contains('\x1b'));
}

#[test]
fn verbose_flag_prints_match_count_to_stderr_only() {
    let (_dir, path) = fixture();
    cmd()
        .arg("-f")
        .arg(&path)
        .args(["-s", "kernel"])
        .arg("-v")
        .assert()
        .success()
        .stderr(predicate::str::contains("entries matched"));
}

#[test]
fn without_verbose_flag_stderr_is_silent_on_search() {
    let (_dir, path) = fixture();
    cmd()
        .arg("-f")
        .arg(&path)
        .args(["-s", "kernel"])
        .assert()
        .success()
        .stderr(predicate::str::is_empty());
}
