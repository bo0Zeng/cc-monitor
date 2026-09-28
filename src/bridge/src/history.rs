//! 历史会话的 monitor 这一侧：读一整份会话（按块经 Channel 发给查看器）· resume / 起新会话的命令渲染。
//! 〔MIG-3b〕删一份会话与 **F62 从某轮建分支** 不在这里了：界面经通道直说那台机器的后端（`src/session-writes.ts`）。
//!
//! ## 〔C4d · 第四波 4B〕清单与注解不在这里了
//!
//! 项目 / 会话清单与注解（星标 / 重命名 / 隐藏 / 上次账号）搬进了**本机常驻后端**（历史跨机 join 的唯一的家：
//! 后端 `history_join.rs` · `history_annotations.rs`），界面经通道问（`src/history-reads.ts`）。这里原先的「两级懒加载」
//! （`list_history_projects`〔散文墓碑〕 · `stream_history_sessions_in_project`〔散文墓碑〕）与注解读写一起删了。
//! 注解那份文件**原地不动**（`<monitor_data_dir>/history-metadata.json`，路径仍由本文件 [`metadata_path`] 算，
//! 起本机后端时交过去）—— star / 重命名 / 隐藏仍然**不改 jsonl**（Claude Code 的数据保持零侵入）。
//!
//! ## 物理删除
//!
//! 用户明确选了「物理删除」。前端二次确认后经通道直说那台机器的后端 `files-delete-session`（〔MIG-3b〕monitor 那条转交删了）；
//! 删完那条注解由界面交本机后端 `history-forget` 连带删。Claude Code 自己也不再能 resume 这个会话。

use crate::copy_table::copy_text;
use crate::messages::JsonlRecord;
use crate::paths;
use serde::Serialize;
use std::path::PathBuf;

// === 〔C4d · 第四波 4B〕历史清单与注解搬进了本机常驻后端 ===
//
// 主会话 09-25 裁（`调研/第四波记录/C4d.md`「主会话裁」第 2 条）：注解（`history-metadata.json`：星标 / 改名 / 隐藏 / 上次账号）的
// **读写者**换成本机常驻后端（文件原地不动：路径由本文件 [`metadata_path`] 算、起本机后端时交过去），它经 `remote_ask` 问远端那台、
// 并上注解、出成品；前端经 `chan.call`（`src/history-reads.ts`）。这里原先那一族随之删了〔散文墓碑〕：
// 两个线上形状（`HistoryProject` / `HistorySessionEntry`）· 注解三件（`HistoryMetadata` / `EntryMetadata` / `MetadataPatch`）·
// 本机项目清单（`list_history_projects` / `local_projects_via`〔散文墓碑〕· 判活绑定 `SessionMapLiveness`〔散文墓碑〕）·
// Codex 合成（`enumerate_codex_sessions` / `codex_projects_from` / `codex_session_entry` / `codex_first_user_excerpt`〔散文墓碑〕）·
// 展开一个项目（`stream_history_sessions_in_project` 与它的 `analyze_jsonl`〔散文墓碑〕一族）· 改注解 / 上次账号表那两条命令。
// 判定逐格搬进后端（`history_join.rs` · `history_annotations.rs` · `agents/codex/history.rs`），判据跟着搬；
// 「迁移前后读出来的注解逐条相等」由结构占位夹具 `tests/__fixtures__/history-metadata.fixture.json` ＋ 旧读者产出的金样
// `tests/__fixtures__/history-metadata.readout.golden.json` 钉着。

/// 读一整份会话的总量上限（从 `remote_history.rs` 搬来，〔LOC1b · 4D〕本机远端同一条）。
///
/// ⚠〔audit-0805 F06〕实测本机最大会话 **270,103,105 字节 / 92,967 行**（就是那次审计对话本身），
/// 已经**越过** 256 MiB 这条线 1,667,649 字节；57 MB 以上的会话有 5 个，不是孤例。
/// ⇒ 上限**会被真实数据打到**，所以「打到之后怎么办」不能是静默（[`session_truncated_message`]）。
/// 〔LOC1b · 4D〕本机从前没有这条上限（monitor 自己 `File::open` 读到底）；本机冷读改走本机后端之后与远端同一条
/// —— 主会话 09-25 认可「本机从此也受，超了明说」。
const MAX_SESSION_BYTES: u64 = 256 * 1024 * 1024;

/// 超限时给用户的话〔audit-0805 F06，定框 **E4/E5**〕。
///
/// 此前（远端那一支还是一条 SSH 流时）读法是 `take(MAX)` ＋ `if n == 0 { break; }` —— 到限与正常 EOF
/// **完全同形** ⇒ 前端拿到一份「看起来完整」的历史，而后面的内容**无声消失**；
/// 同一份数据走后端的 `--fork-session` 却会**硬报错** —— 正是定框 **E5** 要消灭的「同一份数据走不同路得到不同答案」。
/// 抽成纯函数是为了让它可判据。字住文案表（CP2c 抽过：键原是 `rsRemoteHistory.session.truncated`）；
/// 〔LOC1b〕键随函数搬进本文件改名 `rsHistory.session.truncated`，「仍在远端」→「仍在那台机器上」（本机也走这一句）。
fn session_truncated_message(read_bytes: u64, lines_shown: u32) -> String {
    copy_text(
        "rsHistory.session.truncated",
        &[
            ("max", &MAX_SESSION_BYTES.to_string()),
            ("read", &read_bytes.to_string()),
            ("lines", &lines_shown.to_string()),
        ],
    )
}

