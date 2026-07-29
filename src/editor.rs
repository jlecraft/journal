use std::env;
use std::fs;
use std::path::Path;
use std::process::Command;

use anyhow::{bail, Context, Result};
use chrono::Local;

use crate::entry::{self, normalize_trailing_blank_lines, Entry, TIMESTAMP_FMT};
use crate::storage;

/// Runs the no-argument "open $EDITOR to compose a new entry" flow
/// (§5.1). The timestamp is seeded only into a temporary buffer; the
/// real journal file is untouched unless the user actually saves.
pub fn compose_new_entry(path: &Path) -> Result<()> {
    storage::with_exclusive_lock(path, || compose_locked(path))
}

fn compose_locked(path: &Path) -> Result<()> {
    let existing = storage::read_contents(path)?;
    let timestamp_line = format!("[{}]", Local::now().format(TIMESTAMP_FMT));
    let seed = format!("{existing}{timestamp_line}");

    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let temp = tempfile::Builder::new()
        .prefix(".journal-edit-")
        .suffix(".txt")
        .tempfile_in(parent)
        .context("failed to create a temporary file for editing")?;
    fs::write(temp.path(), seed.as_bytes())
        .context("failed to seed the temporary edit buffer")?;

    let before_mtime = mtime_of(temp.path())?;

    let (program, args) = parse_editor_command(env::var("EDITOR").ok().as_deref())?;
    let status = Command::new(&program)
        .args(&args)
        .arg(temp.path())
        .status()
        .with_context(|| format!("failed to launch editor `{program}`"))?;

    if !status.success() {
        bail!("editor exited with a non-zero status; entry was not saved");
    }

    let after_mtime = mtime_of(temp.path())?;
    if after_mtime == before_mtime {
        // Editor exited successfully but never wrote the file (e.g. `:q`
        // with nothing typed, or `:q!`) -- nothing to persist, and the
        // real journal file is left exactly as it was (§5.1).
        return Ok(());
    }

    let edited = fs::read_to_string(temp.path())
        .context("failed to read back the edited buffer")?;
    let normalized = finalize_buffer(&edited);
    fs::write(temp.path(), normalized.as_bytes())
        .context("failed to write normalized entry content")?;

    temp.persist(path)
        .map_err(|e| e.error)
        .with_context(|| format!("failed to atomically replace {}", path.display()))?;
    Ok(())
}

fn mtime_of(path: &Path) -> Result<std::time::SystemTime> {
    fs::metadata(path)
        .and_then(|m| m.modified())
        .with_context(|| format!("failed to read modification time of {}", path.display()))
}

/// Finalizes the saved buffer before it's persisted: applies the same
/// trailing-tag-hoisting (§2.1) and blank-line normalization (§2) to the
/// newly-composed entry that the CLI append path applies, so a tag typed
/// at the end of the body in the editor is just as searchable as one
/// passed on the command line. Everything before that entry is left
/// exactly as the user wrote it -- the tool only reformats what it
/// itself just seeded, not entries the user happened to pass through.
fn finalize_buffer(text: &str) -> String {
    match Entry::last_entry_start(text) {
        Some(offset) => {
            let prefix = &text[..offset];
            // last_entry_start only returns offsets of lines that parsed
            // successfully as headers, so Entry::parse here can't fail.
            let new_entry = Entry::parse(&text[offset..])
                .expect("last_entry_start guarantees a parseable header line");
            let (body, inline_tags) = entry::extract_trailing_tags(&new_entry.body);
            let tags = entry::merge_tags(new_entry.tags, inline_tags);
            let finalized = Entry::new(new_entry.timestamp, tags, body);
            format!("{prefix}{}", finalized.render())
        }
        // The user deleted every header line (including the one we
        // seeded) -- there's no entry structure left to hoist tags into,
        // so just normalize the file's trailing blank lines as a whole.
        None => {
            let trimmed = normalize_trailing_blank_lines(text);
            if trimmed.is_empty() {
                String::new()
            } else {
                format!("{trimmed}\n\n")
            }
        }
    }
}

/// Parses `$EDITOR` (falling back to `vi` if unset, per §5) into a
/// program name and its leading arguments, e.g. `"code --wait"` ->
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

    #[test]
    fn finalize_buffer_collapses_trailing_blank_lines() {
        let out = finalize_buffer("[2026-07-28.14:03:00]\nbody\n\n\n\n");
        assert_eq!(out, "[2026-07-28.14:03:00]\nbody\n\n");
    }

    #[test]
    fn finalize_buffer_of_fully_blank_text_is_empty() {
        assert_eq!(finalize_buffer("\n\n   \n"), "");
    }

    #[test]
    fn finalize_buffer_hoists_trailing_body_tags_onto_the_header() {
        let out = finalize_buffer("[2026-07-28.14:03:00]\nsome text @demo @another");
        assert_eq!(out, "[2026-07-28.14:03:00] @demo @another\nsome text\n\n");
    }

    #[test]
    fn finalize_buffer_leaves_earlier_entries_untouched() {
        let out = finalize_buffer(
            "[2026-01-01.00:00:00] @old\nold body @looks-like-a-tag-but-isnt-trailing extra\n\n\
             [2026-07-28.14:03:00]\nnew body @demo",
        );
        assert_eq!(
            out,
            "[2026-01-01.00:00:00] @old\nold body @looks-like-a-tag-but-isnt-trailing extra\n\n\
             [2026-07-28.14:03:00] @demo\nnew body\n\n"
        );
    }

    #[test]
    fn finalize_buffer_falls_back_to_whole_file_normalization_without_a_header() {
        let out = finalize_buffer("no header line survived editing\n\n\n");
        assert_eq!(out, "no header line survived editing\n\n");
    }
}
