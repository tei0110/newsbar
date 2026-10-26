// リリースビルドで Windows のコンソールを出さない（Tauri の定型）
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    newsbar_lib::run();
}
