use crate::entry::Entry;

pub struct SearchOptions {
    pub all: bool,
    pub limit: Option<usize>,
}

/// A parsed search term (§3): either an `@tag` requiring a full-word
/// match against the entry's tag list, or a plain term matched as a
/// case-insensitive substring anywhere in the entry (timestamp + body).
enum Term {
    Tag(String),
    Substring(String),
}

fn parse_terms(query: &str) -> Vec<Term> {
    query
        .split_whitespace()
        .map(|tok| tok.replace('+', " "))
        .map(|term| {
            if term.starts_with('@') && term.len() > 1 {
                Term::Tag(term.to_lowercase())
            } else {
                Term::Substring(term.to_lowercase())
            }
        })
        .collect()
}

fn term_matches(term: &Term, entry: &Entry, haystack: &str) -> bool {
    match term {
        // Tags are no longer stored separately (§2.1) -- `entry.tags()`
        // recovers them by scanning the body for `@\S+` tokens, wherever
        // they happen to appear.
        Term::Tag(t) => entry.tags().iter().any(|tag| tag.to_lowercase() == *t),
        Term::Substring(s) => haystack.contains(s.as_str()),
    }
}

/// Returns entries matching `query` under the given options, in the
/// order they appear in `entries`, truncated to `opts.limit` if set.
pub fn search<'a>(entries: &'a [Entry], query: &str, opts: &SearchOptions) -> Vec<&'a Entry> {
    let terms = parse_terms(query);
    if terms.is_empty() {
        return Vec::new();
    }

    let mut matches: Vec<&Entry> = entries
        .iter()
        .filter(|e| {
            let haystack = e.render().to_lowercase();
            if opts.all {
                terms.iter().all(|t| term_matches(t, e, &haystack))
            } else {
                terms.iter().any(|t| term_matches(t, e, &haystack))
            }
        })
        .collect();

    if let Some(limit) = opts.limit {
        matches.truncate(limit);
    }
    matches
}

fn line_matches_term(line: &str, lower_line: &str, term: &Term) -> bool {
    match term {
        // Scoped to this one line, unlike `term_matches`'s `entry.tags()`
        // (which scans the whole body) -- `-L/--lines-only` needs to know
        // whether *this* line carries the tag, not the entry as a whole.
        Term::Tag(t) => line.split_whitespace().any(|tok| tok.to_lowercase() == *t),
        Term::Substring(s) => lower_line.contains(s.as_str()),
    }
}

/// Like `search`, but selects individual body lines rather than whole
/// entries, for `-L/--lines-only`: an entry is included only if at least
/// one of its body lines satisfies the term condition, and only those
/// qualifying lines (not the full body) are returned alongside the entry.
/// In `--all` mode a line must contain *every* term itself -- unlike
/// `search`, which is satisfied if the terms are spread across different
/// lines anywhere in the entry.
///
/// Truncation via `opts.limit` caps the number of entries returned (same
/// meaning as in `search`), not the number of lines.
pub fn search_lines<'a>(
    entries: &'a [Entry],
    query: &str,
    opts: &SearchOptions,
) -> Vec<(&'a Entry, Vec<&'a str>)> {
    let terms = parse_terms(query);
    if terms.is_empty() {
        return Vec::new();
    }

    let mut results: Vec<(&Entry, Vec<&str>)> = Vec::new();
    for e in entries {
        let lines: Vec<&str> = e
            .body
            .lines()
            .filter(|line| {
                let lower = line.to_lowercase();
                if opts.all {
                    terms.iter().all(|t| line_matches_term(line, &lower, t))
                } else {
                    terms.iter().any(|t| line_matches_term(line, &lower, t))
                }
            })
            .collect();
        if !lines.is_empty() {
            results.push((e, lines));
        }
    }

    if let Some(limit) = opts.limit {
        results.truncate(limit);
    }
    results
}

/// ANSI bold-red, matching `grep`'s default `GREP_COLORS` match style.
const HIGHLIGHT_START: &str = "\x1b[1;31m";
const HIGHLIGHT_END: &str = "\x1b[0m";

/// Wraps every occurrence of `query`'s terms in `display_text` with ANSI
/// color codes, so a match stands out inside a long entry the way `grep
/// --color` highlights a match inside a long line. Caller decides *whether*
/// to call this at all (only when stdout is an interactive terminal and
/// `NO_COLOR` isn't set) -- this function always colors when it finds a
/// term, regardless of TTY.
pub fn highlight(display_text: &str, query: &str) -> String {
    let terms = parse_terms(query);
    if terms.is_empty() {
        return display_text.to_string();
    }

    let lower = display_text.to_ascii_lowercase();
    let mut ranges: Vec<(usize, usize)> = Vec::new();
    for term in &terms {
        match term {
            Term::Substring(s) => ranges.extend(find_all(&lower, s)),
            Term::Tag(t) => ranges.extend(find_tag_tokens(display_text, &lower, t)),
        }
    }
    if ranges.is_empty() {
        return display_text.to_string();
    }

    apply_highlights(display_text, ranges)
}

