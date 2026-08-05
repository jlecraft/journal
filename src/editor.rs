use std::env;
use std::fs;
use std::path::Path;
use std::process::Command;

use anyhow::{bail, Context, Result};
use chrono::Local;

use crate::config::Config;
use crate::entry::{self, normalize_trailing_blank_lines, Entry, TIMESTAMP_FMT};
use crate::storage;
use crate::vlog;

/// Runs the no-argument "open $EDITOR to compose a new entry" flow
/// (§5.1). The timestamp is seeded only into a temporary buffer; the
/// real journal file is untouched unless the user actually saves.
/// `tags_flag` is `-t/--tags`'s raw value, if given -- its normalized
/// tags line is pre-seeded at the end of the new entry, same as the
/// non-editor append path (§2.1).
pub fn compose_new_entry(path: &Path, tags_flag: Option<&str>, verbose: bool) -> Result<()> {
    let config = crate::config::load()?;
    storage::with_exclusive_lock(path, verbose, || {
        compose_locked(path, tags_flag, &config, verbose)
    })
}

fn compose_locked(
    path: &Path,
    tags_flag: Option<&str>,
    config: &Config,
    verbose: bool,
) -> Result<()> {
    let existing = storage::read_contents(path)?;
    let timestamp_line = format!("[{}]", Local::now().format(TIMESTAMP_FMT));
    let tags_line = tags_flag.and_then(entry::tags_line_from_flag);
    let (seed, cursor_line) = seed_buffer(&existing, &timestamp_line, tags_line.as_deref());

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
    vlog(verbose, format!("editing via temp file {}", temp.path().display()));

    // Rust's default SIGINT/SIGTERM disposition terminates the process
    // without running destructors, so the temp file's Drop-based
    // auto-delete (tempfile::NamedTempFile) wouldn't fire if the user hits
    // Ctrl-C while $EDITOR is running -- clean it up explicitly instead.
    // Registration failure is ignored: this is a one-shot CLI invocation,
    // and falling back to the pre-existing (no cleanup) behavior beats
    // aborting an otherwise-fine editing session over this enhancement.
    let temp_path_for_signal = temp.path().to_path_buf();
    let _ = ctrlc::set_handler(move || {
        let _ = fs::remove_file(&temp_path_for_signal);
        eprintln!("journal: interrupted, removed temporary edit buffer");
        std::process::exit(130); // 128 + SIGINT, standard convention
    });

    let before_mtime = mtime_of(temp.path())?;

    let (program, base_args) = parse_editor_command(env::var("EDITOR").ok().as_deref())?;
    let cursor_args = cursor_positioning_args(config, cursor_line)?;
    vlog(verbose, format!("launching editor: {program} {base_args:?}"));
    // Cursor-positioning args go first, ahead of anything else in
    // $EDITOR: vim/vim-likes run `+cmd`/`-c cmd` arguments in the order
    // given, so if $EDITOR itself carries extra `-c` commands (e.g. for
    // scripting), those must run *after* the cursor has been moved, not
    // before -- otherwise they'd act on whatever line the editor lands
    // on by default instead of the seeded blank line.
    let status = Command::new(&program)
        .args(&cursor_args)
        .args(&base_args)
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
        vlog(verbose, "editor exited without saving changes");
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
    vlog(verbose, "entry saved");
    Ok(())
}

/// Builds the temp buffer's seed content and the 1-indexed line number
/// where the cursor should land: the blank line right after the new
/// timestamp, and before any `-t/--tags` line, so the user's cursor
/// starts on the line where they should begin typing the body.
fn seed_buffer(existing: &str, timestamp_line: &str, tags_line: Option<&str>) -> (String, usize) {
    let mut seed = format!("{existing}{timestamp_line}\n");
    let cursor_line = seed.matches('\n').count() + 1;
    seed.push('\n');
    if let Some(tags) = tags_line {
        seed.push_str(tags);
    }
    (seed, cursor_line)
}

fn mtime_of(path: &Path) -> Result<std::time::SystemTime> {
    fs::metadata(path)
        .and_then(|m| m.modified())
        .with_context(|| format!("failed to read modification time of {}", path.display()))
}

