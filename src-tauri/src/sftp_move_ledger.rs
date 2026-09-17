//! `K-R78`：**那 14 处 SFTP 拨号今天各自卡在哪** —— 一张带读数、由源码派生的登记。
//!
//! # 它为什么在这里
//!
//! `DECISIONS.md#R32` 裁定三第 2 步把「C 类 —— SFTP 整个动作搬进后端」排成一拍，
//! 而 `K-R78` 派工前 PM 现打把 C 类拆成三类（甲 一次性部署 · 乙 池化文件操作 · 丙 MCP 读配置），
//! 判词是「**甲最便宜，乙另立**」。
//!
//! **本件现打把「甲最便宜」这半句也证伪了**，而理由不是工作量，是**四条结构性的挡路石**
//! （逐条住下面 `REGISTERED` 的 `甲` 那几行，每条带一个**逐字校验位**）。
//! ⇒ 甲那一半**本拍没有搬**，`KR78D1` **没有达成**；本模块是那件事的**可核底账**，
//! 不是它的替代品。
//!
//! # 它钉的是什么、不钉什么
//!
//! **钉**：这张表里每一个数都**从源码派生再逐格比对**（`connect_sftp` 的调用点普查 ·
//! 乙那三样要跟着过界的东西 · 记分牌上 `sftp.rs` 那一格），改坏任何一个数当场红。
//!
//! **不钉**（诚实边界，逐条写死）：
//!
//! - **daemon 那一侧的事实本模块一个字都没机检** —— `readonly_guard` 的模式表、
//!   `g6_dependency_signoff` 的判档闭集，都住对面那棵树。从这里 `include_str!` 过去
//!   会**新增一条跨半边编译期边**，那张登记表（`cross_half_edge_registry`）不在本件写区里。
//!   ⚠ 这与 `ssh_source::dial_move_judge` 立乙半时给的理由**逐字同一条**，不是本件新编的借口。
//!   ⇒ 那两条挡路石在下表里带的是**符号住址**，不是校验位；**它们没有牙**。
//! - **本模块不给任何一半编闹钟**：表里没有一格是「等到某天某条件成立就会响」。
//!   一条不会响的闹钟比一句过期的话更贵（`backend/mod.rs` 逐字）。
//!   表里每一格能做的只有一件事：**它引的那句话在盘上变了，它就红，逼人回来重读。**
//!
//! # 量具的作用域 —— 🔴 本表两把尺子量的**不是同一个东西**，别混
//!
//! - 记分牌（`ssh_source::dial_move_judge::DIAL_SITES` ＋ `dial_home_registry` 那条棘轮）
//!   数的是 **`connect_session(` 的调用点**：`sftp.rs` 在它那里只占 **1 处**
//!   —— 因为整份 `sftp.rs` 只有 `connect_sftp` 一个函数去握手。
//! - 本表数的是 **`connect_sftp(` 的调用点**：**14 处 / 4 份文件**，那才是「有多少条路要改」。
//!
//! 「6」与「14」都对，它们回答的是两个问题。把其中一个当成另一个，就是本区最高频的那一类错。

#[cfg(test)]
mod tests {
    use guard_core::production_code;

    // ── 语料 ─────────────────────────────────────────────────────────────────
    //
    // 只读**本 crate 自己**那四份 + 仓内耐久文档，一条跨半边的边都不长。

    const POOL_SRC: &str = include_str!("sftp_pool.rs");
    const SFTP_SRC: &str = include_str!("sftp.rs");
    const MCP_SRC: &str = include_str!("mcp.rs");
    const ACCT_SRC: &str = include_str!("acct_iso_deploy.rs");
    const INVARIANTS: &str = include_str!("../../doc/INVARIANTS.md");
    const PROFILE_SRC: &str = include_str!("profile_installer.rs");
    const LOCAL_DAEMON_SRC: &str = include_str!("local_daemon.rs");
    const BACKEND_SRC: &str = include_str!("backend/mod.rs");

