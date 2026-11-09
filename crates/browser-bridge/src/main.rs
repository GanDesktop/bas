use std::{path::PathBuf, process::ExitCode};

fn main() -> ExitCode {
    let mut args = std::env::args().skip(1);
    let first = args.next();
    if first.as_deref() == Some("--help") {
        println!(
            "Usage: bas-open-url [--] <http-or-https-url>\nOpens a URL through the BAS Android app after user confirmation."
        );
        return ExitCode::SUCCESS;
    }
    if first.as_deref() == Some("--version") {
        println!("bas-open-url {}", env!("CARGO_PKG_VERSION"));
        return ExitCode::SUCCESS;
    }
    let url = if first.as_deref() == Some("--") {
        args.next()
    } else {
        first
    };
    let Some(url) = url.filter(|_| args.next().is_none()) else {
        eprintln!("Usage: bas-open-url [--] <http-or-https-url>");
        return ExitCode::from(1);
    };
    let Some(directory) = std::env::var_os(bas_browser_bridge::DIRECTORY_ENV).map(PathBuf::from)
    else {
        eprintln!("BAS browser bridge is unavailable outside the app's local service");
        return ExitCode::from(3);
    };
    match bas_browser_bridge::open(&directory, &url) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            // URLs can contain OAuth codes; never include the destination in diagnostics.
            eprintln!("{error}");
            ExitCode::from(4)
        }
    }
}
