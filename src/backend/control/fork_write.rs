//! `--fork-session`：在**远端本地**把一个会话从指定消息处分叉出一个新会话文件。
//!
//! # 为什么这一步必须在 daemon 里做
//!
//! 分叉要读整份 jsonl 的祖先链。真机会话动辄几十 MB —— 为了分叉把它拉过 ssh 再算完写回去，
//! 是两趟大文件传输。daemon 就跑在会话所在那台机器上，读写都是本地。
//!
//! # ★ 本模块是 daemon **唯一**被允许写文件系统的地方
//!
//! `readonly_guard` 对它另有一套**更严**的断言（见该文件的白名单层）：
//!
//! - **只准 `create_new`（`O_EXCL`）新建**：目标已存在直接失败。
//! - **不得**删除、改名、复制、建硬链软链。
//! - **不得**用截断或追加打开 —— 那两样能改到既有文件。
//! - **不得**整文件覆盖写、也不建目录（projects 目录本来就在）。
//!
//! ⚠ **上面这几条刻意不写出那些函数的字面名字**。护栏是**子串扫描、不剥注释**，
//! 把 `fs`+`::`+`write` 这种词原样写进注释，会让这个模块被自己的文档判成违规
//! —— 本仓「把注释当代码」已栽过四次（见 `test-support/strip-comments.ts` 头注）。
//! **要改就改措辞，别去放宽护栏。**
//!
//! 这条边界背后的判据不是「daemon 不许碰文件系统」，而是
//! **「daemon 不许改动用户既有数据」**。`O_EXCL` 新建一个此前不存在的文件不违反后者
//! —— 详见 `src/doc/INVARIANTS.md` I7 与 `.claude/planned-build/branch-anywhere/MASTERPLAN.md §4`。
//!
//! # 变换逻辑不在这里
//!
//! 记录变换走共享 crate `branch-core`（monitor 与 daemon **同一份实现**，G1）。
//!
//! # ★〔`K-R88` 09-13〕**「找文件」也不在这里了**
//!
//! 定位源文件那一步先前本模块自己有一份，monitor 侧另有一份、而且**收的入参形状都不一样**
//! （那边收路径、这边收 sid）。`K-R88` 把它收进同一个共享 crate：
//! `branch_core::find_session_file`，两侧都调它，入参形状统一成 sid。
//!
//! ⇒ 本模块今天只剩：**读 → 调变换 → `O_EXCL` 落盘**。
//! 🔴 **写那一半刻意留在这里**（`K-R88` 的射程逐字：本件在收「找」，不搬「写」）——
//! 它是本 crate 只读白名单上那一条，搬它要动的是白名单，那是另一件事。

// U2：合并去重（原来这里各有一份逐字相同的副本）；`S3` 把它搬去了 agent 适配层。
use crate::agents::claudecode::paths::projects_root;
use std::path::Path;

/// 成功时 stdout 输出的一行 JSON（camelCase，与 monitor 侧 `BranchResult` 同形）。
#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct ForkResult {
    session_id: String,
    jsonl_path: String,
}

/// 单份会话 jsonl 的读取上限。
///
/// **Phase G 审计补的**：原来是裸 `read_to_string`（无上限）。同一个 crate 里
/// `common::fs::read_regular_capped` 早就为这件事写好了函数**和**一句结论
///（「`read_to_string` 无上限 → 远端 OOM」，还实测过 symlink→/dev/zero 六秒涨 11GB），
/// 分叉这条新路却没用它。真机上会话 jsonl 到几十 MB 是常态（本仓注释里记过 37MB 一份），
/// daemon 常跑在树莓派/SBC 上，原文 String + 全量 `Value` 双份驻留很容易把它按死。
///
/// 次生危害更隐蔽：进程被 OOM-killer 杀掉时 sshd 送的是 `exit-signal` 而不是 `exit-status`，
/// monitor 侧 `interpret_fork_exec` 会看到 `exit_status: None` ⇒ 报「没收到退出码，连接可能中断」
/// ⇒ 把排查方向带到网络上去。
///
/// 256MB：与 monitor 侧 `remote_history::MAX_SESSION_BYTES` 同一量级，正常会话远够不到。
const MAX_SESSION_JSONL_BYTES: u64 = 256 * 1024 * 1024;

