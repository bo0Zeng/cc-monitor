// 本机起会话那一族判据（旧路形状 · 账号三态 · 注入闸 · `ccm` 容器路 · 中转前缀 · 身份 token · 三条缝）
//   随计划与渲染搬进本机后端：`tests/backend/control/launch_render/local_tests.rs`。这里只剩「只一处」那几条（读正文那一族随判定进了后端）。
use std::path::PathBuf;

/// 每个测试独占的临时目录（仓库约定不引 `tempfile`，用 pid + 计数器保唯一）。
/// **绝不碰用户真实的 `~/.claude`** —— 全部在 `std::env::temp_dir()` 下。
struct TmpDir(PathBuf);
static TMP_SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
impl TmpDir {
    fn new() -> Self {
        let p = std::env::temp_dir().join(format!(
            "hist-{}-{}",
            std::process::id(),
            TMP_SEQ.fetch_add(1, std::sync::atomic::Ordering::SeqCst)
        ));
        std::fs::create_dir_all(&p).expect("mkdir");
        TmpDir(p)
    }
    fn write(&self, name: &str, body: &str) -> PathBuf {
        let f = self.0.join(name);
        std::fs::write(&f, body).expect("write");
        f
    }
}
impl Drop for TmpDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

// 这里原来是本机分叉读 jsonl 那一格（`read_jsonl_values`〔散文墓碑〕：剥 BOM · 静默丢坏行）
// 的判据。本机分叉交给后端之后那个函数零调用方、删了；后端那份读法的判据住 `fork_write_tests.rs`。

/// ★★〔`K-R97` 09-12 后继形态〕**「从 jsonl 头部抠 cwd」这件事，全仓只剩一处了。**
///
/// # 原形是什么、为什么换
///
/// 原形叫「两个提取器仍旧照登记的样子不一致」：同一个问题两处实现 ——
/// monitor 窗口 **30** 行且只认 `JsonlRecord::User`，后端窗口 **40** 行且认任何带非空 cwd
/// 的记录。后果具体：首个带 cwd 的记录落在第 31–40 行时，**两边给两个答案**。
/// 那一版**只钉不改**（走档①：登记 + 钉住），因为「取 30 还是 40」是会改行为的设计决定。
///
/// `K-R97` 把本机项目列表改走后端那条 `--list-projects` ⇒ monitor 那一份**连同它唯一的
/// 调用点一起没了**。⚠ **这不是「对齐到 40」**，是那个设计决定**不再需要有人做** ——
/// 问题只剩一个实现，也就无从不一致。
///
/// ⇒ 本条换成后继形态：**钉住 monitor 侧不许再长出第二份**，并核后端那一份还在。
/// 后端那一份今天住适配层 `agents::first_in_head`（按字节有上界），历史清单 · 会话宣告 · 会话事实的项目目录都问它。
/// ⚠ **这不是降强度**：原形钉的是两个数的差（谁改了都红），后继钉的是「只剩一处」
/// （谁把第二份写回来都红），而后者恰恰是 `K33`「所有命令只许有一处」的形状。
#[test]
fn extracting_cwd_from_a_jsonl_head_now_lives_in_exactly_one_place() {
    // ① monitor 生产段：**一个头部窗口读法都不许有**。
    let own =
        guard_core::production_code(include_str!("../../../src/frontend/shell/src/history.rs"));
    let local: Vec<&str> = own
        .lines()
        .filter(|l| l.contains("reader.lines().map_while(Result::ok).take("))
        .collect();
    assert!(
        local.is_empty(),
        "monitor 侧又长出了一份 jsonl 头部读法：\n{}\n\n\
             ⇒ 这件事的家在后端适配层（`agents::first_in_head`）。\n\
             真要在 monitor 侧读，先回答「为什么这条路问不了后端」，再连同本条一起改。",
        local.join("\n")
    );

    // ② 后端那一份还在、恰好一处：适配层的 `agents::first_in_head`（窗口按字节，`HEAD_CAP`），
    //    历史查询（`observe/history_query.rs`）自己一份头部窗口都没有，只问适配层 —— 否则上面那条会零命中地绿。
    let read = |rel: &str| {
        guard_core::production_code(
            &std::fs::read_to_string(crate::guard_support::repo_root().join(rel))
                .unwrap_or_else(|_| panic!("读不到 {rel}")),
        )
    };
    let agents = read("src/backend/agents/mod.rs");
    assert_eq!(
        (
            agents.matches("fn first_in_head(").count(),
            agents.matches("file.take(HEAD_CAP)").count()
        ),
        (1, 1),
        "后端那一份不是「恰好一处、按字节有上界」了"
    );
    let history = read("src/backend/observe/history_query.rs");
    assert_eq!(
        history
            .lines()
            .filter(|l| l.contains("reader.lines().map_while(Result::ok).take("))
            .count(),
        0,
        "历史查询里又长出了一份自己的头部窗口（项目目录只问适配层）"
    );
}

