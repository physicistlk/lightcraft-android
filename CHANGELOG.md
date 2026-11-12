# Changelog

## 2026-11-12

- Re-ported to upstream lightcraft **v0.21.1** (`361e3bc0c`, the release that carries the 0.21.0
  feature bump plus a patch update) and rebuilt the signed APK:
  `lightcraft-0.21.1-arm64-v8a-20261112.apk`, 27.1 MiB, sha256
  `012cd83891cfe2d201288191058739033e838309416b94b76e5ee7fbafb5a046`.
- The port patch applied cleanly to v0.21.1 with no conflicts and **no hand re-port**: all eight
  port call sites still live in the same files, and 0.21.0 added no new synchronous-picker call
  sites that would need the background-picker gate widened.
- The shipped patch was compared against the live `git diff HEAD` before resetting — they were
  byte-identical, so no staged hunk was missing this round; `pick.rs` (`ask_inline` /
  `take_inline`) is in the regenerated patch (`grep -c ask_inline` → 7) and was recovered intact.
- `cargo check -p lightcraft-android --target aarch64-linux-android` clean (one pre-existing
  dead-code warning in `prefs_watch`). APK verified by hand (the port has no Linux verifier):
  `aapt2 dump badging` → `versionName 0.21.1`, `native-code: 'arm64-v8a'`, minSdk 26, targetSdk
  35; `apksigner verify` → v2 and v3 true, signer `CN=LightCraft Android Local Test`;
  `llvm-nm` on the packaged `.so` → `android_main` and `ANativeActivity_onCreate` exported;
  every LOAD segment `0x4000`-aligned.
- Regenerated patch re-checked against a pristine `v0.21.1` archive: `git apply --check` →
  `PATCH APPLIES CLEAN`.

## 2026-11-09

- Re-ported to upstream lightcraft **v0.20.0** (`192ff729`) and rebuilt the signed APK:
  `lightcraft-0.20.0-arm64-v8a-20261109.apk`, sha256
  `0c8b138e924fcf4f164d47f656ed9b36f50eb095d8f1e69d33baf8bef8f56323`.
- The port patch applied cleanly to v0.20.0; no hunk had to be moved or rewritten by hand this
  round (all eight port call sites still live in the same files as v0.19.0).
- Fixed the canonical patch itself: it had been generated with plain `git diff`, which left out the
  staged `crates/ui-egui/src/pick.rs` change (`ask_inline` / `take_inline`). It is now produced with
  `git diff HEAD` and carries that hunk, so a fresh apply to v0.20.0 gives a buildable tree.
- `cargo check -p lightcraft-android --target aarch64-linux-android` clean (one pre-existing
  dead-code warning in `prefs_watch`); APK verified: versionName `0.20.0`, `native-code:
  'arm64-v8a'`, signed with the local test key.

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
## 2026-11-05

- Port updated to lightcraft v0.18.1.
