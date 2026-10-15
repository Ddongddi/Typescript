//! Removing Typescript from this computer, asked for in the dashboard. On an installed Windows
//! copy the program and the text service go through the installer's own `-Uninstall` (which asks
//! Windows for administrator rights), and only once that worked is what the user left behind
//! removed: the settings, what was learned, the language packs. Run from anything else (a build
//! folder, a `TYPESCRIPT_HOME` test folder) the program is never touched, only the data.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use serde_json::{json, Value};

use crate::env::Env;

/// What the program keeps next to settings.json. In a real install the whole folder is
/// Typescript's and goes; a folder of any other name may hold anything, so only these go.
const KEPT_FILES: [&str; 8] = [
    "settings.json",
    "apps.json",
    "lexicon.json",
    "user_history.json",
    "kanji_learning.json",
    "update.json",
    "debug.log",
    "debug.on",
];
/// Copies the program leaves beside them: `x.bak`, `x.unreadable` (a damaged file moved aside),
/// `x.tmp` (a save that was cut short).
const KEPT_SUFFIXES: [&str; 3] = [".bak", ".unreadable", ".tmp"];

#[derive(Debug, PartialEq)]
pub struct Plan {
    /// `install.ps1` of the installed program this one is running from (Windows only).
    pub installer: Option<PathBuf>,
    /// Folders (removed whole) and files that go once the program is gone, or at once when
    /// there is no installer to wait for.
    pub targets: Vec<PathBuf>,
}

#[derive(Debug, PartialEq)]
pub enum Outcome {
    /// The installer was started (it asks for administrator rights); the data follows it.
    Started,
    /// Only data was removed, now.
    Removed,
}

impl Outcome {
    pub fn name(&self) -> &'static str {
        match self {
            Outcome::Started => "installer",
            Outcome::Removed => "data",
        }
    }
}

fn is_ours(dir: &Path) -> bool {
    dir.file_name()
        .is_some_and(|n| n.eq_ignore_ascii_case("Typescript"))
}

/// The files of `dir` that are the program's own (see `KEPT_FILES`).
fn kept_entries(dir: &Path) -> Vec<PathBuf> {
    let Ok(read) = fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut found: Vec<PathBuf> = read
        .flatten()
        .filter(|e| e.path().is_file())
        .filter(|e| {
            let name = e.file_name().to_string_lossy().to_string();
            KEPT_FILES.contains(&name.as_str()) || KEPT_SUFFIXES.iter().any(|s| name.ends_with(s))
        })
        .map(|e| e.path())
        .collect();
    found.sort();
    found
}

/// What removing would take away, from the folders in `env`. `exe_dir` is where the running
/// program sits; `test_home` is a `TYPESCRIPT_HOME` run, which never touches an installation.
pub fn plan(env: &Env, exe_dir: Option<&Path>, test_home: bool) -> Plan {
    let mut targets = Vec::new();
    let settings_dir = env.settings.parent().map(Path::to_path_buf);
    let packs_root = env.packs.parent().map(Path::to_path_buf);
    if test_home {
        if let Some(dir) = &settings_dir {
            targets.extend(kept_entries(dir));
        }
        targets.push(env.packs.clone());
        targets.push(env.staging());
    } else {
        if let Some(dir) = settings_dir {
            if is_ours(&dir) {
                targets.push(dir);
            } else {
                targets.extend(kept_entries(&dir));
            }
        }
        // On Windows the packs folder sits alone in Typescript's own folder; elsewhere the folder
        // also holds the input method itself, so only the packs go.
        match packs_root {
            Some(root) if cfg!(windows) && is_ours(&root) => targets.push(root),
            _ => {
                targets.push(env.packs.clone());
                targets.push(env.staging());
            }
        }
    }
    targets.retain(|p| p.exists());
    targets.dedup();
    let installer = exe_dir
        .map(|d| d.join("install.ps1"))
        .filter(|p| cfg!(windows) && !test_home && p.is_file())
        .filter(|p| p.with_file_name("typescript_tip.dll").is_file());
    Plan { installer, targets }
}

/// What the dashboard shows before asking.
pub fn summary(plan: &Plan, test_home: bool) -> Value {
    json!({
        "program": plan.installer.is_some(),
        "testHome": test_home,
        "targets": plan.targets.iter().map(|p| p.display().to_string()).collect::<Vec<_>>(),
    })
}

