# LightCraft on Android — port notes

LightCraft 0.6.0 (eframe/egui 0.36, edition 2024) built as a signed arm64-v8a APK, using the recipe
the seven sibling ports of this suite established (DeckCraft, VectorCraft, WordCraft, GridCraft,
PdfCraft, PhotoCraft, SoundCraft).

```
apps/lightcraft-android/
  Cargo.toml          the shell crate: lib naming, features, [package.metadata.android]
  res/values/styles.xml   the two edge-to-edge themes
  res/mipmap-*/ic_launcher.png   the app's own hicolor PNGs at 48/72/96/144/192
  src/lib.rs          android_main, Services, the prefs writer, the quit path
  src/file_picker.rs  the in-app picker (from the shared template)
  src/modifiers.rs    keyboard modifiers (from the shared template)
  src/prefs_watch.rs  preferences written while the app runs (from the shared template)
```

Build:

```bash
. "$HOME/projects/shared-android-port/android-env.sh"
export CARGO_BUILD_JOBS=4
CARGO_APK_RELEASE_KEYSTORE="C:/Users/Admin/android-tools/lightcraft-sideload.keystore" \
CARGO_APK_RELEASE_KEYSTORE_PASSWORD=android \
  bash "$HOME/projects/shared-android-port/build-apk.sh" lightcraft lightcraft-android
```

## The seam

`lightcraft_ui_egui::Services` is a struct of `Option<Box<dyn Fn…>>` fields deriving `Default`, so a
platform that cannot supply a capability simply omits it. Nothing in `lightcraft-engine` or
`lightcraft-ui-egui` is patched apart from the handful of call sites listed below.

`cargo check -p lightcraft-ui-egui --target aarch64-linux-android` passes **unchanged** — and
`crates/ui-egui` has no eframe dependency at all, so the `accesskit` trap eframe `compile_error!`s
alongside `android-native-activity` cannot be reached: this shell is the only eframe in the graph and
never asks for it. (The same was true of DeckCraft and the rest.)

## Every menu item that needs a path

LightCraft's asynchronous seam is `Services::picker`:
`Box<dyn FnMut(pick::PickRequest) -> Result<Receiver<Vec<String>>, String>>`. `pick::ask` consults it
**first** and, when it answers, `pick::poll` runs the asking command again with the path filled in.
So the picker is registered and every File / Export / Import item works:

| Menu | command | route |
|---|---|---|
| File ▸ Import Photos… | `file.addPhotos` | `pick::ask` → picker |
| File ▸ Import from Folder… | `file.addFolder` | `pick::ask` → picker |
| File ▸ Import Lightroom Catalog… | `file.importLightroom` | `pick::ask` → picker |
| File ▸ Find Missing Photos… | `file.findMissing` | `pick::ask` → picker |
| File ▸ Import Profiles & Presets… | `file.importPresets` | `pick::ask` → picker |
| File ▸ Export Presets… | `file.exportPresets` | `pick::ask` → picker |
| File ▸ Import / Export Keywords… | `file.importKeywords` / `file.exportKeywords` | `pick::ask` → picker |
| File ▸ Import / Export Point Curve Presets… | `file.importCurvePresets` / `file.exportCurvePresets` | `pick::ask` → picker |
| File ▸ Export (with a preset) | `app.export` | no folder dialog needed |
| File ▸ Contact Sheet PDF… | `app.contactSheet` | `pick::ask` → picker |
| Settings ▸ General ▸ Open / New Library… | `app.openLibrary` / `app.newLibrary` | `pick::ask` → picker |
| Photo ▸ Auto-Tag from Tracklog… | `photo.tagFromTracklog` | `pick::ask` → picker |
| Photo ▸ Locate in Folder… | `photo.locate` | `pick::ask` → picker |
| Grid ▸ Move to Folder… | `photo.moveToFolder` | `pick::ask` → picker |
| Export dialog ▸ Folder ▸ Choose… | (dialog, not a command) | `pick::ask_inline` → picker |
| Import review ▸ Copy to ▸ Choose… | (dialog, not a command) | `pick::ask_inline` → picker |

The synchronous slots (`pick_files`, `pick_folder`, `pick_save`-equivalents, `pick_preset_files`, …)
stay **`None`**: `rfd` has no Android backend, and a slot answering "cancelled" would leave buttons
that do nothing. Leaving them absent is how the app hides its own commands.

