use std::env;
use std::path::Path;
use std::process::Command;

use anyhow::{bail, Context, Result};

use crate::storage;
use crate::vlog;

/// Runs the no-argument "open `$EDITOR`" flow (§5.1): launches `$EDITOR`
/// directly on the real journal file, under the same write lock
/// `append_entry` uses, so a concurrent append can't interleave with an
/// open editing session. No content is seeded, no temp file or copy is
/// involved -- whatever the user types and saves (or doesn't) is exactly
/// what ends up on disk.
pub fn open_in_editor(path: &Path, verbose: bool) -> Result<()> {
    storage::with_exclusive_lock(path, verbose, || edit_locked(path, verbose))
}

fn edit_locked(path: &Path, verbose: bool) -> Result<()> {
    storage::ensure_exists(path)?;
    let (program, args) = parse_editor_command(env::var("EDITOR").ok().as_deref())?;
    vlog(verbose, format!("launching editor: {program} {args:?}"));
    let status = Command::new(&program)
        .args(&args)
        .arg(path)
        .status()
        .with_context(|| format!("failed to launch editor `{program}`"))?;
    if !status.success() {
        bail!("editor exited with a non-zero status");
    }
    vlog(verbose, "editor exited");
    Ok(())
}

/// Parses `$EDITOR` (falling back to `vi` if unset, per §5) into a
/// program name and its arguments, e.g. `"code --wait"` ->
/// `("code", ["--wait"])`. Uses shell-style word splitting so quoted
/// paths with spaces work.
fn parse_editor_command(editor_env: Option<&str>) -> Result<(String, Vec<String>)> {
    let editor = editor_env.unwrap_or("vi");
    let mut parts = shell_words::split(editor)
        .with_context(|| format!("failed to parse $EDITOR value: {editor:?}"))?;
    if parts.is_empty() {
        bail!("$EDITOR is set to an empty command");
    }
    let program = parts.remove(0);
    Ok((program, parts))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_to_vi_when_editor_unset() {
        let (program, args) = parse_editor_command(None).unwrap();
        assert_eq!(program, "vi");
        assert!(args.is_empty());
    }

    #[test]
    fn splits_editor_with_trailing_args() {
        let (program, args) = parse_editor_command(Some("code --wait")).unwrap();
        assert_eq!(program, "code");
        assert_eq!(args, vec!["--wait"]);
    }

    #[test]
    fn honors_shell_style_quoting() {
        let (program, args) = parse_editor_command(Some("'my editor' --flag")).unwrap();
        assert_eq!(program, "my editor");
        assert_eq!(args, vec!["--flag"]);
    }

    #[test]
    fn empty_editor_value_is_an_error() {
        assert!(parse_editor_command(Some("   ")).is_err());
    }
}
