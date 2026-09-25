//! 〔AS1 · 第四波 4B〕**MCP 推 / 拉**（`设计/96` 的 B）—— monitor 这一侧：**只编排 I/O，零判定**。
//!
//! # 用户裁决（2026-09-24，`99 §1` V111 · V112）
//!
//! 「各管各的，只有显式推 / 拉」· 推 / 拉之前先给看差异，对面有不同就问盖不盖 ·
//! 「内容，原样拷过去并标出可疑项」、不替用户改写 · 不做自动同步、不做冲突合并。
//!
//! # 形状
//!
//! ```text
//! 看差异 mcp_sync_preview(from, fromDir, to, toDir)
//!   ① 这边那台的后端 files-peek <fromDir>/.mcp.json      （原文 A）
//!   ② 对面那台的后端 files-peek <toDir>/.mcp.json        （原文 B，或「不存在」）
//!   ③ 对面那台的后端 mcp-sync-plan {source: A, target: B} （差异 ＋ 可疑项，事实是对面的）
//! 写    mcp_sync_apply(to, toDir, A, B, take, overwrite)
//!   ④ 对面那台的后端 mcp-sync-plan {…, take, overwrite}   （真要写哪几条；differs 没说盖 ⇒ 整趟拒）
//!   ⑤ 按那几条把 A 里的值**原样**合进 B（`mcp.rs::plan_project_mcp` ＋ `upsert_mcp_server_value`，与单条写同一份）
//!   ⑥ 对面那台的后端 files-put（expect = B）—— 对面在看差异之后变了 ⇒ 一个字节不写，说清
//! ```
//!
//! - **判定一处，住后端**（`设计/01 §1.1`）：差异四态、可疑项、「写哪几条」都是 `mcp-sync-plan` 答的；
//!   这里一条规则都不写（`mcp_sync_tests` 零命中钉着）。
//! - **落点一处**：`.mcp.json` 的路径只由 `mcp.rs` 的两个出口给（[`crate::mcp::project_mcp_target`]）。
//! - **写一处**：经那台后端 `files-put`（`user_files::Door`），monitor 一个字节不落盘。
//! - 🔴 **不重读重算**（与 `user_files::edit` 刻意不同）：用户确认的是**他看到的那份差异**；对面在他看过之后变了，
//!   重算等于替他在一份他没看过的内容上做决定 ⇒ `stale` 就停。
//! - **原样**：写过去的值取自原文 A 的解析（值逐个原样），合进 B 走单条写那同一份规划。
//!   ⚠ 两侧的 JSON 都**不保键序**（`serde_json` 没开 `preserve_order`）⇒ 写回去的整份按键名重排 ——
//!   与单条「添加 / 更新」既有的行为同一件事，内容（值）不变。
//!
//! # 买不到
//!
//! - 🔴 真远端 / 真 Windows 一维都没跑过（判据用替身门 ＋ 替身判定）。
//! - user scope（`~/.claude.json`）不在射程：写面只许 `.mcp.json`（SS-14）。

use crate::backend::control::backend_route::{no_channel, route_call_error, Routed};
use crate::backend::control::inbound_client::{client_for, encode_request};
use crate::origin::Origin;
use crate::user_files::{BackendDoor, Door, Refused, REQUEST_LINE_CAP};
use serde_json::{json, Map, Value};

/// 后端那条命令的名字（与 `src/backend/inbound.rs::REGISTRY` 同名，跨半边由判据现抠对拍）。
pub(crate) const CMD: &str = "mcp-sync-plan";

/// 一趟判定的上限（对可疑路径逐条 `stat`，秒级内完成；给足余量同时防卡死）。
const BUDGET: std::time::Duration = std::time::Duration::from_secs(30);

