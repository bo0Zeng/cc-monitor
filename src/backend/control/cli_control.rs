//! 控制面的 CLI 入口：与 SSH 帧入口共用 [`crate::stream::inbound::REGISTRY`] 里同一个 `run`。给本机脚本与 skill 集成用。
//!
//! # 它不实现任何一条命令
//!
//! 把 `--<name>` 还原成 `<name>`，去 `REGISTRY` 查那条 `CommandSpec`，跑它自己的 `run` —— 两个入口在类型上落到同一个闭包。
//! 由此：`readonly_guard::spawn_registry` 的起进程点一处不增（本模块零 `Child::new`）；
//! `launch.rs` 那条「argv 直传，不过 shell」原样继承 —— CLI 面收 stdin JSON，不把参数摊进 argv。
//!
//! # §安全边界：理由与帧入口不是同一条
//!
//! 本入口的调用方是本机任意进程。理由是：本机进程本来就能直接跑 `tmux kill-session` / `tmux new-session`
//! （同一个 tmux server、同一个 `$TMUX_TMPDIR` 下的 socket，权限由文件系统把守）⇒ 本入口不新授任何权力。
//! 这条理由只覆盖「起 / 杀 tmux 会话」这一族；将来若有一条命令能做本机进程原本做不到的事，必须单独立一条理由。
//!
//! # 信封（与 `--resolve` 同形）
//!
//! · 入：stdin 一段 JSON = 那条命令的 `args`（空 stdin = `{}`）。默认读到 EOF；子命令后面跟 [`STDIN_LINE_FLAG`] ⇒ 只读一行
//!   （给 stdin 关不掉的调用方：远端命令经 capture 那一跳交载荷，capture 不关远端 stdin）。
//! · 出：stdout 一行紧凑 JSON（命令没有返回值时是 `{}`），exit 0。例外是 [`crate::TEXT_FLAG`]：只给 `quota-read`，
//!   同一份回包排成给人看的字（`control/quota_text.rs`）；别的命令带它 ⇒ `bad_args`。
//! · 错：exit 2 + stderr 一行 `{"code","message"}`。
//! · exec 模型：1 exec = 1 请求 1 响应 1 退出，无 request-id。

use crate::common::contract;
use crate::stream::inbound::{CommandSpec, Run, REGISTRY};
use crate::stream::wire::Request;
use std::io::Read;

/// stdin 上限（兜 DoS，不是兜格式；同 `resolve_query::MAX_RESOLVE_STDIN`）。超限是拒收，不是截断：
/// 截半的 JSON 会报成「解析失败」，真实原因却是「太大了」⇒ 多读一个字节，超了就说超了。
pub(crate) const MAX_CLI_STDIN: u64 = 1024 * 1024;

/// 能力探测口。集成方按能力兼容，不按版本号；出 JSON（不是 `ccm --ccm-probe` 那种 `key=value` 行：那是 ccm 专用的方言）。
pub(crate) const PROBE_FLAG: &str = "--backend-probe";

/// 「只读一行 stdin」那个修饰词住 [`crate::STDIN_LINE_FLAG`]（argv 三分表那一家；理由见那里的头注）。
pub(crate) use crate::STDIN_LINE_FLAG;

/// 读入参那一段（可喂任意读端 —— 生产交进程的 stdin）。`one_line` ⇒ 读到第一个换行就停，**不再多要一个字节**
/// （调用方的 stdin 可能永远不关）；否则读到 EOF。两形同一个上限、同一种拒法。
pub(crate) fn read_input<R: std::io::BufRead>(
    r: R,
    one_line: bool,
) -> Result<String, (&'static str, String)> {
    let mut buf: Vec<u8> = Vec::new();
    // 多读一个字节，好把「刚好装满」与「超了」分开 —— 只读上限那么多是分不开的。
    let mut capped = r.take(MAX_CLI_STDIN + 1);
    let got = if one_line {
        std::io::BufRead::read_until(&mut capped, b'\n', &mut buf)
    } else {
        capped.read_to_end(&mut buf)
    };
    if let Err(e) = got {
        return Err(("stdin_read_failed", format!("read stdin failed: {e}")));
    }
    if buf.len() as u64 > MAX_CLI_STDIN {
        return Err((
            "args_too_large",
            // 不截断：截半的 JSON 会被报成 bad_request，那句话与真实原因无关。
            contract::malformed(&format!(
                "args JSON over the {MAX_CLI_STDIN}-byte cap, refused (not truncated)"
            )),
        ));
    }
    String::from_utf8(buf).map_err(|e| ("stdin_read_failed", format!("read stdin failed: {e}")))
}

