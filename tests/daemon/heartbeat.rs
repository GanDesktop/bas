use super::*;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use tokio::{
    io::{AsyncRead, AsyncWrite, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    sync::mpsc,
    task::JoinSet,
};
use tokio_util::task::AbortOnDropHandle;

async fn relay(
    mut reader: impl AsyncRead + Unpin,
    mut writer: impl AsyncWrite + Unpin,
    blocked: Arc<AtomicBool>,
    closed: mpsc::UnboundedSender<&'static str>,
    peer: &'static str,
) {
    let mut buffer = [0; 8192];
    loop {
        let count = reader.read(&mut buffer).await.expect("proxy read");
        if count == 0 {
            closed.send(peer).expect("observe peer closure");
            // Do not relay FIN: each peer must discover the black hole itself.
            std::future::pending::<()>().await;
            return;
        }
        if !blocked.load(Ordering::SeqCst) {
            writer
                .write_all(&buffer[..count])
                .await
                .expect("proxy write");
        }
    }
}

#[tokio::test]
async fn heartbeat_detects_a_silent_partition_and_the_same_channel_recovers() {
    let mut process = command();
    process.args(["--listen", "127.0.0.1:0", "--log-backend", "stdout"]);
    let mut daemon = Daemon::start(process).await;
    let upstream = daemon.address.to_string();
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("proxy");
    let target: Target = listener
        .local_addr()
        .expect("proxy address")
        .to_string()
        .parse()
        .expect("target");
    let blocked = Arc::new(AtomicBool::new(false));
    let partition = blocked.clone();
    let (closed, mut closures) = mpsc::unbounded_channel();
    let proxy = AbortOnDropHandle::new(tokio::spawn(async move {
        let mut connections = JoinSet::new();
        loop {
            let (downstream, _) = listener.accept().await.expect("proxy accept");
            let upstream = TcpStream::connect(&upstream).await.expect("upstream");
            let (client_read, client_write) = downstream.into_split();
            let (server_read, server_write) = upstream.into_split();
            connections.spawn(relay(
                client_read,
                server_write,
                partition.clone(),
                closed.clone(),
                "client",
            ));
            connections.spawn(relay(
                server_read,
                client_write,
                partition.clone(),
                closed.clone(),
                "server",
            ));
        }
    }));
    let mut client = Client::connect_target(&target).await.expect("client");
    client
        .list_agents(pb::ListAgentsRequest {})
        .await
        .expect("initial RPC");

    // The idle channel survives a complete heartbeat cycle without an active RPC.
    let cycle = bas::service::heartbeat::INTERVAL + bas::service::heartbeat::TIMEOUT;
    tokio::time::sleep(cycle + Duration::from_secs(1)).await;
    assert!(
        closures.try_recv().is_err(),
        "healthy idle connection stays open"
    );
    let session = client
        .create_session(pb::CreateSessionRequest::default())
        .await
        .expect("create")
        .into_inner()
        .session
        .expect("session");
    let mut stream = client
        .watch_permissions(pb::WatchPermissionsRequest {
            session: session.id.clone(),
        })
        .await
        .expect("watch")
        .into_inner();
    assert!(stream.message().await.expect("snapshot").is_some());

    // Drop bytes in both directions while leaving both TCP sockets open.
    blocked.store(true, Ordering::SeqCst);
    tokio::time::timeout(cycle + Duration::from_secs(5), async {
        assert!(
            stream.message().await.is_err(),
            "stalled stream reports transport failure"
        );
        let mut peers = vec![
            closures.recv().await.expect("first peer"),
            closures.recv().await.expect("second peer"),
        ];
        peers.sort();
        assert_eq!(peers, ["client", "server"]);
    })
    .await
    .expect("both peers detect the partition within the heartbeat bound");

    blocked.store(false, Ordering::SeqCst);
    let recovered = tokio::time::timeout(
        TIMEOUT,
        client.get_session(pb::GetSessionRequest {
            session: session.id.clone(),
        }),
    )
    .await
    .expect("reconnect deadline")
    .expect("same channel reconnects")
    .into_inner()
    .session
    .expect("session retained");
    assert_eq!(
        recovered.id, session.id,
        "transport loss does not delete the session"
    );
    drop(proxy);
    daemon.stop().await;
}
