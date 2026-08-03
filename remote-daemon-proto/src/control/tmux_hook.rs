//! P4b（zero-poll-liveness）：**tmux hook → daemon** 的通知通路。
//!
//! # 它解决什么
//!
//! 「多个 tmux 会话里杀掉其中一个」是**唯一**没有内核事件源的场景：pidfd 只看 server 进程
//! （server 还活着）、socket inotify 只看 server 生死。它此前只能靠 8s 轮询兜住（P4 之前
//! 实测 ~16s）。tmux 自己知道这件事 —— `session-closed` hook 就是那个信号。
//!
//! # 通路（**零文件系统写**）
//!
//! ```text
//! tmux hook (全局 [50], run-shell -b)
//!    └─> <daemon exe> --tmux-notify <daemon_pid> <daemon_starttime>
//!           ├─ 读 /proc/<pid>/stat 校验 starttime 相符（挡 PID 复用误伤无关进程）
//!           └─ kill(pid, SIGUSR1)
//!    daemon: SIGUSR1 流（main.rs，P4a 已就位）
//!           └─> WatcherPoke::poke() ⇒ 往统一 channel 发一拍 ⇒ 立刻重探
//! ```
//!
//! # 为什么不传会话名（这不是偷懒，是设计）
//!
//! 原方案让 hook 把 `#{hook_session_name}` 写进一个事件日志文件。两个问题：
//! ① **撞红线 I7「daemon 只读」**——`readonly_guard` 当场拦下了那个建目录调用。
//!    （**这里刻意不逐字写出那个函数名**：`readonly_guard` 连注释一起扫，
//!    是 fail-closed 的设计；在 daemon 源码的散文里引用它的禁用模式会让全局守卫红。
//!    本轮实测栽过一次 —— 处置是改措辞，**不是**去把那道红线守卫改成剥注释。）
//! ② 会话名要经 shell 引号 —— 名字里有 `"` 或 `$(...)` 就能破坏命令串甚至注入，
//!    原方案只能「接受并登记」这个面。
//!
//! 现在**名字根本不传**：信号无载荷 ⇒ 注入面消失、日志不存在、daemon 写归零。
//! 代价是信号会合并（多个会话同时关可能只来一次）—— 靠**重探 + 与上一份快照差分**
//! 天然免疫，差分一次能报出所有消失的会话，比逐条事件更稳。
//!
//! # `#{@ccm_sid}` 是个陷阱（P0 实测）
//!
//! hook 里用 `#{@ccm_sid}` 取值会拿到**空**（那是会话级 option，hook 执行上下文里未必绑到
//! 目标会话）⇒ 下游把活会话当成灰的。P0 的结论是：**hook 只用 `#{hook_session_name}`**，
//! 名字→sid 的映射由消费侧查表。本模块**连名字都不传**，所以这条陷阱在这里已不适用，
//! 但注释留着 —— 将来若有人想「顺便把名字带上」，得先回头看这一条。

use std::path::Path;

/// hook 槽位。调研实测全局 `[50]` 空着；用固定槽位是为了**可撤销**
/// （`tmux set-hook -gu 'session-closed[50]'`），而不是追加到一串未知 hook 后面。
pub(crate) const HOOK_SLOT: u32 = 50;

/// 装哪几个 hook。`session-renamed` 也要 —— 名字是 monitor 侧查表的键，
/// 改名不通知的话下一次差分会把它误判成「一个消失了、一个新出现」。
pub(crate) const HOOK_EVENTS: [&str; 3] = ["session-created", "session-closed", "session-renamed"];

/// POSIX 单引号包裹。**只用于我们自己产生的路径/数字**（exe 路径、pid、starttime），
/// 不用于任何来自 tmux 或用户的字符串 —— 那条路本设计里根本不存在（见模块头注）。
fn sq(s: &str) -> String {
    // U8c-2b-0（账本 S5）：实现收进 `launch-core` —— 此前全仓有**四份逐字节相同**的
    // POSIX 单引号 quote。保留本地名字，调用点零改。
    launch_core::posix_quote(s)
}

/// 一条 hook 的 tmux 命令参数（不含 `tmux` 本身），供 `Command::args` 直接用。
///
/// 形如：`set-hook -g session-closed[50] run-shell -b '<exe> --tmux-notify <pid> <start>'`
///
/// **`run-shell -b`**：`-b` 是后台执行 —— 不加的话 tmux 会**同步等**这条命令跑完，
/// 把会话关闭这条路径卡在我们的进程启动上。
pub(crate) fn hook_set_args(event: &str, exe: &Path, pid: u32, starttime: u64) -> Vec<String> {
    let payload = format!(
        "{} --tmux-notify {pid} {starttime}",
        sq(&exe.to_string_lossy())
    );
    vec![
        "set-hook".into(),
        "-g".into(),
        format!("{event}[{HOOK_SLOT}]"),
        format!("run-shell -b {}", sq(&payload)),
    ]
}

