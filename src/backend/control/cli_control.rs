//! P4d：控制面的 **CLI 入口** —— 与 SSH 帧入口共用 [`crate::stream::inbound::REGISTRY`] 里同一个 `run`。
//!
//! 〔用 08-12〕「要给后端留暴露接口……以后集成的 skill 就靠着后端来兼容和集成」
//! 「**先把确切的命令组件做出来**，外面怎么变后面再说」。
//!
//! # 它**不实现任何一条命令**，这是全部要点
//!
//! 读面早就暴露了（14 条一次性子命令）；缺的只是控制面那半 —— `launch` / `kill`
//! **只走 SSH 帧那一条**，bash 脚本调不到。本模块补的是**入口**，不是逻辑。
//!
//! 所以它一条命令都不自己写：把 `--<name>` 还原成 `<name>`，去 `REGISTRY` 查那条
//! `CommandSpec`，跑它自己的 `run`。**两个入口在类型上就落到同一个闭包**，
//! 不是「我记得要调同一个函数」。定框 `C1` 排除的正是「给本地单写一套控制逻辑」。
//!
//! ⇒ 由此白拿两条性质：
//! · `readonly_guard::spawn_registry` 的起进程点**一处不增**
//!   （本模块零 `Child::new`；起进程仍只发生在 `control/launch.rs`、`control/kill.rs`）；
//! · `launch.rs` 头注那条「argv 直传，不过 shell」的性质原样继承 ——
//!   CLI 面收 **stdin JSON** 而不是把参数摊进 argv，正是为了不引入一层 shell 解析把它丢掉。
//!
//! # §安全边界：这条入口的理由与帧入口**不是同一条**
//!
//! `control/launch.rs` 那句「这里的校验是形状校验，不是安全边界」，它的依据是
//! **帧入口的对端身份**。本入口的调用方是**本机任意进程**，那条依据在这里不成立，
//! 照抄过来就是一句没人验过的话。本入口自己的理由是另一条，写在这里备核：
//!
//! > 本机进程已经能直接跑 `tmux kill-session` / `tmux new-session` —— 它们与 backend
//! > 跑的是同一个 tmux server，用的是同一个 `$TMUX_TMPDIR` 下的 socket，权限由文件系统
//! > 而不是由后端把守。⇒ 本入口**不新授任何权力**，它只是把「backend 已经会做的事」
//! > 换一种调用法。真正的边界在 tmux socket 的文件权限上，一直如此。
//!
//! ⚠ 这条理由的**射程**：它只覆盖「起/杀 tmux 会话」这一族（今天 `REGISTRY` 上 CLI 面的全部）。
//! 将来若有一条命令能做**本机进程原本做不到**的事（碰别的用户的东西、越过某道门），
//! 上面那句话对它**不成立**，必须单独立一条理由 —— 别默认它跟着继承。
//!
//! # 信封（与 `--resolve` 逐条同形，不新发明）
//!
//! · **入**：stdin 一段 JSON = 那条命令的 `args`（空 stdin = `{}`，`ping` 那类无载荷的用得上）。
//!   默认读到 EOF；子命令后面跟 [`STDIN_LINE_FLAG`] ⇒ **只读一行**（读到第一个换行就停，不等 EOF）——
//!   给「stdin 关不掉」的调用方用（远端命令经 capture 那一跳交载荷，capture 不关远端 stdin）。
//! · **出**：stdout 一行紧凑 JSON（命令没有返回值时是 `{}`），exit 0。
//! · **错**：exit 2 + stderr 一行 `{"code","message"}`。
//! · **exec 模型**：1 exec = 1 请求 1 响应 1 退出，**无 request-id**（`resolve_query` 头注逐字）。

use crate::common::contract;
use crate::stream::inbound::{CommandSpec, Run, REGISTRY};
use crate::stream::wire::Request;
use std::io::Read;

/// stdin 上限。理由抄 `resolve_query::MAX_RESOLVE_STDIN`：兜 DoS，不是兜格式。
///
/// ⚠ **超限是拒收，不是截断。** 第一版写的是 `.take(MAX_CLI_STDIN)` —— 那是**静默截断**：
/// 截半的 JSON 解析失败 ⇒ 回一句 `bad_request: args JSON parse failed`，
/// 而真实原因是「太大了」。`byte_cap_registry` 当场逮住这一处（本轮第十七次），
/// 它的 `ALLOWED_SEMANTICS` 里逐字**没有「静默截断」这一项**。
/// ⇒ 多读一个字节，超了就说超了。
pub(crate) const MAX_CLI_STDIN: u64 = 1024 * 1024;

