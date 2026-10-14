mod activity;
mod claude;
mod claude_cli;
mod codex;
mod indicator;
mod prefs;
#[cfg(any(not(target_os = "macos"), test))]
mod tray_percentage;

/// Packaged executable smoke of the production transport, without opening UI or touching preferences.
pub fn claude_usage_smoke(directory: &str) -> Result<(), String> {
    let executable = claude_cli::discover().ok_or("Claude Code absent")?;
    let payload = claude_cli::read_usage(&executable, std::path::Path::new(directory))
        .map_err(str::to_string)?;
    if payload["rate_limits_available"] != true {
        return Err("Limites d’abonnement non disponibles".into());
    }
    let five = claude_cli::window(&payload["rate_limits"]["five_hour"]);
    let seven = claude_cli::window(&payload["rate_limits"]["seven_day"]);
    if five.is_none() && seven.is_none() {
        return Err("Aucune fenêtre reçue".into());
    }
    println!(
        "CLAUDE_USAGE_SMOKE_PASS {}",
        serde_json::json!({"fiveHour":five,"sevenDay":seven})
    );
    Ok(())
}

/// Entry for the `--claude-statusline-relay` helper mode (no UI is started).
pub fn claude_relay(args: &[String]) -> i32 {
    claude::relay_main(args)
}

