# TrackStudioWorld

A course editor for Mario Kart World, inspired by Track Studio for Mario Kart 8 Deluxe. It
opens the game's course files, lets you move, add and delete objects and paths in a 3D view,
and saves your changes as a mod for an emulator or for a Switch 2 with ns2jb-modloader.

It is written in Rust. The interface uses Kirigami (KDE Frameworks 6) and the 3D view uses
Qt Quick 3D, bridged with CXX-Qt.

> **Not affiliated.** TrackStudioWorld is an independent project. It is not affiliated with,
> endorsed by or connected to [Track Studio](https://mapstudioproject.github.io/TrackStudioDocs/)
> or the Map Studio Project; the name only says what it was inspired by. It is not affiliated
> with or endorsed by Nintendo either.
>
> It contains no game code or assets: you need your own dump of a game you own, and the
> console keys from your own console.

## What it edits

- **Objects** (`Actors`): item boxes, coins, start positions, audiences, obstacles, areas and
  every other type in the game's object database (4,826 types). Parameters are typed from the
  game's own parameter tables and show their defaults; links between objects can be added and
  removed; any other field can be edited as typed JSON.
- **Paths** (`Rails`), all 16 types: lap paths with their checkpoint gates, CPU and item routes,
  glide paths, guide rails, rail slides, Rainbow Road's gravity paths, citizen, water, parking,
  off-road and pull paths. Points, Bézier handles, checkpoint rectangles and per-point
  parameters are editable, and points can be inserted, deleted and linked.
- **Path data** (`Banc/PathAdditionalData`): per-point width and clearance values.

Any course file opens: races, the routes between courses, knockout rallies, battles, missions,
parades and the free-roam grid cells. Stage models are drawn where the game places them, and
objects with their real models.

On save the editor rebuilds the counts the game reads to decide which systems a course needs
(`CountActor`, `CountRail`, `Collects`), grows `AABB` over moved objects, refuses duplicate
hashes, writes BYML exactly the way Nintendo's tools do (an unedited course saves
byte-identical), and raises the resource size table only when a file outgrew its entry.

## How it was checked

- **Against every shipped file**: the BYML writer, the summaries, the size table and the
  parameter types reproduce all 1,311 shipped Banc files ([docs/format.md](docs/format.md)).
- **Against the game's code**: the keys the loader reads, the rotation order and what the counts
  control were read from the executable ([docs/game-code.md](docs/game-code.md)).
- **In game**: an emulator test harness boots the game with mods the editor wrote and drives
  into a Grand Prix race; objects added and moved in the editor appear where they were put and
  work ([docs/testing.md](docs/testing.md)).

## Building

You need:

- Rust 1.90 or newer;
- Qt 6 with Qt Quick 3D, Kirigami and the KDE desktop style (KDE Frameworks 6), and the Breeze
  widget style and icons. It is developed with Qt 6.12 and KDE Frameworks 6.31.

On Arch Linux:

```sh
sudo pacman -S --needed rust qt6-base qt6-declarative qt6-quick3d kirigami qqc2-desktop-style breeze breeze-icons
cargo build --release
```

The binary is `target/release/trackstudioworld`. `.cargo/config.toml` makes the build link
with Rust's bundled lld; see the comment there for why.

## Game files

The editor reads:

- **romfs**: the game's extracted RomFS (with `Banc/`, `RSDB/`, `System/`), version 1.8.0;
- **models**: the game's models exported to FBX with Switch Toolbox, in its usual layout
  (`<Category>/<Model>.<Variant>/<Variant>.fbx` with textures beside each file). Without them
  objects are drawn as boxes and drop-to-ground has no surface.

Set both in **File → Settings…**, or start with:

```sh
trackstudioworld --romfs /path/to/romfs --models /path/to/Models
```

