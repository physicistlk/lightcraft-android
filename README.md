# LightCraft for Android (arm64-v8a)

An unofficial Android port of **[LightCraft](https://github.com/storytold/lightcraft)** — the real
engine and egui interface, packaged as a NativeActivity APK for sideloading on arm64 devices.

| | |
|---|---|
| Upstream project | [`storytold/lightcraft`](https://github.com/storytold/lightcraft) — Apache-2.0 OR MIT |
| Pinned commit | `76b537786` (upstream version `0.6.0`) |
| Android package | `ai.storyteller.lightcraft` |
| Published APK | `lightcraft-0.6.0-arm64-v8a.apk` — 21.3 MB, arm64-v8a, signed |
| APK sha256 | `90199f68dff6c0344356a410161eb40e852a913ecc028ce825ee3bb177fffb7f` |
| Upstream files changed | 7 |
| New port crate | `apps/lightcraft-android/` — about 1 400 lines of Rust plus resources |

## What this repository is

This is **not a fork** of LightCraft. It holds only the Android layer, so it stays small, readable,
and obviously separable from upstream:

```
apps/lightcraft-android/   the new shell crate — copy it into your checkout of upstream
patches/                   the diff against upstream — apply it after checking out the pin
docs/PORT-NOTES.md         what was changed and why, fix by fix, with how each one was checked
scripts/                   cross-compile environment, build and verify scripts (from shared-android-port)
CHANGELOG.md               what is in the published APK
```

The Android layer is not a reimplementation. It is a shell crate that fills the same `Services` struct
the desktop binary fills, so the real engine, the real RAW/HEIC decoders and the real egui UI run
unchanged.

## Build it

Prerequisites: Rust (rustup) with the `aarch64-linux-android` target, Android SDK + NDK r27, a
JDK 21, and `cargo install cargo-apk`.

```bash
git clone https://github.com/storytold/lightcraft.git
cd lightcraft
git fetch --depth 1 origin 76b5377868d912db69a777bda253a5b1b96b22bd
git checkout FETCH_HEAD                        # the exact commit this port targets
git apply /path/to/0001-lightcraft-android-port.patch
cp -r /path/to/lightcraft-android apps/        # the shell crate from this repository

# any keystore of your own signs an installable APK; a throwaway test key is fine for sideloading
keytool -genkeypair -keystore ks.jks -alias lightcraft -keyalg RSA -keysize 2048 -validity 10000 \
        -storepass android -keypass android -dname "CN=Local Test, O=Local, C=US"
export CARGO_APK_RELEASE_KEYSTORE="$PWD/ks.jks" CARGO_APK_RELEASE_KEYSTORE_PASSWORD=android
cargo apk build --release -p lightcraft-android
```

`scripts/android-env.sh` is the cross-compile environment this port was built with (the NDK linker
and the C compiler the native dependencies need, and the x86_64 linker for an emulator build),
`scripts/build-apk.sh` wraps the build, and `scripts/verify-apk.sh` checks the artifact:
`aapt2 dump badging`, `apksigner verify`, and `llvm-nm` on the `.so` inside the APK.

## Install

```bash
adb install -r lightcraft-0.6.0-arm64-v8a.apk
```

or copy the APK to the device and open it (installing from unknown sources must be allowed).

On first run, grant **Settings ▸ Apps ▸ LightCraft ▸ Special access ▸ All files access** if you want
the in-app picker to reach photos outside the app's own folders. Without it, Android 10+ refuses a
`read_dir` of shared storage by path, and the picker says so on those roots — naming the permission and
the Settings path — rather than showing them as empty.

## Verification status

| Claim | How it was checked |
|---|---|
| Installs and launches | `adb install -r`, then the app's own log (`/data/data/ai.storyteller.lightcraft/files/lightcraft.log`) starts with `LightCraft 0.6.0 starting on Android` |
| The library opens and photos render | the log reports `library …: 24 photos`; a screen grab shows the Develop view with a **decoded, rendered photograph**, its histogram, `ISO 100 · 18mm · f/2.8 · 1/250`, and the full edit panel (Light, Curve, Color, Effects) |
| **The file picker works, end to end** | File ▸ *Import from Folder…* → the picker above the editor. `Download` first reported that shared storage needs "All files access" and named the Settings path; after granting it and tapping *Refresh* the folder showed `(2 files)`, *Choose this folder* handed the path back to LightCraft, whose own **Import Photos** dialog found and selected the pushed `test-photo.jpg` (and correctly ignored a `.wav`). After *Import 1 Photo* the photo was in the filmstrip and rendering. |
| **`File ▸ Quit` ends the process** | the log ends with `File > Quit: ending the process`, `pidof` is empty, the task is gone from `dumpsys activity activities`, and the app relaunches cleanly |
| **Preferences survive** | `files/.config/lightcraft/ui.json` (the docking layout and settings) is written **while the app runs**, atomically, and the log holds no write failures after the fix described in `docs/PORT-NOTES.md` |
| **HEIC decoding works** | `heif` is a default feature of the port crate, and it is proven by *decoding*: `lightcraft-cli render` on libheif's genuine `example.heic` (718 114 bytes, `ftyp` magic) wrote a real 419 907-byte 512×342 PNG of a photograph. The HEVC decoder is statically linked into the shipped arm64 `.so` (`lightcraft_heif…ycc8_to_rgb`, `heic_rs…hevc…Frame` symbols in the APK) |
| The artifact is what it claims | `aapt2 dump badging` → package `ai.storyteller.lightcraft`, `versionName 0.6.0`, `minSdk 26`, `targetSdk 35`, `native-code: 'arm64-v8a'`; `apksigner verify --verbose` → v2 and v3 true; `llvm-nm --dynamic --defined-only` on the `.so` inside the APK → `android_main` and `ANativeActivity_onCreate`; the injected `android.app.lib_name` is `lightcraft`, matching `liblightcraft.so`; `configChanges=0x40001ff8`; the theme resolves to `attr/windowFullscreen`; the quit and picker strings are `grep -a`'d out of that `.so` |

**Not verified:** the port's modifier-key tracking was not exercised in the emulator (`adb` cannot
deliver a *held* modifier), and the picker's touch targets and scrolling are only real on a tablet.
Engine GPU compute is used only when the saved preference asks for it; the emulator has no GPU, so the
CPU pipeline ran there. Anything about a specific phone's driver is not something an emulator can
answer.

## Known gaps

- **No Android picker sheet.** The Storage Access Framework needs `startActivityForResult`, which
  `android-activity` 0.6 does not expose, so dialogs are drawn with egui.
- **A few dialog-internal `Choose…` buttons are hidden rather than shown dead** (their synchronous
  slots are `None`, which is how the app hides such a command): the display-profile chooser, the
  watermark graphic in the Export dialog, the AI model installers, the sidebar's *Browse Folder…*, and
  the Auto-Import / Smart-Previews folder pickers.
- **The SAM 3 mask and denoise features are off**: each pulls a large ML stack that is not exercised
  on Android. Catalog, RAW/standard decodes, the develop pipeline and every export format are in; the
  AI panels degrade to "not included in this build".
- **No system clipboard**, no Open With / Show in File Manager, no open-URL (`arboard` and `open` have
  no Android backend), so those services stay `None`.
- The library, settings and log live in the app sandbox (`HOME` is pointed at it), because Android
  sets neither `HOME` nor `USERPROFILE` — without that the app opened onto "There is no home folder to
  keep the library in". A fresh install seeds the library with the demo photos.

## The rest of this set

- [DeckCraft](https://github.com/kanaaka/deckcraft-android)
- [EffectCraft](https://github.com/kanaaka/effectcraft-android)
- [FilmCraft](https://github.com/kanaaka/filmcraft-android)
- [GridCraft](https://github.com/kanaaka/gridcraft-android)
- [PdfCraft](https://github.com/kanaaka/pdfcraft-android)
- [PhotoCraft](https://github.com/kanaaka/photocraft-android)
- [SoundCraft](https://github.com/kanaaka/soundcraft-android)
- [VectorCraft](https://github.com/kanaaka/vectorcraft-android)
- [WordCraft](https://github.com/kanaaka/wordcraft-android)
- [shared-android-port](https://github.com/kanaaka/shared-android-port) — the pieces every port shares

## Credits and licence

LightCraft is © its authors at [`storytold/lightcraft`](https://github.com/storytold/lightcraft) and is
dual-licensed Apache-2.0 / MIT; both licence texts are kept here. The Android layer in this repository
is an unofficial modification by [@kanaaka](https://github.com/kanaaka), carrying the same licence. Not
affiliated with or endorsed by the upstream project. See [`ATTRIBUTION.md`](ATTRIBUTION.md).