fn quote(path: &Path) -> String {
    format!("'{}'", path.display().to_string().replace('\'', "''"))
}

/// The PowerShell the helper runs: a moment for the dashboard to end (it is part of what the
/// installer removes), the installer's own `-Uninstall`, and only if that worked the data.
fn script(installer: &Path, targets: &[PathBuf]) -> String {
    let list = targets
        .iter()
        .map(|p| quote(p))
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        "Start-Sleep -Seconds 2; & {} -Uninstall; if ($?) {{ foreach ($p in @({list})) {{ Remove-Item -LiteralPath $p -Recurse -Force -ErrorAction SilentlyContinue }} }}",
        quote(installer)
    )
}

fn remove_all(targets: &[PathBuf]) {
    for path in targets {
        let _ = if path.is_dir() {
            fs::remove_dir_all(path)
        } else {
            fs::remove_file(path)
        };
        // A folder of Typescript's own left empty (the one a pack folder sat in) goes too.
        if let Some(parent) = path.parent().filter(|p| is_ours(p)) {
            let _ = fs::remove_dir(parent);
        }
    }
}

/// Does it: starts the installer (`launch` gets the PowerShell text) or removes the data now.
pub fn execute(plan: &Plan, launch: impl FnOnce(&str) -> io::Result<()>) -> io::Result<Outcome> {
    match &plan.installer {
        Some(installer) => {
            launch(&script(installer, &plan.targets))?;
            Ok(Outcome::Started)
        }
        None => {
            remove_all(&plan.targets);
            Ok(Outcome::Removed)
        }
    }
}

