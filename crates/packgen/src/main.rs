//! build-packs <data_dir> <out_dir>: writes every language pack to <out_dir>/<id>-<version>/.

use std::path::PathBuf;
use std::process::ExitCode;

use typescript_packgen::{write_pack, PACKS, VERSION};

fn main() -> ExitCode {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    let release = args.first().is_some_and(|a| a == "--release");
    if release {
        args.remove(0);
    }
    let [data, out] = args.as_slice() else {
        eprintln!("usage: build-packs [--release] <data_dir> <out_dir>");
        eprintln!("  default: <out_dir>/<id>-<version>/ folders (for installing from a checkout)");
        eprintln!("  --release: typescript-pack-<id>-<version>.zip files and packs.json");
        return ExitCode::from(2);
    };
    if release {
        return match typescript_packgen::write_release(&PathBuf::from(data), &PathBuf::from(out)) {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("release: {e}");
                ExitCode::FAILURE
            }
        };
    }
    for pack in PACKS {
        let dir = PathBuf::from(out).join(format!("{}-{VERSION}", pack.id));
        if let Err(e) = write_pack(pack.id, &PathBuf::from(data), &dir) {
            eprintln!("{}: {e}", pack.id);
            return ExitCode::FAILURE;
        }
        println!("{}", dir.display());
    }
    ExitCode::SUCCESS
}
