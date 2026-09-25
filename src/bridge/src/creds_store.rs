//! 第三方 API key 那份文件在**本机**的「它在哪」那一个出处。
//!
//! # 〔GP1 · 第四波〕写侧不在这里了
//!
//! 主会话 09-25 裁「每台机器上这份文件的程序写者恰好一个 ＝ **那台的后端**」⇒ 本机那一份也由本机常驻后端写
//! （`apikey_remote::write_key_on` → 帧面 `apikey-key-set` → `src/backend/accounts/upstream/file_face.rs`），
//! 与远端同一条路。monitor 这一侧**一个字节都不落**、一处都不够 `creds-core` 的写半边（`harden` 仍开着，只为 Windows 上读 DACL）。
//! 〔墓碑 —— 从前本模块头注是「写侧（monitor 独占）与读侧掩码」，论证「写盘为什么留在 `src/bridge/src`」
//!  与 `K-H2a` 裁四「本机这一份只有这一侧写」；两段的前提（monitor 是本机那一份的写者）没了。〕
//! 本模块留下的：[`resolve_path`]（本 monitor 认的那一份在哪 —— 起本机后端时交给它的就是这个，写之前也拿它核
//! 后端写的是不是这一份）。〔US1〕读侧掩码也不在这里了（本机那份的状态由本机常驻后端答，见文件末尾）。
//!
//! # 这一档保什么、不保什么
//!
//! 整段逐字住 `creds_core` 的 crate 头注（保：同机器上别的用户读不到 · 顺手打开配置文件不会看见 ·
//! 前端每次读写整份配置时它不在里面；**不保**：已经能以你的身份运行程序的人）。
//! ⚠ 那里还记着两条别在这里重复、但**必须一起读**的：DPAPI 那条「拷走也解不开」的性质**今天没有**，
//! 以及**远端那一侧不许从 SFTP 的 mode 参数拿机密性**。

use creds_core::store;
use std::path::PathBuf;

/// 那份文件在本机的位置。
///
/// ★ **与后端那侧是同一个契约**：相对路径住 `creds_core::store`，两边各自 join 自己的家目录。
/// 由 `the_two_sides_resolve_the_same_file` 对拍 —— 两边各写一份字面量，
/// 漂开的那天没有任何东西会说，而症状是「界面上配好了，上游选择说没配」这种查不出来的形状。
///
/// ⚠ 它**不跟随** `claudeDir` 覆盖：`config.rs` 头注逐字「monitor 自己的设置永远在默认
/// `~/.claude/claudecode-frontend/` 下，不跟随 `claudeDir` 字段变化」。
pub(crate) fn resolve_path() -> Option<PathBuf> {
    Some(crate::paths::resolve_monitor_data_dir()?.join(store::FILE_NAME))
}

// 〔US1 · 第四波 4D〕读侧掩码（`ApikeyCredentialsStatus` · `read_status` · `read_status_at` · `notice_of`）〔散文墓碑〕退役：
//   本机那份文件的状态由本机常驻后端答（帧面 `apikey-read`，`src/backend/accounts/upstream/file_face.rs::read_at`），
//   界面经 `chan.call` 直接问、按形状收（`src/apikey-reads.ts`，「永远只有掩码」那一格由后端应答的形状与跨语言金样钉着）。
//   本机与远端同一条路 —— monitor 这一侧从此不读这份文件。

// 〔GP1 · 第四波〕**这里原来是本机那一份的写口**（`write_key`〔散文墓碑〕 / `write_key_at`〔散文墓碑〕 /
// `check_base_url`〔散文墓碑〕）。主会话 09-25 裁「每台机器一个写者 ＝ 那台的后端」⇒ 本机那一份也交本机常驻后端写
// （`apikey_remote::write_key_on` → 帧面 `apikey-key-set` → `src/backend/accounts/upstream/file_face.rs`），
// monitor 一个字节都不落。那几条写路性质（写的那一刻读盘 · 未知键一个不吃 · 顺序稳定 · 出生即只给本人 ·
// 说不出账号拒写 · Base URL 形状错整次不写）在后端那一份的判据里逐条都有（`tests/backend/accounts/upstream/file_face_tests.rs`，
// 本侧独有的三条随之搬了过去，名字带 `gp1_`）。

#[cfg(test)]
#[path = "../../../tests/bridge/creds_store_tests.rs"]
mod tests;
