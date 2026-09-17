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
#[cfg_attr(test, ts(export, export_to = "../../src/generated/"))]
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
#[cfg_attr(test, ts(export, export_to = "../../src/generated/"))]
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
#[cfg_attr(test, ts(export, export_to = "../../src/generated/"))]
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
/// 与 `usage::aggregate_usage_all` 那条路同形。
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
///   sidecar 不会。极少见，但不是零（同 `usage.rs` 那条路记着的差异）。
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
            |args| local_query::run_query(env!("CCM_TARGET_TRIPLE"), args),
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

/// Codex 一个会话的 list 元信息。`pub(crate)` 供 usage.rs（F5 用量）复用枚举。
pub(crate) struct CodexSessionInfo {
    pub(crate) sid: String,
    pub(crate) path: PathBuf,
    /// session_meta.cwd（分组键；缺 → "" → 归「(codex)」组）。
    pub(crate) cwd: String,
    mtime_ms: i64,
}

/// 枚举本机 Codex 会话：walk `<codex_root>/sessions` 日期树 `rollout-*.jsonl`，读**首行** session_meta
/// 取 cwd。Codex 未启用（无 `~/.codex/sessions`）→ 空 vec（零回归）。`pub(crate)` 供 usage.rs（F5）复用。
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
#[cfg_attr(test, ts(export, export_to = "../../src/generated/"))]
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
/// ⚠ 那句「本模块**不 attach**，一次都不」（`remote-daemon-proto/src/control/launch.rs`
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
        /// ⇒ `ccm` 当场 `die`（`remote-daemon-proto/src/control/ccm/argv.rs` 认不出这个名字 = 退出码 2）
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
    //    （产品决定 ＋ `remote-daemon-proto/src/control/ccm/plan.rs`）**」。
    //    **那句话是陈账：它在等一个 09-12 就已经到了、而且已经落地的决定。**
    //
    //    〔`DECISIONS.md#R28`，用户 09-12 逐字：「把调用方选中的号静默换掉 /
    //     **不要这么做** / 不是有选默认账号吗? **就用那个**」〕
    //    落地处 `remote-daemon-proto/src/control/ccm/plan.rs::resolve_account`
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
///   （`remote-daemon-proto/src/control/ccm/plan.rs`，daemon 侧有判据真去驱动它）。
///
/// ⇒ 今天的形状是：**转发做到了、也声明了** —— `K-R61` 把 `base-url-across-tmux`
/// 补进了 `remote-daemon-proto/src/control/ccm/mod.rs` 的 `CAPABILITIES`，
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
     转发做到了、也声明了（`remote-daemon-proto/src/control/ccm/mod.rs`），\
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
        // 我们自己这份 `ccm`（`remote-daemon-proto/src/control/ccm/plan.rs`）的容器路
        // 那条 `ANTHROPIC_BASE_URL` 转发是**有的**，而先前 `--ccm-probe` 吐的
        // `capabilities=` 串里**没有任何 token 声明它** —— **能力在、声明不在**。
        //
        // 🔴🔴🔴 **三次订正（`K-R61` 09-11）：声明那一半本件补上了，理由跟着重裁。**
        //   上一版这里的理由逐字是「放行会让**装着旧 ccm 的机器**静默吃掉这个变量」，
        //   而 `K34`/`K35` 之后那类机器正在退场 ⇒ **那句话不许再当理由用**。
        //   `base-url-across-tmux` 已进 `remote-daemon-proto/src/control/ccm/mod.rs`
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
/// `src-tauri/src`，不是手写名单）。它钉的是**零调用点**：
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
///    ⇒ **进不了 `scripts/gate.sh`**。重新裁定的落点就是这一栏 + 件文件 `§4`。
///    🔴 **裁定（`D8 §4` 第 1 条，PM 08-29 采纳，第九轮照抄进这一栏）：
///    这一格是「买得到」，不是「做不到」。** 买法**不在判据这一侧** ——
///    是给 `scripts/gate.sh` 加一条**单线程道**，把 `#[ignore]` 那一族纳进第五个数。
///    🔴 `scripts/gate.sh` **不在 `K-H2b` 的写区** ⇒ 第九轮**没做**，抬给 PM（上报口有一条）。
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
/// | `src-tauri/src/lib.rs` 的 `generate_handler!` | ❌ | 注册一行 |
/// | `src-tauri/src/parity_ledger.rs` 的 `LEDGER` | ✅ | **必须同一拍**加一行，否则它当场判「已注册但没进对账表」 |
/// | `src/ipc/commands.ts` ＋ `tests/ipc/commands.vitest.ts` | ❌ | 加包装层；后者那个**命令总数**是写死的（现打 147），要 +1 |
///
/// # 🔴 `K-R109`（09-13）：**接出去了** —— 上面那张「要动四处」的表已经全部落地
///
/// 四处逐一：本函数挂回 `#[tauri::command]`（就在下面）· `lib.rs` 的 `generate_handler!`
/// 注册一行 · `parity_ledger::LEDGER` 同一拍加一行 · `src/ipc/commands.ts` 加包装层。
/// 前端那条 `↗`（`src/remote-launch-run.ts::runLocalResumeIntoExistingTmux`）改成问它要。
/// ⇒ 「`Attach` 没有生产构造点」那条诚实边界**本轮消掉**，连带非 test 的 `cargo build`
/// 那条 `dead_code` 一起（是**注册**杀掉它的，不是接线 —— `generate_handler!` 展开出来的
/// 那个包装函数就是第一个非 test 调用方；读数与量法住 `evidence/K-R109-deathvalue.md`）。
///
/// # 入参为什么是 `String` 而不是 `&str`
///
/// 现打（09-13，量具 `evidence/K-R109-ruler.py` 的 `command-params` 一格；
/// 分母 = 剥掉整行 `//` 注释后 `src-tauri/src/**.rs` 里 `#[tauri::command]` 紧跟着的
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
mod title_coverage {
    /// ★ **每个带 `*title` 字段的 `JsonlRecord` 变体都必须被标题抽取吃到**〔audit-0805 08-06〕。
    ///
    /// # 它钉的是一个**有历史先例**的缺口
    ///
    /// 标题抽取那段 `match` 只认两种记录，其余走 `_ => {}` —— 静默忽略。
    /// 对绝大多数记录类型这是对的（它们与标题无关），
    /// **但对「又冒出一种承载标题的记录」就不是** ——
    /// 而那件事**真的发生过**：Claude Code v2.1.x 把 `ai-title` 改名成 `custom-title`，
    /// 仓里因此多了 `JsonlRecord::CustomTitle` 这一支。
    /// 那次是**靠人发现的**：改名之后标题会静默消失，没有任何判据会红。
    ///
    /// ⇒ 本条把「谁承载标题」这件事变成可机检的：
    /// 人群 = `messages.rs` 的 `JsonlRecord` 里**带 `*title` 字段**的变体（今天 2 个）；
    /// 性质 = 它必须出现在 `history.rs` 的标题抽取段里。
    /// 加第三种标题记录而忘了接 ⇒ 红。
    ///
    /// ⚠ 人群刻意**不按变体名**取（`*Title` 结尾那种）——名字是可以随便起的，
    /// 而「带一个叫 `xxx_title` 的字段」才是它承载标题的实据。
    #[test]
    fn every_title_bearing_record_is_consumed_by_the_extractor() {
        let msgs = include_str!("messages.rs");
        let b = msgs
            .find("enum JsonlRecord")
            .expect("找不到 `JsonlRecord` 定义 —— 记录类型搬家了，本条要跟着改");
        let e = msgs[b..].find("\nfn ").map_or(msgs.len(), |k| b + k);
        let block = &msgs[b..e];

        let mut cur = String::new();
        let mut bearers: Vec<String> = Vec::new();
        for line in block.lines() {
            let ind = line.len() - line.trim_start().len();
            let s = line.trim_start();
            if ind == 4 && s.chars().next().is_some_and(|c| c.is_ascii_uppercase()) {
                cur = s
                    .chars()
                    .take_while(|c| c.is_ascii_alphanumeric())
                    .collect::<String>();
            }
            if !cur.is_empty()
                && s.contains("title")
                && s.contains(':')
                && !bearers.contains(&cur)
                && s.split(':')
                    .next()
                    .is_some_and(|f| f.trim().ends_with("title"))
            {
                bearers.push(cur.clone());
            }
        }
        // 抽取器自检：一个都没抠到 ⇒ 下面的对拍会零命中地绿。
        assert!(
            bearers.len() >= 2,
            "只抠到 {} 个带 `*title` 字段的变体（08-06 实测 2：AiTitle / CustomTitle）\
             —— 抽取坏了，本条此刻是空转的：{bearers:?}",
            bearers.len()
        );

        let here = include_str!("history.rs");
        let missing: Vec<&String> = bearers
            .iter()
            .filter(|v| !here.contains(&format!("JsonlRecord::{v}")))
            .collect();
        assert!(
            missing.is_empty(),
            "这些记录类型带 `*title` 字段，却没被 `history.rs` 的标题抽取接住：{missing:?}\n\
             ⚠ 后果不是报错，是**标题静默消失** —— 会话列表上那一行变回默认名，\n\
             而没有任何判据会红。这件事真发生过一次（`ai-title` 改名成 `custom-title`），\n\
             那次是靠人发现的。\n\
             ⇒ 在标题抽取的 `match` 里给它加一条具名臂。"
        );
    }
}

#[cfg(test)]
mod tests {

    /// 〔audit-0805 08-06〕**防命令注入的那道校验，此前一条判据都没有。**
    ///
    /// # 怎么找到的
    ///
    /// 新先验：抽「只被一处调用」的生产函数。`has_bad_chars` 在全仓只出现两次
    /// （定义 + 一处调用），顺着它找到唯一消费者 [`validate_config_dir_ps`] ——
    /// 而它 5 处出现里**没有一处是测试**。
    ///
    /// 它守的是「**拒绝拼入命令**：非法 CLAUDE_CONFIG_DIR」，也就是把一个用户可控的
    /// 目录名塞进 PowerShell 命令串之前的最后一道闸。失效形态是**静默放行**：
    /// 校验松掉不会让任何测试变红，而后果是命令串里多了一个 `;` 或 `$(...)`。
    ///
    /// ⚠ 它 `#[cfg(any(windows, test))]` —— **Linux 的测试构建里是编译的**，
    /// 所以这一族与 `ROADMAP §5` 的 3y（Windows-only 代码本机连编译都不碰）**不同**：
    /// 这里没有平台借口，只是没人写。
    ///
    /// # 用例挑的是「每一条拒绝理由各一发 + 两条不许误拒」
    ///
    /// 不许误拒那两条是有来历的：头注逐字记着 Phase G 审计抓出的真 bug ——
    /// 早先两边共用「必须 `/` 开头 + 禁 `\`」，于是真实的 Windows 账号目录
    /// `C:\Users\z\.claude-accts\z` **必被拒**，「本机分叉时选具名账号」在主平台 100% 失败。
    /// ⇒ 反向用例把那个回归钉住。
    #[test]
    fn the_config_dir_validator_rejects_every_injection_shape() {
        // ★ 先证明夹具走得通：两种平台的合法绝对路径都必须过。
        for ok in [
            "/home/z/.claude",
            "C:\\Users\\z\\.claude-accts\\z",
            "\\\\server\\share\\claude",
        ] {
            assert!(
                validate_config_dir_ps(ok).is_ok(),
                "合法路径被拒了：{ok:?} —— 这正是 Phase G 抓出的那个真 bug 的形状\n\
                 （早先禁 `\\` ⇒ 每个 Windows 账号目录都过不去，主平台 100% 失败）"
            );
        }

        // ① 非绝对 / 根 / `..` 穿越（两种分隔符、中间与结尾各一）
        for bad in [
            "relative/path",
            ".claude",
            "/",
            "/home/../etc",
            "/home/..",
            "C:\\a\\..\\b",
            "C:\\a\\..",
        ] {
            assert!(
                validate_config_dir_ps(bad).is_err(),
                "路径形态没被拒：{bad:?}"
            );
        }

        // ② 控制字符与 C1 段（`\u{85}` 在很多终端里不可见）
        for bad in [
            "/home/z\u{0}/x",
            "/home/z\n/x",
            "/home/z\u{85}/x",
            "/home/z\u{9f}/x",
        ] {
            assert!(
                validate_config_dir_ps(bad).is_err(),
                "控制字符没被拒：{bad:?}"
            );
        }

        // ③ shell 元字符 —— **逐个**过，不是抽一个代表。
        //    ★ 自检：集合非空，否则这个循环是空转的。
        assert!(
            !crate::backend::control::payload::SHELL_META_COMMON.is_empty(),
            "`SHELL_META_COMMON` 空了 —— 下面这轮是空转的"
        );
        for c in crate::backend::control::payload::SHELL_META_COMMON.chars() {
            let bad = format!("/home/z{c}/x");
            assert!(
                validate_config_dir_ps(&bad).is_err(),
                "shell 元字符 {c:?} 没被拒 —— 它会被原样拼进命令串"
            );
        }

        // ④ 同形欺骗字符（走 `acct_core::is_deceptive_char` 那条并集）
        //    先确认这个字符确实被那张表认得，否则用例本身可能选错了字。
        // ★ 这条自检当场救过一次：第一版选的是 `\u{2044}`（FRACTION SLASH，肉眼像 `/`），
        //   它**不在** `acct_core` 那张表里 —— 若没有这条自检，下面那条会因为别的原因红/绿，
        //   而我会以为「欺骗字符这一支验过了」。
        for deceptive in ['\u{200B}', '\u{202E}', '\u{FEFF}', '\u{00A0}'] {
            assert!(
                acct_core::is_deceptive_char(deceptive),
                "样本字符 {deceptive:?} 不在 `acct_core` 的欺骗字符表里 —— \
                 换一个，否则下面那条在测别的东西"
            );
            assert!(
                validate_config_dir_ps(&format!("/home/z{deceptive}etc")).is_err(),
                "同形/不可见字符 {deceptive:?} 没被拒 —— 它在终端里看不见，却会原样进命令串"
            );
        }
    }

    /// ★★ **把「本机 resume 到底跑什么」钉在真构造器上**〔audit-0805 F08 / 报告 B-2〕。
    ///
    /// # 此前那条判据在替代码说好话
    ///
    /// `launch.rs::local_and_remote_share_the_same_payload` 用的是**手写夹具**
    /// `"… && ccm --tmux claude --resume s1"`，而它**从不调用**真正的 payload 构造器。
    /// 那个夹具里有 `--tmux`，生产里没有 —— **判据恰好体现了生产违反的那个假设**。
    ///
    /// # 本条钉的是**现状**，不是理想
    ///
    /// 它断言生产 payload 里**确实没有容器**（既无 `--tmux` 也无 `cct`）。
    /// 这不是在祝福这个行为 —— 是让它**不能再悄悄变、也不能再被一条漂亮的夹具盖住**。
    /// 真要改成进容器，改完这条会红，那时才是带着证据做决定的时刻。
    ///
    /// ⚠ 功能后果（claude 在 `stdin=/dev/null` 下具体怎么表现）**红线内测不了**，
    /// 本条只钉**命令串**这一层可判据的事实。
    #[test]
    fn the_local_resume_payload_has_no_session_container_today() {
        let choice = local_launch_choice(&LocalPsAction::Resume("s1".into()), None)
            .expect("resume s1 应该能构造出来");
        let rendered = match &choice {
            LocalLaunchChoice::Fixed(c) => c.clone(),
            LocalLaunchChoice::Probe {
                preferred,
                fallback,
                ..
            } => format!("{preferred} | {fallback}"),
        };
        assert!(
            rendered.contains("--resume") && rendered.contains("s1"),
            "抽取器自检：构造出来的串里连 `--resume s1` 都没有 —— 切错东西了：{rendered}"
        );
        assert!(
            !rendered.contains("--tmux") && !rendered.split_whitespace().any(|w| w == "cct"),
            "★★ **回落那条路**产出了会话容器（`--tmux` / `cct`）—— 本条不该再绿。\n\
             \n\
             ⚠ **P3t-Y3 翻面**：本条**测什么没变，自陈换了**。它量的从来只是\n\
             `local_launch_choice`，也就是 **P3t 之后的回落路**（渲染器拒了才走的那条）。\n\
             P3t 之前那等价于「本机 resume 没有容器」；**现在不等价了** ——\n\
             本机先过 `render_local_ccm`，渲得出来就带 `--tmux`。\n\
             ⇒ 本条现在钉的是「**回落路仍是无容器的那条**」：它是诚实降级的落点，\n\
             不是第二条并列的路（顺序由 `the_local_launch_tries_the_renderer_before_the_old_path` 钉）。\n\
             真要连回落也进容器，请连同 `launch.rs` 与 `src/fork-start.ts` 那两条头注一起改。\n\
             实得：{rendered}"
        );
    }

    /// ★★ **P3t-Y3 的翻面另一半 —— 不许翻成更弱的一条。**
    ///
    /// 上面那条钉「回落路没有容器」。光有它，**整个 P3t 被回退掉也不会红**
    /// （回落路本来就该没容器，回退之后它还是没容器）。
    /// ⇒ 必须再钉正面事实：**渲染得出来的时候，那条串真的带 `--tmux`**。
    ///
    /// 判据怎么失效（`P2s-Y3` / `P3 刀 0` 各栽过一次）：翻面时只留「旧事实不再成立」，
    /// 丢掉「新事实成立」。所以这里同时钉容器名**就是传进去的那个**
    /// —— 若它被换成 Rust 自己铸的名字，`U11` 那个撞名坑就回来了。
    /// 判据自己给能力集 —— **不问这台机器**。
    ///
    /// = `CLI_REQUIRED_CAPS`（每次调用都要的静态能力）**加上两条 §37 维度能力**
    /// （`account` 恒真维度要 / `model` 条件式维度要）。第一版只给了前者，
    /// 当场报「维度 account 需要远端 ccm 能力 account」—— 那正是 §37 把两类能力
    /// 分开的意义：静态那张表**不是**全集，照它拼会漏。
    ///
    /// 缺能力时该怎样，由 `ccm_invocation` 自己那两条（`MissingCap` / `DimensionNeedsCap`）钉；
    /// 本文件只需要一个「能力齐」的输入。
    #[cfg(not(windows))]
    fn caps_of_a_current_ccm() -> std::collections::BTreeSet<String> {
        crate::backend::control::ccm_invocation::CLI_REQUIRED_CAPS
            .iter()
            .map(|c| (*c).to_string())
            .chain(["account".to_string(), "model".to_string()])
            .collect()
    }

    // ═══════════════════════════════════════════════════════════════════════
    // `K-R106` `KR106D1`：**本机后端产得出 `attach` 那一句**，而且它落在刚建的那个会话上
    // ═══════════════════════════════════════════════════════════════════════

    /// 后端那份 `ccm` 的 **argv 解析**半。跨半边编译期边，登记住
    /// `cross_half_edge_registry::CROSS_EDGES`。
    #[cfg(not(windows))]
    const CCM_ARGV_SRC: &str = include_str!("../../remote-daemon-proto/src/control/ccm/argv.rs");

    /// 后端那份 `ccm` 的 **计划 + 等价 shell 渲染**半。同上。
    #[cfg(not(windows))]
    const CCM_PLAN_SRC: &str = include_str!("../../remote-daemon-proto/src/control/ccm/plan.rs");

    /// 后端那份 `ccm` 把 `Plan::Attach` 渲成什么 —— **逐字**。
    ///
    /// 钉整行而不是钉 `"tmux attach"` 四个字：`=名:` 那个**精确匹配形**是承重的
    /// （裸 `-t <名>` 会打到兄弟会话上，`session-backend.ts::exactTarget` 头注记着实测）。
    /// 只钉动词的话，把 `={name}:` 改成 `{name}` 照样绿，而那一刀的后果是接错会话。
    #[cfg(not(windows))]
    const CCM_ATTACH_RENDER: &str =
        r#"Plan::Attach { name } => format!("tmux attach -t {}", sq(&format!("={name}:")))"#;

    /// 🔴🔴 `KR106D1`〔用@09-13「**归本机后端就好了啊**」〕：
    /// **本机后端产得出 `attach` 那一句，而且那一句落在它刚建的那个会话上。**
    ///
    /// # 它为什么不判「枚举里有 `Attach` 这个词」
    ///
    /// 那是本件单子逐字点名的失效方向：**加个变体不接线照样绿**。
    /// ⇒ 本条一个字都不读源码里的枚举，它**驱动生产渲染路**
    /// （[`render_local_ccm_with`]，本机 `new`/`resume` 走的同一条），
    /// 从**渲出来的那两串话本身**里把会话名读回来比。
    ///
    /// # 四段各买什么（别读成一段）
    ///
    /// | 段 | 它挡住的那一刀 |
    /// |---|---|
    /// | ① 名字形状 | `K-R87` 那次 `ccm-oneshot-` 两形都不命中 ⇒ 建出来的是**失管会话** |
    /// | ② 建/接同名 | attach 那一臂渲成 `ccm attach <sid>` 或干脆掉进 `_ => new` 兜底 |
    /// | ③ 跨半边 | 我们产的这一串，后端那份 `ccm` 真把它读成「接进这个名字」 |
    /// | ④ fail-closed | 旧路 / spawn 那条路被要求 attach 时**拒**，不许凑一个出来 |
    ///
    /// # ⚠ 它买不到什么（如实写）
    ///
    /// - ③ 是**文本级的两侧同形**，不是真跑一次 `ccm`。真跑那一格归 e2e
    ///   （`ccm-print-parity` 里「attach 到 cc-p1」那条）。**本条不声称跑过。**
    /// - 🔴 **`K-R109`（09-13）订正：前端在问它要了。** 原文写「前端今天还没在问它要：
    ///   `src/remote-launch-run.ts` 那条 `↗` 仍问 `SESSION_BACKEND.attach` 要」——
    ///   那四处接线本轮全部落地（属性 · `generate_handler!` · `LEDGER` · 包装层），
    ///   `runLocalResumeIntoExistingTmux` 现在 `await commands.render_local_attach(…)`。
    ///   ⚠ **本条钉的仍然只是「后端产得出」** —— 「有人在用」那一半由前端那一侧的判据钉
    ///   （`tests/remote-launch-run.vitest.ts` 的 `KR109D2` 两条），两处别混成一处。
    #[test]
    #[cfg(not(windows))]
    fn the_local_backend_renders_an_attach_that_lands_on_the_session_it_just_created() {
        let caps = caps_of_a_current_ccm();
        // ① 名字不是随手起的：它要过 `gate_core` 那两形之一，否则起出来的会话主路认不出、
        //    杀不掉 —— `K-R87` 那次 `ccm-oneshot-<x>` 两形都不命中，就是这个坑。
        const NAME: &str = "s1abcdef-cc";
        assert!(
            gate_core::is_ccm_tmux_name(NAME),
            "本条自己用的名字就过不了 Gate 2 —— 那么下面量到的一切都在量一个失管会话"
        );
        // 阴性对照：`K-R87` 那个形状**必须**不过，否则上面那条是空真。
        let the_r87_shape = format!("ccm-oneshot-{}", "abcdef");
        assert!(
            !gate_core::is_ccm_tmux_name(&the_r87_shape),
            "{the_r87_shape:?} 居然过了 Gate 2 —— 上面那条断言此刻什么都没买到"
        );

        let acct = LaunchAccount::Base;
        // 建那一句（今天就产得出的）与接那一句（本轮加的）**走同一条渲染路、同一个名字**。
        let created = render_local_ccm_with(
            &LocalPsAction::New,
            None,
            Some(&acct),
            Some(NAME),
            &caps,
            true,
        )
        .expect("建那一句本来就渲染得出来 —— 渲不出说明本条的前提变了，回来重裁");
        let attach = render_local_ccm_with(
            &LocalPsAction::Attach,
            None,
            Some(&acct),
            Some(NAME),
            &caps,
            true,
        )
        .expect(
            "本机后端产不出 attach —— `KR106D1` 的正题就是这一句〔用@09-13「归本机后端就好了啊」〕",
        );

        // ② 会话名从**那两串话本身**里读回来，不是拿常量对常量。
        let created_target = created
            .split_whitespace()
            .find_map(|t| t.strip_prefix("--tmux="))
            .unwrap_or_else(|| {
                panic!("建那一句里没有 `--tmux=<名>`，它根本没建容器 —— 下面两条会空转：{created}")
            });
        let mut toks = attach.split_whitespace();
        assert_eq!(
            toks.next(),
            Some("ccm"),
            "attach 那一句不是在调后端的命令行入口（`K26`：`ccm` 就是它）：{attach}"
        );
        assert_eq!(
            toks.next(),
            Some("attach"),
            "\n★ 本机后端渲出来的**动作不是 attach**（实得整串：{attach}）。\n\
             最可能的形状：`Attach` 那一臂掉进了 `render_ccm_invocation` 的 `_ => new` 兜底 ——\n\
             那一刀的后果不是「没接上」，是**另起一条 claude**，而用户以为回到了原会话。"
        );
        let attach_target = toks
            .next()
            .unwrap_or_else(|| panic!("attach 那一句没有目标会话名：{attach}"));
        assert_eq!(
            toks.next(),
            None,
            "`ccm attach <名>` 不收任何修饰 flag（`ccm_invocation` 那一支早于维度循环 return），\
             多出来的东西说明它走了别的分支：{attach}"
        );
        assert_eq!(
            attach_target, created_target,
            "\n★★ **接的不是刚建的那个会话**：建的是 {created_target:?}，接的是 {attach_target:?}。\n\
             这两个名字在生产代码里本来就是同一个 `&str`（`render_local_ccm_with` 的 `name`，\n\
             同时喂给 `Action::Attach` 与 `Container::Tmux`）—— 它们不相等只有一种可能：\n\
             有人给 attach 那一臂另开了一个名字来源。"
        );
        assert!(
            gate_core::is_ccm_tmux_name(attach_target),
            "接进去的那个名字过不了 Gate 2（{attach_target:?}）—— 主路认不出它，杀不掉它"
        );

        // ③ 跨半边：我们产的这一串，后端那份 `ccm` 真把它读成「接进这个名字」。
        let argv_prod = guard_core::production_code(CCM_ARGV_SRC);
        let plan_prod = guard_core::production_code(CCM_PLAN_SRC);
        assert!(
            argv_prod.len() > 3_000 && plan_prod.len() > 5_000,
            "跨半边语料只读进来 {} / {} 字节 —— 下面三条此刻在空转",
            argv_prod.len(),
            plan_prod.len()
        );
        let attach_verb = format!("\"{}\" => {{", "attach");
        assert!(
            argv_prod.contains(attach_verb.as_str()),
            "后端那份 `ccm` 的 argv 解析里，位置动作 `{attach_verb}` 那一支不见了 —— \
             我们产的这一句它读不成 attach"
        );
        assert!(
            argv_prod.contains("o.attach_name = v.clone()"),
            "`ccm attach <名>` 后面那个位置参数不再落进 `attach_name` —— \
             那么「接哪一个」这条信息在后端那半就断了"
        );
        assert!(
            plan_prod.contains(CCM_ATTACH_RENDER),
            "\n后端那份 `ccm` 把 `Plan::Attach` 渲成的那一行变了（本条钉的整行：\n  {CCM_ATTACH_RENDER}\n\
             ）。⚠ 承重的不只是 `tmux attach` 四个字，还有 `=名:` 那个**精确匹配形** ——\n\
             裸 `-t <名>` 会按「精确名 → 名字开头 → glob」解析，打到兄弟会话上\n\
             （`src/session-backend.ts::exactTarget` 头注有 tmux 3.6 的实测）。"
        );

        // ④ fail-closed：旧路与 spawn 那条路被要求 attach 时**拒**。
        let old = build_local_posix_command(&LocalPsAction::Attach, None, None);
        assert!(
            old.as_ref().is_err_and(|e| e.contains("旧路产不出 attach")),
            "\n★ 旧路（`build_local_posix_command`）居然给 attach 渲出了东西：{old:?}\n\
             它只会拼一个**拉起器** ⇒ 渲出来的是「另起一条 claude」。\n\
             **静默产出比拒绝坏得多**：用户以为接回了原会话，实际两条都在跑。"
        );
        let spawned = launch_local(&LocalPsAction::Attach, None, None, None, Some(NAME));
        assert!(
            spawned.as_ref().is_err_and(|e| e.contains("stdio 全 null")),
            "\n★ `launch_local` 收下了 attach：{spawned:?}\n\
             那条路把命令 spawn 出去、stdio 全 null ⇒ 一个接不上任何终端的 attach 进程，\n\
             而它**还会静默成功**。attach 的正题是把用户自己的终端接进去（`§1.3`）。"
        );

        // ⑤ **产出口真的走这条路**：本机后端那个出口喂一份确定的 ccm 事实进去，
        //    拿到的必须与上面那一句**逐字节相同**（不是「长得像」）。
        fn a_current_ccm() -> crate::ccm_probe::CcmProbeResult {
            crate::ccm_probe::CcmProbeResult {
                installed: true,
                version: Some("0.0.0-判据替身".to_string()),
                capabilities: caps_of_a_current_ccm().into_iter().collect(),
                build: None,
            }
        }
        let _probe = override_ccm_probe(CcmProbeSource(a_current_ccm));
        assert_eq!(
            render_local_attach(NAME.to_string()).expect("本机后端那个产出口渲不出来"),
            attach,
            "\n★ `render_local_attach` 交出去的那一串与渲染路现算的不是同一串 ——\n\
             那说明产出口自己又走了一条（两个决定点、两套判据，正是 #76 那条病的形状）。"
        );
    }

    #[test]
    #[cfg(not(windows))]
    fn the_rendered_local_command_really_carries_the_container() {
        let name = "s1abcdef-cc";
        let cmd = render_local_ccm_with(
            &LocalPsAction::Resume("s1abcdef".into()),
            None,
            Some(&LaunchAccount::Base),
            Some(name),
            &caps_of_a_current_ccm(),
            true,
        )
        .expect("账号 0 + 有名字 + 能力齐 ⇒ 必须渲染得出来");
        assert!(
            cmd.contains("--tmux"),
            "渲出来的本机命令里没有 `--tmux` —— 那就还是**无 tty、无 tmux** 的老样子，\n\
             用户敲进去的字会被脚本吃掉。实得：{cmd}"
        );
        assert!(
            cmd.contains(name),
            "容器名不是传进去的那个（`{name}`）—— 名字只许由前端 `mintTmuxName` 铸，\n\
             在 Rust 里另铸一个就是 F13 修掉的撞名坑（见 ROADMAP `U11`）。实得：{cmd}"
        );
    }

