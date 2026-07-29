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
[YYYY-MM-DD.HH:MM:SS]
Entry body, line 1
Entry body, line 2 (optional)
@tag1 @tag2 (optional; present only if -t/--tags was used)

```

- **Line 1:** timestamp in `[YYYY-MM-DD.HH:MM:SS]` format, and nothing else. The
  timestamp is always alone on its own line.
- **Body:** any number of lines, including blank lines. `@tag` tokens may appear
  anywhere in the body -- there is no separate structured storage for tags.
- **Terminator:** every entry ends with exactly one blank line. If the entry text
  supplied by the user has trailing blank lines, they are collapsed to one; if it
  has none, one is appended.

### 2.1 Tags

**Resolved (re-architected):** tags are no longer a structured field hoisted
onto the timestamp line. A tag is simply any token matching `@\S+`, recognized
by shape wherever it appears in the body:

| Method | Example | Result |
|---|---|---|
| Typed inline, anywhere in the text | `journal "my @blood_pressure was 117/75/50"` | `@blood_pressure` stays exactly where typed |
| Explicit `-t/--tags` flag | `journal -t "beer @store" "grabbed a six-pack"` | `@beer @store` is appended as its own line at the end of the entry |

No hoisting, extraction, or de-duplication occurs. `-t/--tags` bare words
(without a leading `@`) are automatically prefixed with `@` before being
concatenated onto their own line; tokens already prefixed are left as-is.
Inline tags typed directly in the entry text are left completely untouched --
they're just body text that happens to match the tag shape.

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
- On match, the **entire entry** (timestamp line and full body) is printed.

**Resolved:** `@`-prefixed search terms require a **full-word match** against
a `@tag` token found anywhere in the entry's body (not a substring match).
This is a deliberate exception to the general substring-matching rule in §3: a
search for `@bp` matches the tag `@bp` but not `@bph`. Non-tag search terms are
unaffected and continue to use case-insensitive substring matching per §3 --
which means a bare word like `blood_pressure` also finds an inline
`@blood_pressure` tag, since it's a substring match against the whole entry.

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
(`[YYYY-MM-DD.HH:MM:SS]`) pre-inserted at the end, followed by a blank line
where the cursor is positioned for the user to type body text. If
`-t/--tags` is also given, its normalized tags line (§2.1) is pre-seeded
after that blank line, so the saved entry ends up in the same
tags-on-their-own-trailing-line shape the non-editor append path produces.

**Resolved:** the timestamp (and any pre-seeded tags line) is inserted only
into the editor's in-memory buffer, not written to the on-disk journal file
beforehand. This means:
- If the user saves and exits normally, the timestamp (plus whatever body
  text and tags they typed) is persisted as part of that save, and the usual
  trailing-blank-line normalization pass runs afterward.
- If the user aborts the editor without saving (e.g. `:q!` in vim), the on-disk
  journal file is never touched, and no orphaned or empty timestamp entry is
  left behind.

This requires the editor to be launched against a temporary buffer/file seeded
with the existing journal contents plus the new timestamp line, rather than
writing the timestamp directly into the real journal file and hoping the editor
either commits or the tool reverts it. On save, the temp buffer's contents
replace the real journal file (e.g. via a rename over the original for
atomicity); on abort, the temp file is discarded.

**Resolved: cursor positioning.** There's no single command-line flag that
positions an editor's cursor on a given line across every editor (vi/vim/nano
use `+N`; GUI editors vary), so journal doesn't hardcode one. Instead, an
optional config file at `$XDG_CONFIG_HOME/journal/config.toml` (falling back
to `~/.config/journal/config.toml`, same default resolution as §4) supports:

```toml
[editor]
args = "+{line}"
```

`args` is shell-word split (same quoting rules as `$EDITOR`), `{line}` is
replaced with the 1-indexed line number of the seeded blank line, and the
result is inserted immediately before the file path argument -- ahead of any
of `$EDITOR`'s own arguments, so it takes effect before anything else
`$EDITOR` might do (e.g. its own `-c` commands, if any). With no config file,
no extra arguments are added and the cursor lands wherever the editor
defaults to.

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
**Resolved (partial):** `$XDG_CONFIG_HOME/journal/config.toml` (falling back
to `~/.config/journal/config.toml`) is now supported, currently for one
setting: `editor.args`, the cursor-positioning template described in §5.1.
Further preferences (default file path, default search mode, color
preference) are still open for future work -- the `[editor]` table structure
leaves room to add sibling tables (e.g. `[search]`, `[color]`) later without
a breaking format change.

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
- `@`-prefixed tag search terms: **full-word match only** (no substring
  matching) against any `@tag` token found anywhere in the entry's body.
- Tags are **not** a structured field: the timestamp line is always alone,
  and `@tag` tokens are just body text recognized by shape. `-t/--tags`
  appends its tags as their own line (auto-prefixing bare words with `@`);
  inline tags stay exactly where they're typed. No hoisting or dedup.
- Default file location (no `-f`, no `$JOURNAL_FILE`): **XDG data directory**
  (`$XDG_DATA_HOME/journal/journal.txt`, falling back to
  `~/.local/share/journal/journal.txt`).
- Running `journal` with **no arguments** opens the file in `$EDITOR`
  (fallback `vi`) via a temp buffer seeded with a new timestamp line and a
  blank line after it (plus a pre-seeded `-t/--tags` line, if given); the
  timestamp is only persisted to the real journal file on save, never written
  ahead of time.
- Cursor positioning on that blank line is **opt-in via config**
  (`$XDG_CONFIG_HOME/journal/config.toml`'s `editor.args`, with a `{line}`
  placeholder), since no single flag positions the cursor across every
  editor. No config means no extra arguments and no positioning.
