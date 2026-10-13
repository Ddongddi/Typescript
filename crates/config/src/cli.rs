//! The command line: each command returns what it printed, so the tests read it.

use std::fmt::Write as _;

use typescript_core::{pack, Language};

use crate::env::Env;
use crate::error::{err, Result};
use crate::fetch::Fetch;
use crate::languages;
use crate::source::Source;
use crate::update;

pub const USAGE: &str = "typescript-config [command]
  (none)            open the dashboard
  setup [--languages ko,en]   choose the languages at install time
  list              the languages: installed, turned on, available
  add <id>          install a language and turn it on (en, ko, ja)
  remove <id>       turn a language off and delete its pack
  order <a,b,c>     change the Esc order of the languages that are on
  check             is there a newer release?
  update            update the packs (and Typescript itself when it changed)
Set TYPESCRIPT_RELEASE to a folder or URL to use another release.";

pub fn list(env: &Env, source: &Source) -> Result<String> {
    let installed = pack::scan(&env.packs);
    let on = languages::enabled(env);
    let catalog = source.catalog().ok();
    let mut ids: Vec<String> = installed
        .packs
        .iter()
        .map(|f| f.manifest.id.clone())
        .collect();
    if let Some(c) = &catalog {
        for p in &c.packs {
            if !ids.contains(&p.id) {
                ids.push(p.id.clone());
            }
        }
    }
    ids.sort();
    let mut out = String::new();
    for id in ids {
        let have = installed.packs.iter().find(|f| f.manifest.id == id);
        let listed = catalog.as_ref().and_then(|c| c.pack(&id));
        let name = have
            .map(|f| f.manifest.name.clone())
            .or_else(|| listed.map(|p| p.name.clone()))
            .unwrap_or_default();
        let position = Language::from_id(&id).and_then(|l| on.iter().position(|x| x == l));
        let state = match (have, listed) {
            (Some(f), _) => format!("installed {}", f.manifest.version),
            (None, Some(p)) => format!("available {:.1} MB", p.size as f64 / 1_048_576.0),
            _ => "unavailable".into(),
        };
        let turned = position.map_or("off".to_string(), |i| format!("on (#{})", i + 1));
        writeln!(out, "{id:<4}{name:<12}{state:<22}{turned}").ok();
    }
    Ok(out)
}

pub fn add(env: &Env, source: &Source, id: &str) -> Result<String> {
    let catalog = source.catalog()?;
    let before = pack::scan(&env.packs).packs.len();
    let on = languages::add_language(env, source.fetch.as_ref(), &source.base, &catalog, id)?;
    let new = pack::scan(&env.packs).packs.len().saturating_sub(before);
    Ok(format!(
        "{id} is on ({new} new pack{}). Order: {}\n",
        if new == 1 { "" } else { "s" },
        on.ids().join(", ")
    ))
}

pub fn remove(env: &Env, id: &str) -> Result<String> {
    let removal = languages::remove_language(env, id)?;
    let mut out = format!(
        "{id} is off. Order: {}\n",
        removal.languages.ids().join(", ")
    );
    if !removal.kept_for.is_empty() {
        writeln!(
            out,
            "its data stays: {} needs it",
            removal.kept_for.join(", ")
        )
        .ok();
    }
    for (folder, why) in &removal.report.kept {
        writeln!(
            out,
            "could not delete {folder} ({why}); an app is using it, try again later"
        )
        .ok();
    }
    Ok(out)
}

pub fn order(env: &Env, list: &str) -> Result<String> {
    let ids: Vec<&str> = list
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .collect();
    let on = languages::reorder(env, &ids)?;
    Ok(format!("Order: {}\n", on.ids().join(", ")))
}

pub fn check(env: &Env, api: &dyn Fetch, url: &str) -> Result<String> {
    let info = update::check(env, api, url, update::now(), true)?;
    Ok(if info.available {
        let first = info.notes.lines().next().unwrap_or("");
        format!(
            "{} is out (you have {}). {first}
Run: typescript-config update
",
            info.latest,
            crate::catalog::CORE_VERSION
        )
    } else {
        format!(
            "typescript {} is the latest
",
            crate::catalog::CORE_VERSION
        )
    })
}

/// A one-line hint for after a command: only when checking is on, a day has passed, and there
/// is something newer. Never an error.
pub fn notice(env: &Env, api: &dyn Fetch, url: &str) -> Option<String> {
    let info = update::check(env, api, url, update::now(), false).ok()?;
    info.available.then(|| {
        format!(
            "typescript {} is out: typescript-config update
",
            info.latest
        )
    })
}

