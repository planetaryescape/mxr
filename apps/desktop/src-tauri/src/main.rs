// Release builds on macOS don't need a console; keep it for other targets' dev runs.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    mxr_desktop_lib::run();
}
