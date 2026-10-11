//! 文件窗口包的构建脚本：只做一件事 —— Windows 上把进程清单与图标编成资源，交给本包**每一个**链接产物（程序与测试程序）。
//!
//! 清单与 monitor 那份是同一个文件（`../shell/windows/app.manifest`，「通用控件 v6」那一条）：
//! 本包用 rfd 的 `common-controls-v6`，按名字引入 comctl32 里只有 v6 才导出的 `TaskDialogIndirect` 一族，缺了清单进程起不来（`0xc0000139`）。
//! 从前这个程序是 monitor 包的第二个 `[[bin]]`，清单与图标都由 monitor 的 `build.rs` / `tauri-build` 给；size2（10-11）搬进本包之后由这里给。
//! 图标：任务栏与 Alt-Tab 上那一格（egui 窗口没设图标时 Windows 取可执行文件的第一个图标资源）。
//! 判据：门禁 `winlink` 那一格读 `cc-monitor-filewin.exe` 的资源表（有 MANIFEST、点名 Common-Controls）。

fn main() {
    let dir =
        std::path::PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    let shell = dir.join("..").join("shell");
    let manifest = shell.join("windows").join("app.manifest");
    let icon = shell.join("icons").join("icon.ico");
    println!("cargo:rerun-if-changed={}", manifest.display());
    println!("cargo:rerun-if-changed={}", icon.display());
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let out = std::path::PathBuf::from(std::env::var("OUT_DIR").expect("OUT_DIR"));
    let rc = out.join("filewin.rc");
    // `1 24` ＝ CREATEPROCESS_MANIFEST_RESOURCE_ID · RT_MANIFEST；`1 ICON` ＝ 第一个图标组。rc 的字符串里反斜杠要成对。
    let esc = |p: &std::path::Path| p.display().to_string().replace('\\', "\\\\");
    std::fs::write(
        &rc,
        format!("1 24 \"{}\"\n1 ICON \"{}\"\n", esc(&manifest), esc(&icon)),
    )
    .expect("写不出 filewin.rc");
    if let Err(e) =
        embed_resource::compile_for_everything(&rc, embed_resource::NONE).manifest_required()
    {
        println!("cargo:warning=清单没编成资源：{e}");
        std::process::exit(1);
    }
}
