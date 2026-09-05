//! `P7c-2` 第一刀：**让「索引引擎住哪一侧」变成可换的**〔用@08-13〕。
//!
//! # 它守的是用户那条约束
//!
//! 〔用 08-13〕逐字：「**此外daemon的实现记得解耦清晰. 如果以后要改索引方式以及解析方式
//! 或者添加新语言才方便**」。
//!
//! `P7c-2 §1b` 量完之后这条被**缩窄过**，如实记着：三轴里只有**索引方式**（查询面）
//! 归我们管；解析方式与加新语言住在 `vendor/code-picture-core`，而 `C7` 逐字
//! 「**vendor code-picture-core 不动**」+ vendor 自己的 SS-10 铁律「副本是上游的镜子，
//! 不是分身」⇒ 那两轴的可改性由「改上游 → re-vendor」的流程决定，不由这一层的接口形状决定。
//!
//! ⇒ 本模块只钉**我们真管得着的那一轴**，且钉的是两条**今天已经成立**的性质
//! （`P7c-2 §4` 的读数）——第一刀因此不是重构，是**把成立的东西变成会红的判据**。
//!
//! # 两条性质
//!
//! ① **命令面只暴露查询语义**：那 20 多条 tauri 命令的签名里不许出现
//!    存储 / grammar / 解析开关这类实现细节。换引擎那天，协议一个字不用动。
//! ② **引擎取用口恰好一处**：`Engine::open` 在生产段恰好一次。
//!    换侧（内嵌 / 侧车 / 编进 daemon）要改的就是那一处。
//!
//! ★ ② **今天就在防一件事**，不只是为了以后：`panorama.rs` 自己的注释记着
//! 「rusqlite 连接对同一 `index.db` 并发写 → `SQLITE_BUSY` + 缓存不一致」。
//! **第二处 `Engine::open` = 第二条连接。**
//!
//! # ⚠ ② 的作用域〔`K-R2` 09-04 扩〕：**每一棵我们编的树，不只 monitor 这一棵**
//!
//! 下面 `mod tests` 里那两条判据的人群都是本 crate 的 `src` —— 也就是 monitor 那一棵树。
//! 而本件要做的事恰恰是**换侧**（解析搬到代码所在地）⇒ 换侧当天对面那棵树里的取用口
//! 它一处都看不见，「恰好一处」会在那一天悄悄退化成「monitor 内部的局部真理」，而判据全绿。
//! ⇒ [`engine_port_scope`] 把这条性质的人群扩到 **monitor · daemon · 共享 crate** 三棵树，
//! 而**树的那份名单自己也被钉住**（再多一棵就出声）。
//!
//! ★ 旧的那两条**一个字没动**，两者钉的不是同一件事：它们钉「那一处在 `panorama.rs` 里、
//! 且在那份文件里恰好一次」，新模块钉「**全体**只有那一处」。合起来才是「全体恰好一次」。
//!
//! # ⚠ 只扫签名，不扫注释
//!
//! 注释里写清楚「底下是 SQLite / 走 tree-sitter」是**好事** —— 那是给人看的实现说明。
//! 泄漏指的是**接口形状**里出现实现细节。⇒ 判据先剥注释再看。
//!
//! 注：本模块整体在 `#[cfg(test)]` 内，非测试构建为空。

#[cfg(test)]
mod tests {
    const PANORAMA: &str = include_str!("panorama.rs");

    /// 只留生产段（剥 `//` 注释与 `#[cfg(test)] mod`）。
    fn prod() -> String {
        guard_core::production_code(PANORAMA)
    }

    /// 取所有 tauri 命令属性之后那条函数签名的**文本**（到第一个 `{` 为止）。
    ///
    /// ⚠ 取的是**签名**不是整个函数体：函数体里出现 `SELECT` 之类的字样，
    /// 说明这一层在自己拼 SQL —— 那是另一条病（今天不存在），
    /// 而本条钉的是**接口形状**。两件事不要混在一条判据里。
    /// tauri 命令属性的字面写法 —— **运行时拼**。
    ///
    /// ⚠ 血的教训（08-13 当场踩到，本会话第三次同族）：这个字面量直接写在本文件里，
    /// 会**污染另一条判据的语料** —— `src/ipc/commands.vitest.ts` 扫全仓 `.rs` 找
    /// 「属性 + 紧随其后的 fn 名」当命令名，于是把**我的测试函数名**抠成了一条命令，
    /// 报「声明了却没注册 ⇒ 前端调不到」。判据的针不只会读到自己，**还会喂给别人**。
    fn cmd_attr() -> String {
        format!("#[tauri::{}]", "command")
    }

