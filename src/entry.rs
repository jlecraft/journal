use chrono::{Local, NaiveDateTime};
use std::collections::HashSet;

/// `strftime`/`strptime` pattern matching the `[YYYY-MM-DD.HH:MM:SS]` format from §2.
pub const TIMESTAMP_FMT: &str = "%Y-%m-%d.%H:%M:%S";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub timestamp: NaiveDateTime,
    pub tags: Vec<String>,
    pub body: String,
}

impl Entry {
    pub fn new(timestamp: NaiveDateTime, tags: Vec<String>, body: impl Into<String>) -> Self {
        Entry {
            timestamp,
            tags,
            body: normalize_trailing_blank_lines(&body.into()),
        }
    }

    pub fn now(tags: Vec<String>, body: impl Into<String>) -> Self {
        Self::new(Local::now().naive_local(), tags, body)
    }

    /// Renders the on-disk representation: header line, normalized body,
    /// and the single trailing blank line that terminates every entry (§2).
    pub fn render(&self) -> String {
        let mut out = format!("[{}]", self.timestamp.format(TIMESTAMP_FMT));
        for tag in &self.tags {
            out.push(' ');
            out.push_str(tag);
        }
        out.push('\n');
        if !self.body.is_empty() {
            out.push_str(&self.body);
            out.push('\n');
        }
        out.push('\n');
        out
    }

