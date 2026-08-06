use assert_cmd::Command;
use std::fs;
use tempfile::TempDir;

fn cmd() -> Command {
    let mut cmd = Command::cargo_bin("journal").unwrap();
    cmd.env_remove("JOURNAL_FILE")
        .env_remove("XDG_DATA_HOME")
        .env("XDG_CONFIG_HOME", tempfile::tempdir().unwrap().keep());
    cmd
}

/// A single fixed-timestamp entry, written directly (not via the CLI) so
/// tests control the exact on-disk header text.
fn fixed_entry_fixture() -> (TempDir, std::path::PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("journal.txt");
    fs::write(&path, "[2026-07-10.08:00:00]\nslept fine\n\n").unwrap();
    (dir, path)
}

/// An entry timestamped exactly 3 days before "now" (computed at fixture
/// creation time), for exercising `-h/--human`'s diff annotation (e.g. the
/// default `short` style renders this as `3d`) without depending on
/// wall-clock time at assertion time.
fn three_days_ago_fixture() -> (TempDir, std::path::PathBuf, chrono::NaiveDateTime) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("journal.txt");
    let then = chrono::Local::now().naive_local() - chrono::Duration::days(3);
    fs::write(
        &path,
        format!("[{}]\nslept fine\n\n", then.format("%Y-%m-%d.%H:%M:%S")),
    )
    .unwrap();
    (dir, path, then)
}

/// A journal file with one entry containing a searchable term, for the
/// color/highlighting tests below.
fn searchable_fixture() -> (TempDir, std::path::PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("journal.txt");
    fs::write(
        &path,
        "[2026-07-10.08:00:00]\nreading linux kernel internals\n\n",
    )
    .unwrap();
    (dir, path)
}

#[test]
fn default_output_shows_exact_on_disk_style_timestamp_with_no_age() {
    let (_dir, path) = fixed_entry_fixture();
    let out = cmd().arg("-f").arg(&path).arg("-1").assert().success().get_output().stdout.clone();
    let s = String::from_utf8(out).unwrap();
    assert!(s.starts_with("### 2026-07-10.08:00:00\n"));
    assert!(!s.contains('('));
}

#[test]
fn human_flag_shows_configured_format_and_short_diff_by_default() {
    let (_dir, path, then) = three_days_ago_fixture();
    let out = cmd()
        .arg("-f")
        .arg(&path)
        .arg("-h")
        .arg("-1")
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let s = String::from_utf8(out).unwrap();
    assert!(s.starts_with(&format!("### {}", then.format("%Y-%m-%d %H:%M"))));
    assert!(s.contains("(3d)"));
}

#[test]
fn human_flag_with_custom_config_toml_uses_configured_format_and_long_diff() {
    let (_dir, path, then) = three_days_ago_fixture();
    let config_home = _dir.path().join("config-home");
    fs::create_dir_all(config_home.join("journal")).unwrap();
    fs::write(
        config_home.join("journal/config.toml"),
        "[timestamp]\nformat = \"%Y/%m/%d\"\ndiff = \"long\"\n",
    )
    .unwrap();

    let out = cmd()
        .env("XDG_CONFIG_HOME", &config_home)
        .arg("-f")
        .arg(&path)
        .arg("-h")
        .arg("-1")
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let s = String::from_utf8(out).unwrap();
    assert!(s.starts_with(&format!("### {}", then.format("%Y/%m/%d"))));
    assert!(s.contains("(3 days ago)"));
}

#[test]
fn human_flag_with_diff_disabled_shows_no_parens() {
    let (_dir, path, then) = three_days_ago_fixture();
    let config_home = _dir.path().join("config-home");
    fs::create_dir_all(config_home.join("journal")).unwrap();
    fs::write(config_home.join("journal/config.toml"), "[timestamp]\ndiff = \"disabled\"\n").unwrap();

    let out = cmd()
        .env("XDG_CONFIG_HOME", &config_home)
        .arg("-f")
        .arg(&path)
        .arg("-h")
        .arg("-1")
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let s = String::from_utf8(out).unwrap();
    assert_eq!(s.lines().next().unwrap(), format!("### {}", then.format("%Y-%m-%d %H:%M")));
}

#[test]
fn search_output_has_no_color_codes_by_default() {
    let (_dir, path) = searchable_fixture();
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
fn lines_only_output_has_no_color_codes_by_default() {
    let (_dir, path) = searchable_fixture();
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
fn dash_dash_color_forces_highlight_even_when_piped() {
    // assert_cmd always captures output through a pipe, so stdout is never
    // a TTY here -- this proves --color is purely opt-in and colors piped
    // output too, unlike the old auto-TTY-gated behavior.
    let (_dir, path) = searchable_fixture();
    let out = cmd()
        .arg("-f")
        .arg(&path)
        .args(["-s", "kernel"])
        .arg("--color")
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let s = String::from_utf8(out).unwrap();
    assert!(s.contains('\x1b'));
}

#[test]
fn dash_dash_no_color_overrides_config_enabled_true() {
    let (_dir, path) = searchable_fixture();
    let config_home = _dir.path().join("config-home");
    fs::create_dir_all(config_home.join("journal")).unwrap();
    fs::write(config_home.join("journal/config.toml"), "[color]\nenabled = true\n").unwrap();

    let out = cmd()
        .env("XDG_CONFIG_HOME", &config_home)
        .arg("-f")
        .arg(&path)
        .args(["-s", "kernel"])
        .arg("--no-color")
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let s = String::from_utf8(out).unwrap();
    assert!(!s.contains('\x1b'));
}

#[test]
fn no_color_env_var_overrides_dash_dash_color() {
    let (_dir, path) = searchable_fixture();
    let out = cmd()
        .env("NO_COLOR", "1")
        .arg("-f")
        .arg(&path)
        .args(["-s", "kernel"])
        .arg("--color")
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let s = String::from_utf8(out).unwrap();
    assert!(!s.contains('\x1b'));
}

#[test]
fn config_color_enabled_true_colors_by_default_with_neither_flag() {
    let (_dir, path) = searchable_fixture();
    let config_home = _dir.path().join("config-home");
    fs::create_dir_all(config_home.join("journal")).unwrap();
    fs::write(config_home.join("journal/config.toml"), "[color]\nenabled = true\n").unwrap();

    let out = cmd()
        .env("XDG_CONFIG_HOME", &config_home)
        .arg("-f")
        .arg(&path)
        .args(["-s", "kernel"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let s = String::from_utf8(out).unwrap();
    assert!(s.contains('\x1b'));
}
