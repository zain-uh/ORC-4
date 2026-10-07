# ORC-4 — Phase 1: Python Prototype Analysis

Scope: every tracked file at `a658755`, plus files deleted in history
(`gui.py`, `pi_panel.py`, recovered from `d453d55`/`cb0cf16`).

## 1. What it currently does

| File | Status | Purpose |
|---|---|---|
| `home.py` | working | Tkinter 960×540 window. Two icon panels (YouTube, Files). ←/→ slide between them, Enter opens the selected one. Only YouTube actually opens. |
| `youtube.py` | partly working | Search box → `yt_dlp` `ytsearchN` → downloads every thumbnail → 2×2 grid with paging, spatial arrow navigation across pages, "load more" by re-searching with a larger N. Enter calls `webbrowser.open(url)`. Esc/B go back. |
| `video.py` | empty | Placeholder for playback. |
| `orc4.service` + `install_service.sh` | working on a desktop Pi | systemd unit that launches `home.py` into an **X11 session** (`DISPLAY=:0`, `.Xauthority`, `graphical.target`). |
| `assets/frame0/*.png` | used | Two ~410 px grey rounded-square icons (YouTube, folder). |
| `HELP ig/IBMPlexMono-SemiBold.ttf` | **not actually used** | Referenced by name but never installed. Tk silently falls back to its default font. |
| `pi_panel.py` (deleted) | abandoned | First concept: a label carousel that launched `chromium-browser --kiosk youtube.com` and `pcmanfm`. |

Flow today:

```
Tk window (needs X11 + desktop)
  └─ home_gui: [YouTube] [Files]   ←/→ slide, Enter
        └─ youtube.start: search → (blocking) yt-dlp + N thumbnail downloads
              └─ 2×2 grid → Enter → webbrowser.open()  ← dead end on the device
```

## 2. What is incomplete

- **Video playback does not exist.** `webbrowser.open` needs a desktop browser. On the target it does nothing useful. `video.py` is empty.
- **Files** has a panel and icon but no implementation.
- **Settings and Google** don't exist.
- **Text input** works only with a physical keyboard focused on a Tk `Entry`.
- **No real status.** The "100%" battery text is a hardcoded string. There's no clock, Wi-Fi or Bluetooth state.
- **No display configuration.** Nothing sets the HDMI/DSI mode, rotation, mirroring or overscan for the FLCOS driver board.
- **No Bluetooth, audio, networking or phone integration.**
- **No config or persistence** of any kind.

## 3. Current architecture

- Two modules talk through **module-level globals** and by passing a `home_gui` callback into `youtube.start`.
- Screen switching = `for w in root.winfo_children(): w.destroy()` followed by a full rebuild (PNGs reloaded each time).
- YouTube state is one `dict` captured by ~25 nested closures inside `start()`.
- Input is bound directly to Tk key events (`<Left>`, `<Return>`, `"b"`). Key meaning lives inside each screen.
- **Everything runs on the Tk UI thread,** including network I/O. Progress is painted by calling `root.update()` re-entrantly from inside event handlers.
- Process model: systemd → X server → Python → Tk.

## 4. Current UI

- **White** background, dark-grey (`#282828`) text, red YouTube accent.
- Launcher: one large icon centred, with the next icon half-visible at the right edge. That partial view of the next panel is a good affordance.
- Slide animation: 10 px steps every 2 ms via `after()`, linear.
- YouTube: centred title + search field + button. Results are a 2×2 grid of 300×168 thumbnails with titles below, `<`/`>` hints at the edges and a 4 px outline as the focus ring.
- Font sizes 20–48 px. These were sensibly chosen large for the tiny panel.

## 5. Hardware interactions

Essentially none:

- **Display:** assumes an X11 desktop at 960×540. No mode-setting.
- **Input:** a keyboard via X11.
- **Network:** `requests` and `yt_dlp` over whatever connection the OS has.
- **Audio, Bluetooth, power, storage:** none.

## 6. Bugs found

**Severe**