    fn command_signatures(prod: &str) -> Vec<String> {
        let attr = cmd_attr();
        let mut out = Vec::new();
        let mut from = 0usize;
        while let Some(rel) = prod[from..].find(attr.as_str()) {
            let at = from + rel;
            let tail = &prod[at..];
            let brace = tail.find('{').unwrap_or(tail.len());
            out.push(tail[..brace].to_string());
            from = at + attr.len();
        }
        out
    }

    /// `P7c2-Y1`：**命令面只暴露查询语义**。
    #[test]
    fn the_panorama_command_surface_leaks_no_storage_or_parser_detail() {
        let prod = prod();
        let sigs = command_signatures(&prod);
        // 反向自检：抽取坏了的话下面的空集会"恰好通过"。
        assert!(
            sigs.len() >= 20,
            "只抽到 {} 条命令签名 —— 抽取坏了，本断言在空转",
            sigs.len()
        );
        // ⚠ 针**运行时拼**：本文件的头注里就写着这些词，写字面量会读到判据自己
        //（本仓栽过四次，见 `P4b §6` 那几条头注）。
        let banned = [
            format!("sql{}", "ite"),
            format!("SEL{}", "ECT"),
            format!("rus{}qlite", ""),
            format!("tree{}sitter", "_"),
            format!("gram{}mar", ""),
            format!("index{}db", "."),
        ];
        let mut hits: Vec<String> = Vec::new();
        for sig in &sigs {
            let low = sig.to_lowercase();
            for b in &banned {
                if low.contains(&b.to_lowercase()) {
                    hits.push(format!("{b} 出现在签名：{}", sig.replace('\n', " ").trim()));
                }
            }
        }
        assert!(
            hits.is_empty(),
            "全景的命令面泄漏了实现细节：{hits:?}\n\
             ⇒ 用户 08-13 逐字要的是「以后要改索引方式…才方便」。协议里一旦出现存储/解析细节，\n\
             换引擎那天就要改协议，而协议的对面是**别人的代码**。\n\
             要拿的东西查询语义表达不了时，正确做法是**加一条查询语义的命令**，\n\
             不是把底下的实现漏上来。"
        );
    }

    /// `P7c2-Y1` 的另一半：**新增命令必须来这里被看一眼**。
    ///
    /// 禁词表挡不住「用一个中性名字包装一个泄漏的口」（如 `panorama_raw_query`）。
    /// 条数钉住之后，加命令的人必然会撞到本条，那时才有机会问一句「它是查询语义吗」。
    #[test]
    fn adding_a_panorama_command_forces_a_look_at_this_seam() {
        // ⚠ **21 不是 22**：裸 grep 数到 22，其中一处命令属性写在注释里
        //（`production_code` 剥掉了它）。判据数的是**生产段**，两个数不一样是对的。
        const COMMANDS_TODAY: usize = 21;
        let n = prod().matches(cmd_attr().as_str()).count();
        assert_eq!(
            n, COMMANDS_TODAY,
            "全景命令数从 {COMMANDS_TODAY} 变成了 {n}。\n\
             **这不是要你改数字了事** —— 先回答：新加的那条是**查询语义**吗？\n\
             （`overview`/`node`/`callers`/`impact` 那样，说的是「代码里有什么」，\n\
             而不是「存储里怎么放的」。）是，就把数字改了；不是，就换个问法。"
        );
    }

