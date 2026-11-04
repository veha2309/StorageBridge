use crate::{runtime::Runtime, store::Node};
use anyhow::{Result, ensure};
use fuser::*;
use std::{
    collections::HashMap,
    ffi::OsStr,
    path::PathBuf,
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
const TTL: Duration = Duration::from_secs(1);
pub fn mount_path() -> String {
    PathBuf::from(std::env::var_os("HOME").unwrap_or_default())
        .join("StorageBridge")
        .to_string_lossy()
        .into_owned()
}
pub fn available() -> bool {
    super::mount::available()
}
pub struct Host {
    session: Option<BackgroundSession>,
    mount: Option<super::mount::Mount>,
}
impl Host {
    pub fn is_mounted(&self) -> bool {
        self.session
            .as_ref()
            .is_some_and(|s| !s.guard.is_finished())
    }
    pub fn unmount(&mut self) -> Result<()> {
        self.mount.take();
        self.session.take();
        Ok(())
    }
}
pub fn mount(runtime: Arc<Runtime>, _: &str) -> Result<Host> {
    ensure!(
        available(),
        "Install macFUSE first: https://macfuse.github.io/"
    );
    let path = PathBuf::from(mount_path());
    use std::os::unix::fs::DirBuilderExt;
    match std::fs::DirBuilder::new().mode(0o700).create(&path) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
        Err(e) => return Err(e.into()),
    }
    let canonical = std::fs::canonicalize(&path)?;
    let cpath = std::ffi::CString::new(canonical.to_string_lossy().as_bytes())?;
    let mut stat = std::mem::MaybeUninit::<libc::statfs>::uninit();
    ensure!(
        unsafe { libc::statfs(cpath.as_ptr(), stat.as_mut_ptr()) } == 0,
        "Cannot inspect mount folder"
    );
    let stat = unsafe { stat.assume_init() };
    let existing = unsafe { std::ffi::CStr::from_ptr(stat.f_mntonname.as_ptr()) };
    ensure!(
        existing.to_bytes() != cpath.as_bytes(),
        "Mount folder is already mounted"
    );
    ensure!(
        !std::fs::symlink_metadata(&path)?.file_type().is_symlink(),
        "Mount folder must not be a symlink"
    );
    for entry in std::fs::read_dir(&path)? {
        let entry = entry?;
        // Finder creates this metadata file even before the volume is mounted.
        ensure!(
            entry.file_name() == ".DS_Store" && entry.file_type()?.is_file(),
            "Mount folder must be empty (apart from Finder's .DS_Store)"
        );
    }
    let root = runtime.engine.lock().root_id()?;
    let (sender, receiver) = std::sync::mpsc::channel();
    let fs = BridgeFs {
        runtime,
        ids: HashMap::from([(1, root)]),
        next: 2,
        ready: Some(sender),
    };
    let (mount, fd) = super::mount::open(&cpath)?;
    let session = Session::from_fd(fs, fd, SessionACL::RootAndOwner).spawn()?;
    let host = Host {
        session: Some(session),
        mount: Some(mount),
    };
    super::mount::wait_ready(receiver)?;
    Ok(host)
}
struct BridgeFs {
    ready: Option<std::sync::mpsc::Sender<()>>,
    runtime: Arc<Runtime>,
    ids: HashMap<u64, String>,
    next: u64,
}
fn errno(error: anyhow::Error) -> i32 {
    if error.is::<crate::storage::ContentLimitExceeded>() {
        return libc::ENOSPC;
    }
    let text = error.to_string().to_lowercase();
    if let Some(io) = error.downcast_ref::<std::io::Error>() {
        return io.raw_os_error().unwrap_or(libc::EIO);
    }
    if text.contains("not found") {
        libc::ENOENT
    } else if text.contains("already exists") {
        libc::EEXIST
    } else if text.contains("not empty") {
        libc::ENOTEMPTY
    } else if text.contains("is open") {
        libc::EBUSY
    } else if text.contains("not a directory") {
        libc::ENOTDIR
    } else if text.contains("is a directory") {
        libc::EISDIR
    } else {
        libc::EIO
    }
}
fn attr(n: &Node, ino: u64) -> FileAttr {
    let time = UNIX_EPOCH + Duration::from_secs(n.modified.max(0) as u64);
    FileAttr {
        ino,
        size: n.size,
        blocks: n.size.div_ceil(512),
        atime: time,
        mtime: time,
        ctime: time,
        crtime: time,
        kind: if n.directory {
            FileType::Directory
        } else {
            FileType::RegularFile
        },
        perm: if n.directory { 0o700 } else { 0o600 },
        nlink: if n.directory { 2 } else { 1 },
        uid: unsafe { libc::getuid() },
        gid: unsafe { libc::getgid() },
        rdev: 0,
        blksize: 4096,
        flags: 0,
    }
}
impl BridgeFs {
    fn inode(&mut self, id: &str) -> u64 {
        if let Some((ino, _)) = self.ids.iter().find(|(_, value)| value.as_str() == id) {
            return *ino;
        }
        let ino = self.next;
        self.next += 1;
        self.ids.insert(ino, id.into());
        ino
    }
    fn id(&self, ino: u64) -> Result<String> {
        self.ids
            .get(&ino)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("File not found"))
    }
    fn node(&self, ino: u64) -> Result<Node> {
        self.runtime.engine.lock().store.get(&self.id(ino)?)
    }
    fn path(&self, parent: u64, name: &OsStr) -> Result<String> {
        let name = name
            .to_str()
            .ok_or_else(|| anyhow::anyhow!("Invalid UTF-8 name"))?;
        ensure!(
            !name.is_empty() && !name.contains(['/', '\\']) && name != "." && name != "..",
            "Invalid filename"
        );
        let e = self.runtime.engine.lock();
        let root_id = e.root_id()?;
        let mut id = self.id(parent)?;
        let mut parts = vec![name.to_owned()];
        while id != root_id {
            let n = e.store.get(&id)?;
            parts.push(n.alias);
            id = n.parent;
        }
        parts.reverse();
        Ok(format!("/{}", parts.join("/")))
    }
    fn remove(&self, parent: u64, name: &OsStr, directory: bool) -> Result<()> {
        let n = self.runtime.resolve(&self.path(parent, name)?)?;
        if directory {
            ensure!(n.directory, "Not a directory");
        } else {
            ensure!(!n.directory, "Is a directory");
        }
        self.runtime.can_delete(&n.id)?;
        self.runtime.delete(&n.id)
    }
}
impl Filesystem for BridgeFs {
    fn statfs(&mut self, _: &Request<'_>, _: u64, reply: ReplyStatfs) {
        let engine = self.runtime.engine.lock();
        let about = engine
            .store
            .setting("about")
            .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
            .unwrap_or_default();
        let total = about["storageQuota"]["limit"]
            .as_str()
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(15 * 1024 * 1024 * 1024);
        let used = about["storageQuota"]["usage"]
            .as_str()
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(0);
        let free = total.saturating_sub(used) / 4096;
        reply.statfs(total / 4096, free, free, 0, 0, 4096, 255, 4096);
    }
    fn getxattr(&mut self, _: &Request<'_>, _: u64, _: &OsStr, _: u32, reply: ReplyXattr) {
        // Finder probes metadata even though this filesystem stores no extended attributes.
        reply.error(libc::ENOATTR);
    }
    fn listxattr(&mut self, _: &Request<'_>, _: u64, size: u32, reply: ReplyXattr) {
        if size == 0 {
            reply.size(0);
        } else {
            reply.data(&[]);
        }
    }
    fn init(&mut self, _: &Request<'_>, _: &mut KernelConfig) -> std::result::Result<(), i32> {
        if let Some(sender) = self.ready.take() {
            let _ = sender.send(());
        }
        Ok(())
    }
    fn lookup(&mut self, _: &Request<'_>, parent: u64, name: &OsStr, reply: ReplyEntry) {
        match self
            .path(parent, name)
            .and_then(|p| self.runtime.resolve(&p))
        {
            Ok(n) => {
                let ino = self.inode(&n.id);
                reply.entry(&TTL, &attr(&n, ino), 0);
            }
            Err(e) => reply.error(errno(e)),
        }
    }
    fn getattr(&mut self, _: &Request<'_>, ino: u64, _: Option<u64>, reply: ReplyAttr) {
        match self.node(ino) {
            Ok(n) => reply.attr(&TTL, &attr(&n, ino)),
            Err(e) => reply.error(errno(e)),
        }
    }
    fn getxtimes(&mut self, _: &Request<'_>, ino: u64, reply: ReplyXTimes) {
        match self.node(ino) {
            Ok(n) => reply.xtimes(UNIX_EPOCH, attr(&n, ino).crtime),
            Err(e) => reply.error(errno(e)),
        }
    }
    fn readdir(
        &mut self,
        _: &Request<'_>,
        ino: u64,
        _: u64,
        offset: i64,
        mut reply: ReplyDirectory,
    ) {
        let result = self.id(ino).and_then(|id| self.runtime.list(&id));
        match result {
            Ok(nodes) => {
                let parent = if ino == 1 {
                    1
                } else {
                    self.node(ino).map(|n| self.inode(&n.parent)).unwrap_or(1)
                };
                let mut entries = vec![
                    (ino, FileType::Directory, ".".to_owned()),
                    (parent, FileType::Directory, "..".to_owned()),
                ];
                for n in nodes {
                    entries.push((
                        self.inode(&n.id),
                        if n.directory {
                            FileType::Directory
                        } else {
                            FileType::RegularFile
                        },
                        n.alias,
                    ));
                }
                for (index, (id, kind, name)) in
                    entries.into_iter().enumerate().skip(offset.max(0) as usize)
                {
                    if reply.add(id, (index + 1) as i64, kind, name) {
                        break;
                    }
                }
                reply.ok();
            }
            Err(e) => reply.error(errno(e)),
        }
    }
    fn open(&mut self, _: &Request<'_>, ino: u64, _: i32, reply: ReplyOpen) {
        let result = self
            .id(ino)
            .and_then(|id| self.runtime.engine.lock().open_node(&id));
        match result {
            Ok(_) => reply.opened(ino, fuser::consts::FOPEN_DIRECT_IO),
            Err(e) => reply.error(errno(e)),
        }
    }
    fn release(
        &mut self,
        _: &Request<'_>,
        ino: u64,
        _: u64,
        _: i32,
        _: Option<u64>,
        _: bool,
        reply: ReplyEmpty,
    ) {
        match self.id(ino) {
            Ok(id) => {
                self.runtime.engine.lock().close_node(&id);
                reply.ok();
            }
            Err(e) => reply.error(errno(e)),
        }
    }
    fn read(
        &mut self,
        _: &Request<'_>,
        ino: u64,
        _: u64,
        offset: i64,
        size: u32,
        _: i32,
        _: Option<u64>,
        reply: ReplyData,
    ) {
        if offset < 0 {
            reply.error(libc::EINVAL);
            return;
        }
        let mut bytes = vec![0; size as usize];
        match self
            .id(ino)
            .and_then(|id| self.runtime.read(&id, offset as u64, &mut bytes))
        {
            Ok(n) => reply.data(&bytes[..n]),
            Err(e) => reply.error(errno(e)),
        }
    }
    fn write(
        &mut self,
        _: &Request<'_>,
        ino: u64,
        _: u64,
        offset: i64,
        data: &[u8],
        _: u32,
        _: i32,
        _: Option<u64>,
        reply: ReplyWrite,
    ) {
        if offset < 0 {
            reply.error(libc::EINVAL);
            return;
        }
        match self
            .id(ino)
            .and_then(|id| self.runtime.write(&id, offset as u64, data, false, false))
        {
            Ok(n) => reply.written(n as u32),
            Err(e) => reply.error(errno(e)),
        }
    }
    fn flush(&mut self, _: &Request<'_>, ino: u64, _: u64, _: u64, reply: ReplyEmpty) {
        match self
            .id(ino)
            .and_then(|id| self.runtime.engine.lock().flush(&id))
        {
            Ok(()) => reply.ok(),
            Err(e) => reply.error(errno(e)),
        }
    }
    fn fsync(&mut self, req: &Request<'_>, ino: u64, fh: u64, _: bool, reply: ReplyEmpty) {
        self.flush(req, ino, fh, 0, reply);
    }
    fn create(
        &mut self,
        _: &Request<'_>,
        parent: u64,
        name: &OsStr,
        _: u32,
        _: u32,
        _: i32,
        reply: ReplyCreate,
    ) {
        match self
            .path(parent, name)
            .and_then(|p| self.runtime.create(&p, false))
        {
            Ok(n) => {
                let ino = self.inode(&n.id);
                reply.created(&TTL, &attr(&n, ino), 0, ino, fuser::consts::FOPEN_DIRECT_IO);
            }
            Err(e) => reply.error(errno(e)),
        }
    }
    fn mkdir(
        &mut self,
        _: &Request<'_>,
        parent: u64,
        name: &OsStr,
        _: u32,
        _: u32,
        reply: ReplyEntry,
    ) {
        match self
            .path(parent, name)
            .and_then(|p| self.runtime.create(&p, true))
        {
            Ok(n) => {
                self.runtime.engine.lock().close_node(&n.id);
                let ino = self.inode(&n.id);
                reply.entry(&TTL, &attr(&n, ino), 0);
            }
            Err(e) => reply.error(errno(e)),
        }
    }
    fn unlink(&mut self, _: &Request<'_>, parent: u64, name: &OsStr, reply: ReplyEmpty) {
        match self.remove(parent, name, false) {
            Ok(()) => reply.ok(),
            Err(e) => reply.error(errno(e)),
        }
    }
    fn rmdir(&mut self, _: &Request<'_>, parent: u64, name: &OsStr, reply: ReplyEmpty) {
        match self.remove(parent, name, true) {
            Ok(()) => reply.ok(),
            Err(e) => reply.error(errno(e)),
        }
    }
    fn rename(
        &mut self,
        _: &Request<'_>,
        parent: u64,
        name: &OsStr,
        newparent: u64,
        newname: &OsStr,
        flags: u32,
        reply: ReplyEmpty,
    ) {
        if flags != 0 {
            reply.error(libc::EINVAL);
            return;
        }
        let result = (|| {
            let n = self.runtime.resolve(&self.path(parent, name)?)?;
            self.runtime
                .rename(&n.id, &self.path(newparent, newname)?, true)
        })();
        match result {
            Ok(()) => reply.ok(),
            Err(e) => reply.error(errno(e)),
        }
    }
    fn setattr(
        &mut self,
        _: &Request<'_>,
        ino: u64,
        mode: Option<u32>,
        uid: Option<u32>,
        gid: Option<u32>,
        size: Option<u64>,
        _: Option<TimeOrNow>,
        mtime: Option<TimeOrNow>,
        _: Option<SystemTime>,
        _: Option<u64>,
        _: Option<SystemTime>,
        _: Option<SystemTime>,
        _: Option<SystemTime>,
        flags: Option<u32>,
        reply: ReplyAttr,
    ) {
        if uid.is_some_and(|v| v != unsafe { libc::getuid() })
            || gid.is_some_and(|v| v != unsafe { libc::getgid() })
            || flags.is_some_and(|v| v != 0)
            || mode.is_some_and(|v| v & 0o7000 != 0)
        {
            reply.error(libc::EPERM);
            return;
        }
        let result = (|| -> Result<Node> {
            let id = self.id(ino)?;
            if let Some(size) = size {
                self.runtime.resize(&id, size)?;
            }
            if let Some(time) = mtime {
                let gate = self.runtime.gate(&id);
                let _guard = gate.lock();
                let e = self.runtime.engine.lock();
                let mut n = e.store.get(&id)?;
                n.modified = match time {
                    TimeOrNow::Now => SystemTime::now(),
                    TimeOrNow::SpecificTime(t) => t,
                }
                .duration_since(UNIX_EPOCH)?
                .as_secs() as i64;
                e.store.put(&n)?;
            }
            self.node(ino)
        })();
        match result {
            Ok(n) => reply.attr(&TTL, &attr(&n, ino)),
            Err(e) => reply.error(errno(e)),
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn inode_paths_follow_parent_renames() -> Result<()> {
        use crate::{engine::Engine, local::LocalRemote};
        use parking_lot::Mutex;
        let tmp = tempfile::tempdir()?;
        let remote = Arc::new(LocalRemote::new(tmp.path().join("remote"))?);
        let mut engine = Engine::new(tmp.path().join("engine"), remote)?;
        engine.initialize()?;
        let root = engine.root_id()?;
        let runtime = Arc::new(Runtime::new(Arc::new(Mutex::new(engine))));
        let mut fs = BridgeFs {
            runtime,
            ids: HashMap::from([(1, root)]),
            next: 2,
            ready: None,
        };
        let folder = fs.runtime.create("/folder", true)?;
        fs.runtime.engine.lock().close_node(&folder.id);
        let ino = fs.inode(&folder.id);
        assert_eq!(fs.inode(&folder.id), ino);
        assert_eq!(fs.path(ino, OsStr::new("file.txt"))?, "/folder/file.txt");
        fs.runtime.rename(&folder.id, "/renamed", false)?;
        assert_eq!(fs.path(ino, OsStr::new("file.txt"))?, "/renamed/file.txt");
        for name in ["..", ".", "a/b", "a\\b", ""] {
            assert!(fs.path(ino, OsStr::new(name)).is_err());
        }
        Ok(())
    }
    #[test]
    fn errors_match_posix() {
        assert_eq!(errno(anyhow::anyhow!("File not found")), libc::ENOENT);
        assert_eq!(
            errno(anyhow::anyhow!("Directory not empty")),
            libc::ENOTEMPTY
        );
    }
}
