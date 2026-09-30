// Use the Windows GUI subsystem in both debug installers and release builds.
#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

fn main() {
    chatgpt_quota_monitor_lib::run()
}