| Option | Meaning |
|---|---|
| `--romfs DIR` | The game's extracted RomFS |
| `--models DIR` | The FBX model export |
| `--mods DIR` | Where mods are saved (default `~/MKW-Mods`) |
| `--ns2jb DIR` | A Switch 2 SD card (or a folder to copy onto one): export there on every save |
| `--open FILE` | Open a course at start, e.g. `"Banc/Race_FirstCircuit_A_00[Game03].bcett.byml.zs"` |
| `--select NAME` | After opening, select the first object of that type, or `PATH#N` for point N of the first path with that name |

Settings are saved in `~/.config/trackstudioworld/config.json`. The first start reads the
game's databases and every course, about 7 s, and caches the result in
`~/.cache/trackstudioworld/`.

## Using it

1. **Open a course** with **File → Open Course…** (Ctrl+O); search by file name or by the
   course's Japanese name. Pick a mod under **From:** to keep editing one you saved.
2. **Select** by clicking in the view or the Outliner. Ctrl+Click adds to the selection;
   clicking a group in the Outliner selects every object of that type.
3. **Edit**:
   - move, rotate or scale with the gizmo (1, 2, 3 switch modes) or type values in Properties;
   - duplicate (Ctrl+D), delete (Del), drop onto the stage surface (G), or change an object
     into another type (**Change Object…**);
   - add the current asset with a double-click in Assets, Q, or Alt+Click in the view;
   - **Paths → New Path…** starts a path of any type, I inserts a point after the selected one,
     L links the selection to the next thing you click;
   - undo and redo with Ctrl+Z and Ctrl+Shift+Z.
4. **Save** with Ctrl+S. The Help panel lists every key.

## Installing a mod

The editor writes a LayeredFS mod to `<mods>/<mod name>/romfs/`.

- **Eden and its forks** (PokeDinner, Eglantine): copy the mod folder into
  `~/.local/share/eden/load/0400C3F00006E000/`, or set the mods folder to that path to save
  straight into it. If a mod seems to do nothing, look for
  `LayeredFS patches applied successfully` in the emulator's log, and check `load_directory`
  in its settings.
- **Switch 2 with ns2jb-modloader**: **File → Export for Switch 2** copies the mod into the SD
  card's `ns2jb-modloader/contents/0400c3f00006e000/romfs` and merges its size table with the
  one already there, so other mods keep working.

## Limitations

- Path widths and clearances (`PathAdditionalData`) are not regenerated when route points
  move; edit them by hand if a route changes a lot.
- Object types never placed in a shipped course get only `IsGridify`, as there is no shipped
  placement to copy their other fields from.
- Reversing a path doesn't rewire the links at its ends.
- No model import, collision, camera, minimap or lighting editing. Course geometry is BFRES
  with Havok collision, and the open world is generated from heightmaps.
- Some models failed in the Switch Toolbox export and show as boxes; skinned models show in
  their bind pose.

## Repository

| Path | What it is |
|---|---|
| `core/` (`mkwtrack`) | The library: file formats, the game catalog, editing with undo, saving mods, picking and scene data. No Qt. |
| `app/` (`trackstudioworld`) | The desktop app. All logic is Rust; QML lays out the panels and draws what Rust hands it. |
| `tools/emutest/` (`mkw-emutest`) | The in-game test harness. |
| `docs/` | How the files work, what the game's code says, and how it is all tested. |

See [CONTRIBUTING.md](CONTRIBUTING.md) to build, test and change it.

## Acknowledgements

- Rimea, "Datamining Mario Kart World - How The Game Works & Unused Content" (2026-11-04), for
  the Banc layout, the Game03 naming, lap checkpoints and Rainbow Road's gravity paths.
- The Track Studio documentation (https://mapstudioproject.github.io/TrackStudioDocs/), for the
  feature set this follows.
- PokeDinner, an Eglantine fork (https://git.eden-emu.dev/Franseven/PokeDinner-a-Eglantine-Fork),
  for running the game and applying LayeredFS mods to loose dumps.

## License

[0BSD](LICENSE).