/// 本入口回显给命令的 `id`。**帧面的 `id` 由客户端发号且不透明**，而一次性 exec
/// 天然 1:1、没有并发的第二条请求可混淆 ⇒ 这里给一个固定值，不假装有号段。
const CLI_REQUEST_ID: &str = "cli";

/// 一条命令**上不上 CLI 面** —— 从 `REGISTRY` 派生，不是手抄一张表。
///
/// 判据是 `Run::Builtin`：那种命令的实现住在 `inbound::dispatch` 的硬臂里，
/// 要 `replies` 通道与在飞表才能跑，而**一次性 exec 里两样都不存在**。
///
/// 另一条：派生出来的名字是 ccm 自己的诊断口（`--ccm-print` 这类）⇒ 不上。二进制叫 `ccm` 时
/// 按 `SUBCOMMANDS` 分流（`control::ccm::intercept`），占了 ccm 的词就把 `ccm --ccm-print` 抢进后端。
/// 第三条：[`STREAM_ONLY`] 那几条能跑，但在一次性进程里答的是假话 ⇒ 不上。
pub(crate) fn cli_exposed(spec: &CommandSpec) -> bool {
    !matches!(spec.run, Run::Builtin)
        && !crate::control::ccm::argv::is_ccm_word(&flag_of(spec.name))
        && !STREAM_ONLY.contains(&spec.name)
}

/// **只在流面上有意义**的命令。
/// `resync` 对齐的是本进程里在跑的 watcher；一次性 exec 里一份都没有 ⇒ 只能答 `watchers: 0`，那是假话。
/// `apikey-routing` 同理：「中转在不在」读的是本进程的监听状态，一次性进程里没有中转 ⇒ 恒答「不在」。
/// `launch-local` 只给界面用（它回的身份 token 要交回界面去回填 sid）。
/// `forward-*` 同理：转发账住本进程（常驻那一个）；一次性进程开出来的转发随进程退出就没了、列出来恒空。
/// `session-restart` 不是「答假话」那一类：它要的是**进程活得比发起方久**（退出排空的票 ＋ 两次几分钟的等待），一次性 exec 给不了。
pub(crate) const STREAM_ONLY: &[&str] = &[
    "resync",
    "apikey-routing",
    // 「直接敲的也走中转」那一段的「中转在不在」同样读本进程的监听状态。
    "relay-optin",
    "launch-local",
    "forward-start",
    "forward-stop",
    "forward-list",
    // 起会话要的终端名：界面问；CLI 那一侧 `ccm` 起会话时自己铸（同一份 `plan::mint_tmux_name`）。
    "terminal-name-mint",
    // 开终端那一串：界面 / 文件窗口开 PowerShell 窗口前问；命令行那一侧用不着（它自己就在终端里）。
    "terminal-ssh",
    // ↗ 那一问：那台答「此刻谁在显示这个会话」—— 只有拉前那一方（界面）用得着。
    "session-terminals",
    // ↗ 那一问的本机一半：同上，只有拉前那一方用得着。
    "terminal-processes",
    // 各台搜索结果合成一份：界面逐台问完才有得合；命令行那一侧 `--search` 只问这一台，用不着合。
    "history-search-merge",
    // 扩展页那张表与「装」的枢纽：读本进程的可达表（远端那几台叫什么、怎么够得着）；一次性进程里那张表是空的 ⇒ 只剩本机、答的是假话。
    "ext-list",
    "ext-hub-preview",
    "ext-hub-apply",
    // 换号重启：停旧 ＋ 起新那一步拿**退出排空的票**（`inbound::DRAIN`），等压缩摘要、等新进程报出各一次有界等待（各 ≤ 1 h）——
    //   「发起方走了那台照样做完」靠的就是那张票钉住常驻进程不退；一次性 exec 里那张票钉的是**命令自己那个进程**，钉不住任何东西，
    //   而调用方得把一个 exec 挂在那儿几分钟。⇒ 这一条真依赖流语义，不是「历史上没人从 CLI 调」。
    "session-restart",
];

/// 命令名 → CLI 子命令（`launch` → `--launch`）。
pub(crate) fn flag_of(name: &str) -> String {
    crate::cli_flag(name)
}

/// CLI 子命令 → 它在 `REGISTRY` 上的那条登记。
pub(crate) fn spec_for(flag: &str) -> Option<&'static CommandSpec> {
    REGISTRY
        .iter()
        .find(|s| cli_exposed(s) && flag_of(s.name) == flag)
}

