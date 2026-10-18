use std::{env, fs, path::PathBuf};

// Optional language blocks are removed before include_str!, so disabled UI never ships.
fn main() {
    println!("cargo:rerun-if-changed=src/web");
    let out = PathBuf::from(env::var_os("OUT_DIR").unwrap()).join("web");
    fs::create_dir_all(&out).unwrap();
    for entry in fs::read_dir("src/web").unwrap() {
        let path = entry.unwrap().path();
        if !path.is_file() {
            continue;
        }
        let mut text = fs::read_to_string(&path).unwrap();
        for (id, feature) in [("ja", "JAPANESE")] {
            for (open, close) in [("/*language:", "*/"), ("<!--language:", "-->")] {
                let start = format!("{open}{id}{close}");
                let end = format!("{open}/{id}{close}");
                while let Some(a) = text.find(&start) {
                    let b = a + start.len();
                    let c = b + text[b..].find(&end).expect("unclosed language UI block");
                    let body = if env::var_os(format!("CARGO_FEATURE_{feature}")).is_some() {
                        text[b..c].to_string()
                    } else {
                        String::new()
                    };
                    text.replace_range(a..c + end.len(), &body);
                }
            }
        }
        fs::write(out.join(path.file_name().unwrap()), text).unwrap();
    }
}
