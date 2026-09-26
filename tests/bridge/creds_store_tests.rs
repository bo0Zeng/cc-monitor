//! # 要求住址：`INVARIANTS §42` → `src/doc/IPC-PROTOCOL.md`「上游选择那份凭据文件在「这台机器」上的读写」节
//!
//! 核原文：该节逐字「**每台机器上的程序写者恰好一个**」·「**路径**与那台机器上 `--relay` 进程的上游选择**同一个出处**」。
//! 〔GP1 · 第四波〕主会话 09-25 裁那一个写者 ＝ **那台的后端**（本机 ＝ 本机常驻后端），monitor 不再写本机那一份
//! （`调研/第四波记录/GP1.md §3`）。本族今天判 monitor 这一侧剩下的：与后端算同一份文件并显式交出去 ·
//! 明文只往下传登记过的那几跳 · 写半边零调用 · 账号 id 只有一份规则。〔US1〕读侧（三态 · 权限提醒）与「写下的那一行
//! 正是起会话那一侧找的那一行」随读者换成那台后端一起搬去后端那一份判据。写路那几条性质（写的那一刻读盘 · 未知键一个不吃 ·
//! 出生即只给本人 · Base URL 形状错整次不写）住后端那一份写口的判据（`tests/backend/accounts/upstream/file_face_tests.rs`）。
//! 明文出口跨三棵树逐处计数那几条守 `设计/20 §6` 第 4 行逐字「明文只有一个出口」（原文点一个，判据登记两个 —— 原文比判据窄）。
//! ⚠ 「key 不进 `config.json`」与 TS 状态类型对拍那几条没有逐字原文。
//! 与 `crates/creds-core/store_tests.rs` 不重复：那族判纯函数。〔JA1 点址 2026-09-24〕

use super::*;

/// ★★ **跨 crate 契约对拍**：monitor 与后端算出来的是**同一份文件**。
///
/// 两边各写一份路径字面量的话，漂开的那天没有任何东西会说，
/// 而症状是「界面上配好了，上游选择说没配」——查不出来的那一类。
#[test]
fn the_two_sides_resolve_the_same_file() {
    let home = dirs::home_dir().expect("这台机器得有 home");
    let mine = resolve_path().expect("monitor 侧算得出来");
    // backend 侧算法：`store::path_under_claude_home(<claude 家目录>)`。
    let backends = store::path_under_claude_home(&home.join(".claude"));
    assert_eq!(mine, backends, "两侧算出来的凭据文件路径不一样 —— 契约漂了");
    // 非空对照：这把尺子分得出不同的路径（不是恒相等）。
    assert_ne!(
        mine,
        store::path_under_claude_home(&home.join(".claude-other"))
    );

    // ★★ `K-H2b` `D1 阻-3`：**上面那个 `home.join(".claude")` 是手写的根** ——
    // 它钉住的只有**相对段**（`claudecode-frontend/apikey-credentials.json` 这一截），
    // 钉不住「两侧的**根**会不会算到两个地方去」。而那正是阻-3 的病：
    // backend 侧的根走 `resolve_home()`，它**认 `CLAUDE_CONFIG_DIR`**；
    // monitor 这一侧**刻意不跟随**（本模块头注逐字）⇒ 中转一旦继承到那个变量，
    // 两侧读写的就是两份文件，而症状是「界面上配好了，上游选择说没配」。
    //
    // ⇒ 今天买断这一格的**不是**路径算法，是**把路径显式传过去**：
    // 〔RL1〕起本机后端时用 `CCM_APIKEY_CREDENTIALS` 把**本函数算出来的这一个**交给它
    // （中转与上游选择住在那个进程里，`local_backend_host::relay_host_envs`）。
    //
    // 🔴 `D6 阻-1` 回修（08-29）：这里先前是两条「`local_backend_host.rs` 的生产段里有没有
    // `crate::creds_store::resolve_path()` / `"CCM_APIKEY_CREDENTIALS".into()` 这两段文本」——
    // **同一族的病**（文本留住、行为摘掉：把那两段文本留在一处用不到的地方，
    // 真正交出去的换成别的路径 ⇒ 两条照绿）。
    // ⇒ 换成读 `local_backend_host::relay_host_envs()` **产出来的那一份**：
    // 那一格的值必须逐字节等于本函数算出来的路径。
    let envs = crate::local_backend_host::relay_host_envs();
    assert_eq!(
        envs.iter()
            .find(|(k, _)| k == "CCM_APIKEY_CREDENTIALS")
            .map(|(_, v)| v.clone()),
        Some(mine.display().to_string()),
        "起中转那一侧交出去的凭据路径不是**本函数**算出来的这一个 —— 它会退回去读 \
             `CLAUDE_CONFIG_DIR` 底下那份，而 monitor 写的这一份不跟随它。实得：{envs:?}"
    );
    // 反空真：这把尺子分得出「不是那个路径」（不是恒相等）。
    assert!(
        !envs.iter().any(|(_, v)| *v
            == store::path_under_claude_home(&home.join(".claude-other"))
                .display()
                .to_string()),
        "这把尺子对任何路径都说「是」—— 它恒真，本条按红处理"
    );
}

