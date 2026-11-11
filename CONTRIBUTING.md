# Contributing to Sanaga

Thank you for wanting to help! This guide explains how to set up the project,
the conventions we follow, and how to send changes.

By taking part you agree to follow our [Code of Conduct](CODE_OF_CONDUCT.md).

## Ways to help

- **Report a bug** — open an issue with the steps to reproduce it, what you
  expected, what happened, and your Linux distribution, desktop (GNOME, KDE,
  Hyprland…), Wayland or X11, and WebKitGTK version.
- **Suggest a feature** — open an issue describing the need first, before
  writing code, so we can agree on the approach.
- **Translate** — see [Translations](#translations).
- **Add devices** to the mobile simulator — see [Devices](#devices-for-the-mobile-simulator).
- **Fix or improve code** — issues labelled `good first issue` are a good start.

## Setting up

Install the requirements listed in the [README](README.md#install), then:

```sh
cargo build          # debug build (optimised enough to be pleasant)
cargo run -- https://example.com
cargo test           # unit tests (no display needed)
cargo clippy --all-targets   # must stay free of warnings (CI refuses them)
```

`cargo fmt` uses the default Rust style with a long line width (see
`rustfmt.toml`); please run it before sending changes.

## Conventions

- **Rust 2021, no `unsafe`** unless there is no other way (and then explain why
  in a comment next to it).
- **Comments say why, in plain English.** Function and module comments describe
  what the person using Sanaga sees, not only what the code does.
- **Keep it light.** Every feature should cost nothing while it is not used:
  no timers or threads running for nothing, no page started before it is shown.
- **Interface texts** are written in French in the code with `tr("…")` /
  `tf("…{}…", &[&value])`, and translated in `assets/i18n/translations.txt`
  (see below). A pull request that adds a text must add its translations.
- **No third-party artwork.** Logos, brand icons and pictures from other
  products must not be added. The device frames and browser bars of the
  simulator are drawn with code (`src/frames.rs`).
- **Tests** go next to the code (`#[cfg(test)] mod tests`). Pure logic (parsing,
  layout, file formats) should have one. Code that reads files or pages from
  outside gets a `fuzz_one` entry point too: see [docs/FUZZING.md](docs/FUZZING.md).

## Project layout

See [How it is built](README.md#how-it-is-built) in the README. In short:
`src/shell.rs` is the GTK window, `src/page.rs` a web page, `src/app/` the whole
interface (one file per area: `toolbar.rs`, `tabs.rs`, `menus.rs`, `devices.rs`…).

## Translations

1. Add or update lines in `assets/i18n/translations.txt`. Each line holds one
   text in five languages, separated by `⟂`:

   ```
   français ⟂ English ⟂ Español ⟂ Português ⟂ Deutsch
   ```

   Keep `{}` placeholders in the same order; write `\n` for a line break.
2. Check and rebuild the tables:

   ```sh
   python3 tools/i18n.py check   # lists texts used in the code but not translated
   python3 tools/i18n.py build   # writes assets/i18n/{en,es,pt,de}.tsv
   ```

To add a new language, add a column to every line, its code to `LANGS` in
`tools/i18n.py`, and its table and name to `src/i18n.rs`.

## Devices for the mobile simulator

Devices are listed in `DEVICES` in `src/app/devices.rs`:

```rust
d("Brand", "Model name", css_width, css_height, Phone, Android, Style::Punch),
```

Use the device's **CSS viewport size in portrait** (what `window.innerWidth`
and `screen.height` report on the real device), its kind (`Phone`, `Tablet`,
`Computer`), its system (`Ios`, `Android`, `Desktop`) and the frame style that
looks closest (`Island`, `Notch`, `Classic`, `Punch`, `Curved`, `Tablet`,
`Laptop`, `Monitor`). Please give a source for the sizes in your pull request.

## Testing your change by hand

`docs/TESTING.md` lists the switches Sanaga reads to test the real window
without touching your own data: a separate data folder (`XDG_DATA_HOME`), a test
profile (`--profile Test`), scripted clicks and keys (`SANAGA_LIVE_TEST`) and
window pictures (`SANAGA_SHOT_DIR`).

## Sending a change

1. Fork the repository and create a branch from `main`
   (`git switch -c fix/tab-menu`).
2. Keep commits focused; write messages that explain the reason for the change.
3. Make sure `cargo build`, `cargo test`, `cargo clippy` and
   `python3 tools/i18n.py check` pass.
4. Open a pull request using the template; add a screenshot or short video for
   anything visible.

A maintainer will review it. We may ask for changes — that is a normal part of
the process, not a judgement on your work.

## License

By contributing, you agree that your contributions are released under the
project's [MIT License](LICENSE).
