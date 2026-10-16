//! U8a-2b：**平面 ②（远端执行面）** —— 在远端真的建 tmux 会话 / 往已有会话键入载荷。
//!
//! # 这一层归哪、不做什么
//!
//! U8a 把「起会话」拆成三个平面：① 计划面（`resolve_query`，已在这儿）·
//! ② 远端执行面（**本模块**）· ③ 本机开窗面（结构上只能是 monitor —— backend 在远端，
//! 开不了你面前的窗）。所以本模块**不 attach**，一次都不。
//!
//! # ★ argv，不过 shell
//!
//! 今天 monitor 那条路是「渲染一整条 shell 串 → `ssh -t "bash -lic '<串>'"`」，于是
//! 引号 / 转义 / 注入是一整类必须一直防的问题。本模块用
//! `Child::new("tmux").args([...])` 直传 argv ⇒ **那类问题在这条路上不存在**，
//! 不是「被挡住了」。这是搬进后端最实在的收益之一。
//!
//! # ★ 这里的校验是**形状校验**，不是安全边界
//!
//! 入方向命令来自**已经握着这台机器 SSH 会话**的对端 —— 它本来就能在这台机器上跑任意命令。
//! backend 再校验一遍挡不住任何它原本挡不住的东西；假装有一层会更糟：下一个人会以为
//! 那是安全边界，从而放松上游真正在把关的地方（前端的 sid 白名单 / launcher denylist）。
//!
//! # ★ 错误码分两层
//!
//! - **协议级**（`stream/inbound/` 独占）：`bad_request`（信封 JSON 坏了）· `line_too_long` ·
//!   `unknown_command` · `duplicate_id` · `handler_panicked` · `not_cancellable`。
//!   语义是「**客户端代码写错了**，别重试」。
//! - **命令级**（本模块）：`bad_args` · `no_tmux` · `no_such_session` · `create_failed` ·
//!   `typed_unconfirmed`。语义是「参数或环境的问题」，可能来自用户输入。
//!
//! 本模块的形状错误刻意叫 **`bad_args` 而不是 `bad_request`** —— 后者是协议级那一层的，
//! 一词两义会让客户端分不出「我发的 JSON 坏了」与「我发的参数不合适」。
//! （`resolve` 那条今天仍回命令级 `bad_request`：它与手机端的一次性契约冻结在
//! 2026-07-18，两条路复用同一个纯函数，改它会破坏那份契约。**如实登记，不顺手改。**）
//!
//! 所以这里只回答一个问题：**这组参数能不能构成一次有意义的 tmux 调用**。
//! 不能就回一条结构化错误（可诊断、fail-fast），而不是把畸形串塞给 tmux 让它以奇怪的方式失败。
//!
//! ⚠ **明确不抄的一条**：开终端那一行（今天住本机后端 `dial/terminal.rs::render`，原在 monitor）里的「禁双引号」
//! 是 PowerShell 5.1 向 native 程序传参的历史畸变（`wt.exe` 那条路）。**与本模块无关**，
//! 这条路根本不过 shell —— 抄它等于把一个 Windows 怪癖套到 tmux argv 上。
//!
//! # ★ `send-into` 是一等模式（#76 防线的形态迁移）
//!
//! `shared/ccm` 的 `--tmux` 只有幂等 create-or-attach 一种形态，**没有**「就地复用已存在的
//! idle tmux、不新建」。所以 monitor 的 CLI 渲染器（今天是 `ccm_invocation.rs`；TS 那份 LR1 已删）对 `send-into`
//! **诚实放弃、强制走兜底**（那条注释逐字写着「这条是防 #76 复发的关键」）。
//!
//! backend 直接调 tmux 之后那个表达力缺口消失：[`Mode::SendInto`] 是一等模式。
//! **但语义要守死**：会话不存在时**报错，绝不顺手新建** —— 顺手新建就是 #76 的反向
//! （用户以为在复用那个 idle 会话，实际上被丢进一个新建的空 shell）。
//! 由 `send_into_never_creates_a_session` 钉住。

use crate::platform::child::{Child, Deadline};
use copy_core::copy_text;

/// 载荷 / 名字 / cwd 的长度上限。取值同 monitor 侧 `launch.rs::MAX_REMOTE_CMD` 的量级 ——
/// 那是「一条人能读的启动命令」的宽松上界，不是安全阈值。
const MAX_FIELD_BYTES: usize = 8 * 1024;

/// 起会话的模式。**没有 `attach-only`** —— attach 是平面 ③。
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
#[serde(rename_all = "kebab-case")]
pub(crate) enum Mode {
    /// 幂等：会话不在就建 + 键入载荷；已在就**什么都不做**（不重复 resume）。
    CreateOrAttach,
    /// 往一个**已存在**的会话键入载荷。不存在 ⇒ 报错，**绝不新建**。
    SendInto,
}

