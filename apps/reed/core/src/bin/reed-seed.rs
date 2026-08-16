//! Seed a running local knot with sample social content so Reed is not empty.
//!
//! ```text
//! just reed-seed
//! just reed-seed knot=http://127.0.0.1:3000
//! cargo run -p reed-core --bin reed-seed -- --knot http://127.0.0.1:3000
//! ```
//!
//! Needs `just knot`. Does not write `.data/` or overwrite an existing `keys.json`.

use std::process;

use reed_core::seed::{keys_path, run, SeedOpts};

fn print_help() {
    eprintln!(
        "reed-seed — publish sample posts and follows to a running knot\n\
         \n\
         Usage:\n\
           reed-seed [--knot URL] [--create-local]\n\
         \n\
         Flags:\n\
           --knot URL       knot base URL (default http://127.0.0.1:3000)\n\
           --create-local   write keys.json only if missing (never clobber)\n\
           --help           this message\n\
         \n\
         Load the Reed identity from $REED_HOME/keys.json or ~/.reed/keys.json.\n\
         Writes knot.txt if missing. Start the knot first: just knot\n"
    );
}

fn parse_args() -> SeedOpts {
    let mut knot = "http://127.0.0.1:3000".to_string();
    let mut create_local = false;
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--knot" => {
                knot = args.next().unwrap_or_else(|| {
                    eprintln!("--knot needs URL");
                    process::exit(2);
                });
            }
            "--create-local" => create_local = true,
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
        keys_path: keys_path(),
        create_local,
    }
}

#[tokio::main]
async fn main() {
    let opts = parse_args();
    let knot = opts.knot.clone();
    match run(opts).await {
        Ok(report) => {
            println!("seeded {} as {}", knot, report.local_did);
            for (i, did) in report.peers.iter().enumerate() {
                println!("  peer {}: {did}", i + 1);
            }
            println!(
                "  {} posts, {} records published",
                report.posts, report.records
            );
            println!("open Reed — Home and Discover fill after refresh");
        }
        Err(e) => {
            let msg = e.to_string();
            if msg.contains("unreachable") {
                eprintln!("knot unreachable at {knot} (is `just knot` running?)");
            } else {
                eprintln!("{e}");
            }
            process::exit(1);
        }
    }
}