That decision has a consequence, so it is worth being precise about: a capability *gate* that looked
only at a synchronous slot (`ui_enabled`'s `app.services.pick_folder.is_some()`) would keep a menu
item greyed out even though its command goes through the picker. Those gates were widened to
`… || app.services.picker.is_some()` — the shape `panels/grid.rs` already had — and the two dialog
buttons that really need a path from the user go through the picker by the new
`pick::ask_inline` / `pick::take_inline` pair (a dialog cannot be "run again" the way a command can,
so its answer is read by the dialog that asked, a frame or two later).

## Upstream files changed

Seven files in `crates/ui-egui`, all small, all needed for the picker to be reachable:

| file | change |
|---|---|
| `src/pick.rs` | `ask_inline` / `take_inline` + `poll` hands a dialog's answer back — the only new mechanism |
| `src/menus.rs` | `ui_enabled`: `photo.tagFromTracklog` and `app.openLibrary \| app.newLibrary \| file.addFolder` accept `picker.is_some()` |
| `src/panels/dialogs.rs` | the Export dialog's folder "Choose…" goes through the picker when there is no synchronous slot |
| `src/import.rs` | the import review's "Copy to / Move to" destination "Choose…" likewise |
| `src/panels/edit.rs` | the curve-preset Import/Export buttons are enabled by `picker.is_some()` too |
| `src/panels/settings.rs` | Settings ▸ General ▸ Open/New Library… likewise (they run `app.openLibrary`/`app.newLibrary`) |
| `src/panels/library_problem.rs` | "Choose Another Library…" likewise (it runs `app.openLibrary`) |

None of these change behaviour on the desktop: `rfd`'s synchronous slots are still registered there,
so every call site keeps its existing path, and `ask_inline` is only reached when they are absent.

## Features

- `heif` is a **default feature** of the shell crate (`lightcraft-codecs/heif`). The official builds
  are documented as passing `--features heif` ("Open HEIC/HEIF photos … Not default so distributors
  choose it (HEVC patents)"), and HEIC is what a phone's camera roll is full of. Without it a port
  reports every iPhone photo as failed.
  **Proven by decoding, not by compiling**: `lightcraft-cli render` on libheif's
  `examples/example.heic` (718 KB, magic `ftypmif1`/`heic`) written a real PNG — see the README/port
  report for the exact command and output.
- `android-native-activity` on — without a backend feature, `android-activity` is pulled with no
  backend and the build dies with `file not found for module activity_impl`.
- `NativeOptions::android_app = Some(app)` — eframe returns an error without it.
- The engine's `sam` and `denoise` features are **off**: each pulls a large ML stack not exercised
  on Android. The catalog, RAW/standard decodes, the develop pipeline and every export format are in.

## Fonts

Nothing to register, and that was checked rather than assumed. LightCraft has no
`set_platform_font_files` / `set_app_font_dir` / `system_font_dirs` anywhere — that is the ArtCraft
suite's text crate, not this one. `ui_egui::theme::install_fonts_with_chinese` installs Inter and the
craft-fonts faces **embedded in the binary**, so there is no system font folder to point at. (The
build warns when `CRAFT_FONTS_DIR` is unset, in which case CJK text renders as boxes; set
`CRAFT_FONTS_DIR=../craft-fonts` to embed them. That is a build input, not an Android bug.)

## Where the library, the settings and the log live

The engine resolves its own defaults from the environment and Android sets neither `HOME` nor
`USERPROFILE`, so `config_dir()` and `library::default_dir()` both came out empty and the app opened
onto "There is no home folder to keep the library in" — a dead end on first launch. `android_main`
points `HOME` at `internal_data_path()` (the app's own sandbox: always writable, no permission):

- settings `<data>/.config/lightcraft/ui.json`
- log `<data>/.config/lightcraft/logs/lightcraft.log`
- library `<data>/Pictures/LightCraft Library` (catalog, `Originals/`, `thumbs/`, presets)
- default export folder `<data>/Pictures/LightCraft Exports`

A brand-new library is seeded with the procedural demo photos, as on the desktop, so the app is
usable the moment it installs instead of opening onto an empty grid.

## The Android gaps, honestly

- **No Android picker sheet.** The Storage Access Framework needs `startActivityForResult`, which
  `android-activity` 0.6 does not expose. Dialogs are drawn with egui.
- **File ▸ Quit ends the process.** `ViewportCommand::Close` ends nothing on Android (egui-winit
  turns it into a plain `ViewportEvent::Close` and winit's window API is stubbed), so the shell ends
  the process after saving — checked in both `logic` and `ui`, because a menu click lands during
  `ui` and no repaint is guaranteed afterwards.
- **No system clipboard** (`arboard` has no Android backend), **no Open With / Show in File Manager /
  open URL** (`open` likewise). Those services stay `None`, which is how the app hides their commands.
- **A few dialog-internal "Choose…" buttons stay hidden rather than appear dead**: the display
  profile chooser (Settings ▸ Display, Develop ▸ Soft Proof), the watermark graphic in the Export
  dialog, the AI model installers (Settings ▸ Faces / Denoise), the sidebar's Browse Folder… and the
  Auto-Import / Smart-Previews folder pickers. Their slots are `None`, so the app draws no button.
- **No stylus pressure or tilt.** A pen degrades to ordinary touch.
- **Touch layout is unverified here.** The picker compiles for Android and its permissions verify in
  the manifest, but the tap targets and the scrolling are only real on a device.

## The settings folder has to be *created*, not assumed (found on the emulator, fixed)

The first build of this port ran, opened the library and looked healthy — and wrote

```
[WARN] lightcraft::prefs_watch couldn't write the preferences: No such file or directory (os error 2)
```

every two seconds, for as long as it ran. `<data>/.config/lightcraft` never existed: the engine's
`config::config_dir()` is a read-only *lookup* (on the desktop the folder is created elsewhere, on
the way out), and `prefs_watch::write_atomic` writes its temporary file **inside** the destination
folder before renaming it, so a missing folder is an `ENOENT` on every write. Nothing else failed —
the log, the library and the demo photos all worked, the app simply never saved a preference, which
is the single thing `prefs_watch` exists to prevent.

`config_dir()` now creates the folder. This is the one bug in this port that no compile-time check
could have caught and that the artifact checks could not see either; it took running the app.
