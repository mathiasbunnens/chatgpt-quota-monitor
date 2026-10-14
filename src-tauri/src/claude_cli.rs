//! Structured Claude Code control protocol. No user message/model request is sent.
use serde_json::{json, Value};
use std::{
    io::{BufRead, BufReader, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::mpsc,
    time::Duration,
};

pub fn discover() -> Option<PathBuf> {
    let mut paths = Vec::new();
    if let Some(path) = std::env::var_os("QUOTA_CLAUDE_BINARY") {
        paths.push(PathBuf::from(path));
    }
    if let Some(home) = std::env::var_os("HOME") {
        paths.push(PathBuf::from(home).join(".local/bin/claude"));
    }
    if let Some(path) = std::env::var_os("PATH") {
        paths.extend(std::env::split_paths(&path).map(|p| p.join("claude")));
    }
    paths.extend(["/opt/homebrew/bin/claude", "/usr/local/bin/claude"].map(PathBuf::from));
    paths.into_iter().find(|p| p.is_file())
}

pub fn read_usage(executable: &Path, cwd: &Path) -> Result<Value, &'static str> {
    std::fs::create_dir_all(cwd).map_err(|_| "workspace")?;
    let mut child = Command::new(executable)
        .args([
            "--safe-mode",
            "--strict-mcp-config",
            "--tools",
            "",
            "--input-format",
            "stream-json",
            "--output-format",
            "stream-json",
            "--verbose",
            "-p",
        ])
        .current_dir(cwd)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|_| "launch")?;
    let stdout = child.stdout.take().ok_or("launch")?;
    let (tx, rx) = mpsc::channel();
    let reader = std::thread::spawn(move || {
        // Bound each line; never save/log the initialize response (account identity).
        let mut reader = BufReader::new(stdout);
        loop {
            let mut line = Vec::new();
            use std::io::Read;
            let count = reader.by_ref().take(1_048_577).read_until(b'\n', &mut line);
            match count {
                Ok(0) | Err(_) => break,
                _ => {}
            }
            if line.len() > 1_048_576 {
                let _ = tx.send(Err("malformed"));
                break;
            }
            if let Ok(value) = serde_json::from_slice::<Value>(&line) {
                if value["type"] == "control_response" && tx.send(Ok(value)).is_err() {
                    break;
                }
            }
        }
    });
    let result = (|| {
        let input = child.stdin.as_mut().ok_or("launch")?;
        for (id, subtype) in [("init", "initialize"), ("usage", "get_usage")] {
            let request =
                json!({"type":"control_request", "request_id":id, "request":{"subtype":subtype}});
            writeln!(input, "{request}").map_err(|_| "transport")?;
            input.flush().map_err(|_| "transport")?;
            let deadline = std::time::Instant::now() + Duration::from_secs(15);
            loop {
                let value = rx
                    .recv_timeout(deadline.saturating_duration_since(std::time::Instant::now()))
                    .map_err(|_| "timeout")??;
                let response = &value["response"];
                if response["request_id"] != id {
                    continue;
                }
                if response["subtype"] != "success" {
                    return Err("unsupported_or_auth");
                }
                if id == "usage" {
                    return Ok(response["response"].clone());
                }
                break;
            }
        }
        Err("malformed")
    })();
    drop(child.stdin.take());
    for _ in 0..20 {
        if matches!(child.try_wait(), Ok(Some(_))) {
            break;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    if matches!(child.try_wait(), Ok(None)) {
        let _ = child.kill();
    }
    let _ = child.wait();
    drop(rx);
    let _ = reader.join();
    result
}

pub fn window(value: &Value) -> Option<crate::claude::Window> {
    let used = value["utilization"].as_f64()?;
    if !used.is_finite() || !(0.0..=100.0).contains(&used) {
        return None;
    }
    let reset = value["resets_at"]
        .as_str()
        .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
        .map(|d| d.timestamp());
    Some(crate::claude::Window {
        used_percent: used,
        resets_at: reset,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn structured_windows_preserve_zero_absence_and_reset() {
        assert_eq!(
            window(&json!({"utilization":0,"resets_at":"2026-10-21T14:00:00Z"}))
                .unwrap()
                .used_percent,
            0.0
        );
        assert!(window(&Value::Null).is_none());
        assert!(window(&json!({"utilization":101})).is_none());
        assert!(
            window(&json!({"utilization":14,"resets_at":"2026-10-21T14:00:00Z"}))
                .unwrap()
                .resets_at
                .is_some()
        );
    }
    #[cfg(unix)]
    #[test]
    fn transport_accepts_structured_reply_and_reaps_child() {
        use std::os::unix::fs::PermissionsExt;
        let dir = std::env::temp_dir().join(format!("quota-cli-fixture-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let exe = dir.join("fake-cli");
        std::fs::write(&exe, "#!/bin/sh\nread first\nprintf '%s\\n' '{\"type\":\"control_response\",\"response\":{\"request_id\":\"init\",\"subtype\":\"success\",\"response\":{}}}'\nread second\nprintf '%s\\n' '{\"type\":\"control_response\",\"response\":{\"request_id\":\"usage\",\"subtype\":\"success\",\"response\":{\"rate_limits_available\":true,\"rate_limits\":{\"five_hour\":{\"utilization\":42}}}}}'\ncat >/dev/null\n").unwrap();
        std::fs::set_permissions(&exe, std::fs::Permissions::from_mode(0o700)).unwrap();
        let value = read_usage(&exe, &dir).unwrap();
        assert_eq!(value["rate_limits"]["five_hour"]["utilization"], 42);
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    #[ignore = "Explicit opt-in real official CLI, no inference"]
    fn live_official_cli() {
        let exe = discover().expect("Claude Code installed");
        let root = std::env::temp_dir().join("quota-monitor-cli-live-test");
        let value = read_usage(&exe, &root).expect("get_usage");
        assert!(value["rate_limits_available"].as_bool().is_some());
        assert!(window(&value["rate_limits"]["five_hour"]).is_some());
    }
}
