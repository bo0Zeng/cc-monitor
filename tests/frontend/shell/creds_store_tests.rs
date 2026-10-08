//! # 要求住址：`INVARIANTS §42` → `src/doc/IPC-PROTOCOL.md`「上游选择那份凭据文件在「这台机器」上的读写」节
//!
//! 核原文：该节逐字「**每台机器上的程序写者恰好一个**」·「**路径**与那台机器上 `--relay` 进程的上游选择**同一个出处**」。
//! 那一个写者 ＝ **那台的后端**（本机 ＝ 本机常驻后端），monitor 不再写本机那一份。
//! 本族今天判 monitor 这一侧剩下的：与后端算同一份文件并显式交出去 ·
//! 明文只往下传登记过的那几跳 · 写半边零调用 · 账号 id 只有一份规则。读侧（三态 · 权限提醒）与「写下的那一行
//! 正是起会话那一侧找的那一行」随读者换成那台后端一起搬去后端那一份判据。写路那几条性质（写的那一刻读盘 · 未知键一个不吃 ·
//! 出生即只给本人 · Base URL 形状错整次不写）住后端那一份写口的判据（`tests/backend/accounts/upstream_select/file_face_tests.rs`）。
//! 明文出口跨三棵树逐处计数那几条守「明文只有一个出口」（原文点一个，判据登记两个 —— 原文比判据窄）。
//! ⚠ 「key 不进 `config.json`」与 TS 状态类型对拍那几条没有逐字原文。
//! 与 `crates/creds-core/store_tests.rs` 不重复：那族判纯函数。〔JA1 点址 2026-09-24〕

use creds_core::store;

/// 凭据文件的位置只跟着家走：monitor 起本机后端时交的那份环境里**没有**任何一格另指它的位置
/// （从前那一格 `CCM_APIKEY_CREDENTIALS` 删了，后端按家推 `creds_core::store::credentials_path`）。
#[test]
fn nobody_hands_the_backend_a_path_for_the_credentials_file() {
    let envs = crate::local_backend_host::backend_env_from(&|_| None, None);
    let names: Vec<&str> = envs.iter().map(|(k, _)| k.as_str()).collect();
    assert_eq!(
        names,
        vec!["CCM_RELAY_PORT"],
        "交给本机后端的环境多了一格：{envs:?}"
    );
    // 反空真：这把尺子看得见被交的格（家那一格在设了的时候出现）。
    let iso = crate::local_backend_host::backend_env_from(
        &|k| (k == crate::config::DATA_DIR_ENV).then(|| "/iso/home".to_string()),
        None,
    );
    assert!(iso
        .iter()
        .any(|(k, v)| k == crate::config::DATA_DIR_ENV && v == "/iso/home"));
}

/// `KS7`：它**不是**前端整份读写的那份配置。
#[test]
fn the_key_never_lands_in_the_config_file_the_frontend_rewrites_wholesale() {
    let cfg = crate::config::resolve_config_path().expect("config path");
    let creds =
        store::credentials_path(&crate::config::resolve_monitor_data_dir().expect("data dir"));
    assert_ne!(cfg, creds, "凭据落在了前端『读—改—写』整份的那个文件上");
    // 同一个目录是**可以**的（`§0a` 要的是「不进那份配置」，不是「不同目录」）。
    assert_eq!(cfg.parent(), creds.parent());
    // ★ 机检：`config.rs` 的生产段里不许出现那个字段名 ——
    //   它一旦出现，就说明有人把 key 塞进 `load_config`/`patch_config` 那条路了（写口从整份换成按键补丁）。
    let cfg_src =
        guard_core::production_code(include_str!("../../../src/frontend/shell/src/config.rs"));
    assert!(
        !cfg_src.contains(store::KEY_FIELD),
        "`config.rs` 的生产段里出现了 `{}` —— key 进了前端整份读写的那份配置",
        store::KEY_FIELD
    );
    // 非空对照：这把尺子**认得出**那个字段名（不是恒不含）。
    assert!(store::template().contains(store::KEY_FIELD));
}

