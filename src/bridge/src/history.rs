//! 历史会话浏览器后端：扫描 `<claude_dir>/projects/**/*.jsonl`，提供
//! list / delete / metadata 增删改 / resume / **F62 从某轮建分支** IPC 命令。
//!
//! ## 与 watcher 的关系
//!
//! watcher.rs / event_replay.rs 只关心**活跃 session**（PID 还在跑）。本模块是
//! 用户**显式触发**的拉取（点"历史"按钮才扫一次），不监听变化，不维护内存索引。
//!
//! ## 两级懒加载
//!
//! 历史浏览器项目组默认折叠，因此后端分两级 IPC：
//!  1. `list_history_projects` —— 项目级元数据，**不读 jsonl 内容**，每项目仅 1 个
//!     1-line read 拿 cwd + 文件 stat 拿 mtime + 数 dir entries。500 个项目 < 50ms
//!  2. `stream_history_sessions_in_project` —— 用户展开某项目时才调，流式 Channel
//!     边解析边发，前端逐条增量渲染。（v2.2 起取代非流式 `list_history_sessions_in_project`）
//!
//! ## 用户元数据
//!
//! star / 重命名 / 隐藏 这些信息**不能改 jsonl**（Claude Code 的数据保持零侵入），
//! 单独存到 `<monitor_data_dir>/history-metadata.json`，结构见 `HistoryMetadata`。
//!
//! ## 物理删除
//!
//! 用户明确选了"物理删除 .jsonl 文件"。前端二次确认后调 `delete_history_session`，
//! 直接 `std::fs::remove_file`。Claude Code 自己也不再能 resume 这个会话。

use crate::backend::observe::local_query::{self, QueryOutcome};
use crate::messages::{ApiMessage, JsonlRecord};
use crate::parser::parse_line;
use crate::paths;
use crate::remote_history::{
    history_project_from_row, log_unknown_reasons, Counted, LivenessOracle, ProjectCounts,
};
use crate::session_map::SessionMap;
use crate::utils::{now_ms, systime_to_ms};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::sync::Arc;

// === 数据结构 ===

/// 项目级元数据 —— 首次 list 时返回，**不含**任何 session 内容。
/// P1.2：全字段 camelCase wire，前端 TS interface 字段名一致。
#[derive(Debug, Serialize, Clone)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../../src/generated/"))]
#[serde(rename_all = "camelCase")]
pub struct HistoryProject {
    /// 真实工作目录路径（从某个 jsonl 的首条 user 消息的 cwd 取）
    pub project_path: String,
    /// 项目名 = cwd 最后一段
    pub project_name: String,
    /// 编码后的目录名（位于 `<claude_dir>/projects/` 之下），前端调用
    /// `stream_history_sessions_in_project` 时传回来作 key
    pub project_dir: String,
    pub session_count: u32,
    /// `K-R92`：**`None` = 不知道**（没查 / 查不了），`Some(0)` = 查过了，真的一个都没有。
    /// 这两件事在 09-12 之前是同一个 `0` —— 病灶与全部论证见本文件
    /// [`the_three_counts_can_say_i_do_not_know`] 与 `remote_history::Counted`。
    pub starred_count: Option<u32>,
    /// 同上：`None` = 不知道，`Some(0)` = 查过了是 0。
    pub hidden_count: Option<u32>,
    /// 该项目下任意 jsonl 文件的最大 mtime（ms）
    // **C03 大整数策略**：量纲是**毫秒时间戳**——2^53-1 ms ≈ **28.5 万年**。
    #[cfg_attr(test, ts(type = "number"))]
    pub last_activity: i64,
    /// 该项目下是否有 session 当前还活着。**`None` = 这条路上答不了**
    /// （远端没有判活真相源 · Codex 无 pidfile），**不是**「没有活会话」。
    pub has_live: Option<bool>,
    /// issue #16：数据来源。None=本地；Some(host)=远端（前端组头显示 [host] 徽标，
    /// 展开时改调 stream_remote_history_sessions）。
    #[cfg_attr(test, ts(optional))]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub origin: Option<String>,
}

#[derive(Debug, Serialize, Clone)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../../src/generated/"))]
#[serde(rename_all = "camelCase")]
pub struct HistorySessionEntry {
    pub session_id: String,
    pub project_path: String,
    pub project_name: String,
    pub ai_title: Option<String>,
    pub first_user_excerpt: String,
    // **C03 大整数策略**：量纲是**毫秒时间戳**——2^53-1 ms ≈ **28.5 万年**。
    #[cfg_attr(test, ts(type = "number"))]
    pub started_at: i64,
    // **C03 大整数策略**：量纲是**毫秒时间戳**——2^53-1 ms ≈ **28.5 万年**。
    #[cfg_attr(test, ts(type = "number"))]
    pub updated_at: i64,
    pub jsonl_path: String,
    /// `K-R92`：**`None` = 这条路上答不了活状态**（远端 / Codex），`Some(false)` = 查过了，没活。
    pub is_live: Option<bool>,
    pub message_count_approx: u32,
    /// Batch11-F32：CC 后台分身会话（⚙ 徽标——防 resume 误选克隆）。
    pub is_bg: bool,
    // 用户元数据合并进来，前端一次拿全
    pub starred: bool,
    pub custom_title: Option<String>,
    pub hidden: bool,
    // issue #12: fork 关系。若本 session 是从某 parent session 用 /branch 分叉来的，
    // 这两个字段记 parent session 的 id 和被 fork 处的 messageUuid。
    #[cfg_attr(test, ts(optional))]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub forked_from_session_id: Option<String>,
    #[cfg_attr(test, ts(optional))]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub forked_from_message_uuid: Option<String>,
    /// issue #16：数据来源。None=本地；Some(host)=远端（前端据此禁用 resume/delete、
    /// 查看走 stream_read_remote_session）。
    #[cfg_attr(test, ts(optional))]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub origin: Option<String>,
}

/// `K-R92`：三态排序档 —— **确定有(2) > 不知道(1) > 确定没有(0)**。
///
/// # 🔴 为什么不直接 `Option` 的派生序
///
/// `Option<bool>` 的派生序是 `None < Some(false) < Some(true)`，按它降序排，
/// 「不知道」会被排到「确定没有活会话」**后面** —— 那是一句断言：
/// 「这个项目比一个已经确定没活的项目更不像活着」。**说不出口的话不许说。**
/// 「不知道」既不许冒充「活着」抢到最前，也不许被当成「确定没活」压到最后 ⇒ 它自成一档，在中间。
///
/// ⚠ 这一层与前端 `src/views/counted.ts` 的 `liveRank` / `starRank` **是同一套档位**，
/// 两侧各有判据钉着（本文件 `unknown_is_its_own_bucket_when_sorting` ·
/// `tests/views/counted.vitest.ts`）。
pub(crate) fn live_rank(v: Option<bool>) -> u8 {
    match v {
        Some(true) => 2,
        None => 1,
        Some(false) => 0,
    }
}

/// 同 [`live_rank`]：**有星标(2) > 不知道(1) > 查过了一个都没有(0)**。
pub(crate) fn star_rank(v: Option<u32>) -> u8 {
    match v {
        Some(n) if n > 0 => 2,
        None => 1,
        Some(_) => 0,
    }
}

