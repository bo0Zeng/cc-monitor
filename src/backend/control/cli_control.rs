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
//! · 出：stdout 一行紧凑 JSON（命令没有返回值时是 `{}`），exit 0。例外是 [`crate::TEXT_FLAG`]（所有命令通用）：
//!   同一份回包里核心写好的那几格（顶上的 `text` · 每一处 `rows`）拼成给人看的字（`control/ship_text.rs`，不按业务写）。
//! · 期限：[`crate::WITHIN_MS_FLAG`] `<毫秒>`（任意位置）与帧面请求信封的 `within_ms` 同名同义，到点回的是同一个码。
//! · 时区：[`crate::TZ_FLAG`] `<IANA 名>`（任意位置）与帧面请求信封的 `tz` 同名同义：回包里「几点」按看的那一台的钟写；没带 ⇒ UTC。
//! · 错：exit 2 + stderr 一行 `{code, message, detail, data?}` —— 与帧面失败应答同一份（[`Failed`]）；带 [`crate::TEXT_FLAG`] ⇒ 那一句 ＋ 复制详情。
//! · exec 模型：1 exec = 1 请求 1 响应 1 退出，无 request-id。

use crate::common::contract;
use crate::stream::detail::Failed;
use crate::stream::inbound::spec::Fail;
use crate::stream::inbound::{CommandSpec, Run, REGISTRY};
use crate::stream::wire::Request;
use std::io::{Read, Write};
use std::time::Duration;

/// 能力探测口。集成方按能力兼容，不按版本号；出 JSON（不是 `ccm --ccm-probe` 那种 `key=value` 行：那是 ccm 专用的方言）。
pub(crate) const PROBE_FLAG: &str = "--backend-probe";

/// 读入参（两个口 · 上限 · 静默窗 · 码）住 [`super::cli_args`]：`--resolve` 也经它读，而它不该为此引到整张命令表。
use super::cli_args::{bad_args, read_args, STDIN_QUIET};

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
/// `cmd` 进复制详情的「命令」那一项；`message` 可带下层原话（`Said.raw`），原话进复制详情、不上句子。
pub fn emit_err(cmd: &str, code: &str, message: impl Into<copy_core::said::Said>) -> i32 {
    // 这几条（常驻起停那两问）不收 `--tz` ⇒ 详情那一行时刻按 UTC 填（同「没带按 UTC」）。
    emit_failed(
        &mut std::io::stderr(),
        &failed_of(cmd, code, message.into()),
        false,
        &crate::Tz::default(),
    )
}

/// [`emit_err`] 交出去的那一份：那一句进 `message`，下层原话进复制详情（[`Failed::new`] 的 `raw`）。
pub(crate) fn failed_of(cmd: &str, code: &str, s: copy_core::said::Said) -> Failed {
    Failed::new(Some(cmd), code, s.said, s.raw.as_deref(), None)
}

