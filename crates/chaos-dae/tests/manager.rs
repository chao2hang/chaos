//! Integration tests for DaeManager using the fake-dae fixture.

use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use chaos_dae::DaeManager;

// Serialize tests that spawn processes / touch shared env assumptions.
static TEST_LOCK: Mutex<()> = Mutex::new(());

fn fixture_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/fake-dae.sh")
}

fn temp_work_dir() -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("chaos-dae-test-{nanos}"));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn chmod_755(path: &std::path::Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(path).unwrap().permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(path, perms).unwrap();
    }
}

#[tokio::test]
async fn write_config_and_reload_with_long_lived_fake() {
    let _guard = TEST_LOCK.lock().unwrap();
    let work_dir = temp_work_dir();
    let log_path = work_dir.join("fake-dae.log");

    // Embed absolute log path so concurrent tests cannot clobber via env.
    let wrapper = work_dir.join("fake-dae-sleep.sh");
    let script = format!(
        "#!/usr/bin/env bash\necho \"fake-dae $*\" >> \"{}\"\nsleep 3\nexit 0\n",
        log_path.display()
    );
    std::fs::write(&wrapper, script).unwrap();
    chmod_755(&wrapper);

    let mgr = DaeManager::new(wrapper, work_dir.clone());
    let config_path = mgr
        .write_config("global {\n  log_level: info\n}\n")
        .await
        .expect("write_config");
    assert_eq!(config_path, work_dir.join("config.dae"));

    mgr.reload().await.expect("reload/spawn");
    assert!(mgr.pid_path().is_file());
    assert!(mgr.is_running(), "long-lived fake should be alive");

    let mut log = String::new();
    for _ in 0..40 {
        if let Ok(s) = std::fs::read_to_string(&log_path) {
            if s.contains("run -c") {
                log = s;
                break;
            }
        }
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
    }

    assert!(
        log.contains("fake-dae run -c"),
        "expected fake-dae invocation in log, got: {log:?}"
    );
    assert!(
        log.contains("config.dae"),
        "expected -c to point at config.dae, got: {log:?}"
    );

    mgr.stop().await.expect("stop");
    // Give kill a moment
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    assert!(!mgr.is_running());

    let _ = std::fs::remove_dir_all(&work_dir);
}

#[tokio::test]
async fn immediate_exit_fake_reports_error() {
    let _guard = TEST_LOCK.lock().unwrap();
    let bin = fixture_bin();
    assert!(bin.is_file(), "missing fixture {}", bin.display());
    chmod_755(&bin);

    let work_dir = temp_work_dir();
    let log_path = work_dir.join("fake-dae.log");
    // Point fixture log via env for the shared fake-dae.sh
    std::env::set_var("FAKE_DAE_LOG", &log_path);

    let mgr = DaeManager::new(bin, work_dir.clone());
    mgr.write_config("global {}\n").await.unwrap();
    let err = mgr.reload().await.expect_err("immediate exit should err");
    let msg = err.to_string();
    assert!(
        msg.contains("exited immediately"),
        "unexpected error: {msg}"
    );

    std::env::remove_var("FAKE_DAE_LOG");
    let _ = std::fs::remove_dir_all(&work_dir);
}

#[tokio::test]
async fn stop_without_pid_is_ok() {
    let _guard = TEST_LOCK.lock().unwrap();
    let work_dir = temp_work_dir();
    let mgr = DaeManager::new(fixture_bin(), work_dir.clone());
    assert!(!mgr.is_running());
    mgr.stop().await.expect("stop noop");
    let _ = std::fs::remove_dir_all(&work_dir);
}
