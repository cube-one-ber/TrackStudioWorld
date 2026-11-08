//! In-game tests for mkwtrack mods.
//!
//! For each scenario this writes a mod with the editor's own save path, boots the game in an
//! Eden-based emulator (PokeDinner/Eglantine) on a private Xvfb display with a private emulator
//! profile, drives the menus into Grand Prix race 1 (Mario Bros. Circuit), holds accelerate,
//! and records screenshots, the emulator log and a JSON report. Checks that can be decided
//! automatically fail the run; the contact sheets are for a person to compare with the
//! baseline.
//!
//! You supply your own game dump, keys and emulator; nothing from the game ships with this tool.

mod emu;
mod frames;
mod scenarios;
mod x11;

use emu::{Emulator, EmulatorConfig, LAYEREDFS_APPLIED, Profile, Xvfb};
use frames::{contact_sheet, diff, mean, white_fraction};
use image::RgbImage;
use mkwtrack::catalog::Catalog;
use mkwtrack::doc::Document;
use mkwtrack::raycast::Bvh;
use mkwtrack::scene::stages_for;
use mkwtrack::workspace::Workspace;
use scenarios::{COURSE, Scenario};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

const USAGE: &str = "\
mkw-emutest: boot Mario Kart World with editor-written mods and record what happens

usage: mkw-emutest --dump DIR --eden EDEN_CLI --keys PROD_KEYS --out DIR [options] [scenario...]

  --dump DIR         game dump with exefs/ (main, main.npdm...) and romfs/ side by side  [MKW_DUMP]
  --eden FILE        the emulator's command-line binary (eden-cli), e.g. from an extracted
                     PokeDinner AppImage: squashfs-root/usr/bin/eden-cli               [MKW_EDEN]
  --keys FILE        prod.keys dumped from your console                                 [MKW_KEYS]
  --profile-template DIR
                     an emulator data folder (with nand/ and sdmc/) to copy system and save
                     data from, e.g. ~/.local/share/eden after the game has been played once
  --out DIR          where profiles, screenshots and reports go
  --xvfb FILE        X server to run the emulator on (default: Xvfb)
  --display N        X display number (default: 97)
  --memory-max SIZE  memory cap for the emulator through systemd-run (default 6G, 'none' to disable)
  --title-timeout S  seconds to wait for the title screen (default 900; a first boot compiles
                     shaders for wherever the save last was, which can take minutes)
  --race-wait S      seconds to wait for the race HUD after the last menu press (default 420)
  --drive S          seconds to hold accelerate (default 90)
  --mods-only        write each scenario's mod to <out>/<scenario>/mod and stop (no emulator)
  --classify PNG...  print what the title and race detectors see in saved screenshots

scenarios: baseline visible stress (default: all three)";

struct Opts {
    dump: PathBuf,
    eden: PathBuf,
    keys: PathBuf,
    profile_template: Option<PathBuf>,
    out: PathBuf,
    xvfb: String,
    display: u32,
    memory_max: Option<String>,
    title_timeout: u64,
    race_wait: u64,
    drive: u64,
    mods_only: bool,
    scenarios: Vec<Scenario>,
}

