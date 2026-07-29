# `journal` — CLI Journaling Tool Specification

**Language:** Rust
**Status:** Draft v0.1 — open questions marked below

## 1. Overview

`journal` is a command-line utility for appending timestamped, taggable entries to
a plain-text journal file, and for searching that file by tag or keyword. Entries
are stored in a flat, human-readable format so the underlying file remains
inspectable and editable without the tool.

## 2. Entry Format

```
[YYYY-MM-DD.HH:MM:SS] @tag1 @tag2
Entry body, line 1
Entry body, line 2 (optional)

```

- **Line 1:** timestamp in `[YYYY-MM-DD.HH:MM:SS]` format, optionally followed by
  zero or more space-delimited `@tag` tokens.
- **Body:** any number of lines, including blank lines.
- **Terminator:** every entry ends with exactly one blank line. If the entry text
  supplied by the user has trailing blank lines, they are collapsed to one; if it
  has none, one is appended.

### 2.1 Tag Extraction Rules

A tag is any token matching `@\S+` preceded by whitespace. Tags may be supplied
two ways, and both are equivalent:

| Method | Example |
|---|---|
| Trailing tags in the body text | `journal "124/80/55 @bp @health"` |
| Explicit `-t/--tags` flag | `journal -t "@bp @health" "124/80/55"` |

When tags are embedded in the body, they must appear as a contiguous run at the
**end** of the entry text; they are stripped from the body and hoisted onto the
timestamp line. When `-t` is used, its value is parsed for `@tag` tokens and
appended to the timestamp line instead, and the body text is left untouched.

**Resolved:** if both `-t` and inline trailing tags are present in the same
invocation, they are combined and de-duplicated (case-sensitive exact match on
the tag token) before being written to the timestamp line.

## 3. Search (`-s/--search`)

```
journal -s "@bp"
journal -s "fm radio"
journal -s "linux+kernel"
```

- The search string is tokenized on whitespace into terms.
- A `+` inside a token joins words into a single multi-word term, with `+`
  replaced by a space for matching purposes (e.g. `linux+kernel` → matches the
  literal substring `linux kernel`).
- **Default (OR) mode:** an entry matches if *any* term is found anywhere in the
  entry (timestamp line + body).
- **`-a/--all` (AND) mode:** an entry matches only if *all* terms are found.
- **Matching for non-tag terms:** case-insensitive, substring-based. A search
  for `"th"` will match `"th"` anywhere in the entry — including inside longer
  words like `"weather"` or `"month"` — so short or common terms can return a
  large number of results. This is expected behavior, not a bug.
- On match, the **entire entry** (timestamp line, tags, and full body) is printed.

**Resolved:** `@`-prefixed search terms are matched only against the tag line,
and require a **full-word match** against individual tags (not a substring
match). This is a deliberate exception to the general substring-matching rule
in §3: a search for `@bp` matches the tag `@bp` but not `@bph`. Non-tag search
terms are unaffected and continue to use case-insensitive substring matching
per §3.

**Open questions:**
1. How multiple matching entries be separated by a blank line or a visual
   delimiter when printed, so entry boundaries are unambiguous in output?
2. Given substring matching is broad by design for non-tag terms, should there
   be a result count / pagination / `--limit` mechanism to keep output
   manageable for short, common search terms?

## 4. File Location

Resolved in this precedence order:

1. `-f/--file <path>` command-line argument
2. `$JOURNAL_FILE` environment variable
3. XDG default: `$XDG_DATA_HOME/journal/journal.txt`, falling back to
   `~/.local/share/journal/journal.txt` if `$XDG_DATA_HOME` is unset (per the
   XDG Base Directory spec). The tool should create the directory and file if
   they don't yet exist.

## 5. Editor Integration

`$EDITOR` is used when an editing action is invoked; if unset, falls back to
`vi`.

### 5.1 No-argument invocation

Running `journal` with no arguments opens the resolved journal file directly in
`$EDITOR` (or `vi`), with the timestamp line for a new entry
(`[YYYY-MM-DD.HH:MM:SS]`) pre-inserted at the end, cursor positioned for the
user to type tags and/or body text.

**Resolved:** the timestamp is inserted only into the editor's in-memory
buffer, not written to the on-disk journal file beforehand. This means:
- If the user saves and exits normally, the timestamp (plus whatever tags/body
  they typed) is persisted as part of that save, and the usual trailing-blank-line
  normalization pass runs afterward.
