use serde_json::{json, Value};
use std::io::{Read, Write};
use std::path::PathBuf;
use tangled_personal::{execute, Store};

#[tokio::main]
async fn main() {
    if let Err(message) = run().await {
        eprintln!("{}", json!({"error":message}));
        std::process::exit(1);
    }
}

async fn run() -> Result<(), String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().is_some_and(|arg| arg == "serve") {
        return serve(&args[1..]).await;
    }
    if args.iter().any(|arg| arg == "--help" || arg == "-h") || args.is_empty() {
        println!("Tangled personal workflows\n\nUsage: tangled-personal [--data DIRECTORY] FEATURE < request.json\n\nBrowser app: tangled-personal serve [--data DIRECTORY] [--listen 127.0.0.1:8788] [--no-open]\n\nFeatures: info, records, memory, capture, notes, feeds, agents, devices, publish, connections\n\nRequests are JSON objects with an action field. Responses are JSON. Errors are JSON on stderr and exit nonzero.\nData defaults to .data/personal. Pass secrets only through private stdin/input files.\nSee docs/features/personal.md for workflows and public schemas.");
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
                let value = args
                    .get(index)
                    .ok_or_else(|| format!("Missing value for {flag}"))?;
                if flag == "--data" {
                    data = value.into();
                } else {
                    input_file = Some(PathBuf::from(value));
                }
            }
            value if value.starts_with('-') => return Err(format!("Unknown option: {value}")),
            value => {
                if feature.replace(value.to_string()).is_some() {
                    return Err("Supply exactly one feature; send request JSON on stdin".into());
                }
            }
        }
        index += 1;
    }
    let feature = feature.ok_or("Missing feature")?;
    if ![
        "info",
        "records",
        "memory",
        "capture",
        "notes",
        "feeds",
        "agents",
        "devices",
        "publish",
        "connections",
    ]
    .contains(&feature.as_str())
    {
        return Err(format!("Unknown feature: {feature}"));
    }
    let input = if feature == "info" {
        json!({})
    } else {
        const MAX_REQUEST: u64 = 64 * 1024 * 1024;
        let reader: Box<dyn Read> = match input_file {
            Some(path) => Box::new(std::fs::File::open(path).map_err(|e| e.to_string())?),
            None => Box::new(std::io::stdin()),
        };
        let mut bytes = Vec::new();
        reader
            .take(MAX_REQUEST + 1)
            .read_to_end(&mut bytes)
            .map_err(|e| e.to_string())?;
        if bytes.len() as u64 > MAX_REQUEST {
            return Err("Request exceeds 64 MiB".into());
        }
        serde_json::from_slice::<Value>(&bytes).map_err(|e| format!("Invalid JSON request: {e}"))?
    };
    let mut store = Store::open(&data)?;
    let output = execute(&feature, &mut store, input)?;
    println!(
        "{}",
        serde_json::to_string(&output).map_err(|e| e.to_string())?
    );
    Ok(())
}

async fn serve(args: &[String]) -> Result<(), String> {
    let mut data = PathBuf::from(".data/personal");
    let mut listen = "127.0.0.1:8788"
        .parse()
        .map_err(|_| "Invalid default listen address")?;
    let mut open = true;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--data" | "--listen" => {
                let flag = &args[index];
                index += 1;
                let value = args
                    .get(index)
                    .ok_or_else(|| format!("Missing value for {flag}"))?;
                if flag == "--data" {
                    data = value.into();
                } else {
                    listen = value.parse().map_err(|_| {
                        "Use a numeric loopback IP and port, such as 127.0.0.1:8788"
                    })?;
                }
            }
            "--no-open" => open = false,
            "--help" | "-h" => {
                println!("Usage: tangled-personal serve [--data DIRECTORY] [--listen 127.0.0.1:8788] [--no-open]\nOpens the local personal app. The printed URL contains the private owner session credential.");
                return Ok(());
            }
            value => return Err(format!("Unknown serve option: {value}")),
        }
        index += 1;
    }
    let server = tangled_personal::web::Server::bind(&data, listen).await?;
    let url = server.startup_url();
    println!("{url}");
    std::io::stdout().flush().map_err(|e| e.to_string())?;
    if open {
        if let Err(message) = tangled_personal::web::open_browser(&url) {
            eprintln!("{message}");
        }
    }
    server.run().await
}
