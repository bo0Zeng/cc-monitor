// 〔MIG-2 · `99 §2.1 ⑬`〕本机起会话那一族判据（旧路形状 · 账号三态 · 注入闸 · `ccm` 容器路 · 中转前缀 · 身份 token · 三条缝）
//   随计划与渲染搬进本机后端：`tests/backend/control/launch_render/local_tests.rs`。这里只剩读正文 · 删会话 · 分叉那几族。
use super::*;

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

// 〔RW1 · 第四波 · 2026-09-24〕这里原来是本机分叉读 jsonl 那一格（`read_jsonl_values`〔散文墓碑〕：剥 BOM · 静默丢坏行）
// 的判据。本机分叉交给后端之后那个函数零调用方、删了；后端那份读法的判据住 `fork_write_tests.rs`。

/// ★★〔`K-R97` 09-12 后继形态〕**「从 jsonl 头部抠 cwd」这件事，全仓只剩一处了。**
///
/// # 原形是什么、为什么换
///
/// 原形叫「两个提取器仍旧照登记的样子不一致」〔audit-0805 08-06〕：同一个问题两处实现 ——
/// monitor 窗口 **30** 行且只认 `JsonlRecord::User`，后端窗口 **40** 行且认任何带非空 cwd
/// 的记录。后果具体：首个带 cwd 的记录落在第 31–40 行时，**两边给两个答案**。
/// 那一版**只钉不改**（走档①：登记 + 钉住），因为「取 30 还是 40」是会改行为的设计决定。
///
/// `K-R97` 把本机项目列表改走后端那条 `--list-projects` ⇒ monitor 那一份**连同它唯一的
/// 调用点一起没了**。⚠ **这不是「对齐到 40」**，是那个设计决定**不再需要有人做** ——
/// 问题只剩一个实现，也就无从不一致。
///
/// ⇒ 本条换成后继形态：**钉住 monitor 侧不许再长出第二份**，并核后端那一份还在。
/// ⚠ **这不是降强度**：原形钉的是两个数的差（谁改了都红），后继钉的是「只剩一处」
/// （谁把第二份写回来都红），而后者恰恰是 `K33`「所有命令只许有一处」的形状。
#[test]
fn extracting_cwd_from_a_jsonl_head_now_lives_in_exactly_one_place() {
    // ① monitor 生产段：**一个头部窗口读法都不许有**。
    let own = guard_core::production_code(include_str!("../../src/bridge/src/history.rs"));
    let local: Vec<&str> = own
        .lines()
        .filter(|l| l.contains("reader.lines().map_while(Result::ok).take("))
        .collect();
    assert!(
        local.is_empty(),
        "monitor 侧又长出了一份 jsonl 头部读法：\n{}\n\n\
             ⇒ `K-R97` 之后这件事的家在后端（`observe/history_query.rs`）。\n\
             真要在 monitor 侧读，先回答「为什么这条路问不了后端」，再连同本条一起改。",
        local.join("\n")
    );

    // ② 后端那一份还在，且窗口是个说得出的数 —— 否则上面那条会零命中地绿
    //    （「两边都没有」与「只剩一处」在断言上长得一样，这一格就是分开它们的那个）。
    let backend_src = std::fs::read_to_string(
        crate::guard_support::repo_root().join("src/backend/observe/history_query.rs"),
    )
    .expect("读不到后端的 history_query.rs");
    let remote: Vec<usize> = guard_core::production_code(&backend_src)
        .lines()
        .filter(|l| l.contains("reader.lines().map_while(Result::ok).take("))
        .filter_map(|l| l.split(".take(").nth(1))
        .filter_map(|s| s.split(')').next())
        .filter_map(|s| s.trim().parse::<usize>().ok())
        .collect();
    assert_eq!(
        remote,
        vec![40],
        "后端那一份不是「恰好一处、窗口 40 行」了（实得 {remote:?}）。\n\
             ① 变成 0 处 ⇒ 那件事没人做了，而 monitor 这侧已经不做了；\n\
             ② 变成 2 处 ⇒ 两份实现在后端里面又长了一次。"
    );
}