#[derive(Debug, Default, Serialize, Deserialize, Clone)]
pub struct HistoryMetadata {
    #[serde(default)]
    pub version: u32,
    #[serde(default)]
    pub entries: HashMap<String, EntryMetadata>,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../../src/generated/"))]
pub struct EntryMetadata {
    #[serde(default)]
    pub starred: bool,
    #[serde(default, rename = "customTitle", alias = "custom_title")]
    pub custom_title: Option<String>,
    #[serde(default)]
    pub hidden: bool,
    // **C03 大整数策略**：量纲是**毫秒时间戳**——2^53-1 ms ≈ **28.5 万年**。
    #[cfg_attr(test, ts(type = "number"))]
    #[serde(default, rename = "updatedAt", alias = "updated_at")]
    pub updated_at: i64,
    /// A4：上次用本工具（cc-monitor）起该会话时选的账号名（DESIGN §3 源②）。
    /// live 探测不到时会话徽章回退用它。None = 从未用本工具带账号起过（旧文件缺此字段亦为 None）。
    #[serde(default, rename = "lastAccount", alias = "last_account")]
    pub last_account: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct MetadataPatch {
    #[serde(default)]
    pub starred: Option<bool>,
    #[serde(default, rename = "customTitle", alias = "custom_title")]
    pub custom_title: Option<Option<String>>,
    #[serde(default)]
    pub hidden: Option<bool>,
    // plain serde default（非 double_option）：缺键 / JSON `null` 都 → None（不改）；
    // 只有给字符串才 → Some(Some(s))。**JSON `null` 到不了 Some(None)**——清空走"空/空白串
    // → Some(Some("")) → update 里 filter 掉"（见 update_history_metadata），不靠 null。
    #[serde(default, rename = "lastAccount", alias = "last_account")]
    pub last_account: Option<Option<String>>,
}

// === IPC 命令 ===

/// 本机这条路的**判活真相源**：`SessionMap` 认本机进程的 pid ⇒ 它**答得出真值**。
///
/// 与远端那个绑定（`remote_history::NoLivenessOracleYet`，恒答「不知道」）是同一个接口的
/// 两个实现 —— 「谁答得出、谁答不出」因此在类型上说得清，而不是散在两条路的函数体里。
pub(crate) struct SessionMapLiveness(pub(crate) Arc<SessionMap>);

impl LivenessOracle for SessionMapLiveness {
    fn is_live(&self, _origin: &str, sid: &str) -> Counted<bool> {
        // 本机有真相源 ⇒ 一律 `Known`。「不知道」那一档在这个绑定里恒不出现。
        Counted::Known(self.0.is_session_active(sid))
    }
}

/// 本机项目列表的**本体**；「去问本机后端」这件事**是参数**。
///
/// # 🔴 为什么查询要作为参数传进来 —— `KR97D3` 判的那个可数的事实
///
/// 同 `KR83D3` 的口径：**别判「代码里有没有 for 循环」**（那判的是写法），
/// 要判**「一次调用里 spawn 了几次」**。真 sidecar 在红线内跑不了 ⇒ 把 spawn 那一步做成入参，
/// 判据就能拿一个**会计数的假查询**喂进来，直接数出「N 个项目 ⇒ 查询被调了几次」。
///
/// 失效方向（本函数存在的理由）：一旦有人为了拿 star/hide 而在下面那个循环里补一句
/// `--list-sessions`，计数当场从 `1` 涨成 `1 + 项目数`，判据红。
///
/// # 三态怎么落地（定框 §5）
///
/// 「后端不在」与「后端在但这条查询失败了」**分开报**：前者是今天这台机器上没有对侧
/// （该提示装 / 该回落），后者是有对侧但它说了不），压成一个 `Err` 就是让上层猜。
/// 本函数把两者都折成 `Err(带身份的一句话)` 交给前端 toast，**但话不一样** ——
/// 与 `local_accounts::list_local_session_accounts` 那条路同形。
pub(crate) fn local_projects_via<Q>(
    query: Q,
    metadata: &HistoryMetadata,
    liveness: &dyn LivenessOracle,
) -> Result<Vec<(HistoryProject, ProjectCounts)>, String>
where
    Q: Fn(&[&str]) -> QueryOutcome,
{
    let stdout = match query(&["--list-projects"]) {
        QueryOutcome::Ok(s) => s,
        // 诚实降级：把「后端不在」原样交给用户（定框 §5），不假装 0 个项目 ——
        // 「一个历史项目都没有」与「今天这台机器上没有对侧」是完全不同的处境。
        QueryOutcome::NoBackend(reason) => {
            return Err(format!("本机后端不在，拿不到历史项目列表：{reason}"));
        }
        QueryOutcome::Failed { code, stderr } => {
            return Err(format!(
                "本机后端的项目列表查询失败（退出码 {code:?}）：{}",
                stderr.trim()
            ));
        }
    };
    let mut rows = Vec::new();
    for line in stdout.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let v: serde_json::Value = match serde_json::from_str(line) {
            Ok(v) => v,
            // 单行坏了不该毁掉整张列表（同远端那条路，逐行跳过）。
            Err(e) => {
                tracing::warn!("本机 --list-projects 行解析失败（跳过）: {e}: {line}");
                continue;
            }
        };
        // ★ 与远端 fan-out **同一份**「行 → 项目」的解释（`K-R97` 收成一处）。
        //   `origin = None` ⇒ 前端看到的仍是「本地项目」。
        if let Some(row) = history_project_from_row(&v, metadata, None, liveness) {
            rows.push(row);
        }
    }
    log_unknown_reasons("本机项目列表", &rows);
    Ok(rows)
}

/// 项目级元数据列表 —— **不读 jsonl 内容**。首次打开历史浏览器时调。
///
/// # 🔴〔`K-R97` 09-12〕它**不再自己遍历 records 根**，改问本机后端
///
/// 从前这里 `resolve_claude_dir()` + `records_dir()` + 逐项目 `read_dir`（`analyze_project_dir`，〔散文墓碑〕已删），
/// 而远端那条路早就是「问那台机器的后端要 `--list-projects`」。⇒ 同一个问题两份实现
/// （`K-R54` 表第 12 行），且本机那份必然与后端那份漂移。
/// 今天两条路**吃同一条查询、同一份解释**（`remote_history::history_project_from_row`），
/// 差别只剩传输：远端多一跳 SSH，本机 exec 一次 sidecar（`backend::observe::local_query`）。
///
/// ⚠ **如实记诚实边界，别读成「完全等价」**：
/// - **`project_dir` 的形状变了**：从前是**绝对路径**，现在是后端给的**编码目录名**
///   （与远端那条路逐字同形）。前端只把它原样传回，
///   而 `stream_history_sessions_in_project` 那侧已经跟着改成「按名字在 records 根下解析 + 围栏」。
/// - **cwd 提取窗口从 30 行变成 40 行、且不再只认 `user` 记录** —— 那是后端那份的口径。
///   两份实现从前**故意不一致**并被一条判据钉着（`ROADMAP §5`）；本件把 monitor 那份删了，
///   分歧随之消失（不是「对齐」，是**只剩一处**）。
/// - **`CLAUDE_CONFIG_DIR` 指向不存在的路径时**：monitor 从前会回落到 `~/.claude`，
///   sidecar 不会。极少见，但不是零。
/// - **Codex 那半没动**：后端侧今天没有 codex 的项目枚举（`--list-projects` 只服务 claude），
///   本机仍自己合成（`codex_projects`）—— 那条登记还挂在 `local_read_surface_registry` 上。
///
/// v2.2 (issue #12)：async + spawn_blocking，避免 sync IO 阻塞 Tauri IPC 派发线程。
#[tauri::command]
pub async fn list_history_projects(
    map: tauri::State<'_, Arc<SessionMap>>,
) -> Result<Vec<HistoryProject>, String> {
    let map = map.inner().clone();
    tokio::task::spawn_blocking(move || {
        let started = std::time::Instant::now();
        let metadata = load_metadata().unwrap_or_default();
        let liveness = SessionMapLiveness(map);
        let rows = local_projects_via(
            |args| {
                local_query::run_query(
                    env!("CCM_TARGET_TRIPLE"),
                    args,
                    &*crate::spawn_managed::local_backend_one_shot_query(),
                )
            },
            &metadata,
            &liveness,
        )?;
        let mut out: Vec<HistoryProject> = rows.into_iter().map(|(p, _)| p).collect();

        // Phase 2 F1a-3：追加 Codex 合成项目（按 session_meta.cwd 内存分组；Codex 未启用 → 空、零回归）。
        out.extend(codex_projects());

        // live → starred → last_activity desc（同 UI 顺序，前端可再排但默认就是这个）
        out.sort_by(|a, b| {
            live_rank(b.has_live)
                .cmp(&live_rank(a.has_live))
                .then_with(|| star_rank(b.starred_count).cmp(&star_rank(a.starred_count)))
                .then(b.last_activity.cmp(&a.last_activity))
        });

        tracing::info!(
            "list_history_projects: {} projects in {}ms（经本机后端）",
            out.len(),
            started.elapsed().as_millis()
        );
        Ok(out)
    })
    .await
    .map_err(|e| format!("spawn_blocking join: {e}"))?
}

// ─── Phase 2 F1a-3：Codex 历史枚举（Codex 无 `projects/<cwd>` 目录 → 按 session_meta.cwd 内存分组成
// 合成「项目」，塞进现有 HistoryProject shape → 前端零改、Codex 会话入列）───

/// Codex 一个会话的 list 元信息。〔`设计/50`：`pub(crate)` 原先是为用量那一轴复用枚举开的，
/// 那一轴整轴退役了 —— 可见性没跟着收窄，如实记在这里。〕
pub(crate) struct CodexSessionInfo {
    pub(crate) sid: String,
    pub(crate) path: PathBuf,
    /// session_meta.cwd（分组键；缺 → "" → 归「(codex)」组）。
    pub(crate) cwd: String,
    mtime_ms: i64,
}

/// 枚举本机 Codex 会话：walk `<codex_root>/sessions` 日期树 `rollout-*.jsonl`，读**首行** session_meta
/// 取 cwd。Codex 未启用（无 `~/.codex/sessions`）→ 空 vec（零回归）。〔`pub(crate)` 的来历同上。〕
pub(crate) fn enumerate_codex_sessions() -> Vec<CodexSessionInfo> {
    use crate::adapter::AgentKind;
    let Some(root) = crate::adapter::for_kind(AgentKind::Codex).data_root() else {
        return Vec::new();
    };
    let sessions_dir = crate::adapter::records_dir_for(AgentKind::Codex, &root);
    if !sessions_dir.is_dir() {
        return Vec::new();
    }
    let layout = crate::adapter::for_kind(AgentKind::Codex).layout();
    let mut out = Vec::new();
    for entry in walkdir::WalkDir::new(&sessions_dir)
        .into_iter()
        .filter_map(Result::ok)
    {
        let p = entry.path();
        if !p.is_file() || !crate::adapter::has_record_ext(p) {
            continue;
        }
        // sid 从 `rollout-<ts>-<uuid>` 文件名末 UUID；非此命名 → 跳（非 Codex 会话文件）。
        let Some(sid) = crate::adapter::session_id_from_path_with(layout, p) else {
            continue;
        };
        out.push(CodexSessionInfo {
            sid,
            path: p.to_path_buf(),
            cwd: read_codex_session_cwd(p),
            mtime_ms: file_mtime_ms(p),
        });
    }
    out
}

/// 读 Codex rollout **首行**（session_meta 是首条记录）→ cwd。缺/坏 → ""（归「(codex)」组）。
fn read_codex_session_cwd(p: &Path) -> String {
    let Ok(f) = File::open(p) else {
        return String::new();
    };
    let mut line = String::new();
    use std::io::BufRead;
    if BufReader::new(f).read_line(&mut line).is_err() {
        return String::new();
    }
    serde_json::from_str::<serde_json::Value>(line.trim())
        .ok()
        .and_then(|v| crate::codex_record::session_meta_cwd(&v).map(str::to_string))
        .unwrap_or_default()
}

fn file_mtime_ms(p: &Path) -> i64 {
    std::fs::metadata(p)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// 合成 Codex 项目（枚举 + 分组）。`project_dir` 键 = `codex:<cwd>`——F1a-3c 的
/// `stream_history_sessions_in_project` 按此前缀识别 Codex 项目 + 还原 cwd。
fn codex_projects() -> Vec<HistoryProject> {
    codex_projects_from(enumerate_codex_sessions())
}

/// 纯：Codex 会话按 cwd 分组 → HistoryProject（供 hermetic 测）。
fn codex_projects_from(sessions: Vec<CodexSessionInfo>) -> Vec<HistoryProject> {
    use std::collections::HashMap;
    let mut groups: HashMap<String, (u32, i64)> = HashMap::new(); // cwd → (count, max_mtime)
    for s in &sessions {
        let e = groups.entry(s.cwd.clone()).or_insert((0, 0));
        e.0 += 1;
        e.1 = e.1.max(s.mtime_ms);
    }
    groups
        .into_iter()
        .map(|(cwd, (count, last))| {
            let project_name = if cwd.is_empty() {
                "(codex)".to_string()
            } else {
                Path::new(&cwd)
                    .file_name()
                    .and_then(|s| s.to_str())
                    .unwrap_or(cwd.as_str())
                    .to_string()
            };
            HistoryProject {
                project_path: cwd.clone(),
                project_name,
                project_dir: format!("codex:{cwd}"),
                session_count: count,
                // `K-R92`：Codex 这条路**没有去数** star/hide（分组只带了 count 与 mtime）——
                // 那是「不知道」，不是「查过了是 0」。写 `Some(0)` 就是把没查说成查过了。
                starred_count: None,
                hidden_count: None,
                last_activity: last,
                // Codex 无 pidfile 判活 = F4 ⇒ **答不了**（`K-R92` 之前这里写死 `false`，
                // 那是一句「这个项目没有活会话」的断言，而根本没人查过）。
                has_live: None,
                origin: None,
            }
        })
        .collect()
}

/// F1a-3c：Codex 会话 → `HistorySessionEntry`（点开走已接 read 路）。元数据（starred/title/hidden）
/// 按 sid 查 `HistoryMetadata`（同 Claude）。started/updated 先 mtime 兜底、count 先 0（F1a MVP）。
fn codex_session_entry(s: &CodexSessionInfo, metadata: &HistoryMetadata) -> HistorySessionEntry {
    let meta = metadata.entries.get(&s.sid).cloned().unwrap_or_default();
    let project_name = if s.cwd.is_empty() {
        "(codex)".to_string()
    } else {
        Path::new(&s.cwd)
            .file_name()
            .and_then(|x| x.to_str())
            .unwrap_or(s.cwd.as_str())
            .to_string()
    };
    HistorySessionEntry {
        session_id: s.sid.clone(),
        project_path: s.cwd.clone(),
        project_name,
        ai_title: None,
        first_user_excerpt: codex_first_user_excerpt(&s.path),
        started_at: s.mtime_ms,
        updated_at: s.mtime_ms,
        jsonl_path: s.path.to_string_lossy().into_owned(),
        // `K-R92`：Codex 判活 = F4（无 pidfile）⇒ **答不了**，不是「没活着」。
        is_live: None,
        message_count_approx: 0,
        is_bg: false,
        starred: meta.starred,
        custom_title: meta.custom_title,
        hidden: meta.hidden,
        forked_from_session_id: None,
        forked_from_message_uuid: None,
        origin: None,
    }
}

/// Codex 会话首条 user message 文本（列表摘要）。读至多 200 行找首个 role=user 的 message；
/// 跳 aterm 坑②：`<environment_context>` 注入上下文是 meta 非真用户输入。截 200 字符。空→""。
fn codex_first_user_excerpt(path: &Path) -> String {
    use std::io::BufRead;
    let Ok(f) = File::open(path) else {
        return String::new();
    };
    for line in BufReader::new(f).lines().map_while(Result::ok).take(200) {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let Ok(v) = serde_json::from_str::<serde_json::Value>(trimmed) else {
            continue;
        };
        if crate::codex_record::classify(&v) == crate::codex_record::CodexRecordKind::Message
            && crate::codex_record::message_role(&v) == Some("user")
        {
            let text = crate::codex_record::unwrap_envelope(&v)
                .and_then(|(_, p)| p.get("content"))
                .map(crate::codex_record::flatten_text)
                .unwrap_or_default();
            let t = text.trim();
            // Phase G 审计修：列表摘要去噪复用渲染路同一 `is_injected_context`（3 标记：environment_context/
            // recommended_plugins/# AGENTS.md instructions）——此前只跳 environment_context、与渲染去噪漂移，
            // 首条是 plugins/AGENTS.md 注入的会话列表预览会露机器注入文本。
            if !t.is_empty() && !crate::codex_record::is_injected_context(t) {
                return t.chars().take(200).collect();
            }
        }
    }
    String::new()
}

/// issue #12: 流式版（取代已删的非流式 `list_history_sessions_in_project`）。
///
/// 用 Tauri 2 `Channel<HistorySessionEntry>` 边解析边发，前端可逐条增量渲染。
/// 收益：大项目（几十个 session × 几 MB jsonl）首条 < 100ms 出现，不再"等齐"。
///
/// 取消：前端 drop channel 引用时 `on_entry.send()` 返 Err → break loop。
/// 因为本 IPC 在 spawn_blocking 里跑同步 IO，立刻终止下次 send 即可释放资源。
///
/// 返回总计 emit 的 entry 数（前端可拿来对账 / 显示进度终值）。
///
/// 〔`K-R97` 09-12〕`project_dir` 的形状变了：**编码目录名**（`codex:<cwd>` 那支除外），
/// 不再是绝对路径 —— 列表那条路改问本机后端之后，`HistoryProject::project_dir`
/// 带回来的就是名字，与远端那条路（`stream_remote_history_sessions`）逐字同形。
#[tauri::command]
pub async fn stream_history_sessions_in_project(
    project_dir: String,
    on_entry: tauri::ipc::Channel<HistorySessionEntry>,
    map: tauri::State<'_, Arc<SessionMap>>,
) -> Result<u32, String> {
    let map = map.inner().clone();
    tokio::task::spawn_blocking(move || {
        let started = std::time::Instant::now();
        // Phase 2 F1a-3c：Codex 合成项目（键 `codex:<cwd>`）→ 枚举 + 过滤该 cwd 列会话；点开走
        // 已接的 read 路（stream_read_session_jsonl 多 kind）。Claude 项目（非 codex: 前缀）走原路（零回归）。
        if let Some(cwd) = project_dir.strip_prefix("codex:") {
            let metadata = load_metadata().unwrap_or_default();
            let mut count = 0u32;
            for s in enumerate_codex_sessions()
                .into_iter()
                .filter(|s| s.cwd == cwd)
            {
                if on_entry.send(codex_session_entry(&s, &metadata)).is_err() {
                    return Ok(count); // 前端 drop channel → 取消
                }
                count += 1;
            }
            return Ok(count);
        }
        // 〔`K-R97` 09-12〕`project_dir` 现在是**编码目录名**，不再是绝对路径 ——
        // 列表那条路改问后端要 `--list-projects` 之后，它带回来的就是名字（与远端那条逐字同形）。
        // 🔴 名字里不许有分隔符 / 上跳：`Path::starts_with` 是**按段比**、不做规范化，
        // `<根>/../etc` 照样 `starts_with(<根>)` ⇒ 只靠下面那道围栏挡不住穿越。
        // 这道拒绝与后端侧 `observe/history_query.rs::list_sessions` 的第一道检查逐字同形。
        if project_dir.contains('/') || project_dir.contains('\\') || project_dir.contains("..") {
            return Err(format!("refuse: invalid project dir name: {project_dir}"));
        }
        let claude_dir = paths::resolve_claude_dir().ok_or("claude dir not found")?;
        let projects_dir = crate::adapter::records_dir(&claude_dir);
        let target = projects_dir.join(&project_dir);
        // ⚠ **围栏刻意留着**（纵深防御，同本文件 `stream_read_session_jsonl` 那道）：
        // 上面拒了分隔符，这里再核一次「解析出来的落点真的在根之内」。
        if !target.starts_with(&projects_dir) {
            return Err(format!(
                "refuse: {} outside {}",
                target.display(),
                projects_dir.display()
            ));
        }
        if !target.is_dir() {
            return Err(format!("{} not a directory", target.display()));
        }
        let metadata = load_metadata().unwrap_or_default();

        let mut count = 0u32;
        let files = match std::fs::read_dir(&target) {
            Ok(d) => d,
            Err(e) => return Err(format!("read {}: {e}", target.display())),
        };
        for f in files.flatten() {
            let p = f.path();
            if crate::adapter::has_record_ext(&p) {
                if let Some(entry) = analyze_jsonl(&p, &metadata, &map) {
                    if on_entry.send(entry).is_err() {
                        // 前端 drop channel → 取消
                        tracing::info!(
                            "stream_history_sessions_in_project({}): cancelled at {} entries",
                            target.file_name().and_then(|s| s.to_str()).unwrap_or("?"),
                            count
                        );
                        return Ok(count);
                    }
                    count += 1;
                }
            }
        }
        tracing::info!(
            "stream_history_sessions_in_project({}): {} sessions in {}ms",
            target.file_name().and_then(|s| s.to_str()).unwrap_or("?"),
            count,
            started.elapsed().as_millis()
        );
        Ok(count)
    })
    .await
    .map_err(|e| format!("spawn_blocking join: {e}"))?
}

/// issue #12: 流式版（取代已删的非流式 `read_session_jsonl`）。
///
/// 按 100 行一 chunk 边读边发，前端可在 ~500ms 内开始渲染首屏（即使整 jsonl
/// 上千条 / 10MB+）。
///
/// 取消：前端 drop channel 时 send 返 Err → break。
#[tauri::command]
pub async fn stream_read_session_jsonl(
    jsonl_path: String,
    on_chunk: tauri::ipc::Channel<Vec<crate::bridge::JsonlLinePayload>>,
) -> Result<u32, String> {
    const CHUNK_SIZE: usize = 100;
    tokio::task::spawn_blocking(move || {
        let started = std::time::Instant::now();
        let target = PathBuf::from(&jsonl_path);
        // Phase 2 F1a：按路径判 agent kind（Claude `~/.claude/projects` vs Codex `~/.codex/sessions`）。
        // Claude 路径 kind=ClaudeCode → 根/session_id/解析与原字节一致（零回归）；Codex 走对应根 + 映射。
        let kind = crate::adapter::kind_of_path(&target);
        let root = crate::adapter::for_kind(kind)
            .data_root()
            .map(|dr| crate::adapter::records_dir_for(kind, &dr))
            .ok_or("agent data dir not found")?;
        if !target.starts_with(&root) {
            return Err(format!(
                "refuse: {} outside {}",
                target.display(),
                root.display()
            ));
        }
        if !crate::adapter::has_record_ext(&target) {
            return Err("not a .jsonl file".into());
        }

        let session_id = crate::adapter::session_id_from_path_with(
            crate::adapter::for_kind(kind).layout(),
            &target,
        )
        .unwrap_or_default();
        let file = File::open(&target).map_err(|e| format!("open {}: {e}", target.display()))?;
        let reader = BufReader::new(file);
        let path_str = target.to_string_lossy().into_owned();
        let mut cwd_seen: Option<String> = None;
        let mut buf: Vec<crate::bridge::JsonlLinePayload> = Vec::with_capacity(CHUNK_SIZE);
        let mut total = 0u32;
        // P5.1：history 流式读时同样给每行 seq（per-file 单调）。SessionViewer
        // 用 RecordTimeline 排序时跟实时 tab 走同一套逻辑。
        let mut next_seq: u64 = 0;

        for line in reader.lines().map_while(Result::ok) {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            let rec = match crate::parser::parse_for_kind(kind, trimmed) {
                Ok(Some(r)) if r.is_displayable() => r,
                _ => continue,
            };
            if let JsonlRecord::User { cwd, .. } = &rec {
                if cwd_seen.is_none() {
                    cwd_seen = cwd.clone();
                }
            }
            let seq = next_seq;
            next_seq += 1;
            buf.push(crate::bridge::JsonlLinePayload {
                session_id: session_id.clone(),
                cwd: cwd_seen.clone(),
                path: path_str.clone(),
                seq,
                // 历史浏览器读本地 jsonl，无远端来源标签。
                origin: None,
                message: rec,
            });
            total += 1;
            if buf.len() >= CHUNK_SIZE {
                let chunk = std::mem::replace(&mut buf, Vec::with_capacity(CHUNK_SIZE));
                if on_chunk.send(chunk).is_err() {
                    tracing::info!(
                        "stream_read_session_jsonl({}): cancelled at {} records",
                        session_id,
                        total
                    );
                    return Ok(total);
                }
            }
        }
        if !buf.is_empty() {
            let _ = on_chunk.send(buf);
        }
        tracing::info!(
            "stream_read_session_jsonl({}): {} records in {}ms",
            session_id,
            total,
            started.elapsed().as_millis()
        );
        Ok(total)
    })
    .await
    .map_err(|e| format!("spawn_blocking join: {e}"))?
}

/// 本地删除的路径守卫（Batch4-F15）：canonicalize 后校验，`..` 与 symlink 穿越都拒。
///
/// 旧实现只做 `PathBuf::starts_with`——那是纯组件前缀比较，不解析 `..` 不解
/// symlink：`<projects>/../../x.jsonl` 能通过校验、由 OS 在 remove_file 时解析。
/// 对照远端版 `sftp.rs::remove_remote_file`（canonicalize 双重守卫），本地反而
/// 更弱。现在两边 canonicalize（Windows 上 canonicalize 产生 `\\?\` 前缀，
/// 单边做必然不匹配），扩展名也在 canonical 路径上查（防 symlink 指向非 jsonl）。
///
/// 返回 canonical 后的删除目标；抽成纯函数以便注入 tempdir 直测。
///
/// 已接受取舍：canonicalize → remove_file 之间存在理论 TOCTOU 窗口（期间目录
/// 组件被换成 symlink）。path-based API 固有限制；威胁模型是"前端传错路径"
/// 而非恶意本地攻击者，与 sftp.rs 远端版（realpath → remove）同级，不做
/// openat/O_NOFOLLOW 级加固。
fn validate_delete_target(jsonl_path: &str, projects_dir: &Path) -> Result<PathBuf, String> {
    let target = PathBuf::from(jsonl_path);
    if !target.exists() {
        return Err(format!("{} does not exist", target.display()));
    }
    let canon_target = target
        .canonicalize()
        .map_err(|e| format!("canonicalize {}: {e}", target.display()))?;
    let canon_projects = projects_dir
        .canonicalize()
        .map_err(|e| format!("canonicalize {}: {e}", projects_dir.display()))?;
    if !canon_target.starts_with(&canon_projects) {
        return Err(format!(
            "refuse delete: {} is outside {}",
            canon_target.display(),
            canon_projects.display()
        ));
    }
    if !crate::adapter::has_record_ext(&canon_target) {
        return Err("refuse delete: not a .jsonl file".into());
    }
    Ok(canon_target)
}

#[tauri::command]
pub fn delete_history_session(session_id: String, jsonl_path: String) -> Result<(), String> {
    // 安全校验：必须在 claude_dir/projects 之下，避免前端传错路径误删别处文件
    let claude_dir = paths::resolve_claude_dir().ok_or("claude dir not found")?;
    let projects_dir = crate::adapter::records_dir(&claude_dir);
    let target = validate_delete_target(&jsonl_path, &projects_dir)?;

    std::fs::remove_file(&target).map_err(|e| format!("remove {}: {e}", target.display()))?;
    tracing::info!("history: deleted {}", target.display());

    // 同步从 metadata 移除条目
    remove_metadata_entry(&session_id);
    Ok(())
}

// === F62：从历史某一轮创建分支 ===
//
// **§1 只读铁律：不修约、正交（照 F47 先例）**。建分支是「用户显式点某条消息 → 复制
// `[根 … 该消息]` 前缀产出一个**全新** jsonl」——原会话一字节不改、纯新增，与「monitor
// 作为监视器不改坏正在监视的会话文件（尤其防自动/后台写）」这条约正交。防误伤守卫：
// ①源路径白名单（canonicalize + starts_with(projects) + `.jsonl`）；②只写**新生成的
// sid**、目标已存在则拒（绝不覆盖任何现存会话）。
//
// **落盘格式 = Claude 原生 `/branch`**（issue #12 `forkedFrom`，本机 fe4aad07 实证 +
// `claude --resume` 回读实测）：复制沿 parentUuid 从分叉点回溯到根的**线性前缀**，逐条
// 保留原 uuid/parentUuid、`sessionId` 改新 id、加 `forkedFrom{sessionId:源, messageUuid:自身}`。
// 分叉点之后的记录、被 ESC 回退的兄弟子树、sidechain 全部不带过来（前缀只走祖先链）。
//
// === G0（branch-anywhere）：上面那句「= 原生格式」已被扩样本复核，并钉成机检 ===
//
// **样本**：两份**早于本功能合入（07-16）**因而只可能是 CC 自己产的 fork
// —— `0473c3a0`(07-03) 与 `fe4aad07`(07-05)；外加一条三代 fork 链
// （`a40059e8 → 7c2a26d6 → 4f3fba62`）证明每次 `/branch` 都产**独立文件**、父会话仍可 resume。
//
// **决定性指纹**：两份原生 fork 的复制段在源文件里分别跨 1964 / 170 行，却只取了
// 1402 / 118 条 —— **跳过了 562 / 52 条落在区间内的旁支**。若官方是「线性文件切片」，
// 那些记录会被一并带走。⇒ **官方 `/branch` 走的就是祖先回溯，与本实现相同。**
//
// 逐字段亦一致：uuid **原样保留**（不 remap）· timestamp **不改** ·
// `slug`/`sourceToolAssistantUUID`/`agentName` **照样带着** · 复制段只有
// `assistant`/`user`/`attachment`/`system` 四类、不带无 uuid 的旁挂记录
// （`mode`/`permission-mode`/`ai-title`/`last-prompt`/`file-history-snapshot`/`queue-operation`）·
// `logicalParentUuid` 在原生 fork 里就带着指向文件外的目标 ⇒ 官方自己不保证这条边。
//
// **⚠ 别拿 `claude-agent-sdk` 的 `fork_session` 当规范。**
// 它也是官方的，但它 remap 全部 uuid、清 `slug` 等字段、改末条 timestamp、
// 且 `up_to_message_id` 是**线性切片** —— 那些是 SDK 自己的选择，**不是 CC 的落盘规范**。
// 规划 branch-anywhere 时正是照着它列出六条「我们的缺口」，被上面这批语料全部证伪。
// 判据只有一个：**CC 自己落在盘上、`claude --resume` 能读回去的那个格式**。
// `branch_matches_native_fork_shape` 就是这条判据的机检版本，改动本函数前先读它。
//
// **用 `serde_json::Value` 原样搬运**（不走有损的 `JsonlRecord` enum，避免丢 gitBranch/
// version/origin 等 schema 外字段）——除 sessionId/forkedFrom 两处有意改动外逐字段忠实。

/// 建分支的返回体（前端据此提示 / 一键 resume 新分支）。
///
/// **`Deserialize` 是给远端那条路用的**（G6）：daemon 的 `--fork-session` 在 stdout 吐同形 JSON，
/// `remote_branch` 直接反序列化成本类型 —— 两条路一个类型，前端的成功处理才只有一份。
#[derive(Debug, Serialize, serde::Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../../src/generated/"))]
pub struct BranchResult {
    #[serde(rename = "sessionId")]
    pub session_id: String,
    #[serde(rename = "jsonlPath")]
    pub jsonl_path: String,
}

// 🔴〔`K-R88` 09-13〕**源会话那一步的守卫搬走了，连同它的入参形状一起。**
//
// 原先这里有一个收**路径**的门（存在性 → 两边 canonicalize → 前缀落在 projects 内 →
// 扩展名 `.jsonl`），而后端那条路收的是 **sid**。同一件事两个入参形状 ⇒
// 「查不到怎么办」两边可以各答各的，而没有任何东西会因此变红。
//
// 今天两侧都走 `branch_core::find_session_file`：**入参只有 sid**，
// 而路径由那一份在记录树里枚举出来。⇒ 界外那种入参**连表达都表达不出来**了 ——
// 这比「表达得出来但被门拦下」强一档（`K-R88` `§0b` 逐字：少一个可被构造的路径入参
// 就少一条路径穿越面）。
// 那道门的两条实证判据（`..` 穿越 · 软链逃逸）没有被删，**换成了新形状的同名两条**，
// 住在本文件测试段里，读的是同一份实现。

/// 读一个 jsonl 文件为逐行 `serde_json::Value`（剥 BOM、跳空行；解析失败的行**保留原样**
/// 不了了之——建分支只复制祖先链上的记录，坏行若不在链上自然被忽略）。
fn read_jsonl_values(path: &Path) -> Result<Vec<serde_json::Value>, String> {
    let file = File::open(path).map_err(|e| format!("open {}: {e}", path.display()))?;
    let reader = BufReader::new(file);
    let mut out = Vec::new();
    for line in reader.lines() {
        let line = line.map_err(|e| format!("read {}: {e}", path.display()))?;
        let trimmed = line.trim_start_matches('\u{feff}').trim();
        if trimmed.is_empty() {
            continue;
        }
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(trimmed) {
            out.push(v);
        }
    }
    Ok(out)
}

// G1：**记录变换已提成共享 crate** `branch-core` —— monitor 与远端 daemon 共用同一份。
// 搬走的理由与选型过程见 `.claude/planned-build/branch-anywhere/features/G1-*.md`；
// 落盘格式的实证判据见该 crate 的 `build_branch_records` 头注。
// **本文件只留 IO**（读 jsonl 的口径 ＋ `O_EXCL` 落盘，那两样是 monitor 侧特有的）；
// 〔`K-R88` 09-13〕「按 sid 找那份源文件」也进了同一个 crate，两侧同一份。
use branch_core::build_branch_records;

/// F62 IPC：从历史会话的某条消息创建分支。前端点消息卡上的 `⑂` 时调，成功返回新 sid。
/// 见本段顶部大注释（§1 正交、原生格式、守卫）。薄壳：resolve_claude_dir → 委托 branch_impl。
///
/// 🔴〔`K-R88` 09-13〕**入参从路径改成了 sid**，与远端那条
/// （`remote_branch::create_remote_branch_session`）**形状一致**。
/// 前端两条路本来就都拿得到 sid（按钮那份上下文里一直有），所以这不是给调用方加负担。
#[tauri::command]
pub fn create_branch_session(
    source_session_id: String,
    message_uuid: String,
) -> Result<BranchResult, String> {
    let claude_dir = paths::resolve_claude_dir().ok_or("claude dir not found")?;
    let projects_dir = crate::adapter::records_dir(&claude_dir);
    branch_impl(&source_session_id, &message_uuid, &projects_dir)
}

/// 建分支核心（可注入 projects_dir 直测，绕开 resolve_claude_dir 全局依赖——同 delete 的
/// validate_delete_target 测法）。安全承诺全在这层：源零改动、只写新 sid、绝不覆盖。
///
/// 「找那份源文件」**不在这里**：走 `branch_core::find_session_file`，与后端同一份（`K-R88`）。
/// 本函数留下的是 monitor 侧特有的两样：读 jsonl 的口径、以及 `O_EXCL` 落盘。
fn branch_impl(
    source_session_id: &str,
    message_uuid: &str,
    projects_dir: &Path,
) -> Result<BranchResult, String> {
    let source = branch_core::find_session_file(projects_dir, source_session_id)?;

    let lines = read_jsonl_values(&source)?;
    let new_sid = uuid::Uuid::new_v4().to_string();
    let records = build_branch_records(&lines, message_uuid, source_session_id, &new_sid)?;

    // 目标写进源会话同目录（projects 内某项目目录），文件名 = 新 sid。
    let parent = source
        .parent()
        .ok_or("refuse branch: source has no parent dir")?;
    let out_path = parent.join(format!("{new_sid}.jsonl"));
    write_branch_file(&out_path, &records)?;
    tracing::info!(
        "history: branched sid={new_sid} from {source_session_id}@{message_uuid} ({} records)",
        records.len()
    );

    Ok(BranchResult {
        session_id: new_sid,
        jsonl_path: out_path.to_string_lossy().into_owned(),
    })
}

/// 把记录序列化成 JSONL 原子写入 `out_path`。**`create_new`：目标已存在则直接失败**——
/// 自证「绝不覆盖任何现存会话」契约，消 exists()→write 的 TOCTOU 窗口。抽出便于直测。
fn write_branch_file(out_path: &Path, records: &[serde_json::Value]) -> Result<(), String> {
    use std::io::Write as _;
    let mut body = String::new();
    for rec in records {
        body.push_str(&serde_json::to_string(rec).map_err(|e| format!("serialize: {e}"))?);
        body.push('\n');
    }
    let mut f = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(out_path)
        .map_err(|e| {
            if e.kind() == std::io::ErrorKind::AlreadyExists {
                format!("refuse branch: {} already exists", out_path.display())
            } else {
                format!("create {}: {e}", out_path.display())
            }
        })?;
    f.write_all(body.as_bytes())
        .map_err(|e| format!("write {}: {e}", out_path.display()))
}

/// 从本地 history-metadata.json 移除某 sid 的条目（best-effort）。本地删除与远端删除
/// （issue F11 `delete_remote_history_session`）共用——元数据是 monitor 本地按 sid 的注解，
/// 无论会话本体在本地还是远端，删除后都该清掉对应注解。
pub(crate) fn remove_metadata_entry(sid: &str) {
    let mut metadata = load_metadata().unwrap_or_default();
    if metadata.entries.remove(sid).is_some() {
        let _ = save_metadata(&metadata);
    }
}

#[tauri::command]
pub fn update_history_metadata(
    session_id: String,
    patch: MetadataPatch,
) -> Result<EntryMetadata, String> {
    let mut metadata = load_metadata().unwrap_or_default();
    let entry = metadata.entries.entry(session_id.clone()).or_default();
    if let Some(s) = patch.starred {
        entry.starred = s;
    }
    if let Some(t) = patch.custom_title {
        // Some(Some(s)) 设置；Some(None) 清空
        entry.custom_title = t.filter(|s| !s.trim().is_empty());
    }
    if let Some(h) = patch.hidden {
        entry.hidden = h;
    }
    if let Some(a) = patch.last_account {
        // Some(Some(name)) 设值；空/空白串 → filter 后 None = 清空（JSON null 走不到这，见 struct 注释）
        entry.last_account = a.filter(|s| !s.trim().is_empty());
    }
    entry.updated_at = now_ms();
    let result = entry.clone();
    save_metadata(&metadata)?;
    Ok(result)
}

/// 纯变换：metadata → sid→lastAccount（只含真有 lastAccount 的条目）。抽出便于单测。
fn last_accounts_of(meta: HistoryMetadata) -> HashMap<String, String> {
    meta.entries
        .into_iter()
        .filter_map(|(sid, e)| e.last_account.map(|a| (sid, a)))
        .collect()
}

/// A4：只读——返回 sid → lastAccount（上次用本工具带账号起该会话时记的）。前端账号徽章
/// 源②（DESIGN §3）：live 探测不到时用它兜底。只含真有 lastAccount 的条目；读失败 → 空表
/// （降级：徽章退回 live/未知，不报错）。**不写 jsonl、不改任何状态。**
#[tauri::command]
pub fn list_last_accounts() -> HashMap<String, String> {
    last_accounts_of(load_metadata().unwrap_or_default())
}

/// 在新终端窗口里 resume 一个历史会话。
///
/// v2.8.1（bug 修复）：改为在 **PowerShell**（系统自带 `powershell.exe`，**加载用户
/// profile**）里跑，命令优先用户的 `cc` wrapper、回退 `claude`。详 `resume_impl`。
/// Windows 上优先 wt.exe，找不到回退独立控制台。其他平台暂不支持。
/// G3b / Phase G：`account` = 用哪个账号起，**三态**见 [`LaunchAccount`]。
///
/// 参数缺席 ⇒ 输出与本参数存在之前**逐字节相同**（既有调用点无需改）。
/// `{"kind":"base"}` ⇒ **显式** `unset CLAUDE_CONFIG_DIR`（不是「什么都不加」——
/// 那会被 shell rc 里的 `export CLAUDE_CONFIG_DIR=<默认账号>` 顶掉 = 静默串号）。
///
/// **订正**：G3b-1 当时把「缺省 / 空 = 账号 0（一个字都不注入，与 IR 的 `--base` 对齐）」
/// 写进了这段注释 —— 那句话是错的：IR 的 `--base` 做的是 `unset`，不是「不注入」，
/// 远端那条路也确实渲染成 `unset CLAUDE_CONFIG_DIR; `。Phase G 审计两个视角各自抓到。
///
/// P3t-Y2：新增 `tmux_name` —— **POSIX 本机**要把会话建进 tmux 时的会话名。
/// 缺席（今天所有调用点都缺席）⇒ 渲染器诚实降级回旧路 ⇒ 与本参数存在之前逐字节相同。
/// 名字必须由前端 `mintTmuxName` 铸（那是全仓唯一带撞名避让的铸造口），所以它只能传进来、
/// 不能在 Rust 里造。Windows 那一侧**不读它**（`C12`）。
#[tauri::command]
pub fn resume_history_session(
    session_id: String,
    cwd: String,
    launcher: Option<String>,
    account: Option<LaunchAccount>,
    tmux_name: Option<String>,
) -> Result<(), String> {
    resume_impl(
        &session_id,
        &cwd,
        launcher.as_deref(),
        account.as_ref(),
        tmux_name.as_deref(),
    )
}

/// F34：用户自定义 resume 启动命令（设置面板「本地 resume 命令」）。
/// 拼进 shell 前必须校验——只允许命令名+简单参数形态（字母数字 `-_.` 与空格），
/// 杜绝 `;`/`|`/`$()` 等注入面。空/纯空白视为未设置。
fn sanitize_launcher(launcher: Option<&str>) -> Result<Option<String>, String> {
    let Some(l) = launcher.map(str::trim).filter(|l| !l.is_empty()) else {
        return Ok(None);
    };
    let valid = l
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | ' '));
    if !valid {
        return Err(format!(
            "refuse resume: 自定义 resume 命令含非法字符（仅允许字母数字、-_.、空格）: {l:?}"
        ));
    }
    Ok(Some(l.to_string()))
}

