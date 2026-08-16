//! Seed a running local knot with public posts so Weft is not an empty explorer.
//!
//! ```text
//! just weft-seed
//! cargo run -p weft-core --bin weft-seed -- --knot http://127.0.0.1:3000
//! ```
//!
//! Needs the knot(s) already listening. `just weft-seed` starts three and
//! calls this binary. Does not overwrite an existing `~/.weft/knot.txt`.

use std::process;

use weft_core::seed::{knot_path, run, SeedOpts};

fn print_help() {
    eprintln!(
        "weft-seed — publish public posts to a running knot\n\
         \n\
         Usage:\n\
           weft-seed [--knot URL]\n\
         \n\
         Flags:\n\
           --knot URL       knot base URL (default http://127.0.0.1:3000)\n\
           --help           this message\n\
         \n\
         Writes ~/.weft/knot.txt if missing. Start knots first: just weft-seed\n\
         (or just knot for a single node).\n"
    );
}

fn parse_args() -> SeedOpts {
    let mut knot = "http://127.0.0.1:3000".to_string();
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--knot" => {
                knot = args.next().unwrap_or_else(|| {
                    eprintln!("--knot needs URL");
                    process::exit(2);
                });
            }
            "-h" | "--help" => {
                print_help();
                process::exit(0);
            }
            other => {
                eprintln!("unknown argument {other}");
                print_help();
                process::exit(2);
            }
        }
    }
    SeedOpts {
        knot,
        knot_path: knot_path(),
    }
}

#[tokio::main]
async fn main() {
    let opts = parse_args();
    let knot = opts.knot.clone();
    match run(opts).await {
        Ok(report) => {
            println!("seeded {} ({} records)", knot, report.records);
            for (i, did) in report.identities.iter().enumerate() {
                println!("  identity {}: {did}", i + 1);
            }
            println!("open Weft — Overview and Topology fill after refresh");
        }
        Err(e) => {
            let msg = e.to_string();
            if msg.contains("unreachable") {
                eprintln!(
                    "knot unreachable at {knot} (is `just weft-seed` / `just knot` running?)"
                );
            } else {
                eprintln!("{e}");
            }
            process::exit(1);
        }
    }
}
