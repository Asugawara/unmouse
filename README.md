![Demo: double tap right Command and every cell of the screen gets a label of j, k, l and ;; typing a label narrows the grid and moves the pointer onto the Save button of a dialog](docs/images/demo.png)

<h1 align="center">unmouse</h1>

<p align="center"><b>Move the mouse pointer from the keyboard on macOS.</b></p>

English | [日本語](README.ja.md)

unmouse is a small background tool for macOS. Double tap the right Command key and every cell on every monitor gets a label made only of `j` `k` `l` `;`. Type the label of the place you want, and the pointer moves to the center of that cell. Click with your mouse or trackpad as usual, or click with a key.

It is for people who want to keep their hands on the home row, and for anyone who wants a keyboard way to reach a button, a link or a corner of the screen without a trackpad swipe.

## What you see

![Every cell of the screen labelled with letters of j, k, l and ;, over three windows](docs/images/hero.png)

*Right after the trigger: every cell has a label, and the next letter to type is bigger and colored.*

![After two letters, only the matching region keeps its labels](docs/images/narrow.png)

*After two letters, only the matching region is left, and the grid inside it is finer.*

![Two monitors labelled as one canvas](docs/images/monitors.png)

*Two monitors are one canvas. A cell on the border is labelled on the monitor that holds most of it.*

## Quick start

**Requirements**: macOS on Apple Silicon or Intel (the release binary is universal). Verified on macOS 15.6.

> [!IMPORTANT]
> Read this before you grant the permission.
>
> - unmouse needs Accessibility (and, on some systems, Input Monitoring). It watches every key event so that it can catch the trigger and swallow the keys you type while its overlay is open.
> - It does not record or send what you type. It has no networking dependencies (see `Cargo.toml`), and the source is here to read.
> - `unmouse run --verbose` prints only the keys unmouse itself consumed. A key passed on to another app is printed as `keyDown passthrough`, without its code.

1. **Install.**

   ```sh
   curl -fsSL https://raw.githubusercontent.com/Asugawara/unmouse/main/install.sh | sh
   ```

   This downloads the release, verifies its checksum (`SHA256SUMS`), puts the binary at `~/.local/bin/unmouse`, then runs `unmouse install`, which copies and signs the binary and registers it as a login item.

2. **Grant the permission.** The first start asks for Accessibility. Open System Settings > Privacy & Security > Accessibility and allow `unmouse`. If it does not start after you allow it, run `unmouse restart`.

3. **Try it.** Double tap the right Command key, then type the label of a place on the screen.

4. **Check the setup.** `unmouse doctor` shows the permission, the event tap, Secure Input, your config and the monitors.

## What it is good for

**Home-row keys only.** Labels use `j` `k` `l` `;`, so your fingers do not leave the home row. The next letter to type is bigger and colored (blue, green, red, magenta), so you can find your place.

**All monitors as one canvas.** There is no step to pick a monitor. Every monitor is labelled at once.

**Click from the keyboard, or just move.** Type the label and the pointer moves to the center of the cell. Press `Space`, `d`, `s` or `a` while you are typing to left-click, right-click, double-click or middle-click at the center of that range. `g` starts and ends a drag. Or click with the mouse or trackpad after the pointer has moved.

**Aim finer with `refine`.** With `refine = true`, the overlay stays after the label is complete and shows the four keys again at the edges of the cell. Keep typing `j` `k` `l` `;` to narrow it down, then click with `Space` and the others.

![After the label is complete, refine keys appear at the edges of the cell around the pointer](docs/images/refine.png)

*`refine = true`: the cell is chosen, and `j` `k` `l` `;` narrow it further.*

**Make it look the way you like.** Label size, layout, colors, weight and background opacity are in the config file.

## Keys

| Key | Action |
|---|---|
| `j` `k` `l` `;` | Type a label (with `refine = true`, they also narrow the cell after it is chosen) |
| `Space` / `d` / `s` / `a` | Left click / right click / double click / middle click, then close |
| `f` | Move the pointer to the center of the range without clicking, then close (same as the trigger while typing) |
| `g` | Start and end a drag |
| `h` or `Backspace` | Go back one letter |
| `Esc` | Cancel |
| The trigger again (double tap right Command) | Close (while typing, move to the center of that range; while dragging, cancel) |

You can change the action keys in `[actions]`. While the overlay is open, a key that is not in the table does nothing and does not reach other apps. The screen does not show a key guide, so use this table.

## Configuration

