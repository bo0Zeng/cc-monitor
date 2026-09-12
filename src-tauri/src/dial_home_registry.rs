//! `K-R74`：**「后端 SSH 解耦干净」这句话今天红不了** —— 把它改述成可判的三样东西。
//!
//! # 它治的那一格
//!
//! 「解耦干净」在盘上一直被表述成「**谁持有流**」。那句话**机器判不了**：
//! 没有一个数会因为它变假而动，于是它既不会红、也不会降 —— 它只会被引用。
//! 用户 09-12 逐字「关于后端 ssh / 你思考一下怎么样才比较解耦干净」，
//! PM 的分层与四步排期住 `DECISIONS.md#R32`，**本模块是第 1 步：记分牌**。
//!
//! 🔴 **本模块一处拨号都不搬**（那是 `K-P6` 的三拍）。它只立三样东西：
//!
//! | 立什么 | 住哪 | 形态 |
//! |---|---|---|
//! | **终点**：一面二值旗 —— `russh` 在不在界面 crate 的依赖里 | [`russh_deps_in`] | 编译期钉 + 现算读数 |
//! | **过程**：一个只许降的数 —— 界面侧**还没搬走**的拨号处数 | [`unmoved_dial_sites_in`] ＋ [`ratchet_backslide`] | 递减棘轮 |
//! | **住址**：拨号锚点只许长在登记的那个家里 | [`SITES`] ＋ [`dial_locality`] | 零命中守卫 |
//!
//! # 🔴 为什么棘轮**不能**骑在「依赖树里有没有 russh」上
//!
//! 09-12 PM 现打：`cargo tree -i russh --manifest-path src-tauri/Cargo.toml` 只有一条边
//! （`russh v0.61.1 └── monitor v3.7.0`）⇒ **`russh` 是 monitor 的直接依赖**。
//! 「在不在依赖树里」是一面**二值旗，降不动**；而「可达包块数」**不随搬运单调降**
//! （同一个包可能被别的路径拉进来）—— 拿它当棘轮，读数会在搬对了的时候不动、
//! 在别处加一个无关依赖的时候乱跳。
//!
//! ⇒ **终点用二值旗，过程用一个已经有人数的数**：
//! [`crate::ssh_source::dial_move_judge::DIAL_SITES`]（住 `ssh_source.rs` 的那张表；
//! 〔`K-R76` 09-12〕它原先还在文件顶层再导出一次，那道绕道已经拆了 ——
//! 模块本身就写成 `pub(crate) mod`，理由与实打读数在那一行旁边）
//! 里 `moved == false` 的**处数合计**。
//! 它已经是机检、已经有判据在核（`six_of_the_seven_dial_sites_are_still_in_this_process`），
//! 而且**归零那一刻正好就是 `russh` 能离开 monitor 的那一刻** —— 两样东西咬在一起。
//!
//! ## 🔴 失效方向（件计划逐字点名，写在这儿别让下一个人重新发明）
//!
//! **只数 `ssh_source.rs` 里有多少行 `use russh`** —— 那数的是**写法**，不是**依赖**：
//! 删掉一行 `use` 改成全路径调用，数字就掉了，而依赖树一个包都没少。
//! ⇒ 本模块的数**一处都不来自 `use` 行**：它来自登记表里的**拨号处数**，
//! 而那张表的每一格由 `K-P6b` 那条判据从**生产段源码**派生再逐格比对。
//!
//! # 这个家今天选在哪，以及它会不会随 `K-P6` 搬走
//!
//! daemon 侧 `remote-daemon-proto/src/dial/mod.rs` 的 `dial_locality` 逐字
//! 「本 crate 里**唯一**允许出现拨号锚点的地方」，它的家是**一元闭集** `dial/`。
//! monitor 侧今天**没有**这一条 —— 本模块补上，而它与 daemon 那份**有两处刻意的不同**，
//! 两处都是读数逼出来的，不是口味：
//!
//! 1. **家今天是多元闭集，不是一元。** 有几份、是哪几份、各几处 —— **唯一住址是 [`SITES`]**，
//!    而且判据每趟把它**现算**印在 `--nocapture` 里（`brief` 13b：闭集不许在散文里复述成员，
//!    连基数也不许）。本节只写**为什么它没被写成一元**：
//!    要写成一元只有两条路 —— **搬代码**（本件明写不做）或者**把锚点集收窄**，
//!    只留住在一份文件里的那几个、把开隧道那个丢掉。
//!    🔴 后者正是件计划点名的失效方向：**把住址定在方便的地方，而不去问它该不该是那个家**。
//!    ⇒ 锚点集与 daemon 那份**逐字相同**（唯一住址 [`anchors`]，同样不在这儿复述），
//!    家诚实地按现打读数记。
//! 2. **没有 daemon 那条 `total == 0 ⇒ Err`。** 在 daemon 那侧，0 处锚点意味着
//!    「代理根本没在拨号」= 坏了；在 monitor 这侧，**0 正是终点**。
//!    把那一支抄过来，等于让判据在这件事做成的那一天变红。
//!    ⇒ 本模块只留**语料地板**（分开「一处都没有」与「压根没扫」），不留「至少一处」。
//!
//! **它会不会随 `K-P6` 那三拍搬走 —— 会。**
//! 🔴 **哪一行在哪一拍消失，唯一住址是 [`SITES`] 的「解锁条件」那一栏**
//!（机检 `every_registered_dial_home_says_what_it_is_and_when_it_could_go` 断它非空且 ≥20 字）——
//! 本节**不复述那几行**，只写它们咬在一起的次序：
//!
//! - `K-P6` **第 2 拍**（C 类：SFTP + 端口转发**整个动作**搬进后端）与 **第 4 拍**（A 类剩余）
//!   各让 [`SITES`] **少一行**；两拍都落地 ⇒ **家变成空集**。
//! - `K-P6` **第 3 拍**（B 类：那 35 个 `connect_and_exec_cmd` 调用点 → 一条后端命令）
//!   **不动 [`SITES`]**，它让 [`unmoved_dial_sites_in`] 那个数往下走 ——
//!   ⚠ **这正是「两个数别读混」的用处**：有一整拍只动其中一个数，合并成一个就看不见它了。
//!
//! 🔴 **家变成空集那一刻，正是二值旗该翻面的那一刻** —— `russh` 从 `src-tauri/Cargo.toml`
//! 里删掉。两件事咬在一起，是本模块唯一的设计意图；`SITES` 空了而旗还立着，
//! 说明有人把依赖留在了 manifest 里没人用，那是另一条要报的账。
//!
//! # ⚠ 它**没有**买到什么（三条，逐条与它买到的那句同句写）
//!
//! - **二值旗量的是 manifest 里的直接依赖，不是 `cargo tree` 那一层。**
//!   09-12 现打两者同值（`cargo tree -i russh` 只有 `russh v0.61.1 └── monitor v3.7.0`
//!   一条边）—— **同值是今天的事实，不是本判据钉住的性质**：明天有个依赖把 `russh`
//!   拉成传递依赖，manifest 上看不出来，本条照样绿。
//! - **锚点按源码文本取。** 换个 `use` 别名、把调用藏进宏，本条看不见 ——
//!   与 daemon `dial_locality` / `readonly_guard` 头注那条诚实边界**同族同形**，别读成证明。
//!   ⚠ 顺带钉一条**别踩的**：本文件的 doc 注释与 [`SITES`] 的说法栏里**逐字写着**几个锚点，
//!   今天它们进不了扫描面有**两个各自独立的理由** —— `scan_tree!` 按构造摘除调用者自己，
//!   而且 `production_code` 把 `//` 开头的行与 `#[cfg(test)]` 段都剥掉。
//!   daemon 那份为了扫到自己**必须** `include_str!` 补一份回来，那时这两道就同时失效
//!   ⇒ **本侧刻意不补**（[`crate_sources`] 头注写着为什么）。谁将来要补，先把这几处字面量拆掉。
//! - **棘轮守的是「这个数不许比它在历史上出现过的最低档还高」，不是「它真的在降」。**
//!   一直不动是合法的（`K-P6` 没开工的每一天都是这样），本条不会因此出声。
//!
//! # 两个数别读混
//!
//! 本模块里有**两个不同的数**，口径不同，刻意不合并：
//!
//! - **拨号处数（今天 6）** = `DIAL_SITES` 里 `moved == false` 那几行的 `connect_session(`
//!   **调用点**合计。棘轮骑的是它。
//! - **锚点处数（[`SITES`] 那张表，今天两行）** = `russh` 层的握手 / 开隧道**锚点**
//!   （与 daemon 逐字同一组三个）。住址判据数的是它。
//!
//! 一个数装两件事是本工作区最贵的那条病 —— 这里两个数各有各的家、各有各的判据，
//! 谁腐了谁红，**不许合并成一个「SSH 债」**。

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    fn repo_root() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("仓根")
            .to_path_buf()
    }

    // ════════════════════════════════════════════════════════════════════════
    //  KR74D3 · 拨号锚点的唯一住址
    // ════════════════════════════════════════════════════════════════════════

    /// **monitor 侧「谁可以拨号」的唯一住址。闭集。**
    ///
    /// `(相对 `src-tauri/src/` 的路径, 生产段里的锚点处数, 它是什么, 解锁条件)`
    ///
    /// 🔴 表名刻意起成 `SITES` —— `scanning_guard_registry::TABLE_DECLS` 那条纪律逐字：
    /// 「新写一条『扫描面 ＋ 常量表』型的判据，那张表要起成 `TABLE_DECLS` 里已有的名字之一」。
    /// 起别的名字 ⇒ `every_registry_guard_keeps_its_reverse_half` **看不见本文件**，
    /// 而看不见与「合规」在输出上一模一样。
    ///
    /// ⚠ 第二栏是**锚点**处数，不是 `DIAL_SITES` 那个**调用点**处数 —— 两个数不是一回事，
    /// 模块头注「两个数别读混」那一节写着为什么不合并。
    const SITES: &[(&str, usize, &str, &str)] = &[
        (
            "ssh_source.rs",
            3,
            "界面进程里真正跑 SSH 握手的那一处（`client::connect(`）· \
             跳板上跑握手的那一处（`client::connect_stream(`）· \
             跳板开隧道的那一处（`channel_open_direct_tcpip(`）。\
             这三处就是「界面自己在拨号」这句话的全部机器形态。",
            "`K-P6` 第 4 拍（A 类剩余：跳板 · 测试连接跟代理走）落地那天 —— \
             那一天这一行整行删掉，家变成空集，`russh` 同拍从 `src-tauri/Cargo.toml` 里走。",
        ),
        (
            "port_forward.rs",
            1,
            "端口转发：每条转发一条独立 SSH 会话，隧道由 `channel_open_direct_tcpip(` 开出去。\
             它要的是**原始字节**，而消费者不是界面 ⇒ `R32` 裁定零把它归在 C 类。",
            "`K-P6` 第 2 拍（C 类：SFTP + 端口转发**整个动作**搬进后端）落地那天 —— \
             字节根本不回界面，这一行随之整行删掉，家收成一元。",
        ),
    ];

    /// 真正把字节交给 SSH 状态机的入口 —— **锚点与 daemon `dial_locality` 逐字同一组**。
    ///
    /// 🔴 **运行时拼**：直接写字面量的话，本模块自己就会被下面的扫描命中
    ///（`scanning_guard_registry` 头注记着的那一族，本仓栽过五次）。
    fn anchors() -> Vec<String> {
        [
            ("client::conn", "ect("),
            ("client::conn", "ect_stream("),
            ("channel_open_direct", "_tcpip("),
        ]
        .iter()
        .map(|(a, b)| format!("{a}{b}"))
        .collect()
    }

    /// 语料地板：低于这个字节数就判「语料没喂进来」，而不是「一处都没有」。
    ///
    /// 🔴 这条与下面那条文件数地板一起，接的是本模块**唯一**一种会静默的坏法：
    /// 家里今天恰好没有违例 ⇒ 「扫过了、干净」与「压根没扫」在输出上一模一样。
    const CORPUS_FLOOR_BYTES: usize = 400_000;
    /// 文件数地板：`src-tauri/src` 09-12 现打 100+ 份 `.rs`（`scan_tree!` 摘掉本文件）。
    const CORPUS_FLOOR_FILES: usize = 80;

    /// **住址判据本体。纯函数** —— 语料与家都由调用方给 ⇒ 阳性/阴性两个方向都切得动。
    ///
    /// 回 `Ok(锚点总数)`；`Err(说法)` = 判据红。三种红各有各的话：
    /// ① 语料太小（空转）· ② 锚点长在家外面 · ③ 家里的处数与登记对不上。
    ///
    /// 🔴 **刻意没有 daemon 那条 `total == 0 ⇒ Err`**：在这一侧 0 是终点，不是故障。
    /// 理由写在模块头注「两处刻意的不同」第 2 条，这里不复述。
    fn dial_locality(corpus: &[(String, String)], home: &[(&str, usize)]) -> Result<usize, String> {
        let bytes: usize = corpus.iter().map(|(_, c)| c.len()).sum();
        if bytes < CORPUS_FLOOR_BYTES {
            return Err(format!(
                "语料只有 {bytes} 字节（地板 {CORPUS_FLOOR_BYTES}）—— 本判据此刻在空转。\n\
                 「一处锚点都没扫到」与「根本没扫」在终端上一模一样，这条地板就是把它们分开的那一刀。"
            ));
        }
        let pats = anchors();
        let mut total = 0usize;
        let mut strays: Vec<String> = Vec::new();
        let mut seen: Vec<(String, usize)> = Vec::new();
        for (name, code) in corpus {
            let mut here = 0usize;
            for p in &pats {
                here += code.matches(p.as_str()).count();
            }
            if here == 0 {
                continue;
            }
            total += here;
            match home.iter().find(|(f, _)| f == name) {
                Some(_) => seen.push((name.clone(), here)),
                None => strays.push(format!("{name} 里有 {here} 处拨号锚点")),
            }
        }
        if !strays.is_empty() {
            return Err(format!(
                "拨号锚点长到登记的家外面去了：{strays:?}\n\
                 monitor 里「与远端跑 SSH 握手 / 开隧道」只许住 `SITES` 登记的那几份文件 —— \
                 别处要拨号，先回答「为什么这一处非得自己拨」，答完把它登记进 `SITES` 并写上解锁条件。\n\
                 🔴 想把它藏进已登记的文件里也不行：下面那一格逐份比对处数。"
            ));
        }
        seen.sort();
        let mut want: Vec<(String, usize)> = home
            .iter()
            .map(|(f, n)| ((*f).to_string(), *n))
            .filter(|(_, n)| *n > 0)
            .collect();
        want.sort();
        if seen != want {
            return Err(format!(
                "家里的锚点处数与 `SITES` 登记对不上。\n  实际 {seen:?}\n  登记 {want:?}\n\
                 **多出来**：有人在已登记的文件里又加了一处拨号 —— \
                 那是「第七个地方」的另一种长法，先答「为什么」。\n\
                 **少了**：那一处搬走了或没了 ⇒ 把这一行的处数改小；改到 0 就把整行删掉，\
                 并同拍去看 `russh` 还能不能从 `src-tauri/Cargo.toml` 里走。"
            ));
        }
        Ok(total)
    }

    /// 本 crate `src/` 递归全部 `.rs` 的**生产段**，相对 `src/` 的路径 + 正文。
    ///
    /// 🔴 **必须走 `guard_core::scan_tree!`**（`scanning_guard_registry` 那条判据钉着）：
    /// 裸 `read_dir` 的扫描型判据会在自己的登记表 / 注释里找到自己 ⇒ 恒绿。
    ///
    /// ⚠ `scan_tree!` **按构造摘除调用者自己那一份**（本文件）。daemon 那侧必须把自己
    /// `include_str!` 补回来（锚点恰恰住在它自己那份里），**本侧刻意不补** ——
    /// 本文件里一处锚点字面量都没有（[`anchors`] 是运行时拼的），补回来只会把
    /// 「判据在自己身上找到自己」那一族重新请进门。
    fn crate_sources() -> Vec<(String, String)> {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut out: Vec<(String, String)> = Vec::new();
        for (path, src) in guard_core::scan_tree!(&root, &["rs"]) {
            let rel = path
                .strip_prefix(&root)
                .unwrap_or(&path)
                .to_string_lossy()
                .replace('\\', "/");
            out.push((rel, guard_core::production_code(&src)));
        }
        out.sort();
        out.dedup_by(|a, b| a.0 == b.0);
        out
    }

    /// `SITES` 投影成 [`dial_locality`] 要的那两栏。
    fn home() -> Vec<(&'static str, usize)> {
        SITES.iter().map(|(f, n, ..)| (*f, *n)).collect()
    }

    /// ★ `KR74D3` 正题：**monitor 侧的拨号锚点只长在登记的那个家里，一处都不多。**
    #[test]
    fn the_dial_only_happens_at_its_registered_home() {
        let corpus = crate_sources();
        assert!(
            corpus.len() >= CORPUS_FLOOR_FILES,
            "只扫到 {} 份源文件（地板 {CORPUS_FLOOR_FILES}）—— 抽取坏了，本条在空转",
            corpus.len()
        );
        // 登记表不许指向一份不在的文件（那会让那一行**静默地**不参与比对）。
        for (f, ..) in SITES {
            assert!(
                corpus.iter().any(|(n, _)| n == f),
                "`SITES` 登记着 {f}，而语料里没有这份文件 —— 它被挪走 / 改名了？\n\
                 登记表指向不存在的文件时，那一行等于不存在，而输出是绿的。"
            );
        }
        match dial_locality(&corpus, &home()) {
            Ok(n) => eprintln!(
                "〔拨号住址〕monitor 侧生产段锚点 {n} 处，全部住在登记的 {} 份文件里\
                 （{}）。**0 是终点，不是故障** —— 家变成空集那天 `russh` 同拍离开 manifest。",
                SITES.len(),
                SITES
                    .iter()
                    .map(|(f, n, ..)| format!("{f}={n}"))
                    .collect::<Vec<_>>()
                    .join(" · ")
            ),
            Err(e) => panic!("{e}"),
        }
    }

    /// ★ `KR74D3` **反向那半**：那把尺子得真的分得开「家里」与「家外」。
    ///
    /// # 没有它，本条是一场仪式
    ///
    /// 真树上今天 `strays` **恰好是空的**、处数**恰好对得上** ⇒ 把 [`dial_locality`] 里
    /// 那两支 `if` 整个删掉（或者把 `!strays.is_empty()` 写成 `strays.is_empty()` 之后
    /// 顺手把分支体换成 `Ok(0)`），**真树上的输出与判对了一模一样（绿）**。
    /// 那正是本仓从头到尾在治的形状：「判过了」与「压根没判」不可区分。
    ///
    /// ⚠ 合成语料里的文件名与数字都取**中性值**（`brief` 12 的 `6g`：别让断言取自夹具的名字）。
    #[test]
    fn the_locality_reader_can_tell_a_stray_from_a_home() {
        let pat = anchors()[0].clone();
        let filler = "x".repeat(CORPUS_FLOOR_BYTES);
        let mk = |rows: &[(&str, usize)]| -> Vec<(String, String)> {
            let mut v: Vec<(String, String)> = rows
                .iter()
                .map(|(f, n)| ((*f).to_string(), pat.repeat(*n)))
                .collect();
            v.push(("甲.rs".to_string(), filler.clone()));
            v
        };
        let h: &[(&str, usize)] = &[("乙.rs", 2)];

        // 正：家里恰好两处 ⇒ 过，并回真实总数。
        assert_eq!(
            dial_locality(&mk(&[("乙.rs", 2)]), h),
            Ok(2),
            "家里处数与登记一致，不许红"
        );

        // ① 家外多了一处 ⇒ 必须红，而且必须点到那份文件的名字。
        let e = dial_locality(&mk(&[("乙.rs", 2), ("丙.rs", 1)]), h)
            .expect_err("锚点长到家外面了，判据居然是绿的");
        assert!(
            e.contains("丙.rs"),
            "红了，但没点到长在家外面的那份文件：{e}"
        );

        // ② 家里多了一处（第七个地方也可以长在已登记的文件里）⇒ 必须红。
        let e2 =
            dial_locality(&mk(&[("乙.rs", 3)]), h).expect_err("家里的处数涨了，判据居然是绿的");
        assert!(e2.contains("对不上"), "红的理由不是处数对不上：{e2}");

        // ③ 家里少了一处 ⇒ 也必须红（登记腐烂比没有登记更糟）。
        assert!(
            dial_locality(&mk(&[("乙.rs", 1)]), h).is_err(),
            "登记说 2 处而实际 1 处，判据居然是绿的 —— 登记表腐烂不出声"
        );

        // ④ 家空了、锚点也没了 ⇒ **必须绿**。这一格是本模块与 daemon 那份的分水岭：
        //    抄了 daemon 的 `total == 0 ⇒ Err`，`K-P6` 做成的那天本条会变红。
        assert_eq!(
            dial_locality(&mk(&[]), &[]),
            Ok(0),
            "家变成空集是本件的**终点**，不许红 —— 别把 daemon 那条 `total == 0 ⇒ Err` 抄过来"
        );

        // ⑤ 语料太小 ⇒ 归**地板**管，不归上面那两支管。两格刻意分开，别并。
        assert!(
            dial_locality(&[], h).is_err(),
            "空语料居然判绿 —— 地板断言没接上"
        );
    }

    /// 登记表每条都要说清「它是什么」与「什么条件满足之后这一行就能删」。
    ///
    /// 同 `dial_move_judge::every_registered_dial_site_says_what_it_is_and_when_it_could_go`
    /// 那张表的纪律 —— 解锁条件那一栏是写给**下一个想删这行的人**看的。
    #[test]
    fn every_registered_dial_home_says_what_it_is_and_when_it_could_go() {
        for (file, n, what, unlock) in SITES {
            assert!(*n >= 1, "{file} 登记了 0 处锚点 —— 那这一行该整行删掉");
            assert!(
                what.chars().count() >= 20,
                "{file} 的说法太短，说不清那几处是什么"
            );
            assert!(
                unlock.trim().chars().count() >= 20,
                "{file} 没写**解锁条件** —— 要写的是「什么条件满足之后这一行就能删」，\
                 不是「为什么现在不能删」"
            );
        }
    }

    // ════════════════════════════════════════════════════════════════════════
    //  KR74D1 · 界面里还有多少 SSH，是一个数得出来的数
    // ════════════════════════════════════════════════════════════════════════

    /// 界面 crate 的 manifest。**编译期读，不走 `read_to_string`** ——
    /// 它与 `src-tauri/src/` 同属这一半，不是跨半边的边
    ///（`cross_half_edge_registry` 那张表管的是 monitor ↔ daemon）。
    const INTERFACE_MANIFEST: &str = include_str!("../Cargo.toml");

    /// **终点那面二值旗**：界面 crate 的 manifest 里，`russh` 家族的直接依赖有哪几个。
    ///
    /// 空 ⇒ 旗翻面 = `R32` 裁定一那句「`russh` 不出现在界面 crate 的依赖树里」成真。
    ///
    /// ⚠ **它量的是 manifest 里的直接依赖，不是 `cargo tree` 那一层** ——
    /// 两者 09-12 现打同值（`cargo tree -i russh` 只有一条边），
    /// 而**同值是今天的事实，不是本函数钉住的性质**。
    fn russh_deps_in(manifest: &str) -> Vec<String> {
        // 🔴 注释行走**共享原语** `guard_core::strip_hash_comment_lines`（TOML 的 `#` 整行注释
        // 与 shell/YAML 同一套），**不在这儿再写一份** —— 本函数的第一版内联了一个
        // `#` 过滤，`structural_scan.rs` 那条「剥注释的实现要登记」的判据 09-12 当场逮到它，
        // 逐字给的两条出路是「登记进 `TRANSFORMERS` 并写明理由 / 答不出来就改成调它」。
        // 这里答不出来（TOML 的整行注释与 YAML 逐字同形）⇒ 走第二条。
        let live = guard_core::strip_hash_comment_lines(manifest);
        let mut out: Vec<String> = Vec::new();
        for line in live.lines() {
            let t = line.trim_start();
            if !t.starts_with("russh") {
                continue;
            }
            let Some(eq) = t.find('=') else { continue };
            let name = t[..eq].trim();
            if name
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
            {
                out.push(name.to_string());
            }
        }
        out.sort();
        out.dedup();
        out
    }

    /// `ssh_source.rs` 在仓里的相对住址 —— 下面要拿它去问 git 历史。
    const DIAL_SITES_REL: &str = "src-tauri/src/ssh_source.rs";

    /// 找 `DIAL_SITES` 那张表表头的针 —— 🔴 **运行时拼，别写成字面量**。
    ///
    /// 承重，理由同 `scanning_guard_registry::ceiling_needle`：下面那个解析器要跑在
    /// **`ssh_source.rs` 的历史版本**上，而针一旦写成字面量，本文件里就多一处同形的串，
    /// 将来谁把这两份文件合并 / 把解析器搬过去，「解析到的是哪一处」就由先后决定 ——
    /// 而错的方向是**静默的绿**。
    fn dial_sites_needle() -> String {
        format!("const DIAL_{}: &[(", "SITES")
    }

    /// 一份 `ssh_source.rs` 源码文本里，`DIAL_SITES` 中 `moved == false` 的**处数合计**。
    ///
    /// 找不到那张表 ⇒ `None`（**不许默默当 0** —— 那正是件计划死值验要逮的那一形）。
    ///
    /// 口径与 `DIAL_SITES` 自己那条判据对齐：第二栏是处数、第三栏是「搬走了没有」，
    /// 只把 `false` 那几行的处数加起来。解析按行走状态机（rustfmt 把每个字段单占一行），
    /// 字符串字段里的多行正文一律落在 `Idle` 那一档被跳过。
    fn unmoved_dial_sites_in(src: &str) -> Option<usize> {
        let at = src.find(dial_sites_needle().as_str())?;
        #[derive(PartialEq)]
        enum S {
            Idle,
            WantCount,
            WantMoved,
        }
        let mut st = S::Idle;
        let mut count = 0usize;
        let mut total = 0usize;
        for line in src[at..].lines().skip(1) {
            let t = line.trim();
            if t == "];" {
                return Some(total);
            }
            match st {
                S::Idle => {
                    if t == "(" {
                        st = S::WantCount;
                    }
                }
                S::WantCount => {
                    if let Some(d) = t.strip_suffix(',') {
                        if !d.is_empty() && d.chars().all(|c| c.is_ascii_digit()) {
                            count = d.parse().ok()?;
                            st = S::WantMoved;
                        }
                    }
                }
                S::WantMoved => {
                    if t == "false," {
                        total += count;
                        st = S::Idle;
                    } else if t == "true," {
                        st = S::Idle;
                    }
                }
            }
        }
        None
    }

    /// 同一个数的**另一份来源**：真常量，不经过文本解析。
    ///
    /// 🔴 它与 [`unmoved_dial_sites_in`] 是本模块的**两条腿**：
    /// 解析器偏了 / 被改成恒 0 ⇒ 两条腿对不上 ⇒ 当场红。
    fn unmoved_from_constant() -> usize {
        crate::ssh_source::dial_move_judge::DIAL_SITES
            .iter()
            .filter(|(_, _, moved, ..)| !*moved)
            .map(|(_, n, ..)| *n)
            .sum()
    }

    /// ★ `KR74D1`：**「界面里还有多少 SSH」是一个数得出来的数，而且它被印出来。**
    ///
    /// 两条腿对拍 —— 这一格就是件计划那条死值验（**把量法改成恒 0 必须红**）的落点：
    /// 量法一旦恒 0，棘轮会以为这件事做完了（0 永远降得下去），
    /// 而这条对拍会当场说「解析器说 0，登记表说 6」。
    #[test]
    fn the_interface_side_ssh_debt_is_one_number_and_it_is_printed() {
        let me = include_str!("ssh_source.rs");
        let parsed = unmoved_dial_sites_in(me).unwrap_or_else(|| {
            panic!(
                "在 `{DIAL_SITES_REL}` 里找不到 `DIAL_SITES` 那张表（针：{}）——\n\
                 表被改了名 / 挪了文件 / 换了列数 ⇒ 本模块三条判据同时失去被测对象。\n\
                 🔴 **不许让它默默当 0** —— 那等于宣布「已经解耦了」。",
                dial_sites_needle()
            )
        });
        let truth = unmoved_from_constant();
        assert_eq!(
            parsed, truth,
            "两条腿对不上：文本解析器读出 {parsed}，而真常量 `DIAL_SITES` 合计 {truth}。\n\
             ⇒ 解析器偏了（历史面上的读数会跟着一起偏，而真树上棘轮**照样绿**），\
             或者有人把量法改成了一个定值。\n\
             🔴 这一格就是「量法恒 0」那一刀的落点：恒 0 ⇒ 棘轮永远说「已经解耦了」。"
        );

        let deps = russh_deps_in(INTERFACE_MANIFEST);
        eprintln!(
            "〔界面侧 SSH 债 · 09-12 立〕过程那个数 = **{truth}** 处拨号还没搬走\
             （口径：`ssh_source.rs::dial_move_judge::DIAL_SITES` 里 `moved == false` 那几行的\
             `connect_session(` **调用点**合计，由 `K-P6b` 那条判据从生产段源码派生）；\
             ⚠ **它买不到**：`use russh` 的行数（那是写法不是依赖）· \
             传递依赖里有没有 `russh`（本模块只看 manifest 的直接依赖）· \
             用 `use` 别名或宏藏起来的调用点。\n\
             〔终点那面二值旗〕`src-tauri/Cargo.toml` 里 `russh` 家族的直接依赖：{deps:?}\
             —— 空了才算 `R32` 裁定一那句话成真。"
        );

        // 终点与过程咬在一起：数还没归零，旗就必须还立着。
        // 反过来那一半（旗翻面了而数还没归零）由上面的对拍与棘轮各自接住。
        assert!(
            truth == 0 || !deps.is_empty(),
            "还有 {truth} 处拨号住在界面里，而 manifest 里已经没有 `russh` 了 ——\n\
             这两句话不可能同时为真：要么那几处拨号其实已经搬走了（改 `DIAL_SITES`），\
             要么依赖被删了而代码还在（那编不过，说明本条的量法坏了）。"
        );
    }

    // ════════════════════════════════════════════════════════════════════════
    //  KR74D2 · 那个数只许降（棘轮）
    // ════════════════════════════════════════════════════════════════════════

    /// 跑一条**只读**的 git，回它的 stdout。
    ///
    /// 🔴 **fail-closed（panic）**：读不到历史时必须红。
    /// 「历史面是空的」与「棘轮没被倒着转」在输出上一模一样 —— 那正是本条要治的形状。
    /// 两种「问不到」分开报：进程起不来（PATH 里没有 git）与 git 自己说不行。
    fn git_read(root: &Path, args: &[&str]) -> String {
        let out = std::process::Command::new("git")
            .current_dir(root)
            .args(args)
            .output()
            .unwrap_or_else(|e| {
                panic!(
                    "起不来 `git`（{e}）—— 本条拿 git 历史当权威，问不到就**不许猜一个出来**。\n\
                     ⚠ 这一支不是「git 说不知道」，是**进程都没起来**（PATH 里没有它）。"
                )
            });
        assert!(
            out.status.success(),
            "`git {}` 在 {} 上退出码 {:?} —— 这一支是「git 起来了、但它说不行」。\n\
             git 自己说：{}\n\
             ⇒ 常见来路：这棵树不在版本控制里 · 浅克隆（`--depth`）把历史截掉了。\n\
                两种都要修环境，**不许把本条改成读不到就跳过**（那等于把闸拆了）。",
            args.join(" "),
            root.display(),
            out.status.code(),
            String::from_utf8_lossy(&out.stderr).trim()
        );
        String::from_utf8_lossy(&out.stdout).into_owned()
    }

    /// `ssh_source.rs` 每一个历史版本上那个数的读数 —— `(短 sha, 处数)`，外加**读不出来**的份数。
    ///
    /// ⚠ 读不出来的**不静默丢掉**：份数一起回，由调用方连读数印出来。
    /// 合法的一形：`DIAL_SITES` 是 `K-P6b`（09-06）才立的，早于它的提交里那张表不存在。
    /// 而**全部读不出来**（git 坏了 / 表改了名）那一形，由下面那条**历史面地板**接住。
    fn ratchet_history(root: &Path) -> (Vec<(String, usize)>, usize) {
        let mut rows = Vec::new();
        let mut unparsed = 0usize;
        for sha in git_read(root, &["log", "--format=%h", "--", DIAL_SITES_REL]).split_whitespace()
        {
            let spec = format!("{sha}:{DIAL_SITES_REL}");
            let blob = std::process::Command::new("git")
                .current_dir(root)
                .args(["show", &spec])
                .output()
                .ok()
                .filter(|o| o.status.success())
                .map(|o| String::from_utf8_lossy(&o.stdout).into_owned());
            match blob.as_deref().and_then(unmoved_dial_sites_in) {
                Some(n) => rows.push((sha.to_string(), n)),
                None => unparsed += 1,
            }
        }
        (rows, unparsed)
    }

    /// 纯算子：今天的读数 `today` 对着历史面 `hist`，棘轮有没有**被倒着转**。
    ///
    /// 回**历史上最低的那一档**当见证；没被倒转 ⇒ `None`。
    ///
    /// 🔴 **单独成函数**，理由与 `scanning_guard_registry::ratchet_backslide` 逐字相同：
    /// 真树上今天 `today` 恰好**等于**历史最低档 ⇒ 把 `>` 写成 `<`、把 `min_by_key`
    /// 写成 `max_by_key`、或者把 `hist` 传成空的，**输出与判对了一模一样（绿）**。
    /// [`the_dial_debt_ratchet_reader_can_tell_a_raise_from_a_drop`] 拿合成读数钉住这一格。
    ///
    /// ⚠ **诚实边界**：`hist` 为空时它回 `None`（= 绿）。**空历史那一格不归它**，
    /// 归 [`the_dial_debt_ratchet_never_turns_backwards`] 里那条**地板**。两格刻意分开。
    fn ratchet_backslide(today: usize, hist: &[(String, usize)]) -> Option<(String, usize)> {
        let low = hist.iter().min_by_key(|(_, v)| *v)?;
        if today > low.1 {
            Some(low.clone())
        } else {
            None
        }
    }

    /// 历史面地板：`DIAL_SITES` 立于 `K-P6b`（09-06），09-12 现打 **5** 份历史版本读得出。
    ///
    /// 🔴 **不许靠调低它让今天好过** —— 那是把闸拆了，而拆完输出还是绿的。
    const HISTORY_FLOOR: usize = 5;

    /// ★ `KR74D2` 正题：**那个数不许比它在历史上出现过的最低档还高。**
    ///
    /// 🔴 **为什么不钉一个固定上限**：`K-R38`（09-06）亲手证过，钉在同一份文件里的上限
    /// **抬一格就过** —— 降到低点再涨回来仍然合规。⇒ 本条比的是**历史上出现过的最低档**，
    /// 那个更低的档还在 git 里，提交了也不会变绿。
    #[test]
    fn the_dial_debt_ratchet_never_turns_backwards() {
        let root = repo_root();
        let today = unmoved_from_constant();
        let (hist, unparsed) = ratchet_history(&root);

        // 抽取器自检：**历史面不许是空的 / 短的**。
        //
        // 🔴 这一格是本条的地基：`ratchet_backslide` 拿到空历史时回 `None`（绿），
        // 于是「git 读不到历史」与「棘轮没被倒着转」**输出完全相同**。
        // 会把历史面弄空的真实来路：浅克隆（`--depth 1`）· `DIAL_SITES` 被改了名
        //（针是按名字认的）· `ssh_source.rs` 被拆开挪走了（`DIAL_SITES_REL` 就馊了）。
        assert!(
            hist.len() >= HISTORY_FLOOR,
            "只从 git 历史里读出 {} 份 `{DIAL_SITES_REL}` 的旧版本读数（地板 {HISTORY_FLOOR}，\
             09-12 实测 5 份，另有 {unparsed} 份读不出来）——\n\
             ⇒ **本条此刻是空转的**：历史面一空，下面那一格恒真地绿。\n\
             常见来路：① 浅克隆把历史截掉了（要 `fetch-depth: 0`）；\n\
                       ② `DIAL_SITES` 被改了名（针是按名字认的）；\n\
                       ③ `ssh_source.rs` 挪了位置 ⇒ `DIAL_SITES_REL`（`{DIAL_SITES_REL}`）馊了。\n\
             🔴 **不许靠调低地板让今天好过**。",
            hist.len()
        );

        // 只在 `--nocapture` 下可见 —— 射程与历史面本身也是读数（`brief` 13b：现算，别写死）。
        eprintln!(
            "〔拨号债棘轮 · 本趟的历史面〕{} 份旧版本读得出（读不出 {unparsed} 份）· 今天 {today}\n  {}",
            hist.len(),
            hist.iter()
                .map(|(s, n)| format!("{s}={n}"))
                .collect::<Vec<_>>()
                .join("  ")
        );

        if let Some((sha, was)) = ratchet_backslide(today, &hist) {
            panic!(
                "🔴 **棘轮被倒着转了**：界面侧还没搬走的拨号今天是 {today} 处，\
                 而它在 `{sha}` 上是 {was} 处。\n\
                 `R32` 裁定一逐字：这个数**只许降** —— 从今天起是机器在守，不再是纪律。\n\
                 ⇒ 处置：把新加的那一处拨号拿掉，或者把它交给后端\
                 （`K-P6` 三拍的形状写在 `DECISIONS.md#R32` 裁定三）。\n\
                 ⚠ 提交了也不会变绿：本条比的是**历史上出现过的最低档**，那个更低的档还在。\n\
                 ⚠ 也别去改 `DIAL_SITES` 的数字了事：`six_of_the_seven_dial_sites_are_still_in_this_process`\
                 会拿生产段源码逐格对拍，而本条拿 git 历史对拍 —— 两条都得骗过去才行。"
            );
        }
    }

    /// ★ `KR74D2` 的**反向那半**：那把比较尺子，得真的分得开「抬上去」与「降下来」。
    ///
    /// # 没有它，本条是一场仪式
    ///
    /// 真树上今天那个数**恰好等于**历史最低档（09-12 现打：历史面 `7 7 7 7 6`，今天 6，
    /// 余量 0）。⇒ 把 [`ratchet_backslide`] 里的 `>` 写成 `<`、把 `min_by_key` 写成
    /// `max_by_key`、或者让历史面传成空的 —— **真树上的输出与判对了一模一样（绿）**。
    /// `K-R38` 那次实打的读数逐字：正题照样绿、只有反向红 ⇒ **反向那半是承重的**。
    ///
    /// ⚠ 夹具里的 sha 与数字都取**中性值**，断言比的是喂进去的那个值本身。
    #[test]
    fn the_dial_debt_ratchet_reader_can_tell_a_raise_from_a_drop() {
        let hist: Vec<(String, usize)> = [("aaa", 8), ("bbb", 7), ("ccc", 6)]
            .iter()
            .map(|(s, v)| ((*s).to_string(), *v))
            .collect();
        let low = ("ccc".to_string(), 6);

        // 正：抬上去 ⇒ 必须逮到，而且点的是**历史最低**那一档（不是最近那一档）。
        // 🔴 「点最低那一档」是承重的：点最近那一档的话，抬上去之后只要**提交一次**，
        // 最近那一档就变成抬过的值 ⇒ 下一趟当场变绿，棘轮咬完就松。
        assert_eq!(
            ratchet_backslide(7, &hist),
            Some(low.clone()),
            "7 高于历史最低档 6 —— 这一格没逮到，说明比较写反了或者点错了档"
        );
        assert_eq!(
            ratchet_backslide(99, &hist),
            Some(low),
            "点的必须是**历史最低**那一档，不是最近的那一档"
        );

        // 平 / 降：棘轮正着转，一格都不许红。
        assert_eq!(
            ratchet_backslide(6, &hist),
            None,
            "与历史最低档持平，不许红"
        );
        assert_eq!(
            ratchet_backslide(5, &hist),
            None,
            "降下去正是要买的动作，不许红"
        );
        assert_eq!(
            ratchet_backslide(0, &hist),
            None,
            "降到 0 —— 那是本件的终点，一格都不许红"
        );

        // 🔴 **空历史 ⇒ 它回 `None`（绿）**，这一格是**故意钉住的诚实边界**，不是缺陷：
        // 接住「历史面读不到」的是 `the_dial_debt_ratchet_never_turns_backwards` 里那条**地板**。
        // 钉在这里，是为了不让谁把这一支改成 panic 之后顺手把那条地板删掉 ——
        // 那样一来两格并成一格，而并完之后**没有任何输出会变**。
        assert_eq!(
            ratchet_backslide(usize::MAX, &[]),
            None,
            "空历史这一支归**地板**管，不归这把尺子管；两格刻意分开，别并"
        );

        // 解析器那一半：针是运行时拼的，拿它自己拼出来的文本正反各喂一遍。
        let mk = |rows: &[(usize, bool)]| {
            let mut s = format!("    {}\n", dial_sites_needle());
            for (n, moved) in rows {
                s.push_str(&format!(
                    "        (\n            \"甲.rs\",\n            {n},\n            {moved},\n\
                     \x20           \"说法\",\n            \"解锁\",\n        ),\n"
                ));
            }
            s.push_str("    ];\n");
            s
        };
        assert_eq!(
            unmoved_dial_sites_in(&mk(&[(4, false), (1, false), (1, false)])),
            Some(6),
            "三行都没搬、处数 4/1/1，解析器数不出 6"
        );
        assert_eq!(
            unmoved_dial_sites_in(&mk(&[(4, true), (1, false), (1, false)])),
            Some(2),
            "`moved == true` 那一行必须**不**计入 —— 搬走了就不算债了"
        );
        assert_eq!(
            unmoved_dial_sites_in(&mk(&[])),
            Some(0),
            "空表要回 Some(0)，与「找不到那张表」（None）**不是一回事**"
        );
        assert_eq!(
            unmoved_dial_sites_in("没有那张表的一段文本"),
            None,
            "找不到表就要回 None，**不许默默当 0**"
        );

        // 二值旗那一半：正反各喂一遍。
        assert_eq!(
            russh_deps_in("[dependencies]\nrussh = \"0.61\"\nrussh-sftp = \"2\"\ntokio = \"1\"\n"),
            vec!["russh".to_string(), "russh-sftp".to_string()],
            "manifest 里有两个 russh 家族的直接依赖，旗子却看不见"
        );
        assert!(
            russh_deps_in("[dependencies]\ntokio = \"1\"\n# russh = \"0.61\"\n").is_empty(),
            "只剩注释掉的那一行 ⇒ 旗必须翻面；注释被当成依赖的话，这件事永远做不完"
        );
    }
}
