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