/// 读一个 jsonl 为逐行 `Value`（剥 BOM、跳空行、坏行忽略——口径同 monitor 侧）。
fn read_jsonl(path: &Path) -> Result<Vec<serde_json::Value>, String> {
    // 走共享的**安全读**（先确认是常规文件，挡掉 FIFO/设备，再 take(cap) 限量）——
    // 不是自己再写一遍 `read_to_string`。
    let bytes = crate::common::fs::read_regular_capped(path, MAX_SESSION_JSONL_BYTES)
        .map_err(|e| format!("refuse fork: read {}: {e}", path.display()))?;
    let text = String::from_utf8_lossy(&bytes);
    Ok(text
        .lines()
        .map(|l| l.trim_start_matches('\u{feff}').trim())
        .filter(|l| !l.is_empty())
        .filter_map(|l| serde_json::from_str::<serde_json::Value>(l).ok())
        .collect())
}

/// 生成一个新 sid。**不引 uuid crate**（daemon 依赖表刻意极简，见 Cargo.toml 抬头）：
/// 用「时间 + 进程 id + 源 sid 的哈希」拼一个 v4 形状的串。
///
/// 唯一性不靠这个串本身保证 —— **靠 `O_EXCL`**：撞了就直接失败，绝不覆盖。
fn new_session_id(source_sid: &str) -> String {
    use std::hash::{Hash, Hasher};
    let nanos = std::time::SystemTime::now()
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

/// `--fork-session <source-sid> <message-uuid>`：stdout 出一行 `ForkResult` JSON、exit 0；
/// 出错 exit 2 + stderr 出 `{code,message}`（与 `--resolve` 同一个错误信封约定）。
pub fn run(agent_home: &Path, args: &[String]) -> i32 {
    match run_inner(agent_home, args) {
        Ok(res) => {
            match serde_json::to_string(&res) {
                Ok(s) => println!("{s}"),
                Err(e) => return fail("serialize", &e.to_string()),
            }
            0
        }
        Err(msg) => fail("fork_failed", &msg),
    }
}

fn fail(code: &str, message: &str) -> i32 {
    let env = serde_json::json!({ "code": code, "message": message });
    eprintln!("{env}");
    2
}

fn run_inner(agent_home: &Path, args: &[String]) -> Result<ForkResult, String> {
    let source_sid = args
        .get(1)
        .ok_or("usage: --fork-session <source-sid> <message-uuid>")?;
    let message_uuid = args
        .get(2)
        .ok_or("usage: --fork-session <source-sid> <message-uuid>")?;

    // 🔴 「找文件」**这一句就是全部** —— 本模块只填「记录树的根在哪」这一格
    //（那是 agent 适配层的知识），找本身两侧同一份（`K-R88`）。
    let source = branch_core::find_session_file(&projects_root(agent_home), source_sid)?;
    let lines = read_jsonl(&source)?;
    let new_sid = new_session_id(source_sid);
    let records = branch_core::build_branch_records(&lines, message_uuid, source_sid, &new_sid)?;

    // 落点 = 源文件同目录（那已是 projects 下某个项目目录），文件名 = 新 sid。
    let dir = source
        .parent()
        .ok_or("refuse fork: source has no parent dir")?;
    let out_path = dir.join(crate::agents::claudecode::records::session_file_name(
        &new_sid,
    ));
    write_new_file(&out_path, &records)?;

    Ok(ForkResult {
        session_id: new_sid,
        jsonl_path: out_path.to_string_lossy().into_owned(),
    })
}

/// **唯一的写盘处**。`create_new(true)` = `O_EXCL`：
/// 目标已存在直接失败，既消掉 `exists()→write` 的 TOCTOU 窗口，
/// 也自证「绝不覆盖任何现存会话」——两个 monitor 同时分叉同一会话时，
/// 后到的那个会拿到错误而不是把先到的那份盖掉。
fn write_new_file(out_path: &Path, records: &[serde_json::Value]) -> Result<(), String> {
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
        .map_err(|e| format!("refuse fork: create {}: {e}", out_path.display()))?;
    // 写失败（磁盘满等）也要把原因带回 UI —— 静默半截文件比报错糟得多。
    f.write_all(body.as_bytes())
        .map_err(|e| format!("refuse fork: write {}: {e}", out_path.display()))?;
    Ok(())
}

#[cfg(test)]
#[path = "../../../tests/backend/control/fork_write_tests.rs"]
mod tests;
