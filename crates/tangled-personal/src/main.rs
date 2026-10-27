use serde_json::{json, Value};
use std::io::Read;
use std::path::PathBuf;
use tangled_personal::{execute, Store};

fn main() {
    if let Err(message) = run() {
        eprintln!("{}", json!({"error":message}));
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|arg| arg == "--help" || arg == "-h") || args.is_empty() {
        println!("Tangled personal workflows\n\nUsage: tangled-personal [--data DIRECTORY] FEATURE < request.json\n\nFeatures: info, records, memory, capture, notes, feeds, agents, devices, publish, connections\n\nRequests are JSON objects with an action field. Responses are JSON. Errors are JSON on stderr and exit nonzero.\nData defaults to .data/personal. Pass secrets only through private stdin/input files.\nSee docs/features/personal.md for workflows and public schemas.");
        return Ok(());
    }
    let mut data = PathBuf::from(".data/personal");
    let mut feature = None;
    let mut input_file = None;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--data" | "--input" => {
                let flag = &args[index];
                index += 1;
                let value = args.get(index).ok_or_else(|| format!("Missing value for {flag}"))?;
                if flag == "--data" { data = value.into(); } else { input_file = Some(PathBuf::from(value)); }
            }
            value if value.starts_with('-') => return Err(format!("Unknown option: {value}")),
            value => {
                if feature.replace(value.to_string()).is_some() { return Err("Supply exactly one feature; send request JSON on stdin".into()); }
            }
        }
        index += 1;
    }
    let feature = feature.ok_or("Missing feature")?;
    if !["info","records","memory","capture","notes","feeds","agents","devices","publish","connections"].contains(&feature.as_str()) {
        return Err(format!("Unknown feature: {feature}"));
    }
    let input = if feature == "info" { json!({}) } else {
        const MAX_REQUEST: u64 = 64 * 1024 * 1024;
        let reader: Box<dyn Read> = match input_file { Some(path) => Box::new(std::fs::File::open(path).map_err(|e|e.to_string())?),None => Box::new(std::io::stdin()) };
        let mut bytes = Vec::new();
        reader.take(MAX_REQUEST + 1).read_to_end(&mut bytes).map_err(|e|e.to_string())?;
        if bytes.len() as u64 > MAX_REQUEST { return Err("Request exceeds 64 MiB".into()); }
        serde_json::from_slice::<Value>(&bytes).map_err(|e|format!("Invalid JSON request: {e}"))?
    };
    let mut store = Store::open(&data)?;
    let output = execute(&feature,&mut store,input)?;
    println!("{}",serde_json::to_string(&output).map_err(|e|e.to_string())?);
    Ok(())
}
