use clap::Parser;

/// `journal` -- append timestamped, taggable entries to a plain-text
/// journal file, and search that file by tag or keyword.
#[derive(Parser, Debug)]
#[command(name = "journal", version, about)]
pub struct Cli {
    /// Entry text to append. Trailing @tags are extracted automatically.
    /// If omitted, opens $EDITOR (or vi) to compose a new entry.
    pub text: Option<String>,

    /// Explicit tags for this entry, e.g. "@bp @health". Combined with any
    /// trailing tags already present in TEXT and de-duplicated.
    #[arg(short = 't', long = "tags", value_name = "TAGS")]
    pub tags: Option<String>,

    /// Path to the journal file. Overrides $JOURNAL_FILE and the XDG default.
    #[arg(short = 'f', long = "file", value_name = "PATH")]
    pub file: Option<String>,
}
