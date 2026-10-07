//! Codex account client. Resets require an explicit action; never starts agent turns.
use crate::{set_codex_snapshots, QuotaPayload};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    fs,
    io::{BufRead, BufReader, Write},
    path::PathBuf,
    process::{Child, ChildStdin, Command, Stdio},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, Sender},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};
use tauri::{Manager, State};
use tauri_plugin_opener::OpenerExt;

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    pub phase: String,
    pub message: String,
    pub executable: Option<String>,
    pub last_checked: Option<String>,
    pub auth_url: Option<String>,
    pub user_code: Option<String>,
    pub plan_type: Option<String>,
    pub active_instances: Option<usize>,
    pub refresh_seconds: u64,
    pub refresh_settings: RefreshSettings,
    pub available_resets: Option<u64>,
}
impl Default for Status {
    fn default() -> Self {
        Self {
            phase: "starting".into(),
            message: "Connexion à Codex…".into(),
            executable: None,
            last_checked: None,
            auth_url: None,
            user_code: None,
            plan_type: None,
            active_instances: None,
            refresh_seconds: 120,
            refresh_settings: RefreshSettings::default(),
            available_resets: None,
        }
    }
}

#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RefreshSettings {
    pub custom_seconds: Option<u64>,
}
impl RefreshSettings {
    fn seconds(&self, plan: Option<&str>) -> u64 {
        self.custom_seconds.unwrap_or(match plan {
            Some("plus" | "pro" | "team" | "business" | "enterprise" | "edu") => 60,
            _ => 120,
        })
    }
    fn adaptive_seconds(&self, plan: Option<&str>, count: Option<usize>) -> u64 {
        if let Some(seconds) = self.custom_seconds {
            return seconds;
        }
        let base = self.seconds(plan);
        match count {
            Some(0) => 300,
            Some(1) | None => base,
            Some(2..=3) => (base / 2).max(30),
            Some(_) => (base / 4).max(15),
        }
    }
    fn valid(&self) -> bool {
        self.custom_seconds
            .is_none_or(|n| [30, 60, 120, 300, 600].contains(&n))
    }
}
#[tauri::command]
pub fn set_refresh_settings(
    app: tauri::AppHandle,
    provider: State<'_, Provider>,
    settings: RefreshSettings,
) -> Result<(), String> {
    if !settings.valid() {
        return Err("Intervalle non pris en charge.".into());
    }
    let path = app
        .path()
        .app_local_data_dir()
        .map_err(|e| e.to_string())?
        .join("refresh.json");
    fs::create_dir_all(path.parent().unwrap()).map_err(|e| e.to_string())?;
    fs::write(
        path,
        serde_json::to_vec(&settings).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    {
        let mut status = provider.shared.status.lock().unwrap();
        status.refresh_seconds =
            settings.adaptive_seconds(status.plan_type.as_deref(), status.active_instances);
        status.refresh_settings = settings;
    }
    provider
        .tx
        .send(Action::Reschedule)
        .map_err(|_| "Service Codex indisponible.".into())
}

#[derive(Default)]
struct Shared {
    status: Mutex<Status>,
    stop: AtomicBool,
    child: Mutex<Option<Child>>,
}
impl Shared {
    fn status(&self, phase: &str, message: &str) {
        if let Ok(mut status) = self.status.lock() {
            status.phase = phase.into();
            status.message = message.into();
            if phase != "ready" {
                status.available_resets = None;
            }
            if phase != "logging_in" {
                status.auth_url = None;
                status.user_code = None;
            }
        }
    }
    fn kill_child(&self) {
        if let Some(mut child) = self.child.lock().unwrap().take() {
            // Give stdin EOF time to stop the app-server and its owned helpers.
            let deadline = Instant::now() + Duration::from_millis(750);
            while Instant::now() < deadline {
                if matches!(child.try_wait(), Ok(Some(_))) {
                    return;
                }
                std::thread::sleep(Duration::from_millis(25));
            }
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}
enum Action {
    Redeem(String, Sender<Result<RedemptionResult, String>>),
    Refresh,
    Reschedule,
    Login(bool),
    CancelLogin,
    Reconnect,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RedemptionResult {
    outcome: String,
    quotas_refreshed: bool,
}

fn poll_due(now: Instant, next_poll: Instant, reset_pending: bool) -> bool {
    reset_pending || now >= next_poll
}

fn redemption_result(
    outcome: Result<String, String>,
    quotas_refreshed: bool,
) -> Result<RedemptionResult, String> {
    outcome.map(|outcome| RedemptionResult {
        outcome,
        quotas_refreshed,
    })
}
pub struct Provider {
    shared: Arc<Shared>,
    tx: Sender<Action>,
}
impl Provider {
    pub fn status(&self) -> Status {
        self.shared.status.lock().unwrap().clone()
    }
    pub fn refresh(&self) {
        let _ = self.tx.send(Action::Refresh);
    }
    #[cfg(target_os = "macos")]
    pub fn login(&self, device: bool) {
        let _ = self.tx.send(Action::Login(device));
    }
    pub fn stop(&self) {
        self.shared.stop.store(true, Ordering::Release);
        self.shared.kill_child();
    }
}

fn saved_path(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    app.path()
        .app_local_data_dir()
        .map(|p| p.join("codex-path.txt"))
        .map_err(|e| e.to_string())
}
fn discover(override_path: Option<String>) -> Option<PathBuf> {
    if let Some(path) = override_path.filter(|s| !s.trim().is_empty()) {
        let path = PathBuf::from(path.trim());
        return (path.is_absolute() && path.is_file()).then_some(path);
    }
    let mut paths = Vec::new();
    #[cfg(target_os = "windows")]
    {
        if let Some(base) = std::env::var_os("LOCALAPPDATA") {
            paths.push(PathBuf::from(base).join("Programs/OpenAI/Codex/bin/codex.exe"));
        }
        if let Some(home) = std::env::var_os("USERPROFILE") {
            paths.push(PathBuf::from(home).join(".local/bin/codex.exe"));
        }
    }
    #[cfg(not(target_os = "windows"))]
    {
        if let Some(home) = std::env::var_os("HOME") {
            paths.push(PathBuf::from(home).join(".local/bin/codex"));
        }
        paths.extend(
            [
                "/opt/homebrew/bin/codex",
                "/usr/local/bin/codex",
                "/usr/bin/codex",
            ]
            .map(PathBuf::from),
        );
        #[cfg(target_os = "macos")]
        paths.extend(
            [
                "/Applications/Codex.app/Contents/Resources/codex",
                "/Applications/ChatGPT.app/Contents/Resources/codex",
            ]
            .map(PathBuf::from),
        );
    }
    if let Some(search) = std::env::var_os("PATH") {
        let name = if cfg!(target_os = "windows") {
            "codex.exe"
        } else {
            "codex"
        };
        paths.extend(
            std::env::split_paths(&search)
                .filter(|p| p.is_absolute())
                .map(|p| p.join(name)),
        );
    }
    paths.into_iter().find(|p| p.is_file())
}

fn read_protocol_line(reader: &mut impl BufRead) -> std::io::Result<Vec<u8>> {
    let mut line = Vec::new();
    std::io::Read::take(reader, 1_048_577).read_until(b'\n', &mut line)?;
    if line.len() > 1_048_576 {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "Codex message too large",
        ));
    }
    Ok(line)
}

struct Session {
    shared: Arc<Shared>,
    stdin: Option<ChildStdin>,
    messages: Receiver<Value>,
    next_id: u64,
    notices: Vec<Value>,
}
impl Drop for Session {
    fn drop(&mut self) {
        self.stdin.take();
        self.shared.kill_child();
    }
}
impl Session {
    fn start(path: &PathBuf, shared: Arc<Shared>) -> Result<Self, String> {
        let mut command = Command::new(path);
        command
            .arg("app-server")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        // A quota client needs neither a project directory nor project-local config.
        command.current_dir(std::env::temp_dir());
        #[cfg(target_os = "windows")]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x08000000);
        }
        let mut child = command.spawn().map_err(|_| {
            "Impossible de lancer Codex. Vérifie le chemin et les permissions.".to_string()
        })?;
        let stdin = child.stdin.take().unwrap();
        let stdout = child.stdout.take().unwrap();
        *shared.child.lock().unwrap() = Some(child);
        let (tx, messages) = mpsc::sync_channel(128);
        std::thread::spawn(move || {
            let mut reader = BufReader::new(stdout);
            loop {
                let Ok(line) = read_protocol_line(&mut reader) else {
                    break;
                };
                if line.is_empty() {
                    break;
                }
                if let Ok(value) = serde_json::from_slice(&line) {
                    if tx.send(value).is_err() {
                        break;
                    }
                }
            }
        });
        let mut session = Self {
            shared,
            stdin: Some(stdin),
            messages,
            next_id: 0,
            notices: Vec::new(),
        };
        session.call("initialize", json!({"clientInfo":{"name":"quota_codex_monitor","version":env!("CARGO_PKG_VERSION")}}))?;
        session.send(json!({"method":"initialized","params":{}}))?;
        Ok(session)
    }
    fn send(&mut self, message: Value) -> Result<(), String> {
        writeln!(
            self.stdin.as_mut().ok_or("Connexion Codex fermée.")?,
            "{message}"
        )
        .map_err(|_| "La connexion à Codex a été fermée.".into())
    }
    fn call(&mut self, method: &str, params: Value) -> Result<Value, String> {
        self.next_id += 1;
        let id = self.next_id;
        self.send(json!({"id":id,"method":method,"params":params}))?;
        let deadline = Instant::now() + Duration::from_secs(30);
        loop {
            if self.shared.stop.load(Ordering::Acquire) {
                return Err("Arrêt de Codex.".into());
            }
            if Instant::now() >= deadline {
                return Err("Codex ne répond pas. Vérifie ta connexion puis réessaie.".into());
            }
            match self.messages.recv_timeout(Duration::from_millis(100)) {
                Ok(message) if message.get("id").and_then(Value::as_u64) == Some(id) => {
                    if message.get("error").is_some() {
                        return Err("Codex n’a pas pu traiter la demande. Vérifie ta connexion, ton compte ChatGPT et la version de Codex.".into());
                    }
                    return message
                        .get("result")
                        .cloned()
                        .ok_or("Réponse Codex invalide.".into());
                }
                Ok(message) => {
                    if self.notices.len() >= 128 {
                        return Err("Trop de notifications Codex.".into());
                    }
                    self.notices.push(message);
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(_) => return Err("Codex s’est arrêté. Une reconnexion sera tentée.".into()),
            }
        }
    }
    fn drain(&mut self) -> Vec<Value> {
        self.notices.extend(self.messages.try_iter().take(128));
        std::mem::take(&mut self.notices)
    }
}

pub fn normalize(result: &Value) -> Result<Vec<QuotaPayload>, String> {
    let buckets: Vec<(String, &Value)> =
        if let Some(map) = result.get("rateLimitsByLimitId").and_then(Value::as_object) {
            map.iter().map(|(id, v)| (id.clone(), v)).collect()
        } else if let Some(bucket) = result.get("rateLimits").filter(|v| v.is_object()) {
            vec![(
                bucket
                    .get("limitId")
                    .and_then(Value::as_str)
                    .unwrap_or("codex")
                    .into(),
                bucket,
            )]
        } else {
            return Err("Codex n’a renvoyé aucune information de quota exploitable.".into());
        };
    let now = chrono::Utc::now().to_rfc3339();
    let mut snapshots = Vec::new();
    for (id, bucket) in buckets {
        let name = bucket
            .get("limitName")
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
            .unwrap_or(&id);
        for slot in ["primary", "secondary"] {
            let Some(window) = bucket.get(slot).filter(|v| !v.is_null()) else {
                continue;
            };
            let used = window
                .get("usedPercent")
                .and_then(Value::as_f64)
                .filter(|v| v.is_finite())
                .ok_or("Pourcentage Codex invalide.")?;
            let mins = window
                .get("windowDurationMins")
                .and_then(Value::as_i64)
                .filter(|v| *v > 0);
            let duration = match mins {
                Some(300) => "5 heures".into(),
                Some(10080) => "7 jours".into(),
                Some(value) if value % 1440 == 0 => format!("{} jours", value / 1440),
                Some(value) if value % 60 == 0 => format!("{} heures", value / 60),
                Some(value) => format!("{value} min"),
                None => slot.into(),
            };
            let is_reserve = id.to_ascii_lowercase().contains("reserve")
                || id.to_ascii_lowercase().contains("luna")
                || id.to_ascii_lowercase().contains("base_model_inference");
            let display_label = if is_reserve {
                "Réserve".into()
            } else if mins == Some(300) {
                "5h".into()
            } else if mins == Some(10080) {
                "7j".into()
            } else {
                format!("{name} · {duration}")
            };
            let reset = window
                .get("resetsAt")
                .and_then(Value::as_i64)
                .and_then(|s| chrono::DateTime::from_timestamp(s, 0))
                .map(|d| d.to_rfc3339())
                .unwrap_or_default();
            snapshots.push(QuotaPayload {
                model: name.into(),
                remaining: (100.0 - used.clamp(0.0, 100.0)).round() as u32,
                limit: 100,
                reset_at: reset,
                checked_at: now.clone(),
                confidence: "high".into(),
                period: format!("codex:{id}:{slot}"),
                reset_label: None,
                source: "codex".into(),
                label: Some(display_label),
                window_minutes: mins,
            });
        }
    }
    Ok(snapshots)
}

pub fn start(app: tauri::AppHandle) -> Provider {
    let shared = Arc::new(Shared::default());
    if let Ok(dir) = app.path().app_local_data_dir() {
        if let Some(settings) = fs::read(dir.join("refresh.json"))
            .ok()
            .and_then(|bytes| serde_json::from_slice::<RefreshSettings>(&bytes).ok())
            .filter(RefreshSettings::valid)
        {
            let mut status = shared.status.lock().unwrap();
            status.refresh_seconds = settings.seconds(None);
            status.refresh_settings = settings;
        }
    }
    let (tx, rx) = mpsc::channel();
    let worker_shared = shared.clone();
    std::thread::spawn(move || worker(app, worker_shared, rx));
    Provider { shared, tx }
}
fn worker(app: tauri::AppHandle, shared: Arc<Shared>, rx: Receiver<Action>) {
    let mut detector = crate::activity::Detector::new();
    let mut next_activity = Instant::now();
    let mut session: Option<Session> = None;
    let mut next_poll = Instant::now();
    let mut failures = 0_u32;
    let mut login: Option<(String, Instant)> = None;
    while !shared.stop.load(Ordering::Acquire) {
        let mut action = match rx.recv_timeout(Duration::from_millis(250)) {
            Ok(action) => Some(action),
            Err(mpsc::RecvTimeoutError::Timeout) => None,
            Err(_) => break,
        };
        let mut pending_reset = None;
        if matches!(action, Some(Action::Redeem(..))) {
            let Some(Action::Redeem(key, reply)) = action.take() else {
                unreachable!()
            };
            let ready = shared.status.lock().unwrap().phase == "ready";
            if !ready || login.is_some() || session.is_none() {
                let _ = reply.send(Err("Reconnecte Codex avant de réessayer.".into()));
                continue;
            }
            let result = session
                .as_mut()
                .unwrap()
                .call(
                    "account/rateLimitResetCredit/consume",
                    json!({"idempotencyKey":key}),
                )
                .and_then(|value| reset_outcome(&value));
            // Read and publish in this iteration, before acknowledging the reset.
            // Activity changes must not defer this read to the adaptive interval.
            pending_reset = Some((reply, result));
        }
        if Instant::now() >= next_activity {
            let count = detector.count();
            let mut status = shared.status.lock().unwrap();
            let old = status.refresh_seconds;
            status.active_instances = count;
            status.refresh_seconds = status
                .refresh_settings
                .adaptive_seconds(status.plan_type.as_deref(), count);
            if status.refresh_seconds < old {
                next_poll =
                    next_poll.min(Instant::now() + Duration::from_secs(status.refresh_seconds));
            } else if status.refresh_seconds > old && status.phase == "ready" {
                next_poll = Instant::now() + Duration::from_secs(status.refresh_seconds);
            }
            next_activity = Instant::now() + Duration::from_secs(10);
        }
        if matches!(action, Some(Action::Reconnect)) {
            shared.status("starting", "Reconnexion à Codex…");
            set_codex_snapshots(&app, None);
            session = None;
            login = None;
            next_poll = Instant::now();
        }
        if matches!(action, Some(Action::Reschedule)) {
            next_poll =
                Instant::now() + Duration::from_secs(shared.status.lock().unwrap().refresh_seconds);
        }
        if matches!(action, Some(Action::Refresh)) {
            next_poll = Instant::now();
        }
        if session.is_none()
            && (Instant::now() >= next_poll || matches!(action, Some(Action::Login(_))))
        {
            let custom = saved_path(&app)
                .ok()
                .and_then(|p| fs::read_to_string(p).ok())
                .or_else(|| std::env::var("QUOTA_CODEX_BINARY").ok());
            if let Some(path) = discover(custom) {
                shared.status.lock().unwrap().executable =
                    Some(path.to_string_lossy().into_owned());
                match Session::start(&path, shared.clone()) {
                    Ok(value) => {
                        session = Some(value);
                    }
                    Err(error) => {
                        shared.status("error", &error);
                        set_codex_snapshots(&app, None);
                        next_poll = Instant::now() + Duration::from_secs(30);
                        continue;
                    }
                }
            } else {
                shared.status.lock().unwrap().executable = None;
                shared.status("missing", "Installe Codex CLI ou indique son exécutable pour activer la synchronisation automatique.");
                set_codex_snapshots(&app, None);
                next_poll = Instant::now() + Duration::from_secs(15);
                continue;
            }
        }
        let Some(client) = session.as_mut() else {
            continue;
        };
        if let Some(Action::Login(device)) = action {
            if login.is_none() {
                shared.status("logging_in", "Préparation de la connexion ChatGPT…");
                match client.call(
                    "account/login/start",
                    json!({"type":if device {"chatgptDeviceCode"} else {"chatgpt"}}),
                ) {
                    Ok(result) => {
                        if let (Some(id), Some(url)) = (
                            result["loginId"].as_str(),
                            result[if device { "verificationUrl" } else { "authUrl" }].as_str(),
                        ) {
                            if !trusted_login_url(url.as_ref()) {
                                shared.status("error", "Adresse de connexion Codex inattendue.");
                                session = None;
                                continue;
                            }
                            login = Some((id.into(), Instant::now()));
                            {
                                let mut status = shared.status.lock().unwrap();
                                status.auth_url = Some(url.into());
                                status.user_code = result["userCode"].as_str().map(str::to_owned);
                                status.message = "Termine la connexion dans le navigateur. Il pourra ensuite être fermé.".into();
                            }
                            // Only an explicit sign-in action opens a browser.
                            let _ = app.opener().open_url(url, None::<&str>);
                        } else {
                            shared.status(
                                "error",
                                "Réponse de connexion invalide. Mets Codex à jour.",
                            );
                            session = None;
                        }
                    }
                    Err(error) => {
                        shared.status("error", &error);
                        session = None;
                    }
                }
            }
            continue;
        }
        if matches!(action, Some(Action::CancelLogin))
            || login
                .as_ref()
                .is_some_and(|(_, at)| at.elapsed() > Duration::from_secs(600))
        {
            if let Some((id, _)) = login.take() {
                let _ = client.call("account/login/cancel", json!({"loginId":id}));
            }
            shared.status(
                "signed_out",
                "Connexion annulée. Connecte ton compte ChatGPT pour lire les quotas.",
            );
            next_poll = Instant::now();
        }
        for notice in client.drain() {
            match notice["method"].as_str() {
                Some("account/login/completed") => {
                    login = None;
                    if notice["params"]["success"].as_bool() == Some(true) {
                        next_poll = Instant::now();
                    } else {
                        shared.status("signed_out", "La connexion n’a pas abouti. Réessaie.");
                        next_poll = Instant::now() + Duration::from_secs(60);
                    }
                }
                Some("account/updated" | "account/rateLimits/updated") if login.is_none() => {
                    next_poll = Instant::now();
                }
                _ => {}
            }
        }
        if login.is_some() || !poll_due(Instant::now(), next_poll, pending_reset.is_some()) {
            continue;
        }
        let result = client
            .call("account/read", json!({"refreshToken":false}))
            .and_then(|account| {
                let mut status = shared.status.lock().unwrap();
                status.plan_type = account["account"]["planType"]
                    .as_str()
                    .map(|s| s.to_ascii_lowercase());
                status.refresh_seconds = status
                    .refresh_settings
                    .adaptive_seconds(status.plan_type.as_deref(), status.active_instances);
                drop(status);
                match account["account"]["type"].as_str() {
                    Some("chatgpt" | "chatgptAuthTokens") => client
                        .call("account/rateLimits/read", Value::Null)
                        .and_then(|v| {
                            let rows = normalize(&v)?;
                            shared.status.lock().unwrap().available_resets = available_resets(&v);
                            Ok(Some(rows))
                        }),
                    _ => Ok(None),
                }
            });
        let quotas_refreshed = matches!(&result, Ok(Some(_)));
        match result {
            Ok(Some(snapshots)) => {
                let empty = snapshots.is_empty();
                shared.status(
                    "ready",
                    if empty {
                        "Connecté à Codex. Aucune fenêtre de quota n’est disponible pour ce compte."
                    } else {
                        "Quotas synchronisés directement avec Codex."
                    },
                );
                shared.status.lock().unwrap().last_checked = Some(chrono::Utc::now().to_rfc3339());
                set_codex_snapshots(&app, Some(snapshots));
                failures = 0;
                next_poll = Instant::now()
                    + Duration::from_secs(shared.status.lock().unwrap().refresh_seconds);
            }
            Ok(None) => {
                shared.status("signed_out", "Connecte un compte ChatGPT à Codex. Une clé API ne fournit pas les quotas de ton abonnement.");
                set_codex_snapshots(&app, None);
                next_poll = Instant::now() + Duration::from_secs(60);
                // A new session picks up an external CLI login on the next attempt.
                session = None;
            }
            Err(error) => {
                shared.status("error", &error);
                set_codex_snapshots(&app, None);
                failures = (failures + 1).min(4);
                session = None;
                next_poll = Instant::now()
                    + Duration::from_secs(
                        (15 * (1 << failures)).max(shared.status.lock().unwrap().refresh_seconds),
                    );
            }
        }
        if let Some((reply, outcome)) = pending_reset {
            // A confirmed redemption remains confirmed even if the following read
            // fails. The UI must not invite spending a second reset in that case.
            let _ = reply.send(redemption_result(outcome, quotas_refreshed));
        }
    }
}

#[tauri::command]
pub fn get_codex_status(provider: State<'_, Provider>) -> Status {
    provider.status()
}
fn available_resets(value: &Value) -> Option<u64> {
    value["rateLimitResetCredits"]["availableCount"].as_u64()
}
fn reset_outcome(value: &Value) -> Result<String, String> {
    match value["outcome"].as_str() {
        Some(outcome @ ("reset" | "alreadyRedeemed" | "nothingToReset" | "noCredit")) => {
            Ok(outcome.into())
        }
        _ => Err("Réponse inconnue. Vérifie les quotas avant de réessayer.".into()),
    }
}
#[tauri::command]
pub async fn redeem_codex_reset(
    provider: State<'_, Provider>,
    idempotency_key: String,
) -> Result<RedemptionResult, String> {
    // Only UUID-shaped keys are accepted. Retries must retain the original key.
    if idempotency_key.len() != 36
        || !idempotency_key.bytes().enumerate().all(|(i, b)| {
            if [8, 13, 18, 23].contains(&i) {
                b == b'-'
            } else {
                b.is_ascii_hexdigit()
            }
        })
    {
        return Err("Identifiant de réinitialisation invalide.".into());
    }
    let (tx, rx) = mpsc::channel();
    provider
        .tx
        .send(Action::Redeem(idempotency_key, tx))
        .map_err(|_| "Service Codex indisponible.")?;
    tauri::async_runtime::spawn_blocking(move || {
        rx.recv()
            .map_err(|_| "Service Codex interrompu.".to_owned())?
    })
    .await
    .map_err(|_| "Réinitialisation interrompue.".to_owned())?
}
#[tauri::command]
pub fn start_codex_login(provider: State<'_, Provider>, device: bool) -> Result<(), String> {
    provider
        .tx
        .send(Action::Login(device))
        .map_err(|_| "Service Codex indisponible.".into())
}
#[tauri::command]
pub fn cancel_codex_login(provider: State<'_, Provider>) {
    let _ = provider.tx.send(Action::CancelLogin);
}
#[tauri::command]
pub fn set_codex_path(
    app: tauri::AppHandle,
    provider: State<'_, Provider>,
    path: String,
) -> Result<(), String> {
    let file = saved_path(&app)?;
    fs::create_dir_all(file.parent().unwrap()).map_err(|e| e.to_string())?;
    if path.trim().is_empty() {
        if file.exists() {
            fs::remove_file(&file).map_err(|e| e.to_string())?;
        }
    } else {
        let p = PathBuf::from(path.trim());
        if !p.is_absolute() || !p.is_file() {
            return Err("Choisis le chemin absolu d’un exécutable Codex existant.".into());
        }
        #[cfg(target_os = "windows")]
        if p.extension().and_then(|v| v.to_str()) != Some("exe") {
            return Err("Sélectionne codex.exe, pas un script .cmd.".into());
        }
        fs::write(file, path.trim()).map_err(|e| e.to_string())?;
    }
    provider
        .tx
        .send(Action::Reconnect)
        .map_err(|_| "Service Codex indisponible.".into())
}
#[tauri::command]
pub fn open_codex_install(app: tauri::AppHandle) -> Result<(), String> {
    app.opener()
        .open_url("https://developers.openai.com/codex/cli", None::<&str>)
        .map_err(|e| e.to_string())
}
fn trusted_login_url(value: &str) -> bool {
    tauri::Url::parse(value).is_ok_and(|url| {
        url.scheme() == "https"
            && url.host_str() == Some("auth.openai.com")
            && url.username().is_empty()
            && url.password().is_none()
            && url.port_or_known_default() == Some(443)
    })
}

#[tauri::command]
pub fn open_codex_login(
    app: tauri::AppHandle,
    provider: State<'_, Provider>,
) -> Result<(), String> {
    let url = provider
        .status()
        .auth_url
        .ok_or("Aucune connexion en cours.")?;
    if !trusted_login_url(url.as_ref()) {
        return Err("Adresse invalide.".into());
    }
    app.opener()
        .open_url(url, None::<&str>)
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn reset_refresh_bypasses_an_adaptive_poll_delay() {
        let now = Instant::now();
        // Reproduce the activity detector extending the interval to five minutes.
        let next_poll = now + Duration::from_secs(300);
        assert!(!poll_due(now, next_poll, false));
        assert!(poll_due(now, next_poll, true));
        assert!(poll_due(next_poll, next_poll, false));
    }
    #[test]
    fn a_failed_refresh_does_not_undo_a_confirmed_reset() {
        for outcome in ["reset", "alreadyRedeemed", "nothingToReset", "noCredit"] {
            let result = redemption_result(Ok(outcome.into()), false).unwrap();
            assert_eq!(result.outcome, outcome);
            assert!(!result.quotas_refreshed);
        }
        assert!(
            redemption_result(Ok("reset".into()), true)
                .unwrap()
                .quotas_refreshed
        );
        assert!(redemption_result(Err("unknown outcome".into()), true).is_err());
    }
    #[test]
    fn reset_count_preserves_unknown_and_uses_authoritative_count() {
        assert_eq!(available_resets(&json!({})), None);
        assert_eq!(
            available_resets(&json!({"rateLimitResetCredits":null})),
            None
        );
        assert_eq!(
            available_resets(&json!({"rateLimitResetCredits":{"availableCount":0}})),
            Some(0)
        );
        assert_eq!(
            available_resets(&json!({"rateLimitResetCredits":{"availableCount":3,"credits":[]}})),
            Some(3)
        );
        assert_eq!(
            available_resets(&json!({"rateLimitResetCredits":{"availableCount":-1}})),
            None
        );
    }
    #[test]
    fn reset_outcomes_do_not_infer_quota() {
        for outcome in ["reset", "alreadyRedeemed", "nothingToReset", "noCredit"] {
            assert_eq!(reset_outcome(&json!({"outcome":outcome})).unwrap(), outcome);
        }
        assert!(reset_outcome(&json!({})).is_err());
    }
    #[test]
    fn oversized_protocol_messages_are_rejected() {
        assert!(read_protocol_line(&mut std::io::Cursor::new(vec![b'x'; 1_048_577])).is_err());
        assert_eq!(
            read_protocol_line(&mut std::io::Cursor::new(b"{}\n")).unwrap(),
            b"{}\n"
        );
        assert!(read_protocol_line(&mut std::io::Cursor::new(b""))
            .unwrap()
            .is_empty());
    }
    #[test]
    fn login_urls_reject_untrusted_destinations() {
        assert!(trusted_login_url(
            "https://auth.openai.com/authorize?state=test"
        ));
        for url in [
            "http://auth.openai.com/",
            "https://auth.openai.com.evil.test/",
            "https://auth.openai.com@evil.test/",
            "https://auth.openai.com:8443/",
            "file:///tmp/login",
        ] {
            assert!(!trusted_login_url(url));
        }
    }
    #[test]
    fn refresh_presets_and_override() {
        let auto = RefreshSettings::default();
        assert_eq!(auto.adaptive_seconds(Some("plus"), Some(0)), 300);
        assert_eq!(auto.adaptive_seconds(Some("plus"), Some(1)), 60);
        assert_eq!(auto.adaptive_seconds(Some("plus"), Some(2)), 30);
        assert_eq!(auto.adaptive_seconds(Some("plus"), Some(4)), 15);
        assert_eq!(
            RefreshSettings {
                custom_seconds: Some(120)
            }
            .adaptive_seconds(Some("plus"), Some(4)),
            120
        );
        assert_eq!(auto.seconds(Some("plus")), 60);
        assert_eq!(auto.seconds(Some("free")), 120);
        assert_eq!(auto.seconds(None), 120);
        assert_eq!(
            RefreshSettings {
                custom_seconds: Some(300)
            }
            .seconds(Some("plus")),
            300
        );
        assert!(!RefreshSettings {
            custom_seconds: Some(0)
        }
        .valid());
    }
    #[test]
    fn weekly_only_and_multiple_buckets_are_preserved() {
        let data = json!({"rateLimitsByLimitId":{
            "codex":{"primary":null,"secondary":{"usedPercent":59,"windowDurationMins":10080,"resetsAt":1790341067}},
            "other":{"limitName":"Other","primary":{"usedPercent":12,"windowDurationMins":300,"resetsAt":null}}
        }});
        let rows = normalize(&data).unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].remaining, 41);
        assert_eq!(rows[0].period, "codex:codex:secondary");
        assert_eq!(rows[0].label.as_deref(), Some("7j"));
        assert!(rows[1].reset_at.is_empty());
    }
    #[test]
    fn empty_success_is_distinct_from_malformed_data() {
        assert!(
            normalize(&json!({"rateLimits":{"primary":null,"secondary":null}}))
                .unwrap()
                .is_empty()
        );
        assert!(normalize(&json!({"rateLimitsByLimitId":{}}))
            .unwrap()
            .is_empty());
        assert!(normalize(&json!({})).is_err());
        assert!(normalize(&json!({"rateLimits":{"primary":{"usedPercent":"invalid"}}})).is_err());
    }
    #[test]
    fn remaining_percentage_is_clamped() {
        let rows = normalize(
            &json!({"rateLimits":{"primary":{"usedPercent":125},"secondary":{"usedPercent":-1}}}),
        )
        .unwrap();
        assert_eq!(rows[0].remaining, 0);
        assert_eq!(rows[1].remaining, 100);
    }
    #[test]
    #[ignore = "requires installed Codex and an existing ChatGPT login"]
    fn live_codex_account_read() {
        let mut detector = crate::activity::Detector::new();
        let instances = detector.count().expect("local process metadata available");
        println!("Detected {instances} top-level Codex instances using local metadata.");
        let path = discover(std::env::var("QUOTA_CODEX_BINARY").ok()).expect("Codex executable");
        let mut session = Session::start(&path, Arc::new(Shared::default())).unwrap();
        let account = session
            .call("account/read", json!({"refreshToken":false}))
            .unwrap();
        assert_eq!(account["account"]["type"].as_str(), Some("chatgpt"));
        let result = session
            .call("account/rateLimits/read", Value::Null)
            .unwrap();
        let rows = normalize(&result).unwrap();
        println!(
            "Available earned resets reported by Codex: {:?}",
            available_resets(&result)
        );
        println!(
            "Validated {} quota windows; no account or credential fields printed.",
            rows.len()
        );
    }
}
