//! `cc-spawn` 怎么找到并使唤 `ccm` —— **那条边**的契约。
//!
//! # 🔴 `K-R48` 第二拍（09-11）：本模块从 2773 行砍到今天这个样子，理由写清楚
//!
//! 它原来的题目是「`shared/ccm` 这份 **bash 脚本**的强度契约」：一张 needle 表 ＋ 一个
//! [`Strength`] 读数 ＋ 一条基线，量的是「那份脚本里还有没有这几个串」「`-t` 目标扫到几处」。
//! 〔用@09-11 `K33`〕逐字「后端**只有一个**…**不要有什么 bash 脚本**，**不要有什么单独的 ccm**」
//! ⇒ **那份脚本删了，这些读数没有被测对象了**。
//!
//! 删掉的 22 条判据与三张表（`REQUIRED_NEEDLES` / `LEDGER` / `BACKEND_BACKED_PATHS`、
//! `measure` / `scan_t_targets` / `pin_t_def` 〔散文墓碑〕 / `BASELINE`）逐条判词住
//! `tests/evidence/K-R48-356-verdicts.tsv` 的同族条目；它们守的**性质**去了哪里，逐条写在
//! 件文件 `features/K-R48-…md` 的 `§8c`。
//!
//! # 留下来的这 7 条为什么留
//!
//! 它们**一条都不读那份脚本** —— 读的是 `src/shared/cc-bus/scripts/cc-spawn` 与
//! `src/shared/cc-bus/SKILL.md`。钉的是「**别人怎么找到并使唤 `ccm`**」这条边：
//! 解析错了、名字自己拍了、撞名乱重试了，后面整条 CLI 契约都无从谈起。
//! 那条边今天仍然存在，只是另一头从「一个 bash 脚本」换成了「后端本体」。
//!
//! ⚠ **住址**：`K-R48` 第一拍说「最自然的新家是 `src/bridge/src/cc_bus.rs`」，
//! 而本拍写区里**没有** `cc_bus.rs` ⇒ 留在原文件，不擅自扩写区。搬不搬归 PM。
//!
//! # ★★ 诚实边界（原 `P4b-Y3`，逐字保留）：它钉的是仓内那份，而本机真正在跑的不是它
//!
//! 实测：`~/.local/bin/cc-*` 全是指向 `~/.claude/skills/cc-bus/scripts/` 的 symlink，
//! 而那份是 07-18 的；仓内这份是另一份。`tool_registry.rs` 声明了
//! `src/shared/cc-bus` → `.claude/skills/cc-bus` 的部署映射，但那是**纯声明表**，
//! **没有任何东西真的按它部署**。
//! ⇒ 本文件里所有判据**证明的是仓内那份的性质，不是本机行为**。别读成「机器上就是这样」。
//! 〔`K-R48` 第一拍 PM 审计逐字复核过这一格：「**工具是，文件不是**」。〕
//!
//! 注：本模块整体在 `#[cfg(test)]` 内，非测试构建为空、零运行期开销。

#![cfg(test)]

#[cfg(test)]
mod tests {
    /// **shell 的生产段** = 剥掉 `#` 注释行 —— 直接用共享原语 `strip_hash_comment_lines`。
    ///
    /// ⚠ 不能用 `guard_core::production_code`：它剥的是 Rust 的 `//`，对 shell 一行都剥不掉。
    /// 08-13 实测：判据用它取「生产段」，结果被**解释性注释里提到的一个串**判红。
    /// ★ 剥注释器**选错了语言，等于没剥**。
    ///
    /// ⚠ 而首版在这里内联了一份 `filter(starts_with('#'))` —— `structural_scan` 的
    /// 「剥注释实现只许一份」登记表**当场逮住**。⇒ 改成调共享原语。
    fn shell_production(src: &str) -> String {
        guard_core::strip_hash_comment_lines(src)
    }