/// True when the process was started as the Claude Code statusLine relay.
pub fn is_claude_relay(args: &[String]) -> bool {
    args.iter().any(|a| a == claude::RELAY_FLAG)
}

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
    let _ = app.emit("quota-updated", ());
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
        .title("Mise à jour de Quota Monitor")
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
    eprintln!("Quota Monitor update failed: {error}");
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
                    "La version {current_version} est installée. Quota Monitor v{next_version} est disponible."
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
        format!("Téléchargement de Quota Monitor v{next_version}…"),
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
                        format!("Téléchargement de Quota Monitor v{progress_version}…"),
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
            format!("Quota Monitor v{next_version} est installé et prêt à être relancé."),
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
fn dismiss_update_window(app: tauri::AppHandle) {
    if let Some(window) = app.get_webview_window("updater") {
        let _ = window.hide();
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
        #[unsafe(method(openUsage:))]
        fn open_usage(&self, _sender: Option<&AnyObject>) {
            open_usage_page();
        }

        #[unsafe(method(openQuotaDashboard:))]
        fn open_quota_dashboard(&self, _sender: Option<&AnyObject>) {
            if let Some(app) = APP_HANDLE.get().cloned() {
                tauri::async_runtime::spawn(async move {
                    let window = if let Some(w) = app.get_webview_window("main") { Some(w) }
                    else { tauri::WebviewWindowBuilder::new(&app, "main", tauri::WebviewUrl::App("index.html".into())).title("Quota Monitor — Détails").inner_size(720.0, 580.0).build().ok() };
                    if let Some(w) = window { let _ = w.show(); let _ = w.set_focus(); }
                });
            }
        }
        #[unsafe(method(openSettings:))]
        fn open_settings_action(&self, _sender: Option<&AnyObject>) {
            if let Some(app) = APP_HANDLE.get().cloned() { open_settings_async(app); }
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
    if !prefs::current(&app).codex_enabled {
        return Vec::new();
    }
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
    let reserve = |p: &QuotaPayload| is_reserve(p);
    let five = |p: &QuotaPayload| {
        !reserve(p) && (p.period == "five-hour" || p.window_minutes == Some(300))
    };
    let weekly =
        |p: &QuotaPayload| !reserve(p) && (p.period == "weekly" || p.window_minutes == Some(10080));
    let five_active = rows.iter().any(|p| five(p) && p.remaining > 0);
    rows.sort_by_key(|p| quota_order(p, &five, &weekly, &reserve, five_active));
    if plan != Some("plus") {
        return rows;
    }
    let relevant: Vec<_> = rows
        .iter()
        .filter(|p| five(p) || weekly(p) || reserve(p))
        .cloned()
        .collect();
    if relevant.is_empty() {
        rows
    } else {
        relevant
    }
}

fn is_reserve(payload: &QuotaPayload) -> bool {
    let identity = format!("{} {}", payload.period, payload.model).to_ascii_lowercase();
    identity.contains("reserve")
        || identity.contains("luna")
        || identity.contains("base_model_inference")
}

fn quota_order(
    payload: &QuotaPayload,
    five: &impl Fn(&QuotaPayload) -> bool,
    weekly: &impl Fn(&QuotaPayload) -> bool,
    reserve: &impl Fn(&QuotaPayload) -> bool,
    five_active: bool,
) -> (u8, bool, String) {
    let rank = if five_active {
        if five(payload) {
            0
        } else if weekly(payload) {
            1
        } else if reserve(payload) {
            2
        } else {
            3
        }
    } else if reserve(payload) {
        0
    } else if weekly(payload) {
        1
    } else if five(payload) {
        2
    } else {
        3
    };
    (
        rank,
        !payload.period.starts_with("codex:codex:"),
        payload.period.clone(),
    )
}

#[tauri::command]
fn request_quota_refresh(app: tauri::AppHandle) {
    crate::claude::request_claude_refresh(app.clone());
    if !prefs::current(&app).codex_enabled {
        return;
    }
    if let Some(provider) = app.try_state::<codex::Provider>() {
        provider.refresh();
    }
}

fn show_settings_window(app: &tauri::AppHandle) -> tauri::Result<()> {
    let window = if let Some(window) = app.get_webview_window("settings") {
        window
    } else {
        tauri::WebviewWindowBuilder::new(
            app,
            "settings",
            tauri::WebviewUrl::App("index.html".into()),
        )
        .title("Réglages de Quota Monitor")
        .inner_size(600.0, 560.0)
        .min_inner_size(520.0, 420.0)
        .center()
        .build()?
    };
    window.show()?;
    window.unminimize()?;
    window.set_focus()?;
    #[cfg(target_os = "macos")]
    let _ = app.run_on_main_thread(|| {
        let mtm = MainThreadMarker::new().expect("settings activation runs on main thread");
        NSApplication::sharedApplication(mtm).activate();
    });
    Ok(())
}

/// Window creation must not run on the main thread inside a menu callback.
fn open_settings_async(app: tauri::AppHandle) {
    tauri::async_runtime::spawn(async move {
        if let Err(error) = show_settings_window(&app) {
            eprintln!("Quota Monitor settings unavailable: {error}");
        }
    });
}

#[tauri::command]
async fn open_settings(app: tauri::AppHandle) -> Result<(), String> {
    show_settings_window(&app).map_err(|e| e.to_string())
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
            if payload.period == "five-hour" || payload.window_minutes == Some(300) {
                local.format("%Hh%M").to_string()
            } else {
                local.format("%d/%m à %Hh%M").to_string()
            }
        })
        .ok()
        .or_else(|| payload.reset_label.clone())
        .unwrap_or_else(|| "--".to_string())
}