    /// 语料地板：低于这个字节数就判「语料没喂进来」，而不是「一处都没有」。
    ///
    /// 🔴 这条是下面 `an_empty_corpus_makes_the_census_red_by_itself` 的被测对象 ——
    /// 「一处都没扫到」与「根本没扫」在终端上一模一样，这条地板就是把它们分开的那一刀。
    const CORPUS_FLOOR_BYTES: usize = 60_000;

    /// 一份源码文本的生产段里，某个函数名的**调用点**处数 —— 剔掉定义行本身。
    ///
    /// ⚠ 这把尺子**数不到**：`use` 别名、函数指针、宏里拼出来的调用。
    /// 与 `dial_move_judge::call_sites` 是同一个洞，**不声称堵住**。
    fn call_sites(code: &str, name: &str) -> usize {
        let needle = format!("{name}(");
        let mut n = 0usize;
        let mut from = 0usize;
        while let Some(rel) = code[from..].find(needle.as_str()) {
            let at = from + rel;
            from = at + needle.len();
            if code[..at].ends_with("fn ") {
                continue; // 定义行，不是调用点
            }
            n += 1;
        }
        n
    }

    /// 四份语料的生产段（剥掉 `#[cfg(test)]` 段）。
    fn corpus() -> Vec<(&'static str, String)> {
        vec![
            ("sftp.rs", production_code(SFTP_SRC)),
            ("sftp_pool.rs", production_code(POOL_SRC)),
            ("mcp.rs", production_code(MCP_SRC)),
            ("acct_iso_deploy.rs", production_code(ACCT_SRC)),
        ]
    }

    // ── 读数（死值验的落点）─────────────────────────────────────────────────
    //
    // 下面每一个数都由上面那把尺子从语料里派生出来再逐格比对。
    // 🔴 **改坏任何一个 ⇒ 必须红。** 这就是「可核」与「散文」的分界。

    /// `connect_sftp(` 的调用点普查：`(文件, 处数, 它属哪一类 · 是什么)`。
    ///
    /// 🔴 **PM 派工单里那句「9 处 / 4 文件」现打是错的** —— 实打 **14 处 / 4 文件**。
    /// 差在两处：派工单那张分类表自己加起来是 5 ＋ 4 ＋ 3 = 12，而 `sftp.rs` 里
    /// 另有 **2 处**它一类都没归（`ensure_daemon_deployed` 自动部署 ·
    /// `remove_remote_file` 删远端会话 jsonl）。
    const DIAL_CENSUS: &[(&str, usize, &str)] = &[
        (
            "sftp.rs",
            6,
            "甲 4 条命令（`deploy_remote_daemon` · `uninstall_remote_daemon` · \
             `install_remote_ccm_helper` · `uninstall_remote_ccm_helper`）\
             ＋ **派工单没归类的 2 处**：`ensure_daemon_deployed`（连接流程里的自动部署）\
             与 `remove_remote_file`（F11 删远端会话 jsonl）",
        ),
        (
            "sftp_pool.rs",
            4,
            "乙 —— 池化文件操作。4 处里有 2 对是同一个函数的「首次建」与「死连重建」",
        ),
        ("mcp.rs", 3, "丙 —— 读/写远端项目 `.mcp.json`"),
        (
            "acct_iso_deploy.rs",
            1,
            "甲 —— 部署 vendored `cc-acct-iso` 到远端",
        ),
    ];

    /// 乙今天的形状：`(几个 `#[tauri::command]`, 几处拨号)`。
    ///
    /// 这两个数是 `KR78D2` 要的「读数」本体。
    const POOL_SHAPE: (usize, usize) = (11, 4);

    // ── 登记表 ───────────────────────────────────────────────────────────────

