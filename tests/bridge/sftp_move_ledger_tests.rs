use guard_core::production_code;

// ── 语料 ─────────────────────────────────────────────────────────────────
//
// 只读**本 crate 自己**那四份 + 仓内耐久文档，一条跨半边的边都不长。

const SFTP_SRC: &str = include_str!("../../src/bridge/src/sftp.rs");
const MCP_SRC: &str = include_str!("../../src/bridge/src/mcp.rs");
const ACCT_SRC: &str = include_str!("../../src/bridge/src/acct_iso_deploy.rs");
const INVARIANTS: &str = include_str!("../../src/doc/INVARIANTS.md");
const PROFILE_SRC: &str = include_str!("../../src/bridge/src/profile_installer.rs");
/// 🔴 〔搬树 2026-09-18 · `设计/16 §5.4b` 纪律 3〕**校验位跟着判据搬。**
/// 挡路石「别名 snippet 只许有一个家」引的那句原文是一条 `#[test]` 里的断言，
/// 剖分把它从 `profile_installer.rs` 搬到了这里。**那句话一个字节没改。**
const PROFILE_TESTS_SRC: &str = include_str!("profile_installer_tests.rs");
const LOCAL_BACKEND_SRC: &str = include_str!("../../src/bridge/src/local_backend_host.rs");

/// 语料地板：低于这个字节数就判「语料没喂进来」，而不是「一处都没有」。
///
/// 🔴 这条是下面 `an_empty_corpus_makes_the_census_red_by_itself` 的被测对象 ——
/// 「一处都没扫到」与「根本没扫」在终端上一模一样，这条地板就是把它们分开的那一刀。
/// 〔SR1b · 2026-09-24〕60 000 → 40 000：乙（`sftp_pool.rs`）整份出了语料 —— 传输台搬进了本机常驻后端，
/// 那份文件只剩中继、一个 SFTP 会话都不拿（`sftp_family_registry_tests::the_relay_holds_no_sftp_at_all` 零命中）。
/// 剩下三份的生产段现打 ≈ 47 KB，地板压在它之下留余量。
const CORPUS_FLOOR_BYTES: usize = 40_000;

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
/// 🔴 **PM 派工单里那句「9 处 / 4 文件」现打是错的** —— 实打 **11 处 / 4 文件**
/// （立表那天 14 处；〔步 23b · 09-20〕零流量复制那一路 +1 = 15；
/// 〔步 24 · 09-20〕多通道池把 `sftp_pool.rs` 那 5 处收敛成 1 处 = **11**，
/// 逐条见 `sftp_pool.rs` 那一行）。
/// 差在两处：派工单那张分类表自己加起来是 5 ＋ 4 ＋ 3 = 12，而 `sftp.rs` 里
/// 另有 **2 处**它一类都没归（`ensure_backend_deployed` 自动部署 ·
/// `remove_remote_file` 删远端会话 jsonl）。
const DIAL_CENSUS: &[(&str, usize, &str)] = &[
    (
        "sftp.rs",
        3,
        "甲 2 条命令（`deploy_remote_backend` · `uninstall_remote_backend`）\
             ＋ **派工单没归类的 1 处**：`ensure_backend_deployed`（连接流程里的自动部署）。\
             〔RW1 · 第四波 09-24〕**4 → 3**：原来派工单没归类的还有一处 —— F11 删远端会话 jsonl 那条 SFTP 直删 ——\
             按用户裁改经远端后端删（`files-delete-session`），那一处随它删了。\
             〔RW1 · 第四波 09-24〕**6 → 4**：`install_remote_alias_block` / `uninstall_remote_alias_block` \
             （F10）按用户裁「按推荐改」改成经那台远端的后端写（`user_files` → `files-peek` / `files-put`），\
             不再拨 SFTP —— 上面「甲」那一格挡路石（铁律 I7）由用户 09-23/24 两裁解开（后端文件管理那一面可以改用户文件）",
    ),
    // 〔SR1b · 2026-09-24〕乙（`sftp_pool.rs`，池化文件操作 → 传输台）那一行摘了：**1 → 0，而且这一次是真搬了** ——
    //   SFTP 客户端、池（按连接记的预算）、传输台整段进了本机常驻后端（`dial/sftp.rs` · `control/transfer.rs`），
    //   这份文件只剩中继，出了本表的人群。
    (
        "mcp.rs",
        1,
        "丙 —— 读远端项目 `.mcp.json`（展示用）。〔RW1 · 第四波 09-24〕**3 → 1**：写 / 删两处（F89a）\
             按用户裁「按推荐改」改经远端后端写（`user_files`），不再拨 SFTP",
    ),
    (
        "acct_iso_deploy.rs",
        1,
        "甲 —— 部署 vendored `cc-acct-iso` 到远端",
    ),
];

// ── 登记表 ───────────────────────────────────────────────────────────────

