//! **抓一屏的只读原语**（tmux 的 `capture-pane`）：把某个窗格**此刻**那一屏的文本抓回来，抓完就返回。
//! 调用方是终端管理那几条（`control/terminals.rs`：`terminal-preview` 抓屏 · `terminal-input` 送之前比指纹）；
//! 本模块自己不上帧面、不上 CLI 面（目标只认名单里的终端，不收 tmux 目标串）。
//!
//! # 🔴 只出原语，不出轮询（`KR86D3`）
//!
//! 本模块**抓一次、立刻返回**。「抓几次 / 隔多久再抓一次」由**调用方**决定 ——
//! 那一半归 `K-R87`。零定时器铁律（`no_timer_guard`）的人群包含本文件，
//! 任何会让线程自己醒来的构件在这里当场红；另有
//! `readonly_guard::capture_is_read_only::the_capture_site_is_one_shot` 钉住
//! 「这一处恰好起一次进程、零循环构件」。**一次抓一屏是刀口纪律，不是性能取舍。**
//!
//! # ★ 为什么它住 `control/`（这是判断，不是实测 —— 写出来让下一个人能反对）
//!
//! 它**只读**，按「读 / 改变世界」那条线看更像 `observe/`。三条现打的理由压向 `control/`：
//!
//! 1. **tmux 这个外部程序的 argv 直传承载今天全在 `control/` 与 `common/`**
//!    （`gate` / `kill` / `launch` / `identity_tag` / `tmux_hook` / `session_snapshot`），
//!    `observe/` 那一侧碰 tmux 走的是 `platform/shell.rs` 的 `sh -c`，与本处形态不同。
//! 2. **进不了 `common/`**：那一层的门槛①是「≥2 个**上层**用」，而本模块今天
//!    只有一个调用方（`control/terminals.rs`）⇒ 不达标，硬塞就是把门槛变成杂物间。
//! 3. ⚠ **`gate` 那条「只读但归 control/」的理由（定框 C13：有没有决策权）在这里
//!    不成立** —— 本模块不参与任何「能不能改这个会话」的决策。不许照抄那句话。
//!
//! # 三态分得开（`KR86D2`）：**抓不到时说得出真实原因，不是静默空串**
//!
//! | 结局 | 判据来自哪儿 | 码 |
//! |---|---|---|
//! | tmux 这个程序起不来 | `Command::output()` 自己的 `Err`（内核级，不猜） | `no_tmux` |
//! | tmux 在、但一个 server 都没有 | 退出码非零 ＋ stderr 命中 [`NO_SERVER_NEEDLES`] | `no_server` |
//! | server 在、这个目标不存在 | 退出码非零 ＋ stderr 命中 [`NO_TARGET_NEEDLES`] | `no_such_session` |
//! | 其它失败 | 退出码非零、两张针都不命中 | `capture_failed` ＋ **stderr 原样回包** |
//! | 成功 | 退出码 0 | `Ok(那一屏)` |
//!
//! ⚠ **空屏是合法的成功**：一个刚建起来、什么都没打印的 pane 抓回来就是空串、退出码 0。
//! 所以「抓不到」这件事**不靠输出是不是空**判 —— 靠退出码。
//! （这一条与 [`super::gate::probe`] 相反：那边 `display-message` 对不存在的目标是
//!  **rc=0 + 空输出**，只能拿「输出为空」当判据；`capture-pane` 对不存在的目标是
//!  **rc≠0 + stderr 有话**，backend 直接拿得到，不必像 monitor 那侧渲染 `NO_PANE` 哨兵。）
//!
//! ⚠ **兜底那一档是「说不清但不撒谎」**：tmux 换了措辞 ⇒ 两张针都不命中 ⇒ 落
//! `capture_failed` 并把 stderr **原样**带出去。它比「压成会话不存在」诚实 ——
//! 后者是拿一个**具体而错误**的答案冒充知识。
//!
//! # 只读，而且「只读」这件事有人在数
//!
//! 这一处起进程已登记进 `readonly_guard::spawn_registry::ALLOWED`。⚠ 那张表的键是
//! `(文件, 起什么程序)`，**分不出被调的是哪条 tmux 子命令**（`ALLOWED` 里
//! `plugin/invoke.rs` 那条自陈过这个 `K6b` 盲区）⇒ 光登记买不到「只读」。
//! 真正钉住它的是 `readonly_guard::capture_is_read_only`：
//! **值级**（这一处发出去的 argv 逐元素就是那条只读形）＋ **文本级**
//! （生产段里不出现任何会改 tmux 状态的动词）＋ 反向自检（合成样本必须被逮到）。

