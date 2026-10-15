//! Pinyin syllables: splitting a typed latin run into Mandarin syllables.
//!
//! The zh language module asks this for its reading; the decide engine can use
//! "this run splits into syllables" as one signal among many. Keys are plain
//! lowercase letters (`nihao` -> `ni hao`), the same convention as the
//! rime-pinyin-simp dictionary (tools/build_pinyin_simp.py, stage 1).

/// Every syllable in the simplified-Pinyin dictionary, sorted for binary search.
static SYLLABLES: &[&str] = &[
    "a", "ai", "an", "ang", "ao", "ba", "bai", "ban", "bang", "bao", "bei", "ben", "beng", "bi",
    "bian", "biao", "bie", "bin", "bing", "bo", "bu", "ca", "cai", "can", "cang", "cao", "ce",
    "cen", "ceng", "cha", "chai", "chan", "chang", "chao", "che", "chen", "cheng", "chi", "chong",
    "chou", "chu", "chua", "chuai", "chuan", "chuang", "chui", "chun", "chuo", "ci", "cong", "cou",
    "cu", "cuan", "cui", "cun", "cuo", "da", "dai", "dan", "dang", "dao", "de", "dei", "den",
    "deng", "di", "dia", "dian", "diao", "die", "ding", "diu", "dong", "dou", "du", "duan", "dui",
    "dun", "duo", "e", "ei", "en", "eng", "er", "fa", "fan", "fang", "fei", "fen", "feng", "fiao",
    "fo", "fou", "fu", "ga", "gai", "gan", "gang", "gao", "ge", "gei", "gen", "geng", "gong",
    "gou", "gu", "gua", "guai", "guan", "guang", "gui", "gun", "guo", "ha", "hai", "han", "hang",
    "hao", "he", "hei", "hen", "heng", "hm", "hong", "hou", "hu", "hua", "huai", "huan", "huang",
    "hui", "hun", "huo", "ji", "jia", "jian", "jiang", "jiao", "jie", "jin", "jing", "jiong",
    "jiu", "ju", "juan", "jue", "jun", "ka", "kai", "kan", "kang", "kao", "ke", "kei", "ken",
    "keng", "kong", "kou", "ku", "kua", "kuai", "kuan", "kuang", "kui", "kun", "kuo", "la", "lai",
    "lan", "lang", "lao", "le", "lei", "leng", "li", "lia", "lian", "liang", "liao", "lie", "lin",
    "ling", "liu", "lo", "long", "lou", "lu", "luan", "lue", "lun", "luo", "lv", "m", "ma", "mai",
    "man", "mang", "mao", "me", "mei", "men", "meng", "mi", "mian", "miao", "mie", "min", "ming",
    "miu", "mo", "mou", "mu", "n", "na", "nai", "nan", "nang", "nao", "ne", "nei", "nen", "neng",
    "ng", "ni", "nian", "niang", "niao", "nie", "nin", "ning", "niu", "nong", "nou", "nu", "nuan",
    "nue", "nuo", "nv", "o", "ou", "pa", "pai", "pan", "pang", "pao", "pei", "pen", "peng", "pi",
    "pian", "piao", "pie", "pin", "ping", "po", "pou", "pu", "qi", "qia", "qian", "qiang", "qiao",
    "qie", "qin", "qing", "qiong", "qiu", "qu", "quan", "que", "qun", "ran", "rang", "rao", "re",
    "ren", "reng", "ri", "rong", "rou", "ru", "ruan", "rui", "run", "ruo", "sa", "sai", "san",
    "sang", "sao", "se", "sen", "seng", "sha", "shai", "shan", "shang", "shao", "she", "shei",
    "shen", "sheng", "shi", "shou", "shu", "shua", "shuai", "shuan", "shuang", "shui", "shun",
    "shuo", "si", "song", "sou", "su", "suan", "sui", "sun", "suo", "ta", "tai", "tan", "tang",
    "tao", "te", "tei", "teng", "ti", "tian", "tiao", "tie", "ting", "tong", "tou", "tu", "tuan",
    "tui", "tun", "tuo", "wa", "wai", "wan", "wang", "wei", "wen", "weng", "wo", "wu", "xi", "xia",
    "xian", "xiang", "xiao", "xie", "xin", "xing", "xiong", "xiu", "xu", "xuan", "xue", "xun",
    "ya", "yan", "yang", "yao", "ye", "yi", "yin", "ying", "yo", "yong", "you", "yu", "yuan",
    "yue", "yun", "za", "zai", "zan", "zang", "zao", "ze", "zei", "zen", "zeng", "zha", "zhai",
    "zhan", "zhang", "zhao", "zhe", "zhei", "zhen", "zheng", "zhi", "zhong", "zhou", "zhu", "zhua",
    "zhuai", "zhuan", "zhuang", "zhui", "zhun", "zhuo", "zi", "zong", "zou", "zu", "zuan", "zui",
    "zun", "zuo",
];

/// Is `s` a pinyin syllable?
pub fn is_syllable(s: &str) -> bool {
    SYLLABLES.binary_search(&s).is_ok()
}

/// Splits a lowercase latin run into syllables, longest valid split first.
/// Returns None when the run cannot be fully split, so the keys stay typed text.
///
/// ```text
/// split("nihao")    -> Some(["ni", "hao"])
/// split("xian")     -> Some(["xian"])
/// split("hello")    -> None
/// ```
pub fn split(keys: &str) -> Option<Vec<&str>> {
    if keys.is_empty() || !keys.bytes().all(|b| b.is_ascii_lowercase()) {
        return None;
    }
    let n = keys.len();
    // can_end[i]: keys[i..] splits into syllables.
    let mut can_end = vec![false; n + 1];
    can_end[n] = true;
    for i in (0..n).rev() {
        for len in 1..=6.min(n - i) {
            if can_end[i + len] && is_syllable(&keys[i..i + len]) {
                can_end[i] = true;
                break;
            }
        }
    }
    if !can_end[0] {
        return None;
    }
    // Walk forward, preferring the longest syllable that still reaches the end.
    let mut out = Vec::new();
    let mut i = 0;
    while i < n {
        for len in (1..=6.min(n - i)).rev() {
            if can_end[i + len] && is_syllable(&keys[i..i + len]) {
                out.push(&keys[i..i + len]);
                i += len;
                break;
            }
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn common_runs_split() {
        assert_eq!(split("nihao"), Some(vec!["ni", "hao"]));
        assert_eq!(split("zhongguo"), Some(vec!["zhong", "guo"]));
        assert_eq!(split("women"), Some(vec!["wo", "men"]));
        assert_eq!(split("xian"), Some(vec!["xian"]));
        assert_eq!(split("zhuang"), Some(vec!["zhuang"]));
    }

    #[test]
    fn unreadable_runs_stay_typed() {
        assert_eq!(split("hello"), None);
        assert_eq!(split("buf"), None);
        assert_eq!(split(""), None);
        assert_eq!(split("ni3"), None);
        assert_eq!(split("Ni"), None);
        assert_eq!(split("나"), None);
    }

    #[test]
    fn syllables() {
        assert!(is_syllable("zhuang"));
        assert!(is_syllable("lv"));
        assert!(!is_syllable("zzz"));
        assert!(!is_syllable(""));
    }
}