    /// **本件的底账**：`(哪一半, 点名的那样东西, 逐字校验位, 它在挡什么)`。
    ///
    /// 「逐字校验位」= 一段**今天真在盘上**的原文；它变了这一行就红，逼人回来重读，
    /// **而不是**在某个日子自己响。空串 = 这一行**没有牙**，只有符号住址（住对面那棵树）。
    ///
    /// ⚠ 表名 `REGISTERED` 是 `scanning_guard_registry::TABLE_DECLS` 闭集里的成员之一，
    /// 刻意不起新名字（`DECISIONS.md#R33` 裁定一：新名字要同拍动那张闭集，而它不在本件写区）。
    const REGISTERED: &[(&str, &str, &str, &str)] = &[
        // ── 乙：`KR78D2` 要的那三样 ───────────────────────────────────────
        (
            "乙",
            "进度通道",
            "Channel<TransferProgress>",
            "它是 **tauri 的 IPC channel**，而 `TransferProgress` 还带 `ts_rs` 那个 `TS` 派生、\
             导出给前端。⚠ 上一句刻意**不写**那个派生的全限定名（模块路径 ＋ `::` ＋ 类型名）——\
             `test/generated-boundary-guard.vitest.ts::tsDerivingSources` 按**整仓文本子串**取人群，\
             本文件写全了就会被当成「又一份派生了它的源文件」而把那条计数判据顶红。\
             〔09-12 实打撞到过：npm 那格 `实得 33 … expected 33 to be 32`。〕\
             ⇒ 搬进后端要先回答「一条 tauri channel 怎么跨进程」：\
             daemon 那棵树**根本没有 tauri 这个依赖**（现打：`remote-daemon-proto/Cargo.toml` \
             的 `[dependencies]` 里一条 tauri 都没有），\
             而 monitor 侧 `backend/` 那道宿主无关守卫 `the_backend_layer_stays_host_agnostic` \
             的禁词表里已经有 `Emitter` / `State<` / `.emit(` 一族。",
        ),
        (
            "乙",
            "per-origin 连接池",
            "static POOL: std::sync::OnceLock<Mutex<HashMap<String, Slot>>>",
            "池是**进程全局**的 `OnceLock<Mutex<HashMap<String, Slot>>>`，靠界面进程活着才有意义。\
             而今天界面 ↔ 本机后端**唯一**那条一次性传输是 \
             `backend::observe::local_query::run_query` —— 它 **exec 一次子进程拿 stdout**。\
             ⇒ 池搬过去会**活不过一次调用**：要么后端那侧先有一条常驻通道，\
             要么池这件事整个重想。**这不是工作量问题，是形状问题。**",
        ),
        (
            "乙",
            "死连接重建重试",
            "Err(e) if looks_like_dead_conn(&e) =>",
            "op 失败且像连接死亡 ⇒ 重建一次再试。校验位刻意钉**那条 match 臂**而不是那个函数名：\
             函数名改掉是编译错（判据轮不到说话），而**把那条臂整条摘掉**编得过、\
             只留一个 unused 警告 —— 那才是这条登记要拦的那一形。\
             它与上一条是**同一个状态**的两面：没有池就没有「这条连接死了」这回事。\
             ⇒ 两样必须一起过界，拆不开。",
        ),
        // ── 甲：本件现打的四条挡路石 ─────────────────────────────────────
        (
            "甲",
            "daemon 只读铁律 I7",
            "后端不许改动用户既有数据",
            "甲里 `install_remote_ccm_helper` / `uninstall_remote_ccm_helper` 改的正是\
             **用户既有的** `~/.bashrc`（备份 → 覆盖写 → 读回校验 → 回滚）。\
             把它搬进后端 = **daemon 进程自身**去改用户既有数据 ⇒ 与铁律 I7 正面撞。\
             🔴 **这是用户拍的红线，实现方不自批** —— `DECISIONS.md#R32` 通篇没有处置这一格。\
             ⚠ 更要紧的是：`readonly_guard` 的模式表全是 `fs::` / `File::` / `OpenOptions` \
             命名空间（本模块**没有机检**这一句，住址 \
             `remote-daemon-proto/src/readonly_guard.rs::FS_MUTATION_PATTERNS`）⇒ \
             SFTP 那套写**一条都不匹配** ⇒ 真搬过去，**机检不会红**，是一次静默越线。\
             〔🔴 `K-R79`（09-12）订正，**原文一个字没动，是历史不是错误**：\
             最后那半句「真搬过去，机检不会红」**今天不成立了**。\
             `FS_MUTATION_PATTERNS` 那张表**仍然**全在 `fs::` / `File::` / `OpenOptions` \
             命名空间（`K-R79` 刻意不往里塞 SFTP 方法名，那是件计划点名的失效方向）——\
             变的是**旁边多了一层**：`readonly_guard::remote_write_layer`，\
             它按「取得远端写能力」＋「SFTP v3 协议操作的方法形」两张网扫 daemon 整棵生产段。\
             `K-R79` 现打过这一刀：往 daemon 生产段插一处会话类型 + 协议打开标志 + 改名，\
             **新层当场红并点名那份文件，而老那两层一格没红** —— 前半句因此仍是真的。\
             ⇒ 读这一行时把结论换成：**今天会红；这条挡路石从「机检看不见」降级成\
             「机检看得见、而『许不许』还没裁」**（`ROADMAP.md#KU31` 仍未裁）。\
             〔本订正**同样没有牙**：理由与本行原有那条逐字同一句〕〕",
        ),
        (
            "甲",
            "依赖签字闭集容不下这一档",
            "",
            "daemon 那棵树只有 `russh`，**没有** `russh-sftp`（现打：`russh-sftp = \"2\"` \
             只在 `src-tauri/Cargo.toml`）。加它 ⇒ \
             `remote-daemon-proto/src/readonly_guard.rs::g6_dependency_signoff::SIGNED` \
             必须同拍加一行（「新加一条而没签字 ⇒ 当场红」）。\
             而那张表的判档是**闭集三档**（`已量·有写面` / `已量·未见写面` / `未量·靠用法签字`），\
             `已量·有写面` 那一档的定义逐字要求写清「**凭什么进不了发布二进制**」—— \
             ⇒ **没有一档容得下「有写面、而且发布二进制里就是要它写」**。\
             〔本行**没有牙**：住对面那棵树，不长跨半边的边。〕\
             〔🔴 `K-R79`（09-12）订正，**原文一个字没动**：最后那句\
             「没有一档容得下」**今天不成立了** —— 判档闭集已从三档加到四档，\
             第四档逐字叫 `已量·有写面·就是要它写`，住 \
             `remote-daemon-proto/src/readonly_guard.rs::g6_dependency_signoff::VERDICTS`。\
             ⚠ **加档不等于开了口子**：那一档自带两条机检的边界 ——\
             ① 签字里必须逐字点名一条**本文件里真存在**的判据（`边界判据：` ＋ 反引号名）；\
             ② 名字里带远端写协议词（`sftp`/`scp`/`rsync`/`webdav`）的依赖**不许**判进\
             「没量过 / 没见写面」那两档。\
             ⇒ 读这一行时把结论换成：**闸已经修好，剩下的是「许不许」那一问**\
             （`ROADMAP.md#KU31`，用户还没裁）。〕",
        ),
        (
            "甲",
            "别名 snippet 只许有一个家",
            "vec![\"sftp.rs\".to_string()]",
            "`profile_installer.rs::the_alias_snippet_has_exactly_one_home_in_the_rust_tree` \
             断言 `shared/ccm-aliases.sh` 在 monitor `src` 树里**恰好一处**，就是 \
             `sftp.rs::CCM_WRAPPER_SNIPPET`；同文件 \
             `the_posix_arm_borrows_the_remote_implementation_instead_of_growing_a_second_one` \
             另断言**本机**那条 POSIX 路借的就是远端这一份（三个符号各恰好 1 次）。\
             ⇒ 把 `install_remote_ccm_helper` 整条搬走：留下第二份 snippet ⇒ 前者红；\
             把 snippet 一起搬走 ⇒ 本机那条路失去实现，后者红。\
             **出路只有一条**：先立一个两侧共用的 crate —— 而那要 `shared_crate_registry` 签字，\
             不在本件写区。",
        ),
        (
            "甲",
            "要部署的那份字节住在界面这一侧",
            "crate::sftp::daemon_binary(std::env::consts::ARCH)",
            "远端 daemon 的字节由 `src-tauri/build.rs` 的 `embedded_daemons` cfg 内嵌进 \
             **monitor** 这份二进制（`sftp::daemon_binary`），而它**不止部署路在用**：\
             `local_daemon.rs` 释放本机后端时读的是同一份。\
             ⇒ 字节搬不走 ⇒ 后端要部署，字节得**从界面递过去** ⇒ \
             过界物就不再是 `R32` 裁定二第 ③ 种那个干净的 `{origin, 动作} → 只回结果`。\
             **这一格要 PM 拍**：是认下「动作带载荷」，还是让后端自己去取那份字节\
             （daemon 侧 `sidecars/codepicture/fetch.rs` 有同形的拉取协议，但它自陈\
             「今天那三样一样都没有」）。",
        ),
        (
            "甲",
            "SFTP 写原语是三类共用的",
            "pub(crate) async fn upload_atomic_verified",
            "`connect_sftp` / `upload_atomic` / `upload_atomic_verified` / `ensure_dir_all` / \
             `read_optional` 的消费者横跨甲乙丙 ＋ 那 2 处没归类的。\
             ⇒ **只搬甲**，两个 crate 里就各有一份 SFTP 写层（同一件事两个家，本区最贵的那条病）；\
             要不重复就得连乙丙一起搬，而那正是件计划逐字禁掉的\
             「为了让棘轮降一格把乙硬塞进本件」。",
        ),
    ];

