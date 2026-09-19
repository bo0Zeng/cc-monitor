//! G6（branch-anywhere）：**远端分叉** —— monitor 侧。
//!
//! 本地分叉在 `history::create_branch_session`（读本机 jsonl、写本机新文件）。
//! 远端会话的 jsonl 在**另一台机器上**，monitor 够不着 —— 所以远端这条路是
//! 「经 ssh 让 daemon 自己在那台机器上分叉」，monitor 只收结果。
//!
//! # 为什么另起一个模块，而不是塞进 `remote_history.rs`
//!
//! 那个模块的头注写着「只读铁律（INVARIANT § 1）：本模块只读远端」。分叉在远端**写**了
//! 一个新文件 —— 虽然是纯新增（见 INVARIANTS §1 里 F62/G6 那两段澄清），但把它塞进一个
//! 自称只读的模块里，等于让那句头注开始说谎。**注释撒谎比没有注释更贵**，所以分家。
//!
//! # 契约（与 daemon `fork_write.rs` 对表；发版后冻结、已知会 aterm）
//!
//! ```text
//! <daemon> --fork-session <source-sid> <message-uuid>
//!   成功: exit 0 + stdout 一行 {"sessionId":"…","jsonlPath":"…"}
//!   失败: exit 2 + stderr 一行 {"code":"…","message":"…"}
//! ```
//!
//! **daemon 只收 sid、不收路径**（见 `branch_core::find_session_file` 头注）：daemon 是被
//! ssh 远程调起来的，少一个可被构造的路径入参就少一条路径穿越的攻击面。所以 monitor 这边
//! 拿到的远端 jsonl 路径**不往回传**，只传 sid。
//!
//! ★〔`K-R88` 09-13〕**这条收窄今天两侧都吃**：本机那条命令
//! （`history::create_branch_session`）也收 sid 了，「找那份文件」两侧同一份实现。
//! ⇒ 下面那句「与本地那条的差异」也跟着少了一条 —— 只剩「活儿在哪台机器上干」。

use crate::history::BranchResult;
use crate::ssh_source::{self, RemoteExec};

/// 分叉整体限时。读一份 jsonl + 写一份新文件，正常是毫秒级；30s 是给巨型会话
/// 与慢链路留的余量，同 `remote_history::LIST_TIMEOUT` 的量级。
const FORK_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);

/// 旧 daemon 掉进流模式的判据：查询模式的输出不可能含 wire 的 `"kind":"hello"`。
///
/// **两种写法都要认**（Phase G 审计：原来只认无空格那条，而 `remote_history::is_old_daemon_hello`
/// 认两条 —— 注释却写着「同一判据」，是句错话）。序列化器今天产的是无空格那条，
/// 所以 `abort_marker` 用它（`abort_marker` 只能给一个子串，且要能在**半行**上命中、不等换行）；
/// 判定时两条都查，免得哪天序列化器换了写法就退化成「等 30s 超时」。
const HELLO_MARKER: &str = r#""kind":"hello""#;
const HELLO_MARKER_SPACED: &str = r#""kind": "hello""#;

fn looks_like_old_daemon(stdout: &str) -> bool {
    stdout.contains(HELLO_MARKER) || stdout.contains(HELLO_MARKER_SPACED)
}

const OLD_DAEMON_MSG: &str =
    "远端 daemon 版本过旧（不支持远端分叉）——请重新部署 daemon 后再试（src/doc/REMOTE-PHASE0-DEPLOY.md）";

/// daemon 失败时 stderr 上的信封。字段少写/多写都容忍不了 —— 认不出就退回展示原文，
/// **绝不**把「认不出的错误」静默成成功。
#[derive(serde::Deserialize)]
struct ForkErrEnvelope {
    code: String,
    message: String,
}

/// 拼进远端命令的 id 一律先过白名单。
///
/// 两个参数最终都会经 `shell_quote` 单引号包裹，所以这里**不是**注入防线的最后一道；
/// 它的作用是 **fail-fast**：一个明显不是 sid/uuid 的串没必要跑一趟 ssh 才被 daemon 拒。
/// 字符集与共享那份 `branch_core::is_plain_sid` 一致（`[A-Za-z0-9-]`，长度 1..=64）。
///
/// ⚠ **这里刻意没有改成直接调它**，理由如实写：本函数要把「长度不对」与「有非法字符」
/// 分成**两句人话**（这是给用户看的 fail-fast 提示），而那份共享判定只交出一个 `bool`。
/// 改成调它就得把两句话压成一句。⇒ **登记成一处已知的形状重复**，不假装收干净了。
fn validate_fork_id(what: &str, s: &str) -> Result<(), String> {
    // 上限与共享那份 `branch_core::is_plain_sid` 对齐（Phase G 审计：原来这边 128、那边 64，
    // 65..=128 的 id 会白跑一趟 ssh 才被拒；注释里引的函数名 `valid_sid` 也不存在）。
    if s.is_empty() || s.len() > 64 {
        return Err(format!("{what} 长度非法（1..=64）"));
    }
    if !s.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
        return Err(format!("{what} 含非法字符（只许字母/数字/连字符）"));
    }
    Ok(())
}