/// Finalizes the saved buffer before it's persisted: applies the same
/// blank-line normalization (§2) to the newly-composed entry that the
/// CLI append path applies. Tags typed inline in the editor need no
/// special handling -- they're just body text, searchable wherever they
/// land (§2.1). Everything before that entry is left exactly as the user
/// wrote it -- the tool only reformats what it itself just seeded, not
/// entries the user happened to pass through.
fn finalize_buffer(text: &str) -> String {
    match Entry::last_entry_start(text) {
        Some(offset) => {
            let prefix = &text[..offset];
            // last_entry_start only returns offsets of lines that parsed
            // successfully as headers, so Entry::parse here can't fail.
            let new_entry = Entry::parse(&text[offset..])
                .expect("last_entry_start guarantees a parseable header line");
            format!("{prefix}{}", new_entry.render())
        }
        // The user deleted every header line (including the one we
        // seeded) -- there's no entry structure left, so just normalize
        // the file's trailing blank lines as a whole.
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

/// Builds the extra argv entries that position the editor's cursor,
/// from the `editor.args` template in the config file (see `config.rs`).
/// There's no flag that works across every editor (vi/vim/nano/emacs use
/// `+N`; GUI editors vary), so this is opt-in: with no config, there are
/// no extra args and the cursor lands wherever the editor defaults to.
fn cursor_positioning_args(config: &Config, cursor_line: usize) -> Result<Vec<String>> {
    let Some(template) = &config.editor.args else {
        return Ok(Vec::new());
    };
    let filled = template.replace("{line}", &cursor_line.to_string());
    shell_words::split(&filled)
        .with_context(|| format!("failed to parse editor.args from config: {template:?}"))
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
    use crate::config::EditorConfig;

    #[test]
    fn seed_buffer_positions_cursor_on_the_blank_line_after_the_timestamp() {
        let (seed, cursor_line) = seed_buffer("", "[2026-07-28.14:03:00]", None);
        assert_eq!(seed, "[2026-07-28.14:03:00]\n\n");
        assert_eq!(cursor_line, 2);
    }

    #[test]
    fn seed_buffer_accounts_for_existing_content() {
        let existing = "[2026-01-01.00:00:00]\nold\n\n"; // 3 lines
        let (seed, cursor_line) = seed_buffer(existing, "[2026-07-28.14:03:00]", None);
        assert_eq!(
            seed,
            "[2026-01-01.00:00:00]\nold\n\n[2026-07-28.14:03:00]\n\n"
        );
        assert_eq!(cursor_line, 5);
    }

    #[test]
    fn seed_buffer_pre_seeds_the_tags_line_after_the_cursor_line() {
        let (seed, cursor_line) = seed_buffer("", "[2026-07-28.14:03:00]", Some("@bp @health"));
        assert_eq!(seed, "[2026-07-28.14:03:00]\n\n@bp @health");
        assert_eq!(cursor_line, 2);
    }

    #[test]
    fn cursor_positioning_args_is_empty_with_no_config() {
        let config = Config::default();
        assert!(cursor_positioning_args(&config, 5).unwrap().is_empty());
    }

    #[test]
    fn cursor_positioning_args_substitutes_the_line_placeholder() {
        let config = Config {
            editor: EditorConfig {
                args: Some("+{line}".to_string()),
            },
        };
        assert_eq!(cursor_positioning_args(&config, 5).unwrap(), vec!["+5"]);
    }

    #[test]
    fn cursor_positioning_args_splits_multiple_shell_words() {
        let config = Config {
            editor: EditorConfig {
                args: Some("+{line} -c startinsert".to_string()),
            },
        };
        assert_eq!(
            cursor_positioning_args(&config, 3).unwrap(),
            vec!["+3", "-c", "startinsert"]
        );
    }

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
    fn finalize_buffer_leaves_inline_tags_in_the_body_untouched() {
        let out = finalize_buffer("[2026-07-28.14:03:00]\nsome text @demo @another");
        assert_eq!(out, "[2026-07-28.14:03:00]\nsome text @demo @another\n\n");
    }

    #[test]
    fn finalize_buffer_leaves_earlier_entries_untouched() {
        let out = finalize_buffer(
            "[2026-01-01.00:00:00]\nold body @old-tag\n\n\
             [2026-07-28.14:03:00]\nnew body @demo",
        );
        assert_eq!(
            out,
            "[2026-01-01.00:00:00]\nold body @old-tag\n\n\
             [2026-07-28.14:03:00]\nnew body @demo\n\n"
        );
    }

    #[test]
    fn finalize_buffer_falls_back_to_whole_file_normalization_without_a_header() {
        let out = finalize_buffer("no header line survived editing\n\n\n");
        assert_eq!(out, "no header line survived editing\n\n");
    }
}
