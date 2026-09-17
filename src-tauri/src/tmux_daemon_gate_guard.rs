//! **§34 Gate 2 的两侧账**：daemon 侧的身份门必须在；monitor 侧 **kill 与 send-keys
//! 都必须已切过去**（F04b 翻了 kill 那半，F04c 翻了另一半 —— 那条禁令整个翻完了）。
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
//! 2. **禁令整个翻面了**（F04b 切 kill、**F04c 切 send-keys**）：定框 C6
//!    「先搬 Gate 2，再切 kill / send-keys」**走完了**。今天钉的是反向 ——
//!    **两条命令都必须走 daemon，不许退回**
//!    （[`tests::kill_now_routes_through_the_daemon`] /
//!    [`tests::send_keys_now_routes_through_the_daemon`]）。
//!    ★★ **`K-R72`（09-12）：那两条又各加了一格 —— 回潮闸。**
//!    `C7` 那条过渡期 SSH 回落**删了**（`K-R54` 裁定表第 1 · 2 处），于是这两条判据
//!    从「主路必须走 daemon **且回落必须还在**」变成「主路必须走 daemon
//!    **且盘上不许再有第二条路**」。⚠ 两次翻面的方向是相反的，别读成同一格改了措辞：
//!    先前那半逐字要求 `connect_and_exec_cmd` **在**，今天逐字要求它**不在**。
//!    ⚠ F04c 的表达力缺口是**补掉**的、不是绕开的：daemon 多了一个 mode 名
//!    `send-keys-raw`（发裸键、不附 `Enter`）。**必须是 mode 名而不是字段** ——
//!    `parse_request` 不 deny unknown fields ⇒ 旧 daemon 会静默忽略字段照样附 `Enter`，
//!    把「打断当前回合」变成「提交用户输入框里排队的文本」。
//! 3. ~~Gate 3 的前提触发器~~ **已在 F04a 触发并改写**：daemon 现在**有** Gate 3
//!    （`control/gate.rs::admit_destructive` + `control/kill.rs`）。那条触发器
//!    「daemon 一出现 `session_windows`/`kill-session` 就红」**如设计般红了一次**
//!    （`出现了 Gate 3 / kill 的标志 ["session_windows", "kill-session"] —— 这多半是好事`），
//!    于是按它自己的要求翻面：从「不许出现」改成 [`the_daemon_now_has_gate3`]（**不许消失**）。
//!    ⚠ **它红了不是误报，是它的岗位。** 删掉它才是错的处置（铁律 13）。
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

    /// Gate 3（`windows==1`，只约束破坏性动作）在 daemon 侧的形状。
    /// **F04a 起：必须存在**（此前是「一个都不该有」）。
    const DAEMON_GATE3_MARKERS: &[&str] = &["session_windows", "kill-session"];

    /// **必须**出现在这两个命令里的东西（走 daemon 的标志）。F04c 起是「必须有」而不是「不许有」。
    const DAEMON_CHANNEL_MARKERS: &[&str] = &["daemon_route::Routed"];

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
            .join("src/backend/control")
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
    ///
    /// ⚠ `K-R72`（09-12）**换了哨兵，而且非换不可**：原来那句断的是
    /// `body.contains("connect_and_exec_cmd")` —— 拿「回落那条 SSH 在不在」当
    /// 「抽到没抽到」的代理。今天回落删了，那个代理**恒假** ⇒ 这条自检会把
    /// 「抽取器好好的」报成「抽错了段」。⇒ 改成断 [`DAEMON_CHANNEL_MARKERS`]：
    /// 那是两条命令**今天唯一那条路**的标志，抽错段一样看得见。
    /// 🔴 顺带记下这一形：**一条自检的哨兵长在被测实现上，实现一变自检先假**。
    #[test]
    fn the_two_command_bodies_are_actually_extracted() {
        for sig in GUARDED_COMMANDS {
            let body = body_of(MONITOR_TMUX, sig);
            assert!(
                body.len() > 400,
                "`{sig}` 只抽到 {} 字节 —— 抽取坏了",
                body.len()
            );
            for m in DAEMON_CHANNEL_MARKERS {
                assert!(
                    body.contains(m),
                    "`{sig}` 的函数体里没有 `{m}` —— 抽错了段，或者它已经改走别的路了"
                );
            }
        }
    }

    /// 远端 tmux 命令里**只读**的动词。不在这张表里的一律按「有破坏性」处理。
    ///
    /// ⚠ **默认拒绝是本条的全部要点**：加一个新动词（`respawn-pane` / `kill-pane` /
    /// `paste-buffer` / `set-option` …）不需要谁想起来把它登记成危险的 ——
    /// 它天然就落在网里，除非有人**明确**把它写进这张只读表并为此负责。
    const READ_ONLY_VERBS: &[&str] = &[
        "ls",
        "list-sessions",
        "list-windows",
        "capture-pane",
        "display-message",
        "show-option",
        "has-session",
    ];

    /// 走 Gate 的唯一入口（Gate 2 远端半支与被守护的命令拼成**一条原子命令**）。
    const GATE_BUILDER: &str = "build_guarded_tmux_cmd";

    /// 取 `at` 所在的那个**顶层函数**（名字，函数体）。
    fn enclosing_fn(src: &str, at: usize) -> (String, String) {
        let head = &src[..at];
        let start = [
            head.rfind("\nfn "),
            head.rfind("\npub fn "),
            head.rfind("\npub async fn "),
            head.rfind("\nasync fn "),
            head.rfind("\npub(crate) fn "),
        ]
        .into_iter()
        .flatten()
        .max()
        .unwrap_or(0);
        let rest = &src[start..];
        let end = rest.find("\n}\n").map(|k| k + 3).unwrap_or(rest.len());
        let body = rest[..end].to_string();
        let name = body
            .split_once("fn ")
            .map(|(_, t)| {
                t.chars()
                    .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
                    .collect::<String>()
            })
            .unwrap_or_default();
        (name, body)
    }

    /// ★ **正题（〔audit-0805 08-06〕新增）**：monitor 侧发出的每一条远端 tmux 命令，
    /// 要么动词是只读的，要么它所在的函数**走 Gate**。
    ///
    /// # 它补的是哪个洞
    ///
    /// 本模块原来只看住**两个写死的签名**（[`GUARDED_COMMANDS`]）。
    /// 实测：往 `tmux.rs` 追加一条
    /// `pub async fn tmux_respawn_pane(..)`，里面直接 `format!("tmux respawn-pane -k -t {..}")`
    /// 再 `connect_and_exec_cmd` —— **`respawn-pane -k` 会杀掉 pane 里正在跑的进程**，
    /// 是彻头彻尾的破坏性操作，而且**完全不经 §34 的任何一道 Gate**。
    /// 全量 `cargo test --workspace`：**985 条全绿，一条都没响。**
    ///
    /// ⇒ 病根还是那一个：**用手写清单描述人群**。看住的是「今天这两个函数」，
    /// 不是「所有会对远端下命令的地方」。加第三个就出圈，而且没有任何信号。
    ///
    /// # 人群与判准（先量后定，量到的都写在这）
    ///
    /// 人群 = `tmux.rs` 生产段里每一处 `tmux <动词>` 字符串。
    ///
    /// # ★★ `K-R72`（09-12）：判准**收紧了一格**，而且是**变强**不是变弱
    ///
    /// 原判准是「动词 ∈ [`READ_ONLY_VERBS`] **或**所在函数含 `build_guarded_tmux_cmd`」。
    /// 今天 monitor 侧那条会拼破坏性 tmux 命令的路整个删了（送键与杀会话只走后端）
    /// ⇒ 那个「或」的右半**没有成员了**，而**留着一个空的或分支是危险的**：
    /// 它给「重新长出一个受托者、然后把破坏性动词挂上去」留着一条合法路。
    /// ⇒ 判准收成一条：**monitor 侧发出的每一条远端 tmux 命令都必须是只读动词。**
    /// 这比原来严格 —— 原来允许「走 Gate 的破坏性动词」，今天一个都不允许。
    ///
    /// ⚠ **人群里仍可能混进错误消息串**（`format!("tmux …: {..}")` 这种）。
    /// 刻意不去区分「命令串」与「消息串」—— 文本上分不干净。⚠ 但**代价随判准收紧变了**：
    /// 原先多算没有代价（那些函数本来就走 Gate），今天多算一处含破坏性动词的**消息串**
    /// 就是一次**误红**。⇒ 现打：`tmux.rs` 生产段里这样的消息串**零处**
    /// （那两处 `tmux kill-session: {..}` / `tmux send-keys: {..}` 随回落一起走了）。
    /// 哪天又长出来，正确处置是把那句消息改得不含裸动词，**不是**把动词塞进只读表。
    #[test]
    fn every_remote_tmux_verb_is_either_read_only_or_routed_through_the_gate() {
        let prod = guard_core::production_code(MONITOR_TMUX);
        // ★ `K-R72`：受托者不许再出现。这一句就是「那个空的或分支」的回潮闸 ——
        //   有人重新拼一个 `build_guarded_tmux_cmd` 出来，本条当场红。
        assert!(
            !prod.contains(GATE_BUILDER),
            "`tmux.rs` 生产段里又出现了 `{GATE_BUILDER}` —— 那是 monitor 自己拼\n\
             「原子 verify+act 远端 shell 串」的受托者，`K-R72` 把它连同它唯一的两个消费者\n\
             （kill / send-keys 的一次性 SSH 回落）一起删了。要恢复它先回 `K-R54` 重新裁定。"
        );
        let mut seen_read_only = 0usize;
        let mut bad: Vec<String> = Vec::new();
        let mut from = 0usize;
        while let Some(rel) = prod[from..].find("tmux ") {
            let i = from + rel;
            from = i + "tmux ".len();
            let verb: String = prod[from..]
                .chars()
                .take_while(|c| c.is_ascii_lowercase() || *c == '-')
                .collect();
            if verb.is_empty() {
                continue;
            }
            let (fname, _) = enclosing_fn(&prod, i);
            if READ_ONLY_VERBS.contains(&verb.as_str()) {
                seen_read_only += 1;
            } else {
                bad.push(format!("  {fname}() 里的 `tmux {verb}`"));
            }
        }
        // 抽取器自检：一处都没数到 ⇒ 剥生产段或扫描坏了，本条此刻量不到东西。
        // 🔴 `K-R112`（09-13）：地板 2 → **1**。`tmux capture-pane` 那一处随抓屏改走
        //    daemon 帧面而不存在了 ⇒ monitor 侧只剩 `tmux … ls`（`list_remote_tmux`）一处。
        //    ⚠ **这一格快到头了**：`list_remote_tmux` 也改走后端的那天，这个人群会归零，
        //    而**归零之后本条就是空真**（`bad` 恒空）—— 到那一拍该做的不是把地板改成 0，
        //    是给它换一份**会漂的活体语料**（同 `tmux.rs::every_target_placeholder_comes_from_exact_target`
        //    今天的做法：人群 0 + 一份合成坏语料承重）。
        assert!(
            seen_read_only >= 1,
            "只数到 {seen_read_only} 处只读动词 —— 抽取器或剥生产段那步坏了，本条此刻空转"
        );
        assert!(
            bad.is_empty(),
            "monitor 侧有**不是只读动词**的远端 tmux 命令：\n{}\n\
             `K-R72` 起 monitor 只许对远端 tmux 下**只读**命令 —— 改状态的一律走后端\n\
             （`C5` 逐字：任何改状态的 tmux 命令一律归 `control/`）。\n\
             ⚠ **别把动词加进 `READ_ONLY_VERBS` 来消红** —— 那张表只收真正不改变远端状态的动词。\n\
             正确动作：把这条命令搬进 daemon 的 `control/`，让它过 `admit` / `admit_destructive`。",
            bad.join("\n")
        );
    }

    // 〔`K-R72` 09-12 留档〕上一版判准的另一半住在这里 —— **刻意只留话，不留代码**。
    //
    // 原来那半逐字是：「直接含 `build_guarded_tmux_cmd` 的函数算走 Gate；调用了这样一个
    // 函数的也算」（求闭包，因为 `kill_remote_tmux` → `build_kill_session_cmd` → Gate  〔散文墓碑〕
    // 中间隔了一层；第一版没求闭包，那两处当场误红）。那段派生逻辑今天**没有被测对象**。
    // 留一段跑不到的代码在这儿，就是本件 `KR72D1` 逐字禁止的那件事的测试侧变体：
    // **盘上留着一条走不到的路。**⇒ 删干净，理由写在这里。

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

    /// ★ **F04a 起翻面：daemon 的 Gate 3 必须还在**（此前钉的是「不许出现」）。
    ///
    /// # 这条触发器完整走过了一遍它设计的生命周期
    ///
    /// U10 立它时钉「不许出现」——因为那时 daemon 没有 Gate 3，下面那条路由禁令
    /// 靠的就是这个前提。F04a 把 Gate 3 搬进来，它**如设计般红了一次**：
    /// `出现了 Gate 3 / kill 的标志 ["session_windows", "kill-session"] —— 这多半是好事`。
    ///
    /// ⚠ 那时正确的处置**不是删掉它**（铁律 13：删判据前先证明它恒绿），
    /// 而是**改写**：前提变了 ⇒ 换成钉新前提。现在它钉「Gate 3 不许消失」。
    #[test]
    fn the_daemon_now_has_gate3() {
        let files = daemon_control_production();
        let all: String = files
            .iter()
            .map(|(_, s)| s.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        let missing: Vec<&str> = DAEMON_GATE3_MARKERS
            .iter()
            .copied()
            .filter(|m| !all.contains(m))
            .collect();
        assert!(
            missing.is_empty(),
            "daemon 的 control 面**找不到** Gate 3 的标志 {missing:?} ——\n\
             §34 的第三道门（`windows == 1`，防误杀多窗口会话）在 daemon 侧没了。\n\
             F04a 把它装在 `control/gate.rs::admit_destructive`；`control/kill.rs` 走它。"
        );
        // 门必须**在路上**（F+02 的教训：模块存在 ≠ 模块被调用）。
        let kill_rs = files
            .iter()
            .find(|(n, _)| n == "kill.rs")
            .map(|(_, s)| s.as_str())
            .unwrap_or("");
        assert!(
            kill_rs.contains("admit_destructive"),
            "`control/kill.rs` 的生产段没有调 `admit_destructive` ——\n\
             门还在仓里但不在路上：kill 会绕过 Gate 2/3 直接杀。"
        );
        // Gate 3 **只给破坏性动作**：非破坏性的 `admit` 不许看窗口数。
        let gate_rs = files
            .iter()
            .find(|(n, _)| n == "gate.rs")
            .map(|(_, s)| s.as_str())
            .unwrap_or("");
        let plain = gate_rs
            .split("pub(crate) fn admit(")
            .nth(1)
            .and_then(|t| t.split("pub(crate) fn admit_destructive").next())
            .unwrap_or("");
        assert!(
            !plain.is_empty(),
            "抽不到非破坏性 `admit` 的函数体 —— 抽取器坏了，下面那条会零命中地绿"
        );
        assert!(
            !plain.contains("windows != 1") && !plain.contains("p.windows"),
            "非破坏性的 `admit` 里出现了窗口数判断 —— `send-keys` 不删除任何东西，\n\
             给它加 Gate 3 会让「往一个多窗口会话里打字」被误拒（monitor 侧 F04 Phase D 审计修过这个错法）。"
        );
    }

    /// ★ **F04b 起翻面：`kill` 必须走 daemon 通道**（此前钉的是「不许走」）。
    ///
    /// # 这条禁令的理由换过三版，现在它的 kill 那半整个翻面了
    ///
    /// · U10 版：「daemon 没有身份门」⇒ F03 装了 Gate 2，前提失效；
    /// · F04a 版前：「daemon 没有 Gate 3、也没有 kill」⇒ F04a 都搬了，前提又失效；
    /// · F04a 版：「平价账没改 + 真远端那跳验不了 ⇒ 独立一件」⇒ **F04b 就是那一件**。
    ///
    /// ⇒ 按前提触发器自己的要求：前提没了就**翻面**，不是删掉（铁律 13）。
    /// 现在它钉「主路不许退回 SSH」。
    #[test]
    fn kill_now_routes_through_the_daemon() {
        let body =
            guard_core::production_code(&body_of(MONITOR_TMUX, "pub async fn kill_remote_tmux("));
        assert!(
            body.contains("daemon_kill::daemon_kill("),
            "`kill_remote_tmux` 的生产段没有调 `daemon_kill::daemon_kill(` ——\n\
             主路退回了「monitor 自己拼一条 SSH 串杀会话」，那是 C5 逐字禁止的\n\
             （任何改状态的 tmux 命令一律归 `control/`），也把 F04a 搬进 daemon 的\n\
             「对**句柄**下手」退回成「对**名字**下手」（TOCTOU 窗口）。\n\
             ⚠ 这不是「换个写法」能满足的判据：C6 那条顺序走到这里就是最后一步。"
        );
        // ★★ **`K-R72`（09-12）：这一格翻了面 —— 从「回落必须还在」变成「不许再有」。**
        //
        // 原来这里逐字断的是 `body.contains("connect_and_exec_cmd") &&
        // body.contains("build_kill_session_cmd")`，理由是 `C7`「回落路径在过渡期必须留」。  〔散文墓碑〕
        // `K-R54` 的裁定表第 2 处把那个过渡期**判结束了**（留 daemon、删回落），
        // `K-R72` 执行。⇒ 今天这一格是**回潮闸**：那条路再回来就红。
        //
        // ⚠ 这不是「放宽」：`C7` 当初买的是「旧版机器上还没有 daemon 时仍杀得掉」，
        // 而 `K35`（09-11）逐字裁掉了「没有后端」这回事 —— 前提没了，禁令跟着翻面，
        // 与本模块头注记的那三次翻面同一条规矩（前提变了就回来重裁，不是悄悄绕过）。
        assert!(
            !body.contains("connect_and_exec_cmd"),
            "`kill_remote_tmux` 里又出现了 `connect_and_exec_cmd` —— 那条一次性 SSH 回落回潮了。\n\
             它杀的是 `=name:`（**名字**），而 daemon 那条先 `admit_destructive` 拿\n\
             `#{{session_id}}` **句柄**再杀 —— 破坏性动作对名字下手就把 TOCTOU 窗口留着。\n\
             ⚠ 要恢复它先回 `K-R54` 重新裁定，别在一次重构里把它带回来。"
        );
    }

    /// ★★ **本件最要紧的一条**：过门被拒绝**绝不**回落到 SSH。
    ///
    /// # 为什么值得单独一条判据
    ///
    /// 「失败就回落」是这类切换最自然的写法，而它在这里是**错的**：
    /// daemon 回 `wrong_owner` / `too_many_windows` 是**门做出的决定**，
    /// 转头用另一条路再杀一次 = 把一次被门拒绝洗成另一条路的成功。
    /// 今天两条路的门恰好等价（都是 §34 三道门）所以功能上看不出差别 ——
    /// **那正是它危险的地方**：哪天有一侧漂了，没有任何判据会红。
    ///
    /// 分流规则本体由 `daemon_route::only_the_errors_that_prove_nothing_was_sent_allow_a_fallback`
    /// 钉住（纯函数，F04c 起 `kill` 与 `send-keys` 共用一份）；
    /// 本条钉的是**生产段真的按三态分了流**，而不是把三态压成两态。
    ///
    /// # ⚠ `K-R72`（09-12）：**名字里那个「ssh_fallback」今天已经不存在了 —— 本条仍然要**
    ///
    /// 回落删了之后，「洗成另一条路的成功」这条具体路径**结构上没有了**。
    /// 但本条守的从来不是那条路，是**三态不许压成两态**：`Refused`（门做的决定）与
    /// `NoChannel`（通道不在）今天各自 `return` 一句**不同的话**，
    /// 而把它们并成一臂（`_ => Err(...)`）会让「你不能动这个会话」与「后端没连上」
    /// 变成同一个读数 —— 那正是 `KR72D1` 那条边界要的反面。
    /// ⇒ 本条**加一格**：三条臂都必须自己 `return`，一条都不许穿到函数尾巴上去。
    /// **名字刻意不改**：它记着这条判据当初为什么立，而那段病史今天仍是读懂它的前提。
    ///
    /// # 🔴 `K-R72` 第二拍（09-12）：**上一版这一格是空的 —— 实打逮出来的，不是想出来的**
    ///
    /// 上一版用的是「从 `Routed::Refused` 往后取 160 字节，看里面有没有 `Err`」＋
    /// 「两个 160 字节窗口逐字不许相同」。那两格**都挡不住真正的压平**：
    /// 把两臂并成 `Routed::Refused(why) | Routed::NoChannel(why) => Err(why),` 之后，
    /// 两个标记**都还在**、窗口里**都有 `Err`**、两个窗口**起点不同所以逐字也不同**
    /// ⇒ 三格全绿。**实打读数**（沙箱，`cargo test -p monitor --lib`）：
    /// 那一刀落在 `kill_remote_tmux` 上 ⇒ `1422 passed; 1 failed`，
    /// 唯一红的是 `tmux::the_local_kill_never_falls_back_to_ssh`（它红是因为
    /// `no_channel_message` 从函数体里没了，**属附带命中，不是本条在守**）；
    /// 同一刀落在 `tmux_send_keys` 上同样只红那一条的 send-keys 版本。
    /// ⇒ **本条当时并没有在守它自称守的那件事。**
    ///
    /// 改法（两处一起改，别只补一格）：
    /// ① **按 `=>` 切臂**，不再取定长窗口 —— 一条臂 = 从标记到它自己那个 `=>` 之间的文本；
    ///    那段里出现 `|` 或出现另一个标记 ⇒ 两态被并成一臂，当场红。
    /// ② **人群从 1 个命令扩到 [`GUARDED_COMMANDS`] 两个** —— 上一版只看 `kill`，
    ///    而 `tmux_send_keys` 那份的三态分流**从来没有判据**。
    #[test]
    fn a_gate_rejection_is_never_laundered_into_the_ssh_fallback() {
        /// 一条 `match` 臂的模式段：从标记起，到它自己那个 `=>` 为止。
        fn pattern_of<'a>(body: &'a str, marker: &str) -> &'a str {
            let at = body.find(marker).expect("调用方已断言过存在");
            let rest = &body[at..];
            &rest[..rest.find("=>").expect("这条臂没有 `=>` —— match 形状变了")]
        }
        for sig in GUARDED_COMMANDS {
            let body = guard_core::production_code(&body_of(MONITOR_TMUX, sig));
            for arm in ["Routed::Done", "Routed::Refused", "Routed::NoChannel"] {
                assert!(
                    body.contains(arm),
                    "`{sig}` 的生产段没有 `{arm}` 分支 —— 三态被压成了两态。\n\
                     三态的分界线是「能不能**证明**这条命令根本没发出去」，不是「成功/失败」。"
                );
            }
            for (arm_name, other, why) in [
                (
                    "Routed::Refused",
                    "Routed::NoChannel",
                    "一次 `wrong_owner` / `too_many_windows` 是**门做的决定**",
                ),
                (
                    "Routed::NoChannel",
                    "Routed::Refused",
                    "「后端通道不在」是**通道的事**，与门无关",
                ),
            ] {
                let pat = pattern_of(&body, arm_name);
                assert!(
                    !pat.contains('|') && !pat.contains(other),
                    "`{sig}` 里 `{arm_name}` 与别的态并成了同一条臂（{why}）——\n\
                     「你不能动这个会话」与「后端没连上」从此共用一个读数，\n\
                     而用户下一步该做的事完全不同。实得这条臂的模式段：{pat:?}"
                );
            }
            // ★ 反向自检：这把尺子在一份**真的压平了**的合成语料上必须分得出来。
            //   ⚠ 语料里不带本仓任何真实函数名（`6g` 那一族：夹具与断言不许同源）。
            const FLATTENED: &str = "match r { A::Routed::Done => Ok(()), \
                 A::Routed::Refused(w) | A::Routed::NoChannel(w) => Err(w), }";
            assert!(
                pattern_of(FLATTENED, "Routed::Refused").contains('|'),
                "尺子瞎了：一份逐字压平的语料没被认出来"
            );
        }
    }

    /// ★ **F04c 起翻面：`send-keys` 也必须走 daemon 通道**（此前钉的是「不许走」）。
    ///
    /// # 这条禁令的四版理由，全部被后续功能推翻，最后它自己翻了面
    ///
    /// · U10 版：「daemon 没有身份门」⇒ F03 装了 Gate 2；
    /// · F04a 版前：「daemon 没有 Gate 3、也没有 kill」⇒ F04a 都搬了；
    /// · F04a 版：「平价账没改 + 真远端那跳验不了 ⇒ 独立一件」⇒ F04b 就是那一件；
    /// · F04b 版：「daemon 的 `type_payload` **恒附 `Enter`**，`enter=false` 表达不出来」
    ///   ⇒ **F04c 给 daemon 补了一个 mode 名**（`send-keys-raw`），缺口没了。
    ///
    /// ⚠ **四版理由都是真的、都在当时成立** —— 前提触发器的价值就在这里：
    /// 它让每一次「前提变了」都必须回来重裁一次，而不是让一条过期的禁令继续挡路，
    /// 也不是让人悄悄绕过它。**它红了不是误报，是它的岗位。**
    #[test]
    fn send_keys_now_routes_through_the_daemon() {
        let body =
            guard_core::production_code(&body_of(MONITOR_TMUX, "pub async fn tmux_send_keys("));
        assert!(
            body.contains("daemon_send_keys::daemon_send_keys("),
            "`tmux_send_keys` 的生产段没有调 `daemon_send_keys::daemon_send_keys(` ——\n\
             主路退回了「monitor 自己拼一条 SSH 串往别人会话里打字」，那是 C5 逐字禁止的。\n\
             ⚠ 定框 C6 的顺序到 F04c 已经走完，退回去就是把它走反。"
        );
        // ★★ **`K-R72`（09-12）：这一格翻了面 —— 从「回落必须还在」变成「不许再有」。**
        // 同 [`kill_now_routes_through_the_daemon`] 那一格的举证；本条对应
        // `K-R54` 裁定表第 1 处（`K-R56` 09-11 先把两条路的「探了没有」补齐才删得掉）。
        assert!(
            !body.contains("connect_and_exec_cmd"),
            "`tmux_send_keys` 里又出现了 `connect_and_exec_cmd` —— 那条一次性 SSH 回落回潮了。\n\
             它在 `cc-*` 形状名上落退化分支（`need_sid`/`need_windows` 双 false），\n\
             而 daemon 的 `admit` 恒先 `probe` ⇒ 两条路的门不等价。\n\
             ⚠ 要恢复它先回 `K-R54` 重新裁定。"
        );
        // ★ `enter` 必须真的传给 **daemon 那条路** —— 不传就等于把 `Escape` 也当成「提交」。
        //
        // ⚠ **这条判据的第一版是恒绿的，变异复验才把它抓出来。**
        // 第一版写的是 `body.contains("&keys, enter,") || body.contains("&keys, enter)")` ——
        // 那个 `||` 是为了「容忍 rustfmt 的换行」加的，结果第二个分支命中了**回落那条**
        // （`build_send_keys_remote_cmd(&target, &keys, enter)?`）⇒ 把 daemon 那处改成  〔散文墓碑〕
        // 硬编码 `true` 时它照样绿。**「扫到了东西，但扫的不是那件事」的又一次**，
        // 而且这次是我自己为了「稳」加的容错造出来的。⇒ 改成**先切出 daemon 那次调用的实参段**
        // 再看，容错去掉。
        let call = "daemon_send_keys::daemon_send_keys(";
        let at = body.find(call).expect("上面已断言过存在");
        let args_seg = &body[at + call.len()..];
        let args = &args_seg[..args_seg.find(')').expect("找不到实参段的收尾括号")];
        assert!(
            args.contains("enter") && !args.contains("true") && !args.contains("false"),
            "`enter` 没有传给 daemon 那条路（实参段是 {args:?}）—— 那么 `Escape`\n\
             （打断当前回合）会被当成「键入并提交」，把用户输入框里排队的文本发出去。"
        );
    }

    /// ★ **两条命令都必须走同一个分流器**（不许各写一份「什么时候可以回落」）。
    ///
    /// 这条与 `daemon_route::both_daemon_commands_use_this_one_router` 不重复：
    /// 那条查**发送端**是不是自己 match `CallError`，本条查**命令体**是不是按同一套三态分流。
    #[test]
    fn both_commands_branch_on_the_same_three_way_verdict() {
        for sig in GUARDED_COMMANDS {
            let body = guard_core::production_code(&body_of(MONITOR_TMUX, sig));
            for m in DAEMON_CHANNEL_MARKERS {
                assert!(
                    body.contains(m),
                    "`{sig}` 的生产段里找不到 `{m}` —— 它要么没走 daemon，\n\
                     要么自己另写了一套「什么时候可以回落」。后者更危险：\n\
                     一次 `wrong_owner` 被判成「daemon 不可用」就会被另一条路重做一遍。"
                );
            }
        }
    }
}
