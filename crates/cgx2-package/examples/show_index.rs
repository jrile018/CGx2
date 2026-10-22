use std::path::Path;

fn main() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/demo");
    match cgx2_package::load_package(&root) {
        Ok(index) => println!("{}", index.render_json()),
        Err(diagnostics) => {
            for diagnostic in diagnostics {
                eprintln!(
                    "{}: {}: {}",
                    diagnostic.file.display(),
                    diagnostic.code,
                    diagnostic.message
                );
            }
            std::process::exit(1);
        }
    }
}
