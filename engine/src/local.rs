//! Deterministic, disk-backed development remote. Never contacts Google.
use crate::{drive::Remote, store::Node};
use anyhow::{Context, Result};
use parking_lot::Mutex;
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};
pub struct LocalRemote {
    root: PathBuf,
    nodes: Mutex<BTreeMap<String, Node>>,
}
impl LocalRemote {
    pub fn new(root: PathBuf) -> Result<Self> {
        std::fs::create_dir_all(&root)?;
        let mut nodes: BTreeMap<String, Node> = std::fs::read(root.join("nodes.json"))
            .ok()
            .and_then(|s| serde_json::from_slice(&s).ok())
            .unwrap_or_default();
        nodes.entry("root".into()).or_insert(Node {
            id: "root".into(),
            parent: "".into(),
            name: "My Drive".into(),
            alias: "".into(),
            directory: true,
            size: 0,
            version: "1".into(),
            cached: false,
            dirty: false,
            deleted: false,
            last_access: 0,
            modified: chrono::Utc::now().timestamp(),
        });
        Ok(Self {
            root,
            nodes: Mutex::new(nodes),
        })
    }
    fn put(&self, mut n: Node) -> Result<Node> {
        let mut nodes = self.nodes.lock();
        n.version = (nodes
            .get(&n.id)
            .and_then(|n| n.version.parse::<u64>().ok())
            .unwrap_or(0)
            + 1)
        .to_string();
        n.dirty = false;
        n.cached = false;
        n.modified = chrono::Utc::now().timestamp();
        nodes.insert(n.id.clone(), n.clone());
        let tmp = self.root.join("nodes.tmp");
        std::fs::write(&tmp, serde_json::to_vec(&*nodes)?)?;
        crate::engine::replace(&tmp, &self.root.join("nodes.json"))?;
        Ok(n)
    }
}
impl Remote for LocalRemote {
    fn read_range(&self, n: &Node, offset: u64, length: usize) -> Result<Vec<u8>> {
        use std::io::{Read, Seek, SeekFrom};
        let current = self.get(&n.id)?.context("File not found")?;
        anyhow::ensure!(current.version == n.version, "Remote file changed");
        let mut f = std::fs::File::open(self.root.join(&n.id))?;
        f.seek(SeekFrom::Start(offset))?;
        let mut data = vec![0; length.min(n.size.saturating_sub(offset) as usize)];
        f.read_exact(&mut data)?;
        Ok(data)
    }
    fn upload_source(
        &self,
        n: &Node,
        source: &dyn crate::streaming::ContentSource,
        _: bool,
    ) -> Result<Node> {
        use std::io::Write;
        let temp = self.root.join(format!("{}.upload", n.id));
        let mut file = std::fs::File::create(&temp)?;
        let mut offset = 0;
        while offset < n.size {
            let mut bytes = vec![0; (n.size - offset).min(1024 * 1024) as usize];
            source.read_exact_at(offset, &mut bytes)?;
            file.write_all(&bytes)?;
            offset += bytes.len() as u64;
        }
        file.sync_all()?;
        drop(file);
        crate::engine::replace(&temp, &self.root.join(&n.id))?;
        self.put(n.clone())
    }
    fn list(&self, parent: &str) -> Result<Vec<Node>> {
        Ok(self
            .nodes
            .lock()
            .values()
            .filter(|n| n.parent == parent && !n.deleted)
            .cloned()
            .collect())
    }
    fn get(&self, id: &str) -> Result<Option<Node>> {
        Ok(self.nodes.lock().get(id).filter(|n| !n.deleted).cloned())
    }
    fn generate_id(&self) -> Result<String> {
        Ok(uuid::Uuid::new_v4().to_string())
    }
    fn download(&self, id: &str, target: &Path) -> Result<()> {
        std::fs::copy(self.root.join(id), target)?;
        Ok(())
    }
    fn upload(&self, n: &Node, source: &Path, _: bool) -> Result<Node> {
        std::fs::copy(source, self.root.join(&n.id))?;
        self.put(n.clone())
    }
    fn mkdir(&self, n: &Node) -> Result<Node> {
        self.put(n.clone())
    }
    fn relocate(&self, n: &Node, _: &str) -> Result<Node> {
        let old = self.get(&n.id)?.context("File not found")?;
        let mut updated = n.clone();
        updated.size = old.size;
        self.put(updated)
    }
    fn trash(&self, id: &str) -> Result<()> {
        if let Some(mut n) = self.get(id)? {
            n.deleted = true;
            self.put(n)?;
        }
        Ok(())
    }
    fn cursor(&self) -> Result<String> {
        Ok("local".into())
    }
    fn changes(&self, _: &str) -> Result<(Vec<(String, Option<Node>)>, String)> {
        Ok((
            self.nodes
                .lock()
                .values()
                .map(|n| (n.id.clone(), if n.deleted { None } else { Some(n.clone()) }))
                .collect(),
            "local".into(),
        ))
    }
    fn about(&self) -> Result<Value> {
        Ok(
            json!({"user":{"displayName":"Local test drive","emailAddress":"No Google connection"},"storageQuota":{"limit":"16106127360","usage":"0"}}),
        )
    }
}
