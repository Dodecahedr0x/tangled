use anyhow::{Context, Result};

#[tokio::main]
async fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().is_some_and(|s| s == "--provider-worker") {
        anyhow::ensure!(
            args.len() == 3,
            "provider worker requires data directory and public URL"
        );
        return tangled_e2e::cluster::provider_worker(std::path::Path::new(&args[1]), &args[2])
            .await;
    }
    if args.iter().any(|s| s == "--help" || s == "-h") {
        println!("tangled-e2e [--benchmark] [--scenario all|reed|loomed|weft|provider|protocol|throughput] [--operations N] [--payload-bytes N] [--warmups N] [--repetitions N] [--output PATH]");
        return Ok(());
    }
    let config = tangled_e2e::runner::Config::parse(&args)?;
    tangled_e2e::runner::run(config)
        .await
        .context("cluster acceptance/benchmark failed")
}
