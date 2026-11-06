# What the game's code says

The editor's assumptions about course files were checked against Mario Kart World's own
executable, so that the files it writes are read the way it expects. This page lists the
relevant functions, with addresses so anyone with the same game version can look for
themselves. It describes behaviour only; no game code is reproduced here.

## Where to look

- **Executable**: `main` from the ExeFS of version 1.8.0 (build ID
  `D00E4F399E411D0613AA8D26B1C02FEF`), decompressed and laid out as a flat memory image at
  base 0. Addresses below are offsets in that image. In Eden-based emulators the module loads
  at about `0x80934000`.
- **Finding the readers**: search the read-only data for a key's NUL-terminated string, then
  for code that builds its address (AArch64 `ADRP` + `ADD`), then disassemble that code.

## Keys the loader reads

| Function | Reads |
|---|---|
| 0x297fa0 | `Actors`, `Rails`, `FilePath`, `Points`, `Name`, `AreaName`, `ParentPrefab`, `Hash`, `Bakeable`, `Parent`, `Links` |
| 0x29b9f0 (objects) | `Translate`, `Rotate`, `Scale`, `Marking`, `RoadDeco`, `ScreenSetting` |
| 0x29ad70 (path points) | `Points`, `Name`, `Hash`, `RailBody`, `IsClosed`, `Translation`, `Scale`, `Rotate`, `Control0`, `Control1`, `RectSize`, `RectOffset` |
| 0x29a090 (links) | `Name`, `Src`, `Dst` |
| 0x295040, 0x2966b0, 0x2970a0 | `Gyaml`, `Dynamic` |
| 0x294530 | `IsGridify` |

`SRTHash` and `InstanceId` do not appear as strings at all. The editor still fills them in
for new objects, in the shipped form, so its files look like Nintendo's.

## Rotation order

After reading the three `Rotate` values, 0x29b9f0 calls sinf and cosf on each angle (with no
degree conversion, so they are radians) and stores nine matrix entries:
m00 = cy·cz, m01 = sx·sy·cz − cx·sz, m02 = cx·sy·cz + sx·sz, m10 = cy·sz,
m11 = sx·sy·sz + cx·cz, m12 = cx·sy·sz − sx·cz, m20 = −sy, m21 = sx·cy, m22 = cx·cy.
That is R = Rz·Ry·Rx: X is applied first.

The editor uses the same convention (`core/src/math.rs`; the test `matches_the_game_matrix`
checks these entries).

## Summaries decide which systems a course gets

`CountActor`, `CountRail`, `Collects` and `AABB` are read before anything spawns:

| Function | Reads | Purpose (inferred from the names) |
|---|---|---|
| 0x2a0f70 | `CountActor[name]` (22 callers) | accessor |
| 0x2a0ed0 | `CountRail[name]` | accessor |
| 0x2a1140 | `CountRail` of `RailSlidePath` and `ColRailSlidePath` | rail slides |
| 0x2a4cc0 | `AABB.Min`, `AABB.Max` (called from the course loader at 0x361a4c and 0x757c1c) | course bounds |
| 0x5f9500 | `CountActor` of `SystemObj_WaterBubbleArea_00`, `WaterStandingWaveArea_00`, `Whirlpool_00`, `WaterShallowArea_00`, `WaterEdgelineMaskArea_00`; `CountRail[WaterCascadePath]` | water |
| 0x7a5a40 | `Collects.MobVehicle` > 0, or `CountActor` of `SystemObj_RaceEntrance`, `ParkingSpace`, `HeliportPoint`, `ObjectRandomSwitcher`, `RideTrailerPlace`, `MissionCarChase` | traffic and set pieces |
| 0x79a0e0 | `Collects.Block` | blocks |
| 0x79af80, 0x7a50d0, 0x7a4d00 | `CountActor` of `SystemObj_AutoGenerate*SlatePoint_00`, `RespawnPoint_00`; `CountRail[CitizenPath]` | respawn points, citizens |
| 0x79bb80 | `CountActor` of `CmnObj_CoinBar000_{A,C,E}_00` | coin bars |
| 0x7a8710 | `CountActor` of `SystemObj_Landmark`, `LandmarkCameraArea`, `AvoidCameraArea`, `CylinderCameraCollisionArea` | cameras |
| 0x79ac10, 0x79ad50 | `CountActor` of `SystemObj_ActorGroup_00`, `SoundEnvSlatePoint_00`, `SoundEnvArea` | grouping, sound |
| 0x2348500 | `CountActor[CmnObj_Coin000_A_01]` | coins |
| 0x964630 | `CountActor[SystemObj_PrioRecoverPoint]` | recovery |

So a course edited without updating these counts can leave a system switched off: a whirlpool
added to a course without one would not get the water system. The editor rebuilds
`CountActor`, `CountRail` and `Collects` on every save, and the in-game stress test adds a type
the course never had (`CmnObj_Coin000_A_01`, checked at 0x2348500).

## `BancTransOffset`

The two references to the string (0xa1dd90 and 0x2949690) register it as a reflected
parameter field, next to `BancScale`, `BancDummyShape`, `BancDummyPath`, `BancWithLabel` and
`BancColor`. These look like settings for Nintendo's level editor rather than something
applied when an object spawns. That agrees with the shipped data (positions already include
the offset) and with the in-game test (boxes placed at ground + 1.5 m sit like the shipped
ones). No code reading the field by offset has been found.

## Course file names

`Banc/%s.bcett.byml` is built in one place, 0x362670, from a name copied up to its first `.`.
No format string in the executable builds the `[Game03]` suffix, so where the suffix is added
is not known. In game, a Grand Prix reads the `[Game03]` file.

## Open questions

- Where the `[Game03]` scene suffix is added.
- Which code reads `BancTransOffset` by offset, if any.
- How PathAdditionalData values (`WDP` and others) are used; the editor shows and edits them
  but does not regenerate them when route points move.