// 〔C4d · 第四波 4B〕历史清单与注解搬进本机常驻后端（主会话 09-25 裁：join 只一个家、注解读写者换成本机后端）——
//   这里原先驱动 monitor 那一份实现的几组判据随被测函数一起退役，它们钉的性质各自在新家有判据（逐条对应）：
//   - `K-R97` 本机项目清单来自后端那一行 · 行里的「不知道」不被压平 · 本机判活答真值 · 一次列举只问一次
//     （`the_local_project_list_is_whatever_the_backend_said` 那一组〔散文墓碑〕）⇒ 后端 `tests/backend/history_join_tests.rs`
//     （`local_liveness_answers_true_and_false_and_unknown_is_its_own_bucket` · `a_local_listing_joins_the_record_tree_and_the_synthesized_history` ·
//     `one_remote_is_asked_exactly_once_with_the_old_subcommands` · `remote_projects_carry_the_annotation_counts_and_say_unknown_honestly`）；
//     「本机后端不在 ≠ 一个项目都没有」⇒ 通道的失败层级（`src/history-reads.ts` 抛、界面说「加载失败」），判据 `tests/history-reads.vitest.ts`；
//   - `K-R92` 线上那几格分得开「不知道」与「真的是 0」· 「不知道」自成一档排序（`the_three_counts_can_say_i_do_not_know` 那两条〔散文墓碑〕）
//     ⇒ 后端 `history_join_tests.rs`（`unreadable_annotations_are_unknown_not_zero` · `projects_sort_unknown_between_known_true_and_known_false`）；
//   - 两个线上形状的驼峰契约（`history_project_camel_case_contract` 那两条〔散文墓碑〕）⇒ 跨语言金样 `tests/__fixtures__/history-products.golden.json`
//     （后端产 · TS 解码器逐键收）；
//   - Codex 分组 · 首条真用户话去注入（`codex_projects_group_by_cwd` 那两条〔散文墓碑〕）⇒ 后端 `tests/backend/agents/codex/history_tests.rs`
//     ＋ `history_join_tests.rs::synthesized_history_groups_by_cwd_under_the_kind_prefix`；
//   - 上次账号的 serde 与 patch 三态 · 只含真有的那几条（`last_account_serde_and_patch_semantics` 那两条〔散文墓碑〕）⇒ 后端
//     `tests/backend/history_annotations_tests.rs`（`patch_semantics_match_what_the_monitor_did` · `last_accounts_are_only_the_entries_that_have_one`）；
//   - 摘录按字符截断（`truncate_chars_unicode` 那三条〔散文墓碑〕）⇒ `search-core` 的 `truncate_excerpt`（后端会话行改用它）；
//   - 「迁移前」旧读者读注解夹具 == 金样（`c4d_the_old_reader_reads_the_annotation_fixture_as_the_golden`〔散文墓碑〕，子步 4 那一拍对过）
//     ⇒ 金样 `tests/__fixtures__/history-metadata.readout.golden.json` 留作「迁移前」的冻结读数，后端新读者照旧对它。

/// 〔RW1〕替身门（临时目录，不碰真实 home）。
fn uf_door(tag: &str) -> (PathBuf, crate::user_files::tests::DiskDoor) {
    let home = crate::user_files::tests::temp_home(tag);
    let door = crate::user_files::tests::DiskDoor::new(&home);
    (home, door)
}

#[test]
fn deleting_a_session_hands_the_backend_only_the_sid() {
    let (home, door) = uf_door("del-sid");
    let sid = format!("rw1-del-{}", std::process::id());
    futures::executor::block_on(delete_via_backend(
        &door,
        &sid,
        &format!("/any/projects/-p/{sid}.jsonl"),
    ))
    .expect("sid 与文件名对得上 ⇒ 交给后端");
    assert_eq!(door.deleted_sids.borrow().as_slice(), &[sid.clone()]);
    // Windows 路径分隔符也认得出 stem。
    futures::executor::block_on(delete_via_backend(
        &door,
        &sid,
        &format!("C:\\u\\.claude\\projects\\-p\\{sid}.jsonl"),
    ))
    .expect("反斜杠路径");
    std::fs::remove_dir_all(&home).ok();
}

#[test]
fn a_sid_that_does_not_match_the_file_name_deletes_nothing() {
    // 🔴 从前远端那一支「不信前端的 sid、自己从路径算」防的是「删 A 的文件、清 B 的注解」。
    //    今天后端只收 sid ⇒ 这一闸在两侧同时防：对不上就一个请求都不发。
    let (home, door) = uf_door("del-mismatch");
    for path in ["/p/projects/-x/other.jsonl", "/p/projects/-x/s1.txt", ""] {
        let e = futures::executor::block_on(delete_via_backend(&door, "s1", path))
            .expect_err("对不上该拒");
        assert!(e.contains("对不上"), "{path}：{e}");
    }
    assert!(
        door.deleted_sids.borrow().is_empty(),
        "对不上还是交给后端删了：{:?}",
        door.deleted_sids.borrow()
    );
    std::fs::remove_dir_all(&home).ok();
}

