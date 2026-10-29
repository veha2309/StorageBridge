use crate::{engine::Engine, store::Node};
use anyhow::{Result, ensure};
use parking_lot::Mutex;
use std::{
    ffi::c_void,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};
use widestring::U16CStr;
use windows::Win32::Foundation::*;
use winfsp::{
    filesystem::{
        DirBuffer, DirInfo, DirMarker, FileInfo, FileSecurity, FileSystemContext, OpenFileInfo,
        VolumeInfo, WideNameInfo,
    },
    host::{FileSystemHost, VolumeParams},
};

pub struct BridgeFs {
    pub engine: Arc<Mutex<Engine>>,
    runtime: Arc<crate::runtime::Runtime>,
    io_tasks: threadpool::ThreadPool,
    directory_tasks: threadpool::ThreadPool,
    security: Vec<u8>,
}
impl winfsp::filesystem::AsyncFileSystemContext for BridgeFs {
    fn spawn_task(&self, future: impl std::future::Future<Output = ()> + Send + 'static) {
        // Keep directory enumeration off the potentially slow content queue.
        let directory = unsafe { self.with_operation_request(|request| request.Kind) }
            == Some(winfsp::constants::FspTransactKind::FspFsctlTransactQueryDirectoryKind as u32);
        let pool = if directory {
            &self.directory_tasks
        } else {
            &self.io_tasks
        };
        pool.execute(move || futures_executor::block_on(future));
    }
}
pub struct Handle {
    id: String,
    dir: DirBuffer,
    delete: AtomicBool,
}
fn error(e: anyhow::Error) -> winfsp::FspError {
    let text = e.to_string();
    if let Some(io) = e.downcast_ref::<std::io::Error>()
        && io.raw_os_error() == Some(112)
    {
        return STATUS_DISK_FULL.into();
    }
    if text.contains("not found") {
        STATUS_OBJECT_NAME_NOT_FOUND.into()
    } else if text.contains("already exists") {
        STATUS_OBJECT_NAME_COLLISION.into()
    } else if text.contains("not empty") {
        STATUS_DIRECTORY_NOT_EMPTY.into()
    } else if text.contains("is open") {
        STATUS_SHARING_VIOLATION.into()
    } else if text.contains("offline") || text.contains("Reconnect") {
        STATUS_DEVICE_OFF_LINE.into()
    } else {
        STATUS_UNSUCCESSFUL.into()
    }
}
fn info(n: &Node) -> FileInfo {
    use sha2::{Digest, Sha256};
    let digest = Sha256::digest(n.id.as_bytes());
    let time = ((n.modified.max(0) as u64) + 11644473600) * 10_000_000;
    FileInfo {
        // Cloud-backed content should not be eagerly content-indexed.
        file_attributes: if n.directory {
            0x10 | 0x2000
        } else {
            0x20 | 0x2000 | if n.cached { 0 } else { 0x1000 }
        },
        allocation_size: n.size.div_ceil(4096) * 4096,
        file_size: n.size,
        creation_time: time,
        last_write_time: time,
        last_access_time: time,
        change_time: time,
        index_number: u64::from_le_bytes(digest[..8].try_into().unwrap()),
        ..Default::default()
    }
}
impl BridgeFs {
    fn fill(&self, id: &str, out: &mut FileInfo) -> winfsp::Result<()> {
        *out = info(&self.engine.lock().store.get(id).map_err(error)?);
        Ok(())
    }
    fn security(&self, out: Option<&mut [c_void]>) -> winfsp::Result<u64> {
        if let Some(out) = out {
            if out.len() < self.security.len() {
                return Err(STATUS_BUFFER_OVERFLOW.into());
            }
            unsafe {
                std::ptr::copy_nonoverlapping(
                    self.security.as_ptr(),
                    out.as_mut_ptr().cast::<u8>(),
                    self.security.len(),
                );
            }
        }
        Ok(self.security.len() as u64)
    }
}
impl FileSystemContext for BridgeFs {
    type FileContext = Handle;
    fn get_security_by_name(
        &self,
        name: &U16CStr,
        out: Option<&mut [c_void]>,
        _: impl FnOnce(&U16CStr) -> Option<FileSecurity>,
    ) -> winfsp::Result<FileSecurity> {
        let n = self
            .runtime
            .resolve(&name.to_string_lossy())
            .map_err(error)?;
        Ok(FileSecurity {
            reparse: false,
            sz_security_descriptor: self.security(out)?,
            attributes: info(&n).file_attributes,
        })
    }
    fn open(
        &self,
        name: &U16CStr,
        options: u32,
        _: u32,
        out: &mut OpenFileInfo,
    ) -> winfsp::Result<Handle> {
        let n = self.runtime.open(&name.to_string_lossy()).map_err(error)?;
        if options & 1 != 0 && !n.directory {
            self.engine.lock().close_node(&n.id);
            return Err(STATUS_NOT_A_DIRECTORY.into());
        }
        if options & 0x40 != 0 && n.directory {
            self.engine.lock().close_node(&n.id);
            return Err(STATUS_FILE_IS_A_DIRECTORY.into());
        }
        *out.as_mut() = info(&n);
        Ok(Handle {
            id: n.id,
            dir: DirBuffer::new(),
            delete: AtomicBool::new(false),
        })
    }
    fn create(
        &self,
        name: &U16CStr,
        options: u32,
        _: u32,
        _: u32,
        _: Option<&[c_void]>,
        _: u64,
        _: Option<&[u8]>,
        _: bool,
        out: &mut OpenFileInfo,
    ) -> winfsp::Result<Handle> {
        let n = self
            .runtime
            .create(&name.to_string_lossy(), options & 1 != 0)
            .map_err(error)?;
        *out.as_mut() = info(&n);
        Ok(Handle {
            id: n.id,
            dir: DirBuffer::new(),
            delete: AtomicBool::new(false),
        })
    }
    fn close(&self, h: Handle) {
        self.engine.lock().close_node(&h.id);
    }
    fn cleanup(&self, h: &Handle, _: Option<&U16CStr>, flags: u32) {
        if (flags & 1 != 0 || h.delete.load(Ordering::SeqCst))
            && let Err(err) = self.runtime.delete(&h.id)
        {
            self.engine.lock().last_error = Some(err.to_string());
        }
    }
    fn set_delete(&self, h: &Handle, _: &U16CStr, delete: bool) -> winfsp::Result<()> {
        if delete {
            self.runtime.can_delete(&h.id).map_err(error)?;
        }
        h.delete.store(delete, Ordering::SeqCst);
        Ok(())
    }
    fn get_file_info(&self, h: &Handle, out: &mut FileInfo) -> winfsp::Result<()> {
        self.fill(&h.id, out)
    }
    fn get_security(&self, _: &Handle, out: Option<&mut [c_void]>) -> winfsp::Result<u64> {
        self.security(out)
    }
    fn read(&self, h: &Handle, buffer: &mut [u8], offset: u64) -> winfsp::Result<u32> {
        match self.runtime.read(&h.id, offset, buffer) {
            Ok(count) => Ok(count as u32),
            Err(err) => {
                self.engine.lock().last_error = Some(err.to_string());
                Err(error(err))
            }
        }
    }
    fn write(
        &self,
        h: &Handle,
        buffer: &[u8],
        offset: u64,
        eof: bool,
        constrained: bool,
        out: &mut FileInfo,
    ) -> winfsp::Result<u32> {
        let count = self
            .runtime
            .write(&h.id, offset, buffer, eof, constrained)
            .map_err(error)?;
        self.fill(&h.id, out)?;
        Ok(count as u32)
    }
    fn flush(&self, h: Option<&Handle>, out: &mut FileInfo) -> winfsp::Result<()> {
        if let Some(h) = h {
            self.engine.lock().flush(&h.id).map_err(error)?;
            self.fill(&h.id, out)?;
        }
        Ok(())
    }
    fn set_file_size(
        &self,
        h: &Handle,
        size: u64,
        allocation: bool,
        out: &mut FileInfo,
    ) -> winfsp::Result<()> {
        if !allocation || size < self.engine.lock().store.get(&h.id).map_err(error)?.size {
            self.runtime.resize(&h.id, size).map_err(error)?;
        }
        self.fill(&h.id, out)
    }
    fn overwrite(
        &self,
        h: &Handle,
        _: u32,
        _: bool,
        _: u64,
        _: Option<&[u8]>,
        out: &mut FileInfo,
    ) -> winfsp::Result<()> {
        self.runtime.resize(&h.id, 0).map_err(error)?;
        self.fill(&h.id, out)
    }
    fn rename(&self, h: &Handle, _: &U16CStr, new: &U16CStr, replace: bool) -> winfsp::Result<()> {
        self.runtime
            .rename(&h.id, &new.to_string_lossy(), replace)
            .map_err(error)
    }
    fn set_basic_info(
        &self,
        h: &Handle,
        _: u32,
        _: u64,
        _: u64,
        write: u64,
        _: u64,
        out: &mut FileInfo,
    ) -> winfsp::Result<()> {
        if write != 0 {
            let gate = self.runtime.gate(&h.id);
            let _guard = gate.lock();
            let e = self.engine.lock();
            let mut n = e.store.get(&h.id).map_err(error)?;
            n.modified = (write / 10_000_000).saturating_sub(11644473600) as i64;
            e.store.put(&n).map_err(error)?;
        }
        self.fill(&h.id, out)
    }
    fn read_directory(
        &self,
        h: &Handle,
        _: Option<&U16CStr>,
        marker: DirMarker<'_>,
        buffer: &mut [u8],
    ) -> winfsp::Result<u32> {
        if let Ok(lock) = h.dir.acquire(marker.is_none(), None) {
            let nodes = self.runtime.list(&h.id).map_err(error)?;
            let e = self.engine.lock();
            if h.id != e.root_id() {
                let n = e.store.get(&h.id).map_err(error)?;
                for (name, id) in [(".", n.id), ("..", n.parent)] {
                    let mut entry = DirInfo::<255>::new();
                    entry.set_name(name)?;
                    *entry.file_info_mut() = info(&e.store.get(&id).map_err(error)?);
                    lock.write(&mut entry)?;
                }
            }
            for n in nodes {
                let mut entry = DirInfo::<255>::new();
                entry.set_name(&n.alias)?;
                *entry.file_info_mut() = info(&n);
                lock.write(&mut entry)?;
            }
        }
        Ok(h.dir.read(marker, buffer))
    }
    fn get_volume_info(&self, out: &mut VolumeInfo) -> winfsp::Result<()> {
        let e = self.engine.lock();
        let about = e
            .store
            .setting("about")
            .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
            .unwrap_or_default();
        out.total_size = about["storageQuota"]["limit"]
            .as_str()
            .and_then(|s| s.parse().ok())
            .unwrap_or(15 * 1024 * 1024 * 1024);
        let used = about["storageQuota"]["usage"]
            .as_str()
            .and_then(|s| s.parse().ok())
            .unwrap_or(0);
        out.free_size = out.total_size.saturating_sub(used);
        out.set_volume_label("Storage Bridge");
        Ok(())
    }
}
pub fn available() -> bool {
    std::path::Path::new("C:\\Program Files (x86)\\WinFsp\\bin\\winfsp-x64.dll").exists()
        || std::path::Path::new("C:\\Program Files\\WinFsp\\bin\\winfsp-x64.dll").exists()
}
pub fn mount(
    runtime: Arc<crate::runtime::Runtime>,
    letter: &str,
) -> Result<FileSystemHost<BridgeFs>> {
    ensure!(
        letter.len() == 2
            && letter.as_bytes()[0].is_ascii_uppercase()
            && &letter[1..] == ":"
            && letter.as_bytes()[0] >= b'D',
        "Choose a drive letter from D: to Z:"
    );
    ensure!(
        !std::path::Path::new(&format!("{letter}\\")).exists(),
        "Drive {letter} is occupied; choose another letter"
    );
    ensure!(available(), "Install WinFsp first: https://winfsp.dev/rel/");
    // Load only the trusted installation path before resolving delay-loaded functions.
    let path =
        if std::path::Path::new("C:\\Program Files (x86)\\WinFsp\\bin\\winfsp-x64.dll").exists() {
            "C:\\Program Files (x86)\\WinFsp\\bin\\winfsp-x64.dll"
        } else {
            "C:\\Program Files\\WinFsp\\bin\\winfsp-x64.dll"
        };
    unsafe {
        windows::Win32::System::LibraryLoader::LoadLibraryW(windows::core::PCWSTR(
            super::wide(path).as_ptr(),
        ))?;
    }
    let _init = winfsp::winfsp_init()?;
    let mut volume = VolumeParams::new();
    volume
        .sector_size(512)
        .sectors_per_allocation_unit(8)
        .max_component_length(255)
        .filesystem_name("StorageBridge")
        .prefix(format!(
            "\\StorageBridge\\Drive{}",
            letter.trim_end_matches(':')
        ))
        .case_sensitive_search(false)
        .case_preserved_names(true)
        .unicode_on_disk(true)
        .file_info_timeout(1000)
        .irp_capacity(100)
        .flush_and_purge_on_cleanup(true);
    let mut host = FileSystemHost::<BridgeFs>::new_async(
        volume,
        BridgeFs {
            engine: runtime.engine.clone(),
            runtime,
            io_tasks: threadpool::ThreadPool::new(8),
            directory_tasks: threadpool::ThreadPool::new(2),
            security: super::security_descriptor()?,
        },
    )?;
    host.mount(letter)?;
    host.start_with_threads(8)?;
    Ok(host)
}