/// `launch` 的入参（线上那一形；`deny_unknown_fields`：多送一格 ⇒ `bad_args`）。值域由 [`parse_request`] 判。
#[derive(Debug, Clone, serde::Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
#[serde(deny_unknown_fields)]
pub(crate) struct LaunchReq {
    /// `create-or-attach`（建或接）· `send-into`（往已在的会话里键入）。
    pub(crate) mode: Mode,
    /// tmux 会话名。
    pub(crate) name: String,
    /// 要键入的那一行（`create-or-attach` 放行换行 · 制表）。
    pub(crate) payload: String,
    /// 可选，仅 create-or-attach。
    #[serde(default)]
    #[cfg_attr(test, ts(optional))]
    pub(crate) cwd: Option<String>,
    /// 可选：create-or-attach 写成意图键 `@ccm_sid_expect`；send-into 键入挂着它的那个窗格。
    #[serde(default)]
    #[cfg_attr(test, ts(optional))]
    pub(crate) ccm_sid: Option<String>,
    /// 可选，仅 create-or-attach：注册表里的一家（空 ⇒ 默认那一家）。
    #[serde(default)]
    #[cfg_attr(test, ts(optional))]
    pub(crate) agent: Option<String>,
    /// 可选，仅 create-or-attach：1–4 位十进制字符串，与 `height` 同时给或都不给。
    #[serde(default)]
    #[cfg_attr(test, ts(optional))]
    pub(crate) width: Option<String>,
    /// 同 `width`。
    #[serde(default)]
    #[cfg_attr(test, ts(optional))]
    pub(crate) height: Option<String>,
    /// 可选：自报的前端（send-into 过「哪个前端的会话」那一维；create-or-attach 写成会话的 `@ccm_client`）。
    #[serde(default)]
    #[cfg_attr(test, ts(optional))]
    pub(crate) client: Option<String>,
}

/// 一次 `launch` 请求（已过形状校验）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LaunchRequest {
    pub(crate) mode: Mode,
    pub(crate) name: String,
    pub(crate) payload: String,
    pub(crate) cwd: Option<String>,
    /// `create-or-attach`：这个会话将要挂的 sid（写成意图键 `@ccm_sid_expect`）；
    /// `send-into`：键入挂着它的那个窗格（不是会话的当前窗格），身份按那个窗格判。
    pub(crate) ccm_sid: Option<String>,
    /// 哪个 AI（`claude` / `codex`）。落成 tmux 的 `@ccm_agent` 标记。
    ///
    /// ★★ 〔`K-P2` `D` 阶段第三拍 09-03〕**它是为「ccm 的 `--tmux` 真的改走这条路」补的**：
    /// `shared/ccm` 那条本地编排里 `new-session` 之后紧跟着
    /// `set-option -t <名> @ccm_agent <agent>`，而这一侧此前**没有任何字段能表达它**
    /// ⇒ 不补的话，「起会话改走后端」会**静默丢掉 `@ccm_agent`**（`REQUIRED_NEEDLES` 里
    /// 那条 needle 守的正是它），而那是本文件反复消灭的病：**看起来生效了，只是少了一件**。
    pub(crate) agent: Option<String>,
    /// 新建会话的宽 / 高（`--tmux-size <W>x<H>` 的两半）。
    ///
    /// # 为什么是**字符串**而不是数字
    ///
    /// ① 它最终原样进 tmux 的 argv（`-x 220 -y 50`），本来就是文本；
    /// ② 线上一直是串（两个前端照发），换成数就改了线上形。
    /// ⇒ 值域由下面 `check_size` 收窄成「非空、纯十进制、≤4 位」，**不靠类型靠校验**。
    ///
    /// ⚠ **两个必须同时给**：只给一半时 tmux 会用默认值补另一半，
    /// 那是「写了个修饰、看起来生效了、其实只生效了一半」——直接 `bad_args`。
    pub(crate) width: Option<String>,
    pub(crate) height: Option<String>,
    /// 这次请求自报是哪个前端（`gate::requester`）。`send-into` 拿它过「哪个前端的会话」那一维；
    /// `create-or-attach` 新建成了就把它写成那个会话的 `@ccm_client`（ccm 自己起的写 `ccm`）。
    pub(crate) client: Option<String>,
}

/// 一次 `launch` 的结局。三个字段就是「没起成 / 起了但没确认 / 起成了」的载体。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LaunchOutcome {
    /// 本次是否**新建**了会话（幂等短路时为 false）。
    pub(crate) created: bool,
    /// `send-keys` **退出码为 0**（幂等短路时为 false —— 会话已在，不重复 resume）。
    ///
    /// ⚠ **不是「载荷真的落进应用了」**：送之前先退出了窗格模式（copy-mode 吃键那一形没了），但程序起没起来、
    /// shell 读没读它，退出码都说不出。
    /// 由 `typed_is_only_as_strong_as_the_send_keys_exit_code` 钉住语义边界。
    pub(crate) typed: bool,
}

/// 失败：码 ＋ 那一句 ＋ 下层原话（tmux 的 stderr · 起不来时的系统报错，进复制详情）。
pub(crate) type CmdErr = crate::wire::spec::Fail;

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
/// argv 直传，**不引号化**（引号化了就成了名字的一部分）。精确匹配形只住这里（判据 `exact_target_is_the_exact_match_shape`）。
pub(crate) fn exact_target(name: &str) -> String {
    format!("={name}:")
}