#[cfg(any(target_os = "macos", test))]
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
        let prefs = prefs::current(&app);
        let claude_snap = claude::snapshot(&app);
        let claude_visible = claude_is_visible(&claude_snap);
        let mtm = MainThreadMarker::new().expect("native menu on main thread");
        let menu = NSMenu::new(mtm);
        menu.setAutoenablesItems(false);
        let target = unsafe { &*(native.target as *const QuotaMenuTarget) };
        menu.setDelegate(Some(ProtocolObject::from_ref(target)));
        let mut cards = Vec::new();
        if prefs.codex_enabled {
            let short = short_codex_quota(&snapshots);
            let details = snapshots
                .iter()
                .map(|p| {
                    format!(
                        "{} : {} % restants — reset {}",
                        quota_label(p),
                        p.remaining,
                        reset_label(p)
                    )
                })
                .collect::<Vec<_>>()
                .join("\n");
            cards.push(("Codex", short.map(|p| p.remaining), details));
        }
        if claude_visible {
            let short = claude_snap
                .metrics
                .iter()
                .find(|m| m.id == "claude:five_hour" && m.availability == "available");
            let mut details = claude_snap
                .metrics
                .iter()
                .map(|m| {
                    format!(
                        "{} : {} — reset {}",
                        m.label,
                        m.used_percent
                            .map(|u| format!("{} % restants", remaining_percent(u)))
                            .unwrap_or_else(|| "Non communiqué".into()),
                        m.reset_at
                            .as_deref()
                            .map(|at| format_reset(at, m.period_duration_seconds <= 18000))
                            .unwrap_or_else(|| "non communiqué".into())
                    )
                })
                .collect::<Vec<_>>()
                .join("\n");
            if !claude_snap.message.is_empty() {
                details.push_str(&format!("\n{}", claude_snap.message));
            }
            let shown = if claude_snap.state == "error" {
                None
            } else {
                short.and_then(|m| m.used_percent).map(remaining_percent)
            };
            cards.push(("Claude", shown, details));
        }
        if cards.is_empty() {
            add_note(mtm, &menu, "Aucun fournisseur actif");
        } else {
            menu.addItem(&make_provider_columns(mtm, &cards, target));
        }
        let connection_phase = app
            .try_state::<codex::Provider>()
            .map(|provider| provider.status().phase)
            .unwrap_or_else(|| "starting".into());
        if prefs.codex_enabled && connection_phase != "ready" {
            let connection = NSMenuItem::new(mtm);
            connection.setTitle(&NSString::from_str(if connection_phase == "logging_in" {
                "Connexion en cours…"
            } else {
                "Connexion à Codex…"
            }));
            connection.setEnabled(connection_phase != "logging_in");
            unsafe {
                connection.setTarget(Some(target));
                connection.setAction(Some(objc2::sel!(connectAccount:)));
            }
            menu.addItem(&connection);
        }
        for (title, action, key) in [
            ("Réglages…", objc2::sel!(openSettings:), ","),
            ("Voir les détails sur Codex…", objc2::sel!(openUsage:), ""),
            (
                "Vérifier les mises à jour…",
                objc2::sel!(checkForUpdates:),
                "",
            ),
        ] {
            let item = NSMenuItem::new(mtm);
            item.setTitle(&NSString::from_str(title));
            item.setKeyEquivalent(&NSString::from_str(key));
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
        let indicator = indicator_for(&app, &snapshots, &claude_snap);
        unsafe {
            let item = &*(native.status_item as *const objc2_app_kit::NSStatusItem);
            item.setMenu(Some(&menu));
            let button = &*(native.status_button as *const objc2_app_kit::NSStatusBarButton);
            button.setTitle(&NSString::from_str(&indicator.title));
            button.setToolTip(Some(&NSString::from_str(&indicator.tooltip)));
        }
    });
}

#[cfg(target_os = "macos")]
fn add_note(mtm: MainThreadMarker, menu: &NSMenu, text: &str) {
    let item = NSMenuItem::new(mtm);
    item.setTitle(&NSString::from_str(text));
    item.setEnabled(false);
    menu.addItem(&item);
}

#[cfg(target_os = "macos")]
fn add_quota_row(
    mtm: MainThreadMarker,
    menu: &NSMenu,
    title: &str,
    value_text: &str,
    reset_text: &str,
    remaining: Option<u32>,
) {
    let (item, handles) = make_quota_item(mtm, title);
    let percentage = remaining.unwrap_or(0);
    unsafe {
        let value = &*(handles.value as *const NSTextField);
        let reset = &*(handles.reset as *const NSTextField);
        let fill = &*(handles.fill as *const NSBox);
        value.setStringValue(&NSString::from_str(value_text));
        reset.setStringValue(&NSString::from_str(reset_text));
        fill.setFrameSize(NSSize::new(
            PROGRESS_WIDTH * f64::from(percentage) / 100.0,
            8.0,
        ));
        fill.setFillColor(&quota_color(percentage));
    }
    menu.addItem(&item);
    menu.addItem(&NSMenuItem::separatorItem(mtm));
}

