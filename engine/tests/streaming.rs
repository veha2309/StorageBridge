use anyhow::Result;
use parking_lot::{Condvar, Mutex};
use std::{
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
        mpsc,
    },
    time::Duration,
};
use storage_bridge::{
    drive::{ChangeBatch, Remote},
    engine::Engine,
    local::LocalRemote,
    runtime::Runtime,
    store::Node,
    streaming::{BLOCK, ContentSource, RAM_BUDGET},
};

struct Observed {
    inner: LocalRemote,
    ranges: AtomicUsize,
    listings: AtomicUsize,
    full: AtomicUsize,
    pattern: AtomicBool,
    block_read: AtomicBool,
    block_upload: AtomicBool,
    started: Mutex<Option<mpsc::Sender<()>>>,
    released: Mutex<bool>,
    wake: Condvar,
}
impl Observed {
    fn block(&self) {
        if let Some(tx) = self.started.lock().as_ref() {
            let _ = tx.send(());
        }
        let mut ready = self.released.lock();
        while !*ready {
            self.wake.wait(&mut ready);
        }
    }
    fn release(&self) {
        *self.released.lock() = true;
        self.wake.notify_all();
    }
    fn arm(&self) -> mpsc::Receiver<()> {
        let (tx, rx) = mpsc::channel();
        *self.started.lock() = Some(tx);
        *self.released.lock() = false;
        rx
    }
}
impl Remote for Observed {
    fn list(&self, p: &str) -> Result<Vec<Node>> {
        self.listings.fetch_add(1, Ordering::SeqCst);
        self.inner.list(p)
    }
    fn get(&self, id: &str) -> Result<Option<Node>> {
        let mut n = self.inner.get(id)?;
        if self.pattern.load(Ordering::SeqCst)
            && let Some(n) = &mut n
            && !n.directory
        {
            n.size = 8 * 1024 * 1024 * 1024;
        }
        Ok(n)
    }
    fn generate_id(&self) -> Result<String> {
        self.inner.generate_id()
    }
    fn download(&self, id: &str, path: &Path) -> Result<()> {
        self.full.fetch_add(1, Ordering::SeqCst);
        self.inner.download(id, path)
    }
    fn read_range(&self, n: &Node, offset: u64, length: usize) -> Result<Vec<u8>> {
        self.ranges.fetch_add(1, Ordering::SeqCst);
        assert!(length <= BLOCK);
        if self.block_read.load(Ordering::SeqCst) {
            self.block();
        }
        if self.pattern.load(Ordering::SeqCst) {
            return Ok((0..length)
                .map(|i| ((offset + i as u64) % 251) as u8)
                .collect());
        }
        self.inner.read_range(n, offset, length)
    }
    fn upload_source(&self, n: &Node, source: &dyn ContentSource, exists: bool) -> Result<Node> {
        if self.block_upload.load(Ordering::SeqCst) {
            self.block();
        }
        self.inner.upload_source(n, source, exists)
    }
    fn mkdir(&self, n: &Node) -> Result<Node> {
        self.inner.mkdir(n)
    }
    fn relocate(&self, n: &Node, p: &str) -> Result<Node> {
        self.inner.relocate(n, p)
    }
    fn trash(&self, id: &str) -> Result<()> {
        self.inner.trash(id)
    }
    fn cursor(&self) -> Result<String> {
        self.inner.cursor()
    }
    fn changes(&self, c: &str) -> Result<ChangeBatch> {
        self.inner.changes(c)
    }
    fn about(&self) -> Result<serde_json::Value> {
        self.inner.about()
    }
}
fn setup() -> (tempfile::TempDir, Arc<Observed>, Arc<Runtime>, String) {
    let tmp = tempfile::tempdir().unwrap();
    let remote = Arc::new(Observed {
        inner: LocalRemote::new(tmp.path().join("remote")).unwrap(),
        ranges: AtomicUsize::new(0),
        listings: AtomicUsize::new(0),
        full: AtomicUsize::new(0),
        pattern: AtomicBool::new(false),
        block_read: AtomicBool::new(false),
        block_upload: AtomicBool::new(false),
        started: Mutex::new(None),
        released: Mutex::new(false),
        wake: Condvar::new(),
    });
    let mut e = Engine::new(tmp.path().join("engine"), remote.clone()).unwrap();
    e.initialize().unwrap();
    let n = e.create("file.txt", false).unwrap();
    e.write(&n.id, 0, b"abcdefghij", false, false).unwrap();
    e.tick().unwrap();
    e.set_online_only(true).unwrap();
    let rt = Arc::new(Runtime::new(Arc::new(Mutex::new(e))));
    (tmp, remote, rt, n.id)
}
#[test]
fn stale_known_folders_are_returned_without_waiting_for_network() {
    let (_tmp, remote, rt, _id) = setup();
    rt.engine.lock().store.set("listed:root", "1").unwrap();
    let before = remote.listings.load(Ordering::SeqCst);
    assert_eq!(rt.list("root").unwrap().len(), 1);
    assert_eq!(remote.listings.load(Ordering::SeqCst), before);
    rt.poll().unwrap();
    assert!(remote.listings.load(Ordering::SeqCst) > before);
}
#[test]
fn restored_cloud_file_reappears_in_cached_directory() {
    let (tmp, remote, rt, id) = setup();
    let original = remote.inner.get(&id).unwrap().unwrap();
    remote.inner.trash(&id).unwrap();
    rt.poll().unwrap();
    assert!(rt.resolve("file.txt").is_err());
    let path = tmp.path().join("restore");
    std::fs::write(&path, b"abcdefghij").unwrap();
    remote.inner.upload(&original, &path, false).unwrap();
    rt.poll().unwrap();
    assert_eq!(rt.resolve("file.txt").unwrap().id, id);
}
#[test]
fn first_read_refreshes_cloud_version_even_when_change_feed_is_late() {
    let (_tmp, remote, rt, id) = setup();
    let node = remote.inner.get(&id).unwrap().unwrap();
    remote.inner.relocate(&node, "root").unwrap();
    let mut bytes = [0; 10];
    rt.read(&id, 0, &mut bytes).unwrap();
    assert_eq!(&bytes, b"abcdefghij");
    assert_eq!(
        rt.engine.lock().store.get(&id).unwrap().version,
        remote.inner.get(&id).unwrap().unwrap().version
    );
}
#[test]
fn write_only_open_refreshes_base_version_before_journaling() {
    let (tmp, remote, rt, id) = setup();
    let node = remote.inner.get(&id).unwrap().unwrap();
    remote.inner.relocate(&node, "root").unwrap();
    rt.write(&id, 1, b"X", false, false).unwrap();
    rt.tick().unwrap();
    assert_eq!(rt.engine.lock().status().unwrap()["pending"], 0);
    assert_eq!(
        std::fs::read(tmp.path().join("remote").join(&id)).unwrap(),
        b"aXcdefghij"
    );
}
#[test]
fn pinned_subsequent_reads_do_not_hold_the_metadata_mutex() {
    let (_tmp, remote, rt, id) = setup();
    remote.pattern.store(true, Ordering::SeqCst);
    {
        let e = rt.engine.lock();
        let mut n = e.store.get(&id).unwrap();
        n.size = 8 * 1024 * 1024 * 1024;
        e.store.put(&n).unwrap();
    }
    rt.open("file.txt").unwrap();
    rt.read(&id, 0, &mut [0; 16]).unwrap();
    let started = remote.arm();
    remote.block_read.store(true, Ordering::SeqCst);
    let task = rt.clone();
    let key = id.clone();
    let handle = std::thread::spawn(move || task.read(&key, BLOCK as u64 + 4, &mut [0; 16]));
    started.recv_timeout(Duration::from_secs(3)).unwrap();
    let free = rt.engine.try_lock_for(Duration::from_secs(1)).is_some();
    remote.release();
    handle.join().unwrap().unwrap();
    rt.engine.lock().close_node(&id);
    assert!(free);
}
#[test]
fn random_reads_of_eight_gib_file_use_only_bounded_ram() {
    let (_tmp, remote, rt, id) = setup();
    remote.pattern.store(true, Ordering::SeqCst);
    {
        let e = rt.engine.lock();
        let mut n = e.store.get(&id).unwrap();
        n.size = 8 * 1024 * 1024 * 1024;
        e.store.put(&n).unwrap();
    }
    let offset = 7 * 1024 * 1024 * 1024 + 17;
    let mut bytes = [0; 64];
    rt.read(&id, offset, &mut bytes).unwrap();
    assert_eq!(bytes[0], (offset % 251) as u8);
    rt.read(&id, offset, &mut bytes).unwrap();
    assert_eq!(remote.ranges.load(Ordering::SeqCst), 1);
    for i in 0..140 {
        rt.read(&id, i * BLOCK as u64, &mut bytes).unwrap();
    }
    let e = rt.engine.lock();
    assert!(e.read_cache.bytes() <= RAM_BUDGET);
    assert!(!e.cache(&id).exists());
    assert_eq!(remote.full.load(Ordering::SeqCst), 0);
    drop(e);
    assert_eq!(rt.read(&id, 8 * 1024 * 1024 * 1024, &mut []).unwrap(), 0);
}
#[test]
fn small_edits_stage_only_changed_bytes_and_stream_upload() {
    let (tmp, remote, rt, id) = setup();
    rt.write(&id, 3, b"XYZ", false, false).unwrap();
    assert_eq!(
        std::fs::metadata(rt.engine.lock().cache(&id))
            .unwrap()
            .len(),
        3
    );
    let mut bytes = [0; 10];
    rt.read(&id, 0, &mut bytes).unwrap();
    assert_eq!(&bytes, b"abcXYZghij");
    rt.tick().unwrap();
    assert_eq!(
        std::fs::read(tmp.path().join("remote").join(&id)).unwrap(),
        b"abcXYZghij"
    );
    assert!(!rt.engine.lock().cache(&id).exists());
    assert_eq!(remote.full.load(Ordering::SeqCst), 0);
}
#[test]
fn overwrite_and_zero_extension_do_not_fetch_old_content() {
    let (tmp, remote, rt, id) = setup();
    rt.resize(&id, 0).unwrap();
    rt.resize(&id, 12).unwrap();
    rt.write(&id, 4, b"new", false, false).unwrap();
    let mut bytes = [0; 12];
    rt.read(&id, 0, &mut bytes).unwrap();
    assert_eq!(&bytes, b"\0\0\0\0new\0\0\0\0\0");
    assert_eq!(remote.ranges.load(Ordering::SeqCst), 0);
    rt.tick().unwrap();
    assert_eq!(
        std::fs::read(tmp.path().join("remote").join(&id)).unwrap(),
        bytes
    );
}
#[test]
fn overlapping_patches_and_truncation_survive_restart() {
    let (tmp, remote, rt, id) = setup();
    rt.write(&id, 2, b"123456", false, false).unwrap();
    rt.write(&id, 4, b"XX", false, false).unwrap();
    rt.resize(&id, 9).unwrap();
    rt.write(&id, 11, b"!", false, false).unwrap();
    drop(rt);
    let e = Engine::new(tmp.path().join("engine"), remote.clone()).unwrap();
    assert_eq!(e.store.get(&id).unwrap().size, 12);
    let rt = Runtime::new(Arc::new(Mutex::new(e)));
    let mut bytes = [0; 12];
    rt.read(&id, 0, &mut bytes).unwrap();
    assert_eq!(&bytes, b"ab12XX56i\0\0!");
    rt.tick().unwrap();
    assert_eq!(
        std::fs::read(tmp.path().join("remote").join(&id)).unwrap(),
        bytes
    );
}
#[test]
fn metadata_mutex_is_available_during_slow_cloud_read() {
    let (_tmp, remote, rt, id) = setup();
    let started = remote.arm();
    remote.block_read.store(true, Ordering::SeqCst);
    let task = rt.clone();
    let handle = std::thread::spawn(move || task.read(&id, 0, &mut [0; 2]));
    started.recv_timeout(Duration::from_secs(3)).unwrap();
    let free = rt.engine.try_lock_for(Duration::from_secs(1)).is_some();
    remote.release();
    handle.join().unwrap().unwrap();
    assert!(free, "Cloud read held the metadata mutex");
}
#[test]
fn metadata_mutex_is_available_during_slow_upload() {
    let (_tmp, remote, rt, id) = setup();
    rt.write(&id, 0, b"edit", false, false).unwrap();
    let started = remote.arm();
    remote.block_upload.store(true, Ordering::SeqCst);
    let task = rt.clone();
    let handle = std::thread::spawn(move || task.tick());
    started.recv_timeout(Duration::from_secs(3)).unwrap();
    let free = rt.engine.try_lock_for(Duration::from_secs(1)).is_some();
    remote.release();
    handle.join().unwrap().unwrap();
    assert!(free, "Upload held the metadata mutex");
    assert_eq!(rt.engine.lock().status().unwrap()["pending"], 0);
}
#[test]
fn remote_conflicts_retain_patch_journal_without_overwriting_cloud() {
    let (tmp, remote, rt, id) = setup();
    rt.write(&id, 1, b"local", false, false).unwrap();
    let mut changed = remote.inner.get(&id).unwrap().unwrap();
    changed.size = 10;
    let p = tmp.path().join("other");
    std::fs::write(&p, b"REMOTE!!!!").unwrap();
    remote.inner.upload(&changed, &p, true).unwrap();
    rt.tick().unwrap();
    let e = rt.engine.lock();
    assert_eq!(e.status().unwrap()["pending"], 1);
    assert!(
        e.status().unwrap()["transfers"][0]["error"]
            .as_str()
            .unwrap()
            .contains("Partial edits")
    );
    assert!(e.cache(&id).exists());
    assert_eq!(
        std::fs::read(tmp.path().join("remote").join(&id)).unwrap(),
        b"REMOTE!!!!"
    );
}
