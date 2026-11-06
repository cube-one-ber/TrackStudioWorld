//! Mario Kart World course data: the file formats, what the game expects of them, and the
//! editing operations of TrackStudioWorld.
//!
//! * [`byml`], [`sarc`], [`restbl`], [`typedjson`], [`io`]: file formats.
//! * [`banc`]: course files and the summaries the game reads from them.
//! * [`workspace`]: loading courses and saving them as LayeredFS mods.
//! * [`catalog`]: object types, parameter tables, path types and stage placements.
//! * [`doc`]: an open course with undo and redo.
//! * [`math`], [`raycast`], [`scene`], [`fbx`]: transforms, picking and what the viewport draws.

pub mod banc;
pub mod byml;
pub mod catalog;
pub mod doc;
pub mod error;
pub mod fbx;
pub mod io;
pub mod math;
pub mod raycast;
pub mod restbl;
pub mod sarc;
pub mod scene;
pub mod typedjson;
pub mod workspace;

pub use error::{Error, Result};
