use parking_lot::Mutex;
use std::sync::{Arc, Barrier};
use storage_bridge::{
    engine::Engine, local::LocalRemote, runtime::Runtime, storage::ContentLimitExceeded,
};

fn setup() -> (tempfile::TempDir, Arc<LocalRemote>, Engine) {
    let tmp = tempfile::tempdir().unwrap();
    let remote = Arc::new(LocalRemote::new(tmp.path().join("remote")).unwrap());
    let mut engine = Engine::new(tmp.path().join("engine"), remote.clone()).unwrap();
    engine.initialize().unwrap();
    (tmp, remote, engine)
}

#[test]
fn boundary_rejection_preserves_bytes_size_and_pending_intent() {
    let (_tmp, _remote, mut e) = setup();
    e.set_content_limit(8).unwrap();
    let n = e.create("accepted", false).unwrap();
    e.write(&n.id, 0, b"accepted", false, false).unwrap();
    let error = e.write(&n.id, 8, b"!", false, false).unwrap_err();
    assert!(error.is::<ContentLimitExceeded>());
    assert!(e.resize(&n.id, 9).unwrap_err().is::<ContentLimitExceeded>());
    assert!(e.write(&n.id, u64::MAX, b"!", false, false).is_err());
    assert_eq!(std::fs::read(e.cache(&n.id)).unwrap(), b"accepted");
    assert_eq!(e.store.get(&n.id).unwrap().size, 8);
    assert_eq!(e.status().unwrap()["pending"], 1);
    assert_eq!(e.status().unwrap()["storage"]["used_bytes"], 8);
    assert_eq!(e.status().unwrap()["storage"]["write_blocked"], true);
    // A full budget permits an overwrite that does not grow the staged file.
    e.write(&n.id, 0, b"A", false, false).unwrap();
    assert_eq!(std::fs::read(e.cache(&n.id)).unwrap(), b"Accepted");
}

#[test]
fn partial_journals_count_actual_growth_and_recover_after_restart() {
    let (tmp, remote, mut e) = setup();
    let n = e.create("cloud", false).unwrap();
    e.write(&n.id, 0, b"abcdefghij", false, false).unwrap();
    e.tick().unwrap();
    e.set_online_only(true).unwrap();
    e.set_content_limit(4).unwrap();
    let rt = Runtime::new(Arc::new(Mutex::new(e)));
    rt.write(&n.id, 2, b"XYZ", false, false).unwrap();
    assert!(
        rt.write(&n.id, 0, b"NO", false, false)
            .unwrap_err()
            .is::<ContentLimitExceeded>()
    );
    assert_eq!(rt.engine.lock().content_bytes().unwrap(), 3);
    let mut result = [0; 10];
    rt.read(&n.id, 0, &mut result).unwrap();
    assert_eq!(&result, b"abXYZfghij");
    drop(rt);
    let e = Engine::new(tmp.path().join("engine"), remote).unwrap();
    assert_eq!(e.content_limit(), 4);
    let rt = Runtime::new(Arc::new(Mutex::new(e)));
    rt.read(&n.id, 0, &mut result).unwrap();
    assert_eq!(&result, b"abXYZfghij");
    rt.tick().unwrap();
    assert_eq!(rt.engine.lock().content_bytes().unwrap(), 0);
    assert_eq!(
        rt.engine.lock().status().unwrap()["storage"]["write_blocked"],
        false
    );
}

#[test]
fn simultaneous_files_share_one_content_allowance() {
    let (_tmp, _remote, mut e) = setup();
    e.set_content_limit(64).unwrap();
    let a = e.create("a", false).unwrap();
    let b = e.create("b", false).unwrap();
    let rt = Arc::new(Runtime::new(Arc::new(Mutex::new(e))));
    let start = Arc::new(Barrier::new(3));
    let handles: Vec<_> = [a.id, b.id]
        .into_iter()
        .map(|id| {
            let rt = rt.clone();
            let start = start.clone();
            std::thread::spawn(move || {
                start.wait();
                rt.write(&id, 0, &[1; 40], false, false)
            })
        })
        .collect();
    start.wait();
    let results: Vec<_> = handles.into_iter().map(|h| h.join().unwrap()).collect();
    assert_eq!(results.iter().filter(|r| r.is_ok()).count(), 1);
    assert_eq!(rt.engine.lock().content_bytes().unwrap(), 40);
}

