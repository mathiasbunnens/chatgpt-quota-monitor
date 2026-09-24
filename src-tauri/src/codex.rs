//! Read-only Codex account client. Never creates threads or starts agent turns.
use crate::{set_codex_snapshots, BrowserQuotaPayload};
use serde::Serialize;
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
        }
    }
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
    Refresh,
    Login(bool),
    CancelLogin,
    Reconnect,
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
        paths.push(PathBuf::from(
            "/Applications/Codex.app/Contents/Resources/codex",
        ));
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
        let (tx, messages) = mpsc::channel();
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines() {
                let Ok(line) = line else {
                    break;
                };
                if let Ok(value) = serde_json::from_str(&line) {
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
                Ok(message) => self.notices.push(message),
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(_) => return Err("Codex s’est arrêté. Une reconnexion sera tentée.".into()),
            }
        }
    }
    fn drain(&mut self) -> Vec<Value> {
        self.notices.extend(self.messages.try_iter());
        std::mem::take(&mut self.notices)
    }
}

pub fn normalize(result: &Value) -> Result<Vec<BrowserQuotaPayload>, String> {
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
            let reset = window
                .get("resetsAt")
                .and_then(Value::as_i64)
                .and_then(|s| chrono::DateTime::from_timestamp(s, 0))
                .map(|d| d.to_rfc3339())
                .unwrap_or_default();
            snapshots.push(BrowserQuotaPayload {
                model: name.into(),
                remaining: (100.0 - used.clamp(0.0, 100.0)).round() as u32,
                limit: 100,
                reset_at: reset,
                checked_at: now.clone(),
                confidence: "high".into(),
                period: format!("codex:{id}:{slot}"),
                reset_label: None,
                source: "codex".into(),
                label: Some(format!("{name} · {duration}")),
            });
        }
    }
    Ok(snapshots)
}

pub fn start(app: tauri::AppHandle) -> Provider {
    let shared = Arc::new(Shared::default());
    let (tx, rx) = mpsc::channel();
    let worker_shared = shared.clone();
    std::thread::spawn(move || worker(app, worker_shared, rx));
    Provider { shared, tx }
}
fn worker(app: tauri::AppHandle, shared: Arc<Shared>, rx: Receiver<Action>) {
    let mut session: Option<Session> = None;
    let mut next_poll = Instant::now();
    let mut failures = 0_u32;
    let mut login: Option<(String, Instant)> = None;
    while !shared.stop.load(Ordering::Acquire) {
        let action = match rx.recv_timeout(Duration::from_millis(250)) {
            Ok(action) => Some(action),
            Err(mpsc::RecvTimeoutError::Timeout) => None,
            Err(_) => break,
        };
        if matches!(action, Some(Action::Reconnect)) {
            set_codex_snapshots(&app, None);
            shared.status("starting", "Reconnexion à Codex…");
            session = None;
            login = None;
            next_poll = Instant::now();
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
                            if !url.starts_with("https://auth.openai.com/") {
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
        if login.is_some() || Instant::now() < next_poll {
            continue;
        }
        let result = client
            .call("account/read", json!({"refreshToken":false}))
            .and_then(|account| match account["account"]["type"].as_str() {
                Some("chatgpt" | "chatgptAuthTokens") => client
                    .call("account/rateLimits/read", Value::Null)
                    .and_then(|v| normalize(&v).map(Some)),
                _ => Ok(None),
            });
        match result {
            Ok(Some(snapshots)) => {
                let empty = snapshots.is_empty();
                set_codex_snapshots(&app, Some(snapshots));
                shared.status(
                    "ready",
                    if empty {
                        "Connecté à Codex. Aucune fenêtre de quota n’est disponible pour ce compte."
                    } else {
                        "Quotas synchronisés directement avec Codex."
                    },
                );
                shared.status.lock().unwrap().last_checked = Some(chrono::Utc::now().to_rfc3339());
                failures = 0;
                next_poll = Instant::now() + Duration::from_secs(60);
            }
            Ok(None) => {
                set_codex_snapshots(&app, None);
                shared.status("signed_out", "Connecte un compte ChatGPT à Codex. Une clé API ne fournit pas les quotas de ton abonnement.");
                next_poll = Instant::now() + Duration::from_secs(60);
                // A new session picks up an external CLI login on the next attempt.
                session = None;
            }
            Err(error) => {
                set_codex_snapshots(&app, None);
                shared.status("error", &error);
                failures = (failures + 1).min(4);
                session = None;
                next_poll = Instant::now() + Duration::from_secs(15 * (1 << failures));
            }
        }
    }
}

#[tauri::command]
pub fn get_codex_status(provider: State<'_, Provider>) -> Status {
    provider.status()
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
#[tauri::command]
pub fn open_codex_login(
    app: tauri::AppHandle,
    provider: State<'_, Provider>,
) -> Result<(), String> {
    let url = provider
        .status()
        .auth_url
        .ok_or("Aucune connexion en cours.")?;
    if !url.starts_with("https://auth.openai.com/") {
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
    fn weekly_only_and_multiple_buckets_are_preserved() {
        let data = json!({"rateLimitsByLimitId":{
            "codex":{"primary":null,"secondary":{"usedPercent":59,"windowDurationMins":10080,"resetsAt":1790341067}},
            "other":{"limitName":"Other","primary":{"usedPercent":12,"windowDurationMins":300,"resetsAt":null}}
        }});
        let rows = normalize(&data).unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].remaining, 41);
        assert_eq!(rows[0].period, "codex:codex:secondary");
        assert!(rows[0].label.as_ref().unwrap().contains("7 jours"));
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
            "Validated {} quota windows; no account or credential fields printed.",
            rows.len()
        );
    }
}
