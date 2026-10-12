//! Tuning and evaluation tool. Usage:
//!   typescript "dkssudgktpdy rmfjgek hello world"   convert arguments
//!   typescript -v "..."                              also print per-word scores
//!   echo "..." | typescript                          convert stdin line by line
//!   typescript --compose < keys.txt                  print "keys<TAB>hangul" per line
//!   typescript --decide < keys_prev.tsv              verdict for "keys<TAB>previous" lines
//!                                                    (previous: english|korean|mixed|empty)

use std::io::{self, BufRead};

use typescript_core::{make_engine, DecisionContext, DecisionEngine, Language, TextConverter};
use typescript_core::hangul::HangulComposer;

fn lines() -> impl Iterator<Item = String> {
    io::stdin().lock().lines().map_while(Result::ok)
}

fn main() {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    // No lexicon: evaluation must not depend on (or change) what this user has learned.
    let engine = make_engine(None);

    match args.first().map(String::as_str) {
        Some("--compose") => {
            for keys in lines() {
                let text = HangulComposer::compose(&keys).map(|c| c.text()).unwrap_or_else(|| keys.clone());
                println!("{keys}\t{text}");
            }
        }
        Some("--decide") => {
            for line in lines() {
                let mut fields = line.split('\t');
                let keys = fields.next().unwrap_or("");
                let previous = fields.next().and_then(Language::parse);
                let d = engine.decide(keys, &DecisionContext { previous_language: previous, partial: false });
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
                        let lang = if d.language == Language::English { "en" } else { "ko" };
                        println!("  {:<16} → {} [{lang} {:.2}] {}", w.keys, d.output, d.confidence, d.explanation);
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