// `brace_block`〔散文墓碑〕随它唯一的调用方（明文逐跳那一条）一起删了。

// 这里原来是两条写路判据（`KS10` 交错写不吃人手编的 · `K-R1` 配 key 不吃同一行的
// `auth_style` / `base_url`），打的是 monitor 那侧的写口。写者换成了那台的后端 ⇒ 两条原样搬去后端那一份写口：
// `file_face_tests::gp1_a_program_write_keeps_everything_the_human_put_there` ·
// `file_face_tests::gp1_a_saved_key_does_not_swallow_the_hand_written_upstream_or_auth_style`。

// 读侧那两条（`KS11` 权限放宽出声 · 三态「读坏了不许退化成没配」）打的是 monitor 那一份状态读者
//   （`creds_store::read_status_at`〔散文墓碑〕）。读者换成那台的后端（`apikey-read`）⇒ 两条原样搬去后端那一份读口：
//   `file_face_tests::us1_a_widened_file_is_called_out_and_an_owner_only_one_is_not` ·
//   `file_face_tests::us1_a_broken_file_is_surfaced_instead_of_looking_unconfigured`。

// ============================================================ `K-H2c` `KH2C1` / `KH2C3`

// 「写侧落下的那一行，正是起会话那一侧会去找的那一行」那条跨两半的判据
//   （`what_the_write_side_wrote_is_exactly_the_row_the_launch_side_looks_for`〔散文墓碑〕）挪去后端：起会话那一侧找行的读者
//   今天是那台后端的 `file_face::rows_at`（`launch-endpoint` · `apikey-routing` 读同一份），与写口同一个模块 ⇒
//   `file_face_tests::us1_what_the_write_side_wrote_is_exactly_the_row_the_launch_answer_uses`（写口真写 → 成品真答 `/s/`）。

/// ★★ **`KH2C1` 的机检那一半**：写侧**没有第二份**「取末段名」的实现。
///
/// `KH2C1` 逐字禁的是「两边各写一份取末段名的逻辑」。行为那一条（上面那个）
/// 在两份实现**碰巧同形**的那一天照样绿 —— 漂开是**以后**才发生的事。
/// ⇒ 这一条钉的是结构：写侧只许**调**那一份唯一的规则，自己不许再取一次末段名。
#[test]
fn the_account_id_rule_is_not_reimplemented_on_the_write_side() {
    // 写侧推 id 的那一半今天住后端写口 `accounts/upstream_select/file_face.rs`（界面经通道交 `configDir`，
    //   那台后端推）。〔GP1 那一版住 monitor `apikey_remote.rs`（`send_key`〔散文墓碑〕）；更早住 `creds_store.rs`。〕
    let src = guard_core::production_code(include_str!(
        "../../../src/backend/accounts/upstream_select/file_face.rs"
    ));
    guard_core::assert_no_test_code("后端写口 id 规则", &src);
    // ① **调**那一份唯一的规则，恰好一处。
    assert_eq!(
        src.matches("apikey_account_id_of_dir(").count(),
        1,
        "写侧调那条唯一规则的次数不是 1 —— 0 次说明它自己算了一份，\n\
             多次说明这条线上有两个地方在推 id。生产段：{src}"
    );
    // ② 自己**不许**再取一次末段名。人群 = 本文件生产段，针 = 四种常见写法。
    for needle in ["file_name(", "rsplit('/')", "split('/')", ".components()"] {
        assert_eq!(
            src.matches(needle).count(),
            0,
            "写侧出现了 `{needle}` —— 那是在长第二份「取末段名」的规则。\n\
                 ⚠ `apikey_account_id_of_dir` 的头注逐字写着它被抽出来的理由：\n\
                 「两边各写一个 basename 规则，漂开的那天症状是『设置里说走 apikey 端点改写、起会话时没走』，\n\
                 而两边看起来都没错」。"
        );
    }
    // 反空真：这把尺子**认得出**那一族针（不是恒 0）。
    // 正控从 `history.rs` 挪到 `remote_history.rs`；那一句（远端删会话取文件名 stem）随删会话进界面删了，
    //   正控再挪到文件窗口的语料那一处（`filewin/corpus.rs` 取文件名那一句，同一族针 `rsplit('/')`，今天真在）。
    let other =
        guard_core::production_code(include_str!("../../../src/frontend/filewin/src/corpus.rs"));
    assert!(
        other.contains("rsplit('/')"),
        "同一把尺子在 `filewin/corpus.rs` 上也数出 0 —— 它恒 0，本条按红处理"
    );
}