/// 独立临时 projects 目录（惯例同 utils.rs / watcher.rs 测试）。
fn temp_projects(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir()
        .join(format!("ccm-hist-del-{}-{}", tag, std::process::id()))
        .join("projects");
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

// === F62：create_branch_session 守卫 + 原生分支格式 ===

#[test]
fn branch_result_camel_case_contract() {
    let r = BranchResult {
        session_id: "new-sid".into(),
        jsonl_path: "/p/new-sid.jsonl".into(),
    };
    let j = serde_json::to_string(&r).unwrap();
    assert!(j.contains("\"sessionId\""), "缺 sessionId: {j}");
    assert!(j.contains("\"jsonlPath\""), "缺 jsonlPath: {j}");
}

// 〔RW1 · 第四波 · 2026-09-24〕这里原来是本机分叉那份实现的四条 IO 判据（`O_EXCL` 不覆盖 · 软链逃逸按 sid 找不到 ·
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
    const CALL: &str = "branch_core::find_session_file(";

    // ① 唯一那份：声明只有一处，且住在共享 crate 里。
    let core = guard_core::production_code(include_str!(
        "../../src/bridge/crates/branch-core/src/lib.rs"
    ));
    let decls = core.matches("pub fn find_session_file").count();
    assert_eq!(
        decls, 1,
        "共享 crate 里 `find_session_file` 的声明有 {decls} 处（该是 1）。\n\
             0 ⇒ 它被搬走/删了，下面两条会零命中地绿；2 ⇒ 唯一那份自己裂了。"
    );

    // ② 〔RW1 · 第四波 09-24〕**monitor 那一侧零处、后端那一侧恰好一处**。
    //    从前两侧各有一处（本机分叉在 monitor 进程里找、写）；本机分叉改成 exec 本机后端的
    //    `--fork-session` 之后，「按 sid 找那份」只剩后端那一处在问 —— monitor 这一侧再出现一处，
    //    就是有人又在本进程里做分叉了（那正是用户裁掉的那一形：monitor 不直接写用户文件）。
    let mine = guard_core::production_code(include_str!("../../src/bridge/src/history.rs"));
    let mine_remote =
        guard_core::production_code(include_str!("../../src/bridge/src/remote_branch.rs"));
    let theirs = r88_backend_production("src/backend/control/fork_write.rs");
    for (who, src, want) in [
        ("monitor `history.rs`", &mine, 0usize),
        ("monitor `remote_branch.rs`", &mine_remote, 0),
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
    // 〔LOC1a〕两侧的分叉交给那台后端的**同一条帧命令**、只有一处发送点（本机 `<local>` 与远端同一个 `fork_on`）。
    assert_eq!(
        mine_remote.matches("\"session-fork\"").count(),
        1,
        "本机与远端那两支都该经同一处发 `session-fork`（`remote_branch.rs::fork_on`）"
    );
    assert!(
        !mine_remote.contains("connect_and_exec_capture(") && !mine_remote.contains("run_query("),
        "monitor 的分叉又自己 exec 了（拨号 capture / 一次性本机后端两条路 LOC1a 都删了）"
    );

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
    let local_path = format!(
        "{}\n{}",
        // 〔步 12·C 09-20〕`pub fn` → `pub async fn`：合并之后这条命令要 `.await`
        // 远端那一支。**只是签名字面量跟上，人群一个字没动** —— 切出来的仍是同一个函数。
        r88_fn_body(&mine, "pub async fn create_branch_session("),
        // 〔RW1〕本机那一支搬进了 `remote_branch.rs`（exec 本机后端），人群跟着搬。
        r88_fn_body(
            &mine_remote,
            "pub(crate) async fn create_local_branch_session("
        )
    );
    // 反向自检：尺子够得着 —— 把针塞进一份副本，量具必须数得出来。
    let poisoned = format!("{local_path}\n  let _ = std::fs::read{}dir(root);\n", "_");
    assert_eq!(
        scan(&poisoned).len(),
        1,
        "阳性对照没过 —— 量具此刻无效，下面那条断言是空真"
    );
    for (who, src) in [
        ("monitor 的分叉那条路", local_path.as_str()),
        ("后端 `fork_write.rs`", theirs.as_str()),
    ] {
        let hits = scan(src);
        assert!(
            hits.is_empty(),
            "{who}上又长出了目录枚举：\n{}\n\n\
                 ⇒ 「按 sid 找那份会话文件」的家在 `branch_core::find_session_file`。\n\
                 真有第二种找法要立，先回答「为什么这一侧不能问那一份」，再连本条一起改。",
            hits.join("\n")
        );
    }
}

/// 从生产段里切出一个函数（含它的签名与函数体）—— 供上面那条按函数切人群。
///
/// 收尾认的是**列 0 的右大括号**（`rustfmt` 保证顶层 item 这么收）。
/// 自检两条：切得到 · 切出来的东西有分量（塌成半截时下面的断言会空真）。
fn r88_fn_body<'a>(src: &'a str, sig: &str) -> &'a str {
    let at = src
        .find(sig)
        .unwrap_or_else(|| panic!("切不到 `{sig}` —— 先修尺子，别改断言"));
    let rest = &src[at..];
    let end = rest.find("\n}\n").map(|i| i + 2).unwrap_or(rest.len());
    let body = &rest[..end];
    assert!(
        body.len() > 120,
        "`{sig}` 只切出 {} 字节 —— 切法坏了",
        body.len()
    );
    body
}