// 历史清单与注解搬进本机常驻后端（join 只一个家、注解读写者是本机后端）——
//   这里原先驱动 monitor 那一份实现的几组判据随被测函数一起退役，它们钉的性质各自在新家有判据（逐条对应）：
//   - `K-R97` 本机清单来自后端那一行 · 本机判活答真值 · 远端只问一次（`the_local_project_list_is_whatever_the_backend_said` 那一组〔散文墓碑〕）
//     ⇒ 后端 `tests/backend/history/history_list_tests.rs`（`the_machine_listing_carries_group_dir_failures_synth_and_last_accounts` ·
//     `a_real_record_tree_becomes_the_machine_listing` · `a_remote_is_asked_raw_once_and_cached_until_fresh`）；
//     「本机后端不在 ≠ 一个项目都没有」⇒ 通道的失败层级（`src/frontend/ui/history-reads.ts` 抛、界面说「加载失败」），判据 `tests/frontend/ui/history-reads.vitest.ts`；
//   - `K-R92` 分得开「不知道」与「真的是 0」· 「不知道」自成一档排序（`the_three_counts_can_say_i_do_not_know` 那两条〔散文墓碑〕）
//     ⇒ 后端 `history_list_tests.rs`（`unreadable_annotations_say_so_and_the_rows_still_come` · `groups_rank_live_then_starred_then_recent_and_keep_failed_dirs`）；
//   - 线上形状的驼峰契约（`history_project_camel_case_contract` 那两条〔散文墓碑〕）⇒ 跨语言金样 `tests/__fixtures__/history-list.golden.json`
//     （后端产 · TS 解码器逐键收）；
//   - Codex 分组 · 首条真用户话去注入（`codex_projects_group_by_cwd` 那两条〔散文墓碑〕）⇒ 后端 `tests/backend/agents/codex/history_tests.rs`
//     ＋ `history_list_tests.rs::the_machine_listing_carries_group_dir_failures_synth_and_last_accounts`；
//   - 上次账号的 serde 与 patch 三态 · 只含真有的那几条（`last_account_serde_and_patch_semantics` 那两条〔散文墓碑〕）⇒ 后端
//     `tests/backend/history/history_annotations_tests.rs`（`patch_semantics_match_what_the_monitor_did`；上次账号那一格今天不归注解，住会话所在那台的起会话账号记录）；
//   - 摘录按字符截断（`truncate_chars_unicode` 那三条〔散文墓碑〕）⇒ 通用搜索口径的 `truncate_excerpt`（后端会话行改用它；今天住 `observe/search_rules.rs`）；
//   - 「迁移前」旧读者读注解夹具 == 金样（`c4d_the_old_reader_reads_the_annotation_fixture_as_the_golden`〔散文墓碑〕，子步 4 那一拍对过）
//     ⇒ 金样 `tests/__fixtures__/history-metadata.readout.golden.json` 留作「迁移前」的冻结读数，后端新读者照旧对它。

// 删会话那道 stem 一致性闸的两条判据（只交 sid · 对不上一个请求都不发）与
//   分叉结果形状那一条（驼峰键）随 monitor 那两条命令删了：界面经通道直说那台后端（`src/frontend/ui/session-writes.ts`），
//   分叉成品由金样 `tests/__fixtures__/session-fork.golden.json` 钉（`tests/frontend/ui/session-writes.vitest.ts` 读同一份）；
//   stem 闸是恒真的（会话行的 `sessionId` 由后端按文件名 stem 出），后端删之前自己判「落点恰是 `<sid>.jsonl`」。

// 这里原来是本机分叉那份实现的四条 IO 判据（`O_EXCL` 不覆盖 · 软链逃逸按 sid 找不到 ·
// 源零改动 ＋ 新文件原生格式 · 以及它们共用的最小会话夹具）。本机分叉改成 exec 本机后端 `--fork-session` 之后，
// 同一组性质由后端那份实现（`src/backend/control/fork_write.rs`）的同形判据守着（`fork_write_tests.rs`），
// 本侧只剩「结果怎么解释」（`remote_branch_tests.rs`：本机那一趟的三态折成与远端同形的结果）。

