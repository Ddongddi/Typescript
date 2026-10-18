//! Installing a given list of languages at install time (`setup --languages ko,en`). Choosing
//! them is the welcome page's first screen (`tacit-config welcome`).

use tacit_core::{Language, Languages};

use crate::catalog::Catalog;
use crate::env::Env;
use crate::error::{err, Result};
use crate::fetch::Fetch;
use crate::install::{install_with_requirements, Installed};
use crate::languages;
use crate::source::Source;

pub fn parse_list(list: &str) -> Vec<String> {
    list.split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

/// Installs `chosen` (and the packs they need) and turns exactly those on, in that order.
/// The request is checked before anything is downloaded, and the languages setting changes
/// only after every download worked.
pub fn apply(
    env: &Env,
    fetch: &dyn Fetch,
    base: &str,
    catalog: &Catalog,
    chosen: &[String],
    say: &mut dyn FnMut(&str),
) -> Result<Languages> {
    if chosen.is_empty() {
        return err("choose at least one language");
    }
    let mut wanted: Vec<Language> = Vec::new();
    for id in chosen {
        let Some(language) = Language::from_id(id) else {
            return err(format!("'{id}' is not a language (en, ko, ja)"));
        };
        if catalog.pack(id).is_none() {
            return err(format!("this release has no '{id}' pack"));
        }
        if wanted.contains(&language) {
            return err(format!("'{id}' is listed twice"));
        }
        wanted.push(language);
    }
    for id in chosen {
        let name = catalog.pack(id).map_or(id.as_str(), |p| p.name.as_str());
        say(&format!("{name} ({id}) 받는 중…"));
        for (installed, outcome) in install_with_requirements(env, fetch, base, catalog, id)? {
            if outcome == Installed::New && installed != *id {
                say(&format!("  {installed} 데이터도 필요해서 함께 받았습니다"));
            }
        }
    }
    let Some(on) = Languages::new(wanted) else {
        return err("choose at least one language");
    };
    languages::save(env, &on)?;
    say("끝났습니다");
    Ok(on)
}

/// The languages `setup` is to install: the `--languages` list, or with `--if-configured` the
/// ones already on (an update keeps them). A first install has neither: the welcome page asks.
fn requested(args: &[String], env: &Env) -> Result<Vec<String>> {
    if let Some(list) = args
        .iter()
        .position(|a| a == "--languages")
        .and_then(|i| args.get(i + 1))
    {
        return Ok(parse_list(list));
    }
    if args.iter().any(|a| a == "--if-configured") {
        if !env.settings.exists() {
            return err("no languages are chosen yet: run tacit-config welcome");
        }
        return Ok(languages::enabled(env)
            .ids()
            .into_iter()
            .map(String::from)
            .collect());
    }
    err("setup needs --languages ko,en (or run tacit-config welcome)")
}

/// `setup --languages ko,en` or `setup --if-configured`. Plain `setup` opens the welcome page
/// instead (see main.rs), so this only runs with one of the two.
pub fn run(args: &[String], env: &Env, source: &Source) -> Result<String> {
    let chosen = requested(args, env)?;
    let catalog = source.catalog()?;
    let on = apply(
        env,
        source.fetch.as_ref(),
        &source.base,
        &catalog,
        &chosen,
        &mut |m| println!("{m}"),
    )?;
    Ok(format!("켜진 언어: {}\n", on.ids().join(", ")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::fixture;

    fn base() -> &'static str {
        "https://example.test/release/packs.json"
    }

    fn pick(ids: &[&str]) -> Vec<String> {
        ids.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn the_chosen_languages_are_installed_and_turned_on_in_order() {
        let f = fixture("setup");
        let mut said = Vec::new();
        let on = apply(
            &f.env,
            &f.source,
            base(),
            &f.catalog,
            &pick(&["ko", "ja"]),
            &mut |m| said.push(m.to_string()),
        )
        .unwrap();
        assert_eq!(on.ids(), vec!["ko", "ja"]);
        // Both need the English word list, so its data comes along without being turned on.
        assert!(f.env.pack_folder("en", "0.4.0").is_dir());
        assert_eq!(crate::languages::enabled(&f.env).ids(), vec!["ko", "ja"]);
        assert!(said.iter().any(|m| m.contains("ko")), "{said:?}");
    }

    #[test]
    fn nothing_chosen_is_refused_and_nothing_is_installed() {
        let f = fixture("setup-none");
        assert!(apply(&f.env, &f.source, base(), &f.catalog, &[], &mut |_| {}).is_err());
        assert!(!f.env.packs.exists());
    }

    #[test]
    fn unknown_and_repeated_languages_are_refused() {
        let f = fixture("setup-bad");
        assert!(apply(
            &f.env,
            &f.source,
            base(),
            &f.catalog,
            &pick(&["xx"]),
            &mut |_| {}
        )
        .is_err());
        assert!(apply(
            &f.env,
            &f.source,
            base(),
            &f.catalog,
            &pick(&["ko", "ko"]),
            &mut |_| {}
        )
        .is_err());
        assert!(!f.env.packs.exists(), "a bad request installs nothing");
    }

    #[test]
    fn a_failed_download_keeps_the_previous_languages() {
        let f = fixture("setup-fail");
        crate::languages::save(&f.env, &tacit_core::Languages::from_ids(&["en"]).unwrap()).unwrap();
        let nowhere = crate::fetch::Dir(f.root.join("nowhere"));
        assert!(apply(
            &f.env,
            &nowhere,
            base(),
            &f.catalog,
            &pick(&["ja"]),
            &mut |_| {}
        )
        .is_err());
        assert_eq!(crate::languages::enabled(&f.env).ids(), vec!["en"]);
    }

    #[test]
    fn if_configured_takes_the_languages_that_are_on() {
        let f = fixture("setup-configured");
        crate::languages::save(
            &f.env,
            &tacit_core::Languages::from_ids(&["ko", "en"]).unwrap(),
        )
        .unwrap();
        assert_eq!(
            requested(&pick(&["--if-configured"]), &f.env).unwrap(),
            vec!["ko", "en"]
        );
    }

    #[test]
    fn if_configured_refuses_a_first_install() {
        let f = fixture("setup-first");
        assert!(requested(&pick(&["--if-configured"]), &f.env).is_err());
        assert!(requested(&[], &f.env).is_err());
        assert_eq!(
            requested(&pick(&["--languages", "ja,en"]), &f.env).unwrap(),
            vec!["ja", "en"]
        );
    }

    #[test]
    fn a_list_of_ids_parses() {
        assert_eq!(parse_list("ko, en ,ja"), vec!["ko", "en", "ja"]);
        assert!(parse_list("").is_empty());
    }
}