/// Byte ranges of every (possibly overlapping) occurrence of `needle` in
/// `haystack`. Both must already be ASCII-lowercased by the caller so
/// offsets found here are valid byte offsets into the original text too
/// (ASCII lowering is byte-length-preserving and never touches non-ASCII
/// bytes, unlike the full-Unicode `.to_lowercase()` used for match
/// *determination* in `term_matches` -- a deliberate narrower case-fold
/// here, since it only decides which bytes get colored, not whether an
/// entry matches).
fn find_all(haystack: &str, needle: &str) -> Vec<(usize, usize)> {
    if needle.is_empty() {
        return Vec::new();
    }
    let mut out = Vec::new();
    let mut start = 0;
    while let Some(pos) = haystack[start..].find(needle) {
        let begin = start + pos;
        let end = begin + needle.len();
        out.push((begin, end));
        start = begin + 1;
    }
    out
}

/// Byte ranges of whitespace-delimited tokens in `display_text` that
/// case-insensitively equal `tag` exactly -- the same full-word semantics
/// `term_matches` uses for `Term::Tag`, so e.g. `@bp` never highlights
/// `@bph`. `lower` is `display_text.to_ascii_lowercase()`, reused so this
/// doesn't recompute it per tag term.
fn find_tag_tokens(display_text: &str, lower: &str, tag: &str) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let mut token_start: Option<usize> = None;
    for (i, c) in display_text.char_indices() {
        if c.is_whitespace() {
            if let Some(start) = token_start.take()
                && &lower[start..i] == tag
            {
                out.push((start, i));
            }
        } else if token_start.is_none() {
            token_start = Some(i);
        }
    }
    if let Some(start) = token_start
        && &lower[start..display_text.len()] == tag
    {
        out.push((start, display_text.len()));
    }
    out
}