// `KH2C3` 后半（顶层那一把不再是写入目标：结构 ＋ 行为两维）打的是 monitor 那侧写口；
// 写口去了后端 ⇒ 原样搬去 `file_face_tests::gp1_the_write_side_never_targets_the_legacy_top_level_slot`。

// 「说不出 id 的时候报错、不回落」那一条（`a_config_dir_that_names_no_account_is_refused_instead_of_falling_back`〔散文墓碑〕）
//   打的是 monitor 那侧入口 `apikey_remote::write_key_on`〔散文墓碑〕；推 id 搬进后端写口之后，同一组形状（空串 · 全空白 · `/` ·
//   最后一段是 `..`）由后端那一份判：`file_face_tests::hx2_the_account_id_is_derived_here_from_the_config_dir`。

/// 明文两个出口，各自**只许出现在哪棵树的哪个文件里**。
///
/// `(方法名, 期望总处数, 期望它住在哪个文件的路径尾巴)`。**默认拒绝**：对不上就红。
const PLAINTEXT_EXIT_SITES: &[(&str, usize, &str)] = &[
    (
        "expose_for_auth_header(",
        1,
        "src/backend/accounts/upstream_select/mod.rs",
    ),
    (
        "expose_for_persisting(",
        1,
        "common/creds-core/src/store.rs",
    ),
    (
        "expose_for_token_request(",
        1,
        "common/creds-core/src/token.rs",
    ),
];

/// 本判据扫哪几棵树。**这就是「取明文恰好 N 处」那句全称的分母。**
const PLAINTEXT_SCAN_TREES: &[&str] = &["src/frontend/shell/src", "src/common", "src/backend"];

