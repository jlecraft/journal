# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What this is

`journal` is a Rust CLI for appending timestamped, taggable entries to a flat
plain-text file and searching it. No database — the storage file is meant to
stay human-readable and hand-editable. The full design rationale (why each
behavior is what it is) lives in `journal-cli-spec.md`; treat it as the
source of truth when a design question comes up that isn't answered by the
code itself. `README.md` has the user-facing usage docs.

## Commands

```sh
cargo build                    # debug build
cargo build --release          # release build -> target/release/journal
cargo test                     # unit tests (src/**) + integration tests (tests/*.rs)
cargo test --lib               # unit tests only
cargo test --test search       # one integration test file (append|editor|last|search|stdin)
cargo test some_test_name       # run tests matching a name, across lib + integration
cargo clippy --all-targets     # lint, including test/example targets
cargo run --example man        # regenerate man/journal.1 from the Cli definition
```

`man/journal.1` is checked in, not generated at build time — regenerate it
with the command above whenever `src/cli.rs`'s flags or doc comments change,
and commit the result alongside the code change.

## Architecture

**No subcommands — one flag-driven command.** `src/cli.rs` defines a single
`Cli` struct (clap derive); behavior is selected by which flags are present,
dispatched by a flat if/match chain in `run()` (`src/main.rs`): search, then
show-last-N, then append-from-arg vs. open-`$EDITOR`. There's no `add`/`list`
verb anywhere.

One flag needs special handling clap can't express: `-N` (e.g. `journal -3`,
"show the last N entries") is a short flag with a variable-length numeric
name, which clap derive can't declare. It's hand-extracted from `argv`
*before* clap parses (`Cli::parse_from_argv` → `extract_last_n_flag` in
`cli.rs`), then merged back into the parsed `Cli`. `Cli::validate()` (not
clap attributes) enforces flag combinations that don't compose cleanly with
this pre-parse step or with each other (e.g. `-a`/`--limit`/`-L` all require
`-s`).

**Module map** (`src/lib.rs` re-exports all of these):
- `entry.rs` — the `Entry` type and all timestamp/format logic. Two
  serializations of the same entry: `render()` (on-disk, `[YYYY-MM-DD HH:MM:SS]`
  header) and `display()`/`display_header()` (human-facing, `### date (age)`
  ATX heading — chosen over a blockquote so Markdown renderers like `bat`
  color only the header line, not the body). `Entry::parse_all` splits a
  whole file into entries by locating header lines, *not* blank lines,
  because a body may legitimately contain blank lines of its own. A header's
  timestamp is a `Timestamp` struct, not a bare `NaiveDateTime` (see
  `timestamps.md` for the grammar this implements): a `DateSpec` (date part
  — `YYYY`/`MM-DD`/`YYYY-MM-DD`, or none at all) plus hour/minute/second,
  each given or omitted independently. Missing seconds default to `00`;
  the `YYYY` shape defaults month/day to `01-01`. Two of `DateSpec`'s four
  variants substitute a *dynamic* value rather than a fixed default,
  resolved fresh on every `resolved()` call rather than fixed at parse
  time: `MonthDayOnly` (the `MM-DD` shape) substitutes the current year,
  and `Today` (no date part at all — takes priority over defaulting
  month/day to `01-01`) substitutes today's actual date. Either way the
  entry always resolves to a real point in time, so it always gets a
  `-h/--human` elapsed-time annotation, even though the substituted part
  is only a stand-in never actually written down. This flexible parsing
  only ever affects *display* —
  `display_header` always shows `timestamp.resolved()` fully expanded,
  whether or not `-h` is given — never the file: `Entry::render` writes
  back the exact bracket-interior text an entry was parsed from
  (`raw_header`) rather than reformatting it from `timestamp`, so a
  hand-typed shorthand header stays shorthand on disk.
