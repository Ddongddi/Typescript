//! What the word on screen reads as: the engine's decision, the underlined preview, and the
//! hysteresis that keeps the preview from flickering between languages.

use super::{char_count, InputSession, TextClient};
use crate::decide::engine::{Decision, DecisionContext, DecisionEngine, Language};
use crate::lang::english::EnglishModel;
use crate::lang::hangul::HangulComposer;
use crate::lang::korean::{KoreanModel, UNKNOWN_SYLLABLE_LOGP};
use crate::store::policy::AppPolicy;
use std::sync::LazyLock;

/// The preview only switches language mid-word when the new reading is at least this likely.
const PREVIEW_FLIP_CONFIDENCE: f64 = 0.7;

static KOREAN: LazyLock<KoreanModel> = LazyLock::new(KoreanModel::default);

/// The last decision and what it was made from, for debugging.
#[derive(Clone, Debug)]
pub struct Trace {
    pub keys: String,
    pub previous: Option<Language>,
    pub decision: Decision,
    /// Still being typed (the preview) rather than committed.
    pub partial: bool,
}

impl InputSession {
    pub(super) fn decide(&mut self, partial: bool) -> Decision {
        let ctx = DecisionContext {
            previous_language: self.previous_language,
            partial,
        };
        let d = self.engine.decide(&self.word.keys, &ctx);
        let threshold = self.policy.conversion_threshold;
        let kept = d.language != Language::English && d.confidence < threshold;
        self.kept_english_by_strict =
            kept && threshold >= AppPolicy::STRICT_THRESHOLD && char_count(&self.word.keys) <= 3;
        let decision = if kept {
            Decision {
                language: Language::English,
                confidence: 1.0 - d.confidence,
                output: self.word.keys.clone(),
                explanation: format!(
                    "kept English ({:.2} < {:.2}); {}",
                    d.confidence, threshold, d.explanation
                ),
                runner_up: d.runner_up,
            }
        } else {
            d
        };
        self.last_trace = Some(Trace {
            keys: self.word.keys.clone(),
            previous: self.previous_language,
            decision: decision.clone(),
            partial,
        });
        decision
    }

    /// What Space/Return commits without Esc: the full-word decision, except that a hesitant
    /// change away from the reading on screen doesn't win. The same rule as the preview, so
    /// what you see is what you get.
    pub(super) fn committed_language(&mut self) -> Language {
        let d = self.decide(false);
        match self.word.engine_preview {
            Some(shown) if shown != d.language && !self.is_weak_flip_allowed(&d, shown) => shown,
            _ => d.language,
        }
    }

    /// Whether `decision` may replace the reading on screen: when it's confident, when it's a
    /// dictionary English word (which is what Space/Return will commit), or when a Strict app
    /// kept a short token English.
    pub(super) fn is_weak_flip_allowed(&self, decision: &Decision, shown: Language) -> bool {
        decision.confidence >= PREVIEW_FLIP_CONFIDENCE
            || (decision.language == Language::English
                && (self.kept_english_by_strict
                    || EnglishModel::shared().is_known_word(&self.word.keys)))
            || self.preview(shown).is_none()
    }

    pub(super) fn update_preview(&mut self, client: &mut dyn TextClient) {
        if self.word.is_empty() {
            self.word.preview_language = None;
            self.word.engine_preview = None;
            client.set_marked_text("");
        } else if let Some(forced) = self.word.override_language() {
            self.word.preview_language = Some(forced);
            let text = match forced {
                Language::Korean => self.word.hangul().unwrap_or_else(|| self.word.keys.clone()),
                other => self.engine.render(&self.word.keys, other),
            };
            client.set_marked_text(&text);
        } else {
            self.split_off_korean_before_other(client);
            let decision = self.decide(true);
            let mut language = decision.language;
            // Hysteresis: a weak flip on one keystroke isn't shown, except into a dictionary
            // English word, which is what Space/Return will commit.
            if let Some(shown) = self.word.preview_language {
                if shown != language
                    && char_count(&self.word.keys) > 1
                    && !self.is_weak_flip_allowed(&decision, shown)
                {
                    language = shown;
                }
            }
            self.word.preview_language = Some(language);
            self.word.engine_preview = Some(language);
            let text = self
                .preview(language)
                .unwrap_or_else(|| self.word.keys.clone());
            client.set_marked_text(&text);
        }
    }

    /// Korean with English typed straight after it reads as broken Hangul: 오늘은 then
    /// google에서 (dhsmfdmsgoogledptj). When the keys only make Hangul with stray jamo, but a
    /// front of two or more ordinary syllables is followed by a known English word (or one with
    /// a particle), the Korean goes in as typed and the rest carries on as its own word.
    pub(super) fn split_off_korean_before_other(&mut self, client: &mut dyn TextClient) {
        let chars: Vec<char> = self.word.keys.chars().collect();
        if chars.len() < 7 {
            return;
        }
        if HangulComposer::compose(&self.word.keys).is_none_or(|c| !c.is_broken()) {
            return;
        }
        // Only what reads as Korean on its own, judged without context.
        let alone = self.engine.decide(
            &self.word.keys,
            &DecisionContext {
                previous_language: None,
                partial: true,
            },
        );
        if alone.language != Language::Korean {
            return;
        }
        for cut in 4..=(chars.len() - 3) {
            let front: String = chars[..cut].iter().collect();
            let back: String = chars[cut..].iter().collect();
            let Some(korean) = HangulComposer::compose(&front) else {
                continue;
            };
            if !korean.stray_jamo().is_empty()
                || korean.syllables().len() < 2
                || !korean
                    .syllables()
                    .iter()
                    .all(|s| KOREAN.syllable_log_probability(*s) > UNKNOWN_SYLLABLE_LOGP)
                || !self.is_other_language_word(&back)
            {
                continue;
            }
            client.insert_text(&korean.text(), None);
            self.word.keys = back;
            self.word.syllable_start = None;
            self.previous_language = Some(Language::Korean);
            self.word.preview_language = None;
            self.word.engine_preview = None;
            return;
        }
    }

    /// A known English word, or one with a Korean particle (google에서).
    pub(super) fn is_other_language_word(&self, keys: &str) -> bool {
        let ctx = DecisionContext {
            previous_language: Some(Language::Korean),
            partial: true,
        };
        match self.engine.decide(keys, &ctx).language {
            Language::English => {
                char_count(keys) >= 3 && EnglishModel::shared().is_known_word(keys)
            }
            Language::Mixed => true,
            Language::Korean => false,
        }
    }

    /// The word so far rendered in `language`, or None when it has no such reading.
    pub(super) fn preview(&self, language: Language) -> Option<String> {
        match language {
            Language::English => Some(self.word.keys.clone()),
            Language::Korean => self.word.hangul(),
            Language::Mixed => {
                let text = self.engine.render(&self.word.keys, Language::Mixed);
                (text != self.word.keys).then_some(text)
            }
        }
    }
}
