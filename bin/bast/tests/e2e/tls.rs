use super::support::{Case, DEADLINE};
use bas_client::service::{
    Client, Target, pb,
    tls::{ClientTls, ConnectionOptions},
};
use rcgen::{BasicConstraints, CertificateParams, ExtendedKeyUsagePurpose, IsCa, KeyPair};
use std::path::{Path, PathBuf};

struct Certificates {
    ca: PathBuf,
    server: PathBuf,
    server_key: PathBuf,
    client: PathBuf,
    client_key: PathBuf,
}

fn certificates(directory: &Path) -> Certificates {
    std::fs::create_dir_all(directory).expect("certificate directory");
    let ca_key = KeyPair::generate().expect("CA key");
    let mut params = CertificateParams::default();
    params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
    let ca = params.self_signed(&ca_key).expect("CA certificate");
    let issue = |name: &str, usage: ExtendedKeyUsagePurpose| {
        let key = KeyPair::generate().expect("leaf key");
        let mut params =
            CertificateParams::new(vec!["localhost".into(), "127.0.0.1".into()]).expect("SANs");
        params.extended_key_usages = vec![usage];
        let certificate = params
            .signed_by(&key, &ca, &ca_key)
            .expect("signed certificate");
        let cert_path = directory.join(format!("{name}.pem"));
        let key_path = directory.join(format!("{name}.key"));
        std::fs::write(&cert_path, certificate.pem()).expect("certificate file");
        std::fs::write(&key_path, key.serialize_pem()).expect("temporary private key");
        (cert_path, key_path)
    };
    let (server, server_key) = issue("server", ExtendedKeyUsagePurpose::ServerAuth);
    let (client, client_key) = issue("client", ExtendedKeyUsagePurpose::ClientAuth);
    let ca_path = directory.join("ca.pem");
    std::fs::write(&ca_path, ca.pem()).expect("CA file");
    Certificates {
        ca: ca_path,
        server,
        server_key,
        client,
        client_key,
    }
}

impl Certificates {
    fn options(&self, target: Target) -> ConnectionOptions {
        ConnectionOptions {
            target,
            tls: Some(ClientTls {
                ca: self.ca.clone(),
                certificate: self.client.clone(),
                private_key: self.client_key.clone(),
                server_name: None,
            }),
        }
    }
}

#[tokio::test]
async fn mtls_authenticates_clients_before_exposing_the_shared_session_service() {
    let case = Case::new("tls");
    let trusted = certificates(&case.root.join("trusted"));
    let foreign = certificates(&case.root.join("foreign"));
    let mut daemon = case
        .start(
            "daemon",
            &[
                "--listen",
                "mtls://127.0.0.1:0",
                "--tls-certificate",
                trusted.server.to_str().expect("server certificate"),
                "--tls-key",
                trusted.server_key.to_str().expect("server key"),
                "--tls-client-ca",
                trusted.ca.to_str().expect("CA"),
            ],
        )
        .await;
    let session = daemon.create().await;
    let options = trusted.options(daemon.addresses[1].clone());
    let mut client = Client::connect_options(&options)
        .await
        .expect("authenticated connection");
    let found = client
        .get_session(pb::GetSessionRequest {
            session: session.id.clone(),
        })
        .await
        .expect("shared session")
        .into_inner()
        .session
        .expect("session");
    case.record("authenticated.txt", &found);
    assert_eq!(found.id, session.id);
    let mut untrusted = options.clone();
    untrusted.tls.as_mut().expect("TLS").certificate = foreign.client;
    untrusted.tls.as_mut().expect("TLS").private_key = foreign.client_key;
    let failure = tokio::time::timeout(DEADLINE, async {
        Client::connect_options(&untrusted)
            .await?
            .list_agents(pb::ListAgentsRequest {})
            .await?;
        anyhow::Ok(())
    })
    .await
    .expect("rejection deadline")
    .expect_err("foreign client is not authorized");
    case.record("rejected.txt", failure.to_string());
    daemon.stop().await;
}