/// F06（unify-launch）：本地路径的动作枚举——与 TS `LaunchAction` 同构。
///
/// # 🔴 `K-R106`〔用@09-13〕：`Attach` 是本轮加的，而它此前那句「不该有」是错的
///
/// 这里原来逐字写着「**无 `attach` 变体：本地会话从无 attach 概念**」。
/// 用户 09-13 亲裁把它推翻了：
///
/// > 「新起一个会话之后，把你的终端接进那个会话那一句 `tmux attach`，归谁产？」
/// > 「**归本机后端就好了啊**」〔用@09-13，`DECISIONS.md#R61` 裁定三〕
///
/// ⚠ 那句「本模块**不 attach**，一次都不」（`src/backend/control/launch.rs`
/// 头注）**仍然对** —— 它说的是**远端后端**，理由逐字是「在远端，**开不了你面前的窗**」。
/// 🔴 **本机后端就在用户面前那台机器上** ⇒ 那条位置约束在这一侧不成立。
/// `R61` 立的就是这件事：**不许再用「daemon」这个词把这两件事压平。**
///
/// # ⚠ `Attach` 与另外两个变体**不是同一类动作**，三处边界写在这里
///
/// 1. **它不起 agent** ⇒ 不需要 sid、不需要账号、不需要中转前缀、不需要身份 token。
///    下面每一处 `match` 的 `Attach` 臂都是这句话的一个面，不是「顺手填 `None`」。
/// 2. **旧路产不出它**（[`local_launch_choice`] 当场拒）：那条路只会拼一个**拉起器**，
///    渲出来的是「起一个新的 claude」，而不是「接进已有的那个」——
///    静默产出它比拒绝更坏（用户以为接回了原会话，实际另起一条）。
/// 3. **它不经 [`launch_local`]**（那里也当场拒）：那条路 `spawn` 出去、stdio 全 null，
///    而 attach 的正题是把**用户自己的终端**接进去（`§1.3`）。⇒ 只渲染，交给调用方。
///
/// # Windows 那一格：变体本身挂 `#[cfg(not(windows))]`
///
/// 与 [`render_local_ccm`] / [`render_local_ccm_with`] **同一条 cfg**。
/// 定框 `C12`〔用 08-12〕逐字「windows不要tmux」⇒ Windows 上没有 tmux 容器，
/// 也就没有「接进那个容器」这个动作 —— 让它在**编译期就不存在**，
/// 而不是运行期再判一次（后者是「加个变体不接线」那一形的温床）。
enum LocalPsAction {
    New,
    Resume(String),
    /// 🔴 `K-R106`：**接进一个已经存在的 tmux 会话**。
    ///
    /// 会话名**不放在变体里**，走 `tmux_name` 那个参数 —— 全仓只有一个地方说得出
    /// 「这次说的是哪个容器」，两处就会漂（而 `ccm attach` 收的正是容器名本身，
    /// `ccm_invocation::render_ccm_invocation` 的 attach 分支读的是 `Container::Tmux`）。
    #[cfg(not(windows))]
    Attach,
}

/// 构造本地 PowerShell 命令体（不含 `-EncodedCommand` 编码）——`build_resume_ps_command`/
/// `build_new_session_ps_command` 曾各自逐字符重复的「F34 自定义命令优先 → cc 别名探测优先 →
/// 回退默认拉起」分支在此收拢成一处（F06：两套 builder 收进同一意图模型）。
///
/// 防注入：resume 场景的 sid 来自前端历史条目，理论上是 UUID，但作为拼进 shell 命令的
/// 不可信输入必须校验——只允许 `[A-Za-z0-9_-]`，否则拒绝（杜绝 `; rm -rf` 之类）。
///
/// 优先 `cc`：检测到用户的 `cc` 函数（PowerShell 集成 wrapper，内部含 `__ccm_bind` +
/// 用户自己的代理 / env 设置）就用它；检测不到才回退默认拉起器。命令在 profile 已加载的
/// PowerShell 里跑（见 resume_impl 不带 -NoProfile），所以即使回退，profile 里的 PATH /
/// 代理 env 仍生效。
///
/// 抽成独立函数是为了单测（不 spawn 进程也能验证防注入 + cc 优先逻辑）。
/// （纯字符串构造，跨平台可编译可测；拉起本身在 launch.rs 按平台门控。）
/// L1：**与平台无关**的本地拉起决策 —— 校验 + 按活跃适配器算出「用哪个命令」。
///
/// 抽出来是因为 L1 给本地加了第二个渲染器（POSIX）。**校验与选择只能有一份**，
/// 否则两个平台迟早各自漂移；而「怎么写这个条件判断」才是平台差异
/// （PowerShell 用 `Get-Command`，POSIX 用 `command -v`）。
///
/// **sid 校验留在这里**：它与前端 `validateLocalLaunch` 是**两道独立防线**，不是重复

/// G3b：账号前缀 —— 把 `CLAUDE_CONFIG_DIR` 注入本地拉起命令。
///
/// # `None` = 账号 0 = **一个字都不注入**
///
/// 与 IR 的 `--base` 语义对齐，也保证「没选账号」这条路的输出**与本功能之前逐字节相同**
/// —— 既有那批钉死输出的测试因此原样全绿，它们就成了「账号 0 不变」的守卫。
///
/// # 校验语义照抄 TS 侧的 `isValidConfigDir`（`src/shell-quote.ts:41`）
///
/// **不重新发明判据**：那边已经因为账号隔离审计 D7（extraEnv key 无校验）收紧过一轮。
/// 拒的东西：非绝对路径 · `/` 本身 · 含 `/../` 或以 `/..` 结尾 · shell 元字符/引号/控制符 ·
/// 可欺骗 Unicode（零宽 / 双向控制 / NBSP / BOM）。
///
/// **非法即 Err，绝不拼进命令** —— 这条路径的产物会进 shell，宽容一格就是注入面。
/// 「这次拉起用哪个账号」。**三态，不是两态** —— Phase G 审计抓出的一条静默串号：
///
/// | 取值 | 含义 | 产出的前缀 |
/// |---|---|---|
/// | 参数缺席（`None`） | **调用方没表态** | 空串（既有调用点逐字节等价旧行为） |
/// | `{"kind":"base"}` | **用户显式选了账号 0** | `unset CLAUDE_CONFIG_DIR; ` |
/// | `{"kind":"named","configDir":"…"}` | 具名账号 | `export CLAUDE_CONFIG_DIR='…'; ` |
///
/// **为什么「账号 0」不能等于「什么都不加」**（`src/shell-quote.ts` 的 Z03 用整段注释写着，
/// 远端那条路也确实渲染成 `unset CLAUDE_CONFIG_DIR; `）：用户的 shell rc 里很可能有一句
/// `export CLAUDE_CONFIG_DIR=<默认账号>`（`cc-acct-iso shellinit` 生成的就是它），
/// 而本地拉起**故意加载 rc**（`launch.rs` 的 `bash -lic` / 不带 `-NoProfile` 的 powershell）
/// ⇒ 「什么都不加」会落到默认账号上 = **静默串号**。弹窗上写着「不注入」，实际起在别的号上，
/// 正是 `fork-launch.ts` 头注要防的那件事。
///
/// **为什么不用一个魔法串**（如 `configDir: "__base__"`）：R05 刚把跨文件比字符串字面量的
/// `"__base__"` 换成判别联合，理由是「拼错一个字符 tsc 抓不到，而行为是基座选项静默变成一个
/// 名叫 `__base__` 的普通账号」。这里不重蹈。
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum LaunchAccount {
    /// 账号 0：**显式不注入**（产出 `unset`），不是「什么都不做」。
    Base,
    Named {
        #[serde(rename = "configDir")]
        config_dir: String,
        /// `K-R53`：这个账号的**名字**。
        ///
        /// # 它为什么要存在（在此之前这一格是空的，而空着的代价是可量的）
        ///
        /// CLI 只会 `--account <名字>`。本变体先前**只有目录**
        /// ⇒ [`render_local_ccm_with`] 对它必然 §35 短路 ⇒ **本机具名账号一条都进不了
        /// ccm 容器路**。而盘上四个本机拉起入口里有三个只说得出具名账号
        /// （`src/accounts.ts::localLaunchAccountSync`），⇒ 那三条**在类型上**走不到后端那条路。
        ///
        /// # ⚠ 它**不是**从 `config_dir` 推出来的
        ///
        /// 推得出一个像样的名字（`cc-acct-iso` 的布局是 `~/.claude-accts/<名字>`，
        /// [`relay_account_id_of_dir`] 就是那么推的），**但那两处的失效方向相反**：
        /// 推错一个中转 id ⇒ 表里查不到 ⇒ 逐字节走旧路（保守）；推错一个 `--account`
        /// ⇒ `ccm` 当场 `die`（`src/backend/control/ccm/argv.rs` 认不出这个名字 = 退出码 2）
        /// ⇒ **一次本来能起的会话变成一条报错**。⇒ 这一格只收**调用方说得出**的名字。
        ///
        /// 前端那一侧的取值口与 `configDir` 那半**同源**
        /// （`accounts.ts::localLaunchAccountNameSync`，两半是同一条规则的两侧）。
        ///
        /// `None` = **调用方只说得出目录**（例：分叉时源会话是活的，继承的是它的目录、
        /// 没有名字）⇒ CLI 仍然说不出 `--account` ⇒ 照旧 §35 短路，与本字段加进来之前逐字同。
        #[serde(default)]
        name: Option<String>,
    },
}

/// 两种 shell 共用的元字符黑名单。**`\` 不在里面** —— 见 `validate_config_dir_ps`：
/// Windows 的账号目录长成 `C:\Users\z\.claude-accts\z`，把 `\` 一律禁掉等于禁掉整个平台。
/// 它在两种 shell 的**单引号**里都是字面量（POSIX `'…'` 无转义；PowerShell `'…'` 无插值），
/// 所以真正要挡的是能提前闭合引号或另起命令的那几个。
///
/// 历史注记（E75，2026-08-01 已修）：这里当初写成**字符串**而不是 `&[char]` 数组，
/// 是因为 `'\"'`（字符字面量里的双引号）会让 `test-support/strip-comments.ts` 的状态机
/// 以为字符串开始了、从此不再剥注释 ⇒ 本文件后面注释里的 `#[tauri::command]` 字样
/// 被 C04a 守卫当成真属性，报出一个不存在的命令。**那是当时的绕法。**
/// 守卫已经会认 Rust 字符字面量了，所以这条约束**不再成立**；`&str` 形态留着只是因为
/// 配 `.contains(c)` 读起来更顺，不是被逼的。
/// ⚠ **U8c-1 起只剩 Windows / 测试期在用**（POSIX 侧已改调 `backend::control::payload`）。
/// cfg 与它唯一的消费者 [`validate_config_dir_ps`] 对齐 —— 不加就是三条 `never used`
/// 警告，而 `cargo build` 不带 `-D warnings` ⇒ **不会红**（clippy 集合差抓到的）。
// 〔audit-0805 08-06〕**这份副本删了**（E3：一个事实恰好一个权威源）。
// 权威源是 `backend::control::payload::SHELL_META_COMMON`，经 `is_command_unsafe_char` 派生。
// ⚠ 不是理论风险：`payload.rs` 的头注逐字记着，本文件此前那张**不可见字符表**就漂过 ——
// 缺 `U+1680` · `U+2000..200A` · `U+202F` · `U+205F` · `U+2060..2064` · `U+3000`，
// 是一处纵深防御缺口。同一个文件、同一族副本，这次连元字符表一起收掉。

