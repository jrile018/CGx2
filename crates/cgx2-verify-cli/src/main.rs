use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::fs::File;
use std::io::Read;
use std::time::Duration;

use cgx2_verify_core::{
    emit_smtlib, observe_solver, parse_solver_result, verify_bounded, Contract, Domain, Expr,
    Parameter, SolverExit, SolverObservation, SolverResponse, Sort, Value, Verdict,
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
    solver: Option<String>,
    timeout_ms: u64,
}

fn options() -> Result<Options, String> {
    let mut args = env::args().skip(1);
    let mut input = None;
    let mut format = "text".to_owned();
    let mut max_cases = DEFAULT_CASES;
    let mut solver = None;
    let mut timeout_ms = DEFAULT_TIMEOUT_MS;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--format" => format = args.next().ok_or("--format requires text or json")?,
            "--max-cases" => {
                max_cases = args
                    .next()
                    .ok_or("--max-cases requires a value")?
                    .parse()
                    .map_err(|_| "--max-cases must be a positive integer")?;
            }
            "--solver" => solver = Some(args.next().ok_or("--solver requires a path")?),
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
    let input = input.ok_or("usage: cgx2-verify CONTRACT.json [--format text|json] [--max-cases N] [--solver PATH] [--timeout-ms N]")?;
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
    Ok(Options {
        input,
        format,
        max_cases,
        solver,
        timeout_ms,
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
        "add" => {
            let (a, b) = binary(count)?;
            Expr::Add(a, b)
        }
        "sub" => {
            let (a, b) = binary(count)?;
            Expr::Sub(a, b)
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
        "arithmetic": "checked_i64_add_sub",
    });
    match verdict {
        Verdict::Proved { scope: proved } => json!({
            "kind": "proved", "meaning": "finite_domain_only",
            "scope": scope, "checked_cases": proved.cases,
            "assumptions": proved.assumptions,
        }),
        Verdict::Disproved { counterexample } => {
            let values: BTreeMap<_, _> = counterexample
                .values
                .iter()
                .map(|(k, v)| (k, value_json(v)))
                .collect();
            json!({"kind": "disproved", "scope": scope, "counterexample": values,
                "failure": format!("{:?}", counterexample.failure)})
        }
        Verdict::Unknown { reason } => {
            json!({"kind": "unknown", "scope": scope, "reason": format!("{reason:?}")})
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

fn solver_json(path: &str, script: &str, timeout_ms: u64) -> Json {
    let timeout = Duration::from_millis(timeout_ms);
    let version = observe_solver(path, &["--version"], "", timeout);
    let observation = observe_solver(path, &[], script, timeout);
    let statuses = observation
        .stdout
        .lines()
        .map(str::trim)
        .filter(|line| matches!(*line, "sat" | "unsat" | "unknown"))
        .collect::<Vec<_>>();
    json!({
        "trust": "untrusted_protocol",
        "executable": path,
        "version": if version.exit == SolverExit::Code(0) && !version.output_limit_exceeded {
            version.stdout.trim().to_owned()
        } else { "unavailable".to_owned() },
        "version_exit": exit_json(version.exit),
        "script_sha256": hash(script.as_bytes()),
        "assumptions": ["mathematical Int constrained to signed i64; checked add/sub have range guards", "uninterpreted calls are pure total functions with bounded i64 results"],
        "statuses": statuses,
        "stdout": observation.stdout,
        "stderr": observation.stderr,
        "exit": exit_json(observation.exit),
        "timeout_ms": timeout_ms,
        "output_limit_exceeded": observation.output_limit_exceeded,
        "write_error": observation.write_error,
        "response": response_json(&observation),
    })
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
    let (contract, domains) = parse_document(&bytes)?;
    let verdict = verify_bounded(&contract, &domains, options.max_cases);
    let mut result = json!({
        "schema_version": 1,
        "input_sha256": hash(&bytes),
        "verdict": verdict_json(&verdict, &domains, &contract, options.max_cases),
    });
    if let Some(path) = &options.solver {
        let script = emit_smtlib(&contract).map_err(|e| format!("SMT emission failed: {e:?}"))?;
        result["solver"] = solver_json(path, &script, options.timeout_ms);
    }
    if options.format == "json" {
        println!(
            "{}",
            serde_json::to_string(&result).expect("serializable result")
        );
    } else {
        println!("input_sha256: {}", result["input_sha256"].as_str().unwrap());
        println!("verdict: {}", result["verdict"]["kind"].as_str().unwrap());
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