    /// ★★ **P3t-Y2 给上面那条补一句射程**（不改它测什么，只改它自称守什么）。
    ///
    /// 上面那条量的是 `local_launch_choice` —— 也就是**回落那条路**的构造器。
    /// P3t-Y2 之后本机拉起先过 CLI 渲染器（`render_local_ccm`），渲不出来才落到它。
    /// ⇒ 「本机 resume 没有容器」这句话**从此不再等价于**「`local_launch_choice` 没有容器」：
    /// 前者要看渲染器渲不渲得出来，后者只看回落。上面那条**照旧恒绿**，但它守的人群窄了。
    ///
    /// 本条不是重复它，是把「窄了多少」写成可执行的：今天 `render_local_ccm` 的**三格纯逻辑拒绝**
    /// 决定了生产上谁能进容器。三格全拒 ⇒ 生产行为与 P3t 之前逐字节相同（Y2 是零行为改动的接线）。
    /// Y2b 前端接线之后，第一格会开，那时上面那条的自陈就该改了。
    ///
    /// # 🔴 `K-R53` 09-11：**本条的名字今天已经比它测的东西宽了一格，别照名字读它**
    ///
    /// 函数名逐字是「前端今天送得出的**每一形**都被拒」——**那句话现在是假的**：
    /// 具名账号带上名字之后渲染得出来（那正是本件开的那一格）。本条测的仍然都成立，
    /// 但它的人群已经缩到「**说不出名字的**那几形」：没有会话名 · 未表态账号 · 只有目录。
    ///
    /// 🔴 **`K-R89` 09-13：人群又缩了一格，而且这一次连「都被拒」那个动词都不对了。**
    /// 「未表态账号」那一形**今天渲染得出来**（`R28`：省略 `--account` 有确定语义）——
    /// 本条的 ② 因此从「必拒」翻成「必渲染得出，且不许带 `--base`/`--account`」。
    /// ⇒ 今天真正**被拒**的只剩**两形**：没有会话名 · 只有目录没有名字。
    /// ⚠ **仍然刻意不改名**（同 `K-R53` 那一拍的理由：改判据名要同拍跑 `pb doc`，
    /// 而本件写区里没有那份生成区）。**全人群那一条仍在继任者手里**
    /// [`every_local_account_shape_gets_a_named_verdict_from_the_backend_path`]，
    /// 而「六格今天各自是什么」在 [`THE_SIX_WAYS_THE_OLD_PATH_STILL_WINS`]。
    ///
    /// ⚠ **刻意不改名**：改判据的名字要同拍跑 `pb doc`（生成区会连带打红），
    /// 而本件的写区里没有那份生成区。⇒ 如实登记在这里，并把**全人群**那一条交给继任者
    /// [`every_local_account_shape_gets_a_named_verdict_from_the_backend_path`]
    /// （它逐格点名、加变体编译不过）。**两条一起读才是今天的分母。**
    #[test]
    #[cfg(not(windows))]
    fn the_local_renderer_refuses_every_shape_the_front_end_can_send_today() {
        let base = LaunchAccount::Base;
        let named = LaunchAccount::Named {
            config_dir: "/home/u/.claude-accts/z".into(),
            name: None,
        };
        let act = LocalPsAction::Resume("s1".into());

        // ① 没名字 —— 名字只许 `mintTmuxName` 铸，Rust 这侧不许补默认值（F13 那个坑）。
        for no_name in [None, Some(""), Some("   ")].into_iter() {
            let no_name = no_name.filter(|n: &&str| !n.trim().is_empty());
            let r = render_local_ccm_with(
                &act,
                None,
                Some(&base),
                no_name,
                &caps_of_a_current_ccm(),
                true,
            );
            assert!(
                r.as_ref().is_err_and(|e| e.contains("tmux 会话名")),
                "没有会话名时必须拒 —— 在 Rust 里铸一个名字就是 F13 修掉的撞名坑第三次。实得：{r:?}"
            );
        }

        // ② 未表态账号（`None`）—— 🔴 **`K-R89` 09-13：这一格从「必拒」翻成「渲染得出来」**。
        //    翻它的不是本判据的口味，是 `DECISIONS.md#R28`（用户 09-12）：省略 `--account`
        //    在 `ccm` 上有确定语义（`plan.rs::resolve_account` 的两支）。
        //    ⚠ **翻的只有「拒不拒」，没翻的那半必须原样守住**：渲染出来的那一串里
        //    **不许出现 `--base`** —— 那是把「继承环境」偷换成「显式清空」＝ #75 病灶。
        let r = render_local_ccm_with(
            &act,
            None,
            None,
            Some("s1abcdef-cc"),
            &caps_of_a_current_ccm(),
            true,
        );
        let cmd = r.as_ref().unwrap_or_else(|e| {
            panic!(
                "未表态账号今天必须渲染得出来（`R28` 之后省略有确定语义）。\n\
                 若它又回到短路，请先回 `DECISIONS.md#R28` 看那一裁是不是被推翻了，\n\
                 别在这里把闸悄悄加回来。实得降级理由：{e}"
            )
        });
        assert!(
            !cmd.contains("--base"),
            "未表态账号被渲染成了 `--base` —— 那是把「继承环境」偷换成「显式清空」，\n\
             正是 #75「resume 在错数据目录找不到会话」。实得：{cmd}"
        );
        assert!(
            !cmd.contains("--account"),
            "未表态账号被渲染成了 `--account <某个号>` —— 那是替用户挑了一个号。实得：{cmd}"
        );

        // ③ 具名账号 —— `LaunchAccount::Named` 只有 configDir、没有名字，而 CLI 只会
        //    `--account <名字>` ⇒ §35 短路。**理由必须是「说不出」，不是别的**：
        //    reason 是生产侧唯一的降级线索，换一个理由就是换一条诊断。
        let r = render_local_ccm_with(
            &act,
            None,
            Some(&named),
            Some("s1abcdef-cc"),
            &caps_of_a_current_ccm(),
            true,
        );
        let reason = r.expect_err(
            "具名账号今天渲染不出来 —— 若它成功了，请先确认 `--account` 的名字是从哪来的",
        );
        assert!(
            reason.contains("account"),
            "具名账号的降级理由该指向 account 维度（§35 短路），实得：{reason}"
        );
    }

    /// ★★★ `K-R53` `KR53D1`：**本机账号的每一形，后端那条路渲染得出来吗** —— 逐格点名。
    ///
    /// # 它判的是**分母**，不是可达性
    ///
    /// 上面那条 (`the_local_renderer_refuses_every_shape_the_front_end_can_send_today`)
    /// 钉的是 P3t-Y2 那一刻的事实「**全拒**」。本条是它的继任者：把
    /// [`LaunchAccount`] 的全部形状加上「参数缺席」逐格喂一次，
    /// **每一格都要说得出自己该是 `Ok` 还是 `Err`、以及 `Err` 的理由指向哪**。
    ///
    /// 失效方向逐字（`KR53D1`）：「再加一个入口而它复用了那个缺一态的旧函数」——
    /// 加一个变体 ⇒ 下面这张表的 `match` 不穷尽 ⇒ **编译不过**，不是静默漏一格。
    ///
    /// # 🔴 缺席那一格：**09-13 `K-R89` 之前是红的，今天是绿的** —— 翻它的是一条裁定，不是一次放宽
    ///
    /// 「参数缺席」的语义是**继承环境**（旧路发空前缀）。这里此前逐字写着「三格里没有一格
    /// 逐字等于『继承』⇒ 那是**产品决定** ＋ 改 `remote-daemon-proto/src/control/ccm/plan.rs`」，
    /// 并把这一格钉成 `Err`。**那段话在 09-12 就过期了，而它一直挂在盘上等一个已经到了的决定。**
    ///
    /// 〔`DECISIONS.md#R28`，用户 09-12 逐字：「把调用方选中的号静默换掉 / **不要这么做** /
    ///  不是有选默认账号吗? **就用那个**」〕⇒ 那个产品决定做了，而且**落地了**：
    /// `remote-daemon-proto/src/control/ccm/plan.rs::resolve_account` 头注挂着 ✅，
    /// 省略被拆成两支，**两支都是这一裁的一部分**：
    ///
    /// | 目标 shell 里有没有 `CLAUDE_CONFIG_DIR` | 旧路（空前缀） | ccm 省略 `--account` | ccm `--base` |
    /// |---|---|---|---|
    /// | 有，= X | 用 X | **尊重 X**（`R08` 那道 `-z` 闸不触发）= 继承 ✅ | `unset` ⇒ 用 `~/.claude` ❌ |
    /// | 没有 | 用 `~/.claude` | **落 manifest 默认号** ✅〔`R28`：「就用那个」〕 | 用 `~/.claude` |
    ///
    /// ⚠ **第二行那一格从 ❌ 翻成 ✅ 的是「该不该」，不是「是什么」** —— 行为一个字节没动，
    /// 动的是对它的判断（`R28` 裁定零逐字：「本裁改的不是行为，是『这是不是我们要的』」）。
    /// ⚠ **别把这张表压成一句「省略就是继承」**：省略是**两支**，只有第一支叫继承。
    ///
    /// ⚠ **本机这条路上第一支到底拿谁的环境**（现打 09-13）：送法是
    /// `launch::build_local_posix_argv` ⇒ `bash -lic '<cmd>'`（login ＋ interactive）
    /// ⇒ 用户 rc/profile 先跑 ⇒ `ccm` 看到的就是**用户 shell 里那一个**，与旧路同源。
    ///
    /// ⇒ **本条今天钉的是**：这一格 `Ok`，且渲染出来的那一串里 `--base` 与 `--account`
    /// **一个都不许有**。谁哪天把它映成 `--base`，这里当场红 —— 那一刀正是 `#75`
    ///（把继承偷换成显式清空）；谁把它映成某个具名号，也当场红（那是 `R28` 禁的静默换号）。
    /// ⚠ **`ccm` 那一侧怎么解释省略，本条一个字都不管** —— 那半的唯一住址是
    /// `plan.rs::resolve_account`，由 `plan::the_four_ways_an_account_gets_picked` 钉着。
    #[test]
    #[cfg(not(windows))]
    fn every_local_account_shape_gets_a_named_verdict_from_the_backend_path() {
        let act = LocalPsAction::Resume("s1".into());
        let caps = caps_of_a_current_ccm();
        let named_with_name = LaunchAccount::Named {
            config_dir: "/home/u/.claude-accts/z".into(),
            name: Some("z".into()),
        };
        let named_dir_only = LaunchAccount::Named {
            config_dir: "/home/u/.claude-accts/z".into(),
            name: None,
        };
        let base = LaunchAccount::Base;

        // 分母 = `LaunchAccount` 的全部形状 + 「参数缺席」。
        //
        // ⚠ 标签**只有一处住址**（`label_of` 里那个 `match`）—— 本条第一版在它旁边另写了一份
        //   `denominator: [&str; 4]` 字面量，那正是 `brief` 第 13b 条禁的「闭集重抄一份」：
        //   两份字面量迟早漂开，而漂开的那天两边看起来都没错。⇒ 标签一律现算。
        //
        // **穷尽性由 `label_of` 那个 `match` 买**：加一个变体而不回来加一行 ⇒ 编译不过，
        // 不是静默漏一格。这正是 `KR53D1` 的失效方向逐字
        //（「再加一个入口而它复用了那个缺一态的旧函数」）在 Rust 这一侧的落点。
        fn label_of(acct: Option<&LaunchAccount>) -> &'static str {
            match acct {
                None => "缺席",
                Some(LaunchAccount::Base) => "Base",
                Some(LaunchAccount::Named { name: Some(_), .. }) => "Named{有名字}",
                Some(LaunchAccount::Named { name: None, .. }) => "Named{只有目录}",
            }
        }
        let shapes: [Option<&LaunchAccount>; 4] = [
            None,
            Some(&base),
            Some(&named_with_name),
            Some(&named_dir_only),
        ];
        // 反重复：四格必须**互不相同**，否则「四格都喂过了」是假的
        //（例：两格都是 `Named{只有目录}` ⇒ 有一种形状根本没被喂过，而条数照样是 4）。
        let labels: Vec<&str> = shapes.iter().map(|a| label_of(*a)).collect();
        let mut uniq = labels.clone();
        uniq.sort_unstable();
        uniq.dedup();
        assert_eq!(
            uniq.len(),
            labels.len(),
            "分母这张表里有两格是同一种形状 ⇒ 有一种形状没被喂过。实得：{labels:?}"
        );

        // 结果**按标签取**，不按下标取 —— 下标取法在 `shapes` 顺序一变时会悄悄换一格来断，
        // 那是一次静默的假读数（本区最贵的那族）。取不到就 `panic`，不会空转。
        let verdicts: Vec<(&str, Result<String, String>)> = shapes
            .iter()
            .map(|acct| {
                (
                    label_of(*acct),
                    render_local_ccm_with(&act, None, *acct, Some("s1abcdef-cc"), &caps, true),
                )
            })
            .collect();
        let verdict = |label: &str| -> &Result<String, String> {
            &verdicts
                .iter()
                .find(|(l, _)| *l == label)
                .unwrap_or_else(|| panic!("分母里没有 `{label}` 这一格 —— 下面那条断言在空转"))
                .1
        };

        // ① 缺席 —— 🔴 **`K-R89` 09-13：这一格翻面了。**
        //    它此前是 `Err` 且理由点着「继承」，依据是头注那张三说法对照表的最后一栏
        //    「省略 `--account` ⇒ 落 manifest 默认号 ⇒ 同样是静默换号」。
        //    **那一栏今天不是「病灶」了** —— 用户 09-12 `R28` 逐字裁「不是有选默认账号吗?
        //    **就用那个**」，并且 `plan.rs::resolve_account` 已经按两支落地（`-z` 闸 ＋ 默认号）。
        //    ⇒ 缺席这一格现在必须 **`Ok`**，且渲染出来的那一串里**两个账号 flag 都不许有**。
        let r = verdict("缺席");
        let cmd = r.as_ref().unwrap_or_else(|e| {
            panic!(
                "「参数缺席」= 继承环境，`R28` 之后它渲染得出来（省略 = `plan.rs::resolve_account`\n\
                 的两支：有继承态就继承 · 裸终端落 manifest 默认号，两支都是那一裁要的）。\n\
                 若它又短路了，先回 `DECISIONS.md#R28` 确认那一裁是不是被推翻，别在这里加闸。\n\
                 实得降级理由：{e}"
            )
        });
        assert!(
            !cmd.contains("--base"),
            "缺席被渲染成 `--base` —— 那是把「继承」偷换成「显式清空」（#75）。实得：{cmd}"
        );
        assert!(
            !cmd.contains("--account"),
            "缺席被渲染成 `--account <某号>` —— 那是替调用方挑了一个号，\n\
             与 `R28` 逐字「不许把调用方选中的号静默换掉」反向。实得：{cmd}"
        );

        // ② Base —— `Ok`，而且渲染出来的那条真的带 `--base`。
        let r = verdict("Base");
        let cmd = r.as_ref().expect("账号 0 是本机一直渲染得出来的那一格");
        assert!(
            cmd.contains("--base"),
            "账号 0 必须显式 `--base`，实得：{cmd}"
        );

        // ③ Named{有名字} —— **本件要开的就是这一格**：`Ok`，且带 `--account z`。
        let r = verdict("Named{有名字}");
        let cmd = r.as_ref().unwrap_or_else(|e| {
            panic!(
                "具名账号**说得出名字**时必须渲染得出来 —— 说不出来就意味着盘上四个本机拉起入口里\n\
                 那三个（`src/tabs.ts` · `src/views/history.ts` 两处）在类型上到不了后端那条路。\n\
                 实得降级理由：{e}"
            )
        });
        assert!(
            cmd.contains("--account z"),
            "具名账号该渲染成 `--account <名字>`，实得：{cmd}"
        );

