mod codex;
mod setup;

#[cfg(target_os = "macos")]
use chrono::{DateTime, Local};
use serde::{Deserialize, Serialize};
#[cfg(any(target_os = "macos", not(debug_assertions)))]
use std::sync::atomic::AtomicBool;
#[cfg(target_os = "macos")]
use std::sync::OnceLock;
use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicU64, Ordering},
        Mutex,
    },
    time::{SystemTime, UNIX_EPOCH},
};
use tauri::{Manager, State};
use tiny_http::{Header, Method, Response, Server};

#[cfg(target_os = "macos")]
use objc2::{
    define_class, msg_send,
    rc::Retained,
    runtime::{AnyObject, ProtocolObject},
    MainThreadOnly,
};
#[cfg(target_os = "macos")]
use objc2_app_kit::{
    NSAlert, NSAlertStyle, NSApplication, NSBox, NSBoxType, NSColor, NSFont, NSImage, NSMenu,
    NSMenuDelegate, NSMenuItem, NSStatusBar, NSTextField, NSVariableStatusItemLength, NSView,
};
#[cfg(target_os = "macos")]
use objc2_foundation::{
    MainThreadMarker, NSData, NSObject, NSObjectProtocol, NSPoint, NSRect, NSSize, NSString,
};

const BRIDGE_ADDRESS: &str = "127.0.0.1:48721";
const BROWSER_CONNECTION_TIMEOUT_SECS: u64 = 45;
#[cfg(target_os = "macos")]
const PROGRESS_WIDTH: f64 = 248.0;

#[derive(Debug, Deserialize, Serialize, Clone)]
struct BrowserQuotaPayload {
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
    #[serde(skip_deserializing, default = "browser_source")]
    source: String,
    #[serde(default)]
    label: Option<String>,
}

#[derive(Debug, Deserialize)]
struct BrowserConnectionPayload {
    connected: bool,
}

fn browser_source() -> String {
    "browser".to_string()
}

#[derive(Default)]
struct QuotaData {
    browser: HashMap<String, BrowserQuotaPayload>,
    // Some(empty) is an authoritative response with no published windows.
    codex: Option<Vec<BrowserQuotaPayload>>,
}
impl QuotaData {
    fn effective(&self) -> Vec<BrowserQuotaPayload> {
        if let Some(snapshots) = &self.codex {
            return snapshots.clone();
        }
        let mut snapshots: Vec<_> = self.browser.values().cloned().collect();
        snapshots.sort_by(|a, b| a.period.cmp(&b.period));
        snapshots
    }
}
#[derive(Default)]
struct QuotaState(Mutex<QuotaData>);

fn publish_quotas(app: &tauri::AppHandle) {
    render_native_menu(app);
}

fn set_codex_snapshots(app: &tauri::AppHandle, snapshots: Option<Vec<BrowserQuotaPayload>>) {
    if let Ok(mut data) = app.state::<QuotaState>().0.lock() {
        data.codex = snapshots;
    }
    publish_quotas(app);
}

static LAST_EXTENSION_MESSAGE_AT: AtomicU64 = AtomicU64::new(0);
static BRIDGE_ERROR: Mutex<Option<String>> = Mutex::new(None);

static REFRESH_TOKEN: AtomicU64 = AtomicU64::new(0);
static LAST_BROWSER_MESSAGE_AT: AtomicU64 = AtomicU64::new(0);
#[cfg(any(target_os = "macos", not(debug_assertions)))]
static UPDATE_CHECK_IN_PROGRESS: AtomicBool = AtomicBool::new(false);
#[cfg(target_os = "macos")]
static APP_HANDLE: OnceLock<tauri::AppHandle> = OnceLock::new();

#[cfg(any(target_os = "macos", not(debug_assertions)))]
#[derive(Clone, Copy, PartialEq, Eq)]
enum UpdateCheckMode {
    #[cfg(not(debug_assertions))]
    Automatic,
    Interactive,
}

#[cfg(any(target_os = "macos", not(debug_assertions)))]
struct UpdateCheckGuard;

#[cfg(any(target_os = "macos", not(debug_assertions)))]
impl Drop for UpdateCheckGuard {
    fn drop(&mut self) {
        UPDATE_CHECK_IN_PROGRESS.store(false, Ordering::Release);
    }
}

