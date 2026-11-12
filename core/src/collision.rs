//! Keeping a course's baked collision in step with its objects.
//!
//! The game takes the collision of many static objects from a file baked per course
//! ([`crate::phive`]), not from the course file. So when the editor moves, deletes or adds
//! such an object, [`sync`] makes the same change in the baked file: it moves the object's
//! shapes, removes them, or adds new ones like those of another object of the same type.
//!
//! What the shipped files show, over all 341 course collision files and their course files:
//!
//! * every object whose type has a physics component has a link in the file, and it is baked
//!   exactly when its type has `IsUseStaticCompound` (the `Bakeable` key of the course file
//!   does not decide it: flags and areas are baked without it);
//! * a baked shape sits at the object's `Translate` exactly, with the quaternion of its
//!   `Rotate` and its `Scale`, also when an object has several shapes;
//! * links are sorted by type name.
//!
//! Some files hold the same objects in another frame: `RaceStage_*` has a course's objects
//! relative to the stage instead of the world. The frame is not stored anywhere, so it is
//! worked out from the shipped file itself: the one rotation and offset that carry every
//! object of the shipped course file onto its baked shapes.

use crate::banc::hash_of;
use crate::byml::Byml;
use crate::error::{Error, Result};
use crate::math::quat_zyx;
use crate::phive::{Link, Part, StaticCompound};
use glam::{Quat, Vec3};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, HashSet};

/// What an object type does for collision in game.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Physics {
    /// No physics component.
    #[default]
    None,
    /// A body of its own, made when the object spawns.
    Own,
    /// Shapes baked into the course's collision file (`IsUseStaticCompound`).
    Baked,
}

/// Where a course file puts an object.
#[derive(Clone, Debug, PartialEq)]
pub struct Pose {
    pub hash: u64,
    pub name: String,
    pub translate: [f32; 3],
    pub rotate: [f32; 3],
    pub scale: [f32; 3],
    pub srt_hash: u32,
}

impl Pose {
    pub fn of(actor: &Byml) -> Pose {
        Pose {
            hash: hash_of(actor),
            name: actor.str_or("Name", "").to_string(),
            translate: actor.vec3("Translate", [0.0; 3]),
            rotate: actor.vec3("Rotate", [0.0; 3]),
            scale: actor.vec3("Scale", [1.0; 3]),
            srt_hash: actor.u64_of("SRTHash").unwrap_or(0) as u32,
        }
    }

    /// The objects of a course file.
    pub fn all(doc: &Byml) -> Vec<Pose> {
        doc.array("Actors").iter().map(Pose::of).collect()
    }

    fn same_place(&self, o: &Pose) -> bool {
        self.translate == o.translate && self.rotate == o.rotate && self.scale == o.scale
    }
}

/// The frame a collision file holds its objects in, relative to the course file.
#[derive(Clone, Copy, Debug)]
struct Frame {
    rotation: Quat,
    translation: Vec3,
}

impl Frame {
    const WORLD: Frame = Frame { rotation: Quat::IDENTITY, translation: Vec3::ZERO };

    fn point(&self, p: [f32; 3]) -> Vec3 {
        self.rotation * Vec3::from(p) + self.translation
    }

    fn turn(&self, rotate: [f32; 3]) -> Quat {
        (self.rotation * quat_zyx(rotate)).normalize()
    }

    fn fits(&self, pose: &Pose, part: &Part) -> bool {
        let at = self.point(pose.translate);
        at.distance(part.instance.translation) < 0.05 + 1e-5 * at.length()
            && self.turn(pose.rotate).dot(part.instance.rotation).abs() > 1.0 - 1e-5
    }

