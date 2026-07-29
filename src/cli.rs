use clap::Parser;

/// `journal` -- append timestamped, taggable entries to a plain-text
/// journal file, and search that file by tag or keyword.
#[derive(Parser, Debug)]
#[command(name = "journal", version, about)]
pub struct Cli {
    /// Entry text to append. Trailing @tags are extracted automatically.
    /// Pass "-" to read the entry text from stdin instead. If omitted
    /// entirely, opens $EDITOR (or vi) to compose a new entry.
    #[arg(conflicts_with = "search")]
    pub text: Option<String>,

    /// Explicit tags for this entry, e.g. "@bp @health". Combined with any
    /// trailing tags already present in TEXT and de-duplicated.
    #[arg(short = 't', long = "tags", value_name = "TAGS", conflicts_with = "search")]
    pub tags: Option<String>,

    /// Path to the journal file. Overrides $JOURNAL_FILE and the XDG default.
    #[arg(short = 'f', long = "file", value_name = "PATH")]
    pub file: Option<String>,

    /// Search the journal instead of appending. Terms are whitespace-
    /// separated; a `+` inside a term joins words (linux+kernel matches
    /// the substring "linux kernel"). @-prefixed terms match tags exactly
    /// (full word); other terms are case-insensitive substring matches
    /// against the whole entry.
    #[arg(short = 's', long = "search", value_name = "QUERY")]
    pub search: Option<String>,

    /// Require every search term to match (default: match on any term).
    /// Only valid alongside -s/--search; checked in `Cli::validate`, not
    /// via clap's `requires`, which doesn't reliably fire here in
    /// combination with `text`'s `conflicts_with = "search"`.
    #[arg(short = 'a', long = "all")]
    pub all: bool,

    /// Cap the number of printed search matches. Only valid alongside
    /// -s/--search; see the note on `all` above.
    #[arg(long = "limit", value_name = "N")]
    pub limit: Option<usize>,
}

impl Cli {
    /// Validates flag combinations clap's derive attributes can't
    /// reliably express here. Returns a usage-error message on failure.
    pub fn validate(&self) -> Result<(), String> {
        if self.search.is_none() {
            if self.all {
                return Err("-a/--all can only be used with -s/--search".to_string());
            }
            if self.limit.is_some() {
                return Err("--limit can only be used with -s/--search".to_string());
            }
        }
        Ok(())
    }
}
