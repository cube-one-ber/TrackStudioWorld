# Changelog

## 0.1.0 (unreleased)

First public release.

- Course editor for every Banc course file: objects, all 16 path types and path data, with a
  3D view of the stage and object models, an outliner, typed parameters, links and undo.
- Saves LayeredFS mods for Eden-based emulators and exports them for ns2jb-modloader on
  Switch 2, rebuilding the summaries the game reads and growing the resource size table when a
  file outgrows its entry.
- `mkwtrack` library: BYML v7 reading and byte-exact writing, SARC, the resource size table,
  typed JSON, the game catalog, an FBX reader and editing operations.
- Tests: unit tests without game files, tests against every shipped file, and `mkw-emutest`,
  which checks mods in an emulator.