/// `KS7`：它**不是**前端整份读写的那份配置。
#[test]
fn the_key_never_lands_in_the_config_file_the_frontend_rewrites_wholesale() {
    let cfg = crate::paths::resolve_config_path().expect("config path");
    let creds = resolve_path().expect("creds path");
    assert_ne!(cfg, creds, "凭据落在了前端『读—改—写』整份的那个文件上");
    // 同一个目录是**可以**的（`§0a` 要的是「不进那份配置」，不是「不同目录」）。
    assert_eq!(cfg.parent(), creds.parent());
    // ★ 机检：`config.rs` 的生产段里不许出现那个字段名 ——
    //   它一旦出现，就说明有人把 key 塞进 `load_config`/`patch_config` 那条路了（〔CFG1〕写口从整份换成按键补丁）。
    let cfg_src = guard_core::production_code(include_str!("../../src/bridge/src/config.rs"));
    assert!(
        !cfg_src.contains(store::KEY_FIELD),
        "`config.rs` 的生产段里出现了 `{}` —— key 进了前端整份读写的那份配置",
        store::KEY_FIELD
    );
    // 非空对照：这把尺子**认得出**那个字段名（不是恒不含）。
    assert!(store::TEMPLATE.contains(store::KEY_FIELD));
}

/// 从 `at` 之后的第一个 `{` 起按花括号配平切一整块。替掉 `.find("\n}")` 那种找收尾的写法
/// （它撞 `needle_anchor_registry` 的递减棘轮，而且会在块里第一个顶格 `}` 上停住）。
fn brace_block(src: &str, at: usize) -> Option<&str> {
    let open = src[at..].find('{')? + at;
    let b = src.as_bytes();
    let (mut depth, mut i) = (0i32, open);
    while i < src.len() {
        match b[i] {
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(&src[open..=i]);
                }
            }
            _ => {}
        }
        i += 1;
    }
    None
}

// 〔GP1 · 第四波〕这里原来是两条写路判据（`KS10` 交错写不吃人手编的 · `K-R1` 配 key 不吃同一行的
// `auth_style` / `base_url`），打的是 monitor 那侧的写口。写者换成了那台的后端 ⇒ 两条原样搬去后端那一份写口：
// `file_face_tests::gp1_a_program_write_keeps_everything_the_human_put_there` ·
// `file_face_tests::gp1_a_saved_key_does_not_swallow_the_hand_written_upstream_or_auth_style`。

// 〔US1 · 第四波 4D〕读侧那两条（`KS11` 权限放宽出声 · 三态「读坏了不许退化成没配」）打的是 monitor 那一份状态读者
//   （`creds_store::read_status_at`〔散文墓碑〕）。读者换成那台的后端（`apikey-read`）⇒ 两条原样搬去后端那一份读口：
//   `file_face_tests::us1_a_widened_file_is_called_out_and_an_owner_only_one_is_not` ·
//   `file_face_tests::us1_a_broken_file_is_surfaced_instead_of_looking_unconfigured`。

// ============================================================ `K-H2c` `KH2C1` / `KH2C3`

