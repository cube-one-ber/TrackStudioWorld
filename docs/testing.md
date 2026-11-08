# Testing

Three layers, from fastest to slowest.

## 1. Unit tests (no game files)

```sh
cargo test --workspace
```

These build small BYML, SARC and size-table files in memory and check reading, writing,
corrupt input (every truncation and single-byte corruption must fail cleanly, never panic),
typed JSON, the transform conventions, the summaries, every editing operation with undo, and
saving a mod into a temporary folder. CI runs them on every push.

## 2. The shipped files

```sh
MKW_ROMFS=/abs/path/romfs cargo test --release -p mkwtrack
```

The path must be absolute (cargo runs tests from `core/`). Without the variable these tests
print that they skipped.

| Test | Checks |
|---|---|
| `core.rs` | Every BYML file under `Banc/` rewrites byte-identically (1,305 of 1,311; the 6 `Banc/PathByml` files come from another writer and read back identically); typed JSON round trips; rebuilding the summaries changes no shipped course; every shipped course passes the save checks; the size-table formula matches every entry and the table rewrites identically; route points have path data (99.6%) and no other point does; no shipped course gives a Problems-panel warning |
| `editing.rs` | Edit sessions on Mario Bros. Circuit: move, duplicate, delete, add, drop to ground on the real stage mesh, insert and delete points (with links into a deleted point retargeted), new path, link, undo and redo, reversing twice; saving and re-reading with the summaries, path-data pruning and unique hashes; size-table growth; refusing duplicate hashes, unsafe paths and bad mod names; the ns2jb-modloader export; the path tools: orienting the shipped lap and route points reproduces them (median 3.3° heading, 0.7° up), split then join restores a lap path, a pasted lap graph links only to itself, and extending, placing coins and saving give a valid mod |
| `models.rs` | Every model under `Model/` parses (5,827 files; the 1,024 `.anim.bfres` files hold no geometry); Mario Bros. Circuit's stage bounds match its ActorInfo; the item box gets its albedo texture; every albedo texture a model uses decodes (6,457) |

## 3. In game: `mkw-emutest`

`tools/emutest` writes mods through the editor's own save path, boots the game in an Eden-based
emulator on a private X display with a private emulator profile, drives the menus into Grand
Prix race 1 (Mushroom Cup, 50cc: Mario Bros. Circuit), holds accelerate, and records
screenshots, the emulator's log and a report.

You need:

- your game dump with `exefs/` and `romfs/` side by side;
- `prod.keys` from your console;
- an Eden-based emulator that runs the game, such as the PokeDinner AppImage, extracted with
  `--appimage-extract` (the harness runs `squashfs-root/usr/bin/eden-cli`);
- Xvfb;
- optionally an emulator data folder from a profile that has played the game, to copy system
  and save data from (see below).

```sh
cargo build --release -p mkw-emutest
target/release/mkw-emutest \
  --dump /path/to/dump --keys /path/to/prod.keys \
  --eden /path/to/squashfs-root/usr/bin/eden-cli \
  --profile-template ~/.local/share/eden \
  --out emutest-out
```

The emulator never touches your own emulator setup: it runs with `HOME` set inside `--out`.
It is capped at 6 GB of memory through `systemd-run` (`--memory-max`).

### Scenarios

| Scenario | Mod |
|---|---|
| `baseline` | none |
| `visible` | 3 new item boxes 6 m in front of the player's grid slot (`StartPosition` 23), and the 7-box item row ahead of the grid lifted 6 m |
| `stress` | 640 new coins behind the grid (the course file outgrows its size-table entry), a type the course never had and whose count the game checks (`CmnObj_Coin000_A_01`), a lap point inserted, a linked route point deleted, a new route linked from another, an object changed into another type |

`--mods-only` writes the mods and stops, to inspect them without the emulator.

### Checks

Each scenario's report (`<out>/<scenario>/report.json`) fails unless:

- the emulator's log shows `LayeredFS patches applied successfully` exactly when a mod is
  installed;
- the title screen appears (its "Press L + R to start" prompt);
- the race HUD appears (the coin icon, coin count and lap counter together);
- the HUD stays up and the picture keeps changing while accelerate is held;
- the emulator is still running at the end.

Whether the edits show up is for a person to judge: compare `sheet-drive.png` and the
screenshots with the baseline's. `--classify FILE...` prints what the title and HUD detectors
see in saved screenshots.

### Troubleshooting

- **The emulator dies before the title screen**: the emulator's GPU texture decoding can run
  long enough to trip the kernel's GPU timeout on some integrated GPUs (`ring gfx timeout`,
  then `VK_ERROR_DEVICE_LOST`). The scenario is tried once more automatically.
- **The game aborts at the start of the Grand Prix** (`2162-0001`): this happens on a freshly
  created emulator profile, with or without a mod. Use `--profile-template` with the data
  folder of a profile that has played the game.
- **A menu press is missed**: the menu script waits 12 s between presses, and some screens can
  need a second press. While it waits for the race, the harness taps A every 20 s, which also
  skips the course previews before the first race. The screenshots `01-after-lr.png` to
  `08-menu.png` and `10-wait-*.png` show what happened.
- **The first boot is slow**: with a played-in save the game first loads wherever the save last
  was, and the first boot compiles that area's shaders, which can take several minutes. Later
  boots with the same profile are much faster. `--title-timeout` sets how long to wait.
