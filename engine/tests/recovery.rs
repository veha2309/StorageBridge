use std::{fs, sync::Arc};
use storage_bridge::{drive::Remote, engine::Engine, local::LocalRemote};

fn setup() -> (tempfile::TempDir, Arc<LocalRemote>, Engine) {
    let tmp = tempfile::tempdir().unwrap();
    let remote = Arc::new(LocalRemote::new(tmp.path().join("remote")).unwrap());
    let mut engine = Engine::new(tmp.path().join("engine"), remote.clone()).unwrap();
    engine.initialize().unwrap();
    (tmp, remote, engine)
}
#[test]
fn reads_writes_and_recovers_pending_work() {
    let (tmp, remote, mut e) = setup();
    let n = e.create("\\hello.txt", false).unwrap();
    e.open_node(&n.id).unwrap();
    e.write(&n.id, 0, b"saved locally", false, false).unwrap();
    e.tick().unwrap();
    assert!(remote.get(&n.id).unwrap().is_none());
    drop(e);
    let mut e = Engine::new(tmp.path().join("engine"), remote.clone()).unwrap();
    e.tick().unwrap();
    assert_eq!(remote.get(&n.id).unwrap().unwrap().size, 13);
    assert_eq!(e.status().unwrap()["pending"], 0);
    let mut bytes = [0; 13];
    e.read(&n.id, 0, &mut bytes).unwrap();
    assert_eq!(&bytes, b"saved locally");
}
#[test]
fn dirty_and_open_files_are_never_evicted() {
    let (_tmp, _remote, mut e) = setup();
    let n = e.create("a", false).unwrap();
    e.write(&n.id, 0, b"keep", false, false).unwrap();
    e.store.set("cache_budget", "0").unwrap();
    e.evict().unwrap();
    assert!(e.cache(&n.id).exists());
    e.store.set("cache_budget", "1000").unwrap();
    e.tick().unwrap();
    e.open_node(&n.id).unwrap();
    e.store.set("cache_budget", "0").unwrap();
    e.evict().unwrap();
    assert!(e.cache(&n.id).exists());
    e.close_node(&n.id);
    e.evict().unwrap();
    assert!(!e.cache(&n.id).exists());
}
#[test]
fn concurrent_remote_edit_preserves_both_versions() {
    let (tmp, remote, mut e) = setup();
    let n = e.create("report.txt", false).unwrap();
    e.write(&n.id, 0, b"original", false, false).unwrap();
    e.tick().unwrap();
    e.write(&n.id, 0, b"local edit", false, false).unwrap();
    let mut changed = remote.get(&n.id).unwrap().unwrap();
    changed.size = 11;
    let bytes = tmp.path().join("changed");
    fs::write(&bytes, b"remote edit").unwrap();
    remote.upload(&changed, &bytes, true).unwrap();
    e.tick().unwrap();
    e.tick().unwrap();
    let nodes = remote.list("root").unwrap();
    assert_eq!(nodes.len(), 2);
    assert!(nodes.iter().any(|n| n.name.contains("conflict")));
    assert_eq!(
        fs::read(tmp.path().join("remote").join(&n.id)).unwrap(),
        b"remote edit"
    );
    let conflict = nodes.iter().find(|n| n.name.contains("conflict")).unwrap();
    assert_eq!(
        fs::read(tmp.path().join("remote").join(&conflict.id)).unwrap(),
        b"local edit"
    );
}
#[test]
fn directory_move_and_trash() {
    let (_tmp, remote, mut e) = setup();
    let folder = e.create("folder", true).unwrap();
    e.tick().unwrap();
    let n = e.create("file.txt", false).unwrap();
    e.tick().unwrap();
    e.rename(&n.id, "folder\\moved.txt", false).unwrap();
    e.tick().unwrap();
    let r = remote.get(&n.id).unwrap().unwrap();
    assert_eq!(r.parent, folder.id);
    assert_eq!(r.name, "moved.txt");
    assert!(e.can_delete(&folder.id).is_err());
    e.delete(&n.id).unwrap();
    e.tick().unwrap();
    assert!(remote.get(&n.id).unwrap().is_none());
}
#[test]
fn remote_changes_invalidate_only_clean_cached_files() {
    let (tmp, remote, mut e) = setup();
    let n = e.create("a", false).unwrap();
    e.write(&n.id, 0, b"old", false, false).unwrap();
    e.tick().unwrap();
    let mut r = remote.get(&n.id).unwrap().unwrap();
    r.size = 3;
    let path = tmp.path().join("bytes");
    fs::write(&path, b"new").unwrap();
    remote.upload(&r, &path, true).unwrap();
    e.poll().unwrap();
    assert!(!e.store.get(&n.id).unwrap().cached);
    let mut buf = [0; 3];
    e.read(&n.id, 0, &mut buf).unwrap();
    assert_eq!(&buf, b"new");
}
#[test]
fn replacement_uploads_before_trashing_old_destination() {
    let (_tmp, remote, mut e) = setup();
    let old = e.create("document.txt", false).unwrap();
    e.tick().unwrap();
    let temp = e.create("temp.tmp", false).unwrap();
    e.write(&temp.id, 0, b"replacement", false, false).unwrap();
    e.rename(&temp.id, "document.txt", true).unwrap();
    assert!(remote.get(&old.id).unwrap().is_some());
    e.tick().unwrap();
    assert!(remote.get(&old.id).unwrap().is_none());
    assert_eq!(remote.get(&temp.id).unwrap().unwrap().name, "document.txt");
}
#[test]
fn open_remote_changes_refresh_after_close() {
    let (_tmp, remote, mut e) = setup();
    let n = e.create("a", false).unwrap();
    e.tick().unwrap();
    e.open_node(&n.id).unwrap();
    let mut r = remote.get(&n.id).unwrap().unwrap();
    r.name = "b".into();
    remote.relocate(&r, "root").unwrap();
    e.poll().unwrap();
    assert_eq!(e.store.get(&n.id).unwrap().name, "a");
    e.close_node(&n.id);
    e.poll().unwrap();
    assert_eq!(e.store.get(&n.id).unwrap().name, "b");
}
#[test]
fn dpapi_roundtrip() {
    #[cfg(windows)]
    {
        let secret = b"test refresh token";
        let protected = storage_bridge::platform::protect(secret, false).unwrap();
        assert_ne!(protected, secret);
        assert_eq!(
            storage_bridge::platform::protect(&protected, true).unwrap(),
            secret
        );
    }
}
