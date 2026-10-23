use std::{env, process::ExitCode};

fn main() -> ExitCode {
    match cgx2c::run(env::args_os().skip(1).collect()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}
