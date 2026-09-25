// `include_str!` 只接**字面量 token**，喂 `const` 会报 `argument must be a string literal`
// ⇒ 用单臂宏拿到「单一落点」。原住 `src/bridge/src/backend/control/tmux.rs`，步 7b 随它唯一的消费者搬来这里；
// 路径也跟着换成相对本文件（`16 §5.4a` 规则 1：路径不只住在字面量里，也住在宏展开里）。
macro_rules! backend_watcher_src {
    () => {
        "../../../../src/backend/observe/watcher.rs"
    };
}

/// ★ P8c（`U3` 08-11 裁定）：**三种 Skip 的原因必须彼此可分**。
///
/// `U3` 的读数逐字记着不可分的后果：「`Unobservable` 计数 = 0，而**那个 0 是瞎的**
/// —— 日志根本不记这一维 ⇒ 分母不存在。『0 次』与『记不下来』长得一模一样」。
/// ⇒ 压成一个无载荷的 `Skip` 时，`#82` 想问的那个频率**问不出来**。
#[test]
fn the_three_skip_reasons_are_distinguishable() {
    use std::collections::HashSet;
    let reasons: HashSet<&str> = [
        // 远端没装 tmux —— 哨兵与显式字段两条路都该给同一个原因。
        classify_tmux_observation("NO_TMUX", None),
        classify_tmux_observation("", Some(OBS_NO_TMUX)),
        // backend 自报观测失败 —— `#82` 要的就是这一格的频率。
        classify_tmux_observation("", Some(OBS_UNOBSERVABLE)),
        // 旧后端的空串歧义。
        classify_tmux_observation("", None),
    ]
    .iter()
    .map(|v| match v {
        TmuxObservation::Skip(r) => r.as_str(),
        TmuxObservation::Backend(_) => panic!("这四种输入都该跳过"),
    })
    .collect();
    assert_eq!(
        reasons.len(),
        3,
        "三种原因必须彼此可分，实得 {reasons:?} —— 压成一个就等于这一维不可测"
    );
    // 标识必须是**机器可读**的稳定串（进日志后要能 grep/统计），不是给人读的句子。
    for r in &reasons {
        assert!(
            r.chars().all(|c| c.is_ascii_lowercase() || c == '_'),
            "原因标识 {r:?} 不是机器可读的稳定串"
        );
    }
}

use super::*;

// ---------- P1（zero-poll-liveness）：观测分类 ----------

fn sids(o: &TmuxObservation) -> Vec<String> {
    match o {
        TmuxObservation::Backend(s) => {
            let mut v: Vec<String> = s.iter().cloned().collect();
            v.sort();
            v
        }
        TmuxObservation::Skip(r) => panic!("期望 Backend，实得 Skip({})", r.as_str()),
    }
}

/// ★ P1 的回归测试（**这条在修之前是红的**）：backend 确证零会话 ⇒ 必须是**有效观测（空集）**，
/// 不是跳过。这就是 `src/doc/INVARIANTS.md` §24bis 那条残留 bug 的机理：
/// 杀掉某 origin 仅剩的 tmux 会话 → server 随之退出 → `tmux ls` 回空 →
/// 旧代码保守跳过 → idle 灰灯卡到断连 flush 才清。
#[test]
fn zero_sessions_is_a_valid_observation_not_a_skip() {
    assert_eq!(
        classify_tmux_observation("", Some("zero_sessions")),
        TmuxObservation::Backend(std::collections::HashSet::new()),
        "backend 确证零会话时必须进对账（空集），否则灰灯永不清"
    );
}

/// 旧后端（无 `observation` 字段）+ 空 raw ⇒ **保持今天的保守行为**。
/// 空 raw 在旧后端那里同时意味着「零会话」和「`tmux ls` 出错被 `|| true` 吞了」，
/// 分不开 ⇒ 只能跳过。**新旧混搭不许回归。**
#[test]
fn old_backend_empty_raw_still_skips() {
    assert_eq!(
        classify_tmux_observation("", None),
        // ★ P8c：连**原因**一起钉 —— 原来只钉「跳了」，而三种完全不同的原因
        // 压成同一个无载荷的 `Skip` 正是 `#82` 问不出频率的来源。
        TmuxObservation::Skip(SkipReason::LegacyAmbiguousEmpty),
        "旧后端的空串语义不可分，必须保守跳过"
    );
}

/// 远端没装 tmux ⇒ 跳过（哨兵与显式分类**两条路都要认**）。
#[test]
fn no_tmux_skips_both_via_sentinel_and_field() {
    assert_eq!(
        classify_tmux_observation("NO_TMUX", None),
        TmuxObservation::Skip(SkipReason::NoTmux)
    );
    assert_eq!(
        classify_tmux_observation("NO_TMUX\n", Some("no_tmux")),
        TmuxObservation::Skip(SkipReason::NoTmux)
    );
}

/// 观测无效（`tmux ls` 非 0/1 退出、exec 失败）⇒ 跳过，**绝不当成零会话**。
#[test]
fn unobservable_skips() {
    assert_eq!(
        classify_tmux_observation("", Some("unobservable")),
        TmuxObservation::Skip(SkipReason::Unobservable),
        "观测失败当成零会话会批量误灰"
    );
}

/// 有会话时照常解析出 sid 集（`@ccm_sid` 为空的会话不进集合，见 `parse_tmux_ls`）。
#[test]
fn sessions_parse_into_sid_set() {
    let raw = "s1\t/p\tclaude\t1\t2\tsid-a\ns2\t/q\tnode\t0\t1\t\ns3\t/r\tbash\t0\t1\tsid-c";
    let o = classify_tmux_observation(raw, None);
    assert_eq!(sids(&o), vec!["sid-a".to_string(), "sid-c".to_string()]);
}

/// 向前兼容：未来后端加了本 monitor 不认识的分类 ⇒ **落回 raw 判据**，
/// 退化成今天的保守行为，不误灰。
#[test]
fn unknown_observation_falls_back_to_raw() {
    assert_eq!(
        classify_tmux_observation("", Some("some_future_kind")),
        TmuxObservation::Skip(SkipReason::LegacyAmbiguousEmpty)
    );
    let o = classify_tmux_observation("s1\t/p\tclaude\t1\t1\tsid-a", Some("some_future_kind"));
    assert_eq!(sids(&o), vec!["sid-a".to_string()]);
}

