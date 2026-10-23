#![cfg(target_os = "linux")]
use cgx2_interop_worker::v1::*;
use std::{collections::BTreeMap, path::PathBuf, time::Duration};

fn config() -> Config {
    let path = PathBuf::from(env!("CARGO_BIN_EXE_cgx2-worker-fixture"));
    let environment = BTreeMap::new();
    Config {
        executable: Artifact {
            sha256: digest(&std::fs::read(&path).unwrap()),
            path,
        },
        artifacts: vec![],
        arguments: vec![],
        environment_sha256: environment_digest(&environment),
        environment,
        runtime_identity: "cgx2-fixture-v1".into(),
        limits: Limits::default(),
        policy: CapabilityPolicy {
            required: Confinement::Unrestricted,
            resources: vec![],
            operations: [
                "crash",
                "fd_probe",
                "group_escape",
                "increment",
                "write",
                "effect_hang",
                "effect_crash",
                "echo",
                "descendant",
                "foreign_error",
                "events",
                "flood",
                "malformed",
                "oversized",
                "truncated",
                "hang",
                "unsolicited",
                "wrong_session",
                "state_limit",
                "duplicate",
            ]
            .map(String::from)
            .to_vec(),
        },
    }
}
fn env(c: &mut Config, name: &str, value: &str) {
    c.environment.insert(name.into(), value.into());
    c.environment_sha256 = environment_digest(&c.environment);
}
fn call(session: &mut Session, id: u64, operation: &str) -> Result<Output> {
    session.invoke(
        id,
        Invocation {
            operation: operation.into(),
            arguments: vec![],
        },
        &Cancellation::default(),
    )
}
fn value(output: Output) -> WireValue {
    output.result.unwrap()
}
fn effect_path(test: &str) -> PathBuf {
    std::env::temp_dir().join(format!("worker-{test}-{}", std::process::id()))
}
#[test]
fn retained_state_and_no_replay_of_write_effect() {
    let mut c = config();
    let path = effect_path("write");
    let _ = std::fs::remove_file(&path);
    env(&mut c, "FIXTURE_EFFECT", path.to_str().unwrap());
    let mut session = Session::spawn(c).unwrap();
    assert_eq!(session.enforcement(), Enforcement::Unrestricted);
    assert_eq!(
        value(call(&mut session, 1, "write").unwrap()),
        WireValue::U64(1)
    );
    assert_eq!(call(&mut session, 1, "write"), Err(Error::DuplicateRequest));
    assert_eq!(call(&mut session, 0, "write"), Err(Error::DuplicateRequest));
    assert_eq!(
        value(call(&mut session, 2, "increment").unwrap()),
        WireValue::U64(2)
    );
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "1\n");
    session.shutdown();
    assert_eq!(session.state(), State::Closed);
    assert_eq!(call(&mut session, 3, "write"), Err(Error::Closed));
    std::fs::remove_file(path).unwrap();
}
#[test]
fn pins_authentication_and_actual_confinement_fail_closed() {
    let mut c = config();
    c.executable.sha256 = "bad".into();
    assert!(matches!(Session::spawn(c), Err(Error::Identity(_))));
    let mut c = config();
    c.environment.insert("BAD".into(), "value".into());
    assert!(matches!(Session::spawn(c), Err(Error::Identity(_))));
    let mut c = config();
    c.artifacts.push(Artifact {
        path: c.executable.path.clone(),
        sha256: "bad".into(),
    });
    assert!(matches!(Session::spawn(c), Err(Error::Identity(_))));
    for mode in ["auth", "identity"] {
        let mut c = config();
        env(&mut c, "FIXTURE_MODE", mode);
        assert!(
            matches!(Session::spawn(c), Err(Error::Identity(_))),
            "{mode}"
        );
    }
    let mut c = config();
    c.policy.required = Confinement::Restricted;
    assert!(Session::spawn(c).is_err()); // No loader read roots: restricted launch must fail closed.
}
#[test]
fn malformed_oversized_truncated_unsolicited_and_state_frames_poison() {
    for operation in [
        "malformed",
        "oversized",
        "truncated",
        "unsolicited",
        "wrong_session",
        "state_limit",
        "flood",
    ] {
        let mut session = Session::spawn(config()).unwrap();
        assert!(
            matches!(
                call(&mut session, 1, operation),
                Err(Error::OutcomeUnknown(_))
            ),
            "{operation}"
        );
        assert_eq!(session.state(), State::Poisoned);
        assert_eq!(call(&mut session, 2, "increment"), Err(Error::Closed));
    }
}
#[test]
fn duplicated_terminal_frame_is_never_accepted_for_next_request() {
    let mut session = Session::spawn(config()).unwrap();
    // Either detect both frames immediately or reject the stale frame before/while dispatching #2.
    let first = call(&mut session, 1, "duplicate");
    if first.is_ok() {
        std::thread::sleep(Duration::from_millis(20));
        assert!(call(&mut session, 2, "increment").is_err());
    }
    assert_eq!(session.state(), State::Poisoned);
}
#[test]
fn uncertain_effects_are_never_retried_after_crash_timeout_or_cancel() {
    for operation in ["effect_hang", "effect_crash"] {
        let mut c = config();
        c.limits.timeout_ms = 200;
        let path = effect_path(operation);
        let _ = std::fs::remove_file(&path);
        env(&mut c, "FIXTURE_EFFECT", path.to_str().unwrap());
        let mut session = Session::spawn(c).unwrap();
        assert!(matches!(
            call(&mut session, 1, operation),
            Err(Error::OutcomeUnknown(_))
        ));
        assert_eq!(call(&mut session, 1, operation), Err(Error::Closed));
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "1\n");
        std::fs::remove_file(path).unwrap();
    }
    let mut c = config();
    let path = effect_path("cancel");
    let _ = std::fs::remove_file(&path);
    env(&mut c, "FIXTURE_EFFECT", path.to_str().unwrap());
    let mut session = Session::spawn(c).unwrap();
    let cancel = Cancellation::default();
    let signal = cancel.clone();
    let observed = path.clone();
    let thread = std::thread::spawn(move || {
        for _ in 0..1000 {
            if std::fs::read_to_string(&observed).is_ok_and(|s| s == "1\n") {
                signal.cancel();
                return;
            }
            std::thread::sleep(Duration::from_millis(1));
        }
        panic!("fixture never wrote effect");
    });
    assert!(matches!(
        session.invoke(
            1,
            Invocation {
                operation: "effect_hang".into(),
                arguments: vec![]
            },
            &cancel
        ),
        Err(Error::OutcomeUnknown(_))
    ));
    thread.join().unwrap();
    assert_eq!(session.state(), State::Poisoned);
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "1\n");
    std::fs::remove_file(path).unwrap();
}
#[test]
fn events_foreign_failure_and_predispatch_validation() {
    let mut session = Session::spawn(config()).unwrap();
    assert!(matches!(
        call(&mut session, 1, "not_admitted"),
        Err(Error::Unavailable(_))
    ));
    let cancel = Cancellation::default();
    cancel.cancel();
    assert_eq!(
        session.invoke(
            1,
            Invocation {
                operation: "increment".into(),
                arguments: vec![]
            },
            &cancel
        ),
        Err(Error::Cancelled)
    );
    let events = call(&mut session, 1, "events").unwrap();
    assert_eq!(events.events.len(), 2);
    assert!(
        call(&mut session, 2, "foreign_error")
            .unwrap()
            .result
            .is_err()
    );
    assert_eq!(
        value(call(&mut session, 3, "increment").unwrap()),
        WireValue::U64(1)
    );
}
fn not_running(pid: u32) -> bool {
    match std::fs::read_to_string(format!("/proc/{pid}/stat")) {
        Ok(stat) => stat
            .rsplit_once(") ")
            .is_some_and(|(_, rest)| rest.starts_with('Z')),
        Err(_) => true,
    }
}
#[test]
fn shutdown_drop_and_timeout_kill_child_process_group() {
    for action in ["shutdown", "drop", "timeout", "crash", "cancel"] {
        let mut c = config();
        c.limits.timeout_ms = 200;
        let mut session = Session::spawn(c).unwrap();
        let parent = session.child_id();
        assert_eq!(
            value(call(&mut session, 1, "group_escape").unwrap()),
            WireValue::Bool(true)
        );
        let WireValue::U64(descendant) = value(call(&mut session, 2, "descendant").unwrap()) else {
            panic!()
        };
        match action {
            "shutdown" => session.shutdown(),
            "crash" => {
                assert!(matches!(
                    call(&mut session, 3, "crash"),
                    Err(Error::OutcomeUnknown(_))
                ));
            }
            "cancel" => {
                let signal = Cancellation::default();
                let other = signal.clone();
                let thread = std::thread::spawn(move || {
                    std::thread::sleep(Duration::from_millis(10));
                    other.cancel();
                });
                assert!(matches!(
                    session.invoke(
                        3,
                        Invocation {
                            operation: "hang".into(),
                            arguments: vec![]
                        },
                        &signal
                    ),
                    Err(Error::OutcomeUnknown(_))
                ));
                thread.join().unwrap();
            }
            "timeout" => {
                assert!(matches!(
                    call(&mut session, 3, "hang"),
                    Err(Error::OutcomeUnknown(_))
                ));
            }
            _ => {}
        }
        drop(session);
        for _ in 0..100 {
            if not_running(descendant as u32) {
                break;
            }
            std::thread::sleep(Duration::from_millis(2));
        }
        assert!(not_running(parent));
        assert!(
            not_running(descendant as u32),
            "descendant survived {action}"
        );
    }
}
#[test]
fn limits_and_handshake_deadline_are_checked() {
    for limits in [
        Limits {
            timeout_ms: 0,
            ..Limits::default()
        },
        Limits {
            timeout_ms: 60_001,
            ..Limits::default()
        },
        Limits {
            frame_bytes: usize::MAX,
            ..Limits::default()
        },
        Limits {
            state_bytes: 0,
            ..Limits::default()
        },
    ] {
        let mut c = config();
        c.limits = limits;
        assert!(matches!(Session::spawn(c), Err(Error::Limit(_))));
    }
    let mut c = config();
    c.limits.timeout_ms = 50;
    env(&mut c, "FIXTURE_MODE", "hello_hang");
    assert!(matches!(Session::spawn(c), Err(Error::Limit(_))));
}

