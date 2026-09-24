use chrono::{DateTime, Local};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Mutex, OnceLock,
    },
    time::{SystemTime, UNIX_EPOCH},
};
use tauri::{Emitter, Manager, State};
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
    NSWorkspace, NSWorkspaceOpenConfiguration,
};
#[cfg(target_os = "macos")]
use objc2_foundation::{
    MainThreadMarker, NSArray, NSData, NSObject, NSObjectProtocol, NSPoint, NSRect, NSSize,
    NSString, NSURL,
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
}

#[derive(Debug, Deserialize)]
struct BrowserConnectionPayload {
    connected: bool,
}

fn browser_source() -> String {
    "browser".to_string()
}

#[derive(Default)]
struct QuotaState(Mutex<HashMap<String, BrowserQuotaPayload>>);

static REFRESH_TOKEN: AtomicU64 = AtomicU64::new(0);
static LAST_BROWSER_MESSAGE_AT: AtomicU64 = AtomicU64::new(0);
static UPDATE_CHECK_IN_PROGRESS: AtomicBool = AtomicBool::new(false);
static APP_HANDLE: OnceLock<tauri::AppHandle> = OnceLock::new();

#[cfg_attr(debug_assertions, allow(dead_code))]
#[derive(Clone, Copy, PartialEq, Eq)]
enum UpdateCheckMode {
    Automatic,
    Interactive,
}

struct UpdateCheckGuard;

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

#[cfg(not(target_os = "macos"))]
async fn show_update_alert(
    _app: &tauri::AppHandle,
    title: impl Into<String>,
    message: impl Into<String>,
    _button: impl Into<String>,
    _is_error: bool,
) {
    eprintln!("{}: {}", title.into(), message.into());
}

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
    five_hour: QuotaViewHandles,
    weekly: QuotaViewHandles,
    status_button: usize,
}

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
            request_quota_refresh();
            if !browser_connection_is_active() {
                ensure_brave_running_in_background();
            }
        }
    }

    impl QuotaMenuTarget {
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
        .map(|snapshots| snapshots.values().cloned().collect())
        .unwrap_or_default()
}

#[tauri::command]
fn request_quota_refresh() -> u64 {
    REFRESH_TOKEN.fetch_add(1, Ordering::Relaxed) + 1
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
    let Some(url) = NSURL::URLWithString(&NSString::from_str(
        "https://chatgpt.com/codex/cloud/settings/analytics#usage",
    )) else {
        return;
    };

    let workspace = NSWorkspace::sharedWorkspace();
    let brave_bundle_id = NSString::from_str("com.brave.Browser");
    if let Some(brave_url) = workspace.URLForApplicationWithBundleIdentifier(&brave_bundle_id) {
        let urls = NSArray::from_slice(&[&*url]);
        let configuration = NSWorkspaceOpenConfiguration::configuration();
        workspace.openURLs_withApplicationAtURL_configuration_completionHandler(
            &urls,
            &brave_url,
            &configuration,
            None,
        );
    } else {
        workspace.openURL(&url);
    }
}

