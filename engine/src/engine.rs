use crate::{
    drive::Remote,
    names,
    store::{Node, Store},
};
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use std::{
    collections::{HashMap, HashSet},
    fs::{File, OpenOptions},
    io::{Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
    sync::Arc,
};

pub const DEFAULT_BUDGET: u64 = 10 * 1024 * 1024 * 1024;
struct PartialDownload(PathBuf);
impl Drop for PartialDownload {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}
pub struct Engine {
    pub store: Store,
    pub remote: Arc<dyn Remote>,
    pub root: PathBuf,
    pub open: HashMap<String, u32>,
    pub last_error: Option<String>,
    pub(crate) refresh: HashSet<String>,
    pub(crate) metadata_only: bool,
    pub read_cache: Arc<crate::streaming::ReadCache>,
    pub(crate) read_views: HashMap<String, Node>,
}
fn now() -> i64 {
    chrono::Utc::now().timestamp()
}
impl Engine {
    pub fn new(root: PathBuf, remote: Arc<dyn Remote>) -> Result<Self> {
        std::fs::create_dir_all(root.join("cache"))?;
        let store = Store::open(&root.join("metadata.db"))?;
        let mut engine = Self {
            store,
            remote,
            root,
            open: HashMap::new(),
            last_error: None,
            refresh: HashSet::new(),
            metadata_only: false,
            read_cache: Arc::new(crate::streaming::ReadCache::default()),
            read_views: HashMap::new(),
        };
        for mut node in engine.store.all()? {
            if node.dirty
                && !node.directory
                && engine
                    .store
                    .setting(&format!("overlay:{}", node.id))
                    .is_none()
                && let Ok(metadata) = std::fs::metadata(engine.cache(&node.id))
            {
                node.size = metadata.len();
                engine.store.put(&node)?;
            }
        }
        let retained: HashSet<_> = engine
            .store
            .all()?
            .iter()
            .filter(|n| n.cached || n.dirty)
            .map(|n| engine.cache(&n.id))
            .collect();
        for entry in std::fs::read_dir(engine.root.join("cache"))? {
            let entry = entry?;
            if entry.file_type()?.is_file() && !retained.contains(&entry.path()) {
                std::fs::remove_file(entry.path())?;
            }
        }
        engine.evict()?;
        Ok(engine)
    }
    pub(crate) fn attached(&self) -> Result<Self> {
        Ok(Self {
            store: Store::open(&self.root.join("metadata.db"))?,
            remote: self.remote.clone(),
            root: self.root.clone(),
            open: self.open.clone(),
            last_error: None,
            refresh: HashSet::new(),
            metadata_only: false,
            read_cache: self.read_cache.clone(),
            read_views: HashMap::new(),
        })
    }
    pub fn root_id(&self) -> String {
        self.store.setting("root_id").unwrap_or("root".into())
    }
    pub fn configure_scope(&mut self, name: &str) -> Result<()> {
        if self.store.setting("scope_folder").as_deref() == Some(name) {
            return Ok(());
        }
        let pending: u32 = self
            .store
            .db
            .query_row("SELECT count(*) FROM jobs", [], |r| r.get(0))?;
        ensure!(
            pending == 0 && self.store.all()?.iter().all(|n| !n.dirty),
            "Finish pending uploads before changing the mounted folder"
        );
        for node in self.store.all()? {
            if self.cache(&node.id).exists() {
                std::fs::remove_file(self.cache(&node.id))?;
            }
        }
        let tx = self.store.db.transaction()?;
        tx.execute("DELETE FROM nodes", [])?;
        tx.execute("DELETE FROM settings WHERE key IN ('root_id','cursor','scope_id','scope_create_id') OR key LIKE 'listed:%'",[])?;
        tx.execute("INSERT INTO settings VALUES('scope_folder',?1) ON CONFLICT(key) DO UPDATE SET value=excluded.value",[name])?;
        tx.commit()?;
        Ok(())
    }
    fn scoped_root(&mut self, account_root: Node) -> Result<Node> {
        let Some(name) = self.store.setting("scope_folder") else {
            return Ok(account_root);
        };
        self.store.set("account_root_id", &account_root.id)?;
        if let Some(id) = self.store.setting("scope_id") {
            let root=self.remote.get(&id)?.context("The StorageBridge folder was removed. Restore it in Google Drive before reconnecting.")?;
            ensure!(
                root.directory,
                "The selected Google Drive item is not a folder"
            );
            return Ok(root);
        }
        let matches: Vec<_> = self
            .remote
            .list(&account_root.id)?
            .into_iter()
            .filter(|n| n.directory && n.name == name)
            .collect();
        ensure!(
            matches.len() <= 1,
            "Multiple StorageBridge folders exist in My Drive. Rename the duplicates before connecting."
        );
        let root = if let Some(n) = matches.into_iter().next() {
            n
        } else {
            let id = if let Some(id) = self.store.setting("scope_create_id") {
                id
            } else {
                let id = self.remote.generate_id()?;
                self.store.set("scope_create_id", &id)?;
                id
            };
            if let Some(existing) = self.remote.get(&id)? {
                existing
            } else {
                self.remote.mkdir(&Node {
                    id,
                    parent: account_root.id,
                    name,
                    alias: String::new(),
                    directory: true,
                    size: 0,
                    version: String::new(),
                    cached: false,
                    dirty: false,
                    deleted: false,
                    last_access: now(),
                    modified: now(),
                })?
            }
        };
        self.store.set("scope_id", &root.id)?;
        Ok(root)
    }
    pub(crate) fn in_scope(&self, node: &Node) -> Result<bool> {
        if self.store.setting("scope_folder").is_none() {
            return Ok(true);
        }
        let root = self.root_id();
        if node.id == root {
            return Ok(true);
        }
        let mut parent = node.parent.clone();
        let mut seen = HashSet::new();
        for _ in 0..256 {
            if parent == root {
                return Ok(true);
            }
            if parent.is_empty()
                || parent == "root"
                || self.store.setting("account_root_id").as_deref() == Some(parent.as_str())
                || !seen.insert(parent.clone())
            {
                return Ok(false);
            }
            let Some(n) = self.remote.get(&parent)? else {
                return Ok(false);
            };
            parent = n.parent;
        }
        anyhow::bail!("Google Drive folder ancestry exceeds the supported depth")
    }
    pub fn initialize(&mut self) -> Result<()> {
        // Capture cursor BEFORE enumeration so changes during enumeration are replayed.
        let cursor = self.remote.cursor()?;
        let account_root = self
            .remote
            .get("root")?
            .context("Google Drive root was not found")?;
        let mut root = self.scoped_root(account_root)?;
        ensure!(root.directory, "The mount root must be a folder");
        root.alias = "".into();
        self.store.set("root_id", &root.id)?;
        self.store.put(&root)?;
        self.list(&root.id)?;
        if self.store.setting("cursor").is_none() {
            self.store.set("cursor", &cursor)?;
        }
        let about = self.remote.about()?;
        self.store.set("about", &about.to_string())?;
        Ok(())
    }
    pub fn cache(&self, id: &str) -> PathBuf {
        use sha2::{Digest, Sha256};
        self.root
            .join("cache")
            .join(format!("{:x}", Sha256::digest(id.as_bytes())))
    }
    pub fn children(&self, parent: &str) -> Result<Vec<Node>> {
        Ok(self
            .store
            .all()?
            .into_iter()
            .filter(|n| n.parent == parent && n.id != parent && !n.deleted)
            .collect())
    }
    pub(crate) fn merge(&mut self, mut n: Node) -> Result<()> {
        if let Ok(old) = self.store.get(&n.id) {
            if old.dirty || self.has_job(&n.id) {
                return Ok(());
            }
            if old.deleted
                && self.store.db.query_row(
                    "SELECT EXISTS(SELECT 1 FROM settings WHERE key LIKE 'replace:%' AND value=?1)",
                    [&n.id],
                    |r| r.get::<_, bool>(0),
                )?
            {
                return Ok(());
            }
            if self.open.get(&n.id).copied().unwrap_or(0) > 0 {
                self.refresh.insert(n.id);
                return Ok(());
            }
            n.alias = old.alias;
            n.last_access = old.last_access;
            n.cached = old.cached && old.version == n.version && self.cache(&n.id).exists();
            if old.cached && !n.cached && self.cache(&n.id).exists() {
                std::fs::remove_file(self.cache(&n.id))?;
            }
            if old.deleted || old.parent != n.parent || old.name != n.name {
                n.alias.clear();
            }
        }
        if n.alias.is_empty() {
            let occupied = self
                .children(&n.parent)?
                .into_iter()
                .filter(|x| x.id != n.id)
                .map(|x| x.alias.to_lowercase())
                .collect();
            n.alias = names::alias(&n.name, &n.id, &occupied);
        }
        self.store.put(&n)?;
        Ok(())
    }
    pub fn list(&mut self, parent: &str) -> Result<Vec<Node>> {
        if self
            .store
            .get(parent)
            .ok()
            .is_some_and(|n| n.directory && n.version.is_empty() && n.dirty)
        {
            return self.children(parent);
        }
        let key = format!("listed:{parent}");
        let last = self
            .store
            .setting(&key)
            .and_then(|s| s.parse::<i64>().ok())
            .unwrap_or(0);
        if self.metadata_only {
            ensure!(last > 0, "Folder metadata is not available offline");
            return self.children(parent);
        }
        if now() - last >= 30 {
            match self.remote.list(parent) {
                Ok(mut nodes) => {
                    nodes.sort_by(|a, b| a.id.cmp(&b.id));
                    let ids: HashSet<_> = nodes.iter().map(|n| n.id.clone()).collect();
                    for n in nodes {
                        self.merge(n)?;
                    }
                    for mut old in self.children(parent)? {
                        if !ids.contains(&old.id) && !old.dirty && !self.has_job(&old.id) {
                            old.deleted = true;
                            self.store.put(&old)?;
                        }
                    }
                    self.store.set(&key, &now().to_string())?;
                }
                Err(e) => {
                    self.last_error = Some(e.to_string());
                    ensure!(
                        last > 0,
                        "Folder is not cached and Google Drive is unavailable: {e}"
                    );
                }
            }
        }
        self.children(parent)
    }
    pub fn resolve(&mut self, path: &str) -> Result<Node> {
        let mut current = self.store.get(&self.root_id())?;
        for part in path.split(['\\', '/']).filter(|s| !s.is_empty()) {
            ensure!(
                part != "." && part != ".." && !part.contains(':'),
                "Invalid path"
            );
            current = self
                .list(&current.id)?
                .into_iter()
                .find(|n| n.alias.to_lowercase() == part.to_lowercase())
                .context("File not found")?;
        }
        Ok(current)
    }
    pub fn open_node(&mut self, id: &str) -> Result<()> {
        let n = self.store.get(id)?;
        ensure!(!n.deleted, "File was deleted");
        *self.open.entry(id.into()).or_default() += 1;
        Ok(())
    }
    pub fn close_node(&mut self, id: &str) {
        if let Some(n) = self.open.get_mut(id) {
            *n = n.saturating_sub(1);
            if *n == 0 {
                self.open.remove(id);
                self.read_views.remove(id);
            }
        }
        if self.online_only()
            && let Err(error) = self.evict()
        {
            self.last_error = Some(format!("Could not remove temporary file data: {error}"));
        }
    }
    pub fn online_only(&self) -> bool {
        self.store.setting("online_only").as_deref() == Some("true")
    }
    pub fn set_online_only(&mut self, enabled: bool) -> Result<()> {
        self.store
            .set("online_only", if enabled { "true" } else { "false" })?;
        self.evict()
    }
    fn hydrate(&mut self, id: &str) -> Result<Node> {
        let mut n = self.store.get(id)?;
        ensure!(!n.directory, "Cannot read a directory");
        let path = self.cache(id);
        if !n.cached || !path.exists() {
            ensure!(
                !n.dirty,
                "Pending local content is missing; recovery is required"
            );
            let part = path.with_extension("download");
            let _partial = PartialDownload(part.clone());
            if let Err(e) = self.remote.download(id, &part) {
                let _ = std::fs::remove_file(&part);
                return Err(e).context("File is not available offline; reconnect to download it");
            }
            ensure!(
                std::fs::metadata(&part)?.len() == n.size,
                "Incomplete download; retry opening the file"
            );
            // Verify the revision after download; do not associate new bytes with an old base.
            let remote = self
                .remote
                .get(id)?
                .context("File disappeared during download")?;
            ensure!(
                remote.version == n.version,
                "Remote file changed during download; refresh and retry"
            );
            replace(&part, &path)?;
            n.cached = true;
        }
        n.last_access = now();
        self.store.put(&n)?;
        Ok(n)
    }
    pub fn read(&mut self, id: &str, offset: u64, buffer: &mut [u8]) -> Result<usize> {
        self.hydrate(id)?;
        let mut f = File::open(self.cache(id))?;
        f.seek(SeekFrom::Start(offset))?;
        Ok(f.read(buffer)?)
    }
    pub fn write(
        &mut self,
        id: &str,
        offset: u64,
        bytes: &[u8],
        append: bool,
        constrained: bool,
    ) -> Result<usize> {
        let mut n = self.hydrate(id)?;
        let offset = if append { n.size } else { offset };
        let length = if constrained {
            bytes.len().min(n.size.saturating_sub(offset) as usize)
        } else {
            bytes.len()
        };
        if length == 0 {
            return Ok(0);
        }
        // Journal dirty state before modifying content; worker is serialized with writes.
        n.dirty = true;
        self.store.stage(&n, "reconcile")?;
        let mut f = OpenOptions::new().write(true).open(self.cache(id))?;
        f.seek(SeekFrom::Start(offset))?;
        let result = f.write_all(&bytes[..length]);
        let flush = f.sync_all();
        n.size = f.metadata()?.len();
        n.modified = now();
        self.store.stage(&n, "reconcile")?;
        result?;
        flush?;
        Ok(length)
    }
    pub fn resize(&mut self, id: &str, size: u64) -> Result<()> {
        let mut n = self.hydrate(id)?;
        n.dirty = true;
        self.store.stage(&n, "reconcile")?;
        let f = OpenOptions::new().write(true).open(self.cache(id))?;
        f.set_len(size)?;
        f.sync_all()?;
        n.size = size;
        n.modified = now();
        self.store.stage(&n, "reconcile")
    }
    pub fn flush(&self, id: &str) -> Result<()> {
        if self.cache(id).exists() {
            OpenOptions::new()
                .write(true)
                .open(self.cache(id))?
                .sync_all()?;
        }
        Ok(())
    }
    pub fn create(&mut self, path: &str, directory: bool) -> Result<Node> {
        let id = self
            .remote
            .generate_id()
            .context("Reconnect to create a new file or folder")?;
        self.create_with_id(path, directory, id)
    }
    pub(crate) fn create_with_id(
        &mut self,
        path: &str,
        directory: bool,
        id: String,
    ) -> Result<Node> {
        let (parent, name) = split(path)?;
        ensure!(name.encode_utf16().count() <= 160, "File name is too long");
        let parent = self.resolve(parent)?;
        ensure!(parent.directory, "Parent is not a directory");
        ensure!(
            !self
                .list(&parent.id)?
                .iter()
                .any(|n| n.alias.to_lowercase() == name.to_lowercase()),
            "File already exists"
        );
        // Preallocated IDs make retries of interrupted creates idempotent.
        let mut n = Node {
            id,
            parent: parent.id,
            name: name.into(),
            alias: name.into(),
            directory,
            size: 0,
            version: String::new(),
            cached: !directory,
            dirty: true,
            deleted: false,
            last_access: now(),
            modified: now(),
        };
        let occupied = self
            .children(&n.parent)?
            .into_iter()
            .map(|x| x.alias.to_lowercase())
            .collect();
        ensure!(
            names::alias(name, &n.id, &occupied) == name,
            "Name contains unsupported Windows characters"
        );
        if !directory {
            File::create(self.cache(&n.id))?.sync_all()?;
        } else {
            n.cached = false;
        }
        self.store.stage(&n, "reconcile")?;
        Ok(n)
    }
    pub fn can_delete(&mut self, id: &str) -> Result<()> {
        let n = self.store.get(id)?;
        ensure!(n.id != self.root_id(), "Cannot delete the drive root");
        if n.directory {
            ensure!(self.list(id)?.is_empty(), "Directory is not empty");
        }
        Ok(())
    }
    pub fn delete(&mut self, id: &str) -> Result<()> {
        self.can_delete(id)?;
        let mut n = self.store.get(id)?;
        n.deleted = true;
        self.store.stage(&n, "reconcile")
    }
    pub fn rename(&mut self, id: &str, path: &str, replace_existing: bool) -> Result<()> {
        let (parent, name) = split(path)?;
        let parent = self.resolve(parent)?;
        let mut n = self.store.get(id)?;
        ensure!(parent.directory && n.id != self.root_id(), "Invalid rename");
        let mut ancestor = parent.clone();
        let mut seen = HashSet::new();
        while ancestor.id != self.root_id() {
            ensure!(
                ancestor.id != id && seen.insert(ancestor.id.clone()),
                "Cannot move a directory into itself"
            );
            ancestor = self.store.get(&ancestor.parent)?;
        }
        let target = self
            .list(&parent.id)?
            .into_iter()
            .find(|x| x.alias.to_lowercase() == name.to_lowercase() && x.id != id);
        if let Some(target) = &target {
            ensure!(
                replace_existing && !target.directory && !n.directory,
                "Destination already exists"
            );
            ensure!(
                self.open.get(&target.id).copied().unwrap_or(0) == 0,
                "Destination is open"
            );
        }
        let occupied = self
            .children(&parent.id)?
            .into_iter()
            .filter(|x| x.id != id && target.as_ref().is_none_or(|t| x.id != t.id))
            .map(|x| x.alias.to_lowercase())
            .collect();
        ensure!(
            names::alias(name, id, &occupied) == name,
            "Unsupported Windows name"
        );
        n.parent = parent.id;
        n.name = name.into();
        n.alias = name.into();
        let tx = self.store.db.transaction()?;
        if let Some(mut target) = target {
            target.deleted = true;
            tx.execute(
                "UPDATE nodes SET data=?1 WHERE id=?2",
                rusqlite::params![serde_json::to_string(&target)?, target.id],
            )?;
            tx.execute("INSERT INTO settings VALUES(?1,?2) ON CONFLICT(key) DO UPDATE SET value=excluded.value",rusqlite::params![format!("replace:{id}"),target.id])?;
            // The replacement's job owns the victim until it is safely uploaded.
            tx.execute("DELETE FROM jobs WHERE id=?", [&target.id])?;
        }
        tx.execute(
            "UPDATE nodes SET data=?1 WHERE id=?2",
            rusqlite::params![serde_json::to_string(&n)?, id],
        )?;
        tx.execute("INSERT INTO jobs(id,kind) VALUES(?1,'reconcile') ON CONFLICT(id) DO UPDATE SET attempts=0,error=NULL,next_try=0",[id])?;
        tx.commit()?;
        Ok(())
    }
    pub(crate) fn has_job(&self, id: &str) -> bool {
        self.store
            .db
            .query_row("SELECT count(*) FROM jobs WHERE id=?", [id], |r| {
                r.get::<_, u32>(0)
            })
            .unwrap_or(0)
            > 0
    }
    pub(crate) fn sync_one(&mut self, id: &str) -> Result<()> {
        if self.open.get(id).copied().unwrap_or(0) > 0 {
            return Ok(());
        }
        let mut n = self.store.get(id)?;
        let remote = self.remote.get(id)?;
        if n.deleted
            && n.directory
            && self
                .store
                .all()?
                .iter()
                .any(|child| child.parent == id && child.id != id && self.has_job(&child.id))
        {
            return Ok(());
        }
        if self.store.setting("scope_folder").is_some() && !n.deleted {
            let parent = self
                .remote
                .get(&n.parent)?
                .context("The target folder is unavailable; pending edits were retained")?;
            ensure!(
                parent.directory && self.in_scope(&parent)?,
                "The target folder moved outside StorageBridge; pending edits were retained"
            );
        }
        if let Some(existing) = &remote {
            ensure!(
                self.in_scope(existing)?,
                "This file moved outside StorageBridge. No remote change was made; local work was retained."
            );
        }
        if n.deleted {
            if remote.is_some() {
                self.remote.trash(id)?;
            }
            n.dirty = false;
        } else {
            if n.dirty && !n.directory {
                let overlay = self
                    .store
                    .setting(&format!("overlay:{id}"))
                    .map(|s| serde_json::from_str::<crate::streaming::Overlay>(&s))
                    .transpose()?;
                ensure!(
                    (n.cached || overlay.is_some()) && self.cache(id).exists(),
                    "Pending local bytes are missing; retained recovery record"
                );
                if overlay.is_none() {
                    n.size = std::fs::metadata(self.cache(id))?.len();
                }
                if remote.as_ref().is_some_and(|r| r.version != n.version) && !n.version.is_empty()
                    || (remote.is_none() && !n.version.is_empty())
                {
                    ensure!(
                        overlay.is_none(),
                        "Remote file changed. Partial edits are retained locally; manual conflict recovery is required."
                    );
                    // Preserve both versions; never overwrite a detected remote revision.
                    let conflict_id = self.remote.generate_id()?;
                    let mut conflict = n.clone();
                    conflict.id = conflict_id;
                    conflict.version.clear();
                    conflict.name = format!(
                        "{} (conflict {})",
                        n.name,
                        chrono::Utc::now().format("%Y%m%d-%H%M%S")
                    );
                    let occupied = self
                        .children(&conflict.parent)?
                        .into_iter()
                        .map(|x| x.alias.to_lowercase())
                        .collect();
                    conflict.alias = names::alias(&conflict.name, &conflict.id, &occupied);
                    std::fs::copy(self.cache(id), self.cache(&conflict.id))?;
                    OpenOptions::new()
                        .write(true)
                        .open(self.cache(&conflict.id))?
                        .sync_all()?;
                    self.store.stage(&conflict, "reconcile")?;
                    n.dirty = false;
                    n.cached = false;
                    self.store.put(&n)?;
                    self.store.db.execute("DELETE FROM jobs WHERE id=?", [id])?;
                    if let Some(replaced) = self.store.setting(&format!("replace:{id}")) {
                        let mut victim = self.store.get(&replaced)?;
                        victim.deleted = false;
                        self.store.stage(&victim, "reconcile")?;
                        self.store.db.execute(
                            "DELETE FROM settings WHERE key=?",
                            [format!("replace:{id}")],
                        )?;
                    }
                    if let Some(r) = remote {
                        self.merge(r)?;
                    } else {
                        n.deleted = true;
                        self.store.put(&n)?;
                    }
                    self.store
                        .event(&format!("Preserved local edits as {}", conflict.alias));
                    self.store.db.execute("DELETE FROM jobs WHERE id=?", [id])?;
                    return Ok(());
                }
                let uploaded = if let Some(overlay) = overlay {
                    let source = crate::streaming::OverlaySource {
                        remote: self.remote.as_ref(),
                        cache: &self.read_cache,
                        overlay,
                        path: self.cache(id),
                        size: n.size,
                    };
                    self.remote.upload_source(&n, &source, remote.is_some())?
                } else {
                    self.remote.upload(&n, &self.cache(id), remote.is_some())?
                };
                n.version = uploaded.version;
                n.modified = uploaded.modified;
                n.dirty = false;
                let tx = self.store.db.transaction()?;
                tx.execute(
                    "UPDATE nodes SET data=?1 WHERE id=?2",
                    rusqlite::params![serde_json::to_string(&n)?, id],
                )?;
                tx.execute(
                    "DELETE FROM settings WHERE key=?",
                    [format!("overlay:{id}")],
                )?;
                tx.commit()?;
            } else if n.directory && remote.is_none() {
                let created = self.remote.mkdir(&n)?;
                n.version = created.version;
                n.dirty = false;
            }
            if n.directory {
                n.dirty = false;
            }
            // Re-read because an upload may create the file or change its version.
            let r = self
                .remote
                .get(id)?
                .context("Remote file disappeared during synchronization")?;
            if !n.directory && n.version != r.version {
                n.cached = false;
                n.size = r.size;
                if self.cache(id).exists() {
                    std::fs::remove_file(self.cache(id))?;
                }
            }
            n.version = r.version.clone();
            if r.name != n.name || r.parent != n.parent {
                let updated = self.remote.relocate(&n, &r.parent)?;
                n.version = updated.version;
            }
            self.finish_replacements(id)?;
        }
        // Clearing dirty state and queue intent must be one transaction.
        let tx = self.store.db.transaction()?;
        tx.execute(
            "UPDATE nodes SET data=?1 WHERE id=?2",
            rusqlite::params![serde_json::to_string(&n)?, id],
        )?;
        tx.execute("DELETE FROM jobs WHERE id=?", [id])?;
        tx.commit()?;
        self.store.event(&format!(
            "{}: {}",
            if n.deleted {
                "Moved to Trash"
            } else {
                "Uploaded"
            },
            n.alias
        ));
        Ok(())
    }
    fn finish_replacements(&mut self, id: &str) -> Result<()> {
        let mut current = id.to_string();
        let mut seen = HashSet::new();
        let mut chain = vec![];
        while let Some(replaced) = self.store.setting(&format!("replace:{current}")) {
            ensure!(
                seen.insert(replaced.clone()),
                "Replacement chain contains a cycle"
            );
            chain.push((current, replaced.clone()));
            current = replaced;
        }
        // Retain outer links until their descendants are finished, so a crash can retry.
        for (current, replaced) in chain.into_iter().rev() {
            if let Some(victim) = self.remote.get(&replaced)? {
                ensure!(
                    self.in_scope(&victim)?,
                    "The replaced file moved outside StorageBridge; it was not trashed"
                );
                self.remote.trash(&replaced)?;
            }
            let mut victim = self.store.get(&replaced)?;
            victim.dirty = false;
            victim.deleted = true;
            self.store.put(&victim)?;
            self.store.db.execute(
                "DELETE FROM settings WHERE key=?",
                [format!("replace:{current}")],
            )?;
        }
        Ok(())
    }
    pub fn tick(&mut self) -> Result<()> {
        let jobs: Vec<String> = {
            let mut stmt = self
                .store
                .db
                .prepare("SELECT id FROM jobs WHERE next_try<=? ORDER BY rowid LIMIT 16")?;
            stmt.query_map([now()], |r| r.get(0))?
                .collect::<std::result::Result<_, _>>()?
        };
        for id in jobs {
            if let Err(e) = self.sync_one(&id) {
                let attempts: u32 = self.store.db.query_row(
                    "SELECT attempts FROM jobs WHERE id=?",
                    [&id],
                    |r| r.get(0),
                )?;
                let delay = (2i64.pow(attempts.min(8)) + 1).min(300);
                self.store.db.execute(
                    "UPDATE jobs SET attempts=attempts+1,error=?1,next_try=?2 WHERE id=?3",
                    rusqlite::params![e.to_string(), now() + delay, id],
                )?;
                self.last_error = Some(e.to_string());
            }
        }
        self.evict()?;
        Ok(())
    }
    pub fn poll(&mut self) -> Result<()> {
        if self.store.setting("cursor").is_none() {
            return self.initialize();
        }
        let pending: Vec<_> = self.refresh.iter().cloned().collect();
        for id in pending {
            if self.open.get(&id).copied().unwrap_or(0) == 0 {
                if let Some(n) = self.remote.get(&id)?
                    && self.in_scope(&n)?
                {
                    self.merge(n)?;
                }
                self.refresh.remove(&id);
            }
        }
        let cursor = self.store.setting("cursor").unwrap();
        let (changes, next) = self.remote.changes(&cursor)?;
        for (id, n) in changes {
            if id == self.root_id() && n.is_none() {
                anyhow::bail!("The StorageBridge folder was removed. Restore it in Google Drive.");
            }
            if let Some(n) = n
                && self.in_scope(&n)?
            {
                self.merge(n)?;
            } else if let Ok(mut old) = self.store.get(&id)
                && !old.dirty
                && !self.has_job(&id)
            {
                old.deleted = true;
                self.store.put(&old)?;
            }
        }
        self.store.set("cursor", &next)?;
        Ok(())
    }
    pub fn evict(&mut self) -> Result<()> {
        let online_only = self.online_only();
        let budget = if online_only {
            0
        } else {
            self.store
                .setting("cache_budget")
                .and_then(|s| s.parse().ok())
                .unwrap_or(DEFAULT_BUDGET)
        };
        let mut nodes = self.store.all()?;
        for n in &nodes {
            if !n.cached
                && !n.dirty
                && !self.has_job(&n.id)
                && !self.open.contains_key(&n.id)
                && self.cache(&n.id).exists()
            {
                std::fs::remove_file(self.cache(&n.id))?;
            }
        }
        let mut total: u64 = nodes
            .iter()
            .filter(|n| n.cached)
            .map(|n| {
                std::fs::metadata(self.cache(&n.id))
                    .map(|m| m.len())
                    .unwrap_or(0)
            })
            .sum();
        nodes.sort_by_key(|n| n.last_access);
        for mut n in nodes {
            if total <= budget {
                break;
            }
            if n.cached
                && !n.dirty
                && !self.has_job(&n.id)
                && self.open.get(&n.id).copied().unwrap_or(0) == 0
            {
                let size = std::fs::metadata(self.cache(&n.id))
                    .map(|m| m.len())
                    .unwrap_or(0);
                if self.cache(&n.id).exists() {
                    std::fs::remove_file(self.cache(&n.id))?;
                }
                n.cached = false;
                self.store.put(&n)?;
                total = total.saturating_sub(size);
            }
        }
        if !online_only && total > budget {
            self.last_error =
                Some("Cache exceeds budget because files are open or awaiting upload".into());
        }
        Ok(())
    }
    pub fn retry(&self) -> Result<()> {
        self.store
            .db
            .execute("UPDATE jobs SET next_try=0,attempts=0,error=NULL", [])?;
        Ok(())
    }
    pub fn status(&self) -> Result<Value> {
        let mut stmt = self
            .store
            .db
            .prepare("SELECT id,attempts,error FROM jobs ORDER BY rowid")?;
        let jobs = stmt
            .query_map([], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, u32>(1)?,
                    r.get::<_, Option<String>>(2)?,
                ))
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        let transfers:Vec<_>=jobs.into_iter().map(|(id,attempts,error)|json!({"id":id,"name":self.store.get(&id).map(|n|n.alias).unwrap_or(id.clone()),"state":if error.is_some(){"retrying"}else if self.open.contains_key(&id){"saved_locally"}else{"queued"},"attempts":attempts,"error":error})).collect();
        let cache = std::fs::read_dir(self.root.join("cache"))?.try_fold(
            0u64,
            |total, entry| -> Result<u64> {
                let metadata = match entry?.metadata() {
                    Ok(metadata) => metadata,
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(total),
                    Err(error) => return Err(error.into()),
                };
                Ok(total
                    + if metadata.is_file() {
                        metadata.len()
                    } else {
                        0
                    })
            },
        )?;
        let mut stmt = self
            .store
            .db
            .prepare("SELECT time,message FROM events ORDER BY seq DESC LIMIT 30")?;
        let events = stmt
            .query_map([], |r| {
                Ok(json!({"time":r.get::<_,i64>(0)?,"message":r.get::<_,String>(1)?}))
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(
            json!({"open_files":self.open.len(),"streaming":true,"read_cache_bytes":self.read_cache.bytes(),"read_cache_budget":crate::streaming::RAM_BUDGET,"online_only":self.online_only(),"pending":transfers.len(),"transfers":transfers,"cache_bytes":cache,"cache_budget":self.store.setting("cache_budget").and_then(|s|s.parse::<u64>().ok()).unwrap_or(DEFAULT_BUDGET),"startup":self.store.setting("startup").as_deref()==Some("true"),"drive_letter":self.store.setting("drive_letter").unwrap_or("G:".into()),"last_error":self.last_error,"about":self.store.setting("about").and_then(|s|serde_json::from_str::<Value>(&s).ok()),"events":events}),
        )
    }
}
fn split(path: &str) -> Result<(&str, &str)> {
    let path = path.trim_end_matches(['\\', '/']);
    let (parent, name) = path.rsplit_once(['\\', '/']).unwrap_or(("", path));
    ensure!(
        !name.is_empty() && name != "." && name != "..",
        "Invalid name"
    );
    Ok((parent, name))
}
pub fn replace(from: &Path, to: &Path) -> Result<()> {
    #[cfg(windows)]
    {
        crate::platform::replace_file(from, to)
    }
    #[cfg(not(windows))]
    {
        std::fs::rename(from, to)?;
        Ok(())
    }
}
