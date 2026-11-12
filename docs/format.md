# Course files

What TrackStudioWorld knows about Mario Kart World's course data, and how each point was
checked. "Data" means checked against every shipped file; "code" means read from the game's
executable (version 1.8.0, see [game-code.md](game-code.md)); "in game" means seen in an
emulator (see [testing.md](testing.md)).

## Where courses live

| File | What it holds |
|---|---|
| `Banc/<Course>[<Scene>].bcett.byml.zs` | One course's objects (`Actors`) and paths (`Rails`), e.g. `Banc/Race_FirstCircuit_A_00[Game03].bcett.byml.zs` for Mario Bros. Circuit |
| `Banc/PathAdditionalData/<Course>.byml` | Per-point values (width, clearance) keyed by point hash |
| `Banc/World_Game03/WorldResident.bcett.byml.zs` | Where the stage models of the open world are placed |
| `Banc/Rainbow_00_Fix.bcett.byml.zs` | Rainbow Road's sky sections |
| `Phive/StaticCompoundBody/Course/<Course>.Nin_Ounce_NVN2.bphsc.zs` | The collision of the course's static objects, baked at the places they shipped at (see [Baked collision](#baked-collision)) |
| `System/Resource/ResourceSizeTable.Product.*.rsizetable.zs` | How much memory the game reserves for each file |
| `RSDB/*.rstbl.byml.zs` | Tables of object types (`Actor`), parameters (`BancParam`), path types (`GraphRailDefine`) and courses (`Course`) |
| `System/GymlArchive/*.sarc.zs` | Per-type parameter files (models, bounds, `BancTransOffset`) |

RSDB `Course` rows name the file as `Work/Banc/Course/Race_X.bcett.json` with `Scene: Game03`;
the scene becomes the `[Game03]` suffix (data). In a Grand Prix the game reads the `[Game03]`
file and spawns its objects (in game).

## Encoding

- BYML version 7, little endian, zstd-compressed (`.zs`). Course files are zero-padded to a
  power of two before compression; PathAdditionalData is not.
- Writing reproduces Nintendo's layout: 1,305 of the 1,311 BYML files under `Banc/` rewrite
  byte-identically, and the other 6 (`Banc/PathByml/*`, from a different writer that puts
  binary blobs first) read back identically (data).
- Numbers keep their BYML types (`f32`, `s32`, `u32`, `u64`...). The raw-field editor and the
  catalog cache use [typed JSON](../core/src/typedjson.rs) to keep them.

## Objects (`Actors`)

| Key | Meaning | Evidence |
|---|---|---|
| `Translate` | World position in metres | code: read by the actor loader (0x29b9f0) |
| `Rotate` | Euler angles in radians, R = Rz·Ry·Rx (X applied first) | code: the loader builds exactly that matrix; data: path points' up axes are 0.6° from the road normal with this order, 8.7° with the reverse |
| `Scale` | Per-axis scale, absent when 1 | code: read by the loader |
| `Hash` | u64, unique among the file's objects, paths and points | code: read (0x297fa0); data: unique in every file |
| `Gyaml`, `Name` | Object type | code: read (0x295040 and others) |
| `Dynamic` | Parameters, typed by the RSDB `BancParam` table | data: all 322 parameter names stored with their table type; enums are stored as their s32 index |
| `Links` | `{Src, Dst, Name, Gyaml}` signals to other objects or points | code: read (0x297fa0, 0x29a090) |
| `IsGridify`, `Bakeable`, `ScreenSetting`, `PrimitiveType`, `ParentPrefab` | Placement flags | code: each is read |
| `SRTHash`, `InstanceId` | Not read from the course file | code: neither string, nor its CRC32/Murmur3/xxHash32/FNV-1a hash, appears in main, sdk or the subsdks. The baked collision file stores each object's `SRTHash` next to its `Hash`; 8% of the shipped ones differ from the course file's, so nothing can depend on them matching (data) |

