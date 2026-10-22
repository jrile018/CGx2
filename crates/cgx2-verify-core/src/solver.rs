use std::io::{Read, Write};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use crate::UnknownReason;

/// Maximum retained bytes from each child output stream.
pub const SOLVER_OUTPUT_LIMIT: usize = 16 * 1024;
/// Address-space limit for each solver subprocess on Linux.
pub const SOLVER_MEMORY_LIMIT_BYTES: u64 = 512 * 1024 * 1024;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SolverExit {
    Code(i32),
    Timeout,
    Unavailable,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SolverObservation {
    pub exit: SolverExit,
    pub stdout: String,
    pub stderr: String,
    pub output_limit_exceeded: bool,
    pub write_error: bool,
}

/// `SatCandidate` is never a disproof until the model is parsed and reevaluated.
/// `Unsat` is only a protocol result, never a trusted proof.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SolverResponse {
    Unsat,
    SatCandidate,
    Unknown(UnknownReason),
}

pub fn parse_solver_result(stdout: &str, stderr: &str, exit_status: SolverExit) -> SolverResponse {
    match exit_status {
        SolverExit::Timeout => return SolverResponse::Unknown(UnknownReason::Timeout),
        SolverExit::Unavailable => {
            return SolverResponse::Unknown(UnknownReason::SolverUnavailable)
        }
        SolverExit::Code(0) => (),
        SolverExit::Code(_) => return SolverResponse::Unknown(UnknownReason::MalformedResponse),
    }
    if !stderr.trim().is_empty() {
        return SolverResponse::Unknown(UnknownReason::MalformedResponse);
    }
    let statuses = stdout
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>();
    if statuses.len() != 4
        || statuses
            .iter()
            .any(|status| !matches!(*status, "sat" | "unsat" | "unknown"))
    {
        return SolverResponse::Unknown(UnknownReason::MalformedResponse);
    }
    if statuses.contains(&"unknown") {
        return SolverResponse::Unknown(UnknownReason::SolverUnknown);
    }
    if statuses[0] == "sat" {
        return SolverResponse::Unknown(UnknownReason::Evaluation(
            "requires may overflow or produce an out-of-range value".into(),
        ));
    }
    if statuses[1] == "unsat" {
        return SolverResponse::Unknown(UnknownReason::InconsistentPrecondition);
    }
    if statuses[2] == "sat" || statuses[3] == "sat" {
        SolverResponse::SatCandidate
    } else {
        SolverResponse::Unsat
    }
}

enum Event {
    Stdout(Vec<u8>, bool),
    Stderr(Vec<u8>, bool),
    Written(bool),
}

fn read_limited(mut pipe: impl Read, limit: usize) -> (Vec<u8>, bool) {
    let mut data = Vec::new();
    let mut chunk = [0_u8; 4096];
    loop {
        match pipe.read(&mut chunk) {
            Ok(0) => return (data, false),
            Ok(n) if data.len() + n <= limit => data.extend_from_slice(&chunk[..n]),
            Ok(_) => {
                data.extend_from_slice(&chunk[..limit - data.len()]);
                return (data, true);
            }
            Err(_) => return (data, true),
        }
    }
}

#[cfg(unix)]
fn kill_group(pid: u32) {
    unsafe extern "C" {
        fn kill(pid: i32, signal: i32) -> i32;
    }
    // The child starts in its own process group. Kill descendants holding pipes open.
    if let Ok(pid) = i32::try_from(pid) {
        unsafe { kill(-pid, 9) };
    }
}