    fn cc_spawn_path() -> std::path::PathBuf {
        crate::guard_support::repo_root()
            .join("src/shared/cc-bus/scripts/cc-spawn")
    }

    /// ★ P4b-Y1/Y2（`C14`〔用 08-12〕「spawn 就是起, 就是 creat」）。
    ///
    /// 那个 `or` 的**第四份**实现住在这里：`P3s` 数出两份（`ccm` 的、TS 的），
    /// `P4d` 的冒烟又撞出 daemon 的 wire mode（第三份），这里是第四份。
    ///
    /// **删复用与留避让必须同时钉**：把「不复用」做成「不避让」的话，同名直接建会撞上
    /// 别人的会话 —— 那正是 `C14` 要消灭的东西的另一面。
    #[test]
    fn cc_spawn_creates_it_never_reuses_a_live_session() {
        let src = std::fs::read_to_string(cc_spawn_path()).expect("读 cc-spawn");
        assert!(
            src.lines().count() >= 50,
            "`cc-spawn` 只剩 {} 行 —— 读法坏了或文件被掏空",
            src.lines().count()
        );
        // ① 复用那条路必须没了。钉的是**形状**不是某句文案：
        //    「探到活会话就 `exit 0`」这条路一旦回来，下面任一条都会命中。
        for gone in ["复用已有会话", "new_flag"] {
            assert!(
                !src.contains(gone),
                "`cc-spawn` 里又出现了 {gone:?} —— 默认复用回潮了。\n                 `C14`〔用 08-12〕逐字：「所有起会话就是起会话……**spawn 就是起, 就是 creat**」。"
            );
        }
        // ② 命名避让必须还在（不复用 ≠ 不避让）—— 但 `C15`〔用@08-13〕之后它**搬进 ccm 了**。
        //    ⚠ 这条判据 08-12 原本钉 `cc-spawn` 里那句 `while tmux has-session`。
        //    搬家之后**不能只是删掉它**：那样「避让还在不在」就没人钉了。改钉两件 ——
        //    （a）cc-spawn 把基名交出去；（b）避让在 ccm 那边（旧判据 `the_avoidance_lives_in_ccm_now` 〔散文墓碑〕，
        //    `K-R48` 第二拍随 `shared/ccm` 删；今天由 `control::ccm::plan::tests::only_two_of_the_three_naming_paths_step_aside_on_a_collision` 钉）。
        // ⚠ 下面两条都判**剥掉 `#` 注释之后**的正文。首版没剥，当场被自己写的那句
        //   「原来这里是 `while tmux has-session …`」（记录搬走了什么的**诚实注释**）判红。
        //   ★ 这是本拍第二次撞上同一族：**匹配单位比事实大** —— 判据要判的是「代码里有没有」，
        //   而 `contains` 判的是「文件里有没有」。散文里提一句被删掉的东西是**好事**，不该被拦。
        let prod_spawn = shell_production(&src);
        guard_core::find_pinned(&prod_spawn, "--tmux-base=\"$base\"").unwrap_or_else(|e| {
            panic!(
                "{e}\n                 ⇒ cc-spawn 不再把**基名**交给 ccm 了。`C15` 之后避让归 ccm：\n                 cc-spawn 自己探一遍名字再传 `--tmux=<名>`，等于同一个事实算两遍，\n                 而且探完到真建之间有窗口期（`P3sc` 之后显式名撞名是 exit 3 响亮失败）。"
            )
        });
        // ⚠ 用 `contains_word` 而不是裸 `contains`：`needle_anchor_registry` 是**递减棘轮**，
        //   本拍新加的两处裸 `contains` 当场把它从 33 顶到 35（「不许把上限调上去让今天好过」）。
        assert!(
            !guard_core::contains_word(&prod_spawn, "while tmux has-session"),
            "`cc-spawn` 里又长出了自己的命名避让 —— `C15`〔用@08-13「cc-bus收进ccm」〕之后\n                          那件事归 `ccm`（`--tmux-base`）。留两份 = 改一处漏一处。"
        );
        // ③ `--new` 保留为 no-op：外面可能有人在传，让它报错等于把别人的脚本弄坏。
        // ⚠ needle 要**唯一确定那个事实**：`--new` 在用法串与注释里也出现
        //（`find_pinned` 当场报「命中 2 处，断言指不明是哪一处」）。钉那条 case 臂本身。
        guard_core::find_pinned(&src, "--new)  shift;;").unwrap_or_else(|e| {
            panic!("{e}\n⇒ `--new` 整个删掉了。它该留成 no-op —— 兼容外面已有的调用方。")
        });
    }