The config file is `~/.config/unmouse/config.toml`. If `$XDG_CONFIG_HOME` is set, unmouse reads `unmouse/config.toml` under it instead. Without a file, the defaults are used.

See `config.example.toml` for every option. These are the ones people change most:

- `hotkey`: the trigger (default `"double:right_cmd"`). Top level. A double tap of a modifier is `"double:right_cmd"` (also `left_cmd`, `right_option`, `left_option`, `right_shift`, `left_shift`, `right_ctrl`, `left_ctrl`). A key combination is written like `"ctrl+alt+j"`.
- `double_tap_ms`: the longest gap between the two taps, and the longest time for one press (ms, 100 to 1000, default 300). Top level. A key press, a mouse move, a click or a scroll between the two taps starts the count again (so holding right Command with Karabiner and using the mouse does not trigger it).
- `label_size`: the label font size (pt, 6 to 48). In `[style]`.
- `label_layout`: `"center"` (default) puts the label as one group in the middle of the cell. `"inline"` puts it at the top left of the cell. `"corners"` puts one letter in each of the four corners (only for 4-letter labels; otherwise it is the same as `"center"`). In `[style]`.
- `key_colors`: the colors of the next letter to type (four `"#rrggbb"` values, in the order of `grid.keys`). The defaults are blue for `j`, green for `k`, red for `l` and magenta for `;`. The next letter is bold (change the weight with `next_weight`) and takes the color of its key. In `[style]`.
- `next_scale`: the size ratio of the next letter to type (the colored one). Default `1.5`, from `1.0` to `3.0`. Big and small letters share a baseline. Refine keys (`refine = true`) get the same ratio, for the next one only. In `[style]`.
- `next_weight`: the weight of the next letter. One of `"regular"` / `"semibold"` / `"bold"` / `"heavy"` / `"black"`. Default `"bold"`. It has no effect when you set a `font` name. In `[style]`.
- `label_bg_opacity` / `label_bg_end_opacity`: the opacity of the white label background. It is `label_bg_opacity` (default `0.72`) up to the first letter and fades toward `label_bg_end_opacity` (default `0.25`) at the right end. Use the same value for a flat background. The top and bottom of the background follow the biggest letter. In `[style]`.
- `max_cell`: the biggest size of one cell (pt, `[width, height]`). A bigger value makes coarser cells. In `[grid]`.
- `subdivide`: how cells are divided. `"half"` (default) gives the two cells of the left column (upper and lower) 5-letter labels and the other six cells 6-letter labels, inside each frame fixed by four letters (the fifth letter `l` is the upper row, `;` the lower row). With two monitors (3840×1080) that is 1024 cells, a mix of 5 and 6 letters. `"off"` divides evenly: 512 cells on two monitors, all 5 letters. `"quarter"` adds one more level of quarters: 2048 cells, all 6 letters. In `[grid]`.
- `scope`: `"all_monitors"` (default) treats all monitors as one screen. `"cursor_monitor"` divides only the monitor that has the pointer. In `[grid]`.
- `refine`: `false` (default) closes the overlay when the label is complete. `true` keeps it open and shows refine keys; keep typing `j` `k` `l` `;` to narrow down, and click with `Space` `d` `s` `a` at that position. In `[grid]`.

## Support and limits

- **macOS only.** The release binary is universal (Apple Silicon and Intel). Verified on macOS 15.6.
- **Secure Input blocks the trigger.** While a password field has focus, macOS hides key events from other apps. `unmouse doctor` shows whether Secure Input is on.
- **Permission after each update.** The release binary is ad-hoc signed, so after you update, macOS asks for the permission again. Remove the old `unmouse` entry in the Accessibility list and add it again. If you build with your own signing identity (`UNMOUSE_SIGN_IDENTITY`), you do not need to grant it again.
- **launchd does not pass your shell environment.** `XDG_CONFIG_HOME` set in your shell is not seen by the login item. Use `~/.config/unmouse/config.toml`.
- **`kill` is not restarted.** If you stop unmouse with `kill`, it stays stopped. Only a crash restarts it.
- **CLI messages and `--help` are in Japanese for now.**

## Details

### How it works

An event tap watches key events. When the trigger fires, unmouse opens one non-activating panel per monitor and draws the grid on it, so the app you were using keeps the focus. While the overlay is open, the keys it uses are swallowed and everything else is ignored. The pointer moves and the clicks are sent as CGEvents. The grid and label logic is pure code that does not touch the system. Nothing goes over the network.

### Auto start

