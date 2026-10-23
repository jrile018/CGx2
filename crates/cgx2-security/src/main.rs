use std::env;
use std::fs::File;
use std::io::{Read, Write};
use std::process::ExitCode;

use cgx2_effect_index::MAX_SOURCE_BYTES;
use cgx2_security::{audit, parse_policy, MAX_POLICY_BYTES};

fn main() -> ExitCode {
    match run() {
        Ok((output, has_findings)) => {
            print!("{output}");
            if has_findings {
                ExitCode::from(1)
            } else {
                ExitCode::SUCCESS
            }
        }
        Err(message) => {
            let _ = writeln!(std::io::stderr(), "{message}");
            ExitCode::from(2)
        }
    }
}

fn run() -> Result<(String, bool), String> {
    let mut args = env::args().skip(1);
    if args.next().as_deref() != Some("audit") {
        return Err(usage());
    }
    let mut file = None;
    let mut policy_file = None;
    let mut format = "text";
    let mut package = false;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--package" => package = true,
            "--policy" if policy_file.is_none() => {
                policy_file = Some(args.next().ok_or_else(usage)?)
            }
            "--format" => {
                format = match args.next().as_deref() {
                    Some("text") => "text",
                    Some("json") => "json",
                    Some("sarif") => "sarif",
                    _ => return Err(usage()),
                };
            }
            _ if arg.starts_with('-') || file.is_some() => return Err(usage()),
            _ => file = Some(arg),
        }
    }
    let file = file.ok_or_else(usage)?;
    let policy_file = policy_file.ok_or_else(usage)?;
    let policy_text = bounded_utf8(&policy_file, MAX_POLICY_BYTES)?;
    if package {
        use cgx2_security::package_v1::{audit_package, parse_package_policy, AnalysisLimits};
        let policy = parse_package_policy(&policy_text)
            .map_err(|e| format!("CGSEC-POLICY {policy_file}: {e}"))?;
        let report = audit_package(
            std::path::Path::new(&file),
            &policy,
            AnalysisLimits::default(),
        )?;
        let output = match format {
            "json" => format!("{}\n", report.to_json()?),
            "sarif" => format!("{}\n", report.to_sarif()?),
            _ => report.to_text()?,
        };
        return Ok((output, !report.findings().is_empty()));
    }
    let policy =
        parse_policy(&policy_text).map_err(|e| format!("CGSEC-POLICY {policy_file}: {e}"))?;
    let source = bounded_utf8(&file, MAX_SOURCE_BYTES)?;
    let report = audit(&source, &policy).map_err(|e| format!("CGSEC-INPUT {file}: {e}"))?;
    let output = match format {
        "json" => format!("{}\n", report.to_json()),
        "sarif" => format!("{}\n", report.to_sarif(&file, &source)),
        _ => report.to_text(&file, &source),
    };
    Ok((output, !report.findings.is_empty()))
}

fn bounded_utf8(path: &str, limit: usize) -> Result<String, String> {
    let mut bytes = Vec::new();
    File::open(path)
        .map_err(|e| format!("{path}: {e}"))?
        .take(limit as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| format!("{path}: {e}"))?;
    if bytes.len() > limit {
        return Err(format!("{path}: exceeds {limit} byte limit"));
    }
    String::from_utf8(bytes).map_err(|e| format!("{path}: invalid UTF-8: {e}"))
}

fn usage() -> String {
    "usage: cgx2-security audit FILE.cg --policy POLICY.json [--format text|json|sarif], or audit --package ROOT --policy PACKAGE_POLICY.json [--format text|json|sarif]".into()
}