/// 这条命令要不要读 stdin：读 `spec.takes_input`（每条命令自己说，理由在 `CommandSpec::takes_input` 头注）。
/// 无条件读会让 `--ping` 这类探活口在一条不关的 stdin 上永远挂住；拿「有没有字段」代用也不对（`bus-list` 无输入、有输出字段）。
pub(crate) fn reads_stdin(spec: &CommandSpec) -> bool {
    spec.takes_input
}

/// 本入口认不认这个 flag。`main` 的分派臂只问它，**不写命令字面量** ——
/// 于是「帧面加一条命令」不需要回来改 `main`。
pub fn handles(flag: &str) -> bool {
    flag == PROBE_FLAG || spec_for(flag).is_some()
}

/// `control/resident.rs` 那两条子命令也走这一份（不另立第 N 份信封，`readonly_guard::error_envelope_registry`）。
pub fn emit_err(code: &str, message: impl Into<String>) -> i32 {
    let body = serde_json::json!({ "code": code, "message": message.into() });
    eprintln!("{body}");
    2
}

/// 能力探测：`{proto, buildId, commands}`。`commands` 必须派生：手抄一份，探测口就会说谎，而 skill 按它的话决定走不走新路。
fn probe() -> i32 {
    let commands: Vec<String> = REGISTRY
        .iter()
        .filter(|s| cli_exposed(s))
        .map(|s| flag_of(s.name))
        .collect();
    let body = serde_json::json!({
        "proto": crate::PROTO_VERSION,
        "buildId": crate::BUILD_ID,
        "commands": commands,
    });
    println!("{body}");
    0
}

/// CLI 控制面的一次性入口。返回进程退出码。
pub async fn run(args: &[String]) -> i32 {
    let flag = args.first().map(String::as_str).unwrap_or_default();
    if flag == PROBE_FLAG {
        return probe();
    }
    let Some(spec) = spec_for(flag) else {
        // 走不到（`main` 只把已知 flag 派到这里），但**不许 panic**：
        // backend 的一次性模式对未知参数的既定行为是 exit 2 + 结构化 stderr。
        return emit_err(
            "unknown_command",
            contract::malformed(&format!("unknown CLI flag {flag}")),
        );
    };
    // 「给人看」那一形只给 `quota-read`（`control/quota_text.rs`）；别的命令带它 ⇒ 用法错，不悄悄忽略。
    let text = args[1..].iter().any(|a| a == crate::TEXT_FLAG);
    if text && spec.name != "quota-read" {
        return emit_err("bad_args", copy_core::copy_text("acct.text.onlyQuota", &[]));
    }
    let mut input = String::new();
    if reads_stdin(spec) {
        let one_line = args.get(1).map(String::as_str) == Some(STDIN_LINE_FLAG);
        match read_input(std::io::stdin().lock(), one_line) {
            Ok(s) => input = s,
            Err((code, message)) => return emit_err(code, message),
        }
    }
    let trimmed = input.trim();
    let cli_args: serde_json::Value = if trimmed.is_empty() {
        serde_json::json!({})
    } else {
        match serde_json::from_str(trimmed) {
            Ok(v) => v,
            Err(e) => return emit_err("bad_request", format!("args JSON parse failed: {e}")),
        }
    };
    let req = Request {
        id: CLI_REQUEST_ID.to_string(),
        cmd: spec.name.to_string(),
        args: cli_args,
        within_ms: None,
        until: None,
    };
    // 总期限与帧面同一处装：登记了上限的（都在阻塞档）按上限装，CLI 面没有发起方期限；别的命令不装（`None`）。
    let total = crate::stream::inbound::install_total(&req);
    // 这三行是本模块的全部：派发落到 `REGISTRY` 自己的 `run`。
    let outcome = match spec.run {
        Run::Blocking(f) => f(req),
        Run::BlockingData(f) => f(req).map_err(|f| (f.code, f.message)),
        Run::Async(f) => f(req).await,
        Run::AsyncData(f) => f(req).await.map_err(|f| (f.code, f.message)),
        Run::Builtin => {
            return emit_err(
                "not_available_in_cli",
                contract::malformed("this command is only served on the frame channel"),
            )
        }
    };
    drop(total);
    match outcome {
        Ok(v) => {
            let v = v.unwrap_or_else(|| serde_json::json!({}));
            if text {
                println!("{}", crate::control::quota_text::render_here(&v));
            } else {
                println!("{v}");
            }
            0
        }
        Err((code, message)) => emit_err(&code, message),
    }
}

#[cfg(test)]
#[path = "../../../tests/backend/control/cli_control_tests.rs"]
mod tests;
