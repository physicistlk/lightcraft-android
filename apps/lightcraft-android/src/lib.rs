//! LightCraft on Android (arm64-v8a): the real engine and egui shell, running on winit's
//! NativeActivity backend.
//!
//! # What this crate is
//!
//! A shell around the *unmodified* LightCraft crates — `lightcraft-engine` (the session, the
//! catalog, the command registry, the develop pipeline and the codecs) and `lightcraft-ui-egui`
//! (the Library grid, the Develop panels, the menus and every dialog). Nothing in those crates is
//! patched except where this port's notes say so, and those are called out below. This crate
//! supplies only what `apps/lightcraft` would normally supply: the [`Services`] implementation, the
//! preferences file, the log file, the library folder, the window options and the entry point. The
//! same shape the DeckCraft, VectorCraft, WordCraft, GridCraft, PdfCraft, PhotoCraft and SoundCraft
//! ports used.
//!
//! # Why a separate crate, and not a feature on `apps/lightcraft`
//!
//! That crate cannot build for Android as written: it carries native dialogs (`rfd`), the OS
//! clipboard (`arboard`), a control server, an X11 event-loop preference and native menu code.
//!
//! `lightcraft_ui_egui::Services` is a plain struct of `Option<Box<dyn Fn…>>` fields that derives
//! `Default`, so the fix is not to patch the app but to build a platform that supplies what it can
//! and omits the rest. Its `ui-egui` is Android-clean: `cargo check -p lightcraft-ui-egui --target
//! aarch64-linux-android` passes with no changes, and it has **no eframe dependency at all** — so
//! the `accesskit` trap eframe `compile_error!`s on cannot be reached here (this crate is the only
//! eframe in the graph and never asks for it).
//!
//! # Which half of the seam this shell fills
//!
//! `Services::picker` is LightCraft's **asynchronous** dialog seam: `pick::ask` hands it a
//! [`lightcraft_ui_egui::pick::PickRequest`], gets a `Receiver` back, returns at once, and each
//! frame `pick::poll` looks on that receiver — and when paths arrive it **runs the command again**
//! with the answer filled in. That is precisely what a dialog drawn with egui can do, so the picker
//! is registered and every File / Export / Import menu item works:
//!
//! | Menu item | command | asks through |
//! |---|---|---|
//! | File ▸ Import Photos… | `file.addPhotos` | `pick::ask` → the picker |
//! | File ▸ Import from Folder… | `file.addFolder` | `pick::ask` → the picker |
//! | File ▸ Import Lightroom Catalog… | `file.importLightroom` | `pick::ask` → the picker |
//! | File ▸ Find Missing Photos… | `file.findMissing` | `pick::ask` → the picker |
//! | File ▸ Import Profiles & Presets… | `file.importPresets` | `pick::ask` → the picker |
//! | File ▸ Export Presets… | `file.exportPresets` | `pick::ask` → the picker |
//! | File ▸ Import / Export Keywords… | `file.importKeywords` / `file.exportKeywords` | `pick::ask` |
//! | File ▸ Import / Export Point Curve Presets… | `file.importCurvePresets` / `…exportCurve…` | `pick::ask` |
//! | File ▸ Export (a preset) | `app.export` | no dialog: no folder needed |
//! | Settings ▸ General ▸ Open / New Library… | `app.openLibrary` / `app.newLibrary` | `pick::ask` |
//! | Photo ▸ Auto-Tag from Tracklog… | `photo.tagFromTracklog` | `pick::ask` |
//! | Photo ▸ Locate in Folder… | `photo.locate` | `pick::ask` |
//! | Grid ▸ Move to Folder… | `photo.moveToFolder` | `pick::ask` |
//!
//! The *synchronous* slots (`pick_files`, `pick_folder`, `save_preset_file`, …) stay `None`: `rfd`
//! has no Android backend and a slot that returned "cancelled" would leave the "Choose…" buttons
//! drawn inside dialogs on screen doing nothing. Instead, the handful of places whose capability
//! *gate* looked only at a synchronous slot were widened to `… || app.services.picker.is_some()`
//! (the shape `panels/grid.rs` already had), and the two dialog buttons that really need a path
//! from the user — Export's folder and the import review's Copy-to folder — go through the picker
//! by the small `pick::ask_inline`/`pick::take_inline` addition to `crates/ui-egui/src/pick.rs`. A
//! dialog cannot be "run again" the way a command can, so its answer is read by the dialog that
//! asked it, a frame or two later.
//!
//! # Where the library and the settings live
//!
//! The engine resolves its own defaults from the environment, and Android sets neither `HOME` nor
//! `USERPROFILE`. Left alone, `config_dir()` and `library::default_dir()` both come out empty and
//! the app opens onto "There is no home folder to keep the library in" — a dead end on first
//! launch. So `android_main` points `HOME` at `internal_data_path()` (the app's own sandbox: always
//! writable, no permission at all), which puts
//!
//! - the settings at `<data>/.config/lightcraft/ui.json`,
//! - the log at `<data>/.config/lightcraft/logs/lightcraft.log`,
//! - the library at `<data>/Pictures/LightCraft Library` (with `Originals/`, `thumbs/`, presets),
//! - the default export folder at `<data>/Pictures/LightCraft Exports`.
//!
//! A brand-new library is seeded with the procedural demo photos, as on the desktop, so the app is
//! usable the moment it installs instead of opening onto an empty grid.
//!
//! # Features
//!
//! - `heif` (**on by default**, from `lightcraft-codecs/heif`, i.e. the optional `lightcraft-heif`
//!   crate): the official LightCraft builds are documented as passing `--features heif`, and HEIC is
//!   what a phone's camera roll is full of. A port that omitted it would report every iPhone photo
//!   as failed — the one thing this app exists for. Proven with `lightcraft-cli` on a real file, not
//!   by compiling (see the port notes).
//! - `android-native-activity` **must** be on. Without a backend feature, `android-activity` is
//!   pulled with no backend and the build fails with `file not found for module activity_impl` plus
//!   `Either "game-activity" or "native-activity" must be enabled as features`.
//! - `NativeOptions::android_app` must be `Some(..)`: eframe hands it to
//!   `EventLoopBuilderExtAndroid::with_android_app` and returns an error without it.
//! - The engine's `sam` (SAM 3 object/describe masks) and `denoise` features are **not** enabled:
//!   each pulls a large ML stack that has not been exercised on Android. Everything else — the
//!   catalog, RAW and standard decodes, the develop pipeline, exports — is in.
//!
//! # Fonts
//!
//! Nothing to register, and it was checked rather than assumed: LightCraft has no
//! `set_platform_font_files` / `set_app_font_dir` / `system_font_dirs` anywhere (that is the ArtCraft
//! suite's text crate, not this one). `ui_egui::theme::install_fonts_with_chinese` installs Inter
//! and the craft-fonts faces **embedded in the binary**, so there is no system font folder to point
//! at and no "Missing Fonts" state to fix.
//!
//! # What Android cannot do yet
//!
//! Honest gaps, not oversights; the full list is in the port notes:
//!
//! - **No Android picker sheet.** The Storage Access Framework needs `startActivityForResult`,
//!   which `android-activity` 0.6 does not expose, so the dialogs are drawn with egui
//!   (`src/file_picker.rs`).
//! - **No system clipboard, no "Open With"/"Show in File Manager", no URL opening** — those services
//!   are desktop-only crates (`arboard`, `open`) and stay `None`, which is how the app hides their
//!   commands.
//! - **A few dialog-internal "Choose…" buttons stay hidden** rather than appear dead: the display
//!   profile chooser (Settings ▸ Display, Develop ▸ Soft Proof), the watermark graphic in the Export
//!   dialog, and the AI model installers (Settings ▸ Faces / Denoise). Their slots are `None`, so the
//!   app draws no button at all for them.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use lightcraft_engine::Session;
use lightcraft_ui_egui::{LightcraftApp, Services, UiState};