/// 已按类型解好的入参 ⇒ **形状校验**（值域）。见模块头注：这不是安全边界。
/// 帧面（typed 分派解入参）与 `ccm --tmux` 那条路（[`super::ccm`] 直接造 [`LaunchReq`]）都过这一道。
pub(crate) fn parse_request(req: LaunchReq) -> Result<LaunchRequest, CmdErr> {
    let LaunchReq {
        mode,
        name,
        payload,
        cwd,
        ccm_sid,
        agent,
        width,
        height,
        client,
    } = req;
    // 〔DUP3 §5 ⑦〕Gate 1 与结束 · 抓屏同一份（`kill::admit_existing_name` → `gate_rules`）；长度照旧。
    super::kill::admit_existing_name(&name)?;
    check_len("name", &name)?;

    // ★★ 🔴 **`create-or-attach` 的 `payload` 放行 `\n` / `\t`，
    //    别的模式一个字节不动。** 这是 `§19 裁六` 登记的那条「真搬那拍的硬前置」。
    //
    // # 为什么必须现在解
    //
    // 那条拒收本来有一个**退路**兜着：`shared/ccm` 判出控制字符就退回本机 tmux 直起，
    // 而本机那条**一直是接受换行的**（`tmux send-keys -t X '第一行<换行>第二行' Enter`）。
    // 用@09-04「统一走后端」把退路删了 ⇒ 那条拒收从「挡住新路」变成**挡住这次调用**，
    // 于是「多行任务」这个**真实能力**整个没了。
    // 现打：`tests/e2e/cc-spawn-uplift.sh` 的「多行任务」一族 **4 条**当场红
    //（仍上总线 / 台账恰好一行 / 那行仍是 4 列 / 换行被转义信息没丢）。
    // ⇒ 这不是判据要不要改的问题，是**能力回归**。修它，不是翻它。
    //
    // # 为什么只放行这两个、只放行这一个模式
    //
    // · `payload` 在这条模式里是**要被键入的东西** —— `\n` 在它里面是一个**有意义的键**
    //   （就是回车），不是畸形字节。本机那条路一直这么理解它。
    // · 别的控制字符（`ESC` / `CR` / `NUL` …）不是「键」，是**会改掉终端状态**的东西
    //   ⇒ 照旧拒收。⚠ 这一格**比本机那条旧路更严**：旧路经 `sq` + `bash` 什么都放过去。
    // · `send-into` **一个字节不动**：`§19 裁六` 逐字「放宽它会同时改掉已有生产调用方的行为」
    //   —— 那条路的 `payload` 语义不同，不该被这一格牵连。裸键那个 mode 已删（无生产调用者）。
    // ⚠ **这里刻意写 `if matches!(…)` 而不是 `match mode { Mode::CreateOrAttach => … }`**
    //   实测时判据当场逮住的：`create_or_attach_never_types_into_a_session_it_did_not_just_create`
    //   用 `arm_of(&src, "Mode::CreateOrAttach =>")` 取那个分支的源码段，而它取的是**第一处**
    //   ⇒ 在 `run()` 之前再写一个同形的 `match` 臂，会把那条判据的**扫描面整个换掉**
    //   （它会去读这里这几行，然后报「分支里没有 type_payload」）。
    //   那正是本仓最贵那族：**尺子量的对象跟标签对不上**。⇒ 换一个不产生那个字面的写法。
    if matches!(mode, Mode::CreateOrAttach) {
        check_typed_payload(&payload)?;
    } else {
        check_field("payload", &payload)?;
    }

    if let Some(c) = &cwd {
        check_field("cwd", c)?;
    }

    if let Some(s) = &ccm_sid {
        check_field("ccm_sid", s)?;
        // 它会被写进 tmux 会话级 option，收紧到确定安全的字符集。⚠ 本命令自己写的是**意图**键 `@ccm_sid_expect`
        //（见 `run` 里那段头注）；这个值要经 `identity_tag` 过检提升之后才进 `@ccm_sid`，
        // 而那一侧有它自己的 `sid_is_safe` —— **两道各自成立，不许因为「上游已经查过」而拆掉任一道**。
        if !s
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        {
            return Err(CmdErr::from((
                "bad_args",
                crate::common::contract::malformed(&format!(
                    "`ccm_sid` must match [A-Za-z0-9_-]: {s:?}"
                )),
            )));
        }
    }

    // `@ccm_agent` 标记的值：给了就得是注册表里由我们起的一家（写空 ⇒ 默认那一家；认不出 ⇒ 拒、列出认得的几家），
    // 标上的是解析好的 kind。没给 ⇒ 不标（这一发起的是什么由载荷说了算，不替它猜）。
    let agent = match agent.as_deref() {
        None => None,
        Some(a) => Some(
            crate::agents::pick_kind(Some(a))
                .map_err(|say| ("bad_args", say))?
                .0
                .to_string(),
        ),
    };

    // 新建会话的尺寸。**两个一起给或都不给**，见字段头注。
    match (&width, &height) {
        (Some(w), Some(h)) => {
            check_size("width", w)?;
            check_size("height", h)?;
        }
        (None, None) => {}
        _ => {
            return Err(CmdErr::from((
                "bad_args",
                crate::common::contract::malformed("`width` and `height` must be given together")
                    .to_string(),
            )))
        }
    }

    let client = super::gate::requester(client.as_deref())?;

    Ok(LaunchRequest {
        mode,
        name,
        payload,
        cwd,
        ccm_sid,
        agent,
        width,
        height,
        client,
    })
}

/// 尺寸那两个字段的值域：**非空、纯十进制、≤4 位**（tmux 自己的上限远小于此）。
///
/// ⚠ 刻意**不复用** [`check_field`]：那一条是给「一条人能读的启动命令」定的（8 KiB 上限），
/// 拿它守一个尺寸数等于什么都没守 —— `-x` 后面跟一个 8000 字符的串，tmux 会把它当参数错误，
/// 而错误信息里看不出是谁给的。
fn check_size(what: &str, v: &str) -> Result<(), CmdErr> {
    if v.is_empty() || v.len() > 4 || !v.chars().all(|c| c.is_ascii_digit()) {
        return Err(CmdErr::from((
            "bad_args",
            crate::common::contract::malformed(&format!(
                "`{what}` must be 1-4 decimal digits: {v:?}"
            )),
        )));
    }
    Ok(())
}

