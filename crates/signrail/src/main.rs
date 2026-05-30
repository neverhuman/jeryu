//! SignRail command-line entry point.

fn main() {
    match signrail::cli::run_env() {
        Ok(output) => println!("{output}"),
        Err(err) => {
            eprintln!("signrail: {err}");
            std::process::exit(1);
        }
    }
}
