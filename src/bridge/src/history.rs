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

use crate::messages::JsonlRecord;
use crate::paths;
use serde::Serialize;
use std::fs::File;
use std::io::{BufRead, BufReader};
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

/// issue #12: 流式版（取代已删的非流式 `read_session_jsonl`）。
///
/// 按 100 行一 chunk 边读边发，前端可在 ~500ms 内开始渲染首屏（即使整 jsonl
/// 上千条 / 10MB+）。
///
/// 取消：前端 drop channel 时 send 返 Err → break。
/// 🔴 **〔步 12·C 2026-09-20〕本机 ＋ 远端两条合成了一条带 `origin` 的。**
///
/// 这一对住 `真相源/97 §二 丙`（「措辞不同」那一档）：本机叫
/// `stream_read_session_jsonl`、远端叫 `stream_read_remote_session`，**名字里没有一个
/// 共同的词** ⇒ 按名字数分叉的量法看不见它。认出它靠的是两条实打的判据：
/// ① 两侧的 chunk 口径**逐字对齐**（每 100 条一发、同一个 `JsonlLinePayload`、
/// 同一套 per-file `seq`）；② 前端的 `SessionViewer` 对两条路**共用同一段消费代码**
/// （`session-viewer.ts` 里那个三目就是全部差别）。
#[tauri::command]
pub async fn stream_read_session_jsonl(
    origin: crate::origin::Origin,
    jsonl_path: String,
    on_chunk: tauri::ipc::Channel<Vec<crate::bridge::JsonlLinePayload>>,
) -> Result<u32, String> {
    const CHUNK_SIZE: usize = 100;
    if let crate::origin::Route::Remote(host) = origin.route("stream_read_session_jsonl")? {
        return crate::remote_history::stream_read_remote_session(jsonl_path, host, on_chunk).await;
    }
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
        // 〔U3b〕seq = **可计行号**（与 watcher / 骨架索引同一个空间）：不可显示的记录照占号、
        // 不出 payload。原先只给可显示的编号 ⇒ 查看器的 seq 与索引对不上、骨架接不上。
        // 「占不占号」只有一个住址：`session_skeleton·rs::LineNumberer`。
        let mut numberer = crate::session_skeleton::LineNumberer::default();

        for line in reader.lines().map_while(Result::ok) {
            let Some((seq, rec)) =
                crate::session_skeleton::numbered_displayable(&mut numberer, &line, |b| {
                    // 〔ST3〕这一支是 `route` 之后的本机那一支 ⇒ `origin` 就是本机。
                    crate::parser::parse_for_kind(kind, &origin, b)
                })
            else {
                continue;
            };
            if let JsonlRecord::User { cwd, .. } = &rec {
                if cwd_seen.is_none() {
                    cwd_seen = cwd.clone();
                }
            }
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

// 〔RW1 · 第四波 · 2026-09-24〕这里原来是本机删除的路径守卫 `validate_delete_target`〔散文墓碑〕（Batch4-F15：
// canonicalize 两边、`..` 与 symlink 穿越都拒）与本进程那一次 `fs::remove_file`。用户裁「只允许后端的文件管理部分
// 写文件」也管本机 ⇒ 删历史会话改成那台机器后端的**一条明确的命令** `files-delete-session`（会话文件围栏唯一的例外，
// **只收 sid**），落点由后端按 sid 在它自己的记录树里找、解到底必须恰是 `<项目>/<sid>.jsonl`
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
            return Err(format!(
                "拒绝删除：界面给的会话 id（{session_id}）与那份文件的名字（{}）对不上 —— 一个字节都没动",
                other.as_deref().unwrap_or("不是一份 .jsonl")
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
// 结果解释与远端共用 `remote_branch::interpret_fork_exec` ⇒ 两件零调用方、删了。

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

// 〔C4c · 第四波 4B〕「resume 之前问记录还在不在」那条 Tauri 命令（`probe_session_record` 与它的答案形状
//   `SessionRecordProbe`〔散文墓碑〕）退役：monitor 那一跳只是「转一条 `history-record`、核两格」，后端早已出成品 ⇒
//   界面经通道直接问（`src/session-reads.ts::probeSessionRecord`，本机与远端同一条路），这里一行都不留。

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
/// `R61` 立的就是这件事：**不许再用「backend」这个词把这两件事压平。**
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
        /// 推得出一个像样的名字（`cc-acct-iso` 的布局是 `~/.claude-alt/<名字>`，
        /// [`apikey_account_id_of_dir`] 就是那么推的），**但那两处的失效方向相反**：
        /// 推错一个 apikey 账号 id ⇒ 表里查不到 ⇒ 逐字节走旧路（保守）；推错一个 `--account`
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
/// Windows 的账号目录长成 `C:\Users\z\.claude-alt\z`，把 `\` 一律禁掉等于禁掉整个平台。
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
/// 于是 Windows 上一个真实账号目录（`C:\Users\z\.claude-alt\z`）**必被拒**，
/// 「本机分叉时选一个具名账号」在主平台上 100% 失败。判据照抄当年的 `local_accounts::looks_absolute`〔散文墓碑〕
/// （〔C4d〕那份参照实现已删；同一课今天住后端 `accounts_query.rs::is_safe_config_dir` 的「平台形式」那一半：
/// 照搬 `starts_with('/')` 会把每个 Windows 账号判成不安全）。
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

/// 🔴 `K-R55`（09-11）：**本机 ccm 探测的取值口** —— 与 [`InjectFactSources`] 是同一条缝的形状。
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
///   不是按文本 —— 理由与 [`PRODUCTION_INJECT_FACTS`] 那一条相同。
/// - **谁绕开这条缝直接调 [`crate::ccm_probe::probe_local_ccm`]**：今天没有人群闸数它
///   （`InjectFactSources` 那三个取值口有一道，住 `payload.rs`）。**登记，不假装钉住了。**
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
/// # ⚠ 本机 `launch` **不经 backend**，而本机 `kill` 经〔E 阶段全局审计 08-12，待决 `U13`〕
///
/// `backend_kill.rs::backend_kill` 那条本机 kill 走的是后端通道（P3 刀 2）；本函数**没有**。
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
///   （`src/backend/control/ccm/plan.rs`，backend 侧有判据真去驱动它）。
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
// `K-H2b` · 〔US1 · 第四波 4D〕注入侧：**问那台后端要成品，照成品执行**
// ═════════════════════════════════════════════════════════════════════════════
//
// 先前这里住着注入侧的三个判断（账号 id 从哪来 · 表里有没有它 · 中转在不在），两样事实 monitor 自己取
// （`apikey_rows`〔散文墓碑〕读凭据文件 · `local_backend_host::relay_running`〔散文墓碑〕连回环口），判断交 `payload::relay_endpoint_for`〔散文墓碑〕。
// 人群与后端装表那一步各算一份（B-decouple §2.1 必须拆 1）。今天：那台机器的后端出成品（帧命令 `launch-endpoint`，
// 上游选择 `accounts/upstream/endpoint.rs`，`设计/20 §3.2` 那张表的唯一实现），这里只**转交入参、执行成品**：
// 本机与远端同一条路（`INVARIANTS §40`）；远端「中转不在就起、有界等」那一截要定时器（后端零定时器）⇒ 留在这里。

/// 「一个 configDir 对应 apikey 表里哪个 id」。
///
/// 〔C4c · 第四波 4B〕**规则住 `acct-core`**（`acct_core::apikey_account_id_of_dir`）。〔US1〕本 crate 里只剩写 key 那一处
/// （`apikey_remote::send_key`）调它 —— 起会话那一侧与界面那一侧的「这个号在不在表里」都由那台后端答。
pub(crate) use acct_core::apikey_account_id_of_dir;

/// 起会话时写进中转路由键第 1 段的那个 agent 名（适配器的 `id()`；读它的是上游选择）。
/// 〔US1〕它随 `launch-endpoint` 的 `agent` 交给那台后端。
pub(crate) fn launch_agent_id() -> &'static str {
    crate::adapter::active().id()
}

/// 那台后端那条帧命令的名字（与 `src/backend/inbound.rs::COMMANDS` 同名）。
pub(crate) const CMD_LAUNCH_ENDPOINT: &str = "launch-endpoint";

/// 中转不在时成品说的处置（线上 `whenDown`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum WhenDown {
    /// `/s/`：拒绝起会话（非它不可）。
    Refuse,
    /// `/t/`：照旧直连（有它更好）。
    Direct,
}

/// 〔US1〕那台后端答的成品（`launch-endpoint`），**严格收**：四个键恒在、类型逐格核；对不上 ⇒ 「两端契约对不上」。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LaunchEndpoint {
    /// 注入的地址（不带钥匙）；`None` = 不注入。
    pub(crate) base_url: Option<String>,
    /// 那台机器上我们的中转在不在听。
    pub(crate) listening: bool,
    /// 中转不在时怎么办（`base_url` 为空时 `None`）。
    pub(crate) when_down: Option<WhenDown>,
    /// `/s/` 那一格的表 id（拒绝时点名用）。
    pub(crate) account: Option<String>,
}

/// `launch-endpoint` 的应答 → [`LaunchEndpoint`]。`where_` 只进报错那句话（哪台机器答的）。
pub(crate) fn launch_endpoint_from_wire(
    where_: &str,
    d: &serde_json::Value,
) -> Result<LaunchEndpoint, String> {
    use serde_json::Value;
    let bad = |what: &str| {
        format!("{where_}后端 `{CMD_LAUNCH_ENDPOINT}` 的应答形状不对：{what} —— 两端契约对不上")
    };
    let Some(o) = d.as_object() else {
        return Err(bad("不是对象"));
    };
    let mut keys: Vec<&str> = o.keys().map(String::as_str).collect();
    keys.sort_unstable();
    if keys != ["account", "baseUrl", "listening", "whenDown"] {
        return Err(bad(&format!("键是 {keys:?}")));
    }
    let opt_str = |k: &str| match &o[k] {
        Value::Null => Ok(None),
        Value::String(s) => Ok(Some(s.clone())),
        _ => Err(bad(&format!("`{k}` 不是字符串或 null"))),
    };
    let when_down = match &o["whenDown"] {
        Value::Null => None,
        Value::String(s) if s == "refuse" => Some(WhenDown::Refuse),
        Value::String(s) if s == "direct" => Some(WhenDown::Direct),
        _ => return Err(bad("`whenDown` 只认 refuse / direct / null")),
    };
    let ep = LaunchEndpoint {
        base_url: opt_str("baseUrl")?,
        listening: o["listening"]
            .as_bool()
            .ok_or_else(|| bad("`listening` 不是布尔"))?,
        when_down,
        account: opt_str("account")?,
    };
    if ep.base_url.is_some() != ep.when_down.is_some() {
        return Err(bad("`baseUrl` 与 `whenDown` 该同有同无"));
    }
    Ok(ep)
}

/// 线上 `account`（`launch-endpoint` 入参）：`{"kind":"named","configDir",…}` · `{"kind":"base"}` · `null`（没表态）。
fn account_wire(account: Option<&LaunchAccount>) -> serde_json::Value {
    match account {
        None => serde_json::Value::Null,
        Some(LaunchAccount::Base) => serde_json::json!({ "kind": "base" }),
        Some(LaunchAccount::Named { config_dir, name }) => {
            serde_json::json!({ "kind": "named", "configDir": config_dir, "name": name })
        }
    }
}

/// `launch-endpoint` 的入参。`key` 是这一发的流标签：resume ⇒ sid；新开 ⇒ 现铸的 nonce（与身份 token 同一份铸法）。
pub(crate) fn launch_endpoint_args(
    account: Option<&LaunchAccount>,
    sid: Option<&str>,
    all_sessions: bool,
) -> serde_json::Value {
    serde_json::json!({
        "agent": launch_agent_id(),
        "account": account_wire(account),
        "key": crate::backend::control::payload::route_key_for_session(sid),
        "allSessions": all_sessions,
    })
}

/// 纯函数半：成品给的地址（或不注入）→ 要拼上去的前缀。空串 = **不走中转**（逐字节旧路）。
fn relay_prefix_for(url: Option<&str>, windows: bool) -> String {
    match url {
        None => String::new(),
        Some(u) if windows => crate::backend::control::payload::relay_env_prefix_ps(u),
        Some(u) => crate::backend::control::payload::relay_env_prefix_posix(u),
    }
}

/// 「问那台后端要成品」这一跳的形状（缝里那一格）：`(远端主机名 | None = 本机, 入参)` → 应答 `data`。
pub(crate) type EndpointAsk = fn(
    Option<String>,
    serde_json::Value,
) -> std::pin::Pin<
    Box<dyn std::future::Future<Output = Result<serde_json::Value, String>> + Send>,
>;

/// 生产上那一跳：经那台机器那条长连接问 `launch-endpoint`（本机 ＝ `<local>` 那一条）。
fn ask_launch_endpoint(
    host: Option<String>,
    args: serde_json::Value,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<serde_json::Value, String>> + Send>>
{
    Box::pin(async move {
        let host =
            host.unwrap_or_else(|| crate::backend::control::inbound_client::LOCAL_ORIGIN.to_string());
        crate::apikey_remote::call(&host, CMD_LAUNCH_ENDPOINT, args).await
    })
}

/// `D5 阻-1`：起会话那一侧的几个「事实」的**取值口**，收成一条判据能替换的缝。
///
/// # 为什么非有这条缝不可（病史五层，别退回去）
///
/// 先前钉这几个入参的是**扫描型**判据（切一段源码看文本在不在）；`D5` 现打：文本留住、行为摘掉 ⇒ 全绿。
/// ⇒ 处置是**不再量文本**：事实一律从本结构取，判据换一份**会记账的替身**进来，断言两件事 ——
/// ㈠ 它**真的被问过**；㈡ 算出来的前缀**真的随替身给的答案变**（治「问完扔掉」那一形）。
///
/// 〔US1 · 4D〕先前的 `rows`（表里有哪几行）与 `running`（中转在不在）两格并成一格 `endpoint`：
/// 两样事实与那张决策表一起搬进了那台后端，起会话这一侧只剩「问一次、照成品执行」。
/// 谁绕开这条缝直接问那台后端，由 `payload_tests::nobody_reaches_the_relay_take_points_without_going_through_the_seam` 数着。
///
/// # 仍然没有判据的那一格（照实写）
///
/// [`platform_is_windows`] 自己的体（`cfg!(windows)`）：在 Linux 上把它写死成 `false` 是恒等变换 ⇒ 任何运行时判据都分不出来。
/// 调用点那一格买回来了：调用点走 `(facts.windows)()`，写死常量那一形由「PowerShell 那一格」当场红。
#[derive(Clone, Copy)]
pub(crate) struct InjectFactSources {
    /// 「那台后端答的成品」—— 生产恒指 [`ask_launch_endpoint`]。
    pub(crate) endpoint: EndpointAsk,
    /// 🔴 「这台机是不是 Windows」——生产恒指 [`platform_is_windows`]〔`D6 阻-3`，08-29〕。
    ///
    /// 写在调用点上的 `cfg!(windows)` 是个**常量表达式**，判据没法让它变；`D6` 的刀 `Xb`（→ `false`）⇒ 全绿，
    /// 而生产后果是 Windows 上中转前缀渲染成 POSIX 形态。收进本结构之后它成了**可翻的一维**。
    pub(crate) windows: fn() -> bool,
    /// 🔴 「全量注入开关开没开」——生产恒指 [`relay_all_sessions_switch`]〔`设计/20 §7` 步 4〕。
    /// 它随 `launch-endpoint` 的 `allSessions` 交给那台后端（决策表在那边）。
    pub(crate) all_sessions: fn() -> bool,
}

/// 全量注入开关的生产取值口：环境变量 [`RELAY_ALL_SESSIONS_ENV`] 恰好是 `1` 才算开。
///
/// # 为什么是一个环境变量、为什么默认关（`设计/20 §7` 步 4 逐字「必须带开关，默认关；真机验过再默认开」）
///
/// - **默认关**：没设 / 设成别的值 ⇒ 关 ⇒ 起会话的命令逐字节与本件之前相同。
/// - **环境变量**：真机验证那一趟要能不重编就翻（`CCM_NO_DEVTOOLS` / `CCM_CJK_FONT` 同形）。
/// - ⚠ 它**不是**设置页上的一个开关：「真机验过再默认开」那一天要做的是把默认值翻过来，不是加一个界面。
pub(crate) fn relay_all_sessions_switch() -> bool {
    std::env::var(RELAY_ALL_SESSIONS_ENV).is_ok_and(|v| v == "1")
}

/// 全量注入开关的环境变量名。
pub(crate) const RELAY_ALL_SESSIONS_ENV: &str = "CCM_RELAY_ALL_SESSIONS";

/// 「这台机是不是 Windows」的生产取值口。**只有这一处**说得出这句话。
///
/// ⚠ 抽成函数不是为了好看：`cfg!(windows)` 写在调用点上时它是个**常量表达式**，
/// 判据没有任何办法让它变。抽出来 + 进 [`InjectFactSources`] 之后，
/// 「调用点用没用这个答案」变成了可翻的一维（见本结构 `windows` 那一格的头注）。
pub(crate) fn platform_is_windows() -> bool {
    cfg!(windows)
}

/// 生产上这条缝里插的那三个取值口。**只有这一处**，判据按地址对拍它。
pub(crate) const PRODUCTION_INJECT_FACTS: InjectFactSources = InjectFactSources {
    endpoint: ask_launch_endpoint,
    windows: platform_is_windows,
    all_sessions: relay_all_sessions_switch,
};

#[cfg(test)]
thread_local! {
    /// 判据装进来的替身。**线程局部** ⇒ 同进程别的判据不受影响（`cargo test` 是多线程跑的）。
    static INJECT_FACTS_OVERRIDE: std::cell::Cell<Option<InjectFactSources>> =
        const { std::cell::Cell::new(None) };
}

/// 装替身，离开作用域自动还原（`assert!` 炸了也还原）。
#[cfg(test)]
pub(crate) struct InjectFactsGuard(Option<InjectFactSources>);

#[cfg(test)]
impl Drop for InjectFactsGuard {
    fn drop(&mut self) {
        INJECT_FACTS_OVERRIDE.with(|c| c.set(self.0));
    }
}

#[cfg(test)]
pub(crate) fn override_inject_facts(facts: InjectFactSources) -> InjectFactsGuard {
    InjectFactsGuard(INJECT_FACTS_OVERRIDE.with(|c| c.replace(Some(facts))))
}

/// 这一拍要用的取值口。生产上恒是 [`PRODUCTION_INJECT_FACTS`]。
pub(crate) fn inject_facts() -> InjectFactSources {
    #[cfg(test)]
    if let Some(f) = INJECT_FACTS_OVERRIDE.with(|c| c.get()) {
        return f;
    }
    PRODUCTION_INJECT_FACTS
}

/// 本机中转没在听时那句「为什么」（本机那一个住在本机常驻后端里，这里起不了第二个）。
pub(crate) const LOCAL_RELAY_NOT_LISTENING: &str =
    "本机后端里的中转没在那个口上听（多半是口被别的程序占着，原因在本机后端的日志里）";

/// 〔RL1 · US1〕一次拉起的中转地址：`None` = 不注入（照旧直连）；`Err` = 该走却走不了（**拒绝起会话**，出声）。
///
/// **本机与远端同一条路**：问那台机器的后端要成品（`launch-endpoint`），照成品执行 ——
/// - 不注入 ⇒ `None`；注入且那台的中转在听 ⇒ 地址；
/// - 注入但中转不在：远端 ⇒ **用到才起**（`remote_relay::listening_or_started`：`relay-status` → `relay-ensure` → 有界等）；
///   本机 ⇒ 起不了第二个（它住在本机后端里）；仍不在 ⇒ 按成品的 `whenDown`：`refuse` ⇒ 拒并说清，`direct` ⇒ 照旧直连。
/// - 问不到那台后端 ⇒ **拒绝起会话**并说清（D11「后端是给定的、不留退路」：说不出这个号要不要走 API key 那条路，
///   就不猜；照旧起出去，一个 API 号会以「与网络故障同形」的失败收场）。
pub(crate) async fn relay_endpoint_on(
    origin: &crate::origin::Origin,
    account: Option<&LaunchAccount>,
    sid: Option<&str>,
) -> Result<Option<String>, String> {
    let facts = inject_facts();
    let remote = match origin.route("relay_endpoint_for_launch")? {
        crate::origin::Route::Local => None,
        crate::origin::Route::Remote(host) => Some(host.to_string()),
    };
    let where_ = remote
        .as_deref()
        .map_or_else(|| "本机".to_string(), |h| format!("[{h}] "));
    let args = launch_endpoint_args(account, sid, (facts.all_sessions)());
    let data = (facts.endpoint)(remote.clone(), args).await.map_err(|e| {
        format!(
            "问不到{where_}后端这一发该走哪（{e}）—— 没有起会话：说不出这个号要不要经中转换上第三方 key，\
             照旧起出去的话，一个 API 号会以一个与网络故障同形的失败收场。等那台机器的后端连上再试"
        )
    })?;
    let ep = launch_endpoint_from_wire(&where_, &data)?;
    let Some(url) = ep.base_url else {
        return Ok(None);
    };
    if ep.listening {
        return Ok(Some(url));
    }
    let why = match &remote {
        Some(_) => match crate::remote_relay::listening_or_started(origin).await {
            Ok(()) => return Ok(Some(url)),
            Err(why) => why,
        },
        None => LOCAL_RELAY_NOT_LISTENING.to_string(),
    };
    match ep.when_down {
        Some(WhenDown::Refuse) => Err(relay_down_refusal(&where_, ep.account.as_deref(), &why)),
        _ => {
            tracing::info!("{where_}中转不在（{why}）⇒ 这一发照旧直连（`/t/` 那一格是「有它更好」）");
            Ok(None)
        }
    }
}

/// 「非它不可」那一格起不来时的说法（本机与远端同一句，只差哪台机器）。
pub(crate) fn relay_down_refusal(where_: &str, account: Option<&str>, why: &str) -> String {
    format!(
        "apikey 端点改写不可用：账号 {account:?} 在{where_}的 apikey 表里有一行，\
         但{where_}的中转没在听（{why}）——\n\
         这一发要是照旧起出去，claude 那边会报一个与网络故障同形的连接失败，而真正的原因在我们这一侧。\n\
         ⇒ 先让那台机器的后端（本机：设置 → 本机后端）与它的中转起来再试，或把该账号那一行从那台机器的凭据文件里去掉。"
    )
}

/// 上一条的**本机起会话那一截**（[`launch_local`] 是同步的：两个 `#[tauri::command]` 的同步调用链）：
/// 在这里等成品（`block_on`），按这台机器是不是 Windows 渲成前缀。
///
/// ⚠ 事实**只从 [`inject_facts`] 取**（理由见 [`InjectFactSources`] 头注）。
fn relay_prefix_for_launch(
    action: &LocalPsAction,
    account: Option<&LaunchAccount>,
) -> Result<String, String> {
    let sid = match action {
        LocalPsAction::Resume(sid) => Some(sid.as_str()),
        LocalPsAction::New => None,
        // attach 不起 agent ⇒ 这一跳没有「往中转上指」这个问题 ⇒ **不问**那台后端（空前缀）；
        // [`launch_local`] 随后在渲染那一截拒掉 attach（它不走 spawn 那条路），拒的理由由那里说。
        #[cfg(not(windows))]
        LocalPsAction::Attach => return Ok(String::new()),
    };
    let url = tauri::async_runtime::block_on(relay_endpoint_on(
        &crate::origin::Origin::local(),
        account,
        sid,
    ))?;
    Ok(relay_prefix_for(url.as_deref(), (inject_facts().windows)()))
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
/// 中转路由键那一份在 `payload::apikey_endpoint_for` 里面，本文件够不着它算好的值。
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
/// ⚠ 平台那一格**走 [`inject_facts`] 那条缝取**，不写 `cfg!(windows)`：
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
    let prefix = launch_identity_env_prefix(&token, (inject_facts().windows)());
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
/// **真正交出去的那一串**以正确的前缀打头、且前缀随 [`InjectFactSources`] 给的答案与
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
    // ② 中转那一格**永远拼不出路由键**（没有账号 id ⇒ `apikey_account_id` 回 `None`）。
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
/// 这个属性、而它下面碰巧跟着一个内部 `fn`（`parity_ledger_tests.rs::registered_commands`
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

// === metadata 那份文件在哪（〔C4d〕读写者是本机常驻后端；路径仍由这里算）===

/// 历史注解那份文件的路径 —— **全仓只此一处算它**。〔C4d · 第四波 4B〕读写者换成了本机常驻后端（主会话 09-25 裁：
/// 文件留在原处、同一路径）：monitor 起本机后端时用 `CCM_HISTORY_METADATA` 把**本函数算出来的这一个**交过去
/// （`local_backend_host::relay_host_envs`），同一路径因此是构造出来的，不是两侧算法对齐出来的。
pub(crate) fn metadata_path() -> Option<PathBuf> {
    Some(paths::resolve_monitor_data_dir()?.join("history-metadata.json"))
}

#[cfg(test)]
#[path = "../../../tests/bridge/history_title_coverage.rs"]
mod title_coverage;

#[cfg(test)]
#[path = "../../../tests/bridge/history_tests.rs"]
mod tests;
