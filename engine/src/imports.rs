//! Durable source-backed uploads. Source content never enters the managed cache.
use crate::{
    runtime::Runtime,
    source::{Source, Stamp},
    store::{Node, Store},
};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::path::{Path, PathBuf};

#[derive(Clone, Serialize, Deserialize)]
pub struct Import {
    pub id: String,
    pub source: PathBuf,
    pub stamp: Stamp,
    pub name: String,
    pub parent: String,
    pub move_source: bool,
    pub remote_id: Option<String>,
    pub sha256: Option<String>,
    pub state: String,
    pub error: Option<String>,
    pub attempts: u32,
    pub next_try: i64,
    #[serde(default)]
    pub deletion_started: bool,
}
impl Import {
    pub fn terminal(&self) -> bool {
        matches!(
            self.state.as_str(),
            "complete" | "source_retained" | "dismissed"
        )
    }
    pub fn save(&self, store: &Store) -> Result<()> {
        store.db.execute(
            "INSERT INTO imports VALUES(?1,?2) ON CONFLICT(id) DO UPDATE SET data=excluded.data",
            rusqlite::params![self.id, serde_json::to_string(self)?],
        )?;
        Ok(())
    }
    pub fn all(store: &Store) -> Result<Vec<Self>> {
        let mut stmt = store
            .db
            .prepare("SELECT data FROM imports ORDER BY rowid")?;
        let rows = stmt
            .query_map([], |r| r.get::<_, String>(0))?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        rows.into_iter()
            .map(|row| Ok(serde_json::from_str(&row)?))
            .collect()
    }
}
impl Runtime {
    pub fn queue_import(&self, path: &Path, folder: &str, move_source: bool) -> Result<String> {
        let source = Source::open(path, move_source)?;
        let parent = self.resolve(folder)?;
        ensure!(parent.directory, "Choose a destination folder");
        let name = source
            .path
            .file_name()
            .and_then(|n| n.to_str())
            .context("Unsupported source filename")?
            .to_string();
        ensure!(
            !self
                .list(&parent.id)?
                .iter()
                .any(|n| n.name.eq_ignore_ascii_case(&name) || n.alias.eq_ignore_ascii_case(&name)),
            "Destination name already exists; choose another folder or rename the source first"
        );
        let e = self.engine.lock();
        ensure!(
            !source.path.starts_with(std::fs::canonicalize(&e.root)?),
            "The engine's own data files cannot be direct-transfer sources"
        );
        let mut jobs = Import::all(&e.store)?;
        ensure!(
            !jobs.iter().any(|j| !j.terminal()
                && (j.stamp.identity == source.stamp.identity
                    || j.parent == parent.id && j.name.eq_ignore_ascii_case(&name))),
            "This source or destination already has a pending direct transfer"
        );
        // Bound retained history and status frame size without touching active work.
        jobs.retain(Import::terminal);
        for old in jobs.iter().take(jobs.len().saturating_sub(49)) {
            e.store
                .db
                .execute("DELETE FROM imports WHERE id=?", [&old.id])?;
        }
        ensure!(
            Import::all(&e.store)?
                .iter()
                .filter(|j| !j.terminal())
                .count()
                < 32,
            "Finish or dismiss pending direct transfers first"
        );
        let job = Import {
            id: uuid::Uuid::new_v4().simple().to_string(),
            source: source.path.clone(),
            stamp: source.stamp.clone(),
            name,
            parent: parent.id,
            move_source,
            remote_id: None,
            sha256: None,
            state: "queued".into(),
            error: None,
            attempts: 0,
            next_try: 0,
            deletion_started: false,
        };
        job.save(&e.store)?;
        Ok(job.id)
    }
    pub fn dismiss_import(&self, id: &str) -> Result<()> {
        let gate = self.gate(&format!("import:{id}"));
        let _guard = gate.try_lock().context(
            "Transfer is active; wait for completion or a retryable error before dismissing",
        )?;
        let e = self.engine.lock();
        let mut job = Import::all(&e.store)?
            .into_iter()
            .find(|j| j.id == id)
            .context("Transfer not found")?;
        ensure!(!job.terminal(), "Transfer already finished");
        job.state = "dismissed".into();
        job.error = None;
        job.save(&e.store)
    }
    pub fn tick_imports(&self) -> Result<()> {
        let selected = {
            let e = self.engine.lock();
            Import::all(&e.store)?
                .into_iter()
                .find(|j| !j.terminal() && j.next_try <= chrono::Utc::now().timestamp())
        };
        let Some(selected) = selected else {
            return Ok(());
        };
        let gate = self.gate(&format!("import:{}", selected.id));
        let Some(_guard) = gate.try_lock() else {
            return Ok(());
        };
        // A dismissal can win the gate after selection. Never run that stale snapshot.
        let Some(mut job) = Import::all(&self.engine.lock().store)?
            .into_iter()
            .find(|j| j.id == selected.id && !j.terminal())
        else {
            return Ok(());
        };
        if let Err(error) = self.run_import(&mut job) {
            job.error = Some(error.to_string());
            job.attempts += 1;
            job.next_try = chrono::Utc::now().timestamp() + (1i64 << job.attempts.min(8));
            // Keep a deletion recovery marker if the process stopped after verified removal.
            if job.state != "deleting" {
                job.state = "error".into();
            }
            job.save(&self.engine.lock().store)?;
        }
        Ok(())
    }
    fn import_phase(&self, job: &mut Import, phase: &str) -> Result<()> {
        job.state = phase.into();
        job.error = None;
        job.save(&self.engine.lock().store)
    }
    fn run_import(&self, job: &mut Import) -> Result<()> {
        let worker = self.engine.lock().attached()?;
        let parent = worker
            .remote
            .get(&job.parent)?
            .context("Destination folder is unavailable")?;
        ensure!(
            parent.directory && worker.in_scope(&parent)?,
            "Destination is outside the mounted folder; source retained"
        );
        if job.remote_id.is_none() {
            ensure!(
                !worker
                    .remote
                    .list(&parent.id)?
                    .iter()
                    .any(|n| n.name.eq_ignore_ascii_case(&job.name)),
                "Destination name already exists; source retained"
            );
            job.remote_id = Some(worker.remote.generate_id()?);
            job.save(&self.engine.lock().store)?;
        }
        let id = job.remote_id.clone().context("Missing upload identity")?;
        let existing = worker.remote.get(&id)?;
        let source = Source::open(&job.source, job.move_source);
        let source = match source {
            Ok(source) if source.stamp == job.stamp => Some(source),
            Ok(_) | Err(_) if existing.is_some() && job.sha256.is_some() => None,
            Ok(_) => {
                anyhow::bail!("Source identity or content changed; no source removal performed")
            }
            Err(error) => return Err(error),
        };
        if let Some(source) = source.as_ref() {
            self.import_phase(job, "hashing")?;
            let hash = source.digest()?;
            ensure!(
                job.sha256.as_ref().is_none_or(|saved| saved == &hash),
                "Source content changed since transfer began; source retained"
            );
            job.sha256 = Some(hash);
            job.save(&self.engine.lock().store)?;
        }
        let uploaded = if let Some(existing) = existing {
            ensure!(
                existing.parent == job.parent && worker.in_scope(&existing)?,
                "Uploaded file moved; source retained"
            );
            existing
        } else {
            let source = source
                .as_ref()
                .context("Source is unavailable; transfer cannot resume")?;
            self.import_phase(job, "uploading")?;
            let node = Node {
                id: id.clone(),
                parent: job.parent.clone(),
                name: job.name.clone(),
                alias: job.name.clone(),
                directory: false,
                size: job.stamp.size,
                version: String::new(),
                head_revision: None,
                cached: false,
                dirty: false,
                deleted: false,
                last_access: chrono::Utc::now().timestamp(),
                modified: chrono::Utc::now().timestamp(),
            };
            worker.remote.upload_source(&node, source, false)?
        };
        self.import_phase(job, "verifying")?;
        ensure!(
            uploaded.size == job.stamp.size,
            "Uploaded size differs; original retained"
        );
        let checksum = worker.remote.sha256(&uploaded)?.context("Upload completed, but a provider SHA-256 checksum is not available yet. Original retained; retry verification")?;
        ensure!(
            job.sha256.as_deref() == Some(checksum.as_str()),
            "Uploaded checksum differs; original retained"
        );
        // Check that this is still the same cloud revision before publishing/deleting.
        ensure!(
            worker
                .remote
                .get(&id)?
                .is_some_and(|n| n.version == uploaded.version
                    && n.parent == job.parent
                    && n.size == job.stamp.size),
            "Remote content changed after verification; original retained"
        );
        self.engine.lock().merge(uploaded.clone())?;
        if job.move_source {
            if let Some(source) = source {
                // The Windows lease denied writers throughout hashing and upload.
                // Recheck cloud state immediately before deletion, without a second long hash pass.
                source.unchanged()?;
                ensure!(
                    worker
                        .remote
                        .get(&id)?
                        .is_some_and(|n| n.version == uploaded.version
                            && n.parent == job.parent
                            && n.size == job.stamp.size),
                    "Cloud revision changed before source removal; original retained"
                );
                job.deletion_started = true;
                self.import_phase(job, "deleting")?;
                source.remove_verified_original()?;
                drop(source); // Windows deletes the exact leased file when its handle closes.
            } else if !job.deletion_started || job.source.exists() {
                job.state = "source_retained".into();
                job.error = Some("Upload verified; source changed or is unavailable. No source removal was performed".into());
                return job.save(&self.engine.lock().store);
            }
        }
        job.state = "complete".into();
        job.error = None;
        job.save(&self.engine.lock().store)?;
        self.engine.lock().store.event(&format!(
            "Direct upload verified: {}{}",
            job.name,
            if job.move_source {
                "; original removed"
            } else {
                "; source retained"
            }
        ));
        Ok(())
    }
}
pub fn status(store: &Store) -> Result<serde_json::Value> {
    let jobs = Import::all(store)?;
    let pending = jobs.iter().filter(|j| !j.terminal()).count();
    Ok(
        json!({"pending":pending,"source_bytes":jobs.iter().filter(|j| !j.terminal()).map(|j| j.stamp.size).sum::<u64>(),
        "jobs":jobs.iter().map(|j| json!({"id":j.id,"name":j.name,"size":j.stamp.size,"move_source":j.move_source,
            "state":j.state,"error":j.error,"attempts":j.attempts})).collect::<Vec<_>>()}),
    )
}