/// 撤销用的参数（人要手动清时照着敲）。
///
/// **生产路径刻意不调它**：daemon 停机时**不需要**摘 hook —— 留着的 hook 指向一个已死的
/// pid，`notify` 那边 starttime 校验不过就静默 no-op；而 server 每次重启 hook 本就没了。
/// 主动摘反而会在「同机跑两个 daemon」时把对方的 hook 也摘掉。
/// 它的价值是**可撤销这件事本身有据可查**（授权时承诺过），由测试钉住形状。
#[allow(dead_code)]
pub(crate) fn hook_unset_args(event: &str) -> Vec<String> {
    vec![
        "set-hook".into(),
        "-gu".into(),
        format!("{event}[{HOOK_SLOT}]"),
    ]
}

/// **在活着的 tmux server 上装/重装三个 hook。**
///
/// 每次感知到 server 存在（含**复活**）都要调 —— hook 活在 server 内存里，
/// server 一重启就全没了。P3 把「server 起来了」变成了事件 ⇒ 这里有现成的时机。
///
/// **socket 定位**：不传 `-L`/`-S`，靠继承的 `TMUX_TMPDIR` / 默认 socket ——
/// daemon 与它观测的那台 server 本来就在同一套 socket 语义下（`tmux ls` 探测也是这么跑的）。
///
/// **失败只 warn 不致命**：装不上 hook = 退回定时探测（P5 之前 ticker 还在），
/// 不是致命错。**但要说出来**，否则「hook 通路没生效」会变成静默降级。
///
/// 返回装成功的条数（供日志与测试用）。
pub(crate) fn install_hooks(exe: &Path, pid: u32, starttime: u64) -> usize {
    let mut ok = 0usize;
    for event in HOOK_EVENTS {
        let args = hook_set_args(event, exe, pid, starttime);
        match std::process::Command::new("tmux")
            .args(&args)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
        {
            Ok(st) if st.success() => ok += 1,
            Ok(st) => tracing::warn!("装 tmux hook {event}[{HOOK_SLOT}] 失败（退出码 {st}）"),
            Err(e) => tracing::warn!("装 tmux hook {event}[{HOOK_SLOT}] 起不来 tmux：{e}"),
        }
    }
    ok
}

