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
//! **daemon 只收 sid、不收路径**（见 `fork_write::find_session_file` 头注）：daemon 是被
//! ssh 远程调起来的，少一个可被构造的路径入参就少一条路径穿越的攻击面。所以 monitor 这边
//! 拿到的远端 jsonl 路径**不往回传**，只传 sid。

use crate::history::BranchResult;
use crate::ssh_source::{self, RemoteExec};

/// 分叉整体限时。读一份 jsonl + 写一份新文件，正常是毫秒级；30s 是给巨型会话
/// 与慢链路留的余量，同 `remote_history::LIST_TIMEOUT` 的量级。
const FORK_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);

/// 旧 daemon 掉进流模式的判据：查询模式的输出不可能含 wire 的 `"kind":"hello"`。
/// 与 `remote_history::is_old_daemon_hello` 同一判据（那边是逐行判、这边是子串判——
/// 这边拿它当 `abort_marker`，必须能在**半行**上就命中，不能等换行）。
const HELLO_MARKER: &str = r#""kind":"hello""#;

const OLD_DAEMON_MSG: &str =
    "远端 daemon 版本过旧（不支持远端分叉）——请重新部署 daemon 后再试（doc/REMOTE-PHASE0-DEPLOY.md）";

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
/// 字符集与 daemon 侧 `fork_write::valid_sid` 一致（`[A-Za-z0-9-]`）。
fn validate_fork_id(what: &str, s: &str) -> Result<(), String> {
    if s.is_empty() || s.len() > 128 {
        return Err(format!("{what} 长度非法（1..=128）"));
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
    if ex.stdout.contains(HELLO_MARKER) {
        return Err(OLD_DAEMON_MSG.to_string());
    }

    let stderr_detail = || -> Option<String> {
        let line = ex.stderr.lines().map(str::trim).find(|l| !l.is_empty())?;
        match serde_json::from_str::<ForkErrEnvelope>(line) {
            Ok(env) => Some(format!("{}（{}）", env.message, env.code)),
            // 认不出信封 → 原样带出（截断防刷屏），比吞掉强。
            Err(_) => Some(line.chars().take(400).collect()),
        }
    };

    match ex.exit_status {
        Some(0) => {
            let line = ex
                .stdout
                .lines()
                .map(str::trim)
                .find(|l| !l.is_empty())
                .ok_or("远端分叉：daemon 报成功却没有输出结果")?;
            serde_json::from_str::<BranchResult>(line)
                .map_err(|e| format!("远端分叉：结果解析失败（{e}）：{line}"))
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
/// 与本地那条（`history::create_branch_session`）的差异只有两处：**收 sid 不收路径**、
/// **活儿在远端干**。返回体同形，所以前端两条路共用同一段成功处理。
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
mod tests {
    use super::*;

    fn ex(stdout: &str, stderr: &str, status: Option<u32>) -> RemoteExec {
        RemoteExec {
            stdout: stdout.to_string(),
            stderr: stderr.to_string(),
            exit_status: status,
        }
    }

    const OK_LINE: &str =
        r#"{"sessionId":"new-sid-1","jsonlPath":"/home/pi/.claude/projects/p/new-sid-1.jsonl"}"#;

    #[test]
    fn exit_zero_parses_result() {
        let r = interpret_fork_exec(&ex(OK_LINE, "", Some(0))).unwrap();
        assert_eq!(r.session_id, "new-sid-1");
        assert!(r.jsonl_path.ends_with("new-sid-1.jsonl"));
    }

    /// ★ 失败必须**可见**：daemon 的 `{code,message}` 要变成人话，不能静默。
    #[test]
    fn exit_two_surfaces_daemon_message() {
        let e = interpret_fork_exec(&ex(
            "",
            r#"{"code":"fork_failed","message":"refuse fork: message uuid not found"}"#,
            Some(2),
        ))
        .unwrap_err();
        assert!(e.contains("message uuid not found"), "got: {e}");
        assert!(e.contains("fork_failed"), "错误码也要带上: {e}");
    }

    /// 认不出信封时**原样带出**，而不是吞成一句泛泛的失败。
    #[test]
    fn unrecognized_stderr_is_passed_through() {
        let e = interpret_fork_exec(&ex("", "bash: line 1: ccmd: command not found", Some(127)))
            .unwrap_err();
        assert!(e.contains("command not found"), "got: {e}");
    }

    /// ★★ 没收到退出码**绝不当成 0**。真出现过的形态：连接被中途掐断，stdout 是空的，
    /// 若把 `None` 当 0 就会走进「报成功却没有输出」那条，措辞会把锅甩给 daemon。
    #[test]
    fn missing_exit_status_is_failure_not_success() {
        let e = interpret_fork_exec(&ex("", "", None)).unwrap_err();
        assert!(e.contains("没收到退出码"), "got: {e}");
    }

    /// exit 0 但 stdout 解析不出来 —— 仍是失败，**绝不**返回空壳结果。
    #[test]
    fn exit_zero_with_garbage_stdout_fails() {
        let e = interpret_fork_exec(&ex("not json at all", "", Some(0))).unwrap_err();
        assert!(e.contains("解析失败"), "got: {e}");
        let e2 = interpret_fork_exec(&ex("", "", Some(0))).unwrap_err();
        assert!(e2.contains("没有输出结果"), "got: {e2}");
    }

    /// ★ 旧 daemon 不认参数会进流模式、先吐 hello 帧。必须**先于**解析判掉，
    /// 否则用户看到的是「结果解析失败」而不是「去重新部署 daemon」。
    #[test]
    fn old_daemon_hello_is_detected_first() {
        let hello = r#"{"kind":"hello","v":1,"build_id":"old"}"#;
        let e = interpret_fork_exec(&ex(hello, "", Some(0))).unwrap_err();
        assert!(e.contains("版本过旧"), "got: {e}");
    }

    /// `HELLO_MARKER` 同时是 `connect_and_exec_capture` 的 abort marker——
    /// 它必须能在**没有换行**的半行上命中，所以只能是子串判、不能是整行判。
    #[test]
    fn hello_marker_matches_partial_line() {
        let partial = r#"{"kind":"hello","v":1,"build_i"#; // 半行，无 \n
        assert!(partial.contains(HELLO_MARKER));
    }

    #[test]
    fn fork_ids_are_whitelisted() {
        assert!(validate_fork_id("sid", "0473c3a0-1111-2222-3333-444455556666").is_ok());
        for bad in ["", "../etc/passwd", "a b", "a;rm -rf /", "a'b", "a/b"] {
            assert!(validate_fork_id("sid", bad).is_err(), "该拒: {bad:?}");
        }
        assert!(validate_fork_id("sid", &"a".repeat(129)).is_err());
    }

    /// 命令形状 = daemon 的 argv 契约。**位置参数顺序错了 daemon 会拿 uuid 当 sid 去找文件**，
    /// 于是报一句「找不到会话」，排查方向被带偏一整轮——所以钉死。
    #[test]
    fn fork_cmd_shape_is_pinned() {
        let c = build_fork_cmd("/home/pi/.cc-monitor/bin/p1q", "src-sid", "msg-uuid");
        assert_eq!(
            c,
            "'/home/pi/.cc-monitor/bin/p1q' --fork-session 'src-sid' 'msg-uuid'"
        );
    }

    /// 纵深防御：id 已过白名单，`shell_quote` 仍照上（白名单哪天被放宽也不至于直接漏）。
    #[test]
    fn daemon_path_with_space_is_quoted() {
        let c = build_fork_cmd("/opt/my daemons/p1q", "s", "u");
        assert!(c.starts_with("'/opt/my daemons/p1q' --fork-session"), "{c}");
    }
}
