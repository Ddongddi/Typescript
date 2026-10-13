//! The install-time choice of languages: a checklist, then an order, then the downloads.

use std::io::IsTerminal;

use dialoguer::theme::ColorfulTheme;
use dialoguer::{MultiSelect, Sort};
use typescript_core::{Language, Languages};

use crate::catalog::Catalog;
use crate::env::Env;
use crate::error::{err, Error, Result};
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

fn ask(e: impl std::fmt::Display) -> Error {
    Error(format!("the choice was cancelled: {e}"))
}

/// The checklist and the order, on the terminal.
pub fn choose(catalog: &Catalog) -> Result<Vec<String>> {
    let theme = ColorfulTheme::default();
    let offered: Vec<_> = catalog
        .packs
        .iter()
        .filter(|p| Language::from_id(&p.id).is_some())
        .collect();
    let labels: Vec<String> = offered
        .iter()
        .map(|p| format!("{:<10} {:>6.1} MB", p.name, p.size as f64 / 1_048_576.0))
        .collect();
    let defaults: Vec<bool> = offered
        .iter()
        .map(|p| p.id == "en" || p.id == "ko")
        .collect();
    let picked = loop {
        let picked = MultiSelect::with_theme(&theme)
            .with_prompt("쓸 언어를 고르세요 (Space 선택, Enter 다음)")
            .items(&labels)
            .defaults(&defaults)
            .interact()
            .map_err(ask)?;
        if !picked.is_empty() {
            break picked;
        }
        println!("하나 이상 골라야 합니다.");
    };
    let mut ids: Vec<String> = picked.iter().map(|&i| offered[i].id.clone()).collect();
    if ids.len() > 1 {
        let names: Vec<&str> = picked.iter().map(|&i| offered[i].name.as_str()).collect();
        let order = Sort::with_theme(&theme)
            .with_prompt("Esc로 넘기는 순서 (↑↓ 이동, Space 집기, Enter 확정)")
            .items(&names)
            .interact()
            .map_err(ask)?;
        ids = order.iter().map(|&i| ids[i].clone()).collect();
    }
    Ok(ids)
}

/// `setup [--languages ko,en]`
pub fn run(args: &[String], env: &Env, source: &Source) -> Result<String> {
    let catalog = source.catalog()?;
    let given = args
        .iter()
        .position(|a| a == "--languages")
        .and_then(|i| args.get(i + 1))
        .map(|list| parse_list(list));
    let chosen = match given {
        Some(list) => list,
        None if std::io::stdin().is_terminal() => choose(&catalog)?,
        None => return err("setup needs a terminal, or --languages ko,en"),
    };
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
        crate::languages::save(
            &f.env,
            &typescript_core::Languages::from_ids(&["en"]).unwrap(),
        )
        .unwrap();
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
    fn a_list_of_ids_parses() {
        assert_eq!(parse_list("ko, en ,ja"), vec!["ko", "en", "ja"]);
        assert!(parse_list("").is_empty());
    }
}