// 〔RW1 · 第四波 · 2026-09-24〕这里原来是 `KR88D2` 的 monitor 那一侧（查不到的 sid ⇒ 报错，不静默挑第一个）。
// monitor 进程里不再有分叉的实现，那条性质只住后端（`fork_write·rs::an_unknown_session_id_is_refused_not_silently_substituted`）。

/// ★ `KR88D2`：**两侧的入参形状一致 —— 都收 sid，都不收路径。**
///
/// 🔴 **〔步 12·C 09-20〕本条的人群从「两条命令」变成「一条命令 ＋ 它的远端那一支」。**
///
/// 判的性质**一个字没放松**，改的只是人群的住址：`create_remote_branch_session`
/// 不再是 `#[tauri::command]`，它是合并后那条命令的远端分支。
/// ⚠ **为什么不干脆只判那一条命令**：那会让本条的牙掉一半 ——
/// `K-R88` 收的是「**同一件事两个入参形状**」，而「两个形状」今天仍然存在
/// （一个在 Tauri 命令上、一个在它调的那个函数上）。少判一侧，
/// 远端那一支哪天退回收路径，本条一声不响。
///
/// ⚠ **入参类型两侧今天不同**（命令那条收 `String`、内部那支收 `&str`）——
/// 那是所有权，不是形状。所以判的是**参数名**（`source_session_id`），不是类型。
#[test]
fn both_branch_commands_take_a_session_id_not_a_path() {
    let local = guard_core::production_code(include_str!("../../src/bridge/src/history.rs"));
    let remote = guard_core::production_code(include_str!("../../src/bridge/src/remote_branch.rs"));
    for (who, src, sig) in [
        (
            "本机（合并后那条命令）",
            &local,
            "pub async fn create_branch_session(",
        ),
        (
            "远端（那条命令的远端分支）",
            &remote,
            "pub(crate) async fn create_remote_branch_session(",
        ),
    ] {
        // 🔴 **收尾括号必须从签名**之后**找起。**〔步 12·C 09-20 实打踩到〕
        //    原来是 `src[at..].find(')')` —— 而 `pub(crate) async fn …(` 这个签名
        //    **自己就含一个 `)`**（`pub(crate)` 那个），于是切出来的区间起点大于终点，
        //    当场 panic 在一条与本条要判的东西毫无关系的地方。
        let at = src
            .find(sig)
            .unwrap_or_else(|| panic!("{who}的签名找不到（`{sig}`）—— 先修尺子"));
        let after = at + sig.len();
        let close = src[after..].find(')').expect("签名没有收尾括号");
        let params = &src[after..after + close];
        assert!(
            params.contains("source_session_id:"),
            "{who}的入参里没有 sid：{params:?}"
        );
        assert!(
            !params.contains("path"),
            "{who}又收路径了：{params:?}\n\
                 ⇒ `K-R88` 收的就是「同一件事两个入参形状」，\n\
                 而多一个可被构造的路径入参就多一条路径穿越面。"
        );
    }
    // 🔴 **〔步 12·C〕本条新增的那一半：`origin` 只许住在命令那一侧。**
    //    合并之后「哪台机器」是命令的参数；远端那一支拿到的是**已经分过本机**的机器名。
    //    要是有人把 `Origin` 往内部那支里塞，本机那条路就会第二次去分本机 ——
    //    而两处分本机正是 `local_origin_registry` 整篇在治的那一形。
    let remote_sig_at = remote
        .find("pub(crate) async fn create_remote_branch_session(")
        .expect("切不到远端那一支");
    let remote_after = remote_sig_at + "pub(crate) async fn create_remote_branch_session(".len();
    let remote_close = remote[remote_after..]
        .find(')')
        .expect("远端那一支的签名没有收尾括号");
    let remote_params = &remote[remote_after..remote_after + remote_close];
    assert!(
        !remote_params.contains("Origin"),
        "远端那一支收了 `Origin` —— 它拿到的应当是**已经分过本机**的机器名（`host: &str`）。\n\
             收 `Origin` 就意味着它要自己再分一次本机，而那正是「同一个判断有两个住址」。"
    );
}

