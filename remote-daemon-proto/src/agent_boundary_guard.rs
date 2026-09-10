//! `S1`：**通用层不许知道任何一个 agent 的名字与文件格式**〔用@08-14〕。
//!
//! # 它守的是什么
//!
//! 用户 08-14 逐字：「**现在的daemon几乎都是兼容claudecode, 那就标清楚, 分离清楚,
//! 后面搞兼容其他agent的时候才方便**」。
//!
//! daemon 今天 21021 行里，agent 专属的知识（哪个目录、什么文件格式、账号叫什么）
//! **散在每一层**（Bx 实测：顶层 30 · `observe/` 111 · `control/` 23 · `platform/` 1 · `common/` 2）。
//! 目标形态是「通用内核 + 每个 agent 一份适配」，而**没有判据的重构会被稀释** ——
//! 搬完当天干净，下一个人往 `watcher.rs` 里加一行 `claude_dir.join("projects")` 就回去了，
//! 而**没有任何东西会红**。
//!
//! # 人群：**逐文件登记**，不是目录规则
//!
//! Bx 摸底否掉了「`observe/` 脏、其余干净」这个想法：`platform/` 与 `common/` 也有命中。
//! ⇒ 谁是"通用层"由 [`CORE_FILES`] 逐条列举。默认**不在表里的文件不扫** ——
//! 这不是漏洞，是**分阶段搬迁的把手**：`S2`/`S3` 每搬完一块，就把那块加进表里，
//! 判据的覆盖面**只增不减**（`the_core_file_list_only_grows` 钉住）。
//!
//! # 针：6 个，逐条判过，**不一把梭**
//!
//! | 针 | 为什么是它 |
//! |---|---|
//! | agent 名字 ×2 | 最强信号 |
//! | 目录/格式知识 ×4 | 「会话住哪个目录、什么后缀、账号变量叫什么」正是要赶出通用层的东西 |
//!
//! ⚠ **`@ccm_sid` 刻意不是针**：它是**我们自己**在 tmux 上设的变量（`control/` 里 10 处，
//! 是 §34 三道门用的），与 agent 无关。一把梭会把它误伤成"耦合"，
//! 而误伤会训练人绕过判据 —— 那比没有判据更坏（skill 铁律 18）。
//!
//! 注：本模块整体在 `#[cfg(test)]` 内，非测试构建为空。

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    /// **已经宣称"通用"的文件**（相对 `src/`）。不在表里的今天不扫。
    ///
    /// ⚠ 这张表是**搬迁进度表**，不是白名单：`S2`/`S3` 每把一块的 agent 知识搬进
    /// `agents/*`，就把那块加进来。**只增不减**（下面那条判据钉住）。
    ///
    /// 今天先放 Bx 实测**最干净**的两层 —— 它们的命中数分别是 1 和 2，
    /// 是"随手就能清掉"的量级；先从它们开始，让判据从第一天就在真实地守着东西，
    /// 而不是挂一个全红的清单等人来看（全红 = 没人看 = 等于没有）。
    const CORE_FILES: &[&str] = &[
        "platform/mod.rs",
        "platform/pidwatch/mod.rs",
        "platform/fallback_guard.rs",
        "common/mod.rs",
        "wire.rs",
        "inbound.rs",
        // ── 以下由 `S3` 加入 ──────────────────────────────────────────
        // `platform/` 整层：`S3` 把 `proc_claude_config_dir` 参数化成 `proc_env_var(pid, name)`
        // 之后，这一层再没有任何一个 agent 的名字。⚠ 这是**整层**进表，不是挑干净的进 ——
        // 「唯一允许平台原语的层」恰恰最不该认识某个 agent 叫什么。
        "platform/proc.rs",
        "platform/liveness.rs",
        "platform/paths.rs",
        "platform/signal.rs",
        // `common/` 整层：`S3` 把 `projects_root` 搬进适配层之后 `common/paths.rs` 整个文件消失了
        //（它当初就违反 `common/` 三条门槛的第③条「无域知识」——`projects` 是 Claude 的布局）。
        "common/fs.rs",
        // ── `S4b`〔08-14〕：**这一轮一个都没加，而这是量出来的，不是忘了** ────────────
        //
        // `S4`/`L1` 都写着「`claude_dir` 改完名，`watcher`/`accounts_query`/`history_query`
        // 就能进表」。`S4b` 改完名之后**实测这三个文件**，那句话**不成立**：
        // 六根针下还剩 **6 / 5 / 5** 处（共 16），其中
        //   · **12 处是 `crate::agents::claudecode::…`** —— 适配层的**地址**本身
        //     （`history_query` 那 5 处全是；它的 `claude` 一词只剩这一种来源）；
        //   · 4 处是散文（`watcher` 两句 warn 里的 `claude` 一词、`accounts_query` 两处
        //     `sessions/` 文案）。
        //
        // ⇒ **刻意不放宽针去凑这三个**。`S3` 设计的中间态（机器留在通用层、知识住适配层）
        // 让通用层用「一次函数调用」拿知识 —— 而那次调用**写死了一个 agent 的名字**，
        // 它是真耦合，不是假阳：加第三个 agent 时这 12 处每一处都要回来看。
        // 把它豁免掉，本判据的正题就从「通用层不认识 agent」滑成「通用层不认识 agent 的**目录布局**」。
        //
        // 卡点因此从 `S4b` 移交 `L2`/`S6`（立接口）。那 12 处 + 另外 15 处同族已由
        // `agent_locality_guard::ADAPTER_CALL_SITES` **逐条登记**（共 8 文件 / 27 处），
        // 收进接口一处、那张表短一条、这里就能进一个文件。
        //
        // ── `K-W1A`〔08-26〕：**新立的 `plugin/` 整层进表** ─────────────────────
        //
        // ★ 这一批与上面几批**性质不同**：上面是「把已有文件搬干净之后收进来」，
        // 这一批是**新造的层从第一天就进表**。理由是这条判据的一个结构性质：
        // `CORE_FILES` 是 **opt-in** 的，不在表里的文件**不扫** ⇒ 新造一个自称「通用」的层
        // 而不进表，「通用层不许知道任何一个 agent 的名字」这条性质对它就是**空的**，
        // 而且**没有任何机检能逼人加**（`KY6` 逐字）。
        //
        // ⇒ 进表不是形式：它当场约束了接口形状。第一刀那个插件的候选安装路径里逐字带着
        // agent 的名字，所以**候选列表只能是入参**，不能住进 `plugin/` —— 这条判据是把它
        // 挡在门外的那道门（另一条独立理由是两个真实实现的查找顺序本来就不同）。
        "plugin/mod.rs",
        "plugin/discover.rs",
        "plugin/invoke.rs",
        "plugin/probe.rs",
        // ── 以下由 `K-R12` 下一拍加入（09-04）──────────────────────────────
        // 新建的 `common/tmux_utf8.rs`（「tmux 的打印通道必须是 UTF-8」那个口径的家），
        // 同 `plugin/` 那一批：**新造的东西从第一天就进表**，理由一样 ——
        // 本表是 opt-in 的，不在表里就**不扫**，而「没人逼你加」正是 `KY6` 那个洞。
        //
        // 现打（09-04，量于 `058a2aa`，口径比本判据更宽：整份文件、大小写不敏感）：
        // 六根针在那个文件里**逐针 0 命中**；同一把尺子打 `observe/watcher.rs` 作**非空对照**
        // 得 57 / 3 / 13 / 12 / 34 / 1 ⇒ 尺子不是坏的，那个 0 是真的 0。
        "common/tmux_utf8.rs",
    ];

    /// 人群下界：低于它说明取法坏了（路径写错 / 扩展名过滤掉）⇒ **红**，不是绿。
    ///
    /// ⚠ `S3` 把它从 5 抬到 10：**下界必须跟着覆盖面涨**，否则搬进来一批之后
    /// 「路径全写错」这种坏法仍然过得去（剩 5 个也满足旧下界）。
    ///
    /// ⚠ `K-W1A`（08-26）把它从 10 抬到 **14**：同一条纪律 —— `plugin/` 四个文件进表之后
    /// 表长 15，下界若还停在 10，「`plugin/` 整个目录路径写错」这种坏法仍然过得去
    ///（剩 11 个也满足旧下界）。抬到 14 之后余量是 1。
    ///
    /// ⚠ `K-R12` 下一拍（09-04）把它从 14 抬到 **15**：同一条纪律，不是顺手改数 ——
    /// `common/tmux_utf8.rs` 进表之后表长 16，下界若还停在 14，
    /// 「`common/` 两个文件的路径一起写错」这种坏法仍然过得去（剩 14 个也满足旧下界）。
    /// 抬到 15 之后余量仍是 1。
    const CORE_FILES_FLOOR: usize = 15;

    /// **已知欠账**（`文件:行内容片段`, 归哪一件, 为什么今天不动它）—— **递减棘轮**。
    ///
    /// 立表当天判据红出的就是这两条，它们**正是 `D3` 预言的那处**：协议把 agent 名字
    /// 焊进了字段名。改它要 bump 协议版本、两侧同改、动 `protocol_doc_guard` 的对拍
    /// ⇒ 归 `S4`，本件不顺手改（一功能一 commit）。
    ///
    /// 三条纪律，缺一条这张表就会变成许愿池：
    /// ① **命中不在表里 ⇒ 红**（新增违规立刻现形）；
    /// ② **表里的条目不再命中 ⇒ 也红**（修好了必须摘登记，否则表会攒成幽灵）；
    /// ③ 所以它**只会缩短**，不会变长 —— `S4` 落地那天这张表清零。
    ///
    /// ★ **`S4` 已清零**，两条各自的去向不一样，记在这里免得下一个人以为是漏删：
    /// - `codex_dir`（连同 `kinds`）：**从协议里删掉了**。它们从来没上过线
    ///   （`main.rs` 一直硬写 `None` / 空 ⇒ `skip_serializing_if` 省略），删是零代价 ——
    ///   `D3` 逐字「越晚改越贵（今天两个字段，将来五个）」，而没人收到过的**现在**最便宜。
    /// - `claude_dir`：**删不掉**（真在线上、有仓外消费方）⇒ 它不在本表里，
    ///   而是进了下面那张**性质相反**的 `FROZEN_COMPAT`。
    ///
    /// ⚠ 空表**不等于判据失效**：正题那条「命中不在表里 ⇒ 红」照常生效，
    /// 只是从今天起**一条欠账都不该有**。空表下再冒出一处，会直接落进 `unknown` 报红。
    const KNOWN_DEBT: &[(&str, &str, &str)] = &[];

    /// **冻结兼容**（`文件`, `行内容片段`, 为什么清不掉, **解锁条件**）—— 与上表**性质相反**。
    ///
    /// `KNOWN_DEBT` 是「欠着、要还」，本表是「**还不了，且知道为什么、什么时候能还**」。
    /// 两张表分开是 `S4` 刻意做的：**把清不掉的东西塞进要清零的表，那张表就永远清不了零，
    /// 递减棘轮从此失去意义**（`S3` 在 `agent_locality_guard::NOT_AGENT_KNOWLEDGE` 上
    /// 立过同一条分界：那张表与欠账表性质相反，进去的东西永远留着）。
    ///
    /// ⚠ 但本表与 `NOT_AGENT_KNOWLEDGE` 仍有一处关键不同，别照抄：
    /// 那张装的是**假阳**（判据看走眼了，那本来就不是 agent 知识）⇒ 永远留着；
    /// 本表装的是**真阳**（`claude_dir` 确实把 agent 名焊进了字段名，`D3` 说得没错）⇒
    /// 只是**今天动不了**。所以它**必须带解锁条件** ——
    /// 没有解锁条件的「冻结」只是「永久豁免」的好听说法
    /// （`every_frozen_compat_entry_states_how_it_gets_unfrozen` 钉住这条）。
    ///
    /// ⚠〔`S4b`〕本表与 `agent_locality_guard::AGENT_NAMED_WIRE_FIELDS` **在 `wire.rs` 这一行
    /// 重叠，但不是重复登记，别合并**：本表的作用域是「**已宣称通用的文件**（[`CORE_FILES`]）里的
    /// agent **字面量**」，那张是「**整棵 `src/`** 里 `<名>_dir` 这一形的**标识符**」。
    /// 证据是：那张表逼出了 `control/resolve_query.rs::ResumeSpec.claude_dir`（同样是冻结的
    /// 线上字段名），而**本表看不见它** —— 那个文件不在 `CORE_FILES` 里。
    /// ⇒ 两个不同的问题各问了一次同一个事实，删掉任一张都会漏掉另一张管的那一半。
    const FROZEN_COMPAT: &[(&str, &str, &str, &str)] = &[(
        "wire.rs",
        "claude_dir",
        "hello 帧里**今天真的在线上**的那个目录字段，而且**有仓外消费方**：aterm 的契约 \
         2026-07-18 冻结，我们改不动它。⇒ 改名是破坏性变更，不是本区能单方面做的事。 \
         `D3` 自己已经给了出路，逐字：「改协议要 bump `PROTO_VERSION` **或走 additive \
         迁移**」——`S4` 走 additive：新字段 `homes`（`[{agent_kind, path}]`）承载 agent \
         维度、agent 名只出现在**值**里，`claude_dir` 原地不动 ⇒ 线上字节零变化。",
        "monitor 与 aterm **都**改读 `homes` 之后。monitor 那半 `S4` 已经做完 \
         （`ssh_source.rs` 的 hello 解析已是「优先 `homes`、回退 `claude_dir`」）⇒ \
         **只剩仓外 aterm 这一个卡点**，我们这边不欠。那天把这个字段从 `wire.rs` 删掉，\
         本条同轮摘登记（下面的幽灵检查会逼着摘）。",
    )];

    fn src_dir() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("src")
    }

    /// 针 —— **运行时拼**。
    ///
    /// 本文件的头注里写着这些词（它得解释自己在防什么），直接写字面量会**读到判据自己**。
    /// 本仓栽过四次（`P4b §6` 那几条头注逐条记着）。
    fn needles() -> Vec<(String, &'static str)> {
        vec![
            (format!("clau{}", "de"), "agent 名字"),
            (format!("cod{}", "ex"), "agent 名字"),
            (format!("{}jsonl", "."), "会话文件后缀"),
            (format!("proje{}", "cts/"), "会话目录布局"),
            (format!("sessi{}", "ons/"), "会话目录布局"),
            (format!("CLAUDE_CONFIG{}", "_DIR"), "账号环境变量"),
        ]
    }

    /// 一行是不是**真代码**（剥 `//` 注释）。
    ///
    /// ⚠ 只剥注释、**不剥字符串**：通用层里出现 `"claude"` 这个字符串字面量，
    /// 那正是要抓的东西（比如 `if kind == "claude"`）。
    fn production_lines(src: &str) -> Vec<(usize, String)> {
        crate::guard_support::production_code(src)
            .lines()
            .enumerate()
            .map(|(i, l)| (i + 1, l.to_string()))
            .filter(|(_, l)| !l.trim().is_empty())
            .collect()
    }

    /// ★ 正题：**已宣称通用的文件里，一个 agent 专属字面量都不许有**。
    #[test]
    fn no_agent_specific_literal_in_the_core_layer() {
        let root = src_dir();
        let mut scanned = 0usize;
        // `文件:行号:命中的词:那一行` —— `S1-Y3` 要的是**逐条可核**，不是一个总数：
        // 只报数字的话，`S2`/`S3` 搬的时候没法对照，人会去改数字而不是搬代码。
        let mut hits: Vec<String> = Vec::new();
        for rel in CORE_FILES {
            let p = root.join(rel);
            let Ok(src) = std::fs::read_to_string(&p) else {
                continue; // 文件被挪走 ⇒ 下面的下界断言会红，比在这里 panic 诊断更清楚
            };
            scanned += 1;
            for (no, line) in production_lines(&src) {
                let low = line.to_lowercase();
                for (n, why) in needles() {
                    if low.contains(&n.to_lowercase()) {
                        hits.push(format!("  {rel}:{no}  [{why}] {}", line.trim()));
                        break; // 一行只报一次，免得同一行刷屏
                    }
                }
            }
        }
        assert!(
            scanned >= CORE_FILES_FLOOR,
            "只扫到 {scanned} 个通用层文件（下界 {CORE_FILES_FLOOR}）—— \
             取法坏了（路径写错？文件挪走了？），本断言此刻是**空转**的"
        );
        // 登记过的挑出来（欠账 `KNOWN_DEBT` + 冻结兼容 `FROZEN_COMPAT`）；
        // 剩下的才是**新增违规**。⚠ 两张表**都**要参与，但它们性质相反：
        // 前者要清零，后者带解锁条件、清不掉（分表的理由见两张表各自的头注）。
        let (known, unknown): (Vec<String>, Vec<String>) = hits.into_iter().partition(|h| {
            KNOWN_DEBT
                .iter()
                .any(|(f, frag, _)| h.contains(f) && h.contains(frag))
                || FROZEN_COMPAT
                    .iter()
                    .any(|(f, frag, _, _)| h.contains(f) && h.contains(frag))
        });
        // ★ 棘轮的第二条：**修好了必须摘登记**。否则表会攒成幽灵，
        //   而幽灵条目会让下一个人以为"这里还欠着 / 这里还冻着"，进而不敢动。
        //   ⚠ 两张表同一条纪律 —— `FROZEN_COMPAT` 也会幽灵化（字段真删了却忘了摘）。
        let mut ghosts: Vec<String> = KNOWN_DEBT
            .iter()
            .filter(|(f, frag, _)| !known.iter().any(|h| h.contains(f) && h.contains(frag)))
            .map(|(f, frag, _)| format!("KNOWN_DEBT {f}:{frag}"))
            .collect();
        ghosts.extend(
            FROZEN_COMPAT
                .iter()
                .filter(|(f, frag, _, _)| !known.iter().any(|h| h.contains(f) && h.contains(frag)))
                .map(|(f, frag, _, _)| format!("FROZEN_COMPAT {f}:{frag}")),
        );
        assert!(
            ghosts.is_empty(),
            "这些登记条目**已经不再命中**了：{ghosts:?}\n\
             ⇒ 修好了就把它从表里摘掉（棘轮只许缩短）。留着 = 让下一个人以为这儿还欠着。"
        );
        assert!(
            unknown.is_empty(),
            "通用层里出现了**新的** agent 专属字面量（{} 处）：\n{}\n\n\
             ⇒ 通用层不许知道任何一个 agent 的名字、目录布局或文件格式。\n\
             正确做法是把这段搬进 `agents/<名>/`，让通用层只认抽象（`agent_kind` 那类**值**）。\n\
             ⚠ 实在搬不动的，**先别改这条判据** —— 先在功能件里写清为什么，再谈例外。",
            unknown.len(),
            unknown.join("\n")
        );
    }

    /// ★ 覆盖面**只增不减**：这张表是搬迁进度表。
    ///
    /// 没有这条的话，最省事的"修法"是把红了的文件从表里删掉 ——
    /// 那正是本仓一路在防的「改数字了事」（`readonly_guard` 用 `assert_eq!` 而不是地板，同理）。
    #[test]
    fn the_core_file_list_only_grows() {
        // **曾经被宣称为通用层的每一个文件**。加进 `CORE_FILES` 的同一轮要追加到这里。
        //
        // ⚠ 写两遍是**刻意的**：它让"把某个文件移出通用层"必须是一个**显式动作**
        //（同 `KNOWN_DEBT` 要手动摘登记）。
        //
        // ★ 这张表原名 `AT_BIRTH`，只装 `S1` 立表当天那 6 个 —— 于是 `S3` 后来加进
        // `CORE_FILES` 的 5 个**可以被静默删掉**（6 个元老还在、长度也够，两条断言全绿）。
        // **棘轮只棘到出生线，不棘到今天。** 这个洞是 `S3` 的变异台架逮出来的
        //（M3「把 platform/proc.rs 摘掉」原本零红），不是谁记起来的。
        const EVER_DECLARED_CORE: &[&str] = &[
            // S1 立表当天
            "platform/mod.rs",
            "platform/pidwatch/mod.rs",
            "platform/fallback_guard.rs",
            "common/mod.rs",
            "wire.rs",
            "inbound.rs",
            // S3 加入
            "platform/proc.rs",
            "platform/liveness.rs",
            "platform/paths.rs",
            "platform/signal.rs",
            "common/fs.rs",
            // K-W1A 加入（08-26）：新立的 `plugin/` 整层，从第一天就在表里
            "plugin/mod.rs",
            "plugin/discover.rs",
            "plugin/invoke.rs",
            "plugin/probe.rs",
            // K-R12 下一拍加入（09-04）：新建的 `common/tmux_utf8.rs`，同样从第一天就在表里
            "common/tmux_utf8.rs",
        ];
        let missing: Vec<&str> = EVER_DECLARED_CORE
            .iter()
            .filter(|f| !CORE_FILES.contains(f))
            .copied()
            .collect();
        assert!(
            missing.is_empty(),
            "这些文件被从通用层清单里**删掉了**：{missing:?}\n\
             ⇒ 删表是把判据的覆盖面缩小，等于把红变绿而问题还在。\n\
             真要移出（比如那个文件确实成了 agent 专属），在功能件里写清楚再动这条断言。"
        );
        assert_eq!(
            CORE_FILES.len(),
            EVER_DECLARED_CORE.len(),
            "\n`CORE_FILES` 与棘轮表长度对不上（{} vs {}）。\n\
             多出来 ⇒ 有人加了通用层文件却没追加进棘轮表，那一条从此可以被静默删掉；\n\
             少了 ⇒ 上面那条会先红。**两张表必须同轮改**。",
            CORE_FILES.len(),
            EVER_DECLARED_CORE.len()
        );
    }

    /// ★ 冻结兼容**必须带解锁条件**，而且**只许缩短**。
    ///
    /// 没有这一条的话，`FROZEN_COMPAT` 就是 `KNOWN_DEBT` 的一个逃生舱：
    /// 清不掉的往里一放，欠账表当场清零、看起来很干净，而问题原封不动
    /// —— 那正是 `S4` 分这两张表要防的事，不是要造的事。
    ///
    /// 两条纪律：
    /// ① 每条都要有**非空的解锁条件**（没有解锁条件的「冻结」= 「永久豁免」的好听说法）；
    /// ② 条数**有天花板**且只许降 —— 否则「加第三个 `<名>_dir` 字段」只要顺手登记一条
    ///    就能过，而那恰恰是 `D3` 排除掉的那条路。
    #[test]
    fn every_frozen_compat_entry_states_how_it_gets_unfrozen() {
        /// 今天恰好 1 条（`wire.rs` 的 `claude_dir`）。**只许降不许升**。
        const FROZEN_COMPAT_CEILING: usize = 1;
        assert!(
            FROZEN_COMPAT.len() <= FROZEN_COMPAT_CEILING,
            "`FROZEN_COMPAT` 涨到 {} 条了（天花板 {FROZEN_COMPAT_CEILING}）。\n\
             这张表**只许缩短**：它装的是「真违规但今天动不了」，多一条就是多欠一笔。\n\
             ⚠ 如果你正在加第二个 `<名>_dir` 字段 —— 那正是 `D3` 逐字排除掉的那条路，\n\
             终点是 hello 里五个并列的目录字段。往 `homes` 里加一项，不要加字段。",
            FROZEN_COMPAT.len()
        );
        for (file, frag, why, unlock) in FROZEN_COMPAT {
            assert!(
                !why.trim().is_empty(),
                "`FROZEN_COMPAT` 的 {file}:{frag} 没写「为什么清不掉」"
            );
            assert!(
                unlock.trim().len() >= 20,
                "`FROZEN_COMPAT` 的 {file}:{frag} 没写**解锁条件**（实得 {} 字节）。\n\
                 没有解锁条件的冻结就是永久豁免 —— 那把这张表和白名单变成同一个东西。\n\
                 要写的是「什么条件满足之后这一条就能删」，不是「为什么现在不能删」。",
                unlock.trim().len()
            );
        }
    }

    /// ★ 反向夹具：**这条判据真的会红**。
    ///
    /// 没有这一格的话，`needles()` 哪天被改坏（比如返回空 vec），正题那条会**安静地全绿**。
    #[test]
    fn the_detector_itself_catches_a_synthetic_violation() {
        let bad = "fn f() {\n    let p = home.join(\".claude\").join(\"projects\");\n}\n";
        let lines = production_lines(bad);
        let found = lines.iter().any(|(_, l)| {
            let low = l.to_lowercase();
            needles()
                .iter()
                .any(|(n, _)| low.contains(&n.to_lowercase()))
        });
        assert!(found, "判定认不出合成的违规样本 —— 正题那条此刻是空转的");
        assert!(!needles().is_empty(), "针表空了 ⇒ 正题恒绿");

        // 注释里的写法**不算**（判据的语料不许混进散文）
        let only_comment = "// 这里以前写的是 claude_dir.join(\"projects\")\nfn g() {}\n";
        let clean = production_lines(only_comment);
        let hit_in_comment = clean.iter().any(|(_, l)| {
            let low = l.to_lowercase();
            needles()
                .iter()
                .any(|(n, _)| low.contains(&n.to_lowercase()))
        });
        assert!(!hit_in_comment, "注释里的写法被当成了真代码");

        // ⚠ `@ccm_sid` **刻意不是针**：它是我们自己的 tmux 变量，不是任何 agent 的。
        let ours = "fn h() {\n    tmux(\"show-options\", \"@ccm_sid\");\n}\n";
        let ours_hit = production_lines(&ours.to_string()).iter().any(|(_, l)| {
            let low = l.to_lowercase();
            needles()
                .iter()
                .any(|(n, _)| low.contains(&n.to_lowercase()))
        });
        assert!(
            !ours_hit,
            "把我们自己的 `@ccm_sid` 误判成 agent 耦合了 —— 误伤会训练人绕过判据"
        );
    }
}
