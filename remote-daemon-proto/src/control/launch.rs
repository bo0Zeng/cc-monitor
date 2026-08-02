//! U8a-2b：**平面 ②（远端执行面）** —— 在远端真的建 tmux 会话 / 往已有会话键入载荷。
//!
//! # 这一层归哪、不做什么
//!
//! U8a 把「起会话」拆成三个平面：① 计划面（`resolve_query`，已在这儿）·
//! ② 远端执行面（**本模块**）· ③ 本机开窗面（结构上只能是 monitor —— daemon 在远端，
//! 开不了你面前的窗）。所以本模块**不 attach**，一次都不。
//!
//! # ★ argv，不过 shell
//!
//! 今天 monitor 那条路是「渲染一整条 shell 串 → `ssh -t "bash -lic '<串>'"`」，于是
//! 引号 / 转义 / 注入是一整类必须一直防的问题。本模块用
//! `Command::new("tmux").args([...])` 直传 argv ⇒ **那类问题在这条路上不存在**，
//! 不是「被挡住了」。这是搬进 daemon 最实在的收益之一。
//!
//! # ★ 这里的校验是**形状校验**，不是安全边界
//!
//! 入方向命令来自**已经握着这台机器 SSH 会话**的对端 —— 它本来就能在这台机器上跑任意命令。
//! daemon 再校验一遍挡不住任何它原本挡不住的东西；假装有一层会更糟：下一个人会以为
//! 那是安全边界，从而放松上游真正在把关的地方（前端的 sid 白名单 / launcher denylist）。
//!
//! # ★ 错误码分两层
//!
//! - **协议级**（`inbound.rs` 独占）：`bad_request`（信封 JSON 坏了）· `line_too_long` ·
//!   `unknown_command` · `duplicate_id` · `handler_panicked` · `not_cancellable`。
//!   语义是「**客户端代码写错了**，别重试」。
//! - **命令级**（本模块）：`invalid_args` · `no_tmux` · `no_such_session` · `create_failed` ·
//!   `typed_unconfirmed`。语义是「参数或环境的问题」，可能来自用户输入。
//!
//! 本模块的形状错误刻意叫 **`invalid_args` 而不是 `bad_request`** —— 后者是协议级那一层的，
//! 一词两义会让客户端分不出「我发的 JSON 坏了」与「我发的参数不合适」。
//! （`resolve` 那条今天仍回命令级 `bad_request`：它与仓外 aterm 的一次性契约冻结在
//! 2026-07-18，两条路复用同一个纯函数，改它会破坏那份契约。**如实登记，不顺手改。**）
//!
//! 所以这里只回答一个问题：**这组参数能不能构成一次有意义的 tmux 调用**。
//! 不能就回一条结构化错误（可诊断、fail-fast），而不是把畸形串塞给 tmux 让它以奇怪的方式失败。
//!
//! ⚠ **明确不抄的一条**：monitor `launch.rs::build_remote_ssh_ps_command` 里的「禁双引号」
//! 是 PowerShell 5.1 向 native 程序传参的历史畸变（`wt.exe` 那条路）。**与本模块无关**，
//! 这条路根本不过 shell —— 抄它等于把一个 Windows 怪癖套到 tmux argv 上。
//!
//! # ★ `send-into` 是一等模式（#76 防线的形态迁移）
//!
//! `shared/ccm` 的 `--tmux` 只有幂等 create-or-attach 一种形态，**没有**「就地复用已存在的
//! idle tmux、不新建」。所以 monitor 的 CLI 渲染器 `launch-render-cli.ts` 对 `send-into`
//! **诚实放弃、强制走兜底**（那条注释逐字写着「这条是防 #76 复发的关键」）。
//!
//! daemon 直接调 tmux 之后那个表达力缺口消失：[`Mode::SendInto`] 是一等模式。
//! **但语义要守死**：会话不存在时**报错，绝不顺手新建** —— 顺手新建就是 #76 的反向
//! （用户以为在复用那个 idle 会话，实际上被丢进一个新建的空 shell）。
//! 由 `send_into_never_creates_a_session` 钉住。

