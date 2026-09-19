//! Subagent JSONL 按需加载。
//!
//! Claude Code 的 Task/Agent tool_use 触发的 subagent 不出现在主 session JSONL，
//! 而是独立写到：
//!   `<encoded-cwd>/<parent-session-id>/subagents/agent-<hash>.jsonl`
//! 同目录还有 `agent-<hash>.meta.json`：`{agentType, description}`。
//!
//! 主 watcher 跳过这些文件；本模块提供一个 IPC 命令，前端在用户展开 Task 折叠
//! 卡时调用，按 (parent_jsonl_path, tool_use.description, tool_use.timestamp)
//! 定位并加载对应 subagent JSONL。
//!
//! # `K-R94`（09-12）：**「找」也交给后端了**
//!
//! 改前两条路只共用了「挑」那一半（`pick_closest`）：远端让 daemon 列候选，
//! **而本机自己做了三样** —— 候选枚举（`read_dir` 扫 `*.meta.json`）· 读首行时间戳 ·
//! 读 jsonl。这三样后端侧的 `--list-subagents` / `--read-session` 早就有了。
//!
//! ⇒ 本模块现在只剩**一条**流程，两条路唯一的差别是**谁去跑那条查询**（[`Backend`]）：
//!
//! 1. `--list-subagents <父会话 jsonl>` ⇒ 后端逐行吐 `{path, description, timestamp}`
//! 2. [`choose_subagent`]：按 `description` 精确串等筛 ＋ [`pick_closest`] 按时间戳挑最近
//!    —— **纯函数，不碰文件系统**；两条路喂进来的是同一种东西
//! 3. `--read-session <选中的 jsonl>` ⇒ 原样透传字节，本侧走既有的 [`parse_line`]
//!
//! 这与 `K28`（前端不许自己发明对外行为，一切对外都经后端）/ `K33`（一件事只许有一处实现）
//! 同向：「有哪些候选」是**后端**回答的问题，本侧只负责在候选里挑。
//!
//! ⚠ **两条 transport 之间一条如实登记的差别**（不在本模块能收的范围里）：
//! 远端那条（`run_list_query`）带 **30s 整体超时**与**单行上限**；本机那条
//! （`local_query::run_query`）**两样都没有** —— 那一层刻意把超时留给调用方（见它自己的头注）。
//! subagent 通常是短命的侧任务、文件很小，但一个长跑的 subagent 可能撞上远端那 30s。
//! 真要收，得把本命令改成**流式**（同 `stream_read_remote_session` 那条 channel 路），
//! 那会改它对前端的返回形状 —— 是另一件事，不在本件里顺手做。

use crate::messages::JsonlRecord;
use crate::parser::parse_line;
use std::path::{Path, PathBuf};

#[derive(Debug, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../../src/generated/"))]
pub struct SubagentLoadResult {
    /// 命中的 jsonl 文件路径（用于前端 debug / 状态栏显示）
    pub path: String,
    /// agentId（从文件名 `agent-<id>.jsonl` 提取）
    pub agent_id: String,
    pub records: Vec<JsonlRecord>,
}

/// **两条路唯一的差别**：谁去跑那条一次性查询。
///
/// 本机 = exec 一次 sidecar 拿 stdout；远端 = 经 ssh exec 同一个二进制。
/// 定框 `C1` 逐字「本地 = 不走 ssh 的远端」——⇒ 子命令、参数、解析、挑选**全是同一份**。
enum Backend {
    Local,
    Remote(Box<crate::ssh_source::RemoteConfig>),
}

impl Backend {
    /// `origin` 缺省 / 空串 = 本机；否则按 label 取那台远端的配置。
    fn for_origin(origin: Option<&str>) -> Result<Self, String> {
        match origin.filter(|o| !o.is_empty()) {
            Some(o) => {
                let cfg = crate::remote_history::require_cfg_by_label(o)?;
                Ok(Backend::Remote(Box::new(cfg)))
            }
            None => Ok(Backend::Local),
        }
    }

    /// 报错文案里的「谁」—— 本机 / 哪台远端。
    fn whose(&self) -> String {
        match self {
            Backend::Local => "本机".to_string(),
            Backend::Remote(cfg) => format!("远端 [{}]", cfg.origin_label()),
        }
    }

