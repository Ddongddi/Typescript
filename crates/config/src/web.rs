//! The dashboard's page. It is written as separate files under `web/` (markup, stylesheets,
//! scripts) and put together once, so the server still answers with a single self-contained
//! HTML document and the content-security policy needs no extra sources.

use std::sync::OnceLock;

const INDEX: &str = include_str!("web/index.html");

/// Stylesheets, in cascade order.
const STYLES: [&str; 5] = [
    include_str!("web/base.css"),
    include_str!("web/controls.css"),
    include_str!("web/panels.css"),
    include_str!("web/chat.css"),
    include_str!("web/keys.css"),
];

/// Scripts, in load order. They share one scope (a classic script), so a later file may use
/// anything an earlier one declares.
const SCRIPTS: [&str; 10] = [
    include_str!("web/core.js"),
    include_str!("web/dropdown.js"),
    include_str!("web/languages.js"),
    include_str!("web/learning.js"),
    include_str!("web/chat.js"),
    include_str!("web/settings.js"),
    include_str!("web/keys.js"),
    include_str!("web/apps.js"),
    include_str!("web/update.js"),
    include_str!("web/main.js"),
];

/// The first-run page (`/welcome`): a greeting and a short guided practice, one thing per screen.
/// It shares the base styles and the first script (token, `h`, `api`) with the dashboard.
const WELCOME: &str = include_str!("web/welcome.html");
const WELCOME_STYLES: [&str; 2] = [
    include_str!("web/base.css"),
    include_str!("web/welcome.css"),
];
const WELCOME_SCRIPTS: [&str; 2] = [include_str!("web/core.js"), include_str!("web/welcome.js")];

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