/// **P1 刻意保留的一处不对称**：`observation` 说有会话、但 raw 里一个 `@ccm_sid` 都没有
/// （老会话 / 未装 wrapper）⇒ 仍然跳过。因为对账的判据是 sid 集，没有 sid 就无从判断，
/// 而"有 tmux 会话但都没绑 sid"**不等于**"零会话"。
#[test]
fn sessions_without_any_ccm_sid_still_skips() {
    assert_eq!(
        classify_tmux_observation("s1\t/p\tbash\t0\t1\t", None),
        TmuxObservation::Skip(SkipReason::LegacyAmbiguousEmpty),
        "有会话但无 @ccm_sid ≠ 零会话，不许当空集喂进对账"
    );
}

/// P1：`observation` 取值集是 monitor↔backend 的**第三个双写点**（前两个：`TMUX_LS_FMT` ·
/// `NO_TMUX` 哨兵）。两个独立 crate 不能共享类型 ⇒ 用与
/// `tmux_ls_fmt_double_write_point_stays_in_sync` 相同的办法钉住：`include_str!` 读后端源
/// + **锚定 const 定义行**（不是裸字面量——否则该串若出现在某条注释里会掩盖真漂移）。
///   **双向**：改 monitor 或后端任一侧忘同步，本测即红。
#[test]
fn observation_tokens_double_write_point_stays_in_sync() {
    let backend_src = include_str!(backend_watcher_src!());
    for (name, value) in [
        ("OBS_ZERO_SESSIONS", OBS_ZERO_SESSIONS),
        ("OBS_NO_TMUX", OBS_NO_TMUX),
        ("OBS_UNOBSERVABLE", OBS_UNOBSERVABLE),
    ] {
        let expected_def = format!("const {name}: &str = \"{value}\";");
        assert!(
            backend_src.contains(&expected_def),
            "observation 双写点漂移：backend watcher.rs 不含 {expected_def:?}\n\
                 （改了分类取值就得两侧同步——同 TMUX_LS_FMT 的纪律）"
        );
    }
    // 反向自检：断言的是「扫到了后端源」而不是「命中若干条」——阈值不能挂在
    // 被检查的量上（rust-ts-boundary 的教训）。
    assert!(
        backend_src.len() > 1000,
        "include_str! 没读到后端源，上面三条断言全是空转"
    );
}

/// A5：send-keys 目标白名单——只认本工具的 cc-* 会话名，拒用户别的 tmux。
#[test]
fn ccm_tmux_name_whitelist() {
    assert!(is_ccm_tmux_name("cc-abc12345"));
    // ★ S4b-3b：新命名 `<X>-cc`（撞名时 `<X>-cc-<N>`）也要本地命中，
    // 否则每次 kill/send-keys 都要多跑一趟远端去核 `@ccm_sid`。
    assert!(is_ccm_tmux_name("abc12345-cc"));
    assert!(is_ccm_tmux_name("abc12345-cc-2"));
    assert!(is_ccm_tmux_name("my-proj-cc"));
    // **老前缀必须继续命中** —— 用户机器上正跑着的会话就是这个形状，
    // 不认它们等于把它们变成 issue #76 那种「失管会话」。
    assert!(is_ccm_tmux_name("cc-proj"));
    // 退化名不该命中：`-cc` 前面得有东西。
    assert!(!is_ccm_tmux_name("-cc"));
    // 名字里恰好含 `-cc` 但不是以它结尾、也不是 `-cc-<数字>` ⇒ 不认
    //（那多半是别人的会话，误认会让我们跳过远端核验就去 kill）。
    assert!(!is_ccm_tmux_name("foo-ccx"));
    assert!(!is_ccm_tmux_name("foo-cc-bar"));
    assert!(is_ccm_tmux_name("cc-abc12345-2")); // pickFreshTmuxName 的 -N 变体
    assert!(!is_ccm_tmux_name("cc-")); // 只前缀无体
    assert!(!is_ccm_tmux_name("web")); // 用户自己的会话
    assert!(!is_ccm_tmux_name("mycc-x")); // 非前缀
    assert!(!is_ccm_tmux_name("cc-a b")); // 空格（注入面）
    assert!(!is_ccm_tmux_name("cc-a;rm")); // 分号
    assert!(!is_ccm_tmux_name("cc-a$x")); // 元字符
}

/// F04 Gate 1：**只有空 target** 恒被拒——`=:` 会解析成「当前会话」，是唯一真正危险的默认值。
///
/// # ⚠ `K-R72`（09-12）：**人群没缩，只是换了住址**
///
/// 送键与杀会话那两条桌面侧回落删掉之后 `build_kill_session_cmd` /  〔散文墓碑〕
/// `build_send_keys_remote_cmd` 不在了 —— 但 Gate 1 **不是那两条回落的东西**：  〔散文墓碑〕
/// 它守的是「任何拿 target 去做事的入口，都得先把空目标拒掉」。⇒ 本条改打
/// **今天三条路各自真正的入口**，一条都没少：
/// ① 谓词本体 [`gate1_reject_empty`]（今天只剩 [`exact_target`] 这个跨轨锚点在用）；
/// ②③④〔C4e · 第四波 4C〕三条路（抓屏 · 送键 · 杀会话）的生产入口原本也在这里真跑一遍、断言在任何 IO 之前就地拒；
///    三条整条迁到界面之后（`src/tmux-control.ts`），那一格随入口搬过去：`tests/tmux-control.vitest.ts`
///    「空目标就地拒，一个字节都不发」三个入口各一条（Tauri 命令 `tmux_send_keys` / `kill_remote_tmux`〔散文墓碑〕删了）。
///
/// 含 glob/元字符但非空的 target **不**在这一层被拒（`shell_quote` 已安全引号化，
/// 字符集收紧是 TS 侧 `isValidNewTmuxName`/`isValidTmuxName` 的职责，
/// 见 `is_safe_tmux_target` 头注）。
#[test]
fn gate1_rejects_only_empty_target() {
    // ① 谓词本体（正反各一格 —— 只钉「空的被拒」的话，把它焊死成恒拒也能绿）
    assert!(
        gate1_reject_empty("").is_err(),
        "空 target 应被 Gate 1 拒绝（谓词本体）"
    );
    assert!(
        gate1_reject_empty("cc-a b").is_ok(),
        "非空 target 不该被 Gate 1 拒绝（谓词本体）"
    );
    // 非空、含元字符/glob 的 target 不被 Gate 1 拒（谓词本体那一格已在 ① 里断过；
    // 这里补一批真实形状 —— 收紧字符集是**另一层**的职责，不许在 Gate 1 顺手做）。
    for safe_nonempty in ["cc-a b", "cc-a;rm", "cc-a$x", "si*", "a'b"] {
        assert!(
            gate1_reject_empty(safe_nonempty).is_ok(),
            "非空 target {safe_nonempty:?} 不该被 Gate 1 拒绝"
        );
    }
}

