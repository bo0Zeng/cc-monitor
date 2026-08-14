//! `U-NP④`：**会话身份打标（`@ccm_sid`）** —— 从 `shared/ccm` 的每秒轮询搬到 daemon。
//!
//! # 它接的是谁的班
//!
//! `shared/ccm` 里原来有一条**与会话同寿、每秒一轮**的后台循环：读
//! `<claude_dir>/sessions/<自己的 PID>.json` 拿 `sessionId`，写进 tmux 会话级 option
//! `@ccm_sid`。那是全仓唯一一条「每会话一条、跑在远端机器上」的轮询
//!（开五个会话 = 每秒五条循环）。用户 08-14 裁定：「**可以动 ccm. 不要轮询**」＋
//!「**ccm 做到必须走 daemon**」⇒ 整条 poller 删掉、**不留轮询退路**，改由本模块打标。
//!
//! # 为什么 daemon 干这件事**不需要**任何新的节拍
//!
//! daemon 已经在 inotify `<claude_dir>/sessions/`（`observe/watcher.rs`）——
//! 那正是 pidfile 目录。它看到 `<PID>.json` 的那一刻，**pid 与 sid 同时在手**：
//! 文件名是 pid、内容里是 sid。`/clear`、`/branch` 会**原地重写同一个 pidfile**
//!（wire 上就是 `session_removed.cause="superseded"`），inotify 的 modify 照样送到
//! ⇒ 换 sid 这件事 daemon 看得见，本模块跟着重打，与旧 poller 的「sid 变了立即刷」等效。
//!
//! # (pid, sid) → 「打到哪个 tmux 会话上」怎么解
//!
//! `@ccm_sid` 是 **tmux 会话级** option，所以必须先知道那个 claude 进程住在哪个会话里。
//!
//! **走 `/proc/<pid>/environ` 的 `TMUX_PANE`**，理由是它**没有陈旧的可能**：
//! 那个值属于**这个进程自己**（tmux 起 pane 时注进环境、`exec` 原样继承），
//! 与旧 poller「自己读自己的 pidfile、写自己的会话」是同一条自指性质。
//!
//! 实测（2026-08-14，私有 socket `-S`）：
//! - `exec` 掉的进程（pane 的直接子进程）：`TMUX_PANE=%0` ✓
//! - **孙子进程**（用户已在 pane 里，shell fork 出 ccm 再 exec ⇒ `pid != pane_pid`）：
//!   `TMUX_PANE=%1` 照样在 ✓ —— 所以**不能**改用 `#{pane_pid}` 去 join，那条只覆盖前一种。
//!
//! **排除掉的另一条**：让 ccm 自己写一个 `@ccm_pid=$$` 当 join 键，daemon 用
//! `list-sessions -F` 反查。它跨平台（不依赖 `/proc`），但**有陈旧面**：ccm 退出后那个值
//! 留在会话里，PID 复用时会把新会话的 sid 打到旧会话上 —— 而 `@ccm_sid` 是破坏性动作
//!（kill）唯一认的事实 ⇒ 打错 = 杀错。`/proc` 那条没有这个面，故取它。
//! 平台代价为零：本模块的唯一调用点在 `process_session_added`，而那条路上游就有
//! `pid_alive`（非 Linux 分支是 `unimplemented!()`）⇒ **今天本来就只在 Linux 上跑**。
//!
//! # 两通道设计**没有被合并掉**
//!
//! `shared/ccm` 的通道 A 写 `@ccm_sid_expect`（**意图**：「打算跑这个 sid」），
//! 通道 B 写 `@ccm_sid`（**事实**）。破坏性动作只认后者。搬家之后这条分离**更硬**了：
//! 事实的写者从「那个会话自己」变成了**独立的第三方**，而且 daemon 是在
//! `process_session_added` 走完 `pid_alive` + `add_time_verdict`（procStart 冒名检查）
//! 之后才调本模块 —— 也就是说打标前已经证过「这个 pid 真的是写那份 pidfile 的那个 claude」。
//! 意图仍然只由 ccm 声明，**本模块一个字都不写 `@ccm_sid_expect`**。
//!
//! # 谁兜「标题被冲掉」
//!
//! 不需要人兜。`shared/ccm` 把 `set-titles-string` 设成 `#{?@ccm_sid,ccm-rbind-#{@ccm_sid},#T}`
//!（2026-07-31 真机踩坑后改的），窗口标题由 **tmux 自己从 `@ccm_sid` 合成**，
//! 与 claude 抢着写的 pane 标题彻底分开 ⇒ marker 常驻。
//! 实测（08-14，私有 socket + `script` 起真 pty client）：**外部** client 执行一次
//! `set-option @ccm_sid <sid>` 之后，tmux 主动往 client 推
//! `ESC ]0;ccm-rbind-<sid> BEL` —— 也就是说打标者是不是那个会话自己**无关紧要**。
//! 旧 poller 里那句「每 20 秒重打一次自愈」在这条 format 落地之后已是余物，随 poller 一起删。
//!
//! # 起进程
//!
//! 探测复用 [`super::gate::probe`]（**零新增起进程点**），只有真要写时才多起一个
//! `tmux set-option`。已登记进 `readonly_guard::spawn_registry`。
//! 改的是 **tmux server 的运行期状态**，不是 daemon 自己写用户既有数据（同 `tmux_hook`）。