// U8c-3-alt（账本 S18 收口）：这里原本有一张 `SPOOFABLE` 表 —— **U7-3 之前的旧集合**，
// 18 项，缺 `U+1680` · `U+2000..200A` · `U+202F` · `U+205F` · `U+2060..2064` · `U+3000`。
// 它在 U8c-1 之后只剩 Windows 那条路在用；本轮 Windows 也改调 `acct_core::is_deceptive_char`
// （「什么算视觉欺骗」是**平台无关**的判断，与 `is_safe_config_dir` 那条「`\` 与盘符」的
// 平台特化不是一回事 —— `acct-core` 头注对后者的「不合」裁决不适用于这里）。
// ⇒ 那张表**删掉**，不是留着不用：留着就是「旧集合还在仓里等下一个人复制」。

#[cfg(any(windows, test))]
fn has_bad_chars(dir: &str, extra: &str) -> bool {
    // 逐项与权威源等价：`is_command_unsafe_char` = 控制字符 | C1 段 | 元字符 | 视觉欺骗字符；
    // 本函数额外多一个调用点自带的 `extra` 集合（今天唯一调用点传空串）。
    dir.chars()
        .any(|c| crate::backend::control::payload::is_command_unsafe_char(c) || extra.contains(c))
}

/// POSIX 侧校验：必须是**绝对 POSIX 路径**，且不含反斜杠（那边的路径里不该有）。
///
/// **U8c-1：判据本体已搬出本文件** —— P4b 起在 `backend::control::payload::config_dir_command_safe`
/// （U8c-1 时在共享 crate `launch-core`——P4c 起那个 crate 叫 `shell-quote-core` 且只剩 quote）。
/// 本函数只剩「把 bool 变成带上下文的 Err」。
///
/// ⚠ **这次搬家不是纯重构，它把校验变严了**：本文件原先用自己那张 `SPOOFABLE`
/// （18 项，是 **U7-3 之前**的旧集合），而 crate 侧建立在 `acct_core::is_deceptive_char`
/// 的**并集**上 —— 多拒 `U+1680` · `U+2000..200A` · `U+202F` · `U+205F` · `U+2060..2064` ·
/// `U+3000`。U7-3 当时把并集给了两个**读 manifest** 的地方，**拼命令这条路漏了**。
///
/// 诚实定级：那是**纵深防御**缺口，不是当时可利用的洞（configDir 的上游 manifest 读取
/// 已经用并集把过一道）。但「权威也保留本地校验」是本仓自己的纪律（`resolve_query.rs` B2）。
fn validate_config_dir_posix(dir: &str) -> Result<(), String> {
    if crate::backend::control::payload::config_dir_command_safe(dir) {
        Ok(())
    } else {
        Err(format!("拒绝拼入命令：非法 CLAUDE_CONFIG_DIR {dir:?}"))
    }
}

/// PowerShell 侧校验。**与 POSIX 那条的唯一实质差别是「什么算绝对路径」** ——
/// Phase G 审计抓出的一个真 bug：原来两边共用「必须 `/` 开头 + 禁 `\`」，
/// 于是 Windows 上一个真实账号目录（`C:\Users\z\.claude-accts\z`）**必被拒**，
/// 「本机分叉时选一个具名账号」在主平台上 100% 失败。判据照抄 `local_accounts::looks_absolute`
/// （那个函数的头注写明：照搬 `starts_with('/')` 会把每个 Windows 账号判成不安全）。
#[cfg(any(windows, test))]
fn validate_config_dir_ps(dir: &str) -> Result<(), String> {
    let b = dir.as_bytes();
    let drive = b.len() >= 3
        && b[0].is_ascii_alphabetic()
        && b[1] == b':'
        && (b[2] == b'\\' || b[2] == b'/');
    let absolute = dir.starts_with('/') || drive || dir.starts_with("\\\\");
    // `..` 两种分隔符都要挡（Windows 上 `/` 与 `\` 都是合法分隔符）。
    let dotdot = dir.contains("/../")
        || dir.ends_with("/..")
        || dir.contains("\\..\\")
        || dir.ends_with("\\..");
    if !absolute || dir == "/" || dotdot {
        return Err(format!("拒绝拼入命令：非法 CLAUDE_CONFIG_DIR {dir:?}"));
    }
    if has_bad_chars(dir, "") {
        return Err(format!("拒绝拼入命令：非法 CLAUDE_CONFIG_DIR {dir:?}"));
    }
    Ok(())
}

/// POSIX 侧前缀。三态见 [`LaunchAccount`]；参数缺席 → 空串（逐字节等同旧行为）。
fn config_dir_prefix_posix(account: Option<&LaunchAccount>) -> Result<String, String> {
    match account {
        None => Ok(String::new()),
        // U8c-1：这条串的逐字节形态由内核持有（e2e 探针 `grep -q "unset CLAUDE_CONFIG_DIR;"`）。
        //
        // ⚠ **P4b 改成委托整条臂**（原来是自己 `UNSET_CONFIG_DIR_PREFIX.to_string()`）。
        // 起因是搬家把一处被 crate 边界藏住的事实暴露了出来：clippy 报
        // `Account::Base is never constructed` —— 也就是**这个三态里的 base 那一态，
        // 生产从来没走到内核里**，本文件自己截住了。两处逐字相同 ⇒ 是重复的决定，不是分工。
        // 字节完全一致（两边都是同一个常量），由
        // `posix_account_prefix_is_byte_identical_after_moving_to_the_kernel` 兜。
        // ⚠ 剩下的 `None` 那臂与整个三态 match 仍是镜像 —— 那是**登记在案的重复**，
        // 收它要连 `validate_config_dir_posix`（POSIX 侧多拒一个 `\`）一起重定，不在 P4b 范围。
        Some(LaunchAccount::Base) => crate::backend::control::payload::config_dir_prefix_posix(
            Some(&crate::backend::control::payload::Account::Base),
        ),
        Some(LaunchAccount::Named { config_dir, .. }) => {
            let d = config_dir.trim();
            // 空串**不是**账号 0，是坏数据（空值 ≠ 未设 —— Z01 起整套设计的支点）。
            if d.is_empty() {
                return Err(
                    "refuse resume: 具名账号的 configDir 是空的（账号 0 请用 kind=base）".into(),
                );
            }
            validate_config_dir_posix(d)?;
            // U8c-1：串本身由内核产出（P4b 起在 `backend::control::payload`），本文件不再自己 format。
            crate::backend::control::payload::config_dir_prefix_posix(Some(
                &crate::backend::control::payload::Account::Named { config_dir: d },
            ))
        }
    }
}

/// PowerShell 侧前缀。同上；PS 里用 `$env:` 且单引号是字面量引号（无插值）。
#[cfg(any(windows, test))]
fn config_dir_prefix_ps(account: Option<&LaunchAccount>) -> Result<String, String> {
    match account {
        None => Ok(String::new()),
        // PS 里把环境变量置 `$null` 就是删掉它（等价于 POSIX 的 `unset`）。
        Some(LaunchAccount::Base) => Ok("$env:CLAUDE_CONFIG_DIR=$null; ".to_string()),
        Some(LaunchAccount::Named { config_dir, .. }) => {
            let d = config_dir.trim();
            if d.is_empty() {
                return Err(
                    "refuse resume: 具名账号的 configDir 是空的（账号 0 请用 kind=base）".into(),
                );
            }
            validate_config_dir_ps(d)?;
            Ok(format!("$env:CLAUDE_CONFIG_DIR='{d}'; "))
        }
    }
}

/// ——前端那道拦 UI 传参，这道拦任何绕过前端到达 IPC 的输入。
enum LocalLaunchChoice {
    /// 用户显式指定了命令（F34）⇒ 不做别名探测，直接用。
    Fixed(String),
    /// 有 wrapper 别名（`cc`）：探测得到就用 `preferred`，否则 `fallback`。
    Probe {
        alias: String,
        preferred: String,
        fallback: String,
    },
}

/// 🔴 `K-R106`：旧路（[`build_local_posix_command`] / [`build_local_ps_command`]）
/// 被要求产 attach 时给出的**理由**，而不是一个 `bool` 分支 ——
/// 与 [`NO_TMUX_NAME`] / [`RELAY_KEEPS_THE_OLD_PATH`] 同一条纪律：
/// 这条路上「为什么这次没接上」只有降级理由这一个线索。
#[cfg(not(windows))]
const OLD_PATH_CANNOT_ATTACH: &str =
    "旧路产不出 attach —— 它只会拼一个拉起器，渲出来的是「另起一条 claude」而不是     「接进已有的那个」；产得出 attach 的只有 ccm 那条容器路〔`K-R106`，用@09-13     「归本机后端就好了啊」〕";

fn local_launch_choice(
    action: &LocalPsAction,
    launcher: Option<&str>,
) -> Result<LocalLaunchChoice, String> {
    // 🔴 `K-R106`：**旧路产不出 attach，而它必须是「拒」不是「凑一个出来」。**
    //    本函数唯一会拼的东西是一个**拉起器**（`cc` / `claude` / F34 自定义命令）——
    //    拿它去表达「接进已有的那个会话」，渲出来的是**另起一条 claude**：
    //    用户以为回到了原会话，实际上开了第二条，而两条都在跑。
    //    ⇒ fail-closed。产得出 attach 的只有 ccm 那条容器路（[`render_local_ccm_with`]）。
    #[cfg(not(windows))]
    if matches!(action, LocalPsAction::Attach) {
        return Err(OLD_PATH_CANNOT_ATTACH.into());
    }
    if let LocalPsAction::Resume(sid) = action {
        let valid = !sid.is_empty()
            && sid
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
        if !valid {
            return Err(format!("refuse resume: invalid session_id {sid:?}"));
        }
    }
    // F-MA：resume flag / 拉起别名 / 默认拉起都走活跃适配器（CC = --resume / cc / claude）。
    let agent = crate::adapter::active();
    let suffix = |bin: &str| -> String {
        match action {
            LocalPsAction::Resume(sid) => format!("{bin} {} {sid}", agent.resume_flag()),
            LocalPsAction::New => bin.to_string(),
            // 上面那道 fail-closed 已经把它拦在函数入口 —— 到不了这里。
            #[cfg(not(windows))]
            LocalPsAction::Attach => unreachable!("attach 在本函数入口就被拒了"),
        }
    };
    // F34：设了自定义命令就直接用（不再别名自动检测——用户显式选择优先）
    if let Some(l) = sanitize_launcher(launcher)? {
        return Ok(LocalLaunchChoice::Fixed(suffix(&l)));
    }
    let def = agent.default_launcher();
    Ok(match agent.launcher_alias() {
        Some(alias) => LocalLaunchChoice::Probe {
            alias: alias.to_string(),
            preferred: suffix(alias),
            fallback: suffix(def),
        },
        None => LocalLaunchChoice::Fixed(suffix(def)),
    })
}

/// **平台门控**：生产路径上它只在 Windows 被调用（POSIX 走 `build_local_posix_command`）；
/// 但逐字节钉死它输出的测试要在所有平台跑 ⇒ `any(windows, test)`。
/// 用精确门控而不是 `#[allow(dead_code)]`——后者会把将来真正的死代码一并盖住。
#[cfg(any(windows, test))]
fn build_local_ps_command(
    action: &LocalPsAction,
    launcher: Option<&str>,
    account: Option<&LaunchAccount>,
) -> Result<String, String> {
    let prefix = config_dir_prefix_ps(account)?;
    Ok(prefix + &match local_launch_choice(action, launcher)? {
        LocalLaunchChoice::Fixed(cmd) => cmd,
        // 有 wrapper 别名（cc）：优先它、检测不到回退 default。
        LocalLaunchChoice::Probe {
            alias,
            preferred,
            fallback,
        } => format!(
            "if (Get-Command {alias} -ErrorAction SilentlyContinue) {{ {preferred} }} else {{ {fallback} }}"
        ),
    })
}

/// L1：本地拉起命令的 **POSIX** 渲染 —— 与上面那个是同一个决策的另一种写法。
///
/// `Get-Command` 的 POSIX 等价物是 `command -v`：它同样能找到 shell **函数**与别名
/// （`ccm` 的 `cc` 集成正是一个函数），而命令跑在 `bash -lic` 里、rc 已加载 ⇒ 找得到。
fn build_local_posix_command(
    action: &LocalPsAction,
    launcher: Option<&str>,
    account: Option<&LaunchAccount>,
) -> Result<String, String> {
    let prefix = config_dir_prefix_posix(account)?;
    Ok(prefix
        + &match local_launch_choice(action, launcher)? {
            LocalLaunchChoice::Fixed(cmd) => cmd,
            LocalLaunchChoice::Probe {
                alias,
                preferred,
                fallback,
            } => {
                format!(
                    "if command -v {alias} >/dev/null 2>&1; then {preferred}; else {fallback}; fi"
                )
            }
        })
}

/// P3t-Y2：**POSIX 本机**走 CLI 渲染器那一条 —— 拿不到就带理由回来。
///
/// 与远端那条（`remote-launch-run.ts::renderLaunchCommand`）**同一个形状**：
/// 先探 ccm，再渲染，渲不出来就带 `reason` 降级。差别只有传输 ——
/// 远端探测走 ssh、渲染在 monitor 这侧；本机探测直接 `bash -lic`。
///
/// # `tmux_name` 为 `None` 时**必须**拒
///
/// 会话名不许在 Rust 里铸 —— `remote-launch.ts::mintTmuxName` 是**全仓唯一的铸造口**，
/// 撞名避让全住在那里。F13 记着这个坑的原样：另一处产 `<sid8>-cc` 却不避让，
/// 于是「精心让出 `<sid8>-cc-2`」被直接撞掉。在这里补一个铸造口 = 第三次犯同一个错。
/// ⇒ 名字由前端传下来（P3t-Y2b 接线）；没传 ⇒ 说不出容器 ⇒ 诚实降级回旧路。
#[cfg(not(windows))]
const NO_TMUX_NAME: &str = "没有 tmux 会话名（前端未传）—— 名字只许由 `mintTmuxName` 铸";

/// 🔴 `K-R55`（09-11）：**本机 ccm 探测的取值口** —— 与 [`RelayFactSources`] 是同一条缝的形状。
///
/// # 它为什么非有不可（不是「为了好看」，是一条判据今天买不到它要的东西）
///
/// `a_launch_that_goes_through_the_relay_still_cannot_get_a_tmux_container` 要证的是
/// **[`launch_local`] 那一行 `relay.is_empty()` 真的在挡**。要证它，判据必须真的驱动
/// [`launch_local`]，而 [`launch_local`] 在 POSIX 上一定会经过 [`render_local_ccm`]
/// ⇒ 一定会问「这台机器装没装 ccm」。
///
/// 没有这条缝时，那个问题的答案**由跑判据的那台机器给** ——
/// 沙箱里没装 ⇒ [`render_local_ccm`] 恒 `Err(NotInstalled)` ⇒ **每一格都回落到旧路**
/// ⇒ 把生产那道闸翻成恒真也看不出区别。于是判据只剩一条出路：**自己再抄一份那道闸**
///（先算前缀、自己判空），而那正是本仓判过三次的那一形 —— **证的是它自己那份拷贝**。
/// PM 09-11 现打：把 `if relay.is_empty()` 换成 `if true`，
/// 点名单跑 **1 passed**、全量 `cargo --lib` **1472 passed / 0 failed**，一个字都不响。
///
/// ⇒ 把「装没装 / 有哪些能力」收进一个可替换的取值口，判据喂一份**确定的** ccm 事实进去，
/// 于是「走不走得进容器」这件事重新变成由**生产那一行**决定的一维。
///
/// # 它买不到什么（如实写）
///
/// - **探测自己答得对不对**：那是 `ccm_probe` 自己那几条判据的事（本缝只管「问不问」）。
/// - **生产上插进这条缝的是不是它**：由
///   `the_local_launch_really_asks_the_production_ccm_probe` 按**函数地址**对拍，
///   不是按文本 —— 理由与 [`PRODUCTION_RELAY_FACTS`] 那一条相同。
/// - **谁绕开这条缝直接调 [`crate::ccm_probe::probe_local_ccm`]**：今天没有人群闸数它
///   （`RelayFactSources` 那三个取值口有一道，住 `payload.rs`）。**登记，不假装钉住了。**
#[cfg(not(windows))]
#[derive(Clone, Copy)]
pub(crate) struct CcmProbeSource(pub(crate) fn() -> crate::ccm_probe::CcmProbeResult);

/// 生产上这条缝里插的那个取值口。**只有这一处**，判据按地址对拍它。
#[cfg(not(windows))]
pub(crate) const PRODUCTION_CCM_PROBE: CcmProbeSource =
    CcmProbeSource(crate::ccm_probe::probe_local_ccm);

#[cfg(all(test, not(windows)))]
thread_local! {
    /// 判据装进来的替身。**线程局部** ⇒ 同进程别的判据不受影响（`cargo test` 是多线程跑的）。
    static CCM_PROBE_OVERRIDE: std::cell::Cell<Option<CcmProbeSource>> =
        const { std::cell::Cell::new(None) };
}

/// 装替身，离开作用域自动还原（`assert!` 炸了也还原）。
#[cfg(all(test, not(windows)))]
pub(crate) struct CcmProbeGuard(Option<CcmProbeSource>);

#[cfg(all(test, not(windows)))]
impl Drop for CcmProbeGuard {
    fn drop(&mut self) {
        CCM_PROBE_OVERRIDE.with(|c| c.set(self.0));
    }
}

#[cfg(all(test, not(windows)))]
pub(crate) fn override_ccm_probe(src: CcmProbeSource) -> CcmProbeGuard {
    CcmProbeGuard(CCM_PROBE_OVERRIDE.with(|c| c.replace(Some(src))))
}

/// 这一跳要用的那个取值口。生产上恒是 [`PRODUCTION_CCM_PROBE`]。
#[cfg(not(windows))]
fn ccm_probe_source() -> CcmProbeSource {
    #[cfg(test)]
    if let Some(s) = CCM_PROBE_OVERRIDE.with(|c| c.get()) {
        return s;
    }
    PRODUCTION_CCM_PROBE
}

#[cfg(not(windows))]
fn render_local_ccm(
    action: &LocalPsAction,
    launcher: Option<&str>,
    account: Option<&LaunchAccount>,
    tmux_name: Option<&str>,
) -> Result<String, String> {
    // ★★ `D6 阻-1`：**说不出容器名就不必先付一次 `bash -lic` 的钱**。
    //    下面那个纯函数半在同一格上也拒（同一个 `NO_TMUX_NAME`，不是两份文案），
    //    所以这不是第二条规则，是把**已经确定的拒**提到探测之前。
    //    ⚠ 它同时是判据能驱动 [`launch_local`] 的前提：不早退的话，一条只想看
    //    「最后交出去的是哪一串」的判据会顺带在跑测试的这台机器上起一次 `bash -lic`
    //    —— 那正是本函数与 `render_local_ccm_with` 当初分家要避开的那件事。
    if tmux_name.is_none_or(str::is_empty) {
        return Err(NO_TMUX_NAME.into());
    }
    // ★ 探测与渲染**分家**（P3t-Y3）：探测是这台机器的事实，渲染是纯函数。
    // 合在一起时，判据的结论会跟着「跑测试的机器装没装 ccm」变 —— 而「本机恰好没装
    // ⇒ 判据静默 return ⇒ 报绿」与「真的测过了」在输出上完全一样，那是「0 passed 不是绿」同族。
    // ⚠ 走 [`ccm_probe_source`] 而不是直接调 —— 直接调时「这台机器装没装 ccm」是判据
    //   够不着的一维，于是任何想驱动 [`launch_local`] 的判据都只能自己再抄一份闸
    //   （理由与失效读数住 [`CcmProbeSource`] 头注）。
    let probe = (ccm_probe_source().0)();
    let caps: std::collections::BTreeSet<String> = probe.capabilities.iter().cloned().collect();
    render_local_ccm_with(action, launcher, account, tmux_name, &caps, probe.installed)
}

