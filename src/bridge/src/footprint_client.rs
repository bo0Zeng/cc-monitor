//! 〔MIG-3b 续 · 主会话 09-28 裁①〕「足迹」里 **monitor 自己那台的那几行**（`HostScope::Client`）的**事实** —— 只读。
//!
//! 申报表与判定都进了后端（`src/backend/footprint/`）；那一族说的是 monitor 这台自己的东西（本机 `ccm` 入口 · 终端 ·
//! shell profile …），事实照旧由 monitor 答（`设计/05 §14.3` E 组「足迹里 monitor 自己那几行」）。界面排先后：
//! 先问这里要环境 → 本机后端 `footprint-report {client:{env}}` 回要 stat 哪些路径 → 再问这里 stat → 带着答案再问后端出报告。
//! 本模块一个工具名、一条判定规则都不认识：只 stat 交来的绝对路径、列一层名字。

use serde_json::{json, Map, Value};
use std::path::{Path, PathBuf};

/// 一个目录最多交几个名字（超了 ⇒ 列不动，不截断 —— 同后端那一侧）。
const MAX_ENTRIES: usize = 4096;
/// 一趟最多 stat 几条（同后端收的上限）。
const MAX_PATHS: usize = 1024;

/// 界面要 monitor 这台的足迹事实：`stat` 不给 ⇒ 只回环境；给了 ⇒ 逐条 `{kind, size, entries?}` / `null`（不在 / 读不动）。
#[tauri::command]
pub fn footprint_client_facts(stat: Option<Vec<String>>) -> Result<Value, String> {
    facts(
        dirs::home_dir(),
        crate::paths::resolve_claude_dir(),
        std::env::var("PATH").ok(),
        stat.as_deref().unwrap_or(&[]),
    )
}

/// [`footprint_client_facts`] 的本体（环境是参数，判据拿夹具喂）。
pub(crate) fn facts(
    home: Option<PathBuf>,
    agent_home: Option<PathBuf>,
    path: Option<String>,
    stat: &[String],
) -> Result<Value, String> {
    let no_home = || crate::copy_table::copy_text("rsFootprintClient.env.noHome", &[]);
    let home = home.ok_or_else(no_home)?;
    let agent_home = agent_home.ok_or_else(no_home)?;
    if stat.len() > MAX_PATHS {
        return Err(crate::copy_table::copy_text(
            "rsFootprintClient.stat.tooMany",
            &[
                ("n", &stat.len().to_string()),
                ("max", &MAX_PATHS.to_string()),
            ],
        ));
    }
    let mut out = Map::new();
    for p in stat {
        let path = Path::new(p);
        // 相对路径按本进程的 cwd 解，那不是任何人的意思 ⇒ 答「不在」之外的「不知道」：不进表，后端会按「没答」拒。
        if path.is_absolute() {
            out.insert(p.clone(), stat_of(path));
        }
    }
    Ok(json!({
        "env": {
            "home": home.display().to_string(),
            "agentHome": agent_home.display().to_string(),
            "path": path,
        },
        "stat": out,
    }))
}

/// 一条路径的事实：不在 / 读不动 ⇒ `null`；目录列不动 / 名字太多 ⇒ 不带 `entries`。
fn stat_of(p: &Path) -> Value {
    let Ok(md) = std::fs::metadata(p) else {
        return Value::Null;
    };
    if !md.is_dir() {
        return json!({ "kind": "file", "size": md.len() });
    }
    let names: Option<Vec<String>> = std::fs::read_dir(p).ok().map(|it| {
        it.filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .take(MAX_ENTRIES + 1)
            .collect()
    });
    match names {
        Some(n) if n.len() <= MAX_ENTRIES => json!({ "kind": "dir", "size": 0, "entries": n }),
        Some(_) => {
            tracing::warn!(
                "足迹：{} 里名字太多（超过上限），没列（按列不动报，不截断）",
                p.display()
            );
            json!({ "kind": "dir", "size": 0 })
        }
        None => json!({ "kind": "dir", "size": 0 }),
    }
}

#[cfg(test)]
#[path = "../../../tests/bridge/footprint_client_tests.rs"]
mod tests;
