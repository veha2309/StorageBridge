#![cfg_attr(windows, windows_subsystem = "windows")]
use anyhow::{Context, Result, ensure};
use parking_lot::Mutex;
use serde_json::{Value, json};
use std::{sync::Arc, time::Duration};
use storage_bridge::{
    auth::Auth,
    drive::{GoogleDrive, Remote},
    engine::Engine,
};

fn main() {
    if let Err(e) = run() {
        eprintln!("Storage Bridge: {e:#}");
        std::process::exit(1);
    }
}
#[cfg(not(any(windows, target_os = "macos")))]
fn run() -> Result<()> {
    anyhow::bail!("The daemon supports Windows and macOS; shared engine code is portable")
}
#[cfg(any(windows, target_os = "macos"))]
fn run() -> Result<()> {
    use storage_bridge::platform::{self, filesystem, ipc};
    let args: Vec<_> = std::env::args().collect();
    let test_instance = args
        .iter()
        .position(|a| a == "--test-instance")
        .map(|i| args.get(i + 1).context("Missing test instance name"))
        .transpose()?;
    if let Some(name) = test_instance {
        ensure!(
            args.iter().any(|a| a == "--demo"),
            "Test instances require --demo"
        );
        ipc::test_instance(name)?;
    }
    if let Some(i) = args.iter().position(|a| a == "--request") {
        let payload: Value =
            serde_json::from_str(args.get(i + 1).context("Supply a JSON request")?)?;
        let reply = ipc::request(&payload)?;
        ensure!(
            reply["ok"] == true,
            "{}",
            reply["error"].as_str().unwrap_or("Engine request failed")
        );
        println!("{reply}");
        return Ok(());
    }
    if args.iter().any(|a| a == "--diagnose") {
        println!(
            "{}",
            json!({"platform":std::env::consts::OS,"winfsp_installed":filesystem::available(),"pipe":ipc::pipe_name()?})
        );
        return Ok(());
    }
    let _instance = ipc::Instance::acquire()?;
    let dirs = directories::ProjectDirs::from("dev", "StorageBridge", "StorageBridge")
        .context("Cannot find local app data")?;
    let demo = args.iter().any(|a| a == "--demo");
    let root = dirs
        .data_local_dir()
        .join(if let Some(name) = test_instance {
            format!("demo-{name}")
        } else if demo {
            "demo".into()
        } else {
            "google".into()
        });
    std::fs::create_dir_all(&root)?;
    #[cfg(target_os = "macos")]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o700))?;
    }
    let auth = Arc::new(Auth::new(root.clone())?);
    let google = GoogleDrive::new(auth.clone())?;
    let progress = google.progress.clone();
    let remote: Arc<dyn Remote> = if demo {
        Arc::new(storage_bridge::local::LocalRemote::new(
            root.join("test-remote"),
        )?)
    } else {
        Arc::new(google)
    };
    let engine = Arc::new(Mutex::new(Engine::new(root, remote)?));
    if !demo {
        engine.lock().configure_scope("StorageBridge")?;
    }
    if demo {
        engine.lock().initialize()?;
    }
    if engine
        .lock()
        .store
        .setting("streaming_initialized")
        .is_none()
    {
        let mut e = engine.lock();
        e.set_online_only(true)?;
        e.store.set("streaming_initialized", "true")?;
    }
    let runtime = Arc::new(storage_bridge::runtime::Runtime::new(engine.clone()));
    let snapshot = Arc::new(Mutex::new(engine.lock().status()?));
    let shared = engine.clone();
    let background = runtime.clone();
    let account = auth.clone();
    let status = snapshot.clone();
    std::thread::spawn(move || {
        loop {
            if demo || account.connected() {
                if let Err(err) = background.tick() {
                    shared.lock().last_error = Some(err.to_string());
                }
                if let Ok(s) = shared.lock().status() {
                    *status.lock() = s;
                }
            }
            std::thread::sleep(Duration::from_secs(1));
        }
    });
    let metadata = runtime.clone();
    let account = auth.clone();
    let shared = engine.clone();
    std::thread::spawn(move || {
        loop {
            if (demo || account.connected())
                && let Err(error) = metadata.poll()
            {
                shared.lock().last_error = Some(error.to_string());
            }
            std::thread::sleep(Duration::from_secs(30));
        }
    });
    let direct = runtime.clone();
    let account = auth.clone();
    std::thread::spawn(move || {
        loop {
            if (demo || account.connected())
                && let Err(error) = direct.tick_imports()
            {
                direct.engine.lock().last_error = Some(error.to_string());
            }
            std::thread::sleep(Duration::from_secs(1));
        }
    });
    let mut host = None;
    if engine.lock().store.setting("startup").as_deref() == Some("true")
        && (demo || auth.connected())
    {
        let letter = engine
            .lock()
            .store
            .setting("drive_letter")
            .unwrap_or("G:".into());
        match filesystem::mount(runtime.clone(), &letter) {
            Ok(h) => host = Some(h),
            Err(e) => engine.lock().last_error = Some(e.to_string()),
        }
    }
    ipc::serve(move |request| {
        #[cfg(target_os = "macos")]
        if host.as_ref().is_some_and(|h| !h.is_mounted()) {
            host = None;
        }
        let mut action = || -> Result<Value> {
            ensure!(request["version"] == 1, "Unsupported IPC protocol version");
            let method = request["method"].as_str().context("Missing method")?;
            let p = &request["params"];
            match method {
                "status" => {
                    let mut s = if let Some(e) = engine.try_lock() {
                        let mut status = e.status()?;
                        status["busy"] = json!(false);
                        status
                    } else {
                        let mut status = snapshot.lock().clone();
                        status["busy"] = json!(true);
                        status
                    };
                    s["transfer"] = json!(*progress.lock());
                    s["sqlite_query"] = runtime.query_state.lock().clone();
                    s["scope_folder"] = json!(if demo {
                        "Local test drive"
                    } else {
                        "StorageBridge"
                    });
                    s["connected"] = json!(demo || auth.connected());
                    s["configured"] = json!(auth.configured());
                    s["login_state"] = json!(*auth.login_state.lock());
                    s["mounted"] = json!(host.is_some());
                    s["winfsp_installed"] = json!(filesystem::available());
                    s["filesystem_available"] = json!(filesystem::available());
                    s["platform"] = json!(std::env::consts::OS);
                    #[cfg(target_os = "macos")]
                    {
                        s["drive_letter"] = json!(filesystem::mount_path());
                    }
                    s["demo"] = json!(demo);
                    Ok(s)
                }
                "configure" => {
                    auth.configure(p["client_json"].as_str().context("Missing client JSON")?)?;
                    Ok(json!({}))
                }
                "login" => {
                    ensure!(!demo, "Restart without --demo to use Google Drive");
                    {
                        let mut state = auth.login_state.lock();
                        ensure!(
                            *state != "waiting_for_browser",
                            "Sign-in is already in progress"
                        );
                        *state = "waiting_for_browser".into();
                    }
                    let a = auth.clone();
                    std::thread::spawn(move || {
                        let result = a.login();
                        *a.login_state.lock() = match result {
                            Ok(()) => "connected".into(),
                            Err(e) => e.to_string(),
                        };
                    });
                    Ok(json!({}))
                }
                "mount" => {
                    ensure!(host.is_none(), "Drive is already mounted");
                    ensure!(demo || auth.connected(), "Sign in first");
                    #[cfg(windows)]
                    let letter = p["letter"].as_str().unwrap_or("G:").to_uppercase();
                    #[cfg(target_os = "macos")]
                    let letter = filesystem::mount_path();
                    if engine.lock().store.setting("root_id").is_none() {
                        runtime.initialize()?;
                    }
                    engine.lock().store.set("drive_letter", &letter)?;
                    host = Some(filesystem::mount(runtime.clone(), &letter)?);
                    Ok(json!({}))
                }
                "unmount" => {
                    ensure!(
                        engine.lock().open.is_empty(),
                        "Files are still open; close them before unmounting"
                    );
                    if let Some(h) = host.as_mut() {
                        #[cfg(windows)]
                        {
                            h.stop();
                            h.unmount();
                        }
                        #[cfg(target_os = "macos")]
                        h.unmount()?;
                    }
                    host = None;
                    Ok(json!({}))
                }
                "settings" => {
                    if let Some(value) = p.get("content_limit") {
                        let limit = value
                            .as_u64()
                            .context("Local content limit must be an integer number of bytes")?;
                        ensure!(
                            (64 * 1024 * 1024..=storage_bridge::storage::MAX_CONTENT_LIMIT)
                                .contains(&limit),
                            "Local content limit must be between 64 MiB and 16 TiB"
                        );
                        engine.lock().set_content_limit(limit)?;
                    }
                    if let Some(value) = p.get("online_only") {
                        engine.lock().set_online_only(
                            value
                                .as_bool()
                                .context("online_only must be true or false")?,
                        )?;
                    }
                    if let Some(budget) = p["cache_budget"].as_u64() {
                        ensure!(
                            (64 * 1024 * 1024..=1024u64 * 1024 * 1024 * 1024 * 16)
                                .contains(&budget),
                            "Cache budget must be between 64 MiB and 16 TiB"
                        );
                        engine
                            .lock()
                            .store
                            .set("cache_budget", &budget.to_string())?;
                    }
                    if let Some(enabled) = p["startup"].as_bool() {
                        ensure!(
                            !demo || !enabled,
                            "Startup reconnect is available in Google Drive mode"
                        );
                        platform::startup(enabled)?;
                        engine
                            .lock()
                            .store
                            .set("startup", if enabled { "true" } else { "false" })?;
                    }
                    Ok(json!({}))
                }
                "import_file" => {
                    ensure!(demo || auth.connected(), "Sign in first");
                    runtime.initialize()?;
                    let path = p["source_path"]
                        .as_str()
                        .context("Choose a local source file")?;
                    ensure!(path.len() <= 32768, "Source path is too long");
                    let folder = p["folder"].as_str().unwrap_or("");
                    ensure!(folder.len() <= 4096, "Destination path is too long");
                    let move_source = p["move_source"].as_bool().unwrap_or(false);
                    let id =
                        runtime.queue_import(std::path::Path::new(path), folder, move_source)?;
                    Ok(json!({"id":id}))
                }
                "dismiss_import" => {
                    runtime.dismiss_import(p["id"].as_str().context("Missing transfer ID")?)?;
                    Ok(json!({}))
                }
                "sqlite_query" => {
                    ensure!(demo || auth.connected(), "Sign in first");
                    runtime.initialize()?;
                    runtime.start_sqlite_query(
                        p["path"]
                            .as_str()
                            .context("Enter a cloud database path")?
                            .into(),
                        p["sql"].as_str().context("Enter a read-only query")?.into(),
                    )?;
                    Ok(json!({}))
                }
                "release_cache" => {
                    engine.lock().release_clean_content()?;
                    Ok(json!({}))
                }
                "retry" => {
                    engine.lock().retry()?;
                    Ok(json!({}))
                }
                "shutdown" => {
                    let e = engine.lock();
                    ensure!(
                        e.open.is_empty(),
                        "Close open files before stopping the engine"
                    );
                    let status = e.status()?;
                    ensure!(
                        status["pending"] == 0,
                        "Pending uploads remain; retry or wait before stopping"
                    );
                    drop(e);
                    if let Some(h) = host.as_mut() {
                        #[cfg(windows)]
                        {
                            h.stop();
                            h.unmount();
                        }
                        #[cfg(target_os = "macos")]
                        h.unmount()?;
                    }
                    host = None;
                    std::thread::spawn(|| {
                        std::thread::sleep(Duration::from_millis(500));
                        std::process::exit(0);
                    });
                    Ok(json!({}))
                }
                _ => anyhow::bail!("Unknown method: {method}"),
            }
        };
        match action() {
            Ok(value) => json!({"version":1,"ok":true,"result":value}),
            Err(e) => json!({"version":1,"ok":false,"error":e.to_string()}),
        }
    })
}