#[cfg(target_os = "macos")]
fn format_reset(rfc3339: &str, short_window: bool) -> String {
    DateTime::parse_from_rfc3339(rfc3339)
        .map(|date| {
            let local = date.with_timezone(&Local);
            if short_window {
                local.format("%Hh%M").to_string()
            } else {
                local.format("%d/%m à %Hh%M").to_string()
            }
        })
        .unwrap_or_else(|_| "non communiquée".into())
}

fn remaining_percent(used: f64) -> u32 {
    (100.0 - used).clamp(0.0, 100.0).round() as u32
}

fn claude_is_visible(snapshot: &claude::ClaudeSnapshot) -> bool {
    !matches!(snapshot.state, "disabled" | "not_configured")
}

#[cfg(any(target_os = "macos", test))]
fn claude_status_line(state: &str) -> Option<&'static str> {
    match state {
        "waiting_for_data" => Some("Claude : en attente de Claude Code…"),
        "stale" => Some("Claude : données anciennes"),
        "error" | "unavailable" | "auth_required" => Some("Claude : erreur de lecture"),
        _ => None,
    }
}

fn short_codex_quota(rows: &[QuotaPayload]) -> Option<&QuotaPayload> {
    rows.iter().find(|row| {
        !is_reserve(row) && (row.window_minutes == Some(300) || row.period == "five-hour")
    })
}

fn codex_display(app: &tauri::AppHandle, rows: &[QuotaPayload]) -> Option<indicator::Display> {
    if !prefs::current(app).codex_enabled {
        return None;
    }
    if let Some(first) = short_codex_quota(rows) {
        return Some(indicator::Display::Percent(first.remaining.min(100)));
    }
    let failed = app
        .try_state::<codex::Provider>()
        .is_some_and(|p| p.status().phase == "error");
    Some(if failed {
        indicator::Display::Error
    } else {
        indicator::Display::Pending
    })
}

fn claude_display(snapshot: &claude::ClaudeSnapshot) -> Option<indicator::Display> {
    if !claude_is_visible(snapshot) {
        return None;
    }
    if matches!(snapshot.state, "error" | "unavailable" | "auth_required") {
        return Some(indicator::Display::Error);
    }
    let short = snapshot
        .metrics
        .first()
        .filter(|m| m.availability == "available")
        .and_then(|m| m.used_percent);
    Some(match short {
        Some(used) => indicator::Display::Percent(remaining_percent(used)),
        None => indicator::Display::Pending,
    })
}

fn indicator_for(
    app: &tauri::AppHandle,
    rows: &[QuotaPayload],
    claude_snap: &claude::ClaudeSnapshot,
) -> indicator::Indicator {
    indicator::compute(
        &prefs::current(app).indicator,
        codex_display(app, rows),
        claude_display(claude_snap),
    )
}

fn quota_label(payload: &QuotaPayload) -> String {
    if is_reserve(payload) {
        return "Réserve".into();
    }
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
        let indicator = indicator_for(app, &snapshots, &claude::snapshot(app));
        let _ = tray.set_tooltip(Some(indicator.tooltip));
        if let Ok(menu) = desktop_menu(app) {
            let _ = tray.set_menu(Some(menu));
        }
        let _ = tray.set_icon(Some(tray_percentage::icon(indicator.icon_value)));
    }
}

#[cfg(target_os = "macos")]
fn frame(x: f64, y: f64, width: f64, height: f64) -> NSRect {
    NSRect::new(NSPoint::new(x, y), NSSize::new(width, height))
}

