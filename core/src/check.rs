//! Course checks for the Problems panel: what would stop a save, and what differs from how
//! every shipped course is built.
//!
//! Each warning describes something the shipped files never do (data: all 1,015 course files
//! of version 1.8.0, see `check_shipped_courses` in `tests/core.rs`), so a warning on a shipped
//! course is a bug in the check:
//!
//! * every `Race_*[Game03]` course has a `LapPathGraph`, a `RoutePathGraph`, exactly one lap
//!   point with `IsStart` and 25 `StartPosition` objects;
//! * every point of the path types in [`FORWARD_TYPES`] faces its next point;
//! * in races, every lap path either loops or its last point links on (the 12 that don't are
//!   in the one-way `InterStage` routes between courses);
//! * every parameter is stored with the type its `BancParam` table gives it;
//! * every race has lap checkpoints, every checkpoint is numbered as
//!   [`checkpoints`](crate::checkpoints) describes, and no checkpoint is also a start, lap
//!   check or goal point.
//!
//! Notes ([`Severity::Info`]) are things shipped files do now and then: links to hashes that
//! are not in the file, points whose `Parent` is not their path (674 shipped points), route
//! paths that end without a link, and objects stacked on one another.

use crate::banc::{course_base, hash_of, validate};
use crate::byml::Byml;
use crate::catalog::Catalog;
use crate::checkpoints;
use crate::math::quat_zyx;
use glam::Vec3;
use std::collections::{BTreeMap, HashMap};

/// Path types whose every shipped point faces the next point.
pub const FORWARD_TYPES: [&str; 10] = [
    "LapPathGraph",
    "RoutePathGraph",
    "GlidePath",
    "GravityPath",
    "GravityCameraPath",
    "OffroadPath",
    "PullPath",
    "RidePath",
    "WaterFlowPath",
    "WhirlpoolPath",
];

/// StartPosition objects in every shipped race.
pub const RACE_START_POSITIONS: usize = 25;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    /// blocks saving
    Error,
    /// unlike every shipped course
    Warning,
    /// unusual, but shipped courses do it too
    Info,
}

impl Severity {
    pub fn name(self) -> &'static str {
        match self {
            Severity::Error => "error",
            Severity::Warning => "warning",
            Severity::Info => "info",
        }
    }
}

#[derive(Clone, Debug)]
pub struct Problem {
    pub severity: Severity,
    pub message: String,
    /// what to select to see it: objects, paths or points
    pub items: Vec<u64>,
    /// a menu action that fixes it, when there is one
    pub fix: Option<&'static str>,
}

fn problem(severity: Severity, message: impl Into<String>, items: Vec<u64>) -> Problem {
    Problem { severity, message: message.into(), items, fix: None }
}

/// Whether a stored value has the type a parameter table gives (`Bool`, `S32`, `U32`, `F32`,
/// `String`, `Enum` stored as s32, `Vec3f`).
pub fn type_matches(kind: &str, v: &Byml) -> bool {
    match (kind, v) {
        ("Bool", Byml::Bool(_))
        | ("S32" | "Enum", Byml::I32(_))
        | ("U32", Byml::U32(_))
        | ("F32", Byml::F32(_))
        | ("String", Byml::Str(_)) => true,
        ("Vec3f", Byml::Array(a)) => a.len() == 3,
        ("Bool" | "S32" | "Enum" | "U32" | "F32" | "String" | "Vec3f", _) => false,
        // a kind the editor doesn't know: nothing to compare with
        _ => true,
    }
}

fn finite(e: &Byml, keys: &[&str]) -> bool {
    keys.iter().all(|k| match e.get(k) {
        Some(Byml::Array(a)) => a.iter().all(|x| x.as_f64().is_none_or(f64::is_finite)),
        _ => true,
    })
}

