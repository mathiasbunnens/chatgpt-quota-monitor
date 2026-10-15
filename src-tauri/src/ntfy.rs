//! Opt-in, best-effort ntfy delivery. A single bounded worker owns the persistent
//! attempt ledger; quota collection never waits for disk or network delivery.
use crate::{claude, codex, prefs, QuotaState};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, fs, path::Path, sync::mpsc, time::Duration};
use tauri::Manager;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Config {
    pub enabled: bool,
    pub server_url: String,
    pub topic: String,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            enabled: false,
            server_url: "https://ntfy.sh".into(),
            topic: String::new(),
        }
    }
}
impl Config {
    pub fn validate(&self) -> Result<(), String> {
        if !self.enabled && self.topic.is_empty() {
            return Ok(());
        }
        self.endpoint().map(|_| ())
    }
    fn endpoint(&self) -> Result<reqwest::Url, String> {
        let mut url =
            reqwest::Url::parse(&self.server_url).map_err(|_| "URL ntfy invalide.".to_string())?;
        if !matches!(url.scheme(), "http" | "https")
            || url.host_str().is_none()
            || !url.username().is_empty()
            || url.password().is_some()
            || url.query().is_some()
            || url.fragment().is_some()
        {
            return Err(
                "Utilisez une URL HTTP(S) sans identifiants, paramètres ni fragment.".into(),
            );
        }
        if self.topic.is_empty()
            || self.topic.len() > 128
            || !self
                .topic
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
        {
            return Err(
                "Topic ntfy requis : 1 à 128 lettres, chiffres, tirets ou underscores.".into(),
            );
        }
        url.path_segments_mut()
            .map_err(|_| "URL ntfy invalide.")?
            .pop_if_empty()
            .push(&self.topic);
        Ok(url)
    }
}

fn send(config: &Config, message: &str) -> Result<(), String> {
    let endpoint = config.endpoint()?;
    let client = reqwest::blocking::Client::builder()
        .connect_timeout(Duration::from_secs(3))
        .timeout(Duration::from_secs(8))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|_| "Client ntfy indisponible.".to_string())?;
    // Never include server responses/URLs in errors: the topic is a bearer secret.
    let response = client
        .post(endpoint)
        .header("Title", "Quota Monitor")
        .header("Content-Type", "text/plain; charset=utf-8")
        .body(message.to_owned())
        .send()
        .map_err(|_| "Envoi ntfy impossible (réseau ou délai dépassé).".to_string())?;
    if !response.status().is_success() {
        return Err(format!(
            "Serveur ntfy : HTTP {}.",
            response.status().as_u16()
        ));
    }
    Ok(())
}

#[tauri::command]
pub async fn test_ntfy(config: Config) -> Result<(), String> {
    // Explicit test is allowed with automatic notifications disabled.
    tauri::async_runtime::spawn_blocking(move || {
        send(
            &config,
            "Test Quota Monitor : les notifications de quota sont configurées.",
        )
    })
    .await
    .map_err(|_| "Test ntfy interrompu.".to_string())?
}

struct Signal(mpsc::SyncSender<()>);
pub fn wake(app: &tauri::AppHandle) {
    if let Some(signal) = app.try_state::<Signal>() {
        let _ = signal.0.try_send(());
    }
}

#[derive(Clone, Debug)]
struct Observation {
    key: String,
    provider: &'static str,
    window: &'static str,
    used: Option<f64>,
    checked: Option<i64>,
    reset: Option<i64>,
    valid: bool,
}
fn timestamp(value: &str) -> Option<i64> {
    chrono::DateTime::parse_from_rfc3339(value)
        .ok()
        .map(|d| d.timestamp())
}
fn observations(app: &tauri::AppHandle, p: &prefs::Prefs) -> Vec<Observation> {
    let mut out = Vec::new();
    if p.codex_enabled {
        let ready = app
            .try_state::<codex::Provider>()
            .is_some_and(|s| s.status().phase == "ready");
        if let Ok(rows) = app.state::<QuotaState>().0.lock() {
            for row in rows.iter() {
                out.push(Observation {
                    key: format!("Codex:{}", row.period),
                    provider: "Codex",
                    window: if crate::is_reserve(row) {
                        "réserve"
                    } else {
                        match row.window_minutes {
                            Some(300) => "5 heures",
                            Some(10080) => "7 jours",
                            _ => "autre limite",
                        }
                    },
                    used: row.used_percent,
                    checked: timestamp(&row.checked_at),
                    reset: timestamp(&row.reset_at),
                    valid: ready && row.confidence == "high" && row.source == "codex",
                });
            }
        }
    }
    if p.claude_enabled {
        out.extend(claude_observations(&claude::snapshot(app)));
    }
    out
}

