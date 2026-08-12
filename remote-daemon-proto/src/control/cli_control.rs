//! P4d：控制面的 **CLI 入口** —— 与 SSH 帧入口共用 [`crate::inbound::REGISTRY`] 里同一个 `run`。
//!
//! 〔用 08-12〕「要给 daemon 留暴露接口……以后集成的 skill 就靠着 daemon 来兼容和集成」
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
//! · `readonly_guard::spawn_registry` 的 `SPAWN_SITES_TODAY` **一处不增**
//!   （本模块零 `Command::new`；起进程仍只发生在 `control/launch.rs`、`control/kill.rs`）；
//! · `launch.rs` 头注那条「argv 直传，不过 shell」的性质原样继承 ——
//!   CLI 面收 **stdin JSON** 而不是把参数摊进 argv，正是为了不引入一层 shell 解析把它丢掉。
//!
//! # §安全边界：这条入口的理由与帧入口**不是同一条**〔P4d-Y4〕
//!
//! `control/launch.rs` 那句「这里的校验是形状校验，不是安全边界」，它的依据是
//! **帧入口的对端身份**。本入口的调用方是**本机任意进程**，那条依据在这里不成立，
//! 照抄过来就是一句没人验过的话。本入口自己的理由是另一条，写在这里备核：
//!
//! > 本机进程已经能直接跑 `tmux kill-session` / `tmux new-session` —— 它们与 daemon
//! > 跑的是同一个 tmux server，用的是同一个 `$TMUX_TMPDIR` 下的 socket，权限由文件系统
//! > 而不是由 daemon 把守。⇒ 本入口**不新授任何权力**，它只是把「daemon 已经会做的事」
//! > 换一种调用法。真正的边界在 tmux socket 的文件权限上，一直如此。
//!
//! ⚠ 这条理由的**射程**：它只覆盖「起/杀 tmux 会话」这一族（今天 `REGISTRY` 上 CLI 面的全部）。
//! 将来若有一条命令能做**本机进程原本做不到**的事（碰别的用户的东西、越过某道门），
//! 上面那句话对它**不成立**，必须单独立一条理由 —— 别默认它跟着继承。
//!
//! # 信封（与 `--resolve` 逐条同形，不新发明）
//!
//! · **入**：stdin 一段 JSON = 那条命令的 `args`（空 stdin = `{}`，`ping` 那类无载荷的用得上）。
//! · **出**：stdout 一行紧凑 JSON（命令没有返回值时是 `{}`），exit 0。
//! · **错**：exit 2 + stderr 一行 `{"code","message"}`。
//! · **exec 模型**：1 exec = 1 请求 1 响应 1 退出，**无 request-id**（`resolve_query` 头注逐字）。

use crate::inbound::{CommandSpec, Run, REGISTRY};
use crate::wire::Request;
use std::io::Read;

/// stdin 上限。理由抄 `resolve_query::MAX_RESOLVE_STDIN`：兜 DoS，不是兜格式。
///
/// ⚠ **超限是拒收，不是截断。** 第一版写的是 `.take(MAX_CLI_STDIN)` —— 那是**静默截断**：
/// 截半的 JSON 解析失败 ⇒ 回一句 `bad_request: args JSON parse failed`，
/// 而真实原因是「太大了」。`byte_cap_registry` 当场逮住这一处（本轮第十七次），
/// 它的 `ALLOWED_SEMANTICS` 里逐字**没有「静默截断」这一项**。
/// ⇒ 多读一个字节，超了就说超了。
const MAX_CLI_STDIN: u64 = 1024 * 1024;

/// 能力探测口〔P4d-Y2〕。
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
/// 第一天就有**。而 `parse_probe_output` 是 **`ccm` 专用**的（实测：daemon 侧零消费者，
/// monitor 也不用 CLI 面 —— 它走帧），让 daemon 去说 ccm 的方言只会多一种方言。
///
/// ⇒ 真正抄过来的是那条**理念**：集成方按**能力**兼容，不按版本号。
pub(crate) const PROBE_FLAG: &str = "--daemon-probe";