1. **The UI freezes during all network work.** `do_search` runs `yt_dlp.extract_info` and then **16 sequential thumbnail downloads (8 s timeout each)** on the UI thread. Offline or on bad Wi-Fi, this can lock the device for minutes.
2. **Re-entrancy.** `root.update()` inside handlers processes queued key events *during* a search. Pressing Enter/Esc/arrows while it says "Searching…" can start a second search, or navigate away while closures still mutate `state` on a destroyed canvas.
3. **"Load more" re-downloads every thumbnail.** In `fetch_more_results`, `_build_entry_dict(entry)` (which downloads the image) runs **before** the `key in seen` dedupe check. Each "load more" re-fetches all 32/48/50 thumbnails and appends them to `state["photos"]`, so memory grows on every load.
4. **The boot service blocks the offline experience.** `After=network-online.target` + `Wants=network-online.target` delays the UI until the network is up, or until the ~90 s wait-online timeout with no Wi-Fi. This directly contradicts the "must work offline" requirement.
5. **The boot service requires a full desktop.** `DISPLAY=:0`, `.Xauthority` and `graphical.target` mean X11 and a desktop session must be running, which contradicts the "embedded device" goal.

**Moderate**

6. **The custom font is never loaded.** "IBM Plex Mono SemiBold" isn't installed (it sits in `HELP ig/`), so every font silently falls back.
7. **Animation is CPU-paced, not frame-paced.** 48 `after(2ms)` steps can't run at 2 ms on a Pi Zero. Duration and smoothness vary with load. Motion is linear with no easing, there's no vsync, and it tears.
8. **Panel geometry is inconsistent.** Icons sit at x=495 and x=951 (456 px apart) but the slide moves 480 px. Neither panel is ever centred: off by 15 px, then by 9 px.
9. **Oversized thumbnails.** `thumbs[-1]` is the *largest* thumbnail (often 1280×720), which is then LANCZOS-resized on the Pi CPU. A 320 px variant exists and should be requested instead.
10. **Raw errors are shown to the user.** `f"Search failed.\n{exc}"` puts a raw exception string on a 0.24" display.

**Minor / hygiene**

- The launcher's maximum index is hardcoded (`MAX_INDEX = 1`), so adding an app means editing several constants.
- `__pycache__/*.pyc` is committed and there's no `.gitignore`.
- `requirements.txt` is unpinned, and yt-dlp breaks often.
- `gui.py` used a Windows path separator (fixed in `home.py`).

## 7. Architectural problems (why patching won't scale)

- **No separation of concerns:** UI, input mapping, networking and navigation are all inside one closure.
- **No navigation model:** "back" is hand-coded per screen by destroying and rebuilding the world.
- **No input abstraction:** physical keys are bound directly in screens, so a phone can't drive it without faking X11 key events.
- **No concurrency model:** blocking I/O sits on the UI thread with re-entrant repaint hacks.
- **The stack is too heavy for the target:** X11 + Tk + Python + Pillow + requests + yt-dlp all stay resident. Boot to UI is likely 25–40 s on a Zero 2 W.
- **The colour scheme is wrong for glasses** (see §9).

## 8. What to retain

- **Horizontal panel carousel** with ←/→ and Enter, plus the **partial view of the neighbouring panel** at the screen edge.
- **One big target per screen** and very large type. The prototype's instinct for size was right.
- **YouTube results as a 2×2 grid of large thumbnails.** Few large items is the correct density for this display.
- **Spatial navigation that flows across pages** (→ on the right column goes to the next page) and **lazy "load more"**.
- **Hierarchical back:** results → search → launcher.
- **Explicit loading, empty and error states.**
- **The IBM Plex family** as the typeface (properly embedded this time).
- **systemd-supervised autostart with restart-on-failure,** as a concept.

## 9. What to redesign

| Area | Prototype | Redesign |
|---|---|---|
| Runtime | Python + Tk + X11 | One Rust binary drawing straight to DRM/KMS. No X, no window manager. |
| Concurrency | Blocking I/O on UI thread | UI thread never blocks. Services run on worker threads and report back via messages. |
| Input | Tk key bindings per screen | Device-agnostic `Action`s (Left/Right/Select/Back/Text…). Keyboard and phone are interchangeable sources. |
| Navigation | destroy-and-rebuild | Screen stack with a uniform Back/Home behaviour. |
| Colour | White background | **Black background.** On see-through optics black = transparent, white = a glowing rectangle over the world. On LCoS, dark UI also reduces glare and eye strain. Light text, one accent colour. |
| Animation | Timer steps | Time-based, eased, vsync-paced. Short durations (field-sequential LCoS shows colour break-up on fast motion). |
| Video | `webbrowser.open` | Native player (mpv with hardware H.264 decode), audio to Bluetooth. |
| Text entry | Physical keyboard only | Phone keyboard as primary; physical keyboard for dev. |
| Boot | after network-online, graphical.target | Starts at the earliest point. Network is optional and comes up in the background. |
| Assets | Loose PNGs + uninstalled TTF | Font and icons embedded in the binary. |
