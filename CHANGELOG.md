# Changelog

## v0.1.0 — first published port (LightCraft 0.6.0)

| | |
|---|---|
| APK | `lightcraft-0.6.0-arm64-v8a.apk` (21.3 MB) |
| sha256 | `90199f68dff6c0344356a410161eb40e852a913ecc028ce825ee3bb177fffb7f` |
| Built from | upstream `storytold/lightcraft` @ `76b5377868d912db69a777bda253a5b1b96b22bd` + `patches/0001-lightcraft-android-port.patch` + `apps/lightcraft-android/` |
| Package | `ai.storyteller.lightcraft` — versionName `0.6.0`, minSdk 26, targetSdk 35 |
| ABI | `arm64-v8a` only |
| Signature | throwaway local test key, not an official key |

### In this release

- Android shell crate: NativeActivity entry point, a `Services` implementation filling the app's
  asynchronous picker, a file logger, preferences written while the app runs (atomic), modifier-key
  synthesis, an Activity that is not recreated on configuration change, launcher icon and the
  edge-to-edge theme variant.
- **The file dialogs work.** The shell registers `Services::picker`, and every `pick::ask` call site
  was read and confirmed to reach it; the capability gates that looked only at a synchronous slot were
  widened, and the export dialog's folder button and the import review's copy-to folder go through the
  app's own `ask_inline` pair, so no menu item draws a button that does nothing.
- **HEIC opens** — `heif` is a default feature here (upstream's own photo-format gap), proven by
  decoding libheif's `example.heic` into a real PNG rather than by compiling.
- **`File ▸ Quit` ends the process** rather than leaving a stale close request that looks like a hang
  and cannot be reopened until the app is cleared from Recents.
- **Settings are actually saved.** The shell creates its settings folder before installing the writer:
  `prefs_watch` writes its temporary file *inside* the destination folder, so a missing one was an
  `ENOENT` every two seconds and no preference was ever persisted. Found by running the app in the
  emulator, not by building it — see `docs/PORT-NOTES.md`.
- A fresh install is usable: `HOME` points at the app sandbox (Android sets neither `HOME` nor
  `USERPROFILE`, which otherwise dead-ended on "There is no home folder to keep the library in"), and
  a new library is seeded with the demo photos.

### Known issues

See *Known gaps* in the [README](README.md) and [`docs/PORT-NOTES.md`](docs/PORT-NOTES.md).
