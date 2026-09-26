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
//! 改前两条路只共用了「挑」那一半（`pick_closest`）：远端让后端列候选，
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
//! 远端那条（〔C4d〕今天是长连接的帧命令，`frame_query` 按行那一档 30s 期限 ＋ 单帧上限）有期限与上限；本机那条
//! （`local_query::run_query`）**两样都没有** —— 那一层刻意把超时留给调用方（见它自己的头注）。
//! subagent 通常是短命的侧任务、文件很小，但一个长跑的 subagent 可能撞上远端那 30s。
//! 真要收，得把本命令改成**流式**（同 `history·rs::stream_read_session_jsonl` 那条 channel 路），
//! 那会改它对前端的返回形状 —— 是另一件事，不在本件里顺手做。

use crate::copy_table::copy_text;
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
/// 本机 = exec 一次本机后端拿 stdout；远端 = 经 ssh exec 同一个二进制。
/// 定框 `C1` 逐字「本地 = 不走 ssh 的远端」——⇒ 子命令、参数、解析、挑选**全是同一份**。
///
/// 〔`设计/10` 骨架 · 子步 3〕它从本模块私有升成 `pub(crate)`：骨架索引与按偏移取正文
/// （`session_skeleton.rs`）问的是**同一个问题**「这条查询谁去跑」—— 另写一份本机/远端分流
/// 就是 `K33` 说的第二处实现。**只改了可见性，一行行为没动。**
pub(crate) enum Backend {
    Local,
    Remote(Box<crate::ssh_source::RemoteConfig>),
}

impl Backend {
    /// 分过本机之后选后端：本机走本机后端，远端按 label 取那台远端的配置。
    ///
    /// # 🔴 〔`设计/05 §8` 步 2，2026-09-20〕入参从 `Option<&str>` 换成 `Route`
    ///
    /// 上一拍这里逐字写着「`origin` 缺省 / **空串** = 本机」—— 两个「没说」的值
    /// 都被**悄悄**归进了本机，而 `INVARIANTS §40` 逐字「本地 ＝ 不走 ssh 的远端」
    /// ⇒ 本机是一个**具名**的 origin，「没说」不是它。
    /// 那种归法不报错：调用方什么都不送，命令去动了本机的文件。
    ///
    /// ⇒ 「没说」从此在到这里**之前**就被拦掉，两道闸各一处：
    /// 线上 `null` 由 `Origin` 的 `Deserialize` 拒（构造不出来）·
    /// 空白名由 [`crate::origin::Origin::route`] 拒（拒的那句话点名命令）。
    /// 本函数因此只收**已经分过本机**的 [`crate::origin::Route`] —— 它只有两个变体，
    /// 「没说」在类型上到不了这里。
    ///
    /// ⚠ 分流点仍然**恰好一处**（`subagent_tests.rs` 钉着 `fn for_origin(` 的处数）。
    pub(crate) fn for_origin(route: crate::origin::Route<'_>) -> Result<Self, String> {
        match route {
            crate::origin::Route::Local => Ok(Backend::Local),
            crate::origin::Route::Remote(host) => {
                let cfg = crate::remote_history::require_cfg_by_label(host)?;
                Ok(Backend::Remote(Box::new(cfg)))
            }
        }
    }

    /// 报错文案里的「谁」—— 本机 / 哪台远端。
    pub(crate) fn whose(&self) -> String {
        match self {
            Backend::Local => copy_text("rsSubagent.whose.local", &[]),
            Backend::Remote(cfg) => copy_text(
                "rsSubagent.whose.remote",
                &[("machine", &(cfg.origin_label()).to_string())],
            ),
        }
    }