/// Merges overlapping/adjacent ranges and wraps each with color codes in a
/// single left-to-right pass, so terms that overlap (e.g. one term a
/// substring of another) never produce nested/malformed escape sequences.
fn apply_highlights(text: &str, mut ranges: Vec<(usize, usize)>) -> String {
    ranges.sort_unstable();
    let mut merged: Vec<(usize, usize)> = Vec::new();
    for (start, end) in ranges {
        match merged.last_mut() {
            Some((_, last_end)) if start <= *last_end => {
                if end > *last_end {
                    *last_end = end;
                }
            }
            _ => merged.push((start, end)),
        }
    }

    let mut out = String::with_capacity(text.len() + merged.len() * (HIGHLIGHT_START.len() + HIGHLIGHT_END.len()));
    let mut cursor = 0;
    for (start, end) in merged {
        out.push_str(&text[cursor..start]);
        out.push_str(HIGHLIGHT_START);
        out.push_str(&text[start..end]);
        out.push_str(HIGHLIGHT_END);
        cursor = end;
    }
    out.push_str(&text[cursor..]);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    fn ts(y: i32, mo: u32, d: u32) -> chrono::NaiveDateTime {
        NaiveDate::from_ymd_opt(y, mo, d)
            .unwrap()
            .and_hms_opt(0, 0, 0)
            .unwrap()
    }

    /// Builds a test entry with `tags` appended as a trailing line, the
    /// same shape `-t/--tags` produces (§2.1). Tags aren't a separate
    /// field anymore -- they're just `@word` tokens in the body.
    fn e(tags: &[&str], body: &str) -> Entry {
        let full_body = if tags.is_empty() {
            body.to_string()
        } else {
            format!("{body}\n{}", tags.join(" "))
        };
        Entry::new(ts(2026, 1, 1), full_body)
    }

    fn opts(all: bool, limit: Option<usize>) -> SearchOptions {
        SearchOptions { all, limit }
    }

    #[test]
    fn or_mode_matches_any_term() {
        let entries = vec![e(&[], "fm radio"), e(&[], "linux kernel"), e(&[], "neither")];
        let results = search(&entries, "radio kernel", &opts(false, None));
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn and_mode_requires_all_terms() {
        let entries = vec![
            e(&[], "fm radio broadcast"),
            e(&[], "fm radio"),
            e(&[], "radio broadcast"),
        ];
        let results = search(&entries, "fm broadcast", &opts(true, None));
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].body, "fm radio broadcast");
    }

    #[test]
    fn plus_joins_words_into_one_substring_term() {
        let entries = vec![e(&[], "reading about linux kernel internals"), e(&[], "unrelated")];
        let results = search(&entries, "linux+kernel", &opts(false, None));
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn substring_matching_is_broad_by_design() {
        let entries = vec![e(&[], "the weather this month")];
        let results = search(&entries, "th", &opts(false, None));
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn tag_term_requires_full_word_match_not_substring() {
        let entries = vec![e(&["@bp"], "reading"), e(&["@bph"], "other")];
        let results = search(&entries, "@bp", &opts(false, None));
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].tags(), vec!["@bp"]);
    }

    #[test]
    fn tag_term_match_is_case_insensitive() {
        let entries = vec![e(&["@BP"], "reading")];
        let results = search(&entries, "@bp", &opts(false, None));
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn plain_term_matches_inside_tag_text_via_substring() {
        // Non-@ terms are ordinary substrings over the whole rendered
        // entry (timestamp line + body), which includes the tag line.
        let entries = vec![e(&["@bp"], "reading")];
        let results = search(&entries, "bp", &opts(false, None));
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn limit_caps_number_of_results() {
        let entries = vec![e(&[], "match one"), e(&[], "match two"), e(&[], "match three")];
        let results = search(&entries, "match", &opts(false, Some(2)));
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn empty_query_matches_nothing() {
        let entries = vec![e(&[], "anything")];
        let results = search(&entries, "   ", &opts(false, None));
        assert!(results.is_empty());
        let results = search(&entries, "   ", &opts(true, None));
        assert!(results.is_empty());
    }

    #[test]
    fn search_lines_or_mode_returns_only_lines_matching_any_term() {
        let entries = vec![e(&[], "fm radio\nunrelated line\nlinux kernel")];
        let results = search_lines(&entries, "radio kernel", &opts(false, None));
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].1, vec!["fm radio", "linux kernel"]);
    }

    #[test]
    fn search_lines_and_mode_requires_both_terms_on_the_same_line() {
        // Whole-entry `search` would match this entry in --all mode (each
        // term appears somewhere), but no single line has both terms.
        let entries = vec![e(&[], "fm radio\nbroadcast only")];
        let results = search_lines(&entries, "fm broadcast", &opts(true, None));
        assert!(results.is_empty());
    }

    #[test]
    fn search_lines_and_mode_returns_the_line_with_both_terms() {
        let entries = vec![e(&[], "fm radio broadcast\nunrelated")];
        let results = search_lines(&entries, "fm broadcast", &opts(true, None));
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].1, vec!["fm radio broadcast"]);
    }

    #[test]
    fn search_lines_tag_term_requires_full_word_match_on_that_line() {
        let entries = vec![e(&[], "reading @bp today\nunrelated @bph line")];
        let results = search_lines(&entries, "@bp", &opts(false, None));
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].1, vec!["reading @bp today"]);
    }

    #[test]
    fn search_lines_excludes_entries_with_no_qualifying_line() {
        let entries = vec![e(&[], "nothing relevant here")];
        let results = search_lines(&entries, "absent", &opts(false, None));
        assert!(results.is_empty());
    }

    #[test]
    fn search_lines_limit_caps_number_of_entries_not_lines() {
        let entries = vec![
            e(&[], "match one\nmatch again"),
            e(&[], "match two"),
            e(&[], "match three"),
        ];
        let results = search_lines(&entries, "match", &opts(false, Some(2)));
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn highlight_wraps_substring_case_insensitively() {
        let out = highlight("### reading about Linux kernel internals", "linux");
        assert_eq!(
            out,
            "### reading about \x1b[1;31mLinux\x1b[0m kernel internals"
        );
    }

    #[test]
    fn highlight_wraps_every_occurrence() {
        let out = highlight("match one, match two", "match");
        assert_eq!(
            out,
            "\x1b[1;31mmatch\x1b[0m one, \x1b[1;31mmatch\x1b[0m two"
        );
    }

    #[test]
    fn highlight_tag_requires_full_word_match_not_substring() {
        let out = highlight("reading @bp today, unrelated @bph entry", "@bp");
        assert_eq!(
            out,
            "reading \x1b[1;31m@bp\x1b[0m today, unrelated @bph entry"
        );
    }

    #[test]
    fn highlight_tag_match_is_case_insensitive() {
        let out = highlight("reading @BP today", "@bp");
        assert_eq!(out, "reading \x1b[1;31m@BP\x1b[0m today");
    }

    #[test]
    fn highlight_merges_overlapping_ranges_from_multiple_terms() {
        // "kernel" and "kern" both match "kernel" -- must not nest/double-wrap.
        let out = highlight("linux kernel internals", "kernel kern");
        assert_eq!(
            out,
            "linux \x1b[1;31mkernel\x1b[0m internals"
        );
    }

    #[test]
    fn highlight_returns_input_unchanged_when_nothing_matches() {
        let text = "nothing relevant here";
        assert_eq!(highlight(text, "absent"), text);
    }

    #[test]
    fn highlight_returns_input_unchanged_for_empty_query() {
        let text = "some entry text";
        assert_eq!(highlight(text, "   "), text);
    }
}
