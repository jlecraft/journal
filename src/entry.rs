use chrono::{Local, NaiveDateTime};

/// `strftime`/`strptime` pattern matching the `[YYYY-MM-DD.HH:MM:SS]` format from §2.
pub const TIMESTAMP_FMT: &str = "%Y-%m-%d.%H:%M:%S";

/// `strftime` pattern for the header as shown to a human (`journal -s`,
/// `journal -N`), distinct from the on-disk `TIMESTAMP_FMT`.
const DISPLAY_TIMESTAMP_FMT: &str = "%Y-%m-%d %H:%M:%S";

/// An entry's timestamp line is always on its own line, with nothing
/// else on it (§2). Tags are no longer a separate structured field --
/// they're just `@word` tokens that happen to appear somewhere in the
/// body, whether typed inline by the user or appended as their own line
/// by `-t/--tags` (§2.1). `Entry::tags` recovers them on demand by
/// scanning the body, rather than the tool tracking them separately.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub timestamp: NaiveDateTime,
    pub body: String,
}

impl Entry {
    pub fn new(timestamp: NaiveDateTime, body: impl Into<String>) -> Self {
        Entry {
            timestamp,
            body: normalize_trailing_blank_lines(&body.into()),
        }
    }

    pub fn now(body: impl Into<String>) -> Self {
        Self::new(Local::now().naive_local(), body)
    }

    /// Every `@tag` token found anywhere in the body, in the order it
    /// appears. A token counts as a tag purely by its `@\S+` shape (§2.1)
    /// -- there's no separate storage to consult.
    pub fn tags(&self) -> Vec<String> {
        self.body
            .split_whitespace()
            .filter(|t| is_tag_token(t))
            .map(|t| t.to_string())
            .collect()
    }

    /// Renders the on-disk representation: the timestamp alone on its
    /// own line, the body, and the single trailing blank line that
    /// terminates every entry (§2).
    pub fn render(&self) -> String {
        let mut out = format!("[{}]\n", self.timestamp.format(TIMESTAMP_FMT));
        if !self.body.is_empty() {
            out.push_str(&self.body);
            out.push('\n');
        }
        out.push('\n');
        out
    }

    /// Renders an entry for display to a human (`journal -s`, `journal
    /// -N`): the same body as `render`, but with the header reformatted
    /// as `### YYYY-MM-DD HH:MM:SS` rather than the on-disk `[...]` form.
    /// An ATX heading, unlike a blockquote, has no lazy-continuation --
    /// Markdown renderers (e.g. `bat`) color only this line, not the body
    /// line that follows it.
    pub fn display(&self) -> String {
        let mut out = format!("### {}\n", self.timestamp.format(DISPLAY_TIMESTAMP_FMT));
        if !self.body.is_empty() {
            out.push_str(&self.body);
            out.push('\n');
        }
        out.push('\n');
        out
    }

    /// Parses a single entry block: a header line followed by its body
    /// lines (a trailing terminator blank line, if present, is trimmed).
    /// Any text found on the header line itself after the timestamp --
    /// which shouldn't normally be there, but can appear from a hand-edit
    /// or an older-format entry -- is folded into the body rather than
    /// discarded: parsing must never silently drop file content.
    pub fn parse(raw: &str) -> Option<Entry> {
        let mut lines = raw.lines();
        let header = lines.next()?;
        let (timestamp, header_overflow) = parse_header(header)?;
        let rest = lines.collect::<Vec<_>>().join("\n");
        let body = match header_overflow {
            Some(overflow) if rest.is_empty() => overflow,
            Some(overflow) => format!("{overflow}\n{rest}"),
            None => rest,
        };
        Some(Entry {
            timestamp,
            body: normalize_trailing_blank_lines(&body),
        })
    }

    /// Parses every entry out of a full journal file's contents. Entry
    /// boundaries are located by header lines rather than blank lines,
    /// since a body may legitimately contain blank lines of its own (§2).
    pub fn parse_all(text: &str) -> Vec<Entry> {
        let lines: Vec<&str> = text.lines().collect();
        let header_idxs: Vec<usize> = lines
            .iter()
            .enumerate()
            .filter_map(|(i, line)| parse_header(line).map(|_| i))
            .collect();

        header_idxs
            .iter()
            .enumerate()
            .filter_map(|(n, &start)| {
                let end = header_idxs.get(n + 1).copied().unwrap_or(lines.len());
                Entry::parse(&lines[start..end].join("\n"))
            })
            .collect()
    }