- If the user aborts the editor without saving (e.g. `:q!` in vim), the on-disk
  journal file is never touched, and no orphaned or empty timestamp entry is
  left behind.

This requires the editor to be launched against a temporary buffer/file seeded
with the existing journal contents plus the new timestamp line, rather than
writing the timestamp directly into the real journal file and hoping the editor
either commits or the tool reverts it. On save, the temp buffer's contents
replace the real journal file (e.g. via a rename over the original for
atomicity); on abort, the temp file is discarded.

**Open question:** should there be a way to open the editor targeted at a
*specific* existing entry (for corrections), as opposed to only appending a new
one? Not in the original spec, but a natural companion feature.

## 6. Recommendations for Linux CLI Convention Compliance

The following are gaps or deviations from common Linux/POSIX CLI conventions,
worth addressing before this is considered idiomatic:

### 6.1 XDG Base Directory fallback
Resolved — see §4. Default path is `$XDG_DATA_HOME/journal/journal.txt`,
falling back to `~/.local/share/journal/journal.txt`, per the XDG Base
Directory spec.

### 6.2 Argument parsing library
Use [`clap`](https://docs.rs/clap) (derive API) rather than hand-rolled parsing.
It gives you, for free:
- POSIX/GNU-style short and long flags (`-s`/`--search`)
- `-h/--help` and `-V/--version` (see 6.3)
- Combined short flags, `--flag=value` syntax, `--` end-of-options marker
- Auto-generated usage/help text and shell completion scripts

### 6.3 Standard flags currently missing
- `-h, --help` — usage summary (expected by every Linux CLI convention)
- `-V, --version` — print version and exit
- Consider `-q/--quiet` and `-v/--verbose` for controlling output noise, useful
  in scripts

### 6.4 Exit codes
Define and document exit codes per convention (0 = success, 1 = general error,
2 = usage error, etc.), so `journal` composes cleanly in shell pipelines and
scripts.

### 6.5 stdin support
Allow entry text to be piped in (`echo "..." | journal -`), which is standard
for Unix filter-style tools and useful for scripting entries from other
programs.

### 6.6 Output stream discipline
- Journal entries / search results → `stdout`
- Errors, warnings, diagnostics → `stderr`
This allows `journal -s foo > results.txt` to work predictably.

### 6.7 `NO_COLOR` / TTY detection
If any colorized output is planned (e.g., highlighting matched search terms),
respect the `NO_COLOR` environment variable and disable color automatically
when stdout is not a TTY (e.g., when piped).

### 6.8 Concurrency / write safety
Since this is an append-only log file potentially touched by multiple
invocations (e.g., a cron job and an interactive user), consider:
- File locking (`flock`) around writes, or
- Atomic append via `O_APPEND` opened file descriptor
to avoid interleaved writes corrupting entries.

### 6.9 Man page / `--help` completeness
For a "proper" Linux utility, ship a `man` page (or generate one via `clap_mangen`)
documenting the entry format, flags, and environment variables in one place.

### 6.10 Config file (optional but idiomatic)
Consider supporting `$XDG_CONFIG_HOME/journal/config.toml` for persistent
preferences (default file path, default search mode, color preference) so
common flags don't need to be repeated on every invocation.

## 7. Consolidated Open Questions

1. How multiple matching entries should be visually delimited in output.
2. Whether search needs a result-count / pagination / `--limit` mechanism for
   non-tag terms, given short substrings can match very broadly.
3. Whether there should be an editor mode targeted at an existing entry (for
   corrections), not just appending a new one.
4. Should there be a way to list all tags in use, or list entries by date range?
   (Not in the original spec, but common companion features for this kind of
   tool.)

## 8. Resolved Decisions Summary

- Non-tag search terms: **case-insensitive, substring-matched**.
- `@`-prefixed tag search terms: matched only against the tag line, **full-word
  match only** (no substring matching).
- `-t` tags and inline trailing tags are **combined and de-duplicated**.
- Default file location (no `-f`, no `$JOURNAL_FILE`): **XDG data directory**
  (`$XDG_DATA_HOME/journal/journal.txt`, falling back to
  `~/.local/share/journal/journal.txt`).
- Running `journal` with **no arguments** opens the file in `$EDITOR`
  (fallback `vi`) via a temp buffer seeded with a new timestamp line; the
  timestamp is only persisted to the real journal file on save, never written
  ahead of time.
