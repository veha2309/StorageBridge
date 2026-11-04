//! A hard ceiling on managed content lengths, independent of clean-cache retention.
use crate::engine::Engine;
use anyhow::{Context, Result, ensure};
use serde::Serialize;
use std::{collections::HashMap, fmt};

pub const DEFAULT_CONTENT_LIMIT: u64 = 1024 * 1024 * 1024;
pub const MAX_CONTENT_LIMIT: u64 = 16 * 1024 * 1024 * 1024 * 1024;

#[derive(Debug)]
pub struct ContentLimitExceeded {
    pub used: u64,
    pub additional: u64,
    pub limit: u64,
}
impl fmt::Display for ContentLimitExceeded {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Local content limit reached: {} bytes stored, {} additional bytes requested, {} byte limit. Close files and let uploads finish, or increase the local content limit in Settings. Previously accepted edits are retained; this write was not accepted.",
            self.used, self.additional, self.limit
        )
    }
}
impl std::error::Error for ContentLimitExceeded {}

#[derive(Serialize)]
pub struct ContentFile {
    pub name: String,
    pub bytes: u64,
    pub reason: &'static str,
}
#[derive(Serialize)]
pub struct StorageUsage {
    pub limit_bytes: u64,
    pub used_bytes: u64,
    pub available_bytes: u64,
    pub pending_bytes: u64,
    pub retained_bytes: u64,
    pub recovery_bytes: u64,
    pub over_limit: bool,
    pub write_blocked: bool,
    pub file_count: usize,
    pub files_truncated: bool,
    pub files: Vec<ContentFile>,
}

impl Engine {
    pub fn content_limit(&self) -> u64 {
        self.store
            .setting("content_limit")
            .and_then(|v| v.parse().ok())
            .filter(|v| (1..=MAX_CONTENT_LIMIT).contains(v))
            .unwrap_or(DEFAULT_CONTENT_LIMIT)
    }

    /// Counts every content file, including partial and orphan/recovery journals.
    pub fn content_bytes(&self) -> Result<u64> {
        let mut total = 0u64;
        for entry in std::fs::read_dir(self.root.join("cache"))? {
            let entry = entry?;
            let metadata = match entry.metadata() {
                Ok(value) => value,
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
                Err(e) => return Err(e.into()),
            };
            if metadata.is_file() {
                total = total
                    .checked_add(metadata.len())
                    .context("Content size overflow")?;
            }
        }
        Ok(total)
    }

    // Caller holds content_writes. Never removes pending, unknown, or open content.
    pub(crate) fn trim_content(&mut self, target: u64, protected: Option<&str>) -> Result<u64> {
        let mut used = self.content_bytes()?;
        let mut nodes = self.store.all()?;
        nodes.sort_by_key(|n| n.last_access);
        for mut n in nodes {
            if used <= target {
                break;
            }
            if n.directory
                || n.dirty
                || self.has_job(&n.id)
                || self.open.contains_key(&n.id)
                || protected == Some(n.id.as_str())
            {
                continue;
            }
            let path = self.cache(&n.id);
            let size = match std::fs::metadata(&path) {
                Ok(metadata) => metadata.len(),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
                Err(e) => return Err(e.into()),
            };
            std::fs::remove_file(path)?;
            n.cached = false;
            self.store.put(&n)?;
            used = used.saturating_sub(size);
        }
        Ok(used)
    }

    /// Caller holds the shared content_writes lock until disk growth is complete.
    pub(crate) fn check_content_growth(
        &mut self,
        additional: u64,
        protected: Option<&str>,
    ) -> Result<()> {
        if additional == 0 {
            return Ok(());
        }
        let limit = self.content_limit();
        let used = self.trim_content(limit.saturating_sub(additional), protected)?;
        if used
            .checked_add(additional)
            .is_none_or(|total| total > limit)
        {
            self.store
                .set("content_rejected_growth", &additional.to_string())?;
            return Err(ContentLimitExceeded {
                used,
                additional,
                limit,
            }
            .into());
        }
        if self.store.setting("content_rejected_growth").is_some() {
            self.store.db.execute(
                "DELETE FROM settings WHERE key='content_rejected_growth'",
                [],
            )?;
        }
        Ok(())
    }

    pub fn set_content_limit(&mut self, limit: u64) -> Result<()> {
        ensure!(
            (1..=MAX_CONTENT_LIMIT).contains(&limit),
            "Invalid local content limit"
        );
        let writes = self.content_writes.clone();
        let _guard = writes.lock();
        let used = self.trim_content(limit, None)?;
        ensure!(
            used <= limit,
            "Cannot lower the local content limit below {used} protected bytes. Close files and let uploads finish first. Pending edits were retained; the previous limit is unchanged."
        );
        self.store.set("content_limit", &limit.to_string())?;
        Ok(())
    }

    pub fn release_clean_content(&mut self) -> Result<()> {
        let writes = self.content_writes.clone();
        let _guard = writes.lock();
        self.trim_content(0, None)?;
        Ok(())
    }

    pub fn storage_usage(&self) -> Result<StorageUsage> {
        let nodes: HashMap<_, _> = self
            .store
            .all()?
            .into_iter()
            .map(|n| (self.cache(&n.id), n))
            .collect();
        let mut files = vec![];
        let (mut used, mut pending, mut retained, mut recovery) = (0u64, 0u64, 0u64, 0u64);
        for entry in std::fs::read_dir(self.root.join("cache"))? {
            let entry = entry?;
            let metadata = match entry.metadata() {
                Ok(value) => value,
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
                Err(e) => return Err(e.into()),
            };
            if !metadata.is_file() {
                continue;
            }
            let size = metadata.len();
            used = used.checked_add(size).context("Content size overflow")?;
            let (name, reason) = if let Some(n) = nodes.get(&entry.path()) {
                if n.dirty || self.has_job(&n.id) {
                    pending += size;
                    (n.alias.clone(), "pending_upload")
                } else {
                    retained += size;
                    (
                        n.alias.clone(),
                        if self.open.contains_key(&n.id) {
                            "open_file"
                        } else {
                            "retained_copy"
                        },
                    )
                }
            } else {
                recovery += size;
                ("Recovery data".into(), "recovery_data")
            };
            files.push(ContentFile {
                name,
                bytes: size,
                reason,
            });
        }
        files.sort_by(|a, b| b.bytes.cmp(&a.bytes).then(a.name.cmp(&b.name)));
        let count = files.len();
        files.truncate(100);
        let limit = self.content_limit();
        let available = limit.saturating_sub(used);
        let rejected = self
            .store
            .setting("content_rejected_growth")
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(0);
        Ok(StorageUsage {
            limit_bytes: limit,
            used_bytes: used,
            available_bytes: available,
            pending_bytes: pending,
            retained_bytes: retained,
            recovery_bytes: recovery,
            over_limit: used > limit,
            write_blocked: used >= limit || rejected > available,
            file_count: count,
            files_truncated: count > files.len(),
            files,
        })
    }
}