#[cfg(target_os = "macos")]
pub fn quota_ui_smoke(directory: &str) {
    use objc2_app_kit::{NSBackingStoreType, NSBitmapImageFileType, NSWindow, NSWindowStyleMask};
    use objc2_foundation::NSDictionary;
    let mtm = MainThreadMarker::new().expect("main thread");
    let _app = NSApplication::sharedApplication(mtm);
    std::fs::create_dir_all(directory).unwrap();
    let target = QuotaMenuTarget::new(mtm);
    let cards = [
        (
            "Codex",
            Some(63),
            "Hebdomadaire : 85 % restants, reset 21/10".into(),
        ),
        (
            "Claude",
            Some(0),
            "Hebdomadaire : 86 % restants, reset 21/10".into(),
        ),
    ];
    for count in [1, 2] {
        let item = make_provider_columns(mtm, &cards[..count], &target);
        let view = item.view().unwrap();
        let bounds = view.bounds();
        assert_eq!(bounds.size.width, if count == 2 { 440.0 } else { 280.0 });
        let window = unsafe {
            NSWindow::initWithContentRect_styleMask_backing_defer(
                mtm.alloc(),
                frame(-20000.0, -20000.0, bounds.size.width, bounds.size.height),
                NSWindowStyleMask::Borderless,
                NSBackingStoreType::Buffered,
                false,
            )
        };
        window.setContentView(Some(&view));
        view.layoutSubtreeIfNeeded();
        let bitmap = view
            .bitmapImageRepForCachingDisplayInRect(bounds)
            .expect("bitmap");
        view.cacheDisplayInRect_toBitmapImageRep(bounds, &bitmap);
        let png = unsafe {
            bitmap.representationUsingType_properties(
                NSBitmapImageFileType::PNG,
                &NSDictionary::new(),
            )
        }
        .unwrap();
        let path = format!("{directory}/native-{count}-providers.png");
        assert!(png.writeToFile_atomically(&NSString::from_str(&path), true));
        println!(
            "NATIVE_LAYOUT_PASS {count} providers: {}x{}",
            bounds.size.width, bounds.size.height
        );
    }
}

