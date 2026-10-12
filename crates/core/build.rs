//! Bakes the models at build time. The TIP is loaded into every application, so building the
//! trigram table and word maps at runtime (~240 ms, ~38 MB per process) is not an option:
//! everything here ends up as read-only data inside the DLL, shared between processes.
//!
//! Outputs (OUT_DIR):
//!   english_levels.txt      "word<TAB>level" lines, sorted, apostrophes stripped, filtered,
//!                           COMMON_WORDS merged (what EnglishModel::new did at runtime)
//!   trigrams.bin            27^3 trigram counts then 27^2 context counts, f32 little endian,
//!                           trained on web2 + COMMON_WORDS
//!   korean_words_sorted.txt "word<TAB>count" from korean_words.txt, sorted unique
//!   korean_vocab_sorted.txt korean_vocabulary.txt, sorted unique

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::{env, fs};

const EXTRA_LEVEL: i32 = 35;
const SYMBOLS: usize = 27;

const SHORT_WORDS: &str = "a i ad ah am an as at aw ax be by do eh go ha he hi hm if in is it lo ma me mo my no of oh ok on or ow ox pa \
pc so to tv uh um up us we ya ye yo id ex vs ai ui ux os js ts";

/// Everyday/technical words SCOWL is missing or ranks rare.
const COMMON_WORDS: &str = "ok okay hi hey yeah yep nope lol omg btw thx thanks pls please email emails app apps api apis url urls \
http https www github git commit push pull merge branch repo json yaml xml html css js ts swift python java \
kotlin rust golang npm yarn pnpm node react vue svelte ios macos android iphone ipad mac wifi bluetooth usb \
login logout signup username password online offline website blog vlog youtube google slack notion figma \
debug deploy refactor config backend frontend database cache server client async await todo todos ai llm gpt \
claude chatgpt laptop smartphone selfie emoji internet podcast startup dev devs devops ui ux \
wanna gonna gotta kinda sorta outta lemme gimme dunno yeah yup nah ain cuz tho thru hiya kiddo sayin \
goin doin nothin somethin y'all ya yo hey oi aye \
wouldn couldn shouldn didn doesn isn wasn weren hasn haven hadn needn mustn aren mightn \
makeup brooklyn shakespeare einstein techno pesto americano cosmo promo garde kanji kana romaji ramen";

fn is_ascii_letters(w: &str) -> bool {
    !w.is_empty() && w.chars().all(|c| c.is_ascii_alphabetic())
}

/// [^, ^, letters..., $] with ^ and $ both encoded as 0.
fn encode(word: &str) -> Vec<usize> {
    let mut codes = vec![0, 0];
    codes.extend(word.bytes().filter(u8::is_ascii_lowercase).map(|b| (b - b'a' + 1) as usize));
    codes.push(0);
    codes
}

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=data");
    let data = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap()).join("data");
    let out = PathBuf::from(env::var("OUT_DIR").unwrap());
    let read = |name: &str| fs::read_to_string(data.join(name)).unwrap_or_else(|e| panic!("data/{name}: {e}"));

    let extras: Vec<&str> = COMMON_WORDS.split_whitespace().collect();
    let short: BTreeSet<&str> = SHORT_WORDS.split_whitespace().collect();

    // English levels
    let mut raw: BTreeMap<String, i32> = BTreeMap::new();
    for line in read("english_words.txt").lines() {
        let mut it = line.split('\t');
        if let (Some(w), Some(l), None) = (it.next(), it.next(), it.next()) {
            if let Ok(level) = l.trim().parse() {
                raw.insert(w.to_string(), level);
            }
        }
    }
    for w in &extras {
        let w = w.to_lowercase();
        let level = raw.get(&w).copied().unwrap_or(EXTRA_LEVEL).min(EXTRA_LEVEL);
        raw.insert(w, level);
    }
    let mut levels: BTreeMap<String, i32> = BTreeMap::new();
    for (word, level) in raw {
        let word: String = word.chars().filter(|c| *c != '\'').collect();
        if !is_ascii_letters(&word) {
            continue;
        }
        // Many 1–2 letter list entries aren't words people type ("ra", "ko"); only real ones count.
        if word.chars().count() > 2 || short.contains(word.as_str()) {
            let e = levels.entry(word).or_insert(level);
            *e = (*e).min(level);
        }
    }
    let text: String = levels.iter().map(|(w, l)| format!("{w}\t{l}\n")).collect();
    fs::write(out.join("english_levels.txt"), text).unwrap();

    // Letter trigrams, trained on Webster's 2nd (public domain) plus the extra words.
    let n = SYMBOLS;
    let mut trigrams = vec![0f32; n * n * n];
    let mut contexts = vec![0f32; n * n];
    let web2 = read("web2.txt");
    let spelling = web2.lines().map(|l| l.to_lowercase()).chain(extras.iter().map(|s| s.to_string()));
    for word in spelling {
        if !is_ascii_letters(&word) {
            continue;
        }
        let codes = encode(&word);
        for i in 2..codes.len() {
            trigrams[(codes[i - 2] * n + codes[i - 1]) * n + codes[i]] += 1.0;
            contexts[codes[i - 2] * n + codes[i - 1]] += 1.0;
        }
    }
    let bytes: Vec<u8> = trigrams.iter().chain(contexts.iter()).flat_map(|f| f.to_le_bytes()).collect();
    fs::write(out.join("trigrams.bin"), bytes).unwrap();

    // Korean word lists
    // "word<TAB>count" (the count orders autocomplete suggestions); contains() looks at the word only.
    let mut words: BTreeMap<String, u64> = BTreeMap::new();
    for line in read("korean_words.txt").lines() {
        let mut fields = line.split('\t');
        let word: &str = fields.next().unwrap_or("");
        if !word.is_empty() {
            let count = fields.next().and_then(|n| n.trim().parse::<u64>().ok()).unwrap_or(1);
            *words.entry(word.to_string()).or_default() += count;
        }
    }
    let sorted: String = words.iter().map(|(w, n)| format!("{w}\t{n}\n")).collect();
    fs::write(out.join("korean_words_sorted.txt"), sorted).unwrap();
    let vocab: BTreeSet<String> =
        read("korean_vocabulary.txt").lines().map(str::trim).filter(|w| !w.is_empty()).map(String::from).collect();
    fs::write(out.join("korean_vocab_sorted.txt"), vocab.iter().map(|w| format!("{w}\n")).collect::<String>()).unwrap();
}