fn parse_args() -> mkwtrack::Result<Opts> {
    let env = |k: &str| std::env::var_os(k).map(PathBuf::from);
    let mut o = Opts {
        dump: env("MKW_DUMP").unwrap_or_default(),
        eden: env("MKW_EDEN").unwrap_or_default(),
        keys: env("MKW_KEYS").unwrap_or_default(),
        profile_template: None,
        out: PathBuf::new(),
        xvfb: "Xvfb".into(),
        display: 97,
        memory_max: Some("6G".into()),
        title_timeout: 900,
        race_wait: 420,
        drive: 90,
        mods_only: false,
        scenarios: vec![],
    };
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        let mut val = || args.next().ok_or_else(|| format!("{a} needs a value"));
        let num = |v: String| v.parse::<u64>().map_err(|_| format!("{v} is not a number"));
        match a.as_str() {
            "--dump" => o.dump = val()?.into(),
            "--eden" => o.eden = val()?.into(),
            "--keys" => o.keys = val()?.into(),
            "--profile-template" => o.profile_template = Some(val()?.into()),
            "--out" => o.out = val()?.into(),
            // models come from the romfs now; accepted so older command lines still work
            "--models" => {
                val()?;
            }
            "--xvfb" => o.xvfb = val()?,
            "--display" => o.display = num(val()?)? as u32,
            "--memory-max" => o.memory_max = Some(val()?).filter(|v| v != "none"),
            "--title-timeout" => o.title_timeout = num(val()?)?,
            "--race-wait" => o.race_wait = num(val()?)?,
            "--drive" => o.drive = num(val()?)?,
            "--mods-only" => o.mods_only = true,
            "-h" | "--help" => {
                println!("{USAGE}");
                std::process::exit(0);
            }
            s if s.starts_with('-') => return Err(format!("unknown option {s}").into()),
            s => o.scenarios.push(Scenario::parse(s).ok_or_else(|| format!("unknown scenario {s}"))?),
        }
    }
    if o.scenarios.is_empty() {
        o.scenarios = Scenario::ALL.to_vec();
    }
    let needed: &[(&str, &PathBuf)] = if o.mods_only {
        &[("--dump", &o.dump), ("--out", &o.out)]
    } else {
        &[("--dump", &o.dump), ("--eden", &o.eden), ("--keys", &o.keys), ("--out", &o.out)]
    };
    for (what, p) in needed {
        if p.as_os_str().is_empty() {
            return Err(format!("{what} is required\n\n{USAGE}").into());
        }
    }
    for (what, p) in needed.iter().filter(|(w, _)| *w != "--out") {
        if !p.exists() {
            return Err(format!("{what} {}: not found", p.display()).into());
        }
    }
    if !o.dump.join("exefs/main").is_file() || !o.dump.join("romfs/Banc").is_dir() {
        return Err(format!("{} needs exefs/main and romfs/Banc", o.dump.display()).into());
    }
    // the emulator runs with another HOME and working directory, so every path must be absolute
    let abs = |p: &Path| std::path::absolute(p).unwrap_or_else(|_| p.to_path_buf());
    o.dump = abs(&o.dump);
    o.eden = abs(&o.eden);
    o.keys = abs(&o.keys);
    o.out = abs(&o.out);
    o.profile_template = o.profile_template.as_deref().map(abs);
    Ok(o)
}

fn sleep(secs: f32) {
    std::thread::sleep(Duration::from_secs_f32(secs));
}

fn say(msg: impl AsRef<str>) {
    eprintln!("[emutest] {}", msg.as_ref());
}

/// Screen size the emulator window fills (it opens at 1280x720, centred).
const SCREEN: (u32, u32) = (1280, 720);
/// The title screen's "Press L + R to start" prompt: about 45% of this band is white text, and
/// the band below it is nearly free of white (bright clouds during the intro fill both).
fn title_prompt(img: &RgbImage) -> bool {
    white_fraction(img, (440, 658, 400, 20)) > 0.3 && white_fraction(img, (440, 684, 400, 24)) < 0.05
}

/// The race HUD (bottom left): a gold coin icon, the coin count and the lap counter ("1/3"),
/// all three at once. Every race frame seen measures about 0.43 gold in the icon box, 0.23
/// white in the count box and 0.09 white in the lap box; menus, free roam and the course
/// previews before a Grand Prix don't come close to all three.
fn race_hud(img: &RgbImage) -> bool {
    let coin = frames::fraction(img, (42, 652, 36, 36), |p| p[0] > 190 && p[1] > 127 && p[2] < 90);
    let count = white_fraction(img, (80, 650, 60, 40));
    let lap = white_fraction(img, (150, 650, 110, 40));
    (0.3..0.6).contains(&coin) && (0.15..0.32).contains(&count) && (0.05..0.2).contains(&lap)
}

struct Run<'a> {
    o: &'a Opts,
    dir: PathBuf,
    shots: Vec<(String, RgbImage)>,
}

impl Run<'_> {
    fn shot(&mut self, x: &x11::Display, name: &str) -> Option<RgbImage> {
        match x.screenshot() {
            Ok(img) => {
                let _ = img.save(self.dir.join(format!("{name}.png")));
                self.shots.push((name.to_string(), img.clone()));
                Some(img)
            }
            Err(e) => {
                say(format!("screenshot {name}: {e}"));
                None
            }
        }
    }
}