    /// ★ P4b D 阶段补审：**`SKILL.md` 不许再教「复用」**。
    ///
    /// 那份文件是 agent 会读的**指令**，而它逐字写着「默认"到就用、没有才建"……
    /// 该目录已有活会话就**复用**……**不会误建重复会话**」——
    /// 删掉代码里的复用之后，这句话当天就成了假话，而**读它的是自动化，不是人**。
    ///
    /// 这正是本仓一路在治的「散文与代码说的不是一件事」。⇒ 立一条禁词守卫。
    #[test]
    fn the_cc_bus_skill_no_longer_teaches_session_reuse() {
        let path = crate::guard_support::repo_root()
            .join("src/shared/cc-bus/SKILL.md");
        let src = std::fs::read_to_string(&path).expect("读 cc-bus SKILL.md");
        assert!(
            src.lines().count() >= 20,
            "`SKILL.md` 只剩 {} 行 —— 读法坏了或文件被掏空",
            src.lines().count()
        );
        // ⚠ 钉的是**关于 cc-spawn 的那句教法**，不是「复用」这两个字本身 ——
        // `cc-busd` / `cc-bus-lib.sh` 里讲「PID 被复用」是另一回事，禁掉它是误伤。
        for banned in ["到就用、没有才建", "已有活会话就"] {
            assert!(
                !src.contains(banned),
                "`cc-bus/SKILL.md` 里又出现了 {banned:?} —— 那是 `cc-spawn` 复用活会话的教法，\n                 而代码里那条路已经删了（`C14`〔用 08-12〕「spawn 就是起, 就是 creat」）。\n                 ★ 读这份文件的是**自动化**，不是人：一句过期的指令会让 agent 按不存在的行为办事。"
            );
        }
    }

    /// ★ P4b-Y3：那条诚实边界**必须写在源码里**，不能只活在计划文件里。
    ///
    /// 这是「禁词守卫」的反面 —— **必需词**守卫：删掉那段话的人会被拦一次。
    /// 它防的不是笔误，是**下一个人把这些判据读成「机器上就是这样」**。
    #[test]
    fn the_cc_spawn_judges_say_out_loud_they_pin_the_repo_copy_not_the_running_one() {
        let me = include_str!("ccm_cli_contract.rs");
        // ⚠ **数次数，不是 `contains`**：本判据自己的数组里就写着这两句
        // ⇒ `contains` 恒真，改掉头注它照样绿（实测 `M4` 第一次就是这么绿的）。
        // 本会话已经第三次栽在「判据被自己要钉的名字命中」上（`P3s-Y2` / `P4d-Y4`）。
        // ⇒ 要求出现 **≥ 2 次**：一次是这里的字面量，另一次必须在头注里。
        for must in ["本机真正在跑的不是它", "没有任何东西真的按它部署"] {
            assert!(
                me.matches(must).count() >= 2,
                "`cc_spawn_path` 的头注里少了 {must:?}。\n                 那段话记的是一条**结构性假绿**：判据钉的是仓内那份，而 `~/.local/bin/cc-*` \n                 指向的是 `~/.claude/skills/` 那份（实测差 167 行）。删掉它，\n                 下一个人就会把这些判据读成「机器上就是这样」。"
            );
        }
    }

