# `journal` — CLI Journaling Tool Specification

**Language:** Rust

This document describes `journal`'s current behavior in full and is kept in
sync with the implementation — it is a reference, not a design log. For
installation and quick usage examples, see `README.md`.

## 1. Overview

`journal` is a command-line utility for appending timestamped, taggable
entries to a plain-text journal file, and for searching that file by tag or
keyword. Entries are stored in a flat, human-readable format so the
underlying file remains inspectable and editable without the tool. There are
no subcommands (no `add`/`list`/`show` verbs) — behavior is selected entirely
by which flags are present on a single top-level command.

## 2. On-Disk Entry Format

```
[YYYY-MM-DD.HH:MM:SS]
Entry body, line 1
Entry body, line 2 (optional)
@tag1 @tag2 (optional; present only if -t/--tags was used)

```

- **Header line:** a timestamp in `[YYYY-MM-DD.HH:MM:SS]` format, and nothing
  else. The timestamp is always alone on its own line.
- **Body:** any number of lines, including blank lines. `@tag` tokens may
  appear anywhere in the body — there is no separate structured storage for
  tags.
- **Terminator:** every entry ends with exactly one blank line. If the entry
  text supplied by the user has trailing blank lines, they are collapsed to
  one; if it has none, one is appended.
- **Multiple entries** in a file are simply concatenated; entry boundaries
  are located by scanning for header lines, not blank lines, since a body may
  legitimately contain blank lines of its own.

### 2.1 Tags

A tag is any token matching `@\S+`, recognized by shape wherever it appears
in the body — tags are not a structured field hoisted onto the header line.

| Method | Example | Result |
|---|---|---|
| Typed inline, anywhere in the text | `journal "my @blood_pressure was 117/75/50"` | `@blood_pressure` stays exactly where typed |
| Explicit `-t/--tags` flag | `journal -t "beer @store" "grabbed a six-pack"` | `@beer @store` is appended as its own line at the end of the entry |

No hoisting, extraction, or de-duplication occurs. `-t/--tags` bare words
(without a leading `@`) are automatically prefixed with `@` before being
concatenated onto their own line; tokens already prefixed are left as-is.
Inline tags typed directly in the entry text are left completely untouched —
they're just body text that happens to match the tag shape.

## 3. Modes of Operation

### 3.1 Append an entry

```sh
journal "124/80/55 @bp @health"
journal -t "sleep" "slept 7 hours"       # appends "@sleep" as its own line
echo "back from a walk @exercise" | journal -
```

Positional `TEXT` is appended as a new entry with the current timestamp.
`-t/--tags` (if given) is normalized per §2.1 and concatenated onto its own
trailing line. A literal `-` as the text argument reads the entry body from
stdin instead (the standard Unix filter-tool convention). `-t/--tags` and
positional `TEXT` both conflict with `-s/--search` at the argument-parsing
level (exit code 2 if combined).

### 3.2 Compose via `$EDITOR` (no arguments)

Running `journal` with no positional text opens the resolved journal file in
`$EDITOR` (falling back to `vi`) via a temporary buffer — see §6 for the full
mechanics. `-t/--tags` works here too, pre-seeding a tags line into the
buffer.

### 3.3 Show the last N entries (`-N`)

```sh
journal -3
```

Prints the last `N` entries in chronological order (oldest → newest, like
`tail`), using the human-facing display format (§4). Fewer than `N` entries
in the journal is not an error — everything available is printed. An empty
journal prints nothing and exits `0`.

`-N` is a short flag whose name is a variable-length run of digits, which
`clap`'s derive API cannot express as a declared flag; it's extracted from
`argv` by hand before the rest of the arguments reach `clap`. It cannot be
combined with `-s/--search`, positional `TEXT`, or `-t/--tags`, and `-0` is
rejected (all three: exit code 2).

### 3.4 Search (`-s/--search`)

```sh
journal -s "@bp"                # entries tagged @bp (exact tag match)
journal -s "fm radio"           # entries containing "fm" OR "radio"
journal -s "fm radio" -a        # entries containing "fm" AND "radio"
journal -s "linux+kernel"       # entries containing the literal phrase "linux kernel"
journal -s "th" --limit 5       # cap the number of results
journal -s "fm radio" -L        # print only the matching lines, not full entries
```

`-a/--all`, `--limit`, and `-L/--lines-only` are usage errors (exit code 2)
unless `-s/--search` is also given. `-v/--verbose` (§12) has no such
restriction — it's orthogonal to every mode in this section.

#### 3.4.1 Term parsing

The query is split on whitespace into terms. Within a term, `+` is replaced
with a space, joining words into one multi-word substring term (e.g.
`linux+kernel` matches the literal substring `linux kernel`). A term starting
with `@` (and longer than just `@`) is a **tag term**; anything else is a
**substring term**. All terms are matched case-insensitively.

- **Substring term:** matches if it appears anywhere in the match scope (see
  §3.4.2 / §3.4.3 for what that scope is) as a plain substring. This is
  broad by design — a search for `"th"` matches inside `"weather"` or
  `"month"` too, so short or common terms can return many results.
