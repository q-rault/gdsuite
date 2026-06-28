mod app;
mod cli;
mod config;
mod migrate;
mod output;
mod process;
mod tooling;

use std::process::ExitCode;

fn main() -> ExitCode {
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