#[test]
fn clean_copies_make_room_but_open_and_dirty_data_are_protected() {
    let (_tmp, _remote, mut e) = setup();
    e.set_content_limit(16).unwrap();
    let a = e.create("clean", false).unwrap();
    e.write(&a.id, 0, &[1; 8], false, false).unwrap();
    e.tick().unwrap();
    e.open_node(&a.id).unwrap();
    let b = e.create("pending", false).unwrap();
    e.write(&b.id, 0, &[2; 8], false, false).unwrap();
    assert!(e.write(&b.id, 8, b"+", false, false).is_err());
    e.release_clean_content().unwrap();
    assert!(e.cache(&a.id).exists());
    assert!(e.cache(&b.id).exists());
    assert!(e.set_content_limit(7).is_err());
    assert_eq!(e.content_limit(), 16);
    e.close_node(&a.id);
    e.write(&b.id, 8, b"+", false, false).unwrap();
    assert!(!e.cache(&a.id).exists());
    assert_eq!(e.content_bytes().unwrap(), 9);
    let usage = e.storage_usage().unwrap();
    assert_eq!(usage.pending_bytes, 9);
    assert_eq!(usage.retained_bytes, 0);
    assert_eq!(usage.files[0].name, "pending");
}

#[test]
fn older_pending_work_above_limit_survives_and_blocks_only_growth() {
    let (tmp, remote, mut e) = setup();
    let n = e.create("legacy", false).unwrap();
    e.write(&n.id, 0, b"retain these bytes", false, false)
        .unwrap();
    // Simulate existing pending work inherited from an older installation.
    e.store.set("content_limit", "4").unwrap();
    drop(e);
    let mut e = Engine::new(tmp.path().join("engine"), remote).unwrap();
    assert!(e.storage_usage().unwrap().over_limit);
    assert_eq!(
        std::fs::read(e.cache(&n.id)).unwrap(),
        b"retain these bytes"
    );
    assert!(e.write(&n.id, 0, b"+", true, false).is_err());
    e.set_online_only(true).unwrap();
    e.tick().unwrap();
    assert_eq!(e.content_bytes().unwrap(), 0);
    assert!(!e.storage_usage().unwrap().write_blocked);
}

#[test]
fn sparse_extensions_and_uncached_reads_do_not_reserve_cloud_size() {
    let (_tmp, _remote, mut e) = setup();
    let n = e.create("cloud", false).unwrap();
    e.write(&n.id, 0, b"abcdefghij", false, false).unwrap();
    e.tick().unwrap();
    e.set_online_only(true).unwrap();
    e.set_content_limit(1).unwrap();
    let rt = Runtime::new(Arc::new(Mutex::new(e)));
    let mut result = [0; 10];
    rt.read(&n.id, 0, &mut result).unwrap();
    assert_eq!(&result, b"abcdefghij");
    assert_eq!(rt.engine.lock().content_bytes().unwrap(), 0);
    rt.resize(&n.id, 8 * 1024 * 1024 * 1024).unwrap();
    assert_eq!(rt.engine.lock().content_bytes().unwrap(), 0);
    rt.write(&n.id, 6, b"X", false, false).unwrap();
    assert_eq!(rt.engine.lock().content_bytes().unwrap(), 1);
}

#[test]
fn conflict_copy_cannot_bypass_limit_or_discard_original() {
    let (_tmp, remote, mut e) = setup();
    let n = e.create("conflict", false).unwrap();
    e.write(&n.id, 0, b"original", false, false).unwrap();
    e.tick().unwrap();
    e.set_content_limit(8).unwrap();
    e.write(&n.id, 0, b"my edits", false, false).unwrap();
    let mut changed = storage_bridge::drive::Remote::get(remote.as_ref(), &n.id)
        .unwrap()
        .unwrap();
    changed.name = "changed remotely".into();
    storage_bridge::drive::Remote::relocate(remote.as_ref(), &changed, &changed.parent).unwrap();
    e.tick().unwrap();
    assert_eq!(e.content_bytes().unwrap(), 8);
    assert_eq!(std::fs::read(e.cache(&n.id)).unwrap(), b"my edits");
    assert_eq!(e.status().unwrap()["pending"], 1);
    assert!(
        e.status().unwrap()["transfers"][0]["error"]
            .as_str()
            .unwrap()
            .contains("Local content limit")
    );
    e.set_content_limit(16).unwrap();
    e.retry().unwrap();
    e.tick().unwrap();
    assert!(e.content_bytes().unwrap() <= 16);
}