/// 一条可疑项（后端判的，原样转给界面）。
#[derive(serde::Serialize, Debug, Clone, PartialEq, Eq)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../../src/generated/"))]
pub struct McpSyncSuspect {
    /// `abs-path` · `command-missing` · `command-relative`（闭集住后端 `mcp_sync::SUSPECT_KINDS`）。
    pub kind: String,
    /// `command` / `args[i]` / `env.<键>` / `cwd`。
    pub field: String,
    pub value: String,
    /// `present` · `absent` · `foreign` · `unknown`；`command-relative` 那种是 `null`。
    pub there: Option<String>,
}

/// 差异表的一行。
#[derive(serde::Serialize, Debug, Clone, PartialEq)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../../src/generated/"))]
pub struct McpSyncRow {
    pub name: String,
    /// `new` · `same` · `differs` · `only-there`（闭集住后端 `mcp_sync::STATES`）。
    pub state: String,
    pub suspects: Vec<McpSyncSuspect>,
    /// 这边那一条的配置（原样；`only-there` 时没有）。
    #[cfg_attr(test, ts(type = "unknown"))]
    pub source: Option<Value>,
    /// 对面那一条的配置（原样；`new` 时没有）。
    #[cfg_attr(test, ts(type = "unknown"))]
    pub target: Option<Value>,
}

/// 看差异的结果。两份原文**原样**带回界面，写的时候原样送回来：写要拿「看差异时读到的那一份」当 CAS 期望。
#[derive(serde::Serialize, Debug, Clone, PartialEq)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../../src/generated/"))]
#[serde(rename_all = "camelCase")]
pub struct McpSyncPreview {
    /// 这边那份的路径（后端解过链接的那一个，给人看）。
    pub source_path: String,
    /// 对面那份的路径。
    pub target_path: String,
    pub source_text: String,
    /// 对面那份不存在 ⇒ `null`。
    pub target_text: Option<String>,
    pub rows: Vec<McpSyncRow>,
}

/// 写的结果。
#[derive(serde::Serialize, Debug, Clone, PartialEq, Eq)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../../src/generated/"))]
pub struct McpSyncApplied {
    /// 写到了哪（没写时是要写的那一份）。
    pub path: String,
    /// 真写了吗（一条都没选 / 算出来与对面逐字相同 ⇒ `false`）。
    pub written: bool,
    /// 写进去的条目名。
    pub names: Vec<String>,
}

/// 对面那台后端的判定（`mcp-sync-plan`）。生产只有 [`BackendJudge`]；判据用替身。
pub(crate) trait Judge {
    async fn plan(&self, args: Value) -> Result<Value, String>;
}

/// 生产那一个：经**要被写的那一台**的后端（本机与远端同一份，只差 origin）。
pub(crate) struct BackendJudge {
    pub origin: Origin,
}

impl Judge for BackendJudge {
    async fn plan(&self, args: Value) -> Result<Value, String> {
        let wire = self.origin.as_wire_str();
        let who = crate::backend::control::cc_bus::machine_label(wire);
        let Some(client) = client_for(wire) else {
            let why = match no_channel(wire) {
                Routed::NoChannel(s) | Routed::Refused(s) => s,
                Routed::Done => String::new(),
            };
            return Err(format!(
                "{who} 的后端没连上，比对要经它来做（{why}）—— 一个字节都没动"
            ));
        };
        if !client.accepts(CMD) {
            return Err(
                crate::backend::control::cc_bus::describe_backend_too_old_for(
                    wire,
                    CMD,
                    "没法比对，一个字节都没动",
                ),
            );
        }
        // 两份原文装进同一行请求：后端一行上限 1 MiB。装不下当场说清，不发。
        let line = encode_request("0", CMD, &args);
        if line.len() > REQUEST_LINE_CAP {
            return Err(format!(
                "两份配置合起来太大，一趟装不下（请求一行 {} 字节，上限 {REQUEST_LINE_CAP}）—— 一个字节都没动",
                line.len()
            ));
        }
        let data = client.call(CMD, args, BUDGET).await.map_err(|e| {
            match route_call_error(&e, |_code, message| format!("{who}：{message}")) {
                Routed::NoChannel(s) | Routed::Refused(s) => s,
                Routed::Done => format!("{who}：比对时出了内部错误，没有拿到结果"),
            }
        })?;
        data.ok_or_else(|| format!("{who} 的后端对 `{CMD}` 回了一条空应答"))
    }
}