- `search.rs` — term parsing (`@tag` = exact full-word match against
  `entry.tags()`; anything else = case-insensitive substring against the
  whole rendered entry) and two search modes: `search()` (whole-entry, for
  the default `-s` view) and `search_lines()` (per-line, for `-L`/
  `--lines-only` — a line must satisfy the term condition itself, so `-a`
  combined with `-L` is *stricter* than plain `-a`: both terms must be on
  the same line, not just somewhere in the entry). Both sort matches by
  resolved timestamp via the shared `sort_by_date` helper (oldest first,
  or newest first with `-r/--reverse-sort`) before `SearchOptions.limit`
  truncates the list, so `--limit` always caps from whichever end sorting
  landed on. Also owns `highlight()`, which wraps matched text in ANSI
  bold-red.
- `storage.rs` — journal file path resolution (`-f` > `$JOURNAL_FILE` > XDG
  data dir) and all file I/O. Writes go through `with_exclusive_lock`, which
  locks a stable `<path>.lock` sidecar file rather than the journal file
  itself. `append_entry` and editor mode (`editor::open_in_editor`, held for
  the whole editing session) both go through this same lock so they
  serialize against each other — a concurrent append can't land while an
  interactive editing session has the file open.
  `with_exclusive_lock`/`append_entry` both take a `verbose: bool` for
  `-v/--verbose`'s lock-acquire/release diagnostics.
- `editor.rs` — the no-argument "open `$EDITOR`" flow. Deliberately thin:
  launches `$EDITOR` directly on the real journal file (creating it empty
  first via `storage::ensure_exists` if needed) under the same
  `storage::with_exclusive_lock` `append_entry` uses, held for the whole
  editing session so a concurrent append can't interleave with it. No
  seeding, no temp file, no mtime diffing, no atomic replace — whatever the
  editor writes (or doesn't) by the time it exits is exactly what's on
  disk, so "quit without saving" is just the file being untouched rather
  than something `journal` detects. A non-zero editor exit status is
  reported as an error, but says nothing about whether the file was
  already modified before that happened.
- `config.rs` — optional TOML config at `$XDG_CONFIG_HOME/journal/config.toml`.
  `[color].enabled` and `[timestamp].format`/`[timestamp].diff` (§4/§7 of
  the spec); a missing file or missing keys fall back to documented
  defaults table by table, key by key.
- `main.rs` — argv → `Cli` → `run()` dispatch, plus the two output-producing
  paths (`run_last`, `run_search`) that turn `Entry`s into printed text.

**Conventions baked into the code, not just docs:**
- Exit codes: `0` success (including "search ran, found nothing to say" for
  `run_last` on an empty journal), `1` runtime error *or* a search found no
  matches (grep-style), `2` usage error (invalid flag combination, caught by
  `Cli::validate()`).
- Color/TTY: search-term highlighting is gated on `stdout().is_terminal()
  && NO_COLOR unset`, so piped output (a file, `less`, `bat`) is always
  plain text and never collides with a downstream renderer's own coloring.
- Tags are not a structured field anywhere — `Entry::tags()` recovers
  `@word` tokens from the body on demand by shape, wherever they happen to
  appear (typed inline, or appended as a line by `-t/--tags`).
- Diagnostics: `-v/--verbose` is threaded as a plain `bool` parameter through
  the `main.rs`/`storage.rs`/`editor.rs` call chains (same pattern as
  `colorize` in `run_search`) and printed via `vlog()` (`lib.rs`), a tiny
  stderr-only helper — no logging framework, consistent with the rest of
  this codebase's preference for hand-rolled solutions over new dependencies
  where one isn't structurally required.

## Testing conventions

Unit tests live inline per module (`#[cfg(test)] mod tests` at the bottom of
each `src/*.rs` file) and cover parsing/formatting/matching logic in
isolation. Integration tests in `tests/*.rs` use `assert_cmd` to invoke the
built binary against a `tempfile::TempDir` journal file, one file per
behavior area (`append.rs`, `editor.rs`, `last.rs`, `search.rs`, `stdin.rs`).
`tests/*.rs` files that construct fixture journal files write the on-disk
`[YYYY-MM-DD HH:MM:SS]` format directly rather than going through the CLI, so
each test controls exact entry content/order independent of the append path.
