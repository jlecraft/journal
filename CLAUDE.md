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
  serializations of the same entry: `render()` (on-disk, `[YYYY-MM-DD.HH:MM:SS]`
  header) and `display()`/`display_header()` (human-facing, `### date (age)`
  ATX heading — chosen over a blockquote so Markdown renderers like `bat`
  color only the header line, not the body). `Entry::parse_all` splits a
  whole file into entries by locating header lines, *not* blank lines,
  because a body may legitimately contain blank lines of its own.
- `search.rs` — term parsing (`@tag` = exact full-word match against
  `entry.tags()`; anything else = case-insensitive substring against the
  whole rendered entry) and two search modes: `search()` (whole-entry, for
  the default `-s` view) and `search_lines()` (per-line, for `-L`/
  `--lines-only` — a line must satisfy the term condition itself, so `-a`
  combined with `-L` is *stricter* than plain `-a`: both terms must be on
  the same line, not just somewhere in the entry). Also owns `highlight()`,
  which wraps matched text in ANSI bold-red.
- `storage.rs` — journal file path resolution (`-f` > `$JOURNAL_FILE` > XDG
  data dir) and all file I/O. Writes go through `with_exclusive_lock`, which
  locks a stable `<path>.lock` sidecar file rather than the journal file
  itself — deliberate, because editor mode replaces the journal file via
  `rename`, and a lock held on a file being renamed away is tied to the old
  inode and stops protecting anything. `append_entry` and editor mode's save
  both go through this same lock so they serialize against each other.
- `editor.rs` — the no-argument "open `$EDITOR`" flow. Seeds a temp file
  (existing content + a fresh timestamp header + a blank line for the
  cursor, optionally a pre-seeded tags line), diffs mtime before/after the
  editor exits to detect "quit without saving" vs. a real save, then
  atomically replaces the journal file with `tempfile::persist`. Only the
  newly-composed entry (found via `Entry::last_entry_start`) gets
  re-normalized on save — everything before it is left byte-for-byte as the
  user last had it.
- `config.rs` — optional TOML config at `$XDG_CONFIG_HOME/journal/config.toml`.
  Currently one setting, `editor.args`, a shell-word-split template with a
  `{line}` placeholder for cursor positioning (editors vary — vi/vim/nano use
  `+N`, GUI editors don't — so this is opt-in, not auto-detected).
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

## Testing conventions

Unit tests live inline per module (`#[cfg(test)] mod tests` at the bottom of
each `src/*.rs` file) and cover parsing/formatting/matching logic in
isolation. Integration tests in `tests/*.rs` use `assert_cmd` to invoke the
built binary against a `tempfile::TempDir` journal file, one file per
behavior area (`append.rs`, `editor.rs`, `last.rs`, `search.rs`, `stdin.rs`).
`tests/*.rs` files that construct fixture journal files write the on-disk
`[YYYY-MM-DD.HH:MM:SS]` format directly rather than going through the CLI, so
each test controls exact entry content/order independent of the append path.
