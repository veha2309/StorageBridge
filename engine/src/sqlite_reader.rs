//! An immutable, read-only SQLite VFS backed exclusively by version-pinned range reads.
use crate::{drive::Remote, store::Node, streaming::ReadCache};
use anyhow::{Context, Result, ensure};
use parking_lot::Mutex;
use rusqlite::{Connection, OpenFlags, ffi, limits::Limit, types::ValueRef};
use serde_json::{Value, json};
use std::{
    collections::HashMap,
    ffi::{CStr, c_char, c_int, c_void},
    sync::{Arc, OnceLock},
    time::{Duration, Instant},
};

const NAME: &str = "storagebridge_readonly_v1";
struct View {
    remote: Arc<dyn Remote>,
    cache: Arc<ReadCache>,
    node: Node,
    deadline: Instant,
    error: Mutex<Option<String>>,
}
static VIEWS: OnceLock<Mutex<HashMap<String, Arc<View>>>> = OnceLock::new();
fn views() -> &'static Mutex<HashMap<String, Arc<View>>> {
    VIEWS.get_or_init(Default::default)
}
#[repr(C)]
struct DbFile {
    base: ffi::sqlite3_file,
    view: *mut Arc<View>,
}

unsafe extern "C" fn close(file: *mut ffi::sqlite3_file) -> c_int {
    unsafe {
        let file = &mut *file.cast::<DbFile>();
        drop(Box::from_raw(file.view));
        file.view = std::ptr::null_mut();
        file.base.pMethods = std::ptr::null();
    }
    ffi::SQLITE_OK
}
unsafe extern "C" fn read(
    file: *mut ffi::sqlite3_file,
    output: *mut c_void,
    amount: c_int,
    offset: i64,
) -> c_int {
    if amount < 0 || offset < 0 {
        return ffi::SQLITE_IOERR_READ;
    }
    // SQLite owns these buffers and invokes xRead only after a successful xOpen.
    let view = unsafe { &**(*file.cast::<DbFile>()).view };
    let output = unsafe { std::slice::from_raw_parts_mut(output.cast::<u8>(), amount as usize) };
    output.fill(0);
    if Instant::now() > view.deadline {
        *view.error.lock() = Some("SQLite query exceeded its time limit".into());
        return ffi::SQLITE_INTERRUPT;
    }
    let length = view
        .node
        .size
        .saturating_sub(offset as u64)
        .min(amount as u64) as usize;
    if length > 0 {
        match view.cache.read(
            view.remote.as_ref(),
            &view.node,
            offset as u64,
            &mut output[..length],
        ) {
            Ok(count) if count == length => {}
            Ok(_) => return ffi::SQLITE_IOERR_SHORT_READ,
            Err(error) => {
                *view.error.lock() = Some(error.to_string());
                return ffi::SQLITE_IOERR_READ;
            }
        }
    }
    if length == amount as usize {
        ffi::SQLITE_OK
    } else {
        ffi::SQLITE_IOERR_SHORT_READ
    }
}
unsafe extern "C" fn write(_: *mut ffi::sqlite3_file, _: *const c_void, _: c_int, _: i64) -> c_int {
    ffi::SQLITE_READONLY
}
unsafe extern "C" fn truncate(_: *mut ffi::sqlite3_file, _: i64) -> c_int {
    ffi::SQLITE_READONLY
}
unsafe extern "C" fn sync(_: *mut ffi::sqlite3_file, _: c_int) -> c_int {
    ffi::SQLITE_OK
}
unsafe extern "C" fn size(file: *mut ffi::sqlite3_file, out: *mut i64) -> c_int {
    unsafe {
        let view = &*(*file.cast::<DbFile>()).view;
        *out = view.node.size as i64;
    }
    ffi::SQLITE_OK
}
unsafe extern "C" fn lock(_: *mut ffi::sqlite3_file, level: c_int) -> c_int {
    if level > ffi::SQLITE_LOCK_SHARED {
        ffi::SQLITE_READONLY
    } else {
        ffi::SQLITE_OK
    }
}
unsafe extern "C" fn unlock(_: *mut ffi::sqlite3_file, _: c_int) -> c_int {
    ffi::SQLITE_OK
}
unsafe extern "C" fn reserved(_: *mut ffi::sqlite3_file, out: *mut c_int) -> c_int {
    unsafe {
        *out = 0;
    }
    ffi::SQLITE_OK
}
unsafe extern "C" fn control(_: *mut ffi::sqlite3_file, _: c_int, _: *mut c_void) -> c_int {
    ffi::SQLITE_NOTFOUND
}
unsafe extern "C" fn sector(_: *mut ffi::sqlite3_file) -> c_int {
    4096
}
unsafe extern "C" fn characteristics(_: *mut ffi::sqlite3_file) -> c_int {
    ffi::SQLITE_IOCAP_IMMUTABLE
}
unsafe extern "C" fn open(
    _: *mut ffi::sqlite3_vfs,
    name: *const c_char,
    file: *mut ffi::sqlite3_file,
    flags: c_int,
    out: *mut c_int,
) -> c_int {
    unsafe {
        (*file).pMethods = std::ptr::null();
    }
    if name.is_null()
        || flags & ffi::SQLITE_OPEN_MAIN_DB == 0
        || flags & (ffi::SQLITE_OPEN_READWRITE | ffi::SQLITE_OPEN_CREATE) != 0
    {
        return ffi::SQLITE_CANTOPEN;
    }
    let Ok(name) = (unsafe { CStr::from_ptr(name) }).to_str() else {
        return ffi::SQLITE_CANTOPEN;
    };
    let Some(view) = views().lock().get(name).cloned() else {
        return ffi::SQLITE_CANTOPEN;
    };
    static METHODS: OnceLock<usize> = OnceLock::new();
    let methods = *METHODS.get_or_init(|| {
        let mut methods: ffi::sqlite3_io_methods = unsafe { std::mem::zeroed() };
        methods.iVersion = 1;
        methods.xClose = Some(close);
        methods.xRead = Some(read);
        methods.xWrite = Some(write);
        methods.xTruncate = Some(truncate);
        methods.xSync = Some(sync);
        methods.xFileSize = Some(size);
        methods.xLock = Some(lock);
        methods.xUnlock = Some(unlock);
        methods.xCheckReservedLock = Some(reserved);
        methods.xFileControl = Some(control);
        methods.xSectorSize = Some(sector);
        methods.xDeviceCharacteristics = Some(characteristics);
        Box::into_raw(Box::new(methods)) as usize
    });
    unsafe {
        let file = &mut *file.cast::<DbFile>();
        file.view = Box::into_raw(Box::new(view));
        file.base.pMethods = methods as *const ffi::sqlite3_io_methods;
        if !out.is_null() {
            *out = ffi::SQLITE_OPEN_READONLY;
        }
    }
    ffi::SQLITE_OK
}
unsafe extern "C" fn delete(_: *mut ffi::sqlite3_vfs, _: *const c_char, _: c_int) -> c_int {
    ffi::SQLITE_READONLY
}
unsafe extern "C" fn access(
    _: *mut ffi::sqlite3_vfs,
    _: *const c_char,
    _: c_int,
    out: *mut c_int,
) -> c_int {
    unsafe {
        *out = 0;
    }
    ffi::SQLITE_OK
}
unsafe extern "C" fn path(
    _: *mut ffi::sqlite3_vfs,
    name: *const c_char,
    length: c_int,
    out: *mut c_char,
) -> c_int {
    if name.is_null() {
        return ffi::SQLITE_CANTOPEN;
    }
    let bytes = unsafe { CStr::from_ptr(name) }.to_bytes_with_nul();
    if length < 0 || bytes.len() > length as usize {
        return ffi::SQLITE_CANTOPEN;
    }
    unsafe {
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), out.cast::<u8>(), bytes.len());
    }
    ffi::SQLITE_OK
}
fn register() -> Result<()> {
    static REGISTERED: OnceLock<c_int> = OnceLock::new();
    let result = *REGISTERED.get_or_init(|| unsafe {
        if ffi::sqlite3_initialize() != ffi::SQLITE_OK {
            return ffi::SQLITE_ERROR;
        }
        let base = ffi::sqlite3_vfs_find(std::ptr::null());
        if base.is_null() {
            return ffi::SQLITE_ERROR;
        }
        let mut vfs = Box::new(*base);
        vfs.iVersion = 1;
        vfs.szOsFile = std::mem::size_of::<DbFile>() as c_int;
        vfs.zName = c"storagebridge_readonly_v1".as_ptr();
        vfs.pNext = std::ptr::null_mut();
        vfs.xOpen = Some(open);
        vfs.xDelete = Some(delete);
        vfs.xAccess = Some(access);
        vfs.xFullPathname = Some(path);
        vfs.xDlOpen = None;
        vfs.xDlSym = None;
        vfs.xDlError = None;
        vfs.xDlClose = None;
        // SQLite retains the VFS for the process lifetime; it never points into a stack frame.
        ffi::sqlite3_vfs_register(Box::into_raw(vfs), 0)
    });
    ensure!(
        result == ffi::SQLITE_OK,
        "Cannot register the read-only SQLite reader"
    );
    Ok(())
}
struct Registration(String);
impl Drop for Registration {
    fn drop(&mut self) {
        views().lock().remove(&self.0);
    }
}

