//! Tuning and evaluation tool. Usage:
//!   typescript "dkssudgktpdy rmfjgek hello world"   convert arguments
//!   typescript -v "..."                              also print per-word scores
//!   echo "..." | typescript                          convert stdin line by line
//!   typescript --compose < keys.txt                  print "keys<TAB>hangul" per line
//!   typescript --suggest < keys.txt                  "keys, language, shown, Tab completion" per line
//!   typescript --decide < keys_prev.tsv            verdict for "keys<TAB>previous" lines
//!                                                    (previous: english|korean|mixed|japanese|empty)
//!   typescript --convert < kana.txt                kanji candidates per kana line
//! Add --japanese to score romaji Japanese too (--decide, -v).

use std::io::{self, BufRead};

use typescript_core::lang::hangul::HangulComposer;
use typescript_core::store::history::suggest;
use typescript_core::{
    make_engine, DecisionContext, DecisionEngine, DictionaryConverter, JapaneseConverter, Language,
    Languages, TextConverter,
};

fn lines() -> impl Iterator<Item = String> {
    io::stdin().lock().lines().map_while(Result::ok)
}

fn main() {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    // No lexicon: evaluation must not depend on (or change) what this user has learned.
    let japanese = args.iter().any(|a| a == "--japanese");
    let engine = make_engine(None, &Languages::legacy(japanese));

    match args.first().map(String::as_str) {
        Some("--compose") => {
            for keys in lines() {
                let text = HangulComposer::compose(&keys)
                    .map(|c| c.text())
                    .unwrap_or_else(|| keys.clone());
                println!("{keys}\t{text}");
            }
        }
        Some("--suggest") => {
            // What Tab would complete each line of keys to, from the built-in word lists.
            for keys in lines() {
                let d = engine.decide(
                    &keys,
                    &DecisionContext {
                        previous_language: None,
                        partial: true,
                    },
                );
                let suggestion = suggest(None, d.language, &d.output).unwrap_or_default();
                println!("{keys}\t{}\t{}\t{suggestion}", d.language.name(), d.output);
            }
        }
        Some("--convert") => {
            // Kana → kanji candidates, for tuning and evaluation (no learning file).
            let converter = DictionaryConverter::new(None);
            for kana in lines() {
                let candidates = converter.candidates(&kana);
                let texts: Vec<String> = candidates
                    .iter()
                    .take(8)
                    .map(|c| format!("{}:{}", c.text, c.kana_count))
                    .collect();
                println!("{kana}\t{}", texts.join(" | "));
            }
        }
        Some("--decide") => {
            for line in lines() {
                let mut fields = line.split('\t');
                let keys = fields.next().unwrap_or("");
                let previous = fields.next().and_then(Language::parse);
                let d = engine.decide(
                    keys,
                    &DecisionContext {
                        previous_language: previous,
                        partial: false,
                    },
                );
                println!(
                    "{keys}\t{}\t{:.4}\t{}\t{}\t{}",
                    d.language.name(),
                    d.confidence,
                    d.output,
                    d.explanation,
                    d.runner_up.map_or("", Language::name)
                );
            }
        }
        _ => {
            let verbose = args.first().is_some_and(|a| a == "-v");
            if verbose {
                args.remove(0);
            }
            let converter = TextConverter::new(engine.as_ref());
            let run = |line: &str| {
                let (output, words) = converter.convert(line);
                println!("{output}");
                if verbose {
                    for w in &words {
                        let d = &w.decision;
                        let lang = match d.language {
                            Language::English => "en",
                            Language::Korean => "ko",
                            Language::Mixed => "mix",
                            Language::Japanese => "ja",
                        };
                        println!(
                            "  {:<16} → {} [{lang} {:.2}] {}",
                            w.keys, d.output, d.confidence, d.explanation
                        );
                    }
                }
            };
            if args.is_empty() {
                lines().for_each(|l| run(&l));
            } else {
                run(&args.join(" "));
            }
        }
    }
}
