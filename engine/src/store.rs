use anyhow::Result;
use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Node {
    pub id: String,
    pub parent: String,
    pub name: String,
    pub alias: String,
    pub directory: bool,
    pub size: u64,
    pub version: String,
    pub cached: bool,
    pub dirty: bool,
    pub deleted: bool,
    pub last_access: i64,
    pub modified: i64,
}

pub struct Store {
    pub db: Connection,
}
impl Store {
    pub fn open(path: &Path) -> Result<Self> {
        let db = Connection::open(path)?;
        db.busy_timeout(std::time::Duration::from_secs(3))?;
        db.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=FULL;
            CREATE TABLE IF NOT EXISTS nodes(id TEXT PRIMARY KEY, data TEXT NOT NULL);
            CREATE TABLE IF NOT EXISTS settings(key TEXT PRIMARY KEY, value TEXT NOT NULL);
            CREATE TABLE IF NOT EXISTS jobs(id TEXT PRIMARY KEY, kind TEXT NOT NULL, attempts INTEGER NOT NULL DEFAULT 0, error TEXT, next_try INTEGER NOT NULL DEFAULT 0);
            CREATE TABLE IF NOT EXISTS events(seq INTEGER PRIMARY KEY AUTOINCREMENT, time INTEGER NOT NULL, message TEXT NOT NULL);")?;
        Ok(Self { db })
    }
    pub fn all(&self) -> Result<Vec<Node>> {
        let mut stmt = self.db.prepare("SELECT data FROM nodes")?;
        let raw = stmt
            .query_map([], |r| r.get::<_, String>(0))?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        raw.into_iter()
            .map(|s| Ok(serde_json::from_str(&s)?))
            .collect()
    }
    pub fn get(&self, id: &str) -> Result<Node> {
        let raw: String = self
            .db
            .query_row("SELECT data FROM nodes WHERE id=?", [id], |r| r.get(0))?;
        Ok(serde_json::from_str(&raw)?)
    }
    pub fn put(&self, node: &Node) -> Result<()> {
        self.db.execute(
            "INSERT INTO nodes VALUES(?1,?2) ON CONFLICT(id) DO UPDATE SET data=excluded.data",
            params![node.id, serde_json::to_string(node)?],
        )?;
        Ok(())
    }
    /// Metadata and intent are committed together; a crash cannot lose queued intent.
    pub fn stage(&mut self, node: &Node, kind: &str) -> Result<()> {
        let tx = self.db.transaction()?;
        tx.execute(
            "INSERT INTO nodes VALUES(?1,?2) ON CONFLICT(id) DO UPDATE SET data=excluded.data",
            params![node.id, serde_json::to_string(node)?],
        )?;
        tx.execute("INSERT INTO jobs(id,kind) VALUES(?1,?2) ON CONFLICT(id) DO UPDATE SET kind=excluded.kind,attempts=0,error=NULL,next_try=0", params![node.id,kind])?;
        tx.commit()?;
        Ok(())
    }
    pub fn setting(&self, key: &str) -> Option<String> {
        self.db
            .query_row("SELECT value FROM settings WHERE key=?", [key], |r| {
                r.get(0)
            })
            .ok()
    }
    pub fn set(&self, key: &str, value: &str) -> Result<()> {
        self.db.execute("INSERT INTO settings VALUES(?1,?2) ON CONFLICT(key) DO UPDATE SET value=excluded.value", [key,value])?;
        Ok(())
    }
    pub fn event(&self, message: &str) {
        let _ = self.db.execute(
            "INSERT INTO events(time,message) VALUES(?1,?2)",
            params![chrono::Utc::now().timestamp(), message],
        );
        let _ = self.db.execute(
            "DELETE FROM events WHERE seq < (SELECT coalesce(max(seq),0)-200 FROM events)",
            [],
        );
    }
}
