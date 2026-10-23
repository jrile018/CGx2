use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::fs::{self, File};
use std::io::Read;
use std::path::Path;
use std::time::Duration;

mod source;

use cgx2_verify_core::scalar_v2::{
    audit_encoding, audit_encoding_with_source, emit_smtlib, emit_witness_smtlib, observe_solver,
    parse_solver_result, validate_solver_witness, verify_bounded, Contract, Counterexample, Domain,
    Expr, Failure, Parameter, SolverExit, SolverObservation, SolverResponse, Sort, Value, Verdict,
};
use serde_json::{json, Map, Value as Json};
use sha2::{Digest, Sha256};

const INPUT_LIMIT: u64 = 64 * 1024;
const NODE_LIMIT: usize = 1024;
const DEPTH_LIMIT: usize = 64;
const DEFAULT_CASES: usize = 100_000;
const CASES_CAP: usize = 1_000_000;
const DEFAULT_TIMEOUT_MS: u64 = 2_000;
const TIMEOUT_CAP_MS: u64 = 10_000;

struct Options {
    input: String,
    format: String,
    max_cases: usize,
    max_expansion: usize,
    solver: Option<String>,
    trusted_solver_sha256: Option<String>,
    timeout_ms: u64,
    function: Option<String>,
    domains: Vec<Domain>,
}

fn parse_domain(text: &str) -> Result<Domain, String> {
    let parts: Vec<_> = text.split(':').collect();
    if parts.len() != 3 || parts[0].is_empty() {
        return Err("--domain requires NAME:MIN:MAX".into());
    }
    let min = parts[1]
        .parse::<i64>()
        .map_err(|_| "--domain MIN must be i64")?;
    let max = parts[2]
        .parse::<i64>()
        .map_err(|_| "--domain MAX must be i64")?;
    Ok(Domain::new(parts[0], min, max))
}

fn options() -> Result<Options, String> {
    let mut args = env::args().skip(1);
    let mut input = None;
    let mut format = "text".to_owned();
    let mut max_cases = DEFAULT_CASES;
    let mut max_expansion = 0;
    let mut solver = None;
    let mut trusted_solver_sha256 = None;
    let mut timeout_ms = DEFAULT_TIMEOUT_MS;
    let mut function = None;
    let mut domains = Vec::new();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--format" => format = args.next().ok_or("--format requires text or json")?,
            "--max-expansion" => {
                max_expansion = args
                    .next()
                    .ok_or("--max-expansion requires 1..=64")?
                    .parse::<usize>()
                    .map_err(|_| "--max-expansion requires 1..=64")?;
                if !(1..=64).contains(&max_expansion) {
                    return Err("--max-expansion requires 1..=64".into());
                }
            }
            "--max-cases" => {
                max_cases = args
                    .next()
                    .ok_or("--max-cases requires a value")?
                    .parse()
                    .map_err(|_| "--max-cases must be a positive integer")?;
            }
            "--solver" => solver = Some(args.next().ok_or("--solver requires a path")?),
            "--trusted-solver-sha256" => {
                trusted_solver_sha256 = Some(
                    args.next()
                        .ok_or("--trusted-solver-sha256 requires a digest")?
                        .to_ascii_lowercase(),
                );
            }
            "--function" => function = Some(args.next().ok_or("--function requires a name")?),
            "--domain" => domains.push(parse_domain(
                &args.next().ok_or("--domain requires NAME:MIN:MAX")?,
            )?),
            "--timeout-ms" => {
                timeout_ms = args
                    .next()
                    .ok_or("--timeout-ms requires a value")?
                    .parse()
                    .map_err(|_| "--timeout-ms must be a positive integer")?;
            }
            _ if arg.starts_with('-') => return Err(format!("unknown option: {arg}")),
            _ if input.is_none() => input = Some(arg),
            _ => return Err("expected one contract path".into()),
        }
    }
    let input = input.ok_or("usage: cgx2-verify CONTRACT.json|FILE.cg [--function NAME] [--domain NAME:MIN:MAX] [--format text|json] [--max-cases N] [--max-expansion 1..64] [--solver PATH] [--trusted-solver-sha256 HEX] [--timeout-ms N]")?;
    if format != "text" && format != "json" {
        return Err("--format must be text or json".into());
    }
    if max_cases == 0 || max_cases > CASES_CAP {
        return Err(format!("--max-cases must be 1..={CASES_CAP}"));
    }
    if timeout_ms == 0 || timeout_ms > TIMEOUT_CAP_MS {
        return Err(format!("--timeout-ms must be 1..={TIMEOUT_CAP_MS}"));
    }
    if solver.as_deref() == Some("") {
        return Err("--solver path must not be empty".into());
    }
    if let Some(digest) = &trusted_solver_sha256 {
        if solver.is_none()
            || digest.len() != 64
            || !digest.bytes().all(|byte| byte.is_ascii_hexdigit())
        {
            return Err(
                "--trusted-solver-sha256 requires --solver and a 64-digit SHA-256 digest".into(),
            );
        }
    }
    Ok(Options {
        input,
        format,
        max_cases,
        max_expansion,
        solver,
        trusted_solver_sha256,
        timeout_ms,
        function,
        domains,
    })
}

