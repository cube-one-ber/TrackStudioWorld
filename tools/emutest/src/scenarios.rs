//! Test mods, each written through the editor's own save path (`Workspace::save`).
//!
//! All of them edit Mario Bros. Circuit, the first race of the Mushroom Cup, because the menu
//! script lands there. The player starts last, from `StartPosition` 23 in a 50cc Grand Prix.

use glam::Vec3;
use mkwtrack::banc::hash_of;
use mkwtrack::byml::Byml;
use mkwtrack::byml::vec3;
use mkwtrack::catalog::Catalog;
use mkwtrack::doc::Document;
use mkwtrack::raycast::Bvh;
use mkwtrack::workspace::Workspace;
use serde_json::{Value, json};

pub const COURSE: &str = "Banc/Race_FirstCircuit_A_00[Game03].bcett.byml.zs";
pub const MOD_NAME: &str = "EmuTest";
const ITEM_BOX: &str = "CmnObj_ItemBox000_A_00";
const COIN: &str = "CmnObj_Coin000_A_00";
const NEW_TYPE: &str = "CmnObj_Coin000_A_01";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scenario {
    /// No mod: the reference run.
    Baseline,
    /// Edits a player sees at the start: three new item boxes 6 m in front of the player's grid
    /// slot, and the first shipped item-box row lifted 6 m.
    Visible,
    /// Heavy structural edits: 640 new coins (the file outgrows its size-table entry), a new
    /// object type for this course, inserted and deleted path points, a new linked route,
    /// a reversed path and a changed object type.
    Stress,
}

impl Scenario {
    pub fn parse(s: &str) -> Option<Scenario> {
        Some(match s {
            "baseline" => Scenario::Baseline,
            "visible" => Scenario::Visible,
            "stress" => Scenario::Stress,
            _ => return None,
        })
    }

    pub fn name(self) -> &'static str {
        match self {
            Scenario::Baseline => "baseline",
            Scenario::Visible => "visible",
            Scenario::Stress => "stress",
        }
    }

    pub const ALL: [Scenario; 3] = [Scenario::Baseline, Scenario::Visible, Scenario::Stress];

    /// Write the scenario's mod into `ws.mods_root/MOD_NAME`. Returns a description of what was
    /// changed (for the report), or None for the baseline.
    pub fn build(self, ws: &Workspace, cat: &Catalog, ground: Option<&Bvh>) -> mkwtrack::Result<Option<Value>> {
        if self == Scenario::Baseline {
            return Ok(None);
        }
        let mut doc = Document::new(ws.load(COURSE, None)?);
        let mut notes = vec![];
        let start = player_start(&doc)?;
        match self {
            Scenario::Baseline => unreachable!(),
            Scenario::Visible => {
                let (pos, rot) = start;
                let (fwd, right) = (rot * Vec3::Z, rot * Vec3::X);
                let yaw = vec3(doc.find(start_hash(&doc)?).unwrap().1.get("Rotate"), [0.0; 3])[1];
                for k in [-1.6f32, 0.0, 1.6] {
                    // add_actor lifts by the type's BancTransOffset (1.5 m for item boxes)
                    let at = pos + fwd * 6.0 + right * k;
                    doc.add_actor(cat, ITEM_BOX, at, yaw).ok_or("item box type missing from the catalog")?;
                }
                notes.push("3 item boxes 6 m in front of StartPosition 23 (the player's slot)".to_string());
                let row = first_item_row(&doc, pos, rot);
                for h in &row {
                    let (p, r, s) = doc.transform(*h).unwrap();
                    doc.push_undo();
                    doc.set_transform(*h, p + Vec3::Y * 6.0, r, s);
                }
                notes.push(format!("the first shipped item-box row ahead of the grid ({} boxes) lifted 6 m", row.len()));
            }
            Scenario::Stress => {
                let (pos, rot) = start;
                let (fwd, right) = (rot * Vec3::Z, rot * Vec3::X);
                // 640 coins in a 16 x 40 grid behind the grid, on the road surface when known
                let mut coins = vec![];
                for i in 0..16 {
                    for j in 0..40 {
                        let at = pos - fwd * (12.0 + 2.5 * j as f32) + right * (2.5 * (i as f32 - 7.5));
                        let h = doc.add_actor(cat, COIN, at, 0.0).ok_or("coin type missing from the catalog")?;
                        coins.push(h);
                    }
                }
                if let Some(g) = ground {
                    let (n, total) = doc.drop_to_ground(&coins, cat, &|p| g.ground_below(p));
                    notes.push(format!("640 coins behind the grid ({n}/{total} dropped onto the stage surface)"));
                } else {
                    notes.push("640 coins behind the grid (at the grid's height)".into());
                }
                // a type this course never places, and one whose CountActor entry the game checks
                // before it sets the related system up (fn 0x2348500 in the 1.8.0 executable)
                let extra = NEW_TYPE;
                doc.add_actor(cat, extra, pos + fwd * 30.0 + right * 6.0, 0.0).ok_or("new type missing from the catalog")?;
                notes.push(format!("new type for this course: {extra}"));
                // insert a lap point after point 3
                let lap = doc.rails().iter().find(|r| r.str_or("Name", "") == "LapPathGraph").ok_or("no lap path")?;
                let p3 = hash_of(&Document::points(lap)[3]);
                doc.insert_point_after(p3).ok_or("insert failed")?;
                notes.push("lap path: point inserted after point 3".into());
                // delete the first point of a route path that other points link into
                let mut incoming = std::collections::HashSet::new();
                for r in doc.rails() {
                    for p in Document::points(r) {
                        for l in p.get("Links").and_then(|x| x.as_array()).into_iter().flatten() {
                            incoming.insert(l.get("Dst").and_then(|d| d.as_u64()).unwrap_or(0));
                        }
                    }
                }
                let route = doc
                    .rails()
                    .iter()
                    .find(|r| {
                        r.str_or("Name", "") == "RoutePathGraph"
                            && Document::points(r).len() > 3
                            && incoming.contains(&hash_of(&Document::points(r)[0]))
                    })
                    .ok_or("no linked route")?;
                let first = hash_of(&Document::points(route)[0]);
                notes.push(doc.delete(&[first]));
                // a new route path, linked from another route's last point
                let new_last = doc.new_rail(cat, "RoutePathGraph", pos + right * 12.0, 0.0).ok_or("new route failed")?;
                let new_rail = hash_of(doc.rails().last().unwrap());
                let src = doc
                    .rails()
                    .iter()
                    .filter(|r| r.str_or("Name", "") == "RoutePathGraph" && hash_of(r) != new_rail)
                    .map(|r| hash_of(Document::points(r).last().unwrap()))
                    .next()
                    .ok_or("no route to link from")?;
                doc.add_link(src, new_rail, "BasicSignal")?;
                notes.push(format!("new 2-point route path (last point {new_last}), linked from another route"));
                // reverse a guide rail and change a wood box into a drum
                if let Some(g) = doc.rails().iter().find(|r| r.str_or("Name", "") == "GuideGraphRail").map(hash_of) {
                    doc.reverse_rail(g);
                    notes.push("a GuideGraphRail reversed".into());
                }
                if let Some(w) = doc.actors().iter().find(|a| a.str_or("Name", "") == "CmnObj_WoodBox000_A_00").map(hash_of) {
                    doc.change_type(cat, w, "CmnObj_Drum000_A_00")?;
                    notes.push("a CmnObj_WoodBox000_A_00 changed into CmnObj_Drum000_A_00".into());
                }
            }
        }
        let res = ws.save(&doc.file, &doc.root, doc.pad.as_ref(), MOD_NAME, &doc.deleted, Some(cat));
        if !res.ok {
            return Err(format!("save failed: {}", res.errors.join("; ")).into());
        }
        Ok(Some(json!({"edits": notes, "written": res.written, "notes": res.notes, "warnings": res.warnings})))
    }
}