fn claude_observations(snap: &claude::ClaudeSnapshot) -> Vec<Observation> {
    snap.metrics
        .iter()
        .map(|metric| Observation {
            key: format!("Claude:{}", metric.id),
            provider: "Claude",
            window: if metric.period_duration_seconds == 18000 {
                "5 heures"
            } else {
                "7 jours"
            },
            used: metric.used_percent,
            checked: snap.observed_at.as_deref().and_then(timestamp),
            reset: metric.reset_at.as_deref().and_then(timestamp),
            valid: snap.enabled && snap.state == "ready" && metric.availability == "available",
        })
        .collect()
}

// Each entry means an attempt was reserved durably, not a delivery receipt.
// Persist before sending: an interrupted process must not repeatedly notify.
#[derive(Default, Serialize, Deserialize)]
struct Ledger(BTreeMap<String, Option<i64>>);
impl Ledger {
    fn observe(&mut self, enabled: bool, o: &Observation, now: i64) -> Option<String> {
        if !enabled
            || !o.valid
            || !o.checked.is_some_and(|at| (0..=120).contains(&(now - at)))
            || o.reset.is_some_and(|at| at <= now)
        {
            return None;
        }
        let used = o.used.filter(|v| v.is_finite() && *v >= 0.0)?;
        if used < 100.0 {
            self.0.remove(&o.key);
            return None;
        }
        if let Some(previous) = self.0.get_mut(&o.key) {
            // Missing reset metadata is not evidence of a new cycle. Enrich a
            // previously unknown date without notifying the same exhaustion twice.
            let reset_elapsed =
                previous.is_some_and(|at| o.checked.is_some_and(|checked| checked >= at));
            if !reset_elapsed && (*previous == o.reset || previous.is_none() || o.reset.is_none()) {
                if previous.is_none() {
                    *previous = o.reset;
                }
                return None;
            }
        }
        self.0.insert(o.key.clone(), o.reset);
        let reset = o
            .reset
            .and_then(|at| chrono::DateTime::from_timestamp(at, 0))
            .map(|at| format!(" Réinitialisation : {} UTC.", at.format("%d/%m/%Y %H:%M")))
            .unwrap_or_default();
        Some(format!(
            "{} · {} : 100 % consommé (0 % restant).{}",
            o.provider, o.window, reset
        ))
    }
    fn save(&self, path: &Path) -> Result<(), String> {
        let tmp = path.with_extension("json.tmp");
        fs::write(
            &tmp,
            serde_json::to_vec(self).map_err(|_| "État ntfy invalide.")?,
        )
        .and_then(|_| fs::rename(tmp, path))
        .map_err(|_| "État ntfy non enregistrable.".into())
    }
}

