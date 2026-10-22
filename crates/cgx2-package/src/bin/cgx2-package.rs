use std::path::{Path, PathBuf};

use cgx2_package::{load_package, PackageDiagnostic};

fn main() {
    let mut args = std::env::args().skip(1);
    let mut root = None;
    let mut format = "text".to_owned();
    if args.next().as_deref() != Some("check") {
        usage();
    }
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--format" => format = args.next().unwrap_or_else(|| usage()),
            value if value.starts_with('-') || root.is_some() => usage(),
            value => root = Some(PathBuf::from(value)),
        }
    }
    if !matches!(format.as_str(), "text" | "json") {
        usage();
    }
    let root = root.unwrap_or_else(|| usage());
    match load_package(&root) {
        Ok(index) => {
            if format == "json" {
                println!("{}", index.render_json());
            } else {
                print!("{}", index.render_text());
            }
        }
        Err(diagnostics) => {
            for diagnostic in diagnostics {
                if format == "json" {
                    println!("{}", render_diagnostic(&diagnostic, &root));
                } else if let Some(span) = diagnostic.span {
                    eprintln!(
                        "{}:{}-{}: {}: {}",
                        diagnostic.file.display(),
                        span.start,
                        span.end,
                        diagnostic.code,
                        diagnostic.message
                    );
                } else {
                    eprintln!(
                        "{}: {}: {}",
                        diagnostic.file.display(),
                        diagnostic.code,
                        diagnostic.message
                    );
                }
                if format != "json" {
                    if let Some(related) = &diagnostic.related {
                        eprintln!(
                            "  declared at {}:{}-{}",
                            related.file.display(),
                            related.span.start,
                            related.span.end
                        );
                    }
                }
            }
            std::process::exit(1);
        }
    }
}

fn usage() -> ! {
    eprintln!("usage: cgx2-package check ROOT [--format text|json]");
    std::process::exit(2);
}

fn render_diagnostic(d: &PackageDiagnostic, root: &Path) -> String {
    let file = d.file.strip_prefix(root).unwrap_or(&d.file);
    let mut out = format!("{{\"code\":\"{}\",\"file\":", d.code);
    push_json_string(&mut out, &file.to_string_lossy());
    out.push_str(",\"message\":");
    push_json_string(&mut out, &d.message);
    if let Some(span) = d.span {
        out.push_str(&format!(
            ",\"span\":{{\"start\":{},\"end\":{}}}",
            span.start, span.end
        ));
    } else {
        out.push_str(",\"span\":null");
    }
    if let Some(related) = &d.related {
        out.push_str(",\"related\":{\"file\":");
        let file = related.file.strip_prefix(root).unwrap_or(&related.file);
        push_json_string(&mut out, &file.to_string_lossy());
        out.push_str(&format!(
            ",\"span\":{{\"start\":{},\"end\":{}}}}}",
            related.span.start, related.span.end
        ));
    }
    out.push('}');
    out
}

fn push_json_string(out: &mut String, value: &str) {
    out.push('"');
    for ch in value.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            ch if ch <= '\u{001f}' => out.push_str(&format!("\\u{:04x}", ch as u32)),
            ch => out.push(ch),
        }
    }
    out.push('"');
}
