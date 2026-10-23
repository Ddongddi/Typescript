//! A support report built from an explicit allowlist: no text, learning, paths or tokens.
use serde_json::{json, Value};
use tacit_core::{pack, AppPolicies, Settings};

use crate::env::Env;

pub fn report(env: &Env, system: Value) -> Value {
    let settings = Settings::load(Some(&env.settings));
    let scan = pack::scan(&env.packs);
    let app = system["activeApp"].as_str();
    let policy = AppPolicies::load(Some(&env.apps)).policy(app);
    json!({
        "version": env!("CARGO_PKG_VERSION"),
        "os": std::env::consts::OS,
        "arch": std::env::consts::ARCH,
        "ime": system["ime"],
        "defaultInputMethod": system["defaultInputMethod"],
        "perAppInputMethod": system["perAppInputMethod"],
        "activeApp": app,
        "appPolicy": {
            "enabled": policy.enabled,
            "useSystemIme": policy.use_system_ime,
            "escapeToApp": policy.escape_to_app,
            "conversionThreshold": policy.conversion_threshold,
        },
        "selectedLanguages": settings.languages.ids(),
        "installedPacks": scan.packs.iter().map(|p| json!({
            "id": p.manifest.id, "version": p.manifest.version,
        })).collect::<Vec<_>>(),
        "unusablePackCount": scan.problems.len(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn report_excludes_user_text_paths_and_unknown_settings() {
        let f = crate::testutil::fixture("diagnostics");
        std::fs::create_dir_all(f.env.settings.parent().unwrap()).unwrap();
        std::fs::write(&f.env.settings, br#"{"private":"secret-draft"}"#).unwrap();
        std::fs::write(&f.env.apps, br#"{"sample.exe":{"useSystemIme":true}}"#).unwrap();
        let report = report(
            &f.env,
            json!({"activeApp":"sample.exe", "token":"secret-token", "text":"secret-text"}),
        );
        assert_eq!(report["appPolicy"]["useSystemIme"], true);
        assert_eq!(report["activeApp"], "sample.exe");
        let text = report.to_string();
        for private in [
            "secret-draft",
            "secret-token",
            "secret-text",
            f.root.to_str().unwrap(),
        ] {
            assert!(!text.contains(private));
        }
    }
}