// 〔US1 · 第四波 4D〕「写侧落下的那一行，正是起会话那一侧会去找的那一行」那条跨两半的判据
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
    // 〔GP1 · 第四波〕写侧推 id 的那一半今天住 `apikey_remote.rs`（`send_key`；本机远端两臂同一个），不再住 `creds_store.rs`。
    let src = guard_core::production_code(include_str!("../../src/bridge/src/apikey_remote.rs"));
    guard_core::assert_no_test_code("apikey_remote 写侧 id 规则", &src);
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
    // 〔C4d · 第四波 4B〕正控从 `history.rs` 挪到 `remote_history.rs`：前者那几处 `file_name(`（本机历史清单扫目录）
    //   随清单搬进本机后端一起没了；后者 `jsonl_stem` 取文件名那一句是同一族针（`rsplit('/')`），今天真在。
    let other = guard_core::production_code(include_str!("../../src/bridge/src/remote_history.rs"));
    assert!(
        other.contains("rsplit('/')"),
        "同一把尺子在 `remote_history.rs::jsonl_stem` 上也数出 0 —— 它恒 0，本条按红处理"
    );
}

// 〔GP1 · 第四波〕`KH2C3` 后半（顶层那一把不再是写入目标：结构 ＋ 行为两维）打的是 monitor 那侧写口；
// 写口去了后端 ⇒ 原样搬去 `file_face_tests::gp1_the_write_side_never_targets_the_legacy_top_level_slot`。

/// ★ 说不出 id 的时候**报错，不回落**。
///
/// 回落到顶层那一格的症状是「用户以为配给了 A，实际写进了 `default`」，
/// 而 `default` 那一行**谁的会话都命中得了** —— 与 `K-H2` `KH2` 逐字禁的
/// 「查不到就拿默认行顶上」是同一族。
#[test]
fn a_config_dir_that_names_no_account_is_refused_instead_of_falling_back() {
    // 〔GP1 · 第四波〕入口换成生产那一个（`apikey_remote::write_key_on`，本机那一臂）：说不出 id ⇒ **一次往返都不做**就拒
    //   （它排在「核路径」那一问与发帧之前），话里说清原因。
    let local = crate::origin::Origin(crate::origin::LOCAL.to_string());
    for bad in ["", "   ", "/"] {
        let e = tauri::async_runtime::block_on(crate::apikey_remote::write_key_on(
            &local,
            bad,
            "KEY-SHOULD-NOT-LAND".to_string(),
            None,
        ))
        .expect_err("说不出账号却写成功了 —— 那一把落到哪儿了？");
        assert!(e.contains("说不出这是哪个账号"), "报错没说清原因：{e}");
        assert!(!e.contains("KEY-SHOULD-NOT-LAND"), "报错里带着明文：{e}");
    }
    // 非空对照：同一个入口喂一个说得出 id 的 configDir ⇒ 过了这一关、走到「问那台后端」那一步
    //   （判据进程里没有本机那条长连接 ⇒ 报的是别的原因，不是「说不出账号」）。
    let e = tauri::async_runtime::block_on(crate::apikey_remote::write_key_on(
        &local,
        "/h/.claude-accts/acct-ok",
        "KEY-OK".to_string(),
        None,
    ))
    .expect_err("判据进程里没有本机后端，不该写成");
    assert!(
        !e.contains("说不出这是哪个账号"),
        "说得出 id 的也被当成说不出了：{e}"
    );
}

/// 明文两个出口，各自**只许出现在哪棵树的哪个文件里**。
///
/// `(方法名, 期望总处数, 期望它住在哪个文件的路径尾巴)`。**默认拒绝**：对不上就红。
const PLAINTEXT_EXIT_SITES: &[(&str, usize, &str)] = &[
    (
        "expose_for_auth_header(",
        1,
        "src/backend/accounts/upstream/mod.rs",
    ),
    (
        "expose_for_persisting(",
        1,
        "crates/creds-core/src/store.rs",
    ),
];

/// 本判据扫哪几棵树。**这就是「取明文恰好 N 处」那句全称的分母。**
const PLAINTEXT_SCAN_TREES: &[&str] = &["src/bridge/src", "src/bridge/crates", "src/backend"];