// P3 归并：iso_parse_* 测试已搬到 utils::tests（函数本身搬到 utils）。

/// ★★ 〔C4c · US1〕**「configDir → 表里的 id」与「哪几个号在表里有行」两条规则只有一个家：`acct-core`**。
///
/// 要求住址：`设计/01 §5` D1「**一个判定只有一个家**」。〔US1〕monitor 那一个绑定（`history::apikey_routed_subset`〔散文墓碑〕）
/// 随界面那一问搬进后端一起删了 ⇒ 两棵生产树里两条规则的**定义**都零处；反空真：同一个识别器在 `acct-core` 上各数得出恰好 1。
#[test]
fn the_two_apikey_rules_are_defined_only_in_acct_core() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let defs = ["fn apikey_account_id_of_dir", "fn apikey_routed_subset"];
    let mut found = Vec::new();
    for dir in [root.join("src"), root.join("../backend")] {
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
        &std::fs::read_to_string(root.join("crates/acct-core/src/lib.rs")).unwrap(),
    );
    for d in defs {
        guard_core::find_pinned(&core_prod, d)
            .unwrap_or_else(|e| panic!("acct-core 里 `{d}` 不是恰好一处：{e}"));
    }
}

const LOC1B_CLAUDE_PAGE: &str = concat!(
    "{\"type\":\"permission-mode\",\"permissionMode\":\"default\"}\n",
    "\n",
    "{\"type\":\"user\",\"uuid\":\"u1\",\"timestamp\":\"2026-01-01T00:00:00Z\",\"cwd\":\"/w\",\"message\":{\"role\":\"user\",\"content\":\"x\"}}\n",
    "{\"type\":\"assistant\",\"uuid\":\"a1\",\"timestamp\":\"2026-01-01T00:00:01Z\",\"message\":{\"role\":\"assistant\",\"content\":[{\"type\":\"text\",\"text\":\"y\"}]}}\n",
);

/// 同一页喂本机与远端两个 pager：记录、seq、cwd、sid 逐格相同，差别**只在**载荷上的 `origin`（本机省略、远端是机器名）。
#[test]
fn loc1b_one_pager_serves_both_sides_and_only_the_payload_origin_differs() {
    let path = "/h/.claude/projects/p/0b8f7a4e-0000-4000-8000-000000000001.jsonl";
    let mut local = SessionPager::new(&crate::origin::Origin::local(), path);
    let mut remote = SessionPager::new(&crate::origin::Origin("aya".into()), path);
    let (l, r) = (
        local.page(LOC1B_CLAUDE_PAGE),
        remote.page(LOC1B_CLAUDE_PAGE),
    );
    // permission-mode 不可显示但占号（seq 0），空行不占号 ⇒ user=1、assistant=2。
    assert_eq!(l.iter().map(|p| p.seq).collect::<Vec<_>>(), vec![1, 2]);
    assert_eq!(r.iter().map(|p| p.seq).collect::<Vec<_>>(), vec![1, 2]);
    for (a, b) in l.iter().zip(&r) {
        assert_eq!(a.session_id, "0b8f7a4e-0000-4000-8000-000000000001");
        assert_eq!(
            (&a.session_id, &a.cwd, &a.path),
            (&b.session_id, &b.cwd, &b.path)
        );
        assert_eq!(a.cwd.as_deref(), Some("/w"), "cwd 从首条 user 记录起往后带");
        assert_eq!(
            serde_json::to_value(&a.message).unwrap(),
            serde_json::to_value(&b.message).unwrap()
        );
    }
    assert!(
        l.iter().all(|p| p.origin.is_none()),
        "本机载荷不带 origin（前端视为本机）"
    );
    assert!(r.iter().all(|p| p.origin.as_deref() == Some("aya")));
}

