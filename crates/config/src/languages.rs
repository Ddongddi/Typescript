//! Which languages are on, in what order: `languages` in settings.json, kept consistent with the
//! packs that are installed.

use typescript_core::{Language, Languages, Settings};

use crate::catalog::Catalog;
use crate::env::Env;
use crate::error::{err, Result};
use crate::fetch::Fetch;
use crate::install::{install_with_requirements, remove_pack, required_by, RemoveReport};

#[derive(Debug)]
pub struct Removal {
    pub languages: Languages,
    pub report: RemoveReport,
    /// Ids of the packs that need this language's data, so its folder was kept.
    pub kept_for: Vec<String>,
}

pub fn enabled(env: &Env) -> Languages {
    Settings::load(Some(&env.settings)).languages
}

pub fn save(env: &Env, languages: &Languages) -> Result<()> {
    let mut settings = Settings::load(Some(&env.settings));
    settings.languages = languages.clone();
    settings.save(&env.settings)?;
    Ok(())
}

fn language(id: &str) -> Result<Language> {
    match Language::from_id(id) {
        Some(l) => Ok(l),
        None => err(format!("'{id}' is not a language (en, ko, ja)")),
    }
}

/// Installs `id` (and the packs it needs) and turns it on, last in the order. A failed install
/// changes nothing.
pub fn add_language(
    env: &Env,
    fetch: &dyn Fetch,
    base: &str,
    catalog: &Catalog,
    id: &str,
) -> Result<Languages> {
    let wanted = language(id)?;
    install_with_requirements(env, fetch, base, catalog, id)?;
    let current = enabled(env);
    if current.contains(wanted) {
        return Ok(current);
    }
    let Some(next) = Languages::new(current.iter().chain(std::iter::once(wanted))) else {
        return err("no languages");
    };
    save(env, &next)?;
    Ok(next)
}

/// Turns `id` off and deletes its pack, unless another installed pack needs its data.
pub fn remove_language(env: &Env, id: &str) -> Result<Removal> {
    let gone = language(id)?;
    let current = enabled(env);
    if !current.contains(gone) {
        return err(format!("'{id}' is not turned on"));
    }
    let Some(next) = current.filtered(|l| l != gone) else {
        return err("at least one language has to stay on");
    };
    save(env, &next)?;
    let kept_for = required_by(env, id);
    let report = if kept_for.is_empty() {
        remove_pack(env, id)?
    } else {
        RemoveReport::default()
    };
    Ok(Removal {
        languages: next,
        report,
        kept_for,
    })
}

