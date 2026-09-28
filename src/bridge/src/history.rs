//! 历史会话的 monitor 这一侧：读一整份会话（按块经 Channel 发给查看器）· 删一份会话（经那台机器的后端）·
//! resume / 起新会话的命令渲染 · **F62 从某轮建分支**。
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
//! 用户明确选了「物理删除」。前端二次确认后调 `delete_history_session`，由那台机器的后端删（`files-delete-session`）；
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

// 〔RW1 · 第四波 · 2026-09-24〕这里原来是本机删除的路径守卫 `validate_delete_target`〔散文墓碑〕（Batch4-F15：
// canonicalize 两边、`..` 与 symlink 穿越都拒）与本进程那一次 `fs::remove_file`。用户裁「只允许后端的文件管理部分
// 写文件」也管本机 ⇒ 删历史会话改成那台机器后端的**一条明确的命令** `files-delete-session`（**只收 sid**；
// 〔AR1 · V119〕当时说它是「会话文件围栏唯一的例外」，FN1 之后写面已无那道围栏，这是它自己的限制），落点由后端按 sid 在它自己的记录树里找、解到底必须恰是 `<项目>/<sid>.jsonl`
// （`src/backend/agents/claudecode/paths.rs::session_file_for_delete`）⇒ 那道路径守卫的活由后端干了，本机这一份零调用方、删了。

/// 🔴 **〔步 12·C 2026-09-20〕本机 ＋ 远端两条删除合成了一条带 `origin` 的。**
///
/// **凭什么说它们是同一件事**：两侧都是「用户显式删掉一份会话 jsonl，然后清掉本机
/// 按 sid 存的那份注解」。后半句**本来就只有一份实现** —— 〔C4d〕今天是本机常驻后端的 `history-forget`
/// （注解的读写者换成了它；界面删成功之后交，`src/history-reads.ts::forgetAnnotation`；当年这里是 `remove_metadata_entry`〔散文墓碑〕）。
///
/// 🔴 **〔RW1 · 第四波 · 2026-09-24〕前半句也只剩一份了**：两侧都经那台机器的后端
/// （[`delete_via_backend`] → `files-delete-session`，只收 sid），本机不再直删、远端不再 SFTP 直删。
/// `jsonl_path` 仍然收：它是**一致性闸**的另一半 —— 前端送来的 `session_id` 必须恰是那份文件名的 stem，
/// 对不上就一个字节不动（从前远端那一支「不信前端的 sid、自己从路径算」要防的「删 A 的文件、清 B 的注解」，
/// 今天由这一闸在两侧同时防：删的是 sid 那一份，清的也是 sid 那一条，而 sid 必须就是界面上那一行的文件名）。
#[tauri::command]
pub async fn delete_history_session(
    origin: crate::origin::Origin,
    session_id: String,
    jsonl_path: String,
) -> Result<(), String> {
    if let crate::origin::Route::Remote(host) = origin.route("delete_history_session")? {
        return crate::remote_history::delete_remote_history_session(host, session_id, jsonl_path)
            .await;
    }
    let door = crate::user_files::BackendDoor::new(crate::origin::Origin::local());
    delete_via_backend(&door, &session_id, &jsonl_path).await
}