/// ★★★ **`KS2` 的人群那一格〔D1 阻-1 回修，08-27〕：三棵树全扫，不是一个文件、也不是一个 crate。**
///
/// # 它替掉的是一个**按 crate 边界画的人群**
///
/// 回修前，「取明文恰好 N 处」这条性质由两处判据分管，而它们的人群加起来**盖不住产品**：
/// · `creds-core/src/lib.rs` 只扫 `include_str!("../../../src/frontend/shell/src/lib.rs")`——**它自己这一个文件**；
/// · `relay/creds_guard.rs` 扫后端那个 crate；
/// ⇒ **`src/frontend/shell` 整个不在任何人的人群里**，而 monitor 恰恰是明文**第一次进程序**的地方
///   （`write_apikey_credentials_key(key: String)`）。
/// D1 审计刀 B 实打：在 monitor 生产段取一次明文 `eprintln!` 出去
/// ⇒ **8 包合计 1284 passed，一条都没红**。
///
/// 件计划 `§0` 逐字警告过这一形：「**判据守的是前门，key 从后门进**」，
/// 而它「同时骗过了 PM 与一路审计」。这次它在**同一件里**又长了一次，只是换了个边界。
///
/// # 分母（写清它算了什么、没算什么）
///
/// 人群 = [`PLAINTEXT_SCAN_TREES`] 那三棵树下**所有 `.rs` 的生产段**（剥掉 `#[cfg(test)]`），
/// ⚠ 先前这一行写着「**外加本文件自己**（见下面那段：`scan_tree!`
/// 按构造摘掉调用者，那正好会把本文件摘出人群）」—— **两处都已经不成立**：
/// 那一刀在这一处不生效，而下面那句手工补回的 `push` 在搬树那一轮就删掉了
/// （理由逐字写在下面那段注里）。⇒ 人群**就是那三棵树**，没有「外加」。
/// 它数的是**两个具名方法的调用点**，不是「明文」这个概念 ——
/// 有人把明文经别的路径带出去（自定义类型、`Deref`），本条看不见；
/// 那一格由 `creds-core` 的 `every_string_returning_exit_is_registered_by_name`
/// 与 `the_type_has_no_second_impl_block_that_hands_the_inner_string_out` 两条在**定义面**兜。
/// **三格一起才成立**：定义面（有几个出口）· 调用面（每个出口被调几次、在哪）· 本条（人群覆盖三棵树）。
#[test]
fn the_two_plaintext_exits_are_called_from_exactly_one_place_each_across_all_three_trees() {
    let root = crate::guard_support::repo_root().to_path_buf();

    let mut files: Vec<(String, String)> = Vec::new();
    for sub in PLAINTEXT_SCAN_TREES {
        // 🔴 〔搬树 2026-09-18 ·  条 73〕**明写「一份都不排除」，不靠 `file!()`。**
        //
        // 上一版靠 `scan_tree!` 的自摘摘掉调用者（当年 = `creds_store.rs` 自己），
        // 再手工 `push` 一份补回来。剖分之后自摘那一刀落空，`creds_store.rs` 走普通
        // 遍历**本来就进人群** ⇒ 手工补回那份成了**第二份**，同一段生产代码被数两遍。
        // 今天两个出口的调用点都不在它里面，所以这一格**没有变红** ——
        // 也就是说它是安静地错着的那一半（条 73 那条纪律正是为这一半立的）。
        for (path, raw) in guard_core::scan_tree_excluding(&root.join(sub), &["rs"], &[]) {
            files.push((
                path.display().to_string().replace('\\', "/"),
                guard_core::production_code(&raw),
            ));
        }
    }
    // ★ 反空真：`creds_store.rs` 是 monitor 侧碰 key 最多的一份，它必须在人群里。
    assert!(
        files.iter().any(|(p, _)| p.ends_with("creds_store.rs")),
        "人群里没有 `creds_store.rs` —— 分母缺了 monitor 侧碰 key 最多的那一份"
    );

    // 采集面自检：三棵树都要扫到东西，且总量不能小得离谱。
    assert!(
        files.len() >= 150,
        "只扫到 {} 个 .rs —— 遍历坏了，下面全是空真",
        files.len()
    );
    for sub in PLAINTEXT_SCAN_TREES {
        let leaf = sub.rsplit('/').next().unwrap_or(sub);
        assert!(
            files
                .iter()
                .any(|(p, _)| p.contains(sub) || p.contains(leaf)),
            "`{sub}` 这棵树一个文件都没扫到 —— 分母缺了一块"
        );
    }

    for (needle, want, home) in PLAINTEXT_EXIT_SITES {
        let mut hits: Vec<String> = Vec::new();
        for (path, prod) in &files {
            // ⚠ **只数调用点，不数定义** —— needle 第一版没剥定义，实测当场红：
            //   `expose_for_auth_header(` 在三棵树里 **2 次**，多出来的那次是
            //   `creds-core/src/lib.rs` 里的 `pub fn expose_for_auth_header(`。
            //   「定义」不是一个出口，「调用」才是；两者混在一个数里，
            //   那个数就同时装了两件事（本区最贵的那族病）。
            for (i, _) in prod.match_indices(needle) {
                if prod[..i].trim_end().ends_with("fn") {
                    continue; // 这是定义
                }
                hits.push(path.clone());
            }
        }
        assert_eq!(
            hits.len(),
            *want,
            "`{needle}` 在三棵树的生产段里出现 {} 次，应当 **{want}** 次：{hits:?}\n\
                 ⚠ `KS2` 逐字：加行是收紧、动断言是放宽 —— 真要多一处，\n\
                 **必须先在件计划里说清那一处是什么**，不许在实现里顺手把这个数改大。",
            hits.len()
        );
        assert!(
            hits[0].ends_with(home),
            "`{needle}` 唯一那处不在 `{home}`，而在 `{}` —— 靶子挪了",
            hits[0]
        );
    }
}

