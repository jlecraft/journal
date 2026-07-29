mod cli;
mod entry;
mod storage;

use std::path::Path;

use anyhow::{bail, Result};
use clap::Parser;
use cli::Cli;
use entry::Entry;

fn main() {
    if let Err(err) = run() {
        eprintln!("journal: {err:#}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let cli = Cli::parse();
    let path = storage::resolve_path(cli.file.as_deref())?;

    match cli.text {
        Some(text) => append(&path, &text, cli.tags.as_deref()),
        None => bail!(
            "no entry text given, and editor mode isn't implemented yet -- \
             pass entry text as an argument for now, e.g. journal \"note @tag\""
        ),
    }
}

fn append(path: &Path, text: &str, tags_flag: Option<&str>) -> Result<()> {
    let (body, inline_tags) = entry::extract_trailing_tags(text);
    let flag_tags = tags_flag.map(entry::parse_tag_flag).unwrap_or_default();
    let tags = entry::merge_tags(inline_tags, flag_tags);
    let e = Entry::now(tags, body);
    storage::append_entry(path, &e.render())
}