    /// 跑一条一次性查询。`argv[0]` 是子命令，其余是它的参数。
    ///
    /// 出的是**逐行、已 trim、已剔空行**的输出 —— 两条路形状一致
    /// （远端那条由 `frame_query` 的按行那一档保证，本机这条在 [`run_local_query`] 里对齐）。
    ///
    /// 〔C2 · SE1 欠账〕失败带**种类**（[`QueryFailure`]）：调用方据种类决定「还要不要再要」，
    /// 不解析 `message` 的文字。种类**在失败发生的那一层当场定**，不事后按文字猜：
    /// 本机那条看退出码与后端自己印的那句 `unknown argument` · 远端拨号那条看首行是不是 hello ·
    /// 帧面那条先问长连接认不认这条命令。
    pub(crate) async fn query(&self, argv: &[&str]) -> Result<Vec<String>, QueryError> {
        match self {
            Backend::Local => run_local_query(argv),
            Backend::Remote(cfg) => {
                // 〔`C1` · 09-24〕认得的形状走长连接（`frame_query::route_argv`）；
                // 认不出的当场说（下面那一支；〔C4d〕逐次拨号那条路删了）。
                if let Some(route) = crate::backend::control::frame_query::route_argv(argv) {
                    let origin = crate::origin::Origin(cfg.origin_label());
                    // 长连接在、却不认这条帧命令 ⇒ 对面的后端比这条查询老（结构性，再要也一样）。
                    if crate::backend::control::frame_query::refuses(&origin, &route) {
                        return Err(QueryError::old_backend(copy_text(
                            "rsSubagent.query.tooOld",
                            &[("machine", &(cfg.origin_label()).to_string())],
                        )));
                    }
                    return crate::backend::control::frame_query::run_routed(&origin, route)
                        .await
                        .map_err(QueryError::transport);
                }
                // 〔C4d · 第四波 4B〕认不出的形状从前落到逐次拨号那条路（`run_list_query`〔散文墓碑〕），而那条路的
                //   放行表 C4c 起就是空的 ⇒ 结局本来就是被拒。主会话 09-25 裁删那条路：这里**当场说**，不拨号、不回落。
                //   这是本程序的 bug（调用方造了一条没上帧面的查询），不是远端的问题 ⇒ 结构性，再要也一样。
                Err(QueryError::transport(copy_text(
                    "rsSubagent.query.noFrameCmd",
                    &[
                        (
                            "argv",
                            &(argv.first().copied().unwrap_or_default()).to_string(),
                        ),
                        ("machine", &(cfg.origin_label()).to_string()),
                    ],
                )))
            }
        }
    }
}

/// 〔C2 · SE1 欠账〕一次性查询**要不到**的种类。
///
/// | 种类 | 在哪一层定 | 含义 |
/// |---|---|---|
/// | `OldBackend` | 本机：退出码 2 ＋ 后端自己印的 `unknown argument: <子命令>` · 帧面：长连接不认这条命令 | **结构性**：同一台后端再要一次还是这样 |
/// | `Transport` | 其余：起不了本机后端 · 超时 · 没有控制通道 · 后端退出码非 0（非上面那一形）· 没有帧命令的查询 | **瞬时**：下一次触发再要 |
///
/// 〔C4d · 第四波 4B〕原先还有一档 `Truncated`（「远端拨号：单行超上限被整行拒收」）与「远端拨号：首行是 hello」那一形 ——
/// 两样都只在逐次拨号那条路上产出，那条路删了，它们随之没有产出者 ⇒ 摘掉（帧面的「装不下」是后端的 `too_large` 明拒，走 `Transport`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum QueryFailure {
    OldBackend,
    Transport,
}

/// 一次性查询的失败：种类 ＋ 给人看的原因。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct QueryError {
    pub(crate) kind: QueryFailure,
    pub(crate) message: String,
}

impl QueryError {
    pub(crate) fn old_backend(message: String) -> Self {
        Self {
            kind: QueryFailure::OldBackend,
            message,
        }
    }
    pub(crate) fn transport(message: String) -> Self {
        Self {
            kind: QueryFailure::Transport,
            message,
        }
    }
}

impl std::fmt::Display for QueryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

/// 只要文字的调用方（`?` 进 `Result<_, String>`）照旧拿到原来那句话。
impl From<QueryError> for String {
    fn from(e: QueryError) -> String {
        e.message
    }
}

/// 本机后端对「不认的子命令」印的那一句的前缀（后端 `main.rs` 一次性查询臂：
/// `cc-monitor-backend query error: unknown argument: <arg>` ＋ 退出 2）。
/// 判据从后端源码里现抠这一串，对拍两侧（`subagent_tests.rs`）。
pub(crate) const UNKNOWN_ARGUMENT: &str = "unknown argument: ";