/// **本件的底账**：`(哪一半, 点名的那样东西, 逐字校验位, 它在挡什么)`。
///
/// 「逐字校验位」= 一段**今天真在盘上**的原文；它变了这一行就红，逼人回来重读，
/// **而不是**在某个日子自己响。空串 = 这一行**没有牙**，只有符号住址（住对面那棵树）。
///
/// ⚠ 表名 `REGISTERED` 是 `scanning_guard_registry::TABLE_DECLS` 闭集里的成员之一，
/// 刻意不起新名字（`DECISIONS.md#R33` 裁定一：新名字要同拍动那张闭集，而它不在本件写区）。
const REGISTERED: &[(&str, &str, &str, &str)] = &[
    // ── 乙 ──〔SR1b · 2026-09-24〕「per-origin 连接池」「死连接重建重试」两行摘了：**挡路石没被绕开，是被拆掉了**。
    //   那两行的判词是「池是进程全局的、靠界面进程活着才有意义；本机后端那条传输是 exec 一次拿 stdout，池搬过去活不过一次调用」。
    //   两个前提 SR1a / SR1b 各拆一个：SR1a 立了本机**常驻**后端与那条常驻流（链路 · 连接池按拨号身份复用）；
    //   SR1b 把传输台放上去，池换成按连接记的预算（`dial/pool.rs::Budget`），死连驱逐换成池里的 `is_closed()` ＋ 开通道失败摘掉重拨。
    // ── 甲：本件现打的四条挡路石 ─────────────────────────────────────
    (
        "甲",
        "backend 只读铁律 I7",
        "后端不许改动用户既有数据",
        "甲里 `install_remote_alias_block` / `uninstall_remote_alias_block` 改的正是\
             **用户既有的** `~/.bashrc`（备份 → 覆盖写 → 读回校验 → 回滚）。\
             把它搬进后端 = **backend 进程自身**去改用户既有数据 ⇒ 与铁律 I7 正面撞。\
             🔴 **这是用户拍的红线，实现方不自批** —— `DECISIONS.md#R32` 通篇没有处置这一格。\
             ⚠ 更要紧的是：`readonly_guard` 的模式表全是 `fs::` / `File::` / `OpenOptions` \
             命名空间（本模块**没有机检**这一句，住址 \
             `src/backend/readonly_guard.rs::FS_MUTATION_PATTERNS`）⇒ \
             SFTP 那套写**一条都不匹配** ⇒ 真搬过去，**机检不会红**，是一次静默越线。\
             〔🔴 `K-R79`（09-12）订正，**原文一个字没动，是历史不是错误**：\
             最后那半句「真搬过去，机检不会红」**今天不成立了**。\
             `FS_MUTATION_PATTERNS` 那张表**仍然**全在 `fs::` / `File::` / `OpenOptions` \
             命名空间（`K-R79` 刻意不往里塞 SFTP 方法名，那是件计划点名的失效方向）——\
             变的是**旁边多了一层**：`readonly_guard::remote_write_layer`，\
             它按「取得远端写能力」＋「SFTP v3 协议操作的方法形」两张网扫后端整棵生产段。\
             `K-R79` 现打过这一刀：往后端生产段插一处会话类型 + 协议打开标志 + 改名，\
             **新层当场红并点名那份文件，而老那两层一格没红** —— 前半句因此仍是真的。\
             ⇒ 读这一行时把结论换成：**今天会红；这条挡路石从「机检看不见」降级成\
             「机检看得见、而『许不许』还没裁」**（`ROADMAP.md#KU31` 仍未裁）。\
             〔本订正**同样没有牙**：理由与本行原有那条逐字同一句〕〕",
    ),
    (
        "甲",
        "依赖签字闭集容不下这一档",
        "",
        "backend 那棵树只有 `russh`，**没有** `russh-sftp`（现打：`russh-sftp = \"2\"` \
             只在 `src/bridge/Cargo.toml`）。加它 ⇒ \
             `src/backend/readonly_guard.rs::g6_dependency_signoff::SIGNED` \
             必须同拍加一行（「新加一条而没签字 ⇒ 当场红」）。\
             而那张表的判档是**闭集三档**（`已量·有写面` / `已量·未见写面` / `未量·靠用法签字`），\
             `已量·有写面` 那一档的定义逐字要求写清「**凭什么进不了发布二进制**」—— \
             ⇒ **没有一档容得下「有写面、而且发布二进制里就是要它写」**。\
             〔本行**没有牙**：住对面那棵树，不长跨半边的边。〕\
             〔🔴 `K-R79`（09-12）订正，**原文一个字没动**：最后那句\
             「没有一档容得下」**今天不成立了** —— 判档闭集已从三档加到四档，\
             第四档逐字叫 `已量·有写面·就是要它写`，住 \
             `src/backend/readonly_guard.rs::g6_dependency_signoff::VERDICTS`。\
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
        "`profile_installer_tests.rs::the_alias_snippet_has_exactly_one_home_in_the_rust_tree` \
             断言 `src/shared/ccm-aliases.sh` 在 monitor `src` 树里**恰好一处**，就是 \
             `sftp.rs::CCM_WRAPPER_SNIPPET`；同文件 \
             `the_posix_arm_borrows_the_remote_implementation_instead_of_growing_a_second_one` \
             另断言**本机**那条 POSIX 路借的就是远端这一份（三个符号各恰好 1 次）。\
             ⇒ 把 `install_remote_alias_block` 整条搬走：留下第二份 snippet ⇒ 前者红；\
             把 snippet 一起搬走 ⇒ 本机那条路失去实现，后者红。\
             **出路只有一条**：先立一个两侧共用的 crate —— 而那要 `shared_crate_registry` 签字，\
             不在本件写区。",
    ),
    (
        "甲",
        "要部署的那份字节住在界面这一侧",
        "crate::sftp::backend_binary(std::env::consts::ARCH)",
        "远端后端的字节由 `src/bridge/build.rs` 的 `embedded_backends` cfg 内嵌进 \
             **monitor** 这份二进制（`sftp::backend_binary`），而它**不止部署路在用**：\
             `local_backend_host.rs` 释放本机后端时读的是同一份。\
             ⇒ 字节搬不走 ⇒ 后端要部署，字节得**从界面递过去** ⇒ \
             过界物就不再是 `R32` 裁定二第 ③ 种那个干净的 `{origin, 动作} → 只回结果`。\
             **这一格要 PM 拍**：是认下「动作带载荷」，还是让后端自己去取那份字节\
             （backend 侧 `sidecars/codepicture/fetch.rs` 有同形的拉取协议，但它自陈\
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
        return Err("生产段里一处 `connect_sftp` 调用点都没有 —— 那不是搬完了，是没扫到。".into());
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
        total, 5,
        "`connect_sftp` 的调用点合计应当是 5 处（3 份文件）—— 实得 {total}。\n\
             ⚠ 〔SR1b · 09-24〕**6 → 5**：乙（`sftp_pool.rs`）那 1 处随传输台搬进了本机常驻后端。\n\
             ⚠ 〔RW1 · 第四波 09-24〕**11 → 6**：远端别名块装 / 卸两条（F10）· 项目 `.mcp.json` 写 / 删两处（F89a）·\n\
             删远端会话一处（F11）改经远端后端写，不再拨 SFTP。\n\
             ⚠ 派工单写的「9 处」现打是错的，来历见 `DIAL_CENSUS` 头注。\n\
             ⚠ 〔步 23b · 09-20〕**14 → 15**：零流量复制那一路的 `copy_inner` 自己拿池槽。\n\
             ⚠ 〔步 24 · 09-20〕**15 → 11**：多通道池落地，`sftp_pool.rs` 里那 5 处\n\
             「槽空就拨号」收敛成 `OriginPool::conn` 一处。**降了不是好消息也不是坏消息** ——\n\
             它量的是「有多少段代码自己去拨号」，不是「离搬进后端还差多远」。"
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

// 〔SR1b · 2026-09-24〕乙那三条（形状 · 要跟着过界的几样在盘上 · 挖掉任一样必红）随乙整份出了语料摘了 ——
//   它们守的是「乙还没过界、为什么」，而乙过界了（见 `REGISTERED` 乙那一段的墓碑）。

/// `KR78D3`：记分牌上 `sftp.rs` 那一格**今天仍是「未搬」，而这张表说得出为什么**。
///
/// 🔴 本条**不改**记分牌，只读它（`K-R74` 立的那两条不在本件写区）。
/// 它买到的那件事是：**「算搬完了」这句话从此要付代价** ——
/// 谁把那一格翻成 `true`，就得先让界面里那 15 处 `connect_sftp` 真的没了。
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
             调用点（丙 `mcp.rs` · `sftp.rs` 自己那几处；〔SR1b〕乙已经过界）。\n\
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
        ("backend 只读铁律 I7", "src/doc/INVARIANTS.md"),
        ("别名 snippet 只许有一个家", "profile_installer_tests.rs"),
        ("要部署的那份字节住在界面这一侧", "local_backend_host.rs"),
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
            "sftp.rs" => production_code(SFTP_SRC),
            "profile_installer.rs" => PROFILE_SRC.to_string(),
            "profile_installer_tests.rs" => PROFILE_TESTS_SRC.to_string(),
            "local_backend_host.rs" => production_code(LOCAL_BACKEND_SRC),
            "src/doc/INVARIANTS.md" => INVARIANTS.to_string(),
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
    let me = include_str!("../../src/bridge/src/sftp_move_ledger.rs");
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

// 〔SR1b · 2026-09-24〕「本机后端那条传输是 exec 一次拿 stdout」那一条（乙那一行的理由）摘了：
//   乙过界了，而那句话本身也早不是全貌（SR1a 立了本机常驻后端那条常驻流）。
