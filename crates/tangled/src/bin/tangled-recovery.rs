//! Offline administrator CLI for verified KnotEngine recovery.
use std::{env, fs, io::Write, path::Path};
use tangled::{recovery::Backup, KnotEngine};

fn run(args: &[String]) -> Result<bool, Box<dyn std::error::Error>> {
    match args.first().map(String::as_str) {
        Some("backup") if args.len() == 4 => {
            let source = Path::new(&args[1]);
            if !source.join("describe.json").is_file() {return Err("source is not an existing KnotEngine store".into());}
            let engine = KnotEngine::open(source, &args[2])?;
            let backup = Backup::capture(&engine)?;
            let bytes = backup.to_bytes()?;
            let mut options = fs::OpenOptions::new();
            options.write(true).create_new(true);
            #[cfg(unix)] { use std::os::unix::fs::OpenOptionsExt; options.mode(0o600); }
            let mut file = options.open(&args[3])?;
            file.write_all(&bytes)?;
            file.sync_all()?;
            println!("{}", serde_json::json!({"archive_bytes":bytes.len(),"records":engine.stored.len(),"blobs":engine.blobs.len(),"blocks":engine.blocks.len()}));
            Ok(true)
        }
        Some("verify") if args.len() == 2 => {
            Backup::from_bytes(&fs::read(&args[1])?)?;
            println!("{{\"verified\":true}}");
            Ok(true)
        }
        Some("restore") if args.len() == 4 => {
            let backup = Backup::from_bytes(&fs::read(&args[1])?)?;
            let engine = backup.restore(Path::new(&args[2]), &args[3])?;
            println!("{}", serde_json::json!({"records":engine.stored.len(),"blobs":engine.blobs.len(),"blocks":engine.blocks.len()}));
            Ok(true)
        }
        Some(command @ ("scrub" | "repair")) if args.len() == 3 => {
            let backup = Backup::from_bytes(&fs::read(&args[1])?)?;
            let report = if command == "scrub" {backup.scrub(Path::new(&args[2]))?} else {backup.repair(Path::new(&args[2]))?};
            let clean = report.is_clean() || command == "repair";
            println!("{}",serde_json::to_string_pretty(&report)?);
            Ok(clean)
        }
        _ => Err("usage (host must be stopped): tangled-recovery backup DATA_DIR DID NEW_ARCHIVE | verify ARCHIVE | restore ARCHIVE EMPTY_DATA_DIR DID | scrub ARCHIVE DATA_DIR | repair ARCHIVE DATA_DIR".into()),
    }
}

fn main() {
    match run(&env::args().skip(1).collect::<Vec<_>>()) {
        Ok(true) => (),
        Ok(false) => std::process::exit(1),
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(2);
        }
    }
}
