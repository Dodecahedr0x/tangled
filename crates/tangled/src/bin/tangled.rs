//! Thin CLI around the Tangled reference client.

use std::env;
use std::io::{self, Read};

fn main() {
    let mut args = env::args().skip(1);
    let cmd = args.next().unwrap_or_else(|| "help".into());
    match cmd.as_str() {
        "help" | "-h" | "--help" => {
            eprintln!(
                "tangled — Tangled protocol reference client\n\
                 \n\
                 Commands:\n\
                   identity     generate a did:sol identity + agreement key\n\
                   cid          CIDv1 dag-cbor blake3 of stdin\n\
                   cid-raw      CIDv1 raw blake3 of stdin\n\
                   did-parse    parse a did:sol string\n\
                   foreign-rkey <protocol> <foreign_id>\n"
            );
        }
        "identity" => {
            let id = tangled::Identity::generate();
            println!("did={}", id.did_string());
            println!(
                "pubkey={}",
                tangled::identity::encode_pubkey_base58(&id.pubkey_bytes())
            );
            println!("agreement_pk={}", hex::encode(id.agreement_public_bytes()));
        }
        "cid" => {
            let mut buf = Vec::new();
            io::stdin().read_to_end(&mut buf).ok();
            println!("{}", tangled::Cid::dag_cbor(&buf));
        }
        "cid-raw" => {
            let mut buf = Vec::new();
            io::stdin().read_to_end(&mut buf).ok();
            println!("{}", tangled::Cid::raw(&buf));
        }
        "did-parse" => {
            let s = args.next().expect("did string");
            match tangled::Did::parse(&s) {
                Ok(d) => println!("{}", d.as_str()),
                Err(e) => {
                    eprintln!("{e}");
                    std::process::exit(1);
                }
            }
        }
        "foreign-rkey" => {
            let proto = args.next().expect("protocol");
            let fid = args.next().expect("foreign_id");
            match tangled::foreign::import_rkey(&proto, &fid) {
                Ok(r) => println!("{r}"),
                Err(e) => {
                    eprintln!("{e}");
                    std::process::exit(1);
                }
            }
        }
        other => {
            eprintln!("unknown command {other}");
            std::process::exit(2);
        }
    }
}