fn run_scenario(o: &Opts, sc: Scenario, profile: &Profile, cat: &Catalog, ground: Option<&Bvh>, xvfb: &Xvfb) -> mkwtrack::Result<Value> {
    let dir = o.out.join(sc.name());
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    // every scenario starts from an empty mod folder; the profile (and its shader caches) is shared
    let _ = std::fs::remove_dir_all(profile.mods_root());
    std::fs::create_dir_all(profile.mods_root()).map_err(|e| e.to_string())?;
    let ws = Workspace { romfs: o.dump.join("romfs"), mods_root: profile.mods_root() };
    let edits = sc.build(&ws, cat, ground)?;
    say(format!("{}: mod written: {}", sc.name(), edits.as_ref().map(|e| e["edits"].to_string()).unwrap_or("none".into())));

    let cfg =
        EmulatorConfig { eden_cli: &o.eden, game: &o.dump.join("exefs/main"), display: &xvfb.display, memory_max: o.memory_max.as_deref() };
    let mut emu = Emulator::start(&cfg, profile, &dir.join("emulator.log"))?;
    let x = x11::Display::connect(&xvfb.display)?;
    let mut run = Run { o, dir: dir.clone(), shots: vec![] };

    // 1. RomFS: the emulator says whether a LayeredFS mod went in
    let romfs_ready = emu.wait_for_log("Loose RomFS image ready", Duration::from_secs(120));
    sleep(3.0);
    let mod_applied = emu.log_text().contains(LAYEREDFS_APPLIED);
    say(format!("{}: RomFS ready {romfs_ready}, LayeredFS mod applied {mod_applied}", sc.name()));

    // 2. title screen: wait for the "Press L + R to start" prompt to hold steady
    let mut title_at = None;
    let mut seen = 0;
    while emu.elapsed() < Duration::from_secs(o.title_timeout) && emu.alive() {
        sleep(3.0);
        let Ok(img) = x.screenshot() else { continue };
        seen = if title_prompt(&img) { seen + 1 } else { 0 };
        if seen >= 3 {
            title_at = Some(emu.elapsed().as_secs());
            break;
        }
    }
    say(format!("{}: title screen {}", sc.name(), title_at.map(|t| format!("at {t} s")).unwrap_or("not detected".into())));
    if !emu.alive() {
        return Err(format!("{BEFORE_TITLE} after {} s: {}", emu.elapsed().as_secs(), emu.last_errors()).into());
    }
    run.shot(&x, "00-title");

    // 3. menus: L+R (again if the prompt stays), then A through Single Player, Grand Prix,
    //    Mario, Standard Kart, Mushroom Cup, 50cc and OK
    x.pointer((SCREEN.0 / 2) as i16, (SCREEN.1 / 2) as i16);
    sleep(0.5);
    for _ in 0..3 {
        x.hold("f+h", 0.5)?;
        sleep(4.0);
        if !x.screenshot().is_ok_and(|img| title_prompt(&img)) {
            break;
        }
    }
    sleep(10.0);
    run.shot(&x, "01-after-lr");
    for i in 0..7 {
        x.hold("a", 0.4)?;
        sleep(12.0);
        run.shot(&x, &format!("{:02}-menu", i + 2));
    }

    if !emu.alive() {
        return Err(format!("emulator exited in the menus after {} s: {}", emu.elapsed().as_secs(), emu.last_errors()).into());
    }

    // 4. the race loads: wait for its HUD. A tap of A every 20 s recovers a menu press the game
    //    dropped (a screen sometimes needs two) and skips the course previews; it does nothing
    //    while a loading screen is up.
    let mut race_at = None;
    let mut hud = 0;
    let mut waited = 0;
    let mut taps = 0;
    while waited < o.race_wait && emu.alive() {
        sleep(3.0);
        waited += 3;
        let Ok(img) = x.screenshot() else { continue };
        hud = if race_hud(&img) { hud + 1 } else { 0 };
        if waited % 30 == 0 {
            run.shot(&x, &format!("10-wait-{waited:03}"));
        }
        if hud == 0 && waited % 21 == 0 {
            x.hold("a", 0.3)?;
            taps += 1;
        }
        if hud >= 2 {
            race_at = Some(emu.elapsed().as_secs());
            break;
        }
    }
    say(format!(
        "{}: race {} ({taps} extra A taps)",
        sc.name(),
        race_at.map(|t| format!("on screen at {t} s")).unwrap_or("not detected".into())
    ));

    // 5. hold accelerate and record
    x.press("a")?;
    let mut drive = vec![];
    let n = (o.drive / 2).max(1);
    for i in 0..n {
        sleep(2.0);
        if let Some(img) = run.shot(&x, &format!("20-drive-{:03}", (i + 1) * 2)) {
            drive.push(img);
        }
        if !emu.alive() {
            break;
        }
    }
    x.release("a")?;
    let alive = emu.alive();
    let elapsed = emu.elapsed().as_secs();
    emu.stop();

    // 6. checks
    let log = emu.log_text();
    let critical: Vec<String> =
        log.lines().filter(|l| l.contains("<Critical>")).map(|l| strip_ansi(l).chars().take(240).collect()).collect();
    let motion: Vec<f32> = drive.windows(2).map(|w| diff(&w[0], &w[1])).collect();
    let mean_motion = if motion.is_empty() { 0.0 } else { motion.iter().sum::<f32>() / motion.len() as f32 };
    let dark = drive.iter().filter(|f| mean(f) < 12.0).count();
    let hud_frames = drive.iter().filter(|f| race_hud(f)).count();
    let want_mod = sc != Scenario::Baseline;
    let checks = json!({
        "mod_applied_as_expected": mod_applied == want_mod,
        "title_screen_reached": title_at.is_some(),
        "race_reached": race_at.is_some(),
        "race_hud_while_driving": hud_frames * 2 >= drive.len(),
        "emulator_alive_after_driving": alive,
        "race_frames_moving": mean_motion > 1.5 && dark < drive.len() / 2,
    });
    let pass = checks.as_object().unwrap().values().all(|v| v == true);
    let report = json!({
        "scenario": sc.name(), "pass": pass, "checks": checks, "mod": edits,
        "layeredfs_applied": mod_applied, "title_at_s": title_at, "race_at_s": race_at, "extra_a_taps": taps, "run_s": elapsed,
        "drive_hud_frames": hud_frames,
        "drive_motion_mean": mean_motion, "drive_dark_frames": dark, "drive_frames": drive.len(),
        "critical_log_lines": critical,
    });
    std::fs::write(dir.join("report.json"), serde_json::to_string_pretty(&report).unwrap()).map_err(|e| e.to_string())?;
    let menus: Vec<RgbImage> = run.shots.iter().filter(|(n, _)| !n.starts_with("20-")).map(|x| x.1.clone()).collect();
    contact_sheet(&menus, 4, 320).save(dir.join("sheet-menus.png")).map_err(|e| e.to_string())?;
    contact_sheet(&drive, 5, 320).save(dir.join("sheet-drive.png")).map_err(|e| e.to_string())?;
    let _ = run.o;
    Ok(report)
}

