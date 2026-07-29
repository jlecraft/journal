use std::io::Read;
use std::path::Path;

use anyhow::{Context, Result};
use clap::Parser;
use journal::cli::Cli;
use journal::entry::Entry;
use journal::{editor, entry, search, storage};

fn main() {
    let cli = Cli::parse();
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

    match cli.text {
        Some(text) => {
            // `-` means "read entry text from stdin" (§6.5), the standard
            // Unix filter-tool convention (e.g. `echo "..." | journal -`).
            let text = if text == "-" { read_stdin_text()? } else { text };
            append(&path, &text, cli.tags.as_deref())?;
            Ok(0)
        }
        None => {
            editor::compose_new_entry(&path)?;
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

fn append(path: &Path, text: &str, tags_flag: Option<&str>) -> Result<()> {
    let (body, inline_tags) = entry::extract_trailing_tags(text);
    let flag_tags = tags_flag.map(entry::parse_tag_flag).unwrap_or_default();
    let tags = entry::merge_tags(inline_tags, flag_tags);
    let e = Entry::now(tags, body);
    storage::append_entry(path, &e.render())
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
