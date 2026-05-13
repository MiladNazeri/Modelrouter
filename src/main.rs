fn main() {
    if let Err(error) = modelrouter::run_cli() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