use crate::common::child_env::WithoutOwnEnv;
use crate::common::tmux_utf8::UTF8_CLIENT_FLAG;
use copy_core::copy_text;
use std::process::{Command, Stdio};

/// 命令级错误：`(code, message)`。与 [`super::launch`] / [`super::gate`] / [`super::kill`] 同型。
pub(crate) type CmdErr = (&'static str, String);

/// tmux 的「把这一屏打到 stdout」子命令。
pub(crate) const CAPTURE_SUBCOMMAND: &str = "capture-pane";

/// `-p` = 打到 stdout（而不是存进 tmux 自己的 buffer —— 存 buffer 会**改 tmux 状态**）。
pub(crate) const PRINT_TO_STDOUT: &str = "-p";

/// `-t` = 目标。
pub(crate) const TARGET_FLAG: &str = "-t";

/// `-e` = 带上颜色等属性（SGR 转义）。只影响打到 stdout 的样子，不改 tmux 状态。
pub(crate) const WITH_ESCAPES: &str = "-e";

/// `-S <起始行>`：负数 = 往回多要几行历史。只读。
pub(crate) const START_LINE: &str = "-S";

/// 这一处**只许**发的 argv：要不要颜色、往回几行。
///
/// 🔴 顺序有两条硬约束，都不是排版：
/// - [`UTF8_CLIENT_FLAG`] **必须排在子命令之前**（`K-R12`：放后面是
///   `rc=1 + unknown flag -u`，而那个响错会被下面的判法读成「抓不到」）；
/// - [`PRINT_TO_STDOUT`] 不许换成 `-b`／落 buffer 的写法 —— 那就不再是只读。
///
/// 逐元素由 `readonly_guard::capture_is_read_only` 钉死。
pub(crate) fn capture_argv_with(target: &str, color: bool, back: u32) -> Vec<String> {
    let mut v: Vec<String> = vec![
        UTF8_CLIENT_FLAG.into(),
        CAPTURE_SUBCOMMAND.into(),
        PRINT_TO_STDOUT.into(),
    ];
    if color {
        v.push(WITH_ESCAPES.into());
    }
    if back > 0 {
        v.push(START_LINE.into());
        v.push(format!("-{back}"));
    }
    v.push(TARGET_FLAG.into());
    v.push(target.into());
    v
}

/// 抓一次那一屏（终端预览用：要不要颜色、往回几行）。`target` 由调用方从名单里取（窗格 / 会话的 tmux ID）。
pub(crate) fn capture_with_on(
    socket: Option<&str>,
    target: &str,
    color: bool,
    back: u32,
) -> Result<String, CmdErr> {
    let argv = capture_argv_with(target, color, back);
    let argv: Vec<&str> = argv.iter().map(String::as_str).collect();
    classify(&spawn_tmux(socket, &argv)?)
}

/// stderr 里「这台机上一个 tmux server 都没有」的形状。
///
/// `(针, 为什么它算这一档)` —— 加一条要连理由一起加。
pub(crate) const NO_SERVER_NEEDLES: &[(&str, &str)] = &[
    (
        "error connecting to",
        "🔴 **本轮真 tmux 打回来的那一句**（沙箱 tmux，显式 `-S <不存在的路径>`）：\
         逐字 `error connecting to /tmp/…/sock (No such file or directory)`。\
         ⚠ 它与下面那条**不是同一句话**，而我先前只登记了下面那条 ⇒ \
         第一趟门禁当场红（落进兜底档 `capture_failed`）。**留着这行来历**：\
         「tmux 连不上时会说 no server running」是个想当然，实测把它证伪了一半。",
    ),
    (
        "no server running",
        "tmux 在**默认 socket**（生产走的那条）上找不到 server 时的原话\
         （`no server running on /tmp/tmux-<uid>/default`）。\
         ⚠ **这一句本轮没有在真 tmux 上打过**：本模块的判据只造得出显式 `-S` 那条路，\
         而默认 socket 在沙箱里没法造出「无 server」态（那台沙箱自己在用 tmux）。\
         ⇒ 它今天只由 `every_registered_needle_lands_in_its_own_bucket` \
         用合成样本证明「落对了档」，**不是**「见过它」。",
    ),
];

/// stderr 里「server 在，而这个目标不存在」的形状。
///
/// `(针, 为什么它算这一档)`。
pub(crate) const NO_TARGET_NEEDLES: &[(&str, &str)] = &[
    (
        "can't find pane",
        "tmux 对 target-pane 解析失败时的原话。`=名:` 收的正是 target-pane（`F01`）",
    ),
    (
        "can't find session",
        "同上，只是解析走到了 target-session 那一支（旧版 tmux / 别的目标形）",
    ),
    (
        "no such session",
        "另一种措辞，一并收 —— 多认一种不会把别的失败误判成它（默认档是 `capture_failed`）",
    ),
];

/// 一次 `capture-pane` 的**原始读数**：退出码 ＋ 两条流。
///
/// 判法与取数分开，是为了让三态**各自测得到**：`no_server` / `no_such_session` /
/// 成功那三档由真 tmux 在隔离 socket 上打出来，而判法本身是纯函数、可以单独喂样本。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RawCapture {
    /// 退出码。`None` = 被信号打断（`ExitStatus::code()` 在 unix 上那一档）。
    pub(crate) code: Option<i32>,
    pub(crate) stdout: String,
    pub(crate) stderr: String,
}

