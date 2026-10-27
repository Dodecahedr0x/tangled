//! Offline recovery: no provider, source session, or network access required.
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
};
use tangled_import::{recovery_open, ArchiveStore};
fn private_write(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options
        .open(path)
        .map_err(|e| e.to_string())?
        .write_all(bytes)
        .map_err(|e| e.to_string())
}
fn run() -> Result<(), String> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    if args.len() != 3 {
        return Err(
            "Usage: tangled-import-recover <backup.json> <passphrase-file> <new-output-directory>"
                .into(),
        );
    }
    let packet = serde_json::from_slice(&fs::read(&args[0]).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    let password = fs::read_to_string(&args[1]).map_err(|e| e.to_string())?;
    let bundle = recovery_open(&packet, password.trim_end_matches(['\r', '\n']))?;
    let id = bundle.identity()?;
    let dir = PathBuf::from(&args[2]);
    fs::create_dir(&dir).map_err(|e| format!("Choose a new output directory: {e}"))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&dir, fs::Permissions::from_mode(0o700)).map_err(|e| e.to_string())?;
    }
    let mut store = ArchiveStore::open(&dir.join("imports"))?;
    store.restore(&bundle.owner, &id, &bundle.archive)?;
    store.verify_archive(&bundle.owner, &id)?;
    let records = store.objects(&bundle.owner, &id)?;
    private_write(
        &dir.join("activity.json"),
        &serde_json::to_vec_pretty(&records).map_err(|e| e.to_string())?,
    )?;
    private_write(&dir.join("recovered-account.json"),&serde_json::to_vec_pretty(&serde_json::json!({"did":bundle.owner,"ed25519":bundle.ed25519,"x25519":bundle.x25519})).map_err(|e|e.to_string())?)?;
    for receipt in store.receipts(&bundle.owner, &id)? {
        let (_, raw) = store.original(&bundle.owner, &id, &receipt.id)?;
        private_write(&dir.join(format!("original-{}", receipt.id)), &raw)?;
    }
    let files = store.files(&bundle.owner, &id)?;
    fs::create_dir(dir.join("attachments")).map_err(|e| e.to_string())?;
    for file in &files {
        let (_, bytes) =
            store.attachment(&bundle.owner, &id, &file.source, &file.account, &file.name)?;
        private_write(&dir.join("attachments").join(&file.key), &bytes)?;
    }
    private_write(
        &dir.join("files.json"),
        &serde_json::to_vec_pretty(&files).map_err(|e| e.to_string())?,
    )?;
    eprintln!("Recovered {} activity records, attachments, and original exports. Keep the output directory and recovered account keys private.",records.len());
    Ok(())
}
fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