/// 后端那棵树上某个文件的**生产段**（运行时读，不是 `include_str!`）。
///
/// ⚠ 刻意**不用** `include_str!`：那会长出一条**编译期**的跨半边，
/// 而 `cross_half_edge_registry` 的头注逐字讲过那条边的代价
/// （后端在目标机上 `cargo build` 就咬住旁边这棵树了）。运行时读没有这个代价 ——
/// 同 `K-R97` 那条 `extracting_cwd_from_a_jsonl_head_now_lives_in_exactly_one_place`。
fn r88_backend_production(rel: &str) -> String {
    let p = crate::guard_support::repo_root().join(rel);
    let raw = std::fs::read_to_string(&p)
        .unwrap_or_else(|e| panic!("读不到后端的 {rel}：{e} —— 先修住址，别绕过本条"));
    guard_core::production_code(&raw)
}

/// ★★ `KR88D1`：**「按 sid 找那份会话文件」这件事，全仓只剩一份实现，两侧都调它。**
///
/// # 它买什么
///
/// 收之前两侧各有一份、而且**入参形状都不一样**（这边收路径、那边收 sid）。
/// 那不是「重复」这么简单：**「查不到怎么办」两边可以各答各的**，
/// 而没有任何东西会因此变红。
///
/// # 🔴 它刻意**不**判什么（`KR88D1` 点名的失效方向）
///
/// **不判「两边源码文本一样」** —— 那是判写法，而且很容易恒绿
/// （两边都没有那段文本时它照样通过）。本条判的是**同一份实现**：
/// 唯一那份的**声明只有一处**，两侧各有**恰好一处**调用，
/// 且两条分叉路径上**一处目录枚举都不许有**（有 = 有人又自己找了一遍）。
///
/// 「改那一份一处、两边行为都跟着变」那一刀是**死值验**，读数落在件文件 `§3-1`：
/// 判据不可能替代它 —— 那一刀要真的改一次再看两边红不红。
#[test]
fn finding_a_session_file_by_sid_now_lives_in_exactly_one_place() {
    // 共享 crate `branch-core` 收进后端适配层 `agents/claudecode/branch.rs`；通用层经注册表那一格（`agents::find_session_file`）够它。
    const CALL: &str = "agents::find_session_file(";

    // ① 唯一那份：声明只有一处，住在适配层（运行期读：它在后端那一半，不长编译期的跨半边边）。
    let core = r88_backend_production("src/backend/agents/claudecode/branch.rs");
    let decls = core.matches("pub(crate) fn find_session_file").count();
    assert_eq!(
        decls, 1,
        "适配层里 `find_session_file` 的声明有 {decls} 处（该是 1）。\n\
             0 ⇒ 它被搬走/删了，下面两条会零命中地绿；2 ⇒ 唯一那份自己裂了。"
    );

    // ② **monitor 那一侧零处、后端那一侧恰好一处**。
    //    从前两侧各有一处（本机分叉在 monitor 进程里找、写）；本机分叉改成 exec 本机后端的
    //    `--fork-session` 之后，「按 sid 找那份」只剩后端那一处在问 —— monitor 这一侧再出现一处，
    //    就是有人又在本进程里做分叉了（那正是删掉的那一形：monitor 不直接写用户文件）。
    let mine =
        guard_core::production_code(include_str!("../../../src/frontend/shell/src/history.rs"));
    // monitor 分叉那一侧的模块删了（界面经通道直说 `session-fork`），人群只剩 `history.rs` 与后端。
    let theirs = r88_backend_production("src/backend/control/fork_write.rs");
    for (who, src, want) in [
        ("monitor `history.rs`", &mine, 0usize),
        ("后端 `fork_write.rs`", &theirs, 1),
    ] {
        let n = src.matches(CALL).count();
        assert_eq!(
            n, want,
            "{who} 的生产段里 `{CALL}` 有 {n} 处（该是 {want}）——\n\
                 monitor 那一侧 ≠ 0 ⇒ 又在本进程里找 / 写会话了（`RW1`：分叉两侧都交给后端）；\n\
                 后端那一侧 ≠ 1 ⇒ 不走共享那份了，或一条路上问了两遍。"
        );
    }
    // 「只有一处发送点」那一格挪到界面：前端 `chan.call` 的 `session-fork` 只在 `src/frontend/ui/session-writes.ts`
    //   （`frame_query_tests::the_channeled_ops_are_sent_only_through_the_channel` 数 monitor 零字面量）。

    // ③ 两条分叉路径上**一处目录枚举都没有** —— 「自己又找了一遍」的形状。
    //
    // ⚠ 人群按**那几个函数**切，不是整份 `history.rs`：这个文件别处本来就有遍历
    //（历史列表那一族），拿整份文件当分母的话本条恒红。
    // ⚠ 针**运行时拼**：本文件的测试段自己落在 `scanning_guard_registry` 的扫描面里，
    //   把那两个词写成字面量会让本条被算进「裸遍历」的人群（09-13 现打，它当场逮到了）。
    let needles = [format!("read{}dir(", "_"), format!("Walk{}", "Dir")];
    let scan = |src: &str| -> Vec<String> {
        src.lines()
            .filter(|l| needles.iter().any(|n| l.contains(n.as_str())))
            .map(|l| l.trim().to_string())
            .collect()
    };
    // monitor 那一侧没有分叉那条路了（界面直说后端），人群只剩后端那一份。
    // 反向自检：尺子够得着 —— 把针塞进一份副本，量具必须数得出来。
    let poisoned = format!("{theirs}\n  let _ = std::fs::read{}dir(root);\n", "_");
    assert_eq!(
        scan(&poisoned).len(),
        1,
        "阳性对照没过 —— 量具此刻无效，下面那条断言是空真"
    );
    for (who, src) in [("后端 `fork_write.rs`", theirs.as_str())] {
        let hits = scan(src);
        assert!(
            hits.is_empty(),
            "{who}上又长出了目录枚举：\n{}\n\n\
                 ⇒ 「按 sid 找那份会话文件」的家在 `agents/claudecode/branch.rs::find_session_file`。\n\
                 真有第二种找法要立，先回答「为什么这一侧不能问那一份」，再连本条一起改。",
            hits.join("\n")
        );
    }
}