/// Error prefix for an emulator that died before any course was loaded (not the mod's fault,
/// so the scenario is tried again).
const BEFORE_TITLE: &str = "emulator exited before the title screen";

fn strip_ansi(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c == '\x1b' {
            for c in chars.by_ref() {
                if c.is_ascii_alphabetic() {
                    break;
                }
            }
        } else {
            out.push(c);
        }
    }
    out
}

fn ground_for_course(cat: &Catalog, ws: &Workspace) -> Option<Bvh> {
    let doc = Document::new(ws.load(COURSE, None).ok()?);
    let stages: Vec<_> = stages_for(cat, &doc.root, 12).into_iter().cloned().collect();
    let bvh = mkwtrack::scene::stage_bvh(&stages, |rel| mkwtrack::bfres::load(&ws.romfs, rel).ok().map(Arc::new));
    (!bvh.is_empty()).then_some(bvh)
}

fn main() {
    // --classify FILE...: what the screen detectors see in saved 1280x720 screenshots
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().is_some_and(|a| a == "--classify") {
        for f in &args[1..] {
            match image::open(f) {
                Ok(img) => {
                    let img = img.to_rgb8();
                    println!("{f}: title prompt {}, race HUD {}", title_prompt(&img), race_hud(&img));
                }
                Err(e) => println!("{f}: {e}"),
            }
        }
        return;
    }
    let o = match parse_args() {
        Ok(o) => o,
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(2);
        }
    };
    let result = (|| -> mkwtrack::Result<bool> {
        std::fs::create_dir_all(&o.out).map_err(|e| e.to_string())?;
        let romfs = o.dump.join("romfs");
        say("reading the game catalog");
        let cat = Catalog::load(&romfs, &o.out.join("catalog-cache"), &|m| say(m))?;
        let ground = ground_for_course(&cat, &Workspace { romfs: romfs.clone(), mods_root: o.out.join("unused") });
        say(format!(
            "stage surface: {}",
            ground.as_ref().map(|g| format!("{} triangles", g.len())).unwrap_or("none (the stage models did not load)".into())
        ));
        if o.mods_only {
            for &sc in &o.scenarios {
                let ws = Workspace { romfs: romfs.clone(), mods_root: o.out.join(sc.name()).join("mod") };
                let _ = std::fs::remove_dir_all(&ws.mods_root);
                let r = sc.build(&ws, &cat, ground.as_ref())?;
                say(format!("{}: {}", sc.name(), r.map(|r| serde_json::to_string_pretty(&r).unwrap()).unwrap_or("no mod".into())));
            }
            return Ok(true);
        }
        let profile = Profile::create(&o.out.join("home"), &o.keys, o.profile_template.as_deref())?;
        let xvfb = Xvfb::start(&o.xvfb, o.display, SCREEN)?;
        let mut all = vec![];
        let mut ok = true;
        for &sc in &o.scenarios {
            say(format!("=== {}", sc.name()));
            let mut result = run_scenario(&o, sc, &profile, &cat, ground.as_ref(), &xvfb);
            if let Err(e) = &result
                && e.message().starts_with(BEFORE_TITLE)
            {
                say(format!("{}: {e}; trying once more", sc.name()));
                result = run_scenario(&o, sc, &profile, &cat, ground.as_ref(), &xvfb);
            }
            match result {
                Ok(r) => {
                    ok &= r["pass"] == true;
                    say(format!("{}: {} {}", sc.name(), if r["pass"] == true { "PASS" } else { "FAIL" }, r["checks"]));
                    all.push(r);
                }
                Err(e) => {
                    ok = false;
                    say(format!("{}: ERROR {e}", sc.name()));
                    all.push(json!({"scenario": sc.name(), "pass": false, "error": e.to_string()}));
                }
            }
        }
        std::fs::write(o.out.join("summary.json"), serde_json::to_string_pretty(&all).unwrap()).map_err(|e| e.to_string())?;
        Ok(ok)
    })();
    match result {
        Ok(true) => say("all scenarios passed"),
        Ok(false) => {
            say("some scenarios failed; see summary.json");
            std::process::exit(1);
        }
        Err(e) => {
            say(format!("error: {e}"));
            std::process::exit(2);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_log_colours() {
        assert_eq!(strip_ansi("\x1b[1;31m[ 1.0] Core <Critical> x\x1b[0m"), "[ 1.0] Core <Critical> x");
    }

    #[test]
    fn scenario_names_round_trip() {
        for sc in Scenario::ALL {
            assert_eq!(Scenario::parse(sc.name()), Some(sc));
        }
        assert_eq!(Scenario::parse("nope"), None);
    }

    #[test]
    fn keys_map_to_keysyms() {
        assert_eq!(x11::keysym("a"), Some(0x61));
        assert_eq!(x11::keysym("F"), Some(0x66));
        assert_eq!(x11::keysym("down"), Some(0xff54));
        assert_eq!(x11::keysym("nope"), None);
    }

    #[test]
    fn detectors_reject_blank_frames() {
        let black = RgbImage::new(SCREEN.0, SCREEN.1);
        let white = RgbImage::from_pixel(SCREEN.0, SCREEN.1, image::Rgb([255, 255, 255]));
        for img in [&black, &white] {
            assert!(!title_prompt(img) && !race_hud(img));
        }
    }
}
