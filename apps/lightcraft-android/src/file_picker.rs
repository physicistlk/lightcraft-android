//! An in-app file picker for Android.
//!
//! # Why this exists
//!
//! LightCraft has no Java, so it cannot show Android's own file picker: the Storage Access
//! Framework needs `startActivityForResult` and `android-activity` 0.6 does not expose it. The
//! desktop shell fills `Services::picker` with `rfd` (native dialogs on their own thread); Android
//! has no equivalent, so this module draws the picker with egui instead.
//!
//! # How it plugs in
//!
//! LightCraft's dialogs do not block the frame. `pick::ask` consults `Services::picker` first: the
//! host is handed the [`PickRequest`] and returns a `Receiver`; the command returns at once and
//! `App::pending_picks` is polled once a frame by [`lightcraft_ui_egui::pick::poll`]. When paths
//! arrive, **the command that asked runs again** with the answer under the parameter it was
//! missing. An empty answer means cancelled.
//!
//! So the contract is: [`start`] stashes the request and returns the receiver, and a later frame's
//! [`draw`] sends the answer. That is exactly what this module does, and it is why the picker can be
//! an ordinary egui window.
//!
//! `pick::ask` reaches `Services::picker` *before* it falls back to the synchronous slots
//! (`pick_files`, `pick_folder`, `save_preset_file`, …), so **every** File / Export / Import menu
//! item goes through here — the synchronous slots only serve the odd "Choose…" button drawn inside
//! a dialog, and the shell registers those answering "cancelled" so the app's own capability gates
//! (`ui_enabled` checks `pick_folder.is_some()`, say) pass and the command reaches the picker.
//!
//! # Permissions
//!
//! Android 10+ forbids reading `/storage/emulated/0/Download` by path. Two routes exist and this
//! picker works with either, because it probes each root and only offers the ones it can actually
//! read:
//!
//! - **App folder** — `external_data_path()`, the app's own directory. Always readable and
//!   writable, no permission at all. (Android 11+ hides it from USB/MTP, so it is not how files get
//!   in.)
//! - **Shared storage** — Download / Pictures / DCIM / Documents. Needs "All files access"
//!   (`MANAGE_EXTERNAL_STORAGE`, granted by hand in Settings ▸ Apps ▸ LightCraft ▸ Special access) or
//!   `READ_MEDIA_IMAGES` for reading media. All four are declared in `Cargo.toml`.

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::OnceLock;
use std::sync::mpsc::{Receiver, Sender};

use lightcraft_ui_egui::pick::{PickKind, PickRequest};

/// The app's external files directory, captured in `android_main`.
///
/// `<shared storage>/Android/data/<package>/files` — always readable and writable with no
/// permission, so it is both the picker's first root and the place Save falls back to.
pub static EXT_DIR: OnceLock<PathBuf> = OnceLock::new();

/// The LightCraft library directory, captured in `android_main`, so the picker can offer it as a
/// root (the library keeps its `Originals/` there).
pub static LIBRARY_DIR: OnceLock<PathBuf> = OnceLock::new();

const SHARED_ROOT: &str = "/storage/emulated/0";

/// Which dialog is open.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// One file to open.
    Open,
    /// Several files.
    OpenMany,
    /// A path to save to.
    Save,
    /// A folder.
    Folder,
}

/// The dialog being shown: what was asked, and how to answer it.
struct Pending {
    kind: Kind,
    /// The dialog's title, shown as the egui window's title.
    title: String,
    /// Extensions the listing is filtered to (none: every file).
    extensions: Option<Vec<String>>,
    /// Where the answer goes; the app polls it once a frame.
    answer: Option<Sender<Vec<String>>>,
}