#[cfg(target_os = "macos")]
fn ensure_brave_running_in_background() {
    let workspace = NSWorkspace::sharedWorkspace();
    let brave_bundle_id = NSString::from_str("com.brave.Browser");
    let Some(brave_url) = workspace.URLForApplicationWithBundleIdentifier(&brave_bundle_id) else {
        open_usage_page();
        return;
    };

    let configuration = NSWorkspaceOpenConfiguration::configuration();
    configuration.setActivates(false);
    workspace.openApplicationAtURL_configuration_completionHandler(
        &brave_url,
        &configuration,
        None,
    );
}

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
fn update_native_menu(app: &tauri::AppHandle, payload: &BrowserQuotaPayload) {
    let native_menu = app
        .state::<NativeMenuState>()
        .0
        .lock()
        .ok()
        .and_then(|menu| menu.clone());
    let Some(native_menu) = native_menu else {
        return;
    };

    let percentage = payload.remaining.min(100);
    let reset = reset_label(payload);

    let handles = match payload.period.as_str() {
        "five-hour" => native_menu.five_hour,
        "weekly" => native_menu.weekly,
        _ => return,
    };
    let update_status = payload.period == "five-hour";

    let _ = app.run_on_main_thread(move || unsafe {
        let value = &*(handles.value as *const NSTextField);
        let reset_label = &*(handles.reset as *const NSTextField);
        let fill = &*(handles.fill as *const NSBox);

        value.setStringValue(&NSString::from_str(&format!("{percentage} % restants")));
        reset_label.setStringValue(&NSString::from_str(&format!("Réinitialisation : {reset}")));
        fill.setFrameSize(NSSize::new(
            PROGRESS_WIDTH * f64::from(percentage) / 100.0,
            8.0,
        ));
        let color = if percentage <= 20 {
            NSColor::systemRedColor()
        } else if percentage <= 40 {
            NSColor::systemOrangeColor()
        } else {
            NSColor::systemGreenColor()
        };
        fill.setFillColor(&color);

        if update_status {
            let button = &*(native_menu.status_button as *const objc2_app_kit::NSStatusBarButton);
            button.setTitle(&NSString::from_str(&format!("{percentage}%")));
        }
    });
}

#[cfg(target_os = "macos")]
fn set_native_menu_disconnected(app: &tauri::AppHandle) {
    let native_menu = app
        .state::<NativeMenuState>()
        .0
        .lock()
        .ok()
        .and_then(|menu| menu.clone());
    let Some(native_menu) = native_menu else {
        return;
    };

    let _ = app.run_on_main_thread(move || unsafe {
        for handles in [native_menu.five_hour, native_menu.weekly] {
            let value = &*(handles.value as *const NSTextField);
            let reset_label = &*(handles.reset as *const NSTextField);
            let fill = &*(handles.fill as *const NSBox);
            value.setStringValue(&NSString::from_str("--"));
            reset_label.setStringValue(&NSString::from_str("Réinitialisation : --"));
            fill.setFrameSize(NSSize::new(0.0, 8.0));
        }

        let button = &*(native_menu.status_button as *const objc2_app_kit::NSStatusBarButton);
        button.setTitle(&NSString::from_str("--"));
    });
}

#[cfg(not(target_os = "macos"))]
fn update_native_menu(_app: &tauri::AppHandle, _payload: &BrowserQuotaPayload) {}

#[cfg(not(target_os = "macos"))]
fn set_native_menu_disconnected(_app: &tauri::AppHandle) {}