/// 本入口回显给命令的 `id`。**帧面的 `id` 由客户端发号且不透明**，而一次性 exec
/// 天然 1:1、没有并发的第二条请求可混淆 ⇒ 这里给一个固定值，不假装有号段。
const CLI_REQUEST_ID: &str = "cli";

/// 一条命令**上不上 CLI 面** —— 从 `REGISTRY` 派生，不是手抄一张表。
///
/// 判据是 `Run::Builtin`：那种命令的实现住在 `inbound::dispatch` 的硬臂里，
/// 要 `replies` 通道与在飞表才能跑，而**一次性 exec 里两样都不存在**。
pub(crate) fn cli_exposed(spec: &CommandSpec) -> bool {
    !matches!(spec.run, Run::Builtin)
}

/// 命令名 → CLI 子命令（`launch` → `--launch`）。
pub(crate) fn flag_of(name: &str) -> String {
    format!("--{name}")
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
/// `cc-monitor-remote --ping` 时的形状）：**`--ping` 永远不返回**。
/// 而这是所有失败里最坏的一种 —— 问「你活着吗」的那条命令，答案是挂住。
///
/// `fields` 的头注逐字：「本命令 `args` / `data` 的字段名。**空 = 无载荷（如 `ping`）**」。
/// ⇒ 判据现成，不用我再手写一张「哪些命令不读 stdin」的表。
pub(crate) fn reads_stdin(spec: &CommandSpec) -> bool {
    !spec.fields.is_empty()
}

/// 本入口认不认这个 flag。`main` 的分派臂只问它，**不写命令字面量** ——
/// 于是「帧面加一条命令」不需要回来改 `main`。
pub(crate) fn handles(flag: &str) -> bool {
    flag == PROBE_FLAG || spec_for(flag).is_some()
}

fn emit_err(code: &str, message: impl Into<String>) -> i32 {
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
pub(crate) async fn run(args: &[String]) -> i32 {
    let flag = args.first().map(String::as_str).unwrap_or_default();
    if flag == PROBE_FLAG {
        return probe();
    }
    let Some(spec) = spec_for(flag) else {
        // 走不到（`main` 只把已知 flag 派到这里），但**不许 panic**：
        // daemon 的一次性模式对未知参数的既定行为是 exit 2 + 结构化 stderr。
        return emit_err("unknown_command", format!("CLI 控制面不认识 {flag}"));
    };
    let mut input = String::new();
    if reads_stdin(spec) {
        // 多读一个字节，好把「刚好装满」与「超了」分开 —— 只读上限那么多是分不开的。
        if let Err(e) = std::io::stdin()
            .take(MAX_CLI_STDIN + 1)
            .read_to_string(&mut input)
        {
            return emit_err("stdin_read_failed", format!("read stdin failed: {e}"));
        }
        if input.len() as u64 > MAX_CLI_STDIN {
            return emit_err(
                "args_too_large",
                format!("args JSON 超过 {MAX_CLI_STDIN} 字节上限，已拒收（不截断：截半的 JSON 会被报成 bad_request，那句话与真实原因无关）"),
            );
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
        Run::Async(f) => f(req).await,
        Run::Builtin => return emit_err("not_available_in_cli", "这条命令只在帧面可用"),
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
mod tests {
    use super::*;

    /// 帧面有、CLI 面没有的命令，**逐条登记理由**。
    ///
    /// 形状抄 `readonly_guard::spawn_registry::ALLOWED`：把「为什么这条不上」写成**数据**，
    /// 好让机检对着它比 —— 散文里说一遍，下一个人加命令时看不见。
    const NOT_ON_CLI: &[(&str, &str)] = &[(
        "cancel",
        "它取消的是**同一条连接上在飞的另一条命令**。一次性 exec 是「1 请求 1 响应 1 退出、\
         无 request-id」（`resolve_query` 头注逐字）⇒ 本进程里没有第二条命令可取消，\
         给它开 CLI 口只会回一条永远找不到目标的应答。要停一条 CLI 命令：杀那个进程。",
    )];

    /// ★ P4d-Y1：帧面的每一条命令，**要么有 CLI 入口，要么在 [`NOT_ON_CLI`] 里写明为什么没有**。
    ///
    /// 钉的是**集合**，不是「加了 `--launch`」—— 后者选哪几条是我的主观，前者是数据对数据的镜子：
    /// 以后帧面加一条命令，CLI 面不跟、又不写理由，这里就红。
    #[test]
    fn every_wire_command_is_either_on_the_cli_or_has_a_written_reason() {
        let mut exposed: Vec<&str> = Vec::new();
        let mut withheld: Vec<&str> = Vec::new();
        for spec in REGISTRY {
            if cli_exposed(spec) {
                exposed.push(spec.name);
            } else {
                withheld.push(spec.name);
            }
        }
        assert!(
            REGISTRY.len() >= 5,
            "REGISTRY 只有 {} 条 —— 本断言在空转",
            REGISTRY.len()
        );
        // ① 上了 CLI 面的，`spec_for` 必须真能查回来（否则 `run` 会落进 unknown_command）。
        for name in &exposed {
            let flag = flag_of(name);
            assert!(
                spec_for(&flag).is_some(),
                "{name} 判为上 CLI 面，但 `spec_for({flag})` 查不到 —— 入口是断的"
            );
        }
        // ② 没上的，必须逐条有理由；且理由表里不许有**已经上了**的命令（那种是过期的理由）。
        let reasons: Vec<&str> = NOT_ON_CLI.iter().map(|(n, _)| *n).collect();
        let mut missing: Vec<&&str> = withheld.iter().filter(|n| !reasons.contains(n)).collect();
        missing.sort();
        assert!(
            missing.is_empty(),
            "这些帧面命令没有 CLI 入口，也没写为什么：{missing:?}\n\
             要么让它上 CLI 面，要么进 `NOT_ON_CLI` 写明理由（写「以后再说」不算理由）。"
        );
        let stale: Vec<&&str> = reasons.iter().filter(|n| exposed.contains(n)).collect();
        assert!(
            stale.is_empty(),
            "`NOT_ON_CLI` 里这些命令**今天已经上了** CLI 面：{stale:?} —— 理由过期了，删掉它。"
        );
        for (name, why) in NOT_ON_CLI {
            assert!(
                why.chars().count() >= 40,
                "`NOT_ON_CLI` 里 {name} 的理由只有 {} 字 —— 那是占位不是理由",
                why.chars().count()
            );
        }
    }

    /// ★ P4d-Y1 的另一半：**CLI 入口不许自己实现命令**。
    ///
    /// 上一条钉的是「集合对上了」，钉不住「它跑的是同一个函数」—— CLI 分支里自己拼一套
    /// tmux 调用，集合照样对得上。所以这里钉**数据流**：本模块的生产段
    /// 不许出现任何一个具体处理器的路径，唯一的出口是 `REGISTRY` 上那条 `spec.run`。
    ///
    /// ⚠ 与它互补的是 `readonly_guard::spawn_registry` 那条 `SPAWN_SITES_TODAY == 6`：
    /// 真去起了进程，那边会红。两条各挡一种绕法。
    #[test]
    fn the_cli_entry_never_names_a_concrete_handler() {
        let raw = include_str!("cli_control.rs");
        let prod = crate::guard_support::production_code(raw);
        for banned in [
            "control::launch::",
            "control::kill::",
            "control::resolve_query::",
            "Command::new(",
        ] {
            assert!(
                !prod.contains(banned),
                "CLI 入口的生产段出现了 {banned:?} —— 那是绕开 `REGISTRY` 自己接了一条实现。\n\
                 本模块的唯一出口是 `spec.run`；要改某条命令的行为，改它在 `REGISTRY` 上那条登记。"
            );
        }
        assert!(
            prod.contains("Run::Blocking(f) => f(req)") && prod.contains("Run::Async(f) => f(req)"),
            "派发不再直接跑 `spec.run` 了 —— 「一份实现，两个入口」这个支点断在这里"
        );
    }

    /// ★ P4d-Y2：探测口报的命令集合 **== 它真能派发的集合**。
    ///
    /// 失效方式很具体：有人在 `probe()` 里手抄一份清单。那样加命令时它不报错，
    /// 只是**开始说谎** —— 而 skill 按它的话决定走不走新路 ⇒ 静默降级，没有任何报错。
    #[test]
    fn the_probe_reports_exactly_what_it_can_dispatch() {
        let raw = crate::guard_support::production_code(include_str!("cli_control.rs"));
        assert!(
            !raw.contains("\"--launch\"") && !raw.contains("\"--kill\""),
            "`probe()` 里出现了硬编码的命令字面量 —— 清单必须从 `REGISTRY` 派生"
        );
        let reported: Vec<String> = REGISTRY
            .iter()
            .filter(|s| cli_exposed(s))
            .map(|s| flag_of(s.name))
            .collect();
        assert!(!reported.is_empty(), "探测口报空清单 —— 本断言在空转");
        for flag in &reported {
            assert!(
                spec_for(flag).is_some(),
                "探测口报了 {flag}，但 `run` 派发不了它 —— 探测口在说谎"
            );
        }
        // 反面：真能派发的，一条都不许漏报。
        for spec in REGISTRY.iter().filter(|s| cli_exposed(s)) {
            let flag = flag_of(spec.name);
            assert!(
                reported.contains(&flag),
                "{flag} 能派发却没进探测口清单 —— skill 会以为这台不支持它"
            );
        }
    }

    /// ★ D 阶段补审：**无载荷的命令不许读 stdin**，否则存活探测口会挂死。
    ///
    /// 钉的是**决定**（`reads_stdin`），不是「跑起来没挂」—— 后者要真起进程 + 一条不关的
    /// 管道，在单测里做不了；而钉决定 + 钉 `run` 用的就是这个决定，两条合起来够。
    #[test]
    fn a_command_with_no_payload_never_waits_on_stdin() {
        let mut checked = 0usize;
        for spec in REGISTRY.iter().filter(|s| cli_exposed(s)) {
            if spec.fields.is_empty() {
                assert!(
                    !reads_stdin(spec),
                    "{} 无载荷（`fields` 空）却要读 stdin —— 它会挂在那儿等 EOF",
                    spec.name
                );
                checked += 1;
            } else {
                assert!(
                    reads_stdin(spec),
                    "{} 有载荷字段却不读 stdin —— args 永远是 {{}}",
                    spec.name
                );
            }
        }
        assert!(
            checked >= 1,
            "一条无载荷命令都没扫到 —— 本断言在空转（08-12 实测 `ping` 是这种）"
        );
        // `run` 必须用的就是上面那个决定，不是另写一份判断。
        let prod = crate::guard_support::production_code(include_str!("cli_control.rs"));
        assert!(
            prod.contains("if reads_stdin(spec) {"),
            "`run` 不再按 `reads_stdin` 决定读不读 stdin —— 上面那条断言此刻钉的是一个没人用的函数"
        );
    }

    /// ★ P4d-Y4：帧入口那条安全边界的**理由**不许被搬到这里。
    ///
    /// 钉的不是「有没有写边界」（写段散文最容易过），而是**那句借来的依据不许出现** ——
    /// 逼下一个人写他自己的理由。
    #[test]
    fn the_cli_entry_does_not_borrow_the_frame_entrys_boundary_reason() {
        let raw = include_str!("cli_control.rs");
        // 下面两串是 `launch.rs` 那句依据的核心措辞（讲对端的 SSH 身份那句）。
        // ⚠ 本注释**刻意不复述它们** —— 复述一遍，本判据自己就会把自己判红。
        for borrowed in ["握着这台机器的 SSH 会话", "对端本来就握着"] {
            let hits = raw.matches(borrowed).count();
            assert!(
                hits <= 1,
                "本模块出现了 {borrowed:?} {hits} 次 —— 那是帧入口的依据。\n\
                 CLI 入口的调用方是本机任意进程，那条依据在这里不成立；写你自己的。\n\
                 （一次是本判据自己的字面量，多于一次就是被搬过来了。）"
            );
        }
        assert!(
            raw.contains("§安全边界"),
            "本模块没有 `§安全边界` 那一节 —— P4d-Y4 要求 CLI 入口写下它自己的理由"
        );
    }
}