/// The picker's state, shared with the shell that draws it.
#[derive(Default)]
pub struct Shared {
    pending: Option<Pending>,
    /// Directory being browsed.
    dir: PathBuf,
    /// File name for a Save dialog.
    name: String,
    /// A message to show inside the picker (usually a permission problem).
    note: Option<String>,
    /// Chosen files, for a multi-select Open.
    picked: Vec<PathBuf>,
    /// Whether the starting directory was already applied.
    started: bool,
    /// List every file, whatever the app's type filter says — the desktop dialogs' "All files (*)".
    /// A filter must never be able to hide a file the app can open.
    show_all: bool,
    /// The roots and whether each can be read, worked out once when the picker opens: probing them
    /// every frame would be seven `read_dir`s per frame.
    roots: Option<Vec<(String, PathBuf, bool)>>,
    /// The browsed folder, read once per visit rather than once per frame.
    listing: Option<Listing>,
}

/// One folder's entries, kept between frames.
struct Listing {
    dir: PathBuf,
    /// `Err` when the folder can't be read at all — shared storage without the permission.
    entries: Result<(Vec<PathBuf>, Vec<PathBuf>), String>,
}

/// Build the state the shell draws from.
pub fn channel() -> Rc<RefCell<Shared>> {
    Rc::new(RefCell::new(Shared::default()))
}

/// Every extension the app's filter offers, or `None` when it offers none (then every file is
/// listed).
///
/// LightCraft's `PickRequest` carries one filter at a time, and that filter is the union of the
/// formats LightCraft reads (`lightcraft_engine::import::EXTENSIONS`), so nothing readable is
/// hidden behind another format's list.
fn extensions_of(request: &PickRequest) -> Option<Vec<String>> {
    let (_, exts) = request.filter.as_ref()?;
    let list: Vec<String> = exts.iter().map(|s| (*s).to_string()).collect();
    (!list.is_empty()).then_some(list)
}

/// The `Services::picker` body: queue the request, return the receiver at once.
pub fn start(shared: &Rc<RefCell<Shared>>, request: PickRequest) -> Option<Receiver<Vec<String>>> {
    let kind = match request.kind {
        PickKind::Files => Kind::OpenMany,
        PickKind::File => Kind::Open,
        PickKind::Folder => Kind::Folder,
        PickKind::Save => Kind::Save,
    };
    let extensions = extensions_of(&request);
    let suggested = request.file_name.clone().unwrap_or_default();
    let title = request
        .title
        .clone()
        .unwrap_or_else(|| match kind {
            Kind::Open | Kind::OpenMany => "Open".to_string(),
            Kind::Save => "Save As".to_string(),
            Kind::Folder => "Choose Folder".to_string(),
        });
    let (tx, rx) = std::sync::mpsc::channel();
    let mut s = shared.borrow_mut();
    // A dialog already open is answered first; this one takes its place.
    if let Some(prev) = s.pending.take()
        && let Some(tx) = prev.answer
    {
        let _ = tx.send(Vec::new());
    }
    s.pending = Some(Pending { kind, title, extensions, answer: Some(tx) });
    s.name = suggested;
    s.picked.clear();
    s.started = false;
    s.note = None;
    // Re-probe and re-read for every dialog. Cached for the app's lifetime, a permission granted in
    // Settings — or a folder whose first read failed — would keep its old answer until the app was
    // killed, which looks exactly like the picker ignoring the grant.
    s.roots = None;
    s.listing = None;
    Some(rx)
}

/// `<external storage>/Android/media/<package>`: the app's own media folder.
///
/// Derived from `external_data_path()` (`…/Android/data/<package>/files`) so the package name isn't
/// written down twice.
///
/// This is the one folder that is both part of the app's own storage (**no permission**) and left
/// **visible over USB/MTP** by Android 11+ — `Android/data` is hidden there, so a file the user
/// copies into the app folder with a cable silently never arrives. Dropping it here works.
pub fn media_dir() -> Option<PathBuf> {
    media_dir_from(EXT_DIR.get()?)
}

/// The derivation, on its own so it can be pinned by a test: `…/Android/data/<package>/files`
/// → `…/Android/media/<package>`.
fn media_dir_from(external_files: &Path) -> Option<PathBuf> {
    let package_dir = external_files.parent()?;
    let android = package_dir.parent()?.parent()?;
    Some(android.join("media").join(package_dir.file_name()?))
}