/// 能力探测口。
///
/// # ⚠ 「范式抄 `ccm --ccm-probe`」抄的是**理念**，不是**线格式**〔E 阶段订正 08-12〕
///
/// 本件 `Y2` 的原话是「范式抄 `ccm --ccm-probe`，而 monitor 侧
/// `ccm_probe::parse_probe_output` **已经会解析那个形状**」——**后半句是假的**，
/// 而且是写下时就没验过的那种假（`P3b §0b` 的 B 类）。实测：
/// `parse_probe_output` 认的是 **`key=value` 行**（首行必须逐字 `name=ccm`，
/// 然后 `version=` / `capabilities=a,b,c`），而本口出的是 **JSON**。两者对不上。
///
/// 保留 JSON 而不是去迁就那个解析器，理由有账：`§0c` ① 是用户对 cc-bus 的不满逐字
/// 「五个命令**全无 `--json`**，输出是定宽 `printf` + 中文表头」⇒ **JSON 进 JSON 出，
/// 第一天就有**。而 `parse_probe_output` 是 **`ccm` 专用**的（实测：backend 侧零消费者，
/// monitor 也不用 CLI 面 —— 它走帧），让后端去说 ccm 的方言只会多一种方言。
///
/// ⇒ 真正抄过来的是那条**理念**：集成方按**能力**兼容，不按版本号。
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

/// **argv 一族也走「只读一行」**：`<后端> --<老子命令> --stdin-line`（恰好这两个词、且不是帧命令）⇒
/// stdin 读一行（同 [`read_input`] 的一行形与上限），那一行是**其余 argv 的 JSON 字符串数组**，拼回去照常分派。
/// 于是自由文本（项目目录名 …）不进远端命令行 —— 远端登录 shell 是 fish 之类也同读。别的形状原样返回、一个字节不读。
pub fn expand_stdin_argv<R: std::io::BufRead>(
    args: Vec<String>,
    r: R,
) -> Result<Vec<String>, (&'static str, String)> {
    let [sub, modifier] = args.as_slice() else {
        return Ok(args);
    };
    if modifier != STDIN_LINE_FLAG || handles(sub) {
        return Ok(args);
    }
    let line = read_input(r, true)?;
    let rest: Vec<String> = serde_json::from_str(line.trim()).map_err(|e| {
        (
            "bad_request",
            contract::malformed(&format!(
                "{sub} {STDIN_LINE_FLAG}: stdin line is not a JSON array of strings: {e}"
            )),
        )
    })?;
    Ok(std::iter::once(sub.clone()).chain(rest).collect())
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
    // tab 栏多选的批量停 / 起：一批会话一次问，只给界面用（命令行那一侧逐个 `--kill` / 直接敲 `ccm` 就是它们）。
    "sessions-stop",
    "sessions-start",
    "sessions-where",
    // 换号重启：要等压缩、等会话报出（几分钟），界面关了那台照样做完 —— 那是常驻流上的事；命令行那一侧逐个 `--kill` 再敲 `ccm --resume` 就是它。
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

/// 这条命令要不要读 stdin —— **从 `REGISTRY` 的 `fields` 派生**。
///
/// # 它修的是一条真缺陷：存活探测口会挂死
///
/// 第一版无条件读 stdin。实测（stdin 接一条不关的管道，也就是 skill 直接
/// `cc-monitor-backend --ping` 时的形状）：**`--ping` 永远不返回**。
/// 而这是所有失败里最坏的一种 —— 问「你活着吗」的那条命令，答案是挂住。
///
/// ⚠⚠ **第二版**：原来这里写的是 `!spec.fields.is_empty()` —— 那是个**代用品**，
/// 在当时的命令集上恰好全对，而 `bus-list`（**无输入、有输出字段**）一来就错，
/// **它挂住等一个永远不来的输入**（实测 `--ping` 120ms 回、`--bus-list` 被掐死才停）。
/// ⇒ 改读 `spec.takes_input`（每条命令自己说）。理由全文在 `CommandSpec::takes_input` 头注。
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

/// 能力探测：`{proto, buildId, commands}`。
///
/// ★ `commands` **必须派生**。手抄一份的后果不是编译错，是**探测口开始说谎** ——
/// 而 skill 正是按它的话决定走不走新路的，⇒ 那种失效是静默的降级，不是报错。
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
    };
    // ★ 这三行是本模块的全部：**派发落到 `REGISTRY` 自己的 `run`**。
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
    match outcome {
        Ok(v) => {
            println!("{}", v.unwrap_or_else(|| serde_json::json!({})));
            0
        }
        Err((code, message)) => emit_err(&code, message),
    }
}

#[cfg(test)]
#[path = "../../../tests/backend/control/cli_control_tests.rs"]
mod tests;