#[test]
fn restricted_host_enforces_real_readonly_filesystem_and_network_or_reports_unavailable() {
    let mut c = config();
    c.policy.required = Confinement::Restricted;
    c.policy.operations.push("confinement_probe".into());
    let allowed = effect_path("allowed");
    let denied = effect_path("denied");
    std::fs::write(&allowed, "pinned read\n").unwrap();
    std::fs::write(&denied, "secret\n").unwrap();
    c.policy.resources = ["/lib", "/lib64", "/usr/lib", "/etc/ld.so.cache"]
        .into_iter()
        .filter(|p| std::path::Path::new(p).exists())
        .map(String::from)
        .collect();
    c.policy.resources.push(allowed.to_str().unwrap().into());
    env(&mut c, "FIXTURE_ALLOWED", allowed.to_str().unwrap());
    env(&mut c, "FIXTURE_DENIED", denied.to_str().unwrap());
    let outcome = Session::spawn(c);
    match outcome {
        Ok(mut session) => {
            eprintln!("real confinement active: {:?}", session.enforcement());
            assert!(matches!(
                session.enforcement(),
                Enforcement::LinuxReadOnlyNoNetwork { .. }
            ));
            assert_eq!(
                value(call(&mut session, 1, "confinement_probe").unwrap()),
                WireValue::List(vec![WireValue::Bool(true); 5])
            );
        }
        Err(Error::Unavailable(reason)) => {
            eprintln!("real confinement unavailable: {reason}");
            assert!(
                std::env::var_os("CGX2_REQUIRE_CONFINEMENT").is_none(),
                "required real confinement unavailable: {reason}"
            );
        }
        Err(other) => panic!("unexpected confinement error: {other}"),
    }
    assert_eq!(std::fs::read_to_string(&allowed).unwrap(), "pinned read\n");
    std::fs::remove_file(allowed).unwrap();
    std::fs::remove_file(denied).unwrap();
}

