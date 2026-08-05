pub mod cli;
pub mod config;
pub mod editor;
pub mod entry;
pub mod search;
pub mod storage;

/// Prints a `-v/--verbose` diagnostic line to stderr; a no-op when
/// `verbose` is false. Diagnostics, not journal output, so stderr per the
/// output-stream discipline in journal-cli-spec.md §11.
pub fn vlog(verbose: bool, msg: impl std::fmt::Display) {
    if verbose {
        eprintln!("journal: {msg}");
    }
}
