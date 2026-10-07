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

Opens a 960×540 window. Controls: arrow keys, Enter = select, Esc = back,
Home = home, Tab = menu.

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
