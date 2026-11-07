use super::support::Case;

#[cfg(unix)]
#[tokio::test]
async fn ipc_and_tcp_reach_the_same_service() {
    let case = Case::new("ipc");
    let runtime = tempfile::tempdir().expect("private IPC directory");
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(runtime.path(), std::fs::Permissions::from_mode(0o700))
        .expect("private socket permissions");
    let socket = runtime
        .path()
        .canonicalize()
        .expect("physical socket directory")
        .join("service.sock");
    let local = format!("unix://{}", socket.display());
    let mut daemon = case.start("daemon", &["--listen", &local]).await;
    let session = daemon.create().await;
    let output = case
        .command(env!("CARGO_BIN_EXE_bast"))
        .args(["--server", &local, "session", "list"])
        .output()
        .await
        .expect("IPC CLI");
    case.record("ipc.txt", &output);
    assert!(output.status.success());
    assert!(String::from_utf8_lossy(&output.stdout).contains(&session.id));
    daemon.stop().await;
    assert!(!socket.exists());
}

#[cfg(windows)]
#[tokio::test]
async fn named_pipe_and_tcp_reach_the_same_service() {
    let case = Case::new("pipe");
    let local = format!("pipe://bas-test-{}", uuid::Uuid::new_v4());
    let mut daemon = case.start("daemon", &["--listen", &local]).await;
    let session = daemon.create().await;
    let output = case
        .command(env!("CARGO_BIN_EXE_bast"))
        .args(["--server", &local, "session", "list"])
        .output()
        .await
        .expect("pipe CLI");
    case.record("pipe.txt", &output);
    assert!(output.status.success());
    assert!(String::from_utf8_lossy(&output.stdout).contains(&session.id));
    daemon.stop().await;
}