/// 一页原文 ⇒ 这一页里可显示的那几条（占号在先、过滤在后），带 per-file `seq`。**纯**：判据直接喂页。
///
/// 〔LOC1b · 4D〕本机远端同一份（从前两侧各写一遍循环体，靠注释「逐字对齐」）。
/// agent 种类按**文件名形态**判（[`crate::adapter::kind_of_record_name`]）：远端路径也判得对，不依赖本机有没有那一家的根。
pub(crate) struct SessionPager {
    kind: crate::adapter::AgentKind,
    session_id: String,
    path: String,
    payload_origin: Option<String>,
    parse_origin: crate::origin::Origin,
    numberer: crate::session_skeleton::LineNumberer,
    cwd_seen: Option<String>,
}

impl SessionPager {
    /// `origin` 是这一份从哪台读来的；载荷里的 `origin` 本机 `None`、远端 `Some(名字)`
    /// （这处不对称是载荷那一层的事，`JsonlLinePayload::origin`，不在本路射程）。
    pub(crate) fn new(origin: &crate::origin::Origin, jsonl_path: &str) -> Self {
        let p = std::path::Path::new(jsonl_path);
        let kind = crate::adapter::kind_of_record_name(p);
        let session_id =
            crate::adapter::session_id_from_path_with(crate::adapter::for_kind(kind).layout(), p)
                .unwrap_or_default();
        Self {
            kind,
            session_id,
            path: jsonl_path.to_string(),
            payload_origin: origin.host_name().map(str::to_string),
            parse_origin: origin.clone(),
            numberer: crate::session_skeleton::LineNumberer::default(),
            cwd_seen: None,
        }
    }

    pub(crate) fn session_id(&self) -> &str {
        &self.session_id
    }

    /// 喂一页（切在行尾，末页可含残尾）。
    pub(crate) fn page(&mut self, text: &str) -> Vec<crate::bridge::JsonlLinePayload> {
        let mut out = Vec::new();
        for line in text.lines() {
            // 〔U3b〕seq = **可计行号**（与 watcher / 骨架索引同一个空间）：不可显示的记录照占号、不出 payload。
            // 「占不占号」只有一个住址：`session_skeleton·rs::LineNumberer`。
            // 〔ST3〕看不懂的行记在读来的那台名下。
            let (kind, origin) = (self.kind, &self.parse_origin);
            let Some((seq, rec)) =
                crate::session_skeleton::numbered_displayable(&mut self.numberer, line, |b| {
                    crate::parser::parse_for_kind(kind, origin, b)
                })
            else {
                continue;
            };
            if let JsonlRecord::User { cwd, .. } = &rec {
                if self.cwd_seen.is_none() {
                    self.cwd_seen = cwd.clone();
                }
            }
            out.push(crate::bridge::JsonlLinePayload {
                session_id: self.session_id.clone(),
                cwd: self.cwd_seen.clone(),
                path: self.path.clone(),
                seq,
                origin: self.payload_origin.clone(),
                message: rec,
                skipped_from: None, // 〔RENDER2〕前端按这一段 `[from, next)` 整段记见过（取回路都是连着的一段）
            });
        }
        out
    }
}

