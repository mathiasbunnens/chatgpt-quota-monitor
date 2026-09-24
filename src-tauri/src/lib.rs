mod activity;
mod codex;
#[cfg(any(not(target_os = "macos"), test))]
mod tray_percentage;

#[cfg(target_os = "macos")]
use chrono::{DateTime, Local};
use serde::Serialize;
use std::sync::atomic::AtomicBool;
#[cfg(target_os = "macos")]
use std::sync::OnceLock;
use std::sync::{atomic::Ordering, Mutex};
use tauri::{Emitter, Manager};

#[cfg(target_os = "macos")]
use objc2::{
    define_class, msg_send,
    rc::Retained,
    runtime::{AnyObject, ProtocolObject},
    MainThreadOnly,
};
#[cfg(target_os = "macos")]
use objc2_app_kit::{
    NSApplication, NSBox, NSBoxType, NSColor, NSFont, NSImage, NSMenu, NSMenuDelegate, NSMenuItem,
    NSStatusBar, NSTextField, NSVariableStatusItemLength, NSView,
};
#[cfg(target_os = "macos")]
use objc2_foundation::{
    MainThreadMarker, NSData, NSObject, NSObjectProtocol, NSPoint, NSRect, NSSize, NSString,
};

#[cfg(target_os = "macos")]
const PROGRESS_WIDTH: f64 = 248.0;

#[derive(Debug, Serialize, Clone)]
struct QuotaPayload {
    model: String,
    remaining: u32,
    limit: u32,
    #[serde(rename = "resetAt", alias = "reset_at")]
    reset_at: String,
    #[serde(rename = "checkedAt", alias = "checked_at")]
    checked_at: String,
    confidence: String,
    period: String,
    #[serde(rename = "resetLabel", alias = "reset_label", default)]
    reset_label: Option<String>,
    source: String,
    #[serde(default)]
    label: Option<String>,
    #[serde(default, rename = "windowMinutes")]
    window_minutes: Option<i64>,
}

#[derive(Default)]
struct QuotaState(Mutex<Vec<QuotaPayload>>);

fn publish_quotas(app: &tauri::AppHandle) {
    render_native_menu(app);
}

fn set_codex_snapshots(app: &tauri::AppHandle, snapshots: Option<Vec<QuotaPayload>>) {
    if let Ok(mut data) = app.state::<QuotaState>().0.lock() {
        *data = snapshots.unwrap_or_default();
    }
    publish_quotas(app);
}

static UPDATE_CHECK_IN_PROGRESS: AtomicBool = AtomicBool::new(false);
#[cfg(target_os = "macos")]
static APP_HANDLE: OnceLock<tauri::AppHandle> = OnceLock::new();

#[derive(Clone, Copy, PartialEq, Eq)]
enum UpdateCheckMode {
    #[cfg(not(debug_assertions))]
    Automatic,
    Interactive,
}

struct UpdateCheckGuard;

