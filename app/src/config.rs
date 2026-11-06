//! Paths the editor works with: saved in `~/.config/mkw-track-editor/config.json` and
//! overridable on the command line.

use serde_json::{Value, json};
use std::path::{Path, PathBuf};

pub const USAGE: &str = "\
usage: mkw-track-editor [options]

  --romfs DIR       the game's extracted romfs (with Banc/, RSDB/, System/)
  --models DIR      Switch Toolbox FBX exports of the game's models (output/Models)
  --mods DIR        where mods are saved (default ~/MKW-Mods)
  --ns2jb DIR       Switch 2 SD card (or a folder to copy onto it): export there on every save
  --open FILE       open a course at start, e.g. \"Banc/Race_FirstCircuit_A_00[Game03].bcett.byml.zs\"
  --select NAME     after opening, select the first object of that type, or PATH#N for point N
                    of the first path with that name
  -h, --help        show this help
  -V, --version     show the version

Paths given here are also used for this session only; set them for good in File > Settings.";

/// Qt command-line options that take a value (QGuiApplication handles them).
const QT_OPTIONS_WITH_VALUE: [&str; 11] = [
    "-platform",
    "-platformpluginpath",
    "-platformtheme",
    "-plugin",
    "-qmljsdebugger",
    "-style",
    "-stylesheet",
    "-display",
    "-geometry",
    "-qwindowgeometry",
    "-qwindowtitle",
];

#[derive(Clone, Debug, Default)]
pub struct Config {
    pub romfs: PathBuf,
    pub models: PathBuf,
    pub mods: PathBuf,
    /// Switch 2 SD card (or a folder copied to it) for ns2jb-modloader exports
    pub ns2jb: PathBuf,
    /// also export to `ns2jb` on every save
    pub ns2jb_on_save: bool,
    pub open: Option<String>,
    pub select: Option<String>,
}

fn home() -> PathBuf {
    std::env::var_os("HOME").map_or_else(|| PathBuf::from("."), PathBuf::from)
}

/// Text shown in the UI hides the home folder (and so the user name): "/home/<user>/x" -> "~/x".
pub fn tilde(s: &str) -> String {
    let h = home();
    let h = h.to_string_lossy();
    let h = h.trim_end_matches('/');
    if h.is_empty() { s.to_string() } else { s.replace(h, "~") }
}

/// A path for a shell command shown to the user: "$HOME/x" when it is under the home folder.
pub fn shell_path(p: &Path) -> String {
    match p.strip_prefix(home()) {
        Ok(rest) => format!("$HOME/{}", rest.display()),
        Err(_) => p.display().to_string(),
    }
}

/// A path typed into a settings field or given on the command line: `~` and `~/...` expand to
/// the home folder, relative paths are taken from the current folder, and empty stays empty.
pub fn expand(s: &str) -> PathBuf {
    let s = s.trim();
    if s.is_empty() {
        return PathBuf::new();
    }
    let p = match s.strip_prefix('~') {
        Some("") => home(),
        Some(rest) if rest.starts_with('/') => home().join(rest.trim_start_matches('/')),
        _ => PathBuf::from(s),
    };
    if p.is_absolute() { p } else { std::env::current_dir().unwrap_or_default().join(p) }
}

fn xdg(var: &str, fallback: &str) -> PathBuf {
    std::env::var_os(var).filter(|v| !v.is_empty()).map_or_else(|| home().join(fallback), PathBuf::from)
}

pub fn config_file() -> PathBuf {
    xdg("XDG_CONFIG_HOME", ".config").join("mkw-track-editor/config.json")
}

pub fn cache_dir() -> PathBuf {
    xdg("XDG_CACHE_HOME", ".cache").join("mkw-track-editor")
}

/// Eden's (and its forks') mod folder for this game.
pub fn emulator_load_dir() -> PathBuf {
    xdg("XDG_DATA_HOME", ".local/share").join(format!("eden/load/{}", mkwtrack::banc::TITLE_ID))
}

