//! Uses existing source-reachable fault hook. No new implementation hook.
extern crate cgx2_cli;
fn main() {
    let args:Vec<String>=std::env::args().collect();
    let source=std::fs::read_to_string(&args[1]).unwrap();
    let index:usize=args[2].parse().unwrap();
    match cgx2_cli::run_checked_source_with_failure_at_reserve(&source,&[],index) {
        Ok((result,_))=>println!("{}",result.value),
        Err(errors)=>{
            for f in errors {
                eprintln!("PRIMARY {} {:?} {:?}",f.error.code,f.error.span,f.error.message);
                eprintln!("CLEANUP {:?}",f.cleanup_errors);
            }
            std::process::exit(1)
        }
    }
}
