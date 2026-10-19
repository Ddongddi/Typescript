//! The dashboard's page. It is written as separate files under `web/` (markup, stylesheets,
//! scripts) and put together once, so the server still answers with a single self-contained
//! HTML document and the content-security policy needs no extra sources.

use std::sync::OnceLock;

const INDEX: &str = include_str!(concat!(env!("OUT_DIR"), "/web/index.html"));

/// Stylesheets, in cascade order.
const STYLES: [&str; 5] = [
    include_str!(concat!(env!("OUT_DIR"), "/web/base.css")),
    include_str!(concat!(env!("OUT_DIR"), "/web/controls.css")),
    include_str!(concat!(env!("OUT_DIR"), "/web/panels.css")),
    include_str!(concat!(env!("OUT_DIR"), "/web/chat.css")),
    include_str!(concat!(env!("OUT_DIR"), "/web/keys.css")),
];

/// Scripts, in load order. They share one scope (a classic script), so a later file may use
/// anything an earlier one declares.
const SCRIPTS: [&str; 11] = [
    include_str!(concat!(env!("OUT_DIR"), "/web/core.js")),
    include_str!(concat!(env!("OUT_DIR"), "/web/dropdown.js")),
    include_str!(concat!(env!("OUT_DIR"), "/web/languages.js")),
    include_str!(concat!(env!("OUT_DIR"), "/web/learning.js")),
    include_str!(concat!(env!("OUT_DIR"), "/web/uninstall.js")),
    include_str!(concat!(env!("OUT_DIR"), "/web/chat.js")),
    include_str!(concat!(env!("OUT_DIR"), "/web/settings.js")),
    include_str!(concat!(env!("OUT_DIR"), "/web/keys.js")),
    include_str!(concat!(env!("OUT_DIR"), "/web/apps.js")),
    include_str!(concat!(env!("OUT_DIR"), "/web/update.js")),
    include_str!(concat!(env!("OUT_DIR"), "/web/main.js")),
];

/// The first-run page (`/welcome`): a greeting and a short guided practice, one thing per screen.
/// It shares the base styles and the first script (token, `h`, `api`) with the dashboard.
const WELCOME: &str = include_str!(concat!(env!("OUT_DIR"), "/web/welcome.html"));
const WELCOME_STYLES: [&str; 2] = [
    include_str!(concat!(env!("OUT_DIR"), "/web/base.css")),
    include_str!(concat!(env!("OUT_DIR"), "/web/welcome.css")),
];
const WELCOME_SCRIPTS: [&str; 2] = [
    include_str!(concat!(env!("OUT_DIR"), "/web/core.js")),
    include_str!(concat!(env!("OUT_DIR"), "/web/welcome.js")),
];

const STYLE_MARK: &str = "<!--styles-->";
const SCRIPT_MARK: &str = "<!--scripts-->";

/// The page, built on first use.
pub fn page() -> &'static str {
    static PAGE: OnceLock<String> = OnceLock::new();
    PAGE.get_or_init(|| {
        INDEX
            .replace(STYLE_MARK, &STYLES.join("\n"))
            .replace(SCRIPT_MARK, &SCRIPTS.join("\n"))
    })
}

/// The welcome page, built on first use.
pub fn welcome_page() -> &'static str {
    static PAGE: OnceLock<String> = OnceLock::new();
    PAGE.get_or_init(|| {
        WELCOME
            .replace(STYLE_MARK, &WELCOME_STYLES.join("\n"))
            .replace(SCRIPT_MARK, &WELCOME_SCRIPTS.join("\n"))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn optional_language_assets_follow_the_build() {
        for (page, sample) in [(page(), "liveConversion"), (welcome_page(), "にほんご")] {
            assert_eq!(page.contains(sample), cfg!(feature = "japanese"));
            assert!(!page.contains("language:ja") && !page.contains("language:/ja"));
        }
        #[cfg(not(feature = "japanese"))]
        for token in [
            "liveConversion",
            "fullWidthDigits",
            "fullWidthSpace",
            ".l-ja",
        ] {
            assert!(
                !page().contains(token),
                "unexpected language asset: {token}"
            );
        }
    }

    #[test]
    fn the_page_is_one_document_with_everything_in_it() {
        let page = page();
        assert!(!page.contains(STYLE_MARK) && !page.contains(SCRIPT_MARK));
        assert_eq!(page.matches("<style>").count(), 1);
        assert_eq!(page.matches("<script>").count(), 1);
        // The policy only allows inline code and the font host: nothing may point elsewhere.
        assert!(!page.contains("<script src") && !page.contains("<link rel=\"stylesheet\""));
    }

    #[test]
    fn the_welcome_page_is_one_document_too() {
        let page = welcome_page();
        assert!(!page.contains(STYLE_MARK) && !page.contains(SCRIPT_MARK));
        assert_eq!(page.matches("<style>").count(), 1);
        assert_eq!(page.matches("<script>").count(), 1);
        assert!(!page.contains("<script src") && !page.contains("<link rel=\"stylesheet\""));
    }

    #[test]
    fn every_element_the_welcome_scripts_look_up_exists_in_its_markup() {
        let scripts = WELCOME_SCRIPTS.join("\n");
        let mut missing = Vec::new();
        for call in scripts.split("$(\"").skip(1) {
            let id = call.split('"').next().unwrap();
            if !WELCOME.contains(&format!("id=\"{id}\"")) {
                missing.push(id.to_string());
            }
        }
        assert!(missing.is_empty(), "no element with id: {missing:?}");
    }

    #[test]
    fn every_element_the_scripts_look_up_exists_in_the_markup() {
        let scripts = SCRIPTS.join("\n");
        let mut missing = Vec::new();
        for call in scripts.split("$(\"").skip(1) {
            let id = call.split('"').next().unwrap();
            if id.ends_with('-') {
                continue; // a prefix: `$("t-" + name)`, checked below
            }
            // Ids the scripts create themselves are written `id: "..."` in an `h(` call.
            let made = scripts.contains(&format!("id: \"{id}\""));
            if !INDEX.contains(&format!("id=\"{id}\"")) && !made {
                missing.push(id.to_string());
            }
        }
        assert!(missing.is_empty(), "no element with id: {missing:?}");
        for tab in ["lang", "set", "keys", "apps", "upd"] {
            for prefix in ["t", "p"] {
                assert!(
                    INDEX.contains(&format!("id=\"{prefix}-{tab}\"")),
                    "{prefix}-{tab}"
                );
            }
        }
    }
}
