use clap::Parser;

/// `journal` -- append timestamped, taggable entries to a plain-text
/// journal file, and search that file by tag or keyword.
#[derive(Parser, Debug)]
#[command(
    name = "journal",
    version,
    about,
    after_help = "Show the last N entries with -N, e.g. `journal -3` prints the 3 most recent entries.",
    disable_help_flag = true
)]
pub struct Cli {
    /// Entry text to append. @tags may appear anywhere in the text and
    /// remain searchable right where you typed them. Pass "-" to read
    /// the entry text from stdin instead. If omitted entirely, opens
    /// $EDITOR (or vi) to compose a new entry.
    #[arg(conflicts_with = "search")]
    pub text: Option<String>,

    /// Show the last N entries, e.g. `-3`. Handled by scanning argv
    /// before clap parsing (see `extract_last_n_flag`) since clap can't
    /// express a short flag with a variable numeric name; never set by
    /// clap itself.
    #[arg(skip)]
    pub last: Option<usize>,

    /// Tags to append as their own line at the end of the entry, e.g.
    /// "bp health" or "@bp @health" -- bare words are automatically
    /// prefixed with @.
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
    /// Only valid alongside -s/--search.
    // Enforced in Cli::validate, not via clap's `requires`: that attribute
    // doesn't reliably fire here in combination with `text`'s
    // `conflicts_with = "search"` (verified via an isolated repro during
    // Milestone 4). This is a plain `//` comment, not `///`, on purpose --
    // clap derive turns doc comments into --help/man page text, and this
    // detail is for maintainers, not end users.
    #[arg(short = 'a', long = "all")]
    pub all: bool,

    /// Cap the number of printed search matches. Only valid alongside
    /// -s/--search.
    #[arg(long = "limit", value_name = "N")]
    pub limit: Option<usize>,

    /// Print only the matching lines of each entry (with the entry's
    /// header above them) instead of the full entry body. Combined with
    /// -a/--all, a line must contain every term itself to be shown, rather
    /// than the terms being spread across different lines of the entry.
    /// Only valid alongside -s/--search.
    #[arg(short = 'L', long = "lines-only")]
    pub lines_only: bool,

    /// Print diagnostic information to stderr: the resolved journal file,
    /// lock acquisition/release, and editor invocation details. Can be
    /// combined with any other mode.
    #[arg(short = 'v', long = "verbose")]
    pub verbose: bool,

    /// Show timestamps in human-readable form: [timestamp].format from
    /// config.toml (default "%Y-%m-%d %H:%M") plus an elapsed-time
    /// annotation controlled by [timestamp].diff -- "disabled" (none),
    /// "short" (e.g. "3h, 1m"; the default), or "long" (e.g. "3 hours, 1
    /// minute ago"). Without this flag, timestamps print exactly as
    /// stored on disk, with no annotation.
    #[arg(short = 'h', long = "human")]
    pub human: bool,

    /// Highlight matched search terms in ANSI color, even when stdout
    /// isn't a terminal (like `grep --color=always`). Without this or
    /// --no-color, falls back to [color].enabled in config.toml (default
    /// off). NO_COLOR, if set, always wins over both.
    #[arg(long = "color", conflicts_with = "no_color")]
    pub color: bool,

    /// Never highlight matched search terms, overriding [color].enabled.
    #[arg(long = "no-color")]
    pub no_color: bool,

    /// Print help.
    #[arg(long = "help", action = clap::ArgAction::Help)]
    help: Option<bool>,
}

impl Cli {
    /// Parses process argv into a `Cli`, first extracting a `-N`
    /// "show last N entries" flag (e.g. `journal -3`) that clap cannot
    /// express as a derive attribute -- a short flag whose name is a
    /// variable run of digits isn't representable there, and clap would
    /// otherwise either reject it as an unknown flag or, on some
    /// configurations, try to consume it as a negative-number value for
    /// another option. Pulling it out of argv before handing the rest to
    /// clap sidesteps both problems.
    pub fn parse_args() -> Self {
        Self::parse_from_argv(std::env::args())
    }

    fn parse_from_argv(args: impl Iterator<Item = String>) -> Self {
        let mut args: Vec<String> = args.collect();
        let last = extract_last_n_flag(&mut args);
        let mut cli = Cli::parse_from(args);
        cli.last = last;
        cli
    }

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
            if self.lines_only {
                return Err("-L/--lines-only can only be used with -s/--search".to_string());
            }
        }
        if let Some(n) = self.last {
            if n == 0 {
                return Err("-N must be a positive number".to_string());
            }
            if self.search.is_some() {
                return Err("-N cannot be combined with -s/--search".to_string());
            }
            if self.text.is_some() {
                return Err("-N cannot be combined with entry text".to_string());
            }
            if self.tags.is_some() {
                return Err("-N cannot be combined with -t/--tags".to_string());
            }
        }
        Ok(())
    }
}