- **Tag term:** requires a **full-word match** against a `@tag` token found
  in the match scope — a search for `@bp` matches the tag `@bp` but not
  `@bph`. A bare word without `@` still finds a tag via ordinary substring
  matching (`blood_pressure` matches `@blood_pressure` as a substring of the
  whole entry).

#### 3.4.2 Whole-entry mode (default)

An entry matches if its terms are satisfied *anywhere* in the entry — the
on-disk header line plus the full body, treated as one string. Default is OR
(any term matches); `-a/--all` requires every term to match somewhere in the
entry, not necessarily on the same line. On match, the entry is printed in
full using the display format (§4). `--limit N` caps the number of matching
entries printed (not the number of matched lines/terms). Multiple matching
entries are separated by a blank line.

#### 3.4.3 Lines-only mode (`-L`/`--lines-only`)

Instead of the full entry body, prints the entry's header (§4) once, followed
by only the body lines that themselves satisfy the term condition:

- Default (OR): a line qualifies if it contains *any* term.
- `-a/--all` combined with `-L`: a line qualifies only if it contains *every*
  term **on that same line** — stricter than whole-entry `-a`, where terms
  may be spread across different lines of the entry.

An entry is included in the output only if it has at least one qualifying
line; `--limit N` still caps the number of entries (not lines) printed.

#### 3.4.4 Highlighting

Matched terms are wrapped in ANSI bold-red (`grep`'s default match style)
wherever they appear in the printed output, in both whole-entry and
lines-only mode. Highlighting is applied only when stdout is an interactive
terminal and the `NO_COLOR` environment variable is unset — piped output (a
file, `less`, a Markdown renderer like `bat`) is always plain text, so raw
ANSI escapes never collide with a downstream renderer's own coloring.

#### 3.4.5 Exit status

A search that finds no matches (in either mode) exits `1` with no output, the
same convention as `grep`. A search that finds at least one match exits `0`.

## 4. Human-Facing Display Format

Search results (§3.4) and `-N` output (§3.3) use a different rendering than
the on-disk format (§2): the header is reformatted as a Markdown ATX heading
with a relative-age annotation, rather than the on-disk `[...]` bracket form:

```
### 2026-07-28 14:03:00 (12 days ago)
124/80/55 @bp @health

```

An ATX heading (`###`), rather than a `>` blockquote, has no Markdown lazy
continuation, so a renderer like `bat` colors only the header line and not
the body line that follows it. The relative-age phrase is computed against
the current time:

| Elapsed | Phrase |
|---|---|
| < 1 minute | `just now` |
| < 1 hour | `N minute(s) ago` |
| < 1 day | `N hour(s) ago` |
| < 90 days | `N day(s) ago` (raw day count, no week rounding) |
| < 1 year | `N month(s) ago` |
| ≥ 1 year | `N year(s) ago` |

A postdated (future) timestamp uses `in N unit(s)` instead of `N unit(s)
ago`, collapsing to `just now` within one minute either direction.

## 5. File Location Resolution

Resolved in this precedence order:

1. `-f/--file <path>` command-line argument
2. `$JOURNAL_FILE` environment variable (an empty/whitespace-only value is
   treated as unset)
3. XDG default: `$XDG_DATA_HOME/journal/journal.txt`, falling back to
   `~/.local/share/journal/journal.txt` if `$XDG_DATA_HOME` is unset, per the
   XDG Base Directory spec. Parent directories are created automatically for
   this default; an explicit `-f`/`$JOURNAL_FILE` path is expected to already
   have an existing parent directory (`touch`-like semantics).

## 6. Editor Integration

`$EDITOR` is used for the no-argument compose flow (§3.2); if unset, falls
back to `vi`. `$EDITOR`'s value is shell-word split (so quoted paths with
spaces work), giving a program name and its own leading arguments.

The timestamp for the new entry (and any pre-seeded `-t/--tags` line) is
inserted only into a temporary buffer, seeded with the journal's existing
contents plus the new header line and a blank line positioned right after it
for the cursor — never written ahead of time to the real journal file. The
editor is launched against that temp file. Whether the user actually saved
is detected by comparing the temp file's modification time before and after
the editor process exits:

- **No change in mtime** (e.g. `:q` or `:q!` in vim with nothing written):
  the real journal file is left completely untouched, and nothing is
  persisted.
- **mtime changed:** the edited buffer is read back, the newly-composed
  entry (found by locating the last header line — everything before it is
  left byte-for-byte as the user last had it) is re-normalized the same way
  the non-editor append path normalizes trailing blank lines, and the result
  atomically replaces the real journal file (temp file persisted over the
  original path).

A non-zero editor exit status is treated as a failure (nothing is persisted,
the process exits with an error).