mod file_picker;
mod modifiers;
mod prefs_watch;

/// `internal_data_path()` captured in `android_main`, before any service closure runs. The app's own
/// sandbox: no runtime permission, which is why the settings, the library and the log land here.
static DATA_DIR: OnceLock<PathBuf> = OnceLock::new();

/// How long quitting waits for renders that are running (issue #620). A GPU render takes well under
/// a second; anything slower gives way to quitting.
const EXIT_WAIT: Duration = Duration::from_secs(2);

/// How often the preferences are looked at; a change is written at the next look. Short enough that
/// nothing a user would miss is stale, long enough that an idle app writes nothing (see
/// [`prefs_watch`]).
const PREFS_EVERY: Duration = Duration::from_secs(2);

/// The settings folder (`<data>/.config/lightcraft`), which `HOME` puts under the app's sandbox.
///
/// Created here, not assumed. `prefs_watch::write_atomic` writes its temporary file *inside* this
/// folder and renames it into place, so a folder that does not exist is an `ENOENT` on every write —
/// and because a write only happens when the state changed, that is a log line every two seconds and
/// the preferences never saved at all, which is the one thing `prefs_watch` exists to prevent. The
/// engine's own `config_dir` is a read-only lookup (the desktop creates the folder elsewhere, on its
/// way out), so the Android shell has to make it.
fn config_dir() -> Option<PathBuf> {
    let dir = lightcraft_engine::config::config_dir()?;
    let _ = std::fs::create_dir_all(&dir);
    Some(dir)
}