use std::process::{Command, Stdio};

/// 载荷 / 名字 / cwd 的长度上限。取值同 monitor 侧 `launch.rs::MAX_REMOTE_CMD` 的量级 ——
/// 那是「一条人能读的启动命令」的宽松上界，不是安全阈值。
const MAX_FIELD_BYTES: usize = 8 * 1024;

/// 起会话的模式。**没有 `attach-only`** —— attach 是平面 ③。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Mode {
    /// 幂等：会话不在就建 + 键入载荷；已在就**什么都不做**（不重复 resume）。
    CreateOrAttach,
    /// 往一个**已存在**的会话键入载荷。不存在 ⇒ 报错，**绝不新建**。
    SendInto,
}

impl Mode {
    fn parse(s: &str) -> Option<Self> {
        match s {
            "create-or-attach" => Some(Mode::CreateOrAttach),
            "send-into" => Some(Mode::SendInto),
            _ => None,
        }
    }
}

/// 一次 `launch` 请求（已过形状校验）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LaunchRequest {
    pub(crate) mode: Mode,
    pub(crate) name: String,
    pub(crate) payload: String,
    pub(crate) cwd: Option<String>,
    pub(crate) ccm_sid: Option<String>,
}

/// 一次 `launch` 的结局。三个字段就是「没起成 / 起了但没确认 / 起成了」的载体。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LaunchOutcome {
    /// 本次是否**新建**了会话（幂等短路时为 false）。
    pub(crate) created: bool,
    /// 载荷是否键入了（幂等短路时为 false —— 会话已在，不重复 resume）。
    pub(crate) typed: bool,
}

