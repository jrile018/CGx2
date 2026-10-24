fn main() {
 let ast = cgx2_syntax::parse_module("fn main() -> i64:\n    42\n").unwrap();
 let p = cgx2_sema::default_managed::compile_module(&ast, "this is not CGx2 source").unwrap();
 println!("authenticated={} source_sha256={} execution={:?}",p.authenticate().is_ok(),p.source_sha256(),p.execute(&[]));
}