    /// The frame that carries the shipped objects onto their baked shapes, or `None` when the
    /// file bakes none of them. An error when no single frame does.
    fn of(sc: &StaticCompound, shipped: &HashMap<u64, &Pose>) -> Result<Option<Frame>> {
        let pairs: Vec<(&Pose, &Part)> = sc
            .links
            .iter()
            .filter_map(|l| shipped.get(&l.hash).map(|p| (*p, l)))
            .flat_map(|(p, l)| l.parts.iter().map(move |x| (p, x)))
            .collect();
        if pairs.is_empty() {
            return Ok(None);
        }
        // The world frame, or the one that carries one object onto its shape
        let from_pair = |(pose, part): &(&Pose, &Part)| {
            let rotation = (part.instance.rotation * quat_zyx(pose.rotate).inverse()).normalize();
            Frame { rotation, translation: part.instance.translation - rotation * Vec3::from(pose.translate) }
        };
        let mut best = (0, Frame::WORLD);
        for frame in std::iter::once(Frame::WORLD).chain(pairs.iter().take(8).map(from_pair)) {
            let fits = pairs.iter().filter(|(pose, part)| frame.fits(pose, part)).count();
            if fits > best.0 {
                best = (fits, frame);
            }
        }
        if best.0 * 20 < pairs.len() * 19 {
            return Err(Error::new(format!(
                "its shapes are not where the game's course file puts its objects ({} of {} fit one frame)",
                best.0,
                pairs.len()
            )));
        }
        Ok(Some(best.1))
    }
}

/// What [`sync`] changed.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Changes {
    /// Objects whose shapes were moved.
    pub moved: usize,
    /// Objects given shapes or a link.
    pub added: usize,
    /// Objects taken out.
    pub removed: usize,
    /// Deleted objects whose shapes were lowered out of reach instead, because they were the
    /// last ones of their body.
    pub parked: usize,
    /// New objects whose type is baked but could not be given shapes: name and why.
    pub without: Vec<(String, String)>,
}

impl Changes {
    pub fn any(&self) -> bool {
        self.moved + self.added + self.removed + self.parked > 0
    }

    pub fn summary(&self) -> String {
        let parts: Vec<String> = [(self.moved, "moved"), (self.added, "added"), (self.removed + self.parked, "removed")]
            .iter()
            .filter(|(n, _)| *n > 0)
            .map(|(n, what)| format!("{n} {what}"))
            .collect();
        parts.join(", ")
    }
}

/// A shape another collision file bakes for an object type: the file and the object in it.
pub type Donor = (StaticCompound, u64);

/// How far a deleted object's shapes are lowered when they cannot be removed: out of reach,
/// but still at coordinates of the size the game's own files have.
const PARKED: f32 = 1000.0;