/// `--tmux-notify <pid> <starttime>`：hook 子进程走这条。
///
/// **fail-closed 的是「误伤」而不是「漏报」**：校验不过就**什么都不做**并退 0。
/// 退 0 是有意的 —— 这是 tmux 的 `run-shell -b` 子进程，非零退出只会在 tmux 里堆错误，
/// 而「daemon 已经不在了」是完全正常的情况（用户关掉 monitor 之后 hook 还留着）。
pub fn notify(args: &[String]) -> i32 {
    let (Some(pid_s), Some(start_s)) = (args.get(1), args.get(2)) else {
        eprintln!("用法: --tmux-notify <daemon_pid> <daemon_starttime>");
        return 2;
    };
    let (Ok(pid), Ok(want_start)) = (pid_s.parse::<u32>(), start_s.parse::<u64>()) else {
        eprintln!("--tmux-notify: pid / starttime 必须是整数");
        return 2;
    };

    // ★ PID 复用防御：光看 /proc/<pid> 存在是不够的 —— daemon 退出后那个 pid 可能已经
    // 被**别的进程**占用，给它发 SIGUSR1 轻则无效、重则打断一个无关进程（很多程序把
    // SIGUSR1 当自定义控制信号，默认处置更是直接终止）。必须比对 starttime。
    match crate::platform::proc::proc_starttime(pid) {
        Some(actual) if actual == want_start => {}
        _ => return 0, // 不是那个 daemon（或它已经不在）⇒ 静默不做事
    }

    // U3：发信号那一步下沉到 `platform::signal`（§1.1-1：平台原语只许在 platform/）。
    // 措辞刻意不写出那个 libc 函数名 —— 「本层还有没有平台原语」是靠 grep 查的，
    // 注释里留一个会让下一个人白查一趟（同 §41.4 第 1 条纪律的形状）。
    // **身份校验留在这里**——那是域判断（「这个 pid 是不是我那个 daemon」），不是平台能力。
    // 发失败仍不是错误：竞态（校验之后、发信号之前 daemon 退出了）。
    let _ = crate::platform::signal::send_sigusr1(pid);
    0
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    /// 判据一律 `format!` 运行时拼、绝不把它当字面量写进源码 —— 否则扫源码的断言会被
    /// **自己那行字面量**命中（P4a 里同一个自指陷阱连踩五次，其中一次让守卫成了安慰剂）。
    fn prod_src() -> &'static str {
        let me = include_str!("tmux_hook.rs");
        let marker = "\n#[cfg(test)]\nmod tests";
        match me.find(marker) {
            Some(i) => &me[..i],
            None => me,
        }
    }

    /// 生产段**再剥掉注释**。本模块的头注**逐字解释**了那两个坑（`#{@ccm_sid}` 陷阱、
    /// 被 `readonly_guard` 拦下的 `fs::create_dir`）—— 不剥的话，两条守卫会被
    /// **解释它们自己的那段散文**命中而恒红（实测：两条一起红）。
    /// 与 `src/paste-block-guard.vitest.ts` 同一处置（那边也是「把注释当代码」栽过）。
    fn prod_code() -> String {
        prod_src()
            .lines()
            .filter(|l| {
                let t = l.trim_start();
                !t.starts_with("//")
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn hook_args_shape() {
        let a = hook_set_args("session-closed", &PathBuf::from("/opt/ccm/daemon"), 42, 999);
        assert_eq!(a[0], "set-hook");
        assert_eq!(a[1], "-g");
        assert_eq!(a[2], "session-closed[50]");
        assert_eq!(
            a[3],
            "run-shell -b ''\\''/opt/ccm/daemon'\\'' --tmux-notify 42 999'"
        );
    }

    /// exe 路径含单引号也不能破坏命令串（我们自己产生的路径，但用户可以把 daemon
    /// 部署到任意目录）。
    #[test]
    fn exe_path_with_quote_is_escaped() {
        let a = hook_set_args("session-closed", &PathBuf::from("/o'p/d"), 1, 2);
        assert!(a[3].contains(r"'\''"), "单引号必须转义：{}", a[3]);
    }

    #[test]
    fn unset_args_shape() {
        assert_eq!(
            hook_unset_args("session-renamed"),
            vec!["set-hook", "-gu", "session-renamed[50]"]
        );
    }

    /// ★ 三个事件缺一不可 —— `session-renamed` 最容易被当成可选：名字是 monitor 侧查表的
    /// 键，改名不通知会让下一次差分把它误判成「一个消失 + 一个新出现」。
    #[test]
    fn all_three_events_are_hooked() {
        for e in ["session-created", "session-closed", "session-renamed"] {
            assert!(HOOK_EVENTS.contains(&e), "缺 hook 事件 {e}");
        }
        assert_eq!(HOOK_EVENTS.len(), 3);
    }

    /// ★ P0 那条陷阱：hook 里绝不许出现 `#{@ccm_sid}`（会拿到空 ⇒ 活会话被判灰）。
    /// 本设计连会话名都不传，这条是**防将来**有人「顺便把名字带上」。
    #[test]
    fn never_uses_ccm_sid_format_in_hooks() {
        let forbidden = format!("#{}@ccm_sid{}", "{", "}");
        let code = prod_code();
        assert!(
            !code.contains(&forbidden),
            "hook 里不许用 {forbidden}（P0 实测：它在 hook 上下文取到空，会把活会话判灰）"
        );
        // 反向自检：真剥出了代码（不是把整份都过滤没了 ⇒ 断言空转）。
        assert!(
            code.contains("fn hook_set_args"),
            "剥注释后代码为空，断言在空转"
        );
    }

    /// ★ 本模块**零文件系统写**（红线 I7；P4 原设计就是栽在这）。
    /// `readonly_guard` 已经全局扫一遍，这里再钉一次是因为**本文件是最可能复发的地方**。
    #[test]
    fn no_filesystem_writes_in_this_module() {
        let src = prod_code();
        for pat in [
            format!("fs::{}", "write"),
            format!("fs::{}", "create_dir"),
            format!("File::{}", "create"),
            format!("{}::new", "OpenOptions"),
        ] {
            assert!(!src.contains(&pat), "本模块不许有文件系统写：{pat}");
        }
        assert!(src.contains("fn notify"), "剥注释后代码为空，断言在空转");
    }

    /// starttime 对不上 ⇒ 什么都不做且退 0（不误伤被复用了 pid 的无关进程）。
    #[test]
    fn notify_rejects_mismatched_starttime() {
        let me = std::process::id();
        let args = vec![
            "--tmux-notify".to_string(),
            me.to_string(),
            "999999999".to_string(), // 几乎不可能等于真实 starttime
        ];
        assert_eq!(notify(&args), 0);
    }

    #[test]
    fn notify_rejects_bad_args() {
        assert_eq!(notify(&["--tmux-notify".into()]), 2);
        assert_eq!(notify(&["--tmux-notify".into(), "x".into(), "1".into()]), 2);
    }
}
