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
//! ★ **这一句此前只是散文，删了不会红**。D1（08-26）的硬读数：把 `control/cc_bus.rs`
//! 那张码表原样抬进本层、**只擦掉插件的名字** ⇒ `436 passed; 0 failed`，**一条不红**。
//! ⇒ 现在它由 [`layer_guard`] 里的两条判据钉着，两条认的都是**形状**、不认插件的名字：
//! `the_generic_port_does_not_translate_exit_codes`（`Some(<整数字面量>)` / 裸整数 `match` 臂）
//! 与 `the_only_exit_code_constants_here_are_the_registered_generic_ones`（本层的 `i32`
//! 常量必须**逐条登记**）。⚠ 它们各自**认不出什么**，逐条写在自己的头注里 —— 别读成全覆盖。
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
//! ## ③ 本层不许认识任何一个具体插件 —— **三条判据，射程各不相同，逐条写明**
//!
//! `cc_bus_boundary_guard` 的 5 根针是 **cc-bus 专有**的 ⇒ 对本层新造的插件面
//! **零覆盖**（那条判据自己的射程就只到 cc-bus 的数据布局）。
//! ⇒ 本层自己带 [`layer_guard`]，里面是**主语不同的三条**：
//!
//! | 判据 | 它认的是 | 今天的分母 | ⚠ 它认不出的 |
//! |---|---|---|---|
//! | `the_generic_port_names_no_concrete_plugin` | **词汇** | [`layer_guard::concrete_plugin_words`] 那张表（成员与条数**只住那一处**，这里不复述 —— 复述一份就会漂） | ★ **它只认表里那几族词**。09-04 加了第二族（代码全景）之后，仍然**认不出第三族** —— 谁再加一个插件，就得在同一轮加上它自己那组词（`E6`） |
//! | `the_generic_port_does_not_translate_exit_codes` | **形状**：`Some(<整数字面量>)` / 裸整数 `match` 臂 | 2 根形状针，**不挑插件** | 具名常量写的表 · 非 `i32` 的码 · 运行期从数据里读的表 · `mod.rs` 本身 |
//! | `the_only_exit_code_constants_here_are_the_registered_generic_ones` | **登记**：本层的 `i32` 常量恰好是登记的那几个 | 登记表今天 **1** 条（`TIMED_OUT_CODE`） | 非 `i32` 类型的码常量 · `mod.rs` 本身 |
//!
//! ★ 第一行那句「换一族词就认不出」是 **D1（08-26）逼出来的**：本段此前**只**为
//! `cc_bus_boundary_guard` 写下过这条限制，**没为自己写** —— 它重犯了自己刚批评过的那个形状。
//! 三条都有**阴性对照**（喂合成的违规样本必须命中），不然就是空真。
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
/// 本层的病不是那个 —— 本层的病是**把某一个插件的语义写进通用口**。
/// 两条判据的**主语不同**，谁也替不了谁：那条针对新插件零覆盖，这条对数据布局零覆盖。
///
/// # ★★ 本模块里是**两族**判据，别把它们读成一件事（D1 08-26 逼出来的分工）
///
/// | 族 | 判据 | 它认的是 | 换一个插件还认得出吗 |
/// |---|---|---|---|
/// | **词汇** | [`concrete_plugin_words`] 那一条 | 某一个插件**写下的名字** | ⛔ **认不出**（针是那一族的专有词） |
/// | **形状** | 退出码那两条 | 「把退出码翻成语义」这件事**本身的形状** | ✅ 认得出（不挑插件），但另有它认不出的形（各自头注里逐条写着） |
///
/// 为什么非得有形状那一族：D1 的硬读数 —— 把 `control/cc_bus.rs` 的码表原样抬进本层、
/// **只擦掉插件的名字**，词汇那一族 `436 passed; 0 failed` **一条不红**。
/// ⇒ 词汇针守的是「有没有写下那个插件的名字」，**不是**「那张码表在哪一层」。
#[cfg(test)]
mod layer_guard {
    /// 具体插件的词汇 —— **运行时拼**（本文件的头注里就写着这些词，直接写字面量会读到判据自己）。
    ///
    /// ⚠ **这张表就是词汇那一族的全部分母。** 条数与成员**只住这里一处** ——
    /// 散文里不复述（复述一份就会漂；报错文案里那个数是 `words.len()` **现算**的）。
    ///
    /// 今天表里是**两族**词：总线那一个插件族，与代码全景那一个插件族。
    /// ⇒ **仍然不是「所有插件的所有词」** —— 本件自己那个假插件（`plugin_walk_fixture`）
    /// 换的又是第三族词，这张表对它**一根都对不上**，那一半由它自带的一组专有针管
    /// （`E6` 逐字：每加一个插件要加它自己那组专有针，不许用一根通用针盖住所有插件）。
    fn concrete_plugin_words() -> Vec<(String, &'static str)> {
        vec![
            // ── 第一族：总线那个插件（`K-W1A` 08-26 立表时的全部内容）──────────
            (format!("cc-{}", "list"), "某个插件的子命令名"),
            (format!("cc-{}", "send"), "某个插件的子命令名"),
            (format!("cc-{}", "kill"), "某个插件的子命令名"),
            (format!("CC_BUS{}", "_"), "某个插件的环境变量前缀"),
            (format!("cc{}-probe", "m"), "某个插件的探测旗标"),
            (format!("skil{}", "ls"), "某个插件的安装位置"),
            // ── 第二族：代码全景那个插件（词由 `K-W2D` 09-04 量出，`K-W2E` 落）─────
            //
            // ★ 为什么非加不可（这不是补充，是**补一个空洞**）：上面那 6 根全是**同一个**
            //   插件族的词，而本条的正题是「通用口不许认识**任何一个**具体插件」。
            //   `K-W2D` 实测（`K-W2E` 复打）：往 `discover::find` 头上塞一个那一族的字面量，
            //   **一条判据都不红** —— 也就是说本条此前对第二个插件是**空的**，
            //   而它自己的头注早就把这件事写下来了（写下来的边界不是判据，只有判据是判据）。
            //
            // ⚠ 刻意**不加**两样，各有理由：
            //   · 那个引擎的**环境变量前缀** —— 它今天 `env::var` 零命中，加了是一根空针；
            //   · **中文那两个字** —— 本文件头注里自带三处，判据会**读到自己**（本仓那族病）。
            (format!("code-{}", "picture"), "另一个插件的二进制名"),
            (format!("cod{}picture", "e"), "另一个插件的数据目录名"),
            (format!("panora{}", "ma"), "另一个插件的命令族前缀"),
            (format!("code_picture{}", "_core"), "另一个插件的库符号"),
            (format!("index{}db", "."), "另一个插件的落盘布局"),
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
    ///
    /// # ⚠ 本条今天的**射程** —— 比判据名小得多，读之前先看这一段
    ///
    /// **分母 = [`concrete_plugin_words`] 那张表**（条数与成员只住那一处，这里不复述；
    /// 报错文案里那个数是 `words.len()` **现算**的）。它**不是**「所有插件的所有词」。
    /// ⇒ ★ **换一族词汇的插件它一个都认不出来。**
    ///
    /// 「一个具体插件的词都不许有」这句话今天**只兑现到表里那几族词所属的那几个插件**。
    /// 这正是本文件为 `cc_bus_boundary_guard` 写下过的那条限制 —— 它对本条**同样成立**，
    /// 此前只写了别人的、没写自己的（D1 08-26 逮到：本条重犯了它自己刚批评过的形状）。
    ///
    /// ★ 09-04（`K-W2D` 量词 / `K-W2E` 落）：表里加了**第二族**。
    /// 加之前那一族**一根都对不上** —— 实测：往 `discover::find` 头上塞一个那一族的字面量，
    /// **一条判据都不红**。⇒ 上面那句「换一族就认不出来」**不是提醒，是当时的读数**。
    /// 而它今天**仍然成立**（对第三族），所以这一段一个字都不许删。
    ///
    /// ⇒ 「换一族词也拦得住」的那一半**不在本条**，在形状那一族：
    /// `the_generic_port_does_not_translate_exit_codes` 与
    /// `the_only_exit_code_constants_here_are_the_registered_generic_ones`。
    #[test]
    fn the_generic_port_names_no_concrete_plugin() {
        let words = concrete_plugin_words();
        let mut hits: Vec<String> = Vec::new();
        for (name, prod) in plugin_sources() {
            for (no, line) in prod.lines().enumerate() {
                for (w, why) in &words {
                    if line.contains(w.as_str()) {
                        hits.push(format!(
                            "  plugin/{name}:{} [{why}] {}",
                            no + 1,
                            line.trim()
                        ));
                        break;
                    }
                }
            }
        }
        let scope: Vec<&str> = words.iter().map(|(w, _)| w.as_str()).collect();
        assert!(
            hits.is_empty(),
            "通用插件调用口里出现了**某一个具体插件**的词汇：\n{hits}\n\n\
             ⇒ 这一层只提供形状（找它 / 传 argv / 问能力 / 拿到码），\
             具体插件的子命令名、环境变量、安装位置、探测旗标一律走**参数**。\n\
             真要让通用口认识某个插件，先回 `E6` 说清为什么那件事不能由调用方传进来。\n\n\
             ⚠ **本条的射程**：今天认的就是下面这 {n} 个词，**分母就是这 {n} 根针** —— \
             换一族词汇的插件它一个都认不出来。\n  {scope}\n\
             「换一族词也拦得住」的那一半归形状那两条\
             （`the_generic_port_does_not_translate_exit_codes` / \
             `the_only_exit_code_constants_here_are_the_registered_generic_ones`）。",
            hits = hits.join("\n"),
            n = words.len(),
            scope = scope.join(" · "),
        );
    }

    /// ★ 阴性对照（**词汇那一族**的）：那条判据真的会咬人。
    ///
    /// 没有这一格的话，[`concrete_plugin_words`] 哪天被改成空 vec，正题那条会**安静地全绿**
    ///（本仓「负向断言没有输入就等于没有」已经踩过五次）。
    ///
    /// ⚠ 它证的是「表里那几族针认得出用**那几族词**写的违规样本」，
    /// **不是**「本层认得出任何插件」—— 形状那一族的阴性对照在
    /// [`the_exit_code_shape_guard_actually_bites`]。
    ///
    /// ★ **每一族各喂一个样本**（09-04 加第二族时同轮加的）：只喂一族的话，
    /// 另一族的针整族失效也被这一族盖住 —— 本仓「N 个独立源只断一次」踩过。
    #[test]
    fn the_port_guard_actually_bites() {
        assert!(!concrete_plugin_words().is_empty(), "词表空了 ⇒ 正题恒绿");
        // 一族一个样本：总线那族 · 代码全景那族。**新加一族词就在这里加一行样本。**
        for (synthetic, whose) in [
            (
                format!("    let bin = find(\"cc-{}\")?;", "list"),
                "总线那一族",
            ),
            (
                format!("    let bin = find(\"cod{}picture\")?;", "e"),
                "代码全景那一族",
            ),
        ] {
            let caught = concrete_plugin_words()
                .iter()
                .any(|(w, _)| synthetic.contains(w));
            assert!(
                caught,
                "{whose}的合成违规样本没被认出来 —— 那一族的针此刻是空转的：{synthetic}"
            );
        }
        // 反向：只是**前缀相同**的词不许误命中（误伤会训练人绕过判据）。
        let innocent = "    let names = candidates_for(name);";
        assert!(
            !concrete_plugin_words()
                .iter()
                .any(|(w, _)| innocent.contains(w)),
            "把通用写法误判成了插件词汇：{innocent}"
        );
    }

    // ───────────────────────────────────────────────────────────────────────
    // 环境那一族〔`K-R26` 09-05〕：**「哪些键交给子进程」只许有一个家**
    //
    // 病历住 `invoke::INHERITED_ENV_KEYS` 的头注（本层此前不清环境 ⇒ 子进程继承 daemon
    // 的整份环境，常驻监听口的地址与令牌跟着漏过去）。本族守的是**修法不会悄悄散开**：
    // 白名单散成两三处之后，「不在表里的键到不了子进程」这句话就再也没人说得准。
    // 形状照 `common::tmux_utf8` 那条「家唯一」：**正面钉家在**、**反面钉别处零处**、
    // 再加一格阴性对照（喂真会违规的输入给判据的核）。
    // ───────────────────────────────────────────────────────────────────────

    /// 本层**动子进程环境**的三种调用形。⚠ 三者两两不含（`.env_clear(` 与 `.envs(`
    /// 都不含 `.env(`），所以下面那张普查表不会重复计数。
    const ENV_CALL_SHAPES: &[&str] = &[".env_clear(", ".env(", ".envs("];

    /// 今天的**登记面**：`(文件, 调用形, 处数)`。三处，全在同一个函数里。
    ///
    /// · `.env_clear(` 一处 —— 那一刀本身；
    /// · `.env(` 两处 —— ① 按白名单逐键喂 · ② 调用方**显式交办**的那几项
    ///   （次序承重：② 排在后面 ⇒ 显式压过继承）。
    ///
    /// 🔴 **加第四处之前先回答一句**：它是在给「继承」这一侧再开一个口子吗？
    /// 是的话，正解是往 [`super::invoke::INHERITED_ENV_KEYS`] 里加一条并写为什么，
    /// **不是**在别处再写一段 `.env(`。
    const ENV_CALL_SITES: &[(&str, &str, usize)] =
        &[("invoke.rs", ".env(", 2), ("invoke.rs", ".env_clear(", 1)];

    /// 判据的**核**：数出本层每个文件里各种「动子进程环境」的调用形各几处。
    ///
    /// 抽出来的理由同 `tmux_utf8::second_homes`：正题与阴性对照必须经由**同一个**函数，
    /// 夹具另写一份扫描证明的是那一份。
    fn env_call_census(files: &[(String, String)]) -> Vec<(String, String, usize)> {
        let mut out: Vec<(String, String, usize)> = Vec::new();
        for (name, prod) in files {
            for shape in ENV_CALL_SHAPES {
                let n = prod.matches(shape).count();
                if n > 0 {
                    out.push((name.clone(), (*shape).to_string(), n));
                }
            }
        }
        out.sort();
        out
    }

    /// ★★ **白名单只有一个家，而那个家是 `invoke.rs` 里那个具名常量。**
    ///
    /// 三半都断言，缺一半就只买到一部分：
    /// - **家在**：`const INHERITED_ENV_KEYS:` 在 `invoke.rs` 的生产段里**恰好一处**
    ///   （没有这一半，把家删掉之后「别处零处」照样成立，本条会绿着报「只有一个家」）；
    /// - **别处零处**：本层其余文件里一处都没有；
    /// - **调用面登记**：动子进程环境的调用形逐处对账 —— 多一处就得有人来回答一句。
    ///
    /// ⚠ **射程**（[`plugin_sources`] 的构造性缺口，这里不重复理由）：采集面里**没有
    /// `mod.rs`**（`scan_tree!` 按构造摘掉调用者自己）⇒ 有人把第二份白名单写进 `mod.rs`
    /// 的生产段，本条看不见。今天 `mod.rs` 的生产段就是那几行 `mod` 声明，藏不进东西，
    /// 但那是**事实**不是**判据**。
    #[test]
    fn the_child_environment_allowlist_has_exactly_one_home() {
        let files = plugin_sources();
        assert!(
            files.len() >= 3,
            "本层只采到 {} 个 .rs —— 采集坏了，下面每一句都在空转",
            files.len()
        );
        let bytes: usize = files.iter().map(|(_, c)| c.len()).sum();
        assert!(
            bytes >= 5_000,
            "本层生产段只有 {bytes} 字节 —— 剥过头了，本条此刻在空转"
        );

        // ── ① 家在，而且在 `invoke.rs` ────────────────────────────────────
        // 锚点收在 `:` 上（非标识符字符）：改了名的 `INHERITED_ENV_KEYSX` 不许命中
        //（`tmux_utf8::kou_jing_homes` 头注逐字记着那条被前缀撑大的活体）。
        let anchor = format!("const INHERITED_ENV_{}:", "KEYS");
        let home = files
            .iter()
            .find(|(n, _)| n == "invoke.rs")
            .map(|(_, p)| p.clone())
            .unwrap_or_else(|| panic!("采集面里没有 `invoke.rs` —— 采集坏了"));
        guard_core::find_pinned(&home, &anchor).unwrap_or_else(|e| {
            panic!(
                "子进程环境白名单的家不在 `invoke.rs` 里（或有两处）：{e}\n\
                 家没了 ⇒ 「不在表里的键到不了子进程」这句话从此没有住址；\n\
                 家有两处 ⇒ 两份靠人对齐，而漂开的那天没有任何东西会红。"
            )
        });

        // ── ② 别处零处 ────────────────────────────────────────────────────
        let dup: Vec<String> = files
            .iter()
            .filter(|(n, _)| n != "invoke.rs")
            .filter(|(_, p)| p.contains(&anchor))
            .map(|(n, _)| n.clone())
            .collect();
        assert!(
            dup.is_empty(),
            "子进程环境白名单在本层有**第二个家**：{dup:?}\n\
             正解是 `use`/引用 `invoke::INHERITED_ENV_KEYS`，不是再声明一份。"
        );

        // ── ③ 动环境的调用面逐处对账 ──────────────────────────────────────
        let got = env_call_census(&files);
        let want: Vec<(String, String, usize)> = {
            let mut w: Vec<(String, String, usize)> = ENV_CALL_SITES
                .iter()
                .map(|(f, s, n)| ((*f).to_string(), (*s).to_string(), *n))
                .collect();
            w.sort();
            w
        };
        assert_eq!(
            got, want,
            "本层「动子进程环境」的调用面与登记的对不上。\n\
             **多出来的**：先回答一句「它是不是在给**继承**这一侧再开一个口子」——\
             是的话，正解是往 `invoke::INHERITED_ENV_KEYS` 里加一条并写为什么。\n\
             **少了的**：`.env_clear(` 掉了 ⇒ 那条暗路又开了（子进程重新继承整份环境）；\
             白名单那一处 `.env(` 掉了 ⇒ 白名单成了摆设，子进程连 `PATH` 都没有。\n\
             ⚠ 本条的采集面里**没有 `mod.rs`**（见本判据头注的射程段）。"
        );

        // ── ④ 阴性对照：普查的核**真的会咬人** ────────────────────────────
        //    夹具的文件名取中性名，断言只认调用形（来自内容），不认路径〔`brief` `6g`〕。
        for shape in ENV_CALL_SHAPES {
            let planted = vec![("a/b.rs".to_string(), format!("    cmd{shape}x);\n"))];
            assert!(
                !env_call_census(&planted).is_empty(),
                "有人在别处写了 `{shape}`，而普查的核没出声 —— 它此刻是空转的"
            );
        }
        // 反向：只是**恰好含同一个前缀**的通用写法不许误命中（误伤会训练人绕过判据）。
        let innocent = "    let env = envelope(&args);\n    cmd.args(&argv);\n";
        assert!(
            env_call_census(&[("a/b.rs".to_string(), innocent.to_string())]).is_empty(),
            "把通用写法误判成了动环境的调用：{innocent}"
        );
    }

    // ───────────────────────────────────────────────────────────────────────
    // 形状那一族：**「把退出码翻成语义」这件事不许住在通用层**（`E6` 的硬核那一条）
    //
    // 它与上面那一族的分工，以及各自认不出什么，写在本模块头注那张表里。
    // ⚠ 为什么这一族必须存在：D1（08-26）把 `control/cc_bus.rs` 的码表原样抬进本层、
    //   只擦掉插件的名字 ⇒ 词汇那一族 `436 passed; 0 failed`，**一条不红**。
    // ───────────────────────────────────────────────────────────────────────

    /// 本层**允许**存在的退出码常量 —— 今天**恰好一条**。
    ///
    /// 它凭什么是通用的：那是**期限命令自己**超时时用的码，
    /// 与「被调的是哪个插件」无关；换任何插件它都是同一个数。
    ///
    /// ⚠ 这张表是一道**登记面**，不是一道过滤器：`plugin/` 生产段里每多一个 `i32` 常量，
    /// 就必须有人当场回答一次「这个码是**所有插件共有**的，还是**某一个插件**的语义？」。
    /// 后者一律住回那个插件自己的适配代码（`E6`），**不许**往这张表里加。
    const EXIT_CODE_CONSTS_IN_THIS_LAYER: &[&str] = &["TIMED_OUT_CODE"];

    /// 一行里有没有「`Some(` 紧跟一个整数字面量」这一形。
    ///
    /// 为什么盯这一形：本层的退出码类型是 `Option<i32>`（`invoke::Done::code`），
    /// 所以「把某个具体的码翻成语义」在 Rust 里落地时几乎必然写成
    /// `match code { Some(<数>) => … }` 或 `code == Some(<数>)`。
    /// ★ **它不认插件的名字，只认这个形状** ⇒ 换谁的码表抬上来都一样命中。
    fn some_of_int_literal(line: &str) -> bool {
        let tag = "Some(";
        let mut rest = line;
        while let Some(i) = rest.find(tag) {
            let tail = &rest[i + tag.len()..];
            let t = tail.trim_start();
            let t = t.strip_prefix('-').unwrap_or(t);
            if t.starts_with(|c: char| c.is_ascii_digit()) {
                return true;
            }
            rest = tail;
        }
        false
    }

    /// 一行里有没有「整数字面量当 `match` 臂」这一形（`3 => …` / `2 | 3 => …`）。
    ///
    /// 它补 [`some_of_int_literal`] 的一个缺口：码被 `unwrap_or` 剥成裸 `i32` 之后，
    /// 同一张表会写成 `match code.unwrap_or(-1) { 2 => …, 3 => … }`，一个 `Some` 都没有。
    fn bare_int_match_arm(line: &str) -> bool {
        let t = line.trim_start();
        let t = t.strip_prefix('-').unwrap_or(t);
        let digits = t.chars().take_while(|c| c.is_ascii_digit()).count();
        if digits == 0 {
            return false;
        }
        let after = t[digits..].trim_start();
        after.starts_with("=>") || (after.starts_with('|') && t.contains("=>"))
    }

    /// 这一行定义的**退出码常量**叫什么（`const` / `static`，且类型里出现 `i32`）。
    ///
    /// 只盯 `i32` 是**刻意收窄**：本层真会有别的整数常量（长度上限、级数之类），
    /// 把它们一并拦下只会训练人绕过判据。退出码在本层的类型恰恰就是 `i32`。
    fn exit_code_const_name(line: &str) -> Option<String> {
        let t = line.trim_start();
        // 可见性修饰（`pub` / `pub(crate)` / `pub(super)` …）先剥掉。
        let t = if t.starts_with("pub") {
            t.find(' ').map(|i| t[i + 1..].trim_start())?
        } else {
            t
        };
        let rest = t
            .strip_prefix("const ")
            .or_else(|| t.strip_prefix("static "))?;
        let (name, ty) = rest.split_once(':')?;
        if !ty.split('=').next()?.contains("i32") {
            return None;
        }
        Some(name.trim().to_string())
    }

    /// ★ 正题（**形状**，不认词）：通用调用口不许把退出码翻成语义。
    ///
    /// 守的是件文件 `§0a-3` 里最硬的那一条：**码 → 语义的映射必须每插件一份**
    ///（同一个 `3` 在一侧是「路由层拒绝」、在另一侧是「撞名该重试」，两个语义互斥）。
    /// 把它抽上来 = 让宿主替所有插件解释它们的退出码 = `E6` 禁的那件事。
    ///
    /// # ⚠ 本条的射程（★ 拿本轮的病理回头扫自己：**这几根针也有它认不出的东西**）
    ///
    /// 它认的是**今天这个仓、Rust 写法下**「码 → 语义」的两种落地形，
    /// 分母 = [`some_of_int_literal`] + [`bare_int_match_arm`] 这 **2 根形状针**。
    /// **认不出**的至少有这几样，逐条写下来，别让人把它读成全覆盖：
    ///
    /// - 码表**不写整数字面量**，改用 `use` 进来的具名常量。这一半有两道旁证但**都不完整**：
    ///   常量若定义在本层 ⇒ [`the_only_exit_code_constants_here_are_the_registered_generic_ones`]
    ///   会红；若从 `control` / `observe` 引进来 ⇒ `layering_guard` 会红。
    ///   ⛔ 两道都不覆盖的缝：常量来自 `plugin/` 之外、又不在那两层里。
    /// - 码的类型不是 `i32`（`u8` / `Option<u8>` / 字符串码）。
    /// - 表是**运行期**从数据里读出来的（`HashMap` / 切片 / 配置文件），源码里一个数字都没有。
    /// - `mod.rs` 本身不在扫描面里 —— [`plugin_sources`] 头注那条构造性缺口，本条**照单继承**。
    #[test]
    fn the_generic_port_does_not_translate_exit_codes() {
        let mut hits: Vec<String> = Vec::new();
        for (name, prod) in plugin_sources() {
            for (no, line) in prod.lines().enumerate() {
                let why = if some_of_int_literal(line) {
                    "把一个具体的退出码写死进了 `Some(…)`"
                } else if bare_int_match_arm(line) {
                    "拿整数字面量当 `match` 臂 —— 那就是一张码表"
                } else {
                    continue;
                };
                hits.push(format!(
                    "  plugin/{name}:{} [{why}] {}",
                    no + 1,
                    line.trim()
                ));
            }
        }
        assert!(
            hits.is_empty(),
            "通用调用口里出现了「**把退出码翻成语义**」的形状：\n{hits}\n\n\
             ⇒ 码 → 语义的映射**必须每插件一份**，住在那个插件自己的适配代码里\
             （同一个数在两个插件那里语义互斥 —— 抬上来就是让宿主替所有插件解释退出码，`E6` 禁的那件事）。\n\
             本层只提供骨架：怎么拿到码 · 怎么认出被信号打断 · 期限命令超时用的那个码 · 摘一行诊断。\n\n\
             ⚠ **本条的射程**：它认的是 2 根**形状**针（`Some(<整数字面量>)` / 裸整数 `match` 臂），\
             不认插件的名字 —— 具名常量写的表、非 `i32` 的码、运行期读出来的表它都认不出，\
             逐条见本判据头注。\n\
             真要在本层留一个数，先把它写成常量并登记进 `EXIT_CODE_CONSTS_IN_THIS_LAYER`\
             （那张表逼你当场回答「它对所有插件都一样吗」）。",
            hits = hits.join("\n"),
        );
    }

    /// ★ 正题（**登记**）：本层的 `i32` 常量恰好是登记过的那几个。
    ///
    /// 它堵的是上一条最容易被绕开的那条路：把码表改写成
    /// `const ROUTER_REJECTED: i32 = 3;` + `Some(ROUTER_REJECTED) => …` ——
    /// 一个整数字面量都不出现在 `match` 里，形状针**看不见**。
    ///
    /// ⚠ 射程：只认**定义在 `plugin/` 生产段里**、且类型里带 `i32` 的常量。
    /// 从别处 `use` 进来的常量不在本条视野内（那一半归 `layering_guard`，且它也有缝 ——
    /// 见 [`the_generic_port_does_not_translate_exit_codes`] 头注那一条）。
    /// `mod.rs` 同样不在扫描面里（[`plugin_sources`] 的构造性缺口）。
    #[test]
    fn the_only_exit_code_constants_here_are_the_registered_generic_ones() {
        let mut got: Vec<String> = Vec::new();
        for (_, prod) in plugin_sources() {
            for line in prod.lines() {
                if let Some(n) = exit_code_const_name(line) {
                    got.push(n);
                }
            }
        }
        got.sort();
        let mut want: Vec<String> = EXIT_CODE_CONSTS_IN_THIS_LAYER
            .iter()
            .map(|s| (*s).to_string())
            .collect();
        want.sort();
        assert_eq!(
            got, want,
            "本层的退出码常量与登记的对不上。\n\
             **多出来的**：先回答「这个码对**所有**插件都是同一个意思吗」——\
             不是的话它是某一个插件的语义，住回那个插件自己的适配代码（`E6`），别加进登记表。\n\
             **少了的**：常量没了就摘登记；也可能是采集坏了，那样本条与形状那条都在空转。"
        );
    }

    /// ★ 阴性对照（**形状那一族**的）：**3 根针各单断一格，外加一格全断**。
    ///
    /// 为什么要一格一格断：3 根针是 **3 个独立源**，只喂一个样本的话，
    /// 任何一根悄悄失效都被另外两根盖住（本仓「N 个独立源只断一次」踩过）。
    /// 最后那一格喂的是**活体夹具** —— D1-L2b 那一刀的原样样本（一张擦掉了名字的真码表）。
    #[test]
    fn the_exit_code_shape_guard_actually_bites() {
        // ① 针一单断：`Some(<数>)` —— `match` 臂与 `==` 比较两种落地都要认得出。
        assert!(
            some_of_int_literal("        Some(3) => Err(rejected(detail)),"),
            "针一瞎了：`Some(<数>)` 当 match 臂没认出来"
        );
        assert!(
            some_of_int_literal("        if out.code == Some( 2 ) {"),
            "针一瞎了：`== Some(<数>)` 没认出来"
        );

        // ② 针二单断：裸整数臂。
        assert!(
            bare_int_match_arm("            2 => Err(bad_args(detail)),"),
            "针二瞎了：裸整数 match 臂没认出来"
        );
        assert!(
            bare_int_match_arm("            2 | 3 => Err(bad_args(detail)),"),
            "针二瞎了：多值裸整数臂没认出来"
        );

        // ③ 针三单断：登记面。
        assert!(
            !EXIT_CODE_CONSTS_IN_THIS_LAYER.is_empty(),
            "登记表空了 ⇒ 那条相等断言会把「本层一个码常量都没有」也判绿"
        );
        assert_eq!(
            exit_code_const_name("pub(crate) const ROUTER_REJECTED: i32 = 3;").as_deref(),
            Some("ROUTER_REJECTED"),
            "针三瞎了：新加的 i32 码常量没被采到"
        );
        assert_eq!(
            exit_code_const_name("const RETRY_LIMIT: usize = 3;"),
            None,
            "针三误伤了非退出码的整数常量 —— 那会训练人绕过判据"
        );

        // ④ 全断（活体夹具）：D1-L2b 抬上来的那张表，名字已全部擦掉。
        //    「擦掉名字」正是词汇那一族看不见它的原因 ⇒ 这一格证的是形状针替补上了。
        let lifted = r#"
pub(crate) fn classify(code: Option<i32>, detail: &str) -> Result<(), (String, String)> {
    match code {
        Some(0) => Ok(()),
        Some(2) => Err(("invalid_args".to_string(), detail.to_string())),
        Some(3) => Err(("rejected".to_string(), detail.to_string())),
        Some(c) => Err(("failed".to_string(), format!("退出码 {c}"))),
        None => Err(("failed".to_string(), detail.to_string())),
    }
}"#;
        let caught = lifted
            .lines()
            .filter(|l| some_of_int_literal(l) || bare_int_match_arm(l))
            .count();
        assert!(
            caught >= 3,
            "一张擦掉了名字的真码表只被认出 {caught} 行 —— 形状那两条此刻在空转"
        );
        // 而词汇那一族对同一份样本**确实是瞎的** —— 这一格把那句诚实话钉成读数。
        assert!(
            !concrete_plugin_words()
                .iter()
                .any(|(w, _)| lifted.contains(w.as_str())),
            "词汇针在这份样本上命中了 ⇒ 样本没擦干净，这一格证不了形状针的必要性"
        );

        // ⑤ 反向：今天生产段里那几行合法写法，一根针都不许命中。
        for innocent in [
            "pub(crate) const TIMED_OUT_CODE: i32 = 124;",
            "        self.code == Some(TIMED_OUT_CODE)",
            "        md.permissions().mode() & 0o111 != 0",
            "            code: out.status.code(),",
            "            vec![deadline_secs.to_string(), bin.display().to_string()],",
        ] {
            assert!(
                !some_of_int_literal(innocent) && !bare_int_match_arm(innocent),
                "误伤了一行合法写法：{innocent}"
            );
        }
    }
}