impl Drop for UpdateCheckGuard {
    fn drop(&mut self) {
        UPDATE_CHECK_IN_PROGRESS.store(false, Ordering::Release);
    }
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct UpdateStatus {
    phase: String,
    message: String,
    progress: Option<u8>,
    current_version: String,
    next_version: Option<String>,
}

#[derive(Default)]
struct UpdateState(Mutex<Option<UpdateStatus>>);

fn update_status(
    app: &tauri::AppHandle,
    phase: &str,
    message: impl Into<String>,
    progress: Option<u8>,
    next_version: Option<String>,
) {
    let status = UpdateStatus {
        phase: phase.into(),
        message: message.into(),
        progress,
        current_version: app.package_info().version.to_string(),
        next_version,
    };
    if let Ok(mut state) = app.state::<UpdateState>().0.lock() {
        *state = Some(status.clone());
    }
    if let Some(window) = app.get_webview_window("updater") {
        let _ = window.emit("update-status", status);
    }
}

fn show_update_window(app: &tauri::AppHandle) -> tauri::Result<()> {
    let window = if let Some(window) = app.get_webview_window("updater") {
        window
    } else {
        tauri::WebviewWindowBuilder::new(
            app,
            "updater",
            tauri::WebviewUrl::App("index.html".into()),
        )
        .title("Mise à jour de Quota Codex")
        .inner_size(420.0, 260.0)
        .min_inner_size(360.0, 240.0)
        .resizable(false)
        .center()
        .build()?
    };
    window.show()?;
    window.set_focus()?;
    #[cfg(target_os = "macos")]
    let _ = app.run_on_main_thread(|| {
        let mtm = MainThreadMarker::new().expect("update popup activation runs on main thread");
        NSApplication::sharedApplication(mtm).activate();
    });
    Ok(())
}

fn show_update_error(app: &tauri::AppHandle, mode: UpdateCheckMode, error: impl ToString) {
    let error = error.to_string();
    eprintln!("Quota Codex update failed: {error}");
    if mode == UpdateCheckMode::Interactive {
        let _ = show_update_window(app);
        update_status(
            app,
            "error",
            format!("Vérifie ta connexion Internet puis réessaie.\n\nDétail : {error}"),
            None,
            None,
        );
    }
}

async fn check_for_available_update(app: tauri::AppHandle, mode: UpdateCheckMode) {
    use tauri_plugin_updater::UpdaterExt;

    if UPDATE_CHECK_IN_PROGRESS.swap(true, Ordering::AcqRel) {
        if mode == UpdateCheckMode::Interactive {
            let _ = show_update_window(&app);
        }
        return;
    }
    let _guard = UpdateCheckGuard;
    let current_version = app.package_info().version.to_string();
    if mode == UpdateCheckMode::Interactive {
        let _ = show_update_window(&app);
        update_status(
            &app,
            "checking",
            "Recherche d’une nouvelle version…",
            None,
            None,
        );
    }

    let update = match app.updater() {
        Ok(updater) => updater.check().await,
        Err(error) => {
            show_update_error(&app, mode, error);
            return;
        }
    };

    match update {
        Ok(Some(update)) => {
            let next_version = update.version.clone();
            let _ = show_update_window(&app);
            update_status(
                &app,
                "available",
                format!(
                    "La version {current_version} est installée. Quota Codex v{next_version} est disponible."
                ),
                None,
                Some(next_version),
            );
        }
        Ok(None) if mode == UpdateCheckMode::Interactive => update_status(
            &app,
            "up-to-date",
            format!("La version {current_version} est la dernière version disponible."),
            None,
            None,
        ),
        Ok(None) => {}
        Err(error) => show_update_error(&app, mode, error),
    }
}

async fn download_and_install_update(app: tauri::AppHandle) {
    use tauri_plugin_updater::UpdaterExt;

    if UPDATE_CHECK_IN_PROGRESS.swap(true, Ordering::AcqRel) {
        return;
    }
    let _guard = UpdateCheckGuard;
    let update = match app.updater() {
        Ok(updater) => updater.check().await,
        Err(error) => {
            show_update_error(&app, UpdateCheckMode::Interactive, error);
            return;
        }
    };
    let update = match update {
        Ok(Some(update)) => update,
        Ok(None) => {
            update_status(
                &app,
                "up-to-date",
                "Aucune mise à jour n’est plus disponible.",
                None,
                None,
            );
            return;
        }
        Err(error) => {
            show_update_error(&app, UpdateCheckMode::Interactive, error);
            return;
        }
    };
    let next_version = update.version.clone();
    update_status(
        &app,
        "downloading",
        format!("Téléchargement de Quota Codex v{next_version}…"),
        Some(0),
        Some(next_version.clone()),
    );
    let progress_app = app.clone();
    let progress_version = next_version.clone();
    let mut downloaded = 0_u64;
    let mut last_progress = 0_u8;
    let result = update
        .download_and_install(
            move |chunk_length, content_length| {
                downloaded = downloaded.saturating_add(chunk_length as u64);
                let progress = content_length
                    .filter(|total| *total > 0)
                    .map(|total| ((downloaded.saturating_mul(100) / total).min(100)) as u8)
                    .unwrap_or(last_progress);
                if progress != last_progress {
                    last_progress = progress;
                    update_status(
                        &progress_app,
                        "downloading",
                        format!("Téléchargement de Quota Codex v{progress_version}…"),
                        Some(progress),
                        Some(progress_version.clone()),
                    );
                }
            },
            || {},
        )
        .await;
    match result {
        Ok(()) => update_status(
            &app,
            "ready",
            format!("Quota Codex v{next_version} est installé et prêt à être relancé."),
            Some(100),
            Some(next_version),
        ),
        Err(error) => show_update_error(&app, UpdateCheckMode::Interactive, error),
    }
}

#[tauri::command]
fn get_update_status(app: tauri::AppHandle) -> UpdateStatus {
    app.state::<UpdateState>()
        .0
        .lock()
        .ok()
        .and_then(|status| status.clone())
        .unwrap_or_else(|| UpdateStatus {
            phase: "checking".into(),
            message: "Recherche d’une nouvelle version…".into(),
            progress: None,
            current_version: app.package_info().version.to_string(),
            next_version: None,
        })
}

#[tauri::command]
fn start_update_check(app: tauri::AppHandle) {
    tauri::async_runtime::spawn(check_for_available_update(
        app,
        UpdateCheckMode::Interactive,
    ));
}

#[tauri::command]
fn start_update_install(app: tauri::AppHandle) {
    tauri::async_runtime::spawn(download_and_install_update(app));
}

#[tauri::command]
fn close_update_window(app: tauri::AppHandle) {
    if let Some(window) = app.get_webview_window("updater") {
        let _ = window.close();
    }
}

#[tauri::command]
fn restart_application(app: tauri::AppHandle) {
    app.restart();
}

#[derive(Clone, Copy)]
#[cfg(target_os = "macos")]
struct QuotaViewHandles {
    value: usize,
    reset: usize,
    fill: usize,
}

#[derive(Clone, Copy)]
#[cfg(target_os = "macos")]
struct NativeQuotaMenu {
    status_item: usize,
    status_button: usize,
    target: usize,
}

#[cfg(target_os = "macos")]
#[derive(Default)]
struct NativeMenuState(Mutex<Option<NativeQuotaMenu>>);

#[cfg(target_os = "macos")]
define_class!(
    #[unsafe(super = NSObject)]
    #[thread_kind = MainThreadOnly]
    struct QuotaMenuTarget;

    unsafe impl NSObjectProtocol for QuotaMenuTarget {}

    unsafe impl NSMenuDelegate for QuotaMenuTarget {
        #[unsafe(method(menuWillOpen:))]
        fn menu_will_open(&self, _menu: &NSMenu) {
            if let Some(app) = APP_HANDLE.get() { request_quota_refresh(app.clone()); }
        }
    }

    impl QuotaMenuTarget {
        #[unsafe(method(quitApp:))]
        fn quit_app(&self, _sender: Option<&AnyObject>) {
            if let Some(app) = APP_HANDLE.get() { app.exit(0); }
        }

        #[unsafe(method(connectAccount:))]
        fn connect_account(&self, _sender: Option<&AnyObject>) {
            let Some(app) = APP_HANDLE.get() else { return; };
            let Some(provider) = app.try_state::<codex::Provider>() else { return; };
            if provider.status().phase == "missing" {
                let _ = codex::open_codex_install(app.clone());
            } else {
                provider.login(false);
            }
        }
        #[unsafe(method(refreshQuota:))]
        fn refresh_quota(&self, _sender: Option<&AnyObject>) {
            if let Some(app) = APP_HANDLE.get() { request_quota_refresh(app.clone()); }
        }
        #[unsafe(method(openUsage:))]
        fn open_usage(&self, _sender: Option<&AnyObject>) {
            open_usage_page();
        }

        #[unsafe(method(checkForUpdates:))]
        fn check_for_updates(&self, _sender: Option<&AnyObject>) {
            if let Some(app) = APP_HANDLE.get().cloned() {
                tauri::async_runtime::spawn(check_for_available_update(
                    app,
                    UpdateCheckMode::Interactive,
                ));
            }
        }
    }
);

#[cfg(target_os = "macos")]
impl QuotaMenuTarget {
    fn new(mtm: MainThreadMarker) -> Retained<Self> {
        let this = mtm.alloc().set_ivars(());
        unsafe { msg_send![super(this), init] }
    }
}

#[tauri::command]
fn get_platform() -> String {
    std::env::consts::OS.to_string()
}

#[tauri::command]
fn get_quota_snapshots(app: tauri::AppHandle) -> Vec<QuotaPayload> {
    let rows = app
        .state::<QuotaState>()
        .0
        .lock()
        .map(|data| data.clone())
        .unwrap_or_default();
    let plan = app
        .try_state::<codex::Provider>()
        .and_then(|provider| provider.status().plan_type);
    visible_quotas(rows, plan.as_deref())
}

fn visible_quotas(mut rows: Vec<QuotaPayload>, plan: Option<&str>) -> Vec<QuotaPayload> {
    let reserve = |p: &QuotaPayload| {
        let identity = format!("{} {}", p.period, p.model).to_ascii_lowercase();
        identity.contains("reserve") || identity.contains("luna")
    };
    let five = |p: &QuotaPayload| {
        !reserve(p) && (p.period == "five-hour" || p.window_minutes == Some(300))
    };
    rows.sort_by_key(|p| {
        (
            !five(p),
            !p.period.starts_with("codex:codex:"),
            p.period.clone(),
        )
    });
    if plan != Some("plus") {
        return rows;
    }
    if let Some(active) = rows.iter().find(|p| five(p) && p.remaining > 0) {
        return vec![active.clone()];
    }
    let reserves: Vec<_> = rows
        .iter()
        .filter(|p| {
            let identity = format!("{} {}", p.period, p.model).to_ascii_lowercase();
            identity.contains("reserve") || identity.contains("luna")
        })
        .cloned()
        .collect();
    if !reserves.is_empty() {
        return reserves;
    }
    // An exhausted five-hour quota is hidden even if reserve is unavailable.
    if rows.iter().any(five) {
        return Vec::new();
    }
    // Weekly-only accounts keep the actual window reported by Codex.
    rows
}

#[tauri::command]
fn request_quota_refresh(app: tauri::AppHandle) {
    if let Some(provider) = app.try_state::<codex::Provider>() {
        provider.refresh();
    }
}

#[tauri::command]
fn open_usage_details(app: tauri::AppHandle) -> Result<(), String> {
    use tauri_plugin_opener::OpenerExt;
    app.opener()
        .open_url(
            "https://chatgpt.com/codex/cloud/settings/analytics#usage",
            None::<&str>,
        )
        .map_err(|e| e.to_string())
}

#[cfg(target_os = "macos")]
fn open_usage_page() {
    if let Some(app) = APP_HANDLE.get() {
        let _ = open_usage_details(app.clone());
    }
}

#[cfg(target_os = "macos")]
fn reset_label(payload: &QuotaPayload) -> String {
    DateTime::parse_from_rfc3339(&payload.reset_at)
        .map(|date| {
            let local = date.with_timezone(&Local);
            if payload.period == "five-hour" {
                local.format("%Hh%M").to_string()
            } else {
                local.format("%d/%m à %Hh%M").to_string()
            }
        })
        .ok()
        .or_else(|| payload.reset_label.clone())
        .unwrap_or_else(|| "--".to_string())
}

fn quota_color_transition(percentage: u32) -> (&'static str, &'static str, f64) {
    match percentage {
        0..=20 => ("red", "red", 0.0),
        21..=40 => ("red", "orange", f64::from(percentage - 20) / 20.0),
        41..=60 => ("orange", "green", f64::from(percentage - 40) / 20.0),
        _ => ("green", "green", 0.0),
    }
}

#[cfg(target_os = "macos")]
fn quota_color(percentage: u32) -> Retained<NSColor> {
    let (from, to, fraction) = quota_color_transition(percentage);
    let color = |name| match name {
        "red" => NSColor::systemRedColor(),
        "orange" => NSColor::systemOrangeColor(),
        _ => NSColor::systemGreenColor(),
    };
    let from_color = color(from);
    if from == to {
        return from_color;
    }
    from_color
        .blendedColorWithFraction_ofColor(fraction, &color(to))
        .unwrap_or(from_color)
}

#[cfg(target_os = "macos")]
fn render_native_menu(app: &tauri::AppHandle) {
    let app = app.clone();
    let handle = app.clone();
    let _ = handle.run_on_main_thread(move || {
        let Some(native) = app
            .state::<NativeMenuState>()
            .0
            .lock()
            .ok()
            .and_then(|v| *v)
        else {
            return;
        };
        let snapshots = get_quota_snapshots(app.clone());
        let mtm = MainThreadMarker::new().expect("native menu on main thread");
        let menu = NSMenu::new(mtm);
        menu.setAutoenablesItems(false);
        let target = unsafe { &*(native.target as *const QuotaMenuTarget) };
        menu.setDelegate(Some(ProtocolObject::from_ref(target)));
        if snapshots.is_empty() {
            let item = NSMenuItem::new(mtm);
            item.setTitle(&NSString::from_str("Quotas indisponibles"));
            item.setEnabled(false);
            menu.addItem(&item);
        }
        for payload in &snapshots {
            let title = quota_label(payload);
            let (item, handles) = make_quota_item(mtm, &title);
            let percentage = payload.remaining.min(100);
            unsafe {
                let value = &*(handles.value as *const NSTextField);
                let reset = &*(handles.reset as *const NSTextField);
                let fill = &*(handles.fill as *const NSBox);
                value.setStringValue(&NSString::from_str(&format!("{percentage} % restants")));
                reset.setStringValue(&NSString::from_str(&format!(
                    "Réinitialisation : {}",
                    reset_label(payload)
                )));
                fill.setFrameSize(NSSize::new(
                    PROGRESS_WIDTH * f64::from(percentage) / 100.0,
                    8.0,
                ));
                let color = quota_color(percentage);
                fill.setFillColor(&color);
            }
            menu.addItem(&item);
            menu.addItem(&NSMenuItem::separatorItem(mtm));
        }
        let connection_phase = app
            .try_state::<codex::Provider>()
            .map(|provider| provider.status().phase)
            .unwrap_or_else(|| "starting".into());
        if connection_phase != "ready" {
            let connection = NSMenuItem::new(mtm);
            connection.setTitle(&NSString::from_str(if connection_phase == "logging_in" {
                "Connexion en cours…"
            } else {
                "Connexion…"
            }));
            connection.setEnabled(connection_phase != "logging_in");
            unsafe {
                connection.setTarget(Some(target));
                connection.setAction(Some(objc2::sel!(connectAccount:)));
            }
            menu.addItem(&connection);
        }
        for (title, action) in [
            ("Actualiser les quotas", objc2::sel!(refreshQuota:)),
            ("Voir les détails sur Codex…", objc2::sel!(openUsage:)),
            ("Vérifier les mises à jour…", objc2::sel!(checkForUpdates:)),
        ] {
            let item = NSMenuItem::new(mtm);
            item.setTitle(&NSString::from_str(title));
            unsafe {
                item.setTarget(Some(target));
                item.setAction(Some(action));
            }
            menu.addItem(&item);
        }
        let quit = NSMenuItem::new(mtm);
        quit.setTitle(&NSString::from_str("Quitter"));
        unsafe {
            quit.setTarget(Some(target));
            quit.setAction(Some(objc2::sel!(quitApp:)));
        }
        menu.addItem(&quit);
        menu.addItem(&make_version_item(
            mtm,
            &app.package_info().version.to_string(),
        ));
        unsafe {
            let item = &*(native.status_item as *const objc2_app_kit::NSStatusItem);
            item.setMenu(Some(&menu));
            let button = &*(native.status_button as *const objc2_app_kit::NSStatusBarButton);
            let title = snapshots
                .first()
                .map(|p| format!("{}%", p.remaining.min(100)))
                .unwrap_or_else(|| "--".into());
            button.setTitle(&NSString::from_str(&title));
            button.setToolTip(Some(&NSString::from_str(
                &snapshots
                    .first()
                    .map(quota_label)
                    .unwrap_or_else(|| "Quota Codex".into()),
            )));
        }
    });
}

#[cfg(not(target_os = "macos"))]
fn open_refresh_settings(app: &tauri::AppHandle) {
    show_dashboard(app);
    let _ = app.emit("open-refresh-settings", ());
}

fn quota_label(payload: &QuotaPayload) -> String {
    payload
        .label
        .clone()
        .unwrap_or_else(|| match payload.period.as_str() {
            "five-hour" => "Limite 5 heures".into(),
            "weekly" => "Limite hebdomadaire".into(),
            "reserve-weekly" => "Réserve Luna".into(),
            _ => payload.period.clone(),
        })
}

#[cfg(not(target_os = "macos"))]
fn render_native_menu(app: &tauri::AppHandle) {
    if let Some(tray) = app.tray_by_id("quota") {
        let snapshots = get_quota_snapshots(app.clone());
        let text = snapshots
            .first()
            .map(|p| {
                format!(
                    "Quota Codex · {} · {} % restants",
                    quota_label(p),
                    p.remaining.min(100)
                )
            })
            .unwrap_or_else(|| "Quota Codex · Quotas indisponibles".into());
        let _ = tray.set_tooltip(Some(text));
        if let Ok(menu) = desktop_menu(app) {
            let _ = tray.set_menu(Some(menu));
        }
        let _ = tray.set_icon(Some(tray_percentage::icon(
            snapshots.first().map(|p| p.remaining.min(100)),
        )));
    }
}

#[cfg(target_os = "macos")]
fn frame(x: f64, y: f64, width: f64, height: f64) -> NSRect {
    NSRect::new(NSPoint::new(x, y), NSSize::new(width, height))
}

#[cfg(target_os = "macos")]
fn make_quota_item(
    mtm: MainThreadMarker,
    title_text: &str,
) -> (Retained<NSMenuItem>, QuotaViewHandles) {
    let container = NSView::initWithFrame(mtm.alloc(), frame(0.0, 0.0, 280.0, 76.0));

    let title = NSTextField::labelWithString(&NSString::from_str(title_text), mtm);
    title.setFrame(frame(16.0, 50.0, 150.0, 18.0));
    title.setFont(Some(&NSFont::boldSystemFontOfSize(13.0)));

    let value = NSTextField::labelWithString(&NSString::from_str("--"), mtm);
    value.setFrame(frame(164.0, 50.0, 100.0, 18.0));
    value.setAlignment(objc2_app_kit::NSTextAlignment::Right);

    let track = NSBox::initWithFrame(mtm.alloc(), frame(16.0, 33.0, PROGRESS_WIDTH, 8.0));
    track.setBoxType(NSBoxType::Custom);
    track.setBorderWidth(0.0);
    track.setCornerRadius(4.0);
    track.setFillColor(&NSColor::quaternaryLabelColor());

    let fill = NSBox::initWithFrame(mtm.alloc(), frame(16.0, 33.0, 0.0, 8.0));
    fill.setBoxType(NSBoxType::Custom);
    fill.setBorderWidth(0.0);
    fill.setCornerRadius(4.0);
    fill.setFillColor(&NSColor::systemGreenColor());

    let reset = NSTextField::labelWithString(&NSString::from_str("Réinitialisation : --"), mtm);
    reset.setFrame(frame(16.0, 9.0, PROGRESS_WIDTH, 16.0));
    reset.setFont(Some(&NSFont::systemFontOfSize(11.0)));
    reset.setTextColor(Some(&NSColor::secondaryLabelColor()));

    container.addSubview(&title);
    container.addSubview(&value);
    container.addSubview(&track);
    container.addSubview(&fill);
    container.addSubview(&reset);

    let handles = QuotaViewHandles {
        value: Retained::as_ptr(&value) as usize,
        reset: Retained::as_ptr(&reset) as usize,
        fill: Retained::as_ptr(&fill) as usize,
    };
    let item = NSMenuItem::new(mtm);
    item.setView(Some(&container));
    item.setEnabled(false);
    (item, handles)
}

#[cfg(target_os = "macos")]
fn make_version_item(mtm: MainThreadMarker, version: &str) -> Retained<NSMenuItem> {
    let container = NSView::initWithFrame(mtm.alloc(), frame(0.0, 0.0, 280.0, 24.0));
    let label =
        NSTextField::labelWithString(&NSString::from_str(&format!("Quota Codex v{version}")), mtm);
    label.setFrame(frame(16.0, 5.0, 248.0, 14.0));
    label.setFont(Some(&NSFont::systemFontOfSize(10.0)));
    label.setTextColor(Some(&NSColor::tertiaryLabelColor()));
    label.setAlignment(objc2_app_kit::NSTextAlignment::Center);
    container.addSubview(&label);

    let item = NSMenuItem::new(mtm);
    item.setView(Some(&container));
    item.setEnabled(false);
    item
}

#[cfg(target_os = "macos")]
fn build_native_menu(app: &tauri::App) -> Result<(), Box<dyn std::error::Error>> {
    let mtm = MainThreadMarker::new().expect("menu setup on main thread");
    let target = QuotaMenuTarget::new(mtm);
    let status_item =
        NSStatusBar::systemStatusBar().statusItemWithLength(NSVariableStatusItemLength);
    let button = status_item.button(mtm).expect("status item button");
    let bytes = include_bytes!("../icons/mascot-menu.png");
    let data = unsafe { NSData::dataWithBytes_length(bytes.as_ptr().cast(), bytes.len()) };
    if let Some(icon) = NSImage::initWithData(mtm.alloc(), &data) {
        icon.setSize(NSSize::new(18.0, 18.0));
        icon.setTemplate(true);
        button.setImage(Some(&icon));
    }
    *app.state::<NativeMenuState>().0.lock().unwrap() = Some(NativeQuotaMenu {
        status_item: Retained::as_ptr(&status_item) as usize,
        status_button: Retained::as_ptr(&button) as usize,
        target: Retained::as_ptr(&target) as usize,
    });
    // The status item and its target live for the lifetime of this menu-bar app.
    std::mem::forget(status_item);
    std::mem::forget(target);
    render_native_menu(app.handle());
    Ok(())
}

#[cfg(not(target_os = "macos"))]
fn show_dashboard(app: &tauri::AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
        request_quota_refresh(app.clone());
    }
}