/// 删一份历史会话（经门）。**两侧共用这一份**（远端那一支只是门开在那台机器上）。
/// 〔C4d〕清注解那一半不在这里了：注解归本机常驻后端，界面删成功之后交 `history-forget`。
pub(crate) async fn delete_via_backend(
    door: &impl crate::user_files::Door,
    session_id: &str,
    jsonl_path: &str,
) -> Result<(), String> {
    match crate::remote_history::jsonl_stem(&jsonl_path.replace('\\', "/")) {
        Some(stem) if stem == session_id => {}
        other => {
            return Err(copy_text(
                "rsHistory.delete.idMismatch",
                &[
                    ("sessionId", &session_id.to_string()),
                    (
                        "fileName",
                        &(other
                            .as_deref()
                            .unwrap_or(&copy_text("rsHistory.delete.notSessionFile", &[])))
                        .to_string(),
                    ),
                ],
            ))
        }
    }
    let gone = door.delete_session(session_id).await?;
    tracing::info!("history: {} 上删掉了 {gone}", door.machine());
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
/// **`Deserialize` 是给远端那条路用的**（G6）：backend 的 `--fork-session` 在 stdout 吐同形 JSON，
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

// 〔RW1 · 第四波 · 2026-09-24〕这里原来是本机分叉读源会话那一格 `read_jsonl_values`〔散文墓碑〕与
// `branch_core::build_branch_records` 的引入（记录变换）。本机分叉改成 exec 本机后端 `--fork-session`，
// 读源 · 变换 · `O_EXCL` 落盘三样全在后端那一份（`control/fork_write.rs`）⇒ 本文件零调用方、删了。

/// F62 IPC：从历史会话的某条消息创建分支。前端点消息卡上的 `⑂` 时调，成功返回新 sid。
/// 见本段顶部大注释（§1 正交、原生格式、守卫）。〔RW1〕薄壳：按 origin 交给那台机器的后端 `--fork-session`。
///
/// 🔴〔`K-R88` 09-13〕**入参从路径改成了 sid**，与远端那条
/// （`remote_branch::create_remote_branch_session`）**形状一致**。
/// 前端两条路本来就都拿得到 sid（按钮那份上下文里一直有），所以这不是给调用方加负担。
///
/// 🔴 **〔步 12·C 2026-09-20〕两条合成了一条带 `origin` 的。**
///
/// **这一对是本批里最便宜的一对，而便宜的理由是前人已经把贵的那部分做完了**：
/// `K-R88`（09-13）把两侧的入参统一成了 sid，`G1` 把记录变换提成了共享 crate
/// `branch-core`，`G6` 让远端那条路吐**同一个** [`BranchResult`]。
/// 被合并掉的那条命令自己的头注逐字写着「与本地那条的差异**今天只剩一处：活儿在远端干**」
/// —— 那句话就是本次合并的判据，不是我新造的。
///
/// ⇒ 合并之后「活儿在哪干」由 `origin` 说，不再由**命令名**说。
/// ⚠ 本机 ＝ `Origin::local()`（线上 `"<local>"`），**不是 `null`**。
#[tauri::command]
pub async fn create_branch_session(
    origin: crate::origin::Origin,
    source_session_id: String,
    message_uuid: String,
) -> Result<BranchResult, String> {
    match origin.route("create_branch_session")? {
        // 〔RW1 · 第四波 09-24〕本机那一支也交给后端写（exec 本机后端 `--fork-session`），本进程一个字节不写。
        crate::origin::Route::Local => {
            crate::remote_branch::create_local_branch_session(&source_session_id, &message_uuid)
                .await
        }
        crate::origin::Route::Remote(host) => {
            crate::remote_branch::create_remote_branch_session(
                host,
                &source_session_id,
                &message_uuid,
            )
            .await
        }
    }
}

// 〔RW1 · 第四波 · 2026-09-24〕这里原来是本机建分支的核心 `branch_impl`〔散文墓碑〕与它的落盘
// `write_branch_file`〔散文墓碑〕（`O_EXCL` 在本进程里写 `~/.claude/projects/<proj>/<new-sid>.jsonl`）。
// 用户裁「只允许后端的文件管理部分写文件」也管本机 ⇒ 本机分叉与远端同一条路：exec 本机后端的
// `--fork-session`（`src/backend/control/fork_write.rs`，写盘白名单层那一处 `O_EXCL`），
// 结果解释与远端共用 `remote_branch::interpret_fork_exec`〔散文墓碑〕（〔LOC1a〕随 exec 那条路一起删了，今天是帧命令 `session-fork`）⇒ 两件零调用方、删了。

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
