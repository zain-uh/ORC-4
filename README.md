# ORC-4

Smart-glasses operating environment for a Raspberry Pi Zero 2 W driving a
0.24" 960×540 FLCOS display. Written in Rust; draws directly to the display
with no desktop stack.

Design docs: [`docs/`](docs/). The original Python prototype is kept for
reference in [`prototype/`](prototype/).

## Run the simulator (dev PC)

```bash
cargo run
```

Opens a 960×540 window. Controls: ←/→ = move, Enter = open/select,
Esc = back, Home = back to launcher, Tab = menu, F2 = composite-video preview.
Close the window to quit (Back on the home screen does nothing, as on the
glasses).

Key bindings can be changed in `dev-data/settings.toml`:

```toml
[input.keys]
space = "select"
```

Debug logging: set `ORC4_LOG=debug`. Settings live in `dev-data/` (git-ignored).

## YouTube

Searching uses the [yt-dlp](https://github.com/yt-dlp/yt-dlp) program. Install it once:

```bash
winget install yt-dlp.yt-dlp
```

Then open a new terminal so it is on PATH. Without it, the YouTube app shows
"YouTube support (yt-dlp) isn't installed" and everything else keeps working.
Thumbnails download with Windows' built-in `curl.exe` on the dev PC (the
device uses a Rust HTTPS client).

## Phone link (dev remote)

The glasses accept a paired phone over WebSocket (protocol in
`src/input/remote/protocol.rs`). Until the React Native app exists, use the
dev remote:

1. Run the simulator (`cargo run`). It listens on `ws://127.0.0.1:4004`.
2. Open `tools/remote.html` in Chrome or Edge and press Connect.
3. In the simulator: Settings › Phone › Pair a phone. Type the 6-digit code
   into the remote and press Pair.

The remote then drives the UI, types into text fields, and sets the clock
and time zone. On the glasses, the link listens on all interfaces; see
Settings › Phone › Address.

## Checks

```bash
cargo test
cargo clippy --all-targets
```

## Windows note

This repo builds with the GNU Rust toolchain on Windows (no Visual Studio
needed). One-time setup:

```bash
rustup toolchain install stable-x86_64-pc-windows-gnu
rustup override set stable-x86_64-pc-windows-gnu
```