/// 上一条的纯函数半 —— 能力集与「装没装」由调用方给，本函数不碰这台机器。
#[cfg(not(windows))]
fn render_local_ccm_with(
    action: &LocalPsAction,
    launcher: Option<&str>,
    account: Option<&LaunchAccount>,
    tmux_name: Option<&str>,
    caps: &std::collections::BTreeSet<String>,
    installed: bool,
) -> Result<String, String> {
    use crate::backend::control::ccm_invocation as ci;

    let Some(name) = tmux_name.filter(|n| !n.is_empty()) else {
        return Err(NO_TMUX_NAME.into());
    };
    let sanitized = sanitize_launcher(launcher)?;
    let agent = crate::adapter::active();
    let default_launcher = agent.default_launcher();
    let (sid_owned, cli_action) = match action {
        LocalPsAction::Resume(sid) => (sid.clone(), None),
        LocalPsAction::New => (String::new(), Some(ci::Action::New)),
        // 🔴 `K-R106`：**这一行就是「本机后端产得出 attach 那一句」的全部接线。**
        //    `ci::Action::Attach` 那一支在 `render_ccm_invocation` 里**早于维度循环 return**
        //    （`ccm attach <名>` 不收任何修饰 flag），名字取的是 `Container::Tmux` 那个
        //    —— 也就是下面 `spec.container` 里的 `name`，与本行这个是**同一个** `&str`。
        //    ⇒ 「接进去的那个」与「刚建的那个」在类型上就是同一个名字，不是两处各写一遍。
        LocalPsAction::Attach => (String::new(), Some(ci::Action::Attach { name })),
    };
    let act = cli_action.unwrap_or(ci::Action::Resume { sid: &sid_owned });

    // ★★ 账号那格是本件真正的边界，把它写清楚（P3t-Y2 摸底 · `K-R53` 09-11 重量）。
    //
    // 本机账号是**三态**，而 CLI 的 `account` 维度**恒真**（F05：沉默 = 意外身份切换）
    // ⇒ 每一态都得说得出话来。逐态对：
    //
    // ① `Some(Base)` —— 旧路发 `unset CLAUDE_CONFIG_DIR;`，CLI 发 `--base`。**同义**，可渲染。
    // ② `Some(Named{config_dir, name: Some(n)})` —— CLI 发 `--account <n>`。**可渲染**。
    //    〔`K-R53` 09-11 开的就是这一格〕名字由**调用方**说（`LaunchAccount::Named::name`
    //    的头注写着为什么不从目录推），前端那一侧与 `configDir` 同源
    //    （`accounts.ts::localLaunchAccountNameSync`）。
    //    在这之前本变体只有目录 ⇒ 本机具名账号**一条都进不了容器**，而盘上四个本机拉起
    //    入口里有三个只说得出具名账号 ⇒ 那三条在类型上到不了后端那条路。
    // ②′ `Some(Named{name: None})` —— 调用方只说得出目录（例：分叉时源会话是活的，
    //    继承的是它的目录、没有名字）⇒ 仍然说不出 ⇒ §35 短路 ⇒ 降级回旧路
    //    （旧路发 `export CLAUDE_CONFIG_DIR='<dir>'`）。
    // ③ `None` —— 旧路发**空前缀**，语义是「继承环境里现有的 `CLAUDE_CONFIG_DIR`」。
    //    ⚠⚠ **这一态绝不能映射成 `Base`**：`--base` 是「显式不注入」，与「继承」不是一回事。
    //    映过去 = 把用户 shell 里已有的账号悄悄清掉 —— 那正是 **#75「resume 在错数据目录
    //    找不到会话」** 的病灶形状。**这条今天仍然成立，一个字都不许松。**
    //
    //    🔴🔴 **`K-R89` 09-13：这一格今天关掉了 —— 而它是被一条已到的裁定关掉的，不是被绕过去的。**
    //
    //    这里此前逐字写着「也不能靠『省略 `--account`』兑现……CLI 语法里今天真的没有
    //    『继承』这一态」，并把出路记成「**③ 那一格要动的是 ccm 省略时的默认语义
    //    （产品决定 ＋ `src/backend/control/ccm/plan.rs`）**」。
    //    **那句话是陈账：它在等一个 09-12 就已经到了、而且已经落地的决定。**
    //
    //    〔`DECISIONS.md#R28`，用户 09-12 逐字：「把调用方选中的号静默换掉 /
    //     **不要这么做** / 不是有选默认账号吗? **就用那个**」〕
    //    落地处 `src/backend/control/ccm/plan.rs::resolve_account`
    //    （头注挂着 ✅），省略被拆成**两支，两支都是这一裁要的行为**：
    //      · `CLAUDE_CONFIG_DIR` **非空** ⇒ 保留不覆盖（`R08` 那道 `-z` 闸）= **继承**；
    //      · 裸终端（都没给）⇒ 落 manifest 的 `isDefault` = 「就用那个」。
    //
    //    ⚠⚠ **别把上面那两支压成一句「省略就是继承」** —— 那是本件被反复叮嘱不许照抄的
    //    那种简写。**说得准的那句是**：省略在这条 CLI 上**有确定语义**，而那个语义
    //    正是 `R28` 裁定的两支。⇒ 这一维**说得出话了**，于是不必再 §35 短路。
    //
    //    ⚠ **本机这条路上「继承」拿到的到底是谁的环境**（现打 09-13，别猜）：
    //    送法是 `launch::build_local_posix_argv` ⇒ `bash -lic '<cmd>'`（**login ＋
    //    interactive**）⇒ 用户自己的 rc/profile 先跑，`ccm` 看到的 `CLAUDE_CONFIG_DIR`
    //    就是**用户 shell 里那一个** —— 与旧路（空前缀 ⇒ 由同一个 shell 决定）**同源**。
    //    唯一分岔在「rc 里什么都没设」那一支：旧路落 `~/.claude`，这条落 manifest 默认号
    //    —— **那正是 `R28` 明说要的**（「不是有选默认账号吗? 就用那个」）。
    //
    //    🔴 **远端那半不在本件射程内**：远端是 ssh 过去，那台机器上的继承态不是 monitor 的
    //    环境（`R28` 裁定四逐字）⇒ `WireAccount` 刻意没有对应变体，那一半归 `K-R90`。
    //
    // ⇒ 今天 ① · ② · ③ 渲染得出来，**只剩 ②′ 不行**（缺的是「名字」这条信息本身，
    //    不是语法）。六格今天版逐格住 `tests::THE_SIX_WAYS_THE_OLD_PATH_STILL_WINS`。
    let acct = match account {
        Some(LaunchAccount::Base) => ci::CliAccount::Base,
        // ② / ②′：名字说得出就说，说不出就老实短路 —— `CliAccount::Named{name:None}`
        //         这一格存在的理由就是后者。
        Some(LaunchAccount::Named { name, .. }) => ci::CliAccount::Named {
            name: name.as_deref(),
        },
        // ③：`R28` 之后省略有了确定语义 ⇒ **表得出态了**（`Inherit` 渲染成「不加任何
        //    账号 flag」）。⚠ 不是 `Base`（那是显式清空 = #75），也不是「沉默」。
        None => ci::CliAccount::Inherit,
    };

    let spec = ci::CliSpec {
        is_ssh: false,
        // ★★ 这里**刻意不读** `host_facts` 那个运行期全局量（P3t-Y2 订正 Y1 的形状）。
        //
        // `host_facts` 存在的理由是 `launch_wire` 那条 IPC 路住在 `backend/` 里、不许有平台 cfg
        // （`backend-split` 的 C10）—— 它只能被宿主**告知**。而本文件是宿主自己，
        // 且本函数整个挂在 `#[cfg(not(windows))]` 下 ⇒ 平台事实由**编译器**给，不是运行期给。
        //
        // 差别不是风格：全局量的缺省是 `false`，忘了接线只会**静默失效**。
        // Y2 写判据时当场撞上了这一形态 —— 单元测试进程从不跑 `lib.rs` 的启动段，
        // 于是「具名账号该按 §35 短路」被 `NotSsh` 抢先答了，判据测的根本不是它自称测的东西。
        local_posix: true,
        action: act,
        container: ci::Container::Tmux {
            name,
            send_into: false,
        },
        cwd: None,
        account: acct,
        ccm_sid: match action {
            LocalPsAction::Resume(sid) => Some(sid.as_str()),
            LocalPsAction::New => None,
            // attach 不起 agent ⇒ 没有「这次要打哪个 sid 的标」这回事。
            #[cfg(not(windows))]
            LocalPsAction::Attach => None,
        },
        model: None,
        launcher: sanitized.as_deref().unwrap_or(default_launcher),
        default_launcher,
        args: &[],
        ccm_path: "ccm",
    };
    ci::render_ccm_invocation(&spec, caps, installed).map_err(|r| r.reason())
}

/// L1：按宿主平台把「本地拉起」送出去。
///
/// 这就是 §40「一条路径，transport 是它唯一的差异」在本地这一侧的落点：
/// 上面两个渲染器共享同一个决策，这里只挑一条送法。
///
/// ★★ **P3t-Y2：POSIX 那半的顺序是硬的 —— 渲染器在前，`build_local_posix_command` 在后。**
/// 后者不再是并列的第二条路，而是「渲染器拒了才走」的回落。理由不是对齐，是它今天就坏：
/// 它产的 `cc --resume <sid>` **不带 `--tmux`** ⇒ ccm 走非容器分支 `exec`，
/// 加上 `launch_local_posix` 的 stdio 全 null ⇒ 一个**无 tty、无 tmux** 的进程，
/// 用户敲进去的字会被脚本吃掉。顺序由 `the_local_launch_tries_the_renderer_before_the_old_path` 钉住。
///
/// # ⚠ 本机 `launch` **不经 daemon**，而本机 `kill` 经〔E 阶段全局审计 08-12，待决 `U13`〕
///
/// `daemon_kill.rs::daemon_kill` 那条本机 kill 走的是 daemon 通道（P3 刀 2）；本函数**没有**。
/// 同一个控制面里两条命令走了两条路，而 `control-parity` 的 `C1` 逐字排除的正是
/// 「本地直接 `Command::new` spawn」这条今天的做法 —— 也就是**本函数下游那条**。
///
/// P3t 做的是把它**修好**（渲染器在前、进 tmux、有 tty），**不是**把它换掉。
/// 这不是漏做，也不是已裁 —— 是**没人裁过**：`launch` 与 `kill`/`send-keys` 可能本来就不同类
///（后两者对**已存在**的会话下达指令，而 launch 是**造**一个，`§1.3` 又把最终那次 exec
/// 钉在用户自己的终端进程里）。⇒ 已开 `U13`，别把这一段读成缺口后顺手「补」上。
///
/// # 🔴 返回值〔`K-P5h` `KP5HD1`〕：**这次拉起的身份 token**
///
/// 上一版回的是 `Result<(), String>`（「成了没有」）。本拍把**铸出来的那个 token**
/// 一路交回给调用方 —— 那是 `K-P5g` 现打的卡点（「写侧把 token 铸完就扔」）唯一的解，
/// 也是 [`new_local_session`] 的调用方能拿到「我刚起的那条是哪个会话」的**唯一**入口。
///
/// ⚠ **它不是 sid**：`K-P5 §3 三` 现打「5 处起会话方没有一处在起新会话时知道 sid」。
/// 拿它反查 sid 是**下一跳**的事（前端 `accounts.ts::sidOfLaunch` 与它旁边那张待回填表），
/// 而那一跳必然要**等进程真的跑起来**才问得到 —— 时序那一格归 `KP5HD3`。
///
/// `K-R53`：**中转在场时，本机拉起照旧走旧路**的那句降级理由。
///
/// 它是一条**降级理由**而不是一个 `bool` 分支，理由与 `render_local_ccm` 的每一条 `Err`
/// 相同：这条路上「为什么这台机没进 tmux」只有一个线索，就是 `launch_local` 里那行
/// `tracing::debug!`。把原因写成一个分支条件 ⇒ 那行日志只会说「渲染器降级」而不说是谁降的。
///
/// # 🔴 退役条件〔`K-R61` 09-11 重裁 —— **挡的已经不是同一件事了**〕
///
/// 上一版这里点的退役条件是「往那份 bash `ccm` 的 `capabilities=` 串里加一个 token」，
/// 而**那份脚本 `07e4e72` 就删了** ⇒ 判据活着、前提死了，中间没有任何东西会响。
/// 那正是 `K-R61` 立件的原因。而重裁之后变的**不只是住址，是前提本身**：
///
/// - 旧话逐字是「放行会让**装旧 ccm 的机器**静默吃掉它」。`K34`/`K35` 之后
///   app 自带并自管环境、后端只有一个 ⇒「对面装了**别的** `ccm`」这个概念本身正在退场，
///   **不许再拿它当理由**；
/// - 我们自己这份 `ccm` 的容器路**本来就转发** `ANTHROPIC_BASE_URL`
///   （`src/backend/control/ccm/plan.rs`，daemon 侧有判据真去驱动它）。
///
/// ⇒ 今天的形状是：**转发做到了、也声明了** —— `K-R61` 把 `base-url-across-tmux`
/// 补进了 `src/backend/control/ccm/mod.rs` 的 `CAPABILITIES`，
/// **差的只是下面那一行还没改成探它**。
///
/// ⇒ 退役条件因此是**一行 Rust**（不是「等用户升级」）：把 [`launch_local`] 里那句
/// `relay.is_empty()` 换成「探到 `base-url-across-tmux` 才放行」。
/// 〔`K-R61 §0e` 逐字裁「**本件不动中转的行为**」⇒ 那一行本轮一个字节不动。〕
///
/// ⚠ **为什么本轮不顺手翻掉那一行**（这是一条**可证伪**的条件，不是「以后再说」）：
/// `ccm_probe` 探的是 **PATH 上那个 `ccm`**，不是仓里这份 ⇒ 翻之前得先有人守住
/// 「用户机器上跑的就是 app 自己推的那一份」。那一格今天没人守；
/// 有人守住的那天，这一段与 [`launch_local`] 体内那段一起退役。
#[cfg(not(windows))]
const RELAY_KEEPS_THE_OLD_PATH: &str =
    "这个号走中转，而这一行还没改成「探到 `base-url-across-tmux` 才放行」——\
     转发做到了、也声明了（`src/backend/control/ccm/mod.rs`），\
     差的只是这一行；`K-R61` 只重裁理由，不动行为";

/// 🔴 `K-R106`：[`launch_local`] 被要求 attach 时给出的理由（同上，是理由不是 `bool`）。
#[cfg(not(windows))]
const ATTACH_IS_NOT_A_SPAWN: &str =
    "attach 不经本机拉起那条路：它 spawn 出去、stdio 全 null，接不上任何终端；     `§1.3` 把最终那次 exec 钉在用户自己的终端进程里 ⇒ 本机后端交的是**那一串**     （`render_local_attach`），不是一次 spawn";