/// `<config>/ui.json` — the saved UI state and app settings, including the library to open.
fn prefs_path() -> Option<PathBuf> {
    config_dir().map(|d| d.join("ui.json"))
}

/// `<config>/logs/lightcraft.log` — where the desktop keeps it, so `Help ▸ Open Log Folder` names the
/// same file here.
fn log_path() -> Option<PathBuf> {
    config_dir().map(|d| d.join("logs").join("lightcraft.log"))
}

/// Read `ui.json`, mirroring the desktop's `load_prefs`. A file that exists but can't be used is
/// reported once and left where it is (the writer then keeps its defaults out of it).
fn load_prefs() -> (Option<UiState>, Option<String>) {
    let Some(path) = prefs_path() else {
        return (None, None);
    };
    let bytes = match std::fs::read(&path) {
        Ok(b) => b,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return (None, None),
        Err(e) => {
            return (
                None,
                Some(format!("The app settings ({}) couldn't be read: {e}. Defaults are used, and the file isn't overwritten.", path.display())),
            );
        }
    };
    match serde_json::from_slice::<UiState>(&bytes) {
        Ok(ui) => (Some(ui), None),
        Err(e) => (
            None,
            Some(format!(
                "The app settings ({}) are damaged ({e}); defaults are used — reopen your library with Settings ▸ Open Library… if it isn't shown.",
                path.display()
            )),
        ),
    }
}

/// What goes to `ui.json`.
fn prefs_ui_bytes(app: &LightcraftApp) -> Vec<u8> {
    serde_json::to_vec_pretty(&app.ui).unwrap_or_default()
}

/// Write the preferences if they changed, at most every [`PREFS_EVERY`] (see [`prefs_watch`]).
///
/// Nothing is written while the session is the temporary one offered when the library couldn't be
/// opened — it promises to save nothing (issue #100).
fn save_prefs_if_due(app: &LightcraftApp, writer: &mut prefs_watch::Writer) {
    if app.library_problem.is_some() {
        return;
    }
    if writer.due() {
        let bytes = prefs_ui_bytes(app);
        writer.store(&bytes);
    }
}

/// The Android `Services`: the asynchronous picker, and nothing Android has no backend for.
fn android_services(ctx: egui::Context, picker: std::rc::Rc<std::cell::RefCell<file_picker::Shared>>) -> Services {
    let _ = ctx;
    Services {
        // LightCraft's asynchronous dialog seam: `pick::ask` hands this a request, gets a receiver
        // back, and `pick::poll` runs the command again with the answer. Every File / Export /
        // Import menu item goes through here (see the module docs for the table).
        picker: Some(Box::new(move |request| file_picker::start(&picker, request).ok_or_else(|| "the in-app picker is gone".to_string()))),
        // Reads and writes go by path: the app's own folder needs no permission, and shared storage
        // needs "All files access". Both writers are the engine's atomic temp-file-and-rename.
        write: Some(Box::new(lightcraft_engine::export::write_file)),
        write_shared: Some(Arc::new(lightcraft_engine::export::write_file)),
        // The UI crate is codec-free on purpose; the host links the encoder.
        png: Some(Box::new(|img: &lightcraft_raster::Rgba8| {
            lightcraft_codecs::encode_png(&lightcraft_codecs::EncodeImage::rgba8(img), &lightcraft_codecs::EncodeMeta::default()).unwrap_or_default()
        })),
        // Help ▸ Open Log Folder names this; the item also wants `reveal`, which Android has none of.
        log_file: log_path().map(|p| p.display().to_string()),
        // Everything else stays `Default` — i.e. absent. A capability the host hasn't got is how the
        // app hides its own command; a slot that answered "cancelled" would draw a dead button.
        ..Default::default()
    }
}