/// 拼远端命令。两个 id 已过白名单，仍照常 `shell_quote`（纵深防御，同 `--search`）。
fn build_fork_cmd(daemon_path: &str, source_sid: &str, message_uuid: &str) -> String {
    format!(
        "{} --fork-session {} {}",
        ssh_source::shell_quote(daemon_path),
        ssh_source::shell_quote(source_sid),
        ssh_source::shell_quote(message_uuid),
    )
}

/// 把一次 exec 的三样东西（stdout/stderr/退出码）判成「分叉结果」或**人话错误**。
///
/// 纯函数，故可直测 —— 这是本模块真正要守的判断，SSH 那半只是搬运。
///
/// 判定顺序刻意如此：
/// 1. **先看旧 daemon** —— 它 stdout 有内容、可能还 exit 0，不先拦会被误读成「输出解析失败」。
/// 2. **`exit_status == None` 归失败**。没拿到退出码 = 连接被掐/服务端不守规矩，
///    把它当 0 正好把「没跑成」读成「跑成了」。
/// 3. exit 0 才解析 stdout；解析不出来仍是失败（**绝不返回一个空壳 `BranchResult`**）。
fn interpret_fork_exec(ex: &RemoteExec) -> Result<BranchResult, String> {
    if looks_like_old_daemon(&ex.stdout) {
        return Err(OLD_DAEMON_MSG.to_string());
    }

    // ★ Phase G 审计：**扫所有行**，不是只看第一条非空行。
    // 远端 `~/.bashrc` 打一行 banner 是常见配置（而本仓的部署流程本来就会动 `.bashrc`），
    // 命令又是经登录 shell 执行的 ⇒ 第一行很可能是噪声。只看第一行的后果是：
    // 分叉**已经成功、文件已落盘、exit 0**，monitor 却报「结果解析失败」，用户重试 ⇒
    // 远端多出一份孤儿分支文件（`O_EXCL` 拦不住，新 sid 不同）。
    let stderr_detail = || -> Option<String> {
        let mut first_nonempty: Option<&str> = None;
        for line in ex.stderr.lines().map(str::trim).filter(|l| !l.is_empty()) {
            if first_nonempty.is_none() {
                first_nonempty = Some(line);
            }
            if let Ok(env) = serde_json::from_str::<ForkErrEnvelope>(line) {
                return Some(format!("{}（{}）", env.message, env.code));
            }
        }
        // 一条信封都认不出 → 把第一行原样带出（截断防刷屏），比吞掉强。
        first_nonempty.map(|l| l.chars().take(400).collect())
    };

    match ex.exit_status {
        Some(0) => {
            let mut saw_any = false;
            for line in ex.stdout.lines().map(str::trim).filter(|l| !l.is_empty()) {
                saw_any = true;
                if let Ok(r) = serde_json::from_str::<BranchResult>(line) {
                    return Ok(r);
                }
            }
            Err(if saw_any {
                "远端分叉：daemon 报成功，但输出里找不到结果 JSON（远端 shell 是不是打了 banner？）"
                    .to_string()
            } else {
                "远端分叉：daemon 报成功却没有输出结果".to_string()
            })
        }
        Some(code) => Err(match stderr_detail() {
            Some(d) => format!("远端分叉失败：{d}"),
            None => format!("远端分叉失败（daemon 退出码 {code}，且没有给出原因）"),
        }),
        None => Err(match stderr_detail() {
            Some(d) => format!("远端分叉失败（没收到退出码，连接可能中断）：{d}"),
            None => "远端分叉失败：没收到退出码，连接可能中断".to_string(),
        }),
    }
}

/// G6 IPC：在远端从某条消息分叉出新会话。前端点远端会话卡上的 `⑂` 时调。
///
/// 与本地那条（`history::create_branch_session`）的差异**今天只剩一处：活儿在远端干**。
/// 〔`K-R88` 09-13〕原先还有一处「收 sid 不收路径」—— 本机那条已经跟着收成 sid 了。
/// 返回体同形，所以前端两条路共用同一段成功处理。
#[tauri::command]
pub async fn create_remote_branch_session(
    origin: String,
    source_session_id: String,
    message_uuid: String,
) -> Result<BranchResult, String> {
    validate_fork_id("源会话 id", &source_session_id)?;
    validate_fork_id("消息 uuid", &message_uuid)?;
    let cfg = crate::load_remote_config_by_label(&origin)
        .ok_or_else(|| format!("远端 '{origin}' 未配置或未启用"))?;

    let cmd = build_fork_cmd(&cfg.daemon_path, &source_session_id, &message_uuid);
    let ex = tokio::time::timeout(
        FORK_TIMEOUT,
        ssh_source::connect_and_exec_capture(&cfg, &cmd, Some(HELLO_MARKER)),
    )
    .await
    .map_err(|_| format!("远端分叉超时（{}s）", FORK_TIMEOUT.as_secs()))??;

    let res = interpret_fork_exec(&ex)?;
    tracing::info!(
        "remote_branch: [{origin}] 分叉 {source_session_id}@{message_uuid} → {}",
        res.session_id
    );
    Ok(res)
}

#[cfg(test)]
#[path = "../../../tests/bridge/remote_branch_tests.rs"]
mod tests;