/// ★★★ **`KS2` 的人群那一格〔D1 阻-1 回修，08-27〕：三棵树全扫，不是一个文件、也不是一个 crate。**
///
/// # 它替掉的是一个**按 crate 边界画的人群**
///
/// 回修前，「取明文恰好 N 处」这条性质由两处判据分管，而它们的人群加起来**盖不住产品**：
/// · `creds-core/src/lib.rs` 只扫 `include_str!("../../src/bridge/src/lib.rs")`——**它自己这一个文件**；
/// · `relay/creds_guard.rs` 扫后端那个 crate；
/// ⇒ **`src/bridge` 整个不在任何人的人群里**，而 monitor 恰恰是明文**第一次进程序**的地方
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
/// ⚠ 〔`P4` 2026-09-21〕先前这一行写着「**外加本文件自己**（见下面那段：`scan_tree!`
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
        // 🔴 〔搬树 2026-09-18 · `设计/99` 条 73〕**明写「一份都不排除」，不靠 `file!()`。**
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
    // 🔴 〔搬树 2026-09-18 · `设计/16 §6.2` C 类〕**住址改对：那张表搬家了。**
    //
    // `INNER_FIELD_USERS` 是 `creds-core` 的**判据用表**（住在它的 `#[cfg(test)]` 段里），
    // 剖分把它从 `crates/creds-core/src/lib.rs` 搬到了
    // `tests/bridge/crates/creds-core/lib_tests.rs`。表本身一个字没改 ——
    // 变的只是它住哪儿。⇒ 指对地方，不是放宽（读数逐字：「一处都找不到 ⇒ 本条按红处理」，
    // 这条反空真**按设计响了**，别把它调松）。
    let table_home =
        crate::guard_support::tests_root().join("bridge/crates/creds-core/lib_tests.rs");
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
    //   会随搬树静默失配（`设计/16 §5.4b`）—— 下面那条 `table.len() > 200` 是它的反空真。
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

// 〔GP1 · 第四波〕这里原来是两条 `KS5` 源码判据（写口里收窄恰好两次 · tmp 出生即窄），切的是 monitor 那侧写口的函数体；
// 写口随写者换成那台的后端一起走了。「出生即只给本人」今天由后端那一份的两条兜：写半边只从账号域那一份写口够得着
// （`readonly_guard::…::the_credentials_write_half_is_reached_only_from_the_account_file_face`）· 写出来的文件是 `0600`
// 且不留临时文件（`file_face_tests::the_written_file_is_owner_only_and_no_temp_file_is_left`）。
// monitor 这一侧只剩一条要钉的：**写半边一处都不调**（`gp1_the_monitor_never_reaches_the_credentials_write_half`）。

/// 〔GP1 · 第四波〕**monitor 生产段一处都不够写半边**（`creds_core::perm::create_private` / `make_private`）。
///
/// monitor 仍开着 `harden`（Windows 上读 DACL 要它），编译器因此兜不住「monitor 写不了这份文件」—— 由本条兜：
/// 人群 = `src/bridge/src` 下全部 `.rs` 的生产段（剥测试段与注释），针两根，**零命中**。
/// 正控：同一把针在后端那一份写口（`src/backend/accounts/upstream/file_face.rs`）上数得到 —— 针没瞎。
/// 要求住址：主会话 09-25 裁「每台机器上这份文件的程序写者恰好一个 ＝ 那台的后端」（`GP1.md §3`）。
#[test]
fn gp1_the_monitor_never_reaches_the_credentials_write_half() {
    let needles = [
        format!("create_{}(", "private"),
        format!("make_{}(", "private"),
    ];
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
    let face = guard_core::production_code(include_str!(
        "../../src/backend/accounts/upstream/file_face.rs"
    ));
    assert!(
        needles.iter().any(|n| face.contains(n.as_str())),
        "正控失败：后端那一份写口里也数不到 —— 针瞎了"
    );
}

// 〔US1 · 第四波 4D〕`ApikeyCredentialsStatus` 那两条（TS 手写类型双向对拍 · 类型装不下明文）随结构体一起退役：
//   状态由那台后端出成品（`apikey-read`），「装不下明文」由后端应答的形状（`file_face_tests` · 跨语言金样
//   `tests/__fixtures__/apikey.golden.json` 的零明文断言）与 TS 解码器的严格收（`tests/apikey-reads.vitest.ts`：多一格就抛）钉着。

// ================================================================ `K-H2` `KH7`

