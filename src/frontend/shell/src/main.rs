//! 二进制入口：仅转调库 crate 的 `monitor_lib::run()`，真正的装配都在 lib.rs。

// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    monitor_lib::run()
}