fn object<'a>(value: &'a Json, allowed: &[&str]) -> Result<&'a Map<String, Json>, String> {
    let map = value.as_object().ok_or("expected object")?;
    for key in map.keys() {
        if !allowed.contains(&key.as_str()) {
            return Err(format!("unsupported field: {key}"));
        }
    }
    Ok(map)
}

fn field<'a>(map: &'a Map<String, Json>, name: &str) -> Result<&'a Json, String> {
    map.get(name)
        .ok_or_else(|| format!("missing field: {name}"))
}

fn name(value: &Json) -> Result<String, String> {
    let name = value.as_str().ok_or("name must be a string")?;
    if name.is_empty() || name.len() > 128 {
        return Err("name length must be 1..=128 bytes".into());
    }
    Ok(name.to_owned())
}

fn sort(value: &Json) -> Result<Sort, String> {
    match value.as_str() {
        Some("bool") => Ok(Sort::Bool),
        Some("i64") => Ok(Sort::I64),
        _ => Err("sort must be bool or i64".into()),
    }
}

fn integer(value: &Json) -> Result<i64, String> {
    value
        .as_i64()
        .ok_or_else(|| "expected signed i64 integer".into())
}

fn expr(value: &Json, depth: usize, count: &mut usize) -> Result<Expr, String> {
    *count += 1;
    if depth > DEPTH_LIMIT || *count > NODE_LIMIT {
        return Err("expression depth or node limit exceeded".into());
    }
    let map = value.as_object().ok_or("expression must be an object")?;
    let op = field(map, "op")?.as_str().ok_or("op must be a string")?;
    let binary = |count: &mut usize| -> Result<(Box<Expr>, Box<Expr>), String> {
        object(value, &["op", "left", "right"])?;
        Ok((
            Box::new(expr(field(map, "left")?, depth + 1, count)?),
            Box::new(expr(field(map, "right")?, depth + 1, count)?),
        ))
    };
    Ok(match op {
        "bool" => {
            object(value, &["op", "value"])?;
            Expr::Bool(
                field(map, "value")?
                    .as_bool()
                    .ok_or("bool value required")?,
            )
        }
        "i64" => {
            object(value, &["op", "value"])?;
            Expr::I64(integer(field(map, "value")?)?)
        }
        "var" => {
            object(value, &["op", "name"])?;
            Expr::Var(name(field(map, "name")?)?)
        }
        "if" => {
            object(value, &["op", "condition", "then", "else"])?;
            Expr::If(
                Box::new(expr(field(map, "condition")?, depth + 1, count)?),
                Box::new(expr(field(map, "then")?, depth + 1, count)?),
                Box::new(expr(field(map, "else")?, depth + 1, count)?),
            )
        }
        "add" => {
            let (a, b) = binary(count)?;
            Expr::Add(a, b)
        }
        "sub" => {
            let (a, b) = binary(count)?;
            Expr::Sub(a, b)
        }
        "mul" => {
            let (a, b) = binary(count)?;
            Expr::Mul(a, b)
        }
        "div" => {
            let (a, b) = binary(count)?;
            Expr::Div(a, b)
        }
        "eq" => {
            let (a, b) = binary(count)?;
            Expr::Eq(a, b)
        }
        "lt" => {
            let (a, b) = binary(count)?;
            Expr::Lt(a, b)
        }
        "le" => {
            let (a, b) = binary(count)?;
            Expr::Le(a, b)
        }
        "gt" => {
            let (a, b) = binary(count)?;
            Expr::Gt(a, b)
        }
        "ge" => {
            let (a, b) = binary(count)?;
            Expr::Ge(a, b)
        }
        "and" => {
            let (a, b) = binary(count)?;
            Expr::And(a, b)
        }
        "or" => {
            let (a, b) = binary(count)?;
            Expr::Or(a, b)
        }
        "not" => {
            object(value, &["op", "arg"])?;
            Expr::Not(Box::new(expr(field(map, "arg")?, depth + 1, count)?))
        }
        "call" => {
            object(value, &["op", "name", "sort", "args"])?;
            let args = field(map, "args")?
                .as_array()
                .ok_or("args must be an array")?;
            let args = args
                .iter()
                .map(|arg| expr(arg, depth + 1, count))
                .collect::<Result<Vec<_>, _>>()?;
            Expr::Uninterpreted {
                name: name(field(map, "name")?)?,
                sort: sort(field(map, "sort")?)?,
                args,
            }
        }
        _ => return Err(format!("unsupported expression op: {op}")),
    })
}

