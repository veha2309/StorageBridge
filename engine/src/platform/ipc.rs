use super::*;
use anyhow::{Context, Result, ensure};
use serde_json::Value;
use std::{
    fs::File,
    io::{Read, Write},
    os::windows::io::{AsRawHandle, FromRawHandle},
    ptr::null_mut,
};
use windows_sys::Win32::{Foundation::*, Storage::FileSystem::*, System::Pipes::*};

pub const MAX_FRAME: usize = 1024 * 1024;
static TEST_INSTANCE: std::sync::OnceLock<String> = std::sync::OnceLock::new();
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
fn suffix() -> &'static str {
    TEST_INSTANCE.get().map(String::as_str).unwrap_or("")
}
pub fn pipe_name() -> Result<String> {
    Ok(format!(
        "\\\\.\\pipe\\StorageBridge-v1-{}{}",
        user_sid()?,
        suffix()
    ))
}
pub fn read_frame(file: &mut File) -> Result<Value> {
    let mut length = [0; 4];
    file.read_exact(&mut length)?;
    let length = u32::from_le_bytes(length) as usize;
    ensure!(
        length > 0 && length <= MAX_FRAME,
        "Invalid IPC frame length"
    );
    let mut body = vec![0; length];
    file.read_exact(&mut body)?;
    Ok(serde_json::from_slice(&body)?)
}
pub fn write_frame(file: &mut File, value: &Value) -> Result<()> {
    let body = serde_json::to_vec(value)?;
    ensure!(body.len() <= MAX_FRAME, "IPC response too large");
    file.write_all(&(body.len() as u32).to_le_bytes())?;
    file.write_all(&body)?;
    Ok(())
}
pub fn request(value: &Value) -> Result<Value> {
    let mut pipe = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(pipe_name()?)
        .context("Storage Bridge background engine is not running")?;
    write_frame(&mut pipe, value)?;
    read_frame(&mut pipe)
}

/// A protected single-instance mutex prevents two engines owning the same cache.
pub struct Instance(HANDLE, HANDLE);
impl Instance {
    pub fn acquire() -> Result<Self> {
        let mut descriptor = security_descriptor()?;
        let sa = SECURITY_ATTRIBUTES {
            nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: descriptor.as_mut_ptr().cast(),
            bInheritHandle: 0,
        };
        let name = wide(&format!(
            "Local\\StorageBridge-v1-{}{}",
            user_sid()?,
            suffix()
        ));
        unsafe {
            let handle = CreateMutexW(&sa, 0, name.as_ptr());
            ensure!(!handle.is_null(), "Cannot acquire engine instance");
            if GetLastError() == ERROR_ALREADY_EXISTS {
                CloseHandle(handle);
                anyhow::bail!("Storage Bridge is already running");
            }
            let installer_guard = CreateMutexW(&sa, 0, wide("Local\\StorageBridgeEngine").as_ptr());
            if installer_guard.is_null() {
                CloseHandle(handle);
                anyhow::bail!("Cannot acquire installer guard");
            }
            Ok(Self(handle, installer_guard))
        }
    }
}
impl Drop for Instance {
    fn drop(&mut self) {
        unsafe {
            CloseHandle(self.0);
            CloseHandle(self.1);
        }
    }
}

pub fn serve(mut handler: impl FnMut(Value) -> Value) -> Result<()> {
    let mut descriptor = security_descriptor()?;
    let sa = SECURITY_ATTRIBUTES {
        nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
        lpSecurityDescriptor: descriptor.as_mut_ptr().cast(),
        bInheritHandle: 0,
    };
    let name = wide(&pipe_name()?);
    loop {
        unsafe {
            let handle = CreateNamedPipeW(
                name.as_ptr(),
                PIPE_ACCESS_DUPLEX | FILE_FLAG_FIRST_PIPE_INSTANCE,
                PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT | PIPE_REJECT_REMOTE_CLIENTS,
                1,
                65536,
                65536,
                5000,
                &sa,
            );
            ensure!(
                handle != INVALID_HANDLE_VALUE,
                "Cannot create secured IPC pipe: {}",
                std::io::Error::last_os_error()
            );
            let connected =
                ConnectNamedPipe(handle, null_mut()) != 0 || GetLastError() == ERROR_PIPE_CONNECTED;
            let mut file = File::from_raw_handle(handle);
            if connected {
                let reply = match read_frame(&mut file) {
                    Ok(v) => handler(v),
                    Err(e) => serde_json::json!({"version":1,"ok":false,"error":e.to_string()}),
                };
                let _ = write_frame(&mut file, &reply);
                FlushFileBuffers(file.as_raw_handle());
                DisconnectNamedPipe(file.as_raw_handle());
            }
        }
    }
}