/// ★★ **两张表必须说同一件事**〔D2 回修，08-27〕：
/// 定义面（`creds-core` 里哪几个 fn 标着 `HandsOut`）与调用面（本文件里哪几个 needle
/// 被数调用点）**必须是同一组名字**。
///
/// # 它买的是什么
///
/// `D2` 点名要「给一条判据钉住 `HandsOut` / `ReadsOnly` 这个区分」。
/// 光在定义面钉「`HandsOut` 恰好 2 条」还不够 —— 有人把一个**新出口**标成 `HandsOut`
/// 并同时把定义面那条相等断言改大，两边就都绿了。
/// ⇒ 本条把它接到**另一个 crate 里的另一张表**上：新出口要绿，得**同时**改两张表，
/// 而调用面那张表一改，`the_two_plaintext_exits_are_called_from_exactly_one_place_each_across_all_three_trees`
/// 立刻要求它「恰好 1 处调用、住在指定文件」。**三张表互相钉住，改一张不够。**
///
/// ⚠ 它**不判**分类对不对（那要判语义）—— 它判的是**两张表有没有说同一件事**。
#[test]
fn the_definition_table_and_the_call_site_table_name_the_same_exits() {
    // 🔴 〔搬树 2026-09-18〕**住址改对：那张表搬家了。**
    //
    // `INNER_FIELD_USERS` 是 `creds-core` 的**判据用表**（住在它的 `#[cfg(test)]` 段里），
    // 剖分把它从 `crates/creds-core/src/lib.rs` 搬到了
    // `tests/common/creds-core/lib_tests.rs`。表本身一个字没改 ——
    // 变的只是它住哪儿。⇒ 指对地方，不是放宽（读数逐字：「一处都找不到 ⇒ 本条按红处理」，
    // 这条反空真**按设计响了**，别把它调松）。
    let table_home = crate::guard_support::tests_root().join("common/creds-core/lib_tests.rs");
    let core = std::fs::read_to_string(&table_home).unwrap_or_else(|e| {
        panic!("读不到定义面那张表所在的 {table_home:?}：{e} —— 抽取器坏了，本条会零命中地绿")
    });

    // 从 `INNER_FIELD_USERS` 里挑出标着 `HandsOut` 的行，取它的名字。
    let at = guard_core::find_pinned(&core, "const INNER_FIELD_USERS:")
        .expect("切不出定义面那张表 —— 本条按红处理");
    // ⚠ 切法**不是** `brace_block` —— 这张表是 `&[ … ]`，它的第一个 `{` 可能落在很远的地方
    //   （实测第一版就是这么切歪的，被下面那条反空真自检当场逮住：「一条 HandsOut 都没抽到」）。
    //   按它自己的收尾 `];` 切才是这张表的边界。
    // 🔴 〔搬树 2026-09-18〕收尾针从 `"\n    ];"` 改成 `"\n];"`：搬出 `mod tests {}` 之后
    //   这张表是**文件顶层的** item，缩进整整少了一级。缩进是位置，而按位置认边界的针
    //   会随搬树静默失配—— 下面那条 `table.len() > 200` 是它的反空真。
    let table = core[at..]
        .find("\n];")
        .map(|i| &core[at..at + i])
        .expect("切不出表体（找不到 `];` 收尾）—— 按红处理");
    assert!(table.len() > 200, "表体只有 {} 字节 —— 切歪了", table.len());
    let mut hands_out: Vec<String> = Vec::new();
    for (i, _) in table.match_indices("Handling::HandsOut") {
        // 往回找最近的一个 `"名字"`。
        let before = &table[..i];
        let Some(q_end) = before.rfind('"') else {
            continue;
        };
        let Some(q_start) = before[..q_end].rfind('"') else {
            continue;
        };
        hands_out.push(before[q_start + 1..q_end].to_string());
    }
    hands_out.sort();
    assert!(
        !hands_out.is_empty(),
        "定义面表里一条 `HandsOut` 都没抽到 —— 抽取器坏了，本条在空转"
    );

    // 调用面那张表的 needle 去掉尾巴那个左括号，就是方法名。
    let mut call_side: Vec<String> = PLAINTEXT_EXIT_SITES
        .iter()
        .map(|(n, _, _)| n.trim_end_matches('(').to_string())
        .collect();
    call_side.sort();

    assert_eq!(
        hands_out, call_side,
        "两张表点的**不是同一组出口**：\n\
             · 定义面（creds-core `INNER_FIELD_USERS` 里标 `HandsOut` 的）= {hands_out:?}\n\
             · 调用面（本文件 `PLAINTEXT_EXIT_SITES`）= {call_side:?}\n\
             ⚠ 两边说的不是一件事时，**各自都绿**，而中间那道缝就是明文出去的地方。"
    );
}