    /// Returns the byte offset in `text` where its last header-line-led
    /// entry begins, or `None` if `text` contains no header line at all.
    /// Used by editor mode to isolate the newly-composed entry from
    /// everything before it, which is left untouched.
    pub(crate) fn last_entry_start(text: &str) -> Option<usize> {
        let mut offset = 0;
        let mut last = None;
        for line in text.split_inclusive('\n') {
            let trimmed = line.strip_suffix('\n').unwrap_or(line);
            if parse_header(trimmed).is_some() {
                last = Some(offset);
            }
            offset += line.len();
        }
        last
    }
}

/// Parses a header line into `(timestamp, header_overflow)`. The
/// timestamp line is always alone (§2); `header_overflow` is whatever
/// text (if any) follows the closing `]` on that same line -- not
/// expected in files this tool writes, but preserved rather than dropped
/// if found (e.g. a hand-edit, or a file from before this format change).
fn parse_header(line: &str) -> Option<(NaiveDateTime, Option<String>)> {
    let rest = line.strip_prefix('[')?;
    let (ts_str, rest) = rest.split_once(']')?;
    let timestamp = NaiveDateTime::parse_from_str(ts_str, TIMESTAMP_FMT).ok()?;
    let overflow = rest.trim();
    Some((timestamp, (!overflow.is_empty()).then(|| overflow.to_string())))
}

fn is_tag_token(token: &str) -> bool {
    token.starts_with('@') && token.len() > 1
}

/// Builds the tags line appended by `-t/--tags` (§2.1): splits `s` on
/// whitespace and prefixes any bare word with `@`, so `-t "beer @store"`
/// becomes `@beer @store`. Returns `None` if `s` has no tokens at all
/// (e.g. an empty or whitespace-only flag value), since then there's no
/// line to add.
pub fn tags_line_from_flag(s: &str) -> Option<String> {
    let tags: Vec<String> = s
        .split_whitespace()
        .map(|tok| if tok.starts_with('@') { tok.to_string() } else { format!("@{tok}") })
        .collect();
    (!tags.is_empty()).then(|| tags.join(" "))
}