    /// 判据主体：普查一遍语料，回 `Err(说法)` = 红。
    ///
    /// **纯函数**（语料由调用方给）⇒ 阳性/阴性两个方向都切得动，
    /// 这正是下面那条反向自检能存在的前提。
    fn census(corpus: &[(&str, String)]) -> Result<usize, String> {
        let bytes: usize = corpus.iter().map(|(_, c)| c.len()).sum();
        if bytes < CORPUS_FLOOR_BYTES {
            return Err(format!(
                "语料只有 {bytes} 字节（地板 {CORPUS_FLOOR_BYTES}）—— 本判据此刻在空转。\n\
                 「一处都没扫到」与「根本没扫」在终端上一模一样。"
            ));
        }
        let mut total = 0usize;
        for (name, code) in corpus {
            let got = call_sites(code, "connect_sftp");
            let Some((_, want, what)) = DIAL_CENSUS.iter().find(|(f, ..)| f == name) else {
                return Err(format!(
                    "语料里有 `{name}`，而 `DIAL_CENSUS` 里没有它这一行 —— 表与人群脱钩了。"
                ));
            };
            if got != *want {
                return Err(format!(
                    "`{name}` 的 `connect_sftp` 调用点实打 {got} 处，表上写 {want} 处。\n\
                     表上那一行说的是：{what}\n\
                     ⇒ 要么这一处真的搬走/长出来了（改表，并在件里交代），\
                     要么这张表已经腐了。"
                ));
            }
            total += got;
        }
        if total == 0 {
            return Err(
                "生产段里一处 `connect_sftp` 调用点都没有 —— 那不是搬完了，是没扫到。".into(),
            );
        }
        Ok(total)
    }