/// Everything worth telling the user about a course, errors first.
pub fn check(root: &Byml, file: &str, cat: Option<&Catalog>) -> Vec<Problem> {
    let mut out = vec![];
    let v = validate(root);
    // validate reports positions ("object #3"); point at the items where it can
    for e in v.errors {
        out.push(problem(Severity::Error, e, vec![]));
    }
    for w in v.warnings {
        let sev = if w.contains("which is not in this file") || w.contains("Parent is not") { Severity::Info } else { Severity::Warning };
        out.push(problem(sev, w, vec![]));
    }
    let actors = root.array("Actors");
    let rails = root.array("Rails");

    // numbers that would put things nowhere
    let bad: Vec<u64> = actors.iter().filter(|a| !finite(a, &["Translate", "Rotate", "Scale"])).map(hash_of).collect();
    let bad_pts: Vec<u64> = rails
        .iter()
        .flat_map(|r| r.array("Points"))
        .filter(|p| !finite(p, &["Translation", "Rotate", "Control0", "Control1", "RectSize", "RectOffset"]))
        .map(hash_of)
        .collect();
    if !bad.is_empty() || !bad_pts.is_empty() {
        out.push(problem(
            Severity::Error,
            format!("{} item(s) have a position, rotation or size that is not a number", bad.len() + bad_pts.len()),
            [bad, bad_pts].concat(),
        ));
    }

    // what every shipped race has
    let base = course_base(file);
    let race = base.starts_with("Race_") && file.contains("[Game03]");
    let of = |n: &str| rails.iter().filter(|r| r.str_or("Name", "") == n).count();
    if race {
        for (ty, what) in [
            ("LapPathGraph", "lap path (lap counting and respawning)"),
            ("RoutePathGraph", "route path (CPU drivers, items and steering assist)"),
        ] {
            if of(ty) == 0 {
                out.push(problem(Severity::Warning, format!("No {what}: every shipped race has a {ty}"), vec![]));
            }
        }
        let starts: Vec<u64> = rails
            .iter()
            .filter(|r| r.str_or("Name", "") == "LapPathGraph")
            .flat_map(|r| r.array("Points"))
            .filter(|p| p.get("Dynamic").and_then(|d| d.get("IsStart")).and_then(|b| b.as_bool()) == Some(true))
            .map(hash_of)
            .collect();
        if of("LapPathGraph") > 0 && starts.len() != 1 {
            out.push(problem(
                Severity::Warning,
                format!("{} lap points have IsStart set: every shipped race has exactly one, where the race starts", starts.len()),
                starts,
            ));
        }
        let grid: Vec<u64> = actors.iter().filter(|a| a.str_or("Name", "") == "StartPosition").map(hash_of).collect();
        if grid.len() != RACE_START_POSITIONS {
            out.push(problem(
                Severity::Warning,
                format!("{} StartPosition objects: every shipped race has {RACE_START_POSITIONS}, one per grid slot", grid.len()),
                grid,
            ));
        }
    }

    // path structure
    let mut route_ends = vec![];
    for r in rails {
        let name = r.str_or("Name", "");
        let pts = r.array("Points");
        let closed = r.get("IsClosed").and_then(|b| b.as_bool()).unwrap_or(false);
        if let Some(last) = pts.last()
            && !closed
            && last.array("Links").iter().all(|l| l.str_or("Name", "BasicSignal") != "BasicSignal")
        {
            match name {
                "LapPathGraph" if race => out.push(Problem {
                    fix: Some("connect"),
                    ..problem(
                        Severity::Warning,
                        "A lap path ends without linking on: the lap graph should be one connected loop (Paths → Connect)",
                        vec![hash_of(r), hash_of(last)],
                    )
                }),
                "RoutePathGraph" => route_ends.push(hash_of(last)),
                _ => {}
            }
        }
        if FORWARD_TYPES.contains(&name) && pts.len() > 1 {
            let back: Vec<u64> = pts
                .windows(2)
                .filter(|w| {
                    let (a, b) = (Vec3::from(w[0].vec3("Translation", [0.0; 3])), Vec3::from(w[1].vec3("Translation", [0.0; 3])));
                    let f = quat_zyx(w[0].vec3("Rotate", [0.0; 3])) * Vec3::Z;
                    (b - a).length() > 1e-3 && f.dot(b - a) < 0.0
                })
                .map(|w| hash_of(&w[0]))
                .collect();
            if !back.is_empty() {
                out.push(Problem {
                    fix: Some("orient"),
                    ..problem(
                        Severity::Warning,
                        format!("{} point(s) of a {name} face against the path: every shipped {name} point faces the next one (Paths → Orient Points Along Path)", back.len()),
                        back,
                    )
                });
            }
        }
    }
    out.extend(checkpoint_problems(rails, race));
    if !route_ends.is_empty() {
        out.push(problem(
            Severity::Info,
            format!("{} route path(s) end without linking on (about 1% of shipped route paths do)", route_ends.len()),
            route_ends,
        ));
    }

    // parameter types
    if let Some(cat) = cat {
        let mut wrong: BTreeMap<String, Vec<u64>> = BTreeMap::new();
        let mut look = |e: &Byml, table: &str| {
            let Some(m) = e.get("Dynamic").and_then(|d| d.as_map()) else { return };
            let Some(schema) = cat.data.params.get(table) else { return };
            for (k, v) in m {
                if let Some(p) = schema.iter().find(|p| &p.name == k)
                    && !type_matches(&p.kind, v)
                {
                    wrong.entry(format!("{k} (should be {})", p.kind)).or_default().push(hash_of(e));
                }
            }
        };
        for a in actors {
            if let Some(info) = cat.actor(a.str_or("Gyaml", "")) {
                look(a, &info.banc_param);
            }
        }
        for r in rails {
            if let Some(def) = cat.data.rails.get(r.str_or("Name", "")) {
                look(r, &def.rail_param);
                for p in r.array("Points") {
                    look(p, &def.point_param);
                }
            }
        }
        for (k, items) in wrong {
            out.push(problem(
                Severity::Warning,
                format!("{} item(s) store {k} with another type than its parameter table", items.len()),
                items,
            ));
        }
    }

    // the same object twice in one place (a duplicate or paste left behind)
    let mut at: HashMap<(&str, [i64; 3]), Vec<u64>> = HashMap::new();
    for a in actors {
        let t = a.vec3("Translate", [0.0; 3]).map(|x| (x * 100.0).round() as i64);
        at.entry((a.str_or("Name", ""), t)).or_default().push(hash_of(a));
    }
    let mut stacked: Vec<u64> = at.into_values().filter(|v| v.len() > 1).flat_map(|v| v.into_iter().skip(1)).collect();
    if !stacked.is_empty() {
        stacked.sort_unstable();
        out.push(problem(Severity::Info, format!("{} object(s) sit exactly where another of the same type is", stacked.len()), stacked));
    }

    out.sort_by_key(|p| p.severity);
    out
}