fn check_field(what: &str, v: &str) -> Result<(), CmdErr> {
    if v.trim().is_empty() {
        return Err(CmdErr::from((
            "bad_args",
            crate::common::contract::malformed(&format!("`{what}` is empty")),
        )));
    }
    check_len(what, v)?;
    // 控制字符会让 send-keys 的语义变掉（`\n` = 多敲一次回车）。形状问题。
    if v.chars().any(char::is_control) {
        return Err(CmdErr::from((
            "bad_args",
            crate::common::contract::malformed(&format!("`{what}` contains a control character")),
        )));
    }
    Ok(())
}

fn check_len(what: &str, v: &str) -> Result<(), CmdErr> {
    if v.len() > MAX_FIELD_BYTES {
        return Err(CmdErr::from((
            "bad_args",
            crate::common::contract::malformed(&format!(
                "`{what}` is too long ({} > {MAX_FIELD_BYTES})",
                v.len()
            )),
        )));
    }
    Ok(())
}

/// `create-or-attach` 的 `payload` 专用检查。
///
/// 与 [`check_field`] **只差一条**：放行 `\n` 与 `\t`。理由与射程写在
/// `parse_request` 里调用它的那一处（那里离决策更近）。
///
/// ⚠ **空 / 过长两条一个字不改** —— 复用 [`check_field`] 判它们，
/// 不在这里抄第二份（抄了就是「同一条规矩两处实现」）。
fn check_typed_payload(v: &str) -> Result<(), CmdErr> {
    // 先把两个「有意义的键」抹掉，再交给原来那把尺子 ——
    // 这样「空」「过长」「别的控制字符」三条判法**逐字复用**，且长度判的仍是原串
    // （下面那次 `check_field` 收到的是抹过的串，只用来判控制字符）。
    if v.trim().is_empty() {
        return Err(CmdErr::from((
            "bad_args",
            crate::common::contract::malformed("`payload` is empty"),
        )));
    }
    if v.len() > MAX_FIELD_BYTES {
        return Err(CmdErr::from((
            "bad_args",
            crate::common::contract::malformed(&format!(
                "`payload` is too long ({} > {MAX_FIELD_BYTES})",
                v.len()
            )),
        )));
    }
    if let Some(bad) = v
        .chars()
        .find(|c| c.is_control() && *c != '\n' && *c != '\t')
    {
        return Err(CmdErr::from((
            "bad_args",
            crate::common::contract::malformed(&format!(
                "`payload` has a control character other than \\n / \\t (U+{:04X})",
                bad as u32
            )),
        )));
    }
    Ok(())
}

/// 一次 tmux 子命令的结局：成没成 ＋ **tmux 自己说的那句话**（stderr）。
///
/// 原先只回一个 bool、stderr 丢进 `Stdio::null()` ⇒ 建会话失败时只能猜
/// （「cwd 不可用？名字非法？」）；隔壁 `kill.rs` 用 `output()` 把 tmux 的原话放进错误 —— 正确形状就在那儿，照它办。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Ran {
    pub(crate) ok: bool,
    /// tmux 的 stderr（[`said_of`]：去首尾空白、有上限；一个字都没说 ⇒ 「（tmux 没说原因）」）。是原话，不是成品句。
    pub(crate) stderr: String,
}

/// tmux 自己说的那句话（stderr）：去首尾空白、截到 [`SAID_CAP`] 字节（截在字符边界上，截了标 `…`）；
/// 一个字都没说 ⇒ 「（tmux 没说原因）」。
pub(crate) fn said_of(stderr: &[u8]) -> String {
    let text = String::from_utf8_lossy(stderr);
    let text = text.trim();
    if text.is_empty() {
        return copy_text("beLaunch.said.silent", &[]);
    }
    if text.len() <= SAID_CAP {
        return text.to_string();
    }
    let mut end = SAID_CAP;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}…", &text[..end])
}

/// tmux 原话进原因时的上限（字节）。tmux 的报错是一行短话；上限只防一个坏掉的 tmux 灌进一整屏。
const SAID_CAP: usize = 400;

/// 跑一次 tmux 子命令。**argv 直传，不过 shell。**
fn tmux(args: &[&str]) -> Result<Ran, CmdErr> {
    ran(Child::new("tmux"), args)
}

/// [`tmux`] 的本体：`cmd` 由调用方造（生产 = `Child::new("tmux")`；判据换一个假 tmux 的绝对路径，
/// 不碰进程级 `PATH`）。stdout 照旧不要（这几条子命令本来就不往 stdout 写）。
pub(crate) fn ran(cmd: Child, args: &[&str]) -> Result<Ran, CmdErr> {
    match cmd.args(args).run(LAUNCH_TMUX_WITHIN) {
        Ok(out) => Ok(Ran {
            ok: out.status.success(),
            stderr: said_of(&out.stderr),
        }),
        Err(e) => Err(CmdErr::from(e.into_cmd_said("no_tmux", |why| {
            copy_text("beLaunch.run.noTmux", &[("why", why)])
        }))),
    }
}

/// 建会话 · 打标 · 送键那几发 tmux 各自的期限：tmux 一发 5 s（同 watcher 探测 tmux 的期限）。
const LAUNCH_TMUX_WITHIN: Deadline = Deadline::secs(5);

/// `launch` 整条命令总期限的上限（过门 · 建会话 · 打标 · 送键共用）。
pub(crate) const LAUNCH_CAP: Deadline = Deadline::secs(8);

/// 建会话之后那几步**次要动作**（失败不阻断键入载荷）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Secondary {
    /// agent 标（`@ccm_agent`）。
    AgentTag,
    /// 身份**意图**键（建会话的人声明「打算跑这个 sid」）。
    IntentTag,
    /// 谁起的（`@ccm_client`）。
    ClientTag,
}

