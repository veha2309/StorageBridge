//! macOS platform services; cloud and cache behavior stay in the shared engine.
pub mod filesystem;
pub mod ipc;
mod mount;
use anyhow::{Context, Result};
use std::path::Path;

const SERVICE: &str = "dev.storagebridge.credentials";
pub fn save_secret(path: &Path, bytes: &[u8]) -> Result<()> {
    // Keep the existing existence checks, but store secret bytes only in Keychain.
    security_framework::passwords::set_generic_password(SERVICE, &path.to_string_lossy(), bytes)?;
    std::fs::write(path, b"StorageBridge:Keychain")?;
    Ok(())
}
pub fn load_secret(path: &Path) -> Result<Vec<u8>> {
    Ok(security_framework::passwords::get_generic_password(
        SERVICE,
        &path.to_string_lossy(),
    )?)
}
pub fn startup(enabled: bool) -> Result<()> {
    let home = std::env::var_os("HOME").context("Cannot locate home directory")?;
    let dir = Path::new(&home).join("Library/LaunchAgents");
    let path = dir.join("dev.storagebridge.engine.plist");
    if !enabled {
        if path.exists() {
            std::fs::remove_file(path)?;
        }
        return Ok(());
    }
    std::fs::create_dir_all(dir)?;
    let exe = std::env::current_exe()?
        .to_string_lossy()
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;");
    std::fs::write(
        path,
        format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict><key>Label</key><string>dev.storagebridge.engine</string><key>ProgramArguments</key><array><string>{exe}</string><string>--daemon</string></array><key>RunAtLoad</key><true/></dict></plist>"#
        ),
    )?;
    Ok(())
}

pub fn open_browser(url: &str) -> Result<()> {
    // The daemon has no AppKit run loop. Launch Services runs in the open helper.
    let mut child = std::process::Command::new("/usr/bin/open")
        .arg(url)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .context("Cannot launch the default browser")?;
    wait_for_browser(&mut child, std::time::Duration::from_secs(10))
}
fn wait_for_browser(child: &mut std::process::Child, timeout: std::time::Duration) -> Result<()> {
    let deadline = std::time::Instant::now() + timeout;
    loop {
        if let Some(status) = child.try_wait()? {
            anyhow::ensure!(
                status.success(),
                "macOS could not open the browser; check your default browser and try again"
            );
            return Ok(());
        }
        if std::time::Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            anyhow::bail!(
                "Opening the browser timed out; check your default browser and try again"
            );
        }
        std::thread::sleep(std::time::Duration::from_millis(25));
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn browser_launcher_reports_failure_and_cannot_wait_forever() -> Result<()> {
        use std::{process::Command, time::Duration};
        let mut failed = Command::new("/usr/bin/false").spawn()?;
        assert!(wait_for_browser(&mut failed, Duration::from_secs(2)).is_err());
        let mut stalled = Command::new("/bin/sleep").arg("30").spawn()?;
        assert!(wait_for_browser(&mut stalled, Duration::ZERO).is_err());
        assert!(stalled.try_wait()?.is_some());
        let mut succeeded = Command::new("/usr/bin/true").spawn()?;
        wait_for_browser(&mut succeeded, Duration::from_secs(2))?;
        Ok(())
    }
}
