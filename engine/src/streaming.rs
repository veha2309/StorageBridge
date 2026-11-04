//! Bounded, version-keyed RAM reads and durable write overlays.
use crate::{drive::Remote, store::Node};
use anyhow::{Context, Result, ensure};
use parking_lot::{Condvar, Mutex};
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, VecDeque},
    fs::File,
    io::{Read, Seek, SeekFrom},
    path::{Path, PathBuf},
    sync::Arc,
};

pub const BLOCK: usize = 1024 * 1024;
pub const RAM_BUDGET: usize = 32 * 1024 * 1024;
pub const MAX_RANGE: usize = 8 * BLOCK;
struct Entry {
    id: String,
    version: String,
    offset: u64,
    data: Arc<Vec<u8>>,
}
#[derive(Default)]
pub struct ReadCache {
    entries: Mutex<VecDeque<Entry>>,
    next: Mutex<HashMap<(String, String), u64>>,
    active: Mutex<usize>,
    available: Condvar,
}
struct Permit<'a>(&'a ReadCache);
impl Drop for Permit<'_> {
    fn drop(&mut self) {
        *self.0.active.lock() -= 1;
        self.0.available.notify_one();
    }
}
impl ReadCache {
    pub fn bytes(&self) -> usize {
        self.entries.lock().iter().map(|e| e.data.len()).sum()
    }
    fn block(&self, remote: &dyn Remote, node: &Node, offset: u64) -> Result<(u64, Arc<Vec<u8>>)> {
        let version = node.content_version();
        {
            let mut entries = self.entries.lock();
            if let Some(i) = entries.iter().position(|e| {
                e.id == node.id
                    && e.version == version
                    && offset >= e.offset
                    && offset - e.offset < e.data.len() as u64
            }) {
                let entry = entries.remove(i).unwrap();
                let data = entry.data.clone();
                let base = entry.offset;
                entries.push_back(entry);
                return Ok((base, data));
            }
        }
        {
            let mut active = self.active.lock();
            while *active >= 4 {
                self.available.wait(&mut active);
            }
            *active += 1;
        }
        let _permit = Permit(self);
        let sequential = self.next.lock().get(&(node.id.clone(), version.clone())) == Some(&offset);
        let window = if sequential {
            remote.sequential_range_size().clamp(BLOCK, MAX_RANGE)
        } else {
            BLOCK
        };
        let length = (node.size.saturating_sub(offset)).min(window as u64) as usize;
        let bytes = remote.read_range(node, offset, length)?;
        ensure!(
            bytes.len() == length,
            "Incomplete ranged read; no data was cached"
        );
        let data = Arc::new(bytes);
        {
            let mut next = self.next.lock();
            if next.len() >= 256 {
                next.clear();
            }
            next.insert(
                (node.id.clone(), version.clone()),
                offset + data.len() as u64,
            );
        }
        let mut entries = self.entries.lock();
        // Another caller may have fetched this same block while we waited.
        entries.retain(|e| !(e.id == node.id && e.version == version && e.offset == offset));
        while entries.iter().map(|e| e.data.len()).sum::<usize>() + data.len() > RAM_BUDGET {
            entries.pop_front();
        }
        entries.push_back(Entry {
            id: node.id.clone(),
            version,
            offset,
            data: data.clone(),
        });
        Ok((offset, data))
    }
    pub fn read(
        &self,
        remote: &dyn Remote,
        node: &Node,
        offset: u64,
        buffer: &mut [u8],
    ) -> Result<usize> {
        let length = node.size.saturating_sub(offset).min(buffer.len() as u64) as usize;
        let mut done = 0;
        while done < length {
            let at = offset + done as u64;
            let start = at / BLOCK as u64 * BLOCK as u64;
            let (base, bytes) = self.block(remote, node, start)?;
            let within = (at - base) as usize;
            let count = (length - done).min(bytes.len().saturating_sub(within));
            ensure!(count > 0, "Invalid ranged read length");
            buffer[done..done + count].copy_from_slice(&bytes[within..within + count]);
            done += count;
        }
        Ok(done)
    }
}