impl Secondary {
    /// 这一步没做成的后果（一句）。
    fn consequence(self) -> &'static str {
        match self {
            Secondary::AgentTag => "agent 标没打上，这个会话在列表里认不出是哪个 agent",
            Secondary::IntentTag => {
                "身份意图没写上：之后对这个会话的 kill / 送键可能被身份门拒（wrong_owner）"
            }
            Secondary::ClientTag => "谁起的没写上：这个会话按名字规则判身份",
        }
    }
    fn what(self) -> &'static str {
        match self {
            Secondary::AgentTag => "打 agent 标",
            Secondary::IntentTag => "写身份意图",
            Secondary::ClientTag => "写谁起的",
        }
    }
}

/// 次要动作的结局 → **一句给日志的话**；做成了 ⇒ `None`。
///
/// 「身份标记与标题是次要动作：失败绝不阻断主要动作」那条决定不动（「处置不是别吞，
/// 是吞了要留一行日志」）—— 本函数只管「说什么」，抽成纯的才判得到；打日志的是 [`secondary`]。
pub(crate) fn secondary_note(
    session: &str,
    step: Secondary,
    r: &Result<Ran, CmdErr>,
) -> Option<String> {
    let why = match r {
        Ok(Ran { ok: true, .. }) => return None,
        Ok(Ran { ok: false, stderr }) => stderr.clone(),
        Err(f) => crate::common::said::Said::from(f.clone()).logged(),
    };
    Some(format!(
        "会话 {session:?} 建好了，但{}没做成：{why} —— {}",
        step.what(),
        step.consequence()
    ))
}

/// 跑一步次要动作；没做成 ⇒ 留一行日志、不阻断。
fn secondary(
    tmux: &dyn Fn(&[&str]) -> Result<Ran, CmdErr>,
    session: &str,
    step: Secondary,
    args: &[&str],
) {
    if let Some(note) = secondary_note(session, step, &tmux(args)) {
        tracing::warn!("{note}");
    }
}

/// 真做事。**只在 `Disposition::spawn` 的独立 task 上跑** —— 它会阻塞（起进程）。
/// J6 新建那一条（`crate::control::gate_rules::new_tmux_name_issue`）判不过时说哪一句；过得了 ⇒ `None`。
/// `ccm` 铸名（`ccm/plan.rs::validate_tmux_name`）与 `create-or-attach` 新建那一支说的是同一句。
pub(crate) fn new_tmux_name_said(n: &str) -> Option<String> {
    use crate::control::gate_rules::TmuxNameIssue as I;
    Some(match crate::control::gate_rules::new_tmux_name_issue(n)? {
        I::Empty | I::LeadingDash => {
            copy_text("bePlan.validateTmuxName.emptyOrDash", &[("name", n)])
        }
        I::TargetSyntax(_) => copy_text("bePlan.validateTmuxName.targetSyntax", &[("name", n)]),
        I::Control(_) => copy_text("bePlan.validateTmuxName.control", &[("name", n)]),
        I::Deceptive(_) => copy_text("bePlan.validateTmuxName.deceptive", &[("name", n)]),
        I::TooLong => copy_text(
            "bePlan.validateTmuxName.tooLong",
            &[
                ("name", n),
                (
                    "max",
                    &crate::control::gate_rules::NEW_TMUX_NAME_MAX.to_string(),
                ),
            ],
        ),
    })
}

pub(crate) fn run(req: &LaunchRequest) -> Result<LaunchOutcome, LaunchFail> {
    run_with(req, &tmux)
}

/// 送键那一发没确认的两种：tmux 明说没送进（`send-keys` 退非零）· tmux 没应答（送没送到不知道）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Delivery {
    NotDelivered,
    Unknown,
}

/// [`run`] 的失败：线上那一份 [`CmdErr`] ＋ 送键没确认时是哪一种、会话是不是本趟新建的。
/// 线上只看 `fail`（两种同码 `typed_unconfirmed`）；`untyped` 给命令行那一处决定接进去还是收掉（`ccm::execute`）。
#[derive(Debug, PartialEq)]
pub(crate) struct LaunchFail {
    pub(crate) fail: CmdErr,
    /// `Some((哪一种, 本趟建的那个会话的令牌))`：只在送键那一发上有。
    pub(crate) untyped: Option<(Delivery, Option<JustMade>)>,
}

impl std::ops::Deref for LaunchFail {
    type Target = CmdErr;
    fn deref(&self) -> &CmdErr {
        &self.fail
    }
}

impl From<CmdErr> for LaunchFail {
    fn from(fail: CmdErr) -> Self {
        LaunchFail {
            fail,
            untyped: None,
        }
    }
}