    /// 跑一条一次性查询。`argv[0]` 是子命令，其余是它的参数。
    ///
    /// 出的是**逐行、已 trim、已剔空行**的输出 —— 两条路形状一致
    /// （远端那条由 `run_list_query` 保证，本机这条在 [`run_local_query`] 里对齐）。
    async fn query(&self, argv: &[&str]) -> Result<Vec<String>, String> {
        match self {
            Backend::Local => run_local_query(argv),
            Backend::Remote(cfg) => {
                // 自由文本（路径）逐个过 `shell_quote`；子命令本身是字面量。
                let mut args = argv[0].to_string();
                for a in &argv[1..] {
                    args.push(' ');
                    args.push_str(&crate::ssh_source::shell_quote(a));
                }
                crate::remote_history::run_list_query(cfg, &args).await
            }
        }
    }
}

/// 本机那条 transport：exec 一次 sidecar 拿 stdout。
///
/// ⚠ 定框 §5：**「后端不在」与「查询失败」不许压成同一句话** ——
/// 前者该提示用户装/起后端，后者该把原因原样端出来。
fn run_local_query(argv: &[&str]) -> Result<Vec<String>, String> {
    use crate::backend::observe::local_query::{run_query, QueryOutcome};
    match run_query(
        env!("CCM_TARGET_TRIPLE"),
        argv,
        &*crate::spawn_managed::local_backend_one_shot_query(),
    ) {
        QueryOutcome::Ok(stdout) => Ok(nonempty_lines(&stdout)),
        QueryOutcome::NoBackend(reason) => Err(format!("本机后端不在：{reason}")),
        QueryOutcome::Failed { code, stderr } => {
            let sub = argv[0];
            let msg = stderr.trim();
            Err(format!("本机后端 {sub} 查询失败（退出码 {code:?}）：{msg}"))
        }
    }
}

/// 与 `run_list_query` 的出参形状对齐：逐行、trim 过、空行剔掉。
fn nonempty_lines(stdout: &str) -> Vec<String> {
    stdout
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(str::to_string)
        .collect()
}

#[tauri::command]
pub async fn load_subagent(
    parent_jsonl_path: String,
    description: String,
    tool_use_timestamp: String,
    origin: Option<String>,
) -> Result<SubagentLoadResult, String> {
    let backend = Backend::for_origin(origin.as_deref())?;
    // 深度防御（与 `stream_read_remote_session` 同一条纪律）：路径来自前端，本侧先做廉价校验；
    // 真正的越权读由后端的 `fence_under_projects` 兜底。
    // ⚠ `K-R94` 起这道校验**两条路都过** —— 改前只有远端那条有，而「同一个入参、两种把关」
    // 正是 `KR94D3` 说的那种两条路不一致。
    if parent_jsonl_path.contains("..") || !parent_jsonl_path.ends_with(".jsonl") {
        return Err(format!("非法父会话路径: {parent_jsonl_path}"));
    }

    let list_argv = ["--list-subagents", parent_jsonl_path.as_str()];
    let listing = backend.query(&list_argv).await?;
    let Some(picked) = choose_subagent(&listing, &description, &tool_use_timestamp) else {
        let whose = backend.whose();
        return Err(format!(
            "{whose} 上没有 description={description:?} 的 subagent"
        ));
    };
    let picked_str = picked.to_string_lossy().into_owned();
    let agent_id = extract_agent_id(&picked).unwrap_or_default();

    // 内容走**既有的** `--read-session`（原样透传字节，本侧走既有解析）：subagent jsonl 就在
    // `<records 根>/<slug>/<sid>/subagents/` 下，那条围栏本来就放行它，不用新造读口。
    let read_argv = ["--read-session", picked_str.as_str()];
    let raw = backend.query(&read_argv).await?;
    let mut records = Vec::new();
    for line in &raw {
        match parse_line(line) {
            Ok(Some(rec)) => records.push(rec),
            Ok(None) => {}
            Err(e) => tracing::warn!("subagent parse skip: {e}"),
        }
    }

    Ok(SubagentLoadResult {
        path: picked_str,
        agent_id,
        records,
    })
}

