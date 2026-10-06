//! **上传那颗按钮** —— 命令栏「上传」/ Ctrl+U ⇒ 系统的选文件框（可多选）⇒ 选到的那几份走拖入那一条
//! （[`super::transfer::run_drop`]：先一次问完同名，再并行传）。起框与接结局住 `shell.rs`（`start_pick` / `settle_pick_with`）。
//!
//! 〔照稿 10-05〕原来那一问（「本机路径，一行一个」手填框 ＋「选择…」）随系统选文件框删了。

use copy_core::copy_text;

/// 命令栏上那颗按钮的字面。**唯一住址**（判据按同一个常量去找它画出来的字）。
pub static UPLOAD_LABEL: std::sync::LazyLock<String> =
    std::sync::LazyLock::new(|| copy_text("rsFilewinUpload.label.upload", &[]));

#[cfg(test)]
#[path = "../../../../tests/frontend/filewin/upload_tests.rs"]
mod tests;