    /// `KR78D2` · `KR78D3` 的共同地基：**这张表里的每一个数都还是真的。**
    ///
    /// 死值验（`M1`）：把 `DIAL_CENSUS` 里任一处数改坏 ⇒ 本条红并点名那一份文件。
    #[test]
    fn every_reading_this_ledger_quotes_is_derived_from_the_tree() {
        let total = census(&corpus()).expect("普查");
        assert_eq!(
            total, 14,
            "`connect_sftp` 的调用点合计应当是 14 处（4 份文件）—— 实得 {total}。\n\
             ⚠ 派工单写的「9 处」现打是错的，来历见 `DIAL_CENSUS` 头注。"
        );
    }

    /// **反向那半**：喂一份空语料，判据必须**自己先红**。
    ///
    /// 没有这一条，「一处都没扫到」就会被读成「全搬完了」。
    #[test]
    fn an_empty_corpus_makes_the_census_red_by_itself() {
        let e = census(&[]).expect_err("空语料必须红");
        assert!(e.contains("空转"), "空语料该报「在空转」，实得：{e}");
    }

    /// `KR78D2` 正题：乙今天几个命令、几处拨号 —— **两个数都从源码派生**。
    ///
    /// 死值验（`M2`）：把 `POOL_SHAPE` 任一格改坏 ⇒ 本条红。
    #[test]
    fn the_pool_shape_this_ledger_registers_is_still_what_is_on_disk() {
        let prod = production_code(POOL_SRC);
        assert!(
            prod.len() > 15_000,
            "`sftp_pool.rs` 的生产段只剩 {} 字节 —— 剥法把它剥没了，下面两个数在空转。",
            prod.len()
        );
        // needle 运行时拼：写成字面量的话本文件会掉进别人的人群里（同类自指陷阱本区踩过多次）。
        let cmd_attr = format!("#[{}::command]", "tauri");
        let (want_cmds, want_dials) = POOL_SHAPE;
        assert_eq!(
            prod.matches(cmd_attr.as_str()).count(),
            want_cmds,
            "乙今天的 `#[tauri::command]` 条数与登记的 {want_cmds} 对不上。"
        );
        assert_eq!(
            call_sites(&prod, "connect_sftp"),
            want_dials,
            "乙今天的拨号处数与登记的 {want_dials} 对不上。"
        );
    }