/// The same languages in a new order.
pub fn reorder(env: &Env, ids: &[&str]) -> Result<Languages> {
    let current = enabled(env);
    let mut wanted = Vec::new();
    for id in ids {
        let l = language(id)?;
        if wanted.contains(&l) {
            return err(format!("'{id}' is listed twice"));
        }
        wanted.push(l);
    }
    let same_set = wanted.len() == current.len() && wanted.iter().all(|l| current.contains(*l));
    if !same_set {
        return err("the order has to list exactly the languages that are on");
    }
    let Some(next) = Languages::new(wanted) else {
        return err("no languages");
    };
    save(env, &next)?;
    Ok(next)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::install::install_with_requirements;
    use crate::testutil::fixture;
    use typescript_core::Languages;

    fn base() -> &'static str {
        "https://example.test/release/packs.json"
    }

    fn ids(l: &Languages) -> Vec<&'static str> {
        l.ids()
    }

    #[test]
    fn adding_a_language_installs_it_and_turns_it_on_at_the_end() {
        let f = fixture("add");
        let l = add_language(&f.env, &f.source, base(), &f.catalog, "ko").unwrap();
        assert_eq!(ids(&l), vec!["en", "ko"]); // en was the default; ko goes after
        assert!(f.env.pack_folder("ko", "0.4.0").is_dir());
        assert_eq!(ids(&enabled(&f.env)), vec!["en", "ko"]);
    }

    #[test]
    fn adding_ko_to_nothing_installs_english_data_without_turning_english_on() {
        let f = fixture("noten");
        save(&f.env, &Languages::from_ids(&["ja"]).unwrap()).unwrap();
        let l = add_language(&f.env, &f.source, base(), &f.catalog, "ko").unwrap();
        assert_eq!(ids(&l), vec!["ja", "ko"]);
        assert!(
            f.env.pack_folder("en", "0.4.0").is_dir(),
            "ko needs the English word list"
        );
    }

    #[test]
    fn adding_twice_changes_nothing() {
        let f = fixture("twice");
        add_language(&f.env, &f.source, base(), &f.catalog, "ko").unwrap();
        let l = add_language(&f.env, &f.source, base(), &f.catalog, "ko").unwrap();
        assert_eq!(ids(&l), vec!["en", "ko"]);
    }

    #[test]
    fn unknown_languages_are_refused() {
        let f = fixture("unknown");
        assert!(add_language(&f.env, &f.source, base(), &f.catalog, "xx").is_err());
        assert!(add_language(&f.env, &f.source, base(), &f.catalog, "mixed").is_err());
        assert!(remove_language(&f.env, "xx").is_err());
    }

    #[test]
    fn the_last_language_cannot_be_removed() {
        let f = fixture("last");
        save(&f.env, &Languages::from_ids(&["ko"]).unwrap()).unwrap();
        let e = remove_language(&f.env, "ko").unwrap_err();
        assert!(e.0.contains("at least one"), "{e}");
        assert_eq!(ids(&enabled(&f.env)), vec!["ko"]);
    }

    #[test]
    fn removing_a_language_turns_it_off_and_deletes_its_pack() {
        let f = fixture("remove");
        add_language(&f.env, &f.source, base(), &f.catalog, "ja").unwrap();
        let removal = remove_language(&f.env, "ja").unwrap();
        assert_eq!(ids(&removal.languages), vec!["en", "ko"]);
        assert!(!f.env.pack_folder("ja", "0.4.0").exists());
        assert!(removal.kept_for.is_empty());
    }

    #[test]
    fn english_can_be_turned_off_but_its_data_stays_while_korean_needs_it() {
        let f = fixture("en-off");
        add_language(&f.env, &f.source, base(), &f.catalog, "ko").unwrap();
        let removal = remove_language(&f.env, "en").unwrap();
        assert_eq!(ids(&removal.languages), vec!["ko"]);
        assert_eq!(removal.kept_for, vec!["ko".to_string()]);
        assert!(f.env.pack_folder("en", "0.4.0").is_dir());
    }

    #[test]
    fn the_order_can_change_but_not_the_set() {
        let f = fixture("order");
        add_language(&f.env, &f.source, base(), &f.catalog, "ko").unwrap();
        let l = reorder(&f.env, &["ko", "en"]).unwrap();
        assert_eq!(ids(&l), vec!["ko", "en"]);
        assert!(reorder(&f.env, &["ko"]).is_err());
        assert!(reorder(&f.env, &["ko", "en", "ja"]).is_err());
        assert!(reorder(&f.env, &["ko", "ko"]).is_err());
        assert_eq!(ids(&enabled(&f.env)), vec!["ko", "en"]);
    }

    #[test]
    fn settings_keep_their_other_keys_and_get_no_byte_order_mark() {
        let f = fixture("keep");
        std::fs::write(
            &f.env.settings,
            "\u{feff}{ \"pauseKey\": \"off\", \"somethingNew\": 7 }",
        )
        .unwrap();
        add_language(&f.env, &f.source, base(), &f.catalog, "ja").unwrap();
        let bytes = std::fs::read(&f.env.settings).unwrap();
        assert_ne!(&bytes[..3], &[0xEF, 0xBB, 0xBF]);
        let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        // the old pause key moves to its new spelling and stays off
        assert!(v.get("pauseKey").is_none());
        assert_eq!(v["pauseKeys"], serde_json::json!([]));
        assert_eq!(v["somethingNew"], 7);
        assert_eq!(v["languages"], serde_json::json!(["en", "ko", "ja"]));
    }

    #[test]
    fn a_failed_install_leaves_the_languages_alone() {
        let f = fixture("fail");
        let nowhere = crate::fetch::Dir(f.root.join("nowhere"));
        assert!(add_language(&f.env, &nowhere, base(), &f.catalog, "ja").is_err());
        assert_eq!(ids(&enabled(&f.env)), vec!["en", "ko"]);
        let _ = install_with_requirements; // keeps the import used
    }
}
