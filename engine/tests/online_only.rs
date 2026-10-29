use std::sync::Arc;
use storage_bridge::{drive::Remote, engine::Engine, local::LocalRemote};

fn setup() -> (tempfile::TempDir, Arc<LocalRemote>, Engine) {
    let tmp = tempfile::tempdir().unwrap();
    let remote = Arc::new(LocalRemote::new(tmp.path().join("remote")).unwrap());
    let mut engine = Engine::new(tmp.path().join("engine"), remote.clone()).unwrap();
    engine.initialize().unwrap();
    (tmp, remote, engine)
}
fn uploaded(engine: &mut Engine) -> String {
    let n = engine.create("file.txt", false).unwrap();
    engine
        .write(&n.id, 0, b"cloud contents", false, false)
        .unwrap();
    engine.tick().unwrap();
    n.id
}
#[test]
fn enabling_mode_removes_existing_clean_copies_but_preserves_cloud() {
    let (_tmp, remote, mut e) = setup();
    let id = uploaded(&mut e);
    assert!(e.cache(&id).exists());
    e.set_online_only(true).unwrap();
    assert!(!e.cache(&id).exists());
    assert!(!e.store.get(&id).unwrap().cached);
    assert!(remote.get(&id).unwrap().is_some());
    assert_eq!(e.status().unwrap()["online_only"], true);
    assert_eq!(e.status().unwrap()["cache_bytes"], 0);
}
#[test]
fn last_handle_close_removes_download_and_next_open_redownloads() {
    let (_tmp, _remote, mut e) = setup();
    let id = uploaded(&mut e);
    e.set_online_only(true).unwrap();
    e.open_node(&id).unwrap();
    e.open_node(&id).unwrap();
    let mut bytes = [0; 14];
    e.read(&id, 0, &mut bytes).unwrap();
    assert_eq!(&bytes, b"cloud contents");
    e.tick().unwrap();
    assert!(e.cache(&id).exists());
    assert!(e.last_error.is_none());
    e.close_node(&id);
    assert!(e.cache(&id).exists());
    e.close_node(&id);
    assert!(!e.cache(&id).exists());
    e.open_node(&id).unwrap();
    e.read(&id, 0, &mut bytes).unwrap();
    assert_eq!(&bytes, b"cloud contents");
    e.close_node(&id);
    assert!(!e.cache(&id).exists());
}
#[test]
fn closed_dirty_files_remain_until_upload_is_acknowledged() {
    let (_tmp, remote, mut e) = setup();
    e.set_online_only(true).unwrap();
    let n = e.create("pending", false).unwrap();
    e.open_node(&n.id).unwrap();
    e.write(&n.id, 0, b"pending edits", false, false).unwrap();
    e.close_node(&n.id);
    assert!(e.cache(&n.id).exists());
    assert!(remote.get(&n.id).unwrap().is_none());
    e.tick().unwrap();
    assert!(remote.get(&n.id).unwrap().is_some());
    assert!(!e.cache(&n.id).exists());
    assert_eq!(e.status().unwrap()["pending"], 0);
}
#[test]
fn restart_keeps_pending_bytes_and_mode_then_cleans_after_upload() {
    let (tmp, remote, mut e) = setup();
    e.set_online_only(true).unwrap();
    let n = e.create("pending", false).unwrap();
    e.write(&n.id, 0, b"recover me", false, false).unwrap();
    drop(e);
    let mut e = Engine::new(tmp.path().join("engine"), remote.clone()).unwrap();
    assert!(e.online_only());
    assert!(e.cache(&n.id).exists());
    e.tick().unwrap();
    assert!(!e.cache(&n.id).exists());
    assert_eq!(
        std::fs::read(tmp.path().join("remote").join(&n.id)).unwrap(),
        b"recover me"
    );
}
#[test]
fn restart_cleans_files_that_were_open_during_crash() {
    let (tmp, remote, mut e) = setup();
    let id = uploaded(&mut e);
    e.open_node(&id).unwrap();
    e.set_online_only(true).unwrap();
    assert!(e.cache(&id).exists());
    drop(e);
    let e = Engine::new(tmp.path().join("engine"), remote).unwrap();
    assert!(!e.cache(&id).exists());
    assert_eq!(e.status().unwrap()["cache_bytes"], 0);
}
#[test]
fn disabling_mode_restores_budgeted_caching() {
    let (_tmp, _remote, mut e) = setup();
    let id = uploaded(&mut e);
    e.set_online_only(true).unwrap();
    e.set_online_only(false).unwrap();
    e.open_node(&id).unwrap();
    e.read(&id, 0, &mut [0; 14]).unwrap();
    e.close_node(&id);
    e.tick().unwrap();
    assert!(e.cache(&id).exists());
    assert_eq!(e.status().unwrap()["online_only"], false);
}
#[test]
fn metadata_jobs_also_protect_copies_until_reconciled() {
    let (_tmp, _remote, mut e) = setup();
    let id = uploaded(&mut e);
    e.rename(&id, "renamed.txt", false).unwrap();
    e.set_online_only(true).unwrap();
    assert!(e.cache(&id).exists());
    e.tick().unwrap();
    assert!(!e.cache(&id).exists());
    assert_eq!(e.resolve("renamed.txt").unwrap().id, id);
}