/// 原文里的 `mcpServers`（**只取值，不判**：合不合法已由后端那一趟判过；这里取不到就是两端对不上）。
///
fn servers_in(text: Option<&str>) -> Map<String, Value> {
    text.and_then(|t| serde_json::from_str::<Value>(t.trim_start_matches('\u{feff}')).ok())
        .and_then(|v| v.get("mcpServers").and_then(Value::as_object).cloned())
        .unwrap_or_default()
}

/// 对面后端的应答认不出来（缺格 / 类型不对）时给人的那句话 —— 几处同一句：对用户来说它们是同一件事。
/// 多半是两台的后端版本不一样；**不猜默认值**（猜出来的「空差异」会让人以为两边一样）。
const UNREADABLE_REPLY: &str =
    "对面的后端答的内容认不出来，多半是两边版本不一样。这一趟什么都没写。";

/// 后端那一行 → 差异表的一行（值由调用方从原文里原样取）。契约对不上 ⇒ 报错，不猜默认值。
fn row_from_wire(
    r: &Value,
    source: &Map<String, Value>,
    target: &Map<String, Value>,
) -> Result<McpSyncRow, String> {
    let s = |k: &str| r.get(k).and_then(Value::as_str).map(str::to_string);
    let broken = || UNREADABLE_REPLY.to_string();
    let name = s("name").ok_or_else(broken)?;
    let state = s("state").ok_or_else(broken)?;
    let suspects = r
        .get("suspects")
        .and_then(Value::as_array)
        .ok_or_else(broken)?
        .iter()
        .map(|x| {
            let g = |k: &str| x.get(k).and_then(Value::as_str).map(str::to_string);
            Ok(McpSyncSuspect {
                kind: g("kind").ok_or_else(broken)?,
                field: g("field").ok_or_else(broken)?,
                value: g("value").ok_or_else(broken)?,
                there: g("there"),
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    Ok(McpSyncRow {
        source: source.get(&name).cloned(),
        target: target.get(&name).cloned(),
        name,
        state,
        suspects,
    })
}

/// 看差异（可测的那一半：门与判定都是参数）。
pub(crate) async fn preview_with(
    from: &impl Door,
    from_target: &str,
    to: &impl Door,
    to_target: &str,
    judge: &impl Judge,
) -> Result<McpSyncPreview, String> {
    let (root, rel) = crate::mcp::split_target(from_target)?;
    let src = from.peek(root, rel).await?;
    let Some(source_text) = src.text else {
        return Err(format!(
            "{} 上没有 {} —— 没有可以拷过去的条目",
            from.machine(),
            src.path
        ));
    };
    let (root, rel) = crate::mcp::split_target(to_target)?;
    let tgt = to.peek(root, rel).await?;
    let data = judge
        .plan(json!({ "source": source_text, "target": tgt.text }))
        .await?;
    let (sv, tv) = (
        servers_in(Some(&source_text)),
        servers_in(tgt.text.as_deref()),
    );
    let rows = data
        .get("rows")
        .and_then(Value::as_array)
        .ok_or_else(|| UNREADABLE_REPLY.to_string())?
        .iter()
        .map(|r| row_from_wire(r, &sv, &tv))
        .collect::<Result<Vec<_>, String>>()?;
    Ok(McpSyncPreview {
        source_path: src.path,
        target_path: tgt.path,
        source_text,
        target_text: tgt.text,
        rows,
    })
}

/// 对面在看差异之后变了的那句话。
fn stale_said(machine: &str) -> String {
    format!(
        "{machine} 上那份配置在你看差异之后又被改过了，这一趟一个字节都没写。重新看一次差异再决定。"
    )
}

/// 写（可测的那一半）。
pub(crate) async fn apply_with(
    to: &impl Door,
    to_target: &str,
    judge: &impl Judge,
    source_text: &str,
    target_text: Option<&str>,
    take: &[String],
    overwrite: &[String],
) -> Result<McpSyncApplied, String> {
    let data = judge
        .plan(json!({
            "source": source_text,
            "target": target_text,
            "take": take,
            "overwrite": overwrite,
        }))
        .await?;
    let names: Vec<String> = data
        .get("write")
        .and_then(Value::as_array)
        .ok_or_else(|| UNREADABLE_REPLY.to_string())?
        .iter()
        .map(|n| n.as_str().map(str::to_string))
        .collect::<Option<_>>()
        .ok_or_else(|| UNREADABLE_REPLY.to_string())?;
    let unchanged = |names| McpSyncApplied {
        path: to_target.to_string(),
        written: false,
        names,
    };
    if names.is_empty() {
        return Ok(unchanged(Vec::new()));
    }
    let source = servers_in(Some(source_text));
    let next = crate::mcp::plan_project_mcp(to_target, target_text, &mut |v: &mut Value| {
        for n in &names {
            let server = source.get(n).cloned().ok_or_else(|| {
                format!("「{n}」不在拷出来的那一份里 —— 两端对不上，一个字节都没写")
            })?;
            crate::mcp::upsert_mcp_server_value(v, n.clone(), server)?;
        }
        Ok(true)
    })?;
    let Some(next) = next.filter(|c| Some(c.as_str()) != target_text) else {
        return Ok(unchanged(names));
    };
    let (root, rel) = crate::mcp::split_target(to_target)?;
    match to.put(root, rel, &next, target_text, false, false).await {
        Ok(landed) => Ok(McpSyncApplied {
            path: landed.path,
            written: landed.changed,
            names,
        }),
        Err(Refused::Stale(_)) => Err(stale_said(&to.machine())),
        Err(e) => Err(e.said()),
    }
}

/// 看差异：「这边」（`from` 那台的 `fromDir`）拷到「对面」（`to` 那台的 `toDir`）会发生什么。
/// 推 ＝ 这边是本页那台；拉 ＝ 对面是本页那台 —— 同一条命令，界面换个方向说。
#[tauri::command]
pub async fn mcp_sync_preview(
    from: Origin,
    from_dir: String,
    to: Origin,
    to_dir: String,
) -> Result<McpSyncPreview, String> {
    let from_target = crate::mcp::project_mcp_target(&from, "mcp_sync_preview", &from_dir)?;
    let to_target = crate::mcp::project_mcp_target(&to, "mcp_sync_preview", &to_dir)?;
    if from == to && from_target == to_target {
        return Err("两边是同一份文件 —— 换一台机器或换一个项目目录".to_string());
    }
    preview_with(
        &BackendDoor::new(from),
        &from_target,
        &BackendDoor::new(to.clone()),
        &to_target,
        &BackendJudge { origin: to },
    )
    .await
}

/// 写：把勾的那几条原样合进对面那份（`differs` 的必须也在 `overwrite` 里）。
/// `sourceText` / `targetText` 是看差异时拿到的那两份原文，原样送回来（后者当 CAS 期望）。
#[tauri::command]
pub async fn mcp_sync_apply(
    to: Origin,
    to_dir: String,
    source_text: String,
    target_text: Option<String>,
    take: Vec<String>,
    overwrite: Vec<String>,
) -> Result<McpSyncApplied, String> {
    let to_target = crate::mcp::project_mcp_target(&to, "mcp_sync_apply", &to_dir)?;
    apply_with(
        &BackendDoor::new(to.clone()),
        &to_target,
        &BackendJudge { origin: to },
        &source_text,
        target_text.as_deref(),
        &take,
        &overwrite,
    )
    .await
}

#[cfg(test)]
#[path = "../../../tests/bridge/mcp_sync_tests.rs"]
mod tests;
