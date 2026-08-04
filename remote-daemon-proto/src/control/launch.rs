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
    /// F04c：往一个**已存在**的会话发**裸键**——与 [`Mode::SendInto`] 的唯一区别是
    /// **不附尾 `Enter`**。
    ///
    /// # 为什么这必须是一个新 **mode 名**，而不是给 `send-into` 加一个 `enter` 字段
    ///
    /// [`parse_request`] 是**手工从 `Map` 取键**的，**不 deny unknown fields** ⇒
    /// 旧版本 daemon 收到一个它不认识的 `enter` 字段会**静默忽略**，照样附 `Enter`。
    /// 而 monitor 唯一会发 `enter=false` 的地方是「优雅退出时发 `Escape` 打断当前回合」——
    /// 多一个 `Enter` 就把它变成「**提交用户输入框里排队的文本**」。
    /// 换成新 mode 名则天然 **fail-closed**：旧 daemon 的 [`Mode::parse`] 返回 `None`
    /// ⇒ `invalid_args` ⇒ monitor 拿到明确错误、干净回落到一次性 SSH。
    /// **能力协商在这里是免费的，不需要新机制。**
    SendKeysRaw,
}

impl Mode {
    fn parse(s: &str) -> Option<Self> {
        match s {
            "create-or-attach" => Some(Mode::CreateOrAttach),
            "send-into" => Some(Mode::SendInto),
            "send-keys-raw" => Some(Mode::SendKeysRaw),
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
        "缺 `mode`（create-or-attach / send-into / send-keys-raw）".to_string(),
    ))?;
    let mode = Mode::parse(mode_raw).ok_or((
        "invalid_args",
        format!("未知 mode `{mode_raw}` —— 只有 create-or-attach / send-into / send-keys-raw；attach 是平面 ③，不归 daemon"),
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
            // ★ **过 §34 的 Gate 2**（F03）。`admit` 一次探测同时办三件事：
            //   1. 会话不存在 ⇒ `no_such_session`，**绝不顺手新建**（顺手新建就是 #76 的反向：
            //      用户以为在复用那个 idle 会话，实际被丢进一个新建的空 shell）；
            //   2. 名字不是本工具形状、`@ccm_sid` 也没设 ⇒ `wrong_owner`，拒绝键入；
            //   3. 通过则回 `#{session_id}` 句柄 —— **后面一律对句柄下手，不对名字**，
            //      名字在窗口期内被重新绑定也打不到别人身上（见 `gate` 模块头注）。
            //
            // ⚠ 顺序不可反：`admit` 必须在 `type_payload` **之前**。
            // 由 `the_send_into_arm_admits_before_it_types` 钉住。
            let handle = super::gate::admit(&req.name, &t)?;
            type_payload(&handle, &req.payload)?;
            Ok(LaunchOutcome {
                created: false,
                typed: true,
            })
        }
        Mode::SendKeysRaw => {
            // 与 `SendInto` **同一道门、同一个顺序**（F03 的 Gate 2）——发裸键也是往别人的
            // 会话里打字，身份门一视同仁。⚠ 不给它 Gate 3：`send-keys` 不删除任何东西
            // （monitor 侧 F04 Phase D 审计修过「给非破坏性动作加 Gate 3」那个错法）。
            let handle = super::gate::admit(&req.name, &t)?;
            // ★ 唯一的区别：**不附 `Enter`**。由 `send_keys_raw_never_appends_enter` 钉住。
            type_keys_raw(&handle, &req.payload)?;
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

/// F04c：发**裸键**，**不附尾 `Enter`**。
///
/// tmux 的 `send-keys` 对每个参数**先试着当键名解析**（`Escape` / `C-c` / `Enter` …），
/// 解析不出来才按字面串敲。所以同一个位置既能发 `/compact` 这种文本、也能发 `Escape`
/// 这种键 —— 区别只在**要不要再补一下回车**。
///
/// 失败仍是 `typed_unconfirmed`（同 [`type_payload`]）：会话在，键未必落。
fn type_keys_raw(target: &str, keys: &str) -> Result<(), CmdErr> {
    if tmux(&["send-keys", "-t", target, keys])? {
        return Ok(());
    }
    Err((
        "typed_unconfirmed",
        format!("会话 {target:?} 在，但 send-keys（裸键）失败 —— 键未必落进去了"),
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

    /// 切出 `run()` 里某一个 `Mode::X =>` 分支的源码。
    ///
    /// ⚠ **收尾锚点必须是「下一个 `Mode::` 分支头」，不能写死某一个分支名。**
    /// F04c 实测踩到：新分支 `Mode::SendKeysRaw` 插在 `SendInto` 与 `CreateOrAttach` 之间，
    /// 而那两条判据的收尾锚点写死了 `Mode::CreateOrAttach =>` ⇒ 它们把**两个分支当成一个**
    /// 扫，断言照样全绿（249 条一条没红）。**扫到了东西，但扫的不是那件事** —— 本仓这一族
    /// 的又一次（`tmux_daemon_gate_guard` 的硬编码文件表是同一个病）。
    /// 「收尾行」与「枚举头」两个针 —— **运行时拼，源码里不留不配对的大括号**。
    ///
    /// ⚠ `readonly_guard::no_test_code_leaks_into_any_production_section` 的剥法是**数括号**：
    /// 字符串字面量里一个孤立的大括号会让 `mod tests` 那一层被提前配平收尾，
    /// 于是测试属性「泄漏」进生产段。它本轮当场逮到我（`[("launch.rs", 2)]`），
    /// 而它的报错文案逐字预言了这个形状。**别去改剥法，改措辞。**
    fn tail_brace() -> String {
        // 码点 7d 就是右大括号。**用码点写、不写字面大括号** —— `\u{..}` 自带一对，
        // 于是这一行的括号收支为 0（详见上面 `tail_brace` 头注那段病史）。
        format!("\n{}\n", '\u{7d}')
    }
    fn enum_mode_head() -> String {
        format!("pub(crate) enum Mode {}", '\u{7b}')
    }

    fn arm_of<'a>(src: &'a str, head: &str) -> &'a str {
        let at = src
            .find(head)
            .unwrap_or_else(|| panic!("找不到分支 `{head}` —— 抽取坏了，断言会空转"));
        let rest = &src[at + head.len()..];
        let end = rest.find("Mode::").unwrap_or(rest.len());
        let arm = &src[at..at + head.len() + end];
        assert!(
            arm.len() > 150,
            "`{head}` 只切出 {} 字节 —— 抽取坏了",
            arm.len()
        );
        arm
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
        let arm = arm_of(&src, "Mode::SendInto =>");
        let verb = format!("new-{}", "session");
        assert!(
            !arm.contains(&verb),
            "`send-into` 分支里出现了建会话 —— 那是 #76 的反向：\n\
             用户以为在复用那个 idle 会话，实际被丢进一个新建的空 shell。\n\
             会话不存在时**报错**（`no_such_session`），别顺手建。"
        );
        // F03：存在性检查从 `has-session` 换成了 `gate::admit`（同一次探测顺带取回
        // `@ccm_sid` 与句柄）。**保证没变**：不存在仍回 `no_such_session`，
        // 由 `admit` 里那条 `let Some(p) = probe(target)? else` 兜着。
        assert!(
            arm.contains("gate::admit"),
            "`send-into` 分支不过 `gate::admit` —— 那就既没查会话在不在（`no_such_session`），\
             也没过 §34 的 Gate 2"
        );
    }

    /// ★ **生产接线（顺序钉）**：`admit` 必须在 `type_payload` **之前**。
    ///
    /// 这条与上面那条不是重复：上面钉「过不过门」，这条钉「门在不在路上」。
    /// 反过来（先键入再核验）＝ 门形同虚设、而两条测试都会因为「函数被调用了」而绿。
    /// 顺序错的形态在本仓出现过（`launch_wire` 的 env 顺序），是**最容易被 review 漏掉**的一类。
    #[test]
    fn the_send_into_arm_admits_before_it_types() {
        let src = crate::guard_support::production_code(include_str!("launch.rs"));
        let arm = arm_of(&src, "Mode::SendInto =>");
        let admit_at = arm.find("gate::admit").expect("分支里没有 `gate::admit`");
        let type_at = arm.find("type_payload").expect("分支里没有 `type_payload`");
        assert!(
            admit_at < type_at,
            "`type_payload` 排在 `gate::admit` 前面 —— 载荷先打出去了，门再判就没意义了"
        );
        // 键入的目标必须是 `admit` 回的**句柄**，不是名字/`t`。见 `gate` 模块头注的 TOCTOU 那段。
        assert!(
            arm.contains("type_payload(&handle"),
            "键入的目标不是 `admit` 回的句柄 —— 对名字下手就把 TOCTOU 窗口放回来了"
        );
    }

    /// ★ `create-or-attach` **刻意不过门**，且理由必须仍然成立：
    /// 它只在 `new-session` 成功（＝会话是本次刚建的）时才键入；
    /// 建失败但会话已存在时**早返回、根本不键入**。
    ///
    /// 这条是「刻意无机检 + 理由」的反面 —— 理由可机检就机检：
    /// 一旦有人让这个分支在「会话已存在」时也去 `type_payload`，本条就红。
    #[test]
    fn create_or_attach_never_types_into_a_session_it_did_not_just_create() {
        let src = crate::guard_support::production_code(include_str!("launch.rs"));
        // ⚠ F04c 改用 `arm_of`：原来是 `&src[at..]`（一直切到文件末尾）——
        // 那不是「这个分支」，是「这个分支之后的全部生产代码」。
        // 本轮新加的 `every_mode_variant_…` 当场点名了它（**它是最后一个分支，所以一直没出事**，
        // 但只要有人在它后面再加一个 mode，断言面就会静默串到别人身上）。
        let arm = arm_of(&src, "Mode::CreateOrAttach =>");
        let early = arm
            .find("created: false,")
            .expect("找不到「已存在 ⇒ 早返回」那一档");
        let types = arm.find("type_payload").expect("分支里没有 type_payload");
        assert!(
            early < types,
            "「会话已存在 ⇒ 早返回」不再排在键入之前 —— 那就会往一个**不是自己刚建的**\n\
             会话里键入，而这个分支没有 Gate 2（F03 刻意只给 send-into 装门，理由就是这条）。\n\
             要么把早返回放回去，要么这里也得过 `gate::admit`。"
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

    // ===== F04c：`send-keys-raw`（发裸键、不附 Enter）=====

    /// 新 mode 名解析得出来，而且**旧的两个没被顺手改掉**。
    #[test]
    fn the_new_mode_name_parses_and_the_old_ones_still_do() {
        assert_eq!(Mode::parse("send-keys-raw"), Some(Mode::SendKeysRaw));
        assert_eq!(Mode::parse("send-into"), Some(Mode::SendInto));
        assert_eq!(Mode::parse("create-or-attach"), Some(Mode::CreateOrAttach));
        // ★ **fail-closed 的那一半**：未知 mode 必须回 `None` ⇒ `invalid_args`。
        // 这正是「为什么是新 mode 名而不是新字段」的全部理由 —— 旧 daemon 会走到这里。
        for unknown in ["send-keys", "attach-only", "SendKeysRaw", "", "send-into "] {
            assert_eq!(Mode::parse(unknown), None, "{unknown:?} 不该被认出来");
        }
        let e = parse_request(&serde_json::json!({
            "mode": "send-keys", "name": "x-cc", "payload": "Escape"
        }))
        .expect_err("未知 mode 必须被拒");
        assert_eq!(e.0, "invalid_args");
        assert!(
            e.1.contains("send-keys-raw"),
            "错误文案没列出真正的 mode 集合，旧 daemon 的使用者会不知道该升级什么：{}",
            e.1
        );
    }

    /// ★★ **本件的核心性质：裸键分支绝不附 `Enter`。**
    ///
    /// 附上了就把「打断当前回合」（`Escape`）变成「**提交用户输入框里排队的文本**」。
    /// 双向钉：裸键那条不许有 `Enter`，而 `send-into` 那条**必须**还有
    /// （否则是把两个语义合并成一个 —— 那才是这一整件要拆开的东西）。
    #[test]
    fn send_keys_raw_never_appends_enter() {
        let src = crate::guard_support::production_code(include_str!("launch.rs"));
        let arm = arm_of(&src, "Mode::SendKeysRaw =>");
        assert!(
            arm.contains("type_keys_raw(&handle"),
            "裸键分支没走 `type_keys_raw`（或没对句柄下手）：{arm}"
        );
        // 抠出两个键入函数的函数体，逐个查 `Enter`。
        let key = "\"Enter\"";
        let raw_body = {
            let at = src
                .find("fn type_keys_raw(")
                .expect("找不到 `type_keys_raw` —— 改名了就把本条一起改");
            let rest = &src[at..];
            &rest[..rest
                .find(tail_brace().as_str())
                .map(|k| k + 3)
                .unwrap_or(rest.len())]
        };
        assert!(
            !raw_body.contains(key),
            "`type_keys_raw` 里出现了 {key} —— 那就不是裸键了。\n\
             生产上唯一会走这条路的是「优雅退出发 `Escape` 打断当前回合」，\n\
             多一个回车 = **提交用户输入框里排队的文本**（`tmux.rs` 头注逐字警告过）。"
        );
        let payload_body = {
            let at = src.find("fn type_payload(").expect("找不到 `type_payload`");
            let rest = &src[at..];
            &rest[..rest
                .find(tail_brace().as_str())
                .map(|k| k + 3)
                .unwrap_or(rest.len())]
        };
        assert!(
            payload_body.contains(key),
            "`type_payload` 不再附 {key} 了 —— 那两个 mode 的区别就消失了，\
             而这一整件存在的理由就是把它们**分开**"
        );
    }

    /// 裸键分支与 `send-into` **同一道门、同一个顺序**；且不许顺手建会话。
    #[test]
    fn the_send_keys_raw_arm_admits_before_it_types_and_never_creates() {
        let src = crate::guard_support::production_code(include_str!("launch.rs"));
        let arm = arm_of(&src, "Mode::SendKeysRaw =>");
        let admit_at = arm
            .find("gate::admit")
            .expect("裸键分支不过 `gate::admit` —— 发裸键也是往别人的会话里打字");
        let type_at = arm
            .find("type_keys_raw")
            .expect("分支里没有 `type_keys_raw`");
        assert!(
            admit_at < type_at,
            "键入排在过门之前 —— 门就没意义了（同 `the_send_into_arm_admits_before_it_types`）"
        );
        let verb = format!("new-{}", "session");
        assert!(!arm.contains(&verb), "裸键分支里出现了建会话");
        // ⚠ Gate 3 **不许**出现：`send-keys` 不删除任何东西。
        // monitor 侧 F04 Phase D 审计修过「给非破坏性动作加 Gate 3」那个错法。
        assert!(
            !arm.contains("admit_destructive"),
            "裸键分支走了带 Gate 3 的门 —— 那会让「往一个多窗口会话里打字」被误拒"
        );
    }

    /// ★★ **覆盖地板（本件补的通用防线）：每个 `Mode` 变体都必须有分支、有解析、被判据扫过。**
    ///
    /// F04c 实测：新增一个变体时，既有那几条「扫某个分支源码」的判据**一条都不会红** ——
    /// 它们只认自己写死的那个分支名。⇒ 加一条**枚举驱动**的判据：变体表变了就红，
    /// 逼人回来给新变体配判据。（同族：`tmux_daemon_gate_guard` 的硬编码文件表。）
    #[test]
    fn every_mode_variant_has_an_arm_and_a_parse_and_is_named_in_some_judge() {
        let raw = include_str!("launch.rs");
        let src = crate::guard_support::production_code(raw);
        // 从 Mode 枚举体里抽变体名。
        let at = src
            .find(enum_mode_head().as_str())
            .expect("找不到 Mode 枚举");
        let body = &src[at..at + src[at..].find(tail_brace().as_str()).expect("枚举没有收尾")];
        let variants: Vec<&str> = body
            .lines()
            .map(str::trim)
            .filter(|l| {
                l.ends_with(',')
                    && !l.contains(' ')
                    && l.chars().next().is_some_and(|c| c.is_ascii_uppercase())
            })
            .map(|l| l.trim_end_matches(','))
            .collect();
        assert_eq!(
            variants.len(),
            3,
            "`Mode` 变体数变了（实得 {variants:?}）—— **这不是让你改数字**：\n\
             新增一个 mode 至少要补三样 —— `Mode::parse` 的一支、`run()` 的一个分支、\n\
             以及一条**扫那个分支源码**的判据（既有那几条只认自己写死的分支名，\n\
             新分支对它们是隐形的）。补完再把这个数棘上来。"
        );
        for v in &variants {
            assert!(
                src.contains(&format!("Mode::{v} =>")),
                "`Mode::{v}` 在 `run()` 里没有分支（或分支头写法不同）"
            );
            assert!(
                src.contains(&format!("Some(Mode::{v})")),
                "`Mode::{v}` 不在 `Mode::parse` 的映射里 —— 那它永远收不到请求"
            );
            // 判据面：整份文件（含 `#[cfg(test)]`）里必须有人点名扫过这个分支。
            assert!(
                raw.contains(&format!("arm_of(&src, \"Mode::{v} =>\")")),
                "没有任何判据用 `arm_of` 扫过 `Mode::{v}` 的分支 —— \n\
                 那个分支可以被改成任何样子而一条判据都不红"
            );
        }
    }
}
