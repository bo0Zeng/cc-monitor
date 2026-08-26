//! `K-W1A`（功能正文住 `plugin-split` 的 `EF02`/`EF03`）：**插件通用调用口**。
//!
//! 宿主怎么把一个外部可执行文件当插件使唤 —— 以及在使唤之前，怎么问清楚
//! 「你会不会我要的那几样」。
//!
//! # 这一层装什么、不装什么（**由两个真实实现反推，不是设计出来的**，`E9`）
//!
//! 反推样本是仓里今天真有的两处：`control/cc_bus.rs`（Rust，宿主调 cc-bus）与
//! `shared/cc-bus/scripts/cc-spawn`（shell，宿主调 ccm）。逐段量过之后：
//!
//! | 段 | 两处的形状 | 落在哪 |
//! |---|---|---|
//! | ① **找它** | 都有，但候选顺序与级数不同（4 级 vs 3 级），且中间那一级语义相反 | [`discover`]，**候选列表是参数** |
//! | ② **问它会什么** | ★ **只有一侧有** —— 这一段不是「抽」出来的，是**新造**的 | [`probe`] |
//! | ③ **传 argv 起它** | 都有 | [`invoke`]，**唯一一处起进程** |
//! | ④ **翻退出码** | 都有，但**语义互斥**（`3` 在一侧是「路由拒绝」、在另一侧是「撞名该重试」） | 只抽**骨架**，见下 |
//!
//! ⇒ ④ 这一段**刻意只抽骨架**：「怎么拿到码」「怎么认出被信号打断」「超时命令的那个码是多少」
//! 「摘一行诊断」。**码 → 语义的映射表必须每插件一份**，住在各自的适配代码里。
//! 把它抽上来就等于让宿主替所有插件解释它们的退出码 —— 那正是 `E6` 禁的那件事。
//!
//! # 三条硬形状，逐条写明是被哪条**机器判据**逼出来的（不是风格偏好）
//!
//! ## ① 候选列表、`PATH` 兜底与否、找不到时那句话的尾巴 —— 全是**参数**
//!
//! 两条独立的理由指向同一个结论：
//! - 反推读数：两个真实实现的查找顺序**级数不同、语义相反**（一个先找「装到用户盘上的位置」，
//!   一个先找「同仓相对路径」）⇒ 写死任何一份都是把某一个插件的处境刻进通用层；
//! - ★ 机器判据：本层进了 `agent_boundary_guard::CORE_FILES`，而那六根针里有 agent 的名字。
//!   cc-bus 的候选路径里**逐字带着那个名字**（`~/.<agent>/skills/...`）⇒ 它**编译不进这一层**。
//!   这条不是提醒，是**当场会红**的东西。
//!
//! ## ② 期限住**子进程**，本层一个计时器都没有
//!
//! `no_timer_guard` 禁 6 个构件，其中包含 `Duration::from_secs`；另一条判据钉住
//! 生产段 `Duration::from_*` 的处数**恰好等于**登记表条数。
//! ⇒ 本层的期限秒数是 `u64`，靠 `timeout(1)` 当 argv 前缀交给子进程，
//! **一个 `Duration` 都不出现**。找不到 `timeout(1)` 就**如实裸跑**（没有期限），
//! 这件事由 [`invoke::argv_for`] 的纯函数判据钉住 —— 不再只是一句头注。
//!
//! ## ③ 本层不许认识任何一个具体插件的词汇
//!
//! `cc_bus_boundary_guard` 的 5 根针是 **cc-bus 专有**的 ⇒ 对本层新造的插件面
//! **零覆盖**（那条判据自己的射程就只到 cc-bus 的数据布局）。
//! ⇒ 本层自己带一条 [`layer_guard`]：`plugin/` 的生产段里不许出现任何具体插件的
//! 子命令名 / 环境变量前缀 / 探测旗标。它有**阴性对照**（喂一段合成的违规样本必须命中），
//! 不然它就是个空真。
//!
//! # 方向（`layering_guard` 里有对应的机器判据，别只信这段散文）
//!
//! - `plugin → control` · `plugin → observe`：**一条都不许**。通用调用口一旦开始认识
//!   控制面或观测面，它就不再是「谁都能用的口」，而是 control 的一个私有助手。
//! - `control → plugin`：**逐条登记 + 条数钉死**，形状照 `ALLOWED_OBSERVE_TO_CONTROL`。
//!   不是禁绝（调用口本来就是给控制面用的），是**让每一条边都被人看见一次**。
//!
//! # 诚实边界（写在这里，因为它删了不会红）
//!
//! - [`probe`] 今天**零生产调用方**：本仓第一刀的那个插件（cc-bus）**没有 probe 口**，
//!   而「不改 cc-bus 本体」是 `EG1` 范围外逐字写明的事。⇒ 这一段的唯一行使者是判据。
//!   ⚠ 这不是仪式：判据钉的是**机制**（缺谁报谁、名字对不上就拒），
//!   而那个机制正是 `EF03` 要新造的东西。等哪天有插件真的说这套方言，它就地可用。
//! - 本层管不到**被起的那个进程自己**在干什么。它 `while true` 每秒跑一圈，
//!   本 crate 的零定时器护栏**一个字都看不见**（那条判据的主语是「daemon 自己的源码」，
//!   不是「daemon + 它起的插件」这个整体）。

