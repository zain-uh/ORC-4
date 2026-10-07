# ORC-4 — Phase 2: Architecture

Decisions confirmed so far:

| Topic | Decision |
|---|---|
| Display | CT0024FL08 0.24" 960×540 FLCOS with an **AV (composite) driver board**, wired to the Pi's composite (TV) output |
| YouTube | Full playback on the glasses, audio to Bluetooth |
| Google | Search + native reader view |
| OS | Raspberry Pi OS Lite 64-bit for development → Buildroot for the product image |

---

## 1. Hardware reality check

### 1.1 The display path is composite video, and that dominates the UI design

```
Rust UI (960×540 logical) ──► KMS ──► VEC (composite encoder) ──► analog CVBS
    ──► AV driver board (decodes, rescales) ──► 960×540 FLCOS panel
```

| Property | Consequence |
|---|---|
| The signal is 720×480i (NTSC) or 720×576i (PAL) | The panel's 960×540 is **never** delivered pixel-for-pixel. The board rescales. |
| Luma bandwidth ≈ 330–440 resolvable columns | Treat the screen as roughly **440 px wide** for detail. Text must be large and bold, never thin. |
| Chroma bandwidth ≈ 100–130 colour columns | **Coloured text and thin coloured lines are unreadable.** Colour is only for large areas. Text is always white/grey on black. |
| Interlaced | 1 px horizontal lines flicker ("twitter"). All strokes ≥ 2 logical px. Avoid 1 px horizontal detail. |
| Analog overscan | The board crops an unknown margin. The safe area must be **user-calibratable**. |
| 4:3 signal shown on a 16:9 panel | The board stretches horizontally. Designing on a 960×540 logical canvas keeps shapes square *on the panel*. |
| FLCOS is field-sequential | Fast motion shows colour break-up. Animations stay short and eased, and there's a "reduce motion" setting. |

