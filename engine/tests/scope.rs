use std::sync::Arc;
use storage_bridge::{drive::Remote, engine::Engine, local::LocalRemote};

#[test]
fn only_storagebridge_contents_are_mounted_and_indexed() {
    let tmp = tempfile::tempdir().unwrap();
    let remote = Arc::new(LocalRemote::new(tmp.path().join("remote")).unwrap());
    let mut full = Engine::new(tmp.path().join("full"), remote.clone()).unwrap();
    full.initialize().unwrap();
    let outside = full.create("private.txt", false).unwrap();
    let folder = full.create("StorageBridge", true).unwrap();
    let inside = full.create("StorageBridge\\test.txt", false).unwrap();
    full.tick().unwrap();
    let mut scoped = Engine::new(tmp.path().join("scoped"), remote.clone()).unwrap();
    scoped.configure_scope("StorageBridge").unwrap();
    scoped.initialize().unwrap();
    scoped.poll().unwrap();
    assert_eq!(scoped.root_id(), folder.id);
    assert_eq!(scoped.resolve("test.txt").unwrap().id, inside.id);
    assert!(scoped.resolve("private.txt").is_err());
    assert!(scoped.store.get(&outside.id).is_err());
    let created = scoped.create("new.txt", false).unwrap();
    scoped.tick().unwrap();
    assert_eq!(remote.get(&created.id).unwrap().unwrap().parent, folder.id);
}
#[test]
fn missing_scope_is_created_once_and_pinned_by_id() {
    let tmp = tempfile::tempdir().unwrap();
    let remote = Arc::new(LocalRemote::new(tmp.path().join("remote")).unwrap());
    let mut e = Engine::new(tmp.path().join("scoped"), remote.clone()).unwrap();
    e.configure_scope("StorageBridge").unwrap();
    e.initialize().unwrap();
    let root = e.root_id();
    e.initialize().unwrap();
    assert_eq!(e.root_id(), root);
    assert_eq!(remote.list("root").unwrap().len(), 1);
    remote.trash(&root).unwrap();
    assert!(e.initialize().unwrap_err().to_string().contains("removed"));
    assert!(remote.list("root").unwrap().is_empty());
}
#[test]
fn duplicate_scope_folders_require_explicit_resolution() {
    let tmp = tempfile::tempdir().unwrap();
    let remote = Arc::new(LocalRemote::new(tmp.path().join("remote")).unwrap());
    let mut full = Engine::new(tmp.path().join("full"), remote.clone()).unwrap();
    full.initialize().unwrap();
    let a = full.create("StorageBridge", true).unwrap();
    full.tick().unwrap();
    let mut duplicate = remote.get(&a.id).unwrap().unwrap();
    duplicate.id = remote.generate_id().unwrap();
    remote.mkdir(&duplicate).unwrap();
    let mut e = Engine::new(tmp.path().join("scoped"), remote.clone()).unwrap();
    e.configure_scope("StorageBridge").unwrap();
    assert!(e.initialize().unwrap_err().to_string().contains("Multiple"));
    assert!(e.store.setting("root_id").is_none());
}
#[test]
fn externally_moved_files_are_not_modified_outside_the_scope() {
    let tmp = tempfile::tempdir().unwrap();
    let remote = Arc::new(LocalRemote::new(tmp.path().join("remote")).unwrap());
    let mut e = Engine::new(tmp.path().join("scoped"), remote.clone()).unwrap();
    e.configure_scope("StorageBridge").unwrap();
    e.initialize().unwrap();
    let n = e.create("test.txt", false).unwrap();
    e.write(&n.id, 0, b"initial", false, false).unwrap();
    e.tick().unwrap();
    e.write(&n.id, 0, b"edited!", false, false).unwrap();
    let mut moved = remote.get(&n.id).unwrap().unwrap();
    moved.parent = "root".into();
    remote.relocate(&moved, &e.root_id()).unwrap();
    e.tick().unwrap();
    assert_eq!(e.status().unwrap()["pending"], 1);
    assert!(
        e.status().unwrap()["transfers"][0]["error"]
            .as_str()
            .unwrap()
            .contains("outside StorageBridge")
    );
    assert_eq!(
        std::fs::read(tmp.path().join("remote").join(&n.id)).unwrap(),
        b"initial"
    );
}

#[test]
fn deleting_a_pending_tree_clears_children_before_parent() {
    let tmp = tempfile::tempdir().unwrap();
    let remote = Arc::new(LocalRemote::new(tmp.path().join("remote")).unwrap());
    let mut e = Engine::new(tmp.path().join("scoped"), remote.clone()).unwrap();
    e.configure_scope("StorageBridge").unwrap();
    e.initialize().unwrap();
    let folder = e.create("folder", true).unwrap();
    let child = e.create("folder\\child", false).unwrap();
    e.delete(&child.id).unwrap();
    e.delete(&folder.id).unwrap();
    e.tick().unwrap();
    e.tick().unwrap();
    assert_eq!(e.status().unwrap()["pending"], 0);
    assert!(remote.list(&e.root_id()).unwrap().is_empty());
}