pub trait ContentSource {
    fn read_exact_at(&self, offset: u64, bytes: &mut [u8]) -> Result<()>;
    fn fingerprint(&self) -> Result<u128>;
}
pub struct FileSource<'a>(pub &'a Path);
impl ContentSource for FileSource<'_> {
    fn read_exact_at(&self, offset: u64, bytes: &mut [u8]) -> Result<()> {
        let mut file = File::open(self.0)?;
        file.seek(SeekFrom::Start(offset))?;
        file.read_exact(bytes)?;
        Ok(())
    }
    fn fingerprint(&self) -> Result<u128> {
        Ok(self
            .0
            .metadata()?
            .modified()?
            .duration_since(std::time::UNIX_EPOCH)?
            .as_nanos())
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Segment {
    pub start: u64,
    pub end: u64,
    pub stored: u64,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Overlay {
    pub base: Node,
    pub base_limit: u64,
    pub segments: Vec<Segment>,
}
impl Overlay {
    pub fn new(base: Node) -> Self {
        Self {
            base_limit: base.size,
            base,
            segments: vec![],
        }
    }
    pub fn patch(&mut self, start: u64, length: u64, stored: u64) -> Result<()> {
        let end = start.checked_add(length).context("File offset overflow")?;
        let mut segments = vec![];
        for s in &self.segments {
            if s.end <= start || s.start >= end {
                segments.push(s.clone());
            } else {
                if s.start < start {
                    segments.push(Segment {
                        start: s.start,
                        end: start,
                        stored: s.stored,
                    });
                }
                if s.end > end {
                    segments.push(Segment {
                        start: end,
                        end: s.end,
                        stored: s.stored + end - s.start,
                    });
                }
            }
        }
        if length > 0 {
            segments.push(Segment { start, end, stored });
        }
        segments.sort_by_key(|s| s.start);
        self.segments = segments;
        Ok(())
    }
    pub fn truncate(&mut self, size: u64) {
        self.base_limit = self.base_limit.min(size);
        self.segments.retain(|s| s.start < size);
        for s in &mut self.segments {
            s.end = s.end.min(size);
        }
    }
}
pub struct OverlaySource<'a> {
    pub remote: &'a dyn Remote,
    pub cache: &'a ReadCache,
    pub overlay: Overlay,
    pub path: PathBuf,
    pub size: u64,
}
impl ContentSource for OverlaySource<'_> {
    fn read_exact_at(&self, offset: u64, bytes: &mut [u8]) -> Result<()> {
        let end = offset
            .checked_add(bytes.len() as u64)
            .context("File offset overflow")?;
        ensure!(end <= self.size, "Read exceeds logical file length");
        bytes.fill(0);
        let mut cursor = offset;
        let base_end = end.min(self.overlay.base_limit);
        for s in &self.overlay.segments {
            if s.end <= offset || s.start >= end {
                continue;
            }
            if cursor < s.start && cursor < base_end {
                let limit = s.start.min(base_end);
                self.cache.read(
                    self.remote,
                    &self.overlay.base,
                    cursor,
                    &mut bytes[(cursor - offset) as usize..(limit - offset) as usize],
                )?;
            }
            cursor = cursor.max(s.end);
        }
        if cursor < base_end {
            self.cache.read(
                self.remote,
                &self.overlay.base,
                cursor,
                &mut bytes[(cursor - offset) as usize..(base_end - offset) as usize],
            )?;
        }
        if !self.overlay.segments.is_empty() {
            let mut file = File::open(&self.path)?;
            for s in &self.overlay.segments {
                let start = s.start.max(offset);
                let limit = s.end.min(end);
                if start < limit {
                    file.seek(SeekFrom::Start(s.stored + start - s.start))?;
                    file.read_exact(
                        &mut bytes[(start - offset) as usize..(limit - offset) as usize],
                    )?;
                }
            }
        }
        Ok(())
    }
    fn fingerprint(&self) -> Result<u128> {
        use sha2::{Digest, Sha256};
        let bytes = serde_json::to_vec(&self.overlay)?;
        let hash = Sha256::digest(bytes);
        Ok(u128::from_le_bytes(hash[..16].try_into().unwrap()))
    }
}
