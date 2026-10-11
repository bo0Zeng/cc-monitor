//! 第三方 API key 那份文件在**本机**的「它在哪」那一个出处。
//!
//! # 写侧不在这里
//!
//! 每台机器上这份文件的程序写者恰好一个 ＝ **那台的后端**：本机那一份也由本机常驻后端写（界面经通道
//! `apikey-key-set` → `src/backend/accounts/upstream_select/file_face.rs`），与远端同一条路。monitor 这一侧
//! **一个字节都不落**（monitor 不开 `harden`，`creds-core` 的写半边在这一侧连编都编不进来）。
//! 那份文件住这台的家，那台后端按家自己推（`creds_core::store::credentials_path`）；本模块只剩判据。
//!
//! # 这一档保什么、不保什么
//!
//! 整段逐字住 `creds_core` 的 crate 头注（保：同机器上别的用户读不到 · 顺手打开配置文件不会看见 ·
//! 前端每次读写整份配置时它不在里面；**不保**：已经能以你的身份运行程序的人）。
//! ⚠ 那里还记着两条别在这里重复、但**必须一起读**的：DPAPI 那条「拷走也解不开」的性质**今天没有**，
//! 以及**远端那一侧不许从 SFTP 的 mode 参数拿机密性**。

// monitor 这一侧不读这份文件：每个 API 号的掩码与「表里有没有行」由那台常驻后端并进账号清单（`accounts-list`）。

#[cfg(test)]
#[path = "../../../../tests/frontend/shell/creds_store_tests.rs"]
mod tests;