    #[test]
    fn cc_spawn_resolves_a_real_ccm_file_not_a_directory() {
        let path = cc_spawn_path();
        let src = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("读不到 {path:?}: {e}"));
        // ★ 抽取器自检：文件被掏空/改名时，下面那条会零命中地绿。
        assert!(
            src.lines().count() >= 50,
            "`cc-spawn` 只剩 {} 行 —— 读法坏了或文件被掏空",
            src.lines().count()
        );
        // 🔴 〔`K-R48` 第二拍 09-11〕**针换了一次，性质一个字没变。**
        //    从前 `cc-spawn` 找的是 `$SELFDIR/../../ccm`（仓内那个 bash 脚本，部署形态下落在
        //    `~/.claude/skills/ccm`）。〔用@09-11 `K33`〕那个文件删了，查找次序整条换成找后端
        //    （`$CCM_BIN` → `~/.cc-monitor/bin/` → PATH）。
        //    ⚠ **那个坑没有变小，反而更宽**：查找次序里每一档都可能撞上**同名目录**
        //    （`~/.cc-monitor/bin/` 下、`PATH` 上都一样），而 `[ -x <目录> ]` 为真。
        //    ⇒ 本条仍然要求**同一处判断里 `-f` 与 `-x` 同时在**，只是落点从那两个字面量
        //    换成了循环体里的 `$_x`。
        // ⚠ **钉性质不钉拼法**〔第一版是 `pin_line` 整行相等，变异当场暴露它过严〕：
        //    把顺序换成 `[ -x X ] && [ -f X ]` 语义完全一样，而整行相等会误红。
        for probe in ["-f \"$_x\"", "-x \"$_x\""] {
            guard_core::find_pinned(&src, probe).unwrap_or_else(|e| {
                panic!(
                    "{e}\n\
                     ⇒ `cc-spawn` 解析 `ccm` 时缺了 `{probe}` 这一半。**只 `-x` 不够**：\n\
                     `[ -x <目录> ]` 为真，而查找次序里那几档都可能是目录\n\
                     （历史实测：`~/.claude/skills/` 下全是目录，装个名叫 `ccm` 的 skill 就会劫持解析，\n\
                     报「是一个目录」后整体失败）。\n\
                     这一处**没有本地回落分支**，所以失败是硬的，不是诚实降级。"
                )
            });
        }
        // 两个测试必须在**同一行**：分散到两处会让「其中一处被删」看起来仍然合规。
        let same_line = src
            .lines()
            .any(|l| l.contains("-f \"$_x\"") && l.contains("-x \"$_x\""));
        assert!(
            same_line,
            "`-f` 与 `-x` 不在同一行了 —— 解析分支被拆开，其中一半可能已经不在那条判断上。"
        );
        // 🔴 反向：那条**已经不存在的**旧路径不许再出现（留着它 = 又去找一个不存在的脚本，
        //    而失败是静默的：`[ -f ]` 不成立就默默往下一档走）。
        // ⚠ 只看**生产段**：脚本头注里那句「原来这里找的是 `$SELFDIR/../../ccm`」是**来历**，
        //    不是代码。不剥注释的话这一条会把自己的墓碑读成复发（本仓最高频那类假红）。
        assert!(
            !shell_production(&src).contains("$SELFDIR/../../ccm"),
            "`cc-spawn` 的生产段又去找 `$SELFDIR/../../ccm` 了 —— 那个 bash 脚本 `K-R48` 已经删了，\n\
             找一个不存在的东西不会报错，只会**静默地落到下一档**。"
        );
    }

    /// ★ `P4b①`②：cc-spawn **从 ccm 读回名字**，且**读不到就停**。
    ///
    /// 读不到还往下走的话，`cc-register` 与 `spawned.tsv` 会写进**空名字** ——
    /// 总线上多一个叫 `""` 的幽灵，`cc-list` 显示在线、`cc-send` 石沉大海。
    /// 本仓给这种形状起过名字：**假成功比失败更坏**（B02 审计阻塞-2 那次逐字同款）。
    #[test]
    fn cc_spawn_reads_the_name_back_instead_of_computing_it() {
        let src = std::fs::read_to_string(cc_spawn_path()).expect("读 cc-spawn");
        let prod = shell_production(&src);
        // ⚠ 光钉「摘取表达式在」**不够**：D 阶段 M2 实测把它改成
        //   `name="$base"; _unused=$(… ccm-session= …)` —— 表达式还在，名字却是自己拍的，判据全绿。
        //   ⇒ 钉的必须是**赋值**：`name=` 恰好一处，且那一处就是摘取。
        let name_assigns: Vec<&str> = prod
            .lines()
            .map(|l| l.trim_start())
            .filter(|l| l.starts_with("name="))
            .collect();
        assert_eq!(
            name_assigns.len(),
            1,
            "`cc-spawn` 生产段里给 `name` 赋值 {} 处（应恰好 1 处）。多一处 = 名字有第二个来源，\n                          而只有 `ccm` 知道它真建出了哪个。实得：{name_assigns:?}",
            name_assigns.len()
        );
        // ⚠ 还不够（M2 第二次实测）：`name="$base"; _unused=$(… ccm-session= …)` **也是一行**，
        //   于是「唯一那处赋值里有摘取表达式」照样成立。⇒ 钉**整个赋值就是那次摘取**：
        //   赋值右手边必须直接是命令替换（`name=$(`），不是先拍一个名字再顺手跑个表达式。
        //   ⚠ 第三轮：`name=$(printf "%s" "$base"); _x=$(… ccm-session= …)` 仍然过（一行两条命令，
        //   前两条判据都成立）。⇒ 那行必须是**纯赋值**：不许用 `;`/`&&` 在同一行串第二条命令。
        //   ★ 到此为止不再加码 —— 判据钉的是**事实的形状**（「name 的唯一来源是 ccm 报的名字」），
        //   不是跟人斗智。真要绕过它得先把这段注释一起改掉，那已经是明知故犯了。
        assert!(
            !name_assigns[0].contains(';') && !name_assigns[0].contains("&&"),
            "给 `name` 赋值那行串了第二条命令 —— 那就分不清 `name` 到底来自哪一条了。实得：{}",
            name_assigns[0]
        );
        assert!(
            name_assigns[0].starts_with("name=$(")
                && name_assigns[0].contains("s/^ccm-session=//p"),
            "`name` 不是从 ccm 报的 `ccm-session=` 摘来的 —— 那它拿什么名字去登记总线？实得：{}",
            name_assigns[0]
        );
        // 读不到就停：`[ -n "$name" ] || { … exit 1; }`
        let bails = prod
            .lines()
            .any(|l| l.contains("[ -n \"$name\" ] ||") && l.contains("exit 1"));
        assert!(
            bails,
            "`cc-spawn` 拿不到会话名时没有停 —— 再往下 `cc-register` 与台账会写进空名字，\n                          产出一个总线上叫 \"\" 的幽灵（`cc-list` 显示在线、`cc-send` 石沉大海）。"
        );
    }

    /// ★ 撞名重试**只对撞名**〔08-13〕—— 别的失败重试多少次都一样，还会掩盖真错误。
    ///
    /// # 为什么这条只能用判据钉
    ///
    /// e2e 看不见它：`cc-spawn-uplift` 里唯一的失败用例是「ccm 版本太旧」，而那条在
    /// **调 ccm 之前**就被能力协商挡了 ⇒ 根本走不到重试循环。变异「什么错都重试」
    /// 在套件上**存活**（差别只剩 5 次重试带来的约 1 秒延迟）——**射程之内的诚实结果**，
    /// 不是判据漏了。⇒ 那条性质只能在这里钉形状。
    ///
    /// ⚠ 顺带钉**退出码的取法**：`if _out=$(cmd); then …; fi` 之后的 `$?` 是
    /// **if 语句自己的状态**（条件为假且无 else ⇒ 0），**不是 cmd 的退出码**。
    /// 08-13 首版就这么写 ⇒ 循环把撞名读成 0、当场 break ⇒ 并发下原本 3 个**响亮失败**
    /// 变成 3 个**假成功**（都报同一个名字，而会话只有 1 个）。**比修之前更坏。**
    #[test]
    fn the_spawn_retry_is_for_name_collisions_only() {
        let src = std::fs::read_to_string(cc_spawn_path()).expect("读 cc-spawn");
        let prod = shell_production(&src);
        // ① 退出码直接取，不经 `if`。
        guard_core::find_pinned(&prod, "&& _rc=0 || _rc=$?").unwrap_or_else(|e| {
            panic!("{e}\n⇒ 退出码没有直接取。`if cmd; then …; fi` 之后的 `$?` 是 if 自己的状态（0），\n                       那会把「撞名失败」读成成功 —— 08-13 实测因此产生过三个假成功。")
        });
        // ② 只对 3 重试；别的立刻停。
        guard_core::find_pinned(&prod, r#"[ "$_rc" = 3 ] || break"#).unwrap_or_else(|e| {
            panic!("{e}\n⇒ 重试不再只针对撞名（exit 3）。什么错都重试会**掩盖真错误**，\n                       而且重试多少次结果都一样（ccm 没装 / 参数错 / 预信任炸了）。")
        });
        // ③ 有上界（不许无限重试）。
        assert!(
            prod.contains("for _try in 1 2 3 4 5;"),
            "重试没有上界 —— 并发极密时会永远转下去，而那时该做的是**告诉用户**。"
        );
    }

    /// ★ `P4b①`③：`cc-spawn` **一件专属逻辑都不剩**了。
    ///
    /// `C15` 的验收就是这句话能不能说出口。三件（命名避让 / 总线登记 / 台账）搬完之后，
    /// 它剩下的只有 **cc-bus 的 id 规则**（`<basename>_cc`，`cc-whoami resolve` 与之对齐）
    /// 与参数转发 —— 那两件本来就该留在 cc-bus 这边。
    #[test]
    fn cc_spawn_no_longer_does_bus_bookkeeping_itself() {
        let src = std::fs::read_to_string(cc_spawn_path()).expect("读 cc-spawn");
        let prod = shell_production(&src);
        for gone in ["spawned.tsv", "cc-register", "list-panes"] {
            assert!(
                !prod.contains(gone),
                "`cc-spawn` 生产段里还有 {gone:?} —— `C15`〔用@08-13「cc-bus收进ccm」〕之后\n                              登记与台账归 `ccm --bus-register`。留在这里 = 两处各做一遍（还会各写一行）。"
            );
        }
        guard_core::find_pinned(&prod, "--detach --bus-register").unwrap_or_else(|e| {
            panic!("{e}\n⇒ `cc-spawn` 不再要求 ccm 做登记了 —— 那新会话就**不在总线上**：\n   `cc-list` 看不到它，`cc-send` 打过去石沉大海。")
        });
        // ⚠ `--bus-note` 必须是**条件给**的：`need_val` 拒收空串（带值旗标的统一纪律），
        //   无条件写 `--bus-note "$task"` 在**没有初始任务**时会让 ccm 当场 die。
        //   ★ 首版就是无条件的：单测全绿、`cc-spawn-uplift` 当场红四条（6/7/8 号不带任务）。
        //   判据钉得了「那个旗标在」，钉不了「不带任务时它还能跑」——**那一格只有 e2e 够得到**。
        guard_core::find_pinned(&prod, "[ -n \"$task\" ] && ccm_args+=(--bus-note").unwrap_or_else(
            |e| panic!("{e}\n⇒ `--bus-note` 又变成无条件给了。任务为空时 ccm 会 die「--bus-note 需要一个值」。"),
        );
    }
}
