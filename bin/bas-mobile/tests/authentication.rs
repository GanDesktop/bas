use bas_client::service::{Client, pb};
use nsh_core::{
    element::Root,
    platform::{Environment, Idiom, Lifecycle, Os},
    prelude::*,
};
use nsh_engine::EngineConfig;
use nsh_testing::{Headless, MemoryStorage};
use std::{fs, path::PathBuf, time::Duration};

#[path = "support/daemon.rs"]
mod backend;
const DEADLINE: Duration = Duration::from_secs(30);

#[test]
fn android_interactive_login_from_conversation_and_agent_details() {
    let artifacts = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/mobile-e2e");
    fs::create_dir_all(&artifacts).expect("artifacts");
    let root = tempfile::Builder::new()
        .prefix("authentication-")
        .tempdir_in(&artifacts)
        .expect("isolated test")
        .keep()
        .canonicalize()
        .expect("absolute root");
    let agent = std::env::current_exe()
        .expect("test binary")
        .parent()
        .expect("deps")
        .parent()
        .expect("profile")
        .join("acp-test-agent");
    assert!(agent.is_file(), "build acp-test-agent before this test");
    let marker = root.join("demo-authenticated");
    let other_marker = root.join("other-authenticated");
    // The registry's persisted empty catalog keeps this test independent of its CDN.
    fs::create_dir_all(root.join("data/bas/acp")).expect("catalog cache");
    fs::write(
        root.join("data/bas/acp/registry.json"),
        serde_json::json!({
            "url":"https://cdn.agentclientprotocol.com/registry/v1/latest/registry.json",
            "registry":{"version":"1", "agents":[]}
        })
        .to_string(),
    )
    .expect("cached catalog");
    let config = serde_json::json!({"agents":[{"id":"demo","kind":"acp","command":{
        "program":agent,"args":["--protocol","v1","--require-auth","--interactive-auth","--auth-state",marker]
    }},{"id":"external-login","kind":"acp","command":{
        "program":agent,"args":["--protocol","v1","--require-auth","--auth-state",marker]
    }},{"id":"other","kind":"acp","command":{
        "program":agent,"args":["--protocol","v1","--require-auth","--interactive-auth","--auth-state",other_marker]
    }}],"acp":{"pool_size":0}});
    let (_daemon, endpoint) = backend::daemon_with(&root, &config.to_string());
    let runtime = tokio::runtime::Runtime::new().expect("runtime");
    runtime.block_on(async {
        let mut client = Client::connect(endpoint.clone()).await.expect("connect");
        client
            .create_workspace(pb::CreateWorkspaceRequest {
                cwd: root.to_string_lossy().into(),
                name: Some("Login project".into()),
            })
            .await
            .expect("workspace");
    });
    let mut app = Headless::start(EngineConfig {
        viewport: Size::new(px(390.0), px(844.0)),
        environment: Environment {
            window: Size::new(px(390.0), px(844.0)),
            os: Os::Android,
            idiom: Idiom::Phone,
            reduce_motion: true,
            ..Default::default()
        },
        storage: Some(MemoryStorage::new([(
            "bas.last-connection".into(),
            endpoint.clone(),
        )])),
        ..EngineConfig::new(Root::of::<bas_mobile::App>())
    })
    .expect("app");
    let mut trace = fs::File::create(root.join("ui-trace.log")).expect("trace");
    app.observe(move |step, batch| nsh_testing::print_batch(&mut trace, step, batch));
    app.wait_for_within("text:Connected", DEADLINE)
        .expect("backend connected");
    app.press("text:New conversation").expect("new session");
    app.press("label:demo").expect("requires authentication");
    app.wait_for_within("id:authentication-page", DEADLINE)
        .expect("automatic login page");
    app.wait_for_within("label:Demo sign in", DEADLINE)
        .expect("agent's methods");
    app.wait_for_gone("label:Check connection", DEADLINE)
        .expect("separate page");
    fs::write(root.join("methods.txt"), app.screen().texts().join("\n")).expect("methods artifact");
    app.press("label:Back").expect("return without logging in");
    app.wait_for("label:Sign in")
        .expect("visible conversation entry");
    app.press("label:Sign in").expect("reopen login");
    app.wait_for_within("label:Demo sign in", DEADLINE)
        .expect("methods reload");
    app.press("label:Demo sign in")
        .expect("start interactive login");
    app.wait_for_within("label:Demo account", DEADLINE)
        .expect("request-scoped form");
    app.press("label:Continue")
        .expect("validate missing required account");
    app.wait_for("label:Demo account")
        .expect("invalid form remains");
    assert!(!marker.exists(), "empty form cannot authenticate");
    app.press("label:Cancel sign in")
        .expect("cancel running flow");
    app.wait_for_gone("label:Demo account", DEADLINE)
        .expect("cancel removes form");
    app.press("label:Demo sign in")
        .expect("retry cancelled login");
    app.wait_for_within("label:Demo account", DEADLINE)
        .expect("new form");
    app.press("label:Decline")
        .expect("decline the agent request");
    app.wait_for_within("label:Demo sign in", DEADLINE)
        .expect("failed login is retryable");
    assert!(!marker.exists(), "decline cannot authenticate");
    app.press("label:Demo sign in")
        .expect("retry declined login");
    app.wait_for_within("label:Demo account", DEADLINE)
        .expect("form");
    app.type_text("label:Demo account", "mobile-demo")
        .expect("account");
    fs::write(root.join("form.txt"), app.screen().texts().join("\n")).expect("form artifact");
    app.press("label:Continue").expect("submit form");
    app.wait_for_within("label:example.com", DEADLINE)
        .expect("browser authorization");
    app.press("label:example.com")
        .expect("open authorization URL");
    assert!(
        app.screen()
            .opened_urls()
            .iter()
            .any(|url| url == "https://example.com/demo-sign-in")
    );
    fs::write(root.join("browser.txt"), app.screen().texts().join("\n")).expect("browser artifact");
    app.press("label:Continue").expect("confirm browser step");
    app.wait_for_gone("id:authentication-page", DEADLINE)
        .expect("login returns to conversation");
    app.wait_until("session ready", DEADLINE, |s| {
        s.enabled("label:Message your agent…").unwrap_or(false)
    })
    .expect("session retried");
    assert!(marker.is_file(), "real agent persisted authentication");
    app.type_text("label:Message your agent…", "Authenticated conversation")
        .expect("prompt");
    app.press("label:Send").expect("send after login");
    app.wait_until("echo", DEADLINE, |s| {
        s.texts()
            .iter()
            .filter(|t| **t == "Authenticated conversation")
            .count()
            == 2
    })
    .expect("agent echo");

    // An unauthenticated agent's failed creation must leave the selected,
    // authenticated conversation and the BAS connection usable.
    app.type_text("label:Message your agent…", "Keep the current agent draft")
        .expect("draft before choosing another agent");
    app.press("label:New conversation")
        .expect("create with another agent");
    app.press("label:other")
        .expect("choose unauthenticated agent");
    app.wait_for_within("id:authentication-page", DEADLINE)
        .expect("other agent login");
    app.wait_for_within("label:Demo sign in", DEADLINE)
        .expect("other login methods");
    app.press("label:Back")
        .expect("cancel creating the other conversation");
    app.wait_for_gone("id:authentication-page", DEADLINE)
        .expect("original conversation");
    app.wait_for_gone("label:Sign in", DEADLINE)
        .expect("other agent cannot demand login for demo");
    app.wait_for("text:Keep the current agent draft")
        .expect("original draft intact");
    app.press("label:Open sidebar").expect("backend status");
    app.wait_for_within("text:Connected", DEADLINE)
        .expect("BAS remains connected");
    app.press("text:New conversation")
        .expect("inspect per-agent status");
    app.wait_for("text:Signed in")
        .expect("authenticated agent status in the picker");
    app.wait_for("text:Sign in required")
        .expect("other agent status in the same picker");
    fs::write(
        root.join("agent-statuses.txt"),
        app.screen().texts().join("\n"),
    )
    .expect("per-agent status artifact");
    app.press("label:other")
        .expect("other agent still requires its own login");
    app.wait_for_within("id:authentication-page", DEADLINE)
        .expect("other login remains isolated");
    app.press("label:Back")
        .expect("return to authenticated agent");
    app.wait_for_gone("id:authentication-page", DEADLINE)
        .expect("conversation restored");
    app.press("label:Send")
        .expect("send preserved draft to authenticated agent");
    app.wait_until("original agent still usable", DEADLINE, |screen| {
        screen
            .texts()
            .iter()
            .filter(|text| **text == "Keep the current agent draft")
            .count()
            == 2
    })
    .expect("authenticated agent still replies");
    assert!(
        !other_marker.exists(),
        "other agent is still unauthenticated"
    );

    // Expiration during a live prompt must use the same login page, without replaying that prompt.
    fs::remove_file(&marker).expect("expire demo authorization");
    app.type_text("label:Message your agent…", "Expired sign-in prompt")
        .expect("expired prompt");
    app.press("label:Send").expect("live ACP AuthRequired");
    app.wait_for_within("id:authentication-page", DEADLINE)
        .expect("live error opens login");
    app.wait_for_within("label:Demo sign in", DEADLINE)
        .expect("login methods");
    app.press("label:Demo sign in").expect("reauthenticate");
    app.wait_for_within("label:Demo account", DEADLINE)
        .expect("reauth form");
    app.type_text("label:Demo account", "mobile-demo")
        .expect("account");
    app.press("label:Continue").expect("form accepted");
    app.wait_for_within("label:example.com", DEADLINE)
        .expect("browser step");
    app.press("label:Continue").expect("confirm authorization");
    app.wait_for_gone("id:authentication-page", DEADLINE)
        .expect("original session restored");
    app.wait_for("text:Authenticated conversation")
        .expect("history preserved");
    assert_eq!(
        app.screen()
            .texts()
            .iter()
            .filter(|text| **text == "Expired sign-in prompt")
            .count(),
        1,
        "authentication does not resend the failed prompt"
    );
    app.type_text("label:Message your agent…", "After renewed login")
        .expect("new user prompt");
    app.press("label:Send")
        .expect("send on the existing session");
    app.wait_until("reauthenticated echo", DEADLINE, |s| {
        s.texts()
            .iter()
            .filter(|text| **text == "After renewed login")
            .count()
            == 2
    })
    .expect("existing session usable after login");
    app.type_text(
        "label:Message your agent…",
        "Keep my draft through login settings",
    )
    .expect("unsent draft");
    app.press("label:App menu").expect("menu");
    app.press("label:Settings").expect("settings");
    app.press("label:Agents").expect("agents");
    app.wait_for_within("text:demo", DEADLINE)
        .expect("installed agent");
    app.press("text:demo").expect("agent detail");
    app.wait_for("text:Signed in")
        .expect("details share the conversation readiness");
    app.wait_for("label:Sign in")
        .expect("login is visible without checking connection");
    app.press("label:Sign in")
        .expect("dedicated login page from details");
    app.wait_for_within("text:Signed in", DEADLINE)
        .expect("already authenticated");
    app.wait_for_gone("label:Check connection", DEADLINE)
        .expect("no underlying settings actions");
    app.press("label:Done").expect("return to agent details");
    app.wait_for("label:Check connection")
        .expect("details restored");
    app.press("label:Back").expect("agents");
    app.press("label:Back").expect("settings");
    app.press("label:Back").expect("conversation");
    app.wait_for("text:Keep my draft through login settings")
        .expect("draft preserved");
    let sessions = runtime.block_on(async {
        Client::connect(endpoint.clone())
            .await
            .expect("client")
            .list_sessions(pb::ListSessionsRequest {
                include_archived: true,
            })
            .await
            .expect("sessions")
            .into_inner()
            .sessions
    });
    assert_eq!(
        sessions.len(),
        1,
        "reauthentication keeps the same conversation"
    );

    // A separate agent process can persist credentials before the original
    // authenticate request returns, as with browser-based sign-in.
    fs::remove_file(&marker).expect("expire authorization before external login");
    app.press("label:App menu").expect("menu");
    app.press("label:Settings").expect("settings");
    app.press("label:Agents").expect("agents");
    app.wait_for_within("text:demo", DEADLINE).expect("agent");
    app.press("text:demo").expect("details");
    app.press("label:Check connection")
        .expect("discover expired credentials");
    app.wait_for_within("text:Sign in required", DEADLINE)
        .expect("check updates shared readiness");
    app.press("label:Close").expect("close check summary");
    app.press("label:Sign in").expect("login page");
    app.wait_for_within("label:Demo sign in", DEADLINE)
        .expect("method");
    app.press("label:Demo sign in").expect("start login");
    app.wait_for_within("label:Demo account", DEADLINE)
        .expect("form");
    app.type_text("label:Demo account", "external-demo")
        .expect("account");
    app.press("label:Continue").expect("submit account");
    app.wait_for_within("label:example.com", DEADLINE)
        .expect("browser step");
    app.press("label:Check sign-in status")
        .expect("check while authentication is pending");
    app.wait_until("readiness check finished", DEADLINE, |screen| {
        screen.enabled("label:Check sign-in status") == Ok(true)
    })
    .expect("pending login checked without cancelling");
    assert!(!marker.exists(), "checking does not authenticate the agent");
    app.wait_for("label:Continue")
        .expect("browser authorization remains pending");
    app.press("label:example.com").expect("open browser");
    app.set_environment("browser foreground", |env| {
        env.lifecycle = Lifecycle::Background
    })
    .expect("background app");
    runtime.block_on(async {
        let mut client = Client::connect(endpoint)
            .await
            .expect("external login client");
        let catalog = client
            .list_agent_catalog(pb::ListAgentCatalogRequest { refresh: false })
            .await
            .expect("catalog")
            .into_inner();
        let catalog: bas_client::management::Catalog =
            serde_json::from_slice(&catalog.catalog_json).expect("catalog payload");
        let spec = catalog
            .entries
            .into_iter()
            .find(|entry| entry.id == "external-login")
            .expect("external login agent")
            .spec
            .expect("installed agent");
        let result = client
            .authenticate_agent(pb::AuthenticateAgentRequest {
                agent: "external-login".into(),
                expected_spec_json: serde_json::to_vec(&spec).expect("agent spec"),
                method_id: "demo-login".into(),
                expected_method_json: Vec::new(),
            })
            .await
            .expect("external process signs in")
            .into_inner();
        assert_eq!(
            result.check.expect("readiness check").status(),
            pb::AgentCheckStatus::Connected
        );
    });
    assert!(marker.is_file(), "external process persisted authorization");
    app.set_environment("return from browser", |env| {
        env.lifecycle = Lifecycle::Active
    })
    .expect("foreground app");
    app.wait_for_within("text:Signed in", DEADLINE)
        .expect("persisted login finishes the waiting authorization page");
    app.wait_for_gone("label:Continue", DEADLINE)
        .expect("stale browser form removed");
    app.wait_for_gone("label:Cancel sign in", DEADLINE)
        .expect("waiting flow stopped");
    fs::write(
        root.join("external-login.txt"),
        app.screen().texts().join("\n"),
    )
    .expect("external login artifact");
    app.press("label:Done")
        .expect("return from completed login");
    app.wait_for("text:Signed in")
        .expect("details reflect the completed login");
    app.wait_for_gone("text:Sign in required", DEADLINE)
        .expect("no stale login requirement in details");
    app.press("label:Back").expect("agents");
    app.press("label:Back").expect("settings");
    app.press("label:Back").expect("conversation");
    app.wait_for("text:Keep my draft through login settings")
        .expect("external login preserves the draft");
    fs::write(
        root.join("conversation.txt"),
        app.screen().texts().join("\n"),
    )
    .expect("conversation artifact");
    fs::write(root.join("result.txt"),"PASS: automatic ACP login page, return/reopen, validation, cancellation, decline/retry, form and browser authorization, creation retry, per-agent login isolation, independent BAS connection and creation state, live prompt expiry, same session/history, direct settings entry, external process login while authenticate is pending, foreground reconciliation and draft preservation\n").expect("result");
}