/// ⚠ **`Err` 那一支不回 token**：拉起没成功就没有「刚起的那条」可言，
/// 回一个 token 会让调用方去等一条根本不存在的会话。
fn launch_local(
    action: &LocalPsAction,
    launcher: Option<&str>,
    cwd: Option<&str>,
    account: Option<&LaunchAccount>,
    tmux_name: Option<&str>,
) -> Result<String, String> {
    // ★★ `K-H2b`：**这一行就是「那条线」** —— 起会话这一刻把 base URL 指向本机中转。
    //    空串 = 这个号不走中转（`§0e` 裁一：官方号一个字节不进中转）。
    let relay = relay_prefix_for_launch(action, account)?;
    // Windows 那半**逐字不动**（`C12`：「windows不要tmux」）。`tmux_name` 在这一侧
    // 连读都不读 —— 读了就是给「Windows 也进容器」留了个口子。
    #[cfg(windows)]
    let base = {
        let _ = tmux_name;
        build_local_ps_command(action, launcher, account)?
    };
    #[cfg(not(windows))]
    let base = {
        // 🔴 `K-R106`：**attach 不走这条路，而这是结构，不是「暂时没接」。**
        //
        // 本函数最后一跳是 `(launch_sink().0)(&cmd, cwd)` —— `launch_local_posix` 把命令
        // `spawn` 出去、**stdio 全 null**。拿它送 `ccm attach <名>` 的结果是：一个看不见、
        // 摸不着、连不上任何终端的 attach 进程，而用户面前什么都没发生（**还会静默成功**）。
        // ⇒ attach 的正题是把**用户自己的终端**接进去（`§1.3` 把最终那次 exec 钉在那里）。
        // 本机后端在这件事上的产物是**那一串**，不是一次 spawn —— [`render_local_attach`] 交它。
        //
        // ⚠ **它为什么住在这个块里、而不是函数入口**（量具事故留档，别搬回去）：
        //   闸带着一个 `#[cfg(not(windows))]` 属性，放在入口就成了本函数里**第一个**
        //   `#[cfg(not(windows))]`，而六格表「Windows」格的观测口正是
        //   「`fn launch_local(` 到第一个 `#[cfg(not(windows))]` 之间有没有 `let _ = tmux_name;`」
        //   ⇒ 现打当场从 `Structural` 翻成 `Closed`（`K-R106` 第一趟门禁真红过一次）。
        //   **改闸的位置，不改那条观测口** —— 改观测口就是「改判据迁就实现」。
        if matches!(action, LocalPsAction::Attach) {
            return Err(ATTACH_IS_NOT_A_SPAWN.into());
        }
        // ★★ `K-H2b`（08-28 第二拍）：**照旧走 ccm 那条容器路，前缀拼在它外面。**
        //
        // # 第一拍为什么绕开它，第二拍为什么不用绕了
        //
        // 第一拍的判断是：ccm 的容器分支把载荷经 `send-keys` 送进**新起的 tmux 会话**，
        // 而 tmux server 的 `update-environment` 默认列表**不含**这个变量
        // ⇒ 在 `ccm` 外侧 export 的东西**在 tmux 边界被吃掉** ⇒ 照旧走 ccm
        // = **静默地没注入**。于是它两害相权选了「注入成功但没有容器」。
        //
        // 那个坑是真的，但**处置选窄了**：那份已删的 bash `ccm` 里本来就有一段**同形的转发**
        //（R08 那条：把继承来的 `CLAUDE_CONFIG_DIR` 写进载荷**内侧**）。
        // 第二拍照它加了一条 `ANTHROPIC_BASE_URL` 的转发 ⇒ **tmux 边界那一格不再是拦路的那格**。
        //
        // ⚠ **那不违反 `§0e` 裁三**：裁三禁的是「把 ccm 当**收口点**」——
        // 三条生产路结构上绕开它，靠它**注入**会长出一个恒绿的假闸。
        // 而注入仍然发生在 `payload.rs`，ccm 只是**别把已经注入好的变量吃掉**。
        // **「不当收口点」≠「不许碰它」。**
        //
        // 🔴🔴 **订正（`D4 阻-3`）：这里先前逐字写着「于是『走中转』与『有 tmux 容器』
        // 不再互斥」—— 那是假话，今天仍然互斥，只是成因换了。**
        //
        // 🔴🔴🔴 **二次订正（`K-R53` 09-11）：成因又换了一次，而互斥**仍然**成立。**
        //
        // `D4` 那一拍的成因是「具名账号根本进不了 ccm」（`Named` 只有目录没有名字）。
        // **本件把那一格开了** —— `LaunchAccount::Named` 现在带名字，`render_local_ccm`
        // 对它渲染得出 `--account <名字>`。⇒ 那个成因**今天不成立了**。
        //
        // 而互斥没有跟着消失，因为下面这一行**显式**把它保住了。为什么要显式保住：
        //
        // 我们自己这份 `ccm`（`src/backend/control/ccm/plan.rs`）的容器路
        // 那条 `ANTHROPIC_BASE_URL` 转发是**有的**，而先前 `--ccm-probe` 吐的
        // `capabilities=` 串里**没有任何 token 声明它** —— **能力在、声明不在**。
        //
        // 🔴🔴🔴 **三次订正（`K-R61` 09-11）：声明那一半本件补上了，理由跟着重裁。**
        //   上一版这里的理由逐字是「放行会让**装着旧 ccm 的机器**静默吃掉这个变量」，
        //   而 `K34`/`K35` 之后那类机器正在退场 ⇒ **那句话不许再当理由用**。
        //   `base-url-across-tmux` 已进 `src/backend/control/ccm/mod.rs`
        //   的 `CAPABILITIES` ⇒ **转发做到了、也声明了**。
        //
        // ⇒ **中转在场就不走 ccm 容器路**，逐字节维持 `K-H2b` 那一拍的行为
        //   —— `K-R61 §0e` 逐字裁「本件不动中转的行为」，这一行本轮一个字节不动。
        //   这一格的退役条件因此收成**一行 Rust**：把下面那句 `relay.is_empty()`
        //   换成「探到 `base-url-across-tmux` 才放行」。清单住
        //   `tests::a_launch_that_goes_through_the_relay_still_cannot_get_a_tmux_container`。
        //
        // ⚠ **为什么本轮不顺手翻**（可证伪，不是「以后再说」）：`ccm_probe` 探的是
        //   **PATH 上那个 `ccm`**，不是仓里这份 ⇒ 翻之前要先有人守住
        //   「用户机器上跑的就是 app 自己推的那一份」。那一格今天没人守。
        //
        // ⚠ **这一格没买到的**：「变量真的穿过了一次**真** tmux 边界」要真机 tmux，
        // 本轮没量 ⇒ 归 e2e；而按上面那条，**今天在本机中转这条路上仍然走不到**
        // —— 不只是「没量」，是「今天量不到」。
        //
        // ⚠ 写法上刻意让 `render_local_ccm(` 与 `build_local_posix_command(` 在本函数体里
        // **各恰好一处** —— `the_local_launch_tries_the_renderer_before_the_old_path`
        // 用它们的相对位置钉「渲染器在前」，两处就管不住顺序了（第一拍被它逮过一次）。
        //
        // ⚠ 中转那一格（上面那段）**在渲染器之前**短路，而不是在它之后再判一次：
        //   在后面判等于「渲染器说了算，我再推翻一次」——两个决定点、两套判据，
        //   正是 `session-backend.ts` 头注里 #76 那条病的形状。
        let rendered = if relay.is_empty() {
            render_local_ccm(action, launcher, account, tmux_name)
        } else {
            Err(RELAY_KEEPS_THE_OLD_PATH.to_string())
        };
        match rendered {
            Ok(rendered) => rendered,
            Err(why) => {
                // 与远端那条降级**同一种说法**：走回落是正常且预期的路径（没装 ccm 的机器
                // 每次拉起都走它）⇒ `debug` 而不是 `warn`。要查「为什么这台机没进 tmux」时，
                // 这一行是唯一线索。
                tracing::debug!("launch: 本机 CLI 渲染器降级 → 旧路：{why}");
                build_local_posix_command(action, launcher, account)?
            }
        }
    };
    // ★★ `D6 阻-1`：**全仓唯一一处**把中转前缀拼到命令前面的地方，两个平台共用。
    //    先前这里是两处（POSIX 一处 · Windows 一处），而守着「两处都拼了」的是一条
    //    数文本的判据 —— `D6` 的刀 `Y1` 把它打穿了（见 `LaunchSink` 头注）。
    //    合成一处之后，这一行在 Linux 上就被
    //    `the_relay_prefix_is_really_prepended_to_the_command_that_gets_launched` 真驱动到。
    //
    // ★★ `K-P5b` `KP5BD3`：**身份那一句拼在中转前缀与命令体之间**，两个平台共用这一行。
    //    位置不是随手挑的：拼在中转前缀**之前**会把
    //    `the_relay_prefix_is_really_prepended_to_the_command_that_gets_launched` 那条
    //    「送出去的那一串逐字节等于『中转前缀 + 基准串』」的相等断言改掉 ——
    //    而那条断言正是 `D6` 刀 `Y1`（算出来没拼上去）今天唯一的牙。⇒ 拼在它后面，
    //    身份那一段落在两趟的**基准串里**，那条断言逐字不动，两件事各自有各自的牙。
    //
    // 🔴🔴〔`K-P5h` `KP5HD1`〕**本拍在这两行上只做了一件事：把铸出来的 token 留下来。**
    //    上一版是 `let cmd = relay + &launch_identity_prefix(action) + &base;` ——
    //    铸法把 token 渲成前缀之后当场丢掉。现在改调 [`launch_identity`]，
    //    **拼进去的仍是同一个 `prefix`（同一份铸法、同一份渲法、同样的顺序）**，
    //    只是 token 那一半没有被扔掉，而是在拉起成功之后交回给调用方。
    //    ⇒ **拼出来的那一串一个字节没变**，这句话由
    //    `the_minted_identity_token_is_handed_back_to_the_caller` 逐字节对拍钉住。
    let identity = launch_identity(action);
    let cmd = relay + &identity.prefix + &base;
    // ★★ 送出去也走缝：判据装一个记账替身，量的是**真正交出去的那一串**，不是源码里的文本。
    (launch_sink().0)(&cmd, cwd)?;
    Ok(identity.token)
}

// ═════════════════════════════════════════════════════════════════════════════
// `K-H2b`：注入侧的三个判断（**账号 id 从哪来 · 表里有没有它 · 中转在不在**）
// ═════════════════════════════════════════════════════════════════════════════

/// 这次拉起的账号在**中转表**里的 id。
///
/// # ⚠ 它是**推出来的**，不是传下来的 —— 这一格必须写清楚
///
/// [`LaunchAccount::Named`] 只有一个字段 `config_dir`，**没有名字**（`render_local_ccm`
/// 头注里那条「②有 configDir 没名字 ⇒ 说不出 ⇒ 降级」记的就是这件事）。
/// 而中转表按**账号 id** 索引 ⇒ 这里只能拿 `config_dir` 的**末段目录名**当 id：
/// `cc-acct-iso` 的布局逐字是 `~/.claude-accts/<名字>`，`local_accounts.rs` 读出来的
/// 账号名**就是那个目录名**。
///
/// **推错了会怎样**：推出一个表里没有的 id ⇒ [`relay_prefix_for`] 回 `None` ⇒
/// **逐字节走旧路**，不是拼一条会 404 的 URL。⇒ 这一格的失效方向是**保守**的。
///
/// - [`LaunchAccount::Base`]（账号 0）⇒ `None`。**说不出 id 就不注入** ——
///   账号 0 是「显式不注入 `CLAUDE_CONFIG_DIR`」那一档，它在 manifest 里没有目录名。
/// - 参数缺席（调用方没表态）⇒ `None`，同上。
fn relay_account_id(account: Option<&LaunchAccount>) -> Option<String> {
    match account {
        Some(LaunchAccount::Named { config_dir, .. }) => relay_account_id_of_dir(config_dir),
        _ => None,
    }
}

/// 上一条的**纯派生半** —— 「一个 configDir 对应中转表里哪个 id」。
///
/// ★ 抽出来的理由是**只许有一份**：界面那一侧（徽章要显「这个号走不走中转」）问的是
/// **同一个问题**，而它手上也只有 configDir。两边各写一个 basename 规则，
/// 漂开的那天症状是「设置里说走中转、起会话时没走」，而两边看起来都没错。
///
/// ⚠⚠ **界面那一侧今天还没有人调它** —— 那条把这个事实端给前端的路（一条只答本机的
/// tauri 命令）**本轮做到一半退回了**：新注册一条命令会让 `parity_ledger.rs` 的
/// `every_tauri_command_is_declared_in_the_ledger` 当场红（实测报文逐字：
/// 「这些命令已注册但**没进平价对账表**：["relay_routing_for"]」），
/// 而那个文件**不在 `K-H2b` 的写区**。⇒ 本函数今天只有起会话那一侧一个调用方；
/// 它被抽出来是为了「接的时候只有一份规则」，**不是**已经接上了。经过住件文件 `§4`。
pub(crate) fn relay_account_id_of_dir(config_dir: &str) -> Option<String> {
    std::path::Path::new(config_dir.trim())
        .file_name()
        .and_then(|s| s.to_str())
        .map(str::to_string)
}

/// `KH2B7` 的**纯派生半**：给一批 configDir 与一张 id 表，答「哪几个走中转」。
///
/// ★ 抽成纯函数的理由与本模块另外两次一样：`relay_rows()` 要读盘、`relay_running()` 要读进程状态，
/// 而**这条规则本身**（怎么从 configDir 推 id、怎么和表比）不该只能对着真实的家目录跑。
///
/// ⚠ **它答的是「表里有没有这一行」，不是「这个 key 能不能用」** —— 后者要到 claude 那边才知道。
/// ⚠ 也不是「这次拉起会不会真的注入」：那还要过 `relay_running` 那一格（`relay_injection_for`）。
pub(crate) fn relay_routed_subset(config_dirs: &[String], rows: &[String]) -> Vec<String> {
    config_dirs
        .iter()
        .filter(|d| relay_account_id_of_dir(d).is_some_and(|id| rows.iter().any(|r| *r == id)))
        .cloned()
        .collect()
}

/// 中转凭据文件里今天有哪几条账号 id。**读不到就是零条**（零条 ⇒ 谁都不走中转）。
///
/// ⚠ 「读不到」与「一条都没配」在这里**故意同一处置**：两者的正确行为都是
/// 「照旧走官方直连」，而把「读文件失败」变成一次起会话失败，是拿一个**能用的**状态
/// 去换一条错误提示。⇒ 只在日志里留一行。
pub(crate) fn relay_rows() -> Vec<String> {
    let Some(p) = crate::creds_store::resolve_path() else {
        return Vec::new();
    };
    relay_rows_at(&p)
}

/// 上一条剥掉「路径从哪来」之后的那一半〔`D1 阻-6`〕。
///
/// ★ 抽出来的理由与 `creds_store::read_status_at` 那次逐字同一条：不抽的话，这段逻辑
/// **只能对着真实家目录下那份文件跑** —— 而判据不许碰用户的真东西，于是它会变成一格
/// **永远没人量过**的代码。`D1` 的刀 C 实测过那个后果：把本函数整个换成 `Vec::new()`，
/// **1221 passed / 0 failed**。
///
/// # ⚠ 它与中转那侧的人群**不完全一致**，差在哪要写清楚
///
/// 中转装表时会把两类行**丢出表**（`relay::table::build`）：① 账号 id 当不了路由段；
/// ② `base_url` 解析不了。本函数**只筛得掉第 ①** 类（`payload::relay_segment_is_safe`
/// 与 `route::segment_is_safe` 是同一条规则，由 `payload.rs` 那边的头注登记着）。
/// **第 ② 类筛不掉** —— 那要一份 `Base::parse`，而它住 daemon 那一侧、monitor 够不着
/// （单向依赖）。
/// ⇒ **残留的症状**：一行 `base_url` 打错的账号，界面会说「经本机中转」而中转那侧 404。
/// **如实登记，不假装两侧人群相等。**〔`D1` 点名的那条同族，处置是「筛掉能筛的、写清剩下的」。〕
pub(crate) fn relay_rows_at(path: &std::path::Path) -> Vec<String> {
    let Ok(raw) = std::fs::read_to_string(path) else {
        return Vec::new();
    };
    match creds_core::store::parse(&raw) {
        Ok(doc) => creds_core::store::read_accounts(&doc)
            .into_iter()
            .map(|e| e.id)
            .filter(|id| crate::backend::control::payload::relay_segment_is_safe(id))
            .collect(),
        Err(e) => {
            tracing::debug!("中转凭据文件读不成表（照旧走官方直连）：{e:?}");
            Vec::new()
        }
    }
}

/// 纯函数半：给定「账号 id / 表里有哪几行 / 中转在不在」，产出要拼上去的前缀。
///
/// 空串 = **不走中转**（逐字节旧路）。`Err` = 该走但走不了（`KH2B2`②，出声不静默）。
fn relay_prefix_for(
    account_id: Option<&str>,
    rows: &[String],
    running: bool,
    sid: Option<&str>,
    windows: bool,
) -> Result<String, String> {
    let agent = crate::adapter::active().id();
    let url = crate::backend::control::payload::relay_injection_for(
        account_id, rows, running, sid, agent,
    )?;
    Ok(match url {
        None => String::new(),
        Some(u) if windows => crate::backend::control::payload::relay_env_prefix_ps(&u),
        Some(u) => crate::backend::control::payload::relay_env_prefix_posix(&u),
    })
}

/// `D5 阻-1`：那两个「本机事实」的**取值口**，收成一条判据能替换的缝。
///
/// # 为什么非有这条缝不可（这是本件病史的第五层，别退回去）
///
/// 先前钉这两个入参的是一条**扫描型**判据：把 [`relay_prefix_for_launch`] 的体切出
/// 700 字节，断言那个窗口里**有没有**那两段文本。`D5` 现打的读数：在同一个窗口里加一行
/// 把两段文本原样留住的死赋值（一个用不到的绑定就够），同时把真入参换成空表 / 常量
/// ⇒ 文本一处不少、锚点命中数一处不少、**全量门禁四个数与干净树逐字相同**，
/// 而「这个号在不在中转表里」「中转在不在跑」两件事**都不再被问**、中转前缀恒空。
///
/// 病史五层，每一层都是**上一层的修法买到的东西被下一层的量法漏掉**：
/// ① 参数位没有账号 → ② 参数位有、值恒空 → ③ 值到得了、判据只量文本 →
/// ④ 判据搬了家（不再把自己算进被测对象）、**仍在量文本** → ⑤ **文本留住、行为摘掉**。
///
/// ⇒ 处置**不是**再写一个更聪明的文本判据（那是第六层），是**不再量文本**：
/// 两个事实一律从本结构取，判据换一份**会记账的替身**进来，断言两件事 ——
/// ㈠ 它**真的被问过**（替身的计数器涨了）；
/// ㈡ 算出来的前缀**真的随替身给的答案变**（表里有这一行 ⇒ 非空；没有 ⇒ 空；中转没跑 ⇒ `Err`）。
/// ★ 第 ㈡ 条正是治第五层的那一格：把答案问完扔掉（`_unused` 那一形），
///   计数器照样涨，而前缀不再随答案变 ⇒ **红**。
///
/// # 🔴🔴 谁在用这条缝 —— **由一道人群闸数着，不是由这段头注数着**〔`D6 阻-4`，08-29〕
///
/// 先前这里逐字写着「这两个取值口的**生产消费方恰好 2**」，并把那个 2 当成了闸。
/// `D6` 的刀 `E5` 打穿它：在 `lib.rs` 加**第三个**消费方、**绕开这条缝**直接调
/// `history::relay_rows()` / `local_daemon::relay_running()` ⇒ **全量门禁四个数与干净树逐字相同**。
/// ⇒ 那句头注买到的是「**这两处**走缝」，**没买到「所有人都得走缝」**。
/// ★ 定性（PM `§8 裁四`）：**治一个「今天数出来的 N」的过程中，长出了一个新的「今天数出来的 N」。**
///
/// **今天数着这件事的是一道闸**，住 `backend/control/payload.rs::
/// `nobody_reaches_the_relay_take_points_without_going_through_the_seam`（**目录扫描**
/// `src/bridge/src`，不是手写名单）。它钉的是**零调用点**：
/// - `relay_rows()` / `relay_running()` 的**调用形**在生产段全树**各恰好 1 处**（就是它们自己的定义行）；
/// - 裸标识符 `relay_rows` / `relay_running` 各恰好 **2** 处（定义 + 本结构这一处）；
/// - [`platform_is_windows`] **不再数总数**〔ccbus-win 09-10〕：它从今天起有了第二类消费方
///   （`cc_bus::resolve_bash` 只要「是不是 Windows」，走缝要顺带付 `relay_rows()` 读文件
///   与 `relay_running()` 问 daemon 两笔钱），⇒ 那一格换成**点名住址**（`PLATFORM_TAKE_SITES`），
///   函数指针那一半改钉**差值**（裸标识符 − 调用形 == 1 = 只有本结构持有它）。
///   ★ 换制的理由是数个数会**抵消**：「加一处绕缝」＋「删一处正当」总数不变 ⇒ 一声不吭。
///   PM 09-10 在沙箱里现打过这一刀，住址制两条都逮得住（读数住 `audits/ccbus-win-PM.md`）。
///
/// ⇒ 谁绕开这条缝直接调那三个取值口、或把它们的函数指针复制到第二个地方，**当场红**。
/// 今天的两个生产消费方（起会话侧 [`relay_prefix_for_launch`] · 界面侧 `crate::relay_routing_for`）
/// 各有一条行为判据；**闸不数它们有几个**，闸数的是「有没有人绕过去」。
///
/// # 它买不到什么（如实写，别读宽）
///
/// 本结构只管「**问不问**」与「**答案用不用**」。「那三个取值口自己答得对不对」由它们各自的
/// 判据买（[`relay_rows_at`] 那条读真文件的 · `local_daemon::relay_running_really_reads_the_handle_table`）。
/// 而「生产上这条缝里插的**就是**那三个取值口」由 `the_production_relay_facts_are_those_two_take_points`
/// 按**函数地址**对拍 —— 不是按文本。
///
/// ⚠ **仍然没有判据的那两格**（`D6 阻-4` / PM `§8 裁六` 订正过这两栏，别再照旧读）：
/// ㈠ [`relay_rows`] 自己那三行胶水（`creds_store::resolve_path()` + [`relay_rows_at`]）。
///    `D6` 的刀 `Xa` 把它掏空成 `Vec::new()` ⇒ **全绿、门禁四个数与干净树逐字相同**。
///    🔴 **先前这里写的理由（「要动真实家目录，红线不许 ⇒ 做不到」）是假的，解锁条件（「要动 `paths.rs`」）也是假的**：
///    `paths.rs` 从 `dirs::home_dir()` 拼路径 ⇒ 在 Linux 上它读的就是 `$HOME`，
///    而**本 crate 今天就有这个手法的先例**（`local_daemon::become_host_with_home` 里那行
///    `std::env::set_var("HOME", …)`）⇒ **写得出来，一个字节都不用动 `paths.rs`**。
///    **真代价**是这种判据必须 `--test-threads=1` ⇒ 只能住 `#[ignore]` 的 e2e 那条道
///    ⇒ **进不了 `tests/scripts/gate.sh`**。重新裁定的落点就是这一栏 + 件文件 `§4`。
///    🔴 **裁定（`D8 §4` 第 1 条，PM 08-29 采纳，第九轮照抄进这一栏）：
///    这一格是「买得到」，不是「做不到」。** 买法**不在判据这一侧** ——
///    是给 `tests/scripts/gate.sh` 加一条**单线程道**，把 `#[ignore]` 那一族纳进第五个数。
///    🔴 `tests/scripts/gate.sh` **不在 `K-H2b` 的写区** ⇒ 第九轮**没做**，抬给 PM（上报口有一条）。
///    ⚠ 别再把这一栏读成「做不到」：那正是 `D8 §10 裁一` 判过两次的那一形。
/// ㈡ [`platform_is_windows`] 自己的体（`cfg!(windows)`）。在 Linux 上把它写死成 `false`
///    是一次**恒等变换** ⇒ **任何运行时判据都分不出来**（它只在 Windows 上有区别，而
///    Windows 运行时行为本件本来就在「判不了」里）。**登记，不假装钉住了。**
///    ⚠ **过一遍 PM 08-29 那道闸**（「标平台判不了要给得出 `cfg`」）：**本函数没有 `cfg`，
///    在 Linux 上真编译**（`cargo test -p monitor --lib` 跑得到它）⇒ 它**不**属于
///    「不进编译单元」那一族。它判不了的理由是**另一条**：在 Linux 上 `false` 是它的真值，
///    换上去是**恒等变换**（`D8 §1` 的排除表逐字：恒等变换不算「剥掉」那张脸）。
///    ⇒ 两条理由别混：一条是**构造上看不见**，一条是**看得见但换不出第二张脸**。
///    ⚠ 它与先前那条被删的文本判据的差别在于：**调用点**那一格今天买回来了 ——
///    调用点走 `(facts.windows)()`，谁在那里写死一个常量，
///    `the_launch_side_really_asks_those_two_take_points_and_uses_their_answers` 的
///    「PowerShell 那一格」当场红（`D6` 的刀 `Xb` 打的正是调用点那一格）。
#[derive(Clone, Copy)]
pub(crate) struct RelayFactSources {
    /// 「这个号在不在中转表里」——生产恒指 [`relay_rows`]。
    pub(crate) rows: fn() -> Vec<String>,
    /// 「中转在不在跑」——生产恒指 [`crate::local_daemon::relay_running`]。
    pub(crate) running: fn() -> bool,
    /// 🔴 「这台机是不是 Windows」——生产恒指 [`platform_is_windows`]〔`D6 阻-3`，08-29〕。
    ///
    /// 先前这一格在调用点上逐字写着 `cfg!(windows)`，而**它是一个常量表达式** ——
    /// 唯一守着它的是那条被删掉的文本判据（反空真①「窗口里有 `cfg!(windows)`」）。
    /// `D6` 的刀 `Xb`（`cfg!(windows)` → `false`）⇒ 全绿，而生产后果是
    /// **Windows 上中转前缀渲染成 POSIX 形态**（`export …` 塞进 PowerShell 串）⇒ 注入整个失效。
    /// ⇒ 收进本结构之后它成了**可翻的一维**：判据喂 `|| true` 就该拿到 PowerShell 形态。
    pub(crate) windows: fn() -> bool,
}