type CmdErr = (&'static str, String);

/// `=name:` —— tmux 的**精确匹配**目标形式（F01）。
///
/// # 别把尾冒号「简化」掉
///
/// 裸 `-t <名>` 不是精确匹配：tmux 依次按「精确名 → 名字**开头** → glob」解析。
/// 本仓踩过（`pickFreshTmuxName` 刻意造 `cc-<sid8>-2/-3`）：只有 `sib-2` 存在时
/// `kill-session -t sib` **杀掉 `sib-2` 且 rc=0**。
/// 而 `=` 前缀只在 target-**session** 解析路径上被识别；`send-keys` 收的是 target-**pane**，
/// `=name` 会直接 `can't find pane`。尾冒号把串强制成 `session:` 形态，`=` 才落对位置。
///
/// **与 monitor 侧的区别**：那边 `tmux::exact_target` 还要 `shell_quote` 一层，因为它拼的是
/// 要穿过 shell 的命令串；这里是 argv 直传，**不引号化**（引号化了就成了名字的一部分）。
/// 两边**形状**必须一致，由 `exact_target_shape_matches_the_monitor_side` 跨轨钉住。
pub(crate) fn exact_target(name: &str) -> String {
    format!("={name}:")
}

/// 从入方向的 `args` 解析 + **形状校验**。见模块头注：这不是安全边界。
pub(crate) fn parse_request(args: &serde_json::Value) -> Result<LaunchRequest, CmdErr> {
    let obj = args
        .as_object()
        .ok_or(("invalid_args", "args 必须是对象".to_string()))?;

    let get_str = |k: &str| -> Option<&str> { obj.get(k).and_then(|v| v.as_str()) };

    let mode_raw = get_str("mode").ok_or((
        "invalid_args",
        "缺 `mode`（create-or-attach / send-into）".to_string(),
    ))?;
    let mode = Mode::parse(mode_raw).ok_or((
        "invalid_args",
        format!("未知 mode `{mode_raw}` —— 只有 create-or-attach / send-into；attach 是平面 ③，不归 daemon"),
    ))?;

    let name = get_str("name")
        .ok_or(("invalid_args", "缺 `name`".to_string()))?
        .to_string();
    check_field("name", &name)?;
    // `=` / `:` 是 tmux 目标语法的一部分；名字里带它们会让 `=name:` 变成别的意思。
    // 这是**形状**问题（会静默打到别的会话上），不是安全问题。
    if name.contains(':') || name.contains('=') {
        return Err((
            "invalid_args",
            format!("`name` 不许含 `:` 或 `=`（它们是 tmux 目标语法）：{name:?}"),
        ));
    }

    let payload = get_str("payload")
        .ok_or(("invalid_args", "缺 `payload`".to_string()))?
        .to_string();
    check_field("payload", &payload)?;

    let cwd = get_str("cwd").map(str::to_string);
    if let Some(c) = &cwd {
        check_field("cwd", c)?;
    }

    let ccm_sid = get_str("ccm_sid").map(str::to_string);
    if let Some(s) = &ccm_sid {
        check_field("ccm_sid", s)?;
        // 它会被拼进 tmux 的格式串（`ccm-rbind-#{@ccm_sid}`），收紧到确定安全的字符集。
        if !s
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        {
            return Err((
                "invalid_args",
                format!("`ccm_sid` 只许 [A-Za-z0-9_-]：{s:?}"),
            ));
        }
    }

    Ok(LaunchRequest {
        mode,
        name,
        payload,
        cwd,
        ccm_sid,
    })
}

fn check_field(what: &str, v: &str) -> Result<(), CmdErr> {
    if v.trim().is_empty() {
        return Err(("invalid_args", format!("`{what}` 为空")));
    }
    if v.len() > MAX_FIELD_BYTES {
        return Err((
            "invalid_args",
            format!("`{what}` 过长（{} > {MAX_FIELD_BYTES}）", v.len()),
        ));
    }
    // 控制字符会让 send-keys 的语义变掉（`\n` = 多敲一次回车）。形状问题。
    if v.chars().any(char::is_control) {
        return Err(("invalid_args", format!("`{what}` 含控制字符")));
    }
    Ok(())
}

/// 跑一次 tmux 子命令，返回它成不成功。**argv 直传，不过 shell。**
fn tmux(args: &[&str]) -> Result<bool, CmdErr> {
    match Command::new("tmux")
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
    {
        Ok(st) => Ok(st.success()),
        Err(e) => Err((
            "no_tmux",
            format!("起不来 tmux（远端装了吗？PATH 里有吗？）：{e}"),
        )),
    }
}

/// 真做事。**只在 `Disposition::spawn` 的独立 task 上跑** —— 它会阻塞（起进程）。
pub(crate) fn run(req: &LaunchRequest) -> Result<LaunchOutcome, CmdErr> {
    let t = exact_target(&req.name);
    match req.mode {
        Mode::SendInto => {
            // ★ 会话不存在 ⇒ 报错，**绝不顺手新建**。顺手新建就是 #76 的反向：
            //   用户以为在复用那个 idle 会话，实际被丢进一个新建的空 shell。
            if !tmux(&["has-session", "-t", &t])? {
                return Err((
                    "no_such_session",
                    format!(
                        "会话 {:?} 不存在；send-into 只往**已存在**的会话键入，不新建",
                        req.name
                    ),
                ));
            }
            type_payload(&t, &req.payload)?;
            Ok(LaunchOutcome {
                created: false,
                typed: true,
            })
        }
        Mode::CreateOrAttach => {
            let mut new_args: Vec<&str> = vec!["new-session", "-d", "-s", &req.name];
            if let Some(cwd) = &req.cwd {
                new_args.push("-c");
                new_args.push(cwd);
            }
            // 幂等闸：会话已存在 ⇒ new-session 失败 ⇒ **短路，什么都不做**。
            // 与今天那条 shell 串 `new-session -d … 2>/dev/null && send-keys …` 逐字同义
            // （不重复 resume）。区别只是这里不吞 stderr 靠 `2>/dev/null`，而是看退出码。
            if !tmux(&new_args)? {
                // 分辨「已存在（幂等）」与「真的建不出来」——今天那条 shell 串分不出来。
                if tmux(&["has-session", "-t", &t])? {
                    return Ok(LaunchOutcome {
                        created: false,
                        typed: false,
                    });
                }
                return Err((
                    "create_failed",
                    format!(
                        "建不出会话 {:?}，且它也不存在（cwd 不可用？名字非法？）",
                        req.name
                    ),
                ));
            }
            // 身份标记与标题是**次要**动作：失败绝不阻断主要动作（键入载荷）。
            // 与 monitor 侧 `session-backend.ts` 里 `(… 2>/dev/null || true) &&` 同一条纪律。
            if let Some(sid) = &req.ccm_sid {
                let _ = tmux(&["set-option", "-t", &t, "@ccm_sid", sid]);
                let _ = tmux(&["set-option", "-t", &t, "set-titles", "on"]);
                let _ = tmux(&[
                    "set-option",
                    "-t",
                    &t,
                    "set-titles-string",
                    "ccm-rbind-#{@ccm_sid}",
                ]);
            }
            type_payload(&t, &req.payload)?;
            Ok(LaunchOutcome {
                created: true,
                typed: true,
            })
        }
    }
}

/// 键入载荷。失败 ⇒ `typed_unconfirmed`：**会话在，载荷未必落**。
///
/// 这一档是 DoD 6 那条「起了但没确认」的落点：调用方**不许**据此重试 create
/// （会话确实在），该做的是告诉用户「会话建好了但没能把命令打进去」。
fn type_payload(target: &str, payload: &str) -> Result<(), CmdErr> {
    if tmux(&["send-keys", "-t", target, payload, "Enter"])? {
        return Ok(());
    }
    Err((
        "typed_unconfirmed",
        format!("会话 {target:?} 在，但 send-keys 失败 —— 载荷未必落进去了；别重试新建"),
    ))
}

/// 入方向命令的入口：`args` → 结局 JSON。
pub(crate) fn launch_for_inbound(
    args: &serde_json::Value,
) -> Result<serde_json::Value, (String, String)> {
    let req = parse_request(args).map_err(|(c, m)| (c.to_string(), m))?;
    let session = req.name.clone();
    let out = run(&req).map_err(|(c, m)| (c.to_string(), m))?;
    Ok(serde_json::json!({
        "session": session,
        "created": out.created,
        "typed": out.typed,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(v: serde_json::Value) -> serde_json::Value {
        v
    }

    #[test]
    fn parses_a_well_formed_create_request() {
        let r = parse_request(&args(serde_json::json!({
            "mode": "create-or-attach",
            "name": "cc-1a2b3c4d",
            "payload": "claude --resume x",
            "cwd": "/home/u/p",
            "ccm_sid": "1a2b3c4d-0000-0000-0000-000000000000",
        })))
        .expect("应当解析成功");
        assert_eq!(r.mode, Mode::CreateOrAttach);
        assert_eq!(r.name, "cc-1a2b3c4d");
        assert_eq!(r.cwd.as_deref(), Some("/home/u/p"));
    }

    /// **没有 `attach-only`** —— attach 是平面 ③，daemon 开不了你面前的窗。
    #[test]
    fn attach_is_not_a_mode_here() {
        let e = parse_request(&args(serde_json::json!({
            "mode": "attach-only", "name": "x", "payload": "y"
        })))
        .unwrap_err();
        assert_eq!(e.0, "invalid_args");
        assert!(e.1.contains("平面 ③"), "错误没说清楚为什么：{}", e.1);
    }

    #[test]
    fn shape_validation_rejects_the_things_that_would_break_tmux() {
        let base = |name: &str, payload: &str| serde_json::json!({ "mode": "send-into", "name": name, "payload": payload });
        for (name, payload, why) in [
            ("", "p", "空名字"),
            ("n", "", "空载荷"),
            ("a:b", "p", "名字含 `:`（tmux 目标语法）"),
            ("a=b", "p", "名字含 `=`"),
            ("n", "a\nb", "载荷含控制字符（会多敲一次回车）"),
        ] {
            match parse_request(&base(name, payload)) {
                Ok(_) => panic!("{why} 居然通过了"),
                Err(e) => assert_eq!(e.0, "invalid_args", "{why}"),
            }
        }
        // 超长
        let long = "x".repeat(MAX_FIELD_BYTES + 1);
        assert_eq!(
            parse_request(&base("n", &long)).unwrap_err().0,
            "invalid_args"
        );
        // `ccm_sid` 会被拼进 tmux 格式串，收紧字符集
        let e = parse_request(&serde_json::json!({
            "mode":"create-or-attach","name":"n","payload":"p","ccm_sid":"a b"
        }))
        .unwrap_err();
        assert_eq!(e.0, "invalid_args");
    }

    /// ★ **不许照抄 monitor 的「禁双引号」** —— 那是 PowerShell 专属。
    ///
    /// 这条路是 argv 直传，双引号只是一个普通字符。抄过来会让一大批合法载荷被拒，
    /// 而且是以一个在这条路上根本不存在的理由。
    #[test]
    fn a_double_quote_in_the_payload_is_perfectly_fine_here() {
        let r = parse_request(&serde_json::json!({
            "mode": "send-into",
            "name": "cc-x",
            "payload": "claude --resume \"my session\"",
        }))
        .expect("双引号在 argv 直传的路上是合法字符");
        assert!(r.payload.contains('"'));
    }

    /// `=name:` 的形状必须与 monitor 侧一致（F01：裸 `-t` 会打到兄弟会话上）。
    #[test]
    fn exact_target_shape_matches_the_monitor_side() {
        assert_eq!(exact_target("cc-abc"), "=cc-abc:");
        // 跨轨对拍：monitor `tmux.rs` 里那条 `format!("={target}:")`。
        const MONITOR_TMUX: &str = include_str!("../../../src-tauri/src/tmux.rs");
        let prod = crate::guard_support::production_code(MONITOR_TMUX);
        // 运行时拼，避免命中本文件自己。
        let shape = format!("=%s{}", "target}:");
        let needle = shape.replace("%s", "{");
        assert!(
            prod.contains(&needle),
            "monitor 侧的精确匹配形状变了（找不到 `{needle}`）—— 两侧必须同形，\
             否则一边打到兄弟会话上而另一边不会，排查起来会非常难"
        );
    }

    /// ★ #76 防线的形态迁移：`send-into` **绝不新建会话**。
    ///
    /// TS 侧那条防线（`launch-render-cli.ts` 让 `send-into` 强制走兜底）挡的是
    /// 「用 create-or-attach 的语法去近似 send-into」；daemon 直接调 tmux 之后那个
    /// 表达力缺口没了，但**语义陷阱还在**：顺手新建就是 #76 的反向。
    ///
    /// 这条扫的是 `run()` 的 `SendInto` 分支源码：它里面不许出现 `new-session`。
    #[test]
    fn send_into_never_creates_a_session() {
        let src = crate::guard_support::production_code(include_str!("launch.rs"));
        let at = src
            .find("Mode::SendInto =>")
            .expect("找不到 SendInto 分支 —— 抽取坏了，本断言在空转");
        let end = src[at..]
            .find("Mode::CreateOrAttach =>")
            .map(|k| at + k)
            .expect("找不到分支收尾锚点");
        let arm = &src[at..end];
        assert!(
            arm.len() > 200,
            "只切出 {} 字节的分支 —— 抽取坏了",
            arm.len()
        );
        let verb = format!("new-{}", "session");
        assert!(
            !arm.contains(&verb),
            "`send-into` 分支里出现了建会话 —— 那是 #76 的反向：\n\
             用户以为在复用那个 idle 会话，实际被丢进一个新建的空 shell。\n\
             会话不存在时**报错**（`no_such_session`），别顺手建。"
        );
        assert!(
            arm.contains("has-session"),
            "`send-into` 分支没有先查会话在不在 —— 那就没法报 `no_such_session`"
        );
    }

    /// 结局映射：三种成功形态的 `created`/`typed` 组合必须互不相同（否则调用方分不出来）。
    #[test]
    fn the_three_success_shapes_are_distinguishable() {
        let created = LaunchOutcome {
            created: true,
            typed: true,
        };
        let idempotent = LaunchOutcome {
            created: false,
            typed: false,
        };
        let sent = LaunchOutcome {
            created: false,
            typed: true,
        };
        assert_ne!(created, idempotent);
        assert_ne!(created, sent);
        assert_ne!(idempotent, sent);
    }
}