    /// `P7c2-Y2`：**引擎取用口恰好一处**。
    #[test]
    fn the_engine_is_opened_in_exactly_one_place() {
        let prod = prod();
        let needle = format!("Engine{}open", "::");
        guard_core::find_pinned(&prod, &needle).unwrap_or_else(|e| {
            panic!(
                "`{needle}` 在生产段不是恰好一处：{e}\n\
                 ⇒ 第二处 = 第二条 rusqlite 连接。`panorama.rs` 自己的注释记着那条真事故：\n\
                 「对同一 index.db 并发写 → SQLITE_BUSY + 缓存不一致」。\n\
                 而且换引擎（内嵌 / 侧车 / 编进 daemon）那天要改的就是这一处 —— 多一处多一份漏改。"
            )
        });
    }

    /// `P7c2-Y2` 的射程：vendor 引擎**只被 `panorama.rs` 导入**。
    ///
    /// 上一条只看 `panorama.rs` 自己。若别的模块也拿到 `Engine`，
    /// 「恰好一处」就只是本文件内的局部真理。
    #[test]
    fn the_engine_type_does_not_escape_the_panorama_module() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut elsewhere: Vec<String> = Vec::new();
        for (path, src) in guard_core::scan_tree!(&root, &["rs"]) {
            let rel = path
                .strip_prefix(&root)
                .unwrap_or(&path)
                .to_string_lossy()
                .replace('\\', "/");
            if rel == "panorama.rs" {
                continue;
            }
            let p = guard_core::production_code(&src);
            // ⚠ 钉的是**导入**，不是"文件里出现过 Engine 这个词"。
            //   第一版用整词匹配 `Engine`，当场误伤 `tool_registry.rs` —— 那里的 `Engine`
            //   在一段**字符串字面量**里（给人看的文案），`production_code` 不剥字符串。
            //   ⇒ 类型要跑出去，必然先出现在 `use` 上。钉那一处，既准又没有假阳性。
            if guard_core::contains_word(&p, &format!("code_picture{}core", "_")) {
                elsewhere.push(rel);
            }
        }
        assert!(
            elsewhere.is_empty(),
            "`Engine` 跑出了 panorama 模块：{elsewhere:?}\n\
             ⇒ 那样「取用口恰好一处」就只是 panorama.rs 内部的局部真理，\n\
             换侧那天要追着改的地方不止一处。要用引擎请走 `panorama.rs` 里那个池子。"
        );
    }
}