impl From<(&'static str, String)> for LaunchFail {
    fn from(e: (&'static str, String)) -> Self {
        CmdErr::from(e).into()
    }
}

/// 本趟刚建成的会话（精确目标 `=名:`）：字段私有、不能复制，只有建会话那一臂（`new-session` 成功之后）造得出。
/// 往里送启动命令（[`TypeAt::JustMade`]）与没送进时收掉它（[`super::kill::end`]）都只收它。
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct JustMade(String);

impl JustMade {
    /// 对它下命令用的精确目标。
    pub(crate) fn target(&self) -> &str {
        &self.0
    }
}

/// 启动命令明确没送进 ⇒ 收掉本趟建的那个只剩 shell 的会话（真 tmux）。回「这一趟把它收掉了」。
pub(crate) fn reap(made: JustMade) -> bool {
    super::kill::end(super::kill::Ending::Bare(made, &tmux)).is_ok()
}

/// 新建那一下（`create-or-attach` 与开空 shell 同一份）：名字过 J6 新建那一条；会话已在 ⇒ 幂等、什么都不做（回 `false`）；
/// 建不出 ⇒ `create_failed` 带 tmux 原话。`size` ＝ `--tmux-size` 的两半（只对新建生效）。
fn create(
    name: &str,
    cwd: Option<&str>,
    size: Option<(&str, &str)>,
    tmux: &dyn Fn(&[&str]) -> Result<Ran, CmdErr>,
) -> Result<bool, CmdErr> {
    let t = exact_target(name);
    // 过不了新建那一条、而那个会话已经在 ⇒ 照旧幂等接回（已有会话走的是已有那一条，进门时判过）；不在 ⇒ 拒并说清。
    if let Some(said) = new_tmux_name_said(name) {
        if tmux(&["has-session", "-t", &t])?.ok {
            return Ok(false);
        }
        return Err(CmdErr::from(("bad_args", said)));
    }
    let mut new_args: Vec<&str> = vec!["new-session", "-d", "-s", name];
    if let Some(cwd) = cwd {
        new_args.push("-c");
        new_args.push(cwd);
    }
    // detached 会话默认 80x24，太窄会把 agent 输出折行；已有会话的尺寸归它自己（可能有人正接着）。
    if let Some((w, h)) = size {
        new_args.extend(["-x", w, "-y", h]);
    }
    // 幂等闸：会话已存在 ⇒ new-session 失败 ⇒ 分辨「已存在」与「真的建不出来」（看退出码，不吞 stderr）。
    let made = tmux(&new_args)?;
    if made.ok {
        return Ok(true);
    }
    if tmux(&["has-session", "-t", &t])?.ok {
        return Ok(false);
    }
    Err(CmdErr::new(
        "create_failed",
        copy_text("beLaunch.create.failed", &[("name", name)]),
    )
    .with_raw(Some(&made.stderr)))
}

/// 开一个空 shell 的 tmux 会话（不起 agent、不键入）：不在 ⇒ 建在 `cwd` 里、记下是哪个前端建的；已在 ⇒ 不动它。回「这一趟建的」。
pub(crate) fn open_shell(
    name: &str,
    cwd: Option<&str>,
    client: Option<&str>,
) -> Result<bool, CmdErr> {
    open_shell_with(name, cwd, client, &tmux)
}

/// [`open_shell`] 的本体（起 tmux 那一下由调用方交进来）。
pub(crate) fn open_shell_with(
    name: &str,
    cwd: Option<&str>,
    client: Option<&str>,
    tmux: &dyn Fn(&[&str]) -> Result<Ran, CmdErr>,
) -> Result<bool, CmdErr> {
    let made = create(name, cwd, None, tmux)?;
    if let Some(client) = client.filter(|_| made) {
        let t = exact_target(name);
        secondary(
            tmux,
            name,
            Secondary::ClientTag,
            &["set-option", "-t", &t, "@ccm_client", client],
        );
    }
    Ok(made)
}

/// [`run`] 的本体：起 tmux 的那一下由调用方交进来（生产 = [`tmux`]；判据交一个不起进程的替身，
/// 驱动 `create-or-attach` 那一臂的每一格结局）。`send-into` 那一臂的门（`gate::admit`）照旧起真 tmux。
fn run_with(
    req: &LaunchRequest,
    tmux: &dyn Fn(&[&str]) -> Result<Ran, CmdErr>,
) -> Result<LaunchOutcome, LaunchFail> {
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
            //   键入只收门发的令牌（`TypeAt::Admitted`）⇒ 不过门编不过。
            //   带了 `ccm_sid` ⇒ 句柄是挂着它的那个窗格（一个 tmux 会话里可以有几个 claude），身份也按它判。
            let at =
                super::gate::admit(&req.name, &t, req.ccm_sid.as_deref(), req.client.as_deref())?;
            type_payload(TypeAt::Admitted(&at), &req.name, &req.payload, tmux)?;
            Ok(LaunchOutcome {
                created: false,
                typed: true,
            })
        }
        Mode::CreateOrAttach => {
            let size = req.width.as_deref().zip(req.height.as_deref());
            if !create(&req.name, req.cwd.as_deref(), size, tmux)? {
                return Ok(LaunchOutcome {
                    created: false,
                    typed: false,
                });
            }
            // 走到这里的只可能是本趟刚建成的会话 ⇒ 发「刚建的」令牌（唯一一处）。
            let fresh = JustMade(t.clone());
            // 身份标记与标题是**次要**动作：失败绝不阻断主要动作（键入载荷）。
            // 与 `ccm` 容器路字符串形（`control/ccm/plan.rs::render_container`）里 `(… 2>/dev/null || true) &&` 同一条纪律
            // （原来点的是 TS 座 `session-backend.ts` 与后端载荷那一层，两份都删了）。
            //
            // ★★ **建会话这一刻写的是「意图」，不是「事实」**〔`K-P2` C 第五拍，09-03；
            //    PM `§13 裁三` 裁「候选丙」〕。
            //
            // # 它修的是什么（**不是**冒名，别把两件事压成一句）
            //
            // 这一处落在**幂等闸之后** —— 会话已存在 ⇒ 上面 `new-session` 失败 ⇒ 短路
            // 返回 `created:false` ⇒ 走到这里的**只可能是刚刚新建成功的那个会话**。
            // 所以它够不着别人的会话，「冒名」那一层在这里不成立。
            //
            // 它真正的问题是**过早取得权威**：`@ccm_sid` 是破坏性动作（`kill`，
            // `super::gate::admit_destructive` → `crate::control::gate_rules::gate2`）**唯一认的事实**。
            // 建会话即写它 ⇒ 一个「声明了 sid、但那个 claude 进程还没起（甚至永远起不来）」
            // 的空会话**当场获得事实身份**。那正是 F04 修掉的 `R10`：
            // 意图（通道 A）与事实（通道 B）之间的那道确认被绕过去了。
            //
            // # 事实由谁写：仍然只有一个人
            //
            // [`super::identity_tag`]：pidfile 出现 **＋** 过 `procStart` 冒名检查之后才写
            //（那份文件逐字「打错就是杀错」）。本处一个字都不写 `@ccm_sid`
            // —— 今天由 `readonly_guard`（写 tmux 变量那一族只许 `control/identity_tag.rs`）与
            // `ccm/plan_tests.rs::the_container_path_carries_every_intent_inward`（渲出的命令只写意图键）钉住。
            //
            // ⚠ **为什么可以现在就改**：这条臂今天**零生产调用方** ——
            //   `launch_wire::the_create_or_attach_mode_is_sent_only_by_the_ccm_container_path`
            //   与 `readonly_guard` 的 `g6_staged_zero` 两条判据一起钉着「生产段不发
            //   `create-or-attach`」。⇒ 本改动今天不改变任何一条在跑的路径的行为，
            //   它是把「接线时会踩的那颗雷」在接线之前拆掉。
            //
            // `@ccm_agent`：与 `shared/ccm` 那条本地编排**同一个顺序**
            // （`new-session` → `@ccm_agent` → `@ccm_sid_expect` → `send-keys`）。
            // 同样是**次要动作**：失败不阻断键入载荷（`shared/ccm` 那边写的是 `|| true`）。
            // 两步都是次要动作：没做成 ⇒ [`secondary`] 留一行日志（哪一步 · tmux 说的 · 后果），不阻断。
            if let Some(agent) = &req.agent {
                secondary(
                    tmux,
                    &req.name,
                    Secondary::AgentTag,
                    &["set-option", "-t", &t, "@ccm_agent", agent],
                );
            }
            if let Some(sid) = &req.ccm_sid {
                secondary(
                    tmux,
                    &req.name,
                    Secondary::IntentTag,
                    &["set-option", "-t", &t, "@ccm_sid_expect", sid],
                );
            }
            // 谁起的（「哪个前端的会话」那一维）：建会话的人自己声明，写在 `send-keys` 之前。
            if let Some(client) = &req.client {
                secondary(
                    tmux,
                    &req.name,
                    Secondary::ClientTag,
                    &["set-option", "-t", &t, "@ccm_client", client],
                );
            }
            type_payload(TypeAt::JustMade(fresh), &req.name, &req.payload, tmux)?;
            Ok(LaunchOutcome {
                created: true,
                typed: true,
            })
        }
    }
}

