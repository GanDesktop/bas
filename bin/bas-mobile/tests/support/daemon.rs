use std::{
    fs,
    io::{BufRead, BufReader, Write},
    path::PathBuf,
    process::{Child, Command, Stdio},
    sync::mpsc,
    time::Duration,
};
const DEADLINE: Duration = Duration::from_secs(30);
pub struct Daemon(Child);
impl Drop for Daemon {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

pub fn daemon_with(root: &std::path::Path, config: &str) -> (Daemon, String) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(root, fs::Permissions::from_mode(0o700))
            .expect("private daemon directory");
    }
    let binary = std::env::var_os("BAS_TEST_DAEMON")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            std::env::current_exe()
                .expect("test executable")
                .parent()
                .expect("deps directory")
                .parent()
                .expect("profile directory")
                .join(format!("bas{}", std::env::consts::EXE_SUFFIX))
        });
    fs::write(root.join("daemon.json"), config).expect("config");
    let mut child = Command::new(binary)
        .args([
            "--listen",
            "127.0.0.1:0",
            "--log-backend",
            "stdout",
            "--log-filter",
            "info",
            "--acp-maintenance",
            "false",
        ])
        .arg("--daemon-settings-file")
        .arg(root.join("daemon.json"))
        .arg("--localdb-path")
        .arg(root.join("sessions.sqlite3"))
        .env("XDG_RUNTIME_DIR", root)
        .env("XDG_CONFIG_HOME", root.join("config"))
        .env("XDG_DATA_HOME", root.join("data"))
        .stdout(Stdio::piped())
        .stderr(fs::File::create(root.join("stderr.log")).expect("stderr"))
        .spawn()
        .expect("build bas before running mobile E2E");
    let stdout = child.stdout.take().expect("stdout");
    let mut log = fs::File::create(root.join("daemon.log")).expect("log");
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            writeln!(log, "{line}").expect("write log");
            if line.contains("serving the bas.v1.Bas service") {
                let address = line
                    .split("address=")
                    .nth(1)
                    .expect("address")
                    .split_whitespace()
                    .next()
                    .expect("value");
                let _ = tx.send(format!("http://{}", address.trim_start_matches("tcp://")));
            }
        }
    });
    let daemon = Daemon(child);
    (daemon, rx.recv_timeout(DEADLINE).expect("daemon readiness"))
}