/// 一次打标的结局。**返回而不是吞掉** —— 调用方今天丢弃它，但日志与测试要看得见。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Outcome {
    /// 打上了（或从「没有」变成了新值）。带上落地的 `#{session_id}` 句柄。
    Tagged(String),
    /// 已经是这个值了 —— **不重复写**。启动重扫时每个会话都会走一遍这里，
    /// 免掉「已经对了还再起一次 `tmux`」。
    AlreadyCurrent,
    /// 这个进程不在 tmux 里（`TMUX_PANE` 没有 / 形状不对）⇒ 没有会话可打。
    NotInTmux,
    /// pane 还在环境里，但 tmux 说它不存在（会话已关 / server 换了）。
    NoSuchPane,
    /// sid 形状不对 —— **fail closed**，不往 tmux 里塞一个没核过的字符串。
    RejectedSid,
    /// 起不来 tmux / tmux 报错。
    Failed(String),
}

/// sid 的字符集。**与 `control::launch::parse_request` 同一条**：它会被拼进 tmux 的
/// 格式串（`ccm-rbind-#{@ccm_sid}`），收紧到确定安全的字符集。
///
/// 这里的 sid 来自 pidfile（claude 自己写的），不是用户输入 —— 但「来源可信」不是放行的
/// 理由：本仓栽过的那些坑里，最贵的一类就是「这个值不可能有问题」。
fn sid_is_safe(sid: &str) -> bool {
    !sid.is_empty()
        && sid.len() <= 128
        && sid
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

/// tmux 的 pane id 形状：`%` + 十进制数字。
///
/// ★ **空串必须挡住**，这条是实测撞出来的（08-14，私有 socket）：
/// `tmux display-message -p -t '' '#{session_name}'` **不报错**，它静默解析成
/// 「当前/最近的那个会话」⇒ 一个空的 `TMUX_PANE` 会让我们把 sid 打到**一个碰巧的会话**上。
/// 而 `@ccm_sid` 是破坏性动作唯一认的事实 —— 打错就是杀错。
fn pane_is_safe(pane: &str) -> bool {
    match pane.strip_prefix('%') {
        Some(n) => !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()),
        None => false,
    }
}

/// 取这个进程所在的 tmux pane（`TMUX_PANE`），核过形状才回。
fn pane_of(pid: u32) -> Option<String> {
    let raw = crate::platform::proc::proc_env_var(pid, "TMUX_PANE")?;
    let pane = raw.trim().to_string();
    pane_is_safe(&pane).then_some(pane)
}