// 这里原来是两条 `KS5` 源码判据（写口里收窄恰好两次 · tmp 出生即窄），切的是 monitor 那侧写口的函数体；
// 写口随写者换成那台的后端一起走了。「出生即只给本人」今天由后端那一份的两条兜：写半边只从账号域那一份写口够得着
// （`readonly_guard::…::the_credentials_write_half_is_reached_only_from_the_account_file_face`）· 写出来的文件是 `0600`
// 且不留临时文件（`file_face_tests::the_written_file_is_owner_only_and_no_temp_file_is_left`）。
// monitor 这一侧只剩一条要钉的：**写半边一处都不调**（`gp1_the_monitor_never_reaches_the_credentials_write_half`）。

/// **monitor 生产段一处都不够写半边**（`creds_core::perm::create_private`）。
///
/// monitor 仍开着 `harden`（Windows 上读 DACL 要它），编译器因此兜不住「monitor 写不了这份文件」—— 由本条兜：
/// 人群 = `src/frontend/shell/src` 下全部 `.rs` 的生产段（剥测试段与注释），针一根，**零命中**。
/// 正控：同一把针在后端自有状态文件原子写那一处（`src/backend/common/own_state.rs`，这份文件的写口经它落盘）上数得到 —— 针没瞎。
/// 要求住址：「每台机器上这份文件的程序写者恰好一个 ＝ 那台的后端」（`GP1.md §3`）。
#[test]
fn gp1_the_monitor_never_reaches_the_credentials_write_half() {
    let needles = [format!("create_{}(", "private")];
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let files = guard_core::scan_tree_excluding(&root, &["rs"], &[]);
    assert!(
        files.len() > 100,
        "只扫到 {} 份 —— 遍历坏了，零命中是空真",
        files.len()
    );
    let mut hits: Vec<String> = Vec::new();
    for (path, raw) in &files {
        let code = guard_core::strip_comment_lines(&guard_core::production_code(raw));
        for n in &needles {
            if code.contains(n.as_str()) {
                hits.push(format!("{} · {n}", path.display()));
            }
        }
    }
    assert!(
        hits.is_empty(),
        "monitor 生产段够到了凭据文件的写半边 —— 本机那一份的写者是本机常驻后端，monitor 一个字节都不写：{hits:?}"
    );
    let face =
        guard_core::production_code(include_str!("../../../src/backend/common/own_state.rs"));
    assert!(
        needles.iter().any(|n| face.contains(n.as_str())),
        "正控失败：后端那一份写口里也数不到 —— 针瞎了"
    );
}

// `ApikeyCredentialsStatus` 那两条（TS 手写类型双向对拍 · 类型装不下明文）随结构体一起退役：
//   状态由那台后端出成品（`apikey-read`），「装不下明文」由后端应答的形状（`file_face_tests` · 跨语言金样
//   `tests/__fixtures__/apikey.golden.json` 的零明文断言）与 TS 解码器的严格收（`tests/frontend/ui/apikey-reads.vitest.ts`：多一格就抛）钉着。