/// 「tmux 这个程序起不来」那一档的**唯一造句处**。
///
/// 抽成函数不是为了省字：`KR86D2` 要求这一档与「会话不存在」**分得开**，
/// 而真把 tmux 从 PATH 上摘掉不在沙箱门禁的射程内（那台沙箱自己要用 tmux）
/// ⇒ 判据只能打这一处。⚠ **如实登记**：本条钉的是「这一档存在、且与另外两档
/// 不同码不同话」，**不是**「在一台没装 tmux 的机器上实测过」。
pub(crate) fn tmux_unavailable(e: &std::io::Error) -> CmdErr {
    (
        "no_tmux",
        copy_text("beCapturePane.run.noTmux", &[("e", &e.to_string())]),
    )
}

/// 把一次原始读数判成结局。**纯函数。**
///
/// 判「抓没抓到」看**退出码**，不看输出是不是空 —— 空屏是合法的成功（模块头注那一段）。
pub(crate) fn classify(raw: &RawCapture) -> Result<String, CmdErr> {
    if raw.code == Some(0) {
        return Ok(raw.stdout.clone());
    }
    let said = raw.stderr.to_ascii_lowercase();
    let hit = |table: &[(&str, &str)]| table.iter().any(|(n, _)| said.contains(n));
    let tail = raw.stderr.trim();
    if hit(NO_SERVER_NEEDLES) {
        return Err((
            "no_server",
            copy_text(
                "beCapturePane.classify.noServer",
                &[("tail", &tail.to_string())],
            ),
        ));
    }
    if hit(NO_TARGET_NEEDLES) {
        return Err((
            "no_such_session",
            copy_text(
                "beCapturePane.classify.noTarget",
                &[("tail", &tail.to_string())],
            ),
        ));
    }
    Err((
        "capture_failed",
        copy_text(
            "beCapturePane.classify.failed",
            &[
                ("status", &format!("{:?}", raw.code)),
                ("tail", &tail.to_string()),
            ],
        ),
    ))
}

/// 本模块唯一起进程的那一处 —— **全 crate 唯一一处 `capture-pane`**：跑一条只读的抓屏命令，读回退出码与两条流。
///
/// `socket`：`None` = 让 tmux 自己按环境解析默认 socket（**生产恒 `None`**）；`Some(p)` = 显式 `-S <p>`，
/// 只为让「真能拿回内容」在一个隔离的 tmux server 上测得出来。
fn spawn_tmux(socket: Option<&str>, argv: &[&str]) -> Result<RawCapture, CmdErr> {
    let mut cmd = Command::new("tmux").without_own_env();
    if let Some(s) = socket {
        cmd.args(["-S", s]);
    }
    let out = cmd
        .args(argv)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .map_err(|e| tmux_unavailable(&e))?;
    Ok(RawCapture {
        code: out.status.code(),
        stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
    })
}

#[cfg(test)]
#[path = "../../../tests/backend/control/capture_pane_tests.rs"]
mod tests;