/// Capture bounded protocol bytes. The deadline includes stdin, process exit and
/// pipe closure. Reader threads are detached on timeout, so a child cannot hold
/// the caller indefinitely even if it deliberately keeps a pipe open.
pub fn observe_solver(
    executable: &str,
    args: &[&str],
    script: &str,
    timeout: Duration,
) -> SolverObservation {
    let empty = |exit| SolverObservation {
        exit,
        stdout: String::new(),
        stderr: String::new(),
        output_limit_exceeded: false,
        write_error: false,
    };
    let mut command = Command::new(executable);
    command
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
        #[cfg(target_os = "linux")]
        unsafe {
            command.pre_exec(|| {
                #[repr(C)]
                struct Rlimit {
                    current: u64,
                    maximum: u64,
                }
                unsafe extern "C" {
                    fn setrlimit(resource: i32, limits: *const Rlimit) -> i32;
                }
                const RLIMIT_AS: i32 = 9;
                let limits = Rlimit {
                    current: SOLVER_MEMORY_LIMIT_BYTES,
                    maximum: SOLVER_MEMORY_LIMIT_BYTES,
                };
                if setrlimit(RLIMIT_AS, &limits) == 0 {
                    Ok(())
                } else {
                    Err(std::io::Error::last_os_error())
                }
            });
        }
    }
    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(_) => return empty(SolverExit::Unavailable),
    };
    let (tx, rx) = mpsc::channel();
    let mut stdin = child.stdin.take().expect("piped stdin");
    let input = script.as_bytes().to_vec();
    let writer_tx = tx.clone();
    thread::spawn(move || {
        let _ = writer_tx.send(Event::Written(stdin.write_all(&input).is_err()));
    });
    let stdout = child.stdout.take().expect("piped stdout");
    let out_tx = tx.clone();
    thread::spawn(move || {
        let (data, overflow) = read_limited(stdout, SOLVER_OUTPUT_LIMIT);
        let _ = out_tx.send(Event::Stdout(data, overflow));
    });
    let stderr = child.stderr.take().expect("piped stderr");
    thread::spawn(move || {
        let (data, overflow) = read_limited(stderr, SOLVER_OUTPUT_LIMIT);
        let _ = tx.send(Event::Stderr(data, overflow));
    });
    let deadline = Instant::now() + timeout;
    let mut out = None;
    let mut err = None;
    let mut written = None;
    let mut exit = None;
    let mut overflow = false;
    loop {
        if let Ok(Some(status)) = child.try_wait() {
            exit = Some(SolverExit::Code(status.code().unwrap_or(-1)));
        }
        if overflow || Instant::now() >= deadline {
            #[cfg(unix)]
            kill_group(child.id());
            let _ = child.kill();
            let _ = child.wait();
            exit = Some(SolverExit::Timeout);
            // Killing closes ordinary child pipes. Collect any already emitted
            // stderr/status bytes, but never wait unboundedly for escaped heirs.
            let grace = Instant::now() + Duration::from_millis(10);
            while Instant::now() < grace && (out.is_none() || err.is_none() || written.is_none()) {
                let wait = grace.saturating_duration_since(Instant::now());
                match rx.recv_timeout(wait) {
                    Ok(Event::Stdout(data, exceeded)) => {
                        out = Some(data);
                        overflow |= exceeded;
                    }
                    Ok(Event::Stderr(data, exceeded)) => {
                        err = Some(data);
                        overflow |= exceeded;
                    }
                    Ok(Event::Written(error)) => written = Some(error),
                    Err(_) => break,
                }
            }
            break;
        }
        if exit.is_some() && out.is_some() && err.is_some() && written.is_some() {
            break;
        }
        let wait = deadline.saturating_duration_since(Instant::now());
        if let Ok(event) = rx.recv_timeout(wait.min(Duration::from_millis(2))) {
            match event {
                Event::Stdout(data, exceeded) => {
                    out = Some(data);
                    overflow |= exceeded;
                }
                Event::Stderr(data, exceeded) => {
                    err = Some(data);
                    overflow |= exceeded;
                }
                Event::Written(error) => written = Some(error),
            }
        }
    }
    SolverObservation {
        exit: exit.unwrap_or(SolverExit::Timeout),
        stdout: String::from_utf8_lossy(&out.unwrap_or_default()).into_owned(),
        stderr: String::from_utf8_lossy(&err.unwrap_or_default()).into_owned(),
        output_limit_exceeded: overflow,
        write_error: written.unwrap_or(false),
    }
}

pub fn run_solver(
    executable: &str,
    args: &[&str],
    script: &str,
    timeout: Duration,
) -> SolverResponse {
    let observation = observe_solver(executable, args, script, timeout);
    if observation.output_limit_exceeded || observation.write_error {
        return SolverResponse::Unknown(UnknownReason::MalformedResponse);
    }
    parse_solver_result(&observation.stdout, &observation.stderr, observation.exit)
}