Stored positions already include the type's `BancTransOffset` (data: every item box sits 1.50 m
above the road, its offset, and coins about 1.2 m). The game registers `BancTransOffset` only as
a reflected parameter next to `BancDummyShape`, `BancColor` and `BancWithLabel`, which look like
settings for Nintendo's level editor (code), and in game, item boxes placed at ground + 1.5 m sit
exactly like the shipped ones. So the editor adds the offset when it places an object.

New objects of a type that a shipped course places copy that placement's `Bakeable`, `IsGridify`,
`PrimitiveType` and `ScreenSetting` (and `Scale` for primitive areas). Types never placed in a
shipped course get only `IsGridify: false`.

## Paths (`Rails`)

A path has `Name` (its type, one of the 16 `GraphRailDefine` rows), `Hash`, `IsClosed` and
`Points`. Each point has `Translation`, `Rotate`, optional Bézier handles `Control0`/`Control1`,
optional `Scale`, `RectSize`/`RectOffset`, `Dynamic` and `Links`, and its `Parent` is the path's
hash (code: the point reader at 0x29ad70 reads each of these keys).

- Paths join up with links: a path's last point links (`BasicSignal`) to the next path's first
  point (data).
- Lap checkpoint gates span `RectOffset.y - RectSize.y` up to `RectOffset.y` around the point
  (data: every shipped gate bottoms out 5 to 10 m below its point with that reading).
- A lap point is a checkpoint when its `Dynamic` has `CheckPoint` (f32). The numbers follow one
  rule (data: 5,778 of the 5,779 shipped values, in 287 files; the other is a 5.1 between a 4
  and a 6). Walk the lap graph along each path's points (round a closed path) and through
  `BasicSignal` and `MessageLink` links between lap points, which usually join a path's last
  point to the next path's first point and sometimes enter a path in the middle. The count
  starts again at 0 at every point with `IsStart`, `IsLapCheck` or `IsGoal`, and each
  checkpoint is one more than the highest count on any way into it, so parallel branches carry
  the same numbers and the count carries on from the longer one where they meet. No shipped
  point is both a checkpoint and a start, lap check or goal point. The editor renumbers after
  every edit that could change the numbers and on save, leaving values whose whole part is
  already right (such as that 5.1).
- PathAdditionalData has an entry for 227,255 of the 228,110 shipped `RoutePathGraph` points
  (the 855 without one are in 14 courses) and for no other point (data), so new route points
  copy a neighbour's entry. Shipped files also hold some entries
  for points that no longer exist, so stale entries are harmless; the editor removes entries
  for the points you delete.

## Summaries

| Key | Meaning | Evidence |
|---|---|---|
| `CountActor` | Objects per type name | data: matches all 1,019 shipped courses; code: 44 references to the key, including an accessor (0x2a0f70) with 22 callers |
| `CountRail` | Paths per type name | data: matches; code: read (0x2a0ed0, 0x2a1140) |
| `Collects` | Objects per class: `Citizen`, `Coin`, `MobVehicle`, `NpcAudience`, `Block` | data: the name-to-class mapping is fitted from the shipped files and reproduces every one; code: `MobVehicle` and `Block` are read |
| `AABB` | `Min`/`Max` bounds | code: read while a course loads (0x2a4cc0) |

The game reads these before it spawns anything, to decide which systems a course needs: for
example the water system only starts if `CountActor` has a water area or whirlpool or
`CountRail` has a `WaterCascadePath`, and traffic only if `Collects.MobVehicle` > 0 (code, see
[game-code.md](game-code.md)). So the editor rebuilds all of them on every save. `AABB` only
ever grows, to cover new or moved objects, because shipped bounds don't always contain every
object.

## Resource size table

The game reserves memory for each file from this table. Nintendo's entries follow a fixed
pattern (data: all 1,311 Banc entries):