#[test]
fn inherited_descriptors_and_oversized_requests_do_not_cross_boundary() {
    use std::os::fd::{AsRawFd, FromRawFd};
    let file = std::fs::File::open("/etc/hosts").unwrap();
    // SAFETY: create an owned test FD >=250 without CLOEXEC, then take ownership exactly once.
    let fd = unsafe { libc::fcntl(file.as_raw_fd(), libc::F_DUPFD, 250) };
    assert_eq!(fd, 250);
    let inherited = unsafe { std::fs::File::from_raw_fd(fd) };
    let mut session = Session::spawn(config()).unwrap();
    assert_eq!(
        value(call(&mut session, 1, "fd_probe").unwrap()),
        WireValue::Bool(true)
    );
    drop(inherited);
    let result = session.invoke(
        2,
        Invocation {
            operation: "echo".into(),
            arguments: vec![WireValue::String("x".repeat(DEFAULT_BYTES + 1))],
        },
        &Cancellation::default(),
    );
    assert!(matches!(result, Err(Error::Limit(_))));
    assert_eq!(session.state(), State::Ready);
    assert_eq!(
        value(call(&mut session, 2, "increment").unwrap()),
        WireValue::U64(1)
    );
}

#[test]
fn a_child_that_stops_reading_cannot_block_dispatch_past_deadline() {
    let mut c = config();
    c.limits.timeout_ms = 100;
    env(&mut c, "FIXTURE_MODE", "stall");
    let mut session = Session::spawn(c).unwrap();
    let start = std::time::Instant::now();
    assert!(matches!(
        session.invoke(
            1,
            Invocation {
                operation: "echo".into(),
                arguments: vec![WireValue::String("x".repeat(DEFAULT_BYTES / 2))]
            },
            &Cancellation::default()
        ),
        Err(Error::OutcomeUnknown(_))
    ));
    assert!(start.elapsed() < Duration::from_secs(2));
    assert_eq!(session.state(), State::Poisoned);
}
