use chrono::{DateTime, Local};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, sync::Mutex};
use tauri::{Manager, State};
use tiny_http::{Header, Method, Response, Server};

#[cfg(target_os = "macos")]
use objc2::{define_class, msg_send, rc::Retained, runtime::AnyObject, MainThreadOnly};
#[cfg(target_os = "macos")]
use objc2_app_kit::{
    NSApplication, NSBox, NSBoxType, NSColor, NSFont, NSImage, NSMenu, NSMenuItem, NSStatusBar,
    NSTextField, NSVariableStatusItemLength, NSView, NSWorkspace,
};
#[cfg(target_os = "macos")]
use objc2_foundation::{
    MainThreadMarker, NSData, NSObject, NSObjectProtocol, NSPoint, NSRect, NSSize, NSString, NSURL,
};

const BRIDGE_ADDRESS: &str = "127.0.0.1:48721";
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

fn browser_source() -> String {
    "browser".to_string()
}

#[derive(Default)]
struct QuotaState(Mutex<HashMap<String, BrowserQuotaPayload>>);

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

    impl QuotaMenuTarget {
        #[unsafe(method(openUsage:))]
        fn open_usage(&self, _sender: Option<&AnyObject>) {
            let url = NSURL::URLWithString(&NSString::from_str(
                "https://chatgpt.com/codex/cloud/settings/analytics#usage",
            ));
            if let Some(url) = url {
                NSWorkspace::sharedWorkspace().openURL(&url);
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

#[cfg(not(target_os = "macos"))]
fn update_native_menu(_app: &tauri::AppHandle, _payload: &BrowserQuotaPayload) {}

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

    let value = NSTextField::labelWithString(&NSString::from_str("En attente"), mtm);
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
    let open_usage = NSMenuItem::new(mtm);
    open_usage.setTitle(&NSString::from_str("Ouvrir la page d’utilisation…"));
    unsafe {
        open_usage.setTarget(Some(&target));
        open_usage.setAction(Some(objc2::sel!(openUsage:)));
    }
    menu.addItem(&open_usage);

    let quit = NSMenuItem::new(mtm);
    quit.setTitle(&NSString::from_str("Quitter"));
    let application = NSApplication::sharedApplication(mtm);
    unsafe {
        quit.setTarget(Some(&application));
        quit.setAction(Some(objc2::sel!(terminate:)));
    }
    menu.addItem(&quit);

    let status_item =
        NSStatusBar::systemStatusBar().statusItemWithLength(NSVariableStatusItemLength);
    status_item.setMenu(Some(&menu));
    let button = status_item.button(mtm).expect("status item button");
    button.setTitle(&NSString::from_str("--%"));
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
                if let Ok(mut snapshots) = app.state::<QuotaState>().0.lock() {
                    snapshots.insert(payload.period.clone(), payload.clone());
                }

                update_native_menu(&app, &payload);
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

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(QuotaState::default())
        .manage(NativeMenuState::default())
        .invoke_handler(tauri::generate_handler![get_platform, get_quota_snapshots])
        .setup(|app| {
            #[cfg(target_os = "macos")]
            app.handle()
                .set_activation_policy(tauri::ActivationPolicy::Accessory)?;

            #[cfg(target_os = "macos")]
            build_native_menu(app)?;

            start_browser_bridge(app.handle().clone());
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