/// The session the app opens with, with the library opened on disk. A brand-new library is seeded
/// with the procedural demo photos, as on the desktop, so a fresh install is usable at once.
fn open_session(library_dir: Option<PathBuf>) -> (Session, Option<lightcraft_ui_egui::panels::library_problem::LibraryProblem>) {
    use lightcraft_ui_egui::panels::library_problem::LibraryProblem;
    let unopened = || Session::new().with_fs().with_default_denoise_models().with_system_clock();
    let Some(dir) = library_dir else {
        log::error!("no library location: HOME is unset, so there is nowhere to keep it");
        return (unopened(), Some(LibraryProblem::new("", "There is no home folder to keep the library in. Choose a folder for it.")));
    };
    let t0 = std::time::Instant::now();
    let mut s = Session::new().with_fs().with_default_denoise_models().with_system_clock();
    match s.open_library(&dir, true) {
        Ok(r) => {
            let (replayed, torn) = (r.replayed, r.torn_bytes);
            log::info!(
                "library {}: {} photos, {replayed} log records replayed{} ({:.1} ms)",
                dir.display(),
                s.catalog.len(),
                if torn > 0 { ", torn tail repaired" } else { "" },
                t0.elapsed().as_secs_f64() * 1000.0
            );
            (s, None)
        }
        Err(e) => {
            log::error!("can't open library {}: {e}", dir.display());
            (unopened(), Some(LibraryProblem::new(dir.to_string_lossy(), e.to_string())))
        }
    }
}

/// A minimal file logger, so a sideloaded APK leaves a readable trace.
///
/// `logcat` is only reachable over adb; a file in the app's own data directory can be pulled with any
/// file manager, which is what a sideloaded build needs.
struct FileLogger {
    sink: Mutex<Option<std::fs::File>>,
}

impl log::Log for FileLogger {
    fn enabled(&self, _: &log::Metadata) -> bool {
        true
    }
    fn log(&self, record: &log::Record) {
        if let Ok(mut guard) = self.sink.lock()
            && let Some(f) = guard.as_mut()
        {
            use std::io::Write;
            let _ = writeln!(f, "[{}] {} {}", record.level(), record.target(), record.args());
        }
    }
    fn flush(&self) {
        if let Ok(mut guard) = self.sink.lock()
            && let Some(f) = guard.as_mut()
        {
            use std::io::Write;
            let _ = f.flush();
        }
    }
}

static LOGGER: OnceLock<FileLogger> = OnceLock::new();

fn init_logging(dir: &Path) {
    let file = std::fs::OpenOptions::new().create(true).append(true).open(dir.join("lightcraft.log"));
    let Ok(file) = file else {
        return;
    };
    let logger = LOGGER.get_or_init(|| FileLogger { sink: Mutex::new(None) });
    if let Ok(mut guard) = logger.sink.lock() {
        *guard = Some(file);
    }
    if log::set_logger(logger).is_ok() {
        log::set_max_level(log::LevelFilter::Info);
    }
}