    /// `KR78D2` 正题：**三样要跟着过界的东西，逐条点名，逐条在盘上。**
    ///
    /// 死值验（`M3`）：把 `sftp_pool.rs` 里任一个校验位改名 ⇒ 本条红并点名是哪一样。
    /// **纯函数**（语料由调用方给）⇒ 阳性/阴性两个方向都切得动。
    /// 直接在真树上判的话，「采到了它、而它过了」与「压根没扫到」在输出上一模一样。
    fn three_things_present(prod: &str) -> Result<usize, String> {
        let mine: Vec<&(&str, &str, &str, &str)> = REGISTERED
            .iter()
            .filter(|(half, ..)| *half == "乙")
            .collect();
        if mine.len() != 3 {
            return Err(format!(
                "乙那一半要点名的应当**恰好三样**（进度通道 · 连接池 · 死连接重建），实得 {}。\n\
                 少一样 = 这条登记不再说得出「挡着的是什么」；多一样 = 有人往里塞了别的东西。",
                mine.len()
            ));
        }
        for (_, what, pin, _) in &mine {
            if pin.is_empty() || !prod.contains(pin) {
                return Err(format!(
                    "乙那一半点名的「{what}」，它的逐字校验位在语料里找不到了：\n  {pin}\n\
                     ⇒ 要么那样东西真的没了（那这条登记要重写，并交代乙的形状变了），\
                     要么它改了名而登记没跟上。"
                ));
            }
        }
        Ok(mine.len())
    }

    #[test]
    fn each_of_the_three_things_that_must_cross_with_the_pool_is_still_on_disk() {
        let prod = production_code(POOL_SRC);
        assert_eq!(three_things_present(&prod), Ok(3));
    }

    /// **反向那半**：三样里少任意一样，判据必须红**并点名是哪一样**。
    ///
    /// 语料是**合成的**（把真语料里那一样的校验位挖掉），所以这条不依赖任何人去动 `sftp_pool.rs`。
    #[test]
    fn a_pool_missing_any_one_of_the_three_is_caught_and_named() {
        let prod = production_code(POOL_SRC);
        for (_, what, pin, _) in REGISTERED.iter().filter(|(half, ..)| *half == "乙") {
            let holed = prod.replace(pin, "«本条被合成语料挖掉了»");
            assert_ne!(
                holed, prod,
                "挖不动「{what}」—— 那说明它本来就不在，本条在空转"
            );
            let e =
                three_things_present(&holed).expect_err(&format!("挖掉「{what}」之后判据必须红"));
            assert!(
                e.contains(what),
                "判据红了却没点名是哪一样（挖掉的是「{what}」），实得：{e}"
            );
        }
    }