// ================================================================ `K-H2` `KH7`

// 墓碑：这里从前是 `PLAINTEXT_HOPS`〔散文墓碑〕与 `the_plaintext_argument_is_only_ever_handed_one_hop_further`〔散文墓碑〕
//   （`K-H2` `KH7`：明文入参在 monitor 里每一跳只许被往下传登记过的那几次 —— Tauri 命令 `write_apikey_credentials_key`〔散文墓碑〕→
//   `apikey_remote::write_key_on`〔散文墓碑〕→ `send_key`〔散文墓碑〕装进 `args.key`）。写 key 改走通道之后，明文在 monitor 里
//   **没有具名绑定**了：它是 `chan_call` 转手的一段不透明字节。下面那一条把「没有」钉成零命中。

/// ★★★ **monitor 生产段里没有一处能把明文 key 叫出名字的写口**（零命中，带正控）。
///
/// 要求住址：`K-H2` `KH7`（明文只许被往下传登记过的那几次）—— 今天那几次在 monitor 里是**零**：界面经通道
/// `chan.call(这台, "apikey-key-set", {configDir, key})` 交那台机器的后端（`src/frontend/ui/apikey-reads.ts::writeApikeyKey`），
/// monitor 只转不透明字节。一旦有人在 monitor 里再开一条收 key 的 Tauri 命令、或自己发 `apikey-key-set`，本条红。
///
/// 人群 = monitor 生产段全部 `.rs`（`src/frontend/shell/src`）。针 = 那条旧命令名 ＋ 帧命令名。正控：后端命令表账号那一族（`registry/accounts.rs`）里那条帧命令数得到；
/// 前端那一处发送口恰好一处（`src/frontend/ui/apikey-reads.ts`）。
#[test]
fn hx2_the_monitor_names_no_plaintext_key_on_the_way_to_the_backend() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let needles = [
        format!("write_apikey_{}_key", "credentials"),
        format!("\"apikey-{}-set\"", "key"),
    ];
    let mut hits: Vec<String> = Vec::new();
    let mut scanned = 0usize;
    for (path, raw) in guard_core::scan_tree_excluding(&root, &["rs"], &[]) {
        scanned += 1;
        let prod = guard_core::production_code(&raw);
        for n in &needles {
            if prod
                .lines()
                .any(|l| !l.trim_start().starts_with("//") && l.contains(n.as_str()))
            {
                hits.push(format!("{} · {n}", path.display()));
            }
        }
    }
    assert!(scanned > 100, "只扫到 {scanned} 份 —— 遍历坏了，本条在空转");
    assert!(
        hits.is_empty(),
        "monitor 生产段又能叫出明文 key 的写口了：{hits:?}"
    );
    // 正控 ①：同一根针在后端命令表（账号那一族）里数得到（针没瞎）。
    let inbound = guard_core::production_code(include_str!(
        "../../../src/backend/stream/inbound/registry/accounts.rs"
    ));
    assert!(
        inbound.contains(needles[1].as_str()),
        "正控失败：后端命令表里也数不到那条帧命令"
    );
    // 正控 ②：前端发送口恰好一处。
    let ts = include_str!("../../../src/frontend/ui/apikey-reads.ts");
    assert_eq!(
        ts.matches(needles[1].as_str()).count(),
        1,
        "前端发 apikey-key-set 的地方不是恰好一处"
    );
}

// ── 加账号表单 apikey 那一支的 Base URL ─────────────────
// 这里原来两条（给了 Base URL 落进那一行、别的不动 · 形状不对整次不写），打的是 monitor 那侧写口；
// 写者换成那台的后端之后，同一组性质由后端那一份判：
// `file_face_tests::base_url_is_written_with_the_key_and_left_alone_when_only_the_key_changes`
// （写入读回 · 只配 key 不动端点 × 三种缺席形 · 四种坏形状拒且文件逐字节不动 · 与装表同一个谓词 `upstream_url_core::usable`）。