/// 〔`K-R2` 09-04〕**把「引擎取用口恰好一处」的人群扩到每一棵我们编的树。**
///
/// # 它补的洞是上一拍自己逮到的，不是设想
///
/// [`tests::the_engine_is_opened_in_exactly_one_place`] 只读 `panorama.rs` 一份；
/// [`tests::the_engine_type_does_not_escape_the_panorama_module`] 只走本 crate 的 `src`。
/// 两条的人群都是 monitor 那一棵树，而本件的正题就是**换侧**
/// ⇒ 换侧当天 daemon 树里的取用口一处也看不见。
/// 🔴 而「恰好一处」是搬家安全性的**承重条件**：第二处取用口 = 第二条 SQLite 连接
/// （`panorama.rs` 自己的注释记着那条真事故）。**承重条件的量具比事实小**，
/// 是本工作区最高频的那一族。
///
/// # 作用域怎么读到另一棵树（写清，别让下一个人猜）
///
/// `env!("CARGO_MANIFEST_DIR")` 是**本 crate 的清单目录**（也就是 `src-tauri`），
/// **只跳一级**就是仓根 —— 那是 cargo 给的事实，不是猜的。
/// 〔`K-R13` 治过「跳两级」那种病：主树上算出来的东西恰好存在、工作树上算错了
/// 却被人补了一个假落点 ⇒ 判据变绿而它证明的事根本不成立。〕
/// 每一棵树在 [`TREES`] 里按**仓根相对路径**登记，判据自己去 `join`。
///
/// # 读不到那棵树时它说什么：**「判不了」，不是 0**
///
/// [`tree_dir`] 先判目录在不在，不在就 `Err` 并把它找的那个住址印出来；
/// [`corpus_of`] 再压一道**文件数地板**，语料塌成空集时同样 `Err`。
/// 两道都不许静默返回「零命中」——**一棵读不到的树与一棵干净的树，在终端上一模一样。**
///
/// # 它买不到什么（逐条写明，别读大一格）
///
/// - **针带词边界**（`guard_core::contains_word`）⇒ 换个名字取用（同族的 `open_at` 那种写法）
///   它认不出来。它挡的是「多一处同名取用」，不是「改名逃逸」。
/// - **只到「哪一份文件里有」这一格**：本模块钉命中的**住址表**，
///   「那一份文件里恰好一次」由上面那条 `find_pinned` 钉 —— 两条合起来才是「全体恰好一次」。
/// - **一棵谁的清单都没引的树，它看不见**（将来单独构建的那棵侧车树就是这一形）。
///   那一格归 `K-W2D` 的 `KW2D4`，形状取决于 `KW2D1` 选哪种，本拍**不猜**。
/// - 本文件自己不在语料里（`scan_tree!` 按构造摘除调用者）⇒ 头注里那几处提及不会被数进来；
///   代价是**本文件的生产段没人扫**，而它整份都在 `#[cfg(test)]` 与 `//!` 里，扫也是空的。
#[cfg(test)]
mod engine_port_scope {
    /// 这条性质的人群：**我们编的每一棵源码树**，按仓根相对路径登记。
    ///
    /// `(标签, 仓根相对的源码树, 文件数地板, why——它凭什么在这条性质的射程里 + 现打读数)`
    const TREES: &[(&str, &str, usize, &str)] = &[
        (
            "monitor",
            "src-tauri/src",
            80,
            "承载界面那一侧，今天唯一的取用口就在这棵树里。\
             ⚠ 两个数别混：09-04 盘上 **105** 份 `.rs`，而**判据的语料是 104 份** ——\
             `scan_tree!` 按构造摘掉调用者自己那一份（也就是本文件）。地板取 80",
        ),
        (
            "daemon",
            "remote-daemon-proto/src",
            55,
            "本件要换到的那一侧 —— 「解析搬到代码所在地」搬的就是往这棵树里搬\
             （09-04 现打 73 份 `.rs`，地板取 55）。今天它这一格是 0，\
             而**「0」只有在尺子接上了的时候才算数**，那一格由本模块第二条判据买",
        ),
        (
            "共享 crate",
            "src-tauri/crates",
            7,
            "两侧的清单都 `path` 依赖它们 ⇒ 谁在这里取用引擎，两侧都会被编进去，\
             而上面那两条老判据一份都看不见（09-04 现打 9 份 `.rs`，地板取 7）",
        ),
    ];

    /// 登记在案的**排除**：`(仓根相对目录, 为什么它不进人群)`。
    ///
    /// 排除要有住址、有理由、还要有幽灵检查 —— **一个没人知道的过滤器与一个没人知道的洞
    /// 长得一模一样。**
    const EXCLUDED_TREES: &[(&str, &str)] = &[(
        "src-tauri/vendor/code-picture-core",
        "引擎本体自己的家：取用口那个符号是在这里**定义**的，把它算进人群等于要求\
         「定义处也只许有一处取用」——那是另一件事。且 `C7` 逐字「vendor 不动」，\
         它进人群只会造出一条谁也不许修的红。monitor 清单的 `[workspace] exclude` \
         也逐字排除着它，两处口径一致。",
    )];

    fn repo_root() -> std::path::PathBuf {
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("src-tauri 的上级 = 仓根")
            .to_path_buf()
    }

    /// 一棵树的绝对住址。**读不到就 `Err`**，不许退化成空语料。
    fn tree_dir(rel: &str) -> Result<std::path::PathBuf, String> {
        let dir = repo_root().join(rel);
        if dir.is_dir() {
            return Ok(dir);
        }
        Err(format!(
            "登记在案的源码树 `{rel}` 在盘上读不到（它找的是 {}）—— 本条此刻**判不了**，\
             而判不了不许写成「那棵树里零命中」。两种可能：那棵树搬家了（改 `TREES` 的登记），\
             或者仓根算错了（本判据只跳一级，见头注那条 `K-R13`）。",
            dir.display()
        ))
    }

    /// 文件数地板。**空语料与干净的树在终端上一模一样**，所以这一道必须是错、不是 0。
    fn floor_check(label: &str, got: usize, floor: usize) -> Result<(), String> {
        if got >= floor {
            return Ok(());
        }
        Err(format!(
            "`{label}` 这棵树只收到 {got} 份 `.rs`（地板 {floor}）—— 遍历坏了，\
             此刻这条判据在空转：它会零命中地绿，而那与「这棵树里真的没有取用口」不是一件事。"
        ))
    }

