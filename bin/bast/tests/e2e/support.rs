use bas_client::service::{Client, Target, pb};
use futures::{StreamExt, TryStreamExt, future::ready};
use std::{fs::File, io::Write, path::PathBuf, process::Stdio, time::Duration};
use tokio::{
    io::{AsyncBufReadExt, BufReader},
    process::{Child, Command},
};
use tokio_util::task::AbortOnDropHandle;

pub const DEADLINE: Duration = Duration::from_secs(20);

pub struct Case {
    pub root: PathBuf,
}

impl Case {
    pub fn new(name: &str) -> Self {
        let base = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/e2e");
        std::fs::create_dir_all(&base).expect("artifact directory");
        let base = base.canonicalize().expect("absolute artifact directory");
        let root = tempfile::Builder::new()
            .prefix(&format!("{name}-"))
            .tempdir_in(base)
            .expect("isolated scenario")
            .keep();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o700))
                .expect("private case directory");
        }
        std::fs::create_dir(root.join("workspace")).expect("workspace");
        std::fs::write(
            root.join("daemon.json"),
            r#"{"agents":[{"id":"embedded","kind":"embedded"}]}"#,
        )
        .expect("isolated configuration");
        std::fs::write(
            root.join("build.txt"),
            concat!(env!("CARGO_PKG_VERSION"), " ", env!("BAS_BUILD_TAG")),
        )
        .expect("build identity");
        eprintln!("artifacts: {}", root.display());
        Self { root }
    }

    pub fn record(&self, name: &str, value: impl std::fmt::Debug) {
        std::fs::write(self.root.join(name), format!("{value:#?}\n")).expect("write artifact");
    }

    pub fn command(&self, program: impl AsRef<std::ffi::OsStr>) -> Command {
        let mut command = Command::new(program);
        command
            .env_clear()
            .envs(std::env::vars_os().filter(|(key, _)| {
                ["PATH", "SystemRoot", "SYSTEMROOT", "WINDIR", "TEMP", "TMP"]
                    .contains(&key.to_string_lossy().as_ref())
            }))
            .env("XDG_CONFIG_HOME", self.root.join("config"))
            .env("XDG_DATA_HOME", self.root.join("data"))
            .env("XDG_STATE_HOME", self.root.join("state"))
            .env("XDG_CACHE_HOME", self.root.join("cache"))
            .env("XDG_RUNTIME_DIR", &self.root)
            .env("APPDATA", self.root.join("config"))
            .env("LOCALAPPDATA", self.root.join("data"))
            .current_dir(self.root.join("workspace"))
            .kill_on_drop(true);
        command
    }

    pub async fn start(&self, label: &str, extra: &[&str]) -> Daemon {
        let mut command = self.command(
            PathBuf::from(env!("CARGO_BIN_EXE_bast"))
                .with_file_name(format!("bas{}", std::env::consts::EXE_SUFFIX)),
        );
        command
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
            .arg(self.root.join("daemon.json"))
            .arg("--localdb-path")
            .arg(self.root.join("sessions.sqlite3"))
            .args(extra)
            .stdout(Stdio::piped())
            .stderr(
                File::create(self.root.join(format!("{label}.stderr"))).expect("stderr artifact"),
            );
        self.record(&format!("{label}.command"), &command);
        let mut child = command
            .spawn()
            .expect("start bas; run cargo build --workspace --bins before the E2E suite");
        let lines = BufReader::new(child.stdout.take().expect("stdout")).lines();
        let mut log =
            File::create(self.root.join(format!("{label}.stdout"))).expect("stdout artifact");
        let mut lines = Box::pin(
            futures::stream::try_unfold(lines, |mut lines| async move {
                lines
                    .next_line()
                    .await
                    .map(|line| line.map(|line| (line, lines)))
            })
            .map_ok(move |line| {
                writeln!(log, "{line}").expect("save daemon log");
                line
            })
            .try_skip_while(|line| ready(Ok(!line.contains("serving the bas.v1.Bas service")))),
        );
        let count = 1 + extra.iter().filter(|arg| **arg == "--listen").count();
        let addresses: Vec<Target> = tokio::time::timeout(
            DEADLINE,
            lines
                .by_ref()
                .try_filter(|line| ready(line.contains("serving the bas.v1.Bas service")))
                .take(count)
                .map_ok(|line| {
                    line.split("address=")
                        .nth(1)
                        .expect("bound address")
                        .split_whitespace()
                        .next()
                        .expect("target")
                        .parse()
                        .expect("valid target")
                })
                .try_collect(),
        )
        .await
        .expect("daemon readiness deadline")
        .expect("read log");
        assert_eq!(addresses.len(), count, "all listeners ready");
        let address = addresses[0].clone();
        let logs = AbortOnDropHandle::new(tokio::spawn(lines.try_for_each(|_| ready(Ok(())))));
        let client = Client::connect_target(&address)
            .await
            .expect("connect to ready daemon");
        Daemon {
            child,
            address,
            addresses,
            client,
            logs,
        }
    }

    pub async fn cli(&self, daemon: &Daemon, label: &str, args: &[&str]) -> std::process::Output {
        let mut command = self.command(env!("CARGO_BIN_EXE_bast"));
        command
            .arg("--server")
            .arg(daemon.address.to_string())
            .args(args);
        self.record(&format!("{label}.command"), &command);
        let output = tokio::time::timeout(DEADLINE, command.output())
            .await
            .expect("CLI deadline")
            .expect("run bast");
        std::fs::write(self.root.join(format!("{label}.stdout")), &output.stdout)
            .expect("CLI stdout");
        std::fs::write(self.root.join(format!("{label}.stderr")), &output.stderr)
            .expect("CLI stderr");
        self.record(&format!("{label}.status"), output.status);
        output
    }

    pub async fn ok(&self, daemon: &Daemon, label: &str, args: &[&str]) -> String {
        let output = self.cli(daemon, label, args).await;
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8(output.stdout).expect("UTF-8 CLI output")
    }
}