pub fn query(
    remote: Arc<dyn Remote>,
    cache: Arc<ReadCache>,
    node: Node,
    sql: &str,
) -> Result<Value> {
    ensure!(
        sql.len() <= 16 * 1024 && !sql.trim().is_empty(),
        "Enter a query up to 16 KiB"
    );
    ensure!(
        !node.directory && node.size >= 100 && node.size <= i64::MAX as u64,
        "Not a standalone SQLite database"
    );
    let mut header = [0; 100];
    cache.read(remote.as_ref(), &node, 0, &mut header)?;
    ensure!(
        &header[..16] == b"SQLite format 3\0",
        "The selected cloud file is not a SQLite database"
    );
    ensure!(
        header[18] == 1 && header[19] == 1,
        "WAL databases are unsupported: export a standalone backup in DELETE journal mode first"
    );
    register()?;
    let key = format!("sqlite_{}", uuid::Uuid::new_v4().simple());
    let deadline = Instant::now() + Duration::from_secs(30);
    let view = Arc::new(View {
        remote: remote.clone(),
        cache,
        node: node.clone(),
        deadline,
        error: Mutex::new(None),
    });
    views().lock().insert(key.clone(), view.clone());
    let _registration = Registration(key.clone());
    let result = (|| -> Result<Value> {
        let connection = Connection::open_with_flags_and_vfs(
            &key,
            OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
            NAME,
        )?;
        connection.set_limit(Limit::SQLITE_LIMIT_LENGTH, 1024 * 1024)?;
        connection.set_limit(Limit::SQLITE_LIMIT_SQL_LENGTH, 16 * 1024)?;
        connection.set_limit(Limit::SQLITE_LIMIT_COLUMN, 64)?;
        connection.set_limit(Limit::SQLITE_LIMIT_ATTACHED, 0)?;
        connection.set_db_config(rusqlite::config::DbConfig::SQLITE_DBCONFIG_DEFENSIVE, true)?;
        connection.execute_batch("PRAGMA query_only=ON; PRAGMA trusted_schema=OFF; PRAGMA temp_store=MEMORY; PRAGMA mmap_size=0; PRAGMA cache_size=-4096;")?;
        connection.progress_handler(1000, Some(move || Instant::now() > deadline));
        connection.authorizer(Some(|context: rusqlite::hooks::AuthContext<'_>| {
            use rusqlite::hooks::{AuthAction, Authorization};
            match context.action {
                AuthAction::Select | AuthAction::Read { .. } => Authorization::Allow,
                AuthAction::Function { function_name }
                    if !["load_extension", "readfile", "writefile"]
                        .iter()
                        .any(|name| function_name.eq_ignore_ascii_case(name)) =>
                {
                    Authorization::Allow
                }
                _ => Authorization::Deny,
            }
        }));
        // Refuse plans that materialize/sort large temporary relations in RAM.
        let mut plan = connection.prepare(&format!("EXPLAIN QUERY PLAN {sql}"))?;
        let details = plan
            .query_map([], |r| r.get::<_, String>(3))?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        ensure!(
            !details
                .iter()
                .any(|d| d.contains("TEMP B-TREE") || d.contains("MATERIALIZE")),
            "This query needs a temporary sort or materialized result. Use an indexed lookup or simpler query"
        );
        drop(plan);
        let mut statement = connection.prepare(sql)?;
        ensure!(
            statement.readonly() && statement.column_count() > 0,
            "Only read-only result-producing statements are allowed"
        );
        let columns: Vec<_> = statement
            .column_names()
            .iter()
            .map(|s| s.chars().take(128).collect::<String>())
            .collect();
        let count = columns.len();
        let mut cursor = statement.query([])?;
        let mut rows = vec![];
        let mut bytes = 0;
        let mut truncated = false;
        while let Some(row) = cursor.next()? {
            if rows.len() == 200 || bytes >= 256 * 1024 {
                truncated = true;
                break;
            }
            let values: Vec<Value> = (0..count)
                .map(|i| -> Result<Value> {
                    Ok(match row.get_ref(i)? {
                        ValueRef::Null => Value::Null,
                        ValueRef::Integer(n) => json!(n),
                        ValueRef::Real(n) => json!(n),
                        ValueRef::Text(text) => json!(
                            String::from_utf8_lossy(text)
                                .chars()
                                .take(1024)
                                .collect::<String>()
                        ),
                        ValueRef::Blob(blob) => json!(format!("[BLOB: {} bytes]", blob.len())),
                    })
                })
                .collect::<Result<_>>()?;
            let length = serde_json::to_vec(&values)?.len();
            if bytes + length > 256 * 1024 {
                truncated = true;
                break;
            }
            bytes += length;
            rows.push(values);
        }
        ensure!(
            remote
                .get(&node.id)?
                .is_some_and(|fresh| fresh.version == node.version),
            "Cloud database changed during the query; results discarded. Retry against a stable snapshot"
        );
        Ok(
            json!({"columns":columns,"rows":rows,"truncated":truncated,"version":node.version,"file_size":node.size}),
        )
    })();
    if let Some(error) = view.error.lock().take() {
        anyhow::bail!(error);
    }
    result
}

