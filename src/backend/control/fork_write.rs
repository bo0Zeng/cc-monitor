//! 分叉（帧命令 `session-fork` 与起新会话带 `forkFrom` 的本体）：在**远端本地**把一个会话从指定消息处分叉出一个新会话文件。
//!
//! # 为什么这一步必须在后端里做
//!
//! 分叉要读整份 jsonl 的祖先链。真机会话动辄几十 MB —— 为了分叉把它拉过 ssh 再算完写回去，
//! 是两趟大文件传输。backend 就跑在会话所在那台机器上，读写都是本地。
//!
//! # ★ 本模块只会新建
//!
//! 写经 [`crate::platform::fs::NewFile`]：那只手只有一件事 —— `O_EXCL` 新建一份此前不存在的文件、一次写满；
//! 目标已存在直接失败。删、改名、复制、截断、追加、覆盖、建目录它都没有（projects 目录本来就在）。
//! 它由分叉那两条命令（`session-fork` · `session-new`）在各自的实现里领，别处拿不到。
//!
//! 这条边界背后的判据不是「backend 不许碰文件系统」，而是
//! **「backend 不许改动用户既有数据」**。`O_EXCL` 新建一个此前不存在的文件不违反后者
//! —— 详见 `src/doc/INVARIANTS.md` I7 与。
//!
//! # 变换逻辑不在这里
//!
//! 记录变换住适配层 `agents/claudecode/branch.rs`（原共享 crate `branch-core`，只剩后端用），经注册表那一格够它（`agents::build_branch_records`）。
//!
//! # ★**「找文件」也不在这里了**
//!
//! 定位源文件那一步先前本模块自己有一份，monitor 侧另有一份、而且**收的入参形状都不一样**
//! （那边收路径、这边收 sid）。`K-R88` 把它收成一份，入参形状统一成 sid；今天住适配层、经注册表那一格够
//! （`agents::find_session_file`）。
//!
//! ⇒ 本模块今天只剩：**读 → 调变换 → `O_EXCL` 落盘**。
//! 🔴 **写那一半刻意留在这里**（`K-R88` 的射程逐字：这里收「找」，不搬「写」）——
//! 写经分叉那只只会新建的手（`NewFile`）。

use crate::platform::fs::{NewFile, NewFileFail};
use std::path::Path;

/// 本体落完盘的结果（帧面回包由它 ＋ `launch` 那三格拼成 [`Forked`]）。
#[derive(Debug)]
struct ForkResult {
    session_id: String,
    jsonl_path: String,
    /// 源会话那份记录（帧面拿它收「分叉之后起」的事实）。
    source: std::path::PathBuf,
}

/// `session-fork` 的入参：从 `sid` 那条会话的 `uuid` 那条消息处分叉。
#[derive(Debug, Clone, serde::Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
#[serde(deny_unknown_fields)]
pub(crate) struct ForkReq {
    /// 源会话 id（只收 sid、不收路径：按 sid 在记录树里找那份文件）。
    pub(crate) sid: String,
    /// 从哪条消息处分叉。
    pub(crate) uuid: String,
}

/// `session-fork` 的回包。
#[derive(Debug, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
#[serde(rename_all = "camelCase")]
pub(crate) struct Forked {
    /// 新会话的 id。
    pub(crate) session_id: String,
    /// 新会话的落点（源文件同目录，`O_EXCL` 新建：撞了就失败，绝不覆盖）。
    pub(crate) jsonl_path: String,
    /// 起分叉出来的新会话要的三格。
    pub(crate) launch: super::fork_launch::Launch,
}

crate::wire::typed::codes! {
    /// `session-fork` 的码。
    pub(crate) enum ForkErr {
        /// 记录树读不出 · 源会话找不着 · 那条消息不在 · 落盘失败。
        ForkFailed = "fork_failed",
    }
}