/// The Android entry point.
///
/// `android-activity` spawns a dedicated thread for this and calls it with the activity handle.
/// `#[unsafe(no_mangle)]` with the Rust ABI is the required shape (edition 2024 requires the
/// `unsafe(..)` wrapper around `no_mangle`), and it is the symbol the injected NativeActivity looks
/// for.
#[cfg(target_os = "android")]
#[unsafe(no_mangle)]
pub fn android_main(app: winit::platform::android::activity::AndroidApp) {
    // Count this window while its loop runs (see `prefs_watch`).
    let _window = prefs_watch::counted_window();
    let data_dir = app.internal_data_path().unwrap_or_else(|| PathBuf::from("."));
    let _ = std::fs::create_dir_all(&data_dir);
    let _ = DATA_DIR.set(data_dir.clone());
    // The engine reads its own defaults out of the environment and Android sets neither `HOME` nor
    // `USERPROFILE`, so without this the settings folder, the library and the default export folder
    // are all empty and the app opens onto a dead end. `internal_data_path()` is the app's own
    // sandbox: always writable, no permission at all.
    //
    // SAFETY: `set_var` is unsafe in edition 2024 because a concurrent `getenv` racing it is a data
    // race. This runs at the very top of `android_main`, before this process reads any of the
    // variables it sets and before it starts a thread that does.
    unsafe { std::env::set_var("HOME", &data_dir) };
    init_logging(&data_dir);
    log::info!("LightCraft {} starting on Android; data dir {}", env!("CARGO_PKG_VERSION"), data_dir.display());
    // The app's external files directory: always readable and writable with no permission, so the
    // picker offers it as a root (see `file_picker`). Android 11+ hides it from USB/MTP, so it is not
    // how files arrive — the picker says so.
    if let Some(ext) = app.external_data_path() {
        let _ = std::fs::create_dir_all(&ext);
        let _ = file_picker::EXT_DIR.set(ext);
    }

    let (prefs, prefs_warning) = load_prefs();
    if let Some(ui) = &prefs {
        // the saved language from the start, so a failed start is reported in it too
        lightcraft_ui_egui::i18n::set_language(ui.language);
    }
    // the library last opened from Settings, else the default under `HOME`
    let library_dir = prefs
        .as_ref()
        .map(|u| u.settings.library_path.clone())
        .filter(|p| !p.is_empty())
        .map(PathBuf::from)
        .or_else(lightcraft_engine::library::default_dir);
    if let Some(dir) = &library_dir {
        let _ = std::fs::create_dir_all(dir);
        let _ = file_picker::LIBRARY_DIR.set(dir.clone());
    }
    // The engine's own GPU compute is a second wgpu instance; the preference decides, as on the
    // desktop, and `crates/gpu` keeps its own fallback if the driver can't do it.
    lightcraft_engine::gpu::set_enabled(prefs.as_ref().is_none_or(|u| u.settings.gpu));

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_title("LightCraft").with_app_id("ai.storyteller.lightcraft"),
        // Required on Android: eframe hands this to `EventLoopBuilderExtAndroid::with_android_app`
        // and errors without it.
        android_app: Some(app),
        ..Default::default()
    };

    let result = eframe::run_native(
        "LightCraft",
        options,
        Box::new(move |cc| {
            let picker = file_picker::channel();
            let (mut session, problem) = open_session(library_dir);
            // AI masks: the SAM 3 checkpoint in <config>/models/sam3 (never required — without it the
            // app offers to download it), and the user's own mirror list.
            session = session.with_default_sam3_model();
            session.face_models_dir = lightcraft_engine::config::default_face_models_dir();
            // Control requests would be answered on the UI thread; there is no control server here.
            session.apple_photos_background = true;
            let mut app = LightcraftApp::new(session, android_services(cc.egui_ctx.clone(), picker.clone()));
            if let Some(ui) = prefs {
                app.ui = ui;
            }
            lightcraft_ui_egui::i18n::set_language(app.ui.language);
            // Nothing draws traffic lights or a native menu bar on Android, so the app keeps its own
            // title bar and in-window menus (as on Windows and Linux).
            app.integrated_titlebar = false;
            app.native_menu = false;
            app.notices.extend(prefs_warning);
            if let Some(p) = problem {
                app.library_problem = Some(p);
            }
            if let Some(rs) = &cc.wgpu_render_state {
                log::info!("wgpu adapter: {:?}", rs.adapter.get_info());
            } else {
                log::warn!("no wgpu render state");
            }
            let mut writer = prefs_watch::Writer::new(prefs_path(), PREFS_EVERY);
            let bytes = prefs_ui_bytes(&app);
            writer.store(&bytes);
            Ok(Box::new(Shell { inner: app, picker, modifiers: modifiers::Tracked::default(), prefs_ui: writer }))
        }),
    );

    match result {
        Ok(()) => log::info!("LightCraft exited normally"),
        Err(e) => log::error!("LightCraft stopped: {e}"),
    }
}

/// Wraps the real app so the platform can act on the frame around it: the picker, the modifier keys,
/// the preferences written while it runs, and the end of the process when the user quits.
///
/// The same shape as the desktop shell (`apps/lightcraft/src/main.rs`): `LightcraftApp` stays
/// untouched and the platform-specific layer hangs off the outside. Every method the desktop
/// forwards is forwarded here too.
struct Shell {
    inner: LightcraftApp,
    /// The in-app picker's state, drawn after the app's own `ui` so it floats above the editor.
    picker: std::rc::Rc<std::cell::RefCell<file_picker::Shared>>,
    /// Modifier keys, tracked here because the platform never reports them: winit's Android backend
    /// sends no `ModifiersChanged` at all, which is why Shift+click and every Ctrl shortcut are dead
    /// without this (see `modifiers`).
    modifiers: modifiers::Tracked,
    /// `ui.json`, written as the app runs rather than only on the way out: Android never gives that
    /// ending, so a swipe from Recents would otherwise take the settings and the library location
    /// with it (see [`prefs_watch`]).
    prefs_ui: prefs_watch::Writer,
}