/// 明文那个入参从 IPC 边界进来之后，一路上**每一跳**允许它出现的地方。
///
/// 每行 `(文件, 切窗口的锚点, 明文那个绑定叫什么, 它该出现的那几个写法)`。
/// 绑定在窗口里出现的次数必须**等于**那几个写法的条数，而且每一条都在。
/// ⚠ **加一行、或给某一行多登一个写法，都是放宽** —— 要先说清多出来的那一处是什么。
///
/// 〔RM1a · 第四波〕这张表从「每跳恰好 1 处」改成「每跳逐处登记」，**只为一格**：
/// `apikey_remote::write_key_on` 按机器分两臂，明文在那个函数体里**就是**两处。其余每跳仍然恰好 1 处（表里各登一条）。
/// 〔GP1 · 第四波〕两臂今天同一条路（都交 `send_key`，本机那一臂多带一格「该是哪一份文件」）；
/// 本机那两跳（`creds_store` 的写口）随写者换成本机常驻后端一起退了 ⇒ 表少两行。
/// `send_key` 装进 `args` 之后就是通用的帧面编码，本表管到那一跳为止。
const PLAINTEXT_HOPS: &[(&str, &str, &str, &[&str])] = &[
    (
        "lib.rs",
        "fn write_apikey_credentials_key(",
        "key",
        // 〔ST2 × RM1a〕Base URL（明文端点，不是凭据）跟着一起按机器走；明文 key 仍然只往下传这一次。
        &["apikey_remote::write_key_on(&origin, &config_dir, key, base_url)"],
    ),
    (
        "apikey_remote.rs",
        "pub(crate) async fn write_key_on(",
        "key",
        &[
            "send_key(LOCAL, config_dir, key, base_url, Some(local_file()?))",
            "send_key(host, config_dir, key, base_url, None)",
        ],
    ),
    (
        "apikey_remote.rs",
        "pub(crate) async fn send_key(",
        "plain",
        &["\"key\": plain"],
    ),
];

/// 数一个**标识符**出现几次 —— 带词边界，不是子串。
///
/// ⚠ **这个助手是第一跑逼出来的，经过记下来**：第一版直接用 `matches(binding).count()`，
/// 实测 `key` 在 `write_apikey_credentials_key` 的函数体里数出 **2** 次 ——
/// 因为它调的那个函数**自己就叫 `write_key`**，`key` 是它的后缀。
/// ⇒ 那一版数的根本不是「明文被碰了几次」，是「这几个字母出现了几次」。
/// **本工作区最贵那族病的又一形：尺子的作用域对不上事实。**
fn count_ident(hay: &str, ident: &str) -> usize {
    fn is_ident_byte(c: u8) -> bool {
        c.is_ascii_alphanumeric() || c == b'_'
    }
    let b = hay.as_bytes();
    let (mut n, mut from) = (0usize, 0usize);
    while let Some(rel) = hay[from..].find(ident) {
        let at = from + rel;
        from = at + ident.len();
        let left_ok = at == 0 || !is_ident_byte(b[at - 1]);
        let right = at + ident.len();
        let right_ok = right >= b.len() || !is_ident_byte(b[right]);
        if left_ok && right_ok {
            n += 1;
        }
    }
    n
}

