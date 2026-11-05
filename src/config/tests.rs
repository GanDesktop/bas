use super::*;

#[test]
fn strict_json_and_domain_rules() {
    for text in [
        r#"{"unknown":1}"#,
        r#"{"daemon":{"unknown":1}}"#,
        r#"{"daemon":{"agents":[{"id":"x","kind":"embedded","unknown":1}]}}"#,
        r#"{"daemon":{"agents":[{"id":"x","kind":"acp","command":{"program":"dummy","unknown":1}}]}}"#,
    ] {
        assert!(serde_json::from_str::<Settings>(text).is_err(), "{text}");
    }
    assert_eq!(
        serde_json::from_str::<Settings>("{}").expect("defaults"),
        Settings::default()
    );
    for millis in ["-1", "0.5", "18446744073709551616"] {
        assert!(
            serde_json::from_str::<AgentSpec>(&format!(
                r#"{{"id":"x","kind":"embedded","cancellation_grace_ms":{millis}}}"#
            ))
            .is_err()
        );
    }
    assert!(validate_agents(&[]).is_err());
    assert!(validate_agents(&[AgentSpec::embedded(), AgentSpec::embedded()]).is_err());
    for text in [
        r#"{"id":" ","kind":"embedded"}"#,
        r#"{"id":"x","kind":"embedded","command":{"program":"dummy"}}"#,
        r#"{"id":"x","kind":"acp"}"#,
        r#"{"id":"x","kind":"acp","command":{"program":""}}"#,
        r#"{"id":"x","kind":"embedded","cancellation_grace_ms":0}"#,
    ] {
        assert!(validate_agents(&[serde_json::from_str(text).expect("structural JSON")]).is_err());
    }
    assert_eq!(
        " BUILTIN ".parse::<agent::Kind>().expect("alias"),
        agent::Kind::Embedded
    );
    assert!("x=embedded:dummy".parse::<AgentSpec>().is_err());
    assert!("x=acp".parse::<AgentSpec>().is_err());
    assert!("=embedded".parse::<AgentSpec>().is_err());
}

#[test]
fn daemon_validation_does_not_resolve_managed_installations() {
    let directory = tempfile::tempdir().expect("installation parent");
    let path = directory.path().join(INSTALLATION_FILE);
    let daemon = Daemon {
        agents: vec![AgentRegistration::Managed(ManagedAgent {
            id: "managed".into(),
            installation: InstallationRef::from_bytes(path.clone(), b"not read by validation")
                .expect("valid reference shape"),
        })],
        acp: AcpLifecycle {
            default_agent: Some("managed".into()),
            ..AcpLifecycle::default()
        },
    };
    daemon
        .validate()
        .expect("missing installation does not block settings validation");
    assert!(!path.exists());
    assert!(
        daemon.agents[0].resolve().is_err(),
        "resolution still requires the manifest"
    );
}