/// ★★ **K-R12：跨 SSH 的 tmux 读要 `-u`，且必须在子命令之前。**
///
/// # ⚠ `K-R72`（09-12）：人群**真的缩了一半**，名字跟着改
///
/// 原名叫 `both_cross_ssh_tmux_reads_…`，那个「both」指的是
/// `build_guarded_tmux_cmd` 的取值 `display-message` ＋ `list_remote_tmux` 的 `ls`。
/// 前者随两条回落一起走了 ⇒ **monitor 侧今天只剩 `ls` 这一条跨 SSH 的 tmux 读**
/// （`capture-pane` 实测不在人群里：它吐原始 UTF-8 字节，见 [`UTF8_CLIENT_FLAG`] 头注）。
/// ⇒ 留「both」在名字里就是一句假话；性质本身**一个字没变**，只是分母从 2 变 1。
///
/// 位置这一维必须单独钉：`-u` 放到子命令**后面**实测是
/// `rc=1 + unknown flag -u`，而这条串把 stderr 与 rc 都丢了 ⇒ 静默退化。
///
/// ⚠ **本条扫的是源码**（那条串拼在 `async fn` 里、外面取不到），如实标注：
/// **盘上有 ≠ 被走到**。行为那一半的死值在 `tests/evidence/K-R12-deathvalue.md` ②/S5
/// （真 tmux 3.4，改前段数 1 / 改后段数 6）。
#[test]
fn the_surviving_cross_ssh_tmux_read_asks_for_a_utf8_client_before_the_subcommand() {
    let prod = guard_core::production_code(include_str!(
        "../../../../src/bridge/src/backend/control/tmux.rs"
    ));
    guard_core::assert_no_test_code("tmux.rs", &prod);
    // 抽取器自检：生产段塌了下面两条就零命中地绿。
    assert!(
        prod.len() > 5_000,
        "生产段只剩 {} 字节 —— 剥法坏了，本条此刻量不到东西",
        prod.len()
    );
    assert!(
        prod.contains("tmux {UTF8_CLIENT_FLAG} ls -F"),
        "`list_remote_tmux` 的命令串没有把 `-u` 放在 `ls` 之前"
    );
    assert!(
        !prod.contains("ls {UTF8_CLIENT_FLAG}") && !prod.contains("ls -u"),
        "`-u` 被放到了 `ls` 后面（实测 rc=1 + unknown flag）"
    );
    // ★ 反向自检：这把尺子分得清「放对了」与「放错了」——
    //   没有它，上面那两句在一份**根本没有 tmux 命令**的语料上也会「绿」。
    let bad = "tmux ls {UTF8_CLIENT_FLAG} -F '{TMUX_LS_FMT}'";
    assert!(
        !bad.contains("tmux {UTF8_CLIENT_FLAG} ls -F") && bad.contains("ls {UTF8_CLIENT_FLAG}"),
        "本条的两个针分不出「`-u` 在子命令前」与「在子命令后」—— 它此刻什么都没在守"
    );
}

/// backend 侧那个「一个口径一个家」的家（相对**仓根**）—— 跨仓对拍的被读对象。
///
/// 单一落点：路径写死在这里一处，backend 再搬家只改这一行。
const BACKEND_KOU_JING_HOME: &str = "src/backend/common/tmux_utf8.rs";