impl Config {
    /// The saved settings, then the command line on top. Exits on `--help`, `--version` or a
    /// bad command line.
    pub fn load() -> Config {
        let mut c = Self::saved();
        if let Err(e) = c.apply_args(std::env::args().skip(1)) {
            eprintln!("mkw-track-editor: {e}\n\n{USAGE}");
            std::process::exit(2);
        }
        c
    }

    fn saved() -> Config {
        let mut c = Config::default();
        let v: Option<Value> = std::fs::read_to_string(config_file()).ok().and_then(|t| serde_json::from_str(&t).ok());
        if let Some(v) = v {
            let p = |k: &str| v.get(k).and_then(|x| x.as_str()).map(PathBuf::from).unwrap_or_default();
            c.romfs = p("romfs");
            c.models = p("models");
            c.mods = p("mods");
            c.ns2jb = p("ns2jb");
            c.ns2jb_on_save = v.get("ns2jbOnSave").and_then(|x| x.as_bool()).unwrap_or(false);
        }
        if c.mods.as_os_str().is_empty() {
            c.mods = home().join("MKW-Mods");
        }
        c
    }

    fn apply_args(&mut self, args: impl IntoIterator<Item = String>) -> Result<(), String> {
        let mut args = args.into_iter();
        while let Some(a) = args.next() {
            let mut value = || args.next().ok_or_else(|| format!("{a} needs a value"));
            match a.as_str() {
                "--romfs" => self.romfs = expand(&value()?),
                "--models" => self.models = expand(&value()?),
                "--mods" => self.mods = expand(&value()?),
                "--ns2jb" => {
                    self.ns2jb = expand(&value()?);
                    self.ns2jb_on_save = true;
                }
                "--open" => self.open = Some(value()?),
                "--select" => self.select = Some(value()?),
                "-h" | "--help" => {
                    println!("{USAGE}");
                    std::process::exit(0);
                }
                "-V" | "--version" => {
                    println!("mkw-track-editor {}", env!("CARGO_PKG_VERSION"));
                    std::process::exit(0);
                }
                // Qt's own options (-platform xcb, -reverse...) are read by QGuiApplication
                s if s.starts_with('-') && !s.starts_with("--") => {
                    if QT_OPTIONS_WITH_VALUE.contains(&s) {
                        args.next();
                    }
                }
                s => return Err(format!("unknown option {s}")),
            }
        }
        Ok(())
    }

    pub fn save(&self) -> Result<(), String> {
        let f = config_file();
        let v =
            json!({"romfs": self.romfs, "models": self.models, "mods": self.mods, "ns2jb": self.ns2jb, "ns2jbOnSave": self.ns2jb_on_save});
        let text = serde_json::to_string_pretty(&v).map_err(|e| e.to_string())?;
        mkwtrack::io::write_atomic(&f, text.as_bytes()).map_err(|e| e.to_string())
    }

    pub fn valid(&self) -> Result<(), String> {
        if !self.romfs.join("Banc").is_dir() {
            return Err(format!("{} has no Banc folder: set the romfs folder in Settings", self.romfs.display()));
        }
        if !self.models.is_dir() {
            return Err(format!("{} is not a folder: set the exported models folder in Settings", self.models.display()));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expands_only_a_leading_tilde() {
        let h = home();
        assert_eq!(expand("~"), h);
        assert_eq!(expand("~/x/y"), h.join("x/y"));
        assert!(expand("a~b").ends_with("a~b"));
        assert_eq!(expand("  "), PathBuf::new());
        assert_eq!(expand("/abs"), PathBuf::from("/abs"));
    }

    #[test]
    fn arguments() {
        let mut c = Config::default();
        let args = ["--romfs", "/r", "-platform", "xcb", "--open", "Banc/X.bcett.byml.zs", "--ns2jb", "/sd", "-reverse"].map(String::from);
        c.apply_args(args).unwrap();
        assert_eq!((c.romfs.as_path(), c.open.as_deref(), c.ns2jb_on_save), (Path::new("/r"), Some("Banc/X.bcett.byml.zs"), true));
        assert!(c.apply_args(["--bogus".to_string()]).is_err());
        assert!(c.apply_args(["--mods".to_string()]).is_err());
    }
}
