use anyhow::{Result, ensure};
use std::{
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};
use storage_bridge::{
    drive::{ChangeBatch, Remote},
    engine::Engine,
    local::LocalRemote,
    store::Node,
};

struct Gate {
    inner: Arc<LocalRemote>,
    offline: AtomicBool,
    partial: AtomicBool,
}

#[test]
fn online_only_keeps_failed_uploads_through_restart() {
    let (tmp, remote, mut e) = setup();
    let n = e.create("pending", false).unwrap();
    e.write(&n.id, 0, b"do not lose", false, false).unwrap();
    e.set_online_only(true).unwrap();
    remote.offline.store(true, Ordering::SeqCst);
    e.tick().unwrap();
    assert!(e.cache(&n.id).exists());
    assert_eq!(e.status().unwrap()["pending"], 1);
    drop(e);
    let mut e = Engine::new(tmp.path().join("engine"), remote.clone()).unwrap();
    assert!(e.online_only());
    assert!(e.cache(&n.id).exists());
    remote.offline.store(false, Ordering::SeqCst);
    e.retry().unwrap();
    e.tick().unwrap();
    assert_eq!(
        std::fs::read(tmp.path().join("remote").join(&n.id)).unwrap(),
        b"do not lose"
    );
    assert!(!e.cache(&n.id).exists());
}
impl Gate {
    fn check(&self) -> Result<()> {
        ensure!(
            !self.offline.load(Ordering::SeqCst),
            "Simulated network disconnection"
        );
        Ok(())
    }
}
impl Remote for Gate {
    fn list(&self, p: &str) -> Result<Vec<Node>> {
        self.check()?;
        self.inner.list(p)
    }
    fn get(&self, id: &str) -> Result<Option<Node>> {
        self.check()?;
        self.inner.get(id)
    }
    fn generate_id(&self) -> Result<String> {
        self.check()?;
        self.inner.generate_id()
    }
    fn download(&self, id: &str, p: &Path) -> Result<()> {
        self.check()?;
        if self.partial.load(Ordering::SeqCst) {
            std::fs::write(p, b"partial")?;
            anyhow::bail!("Interrupted download");
        }
        self.inner.download(id, p)
    }
    fn upload(&self, n: &Node, p: &Path, exists: bool) -> Result<Node> {
        self.check()?;
        self.inner.upload(n, p, exists)
    }
    fn mkdir(&self, n: &Node) -> Result<Node> {
        self.check()?;
        self.inner.mkdir(n)
    }
    fn relocate(&self, n: &Node, p: &str) -> Result<Node> {
        self.check()?;
        self.inner.relocate(n, p)
    }
    fn trash(&self, id: &str) -> Result<()> {
        self.check()?;
        self.inner.trash(id)
    }
    fn cursor(&self) -> Result<String> {
        self.check()?;
        self.inner.cursor()
    }
    fn changes(&self, c: &str) -> Result<ChangeBatch> {
        self.check()?;
        self.inner.changes(c)
    }
    fn about(&self) -> Result<serde_json::Value> {
        self.check()?;
        self.inner.about()
    }
}
fn setup() -> (tempfile::TempDir, Arc<Gate>, Engine) {
    let tmp = tempfile::tempdir().unwrap();
    let remote = Arc::new(Gate {
        inner: Arc::new(LocalRemote::new(tmp.path().join("remote")).unwrap()),
        offline: AtomicBool::new(false),
        partial: AtomicBool::new(false),
    });
    let mut e = Engine::new(tmp.path().join("engine"), remote.clone()).unwrap();
    e.initialize().unwrap();
    (tmp, remote, e)
}
#[test]
fn cached_edits_survive_offline_restart_and_retry() {
    let (tmp, remote, mut e) = setup();
    let n = e.create("cached.txt", false).unwrap();
    e.write(&n.id, 0, b"online", false, false).unwrap();
    e.tick().unwrap();
    remote.offline.store(true, Ordering::SeqCst);
    e.write(&n.id, 0, b"offline", false, false).unwrap();
    e.tick().unwrap();
    assert_eq!(e.status().unwrap()["pending"], 1);
    assert_eq!(e.status().unwrap()["transfers"][0]["attempts"], 1);
    drop(e);
    let mut e = Engine::new(tmp.path().join("engine"), remote.clone()).unwrap();
    let mut buffer = [0; 7];
    e.read(&n.id, 0, &mut buffer).unwrap();
    assert_eq!(&buffer, b"offline");
    remote.offline.store(false, Ordering::SeqCst);
    e.retry().unwrap();
    e.tick().unwrap();
    assert_eq!(e.status().unwrap()["pending"], 0);
    assert_eq!(
        std::fs::read(tmp.path().join("remote").join(&n.id)).unwrap(),
        b"offline"
    );
}
#[test]
fn uncached_offline_reads_fail_without_marking_content_complete() {
    let (_tmp, remote, mut e) = setup();
    let n = e.create("a", false).unwrap();
    e.write(&n.id, 0, b"remote", false, false).unwrap();
    e.tick().unwrap();
    e.store.set("cache_budget", "0").unwrap();
    e.evict().unwrap();
    remote.offline.store(true, Ordering::SeqCst);
    assert!(
        e.read(&n.id, 0, &mut [0; 6])
            .unwrap_err()
            .to_string()
            .contains("offline")
    );
    assert!(!e.store.get(&n.id).unwrap().cached);
    assert!(!e.cache(&n.id).exists());
}
#[test]
fn interrupted_download_is_removed_and_retry_downloads_full_content() {
    let (_tmp, remote, mut e) = setup();
    let n = e.create("a", false).unwrap();
    e.write(&n.id, 0, b"complete file", false, false).unwrap();
    e.tick().unwrap();
    e.store.set("cache_budget", "0").unwrap();
    e.evict().unwrap();
    remote.partial.store(true, Ordering::SeqCst);
    assert!(e.read(&n.id, 0, &mut [0; 13]).is_err());
    assert!(!e.store.get(&n.id).unwrap().cached);
    assert!(!e.cache(&n.id).with_extension("download").exists());
    remote.partial.store(false, Ordering::SeqCst);
    let mut buffer = [0; 13];
    e.read(&n.id, 0, &mut buffer).unwrap();
    assert_eq!(&buffer, b"complete file");
}
#[test]
fn replacement_is_retained_until_new_content_uploads() {
    let (_tmp, remote, mut e) = setup();
    let old = e.create("document.txt", false).unwrap();
    e.write(&old.id, 0, b"old", false, false).unwrap();
    e.tick().unwrap();
    let new = e.create("temp", false).unwrap();
    e.write(&new.id, 0, b"new", false, false).unwrap();
    e.rename(&new.id, "document.txt", true).unwrap();
    remote.offline.store(true, Ordering::SeqCst);
    e.tick().unwrap();
    assert!(remote.inner.get(&old.id).unwrap().is_some());
    assert!(e.cache(&new.id).exists());
    remote.offline.store(false, Ordering::SeqCst);
    e.retry().unwrap();
    e.tick().unwrap();
    assert!(remote.inner.get(&old.id).unwrap().is_none());
    assert!(remote.inner.get(&new.id).unwrap().is_some());
}
#[test]
fn nested_creation_and_repeated_replacement_survive_restart() {
    let (tmp, remote, mut e) = setup();
    let folder = e.create("folder", true).unwrap();
    let a = e.create("folder\\doc", false).unwrap();
    e.write(&a.id, 0, b"one", false, false).unwrap();
    let b = e.create("folder\\tmp1", false).unwrap();
    e.write(&b.id, 0, b"two", false, false).unwrap();
    e.rename(&b.id, "folder\\doc", true).unwrap();
    let c = e.create("folder\\tmp2", false).unwrap();
    e.write(&c.id, 0, b"three", false, false).unwrap();
    e.rename(&c.id, "folder\\doc", true).unwrap();
    drop(e);
    let mut e = Engine::new(tmp.path().join("engine"), remote.clone()).unwrap();
    e.tick().unwrap();
    assert_eq!(e.status().unwrap()["pending"], 0);
    let nodes = remote.inner.list(&folder.id).unwrap();
    assert_eq!(nodes.len(), 1);
    assert_eq!(nodes[0].id, c.id);
    assert_eq!(nodes[0].name, "doc");
}
#[test]
fn missing_dirty_content_is_reported_and_never_acknowledged() {
    let (_tmp, _remote, mut e) = setup();
    let n = e.create("pending", false).unwrap();
    std::fs::remove_file(e.cache(&n.id)).unwrap();
    e.tick().unwrap();
    assert_eq!(e.status().unwrap()["pending"], 1);
    assert!(
        e.status().unwrap()["transfers"][0]["error"]
            .as_str()
            .unwrap()
            .contains("missing")
    );
}
