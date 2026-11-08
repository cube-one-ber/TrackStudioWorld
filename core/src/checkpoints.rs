//! Lap checkpoints: which lap points are checkpoints, and what their numbers should be.
//!
//! A lap point is a checkpoint when its `Dynamic` has `CheckPoint`, a float. The numbers follow
//! one rule in every shipped course (data: 5,778 of the 5,779 `CheckPoint` values in the 288
//! files with a `LapPathGraph`; the other one is a hand-placed 5.1 between a 4 and a 6):
//!
//! * the lap graph runs along each path's points, round to the first point of a closed path,
//!   and through `BasicSignal` and `MessageLink` links between lap points (usually a path's last
//!   point to the next path's first point, sometimes into the middle of a path);
//! * a point with `IsStart`, `IsLapCheck` or `IsGoal` starts the count again;
//! * each checkpoint is one more than the highest count on any way into it, so branches number
//!   their checkpoints side by side and carry on from the longer branch where they meet.
//!
//! The editor renumbers after every edit that can change this, and when it saves. A stored value
//! whose whole part already matches is left alone, so shipped courses keep their numbers.

use crate::banc::hash_of;
use crate::byml::Byml;
use std::collections::HashMap;

/// The lap path type.
pub const LAP: &str = "LapPathGraph";
/// The checkpoint parameter of a lap point.
pub const KEY: &str = "CheckPoint";
/// Lap point flags that start the count again.
pub const MARKERS: [&str; 3] = ["IsStart", "IsLapCheck", "IsGoal"];
/// Link types the lap graph runs through.
const LINKS: [&str; 2] = ["BasicSignal", "MessageLink"];

fn dynamic<'a>(p: &'a Byml, key: &str) -> Option<&'a Byml> {
    p.get("Dynamic").and_then(|d| d.get(key))
}

/// Whether a lap point is a checkpoint.
pub fn is_checkpoint(p: &Byml) -> bool {
    dynamic(p, KEY).is_some()
}

/// Whether a lap point starts the count again (start, lap check or goal).
pub fn is_marker(p: &Byml) -> bool {
    MARKERS.iter().any(|k| dynamic(p, k).and_then(|b| b.as_bool()) == Some(true))
}