#[cfg(target_os = "macos")]
fn make_provider_columns(
    mtm: MainThreadMarker,
    cards: &[(&str, Option<u32>, String)],
    target: &QuotaMenuTarget,
) -> Retained<NSMenuItem> {
    use objc2_app_kit::NSButton;
    let width = if cards.len() == 2 { 440.0 } else { 280.0 };
    let column = width / cards.len() as f64;
    let root = NSView::initWithFrame(mtm.alloc(), frame(0.0, 0.0, width, 116.0));
    for (i, (name, remaining, details)) in cards.iter().enumerate() {
        let x = i as f64 * column + 16.0;
        let title = NSTextField::labelWithString(&NSString::from_str(name), mtm);
        title.setFrame(frame(x, 86.0, column - 32.0, 20.0));
        title.setFont(Some(&NSFont::boldSystemFontOfSize(14.0)));
        root.addSubview(&title);
        let value = NSTextField::labelWithString(
            &NSString::from_str(
                &remaining
                    .map(|p| format!("{p} % restants"))
                    .unwrap_or_else(|| "Indisponible".into()),
            ),
            mtm,
        );
        value.setFrame(frame(x, 62.0, column - 32.0, 20.0));
        root.addSubview(&value);
        let track = NSBox::initWithFrame(mtm.alloc(), frame(x, 48.0, column - 32.0, 8.0));
        track.setBoxType(NSBoxType::Custom);
        track.setBorderWidth(0.0);
        track.setCornerRadius(4.0);
        track.setFillColor(&NSColor::quaternaryLabelColor());
        root.addSubview(&track);
        let percent = remaining.unwrap_or(0);
        let fill = NSBox::initWithFrame(
            mtm.alloc(),
            frame(
                x,
                48.0,
                (column - 32.0) * f64::from(percent.min(100)) / 100.0,
                8.0,
            ),
        );
        fill.setBoxType(NSBoxType::Custom);
        fill.setBorderWidth(0.0);
        fill.setCornerRadius(4.0);
        fill.setFillColor(&quota_color(percent));
        root.addSubview(&fill);
        let button = unsafe {
            NSButton::buttonWithTitle_target_action(
                &NSString::from_str("Détails…"),
                Some(target),
                Some(objc2::sel!(openQuotaDashboard:)),
                mtm,
            )
        };
        button.setFrame(frame(x, 10.0, column - 32.0, 26.0));
        button.setBordered(false);
        button.setToolTip(Some(&NSString::from_str(details)));
        root.addSubview(&button);
    }
    let item = NSMenuItem::new(mtm);
    item.setView(Some(&root));
    item.setEnabled(true);
    item
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
    let label = NSTextField::labelWithString(
        &NSString::from_str(&format!("Quota Monitor v{version}")),
        mtm,
    );
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
    let prefs = prefs::current(app);
    let claude_snap = claude::snapshot(app);
    if !prefs.codex_enabled && !claude_is_visible(&claude_snap) {
        menu.append(&MenuItem::new(
            app,
            "Aucun fournisseur actif",
            false,
            None::<&str>,
        )?)?;
    }
    if prefs.codex_enabled && rows.is_empty() {
        menu.append(&MenuItem::new(
            app,
            "Codex · Quotas indisponibles",
            false,
            None::<&str>,
        )?)?;
    }
    for row in rows {
        menu.append(&MenuItem::new(
            app,
            format!(
                "Codex · {} · {} % restants",
                quota_label(&row),
                row.remaining.min(100)
            ),
            false,
            None::<&str>,
        )?)?;
    }
    if claude_is_visible(&claude_snap) {
        for m in &claude_snap.metrics {
            let text = match (m.availability, m.used_percent) {
                ("available", Some(_)) if m.exhausted => "épuisé".to_string(),
                ("available", Some(u)) => format!("{} % restants", remaining_percent(u)),
                ("unavailable", _) => "réinitialisée · en attente".to_string(),
                _ => "non communiquée".to_string(),
            };
            menu.append(&MenuItem::new(
                app,
                format!("Claude · {} · {text}", m.label),
                false,
                None::<&str>,
            )?)?;
        }
        if let Some(line) = claude_status_line(claude_snap.state) {
            menu.append(&MenuItem::new(app, line, false, None::<&str>)?)?;
        }
    }
    menu.append(&PredefinedMenuItem::separator(app)?)?;
    for (id, title) in [
        ("show", "Afficher les quotas"),
        ("refresh", "Actualiser les quotas"),
        ("settings", "Réglages…"),
        ("usage", "Voir les détails sur Codex…"),
        ("update", "Vérifier les mises à jour…"),
        ("quit", "Quitter"),
    ] {
        menu.append(&MenuItem::with_id(app, id, title, true, None::<&str>)?)?;
    }
    menu.append(&MenuItem::new(
        app,
        format!("Quota Monitor v{}", app.package_info().version),
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
        .tooltip("Quota Monitor · En attente des quotas")
        .menu(&menu)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "show" => show_dashboard(app),
            "settings" => open_settings_async(app.clone()),
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
        eprintln!("Quota Monitor tray unavailable: {error}");
        // A hidden startup must remain recoverable if tray creation fails.
        let _ = window.show();
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
        .manage(prefs::PrefsState::default())
        .invoke_handler(tauri::generate_handler![
            get_platform,
            get_quota_snapshots,
            request_quota_refresh,
            open_settings,
            prefs::get_preferences,
            prefs::set_preferences,
            claude::get_claude_snapshot,
            claude::request_claude_refresh,
            claude::install_claude_relay,
            claude::uninstall_claude_relay,
            open_usage_details,
            get_update_status,
            start_update_check,
            start_update_install,
            dismiss_update_window,
            restart_application,
            codex::get_codex_status,
            codex::set_refresh_settings,
            codex::redeem_codex_reset,
            codex::start_codex_login,
            codex::cancel_codex_login,
            codex::set_codex_path,
            codex::open_codex_install,
            codex::open_codex_login
        ])
        .setup(|app| {
            *app.state::<prefs::PrefsState>().0.lock().unwrap() = prefs::load(app.handle());
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
            .title("Quota Monitor")
            .inner_size(500.0, 620.0)
            .min_inner_size(360.0, 420.0)
            .visible(
                !(cfg!(target_os = "windows")
                    && std::env::args_os().any(|arg| arg == "--autostart")),
            )
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
            claude::start_watcher(app.handle().clone());
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app, event| {
            // Closing the Settings/updater window must not quit the menu-bar app.
            #[cfg(target_os = "macos")]
            if let tauri::RunEvent::ExitRequested { api, code, .. } = &event {
                if code.is_none() {
                    api.prevent_exit();
                }
            }
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
    fn indicator_uses_short_window_even_when_reserve_is_first() {
        let rows = codex::normalize(&serde_json::json!({"rateLimitsByLimitId": {
            "codex": {"primary":{"usedPercent":100,"windowDurationMins":300},"secondary":{"usedPercent":10,"windowDurationMins":10080}},
            "base_model_inference":{"primary":{"usedPercent":30,"windowDurationMins":10080}}
        }})).unwrap();
        let rows = visible_quotas(rows, Some("plus"));
        assert!(is_reserve(&rows[0]));
        assert_eq!(short_codex_quota(&rows).map(|r| r.remaining), Some(0));
    }

    #[test]
    fn weekly_only_does_not_impersonate_short_quota() {
        let rows = codex::normalize(&serde_json::json!({"rateLimits": {
            "primary":{"usedPercent":12,"windowDurationMins":10080}
        }}))
        .unwrap();
        assert!(short_codex_quota(&rows).is_none());
    }

    #[test]
    fn plus_switches_from_five_hour_to_reserve() {
        let mut rows = codex::normalize(&serde_json::json!({"rateLimitsByLimitId": {
            "codex": {"primary": {"usedPercent":20,"windowDurationMins":300}, "secondary":{"usedPercent":10,"windowDurationMins":10080}},
            "base_model_inference": {"primary":{"usedPercent":30,"windowDurationMins":10080}}
        }})).unwrap();
        let selected = visible_quotas(rows.clone(), Some("plus"));
        assert_eq!(selected.len(), 3);
        assert_eq!(selected[0].window_minutes, Some(300));
        assert_eq!(selected[1].window_minutes, Some(10080));
        assert!(selected[2].period.contains("base_model_inference"));
        assert_eq!(visible_quotas(rows.clone(), Some("pro")).len(), 3);
        rows.iter_mut()
            .find(|row| row.window_minutes == Some(300))
            .unwrap()
            .remaining = 0;
        let selected = visible_quotas(rows.clone(), Some("plus"));
        assert_eq!(selected.len(), 3);
        assert!(selected[0].period.contains("base_model_inference"));
        assert_eq!(selected[1].window_minutes, Some(10080));
        assert_eq!(selected[2].window_minutes, Some(300));
        rows.iter_mut()
            .find(|row| row.window_minutes == Some(300))
            .unwrap()
            .remaining = 1;
        assert_eq!(visible_quotas(rows, Some("plus"))[0].remaining, 1);
    }
    #[test]
    fn missing_reserve_is_not_fabricated() {
        let rows = codex::normalize(&serde_json::json!({"rateLimits":{"primary":{"usedPercent":100,"windowDurationMins":300}}})).unwrap();
        assert_eq!(visible_quotas(rows, Some("plus")).len(), 1);
    }

    #[test]
    fn plus_keeps_both_windows_without_reserve() {
        let rows = codex::normalize(&serde_json::json!({"rateLimits":{
            "primary":{"usedPercent":4,"windowDurationMins":300},
            "secondary":{"usedPercent":35,"windowDurationMins":10080}
        }}))
        .unwrap();
        let selected = visible_quotas(rows, Some("plus"));
        assert_eq!(selected.len(), 2);
        assert_eq!(selected[0].window_minutes, Some(300));
        assert_eq!(selected[1].window_minutes, Some(10080));
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
