use std::io::{Read, Write};
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use crate::UnknownReason;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SolverExit {
    Code(i32),
    Timeout,
    Unavailable,
}

/// `SatCandidate` is never a disproof until the model is parsed and reevaluated.
/// `Unsat` is only a protocol result; the caller must bind it to the emitted
/// query, solver identity, and disclosed assumptions before claiming proof.
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
    if statuses.len() != 3
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
    match statuses[2] {
        "unsat" => SolverResponse::Unsat,
        "sat" => SolverResponse::SatCandidate,
        _ => SolverResponse::Unknown(UnknownReason::MalformedResponse),
    }
}

pub fn run_solver(
    executable: &str,
    args: &[&str],
    script: &str,
    timeout: Duration,
) -> SolverResponse {
    let mut child = match Command::new(executable)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
    {
        Ok(child) => child,
        Err(_) => return SolverResponse::Unknown(UnknownReason::SolverUnavailable),
    };
    let mut stdin = child.stdin.take().expect("piped stdin");
    let input = script.as_bytes().to_vec();
    let writer = thread::spawn(move || stdin.write_all(&input));
    let mut stdout = child.stdout.take().expect("piped stdout");
    let mut stderr = child.stderr.take().expect("piped stderr");
    let out_reader = thread::spawn(move || {
        let mut bytes = Vec::new();
        stdout.read_to_end(&mut bytes).map(|_| bytes)
    });
    let err_reader = thread::spawn(move || {
        let mut bytes = Vec::new();
        stderr.read_to_end(&mut bytes).map(|_| bytes)
    });
    let start = Instant::now();
    let exit = loop {
        match child.try_wait() {
            Ok(Some(status)) => break SolverExit::Code(status.code().unwrap_or(-1)),
            Ok(None) if start.elapsed() < timeout => thread::sleep(Duration::from_millis(2)),
            Ok(None) => {
                let _ = child.kill();
                let _ = child.wait();
                break SolverExit::Timeout;
            }
            Err(_) => {
                let _ = child.kill();
                let _ = child.wait();
                break SolverExit::Unavailable;
            }
        }
    };
    let write_result = writer.join();
    let out_result = out_reader.join();
    let err_result = err_reader.join();
    if exit != SolverExit::Code(0) {
        return parse_solver_result("", "", exit);
    }
    if !matches!(write_result, Ok(Ok(()))) {
        return SolverResponse::Unknown(UnknownReason::MalformedResponse);
    }
    let (Ok(Ok(out)), Ok(Ok(err))) = (out_result, err_result) else {
        return SolverResponse::Unknown(UnknownReason::MalformedResponse);
    };
    let (Ok(out), Ok(err)) = (String::from_utf8(out), String::from_utf8(err)) else {
        return SolverResponse::Unknown(UnknownReason::MalformedResponse);
    };
    parse_solver_result(&out, &err, exit)
}