/// ★★ **K-R12 下一拍（09-04）：「同一个口径只有一个家 + 另一侧引用它或有对拍」——
/// 本 const 走的是**对拍**那一支。**
///
/// # 这条钉的是**关系**，不是词表
///
/// 三件事一起断言，缺一件就只买到一角：
///
/// | # | 断的什么 | 缺了它会怎样 |
/// |---|---|---|
/// | ① | 本侧**只有一个**声明（本文件那一处，全 monitor 树无第二处） | 本侧自己先分了两份，对拍再准也没用 |
/// | ② | 本侧那个值与后端家里那一行**逐字相等**（值是从本侧 const **现取**的） | 两侧漂开而两边都不红 —— 正是本件治的那个形状 |
/// | ③ | backend 家里**两种表示都还在** | 有人把家「收口」成一种表示 ⇒ 另一类调用点静默失效 |
///
/// ②③ 都是**读两棵树**才验得了的性质：两个 crate 不共享源码树，共用 `const` 拿不到
/// （七个 `*-core` 的职责逐条都装不下，论据在 `UTF8_CLIENT_FLAG` 的头注里）。
///
/// # ⚠ 作用域，逐条说清（`brief` 12：报一个数就要说清尺子）
///
/// - **在哪跑**：monitor 那格 cargo（`cargo test --workspace --lib`）。
///   backend 自己那格看不见它 —— 但门禁两格都跑，所以任一侧漂开都会在门禁里红。
/// - **读了哪两棵树**：本侧 `include_str!("../../../../src/bridge/src/backend/control/tmux.rs")`（编译期，同一半）+
///   backend 侧 [`BACKEND_KOU_JING_HOME`]（**运行期** `read_to_string`）。
/// - 🔴 **为什么后端那一半刻意用运行期读、而不是 `include_str!`**：
///   `include_str!` 会新长出一条**跨半边的编译期边**，而那种边由
///   `cross_half_edge_registry::CROSS_EDGES` 逐条登记着（多一条就红），
///   **那个文件不在本拍写区**。运行期读在本仓是**既有做法**、不是绕道：
///   `cross_half_edge_registry` 自己就是运行期遍历后端那棵树的
///   （`both_halves()` 扫 `src/backend`），`scanning_guard_registry::PENDING`
///   里也直接列着后端的文件。而且它在该登记表关心的那一维上**更轻**：
///   backend 换布局时这里是一句说得清的运行期失败，不是 `cargo test` 编不过。
///   ⚠ 代价如实写下：这条边因此**不出现在** `CROSS_EDGES` 里。
///   PM 若要它以编译期形态登记，改法是**两处一起动、不许只动一处**：
///   ① 把下面那句运行期读换成编译期读（`include_str!` 配 `concat!` / `env!` 拼路径，
///      形状照本文件已有的 `backend_watcher_src` 那个单一落点宏）；
///   ② 同轮在 `CROSS_EDGES` 里加一条 `monitor→backend` 的登记
///      （读者 `src/bridge/src/backend/control/tmux.rs` · 被读 `src/backend/common/tmux_utf8.rs` ·
///      理由「跨轨对拍：口径的家在对面，本侧那一份必须与它逐字相等」）。
///   🔴 只动 ① 会让那张表的条数当场对不上 —— 它是**两个方向都查**的。
/// - **不管什么**：它不证明「那个旗真的被走到了」（「盘上有 ≠ 被走到」）。
///   行为那一半的死值在 `tests/evidence/K-R12-deathvalue.md`（真 tmux 3.4 私有 socket）。
#[test]
fn utf8_client_kou_jing_has_one_home_and_this_side_matches_it() {
    let prod = guard_core::production_code(include_str!(
        "../../../../src/bridge/src/backend/control/tmux.rs"
    ));
    guard_core::assert_no_test_code("tmux.rs", &prod);

    // ── ① 本侧只有一个声明 ────────────────────────────────────────────
    let decl_prefix = format!("{}: &str =", "UTF8_CLIENT_FLAG");
    guard_core::find_pinned(&prod, &decl_prefix)
        .unwrap_or_else(|e| panic!("本文件生产段里 `{decl_prefix}` 不是恰好一处：{e}"));
    let root = crate::guard_support::repo_root();
    // 🔴 〔搬树 2026-09-18 · `设计/99` 条 73〕**排掉的是谁、为什么 —— 明写。**
    //
    // 排掉 `src/bridge/src/backend/control/tmux.rs`：它就是「那一个家」，上面第 ① 段已经用 `find_pinned`
    // 单独钉过它「恰好一处」。这里数的是**第二个家**，本来就不该把它自己算进去。
    //
    // 上一版靠 `scan_tree!` 的 `file!()` 自摘 —— 当年判据住在 `tmux.rs` 自己的
    // `#[cfg(test)]` 段里，「摘掉调用者」恰好等于「摘掉被测那份」。剖分之后 `file!()`
    // 指向本测试文件，那一刀**整个落空**，`tmux.rs` 回到人群里 ⇒ 报「monitor 侧有第二个」，
    // 而盘上真的只有一个。⇒ 换成明写的排除（摘不到它，`scan_tree_excluding` 当场红）。
    let others = guard_core::scan_tree_excluding(
        &root.join("src/bridge/src"),
        &["rs"],
        &["src/bridge/src/backend/control/tmux.rs"],
    );
    assert!(
        others.len() >= 60,
        "monitor 树只采到 {} 个 .rs —— 遍历坏了，「无第二处」此刻是空转的",
        others.len()
    );
    let dup: Vec<String> = others
        .iter()
        .filter(|(_, raw)| guard_core::production_code(raw).contains(&decl_prefix))
        .map(|(p, _)| p.to_string_lossy().into_owned())
        .collect();
    assert!(
        dup.is_empty(),
        "monitor 侧有**第二个** `{decl_prefix}`：{dup:?}\n\
             ⇒ 从此两份靠人对齐。正解是引用本文件那一处。"
    );

    // ── ② 与后端那个家逐字相等（值现取，不写死） ──────────────────
    let home_path = root.join(BACKEND_KOU_JING_HOME);
    let home = std::fs::read_to_string(&home_path).unwrap_or_else(|e| {
        panic!(
            "读不到后端侧那个家 {home_path:?}：{e}\n\
                 它是 K-R12 下一拍建的「一个口径一个家」。文件被搬了 ⇒ 改本文件那个常量；\
                 家被删了 ⇒ 那个口径退回三份靠人对齐，先回件文件。"
        )
    });
    assert!(
        home.len() > 2_000,
        "backend 那个家只有 {} 字节 —— 没读到内容，下面的对拍是空转的",
        home.len()
    );
    let want_flag = format!("{}: &str = {UTF8_CLIENT_FLAG:?};", "UTF8_CLIENT_FLAG");
    assert!(
        home.contains(&want_flag),
        "跨仓漂移：本侧的旗是 {UTF8_CLIENT_FLAG:?}，而后端那个家里找不到 `{want_flag}`。\n\
             两侧漂开时**两边都不会因为别的判据变红** —— 那正是本件立件时的那个形状。"
    );
    // 段数下溢那个谓词是同一族的第二个口径：比的是**函数体**，不是名字。
    let body = format!("{}.count() < expected", ".split('\\t')");
    guard_core::find_pinned(&prod, &body)
        .unwrap_or_else(|e| panic!("本侧那个下溢谓词的体不是恰好一处：{e}"));
    assert!(
        home.contains(&body),
        "跨仓漂移：下溢谓词的体两侧不一致（本侧是 `{body}`，backend 家里找不到）。\n\
             口径一致本身就是要买的东西：一侧改成 `!=` 就会开始误伤合法内容。"
    );

    // ── ③ backend 家里两种表示都还在 ───────────────────────────────────
    // ⚠ 锚点只钉「那里有一个声明」（`const <名>:`），**不钉类型写法** ——
    //   带类型标注的锚点实测会被 `(&'static str, &'static str)` 这种合法写法误伤，
    //   而它印出来的话是「家里少了 env 形」：一句指向完全错误方向的诊断。
    //   ⚠ 同时**刻意收在 `:` 上**：收在标识符上时 `const <名>X:` 会被裸 `contains`
    //   当成命中（「匹配单位比事实小」那一族），于是「家改名了」这一形看不见。
    // 表里存**标识符**，锚点现拼 —— 反向自检那份「改了名」的夹具必须从标识符派生，
    // 从锚点文本派生的夹具会跟着锚点一起变松，于是「锚点变松了」这件事自己看不见
    // （backend 那侧的同职判据实测栽过这一形，头注里逐字记着）。
    let anchor = |ident: &str| format!("const {ident}:");
    for (label, ident) in [
        ("argv 形（旗）", "UTF8_CLIENT_FLAG"),
        ("env 形", "UTF8_CLIENT_ENV"),
    ] {
        let needle = anchor(ident);
        assert!(
            home.contains(&needle),
            "backend 那个家里少了**{label}**（找不到 `{needle}`）—— \
                 「一个口径两种表示」被收口成一种了，而两类调用点各需要一种：\
                 少了哪一种，那一类调用点就静默退回非 UTF-8 客户端。"
        );
        // 反向自检（③ 那一半的牙）：一个**改了名**的声明不许算命中。
        // 没有这一格，③ 就是「那个大文件里恰好有这个串」式的恒真。
        let renamed = anchor(&format!("{ident}X"));
        assert!(
            !format!("pub {renamed} (&str, &str) = (\"x\", \"y\");\n").contains(&needle),
            "③ 的锚点匹配单位比事实小：`{renamed}` 这样一个改了名的声明也算成 `{needle}` 在"
        );
    }
    // 本侧**不许**长出 env 形：跨 SSH 这一侧没有本地 `Command` 可挂 env，
    // 走 `request_env` 要赌对端 `AcceptEnv`（不认就静默拒绝）⇒ 拿一条静默失效治另一条。
    //
    // ⚠ **必须先剥注释再扫**：本文件的头注里逐字讨论过 `LC_ALL` 那条路为什么不走
    //   （那是**警告**，不是用法），不剥就当场误报 —— 本仓「判据数到注释」已栽过三次。
    let code = guard_core::strip_comment_lines(&prod);
    assert!(
        code.len() > 5_000,
        "剥注释之后只剩 {} 字节 —— 剥过头了，下面那条在空转",
        code.len()
    );
    assert!(
        !code.contains("LC_ALL"),
        "monitor 这一侧长出了 env 形 —— 跨 SSH 那两处该用旗，理由在 `UTF8_CLIENT_FLAG` 头注"
    );

    // ── 反向自检：上面那两条 `contains` 真的分得清 ────────────────────
    // 没有这一格，② 就可能是「随便什么串都在那个大文件里」式的恒真。
    let drifted = format!("{}: &str = \"{UTF8_CLIENT_FLAG}x\";", "UTF8_CLIENT_FLAG");
    assert!(
        !home.contains(&drifted),
        "喂一个**漂了的**值居然也在后端那个家里命中（`{drifted}`）—— \
             ② 那条对拍此刻恒真，它什么都没在守"
    );
    assert!(
        !home.contains(&format!("{}.count() != expected", ".split('\\t')")),
        "backend 家里同时存在 `!=` 那一版下溢谓词 —— 口径不一致，且 `!=` 会误伤合法内容"
    );
}

