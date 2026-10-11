//! 本包构建期的两件事（Windows 上才做），编法都只住 `src/common/win-manifest`：
//! ① 进程清单编成资源、交给本包每一个链接产物（程序 · lib 的测试程序）；monitor 的 `build.rs` 调同一处。
//! ② 程序的图标（任务栏 · Alt-Tab）。从前这个程序是 monitor 包的第二个 `[[bin]]`，图标由 `tauri-build` 给；
//!    size2（10-11）搬进本包之后由这里给，用 monitor 那一份 `icon.ico`。
//! 判据：门禁 `winlink` 那一格读 `cc-monitor-filewin.exe` 与 lib 测试程序的资源表（有 MANIFEST、点名 Common-Controls）。

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    win_manifest::embed_for_every_artifact();
    let dir =
        std::path::PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    win_manifest::embed_icon_for_bins(&dir.join("..").join("shell").join("icons").join("icon.ico"));
}