/// 「这台机是不是 Windows」的生产取值口。**只有这一处**说得出这句话。
///
/// ⚠ 抽成函数不是为了好看：`cfg!(windows)` 写在调用点上时它是个**常量表达式**，
/// 判据没有任何办法让它变。抽出来 + 进 [`RelayFactSources`] 之后，
/// 「调用点用没用这个答案」变成了可翻的一维（见本结构 `windows` 那一格的头注）。
pub(crate) fn platform_is_windows() -> bool {
    cfg!(windows)
}

/// 生产上这条缝里插的那三个取值口。**只有这一处**，判据按地址对拍它。
pub(crate) const PRODUCTION_RELAY_FACTS: RelayFactSources = RelayFactSources {
    rows: relay_rows,
    running: crate::local_daemon::relay_running,
    windows: platform_is_windows,
};

#[cfg(test)]
thread_local! {
    /// 判据装进来的替身。**线程局部** ⇒ 同进程别的判据不受影响（`cargo test` 是多线程跑的）。
    static RELAY_FACTS_OVERRIDE: std::cell::Cell<Option<RelayFactSources>> =
        const { std::cell::Cell::new(None) };
}

/// 装替身，离开作用域自动还原（`assert!` 炸了也还原）。
#[cfg(test)]
pub(crate) struct RelayFactsGuard(Option<RelayFactSources>);

#[cfg(test)]
impl Drop for RelayFactsGuard {
    fn drop(&mut self) {
        RELAY_FACTS_OVERRIDE.with(|c| c.set(self.0));
    }
}

#[cfg(test)]
pub(crate) fn override_relay_facts(facts: RelayFactSources) -> RelayFactsGuard {
    RelayFactsGuard(RELAY_FACTS_OVERRIDE.with(|c| c.replace(Some(facts))))
}

/// 这一拍要用的两个取值口。生产上恒是 [`PRODUCTION_RELAY_FACTS`]。
pub(crate) fn relay_facts() -> RelayFactSources {
    #[cfg(test)]
    if let Some(f) = RELAY_FACTS_OVERRIDE.with(|c| c.get()) {
        return f;
    }
    PRODUCTION_RELAY_FACTS
}

/// 上一条的**接线半**：这台机器上的两个事实（表里有哪几行 · 中转在不在）在这里读。
///
/// ⚠ 两个事实**只从 [`relay_facts`] 取**（理由见 [`RelayFactSources`] 头注：
/// 直接在这里调那两个函数的写法，只能靠「文本在不在」来钉，而那一形 `D5` 已经打穿了）。
fn relay_prefix_for_launch(
    action: &LocalPsAction,
    account: Option<&LaunchAccount>,
) -> Result<String, String> {
    let id = relay_account_id(account);
    let sid = match action {
        LocalPsAction::Resume(sid) => Some(sid.as_str()),
        LocalPsAction::New => None,
        // attach 不起 agent ⇒ 这一跳没有「要往哪个号的中转上指」这个问题。
        // ⚠ 它今天到不了这里（[`launch_local`] 入口就拒了 attach），本臂是**穷尽性**的一半：
        //    哪天有人把 attach 接进那条路，编译器会先逼他读一遍上面这句话。
        #[cfg(not(windows))]
        LocalPsAction::Attach => None,
    };
    let facts = relay_facts();
    relay_prefix_for(
        id.as_deref(),
        &(facts.rows)(),
        (facts.running)(),
        sid,
        (facts.windows)(),
    )
}

// ═════════════════════════════════════════════════════════════════════════════
// `K-P5b`：**起会话方把这条会话的身份塞进下一跳进程的环境**（`L1` 这一处）
// ═════════════════════════════════════════════════════════════════════════════

/// 身份落在进程环境里的那个变量名。**全树只有这一处写下这个字面串** ——
/// `launcher_identity_registry` 那张棘轮表数着它（多一处 ⇒ 红）。
///
/// # 它是什么、不是什么
///
/// 它是**起会话方现铸的一个 token**，不是 sid。`K-P5 §3 三` 现打过一条横贯 5 个起会话方的
/// 结构性事实：**没有一处在起「新」会话时知道 sid**（sid 是 claude 自己起来之后才写进 pidfile 的）
/// ⇒ 身份 token 只能是起会话方现铸的 nonce，resume 那一支可以拿 sid 当那个 nonce。
///
/// ⚠ **不许把「有没有 tmux」或「有没有窗口标题」当它能不能落的判据**（`KP5BD1` 逐字）——
/// 本变量与那两样东西**一格关系都没有**：它是一句 `export`，在哪个终端里、有没有 tmux、
/// 窗口标题写了什么，都不改变它落不落。
pub(crate) const LAUNCH_ID_VAR: &str = "CCM_LAUNCH_ID";

/// 这一次拉起的身份 token。**铸法只有一份** ——
/// 直接调 [`crate::backend::control::payload::route_key_for_session`]，本文件不另写一条规则。
///
/// # 为什么是「共用那一份」而不是「两侧各写一份再对拍」〔`KP5BD1`，照 `K-H2c` 买到的形状〕
///
/// 那一件的读数逐字是「漂开这件事在**结构上不可表示**」。两侧各写一份、再用判据焊住，
/// 买到的只是「今天这几条输入两侧同答」；共用一份实现，**漂开根本没有位置可以发生**。
/// ⇒ 这里刻意**不**写 `match action { Resume(sid) => sid.clone(), New => Uuid::new_v4() }`
///    这种「看起来一样」的第二份 —— 它与那一份的差别只在**白名单回落**那一格
///    （sid 过不了 `relay_segment_is_safe` 时那一份回落到 nonce），而那一格恰恰是
///    「本条真的调了那一份铸法吗」唯一能被判据翻出来的一维。
///
/// # ⚠ 它欠的一笔账（如实登记，别读成缺陷也别读成没有）
///
/// **新开**会话时，中转路由键与本 token 是**两个不同的 nonce**（同一份铸法被调了两次）——
/// 中转那一次在 `payload::relay_injection_for` 里面，本文件够不着它算好的值。
/// 今天不构成缺陷：`mint_route_key` 头注现打登记过「route key 对路由完全惰性、tee 今天零消费者」，
/// 而身份 token 与它**不共享任何消费者**。要它们相等得改 `payload.rs`（本拍只许读它）。
fn launch_identity_token(action: &LocalPsAction) -> String {
    let sid = match action {
        LocalPsAction::Resume(sid) => Some(sid.as_str()),
        LocalPsAction::New => None,
        // attach 不起进程 ⇒ 没有「这一次拉起」可以铸身份。同上，本臂今天到不了。
        #[cfg(not(windows))]
        LocalPsAction::Attach => None,
    };
    crate::backend::control::payload::route_key_for_session(sid)
}

/// 把 token 渲成「设进下一跳进程环境」的那一句前缀。**纯函数**（平台由调用方给）。
///
/// 形状照 `payload::relay_env_prefix_posix` / `relay_env_prefix_ps` 那一对 ——
/// 两个平台的语法真的不同，这不是「两份实现」，是同一件事的两种**书写法**；
/// 决定用哪一种的那一格只有一处（下面 [`launch_identity`] 里那个 `windows`）。
fn launch_identity_env_prefix(token: &str, windows: bool) -> String {
    if windows {
        format!("$env:{LAUNCH_ID_VAR}='{token}'; ")
    } else {
        format!(
            "export {LAUNCH_ID_VAR}={}; ",
            shell_quote_core::posix_quote(token)
        )
    }
}

/// 一次拉起的身份：**铸出来的那个 token** 与**要拼进命令串的那一句前缀**。
///
/// # 🔴 它为什么存在〔`K-P5h` `KP5HD1`〕
///
/// 这个结构是**本拍唯一的行为增量**，而增量只有一句话：**把铸出来的 token 交给调用方**。
///
/// `K-P5g` 交回时现打过一条卡点，逐字：「用 token 回填新会话的 sid 是这条路上最值钱的
/// 那个消费者，它今天**买不到**，卡点是**写侧把 token 铸完就扔**」——
/// 上一版的 `launch_identity_prefix`（本结构的前身，本拍已改名为 [`launch_identity`]）签名是
/// `fn(&LocalPsAction) -> String`，回的是**拼好的前缀**，token 在函数体里当场丢掉
/// ⇒ 全仓**没有任何调用方手上有那个 token**，而 `K-P5 §3 三` 现打的
/// 「5 处起会话方没有一处在起新会话时知道 sid」**正是这个 token 存在的全部理由**。
///
/// # 🔴 additive 的判据钉在哪（别读成「加个字段而已」）
///
/// **拼出来的命令串必须一个字节没变** —— 那是 additive 的全部含义。
/// 钉住它的是 [`tests::the_minted_identity_token_is_handed_back_to_the_caller`] 那一格：
/// 它拿**真正交出去的那一串**与 `前缀 + 基准串` 逐字节相等对拍
///（两边都由生产函数现算，不抄第二份规则）。
/// ⚠ 另有两条老判据在旁边守着同一件事，本拍一个字节都没动它们：
/// `the_launcher_plants_the_session_identity_into_the_process_environment`（身份那一段的形状）
/// 与 `the_relay_prefix_is_really_prepended_to_the_command_that_gets_launched`（中转前缀那一段）。
struct LaunchIdentity {
    /// 铸出来的那个 token 本身。**交给调用方的就是它。**
    token: String,
    /// 渲好的那一句 `export …; ` / `$env:…; `，原样拼进命令串。
    prefix: String,
}

/// 上面两条的**接线半**：铸一个 token，按这台机器是不是 Windows 渲成一句前缀。
///
/// ⚠ 平台那一格**走 [`relay_facts`] 那条缝取**，不写 `cfg!(windows)`：
/// `D6` 的刀 `Xb` 现打过，写在调用点上的 `cfg!(windows)` 是个**常量表达式**，
/// 判据没有任何办法让它变 ⇒ 「Windows 上渲成 POSIX 形态」这一形全绿。
/// 走缝之后它成了可翻的一维（判据喂 `|| true` 就该拿到 PowerShell 形态）。
/// ⚠ 这里**刻意不提 `platform_is_windows` 这个裸标识符** —— `payload.rs` 那道人群闸
/// 数的正是它在生产段里出现几处（定义 1 + 缝里 1），提一次就多一处。
///
/// 🔴〔`K-P5h` `KP5HD1`〕**本拍只改了返回什么，没改铸什么、也没改怎么拼**：
/// 铸法仍是 [`launch_identity_token`]（那一份共用的 `route_key_for_session`），
/// 渲法仍是 [`launch_identity_env_prefix`]，两者的入参与顺序逐字未动 ⇒
/// `prefix` 这一半与上一版那个 `-> String` 的返回值**逐字节相同**。
fn launch_identity(action: &LocalPsAction) -> LaunchIdentity {
    let token = launch_identity_token(action);
    let prefix = launch_identity_env_prefix(&token, (relay_facts().windows)());
    LaunchIdentity { token, prefix }
}

// ═════════════════════════════════════════════════════════════════════════════
// `D6 阻-1`：**最后送出去的那一串**收成一条缝
// ═════════════════════════════════════════════════════════════════════════════

/// 「本机拉起最后把哪一串交出去」的取值口 —— 收成一条判据能替换的缝〔`D6 阻-1`，08-29〕。
///
/// # 为什么非有这条缝不可（这是本件病史的第七层，别退回去）
///
/// 先前钉「前缀真的拼上去了」的是一条**扫描型**判据
/// （`the_relay_prefix_is_really_prepended_to_the_command_that_gets_launched` 的第一版）：
/// 从 `fn launch_local(` 起切 3600 字节，断言那个窗口里**有没有**
/// `relay_prefix_for_launch(action, account)?` · `relay + &`（恰好 2 处）· `let cmd = relay + &base;`。
/// `D6` 的刀 `Y1` 现打：在拼装那一行加
/// `let relay = if relay.is_empty() { relay } else { String::new() };`
/// ⇒ 三样文本**一处不少**（三个锚点数与干净树相同）⇒ **全量门禁四个数与干净树逐字相同**，
/// 而**前缀算出来了没拼上去** —— 本件的正题在生产上被整个摘掉。
/// ⚠ **那条判据自己的头注逐字写着要防的正是这件事**（「算出来却没拼上去，行为上与本件没做完全一样」）
/// —— 威胁模型写对了，买的东西是文本。
///
/// ⇒ 处置**不是**再写一个更聪明的文本判据（那是下一层），是**不量文本**：
/// 把「送出去」收成本结构这一跳，判据换一个**会记账的替身**进来，断言
/// **真正交出去的那一串**以正确的前缀打头、且前缀随 [`RelayFactSources`] 给的答案与
/// **哪个账号**一起变。
///
/// # 顺带被这条缝按平了的一格
///
/// 收缝的同一拍把 [`launch_local`] 里那**两处**拼接（POSIX 一处 · Windows 一处）
/// 合并成了**一处** —— 平台差异现在只剩「`base` 由谁渲」与「送法是哪一个」两格，
/// 而拼前缀那一步两个平台**共用同一行**。
/// ⇒ 先前那条判据的第 ② 颗牙（「两条平台分支各自真的拼上去」）不再需要一条
/// **只能在 Windows 上验证**的断言来守 —— 那一行在 Linux 上就被驱动到了。
#[derive(Clone, Copy)]
pub(crate) struct LaunchSink(pub(crate) fn(&str, Option<&str>) -> Result<(), String>);

/// 生产上这条缝里插的送法。**只有这一处**，判据按地址对拍它。
#[cfg(not(windows))]
pub(crate) const PRODUCTION_LAUNCH_SINK: LaunchSink = LaunchSink(crate::launch::launch_local_posix);
/// 生产上这条缝里插的送法。**只有这一处**，判据按地址对拍它。
///
/// # 🔴 **订正 `D8` 表里的 `F3`：这一格有判据，不是「零感知」**〔`D8 阻-6` / 阻-5，08-29〕
///
/// `D8` 把这一支标成「❌ 没有（**推的，我没打这一刀**）」，理由是「与 `F1` 同属
/// `#[cfg(windows)]`，Linux 上不进编译单元」。**第九轮把这一刀打了，读数与那个推断相反。**
///
/// - **刀**（`§11.6` 形㈠ 的 Windows 版）：加一个 `#[cfg(windows)]` 的新一跳
///   `fn r9_probe_sink(cmd, cwd)`，体里先 `split_once("; ")` 剥掉中转前缀再委托给
///   `launch_powershell_window`，把本 `const` 指向它。锚点 = 本 `const` 的定义，**命中 1**。
/// - **读数**：`cargo test -p monitor --lib` ⇒ **`1236 passed; 1 failed`**，红的正是
///   `payload::nobody_reaches_the_relay_take_points_without_going_through_the_seam`，
///   报文逐字点名「`launch.rs` 之外还有人直接调那两个送法：history.rs: `launch_powershell_window(` × 1」。
///   （快道红 ⇒ 方向安全，按纪律不升全量门。）
///
/// **成因**：那道人群闸是**量文本**的（`guard_core::production_code` + 目录扫描），
/// 而 `production_code` **只剥 `#[cfg(test)]` 段与整行注释，不剥 `#[cfg(windows)]`**
/// ⇒ Windows-only 的源码**在文本这一层是可见的**。
/// ⇒ 🔴 **「带 `#[cfg(windows)]`」蕴含「运行时判据看不见」，不蕴含「所有判据都看不见」。**
/// `F1`（[`crate::launch::launch_powershell_window`] 的**函数体**）仍然买不到 ——
/// 那一刀不新增任何跨文件调用形，量文本的闸够不着它。**两格别合并读。**
#[cfg(windows)]
pub(crate) const PRODUCTION_LAUNCH_SINK: LaunchSink =
    LaunchSink(crate::launch::launch_powershell_window);

#[cfg(test)]
thread_local! {
    /// 判据装进来的替身。**线程局部** ⇒ 同进程别的判据不受影响（`cargo test` 是多线程跑的）。
    static LAUNCH_SINK_OVERRIDE: std::cell::Cell<Option<LaunchSink>> =
        const { std::cell::Cell::new(None) };
}

/// 装替身，离开作用域自动还原（`assert!` 炸了也还原）。
#[cfg(test)]
pub(crate) struct LaunchSinkGuard(Option<LaunchSink>);

#[cfg(test)]
impl Drop for LaunchSinkGuard {
    fn drop(&mut self) {
        LAUNCH_SINK_OVERRIDE.with(|c| c.set(self.0));
    }
}

#[cfg(test)]
pub(crate) fn override_launch_sink(sink: LaunchSink) -> LaunchSinkGuard {
    LaunchSinkGuard(LAUNCH_SINK_OVERRIDE.with(|c| c.replace(Some(sink))))
}

/// 这一拍要用的送法。生产上恒是 [`PRODUCTION_LAUNCH_SINK`]。
pub(crate) fn launch_sink() -> LaunchSink {
    #[cfg(test)]
    if let Some(s) = LAUNCH_SINK_OVERRIDE.with(|c| c.get()) {
        return s;
    }
    PRODUCTION_LAUNCH_SINK
}

/// 薄委托——保留旧函数名与调用点不变（`resume_impl` 只改内部实现，DoD 要求两个
/// `#[tauri::command]` 的签名/行为/错误文案逐字节不变）。
#[cfg(any(windows, test))]
fn build_resume_ps_command(session_id: &str, launcher: Option<&str>) -> Result<String, String> {
    // G3b：本薄委托保持 2 参签名不变（既有测试逐字节钉住它）——账号 0 走这条。
    // 要带账号的调用方直接用 `build_local_ps_command`。
    build_local_ps_command(
        &LocalPsAction::Resume(session_id.to_string()),
        launcher,
        None,
    )
}