/// issue #12: 流式读一整份会话（取代已删的非流式 `read_session_jsonl`；按 100 条一 chunk 边读边发，前端 ~500ms 内开始渲染首屏）。
///
/// 取消：前端 drop channel 时 send 返 Err → 停。
///
/// 🔴 **〔LOC1b · 第四波 4D〕本机与远端是同一条路**（`INVARIANTS §40`「本地＝不走 ssh 的远端」· `设计/00 §2.5 ①`）：
/// 都经**那台机器的后端**帧命令 `history-read` 按字节分页取原文（一页 ≤1 MiB、切在行尾），monitor 解析。
/// 从前本机那一支在这里自己 `File::open` 读 jsonl、自己验根（monitor 里的第二个会话读者，`local_read_surface_registry`
/// 的针只认 `claude_dir` 没数到它）；远端那一支住 `remote_history.rs`。两支合成一条之后，围栏归后端
/// （`observe/history_query.rs::validate_session_path`，Codex 的记录根也认），这里只留两侧同一道廉价预检。
/// 代价如实写：本机后端不在 ⇒ 本机会话也读不了（`D11` 的兑现，同本机实时内容）。
#[tauri::command]
pub async fn stream_read_session_jsonl(
    origin: crate::origin::Origin,
    jsonl_path: String,
    on_chunk: tauri::ipc::Channel<Vec<crate::bridge::JsonlLinePayload>>,
) -> Result<u32, String> {
    const CHUNK_SIZE: usize = 100;
    origin.route("stream_read_session_jsonl")?;
    // 廉价预检（纵深防御；真正的越界由那台后端的围栏兜底）：拒 `..` ＋ 必须 `.jsonl`。
    if jsonl_path.contains("..") || !jsonl_path.ends_with(".jsonl") {
        return Err(copy_text(
            "rsHistory.session.badPath",
            &[("path", &jsonl_path.to_string())],
        ));
    }
    let started = std::time::Instant::now();
    let mut pager = SessionPager::new(&origin, &jsonl_path);
    let mut read_bytes: u64 = 0;
    let mut chunk: Vec<crate::bridge::JsonlLinePayload> = Vec::with_capacity(CHUNK_SIZE);
    let mut total = 0u32;
    let mut offset: u64 = 0;
    // 〔DL1 · `设计/05 §3.3.2`〕读一整份是**一件事**：期限在读第一页之前造一次，每一页都拿同一个时刻去等（不重新计时）。
    //   大小事先不知道 ⇒ 按字节上限给（`frame_query::read_budget(MAX_SESSION_BYTES)`）。
    let deadline = crate::backend::control::frame_query::Deadline::within(
        crate::backend::control::frame_query::read_budget(MAX_SESSION_BYTES),
    );
    loop {
        let page = crate::backend::control::frame_query::read_page(
            &origin,
            &jsonl_path,
            offset,
            None,
            deadline,
        )
        .await?;
        read_bytes += page.next - offset;
        if read_bytes > MAX_SESSION_BYTES {
            // F06：**不许静默截断**。同一份数据走后端的 `--fork-session` 会硬报错，
            // 走这条路却假装读完了 —— 定框 E5 要的是「同一份数据走不同路得到同一个答案」。
            return Err(session_truncated_message(read_bytes, total));
        }
        for payload in pager.page(&page.text) {
            chunk.push(payload);
            total += 1;
            if chunk.len() >= CHUNK_SIZE {
                let full = std::mem::replace(&mut chunk, Vec::with_capacity(CHUNK_SIZE));
                if on_chunk.send(full).is_err() {
                    tracing::info!(
                        "stream_read_session_jsonl({}): 前端取消于 {total} 条",
                        pager.session_id()
                    );
                    return Ok(total);
                }
            }
        }
        offset = page.next;
        if page.eof {
            break;
        }
    }
    if !chunk.is_empty() {
        let _ = on_chunk.send(chunk);
    }
    tracing::info!(
        "stream_read_session_jsonl({}): {total} records in {}ms",
        pager.session_id(),
        started.elapsed().as_millis()
    );
    Ok(total)
}

// 〔MIG-3b · `设计/05 §14.3` C 组 · `§9` 第 12 条〕这里原来是删会话与分叉那两条 Tauri 命令、删会话那道 stem 一致性闸
// 与分叉结果的线上形状。
// 两件「改世界」的事本来就由那台后端做（`files-delete-session` 只收 sid · `session-fork`）；monitor 只剩转交 ⇒ 界面经通道直说
// （`src/session-writes.ts`），转交连同命令一起删。stem 闸是恒真的：会话行由后端 `analyze_session` 按文件名 stem 出 `sessionId`，
// 后端删之前自己再判一次「落点恰是 `<sid>.jsonl`」（`files_write.rs::fenced_session_file`）；删与清注解用的是同一个 sid。

// 🪦〔MIG-2 · `99 §2.1 ⑬`〕这里原来是本机起会话那一整条：三条 Tauri 命令 `resume_history_session` · `new_local_session` ·
// `render_local_attach`〔散文墓碑〕与它们的计划与渲染（F34 启动器白名单 · 账号三态前缀 · `ccm` 容器路 / 旧路 · 中转前缀
// `relay_endpoint_on`〔散文墓碑〕· 身份 token `CCM_LAUNCH_ID` · 三条测试缝）。判定搬进本机后端（帧命令 `launch-local` ·
// `launch-endpoint`，`src/backend/control/launch_render/local.rs`），界面经通道问；monitor 只剩开终端窗口（`launch.rs::open_local_terminal`）。

// === metadata 那份文件在哪（〔C4d〕读写者是本机常驻后端；路径仍由这里算）===

/// 历史注解那份文件的路径 —— **全仓只此一处算它**。〔C4d · 第四波 4B〕读写者换成了本机常驻后端（主会话 09-25 裁：
/// 文件留在原处、同一路径）：monitor 起本机后端时用 `CCM_HISTORY_METADATA` 把**本函数算出来的这一个**交过去
/// （`local_backend_host::relay_host_envs`），同一路径因此是构造出来的，不是两侧算法对齐出来的。
pub(crate) fn metadata_path() -> Option<PathBuf> {
    Some(paths::resolve_monitor_data_dir()?.join(creds_core::store::HISTORY_METADATA_FILE))
}

#[cfg(test)]
#[path = "../../../tests/bridge/history_title_coverage.rs"]
mod title_coverage;

#[cfg(test)]
#[path = "../../../tests/bridge/history_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "../../../tests/bridge/history_f06_tests.rs"]
mod f06_tests;
