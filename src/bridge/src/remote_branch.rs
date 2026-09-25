//! G6（branch-anywhere）：**分叉** —— monitor 侧（本机与远端同一条路）。
//!
//! 〔LOC1a · 第四波 4D〕两支**连传输都是同一条**：问那台机器常驻后端的帧命令 `session-fork {sid, uuid}`
//! （本机 = `<local>` 长连接，远端 = 那台的长连接），后端就地读 → `branch-core` 变换 → `O_EXCL` 新建
//! （`src/backend/control/fork_write.rs`，与 CLI `--fork-session` 同一份本体），monitor 只按形状收结果。
//! 此前本机那一支每次 exec 一个本机后端（`local_query`〔散文墓碑〕）、远端那一支经拨号链路 capture exec
//! `--fork-session` 再解 stdout / stderr / 退出码（`interpret_fork_exec`〔散文墓碑〕）—— 两条都删了（`设计/05 §14.6`）。
//!
//! # 为什么另起一个模块，而不是塞进 `remote_history.rs`
//!
//! 那个模块的头注写着「只读铁律（INVARIANT § 1）：本模块只读远端」。分叉在那台机器上**写**了
//! 一个新文件 —— 虽然是纯新增（见 INVARIANTS §1 里 F62/G6 那两段澄清），但把它塞进一个
//! 自称只读的模块里，等于让那句头注开始说谎。**注释撒谎比没有注释更贵**，所以分家。
//!
//! # 契约（帧命令，与后端 `fork_write.rs::answer_wire` 对表）
//!
//! ```text
//! → {"cmd":"session-fork","args":{"sid":"<源会话 sid>","uuid":"<消息 uuid>"}}
//! ← data {"sessionId":"…","jsonlPath":"…"}      失败码：bad_args · fork_failed
//! ```
//!
//! **后端只收 sid、不收路径**（见 `branch_core::find_session_file` 头注）：少一个可被构造的路径入参
//! 就少一条路径穿越的攻击面。所以 monitor 这边拿到的 jsonl 路径**不往回传**，只传 sid。

use crate::history::BranchResult;

/// 分叉的期限。读一份 jsonl + 写一份新文件，正常是毫秒级；30s 是给巨型会话与慢链路留的余量。
const FORK_BUDGET: std::time::Duration = std::time::Duration::from_secs(30);

/// 交给那台后端的 id 一律先过白名单。
///
/// 后端那一侧（`branch_core::find_session_file` 按 sid 在记录树里找）还会再判一次，所以这里**不是**最后一道；
/// 它的作用是 **fail-fast**：一个明显不是 sid/uuid 的串没必要走一趟长连接才被后端拒。
/// 字符集与共享那份 `branch_core::is_plain_sid` 一致（`[A-Za-z0-9-]`，长度 1..=64）。
///
/// ⚠ **这里刻意没有改成直接调它**，理由如实写：本函数要把「长度不对」与「有非法字符」
/// 分成**两句人话**（这是给用户看的 fail-fast 提示），而那份共享判定只交出一个 `bool`。
/// 改成调它就得把两句话压成一句。⇒ **登记成一处已知的形状重复**，不假装收干净了。
fn validate_fork_id(what: &str, s: &str) -> Result<(), String> {
    // 上限与共享那份 `branch_core::is_plain_sid` 对齐（Phase G 审计：原来这边 128、那边 64，
    // 65..=128 的 id 会白跑一趟才被拒；注释里引的函数名 `valid_sid` 也不存在）。
    if s.is_empty() || s.len() > 64 {
        return Err(format!("{what} 长度非法（1..=64）"));
    }
    if !s.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
        return Err(format!("{what} 含非法字符（只许字母/数字/连字符）"));
    }
    Ok(())
}

/// `session-fork` 的 `data` → [`BranchResult`]。**纯函数**，严格收：缺一格 / 类型不对 ⇒ 报「两端契约对不上」，
/// **绝不**返回一个空壳结果（分叉已经落盘而这边读不出结果时，用户重试会多出一份孤儿分支 —— 所以说清楚是契约问题）。
pub(crate) fn decode_fork(who: &str, data: serde_json::Value) -> Result<BranchResult, String> {
    serde_json::from_value::<BranchResult>(data).map_err(|e| {
        format!("{who}的后端报分叉成功，但结果读不懂（{e}）—— 两端契约对不上，先别重试，刷新会话列表看看")
    })
}

/// 在 `origin` 那台机器上分叉：两个 id 先过白名单，再问那台后端的 `session-fork`。**本机与远端同一个函数。**
async fn fork_on(
    origin: &crate::origin::Origin,
    source_session_id: &str,
    message_uuid: &str,
) -> Result<BranchResult, String> {
    validate_fork_id("源会话 id", source_session_id)?;
    validate_fork_id("消息 uuid", message_uuid)?;
    let who = crate::backend::control::frame_query::who(origin);
    let data = crate::backend::control::frame_query::call(
        origin,
        "session-fork",
        serde_json::json!({ "sid": source_session_id, "uuid": message_uuid }),
        FORK_BUDGET,
    )
    .await?;
    let res = decode_fork(&who, data)?;
    tracing::info!(
        "branch: {who}后端分叉 {source_session_id}@{message_uuid} → {}",
        res.session_id
    );
    Ok(res)
}

/// G6：在远端从某条消息分叉出新会话 —— [`crate::history::create_branch_session`] 的远端那一支。
///
/// ⚠ 名字**刻意没改**：`local_origin_registry` 按「文件::函数」登记着这一处，改名会让那张表静默失配。
/// 它收的是**已经分过本机**的机器名（`host: &str`），不收 `Origin`（`history_tests` 钉着）。
pub(crate) async fn create_remote_branch_session(
    host: &str,
    source_session_id: &str,
    message_uuid: &str,
) -> Result<BranchResult, String> {
    fork_on(
        &crate::origin::Origin(host.to_string()),
        source_session_id,
        message_uuid,
    )
    .await
}

/// 〔RW1 · 第四波 · 2026-09-24 → LOC1a〕**本机那一支**：与远端同一个 [`fork_on`]，origin = `<local>`。
///
/// 用户裁「只允许后端的文件管理部分写文件」**也管本机** ⇒ 分叉出来的新会话由本机常驻后端写
/// （`control/fork_write.rs`，写盘白名单层那一处 `O_EXCL`），本进程一个字节不写。
/// ⚠ 本机后端够不着 ⇒ 明确说（「没有可用的控制通道」），不回落到本进程写（`D11`）。
pub(crate) async fn create_local_branch_session(
    source_session_id: &str,
    message_uuid: &str,
) -> Result<BranchResult, String> {
    fork_on(
        &crate::origin::Origin::local(),
        source_session_id,
        message_uuid,
    )
    .await
}

#[cfg(test)]
#[path = "../../../tests/bridge/remote_branch_tests.rs"]
mod tests;