fn start_hash(doc: &Document) -> mkwtrack::Result<u64> {
    doc.actors()
        .iter()
        .find(|a| {
            a.str_or("Name", "") == "StartPosition"
                && a.get("Dynamic").and_then(|d| d.get("StartPositionId")).and_then(Byml::as_f64) == Some(23.0)
        })
        .map(hash_of)
        .ok_or_else(|| "no StartPosition 23 in the course".into())
}

/// Position and rotation of the player's grid slot.
fn player_start(doc: &Document) -> mkwtrack::Result<(Vec3, glam::Quat)> {
    let (p, q, _) = doc.transform(start_hash(doc)?).ok_or("start position has no transform")?;
    Ok((p, q))
}

/// The shipped item boxes of the first row ahead of the grid: the nearest box in front of the
/// player's slot, and every box chained to it by gaps under 6 m.
fn first_item_row(doc: &Document, pos: Vec3, rot: glam::Quat) -> Vec<u64> {
    let fwd = rot * Vec3::Z;
    let boxes: Vec<(u64, Vec3)> = doc
        .actors()
        .iter()
        .filter(|a| a.str_or("Name", "") == ITEM_BOX && a.get("InstanceId").is_some())
        .map(|a| (hash_of(a), Vec3::from(vec3(a.get("Translate"), [0.0; 3]))))
        .filter(|(_, p)| (*p - pos).dot(fwd) > 20.0)
        .collect();
    let Some(&(_, nearest)) = boxes.iter().min_by(|a, b| a.1.distance(pos).total_cmp(&b.1.distance(pos))) else { return vec![] };
    let mut row = vec![nearest];
    let mut grew = true;
    while grew {
        grew = false;
        for (_, p) in &boxes {
            if !row.contains(p) && row.iter().any(|q| q.distance(*p) < 6.0) {
                row.push(*p);
                grew = true;
            }
        }
    }
    boxes.iter().filter(|(_, p)| row.contains(p)).map(|x| x.0).collect()
}