/// ★★ **K-R12 `J1` 死值验（monitor 这一侧）：段数下溢必须被判废。**
///
/// 死值取自 `tests/evidence/K-R12-deathvalue.md` ①/S5：真 tmux 3.4 + POSIX 客户端，
/// 六列塌成 1 段，连 `文档` 都按显示宽度变成了 `____`。
///
/// 这一条同时把 `§5.4` 点名的那条**误伤**钉成一个可见的读数：**过溢的行今天照样被丢掉**。
/// 处置本拍**刻意没改**（理由见 [`parse_tmux_ls`] 头注），所以这里断言的是**现状**——
/// 哪天有人去修那条误伤，本条会红，那正是它该红的时候。
#[test]
fn a_dirty_line_underflows_and_an_overflowing_line_is_still_dropped_today() {
    const DIRTY: &str = "kr12_/tmp/kr12dv/____/proj_bash_0_1_cc-deadval1";
    const CLEAN: &str = "kr12\t/tmp/kr12dv/文档/proj\tbash\t0\t1\tcc-deadval1";
    const OVERFLOW: &str = "kr12\t/tmp/a\tb\tbash\t0\t1\tcc-deadval1";

    assert!(
        tmux_tab_underflow(DIRTY, TMUX_LS_FMT_FIELDS),
        "真脏字节必须判下溢"
    );
    assert!(
        !tmux_tab_underflow(CLEAN, TMUX_LS_FMT_FIELDS),
        "干净六段不许红"
    );
    assert!(
        !tmux_tab_underflow(OVERFLOW, TMUX_LS_FMT_FIELDS),
        "过溢是合法内容 ⇒ 判据不许红（这一格就是「下溢」而不是「不等于 6」的死值）"
    );

    assert!(parse_tmux_ls(DIRTY).is_empty(), "脏行不许进结果");
    let ok = parse_tmux_ls(CLEAN);
    assert_eq!(ok.len(), 1, "干净行必须解析出来（正对照）");
    assert_eq!(ok[0].name, "kr12");
    assert_eq!(ok[0].path, "/tmp/kr12dv/文档/proj");
    assert_eq!(ok[0].sid.as_deref(), Some("cc-deadval1"));
    assert!(
        parse_tmux_ls(OVERFLOW).is_empty(),
        "⚠ 现状：过溢的行**今天照样被整行丢掉**（`f.len() != 6`）—— \
             这是 K-R12 §5.4 点名的误伤，本拍只让它出声、没有改处置。\
             修它的那一拍会让本条红，那是对的。"
    );
}