        // ④ Named{只有目录} —— 仍然 `Err`：**不许从目录名推一个 `--account` 出来**。
        //    推错的失效方向是 `ccm` 当场 `die`（退出码 2）= 一次能起的会话变成报错，
        //    与 `relay_account_id_of_dir` 那条「推错就回落」的保守方向**相反**。
        let r = verdict("Named{只有目录}");
        assert!(
            r.as_ref().is_err_and(|e| e.contains("account")),
            "只有目录没有名字时必须诚实短路（§35），**不许拿目录名当 `--account`**。实得：{r:?}"
        );
    }

    // ═══════════════════════════════════════════════════════════════════════
    // `K-R89` `KR89D1`：**六格的今天版** —— 一张由行为驱动的表，不是一段散文
    // ═══════════════════════════════════════════════════════════════════════

    /// 一格今天是什么。**三值，别加第四个而不同时给它一条驱动**（下面那个 `match` 会逼你）。
    #[cfg(not(windows))]
    #[derive(PartialEq, Eq, Debug, Clone, Copy)]
    pub(crate) enum CellToday {
        /// 后端那条路今天**渲染得出来** —— 这一格关了。
        Closed,
        /// 今天仍落回旧路，而**挡路的那样东西说得出名字**（第四列就是它）。
        StillFallsBack,
        /// **结构性** —— 不是欠账。翻它要先翻一条定框，不是补一段代码。
        Structural,
    }

    /// 🔴🔴 **六格今天版。`parity_ledger.rs` 的 `launch.render-payload` 那一行点的就是这六格。**
    ///
    /// `(格名, 今天是什么, 现打的说法, 缺什么 / 谁能关它)`
    ///
    /// # 它与账本那一行的分工（别读成两份清单）
    ///
    /// 账本那一行是**散文**，它自己记过两次「改了行为没回来改理由」的前科。
    /// 本表是**同一件事的可执行版**：下面那条判据把每一格**真去驱动一遍**，
    /// 观测到的状态与本表第二列不符 ⇒ **当场红**。
    /// ⇒ 「改一格的行为而不改它的说法」在这里做不到 —— 那正是本表存在的理由。
    ///
    /// # ⚠ 本表买不到什么（诚实边界，别读大）
    ///
    /// - **第三、四列（那两段话）真不真，机器判不了。** 本表钉的是「第二列 == 现打」，
    ///   以及「有人改了行为就必须回来动这张表」。**一段读着有道理的假理由照样过得去。**
    /// - **它不是全部降级面**：它只装 `parity_ledger` 那一行点名的这六格。别的降级理由
    ///   （例：`--base` 那条逃生口、`send-into` 的 #76 防线）不在本表人群里。
    /// - **Windows 那一格在本树上量不到运行时行为**（本模块整个挂 `#[cfg(not(windows))]`）
    ///   ⇒ 它的观测是**源码级**的，如实写在驱动里。
    ///
    /// # 🔴 `K-R89` 09-13 改了哪一格、为什么（这一段是本轮唯一的行为改动）
    ///
    /// 「账号未表态（继承）」从 `StillFallsBack` 翻成 `Closed`。翻它的**不是本件的判断**，
    /// 是 `DECISIONS.md#R28`（用户 09-12 亲裁）＋ 它在
    /// `remote-daemon-proto/src/control/ccm/plan.rs::resolve_account` 上的落地。
    /// 盘上原来有一句陈账逐字写着「③ 那一格要动的是 ccm 省略时的默认语义（**产品决定**
    /// ＋ `plan.rs`）」—— **它在等一个 09-12 就到了的决定**，本轮一并撤掉。
    #[cfg(not(windows))]
    pub(crate) const THE_SIX_WAYS_THE_OLD_PATH_STILL_WINS: &[(&str, CellToday, &str, &str)] = &[
        (
            "账号未表态（继承）",
            CellToday::Closed,
            "🔴 `K-R89` 09-13 关掉的就是这一格。`CliAccount::Inherit` 渲染成「一个账号 flag 都不加」；\
             省略在 `ccm` 上有确定语义（`plan.rs::resolve_account` 两支：`CLAUDE_CONFIG_DIR` 非空 ⇒ \
             保留不覆盖〔`R08` 的 `-z` 闸〕· 裸终端 ⇒ 落 manifest `isDefault`），两支都是 `R28` 要的。",
            "已关。⚠ **只关了本机那半** —— 远端是 ssh 过去、那台机器上的继承态不是 monitor 的环境\
             （`R28` 裁定四逐字）⇒ `WireAccount` 刻意没有对应变体，远端那半归 `K-R90`。",
        ),
        (
            "只说得出目录没名字",
            CellToday::StillFallsBack,
            "`LaunchAccount::Named{name: None}` ⇒ `CliAccount::Named{name: None}` ⇒ 账号维度\
             `cli_flags` 回 `None` ⇒ §35 整条降级。理由是「说不出」，不是「不想说」。",
            "缺的是**名字这条信息本身**，不是 CLI 语法 —— 上游（`accounts.ts` 那个取值口）\
             说得出名字的那天它自己就关了。🔴 **不许从目录名推一个 `--account` 出来**：\
             推错的失效方向是 `ccm` 当场 `die`（rc=2），一次能起的会话变成报错。",
        ),
        (
            "没有 tmux 名",
            CellToday::StillFallsBack,
            "`NO_TMUX_NAME` —— 名字只许 `remote-launch.ts::mintTmuxName` 铸（F13 那个撞名坑），\
             Rust 这侧不许补默认值。⚠ **这一格今天是半开的**（现打 09-13）：resume 那条\
             前端已接线（`views/history.ts::mintLocalTmuxName` · `tabs.ts::mintSessionTmuxName`，\
             人群由 `tests/ipc/commands.vitest.ts` 那条「每处 `resume_history_session` 都带 `tmuxName`」钉着）；\
             而 `new_local_session` 的 Rust 签名里**根本没有 `tmux_name` 这一格** ⇒ 起新会话恒短路。",
            "给 `new_local_session` 加一个名字参数 ＋ 前端在那条路上也过一次铸造口。\
             ⚠ 那要动 `src/ipc/commands.ts` 与两个调用点，**不在 `K-R89` 的写区里**。",
        ),
        (
            "这个号走中转",
            CellToday::StillFallsBack,
            "`launch_local` 里那行 `relay.is_empty()` **显式**保住的互斥 —— 不是渲染器拒的\
             （渲染器单独看已经不再互斥，`K-R53` 开的那一格）。常量是 `RELAY_KEEPS_THE_OLD_PATH`。",
            "退役条件 `K-R61` 已经收成**一行 Rust**（把 `relay.is_empty()` 换成「探到 \
             `base-url-across-tmux` 才放行」），但它的前置是「有人守住『用户机器上跑的 `ccm` \
             就是 app 自己推的那一份』」—— 那一格今天没人守。⚠ 互斥这条性质本身由邻居\
             那条判据钉，本行只记「这一格今天关没关」。",
        ),
        (
            "这台机没装 ccm",
            CellToday::StillFallsBack,
            "`render_ccm_invocation` 的第一行 `if !installed { NotInstalled }`。\
             探测走 `CcmProbeSource` 那条缝（本判据喂确定值，**不问跑它的这台机器**）。",
            "**部署面，不是渲染器的欠账**（`K27`/`K34`：部署是产品的一部分，由客户端做）。\
             远端那条装法 `sftp::install_remote_ccm_helper` 今天就在盘上；本机那条归部署向导。",
        ),
        (
            "Windows",
            CellToday::Structural,
            "`render_local_ccm` / `render_local_ccm_with` 整段挂 `#[cfg(not(windows))]` ⇒ \
             Windows 上那条路**在编译期就不存在**；`launch_local` 的 `#[cfg(windows)]` 那一支\
             连 `tmux_name` 都不读（读了就是给「Windows 也进容器」开口子）。",
            "**不是欠账**：定框 `C12`〔用 08-12〕逐字「windows不要tmux」。要翻它先回去翻定框。",
        ),
    ];

    /// 🔴🔴 `KR89D1`：**六格逐格现打，观测到的与表上写的不一样就红。**
    ///
    /// # 每一格怎么观测的（写在这里，别让读的人去猜）
    ///
    /// 五格靠**真去驱动生产函数**（`render_local_ccm_with` / `launch_local`），
    /// 第六格（Windows）在本树上跑不到运行时，观测是**源码级**的 —— 逐条写在 `observe` 里。
    ///
    /// # ⚠ 与邻居 [`a_launch_that_goes_through_the_relay_still_cannot_get_a_tmux_container`] 的分工
    ///
    /// 「走中转」那一格两处都会驱动一次 `launch_local`，而**它们量的不是两把尺子**：
    /// 同一个观测口（[`LaunchSink`] 那条缝上真正交出去的那一串）、同一个判定
    /// （串里有没有 `--tmux=`）。差别在**结论**：邻居主张的是「两个集合不相交」（互斥），
    /// 本条只记「这一格今天关没关」。⇒ 谁哪天把中转那一行翻掉，**两条一起红**，
    /// 而它们要求的后续动作不同（邻居要重裁互斥，本条要改表）。
    #[test]
    #[cfg(not(windows))]
    fn every_one_of_the_six_cells_is_measured_not_narrated() {
        // 反空真 ①：表得有六行，且**格名互不相同**（重名 ⇒ 有一格根本没被观测过，而条数照样对）。
        assert_eq!(
            THE_SIX_WAYS_THE_OLD_PATH_STILL_WINS.len(),
            6,
            "账本 `launch.render-payload` 那一行点的是六格。加/删一格 ⇒ 同拍改账本那一行，\
             并回来给新格写一条驱动。"
        );
        let mut names: Vec<&str> = THE_SIX_WAYS_THE_OLD_PATH_STILL_WINS
            .iter()
            .map(|(n, ..)| *n)
            .collect();
        let n_all = names.len();
        names.sort_unstable();
        names.dedup();
        assert_eq!(
            names.len(),
            n_all,
            "表里有两行是同一个格名 ⇒ 有一格没被观测过"
        );

        // 反空真 ②：三值**至少两值有人占**。全是同一个值时，下面那条相等断言退化成
        // 「所有格都一样」——那时把某一格的行为翻掉、再把整列一起改，读起来仍然全绿。
        let mut kinds: Vec<CellToday> = THE_SIX_WAYS_THE_OLD_PATH_STILL_WINS
            .iter()
            .map(|(_, v, ..)| *v)
            .collect();
        kinds.sort_by_key(|k| format!("{k:?}"));
        kinds.dedup();
        assert!(
            kinds.len() >= 2,
            "六格今天是同一个状态（{kinds:?}）—— 先确认这是真的；\
             真是真的话，本条那条相等断言此刻买不到「逐格」，请改形状。"
        );

        for (name, want, say, need) in THE_SIX_WAYS_THE_OLD_PATH_STILL_WINS {
            let got = observe_one_cell(name);
            assert_eq!(
                got, *want,
                "\n★ 六格表第「{name}」格：**现打是 {got:?}，表上写的是 {want:?}**。\n\
                 ⇒ 有人改了这一格的行为，而没有回来改它的说法 —— 那正是这张表存在的理由。\n\
                 表上今天写着：{say}\n\
                 表上今天说缺什么：{need}\n\
                 ⚠ 改表的同时把 `parity_ledger.rs` 的 `launch.render-payload` 那一行一起读一遍：\
                 两处说的是同一件事。"
            );
        }
    }

    /// 六格各自的观测口。**一个 `match`，认不出的格名当场 `panic`** ——
    /// 加一格却不给它驱动时，上面那条判据不会静默少测一格。
    #[cfg(not(windows))]
    fn observe_one_cell(name: &str) -> CellToday {
        let act = LocalPsAction::Resume("s1".into());
        let caps = caps_of_a_current_ccm();
        const TMUX: &str = "s1abcdef-cc";
        let dir_only = LaunchAccount::Named {
            config_dir: "/home/u/.claude-accts/z".into(),
            name: None,
        };
        // 纯函数半的观测：渲染得出来 = 这一格关了。
        let pure = |acct: Option<&LaunchAccount>, tmux: Option<&str>, installed: bool| {
            if render_local_ccm_with(&act, None, acct, tmux, &caps, installed).is_ok() {
                CellToday::Closed
            } else {
                CellToday::StillFallsBack
            }
        };
        match name {
            // ① 未表态 —— `R28` 之后渲染得出来。
            "账号未表态（继承）" => pure(None, Some(TMUX), true),
            // ② 只有目录 —— §35 短路。
            "只说得出目录没名字" => pure(Some(&dir_only), Some(TMUX), true),
            // ③ 没有 tmux 名 —— 名字只许铸造口产，Rust 侧不补默认值。
            //    ⚠ 喂 `Base`（一个**确定渲染得出来**的账号形状）⇒ 这一格观测到的
            //    「拒」只可能是名字那一维造成的，不会与账号那一维混在一起。
            "没有 tmux 名" => pure(Some(&LaunchAccount::Base), None, true),
            // ④ 没装 ccm —— 探测结果由参数喂，**不问跑它的这台机器**。
            "这台机没装 ccm" => pure(Some(&LaunchAccount::Base), Some(TMUX), false),
            // ⑤ 走中转 —— 这一格不在纯函数半里（闸在 `launch_local` 体内那行
            //    `relay.is_empty()`）⇒ 必须真跑一趟拉起，量**交出去的那一串**。
            "这个号走中转" => {
                let acct = LaunchAccount::Named {
                    config_dir: "/home/u/.claude-accts/acct-a".into(),
                    name: Some("acct-a".into()),
                };
                fn rows() -> Vec<String> {
                    vec!["acct-a".to_string()]
                }
                fn running() -> bool {
                    true
                }
                fn not_win() -> bool {
                    false
                }
                let _facts = override_relay_facts(RelayFactSources {
                    rows,
                    running,
                    windows: not_win,
                });
                fn a_current_ccm() -> crate::ccm_probe::CcmProbeResult {
                    crate::ccm_probe::CcmProbeResult {
                        installed: true,
                        version: Some("0.0.0-判据替身".to_string()),
                        capabilities: caps_of_a_current_ccm().into_iter().collect(),
                        build: None,
                    }
                }
                let _probe = override_ccm_probe(CcmProbeSource(a_current_ccm));
                thread_local! {
                    static SEEN: std::cell::RefCell<Vec<String>> =
                        const { std::cell::RefCell::new(Vec::new()) };
                }
                fn recorder(cmd: &str, _cwd: Option<&str>) -> Result<(), String> {
                    SEEN.with(|v| v.borrow_mut().push(cmd.to_string()));
                    Ok(())
                }
                let _sink = override_launch_sink(LaunchSink(recorder));
                launch_local(&act, None, None, Some(&acct), Some(TMUX))
                    .expect("走中转这一趟拉起本身不该失败");
                let sent = SEEN.with(|v| {
                    v.borrow()
                        .last()
                        .cloned()
                        .expect("这一趟什么都没送出去 —— 观测口坏了，读数作废")
                });
                // 自检：这一趟**真的**走了中转（否则下面那个判定量的是另一件事）。
                assert!(
                    sent.contains("ANTHROPIC_BASE_URL"),
                    "这一趟没拿到中转前缀 —— 替身没生效，本格此刻在量别的东西。实得：{sent}"
                );
                if sent.contains("--tmux=") {
                    CellToday::Closed
                } else {
                    CellToday::StillFallsBack
                }
            }
            // ⑥ Windows —— 本模块整个挂 `#[cfg(not(windows))]`，跑不到那一支的运行时。
            //    ⇒ 观测是**源码级**的，如实写清它量的是什么：
            //      · `launch_local` 的 `#[cfg(windows)]` 那一支里有 `let _ = tmux_name;`
            //        （逐字：连读都不读，读了就是给「Windows 也进容器」开口子）；
            //      · 渲染器那一半挂着 `#[cfg(not(windows))]`（编译期就不给 Windows）。
            //    两条**都**成立才算「结构性」；少一条就说明有人开了口子。
            "Windows" => {
                let prod = guard_core::production_code(include_str!("history.rs"));
                let at = guard_core::find_pinned(&prod, "fn launch_local(").unwrap_or_else(|e| {
                    panic!("`fn launch_local(` 不是恰好一处 —— 锚点坏了，本格读数作废：{e}")
                });
                let win_arm_keeps_out = prod[at..]
                    .split_once("#[cfg(not(windows))]")
                    .map(|(head, _)| head.contains("let _ = tmux_name;"))
                    .unwrap_or(false);
                // ⚠ 带 `fn ` 前缀才认得出**定义**那一处 —— 不带的话第一处命中的是
                //   `render_local_ccm` 体内那次**调用**，而调用点上没有 cfg 属性。
                let renderer_is_posix_only = prod
                    .find("fn render_local_ccm_with(")
                    .map(|i| prod[..i].trim_end().ends_with("#[cfg(not(windows))]"))
                    .unwrap_or(false);
                if win_arm_keeps_out && renderer_is_posix_only {
                    CellToday::Structural
                } else {
                    CellToday::Closed
                }
            }
            other => panic!(
                "六格表里多了一格「{other}」而没有人给它写观测口 —— \
                 加格与加驱动必须同一拍，否则那一格是**登记了但没量过**。"
            ),
        }
    }

    /// ★★★ `D4 阻-3`：**「走中转」与「有 tmux 容器」今天仍然互斥** —— 把这个事实钉住。
    ///
    /// # 它为什么存在：盘上写着「已消掉」，而其实没消掉
    ///
    /// 第一拍报过一条代价「走中转的号拿不到 tmux 容器」（当时的成因：外侧那句 export
    /// 在 tmux 边界被吃掉）。第二拍照 `R08` 在那份已删的 bash `ccm` 里加了一条转发，于是件文件
    /// 与 [`launch_local`] 的头注都写上了**「不再互斥」**。
    /// 🔴 `D4` 现打证伪：**代价原样还在，只是成因换了。**
    ///
    /// # 🔴🔴 `K-R53` 09-11 **重新裁定**：成因**第二次**换了，而互斥仍然成立
    ///
    /// `D4` 那一拍的成因是「具名账号根本进不了 ccm」（`Named` 只有目录、说不出 `--account`）。
    /// **本件把那一格开了**（[`LaunchAccount::Named::name`]）⇒ **那个成因今天不成立了**：
    /// 下面第 ⓪ 格现打断言的正是这件事 —— 渲染器**单独看已经不再互斥**。
    ///
    /// 今天互斥是由 [`launch_local`] 里那一行 `relay.is_empty()` **显式保住**的
    /// （理由与退役条件住 [`RELAY_KEEPS_THE_OLD_PATH`]）。
    ///
    /// ⚠ 〔`K-R61` 09-11〕这一段先前逐字写着「`capabilities=`（第 624 行，18 个 token）里
    /// 没有任何 token 声明它 ⇒ 放行会让**装旧 ccm 的机器**静默吃掉」——
    /// **那个住址与那个理由今天都不成立了**，重裁后的两句都住
    /// [`RELAY_KEEPS_THE_OLD_PATH`] 的头注，本条不复述第二份。
    ///
    /// ⇒ **这一条从「成因是说不出名字」改成「成因是那一行还没改成探那个能力」。**
    /// 前者是结构性的（只能等改 `LaunchAccount`），后者**有可执行的退役条件**。
    ///
    /// # 🔴🔴🔴 `K-R55` 09-11：**上一版自己抄了一份被测逻辑** —— 换成量真正送出去的那一串
    ///
    /// 上一版的循环体逐字是：
    /// ```text
    /// let prefix = relay_prefix_for_launch(&act, acct).expect(…);
    /// if !prefix.is_empty() { relayed.push(label); }
    /// if prefix.is_empty() && renders(acct) { containered.push(label); }
    /// ```
    /// —— 那个 `prefix.is_empty() &&` **就是 [`launch_local`] 里那道闸的一份拷贝**。
    /// ⇒ 它证的是自己那份拷贝，生产那一行翻不翻它都不知道。
    /// PM 09-11 现打：把生产那行换成 `if true`，**点名单跑 1 passed**；
    /// 实现方 09-11 在沙箱里复打了同一刀，**全量 `cargo --lib` 1472 passed / 0 failed**
    /// （量于 `07e4e72` + 那一刀，镜像 `ccmon-devbox:latest`，`CARGO_TARGET_DIR=pm-targets/k-r55`）
    /// —— 全仓**没有任何一条**判据对那一刀出声。
    ///
    /// ⇒ 本条现在**真的驱动 [`launch_local`]**，两个集合都从
    /// **[`LaunchSink`] 那条缝上收到的那个字符串**里读出来，一个字节的判断逻辑都不自带：
    /// - 「走中转」= 那一串里有 `ANTHROPIC_BASE_URL`（中转前缀唯一的形状）；
    /// - 「有容器」= 那一串里有 `--tmux=`（`render_ccm_invocation` 唯一产出它的地方；
    ///   回落路 `build_local_posix_command` 从不说 tmux）。
    ///
    /// 「装没装 ccm」由 [`CcmProbeSource`] 那条缝喂进来（**不问跑判据的这台机器** ——
    /// 沙箱里没装 ccm，不喂的话四格会一起落到回落路，那时本条又变成空真）。
    ///
    /// # 今天的成因（本条逐格量出来，不是推的）
    ///
    /// 分母 = [`LaunchAccount`] 的**全部形状**加上「参数缺席」，并且**具名那一格喂两个号**
    /// （一个在中转表里、一个不在 —— 只喂一个的话「中转在不在场」这一维的取值域是 1，
    /// 那正是 `D6 阻-2` 逮到过的形状）：
    ///
    /// | 形状 | 送出去那一串带不带中转前缀 | 带不带 `--tmux=`（= [`launch_local`] 的判据） |
    /// |---|---|---|
    /// | 缺席（`None`） | 不带（不走中转） | **是**〔🔴 `K-R89` 09-13 翻的：`R28` 之后省略有确定语义，渲染器说得出「继承」了〕 |
    /// | `Base`（账号 0） | 不带（不走中转） | **是** |
    /// | `Named{acct-a}`（**在中转表里**） | 带 | 否 —— `relay.is_empty()` 那一行挡住 |
    /// | `Named{acct-b}`（不在表里） | 不带 | **是**（`K-R53` 开的就是这一格） |
    ///
    /// # ⚠ 它连带说明了一件别处的事（别让那条判据被读宽）
    ///
    /// 我们自己这份 `ccm` 的容器路那条 `ANTHROPIC_BASE_URL` 转发（连同钉它的
    /// `payload::tests::the_ccm_container_path_forwards_the_relay_base_url_across_the_tmux_boundary`
    /// 与件文件里的 `M12`/`M12b`）量的是一条**在本机中转这条路上今天生产不可达**的路：
    /// 那段 shell 真的会转发，而**没有任何生产输入能同时走到中转与容器**。
    /// 它不是假的，它买不到本件要的那一格。**那条判据的头注里也写了这句话，两处别只改一处。**
    ///
    /// # 🔴 这条前提**还会再变一次** —— 变的那天去哪里重新裁定（`testing.md` 三.11 要的那一栏）
    ///
    /// 〔`K-R61` 09-11 **重裁**〕上一版这里写的是「退役条件今天是**一行 shell**」，
    /// 点的是那份 bash `ccm` 的第 624 行 —— 而它 `07e4e72` 就删了。
    /// 今天的前提是：**转发做到了、也声明了**（`base-url-across-tmux` 已在
    /// `remote-daemon-proto/src/control/ccm/mod.rs` 的 `CAPABILITIES` 里，
    /// 由那棵树的 `the_base_url_token_is_declared_because_the_tmux_path_really_forwards_it`
    /// 真去驱动一遍），**差的只是 [`launch_local`] 那一行还没改成探它**。
    ///
    /// ⇒ 退役条件收成**一行 Rust**：[`launch_local`] 里那句 `relay.is_empty()`
    /// 换成「探到 `base-url-across-tmux` 才放行」。真做那一天，**同一拍**这几样：
    ///   ① 本条会红 —— **在这里重新裁定**；
    ///   ② [`launch_local`] 的头注与 [`RELAY_KEEPS_THE_OLD_PATH`] 跟着改；
    ///   ③ 件计划 `K-H2b §4` 那条登记跟着改
    ///      〔`K-R53` 09-11 报回 PM，**`K-R61` 仍未做**：`K-R61 §0e` 逐字裁「不碰它」〕；
    ///   ④ `CAPABILITIES` 那个 token —— **`K-R61` 已做**，这一样从此不再是待办。
    #[test]
    #[cfg(not(windows))]
    fn a_launch_that_goes_through_the_relay_still_cannot_get_a_tmux_container() {
        let act = LocalPsAction::Resume("s1".into());
        let base = LaunchAccount::Base;
        // 在中转表里的那个号 —— 名字说得出（本件之后前端就是这么传的）。
        let acct_a = LaunchAccount::Named {
            config_dir: "/home/u/.claude-accts/acct-a".into(),
            name: Some("acct-a".into()),
        };
        // 不在中转表里的那个号 —— 「哪个号」这一维的取值域因此是 2，不是 1（`D6 阻-2`）。
        let acct_b = LaunchAccount::Named {
            config_dir: "/home/u/.claude-accts/acct-b".into(),
            name: Some("acct-b".into()),
        };

        // 中转事实由替身给：表里只有 acct-a、中转在跑、不是 Windows。
        fn rows_with_only_acct_a() -> Vec<String> {
            vec!["acct-a".to_string()]
        }
        fn relay_is_running() -> bool {
            true
        }
        fn not_windows() -> bool {
            false
        }
        let _guard = override_relay_facts(RelayFactSources {
            rows: rows_with_only_acct_a,
            running: relay_is_running,
            windows: not_windows,
        });

        // 「这台机器装没装 ccm」也由替身给 —— 本条**不问跑它的那台机器**
        //（沙箱里没装，不喂的话四格一起落到回落路 ⇒ 本条变成空真）。
        fn a_current_ccm() -> crate::ccm_probe::CcmProbeResult {
            crate::ccm_probe::CcmProbeResult {
                installed: true,
                version: Some("0.0.0-判据替身".to_string()),
                capabilities: caps_of_a_current_ccm().into_iter().collect(),
                build: None,
            }
        }
        let _probe = override_ccm_probe(CcmProbeSource(a_current_ccm));

        // 送法替身：本条量的是 [`launch_local`] **真正交出去的那一串**，不是源码、
        // 也不是本条自己再算一遍的什么东西。
        thread_local! {
            static SENT: std::cell::RefCell<Vec<String>> =
                const { std::cell::RefCell::new(Vec::new()) };
        }
        fn recorder(cmd: &str, _cwd: Option<&str>) -> Result<(), String> {
            SENT.with(|v| v.borrow_mut().push(cmd.to_string()));
            Ok(())
        }
        let _sink = override_launch_sink(LaunchSink(recorder));

        // 容器名由调用方给（`mintTmuxName` 那一侧的事），这里只要一个合法的名字。
        const TMUX: &str = "s1abcdef-cc";

        // ⓪ **重新裁定的那一格**：渲染器**单独看**已经不再互斥了 ——
        //    在中转表里的那个号，渲染器今天渲得出来。互斥不再由它保。
        //    （这一格红 = `K-R53` 那一刀被退掉了，那时下面几格的理由也就不成立。）
        //    ⚠ 走的是**生产那个渲染器** [`render_local_ccm`]（探测经缝喂），
        //    不是它的纯函数半 —— 后者会把「生产上探测这一跳还在不在」漏在射程外。
        assert!(
            render_local_ccm(&act, None, Some(&acct_a), Some(TMUX)).is_ok(),
            "渲染器又对具名账号短路了 —— 那是 `K-R53` 之前的形状，\n\
             本条头注里那段「成因换成探不到能力」就不再成立，回去重新裁定。"
        );

        let mut relayed = Vec::new();
        let mut containered = Vec::new();
        let shapes: [(&str, Option<&LaunchAccount>); 4] = [
            ("缺席", None),
            ("Base", Some(&base)),
            ("Named{acct-a·在表里}", Some(&acct_a)),
            ("Named{acct-b·不在表里}", Some(&acct_b)),
        ];
        for (label, acct) in shapes {
            // 🔴 **真去走生产那条路** —— 上一版在这里自己抄了一份 `launch_local` 的闸
            //    （见头注 `K-R55` 那一节），于是把生产那一行翻掉一个字都不响。
            launch_local(&act, None, None, acct, Some(TMUX))
                .unwrap_or_else(|e| panic!("形状 {label} 这一趟拉起本身就失败了：{e}"));
            let sent = SENT.with(|v| {
                v.borrow()
                    .last()
                    .cloned()
                    .unwrap_or_else(|| panic!("形状 {label} 这一趟什么都没送出去"))
            });
            // 两个判定都只看**那一串**：`ANTHROPIC_BASE_URL` 只可能来自中转前缀，
            // `--tmux=` 只可能来自 `render_ccm_invocation`（回落路从不说 tmux）。
            if sent.contains("ANTHROPIC_BASE_URL") {
                relayed.push(label);
            }
            if sent.contains("--tmux=") {
                containered.push(label);
            }
        }
        // 反空真：两边**都非空**（都空的话下面那条不相交是空真）。
        assert_eq!(
            relayed,
            ["Named{acct-a·在表里}"],
            "真拿到中转前缀的形状变了 —— 本条的结论要重新裁定（见头注最后一节）"
        );
        assert_eq!(
            containered,
            ["缺席", "Base", "Named{acct-b·不在表里}"],
            "能走进 ccm 容器的形状变了 —— 本条的结论要重新裁定（见头注最后一节）。\n\
             〔`K-R89` 09-13：「缺席」是本轮新进来的一格 —— `R28` 之后省略 `--account` \
             有确定语义，渲染器不再对它短路。**互斥那条结论没变**，变的是分母。〕"
        );
        // 正题：两个集合不相交 ⇒ 今天没有任何一次本机拉起同时拿到中转前缀与 tmux 容器。
        assert!(
            relayed.iter().all(|l| !containered.contains(l)),
            "「走中转」与「有 tmux 容器」不再互斥了 —— 那是**好事**，但盘上有三处话要跟着改：\n\
             ① 本条（重新裁定）② `launch_local` 头注与 `RELAY_KEEPS_THE_OLD_PATH`\n\
             ③ 件计划 `K-H2b §4` 那条登记。\n\
             （第四样 —— `remote-daemon-proto/src/control/ccm/mod.rs` 的 `CAPABILITIES` 加\n\
             `base-url-across-tmux` —— `K-R61` 已经做了：转发做到了、也声明了。）\n\
             实得：走中转的 {relayed:?} · 有容器的 {containered:?}"
        );
    }

    // ═══════════════════════════════════════════════════════════════════════
    // `K-R61`：退役条件那句话 —— **几处说的是同一件事**，而且**它点名的住址真的在盘上**
    // ═══════════════════════════════════════════════════════════════════════

    /// 本组判据的**自剪线**。见 [`r61_hay`]。
    ///
    /// ⚠ 这个串在本文件里**必须只出现在这一行**（下面两条判据都靠它切被测面）。
    const R61_SELF_CUT: &str = "〔K-R61 判据组自剪线〕";

    /// 本组的被测面 = `history.rs` 全文**截到自剪线为止**。
    ///
    /// 🔴 为什么要剪：本组的锚点是**逐字串**，而它们在下面两条判据里各有一份字面量副本
    /// （判据自带清单，与 `ccm_invocation.rs` 那两处「刻意的重复」同一个理由）。
    /// 不剪的话 [`guard_core::find_pinned`] 会看到两处、当场报「指不明是哪一处」——
    /// 那是**量具把自己也算进了被测面**。
    ///
    /// ⚠ **它买不到的**：自剪线**之后**的文本一律不进射程。有人把同一段话复制到本文件
    /// 更后面去，本组看不见。射程边界就写在这里，别读宽。
    fn r61_hay() -> &'static str {
        let src = include_str!("history.rs");
        let cut = src
            .find(R61_SELF_CUT)
            .expect("自剪线不见了 —— 本组判据此刻在量它自己，读数作废");
        &src[..cut]
    }

    /// 「退役条件那句话」在本文件里的**住址表 —— 只有这一处**。
    ///
    /// 每一处给一对**逐字锚点**（起 / 止）。两个锚点都由 [`guard_core::find_pinned`]
    /// 断言**恰好命中一次**，取「起 → 止」之间那一段 ⇒ 窗口**不可能跨到下一条**：
    /// 止锚点就是紧挨着它的下一个结构物本身。
    ///
    /// ⚠ `K-R61 §0a` 那张表登记的是**四处**；本轮现打**五处**。多出来的两处是
    /// ③（[`launch_local`] 体内那段「为什么要显式保住」）与
    /// ⑤（互斥判据末尾那条 `assert!` 的诊断文案）—— 它们也在说同一件事，`§0a` 漏了。
    /// 数字与名单同住这里（纪律 ⑭）：分母 = 本表的长度，成员 = 本表逐行。
    fn r61_sites() -> Vec<(&'static str, &'static str, &'static str)> {
        vec![
            (
                "① 常量本体 RELAY_KEEPS_THE_OLD_PATH",
                "const RELAY_KEEPS_THE_OLD_PATH: &str =",
                "/// ⚠ **`Err` 那一支不回 token**",
            ),
            (
                "② 常量头注的『退役条件』一节",
                "# 🔴 退役条件〔`K-R61` 09-11 重裁",
                "const RELAY_KEEPS_THE_OLD_PATH: &str =",
            ),
            (
                "③ launch_local 体内『为什么要显式保住』",
                "🔴🔴🔴 **三次订正（`K-R61` 09-11）",
                "let rendered = if relay.is_empty() {",
            ),
            (
                "④ 互斥判据头注『这条前提还会再变一次』",
                "# 🔴 这条前提**还会再变一次**",
                "fn a_launch_that_goes_through_the_relay_still_cannot_get_a_tmux_container() {",
            ),
            (
                "⑤ 互斥判据末尾那条 assert! 的诊断文案",
                "「走中转」与「有 tmux 容器」不再互斥了",
                "实得：走中转的 {relayed:?}",
            ),
        ]
    }

    /// 按住址表切出那几段。锚点唯一性在这里当场核（切之前，不是切之后）。
    fn r61_segments() -> Vec<(&'static str, &'static str)> {
        let hay = r61_hay();
        r61_sites()
            .into_iter()
            .map(|(name, start, end)| {
                let a = guard_core::find_pinned(hay, start).unwrap_or_else(|e| {
                    panic!("{name}：起锚点不是恰好一处 —— 形状变了，先修锚点：{e}")
                });
                let b = guard_core::find_pinned(hay, end).unwrap_or_else(|e| {
                    panic!("{name}：止锚点不是恰好一处 —— 形状变了，先修锚点：{e}")
                });
                assert!(
                    a < b && b - a < 4000,
                    "{name}：切出来的窗口不成形（起 {a} 止 {b}）—— 两个锚点的相对位置变了，\n\
                     照原样切会切到别人身上，本条此刻的读数一律作废。"
                );
                (name, &hay[a..b])
            })
            .collect()
    }

    /// 从一段文本里抽出「反引号括起来、像**仓内路径**的那些串」。
    ///
    /// 🔴 **刻意不要求后缀**：`K-R61 §0d` 那把尺子的 `EXT` 白名单正是把**无后缀**的那一形
    /// 整个滤掉了，于是它一处都数不到本件正在治的那个样本 ——「量一个人群之前，
    /// 先确认尺子逮得到那个已知的样本」。这里只要求：带 `/` · 不是 URL · 不带空格 ·
    /// 只由路径字符组成 · 不是绝对路径。行号后缀（`:123` / `:12-34`）当场剥掉。
    fn r61_paths_in(seg: &str) -> Vec<String> {
        let mut out = Vec::new();
        let mut rest = seg;
        while let Some(a) = rest.find('`') {
            let after = &rest[a + 1..];
            let Some(b) = after.find('`') else { break };
            let raw = after[..b].trim();
            rest = &after[b + 1..];
            let tok = match raw.rsplit_once(':') {
                Some((head, tail))
                    if !tail.is_empty() && tail.chars().all(|c| c.is_ascii_digit() || c == '-') =>
                {
                    head
                }
                _ => raw,
            };
            if !tok.contains('/') || tok.contains("://") || tok.contains(' ') {
                continue;
            }
            if tok.starts_with('/') || tok.starts_with('~') || tok.starts_with("./") {
                continue;
            }
            if !tok
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || "._/+-".contains(c))
            {
                continue;
            }
            out.push(tok.to_string());
        }
        out
    }

    /// `KR61D1`：**那几处说的是同一件事** —— 同一个前提、同一个住址、同一个 token。
    ///
    /// # 它为什么存在
    ///
    /// 上一版这几处逐字点着一份 `07e4e72` 就删掉的 bash 脚本，而互斥那条判据一直是绿的
    /// ⇒ **判据活着、前提死了，中间没有任何东西会响**。本条就是那个「会响的东西」。
    ///
    /// # 判的是什么（三条，缺一不可）
    ///
    /// 1. **同一个前提**：每一处都要有那句承重话（`转发做到了、也声明了`）。
    ///    ⇒ 只把住址换新、把理由留在旧版本上（`K-R61 KR61D1` 逐字点名的失效方向
    ///    「**换地址不换前提**」）在这里当场红。
    /// 2. **同一个住址**：每一处点的都是 [`R61_ADDR`]，而它**在盘上真的存在**
    ///    （存在性那一半由 [`every_address_the_retirement_condition_names_is_still_on_disk`] 守）。
    /// 3. **旧住址一处都不许留**：那份已删的 bash 脚本的旧路径，五段里出现一次就红。
    ///    ⇒ `KR61D1` 那条死值验（「四处中任意一处改回旧住址 ⇒ 必须红」）由这一条兑现。
    ///
    /// # ⚠ 边界（别读宽）
    ///
    /// - 本条判的是**这几段文本互相一致**，**不判**「这段话是真的」。
    ///   「那个文件里真的有那个 token」由 daemon 那棵树的
    ///   `control::ccm::tests::the_base_url_token_is_declared_because_the_tmux_path_really_forwards_it` 守。
    /// - 住址表本身（哪几处算「同职」）是**人写的**。有人在别处再写一段同职的话而不登记，
    ///   本条看不见 —— 那正是 `§0a` 漏掉 ③⑤ 两处的形状。
    #[test]
    fn the_retirement_condition_says_the_same_thing_in_every_place_that_states_it() {
        // 判据自带清单（**不复用生产常量**）：复用的话，谁把生产那一份改了，
        // 循环跟着改，两边一起漂而没有一格红。
        const PREMISE: &str = "转发做到了、也声明了";
        const TOKEN: &str = "base-url-across-tmux";

        let segs = r61_segments();
        assert_eq!(
            segs.len(),
            5,
            "住址表的长度变了 —— 分母变了就要重新裁定，别让它悄悄变"
        );
        // 旧住址：**现搭**，不写成字面量。写成字面量的话本文件里就又多了一处
        // 「那个已删文件的路径」，而本条自己就是来消灭它的。
        let retired = format!("shared/{}", "ccm");

        for (name, seg) in &segs {
            assert!(
                seg.contains(PREMISE),
                "{name} 里没有那句承重话「{PREMISE}」。\n\
                 ⇒ 这正是 `KR61D1` 点名的失效方向：**换地址不换前提**。\n\
                 今天的前提是「我们自己这份 ccm 转发得了、也声明了，差的只是那一行还没改成探它」，\n\
                 不是旧话「对面可能是装了别的 ccm 的机器」（`K34`/`K35` 之后那类机器正在退场）。\n\
                 实得这一段：\n{seg}"
            );
            assert!(
                seg.contains(R61_ADDR),
                "{name} 点的住址不是 `{R61_ADDR}` —— 几处不再指同一个地方。\n\
                 实得这一段：\n{seg}"
            );
            assert!(
                seg.contains(TOKEN),
                "{name} 里没点名那个 token `{TOKEN}` —— 退役条件说不清要探什么。\n\
                 实得这一段：\n{seg}"
            );
            // 旧住址一处都不许留（`-aliases.sh` 那个**还在盘上**，不算）。
            let stale = seg
                .match_indices(retired.as_str())
                .filter(|(i, _)| {
                    seg[i + retired.len()..]
                        .chars()
                        .next()
                        .is_none_or(|c| c != '-')
                })
                .count();
            assert_eq!(
                stale, 0,
                "{name} 里还点着那份已删脚本的旧住址（{stale} 处）—— 那是 `K-R61` 要治的病本身：\n\
                 它 `07e4e72` 就删了，指着它的话不会有任何东西出声。\n\
                 实得这一段：\n{seg}"
            );
        }

        // 整份文件那一格：不只这五段，全文都不许再点那个旧住址。
        // （少了这一格，把旧住址挪出这五段的窗口就能躲过去。）
        let whole = include_str!("history.rs");
        let left: Vec<&str> = whole
            .lines()
            .filter(|l| {
                l.match_indices(retired.as_str()).any(|(i, _)| {
                    l[i + retired.len()..]
                        .chars()
                        .next()
                        .is_none_or(|c| c != '-')
                })
            })
            .collect();
        assert!(
            left.is_empty(),
            "本文件里还有 {} 行点着那份已删脚本：\n  {}",
            left.len(),
            left.join("\n  ")
        );
    }

    /// 退役条件点名的那个住址 —— **本文件里的字面量只有这一处**（`brief` 13b）。
    const R61_ADDR: &str = "remote-daemon-proto/src/control/ccm/mod.rs";

    /// `KR61D2`：**退役条件点名的仓内住址，不在了就得响。**
    ///
    /// 存在性由本条负责，**不由谁记得**。这一条不是给某一个旧名字写的补丁：
    /// 它把那几段里**每一个**看起来像仓内路径的串都拿去盘上核一次 ⇒
    /// 下一个被删掉的文件同样会当场红。
    ///
    /// # ⚠⚠ 反向本条**不主张**（这一句是 `KR61D2` 点名要写进头注的）
    ///
    /// 把住址改成一个**存在但不相干**的文件（比如把 `…/ccm/mod.rs` 换成 `…/ccm/argv.rs`），
    /// **本条逮不到** —— 它只判「在不在」，不判「这个住址讲的是不是那件事」。
    /// 别把它读成「住址对不对有人管」。
    ///
    /// 那半格今天由**别的东西**兜，而且兜得不全，如实写清：
    /// - [`the_retirement_condition_says_the_same_thing_in_every_place_that_states_it`]
    ///   钉着 [`R61_ADDR`] 这**一个**串 ⇒ 换成 `argv.rs` 它会红。但那是**钉死一个字面量**，
    ///   只护得住这一个住址，护不住下一条退役条件点的下一个住址。
    /// - 「那个文件里真的有那个 token」由 daemon 那棵树的
    ///   `the_base_url_token_is_declared_because_the_tmux_path_really_forwards_it` 守。
    /// - 「这段话说的是不是真的」**没有任何东西守**。
    ///
    /// # ⚠ 射程
    ///
    /// 只到 [`r61_sites`] 登记的那几段。**本条不是全仓 doc-link 检查器**
    /// （`K-R61 §0e` 逐字禁的就是顺手做那个）—— 全仓那个人群多大，读数住件文件 `§8`。
    #[test]
    fn every_address_the_retirement_condition_names_is_still_on_disk() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("src-tauri 的上级 = 仓根")
            .to_path_buf();

        let mut checked: Vec<String> = Vec::new();
        let mut missing: Vec<String> = Vec::new();
        for (name, seg) in r61_segments() {
            for p in r61_paths_in(seg) {
                checked.push(format!("{name} → {p}"));
                if !root.join(&p).exists() {
                    missing.push(format!("{name} → `{p}`"));
                }
            }
        }
        // 反空真：抽不到路径的话下面那条 `is_empty()` 是白过的。
        assert!(
            checked.len() >= 4,
            "只从那几段里抽到 {} 个住址 —— 抽取器坏了，本条此刻在空转。抽到的：{checked:?}",
            checked.len()
        );
        assert!(
            missing.is_empty(),
            "退役条件点着 {} 个**盘上没有**的住址：\n  {}\n\
             ⇒ 这就是 `K-R61` 立件的那个形状：判据活着、前提指着一个已经被删掉的文件，\n\
             中间没有任何东西会响。**改住址的同时把那句理由也重读一遍** ——\n\
             `K-R61` 那一轮变的不是住址，是前提本身。\n\
             本轮核过的全部住址（分母 {}）：{checked:?}",
            missing.len(),
            missing.join("\n  "),
            checked.len()
        );
    }

    /// ★★ **P3t-Y2 的顺序判据**：渲染器在前，`build_local_posix_command` 在后。
    ///
    /// 「两条路都在文件里」证明不了任何事 —— 本件的全部内容就是**谁先谁后**：
    /// 旧路必须是「渲染器拒了才走」的回落，不是并列的第二条路。并列意味着
    /// 「本机进不进 tmux」由谁先被写下来决定，那不是一个能守住的性质。
    ///
    /// 形状抄 `local_backend::the_exit_path_really_stops_the_local_backend`：
    /// 锚点当场核唯一性 + 按花括号配平切体 + 切出来的体自检大小（否则会零命中地绿）。
    #[test]
    fn the_local_launch_tries_the_renderer_before_the_old_path() {
        let prod = guard_core::production_code(include_str!("history.rs"));
        // 锚点唯一性：`fn launch_local(` 全树恰好一处（`find_pinned` 自带边界检查）。
        let at = guard_core::find_pinned(&prod, "fn launch_local(").unwrap_or_else(|e| {
            panic!("`fn launch_local(` 不是恰好一处 —— 形状变了，先修锚点：{e}")
        });
        let body = {
            let b = prod.as_bytes();
            let open = (at..b.len())
                .find(|&i| b[i] == b'{')
                .expect("`fn launch_local(` 之后找不到块起点");
            let (mut depth, mut end) = (0i32, b.len());
            for i in open..b.len() {
                if b[i] == b'{' {
                    depth += 1;
                } else if b[i] == b'}' {
                    depth -= 1;
                    if depth == 0 {
                        end = i + 1;
                        break;
                    }
                }
            }
            &prod[open..end]
        };
        assert!(
            body.len() > 200 && body.len() < 3000,
            "切出来的 `launch_local` 体只有 {} 字节 —— 配平切错了，本条会零命中地绿",
            body.len()
        );

        let r = body.find("render_local_ccm(").expect(
            "`launch_local` 体里找不到 `render_local_ccm(` —— 渲染器没接上，本机还是走旧路",
        );
        let old = body
            .find("build_local_posix_command(")
            .expect("`launch_local` 体里找不到 `build_local_posix_command(` —— 回落没了，渲染器拒了就无路可走");
        assert!(
            r < old,
            "★ 顺序反了：`build_local_posix_command` 出现在 `render_local_ccm` **之前**。\n\
             那样旧路就成了并列的第一条路，渲染器变成够不着的死代码 —— 本件等于没做。"
        );
        // 各恰好一处：两处渲染器调用意味着有一条分支绕过了顺序。
        for (needle, n) in [
            (
                "render_local_ccm(",
                body.matches("render_local_ccm(").count(),
            ),
            (
                "build_local_posix_command(",
                body.matches("build_local_posix_command(").count(),
            ),
        ] {
            assert_eq!(
                n, 1,
                "`launch_local` 体里 `{needle}` 出现 {n} 次 —— 不是恰好一处，顺序就管不住了"
            );
        }
        // 回落必须真的住在 `Err` 那条臂里，而不是顺序碰巧靠后。
        let err_arm = body
            .find("Err(why)")
            .expect("`launch_local` 体里找不到 `Err(why)` —— 回落不在降级臂里了");
        assert!(
            err_arm < old,
            "`build_local_posix_command` 不在 `Err(why)` 臂内 —— 它只是碰巧写在后面，\n\
             那不叫「渲染器拒了才走」。"
        );
    }
    use super::*;

    /// 每个测试独占的临时目录（仓库约定不引 `tempfile`，用 pid + 计数器保唯一）。
    /// **绝不碰用户真实的 `~/.claude`** —— 全部在 `std::env::temp_dir()` 下。
    struct TmpDir(PathBuf);
    static TMP_SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    impl TmpDir {
        fn new() -> Self {
            let p = std::env::temp_dir().join(format!(
                "hist-{}-{}",
                std::process::id(),
                TMP_SEQ.fetch_add(1, std::sync::atomic::Ordering::SeqCst)
            ));
            std::fs::create_dir_all(&p).expect("mkdir");
            TmpDir(p)
        }
        fn write(&self, name: &str, body: &str) -> PathBuf {
            let f = self.0.join(name);
            std::fs::write(&f, body).expect("write");
            f
        }
    }
    impl Drop for TmpDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    /// 〔audit-0805 08-06〕**`read_jsonl_values` 的两个无声决定**：剥 BOM · 静默丢弃坏行。
    ///
    /// 它全仓出现 2 次、所在文件测试段 0 次（先验：只被一处调用的生产函数）。
    /// 两个决定都是**成心的**，也都**没人钉**：
    /// - **剥 BOM**（`trim_start_matches('\u{feff}')`）：Windows 上的文件常带 BOM，
    ///   不剥就是第一行永远 parse 不了 —— 而它的表现是「历史少一条」，不报错。
    /// - **坏行静默丢弃**（`if let Ok(v)`）：一条损坏的行不该让整个历史读不出来。
    ///   这是**刻意的韧性**，但它同时意味着「丢了多少」没人知道 ⇒ 至少要钉住
    ///   「好行一条不少」，否则哪天连好行一起丢也不会红。
    #[test]
    fn read_jsonl_values_strips_bom_and_drops_only_the_broken_lines() {
        let tmp = TmpDir::new();
        let body = format!(
            "\u{feff}{}\n\n   \n{}\n{{ 这行不是 JSON \n{}\n",
            r#"{"a":1}"#, r#"{"b":2}"#, r#"{"c":3}"#
        );
        let f = tmp.write("x.jsonl", &body);
        // ★ 夹具自检：文件里确实有坏行与空行，否则下面在测别的东西。
        assert!(
            body.lines().count() >= 6 && body.contains("这行不是 JSON"),
            "夹具没造出「坏行 + 空行」的场面"
        );

        let got = read_jsonl_values(&f).expect("读不该失败 —— 坏行是丢弃不是报错");
        assert_eq!(
            got.len(),
            3,
            "好行应当一条不少（BOM 那条也算）；实得 {:?}",
            got
        );
        assert_eq!(
            got[0].get("a").and_then(|v| v.as_i64()),
            Some(1),
            "第一行没解析出来 —— BOM 多半没被剥掉"
        );
        assert_eq!(got[2].get("c").and_then(|v| v.as_i64()), Some(3));
    }

    /// ★★〔`K-R97` 09-12 后继形态〕**「从 jsonl 头部抠 cwd」这件事，全仓只剩一处了。**
    ///
    /// # 原形是什么、为什么换
    ///
    /// 原形叫「两个提取器仍旧照登记的样子不一致」〔audit-0805 08-06〕：同一个问题两处实现 ——
    /// monitor 窗口 **30** 行且只认 `JsonlRecord::User`，后端窗口 **40** 行且认任何带非空 cwd
    /// 的记录。后果具体：首个带 cwd 的记录落在第 31–40 行时，**两边给两个答案**。
    /// 那一版**只钉不改**（走档①：登记 + 钉住），因为「取 30 还是 40」是会改行为的设计决定。
    ///
    /// `K-R97` 把本机项目列表改走后端那条 `--list-projects` ⇒ monitor 那一份**连同它唯一的
    /// 调用点一起没了**。⚠ **这不是「对齐到 40」**，是那个设计决定**不再需要有人做** ——
    /// 问题只剩一个实现，也就无从不一致。
    ///
    /// ⇒ 本条换成后继形态：**钉住 monitor 侧不许再长出第二份**，并核后端那一份还在。
    /// ⚠ **这不是降强度**：原形钉的是两个数的差（谁改了都红），后继钉的是「只剩一处」
    /// （谁把第二份写回来都红），而后者恰恰是 `K33`「所有命令只许有一处」的形状。
    #[test]
    fn extracting_cwd_from_a_jsonl_head_now_lives_in_exactly_one_place() {
        // ① monitor 生产段：**一个头部窗口读法都不许有**。
        let own = guard_core::production_code(include_str!("history.rs"));
        let local: Vec<&str> = own
            .lines()
            .filter(|l| l.contains("reader.lines().map_while(Result::ok).take("))
            .collect();
        assert!(
            local.is_empty(),
            "monitor 侧又长出了一份 jsonl 头部读法：\n{}\n\n\
             ⇒ `K-R97` 之后这件事的家在后端（`observe/history_query.rs`）。\n\
             真要在 monitor 侧读，先回答「为什么这条路问不了后端」，再连同本条一起改。",
            local.join("\n")
        );

        // ② 后端那一份还在，且窗口是个说得出的数 —— 否则上面那条会零命中地绿
        //    （「两边都没有」与「只剩一处」在断言上长得一样，这一格就是分开它们的那个）。
        let daemon_src = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .parent()
                .expect("仓根")
                .join("remote-daemon-proto/src/observe/history_query.rs"),
        )
        .expect("读不到后端的 history_query.rs");
        let remote: Vec<usize> = guard_core::production_code(&daemon_src)
            .lines()
            .filter(|l| l.contains("reader.lines().map_while(Result::ok).take("))
            .filter_map(|l| l.split(".take(").nth(1))
            .filter_map(|s| s.split(')').next())
            .filter_map(|s| s.trim().parse::<usize>().ok())
            .collect();
        assert_eq!(
            remote,
            vec![40],
            "后端那一份不是「恰好一处、窗口 40 行」了（实得 {remote:?}）。\n\
             ① 变成 0 处 ⇒ 那件事没人做了，而 monitor 这侧已经不做了；\n\
             ② 变成 2 处 ⇒ 两份实现在后端里面又长了一次。"
        );
    }

    // ═══════════════════════════════════════════════════════════════════
    // `K-R97`：本机项目列表改走后端那条 `--list-projects`
    // ═══════════════════════════════════════════════════════════════════

    /// 一行后端产出（形状照 `observe/history_query.rs::project_row`：5 个字段）。
    fn r97_row(dir: &str, path: &str, sids: &[&str], last_ms: i64) -> serde_json::Value {
        serde_json::json!({
            "dirName": dir,
            "projectPath": path,
            "sessionCount": sids.len(),
            "lastActivityMs": last_ms,
            "sessionIds": sids,
        })
    }

    /// 把几行折成后端的 stdout（逐行 JSON）。
    fn r97_stdout(rows: &[serde_json::Value]) -> QueryOutcome {
        QueryOutcome::Ok(
            rows.iter()
                .map(|r| r.to_string())
                .collect::<Vec<_>>()
                .join("\n")
                + "\n",
        )
    }

    /// 会答话的假真相源（同 `remote_history` 测试段那两个夹具的形状）。
    /// ⚠ 它是**夹具**，不是第二份实现：生产那份是 [`SessionMapLiveness`]。
    struct R97Oracle(&'static [&'static str]);
    impl LivenessOracle for R97Oracle {
        fn is_live(&self, _origin: &str, sid: &str) -> Counted<bool> {
            Counted::Known(self.0.contains(&sid))
        }
    }

    /// ★★ `KR97D1`：**本机那条路的数据来自后端** —— 判的是性质，不是写法。
    ///
    /// 第 ① 刀「后端产出变了而本机不跟 ⇒ 红」＋ 第 ② 刀「跟了 ⇒ 绿」都在这里：
    /// 同一条路喂**两份不同的后端产出**，逐字段看它跟不跟。
    ///
    /// ⚠ 逐字**不判**「代码里还有没有 `read_dir`」（那判的是写法，改个写法就瞎）。
    /// 第 ③ 刀「本机退回自己遍历 records 根 ⇒ 红」由**另一处**接住，而且它更硬：
    /// `local_read_surface_registry` 的递减棘轮按「文件 → 命中行数」逐行对账，
    /// 谁把 `resolve_claude_dir()` / `records_dir()` 写回 `history.rs`，那条当场红
    ///（`K-R97` 之后 `src/history.rs` 登记的处数之和是 **13**，写回去就是 15）。
    #[test]
    fn the_local_project_list_is_whatever_the_backend_said() {
        let md = HistoryMetadata::default();
        let live = R97Oracle(&[]);

        let a = local_projects_via(
            |_| r97_stdout(&[r97_row("-w-alpha", "/w/alpha", &["s1", "s2"], 111)]),
            &md,
            &live,
        )
        .expect("后端答了，这一趟该成");
        assert_eq!(a.len(), 1, "后端只说了一个项目，本机却给出 {} 个", a.len());
        assert_eq!(a[0].0.project_name, "alpha");
        assert_eq!(a[0].0.project_path, "/w/alpha");
        assert_eq!(a[0].0.session_count, 2);
        assert_eq!(a[0].0.last_activity, 111);
        assert_eq!(
            a[0].0.project_dir, "-w-alpha",
            "懒加载键要原样带回后端给的名字"
        );
        assert_eq!(a[0].0.origin, None, "本机那条路 origin 恒为 None");

        // ★ 换一份后端产出：**同一个项目键**，其余全变。本机的返回必须跟着变。
        let b = local_projects_via(
            |_| r97_stdout(&[r97_row("-w-alpha", "/w/beta", &["s1", "s2", "s3"], 222)]),
            &md,
            &live,
        )
        .expect("后端答了，这一趟该成");
        assert_eq!(
            (
                b[0].0.project_name.as_str(),
                b[0].0.project_path.as_str(),
                b[0].0.session_count,
                b[0].0.last_activity
            ),
            ("beta", "/w/beta", 3, 222),
            "🔴 后端那一行变了，本机的返回没跟着变 —— 那说明这条路的数据**不是**后端给的。\n\
             这正是本条第 ① 刀：改后端那条的产出，本机跟不跟。"
        );

        // ★ 后端没说的项目不许冒出来（「数据只来自后端」的另一半）。
        let none = local_projects_via(|_| QueryOutcome::Ok(String::new()), &md, &live)
            .expect("空答复也是答复");
        assert!(
            none.is_empty(),
            "后端一行都没说，本机却端出了 {} 个项目",
            none.len()
        );
    }

    /// ★★ `KR97D2`：**「不知道」一路带到本机这条，不被压平。**
    ///
    /// `K-R92` 那一形的预防：后端那一行**没带 sid 清单**（旧版后端）时，
    /// star / hide / 活状态三个数**算不出来** —— 那不是 0、不是「没有星标」、
    /// 更不是「这个项目没有活会话」。
    ///
    /// 第 ① 刀「压平 ⇒ 红」用 `assert_ne!` 逐格钉：`Some(0)` / `Some(false)` 与 `None`
    /// 在类型上分得开，压平当场红。第 ② 刀「带得过去 ⇒ 绿」是那三个 `None`。
    #[test]
    fn an_unknown_from_the_backend_row_is_not_flattened_on_the_local_path() {
        let md = HistoryMetadata::default();
        // 旧版后端那一行：**只有 4 个字段**，没有 `sessionIds`。
        let old_row = serde_json::json!({
            "dirName": "-w-alpha",
            "projectPath": "/w/alpha",
            "sessionCount": 2,
            "lastActivityMs": 111,
        });
        let out = local_projects_via(|_| r97_stdout(&[old_row.clone()]), &md, &R97Oracle(&["s1"]))
            .expect("行是好的，只是少了一个字段");
        let p = &out[0].0;
        assert_eq!(
            p.starred_count, None,
            "🔴 算不出来的星标数被说成了一个数 —— 「不知道」在这一段被压平了"
        );
        assert_ne!(
            p.starred_count,
            Some(0),
            "🔴 `Some(0)` 是同一句谎话换了个类型说一遍：它读作「查过了，一个星标都没有」"
        );
        assert_eq!(p.hidden_count, None, "同上，hidden 那一格");
        assert_ne!(p.hidden_count, Some(0), "同上，hidden 那一格");
        assert_eq!(p.has_live, None, "同上，活状态那一格");
        assert_ne!(
            p.has_live,
            Some(false),
            "🔴 `Some(false)` 读作「查过了，这个项目没有活会话」—— 而根本没人查过"
        );

        // ★ 反面：带了清单就该**算得出真值**，否则上面三条会变成「反正都是 None」的空转。
        let md2 = {
            let mut m = HistoryMetadata::default();
            m.entries.insert(
                "s1".to_string(),
                EntryMetadata {
                    starred: true,
                    ..Default::default()
                },
            );
            m
        };
        let good = local_projects_via(
            |_| r97_stdout(&[r97_row("-w-alpha", "/w/alpha", &["s1", "s2"], 111)]),
            &md2,
            &R97Oracle(&["s2"]),
        )
        .expect("这一行是全的");
        assert_eq!(
            good[0].0.starred_count,
            Some(1),
            "★ 真值端得动（本机 metadata 按 sid 合）"
        );
        assert_eq!(
            good[0].0.hidden_count,
            Some(0),
            "★ 「查过了，是 0」也是一个真值"
        );
        assert_eq!(good[0].0.has_live, Some(true), "★ 活状态端得动");
    }

    /// ★ `KR97D2` 的另一半：**本机这条路的判活真相源答得出真值**，不许跟着远端一起「不知道」。
    ///
    /// 远端那个绑定（`NoLivenessOracleYet`）答不出是有理由的（`SessionMap` 只认本机 pid）；
    /// 本机这个绑定**没有那个理由** —— 它要是也答「不知道」，那就是把一处能查的事说成查不了。
    #[test]
    fn the_local_liveness_oracle_answers_known_not_unknown() {
        let tmp = TmpDir::new();
        let (map, _rx) = SessionMap::load_with_changes(tmp.0.clone(), true);
        let oracle = SessionMapLiveness(map);
        assert_eq!(
            oracle.is_live("", "没有这个会话"),
            Counted::Known(false),
            "🔴 本机答得出「查过了，没活」—— 答成 `Unknown` 就是把能查的事说成查不了"
        );
    }

    /// ★★ `KR97D3`：**一次调用里问了后端几次** —— 判的是这个可数的事实，不是有没有 for 循环。
    ///
    /// 同 `KR83D3` 的口径。失效方向具体得很：一旦有人为了拿 star/hide 而在那个循环里
    /// 补一句 `--list-sessions`，计数当场从 `1` 涨成 `1 + 项目数`。
    #[test]
    fn one_call_asks_the_backend_exactly_once_no_matter_how_many_projects() {
        let md = HistoryMetadata::default();
        let calls = std::cell::Cell::new(0usize);
        let rows = [
            r97_row("-p1", "/w/p1", &["a1"], 1),
            r97_row("-p2", "/w/p2", &["b1", "b2"], 2),
            r97_row("-p3", "/w/p3", &["c1", "c2", "c3"], 3),
        ];
        let out = local_projects_via(
            |args| {
                calls.set(calls.get() + 1);
                assert_eq!(args, &["--list-projects"], "问的不是这条子命令");
                r97_stdout(&rows)
            },
            &md,
            &R97Oracle(&[]),
        )
        .expect("后端答了");
        assert_eq!(out.len(), 3, "夹具没喂进 3 个项目，下面那条计数就没有意义");
        assert_eq!(
            calls.get(),
            1,
            "🔴 3 个项目问了后端 {} 次。一次调用只许问一次 —— \n\
             逐项目再问一次的话，项目列表这个常开界面会变成 N 次进程 spawn。",
            calls.get()
        );
    }

    /// ★ 三态诚实降级（定框 §5）：**「后端不在」不是「一个历史项目都没有」。**
    ///
    /// 这两件事对用户是完全不同的处境：前者该提示装 / 该修，后者是真的空。
    /// 压成一个空列表就是 F14 那次「静默回落」的形状。
    #[test]
    fn a_missing_backend_is_not_an_empty_project_list() {
        let md = HistoryMetadata::default();
        let no_backend = local_projects_via(
            |_| QueryOutcome::NoBackend("找过 [\"…/cc-monitor-remote\"]".into()),
            &md,
            &R97Oracle(&[]),
        )
        .expect_err("后端不在时不许返回一个空列表");
        assert!(
            no_backend.contains("本机后端不在"),
            "报错没说清是「后端不在」：{no_backend}"
        );
        let failed = local_projects_via(
            |_| QueryOutcome::Failed {
                code: Some(2),
                stderr: "read_dir failed\n".into(),
            },
            &md,
            &R97Oracle(&[]),
        )
        .expect_err("查询失败时不许返回一个空列表");
        assert!(
            failed.contains("查询失败") && failed.contains("read_dir failed"),
            "报错没带上后端说的原因：{failed}"
        );
        assert_ne!(
            no_backend, failed,
            "🔴 「后端不在」与「后端在但这条查询失败了」被说成了同一句话 —— \n\
             那正是让上层猜的那一形（定框 §5）。"
        );
    }

    /// Phase 2 F1a-3：Codex 会话按 cwd 分组成合成 HistoryProject（count/max-mtime/name/键/has_live）。
    /// 测试用：把一个 configDir 包成具名账号。
    fn named(d: &str) -> LaunchAccount {
        LaunchAccount::Named {
            config_dir: d.to_string(),
            name: None,
        }
    }

    // ── G3b：本地拉起的账号注入（`CLAUDE_CONFIG_DIR`）─────────────────────────
    //
    // 既有那批**逐字节钉死输出**的测试全部传 `None` 后原样通过 ——
    // 它们因此就是「**账号 0 = 一个字都不注入 = 旧行为**」的守卫，不用再写一条。

    #[test]
    fn account_zero_injects_nothing_byte_for_byte() {
        // 三种「没有账号」的表达（None / 空串 / 纯空白）产出必须完全相同。
        // ① 参数缺席 = 调用方没表态 ⇒ 一个字都不注入（既有调用点逐字节等价旧行为）
        let a = build_local_posix_command(&LocalPsAction::New, None, None).unwrap();
        assert!(
            !a.contains("CLAUDE_CONFIG_DIR"),
            "没表态时不该出现这个变量名"
        );

        // ② ★★ 显式账号 0 = **unset**，不是「什么都不加」。Phase G 审计抓出的静默串号：
        //    本地拉起故意加载 rc，而 rc 里很可能有 `export CLAUDE_CONFIG_DIR=<默认账号>`
        //    ⇒ 「什么都不加」会落到别的号上，而弹窗上写着「不注入」。
        let base = build_local_posix_command(&LocalPsAction::New, None, Some(&LaunchAccount::Base))
            .unwrap();
        assert!(
            base.starts_with("unset CLAUDE_CONFIG_DIR; "),
            "账号 0 必须显式 unset：{base}"
        );
        assert!(base.ends_with(&a), "前缀不该改动命令本体");
        let base_ps =
            build_local_ps_command(&LocalPsAction::New, None, Some(&LaunchAccount::Base)).unwrap();
        assert!(
            base_ps.starts_with("$env:CLAUDE_CONFIG_DIR=$null; "),
            "PS 侧的账号 0 同样要显式清掉：{base_ps}"
        );

        // ③ 具名账号但 configDir 是空串 ⇒ **坏数据，报错**（空值 ≠ 未设）
        assert!(build_local_posix_command(&LocalPsAction::New, None, Some(&named(""))).is_err());
        assert!(build_local_posix_command(&LocalPsAction::New, None, Some(&named("   "))).is_err());
    }

    #[test]
    fn account_prefix_is_prepended_posix_and_ps() {
        let dir = "/home/u/.claude-accts/z";
        let px = build_local_posix_command(&LocalPsAction::New, None, Some(&named(dir))).unwrap();
        assert!(
            px.starts_with(&format!("export CLAUDE_CONFIG_DIR='{dir}'; ")),
            "POSIX 前缀必须在最前面（要先于拉起命令生效）：{px}"
        );
        // 前缀之后仍是原来那条命令，逐字节
        let bare = build_local_posix_command(&LocalPsAction::New, None, None).unwrap();
        assert!(px.ends_with(&bare), "前缀不该改动命令本体");

        let ps = build_local_ps_command(&LocalPsAction::New, None, Some(&named(dir))).unwrap();
        assert!(
            ps.starts_with(&format!("$env:CLAUDE_CONFIG_DIR='{dir}'; ")),
            "{ps}"
        );
    }

    /// ★★ Phase G 审计抓出的真 bug：**Windows 上的账号目录必须被接受**。
    ///
    /// 原来 POSIX 与 PS 共用一条「必须 `/` 开头 + 禁 `\`」的校验 ⇒
    /// `C:\Users\z\.claude-accts\z` 恒被拒 ⇒ 「本机分叉时选一个具名账号」在**主平台**上
    /// 100% 失败（`fork-flow.ts` 是全仓唯一给 `resume_history_session` 传 `configDir` 的
    /// 调用点，所以这个洞是分叉专属的、别处测不到）。
    ///
    /// 判据照抄 `local_accounts::looks_absolute` —— 那个函数的头注已经写明这一课。
    /// 而**旧测试全喂 POSIX 路径**（`/home/u/.claude-accts/z`），所以它们测不出来。
    #[test]
    fn windows_account_dirs_are_accepted_by_the_ps_side() {
        for d in [
            "C:\\Users\\z\\.claude-accts\\z",
            "D:/Users/z/.claude-accts/b",
            "\\\\server\\share\\accts\\z",
        ] {
            assert!(
                build_local_ps_command(&LocalPsAction::New, None, Some(&named(d))).is_ok(),
                "Windows 账号目录被拒了：{d:?}"
            );
        }
        // POSIX 那条**仍然**只收 POSIX 路径（各自平台各自判据，别互相放宽）
        assert!(
            build_local_posix_command(&LocalPsAction::New, None, Some(&named("C:\\Users\\z")))
                .is_err(),
            "POSIX 侧不该接受 Windows 路径"
        );
        // 反斜杠形态的 `..` 也要挡住
        for d in ["C:\\Users\\..\\evil", "C:\\Users\\z\\.."] {
            assert!(
                build_local_ps_command(&LocalPsAction::New, None, Some(&named(d))).is_err(),
                "PS 侧漏了反斜杠 `..`：{d:?}"
            );
        }
        // 引号仍然禁（单引号能提前闭合 PS 的字面量串）
        assert!(
            build_local_ps_command(&LocalPsAction::New, None, Some(&named("C:\\a'; rm x; '")))
                .is_err()
        );
    }

    /// ★ 非法 configDir **绝不拼进命令** —— 这条产物会进 shell，宽容一格就是注入面。
    /// 判据照抄 TS 侧 `isValidConfigDir`（`src/shell-quote.ts:41`），不重新发明。
    #[test]
    fn illegal_config_dir_is_refused_not_sanitized() {
        let bad = [
            "relative/path",              // 非绝对
            "/",                          // 根
            "/home/u/../../etc",          // 含 /../
            "/home/u/..",                 // 以 /.. 结尾
            "/home/u'; rm -rf /; echo '", // 单引号闭合 + 注入
            "/home/u`whoami`",            // 反引号
            "/home/u$HOME",               // 变量展开
            "/home/u;id",                 // 分号
            "/home/u|id",                 // 管道
            "/home/u\n/x",                // 控制符（字面反斜杠 n 不算，见下面真控制符）
            "/home/u\u{0000}x",
            "/home/u\u{200b}x", // 零宽
            "/home/u\u{feff}x", // BOM
            "/home/u\u{00a0}x", // NBSP
        ];
        for d in bad {
            assert!(
                build_local_posix_command(&LocalPsAction::New, None, Some(&named(d))).is_err(),
                "非法 configDir 竟被接受：{d:?}"
            );
            assert!(
                build_local_ps_command(&LocalPsAction::New, None, Some(&named(d))).is_err(),
                "非法 configDir 竟被接受（PS）：{d:?}"
            );
        }
        // 反向自检：正常路径必须通过，否则上面全是空转
        assert!(build_local_posix_command(
            &LocalPsAction::New,
            None,
            Some(&named("/home/u/.claude-accts/z"))
        )
        .is_ok());
    }

    /// ★ U8c-1：POSIX 校验改调内核（P4b 起 `backend::control::payload`）之后**多拒**的那六段码位。
    ///
    /// 这不是纯重构 —— 本文件原先用的 `SPOOFABLE` 是 **U7-3 之前**的旧集合，
    /// 而内核建立在 `acct_core::is_deceptive_char` 的并集上。这条测试点名那六段，
    /// 变异（把内核换回旧表）时会逐个报出来。
    ///
    /// ⚠ **PS 那条路刻意还用旧表**（Windows 平台特化，见 `SPOOFABLE` 头注），
    /// 所以这里只断言 POSIX 侧 —— 断言 PS 侧会当场红，那才是假装做完了。
    #[test]
    fn posix_config_dir_now_rejects_the_code_points_u7_3_added() {
        for (name, c) in [
            ("U+1680 Ogham space", '\u{1680}'),
            ("U+2000 en quad", '\u{2000}'),
            ("U+200A hair space", '\u{200a}'),
            ("U+202F narrow NBSP", '\u{202f}'),
            ("U+205F medium math space", '\u{205f}'),
            ("U+2060 word joiner", '\u{2060}'),
            ("U+3000 ideographic space", '\u{3000}'),
        ] {
            let dir = format!("/home/u/.claude-accts/{c}z");
            assert!(
                build_local_posix_command(&LocalPsAction::New, None, Some(&named(&dir))).is_err(),
                "{name} 应被 POSIX 侧拒掉（acct-core 并集里有它，history.rs 旧表没有）"
            );
        }
    }

    /// ★ U8c-1：合法输入的产物**逐字节不变** —— 搬内核不许改一个字节。
    #[test]
    fn posix_account_prefix_is_byte_identical_after_moving_to_the_kernel() {
        assert_eq!(
            config_dir_prefix_posix(None).unwrap(),
            "",
            "参数缺席仍是空串（既有调用点逐字节等价旧行为）"
        );
        assert_eq!(
            config_dir_prefix_posix(Some(&LaunchAccount::Base)).unwrap(),
            "unset CLAUDE_CONFIG_DIR; ",
            "账号 0 的逐字节形态被 e2e 探针 grep 着"
        );
        assert_eq!(
            config_dir_prefix_posix(Some(&named("/home/u/.claude-accts/z"))).unwrap(),
            "export CLAUDE_CONFIG_DIR='/home/u/.claude-accts/z'; "
        );
    }

    #[test]
    fn codex_projects_group_by_cwd() {
        let sessions = vec![
            CodexSessionInfo {
                sid: "s1".into(),
                path: PathBuf::from("/a"),
                cwd: "/home/u/proj".into(),
                mtime_ms: 100,
            },
            CodexSessionInfo {
                sid: "s2".into(),
                path: PathBuf::from("/b"),
                cwd: "/home/u/proj".into(),
                mtime_ms: 300,
            },
            CodexSessionInfo {
                sid: "s3".into(),
                path: PathBuf::from("/c"),
                cwd: "".into(),
                mtime_ms: 50,
            },
        ];
        let projects = codex_projects_from(sessions);
        assert_eq!(projects.len(), 2, "两个 cwd 组");
        let proj = projects
            .iter()
            .find(|p| p.project_path == "/home/u/proj")
            .expect("proj 组");
        assert_eq!(proj.session_count, 2);
        assert_eq!(proj.last_activity, 300, "组内 max mtime");
        assert_eq!(proj.project_name, "proj", "cwd 末段");
        assert_eq!(proj.project_dir, "codex:/home/u/proj", "键带 codex: 前缀");
        assert_eq!(
            proj.has_live, None,
            "★ `K-R92`：Codex 判活 = F4（无 pidfile）⇒ 这一格是**不知道**。\n\
             上一版这里断言的是 `false` —— 那是「查过了，没有活会话」，而根本没人查过。"
        );
        let unknown = projects
            .iter()
            .find(|p| p.project_path.is_empty())
            .expect("空 cwd 组");
        assert_eq!(unknown.project_name, "(codex)");
        assert_eq!(unknown.project_dir, "codex:");
    }

    /// F1a-3c + Phase G 审计修：Codex 会话摘要取首个**真** user message，跳 CLI 注入块——
    /// 复用渲染路同一 `is_injected_context`（**3 标记**：environment_context / recommended_plugins /
    /// # AGENTS.md instructions），与渲染去噪一致（此前只跳 environment_context）。
    #[test]
    fn codex_first_user_excerpt_skips_injected_context() {
        let dir = std::env::temp_dir().join(format!("ccm-codex-exc-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let f = dir.join("rollout.jsonl");
        let user = |t: &str| {
            format!(
                r#"{{"type":"response_item","payload":{{"type":"message","role":"user","content":[{{"type":"input_text","text":{}}}]}}}}"#,
                serde_json::to_string(t).unwrap()
            )
        };
        // 首 3 条 user = 3 种注入块（全跳）；末 user = 真用户输入（取）。
        let content = [
            r#"{"type":"session_meta","payload":{"cwd":"/p"}}"#.to_string(),
            user("<environment_context>injected</environment_context>"),
            user("<recommended_plugins>\nplugins…"),
            user("# AGENTS.md instructions\n\n<INSTRUCTIONS>\n# AGENTS.md\n本文件…"),
            user("真实问题"),
        ]
        .join("\n");
        std::fs::write(&f, content).unwrap();
        assert_eq!(codex_first_user_excerpt(&f), "真实问题");
        std::fs::remove_dir_all(&dir).ok();
    }

    /// ★★ **建分支入口真的过了围栏吗**〔audit-0805 08-07，Phase G 第 48 件〕。
    ///
    /// 与删除那条**同一族的第二例**。08-07 实测：把当时那行守卫换成裸的
    /// `PathBuf::from(<调用方给的串>)`，**全仓 979 条判据一条不红** ——
    /// 而那条路会去**读**调用方给的任意文件，再把内容拷进 `projects` 目录。
    ///
    /// ⇒ 一族两例，说明这不是某个人某次疏忽：**「围栏有判据」与「那条路过了围栏」
    /// 是两件事，而写判据的注意力天然落在前者**（后者要跑真路，前者只要调个函数）。
    ///
    /// # 🔴〔`K-R88` 09-13〕**围栏换了形状，本条跟着换靶，不是删**
    ///
    /// 入参从路径收成 sid 之后，「一个 `projects` 之外的源」**连表达都表达不出来**：
    /// 一个绝对路径根本不是合法 sid，而合法 sid 只会在记录树里被枚举出来。
    /// ⇒ 本条今天钉的是**那一步真的经过了形状闸**：喂一个界外的绝对路径，
    /// 必须在**任何 IO 之前**被拒，且拒的理由要点名它是 sid 形状不合法。
    #[test]
    fn the_branch_entry_point_actually_goes_through_the_fence() {
        let base = std::env::temp_dir().join(format!(
            "ccm-branch-fence-probe-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let projects = base.join("projects");
        std::fs::create_dir_all(&projects).expect("建临时 projects");
        // 源文件放在 projects **之外**：围栏在的话必须拒。
        let outsider = base.join("outsider.jsonl");
        std::fs::write(&outsider, "{\"type\":\"user\"}\n").expect("造界外源文件");

        let r = branch_impl(&outsider.to_string_lossy(), "uuid-x", &projects);
        let still_there = outsider.exists();
        let _ = std::fs::remove_dir_all(&base);

        let err = r.err().unwrap_or_else(|| {
            panic!(
                "`branch_impl` 接受了一个 **`projects` 之外**的源 —— 围栏没接上。\n\
                 那条路会去读调用方给的任意文件，再把内容拷进 projects 目录。"
            )
        });
        assert!(still_there, "界外那份被动过了");
        // 红要红对成因：必须是**形状闸**拒的，不是后面某步偶然失败。
        assert!(
            err.contains("invalid session id"),
            "拒绝了，但不是形状闸拒的（错误：{err}）—— \
             本条没真跑到那一步，等于空转。"
        );
    }

    /// 下面那条判据的**子进程哨兵**。
    ///
    /// ⚠ 名字是本判据**专属的假变量**（同 `lib.rs` 里 `env_scrub_tests` 那条纪律）——
    /// 真正的 `CLAUDE_CONFIG_DIR` 只经 `Command::env` 给**子进程**，
    /// 本进程与宿主的环境都没有被动过。
    const FENCE_CHILD: &str = "CCM_TEST_DELETE_FENCE_CHILD";

    /// ★★ **删除入口真的过了围栏吗**〔audit-0805 08-07，Phase G 第 47 件〕。
    ///
    /// 下面五条穿越防护判的都是 `validate_delete_target` **这个函数本身**。它们是实的，
    /// 但它们的主语是**围栏**，不是「那条路真的过了围栏」——
    /// 08-07 实测：把 `delete_history_session` 里那行换成
    /// `let target = PathBuf::from(&jsonl_path);`（整个跳过围栏），
    /// **全仓 978 条判据一条不红**，而那条路是 `fs::remove_file`：
    /// 前端传什么就删什么，用户机器上任意文件。
    ///
    /// ⇒ 与 F27（`history_query` 那两份围栏）同族，也是 F+ 第二问反复报的那个形状：
    /// **纯函数层钉满、接线层为零**。
    ///
    /// # 为什么做成端到端而不是扫源码
    ///
    /// 扫「函数体里有没有 `validate_delete_target(`」只是**代理**（上一件刚记过这条）。
    /// 这里能直接跑真路：造一个**在 `projects` 之外**的真临时文件，要求入口拒绝**且文件还在**。
    /// 围栏一旦被绕过，这条会把那个临时文件真删掉 —— 于是「文件还在」这半当场红。
    /// ⚠ 只碰自己造的临时目录；`~/.claude/` 一个字节都不写（红线）。
    ///
    /// # 🔴 09-10：**本条自己造出它要的前提** —— 而且是在**子进程**里
    ///
    /// 上一版依赖「这台机器上 `~/.claude/projects` 存在」。**那不是它要验的性质，
    /// 是它没建立的前提**：`resolve_claude_dir()` 的第三级回落 `~/.claude`
    /// **不检查存在性**，于是在一台干净机器上入口会在**围栏之前**就 `Err`，
    /// 而「拒了」「文件还在」两格照样绿 —— 09-09 云端首跑红的正是最后那格，
    /// 它报的是「围栏没接上 / 措辞改了」，**两条都是假话**。
    ///
    /// ## 为什么**不**在本进程里 `set_var("CLAUDE_CONFIG_DIR", …)`
    ///
    /// 本仓有一条写下来的纪律，逐字在 `lib.rs` 的 `env_scrub_tests` 里：
    /// 「cargo test 多线程跑，进程级 env 是共享的，**绝不能在测试里 set/remove
    /// 真实的 `CLAUDE_*` 变量**（会干扰并发测试与宿主环境）」。
    /// [`RelayFactSources`] 头注 ㈠ 那一栏记着同族的第二条代价：这种判据
    /// 「必须 `--test-threads=1` ⇒ 只能住 `#[ignore]` 的 e2e 那条道」——
    /// 而那等于本条在 CI 上根本不跑。⇒ 两条路都堵死。
    ///
    /// ## 落法：把那一趟整个搬进子进程
    ///
    /// 父进程造一份**自己的** claude 目录，只经 `Command::env` 交给子进程
    ///（子进程在起来那一刻就带着它，**谁的进程环境都没有被改过**），
    /// 再拿本判据自己的可执行文件、以本判据的名字当过滤器跑一趟。
    /// ⇒ 本进程环境一个字节没动 · 并发判据一格没被干扰 · Linux / Windows 上都跑得动。
    ///
    /// ⚠ **反空真**：过滤器一条都没命中时 libtest 的退出码**也是 0**（「0 passed」）——
    /// 那会是一次干净的假绿。所以父进程除了看退出码，还断子进程真的报了 `1 passed`。
    #[test]
    fn the_delete_entry_point_actually_goes_through_the_fence() {
        // ═══ 父进程那一半：造夹具 · 起子进程 · 把子进程的正文转发出来，然后 `return` ═══
        //
        // ⚠ 两半**刻意写在同一个 `#[test]` 里**〔09-10 第二拍〕。
        //   上一版把子进程那一半拆成了一个单独的函数，被 `structural_scan` 里那条
        //   「测试段里长得像判据、却没有 `#[test]`」的机检判红 ——
        //   **那条红是对的，不是误报**：它认的是「**无参无返回**的 `fn 名()`」这个**形状**
        //   （判别式看的是行首那个 `fn ` 与行尾那个 `() {`，**不看名字**
        //   ⇒ 改名闭不了它的嘴），而那正是判据的形状 ——
        //   读的人无从知道那一大段断言到底跑不跑。
        //
        // 🔴 处置**不是**给它随手加一个用不上的参数（或返回值）把判别式糊过去：
        //   那是钻空子，而且**一个字都没治那个真问题** —— 读者照旧分不出它跑不跑。
        // ⇒ 搬回**唯一那个 `#[test]`** 里。读者看见一个 `#[test]` 与一个 `return`，
        //   就知道下面那一半在哪一趟跑；这个文件的测试段里再没有「长得像判据却不是判据」的东西。
        if std::env::var_os(FENCE_CHILD).is_none() {
            let name = format!("ccm-delete-fence-home-{}", std::process::id());
            let claude_dir = std::env::temp_dir().join(name);
            // 造的是**空的**记录目录 —— 围栏只 `canonicalize` 它，不读里面的东西。
            // ⚠ 目录名走生产那一份 `records_dir`，**不在这里另抄一个 `"projects"`**：
            //   本条要的是「入口会去 canonicalize 的那个目录真的在」，而它叫什么名字
            //   归活跃适配器管 —— 抄一份就会漂。
            let records = crate::adapter::records_dir(&claude_dir);
            std::fs::create_dir_all(&records).expect("造 claude 目录夹具失败");

            let exe = std::env::current_exe().expect("拿不到本判据自己的可执行文件");
            let out = std::process::Command::new(&exe)
                .arg("the_delete_entry_point_actually_goes_through_the_fence")
                .arg("--nocapture")
                .arg("--test-threads=1")
                .env(FENCE_CHILD, "1")
                .env("CLAUDE_CONFIG_DIR", &claude_dir)
                .output()
                .expect("起不来子进程 —— 本条判不了，不许当成绿");
            let so = String::from_utf8_lossy(&out.stdout).into_owned();
            let se = String::from_utf8_lossy(&out.stderr).into_owned();
            let _ = std::fs::remove_dir_all(&claude_dir);

            assert!(
                out.status.success(),
                "子进程里那一趟红了（退出码 {:?}）—— 正文在下面，别只看这一行。\n\
                 ── 子进程 stdout ──\n{so}\n── 子进程 stderr ──\n{se}",
                out.status.code()
            );
            // ★ 反空真：过滤器零命中时 libtest 报 `ok. 0 passed;` 而**退出码也是 0**。
            //   ⚠ 针带上 `ok. ` 与 `;` 两侧边界：裸 `"1 passed"` 会被 `11 passed` 顺带满足。
            assert!(
                so.contains("ok. 1 passed;"),
                "子进程没有恰好跑到本判据那一趟（过滤器命中数不是 1）—— 本条会假绿。\n\
                 ── 子进程 stdout ──\n{so}\n── 子进程 stderr ──\n{se}"
            );
            return;
        }

        // ═══ 子进程那一半：真正那一趟（`CLAUDE_CONFIG_DIR` 已经在环境里）═══
        //
        // 前置条件仍然留着当兜底〔09-09 补的那一格，别删〕：注入万一没生效，
        // 本条要说人话，而不是把「前提没建立」报成「围栏没接上」。
        // 唯一会让它没生效的路：这台机器的 monitor config.json 里写了 `claudeDir`
        // 且那个目录真在 —— 它在 `resolve_claude_dir` 里**优先于**环境变量。
        let Some(claude_dir) = paths::resolve_claude_dir() else {
            panic!("解析不出 claude 目录 —— 本条判不了")
        };
        let projects_dir = crate::adapter::records_dir(&claude_dir);
        let canon_projects = projects_dir.canonicalize().unwrap_or_else(|e| {
            panic!(
                "本条的前置条件不成立：{} 打不开（{e}）——\n\
                 入口会在**围栏之前**就失败，那时本条判的根本不是围栏。\n\
                 ⇒ 父进程已经把 `CLAUDE_CONFIG_DIR` 指向一份自己造好的目录；\
                 拿到别的说明这台机器的 monitor config.json 里写了 `claudeDir`\n\
                 （它在 `resolve_claude_dir` 里优先于环境变量）。\n\
                 🔴 不许把本条改成「读不到就跳过」—— 那是把闸拆了。",
                projects_dir.display()
            )
        });

        let dir = std::env::temp_dir().join(format!(
            "ccm-delete-fence-probe-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        std::fs::create_dir_all(&dir).expect("建临时目录");
        let victim = dir.join("victim.jsonl");
        std::fs::write(&victim, "not yours").expect("造临时文件");

        // ★ **反空真**〔09-10 补〕：本条全部的力气都押在「靶子在记录目录**之外**」上。
        //   靶子要是落在里面，围栏**放行**才是对的，而下面那两格会把放行读成缺陷。
        //   先前这一格是**假设**的（「临时目录当然不在 `~/.claude` 里」）——现在现算一次。
        let canon_victim = victim.canonicalize().expect("靶子打不开");
        let outside = !canon_victim.starts_with(&canon_projects);

        let r = delete_history_session("sid".into(), victim.to_string_lossy().into_owned());
        let still_there = victim.exists();
        let _ = std::fs::remove_dir_all(&dir);

        assert!(
            outside,
            "靶子 {} 落在了记录目录 {} **里面** —— 本条的前提不成立，\
             围栏在这一格**放行**才是对的。",
            canon_victim.display(),
            canon_projects.display()
        );
        let err = r.expect_err(
            "`delete_history_session` 接受了一个 **`projects` 之外**的路径 —— \
             围栏没接上，前端传什么就删什么。",
        );
        assert!(
            still_there,
            "那个临时文件**真被删了**（错误：{err}）—— 围栏被绕过，\
             `fs::remove_file` 直接落在了调用方给的路径上。"
        );
        // ★ 红要红对成因：必须是**围栏**拒的，不能是「claude dir not found」之类前置失败，
        //   否则本条会在一个根本没跑到围栏的环境里假绿。
        // ⚠ 08-07 收紧：原写 `contains("refuse delete") || contains("outside")`。
        // 删除这一侧的 `refuse delete:` 前缀**只有围栏在用**（全文件两处，都在围栏里）
        // ⇒ 本条当时没问题。但**建分支那条同形判据栽在这上面**：那边的
        // `refuse branch:` 前缀下游还有四处，跳过围栏之后下游照样报一条同前缀的错，
        // 判据在它自己要抓的那一刀上是绿的。⇒ 这里一并收紧成围栏**独有**的措辞。
        assert!(
            err.contains("is outside"),
            "拒绝了，但不是**围栏的越界检查**拒的（错误：{err}）—— \
             要么围栏没接上而下游某步偶然报了错（两者长得一样，只有这句话分得开），\
             要么围栏的措辞改了而本条没跟。"
        );
    }

    // === Batch4-F15：validate_delete_target 穿越防护 ===

    /// 独立临时 projects 目录（惯例同 utils.rs / watcher.rs 测试）。
    fn temp_projects(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir()
            .join(format!("ccm-hist-del-{}-{}", tag, std::process::id()))
            .join("projects");
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn delete_rejects_dotdot_traversal() {
        let projects = temp_projects("dotdot");
        let root = projects.parent().unwrap();
        // projects 外造一个真实存在的 .jsonl，再用 `..` 从 projects 内指出去
        let outside = root.join("outside.jsonl");
        std::fs::write(&outside, "{}\n").unwrap();
        let sneaky = projects.join("..").join("outside.jsonl");
        let err = validate_delete_target(sneaky.to_str().unwrap(), &projects).unwrap_err();
        assert!(err.contains("refuse delete"), "got: {err}");
        assert!(outside.exists(), "file must survive the refused delete");
        std::fs::remove_dir_all(root).ok();
    }

    #[cfg(unix)]
    #[test]
    fn delete_rejects_symlink_escaping_projects() {
        let projects = temp_projects("symlink");
        let root = projects.parent().unwrap();
        let outside = root.join("secret.jsonl");
        std::fs::write(&outside, "{}\n").unwrap();
        let link = projects.join("innocent.jsonl");
        std::os::unix::fs::symlink(&outside, &link).unwrap();
        let err = validate_delete_target(link.to_str().unwrap(), &projects).unwrap_err();
        assert!(err.contains("refuse delete"), "got: {err}");
        assert!(outside.exists());
        std::fs::remove_dir_all(root).ok();
    }

    #[test]
    fn delete_accepts_normal_jsonl_inside_projects() {
        let projects = temp_projects("ok");
        let proj = projects.join("some-project");
        std::fs::create_dir_all(&proj).unwrap();
        let f = proj.join("abc-123.jsonl");
        std::fs::write(&f, "{}\n").unwrap();
        let canon = validate_delete_target(f.to_str().unwrap(), &projects).unwrap();
        assert!(canon.ends_with("abc-123.jsonl"));
        // 命令壳用返回的 canonical 路径删——等价验证
        std::fs::remove_file(&canon).unwrap();
        assert!(!f.exists());
        std::fs::remove_dir_all(projects.parent().unwrap()).ok();
    }

    #[test]
    fn delete_rejects_non_jsonl_and_missing() {
        let projects = temp_projects("misc");
        // 不存在
        let missing = projects.join("nope.jsonl");
        let err = validate_delete_target(missing.to_str().unwrap(), &projects).unwrap_err();
        assert!(err.contains("does not exist"), "got: {err}");
        // 存在但非 .jsonl
        let txt = projects.join("note.txt");
        std::fs::write(&txt, "x").unwrap();
        let err2 = validate_delete_target(txt.to_str().unwrap(), &projects).unwrap_err();
        assert!(err2.contains("not a .jsonl"), "got: {err2}");
        std::fs::remove_dir_all(projects.parent().unwrap()).ok();
    }

    // === F62：create_branch_session 守卫 + 原生分支格式 ===

    /// `..` 穿越：〔`K-R88`〕**换成 sid 形状之后仍然拒**，且拒得更早（IO 之前）。
    #[test]
    fn branch_source_guard_rejects_dotdot_traversal() {
        let projects = temp_projects("branch-dotdot");
        let root = projects.parent().unwrap();
        let outside = root.join("outside.jsonl");
        std::fs::write(&outside, "{}\n").unwrap();
        for sneaky in ["../outside", "..", "../../etc/passwd"] {
            let err = branch_impl(sneaky, "u1", &projects).unwrap_err();
            assert!(err.contains("invalid session id"), "{sneaky:?} ⇒ {err}");
        }
        assert!(outside.exists());
        std::fs::remove_dir_all(root).ok();
    }

    #[test]
    fn branch_result_camel_case_contract() {
        let r = BranchResult {
            session_id: "new-sid".into(),
            jsonl_path: "/p/new-sid.jsonl".into(),
        };
        let j = serde_json::to_string(&r).unwrap();
        assert!(j.contains("\"sessionId\""), "缺 sessionId: {j}");
        assert!(j.contains("\"jsonlPath\""), "缺 jsonlPath: {j}");
    }

    #[test]
    fn write_branch_file_refuses_existing_target() {
        let dir = temp_projects("branch-write");
        // create_new：目标已存在 → Err，且既存内容零改动（自证「绝不覆盖」）
        let f = dir.join("x.jsonl");
        std::fs::write(&f, "PRE").unwrap();
        let err = write_branch_file(&f, &[serde_json::json!({"a":1})]).unwrap_err();
        assert!(err.contains("already exists"), "got: {err}");
        assert_eq!(
            std::fs::read_to_string(&f).unwrap(),
            "PRE",
            "既存文件被覆盖了"
        );
        // 正常写新文件
        let f2 = dir.join("y.jsonl");
        write_branch_file(&f2, &[serde_json::json!({"a":1})]).unwrap();
        assert_eq!(std::fs::read_to_string(&f2).unwrap(), "{\"a\":1}\n");
        std::fs::remove_dir_all(dir.parent().unwrap()).ok();
    }

    /// 软链逃逸：记录树里一条指向界外的链接，**按 sid 也找不到它**。
    ///
    /// 〔`K-R88`〕原先靠「两边 canonicalize 再比前缀」买这一样；今天靠的是
    /// 「目录项的类型判定**不跟随**链接」——同一份实现，后端那侧有条同形的
    /// `fork_write·rs::a_symlink_inside_the_tree_is_not_a_hit`。
    #[cfg(unix)]
    #[test]
    fn branch_source_guard_rejects_symlink_escape() {
        let projects = temp_projects("branch-symlink");
        let root = projects.parent().unwrap();
        let outside = root.join("secret.jsonl");
        std::fs::write(&outside, "{}\n").unwrap();
        let link = projects.join("innocent.jsonl");
        std::os::unix::fs::symlink(&outside, &link).unwrap();
        let err = branch_impl("innocent", "u1", &projects).unwrap_err();
        assert!(err.contains("not found"), "got: {err}");
        assert!(outside.exists());
        std::fs::remove_dir_all(root).ok();
    }

    /// G1：这条 IO 壳测试要的只是「一段能分叉的会话」。
    /// 纯变换的夹具已随函数搬去 `branch-core`（那里有真正区分算法的
    /// `native_shape_session`）；本地留一份**最小**的，免得为了一个 IO 测试
    /// 把测试夹具也做成跨 crate 的公开面。
    fn io_sample_session() -> Vec<serde_json::Value> {
        vec![
            serde_json::json!({"type":"user","uuid":"u1","parentUuid":null,"timestamp":"t1","sessionId":"SRC","message":{"role":"user","content":"q1"}}),
            serde_json::json!({"type":"assistant","uuid":"u2","parentUuid":"u1","timestamp":"t2","sessionId":"SRC","message":{"role":"assistant","content":"a1"}}),
            serde_json::json!({"type":"system","uuid":"u3","parentUuid":"u2","timestamp":"t3","sessionId":"SRC"}),
            serde_json::json!({"type":"user","uuid":"u4","parentUuid":"u3","timestamp":"t4","sessionId":"SRC","message":{"role":"user","content":"q2"}}),
            serde_json::json!({"type":"assistant","uuid":"u5","parentUuid":"u4","timestamp":"t5","sessionId":"SRC","message":{"role":"assistant","content":"a2"}}),
        ]
    }

    /// 重要（D 审计）：安全关键的写盘壳直测——源零改动 + 新文件原生格式正确。
    /// 注入 tempdir projects 绕开 resolve_claude_dir（同 delete 测法）。
    #[test]
    fn branch_impl_leaves_source_untouched_and_writes_native_branch() {
        let projects = temp_projects("branch-impl");
        let proj = projects.join("proj-x");
        std::fs::create_dir_all(&proj).unwrap();
        let src = proj.join("srcsid.jsonl");
        let mut body = String::new();
        for r in &io_sample_session() {
            body.push_str(&serde_json::to_string(r).unwrap());
            body.push('\n');
        }
        std::fs::write(&src, &body).unwrap();
        let before = std::fs::read(&src).unwrap();

        let res = branch_impl("srcsid", "u4", &projects).unwrap();

        // 源一字节不改
        assert_eq!(std::fs::read(&src).unwrap(), before, "源文件被改动了");
        // 新文件在源同目录、文件名=新 sid
        let out = PathBuf::from(&res.jsonl_path);
        // 两边都 canonicalize 再比,消除平台差异(Windows 上 temp_dir() 会给 8.3 短名
        // RUNNER~1，而枚举出来的那份可能是长名，否则 CI 恒红)。
        assert_eq!(
            std::fs::canonicalize(out.parent().unwrap()).unwrap(),
            std::fs::canonicalize(&proj).unwrap(),
        );
        assert_eq!(out.file_stem().unwrap().to_str().unwrap(), res.session_id);
        // 内容 = 原生分支格式（祖先链 + 新 sid + forkedFrom{srcsid@自身}）
        let out_rows: Vec<serde_json::Value> = std::fs::read_to_string(&out)
            .unwrap()
            .lines()
            .filter(|l| !l.trim().is_empty())
            .map(|l| serde_json::from_str(l).unwrap())
            .collect();
        let uuids: Vec<&str> = out_rows
            .iter()
            .map(|r| r.get("uuid").unwrap().as_str().unwrap())
            .collect();
        assert_eq!(uuids, vec!["u1", "u2", "u3", "u4"]);
        for r in &out_rows {
            assert_eq!(
                r.get("sessionId").unwrap().as_str().unwrap(),
                res.session_id
            );
            assert_eq!(
                r.get("forkedFrom").unwrap().get("sessionId").unwrap(),
                "srcsid"
            );
        }
        std::fs::remove_dir_all(projects.parent().unwrap()).ok();
    }

    // ═══════════════════════════════════════════════════════════════════
    // 🔴 `K-R88`：「按 sid 找那份会话文件」收成一份 ＋ 两侧入参形状一致
    // ═══════════════════════════════════════════════════════════════════

    /// 后端那棵树上某个文件的**生产段**（运行时读，不是 `include_str!`）。
    ///
    /// ⚠ 刻意**不用** `include_str!`：那会长出一条**编译期**的跨半边，
    /// 而 `cross_half_edge_registry` 的头注逐字讲过那条边的代价
    /// （后端在目标机上 `cargo build` 就咬住旁边这棵树了）。运行时读没有这个代价 ——
    /// 同 `K-R97` 那条 `extracting_cwd_from_a_jsonl_head_now_lives_in_exactly_one_place`。
    fn r88_backend_production(rel: &str) -> String {
        let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("仓根")
            .join(rel);
        let raw = std::fs::read_to_string(&p)
            .unwrap_or_else(|e| panic!("读不到后端的 {rel}：{e} —— 先修住址，别绕过本条"));
        guard_core::production_code(&raw)
    }

    /// ★★ `KR88D1`：**「按 sid 找那份会话文件」这件事，全仓只剩一份实现，两侧都调它。**
    ///
    /// # 它买什么
    ///
    /// 收之前两侧各有一份、而且**入参形状都不一样**（这边收路径、那边收 sid）。
    /// 那不是「重复」这么简单：**「查不到怎么办」两边可以各答各的**，
    /// 而没有任何东西会因此变红。
    ///
    /// # 🔴 它刻意**不**判什么（`KR88D1` 点名的失效方向）
    ///
    /// **不判「两边源码文本一样」** —— 那是判写法，而且很容易恒绿
    /// （两边都没有那段文本时它照样通过）。本条判的是**同一份实现**：
    /// 唯一那份的**声明只有一处**，两侧各有**恰好一处**调用，
    /// 且两条分叉路径上**一处目录枚举都不许有**（有 = 有人又自己找了一遍）。
    ///
    /// 「改那一份一处、两边行为都跟着变」那一刀是**死值验**，读数落在件文件 `§3-1`：
    /// 判据不可能替代它 —— 那一刀要真的改一次再看两边红不红。
    #[test]
    fn finding_a_session_file_by_sid_now_lives_in_exactly_one_place() {
        const CALL: &str = "branch_core::find_session_file(";

        // ① 唯一那份：声明只有一处，且住在共享 crate 里。
        let core = guard_core::production_code(include_str!("../crates/branch-core/src/lib.rs"));
        let decls = core.matches("pub fn find_session_file").count();
        assert_eq!(
            decls, 1,
            "共享 crate 里 `find_session_file` 的声明有 {decls} 处（该是 1）。\n\
             0 ⇒ 它被搬走/删了，下面两条会零命中地绿；2 ⇒ 唯一那份自己裂了。"
        );

        // ② 两侧各有**恰好一处**调用（生产段）。
        //    0 ⇒ 那一侧又自己找了一遍；2+ ⇒ 一条路上问了两遍，先说清为什么。
        let mine = guard_core::production_code(include_str!("history.rs"));
        let theirs = r88_backend_production("remote-daemon-proto/src/control/fork_write.rs");
        for (who, src) in [
            ("monitor `history.rs`", &mine),
            ("后端 `fork_write.rs`", &theirs),
        ] {
            let n = src.matches(CALL).count();
            assert_eq!(
                n, 1,
                "{who} 的生产段里 `{CALL}` 有 {n} 处（该是 1）——\n\
                 0 ⇒ 这一侧不走共享那份了（`K-R88` 收的就是这个）；\n\
                 2+ ⇒ 同一条路上问了两遍，先回答为什么。"
            );
        }

        // ③ 两条分叉路径上**一处目录枚举都没有** —— 「自己又找了一遍」的形状。
        //
        // ⚠ 人群按**那几个函数**切，不是整份 `history.rs`：这个文件别处本来就有遍历
        //（历史列表那一族），拿整份文件当分母的话本条恒红。
        // ⚠ 针**运行时拼**：本文件的测试段自己落在 `scanning_guard_registry` 的扫描面里，
        //   把那两个词写成字面量会让本条被算进「裸遍历」的人群（09-13 现打，它当场逮到了）。
        let needles = [format!("read{}dir(", "_"), format!("Walk{}", "Dir")];
        let scan = |src: &str| -> Vec<String> {
            src.lines()
                .filter(|l| needles.iter().any(|n| l.contains(n.as_str())))
                .map(|l| l.trim().to_string())
                .collect()
        };
        let local_path = format!(
            "{}\n{}",
            r88_fn_body(&mine, "pub fn create_branch_session("),
            r88_fn_body(&mine, "fn branch_impl(")
        );
        // 反向自检：尺子够得着 —— 把针塞进一份副本，量具必须数得出来。
        let poisoned = format!("{local_path}\n  let _ = std::fs::read{}dir(root);\n", "_");
        assert_eq!(
            scan(&poisoned).len(),
            1,
            "阳性对照没过 —— 量具此刻无效，下面那条断言是空真"
        );
        for (who, src) in [
            ("monitor 的分叉那条路", local_path.as_str()),
            ("后端 `fork_write.rs`", theirs.as_str()),
        ] {
            let hits = scan(src);
            assert!(
                hits.is_empty(),
                "{who}上又长出了目录枚举：\n{}\n\n\
                 ⇒ 「按 sid 找那份会话文件」的家在 `branch_core::find_session_file`。\n\
                 真有第二种找法要立，先回答「为什么这一侧不能问那一份」，再连本条一起改。",
                hits.join("\n")
            );
        }
    }

    /// 从生产段里切出一个函数（含它的签名与函数体）—— 供上面那条按函数切人群。
    ///
    /// 收尾认的是**列 0 的右大括号**（`rustfmt` 保证顶层 item 这么收）。
    /// 自检两条：切得到 · 切出来的东西有分量（塌成半截时下面的断言会空真）。
    fn r88_fn_body<'a>(src: &'a str, sig: &str) -> &'a str {
        let at = src
            .find(sig)
            .unwrap_or_else(|| panic!("切不到 `{sig}` —— 先修尺子，别改断言"));
        let rest = &src[at..];
        let end = rest.find("\n}\n").map(|i| i + 2).unwrap_or(rest.len());
        let body = &rest[..end];
        assert!(
            body.len() > 120,
            "`{sig}` 只切出 {} 字节 —— 切法坏了",
            body.len()
        );
        body
    }

    /// ★★ `KR88D2`（monitor 这一侧）：**给一个查不到的 sid，处置是报错，
    /// 不是「树上有什么就拿什么」。**
    ///
    /// 树上**真的有两份**别的会话 —— 少了这一步，本条在空树上也绿，
    /// 而「静默取第一个」正是它要逮的那一形。
    /// 后端那侧的同形判据是
    /// `fork_write·rs::an_unknown_session_id_is_refused_not_silently_substituted`，
    /// 两条读的是同一份实现 ⇒ 那份一改，两条一起动。
    #[test]
    fn an_unknown_session_id_is_refused_not_silently_substituted() {
        let projects = temp_projects("branch-unknown");
        let proj = projects.join("proj-x");
        std::fs::create_dir_all(&proj).unwrap();
        let mut body = String::new();
        for r in &io_sample_session() {
            body.push_str(&serde_json::to_string(r).unwrap());
            body.push('\n');
        }
        for sid in ["aaa", "bbb"] {
            std::fs::write(proj.join(format!("{sid}.jsonl")), &body).unwrap();
        }
        // 反向自检：树上真有东西可被「随手挑」。
        assert!(
            branch_impl("aaa", "u4", &projects).is_ok(),
            "夹具没造出可被挑中的会话"
        );

        let err = branch_impl("ccc", "u4", &projects).unwrap_err();
        assert!(
            err.contains("not found") && err.contains("ccc"),
            "查不到的 sid 应当报错并点名，实得：{err}"
        );
        std::fs::remove_dir_all(projects.parent().unwrap()).ok();
    }

    /// ★ `KR88D2`：**两条命令的入参形状一致 —— 都收 sid，都不收路径。**
    ///
    /// 判的是两个 `#[tauri::command]` 的签名本身（生产段现读）：
    /// 本机那条一旦退回收路径，本条当场红。
    #[test]
    fn both_branch_commands_take_a_session_id_not_a_path() {
        let local = guard_core::production_code(include_str!("history.rs"));
        let remote = guard_core::production_code(include_str!("remote_branch.rs"));
        for (who, src, sig) in [
            ("本机", &local, "pub fn create_branch_session("),
            (
                "远端",
                &remote,
                "pub async fn create_remote_branch_session(",
            ),
        ] {
            let at = src
                .find(sig)
                .unwrap_or_else(|| panic!("{who}那条命令的签名找不到（`{sig}`）—— 先修尺子"));
            let close = src[at..].find(')').expect("签名没有收尾括号");
            let params = &src[at + sig.len()..at + close];
            assert!(
                params.contains("session_id: String"),
                "{who}那条命令的入参里没有 sid：{params:?}"
            );
            assert!(
                !params.contains("path"),
                "{who}那条命令又收路径了：{params:?}\n\
                 ⇒ `K-R88` 收的就是「同一件事两个入参形状」，\n\
                 而多一个可被构造的路径入参就多一条路径穿越面。"
            );
        }
    }

    // ─────────────────── `KR92D1`：线上那一格分得开「不知道」和「真的是 0」 ───────────────────

    /// 造一个线上项目行，三格全是**「查过了，真的是 0」**；要哪一格变成「不知道」，
    /// 调用方用 `..` 语法覆盖那一格（这样「只动了一格」在源码上一眼可见）。
    fn wire_project_all_known_zero() -> HistoryProject {
        HistoryProject {
            project_path: "/x/y".into(),
            project_name: "y".into(),
            project_dir: "y-enc".into(),
            session_count: 3,
            starred_count: Some(0),
            hidden_count: Some(0),
            last_activity: 1,
            has_live: Some(false),
            origin: Some("pi".into()),
        }
    }

    /// ★★ `KR92D1`：**过线之后，下游分得出这三个数是「算过的」还是「不知道」。**
    ///
    /// # 判的是性质，不是形状
    ///
    /// 本条**逐字不判**那三格长什么样（`null` / tagged union / 并列一个 `*_known` 布尔都行）——
    /// 它判的是**两份只在「不知道 vs 真的是 0」上不同的行，过线之后字节不同**。
    /// ⇒ 换一种等价表示（第 ② 刀）本条照常绿；把 `Unknown` 压成 `0`/`false`（第 ① 刀）当场红。
    ///
    /// 🔴 **三格逐格分开断**（第 ③ 刀：只修 star/hide 不修 `has_live` ⇒ 必须红）：
    /// 一次只把一格换成「不知道」，三次都要与「真的是 0」那一份可分。
    /// 合起来断一次是接不住的 —— 只要有一格治了，整行就已经不同。
    #[test]
    fn the_three_counts_can_say_i_do_not_know() {
        let wire = |p: &HistoryProject| serde_json::to_string(p).expect("序列化");
        let all_zero = wire(&wire_project_all_known_zero());

        for (格, unknown_row) in [
            (
                "starred_count",
                HistoryProject {
                    starred_count: None,
                    ..wire_project_all_known_zero()
                },
            ),
            (
                "hidden_count",
                HistoryProject {
                    hidden_count: None,
                    ..wire_project_all_known_zero()
                },
            ),
            (
                "has_live",
                HistoryProject {
                    has_live: None,
                    ..wire_project_all_known_zero()
                },
            ),
        ] {
            assert_ne!(
                wire(&unknown_row),
                all_zero,
                "🔴 `{格}` 这一格：「不知道」与「查过了，真的是 0」过线之后**长得一模一样**。\n\
                 那正是 `K-R92` 的题面 —— 后端已经分得开，线上这一格又把它压回去了。\n\
                 ⚠ 三格是一族：只治 star/hide 不治 `has_live`，本条在 `has_live` 那一轮红。\n\
                 现打这一行：{}",
                wire(&unknown_row)
            );
        }

        // 对照组：**真的是 0** 与 **真的是 0** 恒同 —— 上面那三条不是靠「随便变点什么」绿的。
        assert_eq!(
            all_zero,
            wire(&wire_project_all_known_zero()),
            "★ 对照组：同一份输入序列化两次应当逐字节相同"
        );
    }

    /// ★★ `KR92D1` 的排序侧：**「不知道」自成一档**，既不冒充「有」，也不被当成「没有」。
    ///
    /// 失效方向（本条存在的理由）：`Option` 的派生序是 `None < Some(false) < Some(true)`，
    /// 谁哪天把 [`live_rank`] 换回 `b.has_live.cmp(&a.has_live)`，「不知道」就被排到
    /// 「确定没活」后面 —— 那是一句没人查过的断言。
    #[test]
    fn unknown_is_its_own_bucket_when_sorting() {
        assert!(
            live_rank(Some(true)) > live_rank(None) && live_rank(None) > live_rank(Some(false)),
            "★ 活：确定有 > 不知道 > 确定没有（现打 {} / {} / {}）",
            live_rank(Some(true)),
            live_rank(None),
            live_rank(Some(false))
        );
        assert!(
            star_rank(Some(2)) > star_rank(None) && star_rank(None) > star_rank(Some(0)),
            "★ 星标：有 > 不知道 > 查过了一个都没有（现打 {} / {} / {}）",
            star_rank(Some(2)),
            star_rank(None),
            star_rank(Some(0))
        );
        assert_ne!(
            live_rank(None),
            live_rank(Some(false)),
            "🔴 把「不知道」和「确定没有活会话」排进同一档 = 排序这一端仍然分不开"
        );
        assert_ne!(star_rank(None), star_rank(Some(0)), "🔴 同上，星标那一格");
    }

    /// P1.2 contract test：守护后端 wire 跟前端 TS interface 字段名一致。
    /// 改字段名必须同步改前端 views/history.ts 的 HistoryProject / HistorySessionEntry interface。
    /// 若本测试失败 = 后端 wire 漂移；若 tsc 编译错 = 前端 access 漂移。两边都受保护。
    #[test]
    fn history_project_camel_case_contract() {
        let p = HistoryProject {
            project_path: "/x/y".into(),
            project_name: "y".into(),
            project_dir: "/y-encoded".into(),
            session_count: 1,
            starred_count: Some(2),
            hidden_count: Some(3),
            last_activity: 1700_000_000_000,
            has_live: Some(true),
            origin: Some("pi-host".into()), // issue #16：远端来源也走同一 wire 契约
        };
        let j = serde_json::to_string(&p).unwrap();
        for camel_key in [
            "\"projectPath\"",
            "\"projectName\"",
            "\"projectDir\"",
            "\"sessionCount\"",
            "\"starredCount\"",
            "\"hiddenCount\"",
            "\"lastActivity\"",
            "\"hasLive\"",
        ] {
            assert!(
                j.contains(camel_key),
                "HistoryProject wire 缺 {camel_key}: {j}"
            );
        }
        // 反例守护：不应出现任何 snake_case 字段
        for snake_key in [
            "\"project_path\"",
            "\"project_name\"",
            "\"project_dir\"",
            "\"session_count\"",
            "\"starred_count\"",
            "\"hidden_count\"",
            "\"last_activity\"",
            "\"has_live\"",
        ] {
            assert!(
                !j.contains(snake_key),
                "HistoryProject 漏改 {snake_key}: {j}"
            );
        }
    }

    #[test]
    fn history_session_entry_camel_case_contract() {
        let e = HistorySessionEntry {
            session_id: "s-1".into(),
            project_path: "/x".into(),
            project_name: "x".into(),
            ai_title: Some("t".into()),
            first_user_excerpt: "hi".into(),
            started_at: 1,
            updated_at: 2,
            jsonl_path: "/a.jsonl".into(),
            is_live: Some(true),
            message_count_approx: 5,
            is_bg: true,
            starred: false,
            custom_title: None,
            hidden: false,
            forked_from_session_id: Some("p-1".into()),
            forked_from_message_uuid: Some("u-1".into()),
            origin: Some("pi-host".into()),
        };
        let j = serde_json::to_string(&e).unwrap();
        for camel_key in [
            "\"sessionId\"",
            "\"isBg\"",
            "\"projectPath\"",
            "\"projectName\"",
            "\"aiTitle\"",
            "\"firstUserExcerpt\"",
            "\"startedAt\"",
            "\"updatedAt\"",
            "\"jsonlPath\"",
            "\"isLive\"",
            "\"messageCountApprox\"",
            "\"customTitle\"",
            "\"forkedFromSessionId\"",
            "\"forkedFromMessageUuid\"",
        ] {
            assert!(
                j.contains(camel_key),
                "HistorySessionEntry wire 缺 {camel_key}: {j}"
            );
        }
        for snake_key in [
            "\"session_id\"",
            "\"project_path\"",
            "\"first_user_excerpt\"",
            "\"is_live\"",
            "\"forked_from_session_id\"",
        ] {
            assert!(
                !j.contains(snake_key),
                "HistorySessionEntry 漏改 {snake_key}: {j}"
            );
        }
    }

    /// A4：EntryMetadata / MetadataPatch 的 lastAccount serde 契约 + 向后兼容 + 三态 patch。
    #[test]
    fn last_account_serde_and_patch_semantics() {
        // 1) 向后兼容：旧文件无 lastAccount 字段 → None，不报错。
        let old: EntryMetadata =
            serde_json::from_str(r#"{"starred":true,"hidden":false,"updatedAt":9}"#).unwrap();
        assert_eq!(old.last_account, None);

        // 2) camelCase wire：Some(name) 序列化含 "lastAccount"、不含 snake。
        let e = EntryMetadata {
            last_account: Some("z".into()),
            ..Default::default()
        };
        let j = serde_json::to_string(&e).unwrap();
        assert!(j.contains("\"lastAccount\""), "wire 缺 lastAccount: {j}");
        assert!(!j.contains("last_account"), "wire 不该含 snake: {j}");

        // 2b) 旧 snake alias 仍可读入（迁移容错）。
        let via_alias: EntryMetadata = serde_json::from_str(r#"{"last_account":"b"}"#).unwrap();
        assert_eq!(via_alias.last_account, Some("b".into()));

        // 3) MetadataPatch：缺键 / null 都折叠为 None(不改)——与既有 customTitle 同(plain
        //    serde default，非 double_option)；清空经"空串 → filter"实现(见 4))，不靠 null。
        let none: MetadataPatch = serde_json::from_str("{}").unwrap();
        assert_eq!(none.last_account, None);
        let via_null: MetadataPatch = serde_json::from_str(r#"{"lastAccount":null}"#).unwrap();
        assert_eq!(via_null.last_account, None);
        let set: MetadataPatch = serde_json::from_str(r#"{"lastAccount":"z"}"#).unwrap();
        assert_eq!(set.last_account, Some(Some("z".into())));

        // 4) apply 语义（镜像 update_history_metadata 分支）：空白账号名按清空处理。
        fn apply(mut e: EntryMetadata, json: &str) -> EntryMetadata {
            let p: MetadataPatch = serde_json::from_str(json).unwrap();
            if let Some(a) = p.last_account {
                e.last_account = a.filter(|s| !s.trim().is_empty());
            }
            e
        }
        let base = EntryMetadata {
            last_account: Some("z".into()),
            ..Default::default()
        };
        assert_eq!(
            apply(EntryMetadata::default(), r#"{"lastAccount":"z"}"#).last_account,
            Some("z".into())
        );
        assert_eq!(
            apply(base.clone(), r#"{"lastAccount":""}"#).last_account,
            None
        ); // 空串=清空
        assert_eq!(
            apply(base.clone(), r#"{"lastAccount":null}"#).last_account,
            Some("z".into()) // null 折叠为"不改"（同 customTitle）
        );
        assert_eq!(
            apply(base.clone(), r#"{"starred":true}"#).last_account,
            Some("z".into()) // 未提 lastAccount → 不改
        );
        assert_eq!(
            apply(EntryMetadata::default(), r#"{"lastAccount":"   "}"#).last_account,
            None // 纯空白 = 清空
        );
    }

    /// A4：list_last_accounts 的纯变换——只含有 lastAccount 的条目，None 的剔除。
    #[test]
    fn last_accounts_of_filters_none() {
        let mut entries = HashMap::new();
        entries.insert(
            "s-has".to_string(),
            EntryMetadata {
                last_account: Some("z".into()),
                ..Default::default()
            },
        );
        entries.insert("s-none".to_string(), EntryMetadata::default()); // 无 lastAccount
        let out = last_accounts_of(HistoryMetadata {
            version: 1,
            entries,
        });
        assert_eq!(out.get("s-has"), Some(&"z".to_string()));
        assert!(!out.contains_key("s-none"));
        assert_eq!(out.len(), 1);
    }

    // P3 归并：iso_parse_* 测试已搬到 utils::tests（函数本身搬到 utils）。

    #[test]
    fn truncate_chars_unicode() {
        let s = truncate_chars("你好世界abc", 3);
        assert_eq!(s, "你好世…");
    }

    #[test]
    fn truncate_chars_short() {
        let s = truncate_chars("hi", 10);
        assert_eq!(s, "hi");
    }

    #[test]
    fn truncate_chars_newline_replaced() {
        let s = truncate_chars("a\nb\nc", 10);
        assert_eq!(s, "a b c");
    }

    #[test]
    fn resume_cmd_prefers_cc_with_claude_fallback() {
        let sid = "01998f2a-1234-7abc-9def-0123456789ab";
        let cmd = build_resume_ps_command(sid, None).unwrap();
        // 优先 cc、回退 claude，两者都带正确 sid
        assert!(cmd.contains("Get-Command cc"));
        assert!(cmd.contains(&format!("cc --resume {sid}")));
        assert!(cmd.contains(&format!("claude --resume {sid}")));
    }

    #[test]
    fn resume_cmd_rejects_injection() {
        // 含 shell 元字符的 session_id 必须被拒（防命令注入）
        for bad in [
            "a; rm -rf /",
            "a && calc",
            "a`whoami`",
            "a$(id)",
            "a b",
            "a\"b",
            "",
            "a/../b",
        ] {
            assert!(
                build_resume_ps_command(bad, None).is_err(),
                "应拒绝危险 session_id: {bad:?}"
            );
        }
    }

    /// F34：自定义 launcher——合法形态放行、注入面拒绝、空视为未设置。
    #[test]
    fn sanitize_launcher_allows_simple_reject_injection() {
        assert_eq!(sanitize_launcher(None).unwrap(), None);
        assert_eq!(sanitize_launcher(Some("")).unwrap(), None);
        assert_eq!(sanitize_launcher(Some("   ")).unwrap(), None);
        assert_eq!(
            sanitize_launcher(Some("cct")).unwrap().as_deref(),
            Some("cct")
        );
        assert_eq!(
            sanitize_launcher(Some(" cc -p 8 ")).unwrap().as_deref(),
            Some("cc -p 8")
        );
        for bad in ["cc; calc", "cc|id", "cc$(id)", "cc`id`", "cc&&x", "cc\"x"] {
            assert!(sanitize_launcher(Some(bad)).is_err(), "应拒绝: {bad:?}");
        }
    }

    #[test]
    fn resume_cmd_custom_launcher_used_verbatim() {
        let sid = "abc-123";
        let cmd = build_resume_ps_command(sid, Some("cct")).unwrap();
        assert_eq!(cmd, "cct --resume abc-123");
        // 设了自定义命令就不再出现 cc 自动检测
        assert!(!cmd.contains("Get-Command"));
    }

    /// F96：本地起新会话命令——同 cc 优先/回退逻辑，但**不带 resume flag / sid**。
    #[test]
    fn new_session_cmd_prefers_cc_no_resume_flag() {
        let cmd = build_new_session_ps_command(None).unwrap();
        assert!(cmd.contains("Get-Command cc"));
        assert!(cmd.contains("{ cc }"), "cc 分支: {cmd}");
        assert!(cmd.contains("{ claude }"), "回退分支: {cmd}");
        // 起新会话不是 resume：绝不带 --resume / sid
        assert!(
            !cmd.contains("--resume"),
            "起新会话不应带 resume flag: {cmd}"
        );
    }

    #[test]
    fn new_session_cmd_custom_launcher_verbatim() {
        let cmd = build_new_session_ps_command(Some("cct")).unwrap();
        assert_eq!(cmd, "cct");
        assert!(!cmd.contains("Get-Command"));
        assert!(!cmd.contains("--resume"));
    }

    #[test]
    fn new_session_cmd_rejects_injection_launcher() {
        for bad in ["cc; calc", "cc|id", "cc$(id)", "cc`id`", "cc&&x"] {
            assert!(
                build_new_session_ps_command(Some(bad)).is_err(),
                "应拒绝注入 launcher: {bad:?}"
            );
        }
    }

    /// ★ L1：POSIX 渲染器的形状 —— 与 PowerShell 那条是**同一个决策**的另一种写法。
    ///
    /// `Get-Command` 的等价物是 `command -v`（它同样找得到 shell **函数**，
    /// 而 `ccm` 的 `cc` 集成正是一个函数；命令跑在 `bash -lic` 里、rc 已加载）。
    #[test]
    fn posix_renderer_mirrors_the_powershell_one() {
        let sid = "01998f2a-1234-7abc-9def-0123456789ab";
        assert_eq!(
            build_local_posix_command(&LocalPsAction::Resume(sid.to_string()), None, None).unwrap(),
            format!(
                "if command -v cc >/dev/null 2>&1; then cc --resume {sid}; \
                 else claude --resume {sid}; fi"
            )
        );
        assert_eq!(
            build_local_posix_command(&LocalPsAction::New, None, None).unwrap(),
            "if command -v cc >/dev/null 2>&1; then cc; else claude; fi"
        );
        // F34 自定义命令：两边都不做别名探测，**逐字节相同**（这一支没有平台差异）。
        for action in [LocalPsAction::New, LocalPsAction::Resume(sid.to_string())] {
            assert_eq!(
                build_local_posix_command(&action, Some("cct"), None).unwrap(),
                build_local_ps_command(&action, Some("cct"), None).unwrap(),
                "显式指定命令时两个渲染器不该有任何差异"
            );
        }
    }

    /// ★★ **P3t-Y0：Windows 那条路本件一个字不动 —— 而钉的是「该活下来的性质」，不是字节。**
    ///
    /// `C12` 逐字「windows不要tmux」。用内容哈希钉「一个字没改」看着更严，其实更坏：
    /// 将来任何一次正当的 Windows 改动都会让它假红，而假红久了就会被人加豁免 ——
    /// 那时它连性质都不守了。⇒ 钉性质：**Windows 本机的渲染器自己永远不产会话容器**。
    ///
    /// ⚠ `launcher` 必须传 `None`。用户显式指定 `cct`（F34）时输出里当然会有 `cct`，
    /// 那是**用户自己要的**，不是本工具替他加的 —— `posix_renderer_mirrors_the_powershell_one`
    /// 正是拿 `Some("cct")` 在对拍。人群划错这一格，本条会变成「禁止用户用 cct」。
    ///
    /// # 射程（`reach`）
    ///
    /// 够得到：Windows 渲染器的**输出里没有容器**。
    /// **够不到**：Windows 那条路今天还跑不跑得起来 —— 没有 Windows 机器，
    /// 「没改」证明不了「还能跑」。那一格归 `auto-e2e`（本件 §4 已登记）。
    #[test]
    fn the_windows_local_path_never_grows_a_session_container() {
        let sid = "01998f2a-1234-7abc-9def-0123456789ab";
        let named = LaunchAccount::Named {
            config_dir: "C:\\Users\\z\\.claude-accts\\z".into(),
            name: None,
        };
        let accounts: [Option<&LaunchAccount>; 3] =
            [None, Some(&LaunchAccount::Base), Some(&named)];
        let mut checked = 0usize;
        for action in [LocalPsAction::New, LocalPsAction::Resume(sid.to_string())] {
            for acct in accounts {
                let cmd =
                    build_local_ps_command(&action, None, acct).expect("这几组形状都该渲染得出来");
                checked += 1;
                assert!(
                    !cmd.contains("--tmux"),
                    "Windows 渲染器吐了 `--tmux` —— `C12` 逐字「windows不要tmux」。实得：{cmd}"
                );
                assert!(
                    !cmd.split_whitespace().any(|w| w == "cct"),
                    "Windows 渲染器自己挑了带 tmux 的别名 `cct`（用户没指定）。实得：{cmd}"
                );
            }
        }
        // 完备性自检：人群空掉时「全过」与「没测」长得一模一样。
        assert_eq!(checked, 6, "只渲了 {checked} 组，人群跑偏了");

        // ★ 结构半：`launch_local` 的 Windows 那支**不许调渲染器**。
        // 光有上面的行为半不够 —— 渲染器可以在 `launch_local` 里被调、把容器加在
        // `build_local_ps_command` **之外**，那样上面六组照样全绿。
        let prod = guard_core::production_code(include_str!("history.rs"));
        // ⚠ 锚点从裸 `#[cfg(windows)]` 扩到「它 + 它门着的那一行」〔`D6` 回修，08-29〕：
        //   本轮 `PRODUCTION_LAUNCH_SINK` 也按平台分了两支 ⇒ 裸锚点从 1 处变成 2 处，
        //   本条当场红（报文逐字「断言指不明是哪一处」）。**它逮到的是真的**：
        //   锚点不唯一时下面切出来的臂可能是别人的。⇒ 按 `F19` 那条纪律
        //   「把 needle 扩到能唯一确定那个事实的大小」，而**不是**把断言放宽。
        let at = guard_core::find_pinned(&prod, "#[cfg(windows)]\n    let base = {")
            .unwrap_or_else(|e| {
                panic!("`launch_local` 的 Windows 臂锚点不是恰好一处，先修锚点：{e}")
            });
        let arm = {
            let b = prod.as_bytes();
            let open = (at..b.len()).find(|&i| b[i] == b'{').expect("找不到块起点");
            let (mut depth, mut end) = (0i32, b.len());
            for i in open..b.len() {
                if b[i] == b'{' {
                    depth += 1;
                } else if b[i] == b'}' {
                    depth -= 1;
                    if depth == 0 {
                        end = i + 1;
                        break;
                    }
                }
            }
            &prod[open..end]
        };
        assert!(
            arm.len() > 60 && arm.len() < 1500,
            "切出来的 Windows 臂只有 {} 字节 —— 配平切错了，本条会零命中地绿",
            arm.len()
        );
        assert!(
            !arm.contains("render_local_ccm"),
            "`launch_local` 的 Windows 臂调了 CLI 渲染器 —— 那条路会带 `--tmux`。实得：{arm}"
        );
        assert!(
            arm.contains("build_local_ps_command"),
            "`launch_local` 的 Windows 臂不再走 `build_local_ps_command` —— 换路了。实得：{arm}"
        );
    }

    /// ★★ **P3t-Y5 的出口**：把生产渲染器的**真输出**吐给 e2e。
    ///
    /// e2e 要证「这条命令在真 tmux 上干了什么」。若脚本里手抄一份命令串，证的就是手抄那份 ——
    /// 渲染器改了、脚本没改，实测照样绿。⇒ 串必须从**这里**出去。
    ///
    /// `#[ignore]` 是因为它不是判据（不断言任何事），只是个数据出口；
    /// 跑法：`cargo test --lib emit_local_launch_command_for_e2e -- --ignored --nocapture`。
    #[test]
    #[ignore]
    #[cfg(not(windows))]
    fn emit_local_launch_command_for_e2e() {
        let sid = std::env::var("P3T_E2E_SID").unwrap_or_else(|_| "s1abcdef".into());
        let name = std::env::var("P3T_E2E_TMUX").unwrap_or_else(|_| "s1abcdef-cc".into());
        // ★★ launcher 由 e2e 指定，而且必须是个**独一无二的名字**（实测逼出来的）。
        //
        // 第一版让 e2e 拿 PATH shim 顶掉 `claude`。**那在生产送法下不成立**：
        // `launch_local_posix` 用的是 `bash -lic`，**登录 shell 会重跑 profile 并把
        // `~/.local/bin` 重排到 PATH 最前** ⇒ shim 被顶掉、解析到的是用户**真实的 claude**
        // （C7d 逐字禁的那件事，实测真起了两次）。
        // 用一个只在隔离目录里存在的名字，PATH 谁在前都盖不住它 —— 这是结构保证，不是纪律。
        let launcher = std::env::var("P3T_E2E_LAUNCHER").ok();
        let cmd = render_local_ccm_with(
            &LocalPsAction::Resume(sid),
            launcher.as_deref(),
            Some(&LaunchAccount::Base),
            Some(&name),
            &caps_of_a_current_ccm(),
            true,
        )
        .expect("渲染不出来 —— e2e 无对象可跑");
        println!("P3T_CMD<<<{cmd}>>>");
    }

    /// ★ L1：**sid 校验与注入防线在 POSIX 那条路上同样生效**。
    ///
    /// 主计划点名这道校验「要保留——那是一道独立防线，不是重复」。
    /// L1 把它抽进了共享决策 `local_launch_choice`，本测试钉住抽完之后两条路都还有。
    #[test]
    fn posix_renderer_keeps_sid_and_launcher_defenses() {
        for bad in ["", "../etc", "a b", "x;id", "sid$(id)"] {
            assert!(
                build_local_posix_command(&LocalPsAction::Resume(bad.to_string()), None, None)
                    .is_err(),
                "POSIX 渲染器应拒绝非法 sid: {bad:?}"
            );
        }
        for bad in ["cc; calc", "cc|id", "cc$(id)", "cc`id`", "cc&&x"] {
            assert!(
                build_local_posix_command(&LocalPsAction::New, Some(bad), None).is_err(),
                "POSIX 渲染器应拒绝注入 launcher: {bad:?}"
            );
        }
    }

    /// F06：`build_resume_ps_command`/`build_new_session_ps_command` 收拢成
    /// `build_local_ps_command` 后必须逐字节保持——把重构前两个函数曾经产出的具体字符串
    /// 内联成期望值（而非依赖上面 6 条测试的"包含子串"断言，那些不足以证明完全同构）。
    #[test]
    fn unified_builder_byte_identical_to_pre_f06_resume_output() {
        let sid = "01998f2a-1234-7abc-9def-0123456789ab";
        let expected = "if (Get-Command cc -ErrorAction SilentlyContinue) \
             { cc --resume 01998f2a-1234-7abc-9def-0123456789ab } \
             else { claude --resume 01998f2a-1234-7abc-9def-0123456789ab }";
        assert_eq!(build_resume_ps_command(sid, None).unwrap(), expected);
        assert_eq!(
            build_local_ps_command(&LocalPsAction::Resume(sid.to_string()), None, None).unwrap(),
            expected
        );
    }

    #[test]
    fn unified_builder_byte_identical_to_pre_f06_new_session_output() {
        let expected = "if (Get-Command cc -ErrorAction SilentlyContinue) { cc } else { claude }";
        assert_eq!(build_new_session_ps_command(None).unwrap(), expected);
        assert_eq!(
            build_local_ps_command(&LocalPsAction::New, None, None).unwrap(),
            expected
        );
    }

    // ═════════════════════════════════════════════════════════════════════
    // `K-H2b`：接上注入点 —— 注入侧那三个判断
    // ═════════════════════════════════════════════════════════════════════

    /// ★★★ `KH2B5`（`§0e` 裁一）**在这一层的对照** —— 同一条起会话路径、同一个函数，
    /// 只有「这个号在不在中转表里」不同：
    /// api-key 号（表里有行）的命令**带**那个 env，官方号的命令里**一个字节都没有**。
    #[test]
    fn only_an_account_that_has_a_row_in_the_relay_table_gets_the_base_url_prefix() {
        let rows = vec!["acct-a".to_string()];
        let named = |d: &str| LaunchAccount::Named {
            config_dir: d.to_string(),
            name: None,
        };
        // ① 表里有行 ⇒ 前缀在（非空对照：证明这把尺子不是恒空串）。
        let id = relay_account_id(Some(&named("/home/u/.claude-accts/acct-a")));
        assert_eq!(id.as_deref(), Some("acct-a"), "账号 id 是从末段目录名推的");
        let p = relay_prefix_for(id.as_deref(), &rows, true, Some("sid-1"), false).unwrap();
        assert_eq!(
            p, "export ANTHROPIC_BASE_URL='http://127.0.0.1:8788/s/claude-code/acct-a/sid-1'; ",
            "api-key 号的命令没带上中转 base URL —— 那条线还是没接"
        );
        // ② 表里没有这一行（订阅号）⇒ **空串**，命令逐字节与本件之前相同。
        let other = relay_account_id(Some(&named("/home/u/.claude-accts/acct-b")));
        assert_eq!(
            relay_prefix_for(other.as_deref(), &rows, true, Some("sid-1"), false).unwrap(),
            "",
            "没配第三方 key 的号被接进了中转 —— `§0e` 裁一逐字禁这一形"
        );
        // ③ 账号 0 / 没表态 ⇒ 说不出 id ⇒ 空串。
        assert_eq!(relay_account_id(Some(&LaunchAccount::Base)), None);
        assert_eq!(relay_account_id(None), None);
        assert_eq!(
            relay_prefix_for(None, &rows, true, None, false).unwrap(),
            ""
        );
        // ④ Windows 那一侧渲的是 PowerShell 形态（**只到「编得过」**，运行时没量过）。
        let ps = relay_prefix_for(id.as_deref(), &rows, true, Some("sid-1"), true).unwrap();
        assert_eq!(
            ps,
            "$env:ANTHROPIC_BASE_URL='http://127.0.0.1:8788/s/claude-code/acct-a/sid-1'; "
        );
    }

    // ★★★ `D5 阻-1`：**那条扫描型判据（`the_two_inputs_at_the_call_site_are_still_the_two_take_points`）
    //    整条删了**，换成下面**三条**判据（两条行为 + 一条按函数地址对拍）。
    //    删它的理由是一个实测读数，不是风格：
    //
    // 它先前住 `local_daemon.rs`（`D4` 搬过去的，为的是「判据与被扫的代码不同文件」），
    // 而它量的仍然是「`relay_prefix_for_launch` 的体切出 700 字节，窗口里**有没有**那两段文本」。
    // `D5` 现打：在同一个窗口里加一行把那两段文本原样留住的死赋值，同时把真入参换成空表 / 常量
    // ⇒ 文本一处不少、**全量门禁四个数与干净树逐字相同**，而中转注入在生产上被整个摘掉。
    // ⇒ 按铁律 13「删之前先证明它恒绿」——`D5` 那一刀就是那份证明。
    //
    // ★ 本件病史五层，每层都是**上一层的修法买到的东西被下一层的量法漏掉**：
    //   ① 参数位没有账号 → ② 值恒空 → ③ 只量文本 → ④ 判据搬了家、仍只量文本 → ⑤ 文本留住、行为摘掉。
    //   ⇒ **第六层的出路不是更聪明的文本判据，是不量文本。**见 `RelayFactSources` 头注。

    /// ★★★ `D5 阻-1` + `D6 阻-2` + `D6 阻-3`：**那次拉起真的问了那三件事，而且真的用了答案。**
    ///
    /// # 它怎么挡住第五层那一刀
    ///
    /// 替身把「被问了几次」记下来，但**光有计数不够** —— 第五层那一刀（问完把答案扔掉）
    /// 会让计数照涨。⇒ 承重的是第二半：**算出来的前缀必须与「拿替身那几个答案直接喂纯函数」
    /// 逐字节相同**，并且几种答案组合各自落到不同的脸上（非空 / 空 / `Err` / PowerShell 形态）。
    /// 把任何一个入参换成常量，这几格里至少一格当场不同。
    ///
    /// # 🔴🔴 `D6 阻-2`：**「哪个号」也是一维，而它先前的输入域是 1**
    ///
    /// 第一版只喂**一个**账号（`acct-a`）⇒ `D6` 的刀 `E6` 把 [`relay_account_id`] 的答案
    /// `.map(|_| "acct-a")` 写死（那段文本一字不动）⇒ **全绿、门禁四个数与干净树逐字相同**。
    /// 生产后果是**路由键的 `<account>` 段恒是一个号** ⇒ 中转按它取 key ⇒
    /// **acct-b 的会话拿着 acct-a 的那把 key 发请求，两边都显示成功** ——
    /// 正是整个多账号工作要防的最坏那一形。
    /// ⇒ 本条**至少喂两个不同的号**，并断言前缀里的 `<account>` 段跟着变。
    ///
    /// # 🔴 `D6 阻-3`：**平台开关也收进了这条缝**
    ///
    /// `cfg!(windows)` 写在调用点上时是个常量表达式，判据翻不动它 ——
    /// `D6` 的刀 `Xb`（把它写死成 `false`）全绿，而生产后果是 Windows 上渲成 POSIX 形态。
    /// 收进 [`RelayFactSources`] 之后，本条第 ④ 格喂 `|| true` 就该拿到 PowerShell 形态。
    /// ⚠ **它守的是调用点那一格**；[`platform_is_windows`] 自己的体在 Linux 上判不了
    ///（登记在 [`RelayFactSources`] 的**结构头注** ㈡ 那一栏里 —— 不在 [`platform_is_windows`]
    /// 自己的头注里，`D7` 逐字订正过这一处指偏）。
    ///
    /// # 🔴🔴 `D7 阻-4`：**「哪一次拉起」（`action` / `sid`）也是一维，先前它的取值域是 1**
    ///
    /// 第 ①–④ 格把 `rows` / `running` / `windows` / `account` 四维都打开了，
    /// **而 `action` 从头到尾只喂过 `Resume("sid-1")`**。`D7` 两刀实打：
    /// - 刀 `S2`（`LocalPsAction::New => return Ok(String::new())`）⇒ **`1229` 全绿**，
    ///   生产后果是**新开会话那条路上中转注入恒空**，而 `KH2B6` 逐字写着那条路今天生产可达；
    /// - 刀 `S1`（`Resume(_sid) => Some("sid-1")`）⇒ 也全绿，那一维什么都没买。
    /// ⇒ 第 ⑤ 格喂**两个不同的 sid**、第 ⑥ 格喂 **`New`**，两支都断。
    ///
    /// # 它买不到什么
    ///
    /// 它不管那几个取值口**自己答得对不对**（那是 [`relay_rows_at`] 那条读真文件的判据、
    /// 与 `local_daemon::relay_running_really_reads_the_handle_table` 的活），
    /// 也不管**生产上插进那条缝的是不是它们**（那是下一条判据按函数地址对拍的活）。
    /// **三条合起来才等于「这条线真的在问那几件事」。**
    #[test]
    fn the_launch_side_really_asks_those_two_take_points_and_uses_their_answers() {
        use std::cell::{Cell, RefCell};
        thread_local! {
            static ROWS_CALLS: Cell<u32> = const { Cell::new(0) };
            static RUNNING_CALLS: Cell<u32> = const { Cell::new(0) };
            static ROWS_ANSWER: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
            static RUNNING_ANSWER: Cell<bool> = const { Cell::new(false) };
            static WINDOWS_ANSWER: Cell<bool> = const { Cell::new(false) };
        }
        fn spy_rows() -> Vec<String> {
            ROWS_CALLS.with(|c| c.set(c.get() + 1));
            ROWS_ANSWER.with(|v| v.borrow().clone())
        }
        fn spy_running() -> bool {
            RUNNING_CALLS.with(|c| c.set(c.get() + 1));
            RUNNING_ANSWER.with(Cell::get)
        }
        fn spy_windows() -> bool {
            WINDOWS_ANSWER.with(Cell::get)
        }
        fn answer(rows: &[&str], running: bool, windows: bool) {
            ROWS_ANSWER.with(|v| *v.borrow_mut() = rows.iter().map(|s| s.to_string()).collect());
            RUNNING_ANSWER.with(|c| c.set(running));
            WINDOWS_ANSWER.with(|c| c.set(windows));
        }

        let acct_a = LaunchAccount::Named {
            config_dir: "/h/.claude-accts/acct-a".to_string(),
            name: None,
        };
        // 🔴 `D6 阻-2`：**第二个号**。「哪个号」这一维的输入域从 1 变成 2。
        let acct_b = LaunchAccount::Named {
            config_dir: "/h/.claude-accts/acct-b".to_string(),
            name: None,
        };
        let action = LocalPsAction::Resume("sid-1".to_string());
        let _guard = override_relay_facts(RelayFactSources {
            rows: spy_rows,
            running: spy_running,
            windows: spy_windows,
        });

        // ① 表里有这一行 + 中转在跑 ⇒ 前缀 = 纯函数在**替身给的那几个答案**上算出来的那一份。
        answer(&["acct-a", "acct-b"], true, false);
        let want = relay_prefix_for(
            Some("acct-a"),
            &["acct-a".to_string(), "acct-b".to_string()],
            true,
            Some("sid-1"),
            false,
        )
        .expect("纯函数在这组输入上不该报错");
        // 反空真：这组输入下期望值本来就该是非空的，否则下面那条 `assert_eq!` 是「空 == 空」。
        assert!(
            !want.is_empty(),
            "期望值是空串 —— 那下面那条相等断言就是空真，本条按红处理"
        );
        let got = relay_prefix_for_launch(&action, Some(&acct_a)).expect("这一档不该报错");
        assert!(
            ROWS_CALLS.with(Cell::get) >= 1,
            "这次拉起**没问**「这个号在不在中转表里」—— 那一格成了常量"
        );
        assert!(
            RUNNING_CALLS.with(Cell::get) >= 1,
            "这次拉起**没问**「中转在不在跑」—— 那一格成了常量"
        );
        assert_eq!(
            got, want,
            "\n问是问了，**答案没被用上** —— 这正是 `D5` 那一刀的形状：\n\
             把两个事实算出来扔掉（一个用不到的绑定），入参换成空表 / 常量，\n\
             两段文本原地留着 ⇒ 上一版那条「窗口里有没有这段文本」的判据照绿，\n\
             而中转前缀恒空、本件的正题被整个摘掉。"
        );

        // ①b 🔴 `D6 阻-2`：**同一张表、只换一个号** ⇒ 路由键的 `<account>` 段必须跟着变。
        let got_b = relay_prefix_for_launch(&action, Some(&acct_b)).expect("这一档不该报错");
        assert_ne!(
            got, got_b,
            "\n换一个号，拼出来的前缀一个字节都没变 —— 「这次拉起是哪个号」这一维成了常量。\n\
             生产后果：路由键的 `<account>` 段恒指一个号 ⇒ 中转按它取 key ⇒\n\
             **acct-b 的会话拿着 acct-a 的那把 key 发请求，而两边都显示成功。**"
        );
        assert!(
            got.contains("/acct-a/") && got_b.contains("/acct-b/"),
            "路由键里的账号段不是这次拉起的那个号：acct-a ⇒ {got:?} · acct-b ⇒ {got_b:?}"
        );

        // ② **只**把「表里有没有这一行」翻过来 ⇒ 前缀空（不注入，逐字节旧路）。
        answer(&["someone-else"], true, false);
        assert_eq!(
            relay_prefix_for_launch(&action, Some(&acct_a)).expect("不该报错"),
            "",
            "表里没有这个号，却仍然拼出了前缀 —— 「在不在表里」这个答案没被用上"
        );

        // ③ **只**把「中转在不在跑」翻过来 ⇒ `Err`（`KH2B2`②：不许静默）。
        answer(&["acct-a"], false, false);
        assert!(
            relay_prefix_for_launch(&action, Some(&acct_a)).is_err(),
            "中转没在跑却照旧起出去了 —— 「在不在跑」这个答案没被用上，\n\
             症状会长成「claude 连不上 API」，与网络故障同形，而原因在我们这一侧"
        );

        // ④ 🔴 `D6 阻-3`：**只**把「这台机是不是 Windows」翻过来 ⇒ 渲成 PowerShell 形态。
        //    先前这一格是调用点上的 `cfg!(windows)`（常量表达式），刀 `Xb` 把它写死成 `false`
        //    ⇒ 全绿。收进缝之后，写死常量就意味着**这一格的答案没被用上**。
        answer(&["acct-a"], true, true);
        let ps = relay_prefix_for_launch(&action, Some(&acct_a)).expect("这一档不该报错");
        assert_eq!(
            ps, "$env:ANTHROPIC_BASE_URL='http://127.0.0.1:8788/s/claude-code/acct-a/sid-1'; ",
            "\n「这台机是不是 Windows」这个答案没被用上 —— 调用点把那一格写死了。\n\
             生产后果：Windows 上中转前缀渲染成 POSIX 形态（`export …` 塞进 PowerShell 串）\n\
             ⇒ 中转注入在 Windows 上整个失效，而 Windows 运行时行为本件在「判不了」里\n\
             ⇒ **判据是那一格唯一的守卫**。"
        );
        // 阴性对照：同一组输入只翻这一格，答案必须真的不同（否则上面那条是「两张脸长一样」）。
        answer(&["acct-a"], true, false);
        assert_ne!(
            relay_prefix_for_launch(&action, Some(&acct_a)).expect("不该报错"),
            ps,
            "两个平台渲出来的前缀一模一样 —— 这把尺子分不出 PowerShell 与 POSIX"
        );

        // ═══════════════════════════════════════════════════════════════════
        // ⑤⑥ 🔴 `D7 阻-4`：**「哪一次拉起」（`action` / `sid`）也是一维，它的取值域一直是 1**
        // ═══════════════════════════════════════════════════════════════════
        //
        // 上一轮这条判据在 `rows` / `running` / `windows` / `account` 四维上各喂了 ≥2 个值，
        // **而 `action` 只喂过 `LocalPsAction::Resume("sid-1")` 一个**。`D7` 打了两刀：
        // - 刀 `S2`：`LocalPsAction::New => return Ok(String::new())` ⇒ **`1229` 全绿**。
        //   生产后果：**新开会话那条路上中转注入恒空** —— 一个 api-key 号新开一个会话，
        //   claude 直连官方端点、第三方 key 用不上，而门禁四个数一格不动。
        //   ⚠ 而件文件 `§3a` 的 `KH2B6` 逐字写着「新开会话这条路**现在生产可达**」。
        // - 刀 `S1`：`Resume(_sid) => Some("sid-1")`（把 `<key>` 段写死）⇒ 也全绿。
        //   后果轻（`mint_route_key` 头注登记着这一段对路由惰性），**但那一维什么都没买**。
        //
        // ⇒ 与 `term` 那条链同一条纪律：**分叉点上的两支都要喂**，不许只买一支。
        let key_seg = |prefix: &str| -> String {
            let url = prefix
                .split('\'')
                .nth(1)
                .unwrap_or_else(|| panic!("前缀里没有被单引号包住的 URL：{prefix:?}"));
            url.rsplit('/')
                .next()
                .expect("URL 一个路径段都没有")
                .to_string()
        };

        answer(&["acct-a"], true, false);
        // ⑤ **`Resume` 那一支**：`<key>` 段必须是**这一次**的 sid，不是一个写死的串。
        let resumed_1 = relay_prefix_for_launch(&action, Some(&acct_a)).expect("不该报错");
        let resumed_2 =
            relay_prefix_for_launch(&LocalPsAction::Resume("sid-9".to_string()), Some(&acct_a))
                .expect("不该报错");
        assert_eq!(
            key_seg(&resumed_1),
            "sid-1",
            "路由键的 `<key>` 段不是这次的 sid"
        );
        assert_eq!(
            key_seg(&resumed_2),
            "sid-9",
            "\n换一个会话 id，路由键的 `<key>` 段没跟着变 —— 刀 `S1` 的形状：\n\
             `Resume(_sid) => Some(\"sid-1\")` 把这一段写死，那一维什么都没买。\n\
             实得 = {:?}",
            key_seg(&resumed_2)
        );

        // ⑥ 🔴 **`New` 那一支**：新开会话**也真的走中转**，而它的 `<key>` 段是一次性 nonce。
        let new_1 = relay_prefix_for_launch(&LocalPsAction::New, Some(&acct_a))
            .expect("新开会话这一档不该报错");
        assert!(
            !new_1.is_empty() && new_1.contains("ANTHROPIC_BASE_URL"),
            "\n★★ **新开会话那条路上中转前缀是空的** —— 这正是刀 `S2` 的形状：\n\
             `LocalPsAction::New => return Ok(String::new())`，而件文件 `§3a` 的 `KH2B6`\n\
             逐字写着这条路**现在生产可达**。\n\
             生产后果：一个 api-key 号新开一个会话 ⇒ **claude 直连官方端点、第三方 key 用不上**，\n\
             而门禁四个数一格不动。实得 = {new_1:?}"
        );
        assert!(
            new_1.contains("/acct-a/"),
            "新开会话拼出来的路由键里没有这次的账号段：{new_1:?}"
        );
        let new_key = key_seg(&new_1);
        assert_ne!(
            new_key,
            key_seg(&resumed_1),
            "新开会话拿到了 resume 那一支的 `<key>` 段 —— 这一维被抹平了"
        );
        // 反空真：nonce 那一支**每次都不同**（`route_key_for_session(None)` → `mint_route_key`）。
        // 恒定的 `<key>` 意味着这一支被换成了一个常量，而上面那条 `assert_ne!` 分不出来。
        let new_2 = relay_prefix_for_launch(&LocalPsAction::New, Some(&acct_a)).expect("不该报错");
        assert_ne!(
            new_key,
            key_seg(&new_2),
            "两次新开会话拿到同一个 `<key>` 段 —— 那一段不是 nonce，是一个常量"
        );
    }

    /// ★★★ `D5 阻-1` 的**同职第二处**：界面那一侧（`KH2B7` 的 `relay_routing_for`）
    /// **也真的问了那两件事，而且真的用了答案。**
    ///
    /// # 为什么它非有不可（分母在这里）
    ///
    /// 那两个取值口的生产消费方**恰好 2**：起会话那一侧（上一条）与本条这一侧。
    /// 上一条只买了第一处 —— 而「只覆盖了那条病的一个动词」正是本区最近四次打回的形状。
    /// 本条把第二处也钉住：把 `routed` 写死成空表 / 把 `running` 写死成常量，界面就会
    /// **说反**（「这个号走中转」和「中转在跑」两句都是用户唯一看得见的说法），而没有别的判据会红。
    #[test]
    fn the_ui_status_side_asks_those_two_take_points_and_uses_their_answers() {
        use std::cell::{Cell, RefCell};
        thread_local! {
            static ROWS_CALLS: Cell<u32> = const { Cell::new(0) };
            static RUNNING_CALLS: Cell<u32> = const { Cell::new(0) };
            static ROWS_ANSWER: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
            static RUNNING_ANSWER: Cell<bool> = const { Cell::new(false) };
        }
        fn spy_rows() -> Vec<String> {
            ROWS_CALLS.with(|c| c.set(c.get() + 1));
            ROWS_ANSWER.with(|v| v.borrow().clone())
        }
        fn spy_running() -> bool {
            RUNNING_CALLS.with(|c| c.set(c.get() + 1));
            RUNNING_ANSWER.with(Cell::get)
        }
        fn answer(rows: &[&str], running: bool) {
            ROWS_ANSWER.with(|v| *v.borrow_mut() = rows.iter().map(|s| s.to_string()).collect());
            RUNNING_ANSWER.with(|c| c.set(running));
        }

        let dirs = vec![
            "/h/.claude-accts/acct-a".to_string(),
            "/h/.claude-accts/acct-b".to_string(),
        ];
        let _guard = override_relay_facts(RelayFactSources {
            rows: spy_rows,
            running: spy_running,
            // 界面那一侧不看平台（徽章文案两个平台同一份）⇒ 这一格照生产那个取值口，不装替身。
            windows: platform_is_windows,
        });

        // ① 表里只有 `acct-a` + 中转在跑 ⇒ 只有那一个 configDir 被判「走中转」，`running` 为真。
        answer(&["acct-a"], true);
        let got = crate::relay_routing_for(dirs.clone());
        assert!(
            ROWS_CALLS.with(Cell::get) >= 1 && RUNNING_CALLS.with(Cell::get) >= 1,
            "界面这一侧**没问**那两件事（rows={} running={}）—— 那两格成了常量",
            ROWS_CALLS.with(Cell::get),
            RUNNING_CALLS.with(Cell::get)
        );
        assert_eq!(
            got.routed,
            vec!["/h/.claude-accts/acct-a".to_string()],
            "问是问了，**答案没被用上** —— 界面会把「走不走中转」说反"
        );
        assert!(got.running, "「中转在不在跑」的答案没被用上");

        // ② **只**把表翻过来 ⇒ 一个都不走（不是「随便回一份」）。
        answer(&[], true);
        assert!(
            crate::relay_routing_for(dirs.clone()).routed.is_empty(),
            "表空了界面还说有号走中转"
        );
        // ③ **只**把「在不在跑」翻过来 ⇒ `running` 跟着变（且 `routed` 不受它影响，两格分开）。
        answer(&["acct-b"], false);
        let flipped = crate::relay_routing_for(dirs);
        assert!(!flipped.running, "中转没跑，界面还说在跑");
        assert_eq!(
            flipped.routed,
            vec!["/h/.claude-accts/acct-b".to_string()],
            "两格串了 —— `routed` 不该跟着「在不在跑」变"
        );
    }

    /// ★★★ `D5 阻-1` 的第三格：**生产上插进那条缝的，就是那两个真取值口。**
    ///
    /// 上一条把替身换进去量行为 ⇒ 它量不到「生产那一份指的是谁」。
    /// 这一条按**函数地址**对拍（不是按文本）：把 [`PRODUCTION_RELAY_FACTS`] 里任何一格
    /// 换成一个返回常量的闭包 / 别的函数，本条当场红。
    ///
    /// # 🔴🔴 第二半是**我自己找第六层时找出来的**，别删
    ///
    /// 只对拍那个 `const` **不够**：`relay_facts()` 才是生产真正取值的那一跳。
    /// 有人把 `relay_facts()` 改成「不装替身时也回一份写死的」而**一个字节不动那个 `const`**
    /// ⇒ 地址对拍照绿（它读的是 `const`）、行为判据也照绿（它们装了替身、走的是另一支）
    /// ⇒ **又是一次「文本/形状留住、行为摘掉」，全绿。**
    /// ⇒ 所以下面**先在没装替身的状态下调一次 `relay_facts()`**，按地址断言它交出来的就是那两个真取值口。
    #[test]
    fn the_production_relay_facts_are_those_two_take_points() {
        // 反空真排最前：这把尺子**分得出**「不是那个函数」，否则下面两条是恒真。
        fn not_it() -> bool {
            true
        }
        assert!(
            !std::ptr::fn_addr_eq(PRODUCTION_RELAY_FACTS.running, not_it as fn() -> bool),
            "这把尺子对任何同型函数都说「是」—— 它恒真，本条按红处理"
        );
        // ★★ 第二半：**没装替身**的那一跳（= 生产那一跳）交出来的必须就是那两个真取值口。
        //    ⚠ 本条**刻意不装替身**；替身住 thread-local ⇒ 别的判据装的那份影响不到这里。
        let live = relay_facts();
        assert!(
            std::ptr::fn_addr_eq(live.rows, relay_rows as fn() -> Vec<String>),
            "没装替身时 `relay_facts()` 交出来的「表从哪来」不是 `relay_rows` ——\n\
             生产那一跳被换掉了，而只对拍那个 `const` 的判据看不见（第六层的形状）"
        );
        assert!(
            std::ptr::fn_addr_eq(
                live.running,
                crate::local_daemon::relay_running as fn() -> bool
            ),
            "没装替身时 `relay_facts()` 交出来的「中转在不在跑」不是 `local_daemon::relay_running`"
        );
        assert!(
            std::ptr::fn_addr_eq(
                PRODUCTION_RELAY_FACTS.rows,
                relay_rows as fn() -> Vec<String>
            ),
            "生产上「这个号在不在中转表里」不再由 `relay_rows` 答 ——\n\
             换成一个恒空的东西，谁都不走中转，而行为判据（喂替身的那条）照绿"
        );
        assert!(
            std::ptr::fn_addr_eq(
                PRODUCTION_RELAY_FACTS.running,
                crate::local_daemon::relay_running as fn() -> bool
            ),
            "生产上「中转在不在跑」不再由 `local_daemon::relay_running` 答 ——\n\
             换成恒真，`KH2B2`② 那道「起不来就当场拒」的闸整个失效，而行为判据照绿"
        );
        // 🔴 `D6 阻-3`：第三格（平台开关）同样按地址对拍，两跳都拍。
        assert!(
            std::ptr::fn_addr_eq(live.windows, platform_is_windows as fn() -> bool)
                && std::ptr::fn_addr_eq(
                    PRODUCTION_RELAY_FACTS.windows,
                    platform_is_windows as fn() -> bool
                ),
            "生产上「这台机是不是 Windows」不再由 `platform_is_windows` 答 ——\n\
             换成一个恒假的东西，Windows 上前缀渲成 POSIX 形态、注入整个失效，\n\
             而 Windows 运行时在本件的「判不了」里 ⇒ 这一格只有判据这一个守卫"
        );

        // ★★ `D6 阻-1`：**送出去**那条缝同样按地址对拍（同样两跳：`const` 与没装替身的那一跳）。
        //    只对拍 `const` 不够的理由与上面第二半逐字同一条。
        let sink = launch_sink();
        #[cfg(not(windows))]
        let production_sink =
            crate::launch::launch_local_posix as fn(&str, Option<&str>) -> Result<(), String>;
        #[cfg(windows)]
        let production_sink =
            crate::launch::launch_powershell_window as fn(&str, Option<&str>) -> Result<(), String>;
        assert!(
            std::ptr::fn_addr_eq(sink.0, production_sink)
                && std::ptr::fn_addr_eq(PRODUCTION_LAUNCH_SINK.0, production_sink),
            "没装替身时最后送出去的那一步不是 `launch::launch_local_posix` / \
             `launch::launch_powershell_window` ——\n\
             生产那一跳被换掉了，而驱动 `launch_local` 的那条行为判据装了替身、看不见这件事"
        );
    }

    /// ★★★ `K-R55`（09-11）：**本机 ccm 探测那条新缝，生产上插的就是那个真取值口。**
    ///
    /// 上一条对拍的是中转那三格与送法；这一条是同一个形状的第四处 ——
    /// [`CcmProbeSource`] 是本拍为了让
    /// `a_launch_that_goes_through_the_relay_still_cannot_get_a_tmux_container`
    /// 真去走生产那条路才开的，而**开一条缝就欠一条地址对拍**：
    /// 缝一旦在生产上也指着一份写死的答案（比如「恒装着、能力全有」），
    /// 那条行为判据**照绿**（它本来就装替身），而生产上「没装 ccm 要诚实降级」整条没了。
    ///
    /// 两跳都拍，理由与上一条第二半逐字同一条：只拍 `const` 时，
    /// 有人把 [`ccm_probe_source`] 改成「不装替身也回一份写死的」就绕过去了。
    #[test]
    #[cfg(not(windows))]
    fn the_local_launch_really_asks_the_production_ccm_probe() {
        // 反空真排最前：这把尺子分得出「不是那个函数」，否则下面两条恒真。
        fn not_it() -> crate::ccm_probe::CcmProbeResult {
            crate::ccm_probe::CcmProbeResult {
                installed: false,
                version: None,
                capabilities: vec![],
                build: None,
            }
        }
        assert!(
            !std::ptr::fn_addr_eq(
                PRODUCTION_CCM_PROBE.0,
                not_it as fn() -> crate::ccm_probe::CcmProbeResult
            ),
            "这把尺子对任何同型函数都说「是」—— 它恒真，本条按红处理"
        );
        let production =
            crate::ccm_probe::probe_local_ccm as fn() -> crate::ccm_probe::CcmProbeResult;
        // ⚠ **刻意不装替身**（替身住 thread-local ⇒ 别的判据装的那份影响不到这里）。
        assert!(
            std::ptr::fn_addr_eq(ccm_probe_source().0, production),
            "没装替身时 `ccm_probe_source()` 交出来的不是 `ccm_probe::probe_local_ccm` ——\n\
             生产那一跳被换掉了，而只对拍那个 `const` 的判据看不见（第六层的形状）"
        );
        assert!(
            std::ptr::fn_addr_eq(PRODUCTION_CCM_PROBE.0, production),
            "生产上「这台机装没装 ccm、有哪些能力」不再由 `ccm_probe::probe_local_ccm` 答 ——\n\
             换成一份写死的「装着且能力全有」，没装 ccm 的机器会渲出一条带未知 flag 的命令，\n\
             而那时回落分支已经不在了（`render_local_ccm` 头注里那个 fail-open）"
        );
    }

    /// ★★★ `D1 阻-6` 刀 C 的反面：**`relay_rows` 真的去读那份文件、真的解析出行。**
    ///
    /// `D1` 实测过：把它整个换成 `Vec::new()`，**1221 passed / 0 failed** ——
    /// 也就是说「这个号在不在中转表里」这个**取值口**当时一条判据都没有，
    /// 而它一旦恒空，整件事的表现就是「谁都不走中转」，**而且全绿**。
    #[test]
    fn the_rows_really_come_from_that_file_not_from_a_constant() {
        let dir = std::env::temp_dir().join(format!(
            "ccm-rows-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        std::fs::create_dir_all(&dir).expect("建临时目录");
        let f = dir.join("relay-credentials.json");

        // ① 文件不在 ⇒ 零条（**不是**报错：读不到与一条没配的正确行为都是「照旧直连」）。
        assert!(relay_rows_at(&f).is_empty(), "文件不在却读出了行");
        // ② 真写一份（裸 `fs::write` = 人拿编辑器写的那一份）⇒ 逐条读出来。
        std::fs::write(
            &f,
            b"{\n  \"accounts\": {\n    \"acct-a\": { \"api_key\": \"K1\" },\n    \"acct-b\": {}\n  }\n}\n",
        )
        .expect("写夹具");
        let mut got = relay_rows_at(&f);
        got.sort();
        assert_eq!(
            got,
            vec!["acct-a".to_string(), "acct-b".to_string()],
            "没把那份文件里的行读出来 —— 这个取值口恒空的话，谁都不会走中转，而且全绿"
        );
        // ③ 当不了路由段的 id **筛掉**（与中转装表那一侧同一条规则）。
        std::fs::write(
            &f,
            b"{\n  \"accounts\": {\n    \"ok-1\": {},\n    \"has.dot\": {},\n    \"has/slash\": {}\n  }\n}\n",
        )
        .expect("写夹具");
        assert_eq!(
            relay_rows_at(&f),
            vec!["ok-1".to_string()],
            "界面这一侧收下了中转装表时会丢掉的行 —— 那会让界面说「经本机中转」而中转 404"
        );
        // ④ 文件坏了 ⇒ 零条 + 不 panic（人手编打错一个逗号是常态）。
        std::fs::write(&f, b"{ not json").expect("写夹具");
        assert!(relay_rows_at(&f).is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// ★★ `KH2B7` 的产出方：**界面问的那个「有没有行」，与起会话那一侧问的是同一个规则。**
    ///
    /// 两处各写一个 basename 规则，漂开的那天症状是「设置里说走中转、起会话时没走」，
    /// 而两边看起来都没错。⇒ 本条把它钉成**同一个函数的两个调用方**。
    #[test]
    fn the_ui_and_the_launch_side_derive_the_account_id_from_the_same_rule() {
        let rows = vec!["acct-a".to_string()];
        let dirs = vec![
            "/home/u/.claude-accts/acct-a".to_string(),
            "/home/u/.claude-accts/acct-b".to_string(),
            // 末段带尾斜杠 / 带空白的写法也要落到同一个 id 上。
            "  /home/u/.claude-accts/acct-a/  ".to_string(),
        ];
        // 非空对照排最前：先证明这把尺子不是恒空。
        assert_eq!(
            relay_routed_subset(&dirs, &rows),
            vec![
                "/home/u/.claude-accts/acct-a".to_string(),
                "  /home/u/.claude-accts/acct-a/  ".to_string()
            ],
            "界面那一侧筛出来的不是「表里有行」的那几个"
        );
        // ★ 与起会话那一侧**同一个规则**：同一个目录，两条路推出同一个 id。
        let named = LaunchAccount::Named {
            config_dir: "/home/u/.claude-accts/acct-a".to_string(),
            name: None,
        };
        assert_eq!(
            relay_account_id(Some(&named)),
            relay_account_id_of_dir("/home/u/.claude-accts/acct-a"),
            "两个调用方推出来的账号 id 不一样 —— 那正是「设置里说走中转、起会话时没走」的形状"
        );
        // 表里没有的行一个都不许混进来（`KL7` 第 2 条的界面侧倒影）。
        assert!(relay_routed_subset(&dirs, &[]).is_empty(), "空表却筛出了行");
        // 账号 0 / 空 configDir 推不出 id ⇒ 不在结果里（说不出就不表态）。
        assert!(relay_routed_subset(&["".to_string()], &rows).is_empty());
    }

    /// ★★ `KH2B2`②在这一层：中转没在跑 ⇒ **起会话这一侧当场说话**，
    /// 不许渲染成一条指向没人听的口的 URL（那会长成「claude 连不上 API」）。
    #[test]
    fn a_launch_that_needs_the_relay_is_refused_when_the_relay_is_not_running() {
        let rows = vec!["acct-a".to_string()];
        let e = relay_prefix_for(Some("acct-a"), &rows, false, Some("sid-1"), false)
            .expect_err("中转没起来却照旧渲染 —— 症状会与网络故障同形");
        assert!(e.contains("中转没在跑"), "错误得说出真正的原因：{e}");
        // 非空对照：只把「中转在跑」翻过来，同一条路径就不再报错。
        assert!(relay_prefix_for(Some("acct-a"), &rows, true, Some("sid-1"), false).is_ok());
    }

    /// ★★★ **接线判据**：`launch_local` **真正交出去的那一串**以中转前缀打头。
    ///
    /// # 🔴🔴🔴 它先前是一条文本判据，而 `D6` 的刀 `Y1` 把它打穿了（第七层）
    ///
    /// 第一版量的是「从 `fn launch_local(` 起切 3600 字节，窗口里**有没有**这三样文本」：
    /// ① `relay_prefix_for_launch(action, account)?` ② `relay + &` 恰好 2 处
    /// ③ `let cmd = relay + &base;` 与另外两处的先后序。
    /// 刀 `Y1` 在拼装那一行加了一句
    /// `let relay = if relay.is_empty() { relay } else { String::new() };`
    /// ⇒ 三样文本**一处不少**（三个锚点数与干净树逐字相同）
    /// ⇒ **`1227 passed; 0 failed`、`GATE: OK`、四个数与干净树逐字相同**，
    /// **而前缀算出来了没拼上去 —— 本件的正题整个被摘掉。**
    /// ⚠ 而**上一版这段头注里逐字写着它要防的正是这件事**（「算出来却没拼上去，行为上与本件
    /// 没做完全一样」）—— 威胁模型写对了，买的东西是文本。
    /// ⚠ 它还带着 `D4 阻-1` 那一形：判据住 `history.rs::tests`，而它 `include_str!("history.rs")`
    /// 扫的就是本文件 ⇒ `relay + &` 全仓 4 = 生产 2 + 本条的针 1 + 报文 1，按本仓变异纪律
    /// 「锚点全改」会把针一起带走。
    ///
    /// # ⇒ 换成量**真正送出去的那一串**
    ///
    /// [`LaunchSink`] 那条缝让判据能装一个记账替身，于是本条量的不再是源码，是
    /// **`launch_local` 最后交给送法的那个字符串**。三格，每格都能被一刀翻掉：
    ///
    /// | 格 | 断的是什么 | 翻掉它的形状 |
    /// |---|---|---|
    /// | ① | 表里没这个号 ⇒ 那一串里**一个 `ANTHROPIC_BASE_URL` 都没有** | 无条件注入 |
    /// | ② | 表里有 ⇒ 那一串**逐字节等于**「前缀 + ① 那一串」 | 刀 `Y1`（算了没拼）· 掏空注入点 |
    /// | ③ | 换一个号 ⇒ 前缀里的 `<account>` 段跟着变 | 刀 `E6`（账号段写死成常量） |
    /// | ⑤ | **新开会话**那一支也带前缀 | 刀 `S2`（`New => Ok(String::new())`） |
    /// | ⑥ | 换一个 sid ⇒ `<key>` 段跟着变 | 刀 `S1`（`Resume(_sid) => Some("sid-1")`） |
    ///
    /// 🔴 ⑤⑥ 是 `D7 阻-4` 补的：先前 ①–④ **全部走 `Resume("sid-1")`**
    /// ⇒ `action` 这一维的输入域是 1，而**新开会话是 `KH2B6` 逐字写着「现在生产可达」的一条路**。
    ///
    /// # ⚠ 它买不到什么（如实写）
    ///
    /// - **走 ccm 容器那一支**：本条喂 `tmux_name = None` ⇒ 走的是回落那条路。
    ///   而按 `a_launch_that_goes_through_the_relay_still_cannot_get_a_tmux_container`，
    ///   **带中转前缀的拉起今天必然落到回落路** ⇒ 本条驱动的正是那条生产可达的路。
    ///   ccm 那一支上「前缀有没有拼」由同一行代码管（合流之后**只有一处**拼接）。
    /// - **送法自己拿到串之后干了什么**：那是 `launch::launch_local_posix` 自己的判据面；
    ///   「生产上插进这条缝的就是它」由 `the_production_relay_facts_are_those_two_take_points`
    ///   末尾那一格按**函数地址**对拍。
    /// - **谁绕开这条缝直接调送法**：由 `payload.rs` 那道人群闸数着（零调用点）。
    #[test]
    fn the_relay_prefix_is_really_prepended_to_the_command_that_gets_launched() {
        use std::cell::{Cell, RefCell};
        thread_local! {
            static SENT: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
            static ROWS_ANSWER: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
            static RUNNING_ANSWER: Cell<bool> = const { Cell::new(false) };
        }
        fn recorder(cmd: &str, _cwd: Option<&str>) -> Result<(), String> {
            SENT.with(|v| v.borrow_mut().push(cmd.to_string()));
            Ok(())
        }
        fn spy_rows() -> Vec<String> {
            ROWS_ANSWER.with(|v| v.borrow().clone())
        }
        fn spy_running() -> bool {
            RUNNING_ANSWER.with(Cell::get)
        }
        fn answer(rows: &[&str], running: bool) {
            ROWS_ANSWER.with(|v| *v.borrow_mut() = rows.iter().map(|s| s.to_string()).collect());
            RUNNING_ANSWER.with(|c| c.set(running));
        }
        fn last_sent() -> String {
            SENT.with(|v| v.borrow().last().cloned().expect("这一趟什么都没送出去"))
        }

        let _sink = override_launch_sink(LaunchSink(recorder));
        let _facts = override_relay_facts(RelayFactSources {
            rows: spy_rows,
            running: spy_running,
            // 平台那一格照生产那个取值口（本条不翻它 —— 翻它的是上面那条判据的第 ④ 格）。
            windows: platform_is_windows,
        });

        let action = LocalPsAction::Resume("sid-1".to_string());
        let accounts = [
            ("acct-a", "/h/.claude-accts/acct-a"),
            // 🔴 `D6 阻-2`：**两个号**，不是一个 —— 「这次拉起是哪个号」也要能翻。
            ("acct-b", "/h/.claude-accts/acct-b"),
        ];

        let mut with_relay = Vec::new();
        for (id, dir) in accounts {
            let account = LaunchAccount::Named {
                config_dir: dir.to_string(),
                name: None,
            };
            // ① 表里没有这个号 ⇒ 逐字节旧路。这一趟同时是下面那条相等断言的**基准串**。
            answer(&[], true);
            launch_local(&action, None, None, Some(&account), None)
                .expect("不走中转这一趟不该失败");
            let bare = last_sent();
            assert!(
                !bare.is_empty() && bare.contains(dir),
                "基准串不像一条本机拉起命令（连这个号的 configDir 都没有）：{bare:?}"
            );
            assert!(
                !bare.contains("ANTHROPIC_BASE_URL"),
                "表里没有这个号，送出去的那一串却带着中转注入 —— \
                 `KH2B5`「没配第三方 key 的号一个字节都不受影响」当场破了：{bare:?}"
            );

            // ② 表里有这个号 ⇒ 送出去的那一串**逐字节等于**「前缀 + 基准串」。
            answer(&["acct-a", "acct-b"], true);
            let prefix = relay_prefix_for(
                Some(id),
                &["acct-a".to_string(), "acct-b".to_string()],
                true,
                Some("sid-1"),
                cfg!(windows),
            )
            .expect("纯函数在这组输入上不该报错");
            // 反空真：期望的前缀本来就该是非空的，否则下面那条相等断言是「x == x」。
            assert!(!prefix.is_empty(), "期望前缀是空串 —— 本条按红处理");
            launch_local(&action, None, None, Some(&account), None).expect("走中转这一趟不该失败");
            let routed = last_sent();
            assert_eq!(
                routed,
                format!("{prefix}{bare}"),
                "\n★★ **算出来了没拼上去** —— 这正是 `D6` 刀 `Y1` 的形状：\n\
                 `relay_prefix_for_launch` 照样被调、照样答对，而拼装那一行把它扔了\n\
                 ⇒ 起会话的命令串里没有 `ANTHROPIC_BASE_URL`，本件的正题整个被摘掉，\n\
                 **而上一版那条数三样文本的判据照绿。**\n\
                 号 = {id} · 实得 = {routed:?} · 期望 = {:?}",
                format!("{prefix}{bare}")
            );
            with_relay.push((id, routed));
        }

        // ③ 🔴 `D6 阻-2`：**两个号送出去的前缀必须不一样**，而且各自带自己的账号段。
        let (id_a, cmd_a) = &with_relay[0];
        let (id_b, cmd_b) = &with_relay[1];
        assert!(
            cmd_a.contains(&format!("/{id_a}/")) && cmd_b.contains(&format!("/{id_b}/")),
            "\n路由键里的账号段不是这次拉起的那个号 —— 刀 `E6` 的形状：\n\
             把 `relay_account_id` 的答案 `.map(|_| \"acct-a\")` 写死，\n\
             生产后果是 **acct-b 的会话拿着 acct-a 的那把 key 发请求，两边都显示成功**。\n\
             实得：{cmd_a:?} · {cmd_b:?}"
        );
        assert_ne!(
            cmd_a.split("; ").next(),
            cmd_b.split("; ").next(),
            "两个号送出去的第一段（中转前缀）逐字节相同 —— 「哪个号」这一维成了常量"
        );

        // ④ 中转没在跑 ⇒ **当场拒，而且一个字节都没送出去**（`KH2B2`②：不许静默）。
        let before = SENT.with(|v| v.borrow().len());
        answer(&["acct-a"], false);
        let account = LaunchAccount::Named {
            config_dir: "/h/.claude-accts/acct-a".to_string(),
            name: None,
        };
        assert!(
            launch_local(&action, None, None, Some(&account), None).is_err(),
            "中转没在跑却照旧起出去了"
        );
        assert_eq!(
            SENT.with(|v| v.borrow().len()),
            before,
            "拒了却还是往外送了一条命令 —— 那条「当场拒」只拒在返回值上"
        );

        // ═══════════════════════════════════════════════════════════════════
        // ⑤⑥ 🔴 `D7 阻-4`：**「哪一次拉起」这一维，在送出去的那一串上也要翻得动**
        // ═══════════════════════════════════════════════════════════════════
        //
        // 上面 ①–④ 全部走 `Resume("sid-1")` ⇒ `action` 这一维的输入域 = 1。
        // 刀 `S2`（`New => return Ok(String::new())`）在**这条判据上也全绿**，
        // 而**新开会话是 `KH2B6` 逐字写着「现在生产可达」的一条路**。
        answer(&["acct-a"], true);
        let acct = LaunchAccount::Named {
            config_dir: "/h/.claude-accts/acct-a".to_string(),
            name: None,
        };

        // ⑤ **`New` 那一支**：送出去的那一串必须也带中转注入，且账号段是这次的号。
        launch_local(&LocalPsAction::New, None, None, Some(&acct), None)
            .expect("新开会话走中转这一趟不该失败");
        let new_sent = last_sent();
        assert!(
            new_sent.contains("ANTHROPIC_BASE_URL") && new_sent.contains("/acct-a/"),
            "\n★★ **新开会话送出去的那一串里没有中转注入** —— 刀 `S2` 的形状：\n\
             `LocalPsAction::New => return Ok(String::new())`。\n\
             生产后果：一个 api-key 号**新开**一个会话 ⇒ claude 直连官方端点、\n\
             第三方 key 用不上，而全量门禁四个数一格不动。实得 = {new_sent:?}"
        );

        // ⑥ **`Resume` 那一支的 sid 不是写死的**：换一个 sid，送出去的那一串跟着变。
        launch_local(
            &LocalPsAction::Resume("sid-9".to_string()),
            None,
            None,
            Some(&acct),
            None,
        )
        .expect("这一趟不该失败");
        let resumed_9 = last_sent();
        assert!(
            resumed_9.contains("/sid-9'"),
            "\n换一个会话 id，送出去的那一串里的 `<key>` 段没跟着变 —— 刀 `S1` 的形状：\n\
             `Resume(_sid) => Some(\"sid-1\")`。实得 = {resumed_9:?}"
        );
        // 反空真：这两趟本来就该是两条不同的串（否则上面两条里有一条在数同一份东西）。
        assert_ne!(
            new_sent, resumed_9,
            "新开与 resume 送出去的是同一串 —— 「哪一次拉起」这一维成了常量"
        );
    }

    // ═════════════════════════════════════════════════════════════════════════
    // 🔴 `K-P5b` `KP5BD1`：**起会话方把身份塞进了下一跳的进程环境**
    // ═════════════════════════════════════════════════════════════════════════

    /// 从送出去的那一串里，把身份那一段（到第一个 `; ` 为止）抠出来。
    ///
    /// ⚠ 用**变量名**定位，不用位置定位：位置是会变的（今天身份那一段前面还有中转前缀），
    /// 而「哪一段是身份」这件事只有变量名说得准。
    fn identity_segment(cmd: &str) -> Option<&str> {
        let i = cmd.find(LAUNCH_ID_VAR)?;
        let seg = &cmd[i..];
        Some(&seg[..seg.find("; ").unwrap_or(seg.len())])
    }

    /// ★★★ `KP5BD1`：`launch_local` **真正交出去的那一串**里，身份被塞进了进程环境。
    ///
    /// # 它量的是行为，不是文本（这条纪律是 `D6` 刀 `Y1` 花一整轮买回来的）
    ///
    /// 量文本那一形已经在本文件里被打穿过一次：`relay_prefix_for_launch` 照样被调、
    /// 答案照样对，而拼装那一行把它扔了 ⇒ 三个文本锚点一处不少、全量门禁四个数与干净树逐字相同。
    /// ⇒ 本条一个字节的源码都不扫，只看 [`LaunchSink`] 那条缝上**真正交出去的那个字符串**。
    ///
    /// # 五格，每格能被哪一刀翻掉
    ///
    /// | 格 | 断的是什么 | 翻掉它的形状 |
    /// |---|---|---|
    /// | ① | 送出去的那一串里**有** `CCM_LAUNCH_ID=<sid>` 这一句 | 把 `+ &identity.prefix` 从拼装那一行删掉（刀 `Y1` 同形） |
    /// | ② | 换一个 sid ⇒ 那一段跟着变 | `Resume(_) => Some("sid-1")`（身份写死） |
    /// | ③ | **新开**那一支也有身份，且两趟 token 不同 | `New => String::new()`（只给 resume 落身份 —— 而 `K-P5 §3 三` 现打的正是「新开那一支没有 sid」，它才是本件的正主） |
    /// | ④ | **铸法是共用那一份**：喂一个过不了白名单的 sid ⇒ token **不是**那个 sid | 在本文件里另写一份 `match { Resume(s) => s.clone(), … }`（第二份铸法，白名单回落那一格丢了） |
    /// | ⑤ | 平台那一维翻得动：`windows = true` ⇒ 渲成 `$env:` 形态 | 把 `windows` 那一格写死（`D6` 刀 `Xb` 同形，生产后果是 Windows 上塞出一句 POSIX `export`） |
    ///
    /// # ⚠ 它买不到什么（如实写，别读宽）
    ///
    /// - **走 ccm 容器那一支身份到不到得了 agent 进程**：到不了。本条喂 `tmux_name = None`
    ///   ⇒ 走的是回落那条路（渲染器早退，见 [`NO_TMUX_NAME`]）。容器那一支上外侧这句 `export`
    ///   会在 tmux 边界被吃掉（与 `K-H2b` 给 `ANTHROPIC_BASE_URL` 踩过的**同一个坑**，
    ///   那一次的修法是在容器载荷内侧补一句转发）—— 当时那份 bash `ccm` 是红线文件，
    ///   那一句没补 ⇒ 这一格当时是个洞，登记在 `launcher_identity_registry` 的 `L1` 那一行里。
    ///   🔴 〔`K-R61` 09-11 现打〕`remote-daemon-proto/src/control/ccm/plan.rs` 的容器路
    ///   **今天有** `export CCM_LAUNCH_ID=…` 那一句 ⇒ **那个洞的成因很可能已经不在了**。
    ///   但「洞补没补上」的落点是 `launcher_identity_registry` 的 `L1`，**不在 `K-R61` 写区**，
    ///   本轮**没有**去重裁它 —— 已报回 PM。在有人重裁之前，别把这一段读成「已经全覆盖」。
    /// - **读的那一侧**：daemon 从 `/proc/<pid>/environ` 读回来、经 wire 帧发出去 —— 本拍**没有做**
    ///   （面在 `remote-daemon-proto/`，不在本拍写区）。⇒ 今天这个变量**有人写、没人读**。
    /// - **Windows 上的运行时行为**：一行都没量（这台机器是 Linux）。⑤ 买到的只是
    ///   「平台那一格翻得动、渲出来的形态跟着变」，不是「PowerShell 里真的设上了」。
    #[test]
    fn the_launcher_plants_the_session_identity_into_the_process_environment() {
        use std::cell::{Cell, RefCell};
        thread_local! {
            static SENT: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
            static WINDOWS_ANSWER: Cell<bool> = const { Cell::new(false) };
        }
        fn recorder(cmd: &str, _cwd: Option<&str>) -> Result<(), String> {
            SENT.with(|v| v.borrow_mut().push(cmd.to_string()));
            Ok(())
        }
        fn no_rows() -> Vec<String> {
            Vec::new()
        }
        fn relay_up() -> bool {
            true
        }
        fn spy_windows() -> bool {
            WINDOWS_ANSWER.with(Cell::get)
        }
        fn last_sent() -> String {
            SENT.with(|v| v.borrow().last().cloned().expect("这一趟什么都没送出去"))
        }

        let _sink = override_launch_sink(LaunchSink(recorder));
        // 表里一行都没有 ⇒ 中转前缀恒空 ⇒ 本条量到的只有身份那一段（两件事分开量）。
        let _facts = override_relay_facts(RelayFactSources {
            rows: no_rows,
            running: relay_up,
            windows: spy_windows,
        });
        let account = LaunchAccount::Named {
            config_dir: "/h/.claude-accts/acct-a".to_string(),
            name: None,
        };

        // ① resume：身份就是这条会话的 sid，逐字节。
        let sid = "0198f0d2-1111-4222-8333-444455556666";
        launch_local(
            &LocalPsAction::Resume(sid.to_string()),
            None,
            None,
            Some(&account),
            None,
        )
        .expect("这一趟不该失败");
        let first = last_sent();
        assert!(
            !first.contains("ANTHROPIC_BASE_URL"),
            "表里一行都没有，却混进了中转前缀 —— 本条的替身没装上，下面几格量的不是身份：{first:?}"
        );
        let want_first = format!("{LAUNCH_ID_VAR}='{sid}'");
        assert_eq!(
            identity_segment(&first),
            Some(want_first.as_str()),
            "\n★★ **起会话方没把身份塞进进程环境** —— 刀的形状是把\n\
             `+ &identity.prefix` 从拼装那一行删掉（`D6` 刀 `Y1` 同形：\n\
             token 照样铸得出来，只是没拼上去）。\n\
             生产后果：起出来的那条会话**在环境里说不出自己是谁**，\n\
             读的那一侧只能退回去扫窗口标题 —— 那正是本件要消灭的东西。\n\
             实得整串 = {first:?}"
        );

        // ② 换一个 sid ⇒ 身份那一段跟着变（不是常量）。
        let sid9 = "0198f0d2-9999-4222-8333-444455556666";
        launch_local(
            &LocalPsAction::Resume(sid9.to_string()),
            None,
            None,
            Some(&account),
            None,
        )
        .expect("这一趟不该失败");
        let second = last_sent();
        let want_second = format!("{LAUNCH_ID_VAR}='{sid9}'");
        assert_eq!(
            identity_segment(&second),
            Some(want_second.as_str()),
            "换一条会话，环境里的身份没跟着变 —— 「这条会话是谁」成了常量：{second:?}"
        );

        // ③ **新开**那一支也落身份，而且两趟拿到的是两个不同的 nonce。
        //   `K-P5 §3 三` 现打：5 个起会话方**没有一处**在起新会话时知道 sid
        //   ⇒ 新开这一支才是本件的正主，它落不落身份不能靠 resume 那一支代言。
        launch_local(&LocalPsAction::New, None, None, Some(&account), None)
            .expect("新开这一趟不该失败");
        let new_a = identity_segment(&last_sent())
            .expect("新开那一支送出去的串里没有身份 —— `New => String::new()` 那一刀的形状")
            .to_string();
        launch_local(&LocalPsAction::New, None, None, Some(&account), None)
            .expect("新开这一趟不该失败");
        let new_b = identity_segment(&last_sent()).expect("同上").to_string();
        assert_ne!(
            new_a, new_b,
            "两次新开拿到同一个身份 —— nonce 成了常量，两条会话在环境里说自己是同一个人"
        );
        assert!(
            !new_a.contains(sid) && !new_a.contains(sid9),
            "新开那一支把上一条 resume 的 sid 当成了自己的身份：{new_a:?}"
        );

        // ④ **铸法是共用那一份**：喂一个过不了 `relay_segment_is_safe` 白名单的 sid，
        //   共用那份铸法会回落到 nonce；本文件里另写的第二份不会。
        //   ⇒ 这一格是「有没有真的调那一份」唯一翻得出来的一维。
        //
        //   ⚠ 夹具**必须同时满足两件事**，第一版选错了（现打修的）：
        //   ① 过得了 `local_launch_choice` 那道 sid 校验（字母数字 + `-` + `_`，**无长度上限**）——
        //      带 `/` 的串在那一关就被拒了，整趟 `launch_local` 回 `Err`，
        //      本条量到的是「拉起失败」而不是「身份铸法」；
        //   ② 过不了 `relay_segment_is_safe`（同一套字符集，但**多一条 ≤128 字节**）。
        //   ⇒ 两者的差集今天恰好只有**长度**这一维 ⇒ 用一个 129 字节的纯字母 sid。
        let bad = "a".repeat(129);
        let bad = bad.as_str();
        assert!(
            !crate::backend::control::payload::relay_segment_is_safe(bad),
            "夹具选错了：这个 sid 过得了白名单 ⇒ 下面那条断言是空真"
        );
        launch_local(
            &LocalPsAction::Resume(bad.to_string()),
            None,
            None,
            Some(&account),
            None,
        )
        .expect("这一趟不该失败");
        let dirty = identity_segment(&last_sent())
            .expect("这一趟没有身份")
            .to_string();
        assert!(
            !dirty.contains(bad),
            "\n★★ **本文件自己又铸了一份身份** —— 一个过不了白名单的 sid 被原样当成了身份。\n\
             共用的那份铸法（`payload::route_key_for_session`）在这一格会回落到 nonce；\n\
             会这样答的只有第二份实现。⇒ `KP5BD1`「铸法只有一份」当场破。实得 = {dirty:?}"
        );

        // ⑤ 平台那一维翻得动：`windows = true` ⇒ 渲成 PowerShell 形态。
        //   写死那一格的生产后果是 Windows 上往 PowerShell 串里塞一句 POSIX `export`
        //   ⇒ 身份注入整个失效（`D6` 刀 `Xb` 在中转那一格上的同一形）。
        WINDOWS_ANSWER.with(|c| c.set(true));
        launch_local(
            &LocalPsAction::Resume(sid.to_string()),
            None,
            None,
            Some(&account),
            None,
        )
        .expect("这一趟不该失败");
        let ps = last_sent();
        assert!(
            ps.contains(&format!("$env:{LAUNCH_ID_VAR}='{sid}'; ")),
            "\n把「这台机是不是 Windows」翻成 true，身份那一句还是 POSIX 形态 ——\n\
             那一格是个常量，Windows 上会往 PowerShell 串里塞一句 `export`。实得 = {ps:?}"
        );
        // 反空真：POSIX 那一趟本来就该拿不到这个形状（否则上面那条恒真）。
        //
        // ⚠ **09-09 订正（云端 windows-latest 首跑逮到）**：这里原写
        //   `!first.contains("$env:")` —— 断的是**整串**。而 `launch_local` 里 `base`
        //   那一格是 `#[cfg(windows)]` 选的（它**不走**这条缝），Windows 上它渲出
        //   `$env:CLAUDE_CONFIG_DIR='…'; ` ⇒ 原断言在 Windows 上**恒假**，
        //   量的根本不是身份那一段。
        //   ⇒ 收窄到**只盯身份那一段**，并补一条**正**的：POSIX 那一趟必须真的渲出
        //   `export <变量名>=`。两条合起来比原来那一条**更严** ——
        //   原写法在「身份那一段整个不见了」时是绿的。
        assert!(
            first.contains(&format!("export {LAUNCH_ID_VAR}=")),
            "POSIX 那一趟没渲出 `export {LAUNCH_ID_VAR}=` —— 上面那条断言是恒真的：{first:?}"
        );
        assert!(
            !first.contains(&format!("$env:{LAUNCH_ID_VAR}=")),
            "POSIX 那一趟把身份渲成了 `$env:` —— 上面那条断言是恒真的：{first:?}"
        );
    }

    // ═════════════════════════════════════════════════════════════════════════
    // 🔴🔴 `K-P5h` `KP5HD1`：**铸出来的那个 token 真的交到了调用方手上**
    // ═════════════════════════════════════════════════════════════════════════

    /// ★★★ `KP5HD1`：[`launch_local`] / [`new_local_session`] 回的那个串，
    /// **就是塞进那次拉起进程环境里的同一个 token**；而拼出来的命令串**一个字节没变**。
    ///
    /// # 它与旁边那条老判据的分工（两条都要，别合并）
    ///
    /// [`the_launcher_plants_the_session_identity_into_the_process_environment`] 买的是
    /// 「**塞进去了**」；本条买的是「**交出来了**」。`K-P5g` 交回时现打的卡点逐字是
    /// 「写侧把 token 铸完就扔」—— 那一天上面那条老判据**全绿**，
    /// 因为塞进去这件事一直是对的，缺的是**没有任何调用方手上有那个 token**。
    /// ⇒ 两件事各自要有自己的牙。
    ///
    /// # 五格，每格能被哪一刀翻掉
    ///
    /// | 格 | 断的是什么 | 翻掉它的形状 |
    /// |---|---|---|
    /// | ① | 回的那个串**逐字节**是命令里 `CCM_LAUNCH_ID=` 后面那个值 | `Ok("x".into())`（回一个常量）· `Ok(identity.prefix)`（回错那一半） |
    /// | ② | 两趟**新开**回的是两个不同的 token | 同上那个常量刀（`KP5HD1` 的死值验逐字点名的就是它） |
    /// | ③ | resume 那一支回的是 sid 本身 | 把 `New`/`Resume` 两支的返回值接反 |
    /// | ④ | **additive**：交出来这件事没改动命令串 —— 送出去的那一串逐字节等于 `前缀 + 基准串` | 在拼装那一行顺手动一下（多拼 / 少拼 / 换序，`D6` 刀 `Y1` 那一族） |
    /// | ⑤ | 拉起**失败**时不回 token | 把 `?` 换成忽略错误（那会让调用方去等一条不存在的会话） |
    ///
    /// # ⚠ 它买不到什么（如实写）
    ///
    /// - **拿这个 token 真能反查出 sid**：那要一条真的跑起来的会话 + 一个真 daemon。
    ///   本条只买到「token 到了调用方手上」，反查那一跳的判据在前端
    ///   （`tests/accounts.vitest.ts` 的 `K-P5h` 那一组，`KP5HD2`）。
    /// - **走 ccm 容器那一支**：与老判据同一个洞（喂 `tmux_name = None` ⇒ 走回落那条路），
    ///   登记在 `launcher_identity_registry` 的 `L1` 那一行里。
    /// - **Windows 上的运行时行为**：④ 那一格按平台各自取基准串，但这台机器是 Linux，
    ///   PowerShell 那一侧一行都没真跑过。
    #[test]
    fn the_minted_identity_token_is_handed_back_to_the_caller() {
        use std::cell::RefCell;
        thread_local! {
            static SENT2: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
        }
        fn recorder(cmd: &str, _cwd: Option<&str>) -> Result<(), String> {
            SENT2.with(|v| v.borrow_mut().push(cmd.to_string()));
            Ok(())
        }
        fn boom(_cmd: &str, _cwd: Option<&str>) -> Result<(), String> {
            Err("拉起失败（判据夹具）".to_string())
        }
        fn no_rows() -> Vec<String> {
            Vec::new()
        }
        fn relay_up() -> bool {
            true
        }
        fn not_windows() -> bool {
            false
        }
        fn last_sent() -> String {
            SENT2.with(|v| v.borrow().last().cloned().expect("这一趟什么都没送出去"))
        }

        let _sink = override_launch_sink(LaunchSink(recorder));
        // 表里一行都没有 ⇒ 中转前缀恒空 ⇒ ④ 那一格量的是「身份 + 基准串」这两段，
        // 中转那一段由它自己那条判据管（两件事分开量）。
        let _facts = override_relay_facts(RelayFactSources {
            rows: no_rows,
            running: relay_up,
            windows: not_windows,
        });
        let account = LaunchAccount::Named {
            config_dir: "/h/.claude-accts/acct-a".to_string(),
            name: None,
        };

        // ① **新开**那一支：回的那个串就是命令里那个值。
        //    ⚠ 这里刻意**不**拿 `identity_segment` 的整段去比 —— 那样只要回的是
        //    「`CCM_LAUNCH_ID=…` 这一整句」就绿了，而本条要的是**值本身**。
        let token_a = launch_local(&LocalPsAction::New, None, None, Some(&account), None)
            .expect("新开这一趟不该失败");
        assert!(
            !token_a.trim().is_empty(),
            "新开那一趟回了个空串 —— 「交出来」这一步等于没做"
        );
        let sent_a = last_sent();
        let want_a = format!("{LAUNCH_ID_VAR}='{token_a}'");
        assert_eq!(
            identity_segment(&sent_a),
            Some(want_a.as_str()),
            "\n★★ **交回来的 token 不是塞进环境里的那一个。**\n\
             刀的形状：`Ok(\"x\".into())`（回一个常量）或 `Ok(identity.prefix)`（回错那一半）。\n\
             生产后果：起会话方拿着一个**谁也不认识**的串去反查 sid ⇒ 永远查不到，\n\
             而「查不到就不猜」会让整条回填静默失效 —— 与本件没做完全一样。\n\
             交回来的 = {token_a:?}，送出去的整串 = {sent_a:?}"
        );

        // ② 两趟新开 ⇒ 两个**不同**的 token（常量刀在这里也红一次，两格互为纵深）。
        let token_b = launch_local(&LocalPsAction::New, None, None, Some(&account), None)
            .expect("新开这一趟不该失败");
        assert_ne!(
            token_a, token_b,
            "两趟新开交回来的是同一个 token —— 「交出来」那一步回的是常量，\n\
             而 `KP5HD1` 的死值验逐字点名的就是这一刀"
        );

        // ③ resume 那一支：token 就是 sid 本身（`route_key_for_session(Some(sid))` 原样返回）。
        //    ⚠ 这一格**不是**本件的正主（`K-P5g` 现打过它会退化成布尔谓词），
        //    写在这里只为钉住「两支没接反」。
        let sid = "0198f0d2-1111-4222-8333-444455556666";
        let token_r = launch_local(
            &LocalPsAction::Resume(sid.to_string()),
            None,
            None,
            Some(&account),
            None,
        )
        .expect("这一趟不该失败");
        assert_eq!(
            token_r, sid,
            "resume 那一支交回来的不是 sid —— 两支的返回值接反了"
        );

        // ④ ★★ **additive**：送出去的那一串逐字节 = 「身份那一句 + 基准串」。
        //    两边都由**生产函数现算**，本条不抄第二份拼装规则 ——
        //    抄一份的话，改了生产那一行、判据跟着抄错，两边一起错还全绿。
        {
            let act = LocalPsAction::Resume(sid.to_string());
            #[cfg(windows)]
            let base = build_local_ps_command(&act, None, Some(&account))
                .expect("基准串算不出来，④ 这一格是空真");
            #[cfg(not(windows))]
            let base = build_local_posix_command(&act, None, Some(&account))
                .expect("基准串算不出来，④ 这一格是空真");
            let want = format!("{}{base}", launch_identity_env_prefix(&token_r, false));
            let got = last_sent();
            // 反空真：基准串不是空的（空的话下面那条就退化成「送出去的等于身份那一句」）。
            assert!(
                base.len() > 10,
                "基准串只有 {} 字节 —— ④ 这一格在拿一个空壳对拍",
                base.len()
            );
            assert_eq!(
                got, want,
                "\n★★ **「把 token 交出来」这一拍改动了拼出来的命令串** —— \
                 而 `KP5HD1` 逐字要求「拼出来的命令串一个字节没变」。\n\
                 additive 的全部含义就是这一行；两边都是生产函数现算的，\n\
                 对不上说明拼装那一行被动过（多拼 / 少拼 / 换序，`D6` 刀 `Y1` 那一族）。"
            );
        }

        // ⑤ 拉起**失败**时不回 token —— 回了会让调用方去等一条根本不存在的会话。
        let _boom = override_launch_sink(LaunchSink(boom));
        let failed = launch_local(&LocalPsAction::New, None, None, Some(&account), None);
        assert!(
            failed.is_err(),
            "拉起失败了却回了 `Ok` —— 失败被吞掉，调用方会去等一条不存在的会话：{failed:?}"
        );
    }

    // ═════════════════════════════════════════════════════════════════════════
    // 🔴🔴 `D8 阻-1`：**账号传递链那三跳** —— 生产主路，第九轮之前一格判据都没有
    // ═════════════════════════════════════════════════════════════════════════
    //
    // # 病是怎么长出来的（`D8 §4` 第 2 条，别只读结论）
    //
    // 本件所有承重的行为判据（[`the_relay_prefix_is_really_prepended_to_the_command_that_gets_launched`]
    // / [`the_launch_side_really_asks_those_two_take_points_and_uses_their_answers`]）的
    // **驱动入口都是 [`launch_local`] 或更下游**，而**生产入口在它上面两跳**：
    //
    // ```text
    // #[tauri::command] resume_history_session  →  resume_impl  →  launch_local
    // #[tauri::command] new_local_session       ─────────────────→  launch_local
    // ```
    //
    // ⇒ **判据的射程上界正好卡在 `launch_local`，而九轮买的那些牙全长在它的下游。**
    // `D8` 三刀实打（三处调用点各写一次 `account.filter(|_| false)`）：
    // **全量门禁四个数一格不动（`1328 / 一致 / 493 / 1512`）、`GATE: OK`，而中转前缀恒空。**
    //
    // 🔴 **而看起来在守它的那把尺子，作用域对不上事实**：`tests/ipc/commands.vitest.ts:402`
    //「每一处起本机会话的调用都带 `account`」守的是 **TS 那一侧**（`D8` 的 `E4` 实测：
    // 在前端调用点上下同一形状的刀，`npm` 那道门当场红）。
    // **同一根链的 Rust 这一侧三跳，一格都没守。** —— 「两条路只修了一条」。
    //
    // # 为什么是**三条**判据而不是一条（`K22` 的口径）
    //
    // **N 支信号要 N 个只由这一支挡住的探针。** 三跳各自能独立答错，所以三刀的**红名单
    // 必须两两不同**（本轮实打的三张红名单在件文件 `§12` 的变异表里逐行给了）：
    //
    // | 刀 | 落在哪一处 | 红名单 |
    // |---|---|---|
    // | ① | `resume_impl` 里那一处 `account` | 探针 ① **与** ② 一起红 |
    // | ② | `resume_history_session` 里那一处 `account.as_ref()` | **只有**探针 ② 红 |
    // | ③ | `new_local_session` 里那一处 `account.as_ref()` | **只有**探针 ③ 红 |
    //
    // ⚠ 探针 ② 在刀 ① 上也红，是**链的包含关系**（②的驱动路径经过①），不是重复计数：
    // 三张红名单两两不同 ⇒ 三刀可区分 ⇒ **三格**。反过来只写一条探针 ② 的话，
    // ①②两刀的红名单会相同 ⇒ 那才是「N 支只买了一格」。
    //
    // # ⚠ 它们**买不到**什么（如实写）
    //
    // - **前端到底传没传 `account`**：那是 `commands.vitest.ts:402` 那把尺子的面，本族只管
    //   「传进来之后 Rust 这一侧有没有原样送到拼前缀那一行」。**两把尺子各守一侧，别只改一处。**
    // - **`launch_local` 以下的任何一格**：那是上面那条 `…_is_really_prepended_…` 的面。
    //   本族刻意**不**重复买它 —— 三条探针的反空真只断「不走中转那一趟长得像一条真拉起」。
    // - **Windows 那条腿**：`PRODUCTION_LAUNCH_SINK` 在 Windows 上是另一个送法，
    //   而本族喂的是记账替身 ⇒ **送法**那一格三条探针一格都驱动不到（登记，不假装）。
    //   〔09-09 订正：这里原写「而门禁跑在 Linux ⇒ 本族与本文件其余判据同样只驱动
    //    POSIX 那一支」—— **那半句今天是假话**。云端 `Rust lint + test` 跑在
    //    windows-latest，而 `launch_local` 里 `base` 那一格是 `#[cfg(windows)]` 选的
    //    ⇒ 本族在 CI 上驱动的是 **PowerShell** 那一支，在开发机上才是 POSIX 那一支。〕

    thread_local! {
        /// `D8 阻-1` 三支探针共用的记账台：这一趟真正交出去的 `(命令串, cwd)`。
        /// **线程局部** ⇒ 三条判据并行跑互不干扰（`cargo test` 一测一线程）。
        static ENTRY_SENT: std::cell::RefCell<Vec<(String, Option<String>)>> =
            const { std::cell::RefCell::new(Vec::new()) };
        /// 这一拍中转表里有哪几行（探针自己写）。
        static ENTRY_ROWS: std::cell::RefCell<Vec<String>> =
            const { std::cell::RefCell::new(Vec::new()) };
    }

    fn entry_recorder(cmd: &str, cwd: Option<&str>) -> Result<(), String> {
        ENTRY_SENT.with(|v| {
            v.borrow_mut()
                .push((cmd.to_string(), cwd.map(str::to_string)))
        });
        Ok(())
    }
    fn entry_rows() -> Vec<String> {
        ENTRY_ROWS.with(|v| v.borrow().clone())
    }
    fn entry_running() -> bool {
        true
    }
    /// 装台：一条会记账的送法 + 一组答案由探针写死的取值口。两个守卫掉出作用域自动还原。
    fn entry_stage() -> (LaunchSinkGuard, RelayFactsGuard) {
        ENTRY_SENT.with(|v| v.borrow_mut().clear());
        ENTRY_ROWS.with(|v| v.borrow_mut().clear());
        (
            override_launch_sink(LaunchSink(entry_recorder)),
            override_relay_facts(RelayFactSources {
                rows: entry_rows,
                running: entry_running,
                // 平台那一格照生产那个取值口（翻它的是别处那条判据的第 ④ 格）。
                windows: platform_is_windows,
            }),
        )
    }
    fn entry_answer(rows: &[&str]) {
        ENTRY_ROWS.with(|v| *v.borrow_mut() = rows.iter().map(|s| s.to_string()).collect());
    }
    fn entry_last() -> (String, Option<String>) {
        ENTRY_SENT.with(|v| v.borrow().last().cloned().expect("这一趟什么都没送出去"))
    }

    /// ★★ **探针 ①**〔`D8 阻-1`，`KH2B1`〕：`resume_impl` 这一跳把 `account` / `session_id` / `cwd`
    /// **原样**交给 [`launch_local`]。
    ///
    /// 刀 `D8P32`（`resume_impl` 里那一行 `account,` 写成 `account.filter(|_| false)`）
    /// 在本条落地之前是**全量门禁四个数一格不动**的。
    /// ⚠ 行号带尖号（`D8 阻-5` 的纪律）：`c02d954` 上是 `:1826`，本尖上是 `:1856`；
    /// **锚点用文本别用行号** —— `^        account,$` 在本文件全文恰好 **1** 处。
    #[test]
    fn the_resume_hop_above_launch_local_carries_the_account_and_the_sid_through() {
        let (_sink, _facts) = entry_stage();
        let dir = "/h/.claude-accts/acct-r1";
        let account = LaunchAccount::Named {
            config_dir: dir.to_string(),
            name: None,
        };
        // 中性名（`brief` 12：断言用的子串不许取自夹具名字里带含义的那半）。
        let cwd = "/p/one";

        // ① 反空真 —— 表里没有这个号 ⇒ 这条入口本来就该送出一条**不带中转注入**的命令，
        //    而且它得像一条真的本机拉起（这个号的 configDir 在里面）。
        //    没有这一格，下面那条「有前缀」的断言在「整条链恒空」时会读成假红/假绿。
        entry_answer(&[]);
        resume_impl("sid-r1", cwd, None, Some(&account), None).expect("不走中转这一趟不该失败");
        let (bare, bare_cwd) = entry_last();
        assert!(
            bare.contains(dir),
            "基准串里连这个号的 configDir 都没有 —— 这条入口根本没把账号送下去：{bare:?}"
        );
        assert!(
            !bare.contains("ANTHROPIC_BASE_URL"),
            "表里没有这个号，送出去的那一串却带着中转注入：{bare:?}"
        );
        assert_eq!(
            bare_cwd.as_deref(),
            Some(cwd),
            "这一跳把 `cwd` 弄丢了 —— 会话会起在默认目录上，而 toast 照报成功"
        );

        // ② 表里有这个号 ⇒ 送出去的那一串带中转注入，账号段与 sid 段都是**这一发**的。
        entry_answer(&["acct-r1"]);
        resume_impl("sid-r1", cwd, None, Some(&account), None).expect("走中转这一趟不该失败");
        let (routed, _) = entry_last();
        assert!(
            routed.contains("ANTHROPIC_BASE_URL"),
            "\n★★ **`resume_impl` 这一跳把账号扔了** —— 刀 `D8P32` 的形状：\n\
             `launch_local(…, account.filter(|_| false), …)`。\n\
             生产后果：历史页 resume 一个 api-key 号 ⇒ claude 直连官方端点、第三方 key 用不上，\n\
             而在 `D8` 实测里**全量门禁四个数一格不动**。实得 = {routed:?}"
        );
        assert!(
            routed.contains(&format!("/{}/", "acct-r1")),
            "路由键里的账号段不是这一发的号：{routed:?}"
        );
        assert!(
            routed.contains("/sid-r1'"),
            "路由键里的 `<key>` 段不是这一发的 sid —— `session_id` 在这一跳被换掉了：{routed:?}"
        );
        // 逐字节：前缀 + 基准串。剥掉第一段之后剩下的**必须**逐字节等于 ① 那趟的基准串
        // —— 「多注入一个前缀」与「顺手把命令体也换了」在只断 `contains` 的判据上同形。
        let (head, tail) = routed.split_once("; ").expect("走中转那一趟没有前缀段");
        // ⚠ **09-09 订正（云端 windows-latest 首跑逮到）**：`entry_stage` 的平台那一格
        //   照**生产取值口**（真答案），所以在 Windows 上中转前缀**真的**渲成 PS 形态
        //   `$env:ANTHROPIC_BASE_URL='…'`，而这里原来把 POSIX 那一种写死了。
        //   ⇒ 按**这台机器**算出该有的那一种，各断各的。
        //   🔴 **不是「两种都放行」** —— 那会把「渲错了平台形态」这一刀松掉
        //   （`D6` 刀 `Xb` 的正主）。现在两个平台各自只放行一种：
        //   Linux 上渲成 `$env:` 照样红，Windows 上渲成 `export` 也照样红。
        let want_head = if platform_is_windows() {
            "$env:ANTHROPIC_BASE_URL="
        } else {
            "export ANTHROPIC_BASE_URL="
        };
        assert!(
            head.starts_with(want_head),
            "第一段不是中转注入（这台机器该渲成 {want_head:?}）：{head:?}"
        );
        assert_eq!(
            tail, bare,
            "剥掉中转前缀之后的命令体与不走中转那一趟不一样 —— 这一跳除了账号还动了别的"
        );
    }

    /// ★★ **探针 ②**〔`D8 阻-1`，`KH2B1`〕：**前端真正调的那条命令**
    /// [`resume_history_session`]（`#[tauri::command]`）把五个入参原样交给 [`resume_impl`]。
    ///
    /// 刀 `D8P32b`（[`resume_history_session`] 里那一行 `account.as_ref(),` 写成
    /// `account.as_ref().filter(|_| false)`）在本条落地之前是**全量门禁四个数一格不动**的。
    /// ⚠ 行号带尖号（`D8 阻-5`）：`c02d954` 与本尖上都是 `:892`（本轮加的行都在它下面）。
    ///
    /// ⚠ 第 ② 格是**对拍**（同一组输入喂两条入口，两串必须逐字节相同）——
    /// 它买的是「这一跳一个入参都没被换掉」，比只断「有前缀」宽一格：
    /// 换掉 `launcher` / `tmux_name` / `cwd` 中任何一个，这一格也红。
    #[test]
    fn the_resume_command_the_frontend_calls_hands_all_five_arguments_down_unchanged() {
        let (_sink, _facts) = entry_stage();
        let dir = "/h/.claude-accts/acct-r2";
        let cwd = "/p/two";
        entry_answer(&["acct-r2"]);

        // ① 从**那条 `#[tauri::command]`** 进去。
        resume_history_session(
            "sid-r2".to_string(),
            cwd.to_string(),
            Some("cc".to_string()),
            Some(LaunchAccount::Named {
                config_dir: dir.to_string(),
                name: None,
            }),
            None,
        )
        .expect("这一趟不该失败");
        let (from_cmd, cmd_cwd) = entry_last();
        assert!(
            from_cmd.contains("ANTHROPIC_BASE_URL") && from_cmd.contains("/acct-r2/"),
            "\n★★ **那条 `#[tauri::command]` 把账号扔了** —— 刀 `D8P32b` 的形状：\n\
             `resume_impl(…, account.as_ref().filter(|_| false), …)`。\n\
             这一跳就是**历史页 resume 那个按钮真正调的那条命令**，\n\
             而在 `D8` 实测里一刀下去**全量门禁四个数一格不动**。实得 = {from_cmd:?}"
        );
        assert!(
            from_cmd.contains("/sid-r2'"),
            "路由键里的 `<key>` 段不是这一发的 sid：{from_cmd:?}"
        );

        // ② 对拍：同一组输入直接喂下一跳，两串必须**逐字节相同**。
        //    ⇒ 这一跳换掉五个入参里的**任何一个**（不只是 `account`），本格都红。
        let account = LaunchAccount::Named {
            config_dir: dir.to_string(),
            name: None,
        };
        resume_impl("sid-r2", cwd, Some("cc"), Some(&account), None).expect("这一趟不该失败");
        let (from_impl, impl_cwd) = entry_last();
        assert_eq!(
            from_cmd, from_impl,
            "\n那条 `#[tauri::command]` 与它下一跳送出去的不是同一串 —— \
             这一跳换掉了某个入参（不一定是 `account`）"
        );
        assert_eq!(cmd_cwd, impl_cwd, "`cwd` 在这一跳被换掉了");
    }

    /// ★★ **探针 ③**〔`D8 阻-1`，`KH2B1`〕：**「在该目录起新会话」那条命令**
    /// [`new_local_session`]（`#[tauri::command]`）把 `account` / `cwd` 原样交给 [`launch_local`]。
    ///
    /// 刀 `D8P33`（[`new_local_session`] 里那一行 `account.as_ref(),` 写成
    /// `account.as_ref().filter(|_| false)`）在本条落地之前是**全量门禁四个数一格不动**的。
    /// ⚠ 行号带尖号（`D8 阻-5`）：`c02d954` 上是 `:1871`，本尖上是 `:1901`。
    ///
    /// ⚠ 这条路**没有 sid**（`<key>` 段走 nonce，见 `payload::relay_key_for` 的表），
    /// 所以本条只钉账号段与 `cwd`；`<key>` 那一维归 `payload` 那一侧的判据。
    #[test]
    fn the_new_session_command_the_frontend_calls_carries_the_account_and_the_cwd_through() {
        let (_sink, _facts) = entry_stage();
        let tmp = TmpDir::new(); // `new_local_session` 会先核 `cwd` 是不是现存目录
        let cwd = tmp.0.to_string_lossy().into_owned();
        let dir = "/h/.claude-accts/acct-r3";

        // ① 反空真：表里没有这个号 ⇒ 不带中转注入，但 configDir 与 cwd 都得走到。
        entry_answer(&[]);
        new_local_session(
            cwd.clone(),
            None,
            Some(LaunchAccount::Named {
                config_dir: dir.to_string(),
                name: None,
            }),
        )
        .expect("不走中转这一趟不该失败");
        let (bare, bare_cwd) = entry_last();
        assert!(
            bare.contains(dir),
            "基准串里连这个号的 configDir 都没有：{bare:?}"
        );
        assert!(
            !bare.contains("ANTHROPIC_BASE_URL"),
            "表里没有这个号却带着中转注入：{bare:?}"
        );
        assert_eq!(
            bare_cwd.as_deref(),
            Some(cwd.as_str()),
            "这一跳把 `cwd` 弄丢了 —— 「在该目录起新会话」会起到别的目录去"
        );

        // ② 表里有这个号 ⇒ 带中转注入，且账号段是这一发的号。
        entry_answer(&["acct-r3"]);
        new_local_session(
            cwd.clone(),
            None,
            Some(LaunchAccount::Named {
                config_dir: dir.to_string(),
                name: None,
            }),
        )
        .expect("走中转这一趟不该失败");
        let (routed, routed_cwd) = entry_last();
        assert!(
            routed.contains("ANTHROPIC_BASE_URL") && routed.contains("/acct-r3/"),
            "\n★★ **「在该目录起新会话」那条命令把账号扔了** —— 刀 `D8P33` 的形状：\n\
             `launch_local(…, account.as_ref().filter(|_| false), …)`。\n\
             生产后果：一个 api-key 号**新开**会话 ⇒ 直连官方端点，\n\
             而在 `D8` 实测里一刀下去**全量门禁四个数一格不动**。实得 = {routed:?}"
        );
        assert_eq!(
            routed_cwd.as_deref(),
            Some(cwd.as_str()),
            "走中转这一趟把 `cwd` 弄丢了"
        );
        assert_ne!(
            bare, routed,
            "两趟送出去的是同一串 —— 「表里有没有这一行」这一维在这条入口上成了常量"
        );
    }
}
