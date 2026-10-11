//! 本包构建期的两件事（Windows 上才做）：
//! ① 进程清单编成资源、交给本包每一个链接产物（程序 · lib 的测试程序）：清单与编法只住 `src/common/win-manifest`，monitor 的 `build.rs` 调同一处。
//! ② 程序的图标（任务栏 · Alt-Tab：egui 窗口没设图标时 Windows 取可执行文件的第一个图标资源）。从前这个程序是 monitor 包的第二个 `[[bin]]`，
//!    图标由 `tauri-build` 给；size2（10-11）搬进本包之后由这里给，用 monitor 那一份 `icon.ico`。
//! 判据：门禁 `winlink` 那一格读 `cc-monitor-filewin.exe` 与 lib 测试程序的资源表（有 MANIFEST、点名 Common-Controls）。

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    win_manifest::embed_for_every_artifact();
    let dir =
        std::path::PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    let icon = dir.join("..").join("shell").join("icons").join("icon.ico");
    println!("cargo:rerun-if-changed={}", icon.display());
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let out = std::path::PathBuf::from(std::env::var("OUT_DIR").expect("OUT_DIR"));
    let rc = out.join("filewin-icon.rc");
    // `1 ICON` ＝ 第一个图标组。rc 的字符串里反斜杠要成对。
    std::fs::write(
        &rc,
        format!("1 ICON \"{}\"\n", icon.display().to_string().replace('\\', "\\\\")),
    )
    .expect("写不出 filewin-icon.rc");
    if let Err(e) = embed_resource::compile(&rc, embed_resource::NONE).manifest_optional() {
        println!("cargo:warning=图标没编成资源：{e}");
        std::process::exit(1);
    }
}
