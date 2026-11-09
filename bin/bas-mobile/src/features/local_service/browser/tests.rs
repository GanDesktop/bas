use nsh_core::{
    element::Root,
    platform::{Environment, Idiom, Os},
    prelude::*,
};
use nsh_engine::EngineConfig;
use nsh_testing::Headless;
use std::{
    fs,
    path::PathBuf,
    process::{Child, Command, Stdio},
    sync::OnceLock,
    time::Duration,
};

static DIRECTORY: OnceLock<PathBuf> = OnceLock::new();
const DEADLINE: Duration = Duration::from_secs(15);

#[component]
fn BrowserApp() -> impl IntoElement {
    let app = crate::app::root();
    // The same watcher used for the service's JNI-provided private directory.
    super::watch(Some(
        DIRECTORY.get().expect("isolated bridge directory").clone(),
    ));
    app
}

struct Launcher(Child);
impl Drop for Launcher {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

#[test]
fn guest_browser_commands_wait_for_user_and_keep_urls_intact() {
    let artifacts = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/mobile-e2e");
    fs::create_dir_all(&artifacts).expect("artifacts");
    let root = tempfile::Builder::new()
        .prefix("browser-bridge-")
        .tempdir_in(artifacts)
        .expect("isolated bridge")
        .keep()
        .canonicalize()
        .expect("absolute directory");
    let queue = root.join("requests");
    fs::create_dir(&queue).expect("private queue");
    DIRECTORY.set(queue.clone()).expect("single UI flow");
    let helper = std::env::current_exe()
        .expect("test binary")
        .parent()
        .expect("deps")
        .parent()
        .expect("profile")
        .join("bas-open-url");
    assert!(helper.is_file(), "build bas-open-url before this test");
    let launcher = root.join("xdg-open");
    std::os::unix::fs::symlink(&helper, &launcher).expect("same alias as guest image");
    let mut app = Headless::start(EngineConfig {
        viewport: Size::new(px(390.0), px(844.0)),
        environment: Environment {
            os: Os::Android,
            idiom: Idiom::Phone,
            window: Size::new(px(390.0), px(844.0)),
            reduce_motion: true,
            ..Default::default()
        },
        ..EngineConfig::new(Root::of::<BrowserApp>())
    })
    .expect("app");
    let mut trace = fs::File::create(root.join("ui-trace.log")).expect("trace");
    app.observe(move |step, batch| nsh_testing::print_batch(&mut trace, step, batch));
    app.wait_for("label:New connection")
        .expect("app available without remote BAS");

    let url = "https://example.com/authorize?state=a%2Bb&scope=openid%20profile&redirect_uri=http%3A%2F%2F127.0.0.1%3A43123%2Fcallback";
    let mut browser = Launcher(
        Command::new("sh")
            .args(["-c", "exec \"$BROWSER\" \"$1\"", "browser-login", url])
            .env("BROWSER", &helper)
            .env(bas_browser_bridge::DIRECTORY_ENV, &queue)
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .expect("BROWSER invocation"),
    );
    app.wait_for_within("label:Open browser", DEADLINE)
        .expect("host displays the guest request");
    app.wait_for("text:example.com")
        .expect("user sees destination host");
    assert!(
        browser.0.try_wait().expect("launcher status").is_none(),
        "request waits for the user's choice"
    );
    assert!(
        app.screen().opened_urls().is_empty(),
        "no unsolicited browser launch"
    );
    fs::write(root.join("approval.txt"), app.screen().texts().join("\n")).expect("UI artifact");
    app.press("label:Open browser").expect("user opens browser");
    app.wait_for_gone("label:Open browser", DEADLINE)
        .expect("request acknowledged");
    assert!(browser.0.wait().expect("launcher completes").success());
    assert_eq!(app.screen().opened_urls(), &[url.to_owned()]);

    let mut dismissed = Launcher(
        Command::new(&launcher)
            .arg("https://example.com/cancel")
            .env(bas_browser_bridge::DIRECTORY_ENV, &queue)
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .expect("xdg-open invocation"),
    );
    app.wait_for_within("label:Dismiss request", DEADLINE)
        .expect("next request");
    app.press("label:Dismiss request").expect("user cancels");
    app.wait_for_gone("label:Dismiss request", DEADLINE)
        .expect("cancelled request cleared");
    assert_eq!(
        dismissed.0.wait().expect("cancelled launcher").code(),
        Some(4)
    );
    assert_eq!(
        app.screen().opened_urls().len(),
        1,
        "cancellation does not open a URL"
    );

    let mut closed = Launcher(
        Command::new(&launcher)
            .arg("https://example.com/back")
            .env(bas_browser_bridge::DIRECTORY_ENV, &queue)
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .expect("back dismissal"),
    );
    app.wait_for_within("label:Open browser", DEADLINE)
        .expect("request sheet");
    app.back().expect("system back");
    app.wait_for_gone("label:Open browser", DEADLINE)
        .expect("back cancels");
    assert_eq!(closed.0.wait().expect("closed launcher").code(), Some(4));

    let mut killed = Launcher(
        Command::new(&launcher)
            .arg("https://example.com/interrupted")
            .env(bas_browser_bridge::DIRECTORY_ENV, &queue)
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .expect("interrupted login"),
    );
    app.wait_for_within("label:Open browser", DEADLINE)
        .expect("live request");
    killed.0.kill().expect("agent stops its browser helper");
    killed.0.wait().expect("reap helper");
    app.wait_for_gone("label:Open browser", DEADLINE)
        .expect("ended request disappears");
    assert!(
        bas_browser_bridge::pending(&queue)
            .expect("pending requests")
            .is_empty()
    );
    let rejected = Command::new(&launcher)
        .arg("file:///etc/passwd")
        .env(bas_browser_bridge::DIRECTORY_ENV, &queue)
        .output()
        .expect("non-web invocation");
    assert_eq!(rejected.status.code(), Some(4));
    assert!(
        bas_browser_bridge::pending(&queue)
            .expect("invalid URL creates no request")
            .is_empty()
    );
    let missing = Command::new(&launcher)
        .arg(url)
        .env_remove(bas_browser_bridge::DIRECTORY_ENV)
        .output()
        .expect("missing host bridge");
    assert_eq!(missing.status.code(), Some(3));
    fs::write(root.join("result.txt"),"PASS: real BROWSER/xdg-open processes, private file handoff, user-gated openUrl, exact OAuth query preservation, cancellation, system back, process termination and invalid URL rejection. Android OS browser launch and third-party OAuth not exercised.\n").expect("result artifact");
}