fn parse_document(bytes: &[u8]) -> Result<(Contract, Vec<Domain>), String> {
    let root: Json = serde_json::from_slice(bytes).map_err(|e| format!("invalid JSON: {e}"))?;
    let map = object(
        &root,
        &["version", "parameters", "domains", "requires", "ensures"],
    )?;
    if field(map, "version")?.as_u64() != Some(1) {
        return Err("unsupported contract version; expected 1".into());
    }
    let params = field(map, "parameters")?
        .as_array()
        .ok_or("parameters must be an array")?;
    if params.len() > 32 {
        return Err("parameter limit exceeded (32)".into());
    }
    let mut parameters = Vec::new();
    let mut names = BTreeSet::new();
    for item in params {
        let entry = object(item, &["name", "sort"])?;
        let key = name(field(entry, "name")?)?;
        if !names.insert(key.clone()) {
            return Err(format!("duplicate parameter: {key}"));
        }
        parameters.push(Parameter::new(key, sort(field(entry, "sort")?)?));
    }
    let raw_domains = field(map, "domains")?
        .as_array()
        .ok_or("domains must be an array")?;
    let mut domains = Vec::new();
    let mut domain_names = BTreeSet::new();
    for item in raw_domains {
        let entry = object(item, &["name", "min", "max"])?;
        let key = name(field(entry, "name")?)?;
        if !domain_names.insert(key.clone()) {
            return Err(format!("duplicate domain: {key}"));
        }
        let min = integer(field(entry, "min")?)?;
        let max = integer(field(entry, "max")?)?;
        if min > max {
            return Err(format!("empty domain: {key}"));
        }
        domains.push(Domain::new(key, min, max));
    }
    for parameter in &parameters {
        match parameter.sort {
            Sort::I64 if !domain_names.contains(&parameter.name) => {
                return Err(format!("missing domain: {}", parameter.name))
            }
            Sort::Bool if domain_names.contains(&parameter.name) => {
                return Err(format!(
                    "Boolean domain must be implicit: {}",
                    parameter.name
                ))
            }
            _ => (),
        }
    }
    for domain in &domains {
        if !parameters.iter().any(|p| p.name == domain.name) {
            return Err(format!("extra domain: {}", domain.name));
        }
    }
    let mut count = 0;
    let contract = Contract {
        parameters,
        requires: expr(field(map, "requires")?, 0, &mut count)?,
        ensures: expr(field(map, "ensures")?, 0, &mut count)?,
    };
    contract
        .validate()
        .map_err(|e| format!("invalid contract: {e:?}"))?;
    Ok((contract, domains))
}

fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn value_json(value: &Value) -> Json {
    match value {
        Value::Bool(value) => json!(value),
        Value::I64(value) => json!(value),
    }
}
fn counterexample_json(counterexample: &Counterexample) -> Json {
    let values: BTreeMap<_, _> = counterexample
        .values
        .iter()
        .map(|(name, value)| (name, value_json(value)))
        .collect();
    json!({"counterexample": values, "failure": format!("{:?}", counterexample.failure)})
}

fn domain_json(domains: &[Domain]) -> Json {
    Json::Array(
        domains
            .iter()
            .map(|d| json!({"name": d.name, "min": d.min, "max": d.max}))
            .collect(),
    )
}

fn verdict_json(
    verdict: &Verdict,
    domains: &[Domain],
    contract: &Contract,
    max_cases: usize,
) -> Json {
    let bools = contract
        .parameters
        .iter()
        .filter(|p| p.sort == Sort::Bool)
        .map(|p| p.name.as_str())
        .collect::<Vec<_>>();
    let scope = json!({
        "domains": domain_json(domains),
        "boolean_parameters": bools,
        "max_cases": max_cases,
        "arithmetic": "checked_i64_add_sub_mul_div",
    });
    match verdict {
        Verdict::Proved { scope: proved } => json!({
            "kind": "proved", "meaning": "finite_domain_only",
            "scope": scope, "checked_cases": proved.cases,
            "assumptions": proved.assumptions,
        }),
        Verdict::Disproved { counterexample } => {
            let mut result = counterexample_json(counterexample);
            result["kind"] = json!("disproved");
            result["meaning"] = json!("finite_domain_validated_counterexample");
            result["scope"] = scope;
            result
        }
        Verdict::Unknown { reason } => {
            json!({"kind": "unknown", "meaning": "finite_enumeration_unknown", "scope": scope, "reason": format!("{reason:?}")})
        }
    }
}

fn exit_json(exit: SolverExit) -> Json {
    match exit {
        SolverExit::Code(code) => json!({"kind": "code", "code": code}),
        SolverExit::Timeout => json!({"kind": "timeout"}),
        SolverExit::Unavailable => json!({"kind": "unavailable"}),
    }
}

