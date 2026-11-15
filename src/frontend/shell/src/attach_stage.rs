//! **聊天附件那一份的字节，壳只放进本机暂存区**：webview 只给 `File`（没有本机路径）⇒ 主界面交原样字节，壳写成
//! `~/.cc-monitor/staging/<随机 32 位十六进制>.part`（只给本人），回那条路径。问收不收 · 逐块送 · 提交 · 断了重排都是本机后端的事
//! （`attach-draft-put` 交它这条路径，`backend/control/attach_upload.rs`），壳不分块、不问那台。
//!
//! ```text
//! 界面 attach-draft.ts ──attach_stage（原样字节）──▶ 本文件 ──写进本机暂存区──▶ 回路径 ──▶ 界面交 attach-draft-put
//! ```
//!
//! 没成只回通道那一形（[`Fail`]，本侧的 `ours` 层：用法错 · 本机这边坏了），界面照 `chan.ts::decodeFail` 解；系统原话进日志。

// 收 `Request<'_>`（引用）的异步命令：tauri 的命令宏在它旁边生成一道「必须回 `Result`」的编译期检查（`let _: &dyn …`），
// 那一句落在本模块里、不在任何函数里 ⇒ 放行只能写在模块这一层；本文件自己的代码不写 `let _`。
#![expect(
    clippy::let_underscore_must_use,
    reason = "tauri 命令宏给收引用的异步命令生成的「必须回 Result」检查"
)]

use crate::chan::webview::{fail, Fail};
use crate::chan::wire::OursFault;
use crate::origin::Origin;
use std::io::Write as _;
use std::path::{Path, PathBuf};

/// 写一份进 `staging` 那个目录（不在就建、只给本人）：名字是 `stem`（32 位十六进制）＋ `.part`；回那份的路径。已在 ⇒ 拒。
pub(crate) fn put_into(staging: &Path, stem: &str, bytes: &[u8]) -> Result<PathBuf, String> {
    crate::platform::fs::ensure_private_dir(staging).map_err(|s| s.said)?;
    let at = staging.join(format!("{stem}.part"));
    let wrote = |e: std::io::Error| format!("{}: {e}", at.display());
    let mut f = platform_core::atomic::create_new_private(&at).map_err(wrote)?;
    f.write_all(bytes).map_err(wrote)?;
    f.sync_all().map_err(wrote)?;
    Ok(at)
}

/// 主界面交一份附件的原样字节：写进本机暂存区，回那条路径（交 `attach-draft-put`）。请求体不是原样字节 ⇒ 用法错。
#[tauri::command]
pub async fn attach_stage(request: tauri::ipc::Request<'_>) -> Result<String, Fail> {
    let here = Origin::local();
    let tauri::ipc::InvokeBody::Raw(bytes) = request.body() else {
        return Err(fail(&here, "attach_stage", OursFault::Misuse.into()));
    };
    let broke = |why: String| {
        tracing::warn!("attach_stage: {why}");
        fail(&here, "attach_stage", OursFault::Broken.into())
    };
    let staging = crate::data_paths::home()
        .ok_or_else(|| broke("no home dir".to_string()))?
        .join(relay_route_core::STAGING_DIR_REL);
    let stem = uuid::Uuid::new_v4().simple().to_string();
    let bytes = bytes.clone();
    let at = tauri::async_runtime::spawn_blocking(move || put_into(&staging, &stem, &bytes))
        .await
        .map_err(|e| broke(e.to_string()))?
        .map_err(broke)?;
    Ok(at.to_string_lossy().into_owned())
}

#[cfg(test)]
#[path = "../../../../tests/frontend/shell/attach_stage_tests.rs"]
mod tests;