pub fn start(app: tauri::AppHandle) {
    let (tx, rx) = mpsc::sync_channel(1);
    app.manage(Signal(tx));
    std::thread::spawn(move || {
        let Ok(dir) = app.path().app_local_data_dir() else {
            return;
        };
        if fs::create_dir_all(&dir).is_err() {
            return;
        }
        let path = dir.join("ntfy-attempts.json");
        let mut ledger = match fs::read(&path) {
            Ok(bytes) => match serde_json::from_slice::<Ledger>(&bytes) {
                Ok(l) => l,
                Err(_) => {
                    eprintln!("Notifications ntfy suspendues : état local invalide.");
                    return;
                }
            },
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ledger::default(),
            Err(_) => {
                eprintln!("Notifications ntfy suspendues : état local illisible.");
                return;
            }
        };
        while rx.recv().is_ok() {
            let p = prefs::current(&app);
            if !p.ntfy.enabled || p.ntfy.endpoint().is_err() {
                continue;
            }
            for o in observations(&app, &p) {
                if prefs::current(&app) != p {
                    break;
                }
                let before = ledger.0.clone();
                let message = ledger.observe(true, &o, chrono::Utc::now().timestamp());
                if before != ledger.0 && ledger.save(&path).is_err() {
                    eprintln!("Notifications ntfy suspendues : enregistrement impossible.");
                    return;
                }
                if let Some(message) = message {
                    // A queued observation may outlive a settings edit.
                    if prefs::current(&app).ntfy == p.ntfy {
                        if let Err(error) = send(&p.ntfy, &message) {
                            eprintln!("{error}");
                        }
                    }
                }
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    fn sample(used: f64) -> Observation {
        Observation {
            key: "Codex:primary".into(),
            provider: "Codex",
            window: "5 heures",
            used: Some(used),
            checked: Some(1000),
            reset: Some(2000),
            valid: true,
        }
    }
    #[test]
    fn exact_threshold_direction_duplicates_restart_and_rearm() {
        let mut l = Ledger::default();
        for used in [0.0, 50.0, 99.6, 99.99] {
            assert!(l.observe(true, &sample(used), 1000).is_none());
        }
        assert!(l.observe(true, &sample(100.0), 1000).is_some());
        let bytes = serde_json::to_vec(&l).unwrap();
        let mut l: Ledger = serde_json::from_slice(&bytes).unwrap();
        assert!(l.observe(true, &sample(100.0), 1000).is_none());
        assert!(l.observe(true, &sample(99.9), 1000).is_none());
        assert!(l.observe(true, &sample(100.0), 1000).is_some());
        let mut next = sample(100.0);
        next.reset = Some(3000);
        assert!(l.observe(true, &next, 1000).is_some());
        next.key = "Claude:five_hour".into();
        next.provider = "Claude";
        assert!(l.observe(true, &next, 1000).unwrap().contains("Claude"));
        next.reset = None;
        assert!(l.observe(true, &next, 1000).is_none());
        next.key = "Claude:seven_day".into();
        assert!(l.observe(true, &next, 1000).is_some());
        next.reset = Some(3000);
        assert!(l.observe(true, &next, 1000).is_none());
    }
    #[test]
    fn invalid_disabled_stale_and_errors_never_send_or_rearm() {
        let mut l = Ledger::default();
        assert!(l.observe(false, &sample(100.0), 1000).is_none());
        for used in [f64::NAN, f64::INFINITY, -1.0] {
            assert!(l.observe(true, &sample(used), 1000).is_none());
        }
        let mut invalid = sample(100.0);
        invalid.valid = false;
        assert!(l.observe(true, &invalid, 1000).is_none());
        let mut o = sample(100.0);
        for checked in [None, Some(0), Some(1001)] {
            o.checked = checked;
            assert!(l.observe(true, &o, 1000).is_none());
        }
        o = sample(100.0);
        o.reset = Some(999);
        assert!(l.observe(true, &o, 1000).is_none());
        o = sample(100.0);
        o.used = None;
        assert!(l.observe(true, &o, 1000).is_none());
        assert!(l.0.is_empty());
        assert!(l.observe(true, &sample(100.0), 1000).is_some());
        invalid.used = Some(0.0);
        assert!(l.observe(true, &invalid, 1000).is_none());
        assert!(l.observe(true, &sample(100.0), 1000).is_none());
    }
    #[test]
    fn codex_raw_percentage_not_rounded_display_drives_threshold() {
        for (used, expected) in [(0.0, false), (99.6, false), (100.0, true)] {
            let rows = codex::normalize(&serde_json::json!({"rateLimits":{
                "primary":{"usedPercent":used,"windowDurationMins":300}
            }}))
            .unwrap();
            assert_eq!(rows[0].used_percent, Some(used));
            let mut o = sample(used);
            o.used = rows[0].used_percent;
            assert_eq!(
                Ledger::default().observe(true, &o, 1000).is_some(),
                expected
            );
        }
    }
    #[test]
    fn ledger_is_persisted_and_reloaded_from_disk() {
        let dir = std::env::temp_dir().join(format!("ntfy-ledger-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("attempts.json");
        let mut ledger = Ledger::default();
        assert!(ledger.observe(true, &sample(100.0), 1000).is_some());
        ledger.save(&path).unwrap();
        let mut restored: Ledger = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        assert!(restored.observe(true, &sample(100.0), 1000).is_none());
        assert!(restored.observe(true, &sample(0.0), 1000).is_none());
        restored.save(&path).unwrap();
        let mut restored: Ledger = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        assert!(restored.observe(true, &sample(100.0), 1000).is_some());
        assert!(restored.save(&dir.join("missing/attempts.json")).is_err());
        fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn config_defaults_validation_and_roundtrip() {
        let default = Config::default();
        assert!(!default.enabled);
        assert_eq!(default.server_url, "https://ntfy.sh");
        for url in [
            "ftp://localhost",
            "https://user:pass@localhost",
            "https://localhost?q=1",
            "https://localhost/#x",
        ] {
            assert!(Config {
                server_url: url.into(),
                topic: "safe".into(),
                enabled: true
            }
            .endpoint()
            .is_err());
        }
        for topic in ["", "../x", "a/b", "x?y", "a b"] {
            assert!(Config {
                topic: topic.into(),
                ..default.clone()
            }
            .endpoint()
            .is_err());
        }
        let c = Config {
            enabled: true,
            server_url: "http://localhost/base/".into(),
            topic: "random-123".into(),
        };
        assert_eq!(
            c.endpoint().unwrap().as_str(),
            "http://localhost/base/random-123"
        );
        assert_eq!(
            serde_json::from_slice::<Config>(&serde_json::to_vec(&c).unwrap()).unwrap(),
            c
        );
    }
    fn local_post(status: u16) -> (Result<(), String>, String) {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let config = Config {
            server_url: format!("http://{}/base", listener.local_addr().unwrap()),
            topic: "local-test-only".into(),
            enabled: true,
        };
        let server = std::thread::spawn(move || {
            let (mut socket, _) = listener.accept().unwrap();
            socket
                .set_read_timeout(Some(Duration::from_secs(3)))
                .unwrap();
            let mut request = Vec::new();
            let mut buf = [0; 4096];
            loop {
                let n = socket.read(&mut buf).unwrap();
                assert!(n > 0);
                request.extend_from_slice(&buf[..n]);
                if let Some(end) = request.windows(4).position(|b| b == b"\r\n\r\n") {
                    let headers = String::from_utf8_lossy(&request[..end]).to_lowercase();
                    let len: usize = headers
                        .lines()
                        .find_map(|s| s.strip_prefix("content-length: "))
                        .unwrap()
                        .parse()
                        .unwrap();
                    if request.len() >= end + 4 + len {
                        break;
                    }
                }
            }
            write!(
                socket,
                "HTTP/1.1 {status} Test\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
            )
            .unwrap();
            String::from_utf8(request).unwrap()
        });
        let message = Ledger::default()
            .observe(true, &sample(100.0), 1000)
            .unwrap();
        let result = send(&config, &message);
        (result, server.join().unwrap())
    }
    #[test]
    fn real_local_http_post_endpoint_minimal_payload_and_status_errors() {
        let (result, request) = local_post(200);
        result.unwrap();
        assert!(request.starts_with("POST /base/local-test-only HTTP/1.1"));
        assert!(request.contains("Quota Monitor"));
        assert!(request.contains("Codex · 5 heures : 100 % consommé (0 % restant)."));
        assert!(request.contains("UTC."));
        assert!(!request.contains("usedPercent"));
        assert!(local_post(403).0.unwrap_err().contains("403"));
        assert!(local_post(302).0.is_err());
    }
    #[test]
    fn network_failure_is_bounded_and_attempt_is_not_repeated() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        drop(listener);
        let c = Config {
            server_url: url,
            topic: "local-test-only".into(),
            enabled: true,
        };
        let mut l = Ledger::default();
        let message = l.observe(true, &sample(100.0), 1000).unwrap();
        assert!(send(&c, &message).is_err());
        assert!(l.observe(true, &sample(100.0), 1000).is_none());
    }

    #[test]
    fn claude_snapshot_conversion_filters_errors_stale_and_unknown() {
        let data = claude::RelayFile {
            schema_version: 1,
            received_at: 1000,
            five_hour: Some(claude::Window {
                used_percent: 100.0,
                resets_at: Some(2000),
            }),
            seven_day: None,
        };
        let mut snap = claude::compute(true, true, claude::FileState::Data(data), 1000);
        let observations = claude_observations(&snap);
        assert!(Ledger::default()
            .observe(true, &observations[0], 1000)
            .unwrap()
            .contains("Claude · 5 heures"));
        assert!(Ledger::default()
            .observe(true, &observations[1], 1000)
            .is_none());
        for state in ["error", "stale", "waiting_for_data", "disabled"] {
            snap.state = state;
            assert!(Ledger::default()
                .observe(true, &claude_observations(&snap)[0], 1000)
                .is_none());
        }
    }

    #[test]
    fn elapsed_reset_rearms_even_when_next_reset_is_unknown() {
        let mut l = Ledger::default();
        assert!(l.observe(true, &sample(100.0), 1000).is_some());
        let mut next = sample(100.0);
        next.checked = Some(2100);
        next.reset = None;
        assert!(l.observe(true, &next, 2100).is_some());
        assert!(l.observe(true, &next, 2100).is_none());
    }

    #[test]
    fn unresponsive_local_server_times_out() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let c = Config {
            server_url: format!("http://{}", listener.local_addr().unwrap()),
            topic: "local-timeout-only".into(),
            enabled: true,
        };
        let server = std::thread::spawn(move || {
            let (mut socket, _) = listener.accept().unwrap();
            socket
                .set_read_timeout(Some(Duration::from_secs(12)))
                .unwrap();
            let mut bytes = [0; 4096];
            while matches!(socket.read(&mut bytes), Ok(n) if n > 0) {}
        });
        let started = std::time::Instant::now();
        assert!(send(&c, "test local timeout").is_err());
        assert!(started.elapsed() < Duration::from_secs(11));
        server.join().unwrap();
    }
}
