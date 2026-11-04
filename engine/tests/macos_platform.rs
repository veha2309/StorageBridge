#![cfg(target_os = "macos")]
use serde_json::{Value, json};
use std::{
    path::Path,
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};

struct Daemon(Child);
impl Drop for Daemon {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
fn command(home: &Path, instance: &str) -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_storage-bridge"));
    cmd.env_remove("DYLD_LIBRARY_PATH")
        .env_remove("DYLD_FRAMEWORK_PATH")
        .env("HOME", home)
        .args(["--demo", "--test-instance", instance]);
    cmd
}
fn request(home: &Path, instance: &str, value: Value) -> Option<Value> {
    let output = command(home, instance)
        .args(["--request", &value.to_string()])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    serde_json::from_slice(&output.stdout).ok()
}
#[test]
fn daemon_ipc_isolated_instance_and_restart() {
    let home = tempfile::tempdir().unwrap();
    let instance = uuid::Uuid::new_v4().simple().to_string();
    let launch = || {
        Daemon(
            command(home.path(), &instance)
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .unwrap(),
        )
    };
    let status = json!({"version":1,"method":"status"});
    let wait_ready = || {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            if let Some(reply) = request(home.path(), &instance, status.clone()) {
                return reply;
            }
            assert!(Instant::now() < deadline, "Daemon did not become ready");
            std::thread::sleep(Duration::from_millis(50));
        }
    };
    let mut daemon = launch();
    let reply = wait_ready();
    assert_eq!(reply["result"]["platform"], "macos");
    assert_eq!(reply["result"]["demo"], true);
    assert_eq!(reply["result"]["mounted"], false);
    let duplicate = command(home.path(), &instance).output().unwrap();
    assert!(
        !duplicate.status.success(),
        "Two daemons acquired the same cache"
    );
    assert!(
        request(
            home.path(),
            &instance,
            json!({"version":2,"method":"status"})
        )
        .is_none()
    );
    assert!(
        request(
            home.path(),
            &instance,
            json!({"version":1,"method":"settings","params":{"online_only":false}})
        )
        .is_some()
    );
    let shutdown = || {
        assert!(
            request(
                home.path(),
                &instance,
                json!({"version":1,"method":"shutdown"})
            )
            .is_some()
        );
    };
    let wait_exit = |child: &mut Child| {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if let Some(status) = child.try_wait().unwrap() {
                assert!(status.success());
                break;
            }
            assert!(Instant::now() < deadline, "Daemon did not stop");
            std::thread::sleep(Duration::from_millis(50));
        }
    };
    shutdown();
    wait_exit(&mut daemon.0);
    let mut restarted = launch();
    assert_eq!(wait_ready()["result"]["online_only"], false);
    shutdown();
    wait_exit(&mut restarted.0);
}
