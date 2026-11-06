//! The X server and the emulator process, each in its own isolated environment.

use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

/// Mario Kart World's title ID: the emulator looks for LayeredFS mods in `load/<this>/<mod>/romfs`.
pub const TITLE_ID: &str = mkwtrack::banc::TITLE_ID;

/// Log line the emulator prints only when at least one LayeredFS mod was applied. (It prints
/// "Applied RomFS patches (LayeredFS)" for every loose-dump boot, mod or not.)
pub const LAYEREDFS_APPLIED: &str = "LayeredFS patches applied successfully";

pub struct Xvfb {
    child: Option<Child>,
    pub display: String,
}

impl Xvfb {
    /// Start `Xvfb :<n>` unless that display already exists (then it is reused and left running).
    pub fn start(bin: &str, n: u32, size: (u32, u32)) -> Result<Xvfb, String> {
        let display = format!(":{n}");
        let socket = PathBuf::from(format!("/tmp/.X11-unix/X{n}"));
        if socket.exists() {
            return Ok(Xvfb { child: None, display });
        }
        let child = Command::new(bin)
            .args([&display, "-screen", "0", &format!("{}x{}x24", size.0, size.1), "-nolisten", "tcp"])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| format!("could not start {bin}: {e} (install Xvfb or pass --xvfb)"))?;
        let t = Instant::now();
        while !socket.exists() {
            if t.elapsed() > Duration::from_secs(10) {
                return Err(format!("{bin} did not create {}", socket.display()));
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        std::thread::sleep(Duration::from_millis(300));
        Ok(Xvfb { child: Some(child), display })
    }
}

impl Drop for Xvfb {
    fn drop(&mut self) {
        if let Some(c) = &mut self.child {
            let _ = c.kill();
            let _ = c.wait();
        }
    }
}

/// An emulator profile: a private HOME so nothing touches the user's own emulator setup.
pub struct Profile {
    pub home: PathBuf,
}

impl Profile {
    /// A private profile in `home`. `template` is an existing emulator data folder (the one
    /// holding `nand/` and `sdmc/`) whose system and save data are copied in first, so the
    /// game starts as it does for a player rather than on a first launch.
    pub fn create(home: &Path, keys: &Path, template: Option<&Path>) -> Result<Profile, String> {
        let p = Profile { home: home.to_path_buf() };
        if let Some(t) = template {
            for sub in ["nand", "sdmc"] {
                if t.join(sub).is_dir() && !p.data().join(sub).exists() {
                    copy_dir(&t.join(sub), &p.data().join(sub)).map_err(|e| format!("{}: {e}", t.join(sub).display()))?;
                }
            }
        }
        let keys_dir = p.data().join("keys");
        std::fs::create_dir_all(&keys_dir).map_err(|e| e.to_string())?;
        std::fs::copy(keys, keys_dir.join("prod.keys")).map_err(|e| format!("{}: {e}", keys.display()))?;
        let cfg = home.join(".config/eden/sdl2-config.ini");
        std::fs::create_dir_all(cfg.parent().unwrap()).map_err(|e| e.to_string())?;
        // Only the mod folder is set; the emulator fills in its defaults (Vulkan; keyboard
        // profile for player 1: arrows = left stick, A = A, F/H = L/R).
        let ini = format!("[Data%20Storage]\nload_directory\\default=false\nload_directory={}\n", p.load_root().display());
        std::fs::write(&cfg, ini).map_err(|e| e.to_string())?;
        Ok(p)
    }

    pub fn data(&self) -> PathBuf {
        self.home.join(".local/share/eden")
    }

    pub fn load_root(&self) -> PathBuf {
        self.data().join("load")
    }

    /// Where mods for this game go (each in `<this>/<mod name>/romfs`).
    pub fn mods_root(&self) -> PathBuf {
        self.load_root().join(TITLE_ID)
    }
}

fn copy_dir(from: &Path, to: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(to)?;
    for e in std::fs::read_dir(from)? {
        let e = e?;
        let (src, dst) = (e.path(), to.join(e.file_name()));
        if e.file_type()?.is_dir() {
            copy_dir(&src, &dst)?;
        } else {
            std::fs::copy(&src, &dst)?;
        }
    }
    Ok(())
}

pub struct Emulator {
    child: Child,
    pub log: PathBuf,
    started: Instant,
}

pub struct EmulatorConfig<'a> {
    pub eden_cli: &'a Path,
    pub game: &'a Path,
    pub display: &'a str,
    pub memory_max: Option<&'a str>,
}

impl Emulator {
    pub fn start(cfg: &EmulatorConfig, profile: &Profile, log: &Path) -> Result<Emulator, String> {
        let out = std::fs::File::create(log).map_err(|e| e.to_string())?;
        let err = out.try_clone().map_err(|e| e.to_string())?;
        // a memory cap keeps a runaway guest from taking the desktop down with it
        let mut cmd = match cfg.memory_max {
            Some(max) => {
                let mut c = Command::new("systemd-run");
                c.args(["--user", "--scope", "-q", "-p", &format!("MemoryMax={max}"), "-p", "MemorySwapMax=16G"]).arg(cfg.eden_cli);
                c
            }
            None => Command::new(cfg.eden_cli),
        };
        cmd.arg("-g")
            .arg(cfg.game)
            .current_dir(cfg.eden_cli.parent().unwrap_or(Path::new(".")))
            .env("HOME", &profile.home)
            .env("DISPLAY", cfg.display)
            // Xvfb has no DRI3: present Vulkan frames through Mesa's CPU path
            .env("MESA_VK_WSI_DEBUG", "sw")
            .env("SDL_VIDEODRIVER", "x11")
            .env_remove("WAYLAND_DISPLAY")
            .env_remove("XDG_CONFIG_HOME")
            .env_remove("XDG_DATA_HOME")
            .env_remove("XDG_CACHE_HOME")
            .stdin(Stdio::null())
            .stdout(out)
            .stderr(err);
        let child = cmd.spawn().map_err(|e| format!("could not start {}: {e}", cfg.eden_cli.display()))?;
        Ok(Emulator { child, log: log.to_path_buf(), started: Instant::now() })
    }

    pub fn alive(&mut self) -> bool {
        matches!(self.child.try_wait(), Ok(None))
    }

    pub fn elapsed(&self) -> Duration {
        self.started.elapsed()
    }

    pub fn log_text(&self) -> String {
        std::fs::read(&self.log).map(|b| String::from_utf8_lossy(&b).into_owned()).unwrap_or_default()
    }

    /// Wait until the log contains `needle` (true) or the timeout passes or the emulator exits (false).
    pub fn wait_for_log(&mut self, needle: &str, timeout: Duration) -> bool {
        let t = Instant::now();
        while t.elapsed() < timeout {
            if self.log_text().contains(needle) {
                return true;
            }
            if !self.alive() {
                return false;
            }
            std::thread::sleep(Duration::from_millis(500));
        }
        false
    }

    /// The last lines that explain an exit: C++ exceptions, critical and error log lines.
    pub fn last_errors(&self) -> String {
        let log = self.log_text();
        let lines: Vec<&str> = log
            .lines()
            .filter(|l| {
                l.contains("terminate called")
                    || l.contains("what():")
                    || l.contains("<Critical>")
                    || l.contains("<Error>")
                    || l.contains("VK_ERROR")
            })
            .collect();
        lines[lines.len().saturating_sub(3)..].join(" | ")
    }

    pub fn stop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl Drop for Emulator {
    fn drop(&mut self) {
        self.stop();
    }
}
