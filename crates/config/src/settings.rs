//! Dashboard settings: validated changes and the current/default input settings view.
use crate::env::Env;
use serde_json::{json, Value};
use tacit_core::store::keys::{self, Chord, HangulAction};
use tacit_core::store::read_object;
use tacit_core::Settings;

fn key_names(keys: &[Chord]) -> Vec<String> {
    keys.iter().map(ToString::to_string).collect()
}

pub(crate) fn view(settings: &Settings, check_updates: bool) -> Value {
    let mut value = settings.to_value();
    let object = value.as_object_mut().unwrap();
    // Languages are changed through the pack-aware language operations.
    object.remove("languages");
    object.remove("japanese");
    object.insert("checkUpdates".into(), check_updates.into());
    value
}

pub(crate) fn change(env: &Env, body: &Value) -> Result<(), String> {
    let Some(changes) = body.as_object() else {
        return Err("settings have to be an object".into());
    };
    let defaults = view(&Settings::default(), true);
    for (key, value) in changes {
        let Some(kind) = defaults.get(key) else {
            return Err(format!("'{key}' is not a setting you can change here"));
        };
        let ok = (kind.is_boolean() && value.is_boolean())
            || (kind.is_string() && value.is_string())
            || (kind.is_array()
                && value
                    .as_array()
                    .is_some_and(|v| v.iter().all(Value::is_string)));
        if !ok {
            return Err(format!("'{key}' has a value this setting does not take"));
        }
    }
    let mut changes = changes.clone();
    if let Some(text) = changes.get("hangulAction").and_then(Value::as_str) {
        let action: HangulAction = text.parse()?;
        changes.insert("hangulAction".into(), json!(action.to_string()));
    }
    if ["hangulKey", "flipKeys", "pauseKeys"]
        .iter()
        .any(|name| changes.contains_key(*name))
    {
        let current = Settings::load(Some(&env.settings));
        let hangul = changes
            .get("hangulKey")
            .and_then(Value::as_str)
            .map_or_else(|| current.hangul_key.to_string(), str::to_string);
        let new_hangul = keys::parse_hangul(&hangul)?;
        // A list not given keeps its keys, minus those the new Han/Eng key can't take
        // (Alt+HangulKey once the Han/Eng key is RAlt).
        let list = |name: &str, now: &[Chord]| -> Vec<String> {
            match changes.get(name).and_then(Value::as_array) {
                Some(given) => given
                    .iter()
                    .filter_map(Value::as_str)
                    .map(str::to_string)
                    .collect(),
                None => {
                    let kept: Vec<Chord> = now
                        .iter()
                        .copied()
                        .filter(|c| c.resolve(&new_hangul).is_ok())
                        .collect();
                    key_names(&kept)
                }
            }
        };
        let saved = keys::validate(
            &hangul,
            &list("flipKeys", &current.flip_keys),
            &list("pauseKeys", &current.pause_keys),
        )?;
        changes.insert("hangulKey".into(), json!(saved.hangul.to_string()));
        changes.insert("flipKeys".into(), json!(key_names(&saved.flip)));
        changes.insert("pauseKeys".into(), json!(key_names(&saved.pause)));
    }
    let mut file = read_object(&env.settings);
    let object = file
        .as_object_mut()
        .ok_or("settings.json is not an object")?;
    object.remove("pauseKey");
    for (key, value) in &changes {
        object.insert(key.clone(), value.clone());
    }
    tacit_core::store::persist(&env.settings, &file).map_err(|e| format!("could not save: {e}"))
}