impl Shell {
    /// End the process, having saved everything the desktop's `on_exit` saves.
    ///
    /// Nothing can be unsaved by the time this runs: both routes below only reach it once the app's
    /// own `may_close` (or its prompt) has said the window may go. What is left is what `on_exit`
    /// would have done — and done here because a process exit runs no `on_exit`.
    fn quit_now(&mut self, why: &str) -> ! {
        let bytes = prefs_ui_bytes(&self.inner);
        self.prefs_ui.store_now(&bytes);
        if let Err(e) = self.inner.session.close_library() {
            log::error!("quit: saving the library failed: {e}");
        }
        // last, with everything saved: no render may be inside the GPU driver when the process ends
        if !self.inner.shutdown(EXIT_WAIT) {
            log::warn!("quit: background work was still running after {EXIT_WAIT:?}");
        }
        log::info!("{why}: ending the process");
        log::logger().flush();
        std::process::exit(0)
    }

    /// A quit the app asked for, ending in `std::process::exit`.
    ///
    /// **On Android `ViewportCommand::Close` ends nothing.** egui-winit turns it into a plain
    /// `ViewportEvent::Close` and winit's Android backend stubs the window API out, so the app sits
    /// on a stale close request: it looks like a hang, and it cannot be reopened until it is cleared
    /// from Recents. So the process is ended here instead.
    ///
    /// Two routes reach this. **File ▸ Quit** sets `ui.quit`, and the app's own `ui` turns that into
    /// `ViewportCommand::Close` — which is why it is checked *here*, after the app's `ui` has run, and
    /// not left for the next `logic`: a menu click lands during `ui`, no repaint is guaranteed
    /// afterwards, and waiting is exactly how Exit ends up doing nothing until the user clicks again.
    /// Before ending, the app's `may_close` runs — the same check the desktop makes when the window
    /// closes — so the unsaved-changes and still-running-tasks prompts appear here too. The other
    /// route is the app setting `quit_confirmed` (the prompt's Quit Anyway).
    fn end_if_quitting(&mut self, ctx: &egui::Context) {
        if self.inner.quit_confirmed {
            self.quit_now("File > Quit confirmed");
        }
        if std::mem::take(&mut self.inner.ui.quit) {
            if lightcraft_ui_egui::panels::notices::may_close(&mut self.inner) {
                self.quit_now("File > Quit");
            }
            // the prompt is up: keep drawing until it is answered
            ctx.request_repaint();
        }
    }
}

impl eframe::App for Shell {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.inner.logic(ctx);
        save_prefs_if_due(&self.inner, &mut self.prefs_ui);
        self.end_if_quitting(ctx);
    }

    fn raw_input_hook(&mut self, ctx: &egui::Context, raw: &mut egui::RawInput) {
        // Before the app, so the frame it sees already has the modifiers a desktop backend would have
        // reported (see `modifiers`).
        self.modifiers.apply(raw);
        self.inner.raw_input_hook(ctx, raw);
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.inner.ui(ui);
        // After the app, so the picker floats above the editor.
        file_picker::draw(ui.ctx(), &self.picker);
        // The quit dialog's OK is a click during this call, and File ▸ Quit set `ui.quit` during it
        // too: both are checked in the same frame rather than left to the next `logic`, which no
        // repaint is guaranteed to bring.
        self.end_if_quitting(ui.ctx());
    }

    fn on_exit(&mut self) {
        // Android never gives eframe this call (the activity is destroyed under the process), so it is
        // a best-effort mirror of the desktop's `on_exit` for the paths that do get here.
        let bytes = prefs_ui_bytes(&self.inner);
        self.prefs_ui.store_now(&bytes);
        if let Err(e) = self.inner.session.close_library() {
            log::error!("saving the library failed: {e}");
        }
        if !self.inner.shutdown(EXIT_WAIT) {
            log::warn!("quit: background work was still running after {EXIT_WAIT:?}");
        }
    }
}