#[cfg(not(target_os = "macos"))]
fn desktop_menu(app: &tauri::AppHandle) -> tauri::Result<tauri::menu::Menu<tauri::Wry>> {
    use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
    let menu = Menu::new(app)?;
    let rows = get_quota_snapshots(app.clone());
    if rows.is_empty() {
        menu.append(&MenuItem::new(
            app,
            "Quotas indisponibles",
            false,
            None::<&str>,
        )?)?;
    }
    for row in rows {
        menu.append(&MenuItem::new(
            app,
            format!(
                "{} · {} % restants",
                quota_label(&row),
                row.remaining.min(100)
            ),
            false,
            None::<&str>,
        )?)?;
    }
    menu.append(&PredefinedMenuItem::separator(app)?)?;
    for (id, title) in [
        ("show", "Afficher les quotas"),
        ("refresh", "Actualiser les quotas"),
        ("settings", "Réglages d’actualisation…"),
        ("usage", "Voir les détails sur Codex…"),
        ("update", "Vérifier les mises à jour…"),
        ("quit", "Quitter"),
    ] {
        menu.append(&MenuItem::with_id(app, id, title, true, None::<&str>)?)?;
    }
    menu.append(&MenuItem::new(
        app,
        format!("Quota Codex v{}", app.package_info().version),
        false,
        None::<&str>,
    )?)?;
    Ok(menu)
}