#[cfg(target_os = "macos")]
async fn show_update_alert(
    app: &tauri::AppHandle,
    title: impl Into<String>,
    message: impl Into<String>,
    button: impl Into<String>,
    is_error: bool,
) {
    let title = title.into();
    let message = message.into();
    let button = button.into();
    let (sender, receiver) = std::sync::mpsc::sync_channel(1);

    if let Err(error) = app.run_on_main_thread(move || {
        let mtm = MainThreadMarker::new().expect("update alert must run on the main thread");
        let alert = NSAlert::new(mtm);
        alert.setMessageText(&NSString::from_str(&title));
        alert.setInformativeText(&NSString::from_str(&message));
        alert.setAlertStyle(if is_error {
            NSAlertStyle::Warning
        } else {
            NSAlertStyle::Informational
        });
        alert.addButtonWithTitle(&NSString::from_str(&button));
        NSApplication::sharedApplication(mtm).activate();
        alert.runModal();
        let _ = sender.send(());
    }) {
        eprintln!("Quota Codex could not display update alert: {error}");
        return;
    }

    let _ = tauri::async_runtime::spawn_blocking(move || receiver.recv()).await;
}

#[cfg(all(not(target_os = "macos"), not(debug_assertions)))]
async fn show_update_alert(
    _app: &tauri::AppHandle,
    title: impl Into<String>,
    message: impl Into<String>,
    _button: impl Into<String>,
    _is_error: bool,
) {
    eprintln!("{}: {}", title.into(), message.into());
}

#[cfg(any(target_os = "macos", not(debug_assertions)))]
async fn show_update_error(app: &tauri::AppHandle, mode: UpdateCheckMode, error: impl ToString) {
    let error = error.to_string();
    eprintln!("Quota Codex update failed: {error}");
    if mode == UpdateCheckMode::Interactive {
        show_update_alert(
            app,
            "Impossible de mettre Quota Codex à jour",
            format!("Vérifie ta connexion Internet puis réessaie.\n\nDétail : {error}"),
            "OK",
            true,
        )
        .await;
    }
}

