# ORC-4 — Phase 3: Migration Plan (Python → Rust)

Principles:

- Each milestone ends with something **runnable and testable**.
- **Risky hardware assumptions are proven before code depends on them.**
- The Python prototype stays in `prototype/` as a reference until YouTube reaches parity (M6), then it is deleted.

## Hardware spikes (shell only, no Rust; can run on the Pi in parallel with M0–M2)

These decide parts of the design, so they come first.

| Spike | Question | Pass criteria | If it fails |
|---|---|---|---|
| **S0 Composite** | Does KMS composite drive the board? PAL or NTSC? How much overscan? Is the image mirrored? | `modetest` test pattern visible; note mode, cropped margins, readable text size | Try firmware composite (`sdtv_mode`); escalate the HDMI-board recommendation |
| **S1 yt-dlp on Zero 2 W** | Can the Pi resolve a YouTube H.264 stream URL, and does it need a JS runtime (Deno)? | Resolve < 10 s, peak RAM < 200 MB | Plan a remote resolver (phone/cloud) behind `VideoResolver` |
| **S2 Playback** | Can mpv play 480p H.264 via V4L2 M2M on composite, audio to a Bluetooth headset via bluez-alsa? | Smooth 30 fps, CPU < 60 %, audio in sync | Try `--vo=drm` vs `--vo=gpu`; lower to 360p |
| **S3 Radio coexistence** | Wi-Fi streaming + A2DP at the same time on the shared 2.4 GHz chip | No audio dropouts over 10 min with a large cache | Bigger cache, lower bitrate, pre-buffer before play |

## Milestones

| # | Milestone | Replaces in Python | Deliverable / how to test |
|---|---|---|---|
| **M0** | Toolchain + skeleton | — | Rust on Windows; `cargo run` opens the 960×540 simulator window; Esc quits; logger and layered config load. Repo cleanup: `.gitignore`, move Python to `prototype/`. |
| **M1** | UI toolkit | Tk canvas, fonts | Canvas, embedded Plex fonts, text wrap/ellipsis, theme tokens, status bar, easing. Snapshot test renders a reference screen to PNG. Composite-preview filter in the simulator. |
| **M2** | Input abstraction + navigator + launcher | `home.py` | Action/keymap, screen stack, carousel with peeking neighbours and eased vsync-paced slide, registry with 4 apps (placeholders open). Unit tests for navigation. |
| **M3** | **Pi bring-up** (early, to de-risk) | `orc4.service`, X11 | KMS composite backend, evdev keyboard, cross-compile to `aarch64-unknown-linux-musl` from Windows (`cargo-zigbuild`), deploy script, Pi OS Lite trim + systemd unit (no network wait, no X). Measure boot-to-UI, RSS, frame time. **First real readability check → tune type scale.** |
| **M4** | Offline core: Files + Settings | (new) | File browser + text/image viewers; Settings: Display (safe-area calibration, mirror, norm, reduce motion), Device info, logs viewer, System (restart/reboot/shutdown). Atomic persistence. **The device is now useful with no phone and no network.** |
| **M5** | Phone link v1 | (new) | WebSocket server, pairing code, protocol v1, `text_request` forwarding, time sync. `tools/remote_sim.py` drives the UI from the PC. Proves keyboard and phone are interchangeable. |
| **M6** | YouTube browse | `youtube.py` | ytdl helper process, async thumbnails, 2×2 grid with cross-page navigation, dedupe-before-fetch load-more, offline/error states. Parity with the prototype, minus its bugs → delete `prototype/`. |
| **M7** | Playback + Bluetooth audio | `video.py` (empty) | Player service (mpv + IPC), display handoff, minimal OSD, media actions. Bluetooth service: scan/pair/connect via BlueZ D-Bus, bluez-alsa output. Bluetooth settings page. |
| **M8** | Network settings | (new) | iwd service, Wi-Fi list/connect, connectivity probe, offline badges driven by real state. On-screen keyboard fallback for passwords without a phone. |
| **M9** | Search + reader | (new) | `SearchProvider` (API chosen then), results list, readability extraction, paginated reader. |
| **M10** | Product image (Phase 6) | `install_service.sh` | Buildroot BR2_EXTERNAL tree built in CI (GitHub Actions; Buildroot can't build on Windows), read-only root + `/data` partition, ~5 s boot, update mechanism (start with image re-flash; A/B later). |

Phase mapping:

| Your phase | Milestones |
|---|---|
| Phase 4 (implementation) | M0–M2, M4–M6, M9 |
| Phase 5 (hardware) | M3 + S0–S3, M7, M8 |
| Phase 6 (deployment) | M10, with the cross-compile/deploy path established in M3 |

**Deviation from the requested phase order:** I've moved a minimal hardware bring-up (M3) *before* most app work. On composite video, real readability determines the type scale and layout density. Finding that out after building four apps would mean redoing them.

## Dev-PC prerequisites (before M0)

- Rust via rustup (stable, MSVC host) — for the simulator.
- For cross-compiling (M3): `cargo-zigbuild` + Zig, target `aarch64-unknown-linux-musl`. This produces a static binary, so there are no glibc version mismatches with the Pi.
- An SSH client (built into Windows) for deploying.