    /// Parses a single entry block: a header line followed by its body
    /// lines (a trailing terminator blank line, if present, is trimmed).
    pub fn parse(raw: &str) -> Option<Entry> {
        let mut lines = raw.lines();
        let header = lines.next()?;
        let (timestamp, tags) = parse_header(header)?;
        let body = normalize_trailing_blank_lines(&lines.collect::<Vec<_>>().join("\n"));
        Some(Entry {
            timestamp,
            tags,
            body,
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
}

fn parse_header(line: &str) -> Option<(NaiveDateTime, Vec<String>)> {
    let rest = line.strip_prefix('[')?;
    let (ts_str, rest) = rest.split_once(']')?;
    let timestamp = NaiveDateTime::parse_from_str(ts_str, TIMESTAMP_FMT).ok()?;
    let tags = rest
        .split_whitespace()
        .filter(|t| is_tag_token(t))
        .map(|t| t.to_string())
        .collect();
    Some((timestamp, tags))
}

fn is_tag_token(token: &str) -> bool {
    token.starts_with('@') && token.len() > 1
}

/// Parses `-t/--tags` flag content (§2.1) into individual `@tag` tokens,
/// ignoring anything in the string that isn't a well-formed tag.
pub fn parse_tag_flag(s: &str) -> Vec<String> {
    s.split_whitespace()
        .filter(|t| is_tag_token(t))
        .map(|t| t.to_string())
        .collect()
}

/// Strips a contiguous run of trailing `@tag` tokens from `text` (§2.1) and
/// returns `(remaining_body, extracted_tags)`. Only tokens preceded by
/// whitespace and unbroken through to the end of the text count; a single
/// non-tag token anywhere in the trailing run stops the scan.
pub fn extract_trailing_tags(text: &str) -> (String, Vec<String>) {
    let mut tokens: Vec<(usize, usize)> = Vec::new();
    let mut start: Option<usize> = None;
    for (i, c) in text.char_indices() {
        if c.is_whitespace() {
            if let Some(s) = start.take() {
                tokens.push((s, i));
            }
        } else if start.is_none() {
            start = Some(i);
        }
    }
    if let Some(s) = start {
        tokens.push((s, text.len()));
    }

    let tag_count = tokens
        .iter()
        .rev()
        .take_while(|&&(s, e)| is_tag_token(&text[s..e]))
        .count();

    if tag_count == 0 {
        return (text.to_string(), Vec::new());
    }

    let split_idx = tokens.len() - tag_count;
    let tags: Vec<String> = tokens[split_idx..]
        .iter()
        .map(|&(s, e)| text[s..e].to_string())
        .collect();
    let body_end = if split_idx == 0 { 0 } else { tokens[split_idx - 1].1 };
    (text[..body_end].to_string(), tags)
}

/// Combines inline-extracted tags with `-t` flag tags, de-duplicating on
/// exact case-sensitive match while keeping first-seen order (§2.1).
pub fn merge_tags(inline: Vec<String>, flag: Vec<String>) -> Vec<String> {
    let mut seen = HashSet::new();
    inline
        .into_iter()
        .chain(flag)
        .filter(|t| seen.insert(t.clone()))
        .collect()
}

/// Trims trailing blank (or whitespace-only) lines from `body`, per the
/// terminator normalization rule in §2.
fn normalize_trailing_blank_lines(body: &str) -> String {
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
    fn renders_header_with_tags_and_body() {
        let e = Entry::new(
            ts(2026, 7, 28, 14, 3, 0),
            vec!["@bp".into(), "@health".into()],
            "124/80/55",
        );
        assert_eq!(e.render(), "[2026-07-28.14:03:00] @bp @health\n124/80/55\n\n");
    }

    #[test]
    fn renders_header_with_no_tags() {
        let e = Entry::new(ts(2026, 7, 28, 14, 3, 0), vec![], "no tags here");
        assert_eq!(e.render(), "[2026-07-28.14:03:00]\nno tags here\n\n");
    }

    #[test]
    fn collapses_trailing_blank_lines_to_one() {
        let e = Entry::new(ts(2026, 7, 28, 14, 3, 0), vec![], "line1\nline2\n\n\n\n");
        assert_eq!(e.render(), "[2026-07-28.14:03:00]\nline1\nline2\n\n");
    }

    #[test]
    fn appends_blank_line_when_none_present() {
        let e = Entry::new(ts(2026, 7, 28, 14, 3, 0), vec![], "just one line");
        assert_eq!(e.render(), "[2026-07-28.14:03:00]\njust one line\n\n");
    }

    #[test]
    fn extracts_trailing_inline_tags() {
        let (body, tags) = extract_trailing_tags("124/80/55 @bp @health");
        assert_eq!(body, "124/80/55");
        assert_eq!(tags, vec!["@bp", "@health"]);
    }

    #[test]
    fn leaves_non_trailing_at_tokens_in_body() {
        // An @-token that isn't part of the trailing run stays in the body.
        let (body, tags) = extract_trailing_tags("emailed foo@bar.com about it @followup");
        assert_eq!(body, "emailed foo@bar.com about it");
        assert_eq!(tags, vec!["@followup"]);
    }

    #[test]
    fn no_trailing_tags_leaves_text_untouched() {
        let (body, tags) = extract_trailing_tags("just a plain entry");
        assert_eq!(body, "just a plain entry");
        assert!(tags.is_empty());
    }

    #[test]
    fn all_tags_no_body() {
        let (body, tags) = extract_trailing_tags("@bp @health");
        assert_eq!(body, "");
        assert_eq!(tags, vec!["@bp", "@health"]);
    }

    #[test]
    fn parses_tag_flag_value() {
        assert_eq!(parse_tag_flag("@bp @health"), vec!["@bp", "@health"]);
    }

    #[test]
    fn merges_and_dedupes_case_sensitive() {
        let merged = merge_tags(
            vec!["@bp".into(), "@health".into()],
            vec!["@health".into(), "@Health".into(), "@bp".into()],
        );
        // First-seen order wins; "@Health" survives as distinct from "@health".
        assert_eq!(merged, vec!["@bp", "@health", "@Health"]);
    }

    #[test]
    fn round_trips_through_render_and_parse() {
        let e = Entry::new(
            ts(2026, 7, 28, 14, 3, 0),
            vec!["@bp".into()],
            "124/80/55",
        );
        let parsed = Entry::parse(e.render().trim_end_matches('\n')).unwrap();
        assert_eq!(parsed.timestamp, e.timestamp);
        assert_eq!(parsed.tags, e.tags);
        assert_eq!(parsed.body, e.body);
    }

    #[test]
    fn parses_multiple_entries_from_a_journal_file() {
        let file = "[2026-07-28.14:03:00] @bp\n124/80/55\n\n[2026-07-29.09:00:00]\nslept fine\n\n";
        let entries = Entry::parse_all(file);
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].tags, vec!["@bp"]);
        assert_eq!(entries[0].body, "124/80/55");
        assert_eq!(entries[1].tags, Vec::<String>::new());
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
}
