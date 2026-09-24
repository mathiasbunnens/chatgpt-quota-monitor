use serde::Serialize;
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};
use tauri::Manager;
use tauri_plugin_opener::OpenerExt;

const USAGE_URL: &str = "https://chatgpt.com/codex/cloud/settings/analytics#usage";
const EXTENSION_FILES: &[(&str, &[u8])] = &[
    (
        "manifest.json",
        include_bytes!("../../extension/manifest.json"),
    ),
    (
        "service-worker.js",
        include_bytes!("../../extension/service-worker.js"),
    ),
    ("content.js", include_bytes!("../../extension/content.js")),
];

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Browser {
    id: &'static str,
    name: &'static str,
    extensions_url: &'static str,
    #[serde(skip)]
    executable: PathBuf,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SetupInfo {
    extension_path: String,
    browsers: Vec<Browser>,
    selected_browser: Option<String>,
}

fn browser_specs() -> [(&'static str, &'static str, &'static str); 4] {
    [
        ("edge", "Microsoft Edge", "edge://extensions/"),
        ("chrome", "Google Chrome", "chrome://extensions/"),
        ("brave", "Brave", "brave://extensions/"),
        ("chromium", "Chromium", "chrome://extensions/"),
    ]
}

fn browser_executable(id: &str) -> Option<PathBuf> {
    #[cfg(target_os = "windows")]
    {
        let relative = match id {
            "edge" => "Microsoft/Edge/Application/msedge.exe",
            "chrome" => "Google/Chrome/Application/chrome.exe",
            "brave" => "BraveSoftware/Brave-Browser/Application/brave.exe",
            "chromium" => "Chromium/Application/chrome.exe",
            _ => return None,
        };
        for key in ["LOCALAPPDATA", "ProgramFiles", "ProgramFiles(x86)"] {
            if let Some(base) = std::env::var_os(key) {
                let path = PathBuf::from(base).join(relative);
                if path.is_file() {
                    return Some(path);
                }
            }
        }
    }
    #[cfg(target_os = "macos")]
    {
        let name = match id {
            "edge" => "Microsoft Edge",
            "chrome" => "Google Chrome",
            "brave" => "Brave Browser",
            "chromium" => "Chromium",
            _ => return None,
        };
        let path = PathBuf::from(format!("/Applications/{name}.app"));
        return path.is_dir().then_some(path);
    }
    #[cfg(not(target_os = "macos"))]
    {
        let names: &[&str] = match id {
            "edge" => &["microsoft-edge", "microsoft-edge-stable", "msedge.exe"],
            "chrome" => &["google-chrome", "google-chrome-stable", "chrome.exe"],
            "brave" => &["brave-browser", "brave", "brave.exe"],
            "chromium" => &["chromium", "chromium-browser"],
            _ => return None,
        };
        if let Some(paths) = std::env::var_os("PATH") {
            for base in std::env::split_paths(&paths) {
                for name in names {
                    let path = base.join(name);
                    if path.is_file() {
                        return Some(path);
                    }
                }
            }
        }
        None
    }
}

fn browsers() -> Vec<Browser> {
    browser_specs()
        .into_iter()
        .filter_map(|(id, name, extensions_url)| {
            browser_executable(id).map(|executable| Browser {
                id,
                name,
                extensions_url,
                executable,
            })
        })
        .collect()
}

fn write_extension(directory: &Path) -> Result<(), String> {
    fs::create_dir_all(directory).map_err(|error| error.to_string())?;
    for (name, bytes) in EXTENSION_FILES {
        let path = directory.join(name);
        // A stable, per-user location survives installer and AppImage upgrades.
        if fs::read(&path).ok().as_deref() != Some(*bytes) {
            fs::write(path, bytes).map_err(|error| error.to_string())?;
        }
    }
    Ok(())
}

fn data_dir(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    app.path()
        .app_local_data_dir()
        .map_err(|error| error.to_string())
}

pub fn prepare_extension(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    let directory = data_dir(app)?.join("browser-extension");
    write_extension(&directory)?;
    Ok(directory)
}

#[tauri::command]
pub fn get_browser_setup(app: tauri::AppHandle) -> Result<SetupInfo, String> {
    let directory = prepare_extension(&app)?;
    let selected_browser = fs::read_to_string(data_dir(&app)?.join("browser.txt")).ok();
    Ok(SetupInfo {
        extension_path: directory.to_string_lossy().into_owned(),
        browsers: browsers(),
        selected_browser,
    })
}

#[tauri::command]
pub fn reveal_extension_folder(app: tauri::AppHandle) -> Result<(), String> {
    let path = prepare_extension(&app)?;
    app.opener()
        .open_path(path.to_string_lossy(), None::<&str>)
        .map_err(|error| error.to_string())
}

fn launch(browser: &Browser, url: &str) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    let mut command = {
        let mut command = Command::new("/usr/bin/open");
        command.arg("-a").arg(&browser.executable).arg(url);
        command
    };
    #[cfg(not(target_os = "macos"))]
    let mut command = {
        let mut command = Command::new(&browser.executable);
        command.arg(url);
        command
    };
    let mut child = command
        .spawn()
        .map_err(|error| format!("Impossible d’ouvrir {} : {error}", browser.name))?;
    std::thread::spawn(move || {
        let _ = child.wait();
    });
    Ok(())
}

#[tauri::command]
pub fn open_browser_setup(
    app: tauri::AppHandle,
    browser_id: String,
    page: String,
) -> Result<(), String> {
    let browser = browsers()
        .into_iter()
        .find(|browser| browser.id == browser_id)
        .ok_or("Navigateur introuvable. Ouvre sa page des extensions manuellement.")?;
    let url = match page.as_str() {
        "extensions" => browser.extensions_url,
        "usage" => USAGE_URL,
        _ => return Err("Page inconnue".into()),
    };
    prepare_extension(&app)?;
    launch(&browser, url)?;
    fs::write(data_dir(&app)?.join("browser.txt"), browser.id).map_err(|error| error.to_string())
}

#[tauri::command]
pub fn open_selected_usage(app: tauri::AppHandle) -> Result<(), String> {
    if let Ok(id) = fs::read_to_string(data_dir(&app)?.join("browser.txt")) {
        if let Some(browser) = browsers().into_iter().find(|browser| browser.id == id) {
            if launch(&browser, USAGE_URL).is_ok() {
                return Ok(());
            }
        }
    }
    app.opener()
        .open_url(USAGE_URL, None::<&str>)
        .map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn extraction_is_stable_and_repairs_outdated_files() {
        let directory = std::env::temp_dir().join(format!(
            "quota-extension-test-{}-{}",
            std::process::id(),
            crate::unix_timestamp_secs()
        ));
        write_extension(&directory).unwrap();
        let manifest = directory.join("manifest.json");
        let original = fs::read(&manifest).unwrap();
        write_extension(&directory).unwrap();
        assert_eq!(fs::read(&manifest).unwrap(), original);
        fs::write(&manifest, b"outdated").unwrap();
        write_extension(&directory).unwrap();
        assert_eq!(fs::read(&manifest).unwrap(), original);
        for (name, _) in EXTENSION_FILES {
            fs::remove_file(directory.join(name)).unwrap();
        }
        fs::remove_dir(directory).unwrap();
    }

    #[test]
    fn embedded_extension_contains_every_manifest_script() {
        let manifest: serde_json::Value = serde_json::from_slice(EXTENSION_FILES[0].1).unwrap();
        let names: Vec<_> = EXTENSION_FILES.iter().map(|(name, _)| *name).collect();
        assert!(names.contains(&manifest["background"]["service_worker"].as_str().unwrap()));
        for script in manifest["content_scripts"].as_array().unwrap() {
            for name in script["js"].as_array().unwrap() {
                assert!(names.contains(&name.as_str().unwrap()));
            }
        }
    }
}