/// 从**后端给的候选清单**里挑一个。
///
/// ★★ **纯函数：不碰文件系统，也不知道自己在为哪条路服务。**
/// 入参是 `--list-subagents` 的输出行（每行 `{path, description, timestamp}`）。
/// ⇒「候选集由后端决定」在这里是**类型上的**事实：这个函数没有别的地方能变出候选来。
///
/// ★ **筛选留在本侧**，不在 daemon（`C1`：挑选逻辑只准有一份；daemon 侧那半由
/// `the_daemon_never_matches_or_ranks_subagents` 钉住它只列、不挑）。
fn choose_subagent(
    listing: &[String],
    description: &str,
    tool_use_timestamp: &str,
) -> Option<PathBuf> {
    let mut metas: Vec<(PathBuf, Option<String>)> = Vec::new();
    for line in listing {
        let Ok(v) = serde_json::from_str::<serde_json::Value>(line.trim()) else {
            continue;
        };
        if v.get("description").and_then(|d| d.as_str()) != Some(description) {
            continue;
        }
        let Some(path) = v.get("path").and_then(|p| p.as_str()) else {
            continue;
        };
        // 时间戳拿不到就是 `None` —— 后端给 `null` 与**根本没这个键**落到同一档。
        let ts = v
            .get("timestamp")
            .and_then(|t| t.as_str())
            .map(str::to_string);
        metas.push((PathBuf::from(path), ts));
    }
    if metas.is_empty() {
        return None;
    }
    Some(pick_closest(metas, tool_use_timestamp))
}

/// 多个 description 相同的候选 → 取首行 timestamp 与 `tool_use_timestamp` 差距最小的。
///
/// ★★ **P7c-1：它吃 `(路径, 时间戳)` 对，不再自己去读文件。**
///
/// 原来它拿 `Vec<PathBuf>` 并在内部读首行时间戳 —— 那样**只有本机能用**
/// （远端的文件不在本机文件系统上）。改成吃对之后，本机与远端喂给它的是**同一种东西**，
/// 挑选逻辑因此只有一份 —— 定框 `C1` 要的正是这个。
///
/// ⚠ `K-R94` **刻意没动它一个字**（射程 `§0b`）：本件收的是「找」那一半，
/// 已经收好的这一半再动一次就是把它拆开。今天它只剩**一个**调用点（[`choose_subagent`]）。
///
/// **缺时间戳那一档的处置逐字写在这里**：拿不到（`None`），或 `tool_use_timestamp`
/// 自己解析不出来 ⇒ 排序键取 `i64::MAX`；而 `sort_by_key` 是**稳定**排序 ⇒
/// 全缺时保持后端给的次序、取第一条，部分缺时**有时间戳的一定排在前面**。
/// 它**不报错** —— 两条路同走这一档（`KR94D3` ②）。
fn pick_closest(
    mut candidates: Vec<(PathBuf, Option<String>)>,
    tool_use_timestamp: &str,
) -> PathBuf {
    if candidates.len() == 1 {
        return candidates.remove(0).0;
    }
    let target = crate::utils::parse_iso8601_ms(tool_use_timestamp);
    candidates.sort_by_key(|(_, ts)| {
        let first_ts = ts.as_deref().and_then(crate::utils::parse_iso8601_ms);
        match (target, first_ts) {
            (Some(t), Some(f)) => (f - t).abs(),
            _ => i64::MAX,
        }
    });
    candidates.into_iter().next().unwrap().0
}

// P3 归并：parse_ts_ms 已搬到 utils::parse_iso8601_ms（多处复用）。
// 跨月 / 跨年 / 闰年单调性由 utils::days_from_civil 保证（utils 自带回归测试）。

fn extract_agent_id(jsonl_path: &Path) -> Option<String> {
    let stem = jsonl_path.file_stem()?.to_str()?;
    // 文件名形如 agent-<hash>
    stem.strip_prefix("agent-").map(str::to_string)
}

#[cfg(test)]
#[path = "../../../tests/bridge/subagent_tests.rs"]
mod tests;