/// ★ **每一处 `-t {…}` 的目标都必须出自 `exact_target`**〔audit-0805 08-07〕。
///
/// 裸 `-t <名>` 是「精确 → 名字开头 → glob」三级解析。实测（tmux 3.6）只有 `sib-2`
/// 存在时 `kill-session -t sib` 杀掉 `sib-2` 且 **rc=0**、`send-keys -t sib` 投进
/// `sib-2`、`kill-session -t 'si*'` glob 命中。本仓必然踩
/// （`pickFreshTmuxName` 造 `<sid8>-cc-2/-3`、终端 `cct` 造 `<dir>_cc-2/-3`）。
///
/// # ⚠ `K-R72`（09-12）：人群从 4 处缩到 1 处，牙跟着换住址
///
/// 那 4 处里有 3 处（`display-message` / `kill-session` / `send-keys`）随两条回落走了 ——
/// 今天 monitor 侧**只有 `capture-pane` 还在把目标插进一条 tmux 命令串**。
/// 顺带走的还有原来那半「委托给 `build_guarded_tmux_cmd` 也算」的闭环逻辑：
/// **没有受托者了，判准就回到最简的那一条** —— 谁插目标谁调 `exact_target(`。
///
/// 🔴 **分母掉到 1 之后，「地板 ≥ N」这种抽取器自检就买不到东西了**（1 处也过、
/// 0 处才红，而 0 处那天本条本来就该重写）。⇒ 换成**喂一份合成的坏语料**：
/// 同一把尺子必须在坏语料上红。这样「人群只剩一个」不等于「判据变空转」。
#[test]
fn every_target_placeholder_comes_from_exact_target() {
    // 把「找出每一处 `-t {…}` 所在的函数」抽成纯函数 —— 于是同一把尺子既量真生产段，
    // 也量下面那份合成的坏语料。**尺子只有一份**是本条的要点。
    fn sites(src: &str) -> Vec<(String, String)> {
        let lines: Vec<&str> = src.lines().collect();
        let mut out: Vec<(String, String)> = Vec::new();
        for (i, line) in lines.iter().enumerate() {
            if !line.contains("-t {") {
                continue;
            }
            // 往回找最近的 `fn 名字`，再取它的体（到下一个顶格行；
            // `where` / `)` 顶格的是头的一部分 —— 这一族本仓已栽过三次）。
            let mut s = i;
            while s > 0 && !lines[s].contains("fn ") {
                s -= 1;
            }
            let name: String = lines[s]
                .split(" fn ")
                .nth(1)
                .or_else(|| lines[s].strip_prefix("fn "))
                .unwrap_or("")
                .chars()
                .take_while(|c| c.is_alphanumeric() || *c == '_')
                .collect();
            let mut body = Vec::new();
            for (k, l) in lines[s..].iter().enumerate() {
                let cont = l.starts_with("where") || l.starts_with(')') || l.trim() == "{";
                if k > 0 && !l.is_empty() && !l.starts_with(char::is_whitespace) && !cont {
                    break;
                }
                body.push(*l);
            }
            out.push((name, body.join("\n")));
        }
        out
    }
    fn bad_of(src: &str) -> Vec<String> {
        sites(src)
            .into_iter()
            .filter(|(_, body)| !body.contains("exact_target("))
            .map(|(name, _)| name)
            .collect()
    }

    let prod = guard_core::production_code(include_str!(
        "../../../../src/bridge/src/backend/control/tmux.rs"
    ));
    let found = sites(&prod);
    // 🔴 **`K-R112`（09-13）：人群从 1 掉到 0 —— 这是这条棘轮的终态，不是它坏了。**
    //   `build_capture_pane_cmd`〔散文墓碑〕是最后一个把目标插进 tmux 命令串的地方，抓屏改走
    //   `capture-pane` 帧之后它整块删了 ⇒ monitor 侧**再没有一处**在拼 tmux 目标。
    //   ⚠ 分母 0 的判据是**空真** —— 所以这一条的牙从此**全部**压在下面那份合成语料上：
    //   同一把尺子在坏语料上必须红。两样一起断，缺一条本条就成了摆设。
    assert!(
        found.is_empty(),
        "生产段又出现了把目标插进 tmux 命令串的地方：{:?}\n\
             —— 那条路 `K-R72`/`K-R112` 已经收干净了（三条命令全走后端帧面）。\n\
             真要新增一处，`exact_target` 今天只住后端侧\n\
             （`src/backend/control/launch.rs`），别在这里重新长一份。",
        found.iter().map(|(n, _)| n).collect::<Vec<_>>()
    );
    // ★ 反向自检：同一把尺子在**合成的坏语料**上必须红。
    //   ⚠ 语料里刻意不出现 `exact_target`，断言也不取自夹具的名字（`6g` 那一族）。
    const SYNTHETIC_BAD: &str =
        "fn zzz_probe(x: &str) -> String {\n    format!(\"tmux kill-session -t {x} 2>&1\")\n}\n";
    assert_eq!(
        bad_of(SYNTHETIC_BAD),
        vec!["zzz_probe".to_string()],
        "本条的尺子在一份**明摆着裸目标**的语料上都不红 —— 它此刻什么都没在守"
    );
    // ★ 正向自检：同一把尺子在一份**调了 `exact_target` 的**语料上必须不红。
    //   只有反向那一格的话，一把「恒红」的坏尺子也能过 —— 那不是尺子，是常量。
    const SYNTHETIC_OK: &str =
        "fn zzz_ok(x: &str) -> String {\n    let t = exact_target(x);\n    format!(\"tmux kill-session -t {t} 2>&1\")\n}\n";
    assert!(
        bad_of(SYNTHETIC_OK).is_empty(),
        "本条的尺子把一份**调了 `exact_target` 的**语料也判红了 —— 它恒红，不是在守"
    );
}

// ════════════════════════════════════════════════════════════════════════
// `K-R112`（09-13）：抓屏改走 `capture-pane` 帧。〔C4e · 第四波 4C〕抓屏整条迁到界面。
// ════════════════════════════════════════════════════════════════════════
//
// 这里原来住着 `KR112D2` 的两刀机检：「抓屏这条路上没有命令串」（`the_capture_path_asks_the_backend_instead_of_composing_a_shell_line`〔散文墓碑〕，
// 带一个认命令串形态的谓词 `tmux_shell_line_markers` 与它的活体夹具〔散文墓碑〕）与「本机不再是死胡同」
// （`the_local_capture_is_no_longer_a_dead_end`〔散文墓碑〕）。抓屏改由界面经通道直接问那台机器的后端之后，
// monitor 里**那条路本身不在了**（`capture_remote_pane` / `capture_via_backend`〔散文墓碑〕都删了），两刀各自的去处：
// ① 「没有第二份实现」⇒ 下面这条零命中（整棵 monitor 生产段）＋ `frame_query_tests` 那条「已迁的零发送点」；
// ② 「本机与远端同一条路、通道不在时两句话不同」⇒ `tests/tmux-control.vitest.ts`（`<local>` 照样经通道问 · 两句话不同）。

