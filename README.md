# SiliconTherm

SiliconTherm is a terminal-based tool for monitoring thermal sensors and battery status on macOS.

It is designed for developers who want to:

- View CPU, GPU, and battery temperatures in real time.
- Sort, filter, and search sensors quickly.
- Use a lightweight diagnostic view without a desktop GUI.

The application is implemented in Rust and uses a TUI (Terminal UI) built with `ratatui` and `crossterm`.

## Project scope

- Sensor reading through Apple SMC.
- Classification by section (`CPU`, `GPU`, `Battery`).
- Per-sensor visual meter and global summary in the header.
- Battery metrics (capacity, voltage, current, cycle count, power).
- Keyboard and mouse interaction (section switching and column sorting).

## Requirements

- macOS.
- Rust toolchain installed (`cargo`, `rustc`).
- ANSI color-capable terminal.

## Quick start

From the repository root:

```bash
cargo run
```

You can also provide the refresh interval in seconds:

```bash
cargo run -- 1.0
```

If not provided, the default value is `2.0s`.

## Interface controls

- `Tab` / `Shift+Tab`: switch section (`CPU`, `GPU`, `Battery`).
- `1`, `2`, `3`: switch section directly.
- `Arrow Up/Down`, `PgUp/PgDn`, `Home/End`: navigate sensors.
- `/` or `F3`: search.
- `n` / `N`: next or previous match.
- `F4`: toggle active-sensor filtering.
- `F6`: change sorting mode.
- Click table headers: sort by column.
- `+` / `-`: adjust refresh interval.
- `Space`: pause/resume refresh.
- `q` or `F10`: quit.

## Development workflow

Basic commands:

```bash
cargo fmt
cargo test
cargo run -- 2.0
```

Recommended workflow:

1. Keep changes small and scoped.
2. Run `cargo fmt` before validation.
3. Run `cargo test` on each iteration.
4. Test the TUI manually with `cargo run`.

## Notes

- `unsafe` usage is limited to system API boundaries (IOKit/CoreFoundation and SMC).
- Behavior depends on the sensors available on each Mac model.