`unmouse install` copies the binary to `~/Library/Application Support/unmouse/unmouse`, signs it and registers it as a login item. It starts at every login. Stop any `unmouse run` you started by hand before `install`: an old build holds no lock, so you would get two.

- To start it again, run `unmouse restart`. To remove it, run `unmouse uninstall`.
- After you build again, run `install` once more. `restart` does not switch to the new build.
- The permission is granted to the copied binary itself, not to your terminal. The first time, a permission dialog appears. If it does not start after you allow it, run `unmouse restart` (a process that waits for the permission can miss the change).
- Without `UNMOUSE_SIGN_IDENTITY`, the signature is ad-hoc. When the contents of the binary change (for example after `cargo build`), running `install` again drops the permission (the same contents keep it). If it is dropped, remove the old `unmouse` from the list in System Settings and add it again. A self-signed certificate named in `UNMOUSE_SIGN_IDENTITY` removes the need to grant it again.
- The log is `~/Library/Logs/unmouse.log`.

### Subcommands

With no arguments, `unmouse` runs `run` (the resident process).

| Command | What it does |
|---|---|
| `unmouse run [--verbose]` | Stay resident and start on the trigger (default). On start it prints the permission, the tap and the trigger to standard error. `--verbose` prints each key event it receives, one line each |
| `unmouse run --wait-for-permission` | Wait until the permission is granted, then stay resident. The login item runs in this form |
| `unmouse install` | Copy and sign the binary, and register it to start at login |
| `unmouse uninstall` | Remove the login item and the copied binary (the log stays) |
| `unmouse restart` | Restart the registered process |
| `unmouse doctor` | Check the permission, the event tap, Secure Input, the config and the monitors |
| `unmouse layout` | Show the monitors, the label length and the cell size of each monitor |
| `unmouse cursor` | Show the current pointer position |
| `unmouse path <x> <y>` | Show the label of the given point |
| `unmouse demo --keys "<keys>" [--delay-ms <ms>] [--allow-click]` | Start without the event tap and feed the keys in order. Example: `--keys "l l k ; f"`. Click keys (`space d s a g`) are refused unless you add `--allow-click` |

### Install options

`install.sh` reads these environment variables:

| Variable | Meaning |
|---|---|
| `UNMOUSE_VERSION` | The version to install (default: the latest release) |
| `UNMOUSE_BIN_DIR` | Where to put the binary (default: `$HOME/.local/bin`) |
| `UNMOUSE_BASE_URL` | The releases URL (default: `https://github.com/Asugawara/unmouse/releases`) |
| `UNMOUSE_NO_SERVICE=1` | Only install the binary; skip `unmouse install` |

If the version is already installed, the download is skipped. If `~/.local/bin` is not on your `PATH`, the script prints how to add it.

### Build from source

You need Rust 1.89 or later.

```sh
cargo build --release
target/release/unmouse install
```

To make an `.app` bundle instead, run `scripts/bundle.sh`. It builds `target/release/bundle/Unmouse.app`. Set `UNMOUSE_SIGN_IDENTITY` to sign with your own identity; without it the signature is ad-hoc (`-`).

### Update

Run the installer again. If macOS asks for the permission again, grant it again (see Support and limits).

### Uninstall

Run `unmouse uninstall`, then `rm ~/.local/bin/unmouse`. The config (`~/.config/unmouse/config.toml`) and the log (`~/Library/Logs/unmouse.log`) stay; remove them by hand if you want.

### Doctor

Something wrong? Run `unmouse doctor`. It shows whether Accessibility is trusted, whether the event tap can be created, whether Secure Input is on, whether your config loads, and the monitors it found.

## Development

```sh
cargo build
cargo test
cargo clippy --all-targets -- -D warnings
```

The screenshots are generated, not captured. This draws the real overlay code over a synthetic desktop with synthetic monitors and the default config, so nothing from your machine appears:

```sh
cargo run --release --example screenshots -- --out docs/images
```

The `screenshots` workflow regenerates them on pushes to `main` that touch `src/`, `examples/` or `Cargo.lock` (or by hand: `gh workflow run screenshots`), and opens a `docs: update screenshots` pull request when they change.

Releases:

- Conventional commits on `main` make release-please open a release PR.
- Merging that PR creates the tag and a draft release.
- The release workflow waits for `ci` to pass on that commit.
- It then builds `unmouse-<version>-macos-universal.tar.gz` and `SHA256SUMS`.
- Last, it uploads them and publishes the release.

To build the release files locally, run `sh scripts/build-release.sh <version> out`.

## License

MIT, see [LICENSE](LICENSE).