/// 单份会话 jsonl 的读取上限。
///
/// **Phase G 审计补的**：原来是裸 `read_to_string`（无上限）。同一个 crate 里
/// `platform::fs::Ro::read_regular_capped` 早就为这件事写好了函数**和**一句结论
///（「`read_to_string` 无上限 → 远端 OOM」，还实测过 symlink→/dev/zero 六秒涨 11GB），
/// 分叉这条新路却没用它。真机上会话 jsonl 到几十 MB 是常态（本仓注释里记过 37MB 一份），
/// backend 常跑在树莓派/SBC 上，原文 String + 全量 `Value` 双份驻留很容易把它按死。
///
/// 256MB：与 monitor 侧 `remote_history::MAX_SESSION_BYTES` 同一量级，正常会话远够不到。
const MAX_SESSION_JSONL_BYTES: u64 = 256 * 1024 * 1024;

/// 读一个 jsonl 为逐行 `Value`（剥 BOM、跳空行、坏行忽略——口径同 monitor 侧）。
fn read_jsonl(path: &Path) -> Result<Vec<serde_json::Value>, String> {
    // 走共享的**安全读**（先确认是常规文件，挡掉 FIFO/设备，再 take(cap) 限量）——
    // 不是自己再写一遍 `read_to_string`。
    let bytes = crate::platform::fs::Ro::read_regular_capped(path, MAX_SESSION_JSONL_BYTES)
        .map_err(|e| format!("refuse fork: read {}: {e}", path.display()))?;
    let text = String::from_utf8_lossy(&bytes);
    Ok(text
        .lines()
        .map(|l| l.trim_start_matches('\u{feff}').trim())
        .filter(|l| !l.is_empty())
        .filter_map(|l| serde_json::from_str::<serde_json::Value>(l).ok())
        .collect())
}

/// 生成一个新 sid。**不引 uuid crate**（backend 依赖表刻意极简，见 Cargo.toml 抬头）：
/// 用「时间 + 进程 id + 源 sid 的哈希」拼一个 v4 形状的串。
///
/// 唯一性不靠这个串本身保证 —— **靠 `O_EXCL`**：撞了就直接失败，绝不覆盖。
fn new_session_id(source_sid: &str) -> String {
    use std::hash::{Hash, Hasher};
    let nanos = crate::common::time::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let mut h = std::collections::hash_map::DefaultHasher::new();
    source_sid.hash(&mut h);
    std::process::id().hash(&mut h);
    nanos.hash(&mut h);
    let a = h.finish();
    let mut h2 = std::collections::hash_map::DefaultHasher::new();
    a.hash(&mut h2);
    nanos.hash(&mut h2);
    let b = h2.finish();
    format!(
        "{:08x}-{:04x}-4{:03x}-{:04x}-{:012x}",
        (a >> 32) as u32,
        (a >> 16) as u16,
        (a & 0x0fff) as u16,
        ((b >> 48) as u16 & 0x3fff) | 0x8000,
        b & 0xffff_ffff_ffff
    )
}

/// 帧面 `session-fork` 的本体：两个 id 过放行判定 → [`run_inner`]（读 → `branch-core` 变换 → `O_EXCL` 落盘）。
///
/// 本机远端都经那台机器常驻后端的长连接说这一条。家目录由顶层壳 `fork_face` 交进来（本层自己找就得问 `observe::`，反向边）；
/// 判据直接喂夹具家目录。回包比本体多一格 `launch`：由入口按「源会话那份记录 ＋ 源 sid」收齐交回，本层不读观测层。
pub(crate) fn fork_at(
    nf: &NewFile,
    agent_home: &Path,
    req: &ForkReq,
    launch: &dyn Fn(&Path, &str) -> super::fork_launch::Launch,
) -> Result<Forked, ForkErr> {
    let res = fork_checked(nf, agent_home, &req.sid, &req.uuid)
        .map_err(crate::wire::typed::of_pair(ForkErr::ForkFailed))?;
    Ok(Forked {
        launch: launch(&res.source, &req.sid),
        session_id: res.session_id,
        jsonl_path: res.jsonl_path,
    })
}

/// 起新会话那一条（`session-new` 带 `forkFrom`）写分支记录：同 [`fork_at`] 那一份本体，回新会话的 sid。
pub(crate) fn fork_for_launch(
    nf: &NewFile,
    agent_home: &Path,
    sid: &str,
    uuid: &str,
) -> Result<String, (&'static str, String)> {
    fork_checked(nf, agent_home, sid, uuid).map(|r| r.session_id)
}