/// **把 `sid` 打到 `pid` 所在的那个 tmux 会话上。**
///
/// 调用点只有一处：`observe/watcher.rs::process_session_added`，在「这个 pid 确实是写
/// 那份 pidfile 的那个 claude」被证过之后。跨层边已登记进 `layering_guard`。
pub(crate) fn tag(pid: u32, sid: &str) -> Outcome {
    if !sid_is_safe(sid) {
        return Outcome::RejectedSid;
    }
    let Some(pane) = pane_of(pid) else {
        return Outcome::NotInTmux;
    };
    // 探测复用 gate 那一处（**零新增起进程点**）。它顺带把当前 `@ccm_sid` 取回来 ⇒
    // 值没变就一次 `set-option` 都不用起。
    let probed = match super::gate::probe(&pane) {
        Ok(Some(p)) => p,
        Ok(None) => return Outcome::NoSuchPane,
        Err((code, msg)) => return Outcome::Failed(format!("{code}: {msg}")),
    };
    if probed.ccm_sid == sid {
        return Outcome::AlreadyCurrent;
    }
    // ★ 对 `#{session_id}` **句柄**下手，不对名字 —— 与 `gate` / `kill` 同一条纪律：
    // 名字在探测与动手之间可能被重新绑定到别的会话，句柄不会（server 生命周期内唯一、不复用）。
    let target = probed.session_id.clone();
    match std::process::Command::new("tmux")
        .args(["set-option", "-t", &target, "@ccm_sid", sid])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
    {
        Ok(st) if st.success() => Outcome::Tagged(target),
        Ok(st) => Outcome::Failed(format!("tmux set-option 退出码 {st}")),
        Err(e) => Outcome::Failed(format!("起不来 tmux：{e}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_real_session_id_is_accepted() {
        assert!(sid_is_safe("9d66c46d-bf88-4f99-877e-455555555555"));
        assert!(sid_is_safe("a_b-1"));
    }

    /// fail closed：能破坏 tmux 格式串 / 命令语义的都不许过。
    ///
    /// ⚠ **`-t` 这种「像旗标」的值刻意不在这张表里** —— 它由 `[A-Za-z0-9_-]` 放行，
    /// 而那是对的：`set-option -t <handle> @ccm_sid <值>` 里的值在非选项参数之后，
    /// tmux 不会把它再当选项解析；且我们 argv 直传、不过 shell。
    /// 写在这里是因为「看起来危险就该禁」是个很容易顺手加进来的错判 —— 真 sid（UUID）
    /// 本来就带 `-`，收窄到禁 `-` 会把正常会话全挡掉。
    #[test]
    fn a_sid_that_could_break_the_format_string_is_rejected() {
        for bad in ["", "a b", "#{session_name}", "a;b", "$(id)", "a\nb", "a'b"] {
            assert!(!sid_is_safe(bad), "{bad:?} 不该被放行");
        }
        assert!(!sid_is_safe(&"x".repeat(129)), "超长 sid 不该被放行");
    }

    /// ★ 空 pane 目标必须挡住 —— 实测 `-t ''` 会静默解析成「某个会话」。
    #[test]
    fn an_empty_pane_target_is_rejected() {
        assert!(
            !pane_is_safe(""),
            "空目标放行 ⇒ sid 会被打到一个碰巧的会话上"
        );
        assert!(!pane_is_safe("%"), "只有 % 没有数字也不是 pane id");
    }

    #[test]
    fn only_percent_digits_is_a_pane_id() {
        assert!(pane_is_safe("%0"));
        assert!(pane_is_safe("%12"));
        for bad in ["0", "$0", "@0", "%a", "%1x", " %1", "%1 "] {
            assert!(!pane_is_safe(bad), "{bad:?} 不该被当成 pane id");
        }
    }

    /// 一个不存在的 pid 拿不到 `TMUX_PANE` ⇒ 走「不在 tmux 里」，**不会**去猜一个会话。
    ///
    /// ⚠ 本条**不起 tmux**：`pane_of` 在 `gate::probe` 之前返回 `None`。
    #[test]
    fn a_pid_without_tmux_pane_never_reaches_tmux() {
        // PID 0 在 Linux 上不是一个可读的 `/proc` 目录 ⇒ 读不到环境。
        assert_eq!(
            tag(0, "9d66c46d-bf88-4f99-877e-455555555555"),
            Outcome::NotInTmux
        );
    }

    /// sid 不合法时**连环境都不读**（顺序也是判据的一部分：先 fail closed 再做 IO）。
    #[test]
    fn a_bad_sid_short_circuits_before_any_io() {
        assert_eq!(tag(std::process::id(), "bad sid"), Outcome::RejectedSid);
    }
}