pub struct Daemon {
    pub child: Child,
    pub address: Target,
    pub addresses: Vec<Target>,
    pub client: Client,
    logs: AbortOnDropHandle<std::io::Result<()>>,
}

impl Daemon {
    #[cfg(unix)]
    pub async fn stop(mut self) {
        use nix::{
            sys::signal::{Signal, kill},
            unistd::Pid,
        };
        kill(
            Pid::from_raw(self.child.id().expect("running daemon") as i32),
            Signal::SIGTERM,
        )
        .expect("SIGTERM");
        let status = tokio::time::timeout(DEADLINE, self.child.wait())
            .await
            .expect("shutdown deadline")
            .expect("reap daemon");
        assert!(status.success(), "{status}");
        self.logs.await.expect("log task").expect("drain logs");
    }

    #[cfg(windows)]
    pub async fn stop(mut self) {
        self.child.kill().await.expect("stop daemon");
        self.logs.await.expect("log task").expect("drain logs");
    }

    pub async fn create(&mut self) -> pb::Session {
        self.client
            .create_session(pb::CreateSessionRequest::default())
            .await
            .expect("create session")
            .into_inner()
            .session
            .expect("session")
    }
}

pub fn session_id(output: &str) -> &str {
    output
        .lines()
        .find_map(|line| line.strip_prefix("id        "))
        .expect("CLI session ID")
}

pub async fn completion(client: &mut Client, session: &str, cursor: u64) -> pb::AcpObservation {
    let stream = client
        .observe_acp(pb::AttachRequest {
            session: session.into(),
            cursor: Some(cursor),
            replay: 0,
        })
        .await
        .expect("observe accepted turn")
        .into_inner();
    let mut completed = Box::pin(stream.try_filter(|event| {
        ready(
            serde_json::from_slice::<serde_json::Value>(&event.json)
                .expect("ACP JSON")
                .get("result")
                .is_some(),
        )
    }));
    tokio::time::timeout(DEADLINE, completed.try_next())
        .await
        .expect("turn completion deadline")
        .expect("ACP stream")
        .expect("prompt result")
}
