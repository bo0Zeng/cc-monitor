//! Claude 的**账号文件**：`.claude.json` 的信任判定。
//!
//! ⚠ 只装"这份文件长什么样、怎么读出信任位"。**账号清单（manifest）与配置目录白名单
//! 不在这里** —— 那是 `cc-acct-iso` 的格式，属工具而非 agent，留在 `accounts_query`。

use crate::common::fs::read_regular_capped;
use std::path::Path;

/// 账号级配置文件的文件名（住在配置根下）。
pub(crate) const CONFIG_FILE_NAME: &str = ".claude.json";

/// 读取上限。这份文件会被 MCP 配置撑大，给 32MB。
/// 安全：`read_regular_capped` 的 `is_file` 挡 FIFO/设备（审计实测 symlink→`/dev/zero`
/// 6 秒涨 11GB），`take` 限量。
pub(crate) const MAX_CONFIG_BYTES: u64 = 32 * 1024 * 1024;

/// 某个根目录下的配置文件路径。
///
/// ⚠ 有它才能让「账号 0 的配置必须来自 `$HOME`」那条自省判据继续钉得住：
/// 原来那条断言比的是源码里出不出现 `home.join(".claude.json")` 这个**字面量**，
/// 而字面量随本件搬进了适配层。⇒ 判据的比对对象换成这个函数名，性质不变。
pub(crate) fn config_path_in(root: &std::path::Path) -> std::path::PathBuf {
    root.join(CONFIG_FILE_NAME)
}

/// 某个配置根下、对某个 cwd 的**信任状态** → 一行 JSON（`{trusted, known, error}`）。
///
/// `S3` 从 `accounts_query::trust_of_claude_json` 原样搬来（逻辑一字未改）。
///
/// ⚠ **只出这三个字段**：`.claude.json` 里有 `mcpServers` 的环境变量（可能含 API key），
/// 整份吐出去就是把密钥送上 wire。
pub(crate) fn trust_of_config(p: &Path, cwd: &str) -> Result<String, (String, String)> {
    if !p.exists() {
        // 该账号还没有配置文件（全新账号）→ 肯定没信任过，不是错误
        return Ok(
            serde_json::json!({"trusted": false, "known": false, "error": serde_json::Value::Null})
                .to_string(),
        );
    }
    let bytes = read_regular_capped(p, MAX_CONFIG_BYTES)
        .map_err(|e| ("claude_json_unreadable".to_string(), e))?;
    let v: serde_json::Value = serde_json::from_slice(&bytes)
        .map_err(|e| ("claude_json_invalid".to_string(), e.to_string()))?;
    let entry = v.get("projects").and_then(|p| p.get(cwd));
    let known = entry.is_some();
    let trusted = entry
        .and_then(|e| e.get("hasTrustDialogAccepted"))
        .and_then(|b| b.as_bool())
        .unwrap_or(false);
    Ok(
        serde_json::json!({"trusted": trusted, "known": known, "error": serde_json::Value::Null})
            .to_string(),
    )
}