pub(crate) mod discover;
pub(crate) mod invoke;

// ⚠ 今天零生产调用方 —— 理由与射程见本模块头注最后那一节。
// 判据（`probe::tests`）逐条行使它；`allow` 只压编译器的死代码提示，不是压判据。
#[allow(dead_code)]
pub(crate) mod probe;

/// ★ 本层自己的边界判据：**通用调用口不许认识任何一个具体插件**。
///
/// # 为什么要新写一条，而不是靠 `cc_bus_boundary_guard`
///
/// 那条判据的 5 根针是 **cc-bus 专有**的（`agents.tsv` / 收件箱 / 已读位置那一族），
/// 它守的是「daemon 不许绕到 cc-bus 背后读它的**数据文件**」。
/// 本层的病不是那个 —— 本层的病是**把某一个插件的词汇写进通用口**
/// （子命令名、环境变量前缀、探测旗标）。两条判据的**主语不同**，
/// 谁也替不了谁：那条针对新插件零覆盖，这条对数据布局零覆盖。
#[cfg(test)]
mod layer_guard {
    /// 具体插件的词汇 —— **运行时拼**（本文件的头注里就写着这些词，直接写字面量会读到判据自己）。
    fn concrete_plugin_words() -> Vec<(String, &'static str)> {
        vec![
            (format!("cc-{}", "list"), "某个插件的子命令名"),
            (format!("cc-{}", "send"), "某个插件的子命令名"),
            (format!("cc-{}", "kill"), "某个插件的子命令名"),
            (format!("CC_BUS{}", "_"), "某个插件的环境变量前缀"),
            (format!("cc{}-probe", "m"), "某个插件的探测旗标"),
            (format!("skil{}", "ls"), "某个插件的安装位置"),
        ]
    }

    /// 本层今天有哪几个 `.rs` —— ★ **逐个列名，不是数个数**。
    ///
    /// 「数个数」只挡得住「采集漏了」；列名还挡得住「新加了一个文件、而没人看见它」——
    /// 而本层的全部意义就是「这里面的东西必须是通用的」，多一个文件就该有人看一眼。
    const FILES_IN_THIS_LAYER: &[&str] = &["discover.rs", "invoke.rs", "mod.rs", "probe.rs"];

