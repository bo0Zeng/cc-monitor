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
//! · 入：那条命令的 `args`（一段 JSON；空 = `{}`），两个口**二选一**，都是一处读（[`read_args`]，住 `cli_args`），每条子命令自动都有：
//!   - stdin：默认读到 EOF；带 [`crate::STDIN_LINE_FLAG`]（任意位置）⇒ 只读一行（给 stdin 关不掉的调用方：远端命令经 capture 那一跳交载荷，
//!     capture 不关远端 stdin）。stdin 开着却 [`super::cli_args::STDIN_QUIET`] 内一个字节都没来 ⇒ `no_input`，不挂住。
//!   - argv：[`crate::ARGS_B64_FLAG`] `<base64 的 JSON>`（任意位置；给写不了 stdin 的调用方：第二个前端的执行通道只有 stdout）。
//!     给了它就**不碰 stdin**；与 [`crate::STDIN_LINE_FLAG`] 一起给 ⇒ `bad_args`。
//!   上限：stdin [`super::cli_args::MAX_CLI_STDIN`]；argv [`super::cli_args::MAX_ARGS_B64_LEN`]（编码后，留在系统单个参数的上限之内）。超了 `args_too_large`，不截断。
//! · 出：stdout 一行紧凑 JSON（命令没有返回值时是 `{}`），exit 0。例外是 [`crate::TEXT_FLAG`]：只给 `quota-read`，
//!   同一份回包排成给人看的字（`control/quota_text.rs`）；别的命令带它 ⇒ `bad_args`。
//! · 错：exit 2 + stderr 一行 `{"code","message"}`。
//! · exec 模型：1 exec = 1 请求 1 响应 1 退出，无 request-id。

use crate::common::contract;
use crate::stream::inbound::{CommandSpec, Run, REGISTRY};
use crate::stream::wire::Request;
use std::io::{Read, Write};
use std::time::Duration;

/// 能力探测口。集成方按能力兼容，不按版本号；出 JSON（不是 `ccm --ccm-probe` 那种 `key=value` 行：那是 ccm 专用的方言）。
pub(crate) const PROBE_FLAG: &str = "--backend-probe";

/// 读入参（两个口 · 上限 · 静默窗 · 码）住 [`super::cli_args`]：`--resolve` 也经它读，而它不该为此引到整张命令表。
use super::cli_args::{read_args, STDIN_QUIET};

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
/// 第三条：[`STREAM_ONLY`] 那几条能跑，但在一次性进程里结构上答不了 ⇒ 不上；[`UI_ONLY`] 那几条答得了、只是除了界面没人用 ⇒ 暂不上。
pub(crate) fn cli_exposed(spec: &CommandSpec) -> bool {
    !matches!(spec.run, Run::Builtin)
        && !crate::control::ccm::argv::is_ccm_word(&flag_of(spec.name))
        && !STREAM_ONLY.contains(&spec.name)
        && !UI_ONLY.contains(&spec.name)
}

/// **一次性进程里结构上答不了**的命令：答出来是假话（读的是常驻进程才有的状态：watcher · 中转监听 · 转发账 · 可达表），
/// 或做的事活不过这个进程（换号重启要拿退出排空的票钉住常驻进程）。逐条理由是数据，住测试那一侧的 `STREAM_ONLY_WHY`（两向相等）。
///
/// 只收这一种理由。「答得了，只是除了界面没人用」是另一种，住 [`UI_ONLY`] —— 两种混在一张表里时，
/// 后一种被读成前一种，第二个前端（只有一次性 CLI 与流两条路进后端）就够不着。
pub(crate) const STREAM_ONLY: &[&str] = &[
    "resync",
    "apikey-routing",
    "relay-optin",
    "launch-local",
    "forward-start",
    "forward-stop",
    "forward-list",
    "ext-list",
    "ext-hub-preview",
    "ext-hub-apply",
    "session-restart",
];

