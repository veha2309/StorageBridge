//! Read-only live diagnostic. No content file is created and no cloud item is modified.
//! cargo run --release --example verify_mounted_read -- DRIVE_FILE_ID MOUNTED_PATH [--sample]
use anyhow::{Context, Result, ensure};
use sha2::{Digest, Sha256};
use std::{
    io::{Read, Seek, SeekFrom},
    sync::Arc,
    time::Instant,
};
use storage_bridge::{
    auth::Auth,
    drive::{GoogleDrive, Remote},
};

fn main() -> Result<()> {
    let args: Vec<_> = std::env::args().collect();
    ensure!(
        args.len() >= 3,
        "Supply the Drive file ID and mounted file path; add --sample for start/middle/end only"
    );
    let root = directories::ProjectDirs::from("dev", "StorageBridge", "StorageBridge")
        .context("Cannot find application data")?
        .data_local_dir()
        .join("google");
    let db = rusqlite::Connection::open_with_flags(
        root.join("metadata.db"),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )?;
    let scope: String =
        db.query_row("SELECT value FROM settings WHERE key='root_id'", [], |r| {
            r.get(0)
        })?;
    let drive = GoogleDrive::new(Arc::new(Auth::new(root)?))?;
    let node = drive.get(&args[1])?.context("Cloud file not found")?;
    // This diagnostic accepts only files directly inside the configured test scope.
    ensure!(
        node.parent == scope && !node.directory,
        "Select a file directly inside the mounted StorageBridge folder"
    );
    let mut file = std::fs::File::open(&args[2]).context("Cannot open mounted file")?;
    ensure!(
        file.metadata()?.len() == node.size,
        "Mounted size differs from Google"
    );
    let mut buffer = vec![0; storage_bridge::streaming::BLOCK];
    let start = Instant::now();
    if args.iter().any(|a| a == "--sample") {
        for offset in [
            0,
            (2u64 * 1024 * 1024 * 1024).min(node.size.saturating_sub(1)),
            node.size.saturating_sub(buffer.len() as u64),
        ] {
            file.seek(SeekFrom::Start(offset))?;
            let length = (node.size - offset).min(buffer.len() as u64) as usize;
            file.read_exact(&mut buffer[..length])?;
            let remote = drive.read_range(&node, offset, length)?;
            ensure!(
                buffer[..length] == remote,
                "Mounted bytes differ at offset {offset}"
            );
            println!("Verified {length} bytes at offset {offset}");
        }
    } else {
        let expected = drive
            .sha256(&node)?
            .context("Google has not provided a SHA-256 checksum")?;
        let mut hash = Sha256::new();
        let mut count = 0u64;
        let mut report = 256 * 1024 * 1024;
        loop {
            let length = file.read(&mut buffer)?;
            if length == 0 {
                break;
            }
            hash.update(&buffer[..length]);
            count += length as u64;
            if count >= report {
                println!(
                    "Read {count} / {} bytes in {:.1}s",
                    node.size,
                    start.elapsed().as_secs_f64()
                );
                report += 256 * 1024 * 1024;
            }
        }
        ensure!(
            count == node.size,
            "Unexpected end of file at {count} / {} bytes",
            node.size
        );
        ensure!(
            format!("{:x}", hash.finalize()) == expected,
            "Mounted stream SHA-256 differs from Google"
        );
        println!("Full {count}-byte mounted stream matches Google's SHA-256");
    }
    ensure!(
        drive.get(&node.id)?.is_some_and(|n| n.same_content(&node)),
        "Cloud content changed during verification"
    );
    println!(
        "PASS: read-only verification in {:.1}s; no content written to disk",
        start.elapsed().as_secs_f64()
    );
    Ok(())
}