fn mark_browser_disconnected(app: &tauri::AppHandle) {
    let was_connected = LAST_BROWSER_MESSAGE_AT.swap(0, Ordering::AcqRel) > 0;
    if !was_connected {
        return;
    }

    if let Ok(mut snapshots) = app.state::<QuotaState>().0.lock() {
        snapshots.clear();
    }
    set_native_menu_disconnected(app);
    let _ = app.emit("quota-disconnected", ());
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
    let mtm = MainThreadMarker::new().expect("menu setup must run on the main thread");
    let menu = NSMenu::new(mtm);
    menu.setAutoenablesItems(false);

    let (five_hour_item, five_hour) = make_quota_item(mtm, "Limite 5 heures");
    let (weekly_item, weekly) = make_quota_item(mtm, "Limite globale");
    menu.addItem(&five_hour_item);
    menu.addItem(&NSMenuItem::separatorItem(mtm));
    menu.addItem(&weekly_item);
    menu.addItem(&NSMenuItem::separatorItem(mtm));

    let target = QuotaMenuTarget::new(mtm);
    menu.setDelegate(Some(ProtocolObject::from_ref(&*target)));

    let check_updates = NSMenuItem::new(mtm);
    check_updates.setTitle(&NSString::from_str("Vérifier les mises à jour…"));
    unsafe {
        check_updates.setTarget(Some(&target));
        check_updates.setAction(Some(objc2::sel!(checkForUpdates:)));
    }
    menu.addItem(&check_updates);

    let open_usage = NSMenuItem::new(mtm);
    open_usage.setTitle(&NSString::from_str("Ouvrir la page d’utilisation…"));
    unsafe {
        open_usage.setTarget(Some(&target));
        open_usage.setAction(Some(objc2::sel!(openUsage:)));
    }
    menu.addItem(&open_usage);

    menu.addItem(&NSMenuItem::separatorItem(mtm));
    let quit = NSMenuItem::new(mtm);
    quit.setTitle(&NSString::from_str("Quitter"));
    let application = NSApplication::sharedApplication(mtm);
    unsafe {
        quit.setTarget(Some(&application));
        quit.setAction(Some(objc2::sel!(terminate:)));
    }
    menu.addItem(&quit);

    let version = app.package_info().version.to_string();
    menu.addItem(&make_version_item(mtm, &version));

    let status_item =
        NSStatusBar::systemStatusBar().statusItemWithLength(NSVariableStatusItemLength);
    status_item.setMenu(Some(&menu));
    let button = status_item.button(mtm).expect("status item button");
    button.setTitle(&NSString::from_str("--"));
    button.setToolTip(Some(&NSString::from_str("Quota Codex")));

    let icon_bytes = include_bytes!("../icons/mascot-menu.png");
    let data =
        unsafe { NSData::dataWithBytes_length(icon_bytes.as_ptr().cast(), icon_bytes.len()) };
    if let Some(icon) = NSImage::initWithData(mtm.alloc(), &data) {
        icon.setSize(NSSize::new(18.0, 18.0));
        icon.setTemplate(true);
        button.setImage(Some(&icon));
    }

    *app.state::<NativeMenuState>().0.lock().unwrap() = Some(NativeQuotaMenu {
        five_hour,
        weekly,
        status_button: Retained::as_ptr(&button) as usize,
    });

    // AppKit retains the menu and its views. Keep the status item and action target alive
    // for the entire lifetime of this menu-bar-only application.
    std::mem::forget(status_item);
    std::mem::forget(target);
    Ok(())
}

fn start_browser_bridge(app: tauri::AppHandle) {
    std::thread::spawn(move || {
        let Ok(server) = Server::http(BRIDGE_ADDRESS) else {
            eprintln!("Quota Codex bridge unavailable on {BRIDGE_ADDRESS}");
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
                mark_browser_connected();
                if let Ok(mut snapshots) = app.state::<QuotaState>().0.lock() {
                    snapshots.insert(payload.period.clone(), payload.clone());
                }

                update_native_menu(&app, &payload);
                let _ = app.emit("quota-updated", &payload);
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
                snapshots.clear();
            }
            set_native_menu_disconnected(&app);
            let _ = app.emit("quota-disconnected", ());
        }
    });
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .manage(QuotaState::default())
        .manage(NativeMenuState::default())
        .invoke_handler(tauri::generate_handler![
            get_platform,
            get_quota_snapshots,
            request_quota_refresh
        ])
        .setup(|app| {
            let _ = APP_HANDLE.set(app.handle().clone());

            #[cfg(not(debug_assertions))]
            tauri::async_runtime::spawn(check_and_install_update(
                app.handle().clone(),
                UpdateCheckMode::Automatic,
            ));

            #[cfg(target_os = "macos")]
            app.handle()
                .set_activation_policy(tauri::ActivationPolicy::Accessory)?;

            #[cfg(target_os = "macos")]
            build_native_menu(app)?;

            start_browser_bridge(app.handle().clone());
            start_connection_watchdog(app.handle().clone());
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(test)]
mod tests {
    use super::*;

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
