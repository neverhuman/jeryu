fn main() {
    match gitd::cli::run() {
        Ok(code) => std::process::exit(code),
        Err(err) => {
            eprintln!("gitd: {err}");
            std::process::exit(1);
        }
    }
}
