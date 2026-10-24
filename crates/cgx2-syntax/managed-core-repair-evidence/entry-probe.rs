fn main() {
    let source = std::fs::read_to_string(std::env::args().nth(1).unwrap()).unwrap();
    match cgx2_cli::run_checked_source_with_failure_at_reserve(&source, &[], 1) {
        Ok(_) => panic!("expected Session-entry failure"),
        Err(errors) => {
            assert_eq!(errors.len(), 1);
            let error = &errors[0].error;
            let span = error.span.expect("original source span");
            println!("{}:{}..{}", error.code, span.start, span.end);
        }
    }
}