- `.bcett.byml`: decompressed size + 1024;
- plain `.byml` (PathAdditionalData): decompressed size rounded up to 32, + 288;
- `.bphsc` (baked collision): decompressed size rounded up to 32, + 2720, + 320 for each body
  in the file (data: all 2,045 entries checked, every course, mission and stage file and one
  open-world tile in 16).

Entries are a CRC32 of the path without `.zs`, plus a short table of full paths for names whose
CRC32 collides. The editor writes the table into the mod only when a file outgrew its entry,
and only raises entries; the table rewrites byte-identically (data).

## Baked collision

Moving an arrow sign in the course file moves what you see, and nothing you hit: the game
takes the collision of static objects from a file baked per course, `Phive/StaticCompoundBody/
Course/<Course>.Nin_Ounce_NVN2.bphsc.zs`, where every such object's shape sits at the place it
had when Nintendo built the game. So the editor rewrites that file whenever one of these
objects moves, goes or is added.

Which objects (data: the 341 course collision files against their course files, 134,000
objects with physics):

- Objects whose type has a physics component (`PhysicsRef` in its ActorGParam) are listed in
  the file by their `Hash` (all but 404 of them), and no other object is.
- An object's shapes are baked when its type has `IsUseStaticCompound` in its ActorInfo: signs,
  grandstand platforms, barriers, flags, ramps, and the rescue and valley areas (33,301 of the
  33,329 baked objects; the other 28 are of one type). The course file's `Bakeable` key does
  not decide it: flags and areas are baked without it. No object of such a type is listed
  without shapes, so nothing suggests the game can make their collision by itself. The other
  objects with physics (fences, boxes, cones: 101,018) are listed without shapes and carry
  their own collision.
- A baked shape has the object's `Translate` exactly, the quaternion of its `Rotate`, and its
  `Scale` (all 33,429 shapes checked), also when an object has several shapes.

The file is a Phive container (`Phive`, then a table of section offsets and sizes; sections
are aligned to 16):

| Section | Holds |
|---|---|
| 0 | Fixed-size tables that seem to name the collision layers with a body (kept as they are) |
| 1 | One 32-byte link per listed object, sorted by type name: first and last index of its shapes in section 4 (−1 when it has none), `Hash`, `SRTHash` |
| 2, 3 | A hash table over the links: twice as many 8-byte slots as links, a link sits at `Hash` modulo the slot count or in the next free slot; section 3 has the link index of each slot |
| 4 | One 20-byte entry per baked shape: its own index, its collision layer, its link, its index in its body, and its first material and material count in sections 5 and 6 |
| 5, 6 | Material tables, 16 bytes per row |
| 7 to 10 | Not used for a course's objects (kept as they are) |
| 11 on | One Havok tagfile per body: one per collision layer in use, such as the ground layer and the rescue areas' sensor layer |