/// Trims trailing blank (or whitespace-only) lines from `body`, per the
/// terminator normalization rule in §2. Also reused by editor mode
/// (§5.1) to normalize the whole file after a save.
pub(crate) fn normalize_trailing_blank_lines(body: &str) -> String {
    let mut lines: Vec<&str> = body.lines().collect();
    while matches!(lines.last(), Some(l) if l.trim().is_empty()) {
        lines.pop();
    }
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    fn ts(y: i32, mo: u32, d: u32, h: u32, mi: u32, s: u32) -> NaiveDateTime {
        NaiveDate::from_ymd_opt(y, mo, d)
            .unwrap()
            .and_hms_opt(h, mi, s)
            .unwrap()
    }

    #[test]
    fn renders_timestamp_alone_on_its_own_line() {
        let e = Entry::new(ts(2026, 7, 28, 14, 3, 0), "124/80/55 @bp @health");
        assert_eq!(
            e.render(),
            "[2026-07-28.14:03:00]\n124/80/55 @bp @health\n\n"
        );
    }

    #[test]
    fn renders_header_with_no_body() {
        let e = Entry::new(ts(2026, 7, 28, 14, 3, 0), "");
        assert_eq!(e.render(), "[2026-07-28.14:03:00]\n\n");
    }

    #[test]
    fn collapses_trailing_blank_lines_to_one() {
        let e = Entry::new(ts(2026, 7, 28, 14, 3, 0), "line1\nline2\n\n\n\n");
        assert_eq!(e.render(), "[2026-07-28.14:03:00]\nline1\nline2\n\n");
    }

    #[test]
    fn appends_blank_line_when_none_present() {
        let e = Entry::new(ts(2026, 7, 28, 14, 3, 0), "just one line");
        assert_eq!(e.render(), "[2026-07-28.14:03:00]\njust one line\n\n");
    }

    #[test]
    fn tags_are_recovered_from_anywhere_in_the_body() {
        let e = Entry::new(
            ts(2026, 7, 28, 14, 3, 0),
            "my @blood_pressure was 117/75/50",
        );
        assert_eq!(e.tags(), vec!["@blood_pressure"]);
    }

    #[test]
    fn tags_line_appended_by_dash_t_is_also_recovered() {
        let e = Entry::new(ts(2026, 7, 28, 14, 3, 0), "124/80/55\n@bp @health");
        assert_eq!(e.tags(), vec!["@bp", "@health"]);
    }

    #[test]
    fn entry_with_no_at_tokens_has_no_tags() {
        let e = Entry::new(ts(2026, 7, 28, 14, 3, 0), "just plain text");
        assert!(e.tags().is_empty());
    }

    #[test]
    fn tags_line_from_flag_prefixes_bare_words() {
        assert_eq!(
            tags_line_from_flag("beer @store"),
            Some("@beer @store".to_string())
        );
    }

    #[test]
    fn tags_line_from_flag_leaves_already_prefixed_tags_alone() {
        assert_eq!(
            tags_line_from_flag("@bp @health"),
            Some("@bp @health".to_string())
        );
    }

    #[test]
    fn tags_line_from_flag_is_none_for_blank_input() {
        assert_eq!(tags_line_from_flag(""), None);
        assert_eq!(tags_line_from_flag("   "), None);
    }

    #[test]
    fn round_trips_through_render_and_parse() {
        let e = Entry::new(ts(2026, 7, 28, 14, 3, 0), "124/80/55 @bp");
        let parsed = Entry::parse(e.render().trim_end_matches('\n')).unwrap();
        assert_eq!(parsed.timestamp, e.timestamp);
        assert_eq!(parsed.body, e.body);
    }

    #[test]
    fn stray_text_on_the_header_line_is_preserved_in_the_body() {
        // The canonical format only ever puts the timestamp on the
        // header line (§2), but a hand-edit -- or a file from before this
        // format existed -- could leave text there instead. Parsing must
        // fold it into the body, not silently drop it.
        let parsed = Entry::parse("[2026-07-28.14:03:00] oops no newline before this").unwrap();
        assert_eq!(parsed.body, "oops no newline before this");
    }

    #[test]
    fn header_overflow_is_prepended_to_an_existing_body() {
        let parsed = Entry::parse("[2026-07-28.14:03:00] oops\nreal body line").unwrap();
        assert_eq!(parsed.body, "oops\nreal body line");
    }

    #[test]
    fn old_format_header_tags_are_folded_into_the_body() {
        // A file written before this format change may still have tags
        // on the header line; they're preserved as body text (still
        // searchable as tags) rather than dropped.
        let parsed = Entry::parse("[2026-07-28.14:03:00] @bp @health\n124/80/55").unwrap();
        assert_eq!(parsed.body, "@bp @health\n124/80/55");
        assert_eq!(parsed.tags(), vec!["@bp", "@health"]);
    }

    #[test]
    fn parses_multiple_entries_from_a_journal_file() {
        let file = "[2026-07-28.14:03:00]\n124/80/55 @bp\n\n[2026-07-29.09:00:00]\nslept fine\n\n";
        let entries = Entry::parse_all(file);
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].body, "124/80/55 @bp");
        assert_eq!(entries[1].body, "slept fine");
    }

    #[test]
    fn parse_all_tolerates_blank_lines_inside_a_body() {
        let file = "[2026-07-28.14:03:00]\npara one\n\npara two\n\n[2026-07-29.09:00:00]\nnext entry\n\n";
        let entries = Entry::parse_all(file);
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].body, "para one\n\npara two");
        assert_eq!(entries[1].body, "next entry");
    }

    #[test]
    fn last_entry_start_finds_offset_of_final_header() {
        let file = "[2026-07-28.14:03:00]\nfirst\n\n[2026-07-29.09:00:00]\nsecond";
        let offset = Entry::last_entry_start(file).unwrap();
        assert_eq!(&file[offset..], "[2026-07-29.09:00:00]\nsecond");
    }

    #[test]
    fn last_entry_start_finds_the_only_header_with_no_trailing_newline() {
        let file = "[2026-07-28.14:03:00]";
        let offset = Entry::last_entry_start(file).unwrap();
        assert_eq!(offset, 0);
    }

    #[test]
    fn last_entry_start_is_none_without_any_header_line() {
        assert_eq!(Entry::last_entry_start("just some text\nno header here"), None);
    }
}
