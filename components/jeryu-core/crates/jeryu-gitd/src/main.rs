fn main() {
    match jeryu_gitd::cli::run() {
        Ok(code) => std::process::exit(code),
        Err(err) => {
            eprintln!("jeryu_gitd: {err}");
            std::process::exit(1);
        }
    }
}