/// **答得出真话、只是除了界面没人用得着**的命令。这是关于「调用方是谁」的产品判断，不是结构限制 ——
/// 有新的调用方（第二个前端）要它，就挪出去。理由不许是「命令行那一侧直接敲 `ccm` 就是它」：那挡的是第二个**入口**，挡不住第二个**前端**。
/// 逐条理由住测试那一侧的 `UI_ONLY_WHY`（两向相等）。
///
/// 2026-10-09 复核（第二个前端的审计 §4）：起新会话框三问 · tab 栏批量停 / 起 / 问样子 · 铸终端名 · 开终端那一串 · 各台搜索结果合并
/// 九条放出（事实全在这台：tmux 名单现探、会话快照问一次重探一次、记录在不在现读、合并是纯计算 ⇒ 一次性进程答的与常驻那一个同样是真话）。
/// 留下的两条是 ↗ 拉前那一问的两半，只有能拉前桌面窗口的那一方用得着，第二个前端自己也说不用。
pub(crate) const UI_ONLY: &[&str] = &["session-terminals", "terminal-processes"];

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
pub fn emit_err(code: &str, message: impl Into<copy_core::said::Said>) -> i32 {
    emit_err_to(&mut std::io::stderr(), code, message)
}

fn emit_err_to(err: &mut dyn Write, code: &str, message: impl Into<copy_core::said::Said>) -> i32 {
    let _ = writeln!(err, "{}", err_body(code, &message.into()));
    2
}

/// 失败信封 `{code, message, raw?}`：`message` 是给人看的那一句；`raw` 是下层原话（有才带），读的那一方放进复制详情、不上句子。
pub(crate) fn err_body(code: &str, s: &copy_core::said::Said) -> serde_json::Value {
    let mut body = serde_json::json!({ "code": code, "message": s.said });
    if let Some(r) = &s.raw {
        body["raw"] = serde_json::Value::String(r.clone());
    }
    body
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
    run_io(
        args,
        std::io::stdin(),
        STDIN_QUIET,
        &mut std::io::stdout(),
        &mut std::io::stderr(),
    )
    .await
}

/// [`run`] 的本体：stdin / stdout / stderr 是入参（测试喂替身，逐字比两个口的应答）。
pub(crate) async fn run_io<R: Read + Send + 'static>(
    args: &[String],
    stdin: R,
    quiet: Duration,
    out: &mut dyn Write,
    err: &mut dyn Write,
) -> i32 {
    let flag = args.first().map(String::as_str).unwrap_or_default();
    if flag == PROBE_FLAG {
        return probe();
    }
    let Some(spec) = spec_for(flag) else {
        // 走不到（`main` 只把已知 flag 派到这里），但**不许 panic**：
        // backend 的一次性模式对未知参数的既定行为是 exit 2 + 结构化 stderr。
        return emit_err_to(
            err,
            "unknown_command",
            contract::malformed(&format!("unknown CLI flag {flag}")),
        );
    };
    let opts = &args[1..];
    // 「给人看」那一形只给 `quota-read`（`control/quota_text.rs`）；别的命令带它 ⇒ 用法错，不悄悄忽略。
    let text = opts.iter().any(|a| a == crate::TEXT_FLAG);
    if text && spec.name != "quota-read" {
        return emit_err_to(
            err,
            "bad_args",
            copy_core::copy_text("acct.text.onlyQuota", &[]),
        );
    }
    let mut input = String::new();
    if reads_stdin(spec) {
        match read_args(opts, stdin, quiet) {
            Ok(s) => input = s,
            Err((code, message)) => return emit_err_to(err, code, message),
        }
    } else if opts.iter().any(|a| a == crate::ARGS_B64_FLAG) {
        // 不收入参的命令带 argv 载荷：用法错，不悄悄忽略（同 `--text`）。
        return emit_err_to(
            err,
            "bad_args",
            contract::malformed(&format!("{flag} takes no args")),
        );
    }
    let trimmed = input.trim();
    let cli_args: serde_json::Value = if trimmed.is_empty() {
        serde_json::json!({})
    } else {
        match serde_json::from_str(trimmed) {
            Ok(v) => v,
            Err(e) => {
                return emit_err_to(err, "bad_request", format!("args JSON parse failed: {e}"))
            }
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
            return emit_err_to(
                err,
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
                let _ = writeln!(out, "{}", crate::control::quota_text::render_here(&v));
            } else {
                let _ = writeln!(out, "{v}");
            }
            0
        }
        Err((code, message)) => emit_err_to(err, &code, message),
    }
}

#[cfg(test)]
#[path = "../../../tests/backend/control/cli_control_tests.rs"]
mod tests;
