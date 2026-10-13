//! build-packs <data_dir> <out_dir>: writes every language pack to <out_dir>/<id>-<version>/.

use std::path::PathBuf;
use std::process::ExitCode;

use typescript_packgen::{write_pack, PACKS, VERSION};

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [data, out] = args.as_slice() else {
        eprintln!("usage: build-packs <data_dir> <out_dir>");
        return ExitCode::from(2);
    };
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