/// The number a checkpoint point should have, for each checkpoint of the course's lap paths.
/// Checkpoints on a loop with no start, lap check or goal point have no number and are left out.
pub fn numbers(rails: &[Byml]) -> HashMap<u64, u32> {
    let pts: Vec<&Byml> = rails.iter().filter(|r| r.str_or("Name", "") == LAP).flat_map(|r| r.array("Points")).collect();
    let at: HashMap<u64, usize> = pts.iter().enumerate().map(|(i, p)| (hash_of(p), i)).collect();
    let mut preds: Vec<Vec<usize>> = vec![vec![]; pts.len()];
    let mut i = 0;
    for r in rails.iter().filter(|r| r.str_or("Name", "") == LAP) {
        let n = r.array("Points").len();
        if n == 0 {
            continue;
        }
        for k in 1..n {
            preds[i + k].push(i + k - 1);
        }
        if r.get("IsClosed").and_then(|b| b.as_bool()) == Some(true) && n > 1 {
            preds[i].push(i + n - 1);
        }
        for (k, p) in r.array("Points").iter().enumerate() {
            for l in p.array("Links") {
                if LINKS.contains(&l.str_or("Name", ""))
                    && let Some(&d) = l.get("Dst").and_then(|d| d.as_u64()).and_then(|d| at.get(&d))
                {
                    preds[d].push(i + k);
                }
            }
        }
        i += n;
    }
    let cp: Vec<u32> = pts.iter().map(|p| is_checkpoint(p) as u32).collect();
    let reset: Vec<bool> = pts.iter().map(|p| is_marker(p)).collect();
    // the count after each point; a count can't pass the number of checkpoints unless it runs
    // round a loop with no marker, so that is where it stops
    let cap = cp.iter().sum::<u32>() + 1;
    let mut out = vec![0u32; pts.len()];
    loop {
        let mut changed = false;
        for i in 0..pts.len() {
            let before = if reset[i] { 0 } else { preds[i].iter().map(|&p| out[p]).max().unwrap_or(0) };
            let v = (before + cp[i]).min(cap);
            if v > out[i] {
                out[i] = v;
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    pts.iter().zip(out).filter(|(p, v)| is_checkpoint(p) && *v < cap).map(|(p, v)| (hash_of(p), v)).collect()
}

/// Whether a stored value counts as number `n` (its whole part is `n`).
fn matches(stored: Option<f64>, n: u32) -> bool {
    stored.is_some_and(|v| v.floor() == n as f64)
}

/// The checkpoints whose stored number differs from [`numbers`].
pub fn misnumbered(rails: &[Byml]) -> Vec<u64> {
    let want = numbers(rails);
    let mut out: Vec<u64> = rails
        .iter()
        .filter(|r| r.str_or("Name", "") == LAP)
        .flat_map(|r| r.array("Points"))
        .filter_map(|p| want.get(&hash_of(p)).filter(|n| !matches(dynamic(p, KEY).and_then(|v| v.as_f64()), **n)).map(|_| hash_of(p)))
        .collect();
    out.sort_unstable();
    out
}

/// Give every checkpoint of `root`'s lap paths its number. Returns how many changed.
pub fn renumber(root: &mut Byml) -> usize {
    if !root.array("Rails").iter().any(|r| r.str_or("Name", "") == LAP) {
        return 0;
    }
    let want = numbers(root.array("Rails"));
    let mut changed = 0;
    for r in root.array_mut("Rails").iter_mut().filter(|r| r.str_or("Name", "") == LAP) {
        for p in r.array_mut("Points").iter_mut() {
            let Some(&n) = want.get(&hash_of(p)) else { continue };
            if matches(dynamic(p, KEY).and_then(|v| v.as_f64()), n) {
                continue;
            }
            if let Some(d) = p.get_mut("Dynamic") {
                d.insert(KEY, Byml::F32(n as f32));
                changed += 1;
            }
        }
    }
    changed
}

#[cfg(test)]
mod tests {
    use super::*;

    fn point(h: u64, flags: &[(&str, Byml)]) -> Byml {
        let mut p = Byml::map();
        p.insert("Hash", Byml::U64(h));
        p.insert("Translation", Byml::floats(&[h as f32, 0.0, 0.0]));
        if !flags.is_empty() {
            let mut d = Byml::map();
            for (k, v) in flags {
                d.insert(k, v.clone());
            }
            p.insert("Dynamic", d);
        }
        p
    }

    fn cp() -> (&'static str, Byml) {
        (KEY, Byml::F32(0.0))
    }

    fn start() -> (&'static str, Byml) {
        ("IsStart", Byml::Bool(true))
    }

    fn lap(h: u64, points: Vec<Byml>, next: &[u64]) -> Byml {
        let mut points = points;
        let last = points.last_mut().unwrap();
        let src = hash_of(last);
        last.insert("Links", Byml::Array(next.iter().map(|d| crate::doc::link(src, *d, "BasicSignal")).collect()));
        let mut r = Byml::map();
        r.insert("Name", Byml::Str(LAP.into()));
        r.insert("Hash", Byml::U64(h));
        r.insert("Points", Byml::Array(points));
        r
    }

    fn values(root: &Byml) -> Vec<(u64, f64)> {
        let mut v: Vec<(u64, f64)> = root
            .array("Rails")
            .iter()
            .flat_map(|r| r.array("Points"))
            .filter_map(|p| dynamic(p, KEY).and_then(|v| v.as_f64()).map(|v| (hash_of(p), v)))
            .collect();
        v.sort_by_key(|x| x.0);
        v
    }

    #[test]
    fn numbers_along_branches_and_from_each_marker() {
        // start path 1 (cp 11, 12) splits into 2 (cp 21) and 3 (cp 31, 32), which meet in 4
        // (cp 41), whose lap check (42) restarts the count before cp 43; 4 links back to 1
        let mut root = Byml::map();
        root.insert(
            "Rails",
            Byml::Array(vec![
                lap(1, vec![point(10, &[start()]), point(11, &[cp()]), point(12, &[cp()])], &[20, 30]),
                lap(2, vec![point(20, &[]), point(21, &[cp()])], &[40]),
                lap(3, vec![point(30, &[cp()]), point(31, &[cp()]), point(32, &[])], &[40]),
                lap(4, vec![point(40, &[]), point(41, &[cp()]), point(42, &[("IsLapCheck", Byml::Bool(true))]), point(43, &[cp()])], &[10]),
            ]),
        );
        assert_eq!(misnumbered(root.array("Rails")).len(), 7);
        assert_eq!(renumber(&mut root), 7);
        assert_eq!(values(&root), vec![(11, 1.0), (12, 2.0), (21, 3.0), (30, 3.0), (31, 4.0), (41, 5.0), (43, 1.0)]);
        assert_eq!(renumber(&mut root), 0, "a second pass changes nothing");
    }

    #[test]
    fn keeps_values_with_the_right_whole_part() {
        let mut root = Byml::map();
        let mut r = lap(1, vec![point(10, &[start()]), point(11, &[(KEY, Byml::F32(1.5))]), point(12, &[(KEY, Byml::F32(7.0))])], &[]);
        r.insert("IsClosed", Byml::Bool(true));
        root.insert("Rails", Byml::Array(vec![r]));
        assert_eq!(misnumbered(root.array("Rails")), vec![12]);
        assert_eq!(renumber(&mut root), 1);
        assert_eq!(values(&root), vec![(11, 1.5), (12, 2.0)]);
    }

    #[test]
    fn a_loop_without_markers_is_left_alone() {
        let mut root = Byml::map();
        let mut r = lap(1, vec![point(10, &[(KEY, Byml::F32(4.0))]), point(11, &[cp()])], &[]);
        r.insert("IsClosed", Byml::Bool(true));
        root.insert("Rails", Byml::Array(vec![r]));
        assert!(numbers(root.array("Rails")).is_empty());
        assert_eq!(renumber(&mut root), 0);
    }
}
