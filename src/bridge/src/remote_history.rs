//! issue #16 P1a：远端历史浏览的 monitor 侧。
//!
//! 〔C4d · 第四波 4B 订正〕这里原先写着「每条查询走**独立 SSH 连接**一次性 exec `<backend_path> --list-projects` 等」
//! ＋「首行是 hello 就报 backend 版本过旧」—— 那是 issue #16 P1a 的形状。`C1`（09-24）起查询全走那台的长连接
//! （`backend::control::frame_query`，老后端由帧面的能力协商说「还不认」），逐次拨号那条路 C4d 删了。
//!
//! 只读铁律（INVARIANT § 1）：本模块只读远端；resume/delete 对远端在前端禁用。
//! INVARIANTS § 25：本路径是一次性读取（非 at-least-once 行流），SessionViewer
//! 每次 load 全新实例，无重投幂等义务。

use crate::copy_table::copy_text;
use crate::messages::JsonlRecord;
use crate::parser::parse_line;
use crate::ssh_source::RemoteConfig;

/// 读单会话：不设整体超时（会话可能大、合法耗时），总字节上限兜底。
/// 〔`C1` · 09-24〕单次读的期限从此是**每一页**的期限（`frame_query` 的 `PAGE_BUDGET`，60s，
/// 与原来这里的单次 `read_line` 超时同值）；原先那个常量随逐次拨号一起删了。
///
/// ⚠〔audit-0805 F06〕**这里原本还有一句「正常会话毫秒级、远小于上限」——那句今天是假的。**
/// 实测本机最大会话 **270,103,105 字节 / 92,967 行**（就是那次审计对话本身），
/// 已经**越过** 256 MiB 这条线 1,667,649 字节；57 MB 以上的会话有 5 个，不是孤例。
/// ⇒ 上限**会被真实数据打到**，所以「打到之后怎么办」不能是静默。
const MAX_SESSION_BYTES: u64 = 256 * 1024 * 1024;

/// 超限时给用户的话〔audit-0805 F06，定框 **E4/E5**〕。
///
/// # 它此前是**静默**的
///
/// 读法是 `stream.take(MAX_SESSION_BYTES)` + `if n == 0 { break; }` ——
/// 到限之后 `read_line` 返回 0，与**正常 EOF 完全同形** ⇒ 前端拿到一份「看起来完整」的历史，
/// 而后面的内容**无声消失**。同一份数据走后端的 `--fork-session` 那条路会**硬报错**
/// （`common/fs.rs`），走这条路却什么都不说 —— 这正是定框 **E5** 要消灭的
/// 「同一份数据走不同路得到不同答案」。
///
/// 抽成纯函数是为了让它可判据：外面那圈是真 SSH 流，测不了。
fn session_truncated_message(read_bytes: u64, lines_shown: u32) -> String {
    copy_text(
        "rsRemoteHistory.session.truncated",
        &[
            ("max", &MAX_SESSION_BYTES.to_string()),
            ("read", &read_bytes.to_string()),
            ("lines", &lines_shown.to_string()),
        ],
    )
}

pub(crate) fn require_cfg_by_label(label: &str) -> Result<RemoteConfig, String> {
    crate::load_remote_config_by_label(label).ok_or_else(|| {
        copy_text(
            "rsRemoteHistory.cfg.missing",
            &[("label", &label.to_string())],
        )
    })
}

// 〔C4d · 第四波 4B〕逐次拨号那条路（`run_list_query`〔散文墓碑〕与它的老后端识别、超时）删了：
//   `frame_query` 那张「仍拨号」的表 C4c 起就是空的 ⇒ 它一条都放不过去；主会话 09-25 裁删，
//   唯一调用方（`subagent·rs::Backend::query` 的远端回落）同拍改成「没有帧命令就当场说」。

// 〔C4a · 第四波〕远端全文搜索的 fan-out（issue #28）**搬到前端**：`src/views/history-search.ts`
//   对每台远端经通道说帧命令 `history-search`、逐行解释、补 `origin`、与本机索引合并 ——
//   三件事在那边各只有一个家，这里的那一份〔散文墓碑〕同拍删掉（旧名 `search_remote_all`）。

