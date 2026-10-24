// Evidence adapter for the existing feature-gated CLI API; no production hook.
fn main() {
    let args: Vec<String> = std::env::args().collect();
    let source = std::fs::read_to_string(&args[1]).unwrap();
    let index = args[2].parse().unwrap();
    match cgx2_cli::run_checked_source_with_failure_at_reserve(&source, &[], index) {
        Ok((value, _)) => println!("{}", value.value),
        Err(failures) => {
            for failure in failures {
                let error = failure.error;
                if let Some(span) = error.span {
                    eprintln!(
                        "{}: {} at {}..{}",
                        error.code, error.message, span.start, span.end
                    );
                } else {
                    eprintln!("{}: {}", error.code, error.message);
                }
            }
            std::process::exit(1);
        }
    }
}