/// Change `sc` so that it bakes the objects of the edited course the way it baked those of the
/// shipped one. `shipped` and `edited` are the objects of the game's course file and of the
/// one being saved. `physics` says what a type does for collision, and `donor` finds a file
/// that bakes a type this one has no example of.
pub fn sync(
    sc: &mut StaticCompound,
    shipped: &[Pose],
    edited: &[Pose],
    physics: &dyn Fn(&str) -> Physics,
    donor: &mut dyn FnMut(&str) -> Option<Donor>,
) -> Result<Changes> {
    let was: HashMap<u64, &Pose> = shipped.iter().map(|p| (p.hash, p)).collect();
    let now: HashMap<u64, &Pose> = edited.iter().map(|p| (p.hash, p)).collect();
    let frame = Frame::of(sc, &was)?.unwrap_or(Frame::WORLD);
    let mut changes = Changes::default();
    let place = |part: &mut Part, was: Option<&Pose>, now: &Pose| {
        // Keep what did not change exactly as Nintendo's tools wrote it
        let kept = |same: fn(&Pose, &Pose) -> bool| was.is_some_and(|w| same(w, now));
        let t = if kept(|a, b| a.translate == b.translate) { part.instance.translation } else { frame.point(now.translate) };
        let r = if kept(|a, b| a.rotate == b.rotate) { part.instance.rotation } else { frame.turn(now.rotate) };
        let s = if kept(|a, b| a.scale == b.scale) { part.instance.scale } else { Vec3::from(now.scale) };
        part.place(t, r, s);
    };

    // Objects that moved
    let mut names: Vec<Option<String>> = vec![];
    for link in &mut sc.links {
        names.push(was.get(&link.hash).map(|p| p.name.clone()));
        let (Some(w), Some(n)) = (was.get(&link.hash), now.get(&link.hash)) else { continue };
        if link.srt_hash == w.srt_hash {
            link.srt_hash = n.srt_hash;
        }
        if !w.same_place(n) && !link.parts.is_empty() {
            for part in &mut link.parts {
                place(part, Some(w), n);
            }
            changes.moved += 1;
        }
    }

    // New objects: like another object of their type in this file, or in another file
    let known: HashSet<u64> = sc.links.iter().map(|l| l.hash).collect();
    let mut examples: BTreeMap<String, Vec<Part>> = BTreeMap::new();
    for (link, name) in sc.links.iter().zip(&names) {
        if let Some(name) = name {
            let e = examples.entry(name.clone()).or_default();
            if e.is_empty() {
                e.clone_from(&link.parts);
            }
        }
    }
    for pose in edited {
        // An object the game's file has too, in the same place, is as the game has it
        if known.contains(&pose.hash) || was.get(&pose.hash).is_some_and(|w| w.same_place(pose)) {
            continue;
        }
        let parts = match examples.get(&pose.name) {
            Some(parts) => parts.clone(),
            None => match physics(&pose.name) {
                Physics::None => continue,
                Physics::Own => vec![],
                Physics::Baked => {
                    let imported =
                        donor(&pose.name).ok_or_else(|| Error::new("no shipped course bakes this type")).and_then(|(from, hash)| {
                            let link = from.links.iter().find(|l| l.hash == hash && !l.parts.is_empty());
                            let link = link.ok_or_else(|| Error::new("the type's example is not baked"))?;
                            link.parts.iter().map(|p| sc.import_part(&from, p)).collect::<Result<Vec<Part>>>()
                        });
                    match imported {
                        Ok(parts) => parts,
                        Err(e) => {
                            changes.without.push((pose.name.clone(), e.message().to_string()));
                            continue;
                        }
                    }
                }
            },
        };
        examples.entry(pose.name.clone()).or_insert_with(|| parts.clone());
        let mut link = Link::new(pose.hash, pose.srt_hash);
        link.parts = parts;
        for part in &mut link.parts {
            place(part, None, pose);
        }
        // Links are sorted by type name
        let after = names.iter().rposition(|n| n.as_deref().is_some_and(|n| n <= pose.name.as_str()));
        let at = after.map_or_else(|| names.iter().position(Option::is_some).unwrap_or(names.len()), |i| i + 1);
        sc.links.insert(at, link);
        names.insert(at, Some(pose.name.clone()));
        changes.added += 1;
    }

    // Deleted objects. A body must keep at least one shape, so the last ones are moved out of
    // reach instead; the game ignores links to objects that are not there.
    let mut left = vec![0usize; sc.body_count()];
    for part in sc.links.iter().flat_map(|l| &l.parts) {
        left[part.body] += 1;
    }
    let mut keep = vec![];
    for mut link in std::mem::take(&mut sc.links) {
        if !was.contains_key(&link.hash) || now.contains_key(&link.hash) {
            keep.push(link);
            continue;
        }
        let mut counts = left.clone();
        for part in &link.parts {
            counts[part.body] -= 1;
        }
        if link.parts.iter().any(|p| counts[p.body] == 0) {
            for part in &mut link.parts {
                let (t, r, s) = (part.instance.translation, part.instance.rotation, part.instance.scale);
                part.place(Vec3::new(t.x, t.y - PARKED, t.z), r, s);
            }
            keep.push(link);
            changes.parked += 1;
        } else {
            left = counts;
            changes.removed += 1;
        }
    }
    sc.links = keep;
    Ok(changes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::phive::tests::sample;
    use std::f32::consts::FRAC_PI_2;

    fn pose(hash: u64, name: &str, translate: [f32; 3]) -> Pose {
        Pose { hash, name: name.into(), translate, rotate: [0.0; 3], scale: [1.0; 3], srt_hash: 0 }
    }

    fn baked(poses: &[Pose]) -> StaticCompound {
        let objects: Vec<(u64, [f32; 3])> = poses.iter().map(|p| (p.hash, p.translate)).collect();
        StaticCompound::parse(&sample(&objects, 0.5)).unwrap()
    }

    fn reread(sc: &StaticCompound) -> StaticCompound {
        let back = StaticCompound::parse(&sc.to_bytes().unwrap()).unwrap();
        back.check_trees().unwrap();
        back.check_bounds(1e-5).unwrap();
        back
    }

    fn at(sc: &StaticCompound, hash: u64) -> Vec3 {
        sc.links.iter().find(|l| l.hash == hash).unwrap().parts[0].instance.translation
    }

    #[test]
    fn moves_adds_and_removes() {
        let shipped = [pose(1, "Arrow", [0.0; 3]), pose(2, "Arrow", [10.0, 0.0, 0.0]), pose(3, "Stand", [20.0, 0.0, 0.0])];
        let mut sc = baked(&shipped);
        let same = sync(&mut sc, &shipped, &shipped, &|_| Physics::Baked, &mut |_| None).unwrap();
        assert!(!same.any() && same.without.is_empty());

        let mut edited = vec![shipped[0].clone(), shipped[2].clone()];
        edited[0].translate = [5.0, 1.0, 0.0];
        edited[0].srt_hash = 77;
        edited.push(pose(4, "Arrow", [30.0, 0.0, 0.0])); // like the arrows here
        edited.push(pose(5, "Crate", [1.0, 2.0, 3.0])); // carries its own collision
        edited.push(pose(6, "Flag", [40.0, 0.0, 0.0])); // baked, from another file
        edited.push(pose(7, "Ghost", [50.0, 0.0, 0.0])); // baked, but no file has it
        edited.push(pose(8, "Coin", [60.0, 0.0, 0.0])); // no physics
        let physics = |name: &str| match name {
            "Crate" => Physics::Own,
            "Coin" => Physics::None,
            _ => Physics::Baked,
        };
        let mut donor = |name: &str| (name == "Flag").then(|| (StaticCompound::parse(&sample(&[(99, [0.0; 3])], 2.0)).unwrap(), 99));
        let changes = sync(&mut sc, &shipped, &edited, &physics, &mut donor).unwrap();
        assert_eq!((changes.moved, changes.added, changes.removed, changes.parked), (1, 3, 1, 0));
        assert_eq!(changes.without.len(), 1);
        assert_eq!(changes.without[0].0, "Ghost");
        assert_eq!(changes.summary(), "1 moved, 3 added, 1 removed");

        let back = reread(&sc);
        // sorted by type name: Arrow, Arrow, Crate, Flag, Stand
        assert_eq!(back.links.iter().map(|l| l.hash).collect::<Vec<_>>(), [1, 4, 5, 6, 3]);
        assert_eq!(at(&back, 1), Vec3::new(5.0, 1.0, 0.0));
        assert_eq!(back.links[0].srt_hash, 77);
        assert_eq!(at(&back, 4), Vec3::new(30.0, 0.0, 0.0));
        assert_eq!(back.links[1].parts[0].instance.shape, back.links[0].parts[0].instance.shape);
        assert!(back.links[2].parts.is_empty());
        assert_eq!(at(&back, 6), Vec3::new(40.0, 0.0, 0.0));
        assert_ne!(back.links[3].parts[0].instance.shape, back.links[0].parts[0].instance.shape);
        assert_eq!(at(&back, 3), Vec3::new(20.0, 0.0, 0.0));

        // a second sync from the result's own state changes nothing more
        let mut again = back.clone();
        let poses: Vec<Pose> = edited.iter().filter(|p| p.name != "Ghost").cloned().collect();
        assert!(!sync(&mut again, &poses, &poses, &physics, &mut |_| None).unwrap().any());
    }

    #[test]
    fn the_last_shapes_of_a_body_are_parked() {
        let shipped = [pose(1, "Area", [0.0; 3]), pose(2, "Area", [10.0, 0.0, 0.0])];
        let mut sc = baked(&shipped);
        let changes = sync(&mut sc, &shipped, &[], &|_| Physics::Baked, &mut |_| None).unwrap();
        assert_eq!((changes.removed, changes.parked), (1, 1));
        let back = reread(&sc);
        assert_eq!(back.links.len(), 1);
        assert_eq!(at(&back, 2), Vec3::new(10.0, -PARKED, 0.0));
    }

    #[test]
    fn a_file_in_another_frame_is_followed() {
        // the file holds the course turned a quarter about Y and shifted, as RaceStage files do
        let turn = Quat::from_rotation_y(FRAC_PI_2);
        let shift = Vec3::new(100.0, -300.0, 50.0);
        let mut shipped = vec![pose(1, "Arrow", [0.0; 3]), pose(2, "Arrow", [10.0, 0.0, 0.0]), pose(3, "Arrow", [0.0, 0.0, 30.0])];
        for p in &mut shipped {
            p.rotate = [0.0, -FRAC_PI_2, 0.0]; // so the baked boxes are unrotated
        }
        let objects: Vec<(u64, [f32; 3])> = shipped.iter().map(|p| (p.hash, (turn * Vec3::from(p.translate) + shift).to_array())).collect();
        let mut sc = StaticCompound::parse(&sample(&objects, 0.5)).unwrap();
        assert!(!sync(&mut sc, &shipped, &shipped, &|_| Physics::Baked, &mut |_| None).unwrap().any());

        let mut edited = shipped.clone();
        edited[1].translate = [10.0, 5.0, 20.0];
        edited[2].rotate = [0.0, 0.0, 0.0];
        edited.push(Pose { rotate: [0.0; 3], ..pose(4, "Arrow", [7.0, 0.0, 0.0]) });
        let changes = sync(&mut sc, &shipped, &edited, &|_| Physics::Baked, &mut |_| None).unwrap();
        assert_eq!((changes.moved, changes.added), (2, 1));
        let back = reread(&sc);
        assert!(at(&back, 2).abs_diff_eq(turn * Vec3::new(10.0, 5.0, 20.0) + shift, 1e-3));
        assert!(at(&back, 4).abs_diff_eq(turn * Vec3::new(7.0, 0.0, 0.0) + shift, 1e-3));
        let part = |h: u64| back.links.iter().find(|l| l.hash == h).unwrap().parts[0].instance.clone();
        assert!(part(3).rotation.angle_between(turn) < 1e-4);
        assert_eq!(part(3).translation, Vec3::from(objects[2].1));
        assert_eq!(part(1).rotation, Quat::IDENTITY);
    }

    #[test]
    fn a_file_that_fits_no_frame_is_refused() {
        let shipped = [pose(1, "Arrow", [0.0; 3]), pose(2, "Arrow", [10.0, 0.0, 0.0]), pose(3, "Arrow", [0.0, 0.0, 30.0])];
        let mut sc = StaticCompound::parse(&sample(&[(1, [0.0; 3]), (2, [0.0, 50.0, 0.0]), (3, [9.0, 9.0, 9.0])], 0.5)).unwrap();
        let before = sc.to_bytes().unwrap();
        assert!(sync(&mut sc, &shipped, &shipped, &|_| Physics::Baked, &mut |_| None).is_err());
        assert!(sc.to_bytes().unwrap() == before);
    }
}
