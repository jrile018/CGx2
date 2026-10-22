use std::path::Path;
use std::process::ExitCode;

use cgx2_bind::check_package;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (package, format) = match args.as_slice() {
        [check, package] if check == "check" => (package, "text"),
        [check, package, flag, format]
            if check == "check"
                && flag == "--format"
                && matches!(format.as_str(), "text" | "json") =>
        {
            (package, format.as_str())
        }
        _ => {
            eprintln!("usage: cgx2-bind check PACKAGE [--format text|json]");
            return ExitCode::from(2);
        }
    };
    let report = check_package(Path::new(package));
    if format == "json" {
        println!("{}", report.render_json());
    } else {
        print!("{}", report.render_text());
    }
    if report.passed() {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}
