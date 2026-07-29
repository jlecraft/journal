use std::io::Read;
use std::path::Path;

use anyhow::{Context, Result};
use journal::cli::Cli;
use journal::entry::Entry;
use journal::{editor, entry, search, storage};

fn main() {
    let cli = Cli::parse_args();
    if let Err(msg) = cli.validate() {
        eprintln!("journal: {msg}");
        std::process::exit(2);
    }
    match run(cli) {
        Ok(code) => std::process::exit(code),
        Err(err) => {
            eprintln!("journal: {err:#}");
            std::process::exit(1);
        }
    }
}

fn run(cli: Cli) -> Result<i32> {
    let path = storage::resolve_path(cli.file.as_deref())?;

    if let Some(query) = &cli.search {
        return run_search(&path, query, cli.all, cli.limit);
    }

    if let Some(n) = cli.last {
        return run_last(&path, n);
    }

    match cli.text {
        Some(text) => {
            // `-` means "read entry text from stdin" (§6.5), the standard
            // Unix filter-tool convention (e.g. `echo "..." | journal -`).
            let text = if text == "-" { read_stdin_text()? } else { text };
            append(&path, &text, cli.tags.as_deref())?;
            Ok(0)
        }
        None => {
            editor::compose_new_entry(&path, cli.tags.as_deref())?;
            Ok(0)
        }
    }
}

fn read_stdin_text() -> Result<String> {
    let mut text = String::new();
    std::io::stdin()
        .read_to_string(&mut text)
        .context("failed to read entry text from stdin")?;
    Ok(text)
}

/// Appends a new entry. `-t/--tags` (if given) is normalized into a tags
/// line -- bare words get an `@` prefix -- and concatenated onto its own
/// line at the end of the entry text (§2.1); any `@tag` already typed
/// inline in `text` is left exactly where it is, since a token is
/// recognized as a tag by its shape wherever it appears, not by position.
fn append(path: &Path, text: &str, tags_flag: Option<&str>) -> Result<()> {
    let tags_line = tags_flag.and_then(entry::tags_line_from_flag);
    let body = match tags_line {
        Some(line) if text.is_empty() => line,
        Some(line) => format!("{text}\n{line}"),
        None => text.to_string(),
    };
    let e = Entry::now(body);
    storage::append_entry(path, &e.render())
}

/// Prints the last `n` entries in the journal (oldest to newest, same as
/// `tail`), or fewer if the journal has less than `n` entries. An empty
/// journal is not an error: nothing is printed and the exit code is 0.
fn run_last(path: &Path, n: usize) -> Result<i32> {
    let contents = storage::read_contents(path)?;
    let entries = Entry::parse_all(&contents);
    let start = entries.len().saturating_sub(n);

    let mut out = String::new();
    for e in &entries[start..] {
        out.push_str(&e.render());
    }
    print!("{out}");
    Ok(0)
}

/// Returns the process exit code: 0 if at least one entry matched, 1 if
/// none did (grep-style; see §6.4 exit code convention).
fn run_search(path: &Path, query: &str, all: bool, limit: Option<usize>) -> Result<i32> {
    let contents = storage::read_contents(path)?;
    let entries = Entry::parse_all(&contents);
    let opts = search::SearchOptions { all, limit };
    let matches = search::search(&entries, query, &opts);

    if matches.is_empty() {
        return Ok(1);
    }

    let mut out = String::new();
    for e in &matches {
        out.push_str(&e.render());
    }
    print!("{out}");
    Ok(0)
}