/// 本机退出码 ＋ stderr ⇒ 种类。**纯函数**，判据直接喂。
pub(crate) fn local_failure_kind(
    code: Option<i32>,
    stderr: &str,
    subcommand: &str,
) -> QueryFailure {
    let unknown = format!("{UNKNOWN_ARGUMENT}{subcommand}");
    if code == Some(2) && stderr.lines().any(|l| l.trim_end().ends_with(&unknown)) {
        QueryFailure::OldBackend
    } else {
        QueryFailure::Transport
    }
}

/// 本机那条 transport：exec 一次本机后端拿 stdout。
///
/// ⚠ 定框 §5：**「后端不在」与「查询失败」不许压成同一句话** ——
/// 前者该提示用户装/起后端，后者该把原因原样端出来。
fn run_local_query(argv: &[&str]) -> Result<Vec<String>, QueryError> {
    use crate::backend::observe::local_query::{run_query, QueryOutcome};
    match run_query(
        env!("CCM_TARGET_TRIPLE"),
        argv,
        &*crate::spawn_managed::local_backend_one_shot_query(),
    ) {
        QueryOutcome::Ok(stdout) => Ok(nonempty_lines(&stdout)),
        QueryOutcome::NoBackend(reason) => Err(QueryError::transport(copy_text(
            "rsSubagent.localQuery.noBackend",
            &[("reason", &reason.to_string())],
        ))),
        QueryOutcome::Failed { code, stderr } => {
            let sub = argv[0];
            let msg = stderr.trim();
            Err(QueryError {
                kind: local_failure_kind(code, &stderr, sub),
                message: copy_text(
                    "rsSubagent.localQuery.failed",
                    &[
                        ("sub", &sub.to_string()),
                        ("code", &format!("{:?}", code)),
                        ("msg", &msg.to_string()),
                    ],
                ),
            })
        }
    }
}

/// 与帧面按行那一档（`frame_query::lines`）的出参形状对齐：逐行、trim 过、空行剔掉。
fn nonempty_lines(stdout: &str) -> Vec<String> {
    stdout
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(str::to_string)
        .collect()
}

/// ⚠ 本机 ＝ `Origin::local()`（线上 `"<local>"`），**不是 `null`、不是空串**
/// 〔`设计/05 §8` 步 2〕：上一拍这一条收 `Option<String>`，`None` 与 `""` 都被当成本机
/// ⇒ 「省掉 origin」与「本机」在线上长得一模一样。现在它收 `Origin`，
/// 而「没说」在 `Origin` 里已经没有任何表示（理由逐条写在 `origin.rs` 头注）。
#[tauri::command]
pub async fn load_subagent(
    parent_jsonl_path: String,
    description: String,
    tool_use_timestamp: String,
    origin: crate::origin::Origin,
) -> Result<SubagentLoadResult, String> {
    // 分本机这一步**只有一个住址**（`origin_tests.rs` 的 `F` 组两向钉着：
    // 吃 `Origin` 的命令 == 体里经 `.route(` 分本机的命令）。
    let backend = Backend::for_origin(origin.route("load_subagent")?)?;
    // 深度防御（与 `history·rs::stream_read_session_jsonl` 同一条纪律）：路径来自前端，本侧先做廉价校验；
    // 真正的越权读由后端的 `fence_under_projects` 兜底。
    // ⚠ `K-R94` 起这道校验**两条路都过** —— 改前只有远端那条有，而「同一个入参、两种把关」
    // 正是 `KR94D3` 说的那种两条路不一致。
    if parent_jsonl_path.contains("..") || !parent_jsonl_path.ends_with(".jsonl") {
        return Err(copy_text(
            "rsSubagent.load.badParentPath",
            &[("path", &parent_jsonl_path.to_string())],
        ));
    }

    let list_argv = ["--list-subagents", parent_jsonl_path.as_str()];
    let listing = backend.query(&list_argv).await?;
    let Some(picked) = choose_subagent(&listing, &description, &tool_use_timestamp) else {
        let whose = backend.whose();
        return Err(copy_text(
            "rsSubagent.load.notFound",
            &[
                ("whose", &whose.to_string()),
                ("description", &format!("{:?}", description)),
            ],
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
        // 〔ST3〕这些行是 `origin` 那台后端给的 ⇒ 看不懂的记在那台名下。
        match parse_line(&origin, line) {
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
/// ★ **筛选留在本侧**，不在后端（`C1`：挑选逻辑只准有一份；backend 侧那半由
/// `the_backend_never_matches_or_ranks_subagents` 钉住它只列、不挑）。
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
