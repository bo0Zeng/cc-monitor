//! `K-W2D` `KW2D3`：**代码全景的解析发生在哪一侧** —— 第四个指纹，钉的是
//! 「`Engine::open` 落在**哪个进程的地址空间**」。
//!
//! # 为什么非要第四个指纹（前三个在同一台机器上分不开两件事）
//!
//! `K-R2` 立的三个指纹住在量具 `evidence/K-R2-kr22-which-side.py` 里，都钉「**哪台机器**」：
//! ① 本机侧出现被分析仓的源文件吗 · ② 本机侧出现该仓的索引库吗 · ③ 收回来的字节量纲。
//! 而 sidecar 这个形状让「哪一侧」多出**第二层含义**：同一台机器上，
//! **是被起的那个进程解析的，还是宿主自己链着引擎解析的**。
//! 后者在①②③上与前者**完全一样** —— 源文件在代码所在地、索引库在代码所在地、
//! 收回来的只有 JSON。⇒ 只钉前三个，「daemon 自己链了引擎」这种实现照样绿。
//!
//! ★ 本条把那一层变成**静态可判**的形状：一个地址空间要能解析，
//! 前提是**引擎被链进了它**。⇒ 钉两件事：**谁链**（lock 里有没有那个包）·
//! **谁取用**（生产段里有几处打开引擎）。
//!
//! # 今天的读数（就是下面那两张登记表的内容，别在这段散文里复述第二遍）
//!
//! 人群 = 本仓今天的两个 `Cargo.lock`（monitor 那棵 · daemon 那棵）+ 两棵源码树的生产段
//! + `src-tauri/crates/` 下每个共享 crate 的 manifest。
//!
//! # ⚠ 它认不出什么（逐条写，别读成全覆盖）
//!
//! - **不判「远端那台机器上真起了解析进程」** —— 那半是 `K-R2` 那三个指纹的活，
//!   而它们自己也只买到「判据形状 + 阴性对照」，没买到端到端真跨机。
//! - **不判运行期**。静态处数不等于运行期的地址空间数：同一处代码被两个进程各跑一遍，
//!   照样是两条 SQLite 连接。堵那一格靠的是「daemon 这一棵**不许链**」这半，
//!   不是「处数恰好一处」这半。
//! - **第三棵树今天不存在** —— S2（本仓自己出一个薄 sidecar crate）落地那天会出现一棵
//!   新的树、一份新的 lock。本条的人群是**登记的那两份 lock**
//!   ⇒ **「有人加了第三棵树却没登记」这一格本条买不到**。那正是件文件 `§0f` 第 4 条
//!   要求「每一条按树切的判据都重新问一遍」的那一问，落点在 `KW2D4`（不在本条）。
//!   本条能买到的是：第三棵树**若走 `src-tauri/crates/` 那个现成的家**，当场红。
//! - **不认 vendor 本体**（`C7`：`src-tauri/vendor/code-picture-core/` 不动）。
//!   引擎自己的 manifest 里那行 `[package] name` 不是一条依赖声明，采集器按**键的形状**认，
//!   不按「文件里出现过这个名字」认 —— 同一条形状让
//!   `exclude = ["vendor/code-picture-core"]`（那是 workspace 成员身份，不是依赖）
//!   不会被误读成链接（件文件 `§0c` 逐字点过这两个 `vendor` 字样是两件事）。
//! - **与 monitor 侧 `panorama_seam_registry` 有一处重叠，如实说**：那边的
//!   `the_engine_is_opened_in_exactly_one_place` 只看 `panorama.rs` 一个文件、
//!   `the_engine_type_does_not_escape_the_panorama_module` 看 monitor 那一棵树。
//!   本条第三格把「恰好一处」量到**整棵 monitor 树的生产段**上（超集），
//!   而本条真正新买的是 daemon 那半与**跨树的链接登记**。
//!   ⇒ 若 `KW2D4` 落地成「三棵树合起来恰好一处」，本条第三格就成了第二份真相，
//!   那时该合并的是它们，不是各留一半。
//!
//! # 🔴 删任何一格之前先读这一段（变异实测逼出来的，不是风格）
//!
//! daemon 那一格断的是**空集**（今天 daemon 一处都没有）⇒ 采集器被打瞎时它**照样绿**。
//! 09-04 实测：把 [`tests::open_sites`] 改成恒返空集 ⇒ **只有 monitor 那一格红**
//! （它的期望是**非空的 1**），daemon 那一格一声不吭。
//! ⇒ **这两格必须一起在**：删掉 monitor 那一格，daemon 那一格当场退化成空真，
//! 而退化之后它看起来与守着的时候一模一样。
//! 同理，[`tests::lock_has_engine`] 被打瞎时接住它的是那条**相等断言**（不是地板）——
//! 登记表清空或采集器变瞎，两种都让它红，这是**故意的 fail-closed**。
//!
//! 注：本模块整体在 `#[cfg(test)]` 内，非测试构建为空。

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    /// 仓根（`remote-daemon-proto` 的上级）。
    ///
    /// ⚠ 跨树那几份是**运行时读**，不走 `include_str!`：编译期读会给
    /// `cross_half_edge_registry` 添一条新的跨半边，而那张表的口径是「必须编译期读才登记」。
    /// 同一棵树上的先例逐字写着这个取舍（`observe/accounts_query.rs` 那条读 monitor
    /// `history.rs` 的判据）：代价是「文件挪了只有本条红」，补偿是 `expect` + 字节地板。
    fn repo_root() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("remote-daemon-proto 的上级 = 仓根")
            .to_path_buf()
    }

    /// 引擎那个包的两种写法（**运行时拼** —— 直接写字面量会让本文件变成别的判据的语料，
    /// 而 monitor 侧 `panorama_seam_registry` 的头注逐字记着这一族：
    /// 「判据的针不只会读到自己，**还会喂给别人**」）。
    fn engine_names() -> (String, String) {
        (
            format!("code{d}picture{d}core", d = "-"),
            format!("code{u}picture{u}core", u = "_"),
        )
    }

    /// 打开引擎那一处的形状。
    fn open_needle() -> String {
        format!("Engine{}open", "::")
    }

    /// **登记表①**：今天哪几份 `Cargo.lock` 是一个「可能解析代码的地址空间」的账本。
    ///
    /// 键是**仓根相对路径**；`Cargo.lock` 里列的是**整个依赖图**（含传递依赖）
    /// ⇒ 比读 manifest 严一档：有人经由第三个 crate 把引擎间接链进 daemon，这里也看得见。
    const LOCKS: &[&str] = &[
        "src-tauri/Cargo.lock",
        "remote-daemon-proto/Cargo.lock",
    ];

    /// **登记表②**：上面那几份里，**允许**含引擎那个包的是哪几份。
    ///
    /// 今天恰好一份 —— monitor 那棵。它就是「解析发生在 monitor 进程里」这句话的账面形态。
    /// ⚠ 这是一道**相等断言**，不是地板：多一份（daemon 也链了）与少一份（monitor 不再链了）
    /// 都会红，而两种红各有各的处置，诊断里分开写。
    const ENGINE_LINKED_BY: &[&str] = &["src-tauri/Cargo.lock"];

    /// 读一份仓内文件，**读不到就 panic，不许静默成空串地绿**。
    fn read_pinned(rel: &str, floor: usize) -> String {
        let p = repo_root().join(rel);
        let s = std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("读不到 {p:?}：{e}"));
        assert!(
            s.len() >= floor,
            "{rel} 只读到 {} 字节（地板 {floor}）—— 没读到真文件，本条在空转",
            s.len()
        );
        s
    }

    /// 这份 lock 里有没有引擎那个包（`name = "…"` 那一行的**整行**形状）。
    ///
    /// ⚠ 按整行认，不按「文件里出现过这个名字」认：`Cargo.lock` 里同一个名字还会
    /// 以 `dependencies` 列表成员的形式出现，那说明的是「谁依赖它」而不是「这个图里有它」。
    /// 两者今天同真同假，但形状不同 —— 钉承重的那一个。
    fn lock_has_engine(lock: &str, dashed: &str) -> bool {
        let row = format!("name = \"{dashed}\"");
        lock.lines().any(|l| l.trim() == row)
    }

    /// 一行 cargo manifest 是不是「**声明依赖**引擎」那一形。
    ///
    /// cargo 的依赖键 = 行首那个词。⇒ `exclude = [".../code-picture-core"]` 这种
    /// **值里带着名字**的行不算（它是 workspace 成员身份，不是依赖）；
    /// `#` 起头的注释行也不算。
    fn declares_engine_dep(line: &str, dashed: &str, underscored: &str) -> bool {
        let t = line.trim();
        if t.starts_with('#') {
            return false;
        }
        let Some((key, _)) = t.split_once('=') else {
            return false;
        };
        let key = key.trim().trim_matches('"');
        key == dashed || key == underscored
    }

    /// 某棵树的生产段里，打开引擎那一处出现了几次（按文件逐个报，好读诊断）。
    fn open_sites(root: &Path, needle: &str) -> Vec<(String, usize)> {
        let mut out: Vec<(String, usize)> = Vec::new();
        for (p, src) in guard_core::scan_tree!(root, &["rs"]) {
            let rel = p
                .strip_prefix(root)
                .unwrap_or(&p)
                .to_string_lossy()
                .replace('\\', "/");
            let n = guard_core::production_code(&src).matches(needle).count();
            if n > 0 {
                out.push((rel, n));
            }
        }
        out.sort();
        out
    }

    /// ★ 正题①（**链接面**）：引擎只许被链进**登记过的那几个地址空间**。
    ///
    /// 这一格就是第四个指纹的静态形态：链进来 = 能在自己的地址空间里解析。
    /// 「编进 daemon」（件文件 `§0e` 的甲）落地当天，本条**必然红** —— 红得对：
    /// 那一红就是「有人在『解析发生在哪个进程』这件事上签了字」。
    #[test]
    fn the_engine_linkage_registry_still_matches_the_locks_on_disk() {
        let (dashed, _) = engine_names();
        // 反空真：人群不能空，也不能只剩一份（只剩一份时「相等」买不到跨树那半）。
        assert!(
            LOCKS.len() >= 2,
            "lock 人群只剩 {} 份 —— 跨地址空间那半此刻是空转的",
            LOCKS.len()
        );
        let mut linked: Vec<&str> = Vec::new();
        for rel in LOCKS {
            // 地板取得很松（一份真 lock 远大于 10 KiB），它只挡「读到了一份空文件/别的文件」。
            let lock = read_pinned(rel, 10_000);
            if lock_has_engine(&lock, &dashed) {
                linked.push(rel);
            }
        }
        let mut want: Vec<&str> = ENGINE_LINKED_BY.to_vec();
        want.sort();
        linked.sort();
        assert_eq!(
            linked, want,
            "「引擎被链进了哪几个地址空间」与登记的对不上。\n\
             **多出来的**：那个二进制从此**在自己的进程里**就能解析代码 —— \
             这正是 `KW2D3` 要分开的那两件事（是被起的那个进程解析的，\
             还是宿主自己链着引擎解析的）。先回答「这一侧凭什么需要引擎」，再谈改表。\n\
             **少了的**：引擎搬走了（或那份 lock 挪了位）⇒ 摘登记；\
             也可能是读法坏了，那样本条与下面两条一起在空转。"
        );
    }

    /// ★ 正题②（**取用面 · daemon 这一棵**）：daemon 的生产段一处都不许打开引擎。
    ///
    /// 与上一条**主语不同**，谁也替不了谁：上一条看账本（链没链），本条看代码（取用没取用）。
    /// 「依赖在、一行都不调」是真会发生的中间态（`K-R2` 实测过：不真调的话 rustc
    /// 根本不链那个 rlib，量出来是个假的便宜）—— 那时上一条已经红，而本条仍绿；
    /// 反过来，有人从别处（第三棵树的 re-export）拿到引擎类型时，本条红而上一条可能仍绿。
    #[test]
    fn the_daemon_tree_never_parses_in_its_own_address_space() {
        let (_, underscored) = engine_names();
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let opens = open_sites(&root, &open_needle());
        assert!(
            opens.is_empty(),
            "daemon 的生产段里出现了打开引擎那一处：{opens:?}\n\
             ⇒ 那意味着解析发生在**daemon 自己的进程**里，而不是被起的那个进程里。\n\
             件文件 `§0b` 论据三逐字：monitor 一处 + daemon 一处 = **两个进程各持一条连接**，\n\
             而那正是 `panorama.rs` 自己记着的那条真事故（对同一索引库并发写）。\n\
             要让 daemon 有全景能力，走**起一个进程**那条路（那条路的账在起进程点表上），\n\
             不是把引擎链进来。"
        );
        // 类型面：引擎类型要跑进来，必然先出现在 `use` 上（monitor 侧那条同形判据的教训 ——
        // 整词匹配 `Engine` 会误伤字符串字面量里给人看的文案）。
        let mut importers: Vec<String> = Vec::new();
        for (p, src) in guard_core::scan_tree!(&root, &["rs"]) {
            if guard_core::contains_word(&guard_core::production_code(&src), &underscored) {
                importers.push(
                    p.strip_prefix(&root)
                        .unwrap_or(&p)
                        .to_string_lossy()
                        .replace('\\', "/"),
                );
            }
        }
        assert!(
            importers.is_empty(),
            "引擎类型进了 daemon 的生产段：{importers:?}\n\
             ⇒ 同上一段：这一棵树不许在自己的地址空间里解析。"
        );
    }

    /// ★ 正题③（**取用面 · monitor 这一棵**）：本机侧的解析入口**恰好一处**。
    ///
    /// 它是件文件 `§3` 点名要的那一刀（「合成 monitor 侧多一处解析入口 ⇒ 红」）的落点。
    /// 与 monitor 侧那条的差别写在本模块头注最后一段（那边看一个文件，这边量整棵树）。
    #[test]
    fn the_monitor_tree_keeps_exactly_one_parse_entrance() {
        let root = repo_root().join("src-tauri").join("src");
        let files = guard_core::scan_tree!(&root, &["rs"]);
        // 反空真：monitor 那棵树读不到（路径挪了 / 只剩几个文件）⇒ 下面会零命中地绿。
        // 地板 90 · 现打 105（量于本件基点 `d305ffa`）—— 留 15 的余量，
        // 好让「正当地删掉几个文件」不至于误红；余量再大就挡不住「静默缩水」了。
        assert!(
            files.len() >= 90,
            "monitor 树只扫到 {} 个 `.rs` —— 跨树那一份没读到，本条在空转",
            files.len()
        );
        let opens = open_sites(&root, &open_needle());
        let total: usize = opens.iter().map(|(_, n)| n).sum();
        assert_eq!(
            total,
            1,
            "monitor 树的生产段里打开引擎的处数是 {total}（该是 1）：{opens:?}\n\
             **多出来的**：第二处 = 第二条连接（`panorama.rs` 自己记着那条事故）；\
             而对 `KW2D3` 还多一层意思 —— 解析在本机侧多了一个入口，\
             「哪一侧解析」这句话就不再有唯一答案。\n\
             **少了的（0 处）**：引擎取用口搬走了 ⇒ 那是 `KW2D1` 那个分叉真的落地了，\
             该同轮改的是本条与 `ENGINE_LINKED_BY` 两张表，不是把数字改成 0 了事。"
        );
    }

    /// ★ 正题④（**第三棵树最便宜的那个家**）：共享 crate 一个都不许把引擎链进来。
    ///
    /// 为什么单立一条：`src-tauri/crates/` 是本仓今天现成的「第三棵树」的家
    /// （7 个共享 crate 都住那儿），而两侧都 `path` 依赖着它们
    /// ⇒ 往任何一个里塞引擎依赖，等于**同时**把引擎链进两个地址空间，
    /// 而上面那条相等断言看的是 lock、要等 lock 重新解析才看得见。
    #[test]
    fn no_shared_crate_pulls_the_engine_into_a_second_address_space() {
        let (dashed, underscored) = engine_names();
        let root = repo_root().join("src-tauri").join("crates");
        let mans = guard_core::scan_tree!(&root, &["toml"]);
        assert!(
            mans.len() >= 5,
            "共享 crate 只扫到 {} 份 manifest —— 采集坏了，本条在空转",
            mans.len()
        );
        let mut hits: Vec<String> = Vec::new();
        for (p, src) in &mans {
            for line in src.lines() {
                if declares_engine_dep(line, &dashed, &underscored) {
                    hits.push(format!(
                        "  {}: {}",
                        p.strip_prefix(&root).unwrap_or(p).to_string_lossy(),
                        line.trim()
                    ));
                }
            }
        }
        assert!(
            hits.is_empty(),
            "共享 crate 里声明了引擎依赖：\n{}\n\
             ⇒ 两棵树都 `path` 依赖着这些 crate ⇒ 这一行等于**同时**给两个地址空间装上引擎。\n\
             要给某一侧装引擎，就在那一侧自己的 manifest 里装，让上面那条登记表看得见。",
            hits.join("\n")
        );
    }

    /// ★ 阴性对照：**四根针各单断一格 + 反向不误伤**。
    ///
    /// 为什么要一格一格断：上面四条各有自己的采集器，只喂一个样本的话，
    /// 任何一根悄悄失效都被别的盖住（本仓「N 个独立源只断一次」踩过）。
    /// ⚠ 它证的是「**这四根针**认得出用这四种形状写的违规样本」，
    /// **不是**「本条认得出任何一种把解析搬到本机侧的实现」——
    /// 认不出的那几样逐条写在本模块头注里。
    #[test]
    fn the_locus_guard_actually_bites() {
        let (dashed, underscored) = engine_names();
        let needle = open_needle();

        // ① 针一单断（链接面）：daemon 的 lock 里长出引擎那个包 ⇒ 认得出。
        let synthetic_lock = format!(
            "[[package]]\nname = \"{dashed}\"\nversion = \"0.1.0\"\n",
        );
        assert!(
            lock_has_engine(&synthetic_lock, &dashed),
            "针一瞎了：lock 里的引擎包没被认出来"
        );
        // 反向：只是**被谁依赖**（列表成员那一形）不算「这个图里有它」。
        let dep_row = format!(" \"{dashed} 0.1.0\",\n");
        assert!(
            !lock_has_engine(&dep_row, &dashed),
            "针一把依赖列表成员误读成了包本身 —— 那会让相等断言恒真"
        );

        // ② 针二单断（manifest 键的形状）：真依赖行认得出，两种写法都要。
        for line in [
            format!("{dashed} = {{ path = \"../src-tauri/vendor/{dashed}\" }}"),
            format!("{underscored} = \"0.1\""),
        ] {
            assert!(
                declares_engine_dep(&line, &dashed, &underscored),
                "针二瞎了：这一行是真依赖声明却没被认出来：{line}"
            );
        }
        // 反向 A：`exclude` 那一行**值里带着名字**，它是成员身份不是依赖（件文件 `§0c`）。
        assert!(
            !declares_engine_dep(
                &format!("exclude = [\"vendor/{dashed}\"]"),
                &dashed,
                &underscored
            ),
            "针二把 workspace `exclude` 误读成了依赖 —— 那会把 monitor 的 manifest 判成违规"
        );
        // 反向 B：注释掉的那一行不算（不然「注掉一行」会被读成还在链）。
        assert!(
            !declares_engine_dep(
                &format!("# {dashed} = {{ path = \"x\" }}"),
                &dashed,
                &underscored
            ),
            "针二把注释行当成了依赖声明"
        );

        // ③ 针三单断（取用面的计数）：合成「多一处解析入口」⇒ 数得出来是 2。
        let two_entrances = format!(
            "fn a() {{ let e = {needle}(&k, o); }}\nfn b() {{ let e2 = {needle}(&k2, o2); }}\n"
        );
        assert_eq!(
            guard_core::production_code(&two_entrances)
                .matches(needle.as_str())
                .count(),
            2,
            "针三瞎了：合成的第二处解析入口没被数到"
        );
        // 反向：注释里的那一处不算（`panorama.rs` 的注释里就写着这个词，
        // 不剥注释的话 monitor 那一格今天就会红成 5 处）。
        let commented = format!("// 这里讲的是 {needle} 很轻，不扫描\n");
        assert_eq!(
            guard_core::production_code(&commented)
                .matches(needle.as_str())
                .count(),
            0,
            "针三把注释里的提及数成了取用点"
        );

        // ④ 针四单断（类型面）：`use` 进来认得出，而字符串里给人看的文案不许误命中。
        assert!(
            guard_core::contains_word(&format!("use {underscored}::Engine;"), &underscored),
            "针四瞎了：引擎类型的导入没被认出来"
        );
        assert!(
            !guard_core::contains_word(
                &format!("let s = \"{underscored}x\";"),
                &underscored
            ),
            "针四把撑大了的词当成了导入 —— 匹配单位比事实小的那一族"
        );

        // ⑤ 登记表不许空：空了的话上面那两条相等断言会把「一个都没有」也判绿。
        assert!(
            !ENGINE_LINKED_BY.is_empty() && !LOCKS.is_empty(),
            "登记表空了 ⇒ 链接面那条恒绿"
        );
    }
}
