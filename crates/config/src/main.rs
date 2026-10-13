use typescript_config::{cli, env::Env, source::Source};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(env) = Env::detect() else {
        eprintln!("typescript-config: this platform is not supported yet");
        std::process::exit(2);
    };
    let source = Source::detect();
    let result = if args.is_empty() {
        typescript_config::http::run(env, source, true)
    } else {
        cli::run(&args, &env, &source)
    };
    match result {
        Ok(out) => print!("{out}"),
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(1);
        }
    }
}
