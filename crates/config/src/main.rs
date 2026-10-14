use typescript_config::{cli, env::Env, source::Source};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(env) = Env::detect() else {
        eprintln!("typescript-config: this platform is not supported yet");
        std::process::exit(2);
    };
    let source = Source::detect();
    let hint_env = env.clone();
    // The welcome page: `welcome`, and `setup` without --languages (the old terminal questions
    // are the page's first screen now).
    let welcome = matches!(args.first().map(String::as_str), Some("welcome"))
        || (args.first().map(String::as_str) == Some("setup")
            && !args
                .iter()
                .any(|a| a == "--languages" || a == "--if-configured"));
    let result = if args.is_empty() || welcome {
        // TYPESCRIPT_NO_BROWSER: print the address only (headless runs, browser tests).
        let open = std::env::var_os("TYPESCRIPT_NO_BROWSER").is_none();
        typescript_config::http::run(env, source, open, if welcome { "/welcome" } else { "/" })
    } else {
        cli::run(&args, &env, &source)
    };
    match result {
        Ok(out) => {
            print!("{out}");
            // A hint once a day, after commands that are not themselves about updates.
            let about_updates =
                matches!(args.first().map(String::as_str), Some("check" | "update"));
            if !args.is_empty() && !about_updates {
                let (api, url) = typescript_config::update::api_from_env(true);
                if let Some(hint) = typescript_config::cli::notice(&hint_env, api.as_ref(), &url) {
                    eprint!("{hint}");
                }
            }
        }
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(1);
        }
    }
}
