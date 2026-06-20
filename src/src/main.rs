#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

fn main() {
    if let Err(error) = j3launcher::app::run() {
        eprintln!("{}", error.user_message());
        std::process::exit(1);
    }
}
