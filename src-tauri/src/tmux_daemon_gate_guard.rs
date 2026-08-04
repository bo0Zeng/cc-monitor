//! **§34 Gate 2 的两侧账**：daemon 侧的身份门必须在；monitor 侧的两个命令还不许切过去。
//!
//! # 病史：U10 立的那条前提触发器，F03 让它红了 —— 可它没红
//!
//! U10 摸底发现：monitor 的 `tmux_send_keys` / `kill_remote_tmux` 带着 §34 的 **Gate 2 union**
//! （名字命中 `cc-*`/`<X>-cc` **或**远端 `@ccm_sid` 已设，不通过回 `CCM_GUARD_REJECTED`），
//! 而 daemon 的 `control/launch.rs` **只核会话存在性**。把那两条改走 daemon
//! ＝ **静默丢掉一道门**：功能看起来一样、门禁全绿，而门没了。
//!
//! 于是立了一条**前提触发器**：「daemon 一出现身份守卫 ⇒ 本护栏主动红，逼人回来重新裁定」。
//!
//! ⚠ **F03 真的给 daemon 装了门（`control/gate.rs`），而本护栏纹丝不动。**
//! 根因：它的扫描面是**一张硬编码的两文件表**（`launch.rs` + `tmux_hook.rs`），
//! 新加的 `gate.rs` **根本不在它眼里**。这是本仓「扫描面画小了」那一族的又一次 ——
//! `readonly_guard::spawn_registry` 的头注里逐字记着同样的事（那是第五次，而且也是
//! 「新增一个文件，硬编码清单扫不到」）。**同一个坑，同一个仓，第二个模块。**
//! ⇒ F03 把扫描面改成**递归遍历 `control/`**，并配抽取器自检钉住文件数地板。
//!
//! # 今天这个模块钉两件事（前提已变，禁令的理由跟着换）
//!
//! 1. **反向锚点**：daemon 侧的身份门**必须还在**。删了它就红 ——
//!    从「不许出现」翻成「必须存在」，是 F03 之后前提变了的直接后果。
//! 2. **禁令仍在，但理由换了**：monitor 那两条**仍不许**改走 daemon，因为
//!    daemon 今天有 Gate 2、**没有 Gate 3**（`windows==1`，只约束破坏性动作），
//!    而且**根本没有 kill 命令**。切路由是 **F04** 的活，连同 Gate 3 与平价对账一起做。
//!    定框 C6 逐字写着：**先搬 Gate 2，再切 kill / send-keys —— 顺序不可反。**
//! 3. **Gate 3 的前提触发器**：daemon 一旦出现 `session_windows`（Gate 3 的形状），
//!    说明 F04 动工了 ⇒ 本模块该整体退役，主动红一次提醒。
//!
//! ⚠ **约定型守卫**（同 `readonly_guard` 一族）：查的是符号名的源码形态，
//! 挡得住「顺手把这两条改走 daemon」，挡不住「换个名字继续错」。**比没有强，别读成证明。**

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;

    /// daemon 侧「有身份守卫」的标志。F03 之后**必须**出现。
    ///
    /// 选这几个是因为它们是 monitor 侧那道门的**产物名**（拒绝码 / 拒绝文案）——
    /// daemon 复现 Gate 2 最自然的形态就是回一个同族的拒绝码，F03 正是这么做的
    /// （`control/gate.rs::admit` 回 `wrong_owner` + `CCM_GUARD_REJECTED …`）。
    const DAEMON_GATE_MARKERS: &[&str] = &["CCM_GUARD_REJECTED", "wrong_owner"];

    /// Gate 3（`windows==1`，只约束破坏性动作）在 daemon 侧的形状。**今天一个都不该有。**
    const DAEMON_GATE3_MARKERS: &[&str] = &["session_windows", "kill-session"];

    /// monitor 侧**不许**在这两个命令里出现的东西（那是 daemon 通道）。
    const DAEMON_CHANNEL_MARKERS: &[&str] = &["inbound_client", "daemon_send_into"];

    /// 要看住的两个命令。
    const GUARDED_COMMANDS: &[&str] = &[
        "pub async fn tmux_send_keys(",
        "pub async fn kill_remote_tmux(",
    ];

    const MONITOR_TMUX: &str = include_str!("tmux.rs");

    fn daemon_control_dir() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("src-tauri 的上级")
            .join("remote-daemon-proto/src/control")
    }

    /// daemon `control/` 下**全部** `.rs` 的生产段。
    ///
    /// ⚠ **递归遍历，不是硬编码文件表** —— 本模块头注记着为什么：
    /// 硬编码的两文件表让 F03 新增的 `gate.rs` 整个逃出了扫描面。
    fn daemon_control_production() -> Vec<(String, String)> {
        let dir = daemon_control_dir();
        let mut out = Vec::new();
        let mut stack = vec![dir.clone()];
        while let Some(d) = stack.pop() {
            let Ok(rd) = fs::read_dir(&d) else { continue };
            for e in rd.flatten() {
                let p = e.path();
                if p.is_dir() {
                    stack.push(p);
                    continue;
                }
                if p.extension().and_then(|x| x.to_str()) != Some("rs") {
                    continue;
                }
                let rel = p
                    .strip_prefix(&dir)
                    .unwrap_or(&p)
                    .to_string_lossy()
                    .replace('\\', "/");
                let raw = fs::read_to_string(&p).unwrap_or_default();
                out.push((rel, guard_core::production_code(&raw)));
            }
        }
        out.sort();
        out
    }

    /// 从函数签名处截到**列 0 的收尾 `}`** —— 顶层函数就是这个形状。
    fn body_of(src: &str, sig: &str) -> String {
        let at = src
            .find(sig)
            .unwrap_or_else(|| panic!("找不到 `{sig}` —— 签名变了就把本护栏一起改"));
        let rest = &src[at..];
        let end = rest.find("\n}\n").map(|k| k + 3).unwrap_or(rest.len());
        rest[..end].to_string()
    }

    /// ★ 抽取器自检 A：monitor 那两个函数体真的抽到了。
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

    /// ★ 抽取器自检 B：daemon `control/` 的**递归**扫描面没缩水。
    ///
    /// 这条就是 F03 补上的那一条 —— 上一版没有它，扫描面从 5 个文件缩到 2 个也不会红。
    #[test]
    fn the_daemon_control_scan_surface_is_not_a_hardcoded_short_list() {
        let files = daemon_control_production();
        let names: Vec<&str> = files.iter().map(|(n, _)| n.as_str()).collect();
        assert!(
            files.len() >= 5,
            "daemon control/ 只扫到 {} 个 .rs（{names:?}）—— 递归遍历坏了，\
             本模块下面几条会零命中地绿",
            files.len()
        );
        // 门住在这个文件里；它不在扫描面 = 反向锚点是空的。
        assert!(
            names.contains(&"gate.rs"),
            "扫描面里没有 `gate.rs`（实得 {names:?}）—— 那正是 F03 那次没红的形状"
        );
        let total: usize = files.iter().map(|(_, s)| s.len()).sum();
        assert!(
            total > 20_000,
            "daemon control/ 生产段总共只剩 {total} 字节 —— 剥法或路径坏了"
        );
    }

    /// ★ **反向锚点**（F03 起）：daemon 侧的身份门**必须还在**。
    ///
    /// 前提触发器翻了个面：U10 时钉「不许出现」（daemon 还没有门），
    /// F03 装上之后钉「不许消失」。删掉 Gate 2 而门禁全绿，正是这条要挡的。
    #[test]
    fn the_daemon_identity_gate_is_still_there() {
        let files = daemon_control_production();
        let all: String = files
            .iter()
            .map(|(_, s)| s.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        let missing: Vec<&str> = DAEMON_GATE_MARKERS
            .iter()
            .copied()
            .filter(|m| !all.contains(m))
            .collect();
        assert!(
            missing.is_empty(),
            "daemon 的 control 面**找不到**身份守卫的标志 {missing:?} ——\n\
             §34 的 Gate 2 在 daemon 侧没了（F03 把它装在 `control/gate.rs::admit`）。\n\
             这道门挡的是「往一个不是本工具管理的 tmux 会话里打字」。\n\
             真要撤，先回定框 C6 重新裁定，别让它在一次重构里悄悄蒸发。"
        );
        // 门必须在**生产段**、且在 `gate.rs` 里 —— 只在测试里出现等于没有门。
        let gate_rs = files
            .iter()
            .find(|(n, _)| n == "gate.rs")
            .map(|(_, s)| s.as_str())
            .unwrap_or("");
        assert!(
            gate_rs.contains("gate_core::gate2"),
            "`gate.rs` 的生产段没有调 `gate_core::gate2` —— 判定要么被就地重写了一份\
             （那就与 monitor 会漂），要么这道门只剩个壳"
        );
        // ★ **门必须在路上，不只是在仓里。**
        //
        // ⚠ 这一条是本轮变异复验补的：M2「把 `launch.rs` 的 `gate::admit` 拆掉、退回
        // `has-session` + 裸 `type_payload`」时，上面两条**照样全绿** —— 因为 `gate.rs`
        // 文件还在、标志串还在。抓到它的是 daemon 自己那两条接线测试，而本模块
        // （monitor 侧那条禁令的**前提**）却认为「门还在」，前提就成了假的。
        // 「模块存在 ≠ 模块被调用」是判据缺陷的又一种形状：**扫到了东西，但扫的不是那件事。**
        let launch_rs = files
            .iter()
            .find(|(n, _)| n == "launch.rs")
            .map(|(_, s)| s.as_str())
            .unwrap_or("");
        assert!(
            launch_rs.contains("gate::admit"),
            "daemon 的 `control/launch.rs` 生产段没有调 `gate::admit` ——\n\
             门还在仓里，但**不在路上**：`send-into` 会绕过 §34 的 Gate 2 直接键入。\n\
             monitor 侧那条「不许改走 daemon」的禁令，其前提正是「daemon 的门是通的」。"
        );
    }

    /// ★ Gate 3 的前提触发器：daemon 一旦出现 `windows==1` / `kill-session`，
    /// 说明 F04 动工了 ⇒ 下面那条禁令的理由不再成立，回来重新裁定。
    #[test]
    fn the_daemon_still_has_no_gate3_so_the_routing_ban_below_still_applies() {
        let all: String = daemon_control_production()
            .iter()
            .map(|(_, s)| s.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        let found: Vec<&str> = DAEMON_GATE3_MARKERS
            .iter()
            .copied()
            .filter(|m| all.contains(m))
            .collect();
        assert!(
            found.is_empty(),
            "daemon 的 control 面出现了 Gate 3 / kill 的标志 {found:?} —— **这多半是好事**，\n\
             但它意味着下面那条禁令（`send-keys`/`kill` 不许改走 daemon）的前提变了：\n\
             那是 **F04** 的活。请连同「三道门在 daemon 侧怎么复现」与平价对账一起裁定，\n\
             然后把本模块整体退役。"
        );
    }

    /// ★ 正题：**F04 之前**这两个命令不许改走 daemon 通道。
    ///
    /// F03 之后理由换了：不再是「daemon 没有门」（它有了），而是
    /// **daemon 没有 Gate 3、也没有 kill**，且切路由要连平价账一起改（定框 C6 的顺序）。
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
            "`send-keys`/`kill` 被改走了 daemon 通道 —— 那是 **F04**，不是现在。\n\
             F03 已经把 §34 的 Gate 2 搬进 daemon `control/gate.rs`，但 daemon 今天\n\
             **没有 Gate 3**（`windows==1`，防误杀多窗口会话）、**也没有 kill 命令**；\n\
             而且切路由要连 `parity_ledger` 的平价账一起改。\n\
             定框 C6：**先搬 Gate 2，再切 kill / send-keys —— 顺序不可反。**\n{}",
            offenders.join("\n")
        );
    }
}