/// ★★★ **`K-H2` `KH7` 的机检那一半**：明文入参**只许被往下传一次**，
/// 一路上不许进日志、不许被拷进任何别的东西。
///
/// # 它补的是哪一格（别把它读大）
///
/// `KS6` 保的是 key **回**前端那个方向（`ApikeyCredentialsStatus` 在**类型上**装不下明文）。
/// **去**后端那个方向 `write_apikey_credentials_key(key: String)` **入参就是明文**，
/// 而 `K-H2a` 把它逐字登记成 **`判不了`**（`lib.rs` 那段头注：
/// 「⇒ 它的身份是 **`判不了`**，不是「射程外」。**这两个词不是一回事**：
/// 前者欠着一次测量，后者是已经裁过不做。」）。
///
/// 本条**没有**把那一格变成「判得了」。它买到的是**出口之后那一段**：
/// 明文一进来就只有一条路可走 —— 一路传到 `SecretKey::new`，中间任何一处
/// 多碰它一次都会红。
///
/// # ⚠⚠ 它**不保**什么（三条，逐条写死）
///
/// 1. **IPC 那一跳本身仍然判不了**：明文经 WebView 的消息通道序列化过来，
///    那一段不在本仓的写区，本条一个字都没打过它。**原样延续 `K-H2a` 的登记。**
/// 2. **不判语义**：`SecretKey::new(plain)` 里面把明文交给谁，编译器与本条都不管
///    （那是 `K-H2a` 的 `mod sealed` + `KS2` 的活）。
/// 3. **人群是这三个函数体**，不是「所有碰得到明文的代码」。第四跳出现时没有东西会红
///    —— 加一跳就来加一行，那正是要的。
///
/// # 量法与分母
///
/// 窗口 = 每一跳那个函数的花括号块（**有界**），并配两条反空真自检
/// （切不出来 ⇒ 红 · 跨进下一个 item ⇒ 红）。窗口里**先剥注释行**再数
/// —— 判据只该看生效的代码，不该看解释它的话（`creds_guard` 那条 `harden` 判据
/// 第一跑就是被自己的注释撞红的，同一条教训）。
#[test]
fn the_plaintext_argument_is_only_ever_handed_one_hop_further() {
    let sources: &[(&str, &str)] = &[
        ("lib.rs", include_str!("../../src/bridge/src/lib.rs")),
        (
            "apikey_remote.rs",
            include_str!("../../src/bridge/src/apikey_remote.rs"),
        ),
    ];

    for (file, anchor, binding, uses) in PLAINTEXT_HOPS {
        let raw = sources
            .iter()
            .find(|(f, _)| f == file)
            .map(|(_, s)| *s)
            .unwrap_or_else(|| panic!("登记表里的文件 {file} 没被采集 —— 取法坏了"));
        let src = guard_core::production_code(raw);
        let at = guard_core::find_pinned(&src, anchor).unwrap_or_else(|e| {
            panic!("切不出 {file} 的 `{anchor}`（{e}）—— 本条按红处理，不是绿")
        });
        let body = brace_block(&src, at)
            .unwrap_or_else(|| panic!("{file} 的 `{anchor}` 花括号没配平 —— 按红处理"));

        // 反空真自检㈠：窗口不许跨进下一个 item。
        assert!(
            !body.contains("\nfn ") && !body.contains("\npub"),
            "{file} 的 `{anchor}` 窗口跨进了下一个 item —— 窗口无界，下面的断言不算数"
        );
        // 反空真自检㈡：窗口里**确实**有那个绑定（切错地方会让下面恒绿）。
        let code = guard_core::strip_comment_lines(body);
        assert!(
            count_ident(&code, binding) > 0,
            "{file} 的 `{anchor}` 窗口里根本没有 `{binding}` —— 切法坏了，本条在空转"
        );

        // ★ 正题：那个明文绑定出现的次数**等于**登记的写法条数（**按标识符数，不按子串**），
        //   且每一条登记的写法都在。
        let n = count_ident(&code, binding);
        assert_eq!(
            n,
            uses.len(),
            "{file} 的 `{anchor}` 里，明文绑定 `{binding}` 出现了 {n} 次，登记的是 {} 处。\n\
                 ⚠ `K-H2` `KH7`：明文入参只许被往下传登记过的那几次。多碰一次就多一个出口 ——\n\
                 进了一句日志 / 被拷进一个错误消息 / 被塞进一个结构体，都会撞这一条。\n\
                 真要多一处，先在件计划里说清那一处是什么，别在这里把次数改大。\n\
                 窗口（已剥注释）：{code}",
            uses.len()
        );
        for only_use in *uses {
            assert!(
                code.contains(only_use),
                "{file} 的 `{anchor}` 里没有登记的写法 `{only_use}` —— 靶子挪了。\n\
                     窗口（已剥注释）：{code}"
            );
        }
    }
}

// ── 〔第四波 ST2 · `设计/70 §4.4`〕加账号表单 apikey 那一支的 Base URL ─────────────────
// 〔GP1 · 第四波〕这里原来两条（给了 Base URL 落进那一行、别的不动 · 形状不对整次不写），打的是 monitor 那侧写口；
// 写者换成那台的后端之后，同一组性质由后端那一份判：
// `file_face_tests::base_url_is_written_with_the_key_and_left_alone_when_only_the_key_changes`
// （写入读回 · 只配 key 不动端点 × 三种缺席形 · 四种坏形状拒且文件逐字节不动 · 与形状关 `check_base_url_shape` 同一个）。
