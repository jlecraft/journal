//! Regenerates the man page from the `Cli` definition: `cargo run --example man`.
//! Not part of the normal build -- the man page is checked in under `man/`
//! and only needs regenerating when `src/cli.rs` changes (§6.9).

use clap::CommandFactory;
use journal::cli::Cli;

fn main() -> std::io::Result<()> {
    let cmd = Cli::command();
    let man = clap_mangen::Man::new(cmd);

    let mut buffer = Vec::new();
    man.render(&mut buffer)?;

    std::fs::create_dir_all("man")?;
    std::fs::write("man/journal.1", buffer)?;
    println!("wrote man/journal.1");
    Ok(())
}
