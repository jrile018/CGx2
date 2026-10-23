//! Deterministic test child. This is not a language evaluator or an ecosystem adapter.
use cgx2_interop_worker::v1::*;
use std::{
    io::{Read, Write},
    time::Duration,
};

fn read(limits: &Limits) -> Result<Envelope> {
    let mut len = [0; 4];
    std::io::stdin().read_exact(&mut len)?;
    let n = u32::from_be_bytes(len) as usize;
    if n == 0 || n > limits.frame_bytes {
        return Err(Error::Limit("fixture frame".into()));
    }
    let mut bytes = vec![0; n];
    std::io::stdin().read_exact(&mut bytes)?;
    decode(&bytes, limits)
}
fn write(e: &Envelope, limits: &Limits) -> Result<()> {
    std::io::stdout().write_all(&encode(e, limits)?)?;
    std::io::stdout().flush()?;
    Ok(())
}
fn main() {
    if run().is_err() {
        std::process::exit(2);
    }
}
fn run() -> Result<()> {
    let first = read(&Limits::default())?;
    if first.kind != Kind::Hello || first.request_id != 0 {
        return Err(Error::Protocol("hello required".into()));
    }
    let mut hello: Hello = payload(&first)?;
    hello.limits.validate()?;
    let mode = std::env::var("FIXTURE_MODE").unwrap_or_default();
    match mode.as_str() {
        "auth" => hello.nonce.push('x'),
        "identity" => hello.runtime_identity.push('x'),
        "hello_hang" => std::thread::sleep(Duration::from_secs(10)),
        _ => {}
    }
    write(
        &envelope(&first.session_id, 0, Kind::Hello, &hello)?,
        &hello.limits,
    )?;
    if mode == "stall" {
        std::thread::sleep(Duration::from_secs(10));
    }
    let mut counter = 0u64;
    let mut last_id = 0;
    loop {
        let request = read(&hello.limits)?;
        if request.session_id != first.session_id
            || request.request_id == 0
            || request.request_id <= last_id
        {
            return Err(Error::DuplicateRequest);
        }
        last_id = request.request_id;
        if request.kind == Kind::Shutdown || request.kind == Kind::Cancel {
            return Ok(());
        }
        if request.kind != Kind::Invoke {
            return Err(Error::Protocol("invoke required".into()));
        }
        let invocation: Invocation = payload(&request)?;
        if !hello.policy.operations.contains(&invocation.operation) {
            return Err(Error::Unavailable("policy".into()));
        }
        let mut result = WireValue::U64(counter);
        match invocation.operation.as_str() {
            "increment" | "write" | "effect_hang" | "effect_crash" => {
                counter += 1;
                result = WireValue::U64(counter);
                if invocation.operation != "increment" {
                    let file = std::env::var("FIXTURE_EFFECT")
                        .map_err(|_| Error::Protocol("effect path".into()))?;
                    let mut output = std::fs::OpenOptions::new()
                        .create(true)
                        .append(true)
                        .open(file)?;
                    writeln!(output, "{counter}")?;
                    output.sync_all()?;
                }
                if invocation.operation == "effect_hang" {
                    std::thread::sleep(Duration::from_secs(10));
                }
                if invocation.operation == "effect_crash" {
                    std::process::exit(7);
                }
            }
            "echo" => {
                result = invocation
                    .arguments
                    .first()
                    .cloned()
                    .unwrap_or(WireValue::Unit)
            }
            "descendant" => {
                let mut child = std::process::Command::new("/bin/sleep").arg("30").spawn()?;
                result = WireValue::U64(child.id() as u64);
                // Retain descendant intentionally so the parent cleanup can be fault-tested.
                if std::env::var_os("FIXTURE_WAIT").is_some() {
                    let _ = child.wait();
                }
            }
            "fd_probe" => {
                #[cfg(target_os = "linux")]
                // SAFETY: test only checks the chosen descriptor; it does not dereference memory.
                {
                    result = WireValue::Bool(unsafe { libc::fcntl(250, libc::F_GETFD) } < 0);
                }
            }
            "group_escape" => {
                #[cfg(target_os = "linux")]
                // SAFETY: fixture child deliberately attempts group escape; guard must deny it.
                {
                    result = WireValue::Bool(unsafe { libc::setsid() } < 0);
                }
            }
            "confinement_probe" => {
                #[cfg(target_os = "linux")]
                {
                    let allowed = std::env::var("FIXTURE_ALLOWED").unwrap();
                    let denied = std::env::var("FIXTURE_DENIED").unwrap();
                    let read_allowed = std::fs::read(&allowed).is_ok();
                    let write_denied = std::fs::OpenOptions::new()
                        .write(true)
                        .open(&allowed)
                        .is_err();
                    let read_denied = std::fs::read(denied).is_err();
                    // SAFETY: documented socket call with no pointers. No network connection is made.
                    let socket = unsafe { libc::socket(libc::AF_INET, libc::SOCK_STREAM, 0) };
                    let net_denied = socket < 0;
                    if socket >= 0 {
                        unsafe {
                            libc::close(socket);
                        }
                    }
                    // SAFETY: probe runs only in a test child. Seccomp must prevent group escape.
                    let group_denied = unsafe { libc::setsid() } < 0;
                    result = WireValue::List(
                        vec![
                            read_allowed,
                            write_denied,
                            read_denied,
                            net_denied,
                            group_denied,
                        ]
                        .into_iter()
                        .map(WireValue::Bool)
                        .collect(),
                    );
                }
            }
            "foreign_error" => {
                write(
                    &envelope(
                        &request.session_id,
                        request.request_id,
                        Kind::Error,
                        &ForeignFailure {
                            code: "fixture".into(),
                            message: "failure".into(),
                            state_bytes: 8,
                        },
                    )?,
                    &hello.limits,
                )?;
                continue;
            }
            "events" | "flood" => {
                for _ in 0..if invocation.operation == "flood" {
                    65
                } else {
                    2
                } {
                    write(
                        &envelope(
                            &request.session_id,
                            request.request_id,
                            Kind::Stdout,
                            &Event {
                                text: "fixture output".into(),
                            },
                        )?,
                        &hello.limits,
                    )?;
                }
            }
            "malformed" => {
                std::io::stdout().write_all(&[0, 0, 0, 1, b'{'])?;
                continue;
            }
            "oversized" => {
                std::io::stdout().write_all(&u32::MAX.to_be_bytes())?;
                continue;
            }
            "truncated" => {
                std::io::stdout().write_all(&[0, 0, 0, 10, b'{'])?;
                return Ok(());
            }
            "crash" => std::process::exit(7),
            "hang" => std::thread::sleep(Duration::from_secs(10)),
            _ => {}
        }
        let id = if invocation.operation == "unsolicited" {
            request.request_id + 1
        } else {
            request.request_id
        };
        let session = if invocation.operation == "wrong_session" {
            "other"
        } else {
            &request.session_id
        };
        let response = envelope(
            session,
            id,
            Kind::Result,
            &Returned {
                value: result,
                state_bytes: if invocation.operation == "state_limit" {
                    usize::MAX
                } else {
                    8
                },
            },
        )?;
        write(&response, &hello.limits)?;
        if invocation.operation == "duplicate" {
            write(&response, &hello.limits)?;
        }
    }
}
