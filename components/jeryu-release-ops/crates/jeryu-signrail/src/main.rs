//! SignRail command-line entry point.

fn main() {
    match jeryu_signrail::cli::run_env() {
        Ok(output) => println!("{output}"),
        Err(err) => {
            eprintln!("jeryu_signrail: {err}");
            std::process::exit(1);
        }
    }
}