/// 往哪键入：门发的令牌（`send-into`），或本趟刚建成的那个会话的令牌（`create-or-attach`；没送进时随失败交回去收）。
enum TypeAt<'a> {
    Admitted(&'a super::gate::Admitted),
    JustMade(JustMade),
}

/// 键入载荷。失败 ⇒ `typed_unconfirmed`：**会话在，载荷未必落**。
///
/// 这一档是 DoD 6 那条「起了但没确认」的落点：调用方**不许**据此重试 create
/// （会话确实在），该做的是告诉用户「会话建好了但没能把命令打进去」。
///
/// **先退出窗格模式，再送**，两条在同一串 tmux 命令里（`copy-mode -q ; send-keys …`，tmux 一口气跑完，别的插不进中间）：
/// 窗格在 copy-mode / view-mode 里时送的键归那个模式的键表，命令进不了 shell（10-13：一条出声的钩子把新会话切进 view-mode，
/// `ccm` 送的启动命令就这么被吃掉）。`copy-mode -q` 不在模式里时也退 0。`send-into` 那一臂同样：用户点了「就地 resume」，
/// 那个窗格正翻着屏的话翻屏被退出。不改成「把命令交给 `new-session`」：那会让窗格进程就是 agent、一退窗格就没了，
/// 空 shell 那一形（`send-into` 往里敲的就是它）跟着没了。
/// 送键那一发过了期限 ⇒ 也归 `typed_unconfirmed`（会话在，送没送到不知道），期限那句进复制详情。
/// `name`：说给人听的那个 tmux 会话名。本趟新建的那一形失败时把令牌带回去，命令行那一处据此决定收不收。
fn type_payload(
    at: TypeAt<'_>,
    name: &str,
    payload: &str,
    tmux: &dyn Fn(&[&str]) -> Result<Ran, CmdErr>,
) -> Result<(), LaunchFail> {
    let target = match &at {
        TypeAt::Admitted(a) => a.handle(),
        TypeAt::JustMade(m) => m.target(),
    };
    // 一串里先退出窗格模式、再送（见上）。
    #[rustfmt::skip]
    let sent = tmux(&["copy-mode", "-q", "-t", target, ";", "send-keys", "-t", target, payload, "Enter"]);
    let made = match at {
        TypeAt::JustMade(m) => Some(m),
        TypeAt::Admitted(_) => None,
    };
    match sent {
        Ok(r) if r.ok => Ok(()),
        Ok(r) => Err(unconfirmed(Delivery::NotDelivered, name, &r.stderr, made)),
        Err(e) if e.code == crate::platform::child::TIMED_OUT => {
            Err(unconfirmed(Delivery::Unknown, name, &e.message, made))
        }
        Err(e) => Err(e.into()),
    }
}