/// Batch14-F41：wt.exe/PowerShell 拉起机械抽到 `launch.rs::launch_powershell_window`
/// （与远端 resume/attach 族共用），本函数只剩「构造本地 resume 命令体 + 委托拉起」。
/// 非 Windows：launch 层统一报错（仅 Windows 支持，错误文案改为中文）。
fn resume_impl(
    session_id: &str,
    cwd: &str,
    launcher: Option<&str>,
    account: Option<&LaunchAccount>,
    tmux_name: Option<&str>,
) -> Result<(), String> {
    // 🔴〔`K-P5h`〕**resume 这一支刻意把 token 丢掉，那不是疏忽。**
    // `K-P5g` 现打过：resume 时 token **就是 sid**（`route_key_for_session(Some(sid))` 在 sid
    // 过白名单时原样返回）⇒ 「拿 token 反查 sid」在这一支上退化成
    // 「答案要么是它自己、要么 `None`」，一个布尔谓词，**买不到本件的正题**。
    // 本件的正主是**新开**那一支（见 [`new_local_session`]）—— 那一支才没有 sid。
    launch_local(
        &LocalPsAction::Resume(session_id.to_string()),
        launcher,
        Some(cwd),
        account,
        tmux_name,
    )?;
    tracing::info!("history: resumed sid={session_id}");
    Ok(())
}

/// F96（#62）：本地「在该目录起**新**会话」的 PowerShell 命令体——薄委托（同上，DoD 要求
/// 行为逐字节不变）。硬约束（用户 2026-07-15）：agent 名 / resume flag 全走活跃适配器，
/// 本函数不出现 agent 字面量。
#[cfg(any(windows, test))]
fn build_new_session_ps_command(launcher: Option<&str>) -> Result<String, String> {
    build_local_ps_command(&LocalPsAction::New, launcher, None)
}

/// F96（#62）：历史页右键「在该目录起新会话」——本地分支。远端分支走前端
/// `runRemoteLauncher`（复用 F53）。在 `cwd` 起一个全新会话（无 sid、无 resume）。
///
/// # 🔴 返回值〔`K-P5h` `KP5HD1`〕：**这次拉起的身份 token**
///
/// 上一版回 `Result<(), String>`。本件把 [`launch_local`] 交出来的那个 token 原样回给前端 ——
/// **这条命令是全仓唯一「起一条新会话」的 tauri 入口**，也就是唯一一处
/// 「起会话方手上有 token、而这条会话还没有 sid」的地方。
///
/// ⚠ **token 不是 sid，也不许被当成 sid 用**。前端拿它去做的事只有一件：
/// 在这条会话真的跑起来之后，用 `accounts.ts::sidOfLaunch` 从 `--session-accounts`
/// 的行里把 sid **反查**出来（`KP5HD2`）。
/// ⚠ **它是个内部 nonce**：不许显示给用户（同 `K-P5g` 那条判据的口径）。
#[tauri::command]
pub fn new_local_session(
    cwd: String,
    launcher: Option<String>,
    account: Option<LaunchAccount>,
) -> Result<String, String> {
    // F96：起新会话**依赖 cwd 定位**（不像 resume 靠 sid）——cwd 非空且不是现存目录（项目被
    // 移动/删除）就明确报错，别静默在默认目录起会话 + 弹假成功 toast。`launch_powershell_window`
    // 只把存在的 cwd 作窗口起始目录、失效则回落默认，对 resume 无害、对 new-session 是错目录。
    if !cwd.is_empty() && !std::path::Path::new(&cwd).is_dir() {
        return Err(format!("目录不存在，无法在此起新会话：{cwd}"));
    }
    // ★★ `K-H2b` `D1 阻-1`：**账号这一格是本轮加的，加它的理由要写清楚。**
    //
    // 原注释逐字：「起**全新**会话不继承任何账号（那是『新开一个』的语义，不是分叉）」。
    // 那句话**今天仍然对**，它说的是「不从某条旧会话继承」。⚠ 但它被读成了「所以这条路
    // 不该有账号参数」，而后果是：**这条主路上一个账号都说不出**，于是
    // ① 起会话落到 shell rc 里那个默认号上（`config_dir_prefix_posix` 头注逐字点名的静默串号），
    // ② 中转那一格**永远拼不出路由键**（没有账号 id ⇒ `relay_account_id` 回 `None`）。
    // ⇒ 现在收**调用方明说的那一个**：前端传的是「用户此刻选中的当前账号」，
    //   **不是**从别的会话继承来的。参数缺席仍然是「没表态」，逐字节旧行为。
    //
    // P3t-Y2：起新会话这条**暂不传名字**（`None` ⇒ 渲染器诚实降级回旧路）。
    // 名字只许由 `mintTmuxName` 铸，在这里补一个默认名就是 F13 那个坑的第三次。
    let launch_id = launch_local(
        &LocalPsAction::New,
        launcher.as_deref(),
        Some(&cwd),
        account.as_ref(),
        None,
    )?;
    // ⚠ **日志里不写 token**：它是身份凭据形态的 nonce，而 tracing 的 ERROR 那一档会被
    //   `bindErrorToast` 刷到界面上 —— 内部 nonce 一个字节都不该往那条路上走。
    tracing::info!("history: new local session in {cwd}");
    Ok(launch_id)
}

/// 🔴 `K-R106`〔用@09-13〕**本机后端产 `attach` 那一句** —— `K-R54` 表第 3 行的收尾。
///
/// # 用户逐字，这是本命令的全部依据
///
/// > 「新起一个会话之后，把你的终端接进那个会话那一句 `tmux attach`，归谁产？」
/// > 「**归本机后端就好了啊**」〔`DECISIONS.md#R61` 裁定三〕
///
/// # 它**只渲染，不执行**，而这不是偷懒
///
/// `§1.3` 把最终那次 exec 钉在**用户自己的终端进程**里。[`launch_local`] 那条路是
/// `spawn` + stdio 全 null ⇒ 拿它送 attach 等于什么都没发生（还会静默成功）。
/// ⇒ 本机后端在这件事上的产物就是**那一串**；谁把终端接上去由调用方决定。
///
/// # 它走的是**既有那条渲染路**，不是第二条
///
/// [`render_local_ccm`] → [`render_local_ccm_with`] → `ccm_invocation::render_ccm_invocation`
/// —— 与本机 `new` / `resume` 逐字同一条路，同一份能力探测（[`CcmProbeSource`] 那条缝）、
/// 同一条 `NO_TMUX_NAME`。**没有为 attach 新开任何一个决定点**
/// （两个决定点、两套判据正是 issue #76 那条病的形状，`session-backend.ts` 头注记着它）。
///
/// # 🔴 ⚠ 它今天**不是** `#[tauri::command]`，而这是量出来的，不是选择
///
/// 第一版给它挂了 `#[tauri::command]`，想着「注册那一行归 PM」。**门禁当场红两条**
/// （`tests/ipc/commands.vitest.ts` 的 `C04a`）：「这些命令声明了却没注册 ⇒ 前端调不到」
/// 与「TS 静态看不见的命令集变了」。⇒ 本仓**不接受**「声明了不注册」这个中间态。
///
/// 把它接出去要动**四处**，其中三处不在 `K-R106` 的写区：
///
/// | 处 | 在写区吗 | 要做什么 |
/// |---|---|---|
/// | 本函数 | ✅ | 加回 `#[tauri::command]` |
/// | `src/bridge/src/lib.rs` 的 `generate_handler!` | ❌ | 注册一行 |
/// | `src/bridge/src/parity_ledger.rs` 的 `LEDGER` | ✅ | **必须同一拍**加一行，否则它当场判「已注册但没进对账表」 |
/// | `src/ipc/commands.ts` ＋ `tests/ipc/commands.vitest.ts` | ❌ | 加包装层；后者那个**命令总数**是写死的（现打 147），要 +1 |
///
/// # 🔴 `K-R109`（09-13）：**接出去了** —— 上面那张「要动四处」的表已经全部落地
///
/// 四处逐一：本函数挂回 `#[tauri::command]`（就在下面）· `lib.rs` 的 `generate_handler!`
/// 注册一行 · `parity_ledger::LEDGER` 同一拍加一行 · `src/ipc/commands.ts` 加包装层。
/// 前端那条 `↗`（`src/remote-launch-run.ts::runLocalResumeIntoExistingTmux`）改成问它要。
/// ⇒ 「`Attach` 没有生产构造点」那条诚实边界**本轮消掉**，连带非 test 的 `cargo build`
/// 那条 `dead_code` 一起（是**注册**杀掉它的，不是接线 —— `generate_handler!` 展开出来的
/// 那个包装函数就是第一个非 test 调用方；读数与量法住 `tests/evidence/K-R109-deathvalue.md`）。
///
/// # 入参为什么是 `String` 而不是 `&str`
///
/// 现打（09-13，量具 `tests/evidence/K-R109-ruler.py` 的 `command-params` 一格；
/// 分母 = 剥掉整行 `//` 注释后 `src/bridge/src/**.rs` 里 `#[tauri::command]` 紧跟着的
/// **149** 处 `fn`（= 148 个唯一命令名 ＋ `bring_monitor_to_front` 的第二份 cfg 实现））：
/// **入参出现 `&str` 的 0 处**。⚠ 不剥注释会读成 16 处 —— 那 16 处全是散文里逐字提到
/// 这个属性、而它下面碰巧跟着一个内部 `fn`（`parity_ledger.rs::registered_commands`
/// 的头注逐字警告过这个形状）。**一个数不写清它的剥法，就是半句假话。**
///
/// 命令入参要从 IPC 那一侧反序列化出来，借用形态在这条路上不是「省一次拷贝」，
/// 是**给自己找一个只在某些 tauri 版本上成立的前提**。⇒ 与全仓同形，owned。
/// 判据侧的调用点跟着改一处（`.to_string()`）。
///
/// # Windows：拒，而且理由是定框
///
/// `C12`〔用 08-12〕逐字「windows不要tmux」⇒ 那台机器上没有 tmux 容器，
/// 也就没有「接进那个容器」这件事。[`LocalPsAction::Attach`] 这个变体本身就挂着
/// `#[cfg(not(windows))]`（与 [`render_local_ccm_with`] 同一条 cfg）——**编译期就不存在**。
///
/// 🔴 **而本函数不能再整个挂那条 cfg 了，这是注册面逼出来的**：`generate_handler![…]`
/// 收的是一串**路径**，`#[cfg]` 挂不进去（那个宏不解析属性）⇒ 命令名在 Windows 上必须
/// 也解析得到，否则 `cargo check --target x86_64-pc-windows-gnu`（门禁 `winchk` 那一格）
/// 当场编不过。⇒ **cfg 收进函数体**：Windows 那一支直接拒，理由就是 `C12`，
/// 不是运行期探测。仓里的先例是 `lib.rs::bring_monitor_to_front`（两侧各一份实现）——
/// 本函数取的是同一条路的另一种写法（一个声明、体内分叉），因为 Windows 那一支
/// 只有一行、单独立一个同名 `fn` 反而多一处要对齐的签名。
#[tauri::command]
pub fn render_local_attach(tmux_name: String) -> Result<String, String> {
    #[cfg(not(windows))]
    {
        render_local_ccm(&LocalPsAction::Attach, None, None, Some(tmux_name.as_str()))
    }
    #[cfg(windows)]
    {
        let _ = tmux_name;
        Err(WINDOWS_HAS_NO_TMUX_CONTAINER.to_string())
    }
}

/// Windows 上 [`render_local_attach`] 的拒词。**它是定框 `C12` 的字面**，不是一句提示语。
#[cfg(windows)]
pub(crate) const WINDOWS_HAS_NO_TMUX_CONTAINER: &str =
    "Windows 上没有 tmux 容器（定框 `C12`〔用 08-12〕逐字「windows不要tmux」）\
     ⇒ 也就没有「把终端接进那个容器」这件事。要翻它先回去翻定框。";

// === 内部：jsonl 级扫描 ===
//
// 〔`K-R97` 09-12〕项目级那一段（遍历 records 根 + 从头部抠 cwd）**不在这里了** ——
// 本机的项目列表改问后端要 `--list-projects`，那两件事今天由后端一趟做完。
// 从头部抠 cwd 这件事从此**全仓只剩一处**（后端那一份）；两份实现之间那道
// 「窗口 30 vs 40 行、认不认非 user 记录」的登记分歧随之消失。

fn analyze_jsonl(
    path: &Path,
    metadata: &HistoryMetadata,
    map: &SessionMap,
) -> Option<HistorySessionEntry> {
    let session_id = path.file_stem()?.to_str()?.to_string();
    let file = File::open(path).ok()?;
    let updated_at = file
        .metadata()
        .ok()
        .and_then(|m| m.modified().ok())
        .map(systime_to_ms)
        .unwrap_or(0);
    let total_size = file.metadata().ok().map(|m| m.len()).unwrap_or(0);

    let mut cwd: Option<String> = None;
    let mut ai_title: Option<String> = None;
    let mut first_user_excerpt = String::new();
    let mut started_at: i64 = 0;
    let mut message_count: u32 = 0;
    // issue #12: 第一条带 forkedFrom 的 user/assistant 就锁住（典型整 session 共享）
    let mut forked_from_session_id: Option<String> = None;
    let mut forked_from_message_uuid: Option<String> = None;
    // Batch11-F32：CC 后台分身会话探测（记录级 sessionKind:"bg"——官方 resume
    // 选择器同款信号）。JsonlRecord 不透传未知字段，故对原始行做字符串探测
    // （两种空格形态；仅徽标用途，误报面可忽略）。
    let mut is_bg = false;

    let reader = BufReader::new(file);
    for line in reader.lines().map_while(Result::ok) {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if !is_bg
            && (trimmed.contains(r#""sessionKind":"bg""#)
                || trimmed.contains(r#""sessionKind": "bg""#))
        {
            is_bg = true;
        }
        let rec = match parse_line(trimmed) {
            Ok(Some(r)) => r,
            _ => continue,
        };
        match &rec {
            JsonlRecord::User {
                cwd: c,
                timestamp,
                message,
                forked_from,
                ..
            } => {
                if cwd.is_none() {
                    if let Some(v) = c {
                        cwd = Some(v.clone());
                    }
                }
                if started_at == 0 {
                    started_at = iso_to_ms(timestamp);
                }
                if first_user_excerpt.is_empty() {
                    let text = extract_user_text(message);
                    if !text.is_empty() {
                        first_user_excerpt = truncate_chars(&text, 120);
                    }
                }
                if forked_from_session_id.is_none() {
                    if let Some(fk) = forked_from {
                        forked_from_session_id = Some(fk.session_id.clone());
                        forked_from_message_uuid = Some(fk.message_uuid.clone());
                    }
                }
                message_count += 1;
            }
            JsonlRecord::Assistant {
                timestamp,
                forked_from,
                ..
            } => {
                if started_at == 0 {
                    started_at = iso_to_ms(timestamp);
                }
                if forked_from_session_id.is_none() {
                    if let Some(fk) = forked_from {
                        forked_from_session_id = Some(fk.session_id.clone());
                        forked_from_message_uuid = Some(fk.message_uuid.clone());
                    }
                }
                message_count += 1;
            }
            JsonlRecord::AiTitle { ai_title: t, .. } => {
                // 后出现的覆盖（Claude 在会话里可能多次更新 ai-title）
                ai_title = Some(t.clone());
            }
            JsonlRecord::CustomTitle {
                custom_title: t, ..
            } => {
                // Claude Code v2.1.x 起新名字，语义同 ai-title
                ai_title = Some(t.clone());
            }
            _ => {}
        }
    }

    // 无任何可识别记录 → 跳过（异常 / 空文件）
    if started_at == 0 && message_count == 0 && cwd.is_none() {
        // 但仍然给一个最小条目，让用户能看到并删除空文件
        if total_size == 0 {
            return None;
        }
    }

    let project_path = cwd.unwrap_or_default();
    let project_name = Path::new(&project_path)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or(&project_path)
        .to_string();

    let entry_meta = metadata
        .entries
        .get(&session_id)
        .cloned()
        .unwrap_or_default();

    Some(HistorySessionEntry {
        session_id: session_id.clone(),
        project_path: project_path.clone(),
        project_name,
        ai_title,
        first_user_excerpt,
        is_bg,
        started_at,
        updated_at,
        jsonl_path: path.to_string_lossy().into_owned(),
        // 本机这条路有真相源 ⇒ `Some`（`K-R92`：远端那条路答不了时给 `None`）。
        is_live: Some(map.is_session_active(&session_id)),
        message_count_approx: message_count,
        starred: entry_meta.starred,
        custom_title: entry_meta.custom_title.clone(),
        hidden: entry_meta.hidden,
        forked_from_session_id,
        forked_from_message_uuid,
        origin: None, // 本地扫描路径恒为本地
    })
}

/// 从 user 消息的 content 抠出纯文本预览。content 可以是 string 或 [Block...]。
fn extract_user_text(message: &ApiMessage) -> String {
    use serde_json::Value;
    match &message.content {
        Value::String(s) => clean_user_text(s),
        Value::Array(arr) => {
            for block in arr {
                if let Some(t) = block.get("type").and_then(|t| t.as_str()) {
                    if t == "text" {
                        if let Some(s) = block.get("text").and_then(|t| t.as_str()) {
                            let cleaned = clean_user_text(s);
                            if !cleaned.is_empty() {
                                return cleaned;
                            }
                        }
                    }
                }
            }
            String::new()
        }
        _ => String::new(),
    }
}

/// 去掉 Claude Code CLI 注入的 prompt 包装（<task-notification>/<system-reminder> 等），
/// 与前端 cards/index.ts 的 isInternalUserNoise 同一意图（这里更宽松，只是预览用）。
fn clean_user_text(s: &str) -> String {
    let mut out = s.to_string();
    // 简单去 tag 包裹（不需要完美，预览而已）
    for tag in [
        "task-notification",
        "system-reminder",
        "local-command-caveat",
        "local-command-stdout",
    ] {
        let open = format!("<{tag}>");
        let close = format!("</{tag}>");
        while let (Some(i), Some(j)) = (out.find(&open), out.find(&close)) {
            if j > i {
                out.replace_range(i..j + close.len(), "");
            } else {
                break;
            }
        }
    }
    out.trim().to_string()
}

fn truncate_chars(s: &str, n: usize) -> String {
    let mut out = String::new();
    for (i, ch) in s.chars().enumerate() {
        if i >= n {
            out.push('…');
            break;
        }
        if ch == '\n' || ch == '\r' {
            out.push(' ');
        } else {
            out.push(ch);
        }
    }
    out
}

// === metadata 持久化 ===

fn metadata_path() -> Option<PathBuf> {
    Some(paths::resolve_monitor_data_dir()?.join("history-metadata.json"))
}

pub(crate) fn load_metadata() -> Result<HistoryMetadata, String> {
    let path = metadata_path().ok_or("no monitor data dir")?;
    if !path.exists() {
        return Ok(HistoryMetadata::default());
    }
    let raw = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
    serde_json::from_str::<HistoryMetadata>(&raw).map_err(|e| {
        tracing::warn!("history-metadata.json parse failed ({e}); using empty");
        e.to_string()
    })
}

fn save_metadata(m: &HistoryMetadata) -> Result<(), String> {
    let path = metadata_path().ok_or("no monitor data dir")?;
    // 走 utils::atomic_write_json：Windows ReplaceFileW + dst-not-exist fallback，
    // 非 Windows 单步 rename，全程原子。比早期"write tmp + remove + rename"三步更
    // 不易丢文件——后者中途 crash 用户的 star/重命名/隐藏全失。
    crate::utils::atomic_write_json(&path, m).map_err(|e| e.to_string())
}

// === 时间换算 → utils 归并（P3）===
// `systime_to_ms` / `parse_iso8601_ms` / `now_ms` 已搬到 crate::utils。
// 本地保留两个适配 helper（带 unwrap_or(0) 兜底）以最小化修改面。

fn iso_to_ms(iso: &str) -> i64 {
    // Claude 写的 timestamp 形如 "2026-05-20T15:11:42.345Z"；失败返 0
    // （前端会显示为 1970，看得见但不崩）。
    crate::utils::parse_iso8601_ms(iso).unwrap_or(0)
}

#[cfg(test)]
#[path = "../../../tests/bridge/history_title_coverage.rs"]
mod title_coverage;

#[cfg(test)]
#[path = "../../../tests/bridge/history_tests.rs"]
mod tests;
