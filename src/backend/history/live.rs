//! **这台此刻活着的会话**（原住 `observe/accounts_query.rs`，10-13 挪来）：读 pidfile 目录（[`pidfiles`]，有上限、跳过要说清）＋ 判活
//! （[`live_session_ids`]：存在性 ＋ 有 `procStart` 时对拍启动时刻）。历史清单的「在跑」· 全文搜索 · 账号归属 · 轮换数「另 N 会话也在用」都问它。

use crate::platform::fs::Ro;
use std::path::Path;

/// 单个 `sessions/<PID>.json` 读取上限（正常几百字节）。
const MAX_SESSION_FILE_BYTES: u64 = 1024 * 1024;
/// `sessions/*.json` 扫描上限，防病态目录拖垮一次性查询。
const MAX_SESSION_FILES: usize = 500;

/// 从 pidfile 字节里取 `procStart`（CC 写的是 starttime ticks 的十进制字符串；容忍裸数字）。
pub(crate) fn parse_procstart_ticks(v: &serde_json::Value) -> Option<u64> {
    let field = v.get("procStart")?;
    if let Some(s) = field.as_str() {
        return s.trim().parse::<u64>().ok();
    }
    field.as_u64()
}

/// pidfile 目录里每一份读得出来的 `(pid, 内容)`（上限、跳过要说清）。抽出来让「这台机器上哪几个会话活着」（[`live_session_ids`]）
/// 与账号归属读同一批 pidfile；pidfile 目录问注册表里后端盯着的那一家（与判活同一处 `agents::pidfile_dir`）。
pub(crate) fn pidfiles(agent_home: &Path) -> Vec<(u32, serde_json::Value)> {
    let mut out = Vec::new();
    let dir = crate::agents::pidfile_dir(agent_home);
    let Ok(rd) = Ro::read_dir(&dir) else {
        return Vec::new(); // 没有 sessions/ → 零行（exit 0）
    };
    let mut seen = 0usize;
    for ent in rd.flatten() {
        if seen >= MAX_SESSION_FILES {
            tracing::warn!("sessions/ 条目超过 {MAX_SESSION_FILES}，其余跳过");
            break;
        }
        let path = ent.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        let Some(pid) = path
            .file_stem()
            .and_then(|s| s.to_str())
            .and_then(|s| s.parse::<u32>().ok())
        else {
            continue;
        };
        seen += 1;
        let bytes = match Ro::read_regular_capped(&path, MAX_SESSION_FILE_BYTES) {
            Ok(b) => b,
            Err(e) => {
                // 跳过要说清是谁：一个超过 `MAX_SESSION_FILE_BYTES` 的 pidfile 被静默丢掉，那个会话就永远不归属到任何账号、也没人说得出是哪一个。
                tracing::warn!(
                    "会话文件 {} 读不了，跳过（不归属该会话）：{e}",
                    path.display()
                );
                continue;
            }
        };
        let v: serde_json::Value = match serde_json::from_slice(&bytes) {
            Ok(v) => v,
            Err(_) => continue,
        };
        out.push((pid, v));
    }
    out
}

/// 这台此刻活着的会话（[`live_session_ids`]，家取这台那一家的家）：轮换数「另 N 会话也在用」· 判「会话走了」都问它。
pub(crate) fn live_here() -> std::collections::BTreeSet<String> {
    live_session_ids(&super::listing::agent_home())
}

/// 这台机器上此刻活着的会话（sid 集合）—— 历史跨机 join 的本机判活源头：pidfile 里的会话 id ＋ 进程还是不是同一个
/// （`platform::proc::session_alive`：存在性 ＋ 有 `procStart` 时对拍启动时刻 —— 与 watcher 加会话那一道闸同一个平台原语，
/// 不是账号归属那道更严的「缺 `procStart` 就不认」）。
pub(crate) fn live_session_ids(agent_home: &Path) -> std::collections::BTreeSet<String> {
    pidfiles(agent_home)
        .into_iter()
        .filter(|(pid, v)| crate::platform::proc::session_alive(*pid, parse_procstart_ticks(v)))
        .filter_map(|(_, v)| {
            v.get("sessionId")
                .and_then(|x| x.as_str())
                .filter(|s| !s.is_empty())
                .map(str::to_string)
        })
        .collect()
}

#[cfg(test)]
#[path = "../../../tests/backend/history/live_tests.rs"]
mod tests;