#[cfg(any(target_os = "macos", not(debug_assertions)))]
async fn check_and_install_update(app: tauri::AppHandle, mode: UpdateCheckMode) {
    use tauri_plugin_updater::UpdaterExt;

    if UPDATE_CHECK_IN_PROGRESS.swap(true, Ordering::AcqRel) {
        if mode == UpdateCheckMode::Interactive {
            show_update_alert(
                &app,
                "Vérification déjà en cours",
                "Quota Codex vérifie ou installe déjà une mise à jour.",
                "OK",
                false,
            )
            .await;
        }
        return;
    }
    let _guard = UpdateCheckGuard;
    let current_version = app.package_info().version.to_string();

    let update = match app.updater() {
        Ok(updater) => updater.check().await,
        Err(error) => {
            show_update_error(&app, mode, error).await;
            return;
        }
    };

    match update {
        Ok(Some(update)) => {
            let next_version = update.version.clone();
            if mode == UpdateCheckMode::Interactive {
                show_update_alert(
                    &app,
                    "Mise à jour disponible",
                    format!(
                        "La version {current_version} est installée. Quota Codex v{next_version} va être téléchargé et installé. Une confirmation apparaîtra lorsque l’installation sera terminée."
                    ),
                    "Installer",
                    false,
                )
                .await;
            }

            let result = update
                .download_and_install(|_chunk_length, _content_length| {}, || {})
                .await;
            if let Err(error) = result {
                show_update_error(&app, mode, error).await;
            } else {
                show_update_alert(
                    &app,
                    "Mise à jour installée",
                    format!(
                        "La version {next_version} a été installée. Quota Codex va maintenant redémarrer."
                    ),
                    "Redémarrer",
                    false,
                )
                .await;
                app.restart();
            }
        }
        Ok(None) if mode == UpdateCheckMode::Interactive => {
            show_update_alert(
                &app,
                "Quota Codex est à jour",
                format!("La version {current_version} est la dernière version disponible."),
                "OK",
                false,
            )
            .await;
        }
        Ok(None) => {}
        Err(error) => show_update_error(&app, mode, error).await,
    }
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

        #[unsafe(method(showDashboard:))]
        fn show_dashboard_action(&self, _sender: Option<&AnyObject>) {
            if let Some(app) = APP_HANDLE.get() { show_dashboard(app); }
        }

        #[unsafe(method(openUsage:))]
        fn open_usage(&self, _sender: Option<&AnyObject>) {
            open_usage_page();
        }

        #[unsafe(method(checkForUpdates:))]
        fn check_for_updates(&self, _sender: Option<&AnyObject>) {
            if let Some(app) = APP_HANDLE.get().cloned() {
                tauri::async_runtime::spawn(check_and_install_update(
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
fn get_quota_snapshots(state: State<'_, QuotaState>) -> Vec<BrowserQuotaPayload> {
    state
        .0
        .lock()
        .map(|snapshots| snapshots.effective())
        .unwrap_or_default()
}

#[tauri::command]
fn request_quota_refresh(app: tauri::AppHandle) -> u64 {
    if let Some(provider) = app.try_state::<codex::Provider>() {
        provider.refresh();
    }
    REFRESH_TOKEN.fetch_add(1, Ordering::Relaxed) + 1
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ConnectionStatus {
    extension_connected: bool,
    usage_page_open: bool,
    bridge_error: Option<String>,
}

#[tauri::command]
fn get_connection_status() -> ConnectionStatus {
    ConnectionStatus {
        extension_connected: connection_is_recent_at(
            LAST_EXTENSION_MESSAGE_AT.load(Ordering::Acquire),
            unix_timestamp_secs(),
        ),
        usage_page_open: browser_connection_is_active(),
        bridge_error: BRIDGE_ERROR.lock().ok().and_then(|error| error.clone()),
    }
}

fn unix_timestamp_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or_default()
}

fn connection_is_recent_at(last_message_at: u64, now: u64) -> bool {
    last_message_at > 0 && now.saturating_sub(last_message_at) <= BROWSER_CONNECTION_TIMEOUT_SECS
}

fn browser_connection_is_active() -> bool {
    connection_is_recent_at(
        LAST_BROWSER_MESSAGE_AT.load(Ordering::Acquire),
        unix_timestamp_secs(),
    )
}

fn mark_browser_connected() {
    LAST_BROWSER_MESSAGE_AT.store(unix_timestamp_secs(), Ordering::Release);
}

#[cfg(target_os = "macos")]
fn open_usage_page() {
    if let Some(app) = APP_HANDLE.get() {
        let _ = setup::open_selected_usage(app.clone());
    }
}

#[cfg(target_os = "macos")]
fn reset_label(payload: &BrowserQuotaPayload) -> String {
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
        let snapshots = get_quota_snapshots(app.state::<QuotaState>());
        let mtm = MainThreadMarker::new().expect("native menu on main thread");
        let menu = NSMenu::new(mtm);
        menu.setAutoenablesItems(false);
        let target = unsafe { &*(native.target as *const QuotaMenuTarget) };
        menu.setDelegate(Some(ProtocolObject::from_ref(target)));
        if snapshots.is_empty() {
            let item = NSMenuItem::new(mtm);
            item.setTitle(&NSString::from_str(
                "Quotas indisponibles · Ouvrir le tableau de bord",
            ));
            unsafe {
                item.setTarget(Some(target));
                item.setAction(Some(objc2::sel!(showDashboard:)));
            }
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
                fill.setFillColor(&if percentage <= 20 {
                    NSColor::systemRedColor()
                } else if percentage <= 40 {
                    NSColor::systemOrangeColor()
                } else {
                    NSColor::systemGreenColor()
                });
            }
            menu.addItem(&item);
            menu.addItem(&NSMenuItem::separatorItem(mtm));
        }
        for (title, action) in [
            ("Tableau de bord et connexion…", objc2::sel!(showDashboard:)),
            (
                "Voir les détails sur la page d’utilisation…",
                objc2::sel!(openUsage:),
            ),
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

fn quota_label(payload: &BrowserQuotaPayload) -> String {
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
        let snapshots = get_quota_snapshots(app.state::<QuotaState>());
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
    }
}

fn mark_browser_disconnected(app: &tauri::AppHandle) {
    let was_connected = LAST_BROWSER_MESSAGE_AT.swap(0, Ordering::AcqRel) > 0;
    if !was_connected {
        return;
    }

    if let Ok(mut snapshots) = app.state::<QuotaState>().0.lock() {
        snapshots.browser.clear();
    }
    publish_quotas(app);
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

fn show_dashboard(app: &tauri::AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
        request_quota_refresh(app.clone());
    }
}

#[cfg(not(target_os = "macos"))]
fn build_desktop(app: &tauri::App) -> Result<(), Box<dyn std::error::Error>> {
    use tauri::{
        menu::{Menu, MenuItem},
        tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    };

    let window = app
        .get_webview_window("main")
        .expect("main window was created");
    let show = MenuItem::with_id(app, "show", "Afficher les quotas", true, None::<&str>)?;
    let refresh = MenuItem::with_id(app, "refresh", "Actualiser les quotas", true, None::<&str>)?;
    let usage = MenuItem::with_id(app, "usage", "Ouvrir Codex", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quitter", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&show, &refresh, &usage, &quit])?;
    let tray = TrayIconBuilder::with_id("quota")
        .icon(tauri::image::Image::from_bytes(include_bytes!(
            "../icons/32x32.png"
        ))?)
        .tooltip("Quota Codex · En attente des quotas")
        .menu(&menu)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "show" => show_dashboard(app),
            "refresh" => {
                request_quota_refresh(app.clone());
            }
            "usage" => {
                let _ = setup::open_selected_usage(app.clone());
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

fn start_browser_bridge(app: tauri::AppHandle) {
    std::thread::spawn(move || {
        let Ok(server) = Server::http(BRIDGE_ADDRESS) else {
            let message = format!("Le port {BRIDGE_ADDRESS} est indisponible. Ferme les autres instances de Quota Codex puis relance l’application.");
            eprintln!("{message}");
            if let Ok(mut error) = BRIDGE_ERROR.lock() {
                *error = Some(message);
            }
            return;
        };

        for mut request in server.incoming_requests() {
            let cors =
                Header::from_bytes("Access-Control-Allow-Origin", "*").expect("valid CORS header");
            let content_type = Header::from_bytes("Content-Type", "application/json")
                .expect("valid content type header");

            if request.method() == &Method::Options {
                let response = Response::empty(204)
                    .with_header(cors)
                    .with_header(
                        Header::from_bytes("Access-Control-Allow-Methods", "POST, OPTIONS")
                            .expect("valid methods header"),
                    )
                    .with_header(
                        Header::from_bytes("Access-Control-Allow-Headers", "Content-Type")
                            .expect("valid headers header"),
                    );
                let _ = request.respond(response);
                continue;
            }

            if request.method() == &Method::Get && request.url() == "/refresh" {
                let refresh_token = REFRESH_TOKEN.load(Ordering::Relaxed);
                let response = Response::from_string(
                    serde_json::json!({ "refreshToken": refresh_token }).to_string(),
                )
                .with_header(cors)
                .with_header(content_type);
                let _ = request.respond(response);
                continue;
            }

            if request.method() == &Method::Post && request.url() == "/connection" {
                let mut body = String::new();
                let payload = request
                    .as_reader()
                    .read_to_string(&mut body)
                    .ok()
                    .and_then(|_| serde_json::from_str::<BrowserConnectionPayload>(&body).ok());

                if let Some(payload) = payload {
                    LAST_EXTENSION_MESSAGE_AT.store(unix_timestamp_secs(), Ordering::Release);
                    if payload.connected {
                        mark_browser_connected();
                    } else {
                        mark_browser_disconnected(&app);
                    }
                    let response = Response::from_string("{\"ok\":true}")
                        .with_header(cors)
                        .with_header(content_type);
                    let _ = request.respond(response);
                } else {
                    let response = Response::from_string("{\"ok\":false}")
                        .with_status_code(400)
                        .with_header(cors)
                        .with_header(content_type);
                    let _ = request.respond(response);
                }
                continue;
            }

            if request.method() != &Method::Post || request.url() != "/quota" {
                let response = Response::from_string("Not found")
                    .with_status_code(404)
                    .with_header(cors);
                let _ = request.respond(response);
                continue;
            }

            let mut body = String::new();
            let parsed = request
                .as_reader()
                .read_to_string(&mut body)
                .ok()
                .and_then(|_| serde_json::from_str::<BrowserQuotaPayload>(&body).ok());

            if let Some(payload) = parsed.filter(|payload| payload.limit > 0) {
                LAST_EXTENSION_MESSAGE_AT.store(unix_timestamp_secs(), Ordering::Release);
                mark_browser_connected();
                if let Ok(mut snapshots) = app.state::<QuotaState>().0.lock() {
                    snapshots
                        .browser
                        .insert(payload.period.clone(), payload.clone());
                }

                publish_quotas(&app);
                let response = Response::from_string("{\"ok\":true}")
                    .with_header(cors)
                    .with_header(content_type);
                let _ = request.respond(response);
            } else {
                let response = Response::from_string("{\"ok\":false}")
                    .with_status_code(400)
                    .with_header(cors)
                    .with_header(content_type);
                let _ = request.respond(response);
            }
        }
    });
}

fn start_connection_watchdog(app: tauri::AppHandle) {
    std::thread::spawn(move || loop {
        std::thread::sleep(std::time::Duration::from_secs(1));
        let last_message_at = LAST_BROWSER_MESSAGE_AT.load(Ordering::Acquire);
        if last_message_at == 0 || connection_is_recent_at(last_message_at, unix_timestamp_secs()) {
            continue;
        }

        if LAST_BROWSER_MESSAGE_AT
            .compare_exchange(last_message_at, 0, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
        {
            if let Ok(mut snapshots) = app.state::<QuotaState>().0.lock() {
                snapshots.browser.clear();
            }
            publish_quotas(&app);
        }
    });
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .manage(QuotaState::default())
        .invoke_handler(tauri::generate_handler![
            get_platform,
            get_quota_snapshots,
            request_quota_refresh,
            get_connection_status,
            setup::get_browser_setup,
            setup::reveal_extension_folder,
            setup::open_browser_setup,
            setup::open_selected_usage,
            codex::get_codex_status,
            codex::start_codex_login,
            codex::cancel_codex_login,
            codex::set_codex_path,
            codex::open_codex_install,
            codex::open_codex_login
        ])
        .setup(|app| {
            if let Err(error) = setup::prepare_extension(app.handle()) {
                eprintln!("Impossible de préparer l’extension : {error}");
            }
            #[cfg(target_os = "macos")]
            let _ = APP_HANDLE.set(app.handle().clone());

            #[cfg(not(debug_assertions))]
            tauri::async_runtime::spawn(check_and_install_update(
                app.handle().clone(),
                UpdateCheckMode::Automatic,
            ));

            let window = tauri::WebviewWindowBuilder::new(
                app,
                "main",
                tauri::WebviewUrl::App("index.html".into()),
            )
            .title("Quota Codex")
            .inner_size(500.0, 620.0)
            .min_inner_size(360.0, 420.0)
            .build()?;
            #[cfg(target_os = "macos")]
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
            #[cfg(not(target_os = "macos"))]
            let _ = window;
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
            start_browser_bridge(app.handle().clone());
            start_connection_watchdog(app.handle().clone());
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
    fn direct_source_wins_and_browser_disconnect_does_not_clear_it() {
        let rows = codex::normalize(&serde_json::json!({"rateLimits":{"secondary":{"usedPercent":59,"windowDurationMins":10080}}})).unwrap();
        let mut data = QuotaData::default();
        let mut browser = rows[0].clone();
        browser.source = "browser".into();
        browser.remaining = 5;
        data.browser.insert("weekly".into(), browser);
        data.codex = Some(rows);
        assert_eq!(data.effective()[0].remaining, 41);
        data.browser.clear();
        assert_eq!(data.effective()[0].source, "codex");
    }

    #[test]
    fn authoritative_empty_hides_fallback_until_primary_fails() {
        let mut data = QuotaData::default();
        let rows =
            codex::normalize(&serde_json::json!({"rateLimits":{"primary":{"usedPercent":25}}}))
                .unwrap();
        data.browser.insert("five-hour".into(), rows[0].clone());
        data.codex = Some(Vec::new());
        assert!(data.effective().is_empty());
        data.codex = None;
        assert_eq!(data.effective().len(), 1);
    }

    #[test]
    fn browser_connection_expires_after_timeout() {
        assert!(connection_is_recent_at(
            100,
            100 + BROWSER_CONNECTION_TIMEOUT_SECS
        ));
        assert!(!connection_is_recent_at(
            100,
            101 + BROWSER_CONNECTION_TIMEOUT_SECS
        ));
    }

    #[test]
    fn missing_browser_message_is_disconnected() {
        assert!(!connection_is_recent_at(0, 100));
    }
}