#[cfg(not(target_os = "macos"))]
fn build_desktop(app: &tauri::App) -> Result<(), Box<dyn std::error::Error>> {
    use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};

    let window = app
        .get_webview_window("main")
        .expect("main window was created");
    let menu = desktop_menu(app.handle())?;
    let tray = TrayIconBuilder::with_id("quota")
        .icon(tray_percentage::icon(None))
        .tooltip("Quota Codex · En attente des quotas")
        .menu(&menu)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "show" => show_dashboard(app),
            "settings" => open_refresh_settings(app),
            "refresh" => {
                request_quota_refresh(app.clone());
            }
            "usage" => {
                let _ = open_usage_details(app.clone());
            }
            "update" => {
                tauri::async_runtime::spawn(check_for_available_update(
                    app.clone(),
                    UpdateCheckMode::Interactive,
                ));
            }
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                show_dashboard(tray.app_handle());
            }
        });
    #[cfg(target_os = "windows")]
    let tray = tray.show_menu_on_left_click(false);
    if let Err(error) = tray.build(app) {
        eprintln!("Quota Codex tray unavailable: {error}");
    } else {
        // Linux may report success without a visible tray host. Closing its window
        // therefore exits normally; minimizing keeps the bridge running.
        #[cfg(target_os = "windows")]
        window.on_window_event({
            let window = window.clone();
            move |event| {
                if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                    if window.hide().is_ok() {
                        api.prevent_close();
                    }
                }
            }
        });
    }
    #[cfg(not(target_os = "windows"))]
    let _ = window;
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .manage(QuotaState::default())
        .manage(UpdateState::default())
        .invoke_handler(tauri::generate_handler![
            get_platform,
            get_quota_snapshots,
            request_quota_refresh,
            open_usage_details,
            get_update_status,
            start_update_check,
            start_update_install,
            close_update_window,
            restart_application,
            codex::get_codex_status,
            codex::set_refresh_settings,
            codex::start_codex_login,
            codex::cancel_codex_login,
            codex::set_codex_path,
            codex::open_codex_install,
            codex::open_codex_login
        ])
        .setup(|app| {
            #[cfg(target_os = "macos")]
            let _ = APP_HANDLE.set(app.handle().clone());

            #[cfg(not(debug_assertions))]
            tauri::async_runtime::spawn(check_for_available_update(
                app.handle().clone(),
                UpdateCheckMode::Automatic,
            ));

            #[cfg(not(target_os = "macos"))]
            let _window = tauri::WebviewWindowBuilder::new(
                app,
                "main",
                tauri::WebviewUrl::App("index.html".into()),
            )
            .title("Quota Codex")
            .inner_size(500.0, 620.0)
            .min_inner_size(360.0, 420.0)
            .build()?;
            #[cfg(target_os = "macos")]
            app.handle()
                .set_activation_policy(tauri::ActivationPolicy::Accessory)?;

            #[cfg(target_os = "macos")]
            {
                app.manage(NativeMenuState::default());
                build_native_menu(app)?;
            }

            #[cfg(not(target_os = "macos"))]
            build_desktop(app)?;

            app.manage(codex::start(app.handle().clone()));
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app, event| {
            if let tauri::RunEvent::Exit = event {
                if let Some(provider) = app.try_state::<codex::Provider>() {
                    provider.stop();
                }
            }
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plus_switches_from_five_hour_to_reserve() {
        let mut rows = codex::normalize(&serde_json::json!({"rateLimitsByLimitId": {
            "codex": {"primary": {"usedPercent":20,"windowDurationMins":300}, "secondary":{"usedPercent":10,"windowDurationMins":10080}},
            "luna-reserve": {"primary":{"usedPercent":30,"windowDurationMins":10080}}
        }})).unwrap();
        assert_eq!(visible_quotas(rows.clone(), Some("plus"))[0].remaining, 80);
        assert_eq!(visible_quotas(rows.clone(), Some("plus")).len(), 1);
        assert_eq!(visible_quotas(rows.clone(), Some("pro")).len(), 3);
        rows[0].remaining = 0;
        let selected = visible_quotas(rows.clone(), Some("plus"));
        assert_eq!(selected.len(), 1);
        assert!(selected[0].period.contains("luna-reserve"));
        rows[0].remaining = 1;
        assert_eq!(visible_quotas(rows, Some("plus"))[0].remaining, 1);
    }
    #[test]
    fn missing_reserve_is_not_fabricated() {
        let rows = codex::normalize(&serde_json::json!({"rateLimits":{"primary":{"usedPercent":100,"windowDurationMins":300}}})).unwrap();
        assert!(visible_quotas(rows, Some("plus")).is_empty());
    }

    #[test]
    fn quota_color_blends_between_thresholds() {
        assert_eq!(quota_color_transition(20), ("red", "red", 0.0));
        assert_eq!(quota_color_transition(30), ("red", "orange", 0.5));
        assert_eq!(quota_color_transition(40), ("red", "orange", 1.0));
        assert_eq!(quota_color_transition(50), ("orange", "green", 0.5));
        assert_eq!(quota_color_transition(60), ("orange", "green", 1.0));
        assert_eq!(quota_color_transition(80), ("green", "green", 0.0));
    }
}
