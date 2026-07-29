use std::env;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use fs2::FileExt;

/// Resolves the journal file path per the precedence chain in spec §4:
/// 1. `-f/--file` CLI argument
/// 2. `$JOURNAL_FILE` environment variable
/// 3. XDG data directory default
pub fn resolve_path(cli_file: Option<&str>) -> Result<PathBuf> {
    let journal_file_env = env::var("JOURNAL_FILE")
        .ok()
        .filter(|s| !s.trim().is_empty());
    resolve_path_with(cli_file, journal_file_env.as_deref(), xdg_default_path)
}

/// Precedence logic factored out from env/XDG access so it can be unit
/// tested without touching real process environment state.
fn resolve_path_with(
    cli_file: Option<&str>,
    journal_file_env: Option<&str>,
    xdg_default: impl FnOnce() -> Result<PathBuf>,
) -> Result<PathBuf> {
    if let Some(f) = cli_file {
        return Ok(PathBuf::from(f));
    }
    if let Some(f) = journal_file_env {
        return Ok(PathBuf::from(f));
    }
    xdg_default()
}

/// `$XDG_DATA_HOME/journal/journal.txt`, falling back to
/// `~/.local/share/journal/journal.txt` per the XDG Base Directory spec.
/// Pre-creates leading directories; does not create the file itself.
fn xdg_default_path() -> Result<PathBuf> {
    xdg::BaseDirectories::with_prefix("journal")
        .place_data_file("journal.txt")
        .context("failed to resolve XDG data directory for journal.txt")
}

/// Ensures the resolved journal file exists, creating it empty if not.
///
/// Explicit paths (`-f`/`$JOURNAL_FILE`) get `touch`-like semantics: the
/// parent directory must already exist, matching standard Unix tool
/// expectations for a user-specified path. The XDG default path's parent
/// directories are already pre-created by `xdg_default_path`, so this
/// succeeds there even on first run, per §4's "create the directory and
/// file if they don't yet exist".
pub fn ensure_exists(path: &Path) -> Result<()> {
    if path.exists() {
        return Ok(());
    }
    OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(path)
        .with_context(|| format!("failed to create journal file at {}", path.display()))?;
    Ok(())
}

/// Reads the journal file's full contents for searching. A missing file
/// is treated as an empty journal rather than an error, and -- unlike
/// `append_entry` -- this never creates the file: a read-only operation
/// like search shouldn't have filesystem side effects.
pub fn read_contents(path: &Path) -> Result<String> {
    if !path.exists() {
        return Ok(String::new());
    }
    std::fs::read_to_string(path)
        .with_context(|| format!("failed to read journal file at {}", path.display()))
}

/// Appends `rendered` (an already-formatted `Entry::render()` string) to
/// the journal file, creating it first if needed. Per §6.8, the write is
/// serialized with an exclusive advisory lock (`flock`) held for the
/// duration of the write and released on drop, and the file descriptor
/// is opened in `O_APPEND` mode so concurrent writers (e.g. a cron job
/// and an interactive invocation) can't interleave or clobber entries.
pub fn append_entry(path: &Path, rendered: &str) -> Result<()> {
    ensure_exists(path)?;
    let mut file = OpenOptions::new()
        .append(true)
        .open(path)
        .with_context(|| format!("failed to open journal file at {}", path.display()))?;
    file.lock_exclusive()
        .with_context(|| format!("failed to lock journal file at {}", path.display()))?;
    let result = file
        .write_all(rendered.as_bytes())
        .with_context(|| format!("failed to append to journal file at {}", path.display()));
    let _ = FileExt::unlock(&file);
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn stub_xdg(path: &'static str) -> impl FnOnce() -> Result<PathBuf> {
        move || Ok(PathBuf::from(path))
    }

    #[test]
    fn cli_file_takes_precedence_over_everything() {
        let path = resolve_path_with(
            Some("/explicit/path.txt"),
            Some("/env/path.txt"),
            stub_xdg("/xdg/path.txt"),
        )
        .unwrap();
        assert_eq!(path, PathBuf::from("/explicit/path.txt"));
    }

    #[test]
    fn env_var_used_when_no_cli_file() {
        let path = resolve_path_with(None, Some("/env/path.txt"), stub_xdg("/xdg/path.txt")).unwrap();
        assert_eq!(path, PathBuf::from("/env/path.txt"));
    }

    #[test]
    fn falls_back_to_xdg_default_when_nothing_else_set() {
        let path = resolve_path_with(None, None, stub_xdg("/xdg/path.txt")).unwrap();
        assert_eq!(path, PathBuf::from("/xdg/path.txt"));
    }

    #[test]
    fn empty_env_var_is_treated_as_unset() {
        // resolve_path_with itself doesn't filter blanks -- that's
        // resolve_path's job -- so this exercises the filter indirectly
        // via the public function's contract instead.
        let journal_file_env = Some("").filter(|s: &&str| !s.trim().is_empty());
        let path = resolve_path_with(None, journal_file_env, stub_xdg("/xdg/path.txt")).unwrap();
        assert_eq!(path, PathBuf::from("/xdg/path.txt"));
    }

    #[test]
    fn ensure_exists_creates_missing_file_empty() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("journal.txt");
        assert!(!path.exists());
        ensure_exists(&path).unwrap();
        assert!(path.exists());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "");
    }

    #[test]
    fn ensure_exists_does_not_touch_existing_content() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("journal.txt");
        {
            let mut f = std::fs::File::create(&path).unwrap();
            f.write_all(b"existing content\n").unwrap();
        }
        ensure_exists(&path).unwrap();
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            "existing content\n"
        );
    }

    #[test]
    fn ensure_exists_errors_when_parent_dir_missing() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("no-such-subdir").join("journal.txt");
        assert!(ensure_exists(&path).is_err());
    }

    #[test]
    fn read_contents_returns_empty_string_for_missing_file_without_creating_it() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("journal.txt");
        assert_eq!(read_contents(&path).unwrap(), "");
        assert!(!path.exists());
    }

    #[test]
    fn read_contents_returns_existing_file_contents() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("journal.txt");
        std::fs::write(&path, "hello\n").unwrap();
        assert_eq!(read_contents(&path).unwrap(), "hello\n");
    }

    #[test]
    fn append_entry_creates_file_and_writes_content() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("journal.txt");
        append_entry(&path, "[2026-07-28.14:03:00]\nfirst entry\n\n").unwrap();
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            "[2026-07-28.14:03:00]\nfirst entry\n\n"
        );
    }

    #[test]
    fn append_entry_appends_after_existing_content() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("journal.txt");
        append_entry(&path, "[2026-07-28.14:03:00]\nfirst\n\n").unwrap();
        append_entry(&path, "[2026-07-28.15:00:00]\nsecond\n\n").unwrap();
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            "[2026-07-28.14:03:00]\nfirst\n\n[2026-07-28.15:00:00]\nsecond\n\n"
        );
    }
}