/// Lap checkpoints: there, numbered in order along the lap, and not on a marker point.
fn checkpoint_problems(rails: &[Byml], race: bool) -> Vec<Problem> {
    let mut out = vec![];
    let lap: Vec<&Byml> = rails.iter().filter(|r| r.str_or("Name", "") == checkpoints::LAP).flat_map(|r| r.array("Points")).collect();
    let cps: Vec<&Byml> = lap.iter().copied().filter(|p| checkpoints::is_checkpoint(p)).collect();
    if race && !lap.is_empty() && cps.is_empty() {
        out.push(problem(
            Severity::Warning,
            "No lap point is a checkpoint: every shipped race has some along the lap (select lap points and tick Checkpoint)",
            vec![],
        ));
    }
    let numbered = checkpoints::numbers(rails);
    let loose: Vec<u64> = cps.iter().map(|p| hash_of(p)).filter(|h| !numbered.contains_key(h)).collect();
    if !loose.is_empty() {
        out.push(problem(
            Severity::Warning,
            format!(
                "{} checkpoint(s) are on a lap loop with no start, lap check or goal point, so they can't be numbered: every shipped loop has one",
                loose.len()
            ),
            loose,
        ));
    }
    let wrong = checkpoints::misnumbered(rails);
    if !wrong.is_empty() {
        out.push(Problem {
            fix: Some("renumber"),
            ..problem(
                Severity::Warning,
                format!("{} checkpoint(s) are numbered out of order: shipped checkpoints count up from 1 after each start, lap check and goal point", wrong.len()),
                wrong,
            )
        });
    }
    let both: Vec<u64> = cps.iter().filter(|p| checkpoints::is_marker(p)).map(|p| hash_of(p)).collect();
    if !both.is_empty() {
        out.push(problem(
            Severity::Warning,
            format!("{} checkpoint(s) are also a start, lap check or goal point: no shipped point is both", both.len()),
            both,
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::banc::tests::{actor, course, rail};

    fn race() -> Byml {
        let mut d = course();
        let mut actors = vec![];
        for i in 0..RACE_START_POSITIONS as u64 {
            actors.push(actor("StartPosition", 100 + i, [i as f32, 0.0, 0.0]));
        }
        d.insert("Actors", Byml::Array(actors));
        let mut lap = rail("LapPathGraph", 20, &[(21, [0.0; 3]), (22, [10.0, 0.0, 0.0])]);
        let mut start = Byml::map();
        start.insert("IsStart", Byml::Bool(true));
        lap.array_mut("Points")[0].insert("Dynamic", start);
        let mut cp = Byml::map();
        cp.insert("CheckPoint", Byml::F32(1.0));
        lap.array_mut("Points")[1].insert("Dynamic", cp);
        lap.insert("IsClosed", Byml::Bool(true));
        // facing +X, along the paths
        for r in [&mut lap] {
            for p in r.array_mut("Points") {
                p.insert("Rotate", Byml::floats(&[0.0, std::f32::consts::FRAC_PI_2, 0.0]));
            }
        }
        let mut route = d.array("Rails")[0].clone();
        for p in route.array_mut("Points") {
            p.insert("Rotate", Byml::floats(&[0.0, std::f32::consts::FRAC_PI_2, 0.0]));
        }
        route.insert("IsClosed", Byml::Bool(true));
        d.insert("Rails", Byml::Array(vec![route, lap]));
        d
    }

    const RACE: &str = "Banc/Race_Test_00[Game03].bcett.byml.zs";

    #[test]
    fn a_complete_race_has_no_problems() {
        let p = check(&race(), RACE, None);
        assert!(p.is_empty(), "{p:?}");
    }

    #[test]
    fn finds_what_races_need() {
        let mut d = race();
        d.array_mut("Actors").pop();
        d.array_mut("Rails").remove(1);
        let p = check(&d, RACE, None);
        let msgs: Vec<&str> = p.iter().map(|p| p.message.as_str()).collect();
        assert!(msgs.iter().any(|m| m.starts_with("No lap path")), "{msgs:?}");
        assert!(msgs.iter().any(|m| m.starts_with("24 StartPosition")), "{msgs:?}");
        // other courses (battles, missions) don't need them
        assert!(check(&d, "Banc/BalloonBattle_X[Game03].bcett.byml.zs", None).is_empty());
    }

    #[test]
    fn finds_checkpoint_problems() {
        let msgs = |d: &Byml| check(d, RACE, None).into_iter().map(|p| (p.message, p.fix)).collect::<Vec<_>>();
        let mut d = race();
        let pts = d.array_mut("Rails")[1].array_mut("Points");
        pts[1].get_mut("Dynamic").unwrap().insert("CheckPoint", Byml::F32(3.0));
        let m = msgs(&d);
        assert!(m.iter().any(|(m, f)| m.starts_with("1 checkpoint(s) are numbered out of order") && *f == Some("renumber")), "{m:?}");
        // a checkpoint on the start point, and then no start at all
        let pts = d.array_mut("Rails")[1].array_mut("Points");
        pts[0].get_mut("Dynamic").unwrap().insert("CheckPoint", Byml::F32(1.0));
        pts[1].remove("Dynamic");
        assert!(msgs(&d).iter().any(|(m, _)| m.starts_with("1 checkpoint(s) are also a start")), "{:?}", msgs(&d));
        let pts = d.array_mut("Rails")[1].array_mut("Points");
        pts[0].get_mut("Dynamic").unwrap().remove("IsStart");
        assert!(msgs(&d).iter().any(|(m, _)| m.starts_with("1 checkpoint(s) are on a lap loop with no start")), "{:?}", msgs(&d));
        let pts = d.array_mut("Rails")[1].array_mut("Points");
        pts[0].remove("Dynamic");
        assert!(msgs(&d).iter().any(|(m, _)| m.starts_with("No lap point is a checkpoint")), "{:?}", msgs(&d));
    }

    #[test]
    fn finds_backward_points_and_open_lap_ends() {
        let mut d = race();
        let lap = &mut d.array_mut("Rails")[1];
        lap.insert("IsClosed", Byml::Bool(false));
        lap.array_mut("Points")[0].insert("Rotate", Byml::floats(&[0.0, -1.5, 0.0]));
        let p = check(&d, RACE, None);
        let back = p.iter().find(|p| p.fix == Some("orient")).expect("backward point");
        assert_eq!((back.severity, back.items.as_slice()), (Severity::Warning, &[21][..]));
        assert!(p.iter().any(|p| p.fix == Some("connect") && p.items.contains(&22)));
    }

    #[test]
    fn errors_come_first() {
        let mut d = race();
        d.array_mut("Actors").push(actor("CmnObj_Coin000_A_00", 21, [f32::NAN, 0.0, 0.0]));
        let p = check(&d, RACE, None);
        assert_eq!(p[0].severity, Severity::Error);
        assert!(p.iter().any(|p| p.message.contains("duplicate Hash 21")));
        assert!(p.iter().any(|p| p.severity == Severity::Error && p.message.contains("not a number") && p.items == [21]));
    }

    #[test]
    fn stacked_objects() {
        let mut d = race();
        d.array_mut("Actors").push(actor("StartPosition", 999, [3.0, 0.0, 0.0]));
        let p = check(&d, RACE, None);
        assert!(p.iter().any(|p| p.severity == Severity::Info && p.items == [999]), "{p:?}");
    }

    #[test]
    fn parameter_types() {
        assert!(type_matches("Enum", &Byml::I32(2)) && !type_matches("F32", &Byml::I32(2)) && type_matches("Struct", &Byml::map()));
    }
}