/// 两个 id 先过放行判定，再交 [`run_inner`]。
fn fork_checked(
    nf: &NewFile,
    agent_home: &Path,
    sid: &str,
    uuid: &str,
) -> Result<ForkResult, (&'static str, String)> {
    // 〔`INVARIANTS §47` ①〕两个 id 在这里先过放行判定（界面经通道直说、不再判；本侧 = 真去用它的这一侧）：
    //   判定只有共享那一份 `session_id_ok`，这里只把「长度不对」与「形状不对」分成两句话。
    for (what, id) in [
        (
            copy_core::copy_text("rsRemoteBranch.what.sourceId", &[]),
            sid,
        ),
        (
            copy_core::copy_text("rsRemoteBranch.what.messageId", &[]),
            uuid,
        ),
    ] {
        if !shell_quote_core::session_id_ok(id) {
            let said = if id.len() > shell_quote_core::SESSION_ID_MAX {
                copy_core::copy_text("rsRemoteBranch.forkId.badLength", &[("what", &what)])
            } else {
                copy_core::copy_text("rsRemoteBranch.forkId.badChar", &[("what", &what)])
            };
            return Err(("bad_args", said));
        }
    }
    run_inner(nf, agent_home, sid, uuid).map_err(|m| ("fork_failed", m))
}

fn run_inner(
    nf: &NewFile,
    agent_home: &Path,
    source_sid: &str,
    message_uuid: &str,
) -> Result<ForkResult, String> {
    // 🔴 「找文件」**这一句就是全部** —— 本模块只填「记录树的根在哪」这一格
    //（那是 agent 适配层的知识），找本身两侧同一份（`K-R88`）。
    let root = crate::agents::records_root(agent_home)
        .ok_or("refuse fork: no agent here keeps a record tree")?;
    let source = crate::agents::find_session_file(&root, source_sid)?;
    let lines = read_jsonl(&source)?;
    let new_sid = new_session_id(source_sid);
    // 分叉变换问这份记录归的那一家。
    let kind = crate::agents::record_kind_of(&source).unwrap_or_default();
    let records =
        crate::agents::build_branch_records(kind, &lines, message_uuid, source_sid, &new_sid)?;

    // 落点 = 源文件同目录（那已是 projects 下某个项目目录），文件名 = 新 sid。
    let dir = source
        .parent()
        .ok_or("refuse fork: source has no parent dir")?;
    let file_name = crate::agents::session_file_name(kind, &new_sid)
        .ok_or("refuse fork: this agent has no session file naming")?;
    let out_path = dir.join(file_name);
    write_new_file(nf, &out_path, &records)?;

    Ok(ForkResult {
        session_id: new_sid,
        jsonl_path: out_path.to_string_lossy().into_owned(),
        source,
    })
}

/// **唯一的写盘处**。`create_new(true)` = `O_EXCL`：
/// 目标已存在直接失败，既消掉 `exists()→write` 的 TOCTOU 窗口，
/// 也自证「绝不覆盖任何现存会话」——两个 monitor 同时分叉同一会话时，
/// 后到的那个会拿到错误而不是把先到的那份盖掉。
fn write_new_file(
    nf: &NewFile,
    out_path: &Path,
    records: &[serde_json::Value],
) -> Result<(), String> {
    let mut body = String::new();
    for rec in records {
        body.push_str(&serde_json::to_string(rec).map_err(|e| format!("serialize: {e}"))?);
        body.push('\n');
    }
    // 写失败（磁盘满等）也要把原因带回 UI —— 静默半截文件比报错糟得多。
    nf.create(out_path, body.as_bytes()).map_err(|f| match f {
        NewFileFail::Create(e) => format!("refuse fork: create {}: {e}", out_path.display()),
        NewFileFail::Write(e) => format!("refuse fork: write {}: {e}", out_path.display()),
    })
}

#[cfg(test)]
#[path = "../../../tests/backend/control/fork_write_tests.rs"]
mod tests;