    /// 一棵树的语料：`(「标签:树内相对路径」, 原文)`。
    fn corpus_of(label: &str, rel: &str, floor: usize) -> Result<Vec<(String, String)>, String> {
        let dir = tree_dir(rel)?;
        let mut out: Vec<(String, String)> = Vec::new();
        for (path, src) in guard_core::scan_tree!(&dir, &["rs"]) {
            let inside = path
                .strip_prefix(&dir)
                .unwrap_or(&path)
                .to_string_lossy()
                .replace('\\', "/");
            out.push((format!("{label}:{inside}"), src));
        }
        floor_check(label, out.len(), floor)?;
        out.sort();
        Ok(out)
    }

    /// 两根针，**运行时拼** —— 本文件的头注里逐字写着它们，写字面量会读到判据自己。
    /// （`scan_tree!` 已经按构造摘掉本文件，这是第二道；本仓这一族栽过五次。）
    fn port_needles() -> Vec<(String, &'static str)> {
        vec![
            (format!("Engine{}open", "::"), "引擎取用口"),
            (format!("code_picture{}core", "_"), "引擎类型的导入口"),
        ]
    }

    /// 纯函数：一份语料里命中这根针的**住址表**（生产段、带词边界）。
    ///
    /// 抽成纯函数是为了能**直接喂夹具** —— 否则「daemon 那棵树 0 处」这个读数
    /// 与「这把尺子根本数不出东西」在终端上没有区别。
    fn ports_in(corpus: &[(String, String)], needle: &str) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        for (addr, src) in corpus {
            if guard_core::contains_word(&guard_core::production_code(src), needle) {
                out.push(addr.clone());
            }
        }
        out.sort();
        out
    }

    /// ★ 正题：**三棵树合起来，取用口的住址表只有那一处。**
    #[test]
    fn the_engine_port_is_pinned_across_every_tree_we_compile() {
        let mut corpus: Vec<(String, String)> = Vec::new();
        let mut sizes: Vec<String> = Vec::new();
        for (label, rel, floor, why) in TREES {
            assert!(
                why.trim().chars().count() >= 20,
                "`{label}` 没写清它凭什么在这条性质的射程里（实得 {} 字）",
                why.trim().chars().count()
            );
            let files = corpus_of(label, rel, *floor).unwrap_or_else(|e| panic!("{e}"));
            sizes.push(format!("{label} {} 份", files.len()));
            corpus.extend(files);
        }
        for (needle, what) in port_needles() {
            let found = ports_in(&corpus, &needle);
            assert_eq!(
                found,
                vec!["monitor:panorama.rs".to_string()],
                "{what}（针 `{needle}`）的住址表变了：{found:?}\n\
                 各树语料：{sizes:?}\n\
                 ★ **这不是要你把这里的期望改掉了事** —— 先回答是下面哪一种：\n\
                 ① 多出来的那一处是**第二个取用口** ⇒ 那就是第二条 SQLite 连接，\n\
                 「换侧要改的就是那一处」这句承重的话当场作废；\n\
                 ② 是**换侧**（取用口搬到 daemon / 搬进某个共享 crate）⇒ 把期望改成新住址，\n\
                 并**同轮**回答 `K-W2D` 的 `KW2D4`：新那一侧谁在数、`panorama.rs` 那条\n\
                 `find_pinned` 还指得对不对（它只读那一份文件）；\n\
                 ③ 少了 ⇒ 多半是抽取坏了、或引擎整个搬走了，两种都要人看一眼。"
            );
        }
    }

    /// ★★ **「0」要先证明尺子接上了** —— 否则 daemon 那棵树的零命中什么也说明不了。
    ///
    /// 三刀：树指错 ⇒ 出声；语料塌了 ⇒ 出声；**合成一处喂进 daemon 侧的语料 ⇒ 数得出来**。
    ///
    /// # ⚠ 第三刀证明的比它听起来的少（**实测出来的边界，不是谦虚**）
    ///
    /// 那份夹具的文本是**用同一根针拼出来的** ⇒ 针本身错了，这一条照样绿。
    /// 09-04 现打（死值验 M9，把针改成一个盘上不存在的写法再跑一趟门禁）：
    /// **本条绿，而上面那条正题当场红**，报文逐字「住址表变了：`[]`」。
    /// ⇒ 分工写清：本条证的是「**剥生产段 + 词边界这条链，在 daemon 侧的语料上真的会命中**」；
    /// 「**针指的是不是那个事实**」由正题钉着（针错了住址表就空，正题红）。两条合起来才是一句完整的话。
    ///
    /// ★ 为什么不把夹具改成独立字面量：那要在本文件里写下针的**完整字面量**，
    /// 而本模块的针刻意运行时拼（头注第二道防线）。⇒ 这里选**登记边界 + 由正题补位**，
    /// 而不是把防线换掉。〔同一轮里另一条控制（`readonly_guard` 那侧的 feature 探针）
    /// 走的是相反的选择 —— 那条**没有**正题替它补位，所以它必须换独立见证。〕
    #[test]
    fn a_tree_it_cannot_read_makes_it_say_so_instead_of_counting_zero() {
        let daemon = TREES
            .iter()
            .find(|(label, ..)| *label == "daemon")
            .expect("`TREES` 里没有 daemon 那一棵 —— 而本件的整个题目就是往那一侧搬");
        // ① 指错的树：`Err`，而且诊断要点出它找的那个住址（读的人才知道去哪儿看）。
        let bogus = format!("{}-此处刻意不存在", daemon.1);
        let verdict = tree_dir(&bogus);
        let why = verdict.expect_err(
            "指到一个不存在的目录上，`tree_dir` 居然给了 `Ok` —— \
             那意味着「读不到那棵树」会静默变成「那棵树里零命中」",
        );
        assert!(
            guard_core::contains_word(&why, &bogus),
            "诊断里没印出它找的那个住址：{why}"
        );
        // ② 语料塌成空集：地板那一支同样是错，不是 0。
        assert!(
            floor_check("夹具", 0, 3).is_err(),
            "空语料过了地板 —— 那正是「零命中地绿」的入口"
        );
        assert!(
            floor_check("夹具", 3, 3).is_ok(),
            "地板把恰好够的语料也挡了 —— 那会变成假红，而假红最省事的消法是把判据删掉"
        );
        // ③ 尺子真的能在 daemon 侧的语料上数出命中。
        for (needle, what) in port_needles() {
            let fixture = vec![
                (
                    "daemon:某个适配层.rs".to_string(),
                    format!("fn 取一次() {{ let e = {needle}(&key)?; }}"),
                ),
                (
                    "daemon:干净的一份.rs".to_string(),
                    "fn f() -> u8 { 7 }".to_string(),
                ),
            ];
            assert_eq!(
                ports_in(&fixture, &needle),
                vec!["daemon:某个适配层.rs".to_string()],
                "{what} 这根针在 daemon 侧的语料上数不出命中 —— \
                 那么正题里 daemon 那棵树的「0 处」是**空真**，说明不了任何事"
            );
        }
    }

    /// ★ 人群的**分母自己**也要钉住：**再多一棵编进来的树，本条当场说话。**
    ///
    /// 分母从两份清单的**依赖段**里现算（`path = "…"` 那一形），不写死一张 crate 名单
    /// —— 名单会腐，而清单是那件事唯一的住址。
    ///
    /// ⚠ 它看不见的那一类，如实写在这里：**谁的清单都没引的 crate**
    /// （单独构建、只在运行期被起进程调用的那种）。那正是侧车那一形，归 `KW2D4`。
    #[test]
    fn the_set_of_trees_this_scope_covers_is_itself_pinned() {
        let root = repo_root();
        let mut dirs: Vec<String> = Vec::new();
        for (manifest_rel, home) in [
            ("src-tauri/Cargo.toml", "src-tauri"),
            ("remote-daemon-proto/Cargo.toml", "remote-daemon-proto"),
        ] {
            let manifest = std::fs::read_to_string(root.join(manifest_rel))
                .unwrap_or_else(|e| panic!("读不到 {manifest_rel}: {e}"));
            dirs.extend(in_repo_path_deps(&manifest, home));
        }
        dirs.sort();
        dirs.dedup();
        // 抽取器自检：分母塌了下面那条对账就零命中地绿。
        // 地板而不是相等：**变大那个方向由下面那条逐条对账管**，这一道只挡「抽取坏了」。
        assert!(
            dirs.len() >= 8,
            "两份清单的依赖段里只抽出 {} 条仓内 `path` 依赖\
             （09-04 现打 8：7 个共享 crate + vendor 里的引擎）—— 抽取坏了：{dirs:?}",
            dirs.len()
        );
        let mut uncovered: Vec<String> = Vec::new();
        for dir in &dirs {
            let covered = TREES
                .iter()
                .any(|(_, rel, ..)| dir.as_str() == *rel || dir.starts_with(&format!("{rel}/")))
                || EXCLUDED_TREES.iter().any(|(rel, _)| dir.as_str() == *rel);
            if !covered {
                uncovered.push(dir.clone());
            }
        }
        assert!(
            uncovered.is_empty(),
            "有仓内 crate 被编进来了，而 `TREES` 的射程盖不住它：{uncovered:?}\n\
             ★ 这正是 `K-W2D` `KW2D4` 说的「第三棵树」那一刻。两条路，选一条：\n\
             ① 它可能承载引擎取用口 ⇒ 登记进 `TREES`（带文件数地板与理由）；\n\
             ② 它按构造不可能 ⇒ 登记进 `EXCLUDED_TREES` 并写清为什么。\n\
             **不许静默加一条过滤** —— 没人知道的过滤器与没人知道的洞长得一模一样。"
        );
        for (rel, why) in EXCLUDED_TREES {
            assert!(
                dirs.iter().any(|d| d.as_str() == *rel),
                "排除项 `{rel}` 今天已经不在两份清单的依赖面上了 —— 幽灵条目，同轮摘掉"
            );
            assert!(
                why.trim().chars().count() >= 20,
                "排除项 `{rel}` 的理由太短（实得 {} 字）—— 这一列的读者是下一个想再排除一棵树的人",
                why.trim().chars().count()
            );
        }
    }

    /// 一份清单的**依赖段**里所有 `path = "…"`，归一化成仓根相对目录。
    ///
    /// 只认依赖段：`[[bin]]` 那条 `path` 指的是入口文件，不是一棵树。
    /// 段的判据是「段名以 `dependencies]` 收尾」⇒ `[build-dependencies]` 与
    /// `[target.'…'.dependencies]` 一并收得进来（那两种今天在 monitor 清单上真的有）。
    ///
    /// ⚠ 剥 `#` 整行注释走**共享原语**（`strip_comment_lines` 的 YAML/TOML 兄弟），
    /// 不在这里自己写第二份 —— 本函数第一版内联了一个 `#` 过滤，
    /// 而 `structural_scan` 那张「剥注释实现只许一份」的登记表**当场逮住了它**
    /// （09-04 现打，同一趟门禁里连本文件与 daemon 那侧两处一起点名）。
    /// 先例逐字在那张表里：另一处「已变成一句委托 ⇒ 登记删掉」。
    fn in_repo_path_deps(manifest_text: &str, home: &str) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        let mut in_deps = false;
        let uncommented = guard_core::strip_hash_comment_lines(manifest_text);
        for line in uncommented.lines() {
            let entry = line.trim();
            if entry.starts_with('[') {
                in_deps = entry.ends_with("dependencies]");
                continue;
            }
            if !in_deps {
                continue;
            }
            let quoted: Vec<&str> = entry.split('"').collect();
            for (k, seg) in quoted.iter().enumerate() {
                // 奇数下标才是引号**里面**；它前面那一段必须出现 `path` 这个词。
                if k % 2 == 0 || !guard_core::contains_word(quoted[k - 1], "path") {
                    continue;
                }
                let joined = format!("{home}/{seg}");
                let mut norm: Vec<&str> = Vec::new();
                for part in joined.split('/') {
                    match part {
                        "." | "" => {}
                        ".." => {
                            norm.pop();
                        }
                        other => norm.push(other),
                    }
                }
                out.push(norm.join("/"));
            }
        }
        out
    }
}