/// Starts the helper without a window, on its own, so it outlives the dashboard.
pub fn launch(script: &str) -> io::Result<()> {
    let mut command = Command::new("powershell.exe");
    command
        .args([
            "-NoProfile",
            "-ExecutionPolicy",
            "Bypass",
            "-WindowStyle",
            "Hidden",
            "-Command",
            script,
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // CREATE_NO_WINDOW | CREATE_NEW_PROCESS_GROUP
        command.creation_flags(0x0800_0000 | 0x0000_0200);
    }
    command.spawn().map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::fixture;

    fn touch(path: &Path) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, "x").unwrap();
    }

    #[test]
    fn a_test_home_loses_only_the_programs_files_and_packs() {
        let f = fixture("uninstall-test-home");
        let home = f.root.join("home");
        for name in [
            "settings.json",
            "lexicon.json",
            "x.bak",
            "kanji_learning.json",
        ] {
            touch(&home.join(name));
        }
        touch(&home.join("notes-i-keep.txt"));
        touch(&home.join("packs").join("en-1").join("manifest.json"));
        let env = Env {
            settings: home.join("settings.json"),
            apps: home.join("apps.json"),
            packs: home.join("packs"),
        };
        let plan = plan(&env, None, true);
        assert_eq!(plan.installer, None);
        assert_eq!(
            execute(&plan, |_| panic!("no installer")).unwrap(),
            Outcome::Removed
        );
        assert!(
            home.join("notes-i-keep.txt").exists(),
            "what is not ours stays"
        );
        for gone in [
            "settings.json",
            "lexicon.json",
            "x.bak",
            "kanji_learning.json",
            "packs",
        ] {
            assert!(!home.join(gone).exists(), "{gone} should be gone");
        }
        assert!(home.exists(), "a folder that still holds something stays");
    }

    #[test]
    fn typescripts_own_folders_go_whole() {
        let f = fixture("uninstall-real");
        let settings = f.root.join("Roaming").join("Typescript");
        let packs = f.root.join("Local").join("Typescript").join("packs");
        touch(&settings.join("lexicon.json"));
        touch(&settings.join("anything-else.dat"));
        touch(&packs.join("en-1").join("manifest.json"));
        let env = Env {
            settings: settings.join("settings.json"),
            apps: settings.join("apps.json"),
            packs: packs.clone(),
        };
        let plan = plan(&env, None, false);
        assert!(plan.targets.contains(&settings), "{:?}", plan.targets);
        execute(&plan, |_| panic!("no installer")).unwrap();
        assert!(!settings.exists(), "the settings folder goes whole");
        assert!(!packs.exists());
    }

    #[test]
    fn a_folder_of_another_name_loses_only_the_programs_files() {
        let f = fixture("uninstall-other-name");
        let dir = f.root.join("my-stuff");
        touch(&dir.join("lexicon.json"));
        touch(&dir.join("photo.jpg"));
        let env = Env {
            settings: dir.join("settings.json"),
            apps: dir.join("apps.json"),
            packs: dir.join("packs"),
        };
        execute(&plan(&env, None, false), |_| panic!("no installer")).unwrap();
        assert!(dir.join("photo.jpg").exists());
        assert!(!dir.join("lexicon.json").exists());
    }

    #[cfg(windows)]
    #[test]
    fn an_installed_program_hands_the_removal_to_its_installer_and_waits() {
        let f = fixture("uninstall-installed");
        let exe_dir = f.root.join("Program Files").join("Typescript");
        touch(&exe_dir.join("install.ps1"));
        touch(&exe_dir.join("typescript_tip.dll"));
        let settings = f.root.join("Roaming").join("Typescript");
        touch(&settings.join("lexicon.json"));
        let env = Env {
            settings: settings.join("settings.json"),
            apps: settings.join("apps.json"),
            packs: f.root.join("Local").join("Typescript").join("packs"),
        };
        let planned = plan(&env, Some(&exe_dir), false);
        assert_eq!(planned.installer, Some(exe_dir.join("install.ps1")));
        let mut ran = String::new();
        let outcome = execute(&planned, |s| {
            ran = s.to_string();
            Ok(())
        })
        .unwrap();
        assert_eq!(outcome, Outcome::Started);
        assert!(
            ran.contains("-Uninstall") && ran.contains("if ($?)"),
            "{ran}"
        );
        assert!(settings.exists(), "the data waits for the installer");
        // A program that is not installed (a build folder) never starts an installer.
        assert_eq!(plan(&env, Some(&f.root), false).installer, None);
        // And a test home never does, even next to an installer.
        assert_eq!(plan(&env, Some(&exe_dir), true).installer, None);
    }

    /// Runs the real helper (PowerShell) against a stand-in installer that ends the way `ending`
    /// says, and waits for it. Returns once the data is gone or after ten seconds.
    #[cfg(windows)]
    fn run_helper(name: &str, ending: &str) -> (crate::testutil::Fixture, PathBuf, PathBuf) {
        let f = fixture(name);
        let installer = f.root.join("install.ps1");
        fs::write(
            &installer,
            format!(
                "param([switch]$Uninstall); Set-Content '{}' 'ran'; {ending}",
                f.root.join("ran.txt").display()
            ),
        )
        .unwrap();
        let data = f.root.join("data");
        touch(&data.join("lexicon.json"));
        let plan = Plan {
            installer: Some(installer),
            targets: vec![data.clone()],
        };
        assert_eq!(execute(&plan, launch).unwrap(), Outcome::Started);
        for _ in 0..100 {
            std::thread::sleep(std::time::Duration::from_millis(100));
            if !data.exists() {
                break;
            }
        }
        // The helper has had its say: give a failing one a moment more to (not) delete.
        std::thread::sleep(std::time::Duration::from_millis(800));
        let ran = f.root.join("ran.txt");
        // The folder is handed back with the paths: it is deleted when it is dropped.
        (f, data, ran)
    }

    #[cfg(windows)]
    #[test]
    fn the_helper_removes_the_data_after_an_installer_that_worked() {
        let (_kept, data, ran) = run_helper("uninstall-helper-ok", "");
        assert!(ran.exists(), "the installer was run with -Uninstall");
        assert!(!data.exists(), "the data goes after it");
    }

    #[cfg(windows)]
    #[test]
    fn the_helper_keeps_the_data_when_the_installer_failed() {
        let (_kept, data, ran) = run_helper("uninstall-helper-failed", "throw 'declined'");
        assert!(ran.exists());
        assert!(
            data.exists(),
            "a declined or failed removal must not take the data"
        );
    }

    #[test]
    fn a_quote_in_a_path_cannot_leave_its_string() {
        let text = script(
            Path::new("C:\\a'b\\install.ps1"),
            &[PathBuf::from("C:\\x'; calc; '")],
        );
        assert!(text.contains("'C:\\a''b\\install.ps1'"), "{text}");
        assert!(text.contains("'C:\\x''; calc; '''"), "{text}");
    }
}