/// [`type_payload`] 没确认的那一份：同码 `typed_unconfirmed`，句子按哪一种说，tmux 原话进复制详情。
fn unconfirmed(delivery: Delivery, name: &str, raw: &str, made: Option<JustMade>) -> LaunchFail {
    let said = match delivery {
        Delivery::NotDelivered => copy_text("beLaunch.type.failed", &[("name", name)]),
        Delivery::Unknown => copy_text("beLaunch.type.timedOut", &[("name", name)]),
    };
    LaunchFail {
        fail: CmdErr::new("typed_unconfirmed", said).with_raw(Some(raw)),
        untyped: Some((delivery, made)),
    }
}

/// 终端管理送字的那一段字面字过不过形状：非空、不超长、除 `\n` / `\t` 外没有控制字符（按键走 [`press_key`]）。
pub(crate) fn check_input_text(text: &str) -> Result<(), CmdErr> {
    check_typed_payload(text)
}

/// 送一段**字面字**（终端管理 L1 送字那一半）：单行 `send-keys -l`（不解释成键名）；多行按粘贴送
/// （`paste-buffer -p`：程序开了括号粘贴就带上，用完即删那个缓冲）。`enter` ⇒ 之后再补一个回车键。
/// 只收门发的令牌（[`super::gate::admit`] · [`super::gate::admit_with`]）；`tmux` 同 [`run_with`] 那一口（测试交隔离 socket 上的）。
pub(crate) fn type_literal(
    at: &super::gate::Admitted,
    text: &str,
    enter: bool,
    tmux: &dyn Fn(&[&str]) -> Result<Ran, CmdErr>,
) -> Result<Ran, CmdErr> {
    let target = at.handle();
    let sent = if text.contains('\n') {
        static SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let buf = format!(
            "ccm-input-{}-{}",
            std::process::id(),
            SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        );
        let set = tmux(&["set-buffer", "-b", &buf, "--", text])?;
        if !set.ok {
            return Ok(set);
        }
        tmux(&["paste-buffer", "-p", "-d", "-b", &buf, "-t", target])?
    } else {
        tmux(&["send-keys", "-t", target, "-l", "--", text])?
    };
    if !sent.ok || !enter {
        return Ok(sent);
    }
    tmux(&["send-keys", "-t", target, "Enter"])
}

/// 送一个键（`key` 是 tmux 的键名，来自终端管理那张有限键表；不收任意串）。只收门发的令牌。
pub(crate) fn press_key(
    at: &super::gate::Admitted,
    key: &str,
    tmux: &dyn Fn(&[&str]) -> Result<Ran, CmdErr>,
) -> Result<Ran, CmdErr> {
    tmux(&["send-keys", "-t", at.handle(), key])
}

/// 入方向命令的入口：`args` → 结局 JSON。
/// `launch` 的回包。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub(crate) struct Launched {
    /// 落在哪个会话上。
    pub(crate) session: String,
    /// 这一次新建了会话。
    pub(crate) created: bool,
    /// `send-keys` 退出 0：只有 `send-keys` 的退出码那么强（送之前同一串命令里先退出窗格模式（copy-mode 等），键不会被模式吃掉；程序起没起来它不知道）。
    pub(crate) typed: bool,
}

/// 帧面成品 `{session, created, typed}` 的构造器（纯）：跨语言金样 `tests/__fixtures__/tmux-control.golden.json` 拿**同一个**构造器对拍。
pub(crate) fn reply(session: &str, created: bool, typed: bool) -> Launched {
    Launched {
        session: session.to_string(),
        created,
        typed,
    }
}

crate::wire::typed::codes! {
    /// `launch` 的码：形状 · 过门（send-into）· 建 / 键入那几发 tmux。
    pub(crate) enum LaunchErr {
        /// tmux 起不来。
        NoTmux = "no_tmux",
        /// send-into：没有这个会话（绝不新建）。
        NoSuchSession = "no_such_session",
        /// send-into：身份门不过。
        WrongOwner = "wrong_owner",
        /// 建会话那一发失败。
        CreateFailed = "create_failed",
        /// 会话在，启动命令没送进去：`send-keys` 退非零，或那一发过了期限（送没送到不知道）。
        TypedUnconfirmed = "typed_unconfirmed",
        /// 那一发 `tmux` 过了期限。
        ChildTimedOut = "child_timed_out",
    }
}

/// `launch`：建 tmux 会话并键入载荷，或键入一个已在的会话（远端执行面）。阻塞档（起 tmux）。
pub(crate) struct Launch;

impl crate::wire::typed::Command for Launch {
    const NAME: &'static str = "launch";
    type Req = LaunchReq;
    type Reply = Launched;
    type Err = LaunchErr;
}

impl crate::wire::typed::Handle for Launch {
    type Needs = crate::wire::typed::needs::Nothing;
    fn run(
        req: LaunchReq,
        _cx: crate::wire::typed::Cx<Self::Needs>,
    ) -> impl std::future::Future<Output = Result<Launched, LaunchErr>> + Send + 'static {
        async move {
            let of = |f| crate::wire::typed::of_fail(f, LaunchErr::CreateFailed);
            let req = parse_request(req).map_err(of)?;
            let out = run(&req).map_err(|e| of(e.fail))?;
            Ok(reply(&req.name, out.created, out.typed))
        }
    }
}

#[cfg(test)]
#[path = "../../../tests/backend/control/launch_tests.rs"]
mod tests;
