use typescript_config::{cli, env::Env, source::Source};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(env) = Env::detect() else {
        eprintln!("typescript-config: this platform is not supported yet");
        std::process::exit(2);
    };
    let source = Source::detect();
    match cli::run(&args, &env, &source) {
        Ok(out) => print!("{out}"),
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(1);
        }
    }
}