// 〔C4d · 第四波 4B〕远端历史清单那一族（`K-R83` / `K-R92` 的「不知道」三态 `Counted` / `WhyUnknown` / 判活入参 `LivenessOracle`
//   · 行 ⇒ 项目 / 会话的 join `history_project_from_row` / `remote_session_entry`〔散文墓碑〕· 逐台 fan-out
//   `fanout_list_projects` 与 `list_remote_history_projects`〔散文墓碑〕· 展开远端一个项目 `stream_remote_history_sessions`〔散文墓碑〕）
//   整族搬进了本机常驻后端（`src/backend/history_join.rs`：判定一字不改、判据跟着搬 —— `tests/backend/history_join_tests.rs`）。
//   主会话 09-25 裁「join 只一个家」：注解归本机后端，它经 `remote_ask` 问远端那台、并上注解、出成品；界面经 `chan.call`
//   （`src/history-reads.ts`，逐台 fan-out 也搬到那里）。这里只剩读一整份远端会话与删一份远端会话两件。

/// 流式读取远端单个会话（对齐本地 stream_read_session_jsonl 的 chunk 口径：
/// 每 100 条一发，payload 带 origin=Some(host)，SessionViewer 零改动复用）。
///
/// 🔴 **〔步 12·C 2026-09-20〕它不再是一条 Tauri 命令。**
/// 上线的那一条是 [`crate::history::stream_read_session_jsonl`]，本函数是它的远端那一支。
/// 同上一条：这是 `真相源/97 §二 丙`「措辞不同」那一档 —— 判它是一对靠的是
/// **chunk 口径逐字对齐**（每 100 条一发、同一个 `JsonlLinePayload`、同一套 per-file `seq`），
/// 不是名字。
///
/// ⚠ **两侧有一处如实记着的不对称**：`origin` 字段本机侧填 `None`、远端侧填 `Some(label)`。
/// 那**不是**这次合并引入的，也**不是**这次合并要治的 —— 它是载荷那一层的事
/// （`JsonlLinePayload::origin`），治它要动前端 `SessionViewer` 的来源判定。
/// 本步只合命令面，**不顺手改载荷语义**。
pub(crate) async fn stream_read_remote_session(
    jsonl_path: String,
    host: &str,
    on_chunk: tauri::ipc::Channel<Vec<crate::bridge::JsonlLinePayload>>,
) -> Result<u32, String> {
    const CHUNK_SIZE: usize = 100;
    let cfg = require_cfg_by_label(host)?;
    // 深度防御（与 stream_remote_history_sessions 的 project_dir 校验对称）：jsonl_path 来自
    // 前端，monitor 侧先做廉价校验（拒 `..` + 强制 .jsonl 后缀）。真正的越权读由后端侧
    // canonicalize + projects/ 前缀 + symlink 逃逸校验兜底，这里补齐不对称的防御缺口。
    if jsonl_path.contains("..") || !jsonl_path.ends_with(".jsonl") {
        return Err(copy_text(
            "rsRemoteHistory.session.badPath",
            &[("path", &jsonl_path.to_string())],
        ));
    }
    let started = std::time::Instant::now();
    // 与本地 history.rs 的 file_stem 口径一致：剥**一个** ".jsonl" 后缀（strip_suffix
    // 是字面后缀，不是 trim_end_matches 的字符集语义）。
    let file_name = jsonl_path.rsplit(['/', '\\']).next().unwrap_or("");
    let session_id = file_name
        .strip_suffix(".jsonl")
        .unwrap_or(file_name)
        .to_string();
    // 〔`C1` · 2026-09-24〕走长连接的 `history-read`，按字节分页（一页 ≤1 MiB、切在行尾），
    // 不再为读一份会话单拨一条 SSH。总量上限（F06）与逐行口径一个字没动。
    let origin = cfg.origin_label();
    let wire_origin = crate::origin::Origin(origin.clone());
    let mut read_bytes: u64 = 0;
    let mut cwd_seen: Option<String> = None;
    let mut chunk: Vec<crate::bridge::JsonlLinePayload> = Vec::with_capacity(CHUNK_SIZE);
    let mut total = 0u32;
    // 〔U3b〕seq = **可计行号**（同本机那一支，住址 `session_skeleton·rs::LineNumberer`）
    let mut numberer = crate::session_skeleton::LineNumberer::default();
    let mut offset: u64 = 0;
    loop {
        let page = crate::backend::control::frame_query::read_page(
            &wire_origin,
            &jsonl_path,
            offset,
            None,
        )
        .await?;
        read_bytes += page.next - offset;
        if read_bytes > MAX_SESSION_BYTES {
            // F06：**不许静默截断**。同一份数据走后端的 `--fork-session` 会硬报错，
            // 走这条路却假装读完了 —— 定框 E5 要的是「同一份数据走不同路得到同一个答案」。
            return Err(session_truncated_message(read_bytes, total));
        }
        for line in page.text.lines() {
            // 与本地 stream_read_session_jsonl 同口径：parse + displayable 过滤 + per-file seq
            // 〔U3b〕先占号、后过滤（住址 `session_skeleton·rs::numbered_displayable`）。
            // 〔合并 C1＋U3b〕C1 把读法换成帧面分页之后，原先按首行认「老后端 hello」那一格由帧面的
            //   能力协商接管（旧后端不认 `history-read` ⇒ 发之前就说「后端还不认」，不会拿到 hello 行）。
            let Some((seq, rec)) =
                // 〔ST3〕看不懂的行记在这台远端名下。
                crate::session_skeleton::numbered_displayable(&mut numberer, line, |b| {
                    parse_line(&wire_origin, b)
                })
            else {
                continue;
            };
            if let JsonlRecord::User { cwd, .. } = &rec {
                if cwd_seen.is_none() {
                    cwd_seen = cwd.clone();
                }
            }
            chunk.push(crate::bridge::JsonlLinePayload {
                session_id: session_id.clone(),
                cwd: cwd_seen.clone(),
                path: jsonl_path.clone(),
                seq,
                origin: Some(origin.clone()),
                message: rec,
            });
            total += 1;
            if chunk.len() >= CHUNK_SIZE {
                let full = std::mem::replace(&mut chunk, Vec::with_capacity(CHUNK_SIZE));
                if on_chunk.send(full).is_err() {
                    tracing::info!(
                        "stream_read_remote_session({session_id}): 前端取消于 {total} 条"
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
        "stream_read_remote_session({session_id}): {total} records in {}ms",
        started.elapsed().as_millis()
    );
    Ok(total)
}

/// 删除一个远端历史会话的 jsonl（issue 未拆，F11）—— [`crate::history::delete_history_session`] 的远端那一支。
///
/// 🔴 **〔RW1 · 第四波 · 2026-09-24〕F11 按用户裁「按推荐改」：经那台远端的后端删**
/// （`files-delete-session`，只收 sid；落点由远端后端按 sid 在它自己的记录树里找）。
/// 从前那一道 SFTP 直删 ＋ 双重路径守卫（`sftp::remove_remote_file`〔散文墓碑〕）整条走了；
/// 「删 A 的文件、清 B 的注解」那个口由 [`crate::history::delete_via_backend`] 的一致性闸在两侧同时防。
pub(crate) async fn delete_remote_history_session(
    host: &str,
    session_id: String,
    jsonl_path: String,
) -> Result<(), String> {
    let door = crate::user_files::BackendDoor::new(crate::origin::Origin(host.to_string()));
    crate::history::delete_via_backend(&door, &session_id, &jsonl_path).await
}

/// 远端 POSIX 路径取文件名 stem（去目录、去 `.jsonl`）。非 jsonl / 无文件名 → None。
pub(crate) fn jsonl_stem(path: &str) -> Option<String> {
    let name = path.rsplit('/').next()?;
    name.strip_suffix(".jsonl").map(str::to_string)
}

#[cfg(test)]
#[path = "../../../tests/bridge/remote_history_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "../../../tests/bridge/remote_history_f06_tests.rs"]
mod f06_tests;

// 〔C4d〕`K-R83` 那三条判据（`remote_history_kr83_tests.rs`）随被测的 join 一起搬进后端（`history_join_tests.rs`）。
