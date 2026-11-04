use anyhow::Result;
use parking_lot::Mutex;
use std::{
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
};
use storage_bridge::{
    drive::{ChangeBatch, Remote},
    engine::Engine,
    imports::Import,
    local::LocalRemote,
    runtime::Runtime,
    source::Source,
    store::Node,
    streaming::{BLOCK, ContentSource, RAM_BUDGET},
};

struct Observed {
    inner: LocalRemote,
    checksum: AtomicUsize, // 0 valid, 1 missing, 2 incorrect
    fail_after_ack: AtomicBool,
    uploads: AtomicUsize,
    bytes: AtomicUsize,
    virtual_large: AtomicBool,
    change_after_range: AtomicBool,
    changed: AtomicBool,
}
impl Remote for Observed {
    fn list(&self, p: &str) -> Result<Vec<Node>> {
        self.inner.list(p)
    }
    fn get(&self, id: &str) -> Result<Option<Node>> {
        let mut n = self.inner.get(id)?;
        if let Some(n) = &mut n
            && !n.directory
        {
            if self.virtual_large.load(Ordering::SeqCst) {
                n.size = 3 * 1024 * 1024 * 1024;
            }
            if self.changed.load(Ordering::SeqCst) {
                n.version = "changed".into();
            }
        }
        Ok(n)
    }
    fn generate_id(&self) -> Result<String> {
        self.inner.generate_id()
    }
    fn download(&self, _: &str, _: &Path) -> Result<()> {
        panic!("Whole database download is forbidden")
    }
    fn sha256(&self, n: &Node) -> Result<Option<String>> {
        match self.checksum.load(Ordering::SeqCst) {
            1 => Ok(None),
            2 => Ok(Some("incorrect".into())),
            _ => self.inner.sha256(n),
        }
    }
    fn upload_source(&self, n: &Node, source: &dyn ContentSource, exists: bool) -> Result<Node> {
        self.uploads.fetch_add(1, Ordering::SeqCst);
        let result = self.inner.upload_source(n, source, exists)?;
        if self.fail_after_ack.swap(false, Ordering::SeqCst) {
            anyhow::bail!("Simulated loss of final acknowledgement");
        }
        Ok(result)
    }
    fn read_range(&self, n: &Node, offset: u64, length: usize) -> Result<Vec<u8>> {
        assert!(length <= BLOCK);
        self.bytes.fetch_add(length, Ordering::SeqCst);
        let actual = self.inner.get(&n.id)?.unwrap();
        let available = actual.size.saturating_sub(offset).min(length as u64) as usize;
        let mut bytes = if available > 0 {
            self.inner.read_range(&actual, offset, available)?
        } else {
            vec![]
        };
        bytes.resize(length, 0);
        if self.change_after_range.load(Ordering::SeqCst) {
            self.changed.store(true, Ordering::SeqCst);
        }
        Ok(bytes)
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
fn setup() -> (tempfile::TempDir, Arc<Observed>, Arc<Runtime>) {
    let tmp = tempfile::tempdir().unwrap();
    let remote = Arc::new(Observed {
        inner: LocalRemote::new(tmp.path().join("remote")).unwrap(),
        checksum: AtomicUsize::new(0),
        fail_after_ack: AtomicBool::new(false),
        uploads: AtomicUsize::new(0),
        bytes: AtomicUsize::new(0),
        virtual_large: AtomicBool::new(false),
        change_after_range: AtomicBool::new(false),
        changed: AtomicBool::new(false),
    });
    let mut engine = Engine::new(tmp.path().join("engine"), remote.clone()).unwrap();
    engine.initialize().unwrap();
    engine.set_online_only(true).unwrap();
    engine.set_content_limit(1024 * 1024).unwrap();
    (
        tmp,
        remote,
        Arc::new(Runtime::new(Arc::new(Mutex::new(engine)))),
    )
}
fn job(rt: &Runtime, id: &str) -> Import {
    Import::all(&rt.engine.lock().store)
        .unwrap()
        .into_iter()
        .find(|j| j.id == id)
        .unwrap()
}
fn assert_no_staging(rt: &Runtime) {
    assert_eq!(
        rt.engine.lock().status().unwrap()["storage"]["used_bytes"],
        0
    );
    assert_eq!(
        std::fs::read_dir(rt.engine.lock().root.join("cache"))
            .unwrap()
            .count(),
        0
    );
}
#[test]
fn direct_upload_larger_than_allowance_never_stages_a_second_copy() {
    let (tmp, remote, rt) = setup();
    let source = tmp.path().join("larger-than-budget.bin");
    std::fs::write(&source, vec![42; 8 * 1024 * 1024]).unwrap();
    let id = rt.queue_import(&source, "", false).unwrap();
    assert_eq!(rt.engine.lock().status().unwrap()["pending"], 1);
    assert_no_staging(&rt);
    rt.tick_imports().unwrap();
    assert_eq!(job(&rt, &id).state, "complete");
    assert!(source.exists());
    assert_eq!(remote.uploads.load(Ordering::SeqCst), 1);
    assert_no_staging(&rt);
    assert_eq!(rt.engine.lock().status().unwrap()["pending"], 0);
}
#[test]
fn missing_or_wrong_checksum_keeps_original_and_retries_only_verification() {
    for checksum in [1, 2] {
        let (tmp, remote, rt) = setup();
        let source = tmp.path().join("keep.bin");
        std::fs::write(&source, b"irreplaceable").unwrap();
        remote.checksum.store(checksum, Ordering::SeqCst);
        let id = rt.queue_import(&source, "", cfg!(windows)).unwrap();
        rt.tick_imports().unwrap();
        assert_eq!(job(&rt, &id).state, "error");
        assert!(source.exists());
        assert_no_staging(&rt);
        remote.checksum.store(0, Ordering::SeqCst);
        rt.engine.lock().retry().unwrap();
        rt.tick_imports().unwrap();
        assert_eq!(job(&rt, &id).state, "complete");
        assert_eq!(source.exists(), !cfg!(windows));
        assert_eq!(remote.uploads.load(Ordering::SeqCst), 1);
    }
}
#[test]
fn restart_after_lost_upload_ack_verifies_existing_remote_without_duplicate() {
    let (tmp, remote, rt) = setup();
    let source = tmp.path().join("resume.bin");
    std::fs::write(&source, b"survive restart").unwrap();
    remote.fail_after_ack.store(true, Ordering::SeqCst);
    let id = rt.queue_import(&source, "", false).unwrap();
    rt.tick_imports().unwrap();
    assert_eq!(job(&rt, &id).state, "error");
    assert!(source.exists());
    drop(rt);
    let engine = Engine::new(tmp.path().join("engine"), remote.clone()).unwrap();
    engine.retry().unwrap();
    let rt = Runtime::new(Arc::new(Mutex::new(engine)));
    rt.tick_imports().unwrap();
    assert_eq!(job(&rt, &id).state, "complete");
    assert_eq!(remote.uploads.load(Ordering::SeqCst), 1);
    assert_no_staging(&rt);
}
#[test]
fn changed_source_and_duplicate_destination_are_not_overwritten_or_removed() {
    let (tmp, remote, rt) = setup();
    let source = tmp.path().join("changed.bin");
    std::fs::write(&source, b"old").unwrap();
    let id = rt.queue_import(&source, "", cfg!(windows)).unwrap();
    assert!(rt.queue_import(&source, "", false).is_err());
    std::fs::write(&source, b"a replacement").unwrap();
    rt.tick_imports().unwrap();
    assert_eq!(job(&rt, &id).state, "error");
    assert_eq!(remote.uploads.load(Ordering::SeqCst), 0);
    assert_eq!(std::fs::read(&source).unwrap(), b"a replacement");
    rt.dismiss_import(&id).unwrap();
    let next = rt.queue_import(&source, "", false).unwrap();
    rt.tick_imports().unwrap();
    assert_eq!(job(&rt, &next).state, "complete");
    assert!(rt.queue_import(&source, "", false).is_err());
}
#[cfg(windows)]
#[test]
fn windows_lease_blocks_other_writers_and_deleters() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("leased");
    std::fs::write(&path, b"leased").unwrap();
    let lease = Source::open(&path, true).unwrap();
    assert!(std::fs::OpenOptions::new().write(true).open(&path).is_err());
    assert!(std::fs::remove_file(&path).is_err());
    assert!(std::fs::rename(&path, tmp.path().join("renamed")).is_err());
    drop(lease);
    assert!(path.exists());
}
#[cfg(windows)]
#[test]
fn replaced_original_after_lost_ack_is_retained_after_cloud_verification() {
    let (tmp, remote, rt) = setup();
    let source = tmp.path().join("replaced.bin");
    std::fs::write(&source, b"uploaded version").unwrap();
    remote.fail_after_ack.store(true, Ordering::SeqCst);
    let id = rt.queue_import(&source, "", true).unwrap();
    rt.tick_imports().unwrap();
    std::fs::remove_file(&source).unwrap();
    std::fs::write(&source, b"new local content").unwrap();
    rt.engine.lock().retry().unwrap();
    rt.tick_imports().unwrap();
    assert_eq!(job(&rt, &id).state, "source_retained");
    assert_eq!(std::fs::read(&source).unwrap(), b"new local content");
    assert_eq!(remote.uploads.load(Ordering::SeqCst), 1);
}
#[cfg(windows)]
#[test]
fn recovery_after_verified_source_removal_does_not_reupload() {
    let (tmp, remote, rt) = setup();
    let source = tmp.path().join("removed.bin");
    std::fs::write(&source, b"uploaded").unwrap();
    remote.fail_after_ack.store(true, Ordering::SeqCst);
    let id = rt.queue_import(&source, "", true).unwrap();
    rt.tick_imports().unwrap();
    let mut saved = job(&rt, &id);
    let uploaded = remote
        .get(saved.remote_id.as_deref().unwrap())
        .unwrap()
        .unwrap();
    assert_eq!(remote.sha256(&uploaded).unwrap(), saved.sha256);
    // Simulate the durable deletion marker and file removal before the final complete commit.
    saved.deletion_started = true;
    saved.state = "deleting".into();
    saved.save(&rt.engine.lock().store).unwrap();
    std::fs::remove_file(&source).unwrap();
    drop(rt);
    let engine = Engine::new(tmp.path().join("engine"), remote.clone()).unwrap();
    engine.retry().unwrap();
    let rt = Runtime::new(Arc::new(Mutex::new(engine)));
    rt.tick_imports().unwrap();
    assert_eq!(job(&rt, &id).state, "complete");
    assert!(!source.exists());
    assert_eq!(remote.uploads.load(Ordering::SeqCst), 1);
    assert_no_staging(&rt);
}
#[test]
fn engine_data_files_cannot_be_selected_as_sources() {
    let (_tmp, _remote, rt) = setup();
    let source = rt.engine.lock().root.join("cache").join("protected");
    std::fs::write(&source, b"pending").unwrap();
    assert!(
        rt.queue_import(&source, "", false)
            .unwrap_err()
            .to_string()
            .contains("own data")
    );
    assert!(source.exists());
}
#[cfg(not(windows))]
#[test]
fn automatic_removal_requires_the_windows_source_lease() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("source");
    std::fs::write(&path, b"keep").unwrap();
    assert!(Source::open(&path, true).is_err());
    assert!(path.exists());
}
fn database(path: &Path) {
    let mut db = rusqlite::Connection::open(path).unwrap();
    db.execute_batch("PRAGMA journal_mode=DELETE; CREATE TABLE items(id INTEGER PRIMARY KEY, name TEXT); CREATE TABLE padding(data BLOB);").unwrap();
    let tx = db.transaction().unwrap();
    for id in 0..500 {
        tx.execute(
            "INSERT INTO items VALUES(?1,?2)",
            rusqlite::params![id, format!("item-{id}")],
        )
        .unwrap();
    }
    tx.execute("INSERT INTO padding VALUES(zeroblob(12*1024*1024))", [])
        .unwrap();
    tx.commit().unwrap();
}
fn import_database(tmp: &tempfile::TempDir, rt: &Runtime) -> String {
    let path = tmp.path().join("snapshot.sqlite");
    database(&path);
    let id = rt.queue_import(&path, "", false).unwrap();
    rt.tick_imports().unwrap();
    assert_eq!(job(rt, &id).state, "complete");
    "snapshot.sqlite".into()
}
#[test]
fn sqlite_indexed_query_of_three_gib_logical_file_reads_only_needed_ranges() {
    let (tmp, remote, rt) = setup();
    let path = import_database(&tmp, &rt);
    // A real SQLite image plus virtual trailing zeroes models 3 GiB without allocating a fixture that size.
    remote.virtual_large.store(true, Ordering::SeqCst);
    let result = rt
        .query_sqlite(&path, "SELECT name FROM items WHERE id=427")
        .unwrap();
    assert_eq!(result["rows"][0][0], "item-427");
    assert_eq!(result["file_size"], 3u64 * 1024 * 1024 * 1024);
    assert!(remote.bytes.load(Ordering::SeqCst) <= 2 * BLOCK);
    assert!(rt.engine.lock().read_cache.bytes() <= RAM_BUDGET);
    assert_no_staging(&rt);
}
#[test]
fn sqlite_rejects_mutations_attachments_pragmas_and_temporary_sorts() {
    let (tmp, _remote, rt) = setup();
    let path = import_database(&tmp, &rt);
    for sql in [
        "DELETE FROM items",
        "UPDATE items SET name='bad'",
        "CREATE TABLE bad(x)",
        "ATTACH ':memory:' AS evil",
        "PRAGMA journal_mode=WAL",
        "SELECT load_extension('evil')",
        "SELECT * FROM items ORDER BY name",
        "SELECT 1; DELETE FROM items",
    ] {
        assert!(
            rt.query_sqlite(&path, sql).is_err(),
            "unexpectedly allowed: {sql}"
        );
    }
    assert_eq!(
        rt.query_sqlite(&path, "SELECT count(*) FROM items")
            .unwrap()["rows"][0][0],
        500
    );
    let result = rt.query_sqlite(&path, "SELECT id FROM items").unwrap();
    assert_eq!(result["rows"].as_array().unwrap().len(), 200);
    assert_eq!(result["truncated"], true);
    assert_no_staging(&rt);
}
#[test]
fn sqlite_discards_results_when_revision_changes_and_rejects_cloud_sidecars() {
    let (tmp, remote, rt) = setup();
    let path = import_database(&tmp, &rt);
    remote.change_after_range.store(true, Ordering::SeqCst);
    let error = rt
        .query_sqlite(&path, "SELECT name FROM items WHERE id=1")
        .unwrap_err();
    assert!(error.to_string().contains("changed"));
    remote.changed.store(false, Ordering::SeqCst);
    remote.change_after_range.store(false, Ordering::SeqCst);
    // Windows/macOS can create sidecars with a differently cased database basename.
    let sidecar = tmp.path().join("SNAPSHOT.SQLITE-wal");
    std::fs::write(&sidecar, b"journal").unwrap();
    rt.queue_import(&sidecar, "", false).unwrap();
    rt.tick_imports().unwrap();
    assert!(
        rt.query_sqlite(&path, "SELECT 1")
            .unwrap_err()
            .to_string()
            .contains("sidecars")
    );
}
#[test]
fn sqlite_source_requires_a_standalone_delete_journal_snapshot() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("data.sqlite");
    database(&path);
    let mut wal = path.as_os_str().to_os_string();
    wal.push("-wal");
    std::fs::write(Path::new(&wal), b"wal").unwrap();
    assert!(Source::open(&path, false).is_err());
    std::fs::remove_file(Path::new(&wal)).unwrap();
    let mut bytes = std::fs::read(&path).unwrap();
    bytes[18] = 2;
    bytes[19] = 2;
    std::fs::write(&path, bytes).unwrap();
    assert!(Source::open(&path, false).is_err());
}
