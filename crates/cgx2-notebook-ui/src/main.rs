use std::env;
use std::net::TcpListener;
use std::path::PathBuf;
use std::process::ExitCode;

use cgx2_notebook_kernel::{is_worker_argument, scalar_worker_main};
use cgx2_notebook_ui::{serve, UiSession};

fn run() -> Result<(), String> {
    let mut args = env::args_os().skip(1);
    if args.next().as_deref() != Some(std::ffi::OsStr::new("open")) {
        return Err("usage: cgx2-notebook-ui open FILE.cgnb [--port PORT]".into());
    }
    let input = PathBuf::from(args.next().ok_or("open requires FILE.cgnb")?);
    let mut port = 0;
    if let Some(option) = args.next() {
        if option != "--port" {
            return Err("only --port PORT is supported".into());
        }
        port = args
            .next()
            .ok_or("--port requires a number")?
            .to_string_lossy()
            .parse::<u16>()
            .map_err(|_| "invalid port number")?;
    }
    if args.next().is_some() {
        return Err("too many arguments".into());
    }
    let mut session = UiSession::open(input)?;
    session.configure_worker(env::current_exe().map_err(|error| error.to_string())?);
    let listener = TcpListener::bind(("127.0.0.1", port))
        .map_err(|error| format!("cannot bind 127.0.0.1:{port}: {error}"))?;
    println!(
        "Open http://127.0.0.1:{}/ in a local browser. Press Ctrl+C here to stop.",
        listener
            .local_addr()
            .map_err(|error| error.to_string())?
            .port()
    );
    serve(listener, session).map_err(|error| format!("server stopped: {error}"))
}

fn main() -> ExitCode {
    if env::args()
        .nth(1)
        .as_deref()
        .is_some_and(is_worker_argument)
    {
        return if scalar_worker_main() == 0 {
            ExitCode::SUCCESS
        } else {
            ExitCode::FAILURE
        };
    }
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}
