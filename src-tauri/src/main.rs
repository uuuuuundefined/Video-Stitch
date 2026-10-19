#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    #[cfg(target_os = "macos")]
    if video_stitch::cleanup::run_if_requested() {
        return;
    }
    video_stitch::desktop::run();
}