Each body is a `hknpCompoundShape`. Its instances (`hknpShapeInstance`, 64 bytes) hold a
rotation, translation, flags and scale, a `shapeTag` that is the index of the shape's entry in
section 4, and a relative pointer to the shape itself, which is stored once per body however
many objects use it. Flag 0x04 marks a rotation and 0x08 a scale other than 1. Collision
queries walk a tree of boxes over the instances (`hkcdSimdTree`: 128-byte nodes with four
children each, node 0 unused, node 1 the root), so a moved instance needs its box in the tree
changed too. The body also stores the union of the boxes, a bounding radius, the bits needed
to number its instances, and an estimate of its shape keys (the sum over its instances of
2^`numShapeKeyBits` of the shape, or of a nested compound's own estimate).

An instance's box is the box of the eight transformed corners of the shape's own box for
meshes, nested compounds and unscaled boxes; the vertices plus the radius for unscaled
capsules and spheres; and for scaled boxes, the box of the vertices after moving them so that
the rounded surface scales while the radius stays. The editor reproduces 38,937 of the 38,964
shipped boxes this way; the rest are scaled capsules and other convex shapes, for which it
writes a box that is never smaller than the game's.

Tagfiles describe their own types, so the editor finds every field by name in the file's type
section rather than at a fixed offset, and refuses a file whose types are not laid out as
expected.

How this was checked (data): every one of those 2,045 files is rebuilt byte for byte from the
editor's model of it (links, hash table, entries, instances); every shipped tree follows the
rules the editor's own trees are built by; and after an edit the file is read back and checked
against the same rules before it is written. The editor's changes have not yet been tried in
game: `mkw-emutest collision` is the test for that (see [testing.md](testing.md)).

What the editor does on save, always starting from the game's file:

- **Moved, turned or scaled object**: its shapes get the new placement and the tree is
  rebuilt. The tree fits where the old one was, so the file keeps its size.
- **Deleted object**: its shapes and its link go. A body cannot be left empty, so when the
  last shapes of a body are deleted (say, every rescue area of a course) they are lowered
  1 km instead, out of reach, with their link kept; shipped files have links to objects
  that are not in their course too (371 of them).
- **New object**: it gets the shapes of another object of its type in the same file. For a type
  the course has none of, the shape is copied in from another course's collision file, with
  its materials; that needs the file to have a body for the shape's collision layer already,
  and the save warns when it does not, since the object will then have no collision.
- **Unchanged again**: the mod's copy of the collision file is removed.

A course's objects can be baked in more than one file (345 files in all): a
race also has a `RaceStage_*` file, and two have a `Mission_*` one. 28 of the files hold the
objects in another frame, relative to the stage instead of the world. Nothing in a file names
the course it belongs to or its frame, so the editor finds the files by the objects' hashes
and works the frame out from the shipped data: the one rotation and offset that carry every
object of the game's course file onto its baked shapes (data: one frame fits every file).

Not handled: the collision of the stage model itself and of the open world (other files),
and new shapes that no shipped course bakes.

## Models and textures

The viewport, the thumbnails and drop-to-ground read the game's own models (data: every file
under `Model/` parses, and Mario Bros. Circuit's stage model spans the box its ActorInfo
gives, `ModelAABBMin` and `ModelAABBMax`, to within a metre).

- An object type's ModelInfo names its model as `Work/Model/<Category>/<Model>/output/<Variant>.fmdb`;
  the file is `Model/<Model>.<Variant>.bfres.zs`.
- BFRES version 10. Each shape has one mesh per level of detail; the first is the most detailed.
  Index and vertex data sit in one buffer after the structures. Mesh indices add the mesh's
  base vertex. Vertex attributes use nn::gfx formats (positions as 32-bit or half floats,
  normals as 10-10-10-2, texture coordinates as 16- or 32-bit).
- Shapes with no skinning follow their bone; shapes with one weight follow the bone each
  vertex's `_i0` index names through the skeleton's matrix-to-bone list; smooth-skinned
  vertices are stored in model space.
- A material's textures and samplers are parallel lists; the `_a0` sampler is the albedo. Its
  render info says whether it is alpha-tested (`gsys_alpha_test_enable`,
  `gsys_render_state_mode` = `mask`).
- Textures are `Tex/<name>.bntx.zs`, one texture per file, on the Switch 2 platform (`Ounc`).
  Their surfaces are block linear with the Tegra X1's block heights, but each 512-byte GOB is
  ordered as the Orin TRM describes. Albedo textures are ASTC 6x6 or 8x8, mostly sRGB (data:
  all 6,457 that models use decode).

## Mods

The editor saves a LayeredFS mod: `<mods>/<mod name>/romfs/` with only the changed files.

- Eden and its forks read `~/.local/share/eden/load/0400C3F00006E000/<mod name>/romfs`. When
  a mod seems to do nothing, look for `LayeredFS patches applied successfully` in the
  emulator's log; `Applied RomFS patches (LayeredFS)` is printed for every loose-dump boot,
  mod or not.
- ns2jb-modloader (Switch 2) reads one shared `contents/0400c3f00006e000/romfs` on the SD card.
  Exports copy files over the top and merge the size table with the one already there.