// 这里原来是 `KR88D2` 的 monitor 那一侧（查不到的 sid ⇒ 报错，不静默挑第一个）。
// monitor 进程里不再有分叉的实现，那条性质只住后端（`fork_write·rs::an_unknown_session_id_is_refused_not_silently_substituted`）。

// `KR88D2`「两侧入参都收 sid、都不收路径」那一条随
//   monitor 那两条分叉命令一起退役：今天只有一处发出分叉（界面 `session-writes.ts::forkSession`，请求体恰好 `{sid, uuid}`，
//   由金样 `session-fork.golden.json` 的 `request` 钉），后端入口只认这两格。

// P3 归并：iso_parse_* 测试已搬到 utils::tests（函数本身搬到 utils）。

/// ★★ **「configDir → 表里的 id」与「哪几个号在表里有行」两条规则只有一个家：`acct-core`**。
///
/// 要求：「**一个判定只有一个家**」。monitor 那一个绑定（`history::apikey_routed_subset`〔散文墓碑〕）
/// 随界面那一问搬进后端一起删了 ⇒ 两棵生产树里两条规则的**定义**都零处；反空真：同一个识别器在 `acct-core` 上各数得出恰好 1。
#[test]
fn the_two_apikey_rules_are_defined_only_in_acct_core() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let defs = ["fn apikey_account_id_of_dir", "fn apikey_routed_subset"];
    let mut found = Vec::new();
    for dir in [root.join("src"), root.join("../../backend")] {
        for (p, rule_src) in guard_core::scan_tree!(&dir, &["rs"]) {
            let rule_prod = guard_core::production_code(&rule_src);
            for d in defs {
                if guard_core::contains_word(&rule_prod, d) {
                    found.push((p.clone(), d));
                }
            }
        }
    }
    assert!(
        found.is_empty(),
        "两条 apikey 规则在两半里又长出了定义：{found:?}"
    );
    let core_prod = guard_core::production_code(
        &std::fs::read_to_string(
            crate::guard_support::repo_src_root().join("common/acct-core/src/lib.rs"),
        )
        .unwrap(),
    );
    for d in defs {
        guard_core::find_pinned(&core_prod, d)
            .unwrap_or_else(|e| panic!("acct-core 里 `{d}` 不是恰好一处：{e}"));
    }
}

// `SessionPager` 那四条（本机远端同一个分页器 · 行号跨页连续 · 种类按文件名判 · 冷读只经后端）
//   随读正文那条命令一起退役：判定进了后端，住 `tests/backend/observe/record_page_tests.rs`（编号 · 只出进界面的 ·
//   按根认是哪一家）与 `tests/backend/faces/read_face_tests.rs`（`history-page` 那一臂）。

// `remote_history.rs` 整份删了（最后一个函数 `require_cfg_by_label`〔散文墓碑〕随子 agent 那条命令退役），它的测试文件里
//   这一条与它无关的对照挪到这里。
#[test]
fn shell_quote_via_the_shared_core() {
    // `stream_source` 那一层转调壳删了（零生产调用方），对照直指共享内核。
    assert_eq!(
        shell_quote_core::posix_quote("/a/b c.jsonl"),
        "'/a/b c.jsonl'"
    );
    assert_eq!(shell_quote_core::posix_quote("a'b"), r"'a'\''b'");
}