**Hardware recommendation (not blocking):** composite is the single largest limit on readability. The same class of FLCOS panel is sold with HDMI driver boards. For example, [Display Components on Tindie](https://www.tindie.com/products/oled-modules/720p-038-inch-micro-flcos-display-1280x720/) sells 0.38" 1280×720 FLCOS panels with HDMI controller boards. The Pi Zero 2 W has a mini-HDMI port. For the product, I recommend moving to an HDMI (or MIPI) driver board.

The software below treats the output connector, mode and scaling as **configuration**, so that switch needs no code changes.

### 1.2 Pi Zero 2 W budget

| Resource | Reality | Design rule |
|---|---|---|
| CPU | 4× Cortex-A53 @ 1 GHz | One UI thread, few worker threads. No redraw when idle. |
| RAM | 512 MB shared with GPU | ORC-4 core < 40 MB RSS. Heavy helpers (yt-dlp, mpv) start on demand and exit afterwards. |
| GPU | VideoCore IV + HVS (hardware scaler/compositor) | Use the **HVS for scaling and mirroring**. Use CPU 2D rasterising for the UI. No GL/Mesa needed for the UI. |
| Video decode | Hardware H.264 only (V4L2 M2M) | YouTube must use **avc1 (H.264) ≤ 480p**. VP9/AV1 are software-only and far too slow. |
| Radio | One 2.4 GHz chip for both Wi-Fi and Bluetooth | Wi-Fi streaming + A2DP audio at the same time **will contend**. This must be tested early, with large playback buffers. |
| Clock | No RTC | Time is unknown until NTP or the phone provides it. Never display a fake time. |
| Storage | SD card, power can be cut anytime | Atomic writes for settings. Read-only root in the product image. |

---

## 2. Rendering technology: evaluation and choice

Requirements: a 960×540 logical canvas, ~6 element types (text, rounded rects, images, icons, rings, gradients), 60 fps transitions, 0 fps idle, tiny RAM, fast start, no desktop stack.

| Option | Verdict |
|---|---|
| X11/Wayland + toolkit (GTK/Qt/Tk) | ✗ Needs a display server and desktop libraries: tens of MB and seconds of boot, for features we don't use. |
| Chromium/Electron/web | ✗ Excluded by the spec, and would not fit in RAM alongside video playback. |
| **Slint** (linuxkms + software renderer) | Technically a good fit, but its royalty-free licence **excludes embedded devices**. A product would need GPLv3 or a paid licence. It also imposes its own widget look. |
| LVGL | Excellent on MCUs, but it's C with immature Rust bindings, and its styling fights a custom look. |
| SDL2 kmsdrm | Works, but adds a C dependency that only replaces ~300 lines of KMS code. |
| egui/iced | Desktop-feeling, and expect a GPU (GL). |
| fbdev `/dev/fb0` | Simple, but no page flip (tears) and no hardware scaling or mirroring. |
| **DRM/KMS dumb buffers + CPU 2D (tiny-skia) + fontdue** | ✓ **Chosen.** |

Why the chosen stack fits:

- **No GPU driver dependency** for the UI.
- **Vsync page flips** mean no tearing.
- The **HVS scales** the 960×540 frame to 720×480/576 at zero CPU cost, and does mirroring/rotation via the plane `rotation` property if the optics need it.
- All pure Rust.
- A 960×540 RGBA frame is 2 MB. A full redraw of a simple scene is a few ms on an A53.

On the dev PC, the same canvas is shown in a window (minifb), including an optional **composite preview filter** (downscale to 720×480, horizontal low-pass, interlace-style softening). That lets you judge readability without the hardware.

---

## 3. Directory structure

```
ORC-4/
├── Cargo.toml                 single binary crate (a workspace only if a 2nd binary appears)
├── src/
│   ├── main.rs                parse args, load config, pick backends
│   ├── runtime.rs             main loop: events → screens → display
│   ├── app/                   ── pure logic: no I/O, no hardware, unit-testable
│   │   ├── action.rs          Action, InputEvent
│   │   ├── event.rs           Event (Input | Service | Tick)
│   │   └── anim.rs            Tween, easing
│   ├── ui/                    ── drawing toolkit; knows pixels, not hardware
│   │   ├── canvas.rs          Canvas = 960×540 Pixmap + helpers
│   │   ├── text.rs            embedded fonts, glyph cache, wrap/ellipsis
│   │   ├── theme.rs           colours, type scale, spacing, safe area, motion
│   │   ├── icons.rs           vector app icons (play, search, folder, gear)
│   │   ├── nav.rs             Screen trait, Navigator (screen stack); in ui/ because it draws transitions
│   │   └── widgets/           status_bar, list, grid, card, toast, spinner, osk
│   ├── screens/               ── the "apps"; each implements Screen
│   │   ├── registry.rs        AppEntry list → launcher panels
│   │   ├── launcher.rs        panel carousel
│   │   ├── youtube/           search, results grid, player
│   │   ├── files/             browser, text/image viewers
│   │   ├── settings/          sections + detail pages
│   │   └── search/            Google: query, results, reader
│   ├── services/              ── background workers; never touch the UI
│   │   ├── mod.rs             ServiceHub, Request, ServiceEvent, Ticket
│   │   ├── http.rs            fetch + image decode
│   │   ├── youtube.rs         yt-dlp helper-process client
│   │   ├── player.rs          mpv process + JSON IPC
│   │   ├── bluetooth.rs       BlueZ over D-Bus
│   │   ├── network.rs         iwd over D-Bus, connectivity probe
│   │   ├── storage.rs         directory listing, file ops
│   │   └── system.rs          clock, device info, power actions
│   ├── input/                 ── anything that produces Actions
│   │   ├── mod.rs             InputSource trait, spawning
│   │   ├── keymap.rs          key code → Action (configurable)
│   │   ├── evdev.rs           Linux keyboards (/dev/input), hotplug
│   │   └── remote/            phone link: protocol.rs, ws.rs, (later) ble.rs
│   ├── platform/              ── display backends
│   │   ├── mod.rs             Display trait
│   │   ├── kms.rs             DRM/KMS (Linux)
│   │   └── window.rs          dev-PC simulator window (+ keyboard)
│   ├── config.rs              Settings model, layered load, atomic save
│   └── logging.rs             tiny logger + in-memory ring buffer
├── assets/fonts, assets/icons  embedded at compile time (include_bytes!)
├── tools/
│   ├── ytdl_helper.py         persistent yt-dlp JSON-lines helper
│   └── remote_sim.py          phone-protocol test client for the dev PC
├── deploy/
│   ├── pi-os-lite/            setup script, orc4.service, config.txt/cmdline snippets
│   └── buildroot/             BR2_EXTERNAL tree (Phase 6)
├── prototype/                 the Python code, kept for reference until YouTube parity
└── docs/
```

**Why this differs from the suggested layout.** The suggested `display/`, `hardware/`, `bluetooth/`, `network/`, `files/` and `settings/` all still exist. They are grouped by **role**, so the dependency direction is enforced by the folder structure:

```
          main.rs (wiring)
             │
   ┌─────────┼───────────────┬──────────────┐
screens ──► ui ──► app       input        platform
   │                ▲          │              │
   └──► services ───┘          └──► app ◄─────┘ (platform only sees ui::Canvas)
```

- `app` depends on nothing (named `app`, not `core`, to avoid clashing with Rust's built-in `core` crate).
- `screens` never import `platform`, `input` sources or OS APIs.
- `services` never import `ui` or `screens`.
- Phone code lives in `input/remote` and never touches the UI.

"Files" splits naturally into a screen (`screens/files`) and a worker (`services/storage`). "Settings" splits into the model (`config.rs`) and the screens.

---

## 4. Core types

### 4.1 Input: actions, not keys

```rust
pub enum Action {
    Up, Down, Left, Right,
    Select,            // Enter / phone tap
    Back,              // Esc / phone back
    Home,              // long-press Back / dedicated phone button
    Menu,              // context options for the focused item
    Text(TextEdit),    // from phone keyboard or physical keyboard
    Media(MediaKey),   // PlayPause, Seek(i32), VolumeUp, VolumeDown
}
pub enum TextEdit { Insert(String), Backspace, Submit, Clear }

pub struct InputEvent { pub action: Action, pub repeat: bool, pub source: SourceKind }
pub enum SourceKind { Keyboard, Phone, Simulator }   // diagnostics only; screens never branch on it

pub trait InputSource: Send + 'static {
    fn run(self: Box<Self>, out: Sender<Event>);      // owns its thread; sends Event::Input
}
```

- The keyboard and the phone are both `InputSource`s feeding the same channel. A source can be added or removed without touching any screen.
- Key → Action mapping lives in `keymap.rs` and is overridable in config.

### 4.2 Event loop: one UI thread, one channel

```rust
pub enum Event {
    Input(InputEvent),
    Service(ServiceEvent),
    Tick,                                 // minute-change for the clock
    Shutdown,
}
```

```text
loop:
    timeout = if animating { 0 (present() paces to vblank) } else { until next minute }
    drain all pending events (recv_timeout, then try_recv)
        Input   → navigator.handle(action)
        Service → navigator.dispatch(event)  (stale tickets dropped)
    if dirty or animating:
        navigator.draw(&mut canvas)
        display.present(&canvas)          // page flip; blocks until vblank
```

- With nothing happening, the process sleeps in `recv_timeout`. It uses 0 % CPU and is woken by input, a service result or the minute tick.
- All sources and services hold a clone of `Sender<Event>` (`std::sync::mpsc`; no async runtime in the core).

### 4.3 Screens and navigation

```rust
pub trait Screen {
    fn handle(&mut self, a: &Action, cx: &mut Cx) -> Nav;          // Nav::Unhandled lets defaults apply
    fn on_service(&mut self, e: &ServiceEvent, cx: &mut Cx) -> Nav { Nav::Stay }
    fn animate(&mut self, now: Instant) -> bool { false }          // true = still moving
    fn draw(&self, c: &mut Canvas, t: &Theme);
    fn on_show(&mut self, cx: &mut Cx) {}
    fn on_hide(&mut self, cx: &mut Cx) {}
    fn text_request(&self) -> Option<TextRequest> { None }         // "I want text input"
}

pub enum Nav { Stay, Unhandled, Push(Box<dyn Screen>), Pop, Replace(Box<dyn Screen>), Home }

pub struct Navigator { stack: Vec<Box<dyn Screen>>, transition: Option<Transition>, dirty: bool }
```

- **Uniform defaults.** `Back` + `Unhandled` → `Pop`. `Home` → pop to the launcher.
- **Transitions are owned by the navigator,** so every screen gets identical push/pop motion for free.
- `Cx` gives screens a `ServiceHub` handle, read access to `Settings`, the status model and a toast queue.
- `text_request()` is how a screen says "a text field is focused". The phone link forwards it so the phone pops up its keyboard.

### 4.4 App registry: adding an app is one entry

```rust
pub struct AppEntry {
    pub id: &'static str,
    pub name: &'static str,
    pub icon: Icon,
    pub accent: Color,              // used for large areas only (see §1.1)
    pub needs_network: bool,        // panel shows an "Offline" badge; app still opens and explains
    pub open: fn(&mut Cx) -> Box<dyn Screen>,
}
pub fn apps() -> Vec<AppEntry> { vec![youtube::entry(), search::entry(), files::entry(), settings::entry()] }
```

Panel order and visibility can later come from config (e.g. hide YouTube).

### 4.5 Services: request/response with tickets

```rust
pub struct Ticket(u64);                      // monotonically increasing per request

pub enum Request {
    YtSearch { query: String, count: u32 },
    FetchImage { url: String, w: u32, h: u32 },
    ListDir { path: PathBuf },
    Play { source: MediaSource },  PlayerCmd(PlayerCmd),
    BtScan(bool), BtConnect(Address), WifiScan, WifiConnect { ssid: String, psk: Option<String> },
    WebSearch { query: String },  FetchArticle { url: String },
    Power(PowerAction),
}

pub enum ServiceEvent {
    Done { ticket: Ticket, result: Result<Reply, UserError> },
    Status(StatusUpdate),                   // push updates: network, bt, battery, clock, player
}

pub enum UserError { Offline, Timeout, NotFound, Unavailable(&'static str), Storage, Internal }
```

- **Stale-result rule.** A screen remembers the ticket it is waiting on and ignores any other. A new search supersedes the old one, and leaving a screen orphans its tickets. This structurally removes the prototype's re-entrancy bugs.
- **One thread per service** with an inbox (`Sender<(Ticket, Request)>`). The HTTP service has a small pool (2 threads) for thumbnails.
- Heavy helpers (yt-dlp, mpv) are **child processes**. A crash in them can't take down the UI, they're killed to reclaim RAM, and they're updated independently of the binary.
- `UserError` holds friendly, display-ready categories. Raw error detail goes to the log only.

---

## 5. Display architecture

```rust
pub trait Display {
    fn present(&mut self, frame: &Canvas) -> Result<()>;   // vsync-paced
    fn set_blank(&mut self, blank: bool) -> Result<()>;    // sleep / wake
    fn suspend(&mut self) -> Result<()>;                   // release DRM master (video handoff)
    fn resume(&mut self) -> Result<()>;                    // re-acquire, re-set mode, redraw
}
```

**`platform::kms` (Linux)**

1. Open the DRM card that has the configured connector (`Composite-1` now, `HDMI-A-1` later). Pick its mode from config (`720x576i`/`720x480i`; test both on the board).
2. Allocate two XRGB8888 dumb buffers **at the logical size (960×540)**. Attach them to the primary plane with `dst` set to the full mode, so the **HVS scales**. If the driver refuses plane scaling, fall back to output-size buffers with a CPU scale on present.
3. Each frame: convert RGBA → XRGB into the back buffer (row-wise, ~1–2 ms; later only the dirty rect), then page flip and wait for the flip event.
4. Mirroring for the optics: plane `rotation` property (`reflect-x`/`reflect-y`) if supported, otherwise CPU flip on convert.
5. Safe area: not a display concern. It's a theme inset, calibrated by the user.

**`platform::window` (dev PC):** a minifb window at 960×540 that delivers keyboard events as an `InputSource`, with an optional composite-preview filter.

**Video handoff.** The player screen calls `display.suspend()`, launches mpv on DRM, and shows nothing itself. When mpv exits it calls `resume()`. Two processes can't both own DRM; a clean handoff is simpler and more robust than embedding a decoder.

---

## 6. UI design system

| Token | Value | Why |
|---|---|---|
| Background | `#000000` | Transparent on see-through optics, darkest on LCoS, no glare |
| Text primary / secondary / disabled | `#F2F2F2` / `#A6A6A6` / `#5C5C5C` | Luma-only contrast survives composite |
| Surface (cards) | `#1A1A1A` | Separation without lines |
| Focus | 4 px white ring + 1.04× scale | Unmistakable at low resolution; ≥ 2 px for interlace |
| App accents | YouTube red, Search blue, Files amber, Settings grey | Large tinted areas only, never text |
| Type | IBM Plex Sans SemiBold (UI) + IBM Plex Mono (numbers/clock) | Retains the prototype's family; Sans reads better than Mono for words |
| Type scale (logical px) | Display 72 · Title 44 · Body 32 · Caption 26 · **min 24** | Composite ≈ 440 effective columns |
| Safe area | default 56 px sides / 40 px top-bottom, user-calibrated | Analog overscan |
| Motion | panel slide 220 ms ease-out-cubic; focus 120 ms; "reduce motion" = cross-fade | FLCOS colour break-up |
| Density | ≤ 4 grid items or ≤ 5 list rows per screen | Glanceable; matches the prototype's 2×2 instinct |

Screen layouts:

```
LAUNCHER                                   LIST SCREEN (Files / Settings)
┌──────────────────────────────────────┐   ┌──────────────────────────────────────┐
│ ◖ wifi  bt                     14:32 │   │ ‹ Files                    14:32     │
│                                      │   │                                      │
│ ▌     ┌──────────────────┐        ▐  │   │  ▸ Music                     12 items │
│ ▌ ◂   │                  │    ▸   ▐  │   │ ┃▸ Videos                     3 items┃│  ← focused row
│ ▌     │       ICON       │        ▐  │   │  ▸ notes.txt                    2 KB │
│ ▌     └──────────────────┘        ▐  │   │  ▸ photo.jpg                  1.2 MB │
│             YouTube                  │   │                                      │
│            ●  ○  ○  ○                │   │                              3 / 14  │
└──────────────────────────────────────┘   └──────────────────────────────────────┘
 neighbours peek at the edges, dimmed; Enter opens, Esc/Back steps out one level
```

---

## 7. Applications

**Launcher.** A carousel built from the registry. The centred card is about 520×320 with icon and name below. The previous and next cards peek dimmed at the edges. Page dots show position. Network-dependent apps show an "Offline" badge but still open.

**YouTube.**
- Flow: search (text from the phone or keyboard) → 2×2 grid. Spatial navigation flows across pages, as in the prototype. Lazy "load more", deduped **before** fetching. Thumbnails come from `i.ytimg.com/vi/<id>/mqdefault.jpg` (320×180, exactly the right size), loaded asynchronously, with a placeholder card until they arrive.
- Resolution: `services::youtube` talks to `tools/ytdl_helper.py`. This is a **persistent** Python process using the JSON-lines protocol, so yt-dlp's import cost is paid once per session. It starts when YouTube opens and stops 60 s after you leave. The helper is behind a `VideoResolver` trait, so a remote or cloud resolver can replace it.
- Format: `bv*[vcodec^=avc1][height<=480]+ba[ext=m4a]/b[vcodec^=avc1][height<=480]`.
- Playback: mpv with `--hwdec=v4l2m2m-copy` on DRM, audio to the Bluetooth ALSA device, and generous `--cache` (radio contention). The OSD (title, progress) uses mpv's own overlay via IPC.
- ⚠ **Risk:** recent yt-dlp releases need an external JavaScript runtime (e.g. Deno) for full YouTube support. Deno on a 512 MB Pi is unproven. This gets a spike (S1) before any YouTube Rust code is written. If it fails, the resolver moves off-device (phone or small cloud service). That would be a justified offload, because it's about scraping fragility, not compute.

**Search ("Google").**
- Flow: query → result list (title + domain + one-line snippet) → **reader view**. The reader fetches the page and extracts its main text with a readability-style heuristic. It renders natively in large type, paginated with ↑/↓, with no images and a size cap.
- The provider is behind a `SearchProvider` trait. The concrete API is chosen at that milestone, after checking current terms and quotas: Google Programmable Search, Brave Search API, etc. Scraping google.com's HTML is excluded because it's fragile and against ToS.

**Files (offline).**
- Root is `/data/files`, configurable. It shows a folder list with size/count.
- Viewers: text (paginated), images (fit-to-screen, decoded on a worker), audio/video through the same player service.
- Delete with confirmation.
- Later: USB gadget mass-storage mode, so a PC can copy files onto the glasses over the Zero's OTG port. Also phone upload.

**Settings (offline).**

| Section | Contents |
|---|---|
| Display | safe-area calibration (arrow-key edge adjustment), mirror X/Y, TV norm (PAL/NTSC), reduce motion, UI dimming |
| Bluetooth | paired devices, scan & pair, connect/disconnect, audio output |
| Network | Wi-Fi list, connect (password via phone or keyboard; on-screen keyboard fallback), status, IP |
| Phone | pairing code, connected phone, unpair |
| Sound | volume |
| Device | version, storage, memory, CPU temp, uptime, **recent logs viewer** |
| System | restart ORC-4, reboot, shut down, reset settings |

---

## 8. Phone integration

```
React Native app ──(WebSocket over Wi-Fi, v1)──┐
                 ──(BLE GATT, later)───────────┤──► input/remote ──► Event::Input(Action)
                                               │◄── state/text_request/status (outbound)
```

- **Protocol** is versioned JSON and transport-independent:
  - Phone → glasses: `{"t":"hello","v":1,"token":"…"}`, `{"t":"action","a":"left"}`, `{"t":"text","op":"insert","s":"cats"}`, `{"t":"time","unix":…}`
  - Glasses → phone: `{"t":"screen","id":"youtube.search","text_request":{"prompt":"Search YouTube","value":""}}`, `{"t":"status","wifi":…,"bt":…}`
- **Security:** pairing shows a 6-digit code on the glasses, and the phone stores the issued token. Unpaired clients are ignored. Without this, anyone on the same Wi-Fi could drive the glasses.
- The **phone sets the clock** on connect, which solves the no-RTC problem when offline.
- **WebSocket first** because it's testable from the PC (`tools/remote_sim.py`) and React Native has it built in. **BLE GATT later** because it works without shared Wi-Fi. Both feed the same protocol handler.
- Sync `tungstenite` on a std thread, so no async runtime.

---

## 9. Bluetooth, network and system services

- **Bluetooth audio: BlueZ + bluez-alsa.** This is much lighter than PipeWire for one job: the Pi acting as an A2DP source to headphones. mpv outputs to `bluealsa`. Device management goes over BlueZ D-Bus via `zbus` (pure Rust, blocking API), with a "just works" pairing agent.
- **Wi-Fi: iwd on both Pi OS Lite and Buildroot.** That gives one implementation, a clean D-Bus API and fast association. NetworkManager is disabled on the dev image.
  - Connectivity states: `Offline / Connecting / Online{ssid, rssi}` via a lightweight HTTP 204 probe.
  - Network-dependent screens subscribe to these states and show a friendly offline state instead of failing.
- **Power/battery:** `PowerSource` trait with a `None` implementation today, which hides the battery icon. No fake "100%".
- **Clock:** shown only once time is known (NTP, phone, or saved from the last run).

---

## 10. Settings and persistence

```rust
#[derive(Serialize, Deserialize, Default)] #[serde(default)]
pub struct Settings {
    pub version: u32,
    pub display: DisplaySettings,   // connector, mode, mirror_x/y, safe_area, reduce_motion, dim
    pub input: InputSettings,       // keymap overrides, repeat
    pub audio: AudioSettings,       // volume, preferred bt sink
    pub phone: PhoneSettings,       // paired tokens
    pub files: FilesSettings,       // root
    pub apps: AppsSettings,         // order/hidden
}
```

- **Layered loading:** compiled defaults → `/etc/orc4/config.toml` (image) → `/data/orc4/settings.toml` (user).
- `#[serde(default)]` everywhere, so old files keep loading after an upgrade.
- **Saving** is debounced (1 s) and atomic: write `settings.toml.tmp`, then fsync, then rename. This is safe against power loss.
- A corrupt file is logged, moved aside, and defaults are used. A bad config can never brick the device.

---

## 11. Error handling and logging

| Layer | Strategy |
|---|---|
| Services | `anyhow` internally. Mapped to `UserError` at the boundary. A panicking service thread is caught and restarted with backoff. |
| Screens | Show `UserError` as friendly copy ("No internet connection", "YouTube is unavailable right now"). Never raw strings. |
| Hardware | Missing input device → wait for hotplug. Display init failure → log and retry. Missing Bluetooth/Wi-Fi → those settings sections show "Unavailable"; everything else works. |
| Process | A fatal error or panic exits non-zero. systemd restarts it within 1 s (`Restart=always`). |
| Logging | `log` facade + ~50-line logger: stderr (journald on Pi OS, file on Buildroot) plus a 200-line in-memory ring buffer shown in Settings → Device → Logs. Level via config or `ORC4_LOG`. |

---

## 12. Dependencies (deliberately small)

| Crate | Purpose | When |
|---|---|---|
| `tiny-skia` | 2D rasterising (AA shapes, images, gradients) | M1 |
| `fontdue` | glyph rasterising | M1 |
| `log`, `anyhow` | logging facade, error context | M0 |
| `serde`, `toml` | settings | M0 |
| `minifb` | dev-PC window (non-Linux-target builds) | M0 |
| `drm`, `evdev` | KMS display, Linux input (Linux only) | M3 |
| `tungstenite`, `serde_json` | phone link | M5 |
| `ureq` (rustls), `zune-jpeg`, `png` | HTTP, thumbnails | M6 |
| `zbus` | BlueZ, iwd | M8 |

No async runtime, no GUI framework, no C libraries in the core.

## 13. Resource targets

| Metric | Target |
|---|---|
| ORC-4 RSS (idle) | < 40 MB (2 MB canvas, 2× buffers, glyph cache, ≤ 50 thumbnails ≈ 12 MB) |
| Idle CPU | ~0 % (no redraws) |
| Frame time during animation | < 10 ms on the Zero 2 W |
| Binary start → first frame | < 300 ms |
| Power-on → UI | Pi OS Lite ≈ 12–15 s; Buildroot ≈ 5 s |

## 14. Testing strategy

- **Unit tests** for `app`, keymaps, grid navigation math and config migration.
- **Snapshot tests:** render each screen headlessly to PNG and compare with golden images. This catches visual regressions with no hardware.
- **Scripted input tests:** feed `Action` sequences to the navigator and assert on the resulting state.
- **On-device checks** per milestone: frame time, RSS, boot time.
