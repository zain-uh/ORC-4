# ORC-4 frame CAD — handover

Status as of 2026-10-25: **analysis done, nothing modelled yet.** The original
design (`Onyx_1.5.f3d`) has not been changed.

## Goal

Remake the Onyx 1.5 glasses frame as an unbranded, printable design with
enclosed space for the electronics, built in Fusion 360.

## Blockers

1. **Fusion 360 is not installed on the dev PC.** No Autodesk folder, no
   Fusion process.
2. **No Fusion connection in Claude Code.** The session only has the Claude
   Docs and scheduled-tasks connectors. The claude.ai registry only has
   "Autodesk Product Help", which searches help pages and can't edit models.
3. **No CAD branch yet.** Work so far has been on `rust-rewrite`, which holds
   the software.

To unblock:
- Install Fusion on the dev PC.
- Open Onyx_1.5.
- Enable a Fusion connection for Claude.
- Start a new session and say "go CAD".

Fallback: Claude writes a Fusion Python script that you run in Fusion
(Utilities › Scripts and Add-Ins).

## Requirements (from the user's brief)

| Area | Requirement |
|---|---|
| Branding | Remove all logos and text. Leave smooth surfaces. |
| Fit | Keep the existing frame's fit and overall size. |
| Display | Keep the existing display mount. |
| Pi Zero 2 W | Board 65 × 30 × 5 mm. Cavity has 2 mm clearance on each side, so the inside is **69 × 34 × 9 mm**. Openings for the micro-SD slot and **one** micro-USB port. Vents over the processor. 1–2 mm air gap from the skin. Enclosed. |
| Battery | Flat LiPo, ~1000 mAh, about 50 × 34 × 8 mm. Charger/boost board about 25 × 20 × 5 mm. In an enclosed, vented pod on the **opposite side** to the Pi, for weight balance. |
| Wiring | 3–4 mm channels for the composite video (AV) wires to the CT0024FL08 driver board, and for power across the bridge. |
| Printing | PETG on a normal FDM printer. Walls at least 1.6 mm. Minimal supports. |
| Assembly | Split into parts that screw together so it can be opened. |
| Deliverables | One STL per part, plus a short assembly guide. |

## Decisions made

| Decision | Choice | Why |
|---|---|---|
| Pod size | Accept pods **about 13 mm thick** near the hinge (about 8 mm now) | The Pi and the 1000 mAh cell need it. |
| Pi side | **Display side** (arm B, the arm with the existing box) | Short composite wires mean less picture noise, and only power crosses the bridge. The pods look the same from the front. |
| Fastening | **M2 screws into heat-set inserts** | Survives many open/close cycles in PETG. |
| Tool | **Fusion 360 through a connector** (the user's choice) | Works on the real solids. |

Estimated outside size of each pod (inside plus 1.6 mm walls):
- **Pi pod:** about 73 × 38 × 13 mm, on the outside of the temple, running back from the hinge.
- **Battery pod:** about 84 × 40 × 13 mm, with the charger behind the battery rather than stacked on it. Stacking would make it about 17 mm thick.

## What the existing design contains

`Onyx_1.5.f3d` is a Fusion archive, not an STL. Its solids were read directly
from the ACIS data inside it; units are cm in the file, mm below.

| Part | Size (mm) | Notes |
|---|---|---|
| Front frame | **146.8 × 48.1 × 24.0** | Lens rims and bridge. There's a pocket under one lens (about 18 × 8). |
| Arm A | ~130 long × 52 drop × 16.5 thick | Plain arm. |
| Arm B | ~130 long × 52 drop × 23.7 thick | Has a box at the hinge end. |
| Box on arm B | about 30–34 × 27 × 7.4 | Measured from its corner points. Too small for the Pi. |
| Display mount | 25.6 × 20.1 × 14.9 | Bracket with a pocket of about 14 × 8. |
| Display stand-in? | 14.4 × 10.3 × 8.4 | Simple 6-face block. |
| Lid | 27 × 35 × 2.6 | 4 corner bosses. Probably the lid of the arm B box. |
| Logo | 18 × 17 | 5 separate bodies: flat decals and a 1 mm raised copy. Removing the branding = deleting these bodies. |
| Hinge parts | 14 × 4.9 × 9 | 6 bodies. |
| Grip pads | 22.5 × 6.6 × 2.4 | 4 bodies, orange, on the arm tips. |
| Unknown | 34.5 × 36.2 × 2.0 and 12.5 × 23.6 × 23.1 | Not identified. |

**Not known yet (needs Fusion):**
- Where each part sits in the assembly. Placements are stored in a format
  that can't be decoded outside Fusion.
- Whether the frame or arms have recesses under the logo.

## Open questions for the user

Don't invent these:

1. The size of the display driver board (CT0024FL08 AV board), and where it
   should mount.
2. The exact battery and charger parts. Their real sizes set the battery pod.
3. The heat-set insert size: hole diameter and length.
4. Whether the arms must still fold. If they do, wires crossing the hinges need
   a slack loop or a flexible route.

Positions of the Pi's SD slot, micro-USB port and mounting holes come from
Raspberry Pi's official Zero 2 W mechanical drawing. Look them up; don't
work them out from memory.

## Modelling plan

1. **Protect the original.**
   - Create branch `cad` and a `cad/` folder for exported STLs, scripts and the guide.
   - In Fusion, save as **Onyx_2.0**. Never edit Onyx_1.5.
2. **Inspect.**
   - Component tree, placements and logo recesses.
   - Confirm the measurements above.
3. **Clean up and add stand-ins.**
   - Delete the 5 logo bodies and fill any recesses.
   - Add stand-in blocks for the Pi, battery, charger and driver board to check clearances.
4. **Build the pods.**
   - **Pi pod** replaces the box on arm B. It has the SD and USB openings, vents over the processor, and 1.5 mm standoff ribs on the skin side.
   - **Battery pod** goes on arm A, with vents.
   - Each pod splits into a shell and a lid, with M2 insert bosses.
5. **Wires and output.**
   - Cut 3.5 mm channels through the brow bar, past the hinges, and to the driver board.
   - Check every wall is at least 1.6 mm, overhangs are at most 45°, and each part has a print orientation.
   - Export one STL per part and write `cad/ASSEMBLY.md`.

## Analysis tools from the previous session

These are in a temporary scratchpad and may be deleted:
`C:\Users\09zu0\AppData\Local\Temp\claude\C--Users-09zu0-Documents-GitHub-ORC-4\5db23571-0680-4130-a447-7ae19d6cdc64\scratchpad\`

| Path | What it is |
|---|---|
| `f3d_extract/` | The unzipped .f3d. Solids are in `FusionAssetName[Active]/Breps.BlobParts/*.smb`. |
| `tools/sab.ts` | Deno reader for the ACIS binary data: entity counts and bounding boxes. Run: `deno run --allow-read tools/sab.ts <file.smb>` |
| `tools/render.ts` | Wireframe PNG of a body in 3 views. |
| `tools/box.ts` | Measures one region of a body. |
| `renders/*.png` | Wireframes of the frame, the arms and the small parts. |

The dev PC has Deno installed. It has no Python, OpenSCAD or FreeCAD.
