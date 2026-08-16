//! Follow a real Mastodon/ActivityPub account via the local knot.
//!
//! ```text
//! just reed-ap-seed
//! just reed-ap-seed knot=http://127.0.0.1:3000
//! REED_AP_ACCT=acct:Mastodon@mastodon.social just reed-ap-seed
//! cargo run -p reed-core --bin reed-ap-seed -- --knot http://127.0.0.1:3000 --create-local
//! ```
//!
//! Needs `just knot` (ActivityPub is on by default). Does not overwrite an
//! existing `keys.json`. Resolve runs WebFinger + actor fetch, then GETs the
//! public outbox (first page) so Reed sees posts without inbound S2S.
//!
//! Inbound replies from Mastodon still need `--public-url` and a reachable
//! knot inbox. Outbound Create to the instance inbox is still attempted.
//!
//! `--mock` keeps the loopback peer path (`just reed-ap-demo-mock`).

use std::process;

use reed_core::seed::{default_ap_acct, keys_path, run_ap_acct, run_ap_mock, SeedOpts};

fn print_help() {
    eprintln!(
        "reed-ap-seed — follow a real Mastodon account and pull its outbox\n\
         \n\
         Usage:\n\
           reed-ap-seed [--knot URL] [--create-local] [--acct ACCT] [--mock]\n\
         \n\
         Flags:\n\
           --knot URL       knot base URL (default http://127.0.0.1:3000)\n\
           --create-local   write keys.json only if missing (never clobber)\n\
           --acct ACCT      acct:user@instance (default $REED_AP_ACCT or\n\
                            acct:Gargron@mastodon.social)\n\
           --mock           loopback mock peer (no public internet)\n\
           --help           this message\n\
         \n\
         Default demo account: acct:Gargron@mastodon.social\n\
         Override: REED_AP_ACCT=acct:user@instance  or  --acct acct:user@instance\n\
         \n\
         Resolve pulls the public outbox (no HTTP signatures). Inbox POST still\n\
         requires signatures. CI never hits the public internet.\n\
         Start the knot first: just knot\n"
    );
}

struct ApArgs {
    opts: SeedOpts,
    acct: String,
    mock: bool,
}

fn parse_args() -> ApArgs {
    let mut knot = "http://127.0.0.1:3000".to_string();
    let mut create_local = false;
    let mut acct = default_ap_acct();
    let mut mock = false;
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--knot" => {
                knot = args.next().unwrap_or_else(|| {
                    eprintln!("--knot needs URL");
                    process::exit(2);
                });
            }
            "--acct" => {
                acct = args.next().unwrap_or_else(|| {
                    eprintln!("--acct needs acct:user@instance");
                    process::exit(2);
                });
            }
            "--create-local" => create_local = true,
            "--mock" => mock = true,
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
    ApArgs {
        opts: SeedOpts {
            knot,
            keys_path: keys_path(),
            create_local,
        },
        acct,
        mock,
    }
}

#[tokio::main]
async fn main() {
    let args = parse_args();
    let knot = args.opts.knot.clone();
    let result = if args.mock {
        run_ap_mock(args.opts).await
    } else {
        run_ap_acct(args.opts, args.acct).await
    };
    match result {
        Ok(report) => {
            println!("seeded {} as {}", knot, report.local_did);
            println!("  followed {} ({})", report.acct, report.actor);
            println!(
                "  {} federated notes, {} local records",
                report.notes, report.records
            );
            println!("open Reed — Refresh Home and Discover for federated posts");
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