/// ★★〔C4e · 第四波 4C〕**monitor 里抓屏一条路都不剩**（零命中 ＋ 正控）。
///
/// 守的要求：`设计/05 §14.3` 逐字「迁到通道之后，业务解释是不是**只有一个家**」——
/// 抓屏的解释今天只住 `src/tmux-control.ts`；monitor 里再长出一条拼 shell 串抓屏的路，就是同一件事的第二份实现
/// （`K-R112` 删掉的那一形：`command -v tmux` 门控 ＋ 两个哨兵）。
/// 帧命令名 `"capture-pane"` 那一格由 `frame_query_tests::the_channeled_ops_are_sent_only_through_the_channel` 管
/// （`CHANNELED_ELSEWHERE` 那一行：monitor 生产段零字面量），本条管**shell 串那几种形态**。
/// 正控：同一份语料上认得出今天真在的那条只读 tmux 调用（`list_remote_tmux` 的格式串常量）。
#[test]
fn the_monitor_has_no_capture_path_any_more() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut corpus = String::new();
    let mut files = 0usize;
    for (_, one_file) in guard_core::scan_tree_excluding(&root, &["rs"], &[]) {
        files += 1;
        corpus.push_str(&guard_core::strip_comment_lines(
            &guard_core::production_code(&one_file),
        ));
        corpus.push('\n');
    }
    assert!(files > 100, "只扫到 {files} 份 monitor 源码 —— 遍历坏了");
    // 形态现拼，免得本文件自己被别的扫描收进人群。
    let shapes = [
        format!("tmux {}", ["capture", "pane"].join("-")),
        ["NO", "PANE"].join("_"),
        ["capture", "via", "backend"].join("_"),
    ];
    for shape in &shapes {
        assert!(
            !corpus.contains(shape.as_str()),
            "monitor 生产段里又出现了 `{shape}` —— 抓屏在 monitor 里长回了一条路；\
             它只许住界面一处（`src/tmux-control.ts::capturePane`）"
        );
    }
    assert!(
        guard_core::contains_word(&corpus, "TMUX_LS_FMT"),
        "正控失败：同一份语料里认不出 `list_remote_tmux` 那条只读 tmux 调用 —— 上面的零命中不可信"
    );
}

// 〔C4e · 第四波 4C〕这里原来住着「本机杀会话 / 送键不许回落到 SSH」两条（`the_local_kill_never_falls_back_to_ssh`〔散文墓碑〕 /
//   `the_local_send_keys_never_falls_back_to_ssh`〔散文墓碑〕，P3 刀 2 · K-R56 · K-R72）：回潮闸（生产段里不许再有
//   `connect_and_exec_cmd`）＋「说真实原因」（对 `<local>` 不报「未找到远端配置」、本机与远端两句话不同）。
//   两条命令整条迁到界面之后：
//   ① 回潮闸 ⇒ `tmux_backend_gate_guard` 那两条改钉「monitor 生产段里一处破坏性 tmux 动词都没有」（界面那一侧结构上没有 SSH）；
//   ② 说真实原因 ⇒ `tests/tmux-control.vitest.ts`「通道不在：本机与远端两句话不同」（结束会话 · 发按键各一遍）。

/// F01 回归：tmux `-t` 目标**必须**精确匹配（`'=<名>:'`），绝不留裸目标。
///
/// 删掉这条性质会让换号重启把 `/exit` 敲进**兄弟会话里还活着的 claude** 并 kill 它，
/// 而 UI 报告「已重启」。**尾冒号不能省**：`send-keys`/`capture-pane` 收 target-pane，
/// `=名`（无冒号）在那条路径上 rc=1 完全失效。
///
/// # 🔴 `K-R112`（09-13）：**牙换住址 —— 两侧各一份变一份，而这一条跟过去数**
///
/// `K-R72` 把产物侧人群从三个构造器缩到 `capture-pane` 一个；本件把最后那一个也走掉了，
/// 连同它的产地 `exact_target`。⇒ **monitor 侧今天没有这条性质可断**。
///
/// ⚠ 这时有两种写法，只有一种是诚实的：
/// · 把本条删掉 ⇒ 「精确匹配」从此**无人在数**（而它仍然承重）；
/// · 让本条**跟到新住址去数** ⇒ 就是下面这样。
/// 判的是后端那棵树（读文件，不跨 crate 调用）—— 同 `tmux_backend_gate_guard`
/// 那几条两树对拍的做法。
///
/// ⚠ **它买不到什么**：只证明那两处**调了** `exact_target`，不证明 `exact_target`
/// 自己产的形状对 —— 那由后端那棵树自己的判据钉（本条够不着它的运行期）。
#[test]
fn tmux_targets_use_exact_match() {
    // ① monitor 侧：一处裸目标都不许再有（本件之后这一侧连命令串都没有了）。
    let mine = guard_core::production_code(include_str!(
        "../../../../src/bridge/src/backend/control/tmux.rs"
    ));
    assert!(
        !mine.contains("-t {"),
        "monitor 的 `tmux.rs` 生产段又出现了 `-t {{…}}` —— 那条路已经收干净了"
    );
    // ①b [`exact_target`] 自己产的形状 —— 它今天零生产调用方，但**是后端那条
    //     跨轨对拍的锚点**（见它的头注），所以这几格照旧断。
    assert_eq!(exact_target("cc-x").unwrap(), "'=cc-x:'");
    assert_eq!(exact_target("proj_cc-2").unwrap(), "'=proj_cc-2:'");
    // glob 名即便漏进来也被引号原样包住（不脱出成 shell glob）。
    assert_eq!(exact_target("si*").unwrap(), "'=si*:'");
    // 含单引号的名字仍被正确转义（`shell_quote` 的 `'\''` 形态）。
    assert!(exact_target("a'b").unwrap().starts_with("'=a"));
    assert!(exact_target("a'b").unwrap().ends_with("b:'"));
    // Gate 1：空 target 必须被拒（`=:` 会被 tmux 解析成「当前会话」）。
    assert!(exact_target("").is_err(), "空 target 必须被 Gate 1 拒绝");
    // ② 那条性质的新住址：backend 侧抓屏与杀会话**都**过 `exact_target`。
    // 〔搬树 2026-09-17〕后端树从 `remote-daemon-proto/src/` 搬到 `<repo>/src/backend/`
    // ⇒ **中间那层 `src` 没了**。原来是 `.join("src/backend").join("src").join("control")`。
    let backend = crate::guard_support::backend_src_root().join("control");
    let mut checked = 0usize;
    for (file, why) in [("capture_pane.rs", "抓屏"), ("kill.rs", "杀会话")] {
        let p = backend.join(file);
        let raw = std::fs::read_to_string(&p)
            .unwrap_or_else(|e| panic!("读不到 {p:?}：{e} —— 本条的被测对象没了，它此刻在空转"));
        let prod = guard_core::production_code(&raw);
        assert!(
            prod.contains("exact_target("),
            "backend 的 `control/{file}`（{why}）生产段里没有 `exact_target(` —— \n\
                 「`-t <名>` 必须是 `=<名>:` 精确形态」这条性质**今天两棵树上都没人守了**：\n\
                 monitor 侧 `K-R112` 已把它走掉（那一处的墓碑在本文件里），\n\
                 而这一处正是它唯一的新住址。裸目标会走 tmux 的\n\
                 「精确→名字开头→glob」三级解析 —— `cc-abc12345` 会命中 `cc-abc12345-2`。"
        );
        checked += 1;
    }
    assert_eq!(checked, 2, "只核到 {checked} 处 —— 本断言在空转");
}