    /// `KR78D3`：记分牌上 `sftp.rs` 那一格**今天仍是「未搬」，而这张表说得出为什么**。
    ///
    /// 🔴 本条**不改**记分牌，只读它（`K-R74` 立的那两条不在本件写区）。
    /// 它买到的那件事是：**「算搬完了」这句话从此要付代价** ——
    /// 谁把那一格翻成 `true`，就得先让界面里那 14 处 `connect_sftp` 真的没了。
    ///
    /// 死值验（`M4`）：把 `DIAL_SITES` 里 `sftp.rs` 那一行的第三栏改成 `true` ⇒ 本条红。
    #[test]
    fn the_sftp_row_on_the_scoreboard_is_still_unmoved_and_this_ledger_says_why() {
        let row = crate::ssh_source::dial_move_judge::DIAL_SITES
            .iter()
            .find(|(f, ..)| *f == "sftp.rs")
            .expect("记分牌上应当有 `sftp.rs` 那一行");
        let (_, sess_sites, moved, _, _) = row;

        // ① 两把尺子的作用域**当场对一遍**，免得下一个人把 6 和 14 读成同一个数。
        assert_eq!(
            *sess_sites, 1,
            "记分牌数的是 `connect_session(` 的调用点，`sftp.rs` 在它那里应当是 1 处（`connect_sftp` 自己）。"
        );

        // ② 「还没搬」这句话的**理由**现算：甲之外仍有多少处 `connect_sftp`。
        let others: usize = DIAL_CENSUS
            .iter()
            .filter(|(f, ..)| *f != "acct_iso_deploy.rs")
            .map(|(f, _, _)| {
                let src = match *f {
                    "sftp.rs" => SFTP_SRC,
                    "sftp_pool.rs" => POOL_SRC,
                    "mcp.rs" => MCP_SRC,
                    other => panic!("`DIAL_CENSUS` 多出一份没有语料的文件：{other}"),
                };
                call_sites(&production_code(src), "connect_sftp")
            })
            .sum();
        assert!(
            others > 0,
            "甲之外一处 `connect_sftp` 都没有了 —— 那时 `sftp.rs` 那一格才轮得到讨论「搬完了」。"
        );

        assert!(
            !*moved,
            "记分牌把 `sftp.rs` 那一格标成「已搬」，而界面进程里还有 {others} 处 `connect_sftp` \
             调用点（乙 `sftp_pool.rs` · 丙 `mcp.rs` · `sftp.rs` 自己那 6 处）。\n\
             这两句话不可能同时为真。"
        );
    }