/// 失败那一份投到 CLI 面（[`Failed::emit_to`]：与别的 CLI 出口同一处）。
fn emit_failed(err: &mut dyn Write, f: &Failed, text: bool, tz: &crate::Tz) -> i32 {
    f.emit_to(err, text, tz)
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
///
/// `tz` ＝ 看的那一台的时区（[`crate::TZ_FLAG`]，`main` 剥流旗标那一步剥出来的）：回包里「几点」按它写，同帧面信封的 `tz`。
pub async fn run(args: &[String], tz: &crate::Tz) -> i32 {
    run_io(
        args,
        tz,
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
    tz: &crate::Tz,
    stdin: R,
    quiet: Duration,
    out: &mut dyn Write,
    err: &mut dyn Write,
) -> i32 {
    let flag = args.first().map(String::as_str).unwrap_or_default();
    if flag == PROBE_FLAG {
        return probe();
    }
    let opts = &args[1..];
    // 「给人看」那一形：成功 ＝ 回包里写好的格拼成字（`control/ship_text.rs`，不分命令）；失败那一形 ＝ 那一句 ＋ 复制详情。
    let text = opts.iter().any(|a| a == crate::TEXT_FLAG);
    let Some(spec) = spec_for(flag) else {
        // 走不到（`main` 只把已知 flag 派到这里），但**不许 panic**：
        // backend 的一次性模式对未知参数的既定行为是 exit 2 + 结构化 stderr。
        let f = Failed::new(
            None,
            "unknown_command",
            contract::malformed(&format!("unknown CLI flag {flag}")),
            None,
            None,
        );
        return emit_failed(err, &f, text, tz);
    };
    let refuse = |err: &mut dyn Write, code: &str, message: String| {
        emit_failed(
            err,
            &Failed::new(Some(spec.name), code, message, None, None),
            text,
            tz,
        )
    };
    let within_ms = match within_of(opts) {
        Ok(ms) => ms,
        Err((code, message)) => return refuse(err, code, message),
    };
    // 出口的声明：与帧面同一处解、同一处拒（开跑之前）、同一处裁（成功的应答）。
    let (view, plan) = match view_of(opts)
        .map_err(crate::stream::inbound::spec::Fail::from)
        .and_then(|v| crate::stream::inbound::views::plan_for(spec.name, &v).map(|p| (v, p)))
    {
        Ok(vp) => vp,
        Err(f) => {
            return emit_failed(
                err,
                &f.settle(spec.name, &serde_json::Value::Null),
                text,
                tz,
            )
        }
    };
    let mut input = String::new();
    if reads_stdin(spec) {
        match read_args(opts, stdin, quiet) {
            Ok(s) => input = s,
            Err((code, message)) => return refuse(err, code, message),
        }
    } else if opts.iter().any(|a| a == crate::ARGS_B64_FLAG) {
        // 不收入参的命令带 argv 载荷：用法错，不悄悄忽略（同 `--text`）。
        return refuse(
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
            Err(e) => return refuse(err, "bad_request", format!("args JSON parse failed: {e}")),
        }
    };
    let req = Request {
        id: CLI_REQUEST_ID.to_string(),
        cmd: spec.name.to_string(),
        args: cli_args.clone(),
        within_ms,
        // 声明已在上面解过、拒过（`plan`）；信封里照样带着，与帧面同形。
        view,
        tz: tz.clone(),
        // 与帧面同一处换算：发起方期限从收到起算、减余量换成截止时刻。
        until: crate::stream::inbound::until_of(within_ms),
    };
    // 总期限与帧面同一处装：登记了上限的（都在阻塞档）按上限与发起方期限里早的那个装；别的命令不装（`None`）。
    let total = crate::stream::inbound::install_total(&req);
    // 这几行是本模块的全部：派发落到 `REGISTRY` 自己的 `run`。
    let outcome: Result<Option<serde_json::Value>, Fail> = match spec.run {
        Run::Blocking(f) => f(req).map_err(Fail::from),
        Run::BlockingData(f) => f(req),
        Run::Async(f) => f(req).await.map_err(Fail::from),
        Run::AsyncData(f) => f(req).await,
        Run::Builtin => {
            return refuse(
                err,
                "not_available_in_cli",
                contract::malformed("this command is only served on the frame channel"),
            )
        }
    };
    drop(total);
    match outcome {
        Ok(v) => {
            let v = plan.apply(v).unwrap_or_else(|| serde_json::json!({}));
            // 成功的回包里也可能带复制详情（读不出那一形的 `detail`）：那一行时刻按 `--tz` 填。
            let line = if text {
                crate::control::ship_text::ship_text(&v)
            } else {
                v.to_string()
            };
            let _ = writeln!(out, "{}", crate::common::time::fill_at(&line, tz));
            0
        }
        // 失败那一份与帧面同一处出（「码 → 句」表 · 复制详情 · 按码定形的 `data`）。
        Err(f) => emit_failed(err, &f.settle(spec.name, &cli_args), text, tz),
    }
}

/// 声明口 [`crate::VIEW_FLAG`]（位置不限）⇒ 声明那一团（缺 ＝ `null` ＝ 全量）。值以 `{` 打头 ＝ JSON 原样，否则当 base64 解。
/// 缺值 · 给两次 · 解不出 ⇒ `bad_args`；声明本身认不认得由登记处那一处判（`stream/inbound/views.rs`，同帧面）。
pub(crate) fn view_of(opts: &[String]) -> Result<serde_json::Value, (&'static str, String)> {
    let flag = crate::VIEW_FLAG;
    let mut seen: Option<&str> = None;
    let mut i = 0;
    while i < opts.len() {
        if opts[i] == flag {
            let Some(v) = opts.get(i + 1).filter(|v| !v.starts_with("--")) else {
                return Err(bad_args(&format!("{flag} needs a value")));
            };
            if seen.replace(v.as_str()).is_some() {
                return Err(bad_args(&format!("{flag} given twice")));
            }
            i += 2;
            continue;
        }
        i += 1;
    }
    let Some(v) = seen else {
        return Ok(serde_json::Value::Null);
    };
    let text = if v.trim_start().starts_with('{') {
        v.to_string()
    } else {
        let bytes = crate::stream::wire::b64_decode(v)
            .map_err(|e| bad_args(&format!("{flag}: neither JSON nor base64 ({e})")))?;
        String::from_utf8(bytes).map_err(|e| bad_args(&format!("{flag} is not UTF-8: {e}")))?
    };
    serde_json::from_str(&text).map_err(|e| bad_args(&format!("{flag}: not JSON ({e})")))
}

/// 期限口 [`crate::WITHIN_MS_FLAG`]（位置不限）：缺值 · 给两次 ⇒ `bad_args`（同 [`crate::ARGS_B64_FLAG`]）；
/// 值的读法与帧面信封那一格同一处（[`crate::stream::wire::within_ms_of`]：不是正整数 ⇒ 当没带）。
fn within_of(opts: &[String]) -> Result<Option<u64>, (&'static str, String)> {
    let mut seen: Option<Option<u64>> = None;
    let mut i = 0;
    while i < opts.len() {
        if opts[i] == crate::WITHIN_MS_FLAG {
            let Some(v) = opts.get(i + 1).filter(|v| !v.starts_with("--")) else {
                return Err(bad_args(&format!(
                    "{} needs a value",
                    crate::WITHIN_MS_FLAG
                )));
            };
            let ms = serde_json::from_str::<serde_json::Value>(v)
                .ok()
                .and_then(|v| crate::stream::wire::within_ms_of(&v));
            if seen.replace(ms).is_some() {
                return Err(bad_args(&format!("{} given twice", crate::WITHIN_MS_FLAG)));
            }
            i += 2;
            continue;
        }
        i += 1;
    }
    Ok(seen.flatten())
}

#[cfg(test)]
#[path = "../../../tests/backend/control/cli_control_tests.rs"]
mod tests;
