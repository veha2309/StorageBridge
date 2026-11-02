use anyhow::{Context, Result, ensure};
use serde_json::Value;
use std::{
    fs::{File, OpenOptions, Permissions},
    io::{Read, Write},
    os::{
        fd::AsRawFd,
        unix::{
            fs::{DirBuilderExt, OpenOptionsExt, PermissionsExt},
            net::{UnixListener, UnixStream},
        },
    },
    path::PathBuf,
    sync::OnceLock,
    time::Duration,
};
const MAX_FRAME: usize = 1024 * 1024;
static TEST_INSTANCE: OnceLock<String> = OnceLock::new();
pub fn test_instance(name: &str) -> Result<()> {
    ensure!(
        !name.is_empty()
            && name.len() <= 32
            && name.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'-'),
        "Invalid test instance name"
    );
    TEST_INSTANCE
        .set(format!("-test-{name}"))
        .map_err(|_| anyhow::anyhow!("Test instance already configured"))
}
fn directory() -> Result<PathBuf> {
    let path = PathBuf::from(format!("/tmp/storagebridge-{}", unsafe { libc::getuid() }));
    match std::fs::DirBuilder::new().mode(0o700).create(&path) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
        Err(e) => return Err(e.into()),
    }
    use std::os::unix::fs::MetadataExt;
    let m = std::fs::symlink_metadata(&path)?;
    ensure!(
        m.is_dir() && m.uid() == unsafe { libc::getuid() } && m.mode() & 0o077 == 0,
        "Unsafe IPC directory"
    );
    Ok(path)
}
pub fn pipe_name() -> Result<String> {
    Ok(directory()?
        .join(format!(
            "v1{}.sock",
            TEST_INSTANCE.get().map(String::as_str).unwrap_or("")
        ))
        .to_string_lossy()
        .into_owned())
}
pub fn read_frame(stream: &mut impl Read) -> Result<Value> {
    let mut size = [0; 4];
    stream.read_exact(&mut size)?;
    let size = u32::from_le_bytes(size) as usize;
    ensure!(size > 0 && size <= MAX_FRAME, "Invalid IPC frame length");
    let mut body = vec![0; size];
    stream.read_exact(&mut body)?;
    Ok(serde_json::from_slice(&body)?)
}
pub fn write_frame(stream: &mut impl Write, value: &Value) -> Result<()> {
    let body = serde_json::to_vec(value)?;
    ensure!(body.len() <= MAX_FRAME, "IPC response too large");
    stream.write_all(&(body.len() as u32).to_le_bytes())?;
    stream.write_all(&body)?;
    Ok(())
}
pub fn request(value: &Value) -> Result<Value> {
    let mut stream =
        UnixStream::connect(pipe_name()?).context("Background engine is not running")?;
    stream.set_read_timeout(Some(Duration::from_secs(120)))?;
    write_frame(&mut stream, value)?;
    read_frame(&mut stream)
}
pub struct Instance(File);
impl Instance {
    pub fn acquire() -> Result<Self> {
        let path = format!("{}.lock", pipe_name()?);
        let file = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW)
            .open(path)?;
        ensure!(
            unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } == 0,
            "Storage Bridge is already running"
        );
        Ok(Self(file))
    }
}
impl Drop for Instance {
    fn drop(&mut self) {
        unsafe {
            libc::flock(self.0.as_raw_fd(), libc::LOCK_UN);
        }
    }
}
pub fn serve(mut handler: impl FnMut(Value) -> Value) -> Result<()> {
    let path = pipe_name()?;
    // Only the instance-lock owner may remove a stale endpoint.
    if std::fs::symlink_metadata(&path).is_ok() {
        std::fs::remove_file(&path)?;
    }
    let listener = UnixListener::bind(&path)?;
    std::fs::set_permissions(&path, Permissions::from_mode(0o600))?;
    for connection in listener.incoming() {
        let mut stream = connection?;
        stream.set_read_timeout(Some(Duration::from_secs(5)))?;
        stream.set_write_timeout(Some(Duration::from_secs(5)))?;
        let reply = match read_frame(&mut stream) {
            Ok(value) => handler(value),
            Err(e) => serde_json::json!({"version":1,"ok":false,"error":e.to_string()}),
        };
        let _ = write_frame(&mut stream, &reply);
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn framing() {
        let value = serde_json::json!({"version":1,"method":"status"});
        let mut bytes = Vec::new();
        write_frame(&mut bytes, &value).unwrap();
        assert_eq!(read_frame(&mut &bytes[..]).unwrap(), value);
        assert!(read_frame(&mut &0u32.to_le_bytes()[..]).is_err());
        assert!(read_frame(&mut &((MAX_FRAME + 1) as u32).to_le_bytes()[..]).is_err());
    }
}