/// Flags that consume the next argv token as their value. `-N` detection
/// must skip over those values (e.g. the `-3` in `--limit -3`) rather
/// than mistaking them for the last-N flag itself.
const VALUE_FLAGS: [&str; 7] = ["-f", "--file", "-t", "--tags", "-s", "--search", "--limit"];

/// Scans `args` (including argv[0]) for the first `-N` token, removes it,
/// and returns the parsed count. Stops at a bare `--`, since everything
/// after that end-of-options marker is positional text, not a flag.
fn extract_last_n_flag(args: &mut Vec<String>) -> Option<usize> {
    let mut i = 1;
    while i < args.len() {
        if args[i] == "--" {
            break;
        }
        if VALUE_FLAGS.contains(&args[i].as_str()) {
            i += 2;
            continue;
        }
        if is_last_n_token(&args[i]) {
            let raw = args.remove(i);
            return raw[1..].parse().ok();
        }
        i += 1;
    }
    None
}

/// True for tokens like `-3` or `-42`: a `-` followed by one or more
/// ASCII digits and nothing else. Excludes `--` and long flags since
/// their second byte isn't a digit.
fn is_last_n_token(s: &str) -> bool {
    let rest = match s.strip_prefix('-') {
        Some(r) if !r.is_empty() => r,
        _ => return false,
    };
    !rest.starts_with('-') && rest.bytes().all(|b| b.is_ascii_digit())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(s: &[&str]) -> Vec<String> {
        std::iter::once("journal".to_string())
            .chain(s.iter().map(|s| s.to_string()))
            .collect()
    }

    #[test]
    fn extracts_bare_last_n_flag() {
        let mut a = args(&["-3"]);
        assert_eq!(extract_last_n_flag(&mut a), Some(3));
        assert_eq!(a, args(&[]));
    }

    #[test]
    fn extracts_multi_digit_last_n_flag() {
        let mut a = args(&["-42"]);
        assert_eq!(extract_last_n_flag(&mut a), Some(42));
    }

    #[test]
    fn leaves_args_untouched_when_no_last_n_flag_present() {
        let mut a = args(&["-s", "foo", "-a"]);
        assert_eq!(extract_last_n_flag(&mut a), None);
        assert_eq!(a, args(&["-s", "foo", "-a"]));
    }

    #[test]
    fn does_not_mistake_a_value_flags_argument_for_last_n() {
        // The "-3" here is --limit's value, not a last-N flag.
        let mut a = args(&["-s", "foo", "--limit", "-3"]);
        assert_eq!(extract_last_n_flag(&mut a), None);
        assert_eq!(a, args(&["-s", "foo", "--limit", "-3"]));
    }

    #[test]
    fn does_not_treat_double_dash_or_long_flags_as_last_n() {
        let mut a = args(&["--", "-3"]);
        assert_eq!(extract_last_n_flag(&mut a), None);
    }

    #[test]
    fn does_not_match_non_numeric_dash_tokens() {
        let mut a = args(&["-a3"]);
        assert_eq!(extract_last_n_flag(&mut a), None);
    }

    #[test]
    fn validate_rejects_last_n_combined_with_search() {
        let cli = Cli {
            text: None,
            last: Some(3),
            tags: None,
            file: None,
            search: Some("foo".to_string()),
            all: false,
            limit: None,
            lines_only: false,
            verbose: false,
            human: false,
            color: false,
            no_color: false,
            help: None,
        };
        assert!(cli.validate().is_err());
    }

    #[test]
    fn validate_rejects_zero_as_last_n() {
        let cli = Cli {
            text: None,
            last: Some(0),
            tags: None,
            file: None,
            search: None,
            all: false,
            limit: None,
            lines_only: false,
            verbose: false,
            human: false,
            color: false,
            no_color: false,
            help: None,
        };
        assert!(cli.validate().is_err());
    }

    #[test]
    fn validate_accepts_bare_last_n() {
        let cli = Cli {
            text: None,
            last: Some(3),
            tags: None,
            file: None,
            search: None,
            all: false,
            limit: None,
            lines_only: false,
            verbose: false,
            human: false,
            color: false,
            no_color: false,
            help: None,
        };
        assert!(cli.validate().is_ok());
    }

    #[test]
    fn validate_rejects_lines_only_without_search() {
        let cli = Cli {
            text: Some("some entry".to_string()),
            last: None,
            tags: None,
            file: None,
            search: None,
            all: false,
            limit: None,
            lines_only: true,
            verbose: false,
            human: false,
            color: false,
            no_color: false,
            help: None,
        };
        assert!(cli.validate().is_err());
    }
}