#[test]
fn parse_multi_session() {
    // 真 TAB 分隔(Rust "\t" = 0x09)。6 列,末列 @ccm_sid。
    let out = "cc-abc12345\t/home/pi/proj\tclaude\t1\t2\tsess-42\nweb\t/srv/web\tzsh\t0\t1\t\n";
    let s = parse_tmux_ls(out);
    assert_eq!(s.len(), 2);
    assert_eq!(s[0].name, "cc-abc12345");
    assert_eq!(s[0].path, "/home/pi/proj");
    assert_eq!(s[0].command, "claude");
    assert!(s[0].attached);
    assert_eq!(s[0].windows, 2);
    // @ccm_sid 有值 → Some;空串 → None(向后兼容老会话)。
    assert_eq!(s[0].sid.as_deref(), Some("sess-42"));
    assert!(!s[1].attached);
    assert_eq!(s[1].command, "zsh");
    assert_eq!(s[1].sid, None);
}

#[test]
fn parse_skips_malformed_and_handles_edges() {
    // 空输出 → 空。
    assert!(parse_tmux_ls("").is_empty());
    assert!(parse_tmux_ls("\n\n").is_empty());
    // 字段数不符(无 TAB / 少字段 / 旧 5 列)→ 跳过;name 空 → 跳过。
    let out = "no tabs here\nn\t/p\tsh\t0\n\t/p\tclaude\t1\t1\told5\t/p\tclaude\t1\t2\ngood\t/home/a b\tclaude\t1\t3\t";
    let s = parse_tmux_ls(out);
    assert_eq!(s.len(), 1, "只有最后一行(6 列)合法");
    assert_eq!(s[0].name, "good");
    // 路径含空格(非 TAB)保留。
    assert_eq!(s[0].path, "/home/a b");
    assert_eq!(s[0].windows, 3);
    // 末列空串 → sid None。
    assert_eq!(s[0].sid, None);
}

#[test]
fn parse_windows_nonnumeric_falls_back_zero() {
    let s = parse_tmux_ls("n\t/p\tclaude\t1\tNaN\tsid-x");
    assert_eq!(s.len(), 1);
    assert_eq!(s[0].windows, 0);
    assert_eq!(s[0].sid.as_deref(), Some("sid-x"));
}

#[test]
fn parse_sid_rejects_unexpanded_format_and_garbage() {
    // 极老 tmux 不展开 `#{@ccm_sid}` → 原样字面串(含 `#{}`)→ 当 None,否则 findClaudeTmux 的
    // anySidKnown 恒真、老 wrapper 用户永远走不到 cwd 回退(审计建议)。
    let s = parse_tmux_ls("n\t/p\tclaude\t1\t1\t#{@ccm_sid}");
    assert_eq!(s.len(), 1);
    assert_eq!(s[0].sid, None, "未展开格式串不当 sid");
    // 合法 sid 字符集(字母数字 + - + _)照收。
    let s2 = parse_tmux_ls("n\t/p\tclaude\t1\t1\tab_c-12");
    assert_eq!(s2[0].sid.as_deref(), Some("ab_c-12"));
}

// 〔C4e · 第四波 4C〕这里原来住着「抓不到的五档分得开、认不出的码不许猜」（`the_five_capture_refusals_stay_apart`〔散文墓碑〕，
//   驱动 monitor 的 `describe_capture_refusal`〔散文墓碑〕）。那一份说法随抓屏迁到界面：同一条性质住
//   `tests/tmux-control.vitest.ts`（码集合取自跨语言金样 —— 与后端 `REGISTRY` 那一块对拍过的同一份，不是手抄）。

#[test]
fn fmt_uses_real_tab_not_literal_backslash_t() {
    // 回归调研 03 §3.1 坑:格式串里必须是真 TAB 字节,不能是字面 \t。
    assert!(TMUX_LS_FMT.contains('\t'), "格式串须含真 TAB");
    assert!(!TMUX_LS_FMT.contains("\\t"), "格式串不得含字面反斜杠-t");
}

#[test]
fn tmux_ls_fmt_double_write_point_stays_in_sync() {
    // F08a：TMUX_LS_FMT 双写点断言（红线 I8 的机器化护栏）。monitor(本 const) 与 backend
    // (`src/backend/observe/watcher.rs`) 分属两个独立 crate、不能共享 const，但两侧
    // `tmux ls -F` 格式串**必须逐字一致**（否则后端推的列 monitor 解错位）。编译期
    // include_str! 读后端源，把本 const 的真 TAB 折回源码里的 `\t` 转义再断言后端源
    // 含该带引号字面量——**双向**：改 monitor 或后端任一侧忘同步，本测即红。
    let backend_src = include_str!(backend_watcher_src!());
    let source_literal = TMUX_LS_FMT.replace('\t', "\\t");
    // 锚定到 const 定义行（非裸字面量）——否则该字面量若也出现在某条注释里，会掩盖真 const 漂移
    // （假阴性）。backend 侧常量名同为 TMUX_LS_FMT（红线 I8 不许改），故按定义行精确比对。
    let expected_def = format!("const TMUX_LS_FMT: &str = \"{source_literal}\";");
    assert!(
        backend_src.contains(&expected_def),
        "TMUX_LS_FMT 双写点漂移：backend watcher.rs 不含与 monitor 侧一致的定义 {expected_def:?}\n\
             （改了 tmux ls 格式串就得两侧同步——红线 I8）"
    );
}