    /// 本件登记的挡路石，**每一条的逐字校验位都还在盘上**（没有校验位的那条明写「没有牙」）。
    ///
    /// 死值验（`M5`）：把任一条校验位所引的那句原文改掉 ⇒ 本条红并点名是哪一条。
    #[test]
    fn every_blocker_this_ledger_registers_is_still_quoted_verbatim_on_disk() {
        let toothless: Vec<&str> = REGISTERED
            .iter()
            .filter(|(_, _, pin, _)| pin.is_empty())
            .map(|(_, what, ..)| *what)
            .collect();
        assert_eq!(
            toothless,
            vec!["依赖签字闭集容不下这一档"],
            "「没有牙」的那几条是闭集，只此一条（它住对面那棵树，本件不长跨半边的边）。\n\
             多一条 = 有人把一句无从核对的话混进了这张表。实得 {toothless:?}"
        );

        // 每条校验位该去哪一份语料里找 —— 表与语料的绑定写死，别让判据自己去猜。
        let where_to_look: &[(&str, &str)] = &[
            ("进度通道", "sftp_pool.rs"),
            ("per-origin 连接池", "sftp_pool.rs"),
            ("死连接重建重试", "sftp_pool.rs"),
            ("daemon 只读铁律 I7", "doc/INVARIANTS.md"),
            ("别名 snippet 只许有一个家", "profile_installer.rs"),
            ("要部署的那份字节住在界面这一侧", "local_daemon.rs"),
            ("SFTP 写原语是三类共用的", "sftp.rs"),
        ];
        assert_eq!(
            where_to_look.len(),
            REGISTERED.len() - toothless.len(),
            "有牙的条数与「去哪找」那张表对不上 —— 加了一行却没说它去哪核。"
        );

        for (what, file) in where_to_look {
            let (_, _, pin, _) = REGISTERED
                .iter()
                .find(|(_, w, ..)| w == what)
                .unwrap_or_else(|| panic!("`REGISTERED` 里找不到「{what}」这一行"));
            let hay = match *file {
                "sftp_pool.rs" => production_code(POOL_SRC),
                "sftp.rs" => production_code(SFTP_SRC),
                "profile_installer.rs" => PROFILE_SRC.to_string(),
                "local_daemon.rs" => production_code(LOCAL_DAEMON_SRC),
                "doc/INVARIANTS.md" => INVARIANTS.to_string(),
                other => panic!("没有这份语料：{other}"),
            };
            assert!(
                hay.contains(*pin),
                "挡路石「{what}」的逐字校验位在 `{file}` 里找不到了：\n  {pin}\n\
                 ⇒ 它引的那句话在盘上变了。**回来重读这一条**，别顺手把校验位改成新的原文 ——\
                 那等于把「有人动过」这件事抹掉。"
            );
        }
    }

    /// 🔴 **本件没有给任何一半编闹钟** —— 这一条把那句承诺变成机检。
    ///
    /// 判法：本文件的**测试段**里不许出现「等到 X 就会响」那一族的措辞。
    /// 它拦不住一个存心绕开的人（禁词是文本），但它拦得住**顺手写下**那种 ——
    /// 而本区那两次真事故（`backend/mod.rs` 的墓碑两段）恰恰都是顺手写下的。
    #[test]
    fn this_ledger_does_not_wind_up_an_alarm_clock() {
        let me = include_str!("sftp_move_ledger.rs");
        // 运行时拼：写成字面量的话本条自己就会被自己扫到（自指陷阱）。
        let banned: Vec<String> = [
            ("等那", "天"),
            ("到时候", "会红"),
            ("那一刻", "会红"),
            ("自动提", "醒"),
        ]
        .iter()
        .map(|(a, b)| format!("{a}{b}"))
        .collect();
        let hits: Vec<&String> = banned.iter().filter(|w| me.contains(w.as_str())).collect();
        assert!(
            hits.is_empty(),
            "本登记里出现了「闹钟」措辞 {hits:?} —— 一个不会响的闹钟比一句过期的话更贵。\n\
             要么给它一条真会红的判据，要么如实写「今天没有人在看着这一格」。"
        );
        // 反向那半：禁词表自己不许是空的（空表恒绿）。
        assert_eq!(banned.len(), 4, "禁词表被掏空了，本条在空转。");
    }

    /// 「后端那侧到底有没有常驻通道」这句话，**本件引用的那一条今天还在**。
    ///
    /// 乙那一行拿它当理由（池活不过一次 exec），所以它腐了乙那一行就假了。
    #[test]
    fn the_one_shot_shape_of_todays_local_backend_transport_is_still_what_this_ledger_claims() {
        let prod = production_code(include_str!("backend/observe/local_query.rs"));
        for pin in [
            "std::process::Command::new(&bin)",
            "pub(crate) fn run_query(",
        ] {
            assert!(
                prod.contains(pin),
                "`local_query.rs` 里找不到 `{pin}` —— 本登记里「后端那条传输是 exec 一次拿 stdout」\
                 这句话失去了依据，乙那一行的理由要重写。"
            );
        }
        // 顺带钉住：`backend/` 那道宿主无关守卫还在（乙那一行也引了它）。
        assert!(
            BACKEND_SRC.contains("the_backend_layer_stays_host_agnostic"),
            "`backend/mod.rs` 里找不到那道宿主无关守卫 —— 乙那一行引的第二个理由也没了。"
        );
    }
}
