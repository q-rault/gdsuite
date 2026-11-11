mod app;
mod bundled;
mod cli;
mod config;
mod migrate;
mod output;
mod process;
mod tooling;
mod update;

use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<_> = std::env::args_os().collect();
    if let Some(code) = bundled::dispatch(&args) {
        return code;
    }

    match app::run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            if let Some(command_error) = error.downcast_ref::<app::CommandError>() {
                eprintln!("error: {}", command_error.message);
                return ExitCode::from(command_error.code);
            }

            eprintln!("error: {error:?}");
            ExitCode::from(app::EXIT_INTERNAL_ERROR)
        }
    }
}
