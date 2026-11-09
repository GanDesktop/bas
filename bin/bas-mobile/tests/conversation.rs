use bas_client::service::{Client, pb};
use nsh_core::{
    element::Root,
    platform::{Environment, Idiom, Os},
    prelude::*,
};
use nsh_engine::EngineConfig;
use nsh_testing::{Headless, MemoryStorage};
use std::{fs, path::PathBuf, time::Duration};

const DEADLINE: Duration = Duration::from_secs(30);
#[path = "support/daemon.rs"]
mod backend;
use backend::{Daemon, daemon_with};

fn daemon(root: &std::path::Path) -> (Daemon, String) {
    daemon_with(root, r#"{"agents":[{"id":"embedded","kind":"embedded"}]}"#)
}

#[test]
fn android_workspace_conversation_and_backend_flow() {
    let artifacts = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/mobile-e2e");
    fs::create_dir_all(&artifacts).expect("artifacts");
    let artifacts = artifacts
        .canonicalize()
        .expect("absolute artifact directory");
    let root = tempfile::Builder::new()
        .prefix("conversation-")
        .tempdir_in(artifacts)
        .expect("isolated test")
        .keep();
    let (_daemon, endpoint) = daemon(&root);
    let remote_root = root.join("remote-backend");
    fs::create_dir(&remote_root).expect("remote backend directory");
    let (_remote_daemon, remote_endpoint) = daemon(&remote_root);
    let runtime = tokio::runtime::Runtime::new().expect("runtime");
    fs::create_dir(root.join("second")).expect("second workspace directory");
    runtime.block_on(async {
        let mut client = Client::connect(endpoint.clone()).await.expect("connect");
        client
            .create_workspace(pb::CreateWorkspaceRequest {
                cwd: root.to_string_lossy().into(),
                name: Some("Mobile project".into()),
            })
            .await
            .expect("workspace");
        client
            .create_workspace(pb::CreateWorkspaceRequest {
                cwd: root.join("second").to_string_lossy().into(),
                name: Some("Second project".into()),
            })
            .await
            .expect("second workspace");
    });
    runtime.block_on(async {
        let mut client = Client::connect(remote_endpoint.clone())
            .await
            .expect("remote connection");
        client
            .create_workspace(pb::CreateWorkspaceRequest {
                cwd: remote_root.to_string_lossy().into(),
                name: Some("Remote project".into()),
            })
            .await
            .expect("remote workspace");
    });
    let storage = MemoryStorage::new([(
        "bas.connections".into(),
        serde_json::to_string(&[&endpoint, &remote_endpoint]).expect("connections"),
    )]);
    let config = EngineConfig {
        viewport: Size::new(px(390.0), px(844.0)),
        environment: Environment {
            window: Size::new(px(390.0), px(844.0)),
            os: Os::Android,
            idiom: Idiom::Phone,
            reduce_motion: true,
            ..Default::default()
        },
        storage: Some(storage.clone()),
        ..EngineConfig::new(Root::of::<bas_mobile::App>())
    };
    let mut app = Headless::start(config).expect("app");
    let mut trace = fs::File::create(root.join("ui-trace.log")).expect("UI trace");
    app.observe(move |step, batch| nsh_testing::print_batch(&mut trace, step, batch));
    app.wait_for("label:New connection")
        .expect("connection-first welcome");
    app.wait_for("label:App menu")
        .expect("global menu remains available");
    app.wait_for_gone("label:Open sidebar", DEADLINE)
        .expect("no sidebar before connection");
    app.wait_for_gone("label:Message your agent…", DEADLINE)
        .expect("no composer before connection");
    app.wait_for_gone("label:Connected backend", DEADLINE)
        .expect("no hidden drawer before connection");
    app.press("label:New connection")
        .expect("open connection settings");
    app.clear("label:Backend URL").expect("clear address");
    app.type_text("label:Backend URL", &endpoint)
        .expect("address");
    app.press("label:Test connection")
        .expect("test saved backend");
    app.wait_for_within("text:Connection successful", DEADLINE)
        .expect("read-only probe");
    app.clear("label:Backend URL").expect("edit tested address");
    app.type_text("label:Backend URL", &remote_endpoint)
        .expect("another connection draft");
    app.wait_for_gone("text:Connection successful", DEADLINE)
        .expect("test result belongs to the tested address");
    app.clear("label:Backend URL").expect("restore address");
    app.type_text("label:Backend URL", &endpoint)
        .expect("original connection");
    app.press("label:Connect").expect("connect");
    app.wait_for_within("text:Connected", DEADLINE)
        .expect("connected");
    app.press("label:Back").expect("settings overview");
    app.press("label:Back").expect("conversation");
    app.press("label:Open sidebar").expect("sidebar");
    app.press("label:Workspace").expect("workspace picker");
    app.press("role:option[Mobile project]")
        .expect("select mobile project");
    app.wait_for("text:Mobile project")
        .expect("workspace selector");
    app.press("text:New conversation")
        .expect("new conversation");
    app.press("label:embedded").expect("choose agent");
    app.wait_for_gone("text:New conversation", DEADLINE)
        .expect("drawer closed");
    app.press("label:New conversation")
        .expect("header new conversation shortcut");
    app.wait_for("label:embedded")
        .expect("shared agent chooser");
    app.back()
        .expect("dismiss chooser without creating a session");
    app.type_text("label:Message your agent…", "Mobile E2E message")
        .expect("draft");
    app.press("label:Send").expect("send");
    app.wait_until("agent reply", DEADLINE, |screen| {
        screen.texts().join("\n").contains("echo agent:")
    })
    .expect("real daemon reply");
    fs::write(
        root.join("conversation.txt"),
        app.screen().texts().join("\n"),
    )
    .expect("transcript artifact");
    app.press("label:Open sidebar").expect("sidebar");
    app.wait_for("label:Connected backend")
        .expect("backend footer");
    app.press("label:Workspace").expect("workspace picker");
    app.press("role:option[Second project]")
        .expect("second workspace");
    app.wait_for("text:No conversations in this workspace yet")
        .expect("workspace filter");
    app.press("label:Workspace").expect("workspace picker");
    app.press("role:option[Mobile project]")
        .expect("first workspace");
    app.press("text:Untitled conversation")
        .expect("existing session");
    app.wait_for_within("text:/echo agent:/", DEADLINE)
        .expect("history after selecting recycled row");
    app.type_text("label:Message your agent…", "Second mobile message")
        .expect("second draft");
    app.press("label:Send").expect("second send");
    app.wait_until("second reply", DEADLINE, |screen| {
        screen
            .texts()
            .iter()
            .filter(|text| **text == "Second mobile message")
            .count()
            == 2
    })
    .expect("live updates survive drawer closure");
    app.press("label:Open sidebar").expect("sidebar");
    app.on_dialog(|_| nsh_testing::DialogAnswer::Press("Archive conversation".into()));
    app.long_press("label:Untitled conversation")
        .expect("session actions");
    app.press("label:Archive conversation")
        .expect("archive on backend");
    app.wait_for_gone("label:Untitled conversation", DEADLINE)
        .expect("archived session leaves recent list");
    app.press("label:Filter conversations")
        .expect("filter menu");
    app.press("label:Archived conversations")
        .expect("archive filter");
    app.wait_for_within("label:Untitled conversation", DEADLINE)
        .expect("archived session remains available");
    app.on_dialog(|_| nsh_testing::DialogAnswer::Press("Restore conversation".into()));
    app.long_press("label:Untitled conversation")
        .expect("archived session actions");
    app.press("label:Restore conversation")
        .expect("restore on backend");
    app.wait_for_gone("label:Untitled conversation", DEADLINE)
        .expect("restored session leaves archive");
    app.press("label:Filter conversations")
        .expect("filter menu");
    app.press("label:By time").expect("recent list");
    app.wait_for_within("label:Untitled conversation", DEADLINE)
        .expect("restored session visible");
    app.press("label:Connected backend")
        .expect("backend picker");
    app.press(&format!("role:option[{remote_endpoint}]"))
        .expect("remote backend");
    app.wait_for_within("text:Remote project", DEADLINE)
        .expect("remote workspace loaded");
    app.wait_for("text:No conversations in this workspace yet")
        .expect("old sessions cleared");
    app.press("label:Connected backend")
        .expect("backend picker");
    app.press(&format!("role:option[{endpoint}]"))
        .expect("original backend");
    app.wait_for_within("text:Connected", DEADLINE)
        .expect("reconnected");
    app.back().expect("Android back");
    app.wait_for_gone("label:Connected backend", DEADLINE)
        .expect("back closes drawer");
    assert_eq!(
        storage.entries().get("bas.last-connection"),
        Some(&endpoint)
    );
    fs::write(root.join("result.txt"), "PASS: connect, switch workspaces, create/select session, send/receive after drawer closure, switch two real backends, Android back, persisted backend\n").expect("result artifact");
    eprintln!("artifacts: {}", root.display());
}

#[test]
fn android_acp_controls_permissions_forms_and_server_settings() {
    let artifacts = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/mobile-e2e");
    fs::create_dir_all(&artifacts).expect("artifacts");
    let root = tempfile::Builder::new()
        .prefix("management-")
        .tempdir_in(artifacts)
        .expect("isolation")
        .keep()
        .canonicalize()
        .expect("absolute path");
    let agent = std::env::current_exe()
        .expect("test binary")
        .parent()
        .expect("deps")
        .parent()
        .expect("debug")
        .join("acp-test-agent");
    assert!(agent.is_file(), "build acp-test-agent before mobile E2E");
    let config = serde_json::json!({"agents":[{"id":"embedded","kind":"embedded"},{"id":"demo","kind":"acp","command":{"program":agent,"args":["--protocol","v1"]}}]});
    let (_daemon, endpoint) = daemon_with(&root, &config.to_string());
    let runtime = tokio::runtime::Runtime::new().expect("runtime");
    runtime.block_on(async {
        let mut client = Client::connect(endpoint.clone()).await.expect("connect");
        client
            .create_workspace(pb::CreateWorkspaceRequest {
                cwd: root.to_string_lossy().into(),
                name: Some("ACP project".into()),
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
    let mut trace = fs::File::create(root.join("ui-trace.log")).expect("UI trace");
    app.observe(move |step, batch| nsh_testing::print_batch(&mut trace, step, batch));
    app.wait_for_within("text:Connected", DEADLINE)
        .expect("connect");
    app.press("text:New conversation")
        .expect("new conversation");
    app.press("label:demo").expect("create ACP session");
    app.wait_for_gone("text:New conversation", DEADLINE)
        .expect("drawer closed");
    app.wait_for_gone("label:Model", DEADLINE)
        .expect("collapsed composer has no model");
    app.type_text("label:Message your agent…", "/permission")
        .expect("focus composer");
    app.set_environment("keyboard shown", |env| env.keyboard_height = px(300.0))
        .expect("IME inset");
    app.wait_for_within("text:Fast", DEADLINE)
        .expect("ACP reported model selector");
    app.press("label:Model").expect("model selector");
    app.wait_for_within("role:option[Deep]", DEADLINE)
        .expect("model choices");
    app.press("role:option[Deep]").expect("choose model");
    app.wait_for_within("text:Deep", DEADLINE)
        .expect("acknowledged model");
    app.press("label:Session configuration")
        .expect("harness menu");
    app.press("label:Thinking").expect("ACP boolean toggle");
    app.back().expect("close harness menu");
    app.wait_until("send available", DEADLINE, |screen| {
        screen.enabled("label:Send").unwrap_or(false)
    })
    .expect("ready to submit");
    app.press("label:Send").expect("submit");
    app.wait_for_within("label:Allow once", DEADLINE)
        .expect("pending permission");
    app.press("label:Allow once")
        .expect("answer agent-provided option");
    app.wait_for_within("text:/Permission granted/", DEADLINE)
        .expect("tool resumed");
    app.wait_for_within("label:Send", DEADLINE)
        .expect("turn completed");
    app.type_text("label:Message your agent…", "/elicit")
        .expect("elicitation prompt");
    app.wait_until("send available", DEADLINE, |screen| {
        screen.enabled("label:Send").unwrap_or(false)
    })
    .expect("ready to submit");
    app.press("label:Send").expect("submit");
    app.wait_for_within("label:Project", DEADLINE)
        .expect("agent form");
    app.type_text("label:Project", "Mobile project")
        .expect("form field");
    app.press("label:Continue").expect("submit form");
    app.wait_for_within("text:/Elicitation result: accept/", DEADLINE)
        .expect("validated form response");
    app.wait_for_within("label:Send", DEADLINE)
        .expect("form turn complete");
    app.type_text("label:Message your agent…", "/tool")
        .expect("tool prompt");
    app.press("label:Send").expect("submit tool demo");
    app.wait_for_within("text:/Tool call demo complete/", DEADLINE)
        .expect("tool completed");
    app.press("text:Inspect the scripted workspace")
        .expect("expand tool");
    app.wait_for_within("text:Input", DEADLINE)
        .expect("tool input rendered");
    app.wait_for_within("text:/Updated by acp-test-agent/", DEADLINE)
        .expect("structured diff rendered");
    app.wait_for_gone("text:Output", DEADLINE)
        .expect("structured diff replaces duplicate raw output");
    app.press("label:Back to content top")
        .expect("return to expanded header");
    app.press("label:Collapse content")
        .expect("collapse from footer");
    app.wait_for_gone("text:/Updated by acp-test-agent/", DEADLINE)
        .expect("collapsed tool body");
    app.type_text("label:Message your agent…", "/activity")
        .expect("activity demo");
    app.wait_until("send available", DEADLINE, |screen| {
        screen.enabled("label:Send").unwrap_or(false)
    })
    .expect("ready");
    app.press("label:Send").expect("submit grouped activity");
    app.wait_for_within("text:Activity demo complete.", DEADLINE)
        .expect("grouped turn completed");
    app.wait_for("text:The first activity group is complete.")
        .expect("prose outside fold");
    app.wait_for("label:Retry verification after the explanation")
        .expect("prose splits activity groups");
    app.wait_for_gone("label:Verify exported image", DEADLINE)
        .expect("activities collapsed by default");
    app.press("id:activity-summary")
        .expect("expand activity group");
    app.wait_for("label:Verify exported image")
        .expect("group members visible");
    app.wait_for("label:Failed")
        .expect("failed tool remains visible");
    app.press("label:Verify exported image")
        .expect("expand failed tool");
    app.wait_for("text:/Scripted verification failure/")
        .expect("full result preserved");
    app.press("id:activity-summary")
        .expect("collapse activity group");
    app.wait_for_gone("label:Verify exported image", DEADLINE)
        .expect("group collapsed again");
    fs::write(
        root.join("grouped-activity.txt"),
        app.screen().texts().join("\n"),
    )
    .expect("grouping artifact");
    app.set_environment("keyboard hidden for result review", |env| {
        env.keyboard_height = px(0.0)
    })
    .expect("review result above collapsed keyboard");
    app.scroll("label:Retry verification after the explanation", px(-300.0))
        .expect("scroll image tool into view");
    app.press("label:Retry verification after the explanation")
        .expect("expand image result");
    app.wait_for("id:message-image").expect("image rendered");
    app.wait_for("text:/Image verification complete/")
        .expect("streamed terminal result rendered");
    app.wait_for("text:/progress: 2/")
        .expect("terminal carriage return applied");
    app.wait_for_gone("text:/progress: 1/", DEADLINE)
        .expect("terminal line replaced");
    app.wait_for_gone("text:/iVBORw0KGgo/", DEADLINE)
        .expect("image encoding is not rendered as text");
    fs::write(
        root.join("image-result.txt"),
        app.screen().texts().join("\n"),
    )
    .expect("image result artifact");
    app.press("label:Retry verification after the explanation")
        .expect("collapse image result");
    app.wait_for_gone("id:message-image", DEADLINE)
        .expect("image unmounted when collapsed");
    app.press("label:Retry verification after the explanation")
        .expect("reopen image result");
    app.wait_for("id:message-image").expect("image remounted");
    app.type_text("label:Message your agent…", "")
        .expect("return to composer");
    app.set_environment("keyboard shown for attachment", |env| {
        env.keyboard_height = px(300.0)
    })
    .expect("attachment composer");
    app.type_text("label:Message your agent…", "/usage")
        .expect("request usage update");
    app.press("label:Send").expect("submit usage demo");
    app.wait_for_within("text:/Usage update sent/", DEADLINE)
        .expect("agent usage arrived");
    app.press("label:Session configuration")
        .expect("usage menu");
    app.wait_for("label:Context window")
        .expect("context progress bar");
    app.wait_for("text:15%").expect("context usage percentage");
    app.back().expect("close usage menu");
    let attachment = root.join("context.txt");
    fs::write(&attachment, "Context from mobile attachment").expect("attachment fixture");
    app.on_pick(move |_| {
        vec![nsh_core::picker::PickedFile::new(
            "context.txt",
            "text/plain",
            fs::read(&attachment).expect("selected file").into(),
        )]
    });
    app.press("label:Attach file").expect("attachments menu");
    app.press("label:Files").expect("file picker");
    app.wait_for_within("label:context.txt", DEADLINE)
        .expect("attachment chip");
    app.press("label:context.txt").expect("preview attachment");
    app.wait_for("text:Context from mobile attachment")
        .expect("text preview");
    app.back().expect("close preview");
    app.wait_for_gone("text:Context from mobile attachment", DEADLINE)
        .expect("preview closed");
    app.wait_for("label:context.txt")
        .expect("preview preserves attachment");
    app.press("label:context.txt").expect("reopen preview");
    app.wait_for("text:Context from mobile attachment")
        .expect("preview can reopen");
    app.press("label:Cancel").expect("close preview button");
    app.wait_for_gone("text:Context from mobile attachment", DEADLINE)
        .expect("preview closes again");
    app.long_press("label:context.txt")
        .expect("attachment actions");
    app.press("label:Remove attachment")
        .expect("remove from menu");
    app.wait_for_gone("label:context.txt", DEADLINE)
        .expect("attachment removed");
    app.set_environment("keyboard hidden for navigation", |env| {
        env.keyboard_height = px(0.0)
    })
    .expect("hide keyboard");
    app.long_press("id:turn-rail").expect("turn overview");
    app.wait_for("label:Bookmarked turns only")
        .expect("turn bookmark filter");
    app.press("label:Toggle turn bookmark 1")
        .expect("bookmark first prompt");
    app.press("label:Bookmarked turns only")
        .expect("filter bookmarked turns");
    app.wait_for_gone("label:/Jump to turn.*2/", DEADLINE)
        .expect("only bookmarked turn remains");
    app.press("label:/Jump to turn.*1/")
        .expect("jump to bookmarked prompt");
    app.wait_for_gone("label:Bookmarked turns only", DEADLINE)
        .expect("overview closes on selection");
    app.drag("id:turn-rail", point(px(0.0), px(120.0)))
        .expect("scrub forward across turns");
    fs::write(
        root.join("acp-conversation.txt"),
        app.screen().texts().join("\n"),
    )
    .expect("conversation artifact");

    app.set_environment("keyboard for draft", |env| env.keyboard_height = px(300.0))
        .expect("show keyboard");
    app.type_text("label:Message your agent…", "Keep my draft")
        .expect("draft before settings");
    app.set_environment("keyboard hidden", |env| env.keyboard_height = px(0.0))
        .expect("hide IME");
    app.wait_for_gone("label:Model", DEADLINE)
        .expect("composer collapses");
    app.wait_for("text:Keep my draft")
        .expect("draft survives collapse");
    app.press("label:Open sidebar").expect("session drawer");
    app.long_press("label:Untitled conversation")
        .expect("session context menu");
    app.wait_for("label:Rename conversation")
        .expect("session actions");
    app.press("label:Rename conversation")
        .expect("rename sheet");
    app.clear("label:Conversation title").expect("clear title");
    app.type_text("label:Conversation title", "Reviewed layout")
        .expect("title");
    app.press("label:Save").expect("save title");
    app.wait_for_within("label:Reviewed layout", DEADLINE)
        .expect("renamed");
    app.long_press("label:Reviewed layout")
        .expect("session menu");
    app.press("label:Pin conversation").expect("pin session");
    app.wait_for("text:Pinned conversations")
        .expect("separate pinned section");
    app.long_press("label:Reviewed layout")
        .expect("pinned session menu");
    app.press("label:Add bookmark").expect("bookmark session");
    app.long_press("label:Reviewed layout")
        .expect("pinned menu again");
    app.press("label:Unpin conversation")
        .expect("unpin session");
    app.wait_for_gone("text:Pinned conversations", DEADLINE)
        .expect("empty pinned section removed");
    app.press("label:Filter conversations")
        .expect("conversation filters");
    app.press("label:Bookmarked conversations")
        .expect("bookmarked sessions");
    app.wait_for("label:Reviewed layout")
        .expect("bookmark filter includes session");
    app.press("label:Filter conversations")
        .expect("restore filter");
    app.press("label:By time").expect("date grouping");
    app.wait_for("text:Today").expect("date section");
    app.back().expect("close drawer");
    app.wait_for("text:Keep my draft")
        .expect("rename preserves draft");
    let light = app
        .screen()
        .span_style("text:Reviewed layout")
        .expect("light title")
        .color;
    app.set_environment("Android system dark theme", |env| {
        env.color_scheme = nsh_core::platform::ColorScheme::Dark
    })
    .expect("system dark");
    let dark = app
        .screen()
        .span_style("text:Reviewed layout")
        .expect("dark title")
        .color;
    assert_ne!(
        light, dark,
        "system appearance changes the application's colors"
    );
    app.set_environment("Android system light theme", |env| {
        env.color_scheme = nsh_core::platform::ColorScheme::Light
    })
    .expect("system light");
    assert_eq!(
        light,
        app.screen()
            .span_style("text:Reviewed layout")
            .expect("restored light title")
            .color
    );
    fs::write(
        root.join("system-theme.txt"),
        format!("Light: {light:?}\nDark: {dark:?}\n"),
    )
    .expect("theme artifact");
    app.press("label:App menu").expect("global menu");
    app.wait_for_gone("label:Rename conversation", DEADLINE)
        .expect("global menu excludes session actions");
    app.press("label:Settings").expect("settings");
    app.press("label:Conversation preferences")
        .expect("frontend preferences");
    app.press("label:Show thinking").expect("toggle preference");
    app.press("label:Back").expect("settings overview");
    app.press("label:Back").expect("conversation");
    app.wait_for("text:Keep my draft")
        .expect("settings preserves draft");
    app.press("label:App menu").expect("global menu");
    app.press("label:Settings").expect("settings");
    app.press("label:Session management").expect("policy");
    app.wait_for_within("text:1800", DEADLINE)
        .expect("server policy");
    app.clear("label:Idle timeout (seconds)")
        .expect("clear idle timeout");
    app.type_text("label:Idle timeout (seconds)", "600")
        .expect("edit timeout");
    app.wait_for_within("text:Saved", DEADLINE)
        .expect("saved acknowledgement");
    let saved = runtime.block_on(async {
        Client::connect(endpoint.clone())
            .await
            .expect("client")
            .get_acp_configuration(pb::GetAcpConfigurationRequest {})
            .await
            .expect("read policy")
            .into_inner()
    });
    let policy: bas_client::config::AcpLifecycle =
        serde_json::from_slice(&saved.policy_json).expect("policy");
    assert_eq!(policy.idle_timeout_secs.expect("idle timeout").get(), 600);

    app.press("label:Back").expect("settings overview");
    app.press("label:Agents").expect("agents");
    app.wait_until("agent catalog ready", DEADLINE, |screen| {
        screen.enabled("label:Add agent").unwrap_or(false)
    })
    .expect("agent page");
    app.press("label:Add agent").expect("catalog");
    app.wait_until("custom registration ready", DEADLINE, |screen| {
        screen.enabled("label:Custom agent").unwrap_or(false)
    })
    .expect("catalog loaded");
    app.press("label:Custom agent")
        .expect("new custom registration");
    app.type_text("label:Agent ID", "mobile-added")
        .expect("agent id");
    app.type_text(
        "label:Executable on server",
        agent.to_str().expect("agent path"),
    )
    .expect("program");
    app.type_text("label:Arguments · one per line", "--protocol\nv1")
        .expect("arguments");
    app.press("label:Save").expect("save custom agent");
    app.wait_for_within("text:mobile-added", DEADLINE)
        .expect("registered agent");
    app.press("text:mobile-added").expect("agent details");
    app.press("label:Check connection")
        .expect("check added agent");
    app.wait_for_within("text:/Connected/", DEADLINE)
        .expect("handshake result");
    let registrations = runtime.block_on(async {
        Client::connect(endpoint.clone())
            .await
            .expect("client")
            .get_agent_configuration(pb::GetAgentConfigurationRequest {})
            .await
            .expect("read registrations")
            .into_inner()
    });
    let registrations: Vec<bas_client::config::AgentRegistration> =
        serde_json::from_slice(&registrations.registrations_json).expect("registrations");
    assert_eq!(registrations.len(), 3);
    assert_eq!(
        registrations
            .last()
            .expect("new registration")
            .id()
            .as_str(),
        "mobile-added"
    );
    fs::write(root.join("settings.txt"), app.screen().texts().join("\n"))
        .expect("settings artifact");
    app.press("label:Back").expect("agents");
    app.press("label:Back").expect("settings");
    app.press("label:Back").expect("conversation");
    app.clear("label:Message your agent…").expect("clear draft");
    app.type_text("label:Message your agent…", "/stop refusal")
        .expect("refusal response");
    app.press("label:Send").expect("submit refusal turn");
    app.wait_for_within("text:The agent refused to continue this turn.", DEADLINE)
        .expect("successful RPC with refusal still explains the outcome");
    app.type_text("label:Message your agent…", "/permission")
        .expect("start a turn that waits for user input");
    app.press("label:Send").expect("submit interrupted turn");
    app.wait_for_within("label:Allow once", DEADLINE)
        .expect("agent is running");
    app.wait_for("text:Agent is working…")
        .expect("visible execution status");
    runtime.block_on(async {
        let mut client = Client::connect(endpoint).await.expect("client");
        let session = client
            .list_sessions(pb::ListSessionsRequest {
                include_archived: false,
            })
            .await
            .expect("sessions")
            .into_inner()
            .sessions
            .into_iter()
            .find(|session| session.agent == "demo")
            .expect("active demo session");
        client
            .close_session(pb::GetSessionRequest {
                session: session.id,
            })
            .await
            .expect("stop the real agent during its turn");
    });
    app.wait_for_within(
        "text:The conversation was stopped before the reply completed.",
        DEADLINE,
    )
    .expect("interrupted turn must explain why no reply arrived");
    app.wait_for_gone("label:Cancel", DEADLINE)
        .expect("interrupted turn no longer running");
    app.wait_for_gone("id:prompt-progress", DEADLINE)
        .expect("progress ends with interruption");
    fs::write(
        root.join("interrupted-turn.txt"),
        app.screen().texts().join("\n"),
    )
    .expect("interruption artifact");
    fs::write(root.join("result.txt"), "PASS: real ACP model and boolean configuration, tool approval, form elicitation, keyboard expansion/collapse, draft preservation, long-press session actions, pin/bookmark filters, timestamp groups, turn rail and bookmarked turn navigation, attachment preview/reopen/back/delete, expanded content footer, Android system light/dark theme, section-based settings, revision-checked auto-save and custom agent registration\n").expect("result artifact");
}