**Interruption** (`Ctrl-C`/`SIGINT`, or `SIGTERM`) during this flow is
handled explicitly. The temp buffer would otherwise leak: its normal cleanup
relies on being dropped when it goes out of scope, but Rust's default signal
disposition terminates the process immediately without running destructors.
A signal handler, registered right after the temp file is created, removes
it and exits with status `130` (128 + `SIGINT`, the standard convention).
This is safe even if the signal arrives just after a successful save — by
then the temp path has already been renamed away to become the real journal
file, so it no longer names anything on disk and the removal is a harmless
no-op. No other mode creates a temporary file, so no other code path
installs a handler.

**Cursor positioning** is opt-in via the config file (§7), since no single
command-line flag positions an editor's cursor across every editor
(vi/vim/nano use `+N`; GUI editors vary). With no config, no extra arguments
are passed and the cursor lands wherever the editor defaults to.

## 7. Config File

An optional TOML file at `$XDG_CONFIG_HOME/journal/config.toml` (falling
back to `~/.config/journal/config.toml`, same resolution as §5) configures
editor cursor positioning:

```toml
[editor]
args = "+{line}"
```

`args` is shell-word split (same quoting rules as `$EDITOR` itself) and
`{line}` is replaced with the 1-indexed line number of the seeded blank line.
The resulting arguments are inserted immediately before the file path
argument — ahead of any of `$EDITOR`'s own arguments, so cursor positioning
takes effect before anything else `$EDITOR` might do (e.g. its own `-c`
commands). A missing config file is not an error; it just means no extra
arguments are added. The `[editor]` table leaves room for future sibling
tables (e.g. `[search]`, `[color]`) without a breaking format change.

## 8. Environment Variables

| Variable | Purpose |
|---|---|
| `EDITOR` | Editor launched by no-argument invocation (§3.2, §6); falls back to `vi` |
| `JOURNAL_FILE` | Journal file path override (§5) |
| `XDG_DATA_HOME` | Base directory for the default journal file location (§5) |
| `XDG_CONFIG_HOME` | Base directory for the config file (§7) |
| `NO_COLOR` | Disables search-term highlighting when set (§3.4.4) |

## 9. Exit Codes

| Code | Meaning |
|---|---|
| 0 | Success — entry appended/saved, `-N` printed (or the journal was empty), or a search found at least one match |
| 1 | Runtime error (I/O failure, editor exited non-zero, etc.), or a search found no matches |
| 2 | Usage error — an invalid flag combination, caught either by `clap` itself (e.g. `-s` combined with `-t`) or by explicit validation (e.g. `-a` without `-s`, `-N` combined with `-s`) |
| 130 | The no-argument editor flow was interrupted (`SIGINT`/`SIGTERM`) while composing an entry; the temporary edit buffer is removed before exiting (§6) |

## 10. Concurrency / Write Safety

Since the journal file may be touched by multiple invocations concurrently
(e.g. a cron job and an interactive session), writes are serialized with an
exclusive lock (`flock`) taken on a stable sidecar file, `<path>.lock` —
*not* on the journal file itself. This matters because editor-mode saves
(§6) replace the journal file via an atomic rename: a lock held on the file
being renamed away is tied to the old inode and stops protecting anything
once the rename happens, silently reopening a lost-update race. Locking a
sidecar path that's never renamed avoids that hazard. Both the non-editor
append path and the editor save path take this same lock, so they serialize
against each other correctly. The write itself is also opened in append
(`O_APPEND`) mode as defense in depth.

## 11. Output Stream Discipline

Journal entries and search results are written to `stdout`; errors, warnings,
and diagnostics go to `stderr`. This allows `journal -s foo > results.txt` to
work predictably, and keeps `journal`'s output composable in shell
pipelines.

## 12. Diagnostics (`-v/--verbose`)

`-v/--verbose` prints diagnostic lines to stderr — never stdout, per §11 —
and can be combined with any mode. Without it, a successful run is
completely silent on stderr, the usual Unix convention. Diagnostics
currently emitted:

| When | Diagnostic |
|---|---|
| After resolving the journal file (§5) | `using journal file <path>` |
| Around the sidecar lock (§10) | `acquiring lock at <path>.lock`, then `lock released` |
| After appending an entry | `appended entry at <timestamp>` |
| Editor mode: temp buffer created | `editing via temp file <path>` |
| Editor mode: launching `$EDITOR` | `launching editor: <program> <args>` |
| Editor mode: outcome | `editor exited without saving changes`, or `entry saved` |
| `-N` | `<total> entries in journal, printing last <n>` |
| `-s/--search` (either mode) | `<n> entries matched` |

## 13. Man Page / `--help`

`-h/--help` and `-V/--version` are provided by `clap`. A man page is checked
in at `man/journal.1`, generated from the same flag definitions via
`cargo run --example man` (not regenerated automatically at build time — see
`CLAUDE.md` for the regeneration workflow).

## 14. Possible Future Work

Not currently implemented; listed here as known gaps rather than mixed into
the normative sections above:

1. An editor mode targeted at an existing entry, for corrections — currently
   the no-argument editor flow only ever appends a new entry.
2. A way to list all tags in use, or list/filter entries by date range.