/// Runs one non-interactive command.
pub fn run(args: &[String], env: &Env, source: &Source) -> Result<String> {
    match args.first().map(String::as_str) {
        Some("list") => list(env, source),
        Some("add") => match args.get(1) {
            Some(id) => add(env, source, id),
            None => err("add: which language? (en, ko, ja)"),
        },
        Some("remove") => match args.get(1) {
            Some(id) => remove(env, id),
            None => err("remove: which language?"),
        },
        Some("order") => match args.get(1) {
            Some(list) => order(env, list),
            None => err("order: give the ids, e.g. order ko,en"),
        },
        Some("check") => {
            let (api, url) = update::api_from_env(false);
            check(env, api.as_ref(), &url)
        }
        Some("update") => {
            let (api, url) = update::api_from_env(false);
            update::apply(env, source, api.as_ref(), &url, &update::launch_installer)
        }
        Some("setup") => crate::setup::run(&args[1..], env, source),
        Some("help") | Some("--help") | Some("-h") => Ok(format!("{USAGE}\n")),
        Some(other) => err(format!("unknown command '{other}'\n{USAGE}")),
        None => err(USAGE),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::{fixture, publish};

    fn args(line: &str) -> Vec<String> {
        line.split_whitespace().map(String::from).collect()
    }

    #[test]
    fn list_shows_installed_available_and_the_order() {
        let f = fixture("cli-list");
        let source = publish(&f);
        run(&args("add ko"), &f.env, &source).unwrap();
        let out = run(&args("list"), &f.env, &source).unwrap();
        assert!(out.contains("installed 0.4.0"), "{out}");
        assert!(
            out.lines()
                .any(|l| l.starts_with("ja") && l.contains("available") && l.ends_with("off")),
            "{out}"
        );
        assert!(
            out.lines()
                .any(|l| l.starts_with("ko") && l.ends_with("on (#2)")),
            "{out}"
        );
    }

    #[test]
    fn add_remove_and_order_round_trip() {
        let f = fixture("cli-flow");
        let source = publish(&f);
        assert!(run(&args("add ja"), &f.env, &source)
            .unwrap()
            .contains("en, ko, ja"));
        assert!(run(&args("order ja,en,ko"), &f.env, &source)
            .unwrap()
            .contains("ja, en, ko"));
        let out = run(&args("remove ja"), &f.env, &source).unwrap();
        assert!(out.contains("en, ko"), "{out}");
        assert!(run(&args("remove ja"), &f.env, &source).is_err());
    }

    #[test]
    fn check_says_whether_there_is_something_newer() {
        let f = fixture("cli-check");
        let release = f.root.join("release");
        let api = crate::fetch::Dir(release.clone());
        let body = serde_json::json!({ "tag_name": "v9.0.0", "body": "first line
second", "assets": [] });
        std::fs::write(release.join("latest"), body.to_string()).unwrap();
        let out = check(&f.env, &api, "https://x.test/latest").unwrap();
        assert!(
            out.contains("9.0.0 is out") && out.contains("first line") && !out.contains("second"),
            "{out}"
        );
        let same = format!(
            r#"{{ "tag_name": "v{}", "assets": [] }}"#,
            crate::catalog::CORE_VERSION
        );
        std::fs::write(release.join("latest"), same).unwrap();
        assert!(check(&f.env, &api, "https://x.test/latest")
            .unwrap()
            .contains("is the latest"));
    }

    #[test]
    fn the_hint_after_a_command_is_quiet_when_there_is_nothing_or_no_network() {
        let f = fixture("cli-notice");
        let down = crate::fetch::Dir(f.root.join("nowhere"));
        assert!(notice(&f.env, &down, "https://x.test/latest").is_none());
        let release = f.root.join("release");
        std::fs::write(
            release.join("latest"),
            r#"{ "tag_name": "v9.0.0", "assets": [] }"#,
        )
        .unwrap();
        let api = crate::fetch::Dir(release);
        assert!(notice(&f.env, &api, "https://x.test/latest")
            .unwrap()
            .contains("9.0.0"));
    }

    #[test]
    fn mistakes_are_explained() {
        let f = fixture("cli-bad");
        let source = publish(&f);
        assert!(run(&args("add"), &f.env, &source).is_err());
        assert!(run(&args("frobnicate"), &f.env, &source).is_err());
        assert!(run(&args("order en"), &f.env, &source).is_err());
    }
}