/// Roots to offer, as (label, path).
fn roots() -> Vec<(String, PathBuf)> {
    let mut v = Vec::new();
    if let Some(d) = EXT_DIR.get() {
        v.push(("App folder".to_string(), d.clone()));
    }
    if let Some(m) = media_dir() {
        v.push(("App media (USB)".to_string(), m));
    }
    if let Some(l) = LIBRARY_DIR.get() {
        v.push(("Library".to_string(), l.clone()));
    }
    for (label, sub) in [("Download", "Download"), ("Pictures", "Pictures"), ("DCIM", "DCIM"), ("Documents", "Documents")] {
        v.push((label.to_string(), Path::new(SHARED_ROOT).join(sub)));
    }
    v.push(("Internal storage".to_string(), PathBuf::from(SHARED_ROOT)));
    v
}

/// The best directory to open first: the app folder is always writable, so it is the only safe
/// default — desktop paths arriving as a suggestion never exist here.
fn default_dir() -> PathBuf {
    EXT_DIR.get().cloned().unwrap_or_else(|| PathBuf::from(SHARED_ROOT))
}

fn readable(dir: &Path) -> bool {
    std::fs::read_dir(dir).is_ok()
}

/// Shared storage outside the app's own folders — `/storage/emulated/0/…` but not
/// `/storage/emulated/0/Android/…`.
///
/// This is where the OS lies to the app: without "All files access" a shared folder still
/// **opens** (`read_dir` succeeds, so [`readable`] says yes and no lock is shown), but its
/// **contents are filtered away** — an empty listing for a folder full of files. Reproduced in the
/// emulator on a sibling port: `/sdcard/Documents` held five files, the picker read zero. So an
/// empty listing here is not evidence of an empty folder, and the picker must say so.
fn is_shared_storage(dir: &Path) -> bool {
    let shared = Path::new(SHARED_ROOT);
    dir.starts_with(shared) && !dir.starts_with(shared.join("Android"))
}