/// 页边界不重置行号：第二页接着第一页的号往下数（分页是 transport 的事，seq 口径不许跟着变）。
#[test]
fn loc1b_numbering_continues_across_pages() {
    let path = "/h/.claude/projects/p/s.jsonl";
    let mut whole = SessionPager::new(&crate::origin::Origin::local(), path);
    let one: Vec<u64> = whole
        .page(LOC1B_CLAUDE_PAGE)
        .iter()
        .map(|p| p.seq)
        .collect();
    let (head, tail) =
        LOC1B_CLAUDE_PAGE.split_at(LOC1B_CLAUDE_PAGE.find("{\"type\":\"assistant\"").unwrap());
    let mut paged = SessionPager::new(&crate::origin::Origin::local(), path);
    let mut two: Vec<u64> = paged.page(head).iter().map(|p| p.seq).collect();
    two.extend(paged.page(tail).iter().map(|p| p.seq));
    assert_eq!(one, two);
}

/// 种类按文件名形态判：远端的 Codex rollout 路径（本机没有那一家的根）也走 Codex 解析、sid 取末尾 UUID；
/// 同一行放进 Claude 名字的文件 ⇒ 按 Claude 解（抢救成 `Unrecognized`），两形确实分叉（正反控）。
#[test]
fn loc1b_the_agent_kind_comes_from_the_file_name_on_either_side() {
    let uuid = "0b8f7a4e-0000-4000-8000-000000000002";
    let codex_path =
        format!("/far/away/.codex/sessions/2026/01/01/rollout-2026-01-01T00-00-00-{uuid}.jsonl");
    let line = "{\"timestamp\":\"2026-01-01T00:00:00Z\",\"type\":\"response_item\",\"payload\":{\"type\":\"message\",\"role\":\"user\",\"content\":[{\"type\":\"input_text\",\"text\":\"x\"}]}}\n";
    let mut codex = SessionPager::new(&crate::origin::Origin("aya".into()), &codex_path);
    let got = codex.page(line);
    assert_eq!(got.len(), 1);
    assert_eq!(got[0].session_id, uuid);
    assert!(
        matches!(got[0].message, crate::messages::JsonlRecord::User { .. }),
        "Codex 的 user 消息要映射成 User 记录：{:?}",
        got[0].message
    );
    let mut claude = SessionPager::new(
        &crate::origin::Origin("aya".into()),
        "/far/away/.claude/projects/p/s.jsonl",
    );
    let as_claude = claude.page(line);
    assert!(
        !as_claude
            .iter()
            .any(|p| matches!(p.message, crate::messages::JsonlRecord::User { .. })),
        "同一行按 Claude 解不该成 User —— 否则上一条没证明种类真的按名字分了"
    );
}

/// 读一整份会话的那条 Tauri 命令**只有一条路**：经那台后端的 `history-read` 分页，体里零处按本机分叉、零处自己开文件。
/// 正控：针在一段写着本机分支的合成串里认得出来（否则零命中是空真）。
#[test]
fn loc1b_the_cold_read_command_has_no_local_branch_and_opens_no_file() {
    let prod = guard_core::production_code(include_str!("../../src/bridge/src/history.rs"));
    let start = prod
        .find("pub async fn stream_read_session_jsonl(")
        .expect("找不到那条命令 —— 本条空转");
    let end = prod[start..]
        .find("\n}\n")
        .map(|k| start + k)
        .expect("找不到函数尾");
    let body = &prod[start..end];
    let needles = [
        "Route::Local",
        "Route::Remote",
        "is_local()",
        "File::open",
        "remote_history::",
    ];
    let hits = |s: &str| needles.iter().filter(|n| s.contains(*n)).count();
    assert_eq!(
        hits(body),
        0,
        "冷读命令体里又长出了按本机 / 远端分叉或自己开文件的那一形：\n{body}"
    );
    assert_eq!(
        body.matches("frame_query::read_page(").count(),
        1,
        "取原文只经那台后端这一处"
    );
    // 正控：旧形状（本机分支自己开文件）会被认出来。
    let old = "if let crate::origin::Route::Remote(host) = origin.route(\"x\")? { return crate::remote_history::f(host).await; } let file = File::open(&target);";
    assert_eq!(hits(old), 3);
}
