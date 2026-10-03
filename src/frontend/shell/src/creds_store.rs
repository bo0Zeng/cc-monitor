//! 第三方 API key 那份文件在**本机**的「它在哪」那一个出处。
//!
//! # 写侧不在这里了
//!
//! 「每台机器上这份文件的程序写者恰好一个 ＝ **那台的后端**」⇒ 本机那一份也由本机常驻后端写
//! （界面经通道 `chan.call(这台, "apikey-key-set", …)` → `src/backend/accounts/upstream_select/file_face.rs`；
//!  〔GP1 那一版〕经 monitor 的 `apikey_remote::write_key_on`〔散文墓碑〕），
//! 与远端同一条路。monitor 这一侧**一个字节都不落**、一处都不够 `creds-core` 的写半边（monitor 不开 `harden`，写半边在这一侧连编都编不进来 —— `src/frontend/shell/Cargo.toml` 那一行）。
//! 〔墓碑 —— 从前本模块头注是「写侧（monitor 独占）与读侧掩码」，论证「写盘为什么留在 `src/frontend/shell/src`」
//!  与 `K-H2a` 裁四「本机这一份只有这一侧写」；两段的前提（monitor 是本机那一份的写者）没了。〕
//! 那份文件在哪也不由这里说：它住这台的家（`~/.cc-monitor`），那台后端按家自己推（`creds_core::store::credentials_path`），
//! monitor 起本机后端时不交路径。本模块只剩判据（本 monitor 这一侧不读、不写、不交明文的那几条）。
//!
//! # 这一档保什么、不保什么
//!
//! 整段逐字住 `creds_core` 的 crate 头注（保：同机器上别的用户读不到 · 顺手打开配置文件不会看见 ·
//! 前端每次读写整份配置时它不在里面；**不保**：已经能以你的身份运行程序的人）。
//! ⚠ 那里还记着两条别在这里重复、但**必须一起读**的：DPAPI 那条「拷走也解不开」的性质**今天没有**，
//! 以及**远端那一侧不许从 SFTP 的 mode 参数拿机密性**。

// 读侧掩码（`ApikeyCredentialsStatus` · `read_status` · `read_status_at` · `notice_of`）〔散文墓碑〕退役：
//   本机那份文件的状态由本机常驻后端答（帧面 `apikey-read`，`src/backend/accounts/upstream_select/file_face.rs::read_at`），
//   界面经 `chan.call` 直接问、按形状收（`src/frontend/ui/apikey-reads.ts`，「永远只有掩码」那一格由后端应答的形状与跨语言金样钉着）。
//   本机与远端同一条路 —— monitor 这一侧从此不读这份文件。

// **这里原来是本机那一份的写口**（`write_key`〔散文墓碑〕 / `write_key_at`〔散文墓碑〕 /
// `check_base_url`〔散文墓碑〕）。「每台机器一个写者 ＝ 那台的后端」⇒ 本机那一份也交本机常驻后端写
// （界面经通道发 `apikey-key-set` → `src/backend/accounts/upstream_select/file_face.rs`；〔GP1 那一版〕经 `apikey_remote::write_key_on`〔散文墓碑〕），
// monitor 一个字节都不落。那几条写路性质（写的那一刻读盘 · 未知键一个不吃 · 顺序稳定 · 出生即只给本人 ·
// 说不出账号拒写 · Base URL 形状错整次不写）在后端那一份的判据里逐条都有（`tests/backend/accounts/upstream_select/file_face_tests.rs`，
// 本侧独有的三条随之搬了过去，名字带 `gp1_`）。

#[cfg(test)]
#[path = "../../../../tests/frontend/shell/creds_store_tests.rs"]
mod tests;