/// Everything in `dir`, folders first, both sorted, hidden files left out.
///
/// Unfiltered on purpose: the type filter is applied in memory by [`keep`], so the whole folder is
/// read once per visit and the picker can still say how many files a filter is hiding.
fn entries(dir: &Path) -> Result<(Vec<PathBuf>, Vec<PathBuf>), String> {
    let rd = std::fs::read_dir(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let (mut dirs, mut files) = (Vec::new(), Vec::new());
    for entry in rd.flatten() {
        let path = entry.path();
        if entry.file_name().to_string_lossy().starts_with('.') {
            continue;
        }
        if path.is_dir() {
            dirs.push(path);
        } else {
            files.push(path);
        }
    }
    dirs.sort_by_key(|p| p.file_name().map(|n| n.to_string_lossy().to_ascii_lowercase()));
    files.sort_by_key(|p| p.file_name().map(|n| n.to_string_lossy().to_ascii_lowercase()));
    Ok((dirs, files))
}

/// Whether the app's filter offers `path`. `.HEIC` sits beside `.jpg` and `.NEF` in the Open
/// dialog's list. `None` means every file, as does "Show all files".
fn keep(path: &Path, extensions: Option<&Vec<String>>) -> bool {
    match extensions {
        None => true,
        Some(exts) => {
            let ext = path.extension().map(|e| e.to_string_lossy().to_ascii_lowercase());
            ext.is_some_and(|e| exts.iter().any(|w| w.eq_ignore_ascii_case(&e)))
        }
    }
}

fn file_label(path: &Path) -> String {
    path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default()
}

/// Draw the picker if a dialog is queued. Called from the shell after the app's own `ui`, so the
/// window floats on top of the editor.
pub fn draw(ctx: &egui::Context, shared: &Rc<RefCell<Shared>>) {
    let mut s = shared.borrow_mut();
    let Some(kind) = s.pending.as_ref().map(|p| p.kind) else {
        return;
    };
    // First frame: pick a starting directory.
    if !s.started {
        s.started = true;
        s.dir = default_dir();
        if kind == Kind::Save && s.name.is_empty() {
            s.name = "export".to_string();
        }
    }

    let mut answer: Option<Vec<String>> = None;
    let mut cancel = false;
    let mut navigate: Option<PathBuf> = None;
    // Set by Refresh: the caches must not be written back at the end of the frame.
    let mut refresh = false;

    let dir = s.dir.clone();
    // Probe the roots once, not seven `read_dir`s every frame.
    if s.roots.is_none() {
        s.roots = Some(roots().into_iter().map(|(label, path)| (label, path.clone(), readable(&path))).collect());
    }
    // Read the folder once per visit rather than once per frame: the type filter then runs over
    // memory, and the picker can say how many files it is hiding.
    if s.listing.as_ref().is_none_or(|l| l.dir != dir) {
        s.listing = Some(Listing { dir: dir.clone(), entries: entries(&dir) });
    }
    let roots_now = s.roots.clone().unwrap_or_default();
    // Read before the window: the file loop filters by it while mutating `s`.
    let show_all = s.show_all;
    // Moved out while the window draws, so the closure can hold these and `s` at once.
    let listing = s.listing.take();
    let exts = s.pending.as_ref().and_then(|p| p.extensions.clone());
    let title = s.pending.as_ref().map(|p| p.title.clone()).unwrap_or_default();
    let multi = kind == Kind::OpenMany;
    let all_files: &[PathBuf] = match listing.as_ref().map(|l| &l.entries) {
        Some(Ok((_, files))) => files,
        _ => &[],
    };
    // A Folder dialog lists no files, so nothing is "hidden" by the filter there.
    let listed_count = if kind == Kind::Folder { all_files.len() } else { all_files.iter().filter(|f| show_all || keep(f, exts.as_ref())).count() };
    let hidden = all_files.len() - listed_count;
    // A tablet screen is smaller than the desktop default, and in portrait narrower than the 620
    // points the desktop dialogs hard-code. `content_rect` is the area egui considers safe (it
    // excludes an OS status bar where the backend reports one).
    let screen = ctx.content_rect();
    let (w, h) = ((screen.width() * 0.92).min(720.0), (screen.height() * 0.82).min(620.0));

    egui::Window::new(title)
        .collapsible(false)
        .resizable(true)
        .default_width(w)
        .default_height(h)
        .default_pos(screen.center() - egui::vec2(w, h) / 2.0)
        .show(ctx, |ui| {
            // Finger-sized rows: the default spacing is sized for a mouse.
            ui.spacing_mut().item_spacing = egui::vec2(6.0, 6.0);
            ui.spacing_mut().button_padding = egui::vec2(10.0, 8.0);

            // Roots. A locked one stays tappable and says why: greyed out with a lock, it reads as
            // "your files aren't here" and gives no way forward.
            ui.horizontal_wrapped(|ui| {
                for (label, path, ok) in roots_now.iter() {
                    let here = *path == dir;
                    let text = if *ok { label.clone() } else { format!("{label} \u{1F512}") };
                    if ui.add_enabled(!here, egui::Button::new(text)).clicked() {
                        if *ok {
                            navigate = Some(path.clone());
                        } else {
                            s.note = Some(format!(
                                "{label} can't be read. Grant \u{201C}All files access\u{201D} in Settings \u{25B8} Apps \u{25B8} LightCraft \u{25B8} Special access, then reopen this dialog."
                            ));
                        }
                    }
                }
            });
            ui.separator();

            // Current path + up
            ui.horizontal_wrapped(|ui| {
                if ui.button("\u{2B06} Up").clicked()
                    && let Some(parent) = dir.parent()
                {
                    navigate = Some(parent.to_path_buf());
                }
                if ui.button("\u{1F4C1} App folder").clicked() {
                    navigate = Some(default_dir());
                }
                // Grant the permission, come back and tap Refresh: the roots and the folder are
                // probed again rather than keeping the answer from before the grant.
                if ui.button("\u{21BB} Refresh").clicked() {
                    refresh = true;
                }
                ui.label(egui::RichText::new(dir.display().to_string()).monospace().small());
                ui.label(egui::RichText::new(format!("({} files)", all_files.len())).weak().small());
            });

            // The type filter, and the way out of it. The desktop dialogs always offer "All files";
            // without one, anything the app's filter list omits is invisible with no hint that it
            // exists.
            ui.horizontal_wrapped(|ui| {
                ui.checkbox(&mut s.show_all, "Show all files");
                if hidden > 0 && !s.show_all {
                    ui.colored_label(egui::Color32::from_rgb(150, 150, 150), format!("{hidden} file(s) hidden by the file-type filter"));
                }
            });

            if let Some(note) = &s.note {
                ui.colored_label(egui::Color32::from_rgb(220, 160, 60), note);
            }

            ui.separator();
            let list_h = (h - 210.0).max(120.0);
            egui::ScrollArea::vertical().max_height(list_h).show(ui, |ui| match listing.as_ref().map(|l| &l.entries) {
                Some(Err(e)) => {
                    ui.label("Cannot read this folder.");
                    ui.label(egui::RichText::new(e.clone()).monospace().weak().small());
                    ui.label("Grant \"All files access\" in Settings \u{25B8} Apps \u{25B8} LightCraft \u{25B8} Special access to browse shared storage.");
                }
                Some(Ok((dirs, files))) => {
                    if dirs.is_empty() && listed_count == 0 {
                        ui.label(if hidden > 0 {
                            format!("(no matching files \u{2014} {hidden} hidden by the file-type filter)")
                        } else {
                            "(no files visible here)".to_string()
                        });
                        // The commonest trap on a tablet: the folder really does have photos, but the
                        // system is hiding its contents, so it reads as empty.
                        if hidden == 0 && is_shared_storage(&dir) {
                            ui.colored_label(
                                egui::Color32::from_rgb(220, 160, 60),
                                "If this folder has photos that don't appear, grant \u{201C}All files access\u{201D} in Settings \u{25B8} Apps \u{25B8} LightCraft \u{25B8} Special access, then tap Refresh. Without it the system hides shared folders' contents from LightCraft.",
                            );
                        }
                    }
                    for d in dirs {
                        if ui.button(format!("\u{1F4C1} {}", file_label(d))).clicked() {
                            navigate = Some(d.clone());
                        }
                    }
                    if kind != Kind::Folder {
                        for f in files.iter().filter(|f| show_all || keep(f, exts.as_ref())) {
                            let chosen = s.picked.iter().any(|p| p == f);
                            let mark = if chosen { "\u{2611}" } else { "\u{2610}" };
                            let label = if multi { format!("{mark} {}", file_label(f)) } else { format!("   {}", file_label(f)) };
                            if ui.button(label).clicked() {
                                if kind == Kind::Save {
                                    s.name = file_label(f);
                                } else if multi {
                                    if chosen {
                                        s.picked.retain(|p| p != f);
                                    } else {
                                        s.picked.push(f.clone());
                                    }
                                } else {
                                    answer = Some(vec![f.to_string_lossy().to_string()]);
                                }
                            }
                        }
                    }
                }
                // No folder read yet.
                None => {
                    ui.label("(empty folder)");
                }
            });

            ui.separator();
            ui.horizontal_wrapped(|ui| match kind {
                Kind::Save => {
                    ui.label("File name:");
                    ui.add(egui::TextEdit::singleline(&mut s.name).desired_width((w - 260.0).max(160.0)));
                    if ui.button("Save").clicked() {
                        let name = s.name.trim().to_string();
                        if name.is_empty() {
                            s.note = Some("Enter a file name.".to_string());
                        } else {
                            // The app's atomic write needs the parent to exist, so answer with the
                            // whole path and not just the name.
                            answer = Some(vec![dir.join(name).to_string_lossy().to_string()]);
                        }
                    }
                }
                Kind::Folder => {
                    if ui.button("Choose this folder").clicked() {
                        answer = Some(vec![dir.to_string_lossy().to_string()]);
                    }
                }
                Kind::OpenMany => {
                    let n = s.picked.len();
                    let label = if n == 0 { "Open".to_string() } else { format!("Open {n} file(s)") };
                    if ui.add_enabled(n > 0, egui::Button::new(label)).clicked() {
                        answer = Some(s.picked.iter().map(|p| p.to_string_lossy().to_string()).collect());
                    }
                }
                Kind::Open => {
                    if ui.button("Open").clicked() {
                        // A file has to be tapped in the list above for an Open dialog.
                        s.note = Some("Tap a file to open it.".to_string());
                    }
                }
            });
            if ui.button("Cancel").clicked() {
                cancel = true;
            }

            // Honest, and it corrects what the first draft of this picker said: Android 11+ blocks
            // copying into `Android/data` over USB/MTP, so "push files here" was wrong advice.
            if dir == default_dir() {
                ui.label(
                    egui::RichText::new(
                        "The app's own folder: files here open and save with no permission. Android 11+ blocks copying into Android/data over USB \u{2014} to bring photos in, put them in Download and grant \u{201C}All files access\u{201D}.",
                    )
                    .weak()
                    .small(),
                );
            }
        });

    // Back into the cache; a navigation made it stale and the next frame re-reads it. Refresh clears
    // both caches instead of keeping this frame's answer, so the probe and the folder read run again
    // — that is the whole point of the button.
    if refresh {
        s.roots = None;
        s.listing = None;
        s.note = None;
    } else {
        s.listing = listing;
    }

    if let Some(dir) = navigate {
        s.dir = dir;
        s.note = None;
    }
    if let Some(paths) = answer {
        if let Some(pending) = s.pending.take()
            && let Some(tx) = pending.answer
        {
            let _ = tx.send(paths);
        }
        s.picked.clear();
    } else if cancel {
        // Cancelled: an empty answer is what the app reads as one.
        if let Some(pending) = s.pending.take()
            && let Some(tx) = pending.answer
        {
            let _ = tx.send(Vec::new());
        }
        s.picked.clear();
    }
    // The app looks at the receiver about once a second while it waits; this keeps the picker
    // itself responsive between those frames.
    ctx.request_repaint();
}

#[cfg(test)]
mod tests {
    use super::*;
    use lightcraft_ui_egui::pick::PickRequest;

    /// A scratch folder with the given files in it.
    fn scratch(name: &str, files: &[&str]) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("lightcraft-android-picker-{name}"));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        for f in files {
            std::fs::write(dir.join(f), b"x").unwrap();
        }
        dir
    }

    /// The picker lists every extension the app's filter offers, so a format LightCraft reads —
    /// `.heic` off a phone, `.nef` out of a camera — is never hidden. `heic` is the one that broke a
    /// sibling photo port, so it is pinned here.
    #[test]
    fn the_open_dialog_lists_heic_and_raw() {
        let exts: Vec<String> = lightcraft_engine::import::EXTENSIONS.iter().map(|e| (*e).to_string()).collect();
        for want in ["heic", "nef", "jpg", "png"] {
            assert!(exts.iter().any(|e| e.eq_ignore_ascii_case(want)), "\"{want}\" is not among the import extensions: {exts:?}");
        }

        let dir = scratch("heic", &["IMG_0001.HEIC", "IMG_0002.heic", "shot.nef", "notes.txt"]);
        let (dirs, files) = entries(&dir).unwrap();
        let listed: Vec<String> = files.iter().filter(|f| keep(f, Some(&exts))).map(|p| file_label(p)).collect();
        assert!(dirs.is_empty(), "no subfolders were made");
        assert!(listed.contains(&"IMG_0001.HEIC".to_string()), "a .HEIC the app can read must be listed: {listed:?}");
        assert!(listed.contains(&"IMG_0002.heic".to_string()), "the extension match is case-insensitive: {listed:?}");
        assert!(listed.contains(&"shot.nef".to_string()), "{listed:?}");
        // Not a format the app reads, so hidden — which is exactly why the picker says how many it
        // is hiding and offers "Show all files".
        assert!(!listed.contains(&"notes.txt".to_string()), "a format the app doesn't read is filtered out: {listed:?}");

        // The folder read is unfiltered, so the count of hidden files is knowable...
        assert!(files.iter().any(|p| file_label(p) == "notes.txt"), "the folder read still sees it: {files:?}");
        // ...and "Show all files" reaches anything the filter would drop.
        assert!(keep(&dir.join("notes.txt"), None), "no filter offers every file");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// An unreadable folder is reported, not silently empty — that is what "my files aren't there"
    /// looks like when the permission is missing.
    #[test]
    fn an_unreadable_folder_is_an_error_not_an_empty_one() {
        let missing = std::env::temp_dir().join("lightcraft-android-picker-does-not-exist");
        let _ = std::fs::remove_dir_all(&missing);
        assert!(entries(&missing).is_err(), "a folder that can't be read is an Err, so the picker can explain");
    }

    /// Hidden files stay out of the listing, as on the desktop.
    #[test]
    fn dotfiles_are_left_out() {
        let dir = scratch("hidden", &["shown.heic", ".hidden.heic"]);
        let (_, files) = entries(&dir).unwrap();
        let names: Vec<String> = files.iter().map(|p| file_label(p)).collect();
        assert_eq!(names, vec!["shown.heic".to_string()], "{names:?}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The USB-visible root's path. `Android/data` is hidden from MTP on Android 11+, so this folder
    /// is the one a user can actually copy a photo into; the package name is derived rather than
    /// written down a second time, and this is what pins it.
    #[test]
    fn the_usb_visible_root_is_derived_from_the_app_folder() {
        let ext = Path::new("/storage/emulated/0/Android/data/ai.storyteller.lightcraft/files");
        assert_eq!(media_dir_from(ext), Some(PathBuf::from("/storage/emulated/0/Android/media/ai.storyteller.lightcraft")));
        assert_eq!(media_dir_from(Path::new("/nowhere")), None, "a path without the Android layout has no media folder");
    }

    /// Shared storage is where the OS hides a folder's contents; the app's own `Android/…` folders
    /// are not, so an empty listing there really is empty.
    #[test]
    fn shared_storage_is_recognised_so_an_empty_listing_can_be_explained() {
        for d in ["/storage/emulated/0/Documents", "/storage/emulated/0/Download", "/storage/emulated/0"] {
            assert!(is_shared_storage(Path::new(d)), "{d} is shared storage");
        }
        for d in [
            "/storage/emulated/0/Android/data/ai.storyteller.lightcraft/files",
            "/storage/emulated/0/Android/media/ai.storyteller.lightcraft",
            "/data/user/0/ai.storyteller.lightcraft/files",
        ] {
            assert!(!is_shared_storage(Path::new(d)), "{d} is the app's own");
        }
    }

    /// A Save dialog answers with the whole path — the app's atomic write needs the parent to exist,
    /// and only a name would leave it writing into the working directory.
    #[test]
    fn a_save_answer_is_the_directory_joined_with_the_name() {
        let exts = extensions_of(&PickRequest::save("Export", "JPEG", &["jpg", "jpeg"], "photo.jpg"));
        assert_eq!(exts.map(|e| e.len()), Some(2));
        let answer = Path::new("/storage/emulated/0/Documents").join("photo.jpg");
        assert_eq!(answer.to_string_lossy(), "/storage/emulated/0/Documents/photo.jpg");
    }
}