impl crate::runtime::Runtime {
    pub fn query_sqlite(&self, path: &str, sql: &str) -> Result<Value> {
        let node = self.resolve(path)?;
        let (remote, cache, worker) = {
            let e = self.engine.lock();
            ensure!(
                !node.dirty && !e.has_job(&node.id),
                "Wait for pending edits to upload before querying a cloud snapshot"
            );
            (e.remote.clone(), e.read_cache.clone(), e.attached()?)
        };
        let fresh = remote.get(&node.id)?.context("Cloud file is unavailable")?;
        ensure!(
            worker.in_scope(&fresh)?,
            "Database moved outside the mounted folder"
        );
        let siblings = remote.list(&fresh.parent)?;
        ensure!(
            !siblings.iter().any(|n| ["-wal", "-shm", "-journal"]
                .iter()
                .any(|suffix| n.name.to_lowercase()
                    == format!("{}{suffix}", fresh.name).to_lowercase())),
            "Cloud journal sidecars are present. Use a clean, standalone SQLite snapshot"
        );
        query(remote, cache, fresh, sql)
    }
    pub fn start_sqlite_query(self: &Arc<Self>, path: String, sql: String) -> Result<()> {
        ensure!(
            path.len() <= 4096 && sql.len() <= 16 * 1024,
            "Database path or query is too long"
        );
        {
            let mut state = self.query_state.lock();
            ensure!(
                state["state"] != "running",
                "A SQLite query is already running"
            );
            *state = json!({"state":"running"});
        }
        let runtime = self.clone();
        std::thread::spawn(move || {
            let result = runtime.query_sqlite(&path, &sql);
            *runtime.query_state.lock() = match result {
                Ok(result) => json!({"state":"complete","result":result}),
                Err(error) => json!({"state":"error","error":error.to_string()}),
            };
        });
        Ok(())
    }
}
