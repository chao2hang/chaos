//! Integration tests for DaeManager using the fake-dae fixture.

use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use chaos_dae::DaeManager;

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

#[tokio::test]
async fn write_config_and_reload_invokes_fake_dae() {
    let bin = fixture_bin();
    assert!(bin.is_file(), "missing fixture {}", bin.display());

    // Ensure executable for spawn.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(&bin).unwrap().permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&bin, perms).unwrap();
    }

    let work_dir = temp_work_dir();
    let log_path = work_dir.join("fake-dae.log");
    let _ = std::fs::remove_file(&log_path);

    let mgr = DaeManager::new(bin, work_dir.clone());

    let config_path = mgr
        .write_config("global {\n  log_level: info\n}\n")
        .await
        .expect("write_config");
    assert_eq!(config_path, work_dir.join("config.dae"));
    let written = tokio::fs::read_to_string(&config_path).await.unwrap();
    assert!(written.contains("log_level"));

    // Point fake-dae log at work_dir so tests don't clobber /tmp/fake-dae.log.
    std::env::set_var("FAKE_DAE_LOG", &log_path);

    mgr.reload().await.expect("reload/spawn");

    // Pid file is written even if the fixture exits immediately.
    assert!(
        mgr.pid_path().is_file(),
        "expected pid file at {}",
        mgr.pid_path().display()
    );

    // Wait briefly for the short-lived process to append the log.
    let mut log = String::new();
    for _ in 0..20 {
        if let Ok(s) = std::fs::read_to_string(&log_path) {
            if !s.is_empty() {
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
        log.contains(work_dir.to_string_lossy().as_ref())
            || log.contains("run -c"),
        "log should mention work dir or run args: {log:?}"
    );

    // stop is best-effort (fixture may already have exited).
    mgr.stop().await.expect("stop");
    assert!(!mgr.is_running());

    let _ = std::fs::remove_dir_all(&work_dir);
    std::env::remove_var("FAKE_DAE_LOG");
}

#[tokio::test]
async fn stop_without_pid_is_ok() {
    let work_dir = temp_work_dir();
    let mgr = DaeManager::new(fixture_bin(), work_dir.clone());
    assert!(!mgr.is_running());
    mgr.stop().await.expect("stop noop");
    let _ = std::fs::remove_dir_all(&work_dir);
}