fn response_json(observation: &SolverObservation) -> Json {
    if observation.output_limit_exceeded || observation.write_error {
        return json!("malformed_response");
    }
    match parse_solver_result(&observation.stdout, &observation.stderr, observation.exit) {
        SolverResponse::Unsat => json!("unsat_protocol_only"),
        SolverResponse::SatCandidate => json!("sat_candidate_unvalidated"),
        SolverResponse::Unknown(reason) => json!(format!("unknown:{reason:?}")),
    }
}

/// Only these solver command lines have a defined stdin SMT-LIB protocol here.
/// A basename or version string is not an attestation of the executable.
fn solver_protocol(path: &str) -> (&'static str, &'static [&'static str]) {
    match Path::new(path).file_name().and_then(|name| name.to_str()) {
        Some("z3") => ("z3", &["-in", "-smt2"]),
        Some("cvc5") => (
            "cvc5",
            &["--lang=smt2", "--incremental", "--produce-models"],
        ),
        _ => ("unrecognized", &[]),
    }
}

fn executable_hash(path: &str) -> Option<String> {
    let path = fs::canonicalize(path).ok()?;
    let metadata = fs::metadata(&path).ok()?;
    if !metadata.is_file() || metadata.len() > 128 * 1024 * 1024 {
        return None;
    }
    let mut file = File::open(path).ok()?;
    let mut digest = Sha256::new();
    let mut buffer = [0_u8; 8192];
    loop {
        let count = file.read(&mut buffer).ok()?;
        if count == 0 {
            break;
        }
        digest.update(&buffer[..count]);
    }
    Some(format!("{:x}", digest.finalize()))
}

fn solver_fixtures() -> [(&'static str, Contract, &'static str); 3] {
    let x = Expr::Var("x".into());
    [
        (
            "true",
            Contract {
                parameters: vec![],
                requires: Expr::Bool(true),
                ensures: Expr::Bool(true),
            },
            "unsat\nsat\nunsat\nunsat",
        ),
        (
            "false",
            Contract {
                parameters: vec![],
                requires: Expr::Bool(true),
                ensures: Expr::Bool(false),
            },
            "unsat\nsat\nsat\nunsat",
        ),
        (
            "overflow",
            Contract {
                parameters: vec![Parameter::new("x", Sort::I64)],
                requires: Expr::Bool(true),
                ensures: Expr::Eq(
                    Box::new(Expr::Add(Box::new(x.clone()), Box::new(Expr::I64(1)))),
                    Box::new(Expr::Add(Box::new(x), Box::new(Expr::I64(1)))),
                ),
            },
            "unsat\nsat\nunsat\nsat",
        ),
    ]
}

fn solver_fixture_catalog() -> Json {
    Json::Array(
        solver_fixtures()
            .into_iter()
            .map(|(name, contract, expected)| {
                let script = match emit_smtlib(&contract) {
                    Ok(script) => script,
                    Err(error) => return json!({"name": name, "emission_error": format!("{error:?}"), "valid": false}),
                };
                json!({"name": name, "smtlib": script, "script_sha256": hash(script.as_bytes()),
            "expected": expected.split('\n').collect::<Vec<_>>()})
            })
            .collect(),
    )
}

fn solver_fixture_probes(executable: &str, args: &[&str], timeout: Duration) -> (bool, Vec<Json>) {
    let mut evidence = Vec::new();
    let mut passed = true;
    for (name, contract, expected) in solver_fixtures() {
        let script = match emit_smtlib(&contract) {
            Ok(script) => script,
            Err(error) => {
                passed = false;
                evidence.push(
                    json!({"name": name, "emission_error": format!("{error:?}"), "valid": false}),
                );
                continue;
            }
        };
        let observed = observe_solver(executable, args, &script, timeout);
        let valid = observed.exit == SolverExit::Code(0)
            && !observed.output_limit_exceeded
            && !observed.write_error
            && observed.stderr.is_empty()
            && (observed.stdout == expected || observed.stdout == format!("{expected}\n"));
        passed &= valid;
        evidence.push(
            json!({"name": name, "script_sha256": hash(script.as_bytes()),
            "expected": expected.split('\n').collect::<Vec<_>>(), "observed": observed.stdout,
            "exit": exit_json(observed.exit), "valid": valid}),
        );
    }
    (passed, evidence)
}

fn solver_json(
    path: &str,
    script: &str,
    contract: &Contract,
    timeout_ms: u64,
    selected: Option<&source::Selected>,
    trusted_solver_sha256: Option<&str>,
) -> Json {
    let timeout = Duration::from_millis(timeout_ms);
    let (family, args) = solver_protocol(path);
    let canonical = fs::canonicalize(path).ok();
    let executable = canonical.as_ref().and_then(|p| p.to_str()).unwrap_or(path);
    let executable_sha256 = executable_hash(executable);
    let pin_valid =
        trusted_solver_sha256.is_some_and(|pin| executable_sha256.as_deref() == Some(pin));
    let version = observe_solver(executable, &["--version"], "", timeout);
    let version_text = version.stdout.trim();
    let version_valid = version.exit == SolverExit::Code(0)
        && !version.output_limit_exceeded
        && !version.write_error
        && version.stderr.trim().is_empty()
        && version_text.is_ascii()
        && version_text.lines().count() == 1
        && version_text.len() <= 256
        && match family {
            "z3" => version_text.starts_with("Z3 version "),
            "cvc5" => {
                version_text.starts_with("This is cvc5 version ")
                    || version_text.starts_with("cvc5 version ")
            }
            _ => false,
        };
    let observation = if version_valid && executable_sha256.is_some() {
        observe_solver(executable, args, script, timeout)
    } else if family == "unrecognized" {
        // Preserve existing protocol fixture observations without granting solver identity.
        observe_solver(executable, &[], script, timeout)
    } else {
        observe_solver("/definitely/missing/cgx2-solver", &[], script, timeout)
    };
    let statuses = observation
        .stdout
        .lines()
        .map(str::trim)
        .filter(|line| matches!(*line, "sat" | "unsat" | "unknown"))
        .collect::<Vec<_>>();
    let executable_stable_before_probes =
        executable_sha256.is_some() && executable_sha256 == executable_hash(executable);
    let response = response_json(&observation);
    let (fixtures_valid, fixture_evidence) = if version_valid && executable_stable_before_probes {
        solver_fixture_probes(executable, args, timeout)
    } else {
        (false, Vec::new())
    };
    let executable_stable =
        executable_stable_before_probes && executable_sha256 == executable_hash(executable);
    let bounded_reason = if family == "unrecognized" {
        "unrecognized_solver"
    } else if executable_sha256.is_none() {
        "unreadable_executable"
    } else if !version_valid {
        "invalid_version_response"
    } else if !executable_stable {
        "executable_changed"
    } else if !fixtures_valid {
        "solver_fixture_failed"
    } else if trusted_solver_sha256.is_none() {
        "solver_digest_unpinned"
    } else if !pin_valid {
        "solver_digest_mismatch"
    } else if contract.requires.contains_uninterpreted()
        || contract.ensures.contains_uninterpreted()
    {
        "unsupported_uninterpreted_call"
    } else if response != "unsat_protocol_only" {
        "incomplete_solver_obligation"
    } else {
        "none"
    };
    let mut result = json!({
        "trust": "untrusted_protocol",
        "executable": path,
        "canonical_executable": executable,
        "family": family,
        "arguments": args,
        "executable_sha256": executable_sha256,
        "trusted_solver_sha256": trusted_solver_sha256,
        "pin_valid": pin_valid,
        "identity_valid": version_valid,
    "executable_stable": executable_stable,
    "fixtures_valid": fixtures_valid,
    "fixture_probes": fixture_evidence,
        "version": if version.exit == SolverExit::Code(0) && !version.output_limit_exceeded {
            version.stdout.trim().to_owned()
        } else { "unavailable".to_owned() },
        "version_exit": exit_json(version.exit),
        "script_sha256": hash(script.as_bytes()),
        "assumptions": ["mathematical Int constrained to signed i64; checked add/sub/mul/div have definedness guards; division truncates toward zero", "uninterpreted calls are pure total functions with bounded i64 results"],
        "statuses": statuses,
        "stdout": observation.stdout,
        "stderr": observation.stderr,
        "exit": exit_json(observation.exit),
        "timeout_ms": timeout_ms,
        "memory_limit_bytes": if cfg!(target_os = "linux") {
            Some(cgx2_verify_core::scalar_v2::SOLVER_MEMORY_LIMIT_BYTES)
        } else { None },
        "output_limit_exceeded": observation.output_limit_exceeded,
        "write_error": observation.write_error,
        "response": response,
        "bounded_reason": bounded_reason,
        "bounded_result": if bounded_reason == "none" {
                "unsat_observed"
            } else { "unknown" },
        "witness_attempts": [],
    });
    if response_json(&observation) == "sat_candidate_unvalidated"
        && family != "unrecognized"
        && version_valid
        && executable_stable
        && fixtures_valid
        && !contract.requires.contains_uninterpreted()
        && !contract.ensures.contains_uninterpreted()
    {
        for (index, failure) in [(2, Failure::EnsuresFalse), (3, Failure::Overflow)] {
            if statuses.get(index) != Some(&"sat") {
                continue;
            }
            let query = match emit_witness_smtlib(contract, failure) {
                Ok(query) => query,
                Err(error) => {
                    if let Some(attempts) = result["witness_attempts"].as_array_mut() {
                        attempts.push(json!({"condition": format!("{failure:?}"), "validation": format!("unknown:emission:{error:?}")}));
                    }
                    continue;
                }
            };
            let witness = observe_solver(executable, args, &query, timeout);
            let checked = if witness.output_limit_exceeded
                || witness.write_error
                || witness.exit != SolverExit::Code(0)
                || !witness.stderr.trim().is_empty()
            {
                Err(cgx2_verify_core::scalar_v2::UnknownReason::MalformedResponse)
            } else {
                validate_solver_witness(contract, &witness.stdout, failure).and_then(|candidate| {
                    if let Some(selected) = selected {
                        let source_valid = selected
                            .source_obligations(&candidate.values)
                            .is_ok_and(|obligations| obligations[index]);
                        if !source_valid {
                            return Err(cgx2_verify_core::scalar_v2::UnknownReason::Evaluation(
                                "witness fails source parity".into(),
                            ));
                        }
                    }
                    Ok(candidate)
                })
            };
            result["witness_attempts"]
                .as_array_mut()
                .expect("array")
                .push(json!({
                    "condition": if index == 2 { "ensures_false" } else { "ensures_overflow" },
                    "script_sha256": hash(query.as_bytes()),
                    "exit": exit_json(witness.exit),
                    "stdout": witness.stdout,
                    "stderr": witness.stderr,
                    "output_limit_exceeded": witness.output_limit_exceeded,
                    "write_error": witness.write_error,
                    "validation": match &checked { Ok(_) => "validated".to_owned(), Err(e) => format!("unknown:{e:?}") },
                }));
            if let Ok(counterexample) = checked {
                result["response"] = json!("validated_counterexample");
                result["validated_counterexample"] = counterexample_json(&counterexample);
                break;
            }
        }
    }
    if executable_sha256 != executable_hash(executable) {
        result["executable_stable"] = json!(false);
        result["bounded_reason"] = json!("executable_changed");
        result["bounded_result"] = json!("unknown");
    }
    result
}

fn source_unknown(options: &Options, bytes: &[u8], diagnostic: &source::Diagnostic) {
    let result = json!({
        "schema_version": 3,
        "input_sha256": hash(bytes),
        "source": {"path": options.input, "function": options.function,
            "span": {"start": diagnostic.span.start, "end": diagnostic.span.end}},
        "verdict": {"kind": "unknown", "meaning": "source_subset_unsupported",
            "scope": {"requested_domains": domain_json(&options.domains), "max_cases": options.max_cases},
            "diagnostic": diagnostic.json()},
    "unsupported_obligations": [diagnostic.json()],
    "bounded_solver_result": {"kind": "unrun", "reason": "source_subset_unsupported", "source_parity": false},
    "proof": {"kind": "unknown", "meaning": "no_trusted_solver_proof", "reason": "source_subset_unsupported"},
    });
    let mut result = result;
    result["proof"]["state"] = json!("unsupported");
    result["proof"]["scope"] = json!({
        "kind": "requested_source_function",
        "function": options.function,
        "input_sha256": hash(bytes),
        "unsupported_span": {"start": diagnostic.span.start, "end": diagnostic.span.end},
    });
    if options.format == "json" {
        println!("{}", result);
    } else {
        println!("input_sha256: {}", result["input_sha256"]);
        println!("verdict: unknown");
        println!("proof_state: unsupported");
        println!(
            "{}: {} at byte {}..{}",
            diagnostic.code, diagnostic.message, diagnostic.span.start, diagnostic.span.end
        );
    }
}

fn run() -> Result<(), String> {
    let options = options()?;
    let mut bytes = Vec::new();
    File::open(&options.input)
        .map_err(|e| format!("cannot open contract: {e}"))?
        .take(INPUT_LIMIT + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| format!("cannot read contract: {e}"))?;
    if bytes.len() as u64 > INPUT_LIMIT {
        return Err(format!("contract exceeds {INPUT_LIMIT} byte limit"));
    }
    let source_input = Path::new(&options.input)
        .extension()
        .is_some_and(|ext| ext == "cg");
    let (contract, domains, source_meta, source_selected) = if source_input {
        let text = std::str::from_utf8(&bytes).map_err(|_| "source must be UTF-8")?;
        match source::parse_with_expansion(
            text,
            options.function.as_deref(),
            options.domains.clone(),
            options.max_expansion,
        ) {
            Ok(selected) => (
                selected.contract.clone(),
                selected.domains.clone(),
                Some(json!({"path": options.input, "function": selected.name,
                "span": {"start": selected.span.start, "end": selected.span.end},
                "obligations": selected.obligations,
                "unsupported_obligations": []})),
                Some(selected),
            ),
            Err(diagnostic) => {
                source_unknown(&options, &bytes, &diagnostic);
                return Ok(());
            }
        }
    } else {
        if options.function.is_some() || !options.domains.is_empty() {
            return Err("--function and --domain require a .cg source file".into());
        }
        let (contract, domains) = parse_document(&bytes)?;
        (contract, domains, None, None)
    };
    let script = match emit_smtlib(&contract) {
        Ok(script) => script,
        Err(error) => {
            let result = json!({"schema_version": if source_input { 3 } else { 2 },
                "input_sha256": hash(&bytes), "source": source_meta,
                "verdict": {"kind": "unknown", "reason": format!("SMT emission failed: {error:?}")},
                "proof": {"kind": "unknown", "state": "unsupported", "reason": "smt_emission_failed"},
                "diagnostic": {"code": "CGV003", "message": format!("{error:?}")},
                "bounded_solver_result": {"kind": "unrun", "source_parity": false},
                "solver": {"bounded_reason": "emission_failed", "launched": false}});
            if options.format == "json" {
                println!("{result}");
            } else {
                println!("proof: unknown (SMT emission failed: {error:?})");
            }
            return Ok(());
        }
    };
    let verdict = verify_bounded(&contract, &domains, options.max_cases);
    let mut result = json!({
        "schema_version": if source_input { 3 } else { 2 },
        "input_sha256": hash(&bytes),
        "verdict": verdict_json(&verdict, &domains, &contract, options.max_cases),
    });
    if let Some(source_meta) = source_meta {
        result["source"] = source_meta;
        if let Verdict::Unknown { reason } = &verdict {
            result["verdict"]["diagnostic"] = json!({
                "code": "CGV002",
                "message": format!("finite verification unavailable: {reason:?}"),
                "span": result["source"]["span"].clone(),
            });
        }
    }
    let audit = if let Some(selected) = &source_selected {
        audit_encoding_with_source(&contract, &domains, options.max_cases, &script, &|values| {
            selected.source_obligations(values)
        })
    } else {
        audit_encoding(&contract, &domains, options.max_cases, &script)
    };
    result["bounded_solver_result"] = match &audit {
        Ok(checked) => {
            json!({"kind": "cross_checked", "evaluator": "independent_smt_term_interpreter", "checked_cases": checked.cases,
            "obligations": {"requires_defined": checked.obligations[0], "requires_satisfiable": checked.obligations[1],
                "ensures_false": checked.obligations[2], "ensures_overflow": checked.obligations[3]},
            "source_parity": source_selected.is_some()})
        }
        Err(error) => {
            let kind = if matches!(
                error,
                cgx2_verify_core::scalar_v2::AuditError::Mismatch { .. }
                    | cgx2_verify_core::scalar_v2::AuditError::SourceMismatch { .. }
            ) {
                "failed"
            } else {
                "unrun"
            };
            json!({"kind": kind, "reason": format!("{error:?}"), "source_parity": false})
        }
    };
    result["smtlib"] = json!(script);
    result["scalar_profile"] = json!({"version": 2, "immutable_locals": true, "checked_operations": ["add", "sub", "mul", "div"], "max_expansion": options.max_expansion, "expansion_cap": 64, "literal_accumulator": "library_only_shared_effect_admission_blocked"});
    result["solver_fixture_catalog"] = solver_fixture_catalog();
    if let Some(path) = &options.solver {
        result["solver"] = solver_json(
            path,
            &script,
            &contract,
            options.timeout_ms,
            source_selected.as_ref(),
            options.trusted_solver_sha256.as_deref(),
        );
    }
    let source_parity = source_selected.is_some() && audit.is_ok();
    let trusted_solver = source_parity
        && result["solver"]["bounded_reason"] == "none"
        && result["solver"]["statuses"] == json!(["unsat", "sat", "unsat", "unsat"])
        && result["solver"]["fixtures_valid"] == true
        && result["solver"]["identity_valid"] == true
        && result["solver"]["pin_valid"] == true
        && result["solver"]["executable_stable"] == true;
    result["proof"] = match &verdict {
        Verdict::Disproved { counterexample } if audit.is_ok() => {
            let mut proof = counterexample_json(counterexample);
            proof["kind"] = json!("disproved");
            proof["meaning"] = json!("finite_domain_cross_checked_counterexample");
            proof
        }
        _ if result["solver"]["response"] == "validated_counterexample" && audit.is_ok() => {
            let mut proof = result["solver"]["validated_counterexample"].clone();
            proof["kind"] = json!("disproved");
            proof["meaning"] = json!("solver_witness_validated_against_source");
            proof
        }
        _ if trusted_solver => {
            result["solver"]["trust"] = json!("recognized_live_solver_with_fixture_probes");
            json!({"kind": "proved", "meaning": "selected_pure_function_contract_under_solver_assumptions",
                "trust_conditions": ["four_complete_obligations", "source_and_smt_term_parity",
                    "recognized_solver_version_and_sha256", "true_false_overflow_fixture_probes",
                    "caller_trusts_solver_executable"]})
        }
        _ => json!({
            "kind": "unknown",
            "meaning": "no_trusted_solver_proof",
            "reason": if audit.is_err() { "encoding_or_source_audit_unrun_or_failed" }
                else if source_selected.is_none() { "source_parity_unrun" }
                else if options.solver.is_none() { "live_solver_unrun" }
                else { result["solver"]["bounded_reason"].as_str().unwrap_or("incomplete_solver_obligation") },
        }),
    };
    let proof_state = match result["proof"]["kind"].as_str() {
        Some("proved") => "proven",
        Some("disproved") => "counterexample",
        _ if matches!(
            &verdict,
            Verdict::Unknown {
                reason: cgx2_verify_core::scalar_v2::UnknownReason::Unsupported(_)
            }
        ) || result["solver"]["bounded_reason"] == "unsupported_uninterpreted_call" =>
        {
            "unsupported"
        }
        _ if result["solver"]["output_limit_exceeded"] != true
            && (result["solver"]["exit"]["kind"] == "timeout"
                || result["solver"]["version_exit"]["kind"] == "timeout"
                || result["solver"]["fixture_probes"]
                    .as_array()
                    .is_some_and(|probes| {
                        probes
                            .iter()
                            .any(|probe| probe["exit"]["kind"] == "timeout")
                    })
                || result["solver"]["witness_attempts"]
                    .as_array()
                    .is_some_and(|attempts| {
                        attempts
                            .iter()
                            .any(|attempt| attempt["exit"]["kind"] == "timeout")
                    })) =>
        {
            "unknown"
        }
        _ if options.solver.is_some()
            && (result["solver"]["exit"]["kind"] == "unavailable"
                || (result["solver"]["exit"]["kind"] == "code"
                    && result["solver"]["exit"]["code"] != 0)
                || result["solver"]["response"] == "unknown:MalformedResponse"
                || result["solver"]["output_limit_exceeded"] == true
                || result["solver"]["write_error"] == true
                || result["solver"]["bounded_reason"] == "invalid_version_response"
                || result["solver"]["bounded_reason"] == "unreadable_executable"
                || result["solver"]["bounded_reason"] == "executable_changed"
                || result["solver"]["bounded_reason"] == "solver_fixture_failed"
                || result["solver"]["bounded_reason"] == "solver_digest_mismatch"
                || result["solver"]["bounded_reason"] == "unrecognized_solver") =>
        {
            "solver_failure"
        }
        _ => "unknown",
    };
    result["proof"]["state"] = json!(proof_state);
    result["proof"]["scope"] = json!({
        "kind": if source_input { "selected_source_function_contract" } else { "json_contract" },
        "function": source_selected.as_ref().map(|selected| selected.name.as_str()),
        "source_span": result["source"]["span"],
        "input_sha256": result["input_sha256"],
        "obligation_sha256": hash(script.as_bytes()),
        "finite_domains": domain_json(&domains),
        "max_cases": options.max_cases,
    });
    if options.solver.is_some() {
        result["proof"]["solver_provenance"] = json!({
            "command": result["solver"]["canonical_executable"],
            "arguments": result["solver"]["arguments"],
            "version": result["solver"]["version"],
            "executable_sha256": result["solver"]["executable_sha256"],
            "trusted_solver_sha256": options.trusted_solver_sha256,
            "timeout_ms_per_process": options.timeout_ms,
            "output_limit_bytes_per_stream": cgx2_verify_core::scalar_v2::SOLVER_OUTPUT_LIMIT,
        });
    }
    if options.format == "json" {
        println!(
            "{}",
            serde_json::to_string(&result).expect("serializable result")
        );
    } else {
        println!("input_sha256: {}", result["input_sha256"].as_str().unwrap());
        println!("verdict: {}", result["verdict"]["kind"].as_str().unwrap());
        println!(
            "proof: {} ({})",
            result["proof"]["kind"].as_str().unwrap(),
            result["proof"]["meaning"].as_str().unwrap()
        );
        println!("proof_state: {}", result["proof"]["state"]);
        println!("proof_scope: {}", result["proof"]["scope"]);
        println!(
            "encoding_audit: {} (source_parity: {})",
            result["bounded_solver_result"]["kind"].as_str().unwrap(),
            result["bounded_solver_result"]["source_parity"]
        );
        println!("scope: {}", result["verdict"]["scope"]);
        match &verdict {
            Verdict::Proved { scope } => {
                println!("checked_cases: {} (finite domain only)", scope.cases)
            }
            Verdict::Disproved { .. } => {
                println!("counterexample: {}", result["verdict"]["counterexample"])
            }
            Verdict::Unknown { reason } => println!("reason: {reason:?}"),
        }
        if let Some(solver) = result.get("solver") {
            println!(
                "solver: {} ({})",
                solver["trust"].as_str().unwrap(),
                solver["response"]
            );
        }
    }
    Ok(())
}

fn main() {
    if let Err(error) = run() {
        eprintln!("cgx2-verify: {error}");
        std::process::exit(2);
    }
}