    /// 收集 `src/plugin/` 下每个 `.rs` 的 `(相对名, 生产段)`。
    ///
    /// ⚠ 走 `guard_core::scan_tree!` 而不是自己 `read_dir`：`scanning_guard_registry`
    /// 逐字要求新写的扫描型判据都走它（治「判据在自己的语料里找到自己 ⇒ 恒绿」那一族，
    /// 实测五次）。**代价如实写**：它按构造摘掉**调用者自己那份**，
    /// 所以下面扫到的集合里**没有 `mod.rs`**。
    /// 这一格今天不承重 —— `mod.rs` 的生产段就是那几行 `mod` 声明（本文件里除了它们全是
    /// 注释与 `#[cfg(test)]`），针藏不进去；但它是**真实的射程缺口**，写在这里别让人以为它全覆盖。
    fn plugin_sources() -> Vec<(String, String)> {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("src")
            .join("plugin");
        let mut out: Vec<(String, String)> = guard_core::scan_tree!(&root, &["rs"])
            .into_iter()
            .map(|(p, s)| {
                let rel = p
                    .strip_prefix(&root)
                    .unwrap_or(&p)
                    .to_string_lossy()
                    .replace('\\', "/");
                (rel, crate::guard_support::production_code(&s))
            })
            .collect();
        out.sort();
        out
    }

    /// 采集自检：**扫到的那几个文件，就是本层今天该有的那几个**。
    #[test]
    fn the_plugin_layer_collection_is_complete() {
        let files = plugin_sources();
        let mut got: Vec<String> = files.iter().map(|(n, _)| n.clone()).collect();
        // `scan_tree!` 按构造摘掉调用者自己 ⇒ 把本文件补回来再比（见 `plugin_sources` 头注）。
        got.push("mod.rs".to_string());
        got.sort();
        let mut want: Vec<String> = FILES_IN_THIS_LAYER.iter().map(|s| s.to_string()).collect();
        want.sort();
        assert_eq!(
            got, want,
            "plugin/ 里的文件与登记的对不上。\n\
             **多出来的**：新加进通用层的文件必须有人看一眼「它真的通用吗」——\
             把它加进 `FILES_IN_THIS_LAYER`，**同轮**加进 `agent_boundary_guard::CORE_FILES`\
             与它的棘轮表（不进那张表 ⇒ 「通用层不许认识 agent」对它是空的）。\n\
             **少了的**：文件没了就摘登记；也可能是采集坏了，那样下面那条对漏掉的是瞎的。"
        );
        for (name, prod) in &files {
            crate::guard_support::assert_no_test_code(&format!("plugin/{name}"), prod);
        }
    }

    /// ★ 正题：通用调用口的生产段里，一个具体插件的词都不许有。
    #[test]
    fn the_generic_port_names_no_concrete_plugin() {
        let mut hits: Vec<String> = Vec::new();
        for (name, prod) in plugin_sources() {
            for (no, line) in prod.lines().enumerate() {
                for (w, why) in concrete_plugin_words() {
                    if line.contains(&w) {
                        hits.push(format!("  plugin/{name}:{} [{why}] {}", no + 1, line.trim()));
                        break;
                    }
                }
            }
        }
        assert!(
            hits.is_empty(),
            "通用插件调用口里出现了**某一个具体插件**的词汇：\n{}\n\n\
             ⇒ 这一层只提供形状（找它 / 传 argv / 问能力 / 拿到码），\
             具体插件的子命令名、环境变量、安装位置、探测旗标一律走**参数**。\n\
             真要让通用口认识某个插件，先回 `E6` 说清为什么那件事不能由调用方传进来。",
            hits.join("\n")
        );
    }

    /// ★ 阴性对照：这条判据真的会咬人。
    ///
    /// 没有这一格的话，[`concrete_plugin_words`] 哪天被改成空 vec，正题那条会**安静地全绿**
    ///（本仓「负向断言没有输入就等于没有」已经踩过五次）。
    #[test]
    fn the_port_guard_actually_bites() {
        assert!(!concrete_plugin_words().is_empty(), "词表空了 ⇒ 正题恒绿");
        let synthetic = format!("    let bin = find(\"cc-{}\")?;", "list");
        let caught = concrete_plugin_words()
            .iter()
            .any(|(w, _)| synthetic.contains(w));
        assert!(
            caught,
            "合成的违规样本没被认出来 —— 正题那条此刻是空转的：{synthetic}"
        );
        // 反向：只是**前缀相同**的词不许误命中（误伤会训练人绕过判据）。
        let innocent = "    let names = candidates_for(name);";
        assert!(
            !concrete_plugin_words()
                .iter()
                .any(|(w, _)| innocent.contains(w)),
            "把通用写法误判成了插件词汇：{innocent}"
        );
    }
}
