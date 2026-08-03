//! **U10 摸底裁决的机检**：`send-keys` / `kill` 改走 daemon 之前，daemon 侧必须先有身份守卫。
//!
//! # 摸底发现的那道会被悄悄丢掉的门
//!
//! monitor 的 `tmux_send_keys` / `kill_remote_tmux` 带着 §34 的 **Gate 2 union**：
//! 目标要么本地 `is_ccm_tmux_name` 命中（`cc-*` 前缀），要么远端 `@ccm_sid` 已设 ——
//! 后者由**嵌进远端命令的守卫**核验，不通过就回 `CCM_GUARD_REJECTED`。
//! 那道门挡的是「往一个不是本工具管理的 tmux 会话里打字 / 把它杀掉」。
//!
//! daemon 的 `control/launch.rs` 今天**只核会话存在性**（`no_such_session`）——
//! 它**写** `@ccm_sid`（建会话时 `set-option`），但从不**核验**它。
//! ⇒ 把 `send-keys`/`kill` 改走 daemon，等于**丢掉 §34 的 Gate 2**。
//!
//! 这不是「还没做完」，是一条**会静默降低安全性的路**：功能看起来一样、门禁全绿、
//! 而一道门没了。U8a-2c 之所以敢切 `send-into`，正因为**那条路本来就没有 Gate 2**
//! （`session-backend.ts` 产的裸 `send-keys` 不带身份守卫，它的安全性来自会话名是自己刚挑的）。
//!
//! # 本护栏的形状：**前提触发器**，不是永久禁令
//!
//! - daemon 侧**还没有**身份守卫 ⇒ 钉住 monitor 那两个命令**不许**改走 daemon 通道；
//! - daemon 侧**一出现**身份守卫 ⇒ 本护栏**主动红**，逼人回来重新裁定（那时禁令就该撤了）。
//!
//! 同 `ccm_invocation::env_reset_can_never_be_reached_in_the_cli_renderer` 的思路：
//! 钉的是**不可达/不该做的前提本身**，前提一变就叫人回来。
//!
//! ⚠ **约定型守卫**（同 `readonly_guard` 一族）：查的是符号名的源码形态，
//! 挡得住「顺手把这两条改走 daemon」，挡不住「换个名字继续错」。**比没有强，别读成证明。**

#[cfg(test)]
mod tests {
    /// daemon 侧「有身份守卫」的标志。今天**一个都不该出现**。
    ///
    /// 选这几个是因为它们是 monitor 侧那道门的**产物名**（拒绝码/拒绝文案）——
    /// daemon 真要复现 Gate 2，最自然的形态就是回一个同族的拒绝码。
    const DAEMON_GATE_MARKERS: &[&str] = &["CCM_GUARD_REJECTED", "wrong_owner", "not_owned"];

    /// monitor 侧**不许**在这两个命令里出现的东西（那是 daemon 通道）。
    const DAEMON_CHANNEL_MARKERS: &[&str] = &["inbound_client", "daemon_send_into"];

    /// 要看住的两个命令。
    const GUARDED_COMMANDS: &[&str] = &[
        "pub async fn tmux_send_keys(",
        "pub async fn kill_remote_tmux(",
    ];

    const MONITOR_TMUX: &str = include_str!("tmux.rs");
    const DAEMON_LAUNCH: &str = include_str!("../../remote-daemon-proto/src/control/launch.rs");
    const DAEMON_HOOK: &str = include_str!("../../remote-daemon-proto/src/control/tmux_hook.rs");

    /// 从函数签名处截到**列 0 的收尾 `}`** —— 顶层函数就是这个形状。
    fn body_of(src: &str, sig: &str) -> String {
        let at = src
            .find(sig)
            .unwrap_or_else(|| panic!("找不到 `{sig}` —— 签名变了就把本护栏一起改"));
        let rest = &src[at..];
        let end = rest.find("\n}\n").map(|k| k + 3).unwrap_or(rest.len());
        rest[..end].to_string()
    }

    /// ★ 抽取器自检：抽不到函数体时下面那条会零命中变绿。
    #[test]
    fn the_two_command_bodies_are_actually_extracted() {
        for sig in GUARDED_COMMANDS {
            let body = body_of(MONITOR_TMUX, sig);
            assert!(
                body.len() > 400,
                "`{sig}` 只抽到 {} 字节 —— 抽取坏了",
                body.len()
            );
            assert!(
                body.contains("connect_and_exec_cmd"),
                "`{sig}` 的函数体里没有 `connect_and_exec_cmd` —— 抽错了段，或者它已经改走别的路了"
            );
        }
    }

    /// ★ 前提触发器：daemon 侧**一出现**身份守卫，本条就红，逼人回来重新裁定。
    #[test]
    fn the_daemon_still_has_no_identity_gate_so_the_ban_below_still_applies() {
        let prod: String = [DAEMON_LAUNCH, DAEMON_HOOK]
            .iter()
            .map(|s| guard_core::production_code(s))
            .collect::<Vec<_>>()
            .join("\n");
        assert!(
            prod.len() > 3000,
            "daemon control 生产段只剩 {} 字节 —— 剥法或路径坏了",
            prod.len()
        );
        let found: Vec<&str> = DAEMON_GATE_MARKERS
            .iter()
            .copied()
            .filter(|m| prod.contains(m))
            .collect();
        assert!(
            found.is_empty(),
            "daemon 的 control 面出现了身份守卫的标志 {found:?} —— **这是好事**，\n\
             但它意味着本护栏下面那条禁令（`send-keys`/`kill` 不许改走 daemon）的前提变了：\n\
             请回到 U10 重新裁定「§34 的三道门在 daemon 侧怎么复现」，然后把本护栏删掉或改写。"
        );
    }

    /// ★ 正题：daemon 侧没有 Gate 2 之前，这两个命令**不许**改走 daemon 通道。
    ///
    /// 变异「把 `tmux_send_keys` 改成调 `daemon_send_into`」= 功能看起来一样、
    /// 而 §34 的 Gate 2 静默消失。
    #[test]
    fn send_keys_and_kill_do_not_route_through_the_daemon_yet() {
        let mut offenders = Vec::new();
        for sig in GUARDED_COMMANDS {
            let body = guard_core::production_code(&body_of(MONITOR_TMUX, sig));
            for m in DAEMON_CHANNEL_MARKERS {
                if body.contains(m) {
                    offenders.push(format!("  {sig} 里出现了 `{m}`"));
                }
            }
        }
        assert!(
            offenders.is_empty(),
            "`send-keys`/`kill` 被改走了 daemon 通道，而 daemon 侧还没有身份守卫 ——\n\
             §34 的 Gate 2（`cc-*` 前缀 **或** 远端 `@ccm_sid` 已设，不通过回 `CCM_GUARD_REJECTED`）\n\
             会就此静默消失：功能看起来一样、门禁全绿，而「不许往别人的 tmux 里打字/杀它」那道门没了。\n\
             要切先把 Gate 2 搬进 daemon `control/`（那时上面那条前提触发器会先红，提醒你重新裁定）。\n{}",
            offenders.join("\n")
        );
    }
}
