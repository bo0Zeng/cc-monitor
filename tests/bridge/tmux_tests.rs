// `include_str!` 只接**字面量 token**，喂 `const` 会报 `argument must be a string literal`
// ⇒ 用单臂宏拿到「单一落点」。原住 `src/bridge/src/tmux.rs`，步 7b 随它唯一的消费者搬来这里；
// 路径也跟着换成相对本文件（`16 §5.4a` 规则 1：路径不只住在字面量里，也住在宏展开里）。
macro_rules! daemon_watcher_src {
    () => {
        "../../src/backend/observe/watcher.rs"
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
        // daemon 自报观测失败 —— `#82` 要的就是这一格的频率。
        classify_tmux_observation("", Some(OBS_UNOBSERVABLE)),
        // 旧 daemon 的空串歧义。
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

/// ★ P1 的回归测试（**这条在修之前是红的**）：daemon 确证零会话 ⇒ 必须是**有效观测（空集）**，
/// 不是跳过。这就是 `src/doc/INVARIANTS.md` §24bis 那条残留 bug 的机理：
/// 杀掉某 origin 仅剩的 tmux 会话 → server 随之退出 → `tmux ls` 回空 →
/// 旧代码保守跳过 → idle 灰灯卡到断连 flush 才清。
#[test]
fn zero_sessions_is_a_valid_observation_not_a_skip() {
    assert_eq!(
        classify_tmux_observation("", Some("zero_sessions")),
        TmuxObservation::Backend(std::collections::HashSet::new()),
        "daemon 确证零会话时必须进对账（空集），否则灰灯永不清"
    );
}

/// 旧 daemon（无 `observation` 字段）+ 空 raw ⇒ **保持今天的保守行为**。
/// 空 raw 在旧 daemon 那里同时意味着「零会话」和「`tmux ls` 出错被 `|| true` 吞了」，
/// 分不开 ⇒ 只能跳过。**新旧混搭不许回归。**
#[test]
fn old_daemon_empty_raw_still_skips() {
    assert_eq!(
        classify_tmux_observation("", None),
        // ★ P8c：连**原因**一起钉 —— 原来只钉「跳了」，而三种完全不同的原因
        // 压成同一个无载荷的 `Skip` 正是 `#82` 问不出频率的来源。
        TmuxObservation::Skip(SkipReason::LegacyAmbiguousEmpty),
        "旧 daemon 的空串语义不可分，必须保守跳过"
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

/// 向前兼容：未来 daemon 加了本 monitor 不认识的分类 ⇒ **落回 raw 判据**，
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

/// P1：`observation` 取值集是 monitor↔daemon 的**第三个双写点**（前两个：`TMUX_LS_FMT` ·
/// `NO_TMUX` 哨兵）。两个独立 crate 不能共享类型 ⇒ 用与
/// `tmux_ls_fmt_double_write_point_stays_in_sync` 相同的办法钉住：`include_str!` 读 daemon 源
/// + **锚定 const 定义行**（不是裸字面量——否则该串若出现在某条注释里会掩盖真漂移）。
///   **双向**：改 monitor 或 daemon 任一侧忘同步，本测即红。
#[test]
fn observation_tokens_double_write_point_stays_in_sync() {
    let daemon_src = include_str!(daemon_watcher_src!());
    for (name, value) in [
        ("OBS_ZERO_SESSIONS", OBS_ZERO_SESSIONS),
        ("OBS_NO_TMUX", OBS_NO_TMUX),
        ("OBS_UNOBSERVABLE", OBS_UNOBSERVABLE),
    ] {
        let expected_def = format!("const {name}: &str = \"{value}\";");
        assert!(
            daemon_src.contains(&expected_def),
            "observation 双写点漂移：daemon watcher.rs 不含 {expected_def:?}\n\
                 （改了分类取值就得两侧同步——同 TMUX_LS_FMT 的纪律）"
        );
    }
    // 反向自检：断言的是「扫到了 daemon 源」而不是「命中若干条」——阈值不能挂在
    // 被检查的量上（rust-ts-boundary 的教训）。
    assert!(
        daemon_src.len() > 1000,
        "include_str! 没读到 daemon 源，上面三条断言全是空转"
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
/// ① 谓词本体 [`gate1_reject_empty`]（`exact_target` 与两条后端命令共用的那一份）；
/// ② `capture-pane` 构造器（经 [`exact_target`]，今天唯一还在拼 shell 串的那条）；
/// ③④ 两条后端命令的**生产入口本体** —— 真调 [`tmux_send_keys`] / [`kill_remote_tmux`]，
///    断言它在**任何 IO 之前**就地拒。那一句同时是「本地校验先于一切往返」这条性质的读数：
///    它报的若是「后端通道不在」，就说明 Gate 1 跑到 IO 后面去了。
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
    // ②③④ 三条后端命令：**真跑生产入口**〔`K-R112` 09-13：抓屏从「构造器那一格」
    //     挪进这一段 —— 它的构造器随那条 SSH 串一起删了，而生产入口比构造器强一格〕。
    // ⚠ 不必登记入方向通道 —— Gate 1 在 `daemon_*` 之前，根本走不到那一步。
    let rt = tokio::runtime::Builder::new_current_thread()
        .build()
        .expect("建不出 runtime —— 本条无从判断，别读成绿");
    let sk = rt
        .block_on(tmux_send_keys(
            "no-such-origin".to_string(),
            String::new(),
            "/exit".to_string(),
            Some(true),
        ))
        .expect_err("空 target 的 send-keys 不该报成功");
    let kill = rt
        .block_on(kill_remote_tmux(
            "no-such-origin".to_string(),
            String::new(),
        ))
        .expect_err("空 target 的 kill 不该报成功");
    let cap = rt
        .block_on(capture_remote_pane(
            "no-such-origin".to_string(),
            String::new(),
        ))
        .expect_err("空 target 的 capture-pane 不该报成功");
    for (label, err) in [("send-keys", &sk), ("kill", &kill), ("capture-pane", &cap)] {
        assert!(
            err.contains("非法 tmux 目标（空）"),
            "{label} 的空 target 没被 Gate 1 就地拒。实得：{err}"
        );
        assert!(
            !err.contains("后端通道不在"),
            "{label} 走到后端那一步才失败 —— Gate 1 不再先于一切 IO 了。实得：{err}"
        );
    }
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
    let prod = guard_core::production_code(include_str!("../../src/bridge/src/tmux.rs"));
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

/// daemon 侧那个「一个口径一个家」的家（相对**仓根**）—— 跨仓对拍的被读对象。
///
/// 单一落点：路径写死在这里一处，daemon 再搬家只改这一行。
const DAEMON_KOU_JING_HOME: &str = "src/backend/common/tmux_utf8.rs";

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
/// | ② | 本侧那个值与 daemon 家里那一行**逐字相等**（值是从本侧 const **现取**的） | 两侧漂开而两边都不红 —— 正是本件治的那个形状 |
/// | ③ | daemon 家里**两种表示都还在** | 有人把家「收口」成一种表示 ⇒ 另一类调用点静默失效 |
///
/// ②③ 都是**读两棵树**才验得了的性质：两个 crate 不共享源码树，共用 `const` 拿不到
/// （七个 `*-core` 的职责逐条都装不下，论据在 `UTF8_CLIENT_FLAG` 的头注里）。
///
/// # ⚠ 作用域，逐条说清（`brief` 12：报一个数就要说清尺子）
///
/// - **在哪跑**：monitor 那格 cargo（`cargo test --workspace --lib`）。
///   daemon 自己那格看不见它 —— 但门禁两格都跑，所以任一侧漂开都会在门禁里红。
/// - **读了哪两棵树**：本侧 `include_str!("../../src/bridge/src/tmux.rs")`（编译期，同一半）+
///   daemon 侧 [`DAEMON_KOU_JING_HOME`]（**运行期** `read_to_string`）。
/// - 🔴 **为什么 daemon 那一半刻意用运行期读、而不是 `include_str!`**：
///   `include_str!` 会新长出一条**跨半边的编译期边**，而那种边由
///   `cross_half_edge_registry::CROSS_EDGES` 逐条登记着（多一条就红），
///   **那个文件不在本拍写区**。运行期读在本仓是**既有做法**、不是绕道：
///   `cross_half_edge_registry` 自己就是运行期遍历 daemon 那棵树的
///   （`both_halves()` 扫 `src/backend`），`scanning_guard_registry::PENDING`
///   里也直接列着 daemon 的文件。而且它在该登记表关心的那一维上**更轻**：
///   daemon 换布局时这里是一句说得清的运行期失败，不是 `cargo test` 编不过。
///   ⚠ 代价如实写下：这条边因此**不出现在** `CROSS_EDGES` 里。
///   PM 若要它以编译期形态登记，改法是**两处一起动、不许只动一处**：
///   ① 把下面那句运行期读换成编译期读（`include_str!` 配 `concat!` / `env!` 拼路径，
///      形状照本文件已有的 `daemon_watcher_src` 那个单一落点宏）；
///   ② 同轮在 `CROSS_EDGES` 里加一条 `monitor→daemon` 的登记
///      （读者 `src/bridge/src/tmux.rs` · 被读 `src/backend/common/tmux_utf8.rs` ·
///      理由「跨轨对拍：口径的家在对面，本侧那一份必须与它逐字相等」）。
///   🔴 只动 ① 会让那张表的条数当场对不上 —— 它是**两个方向都查**的。
/// - **不管什么**：它不证明「那个旗真的被走到了」（「盘上有 ≠ 被走到」）。
///   行为那一半的死值在 `tests/evidence/K-R12-deathvalue.md`（真 tmux 3.4 私有 socket）。
#[test]
fn utf8_client_kou_jing_has_one_home_and_this_side_matches_it() {
    let prod = guard_core::production_code(include_str!("../../src/bridge/src/tmux.rs"));
    guard_core::assert_no_test_code("tmux.rs", &prod);

    // ── ① 本侧只有一个声明 ────────────────────────────────────────────
    let decl_prefix = format!("{}: &str =", "UTF8_CLIENT_FLAG");
    guard_core::find_pinned(&prod, &decl_prefix)
        .unwrap_or_else(|e| panic!("本文件生产段里 `{decl_prefix}` 不是恰好一处：{e}"));
    let root = crate::guard_support::repo_root();
    // 🔴 〔搬树 2026-09-18 · `设计/99` 条 73〕**排掉的是谁、为什么 —— 明写。**
    //
    // 排掉 `src/bridge/src/tmux.rs`：它就是「那一个家」，上面第 ① 段已经用 `find_pinned`
    // 单独钉过它「恰好一处」。这里数的是**第二个家**，本来就不该把它自己算进去。
    //
    // 上一版靠 `scan_tree!` 的 `file!()` 自摘 —— 当年判据住在 `tmux.rs` 自己的
    // `#[cfg(test)]` 段里，「摘掉调用者」恰好等于「摘掉被测那份」。剖分之后 `file!()`
    // 指向本测试文件，那一刀**整个落空**，`tmux.rs` 回到人群里 ⇒ 报「monitor 侧有第二个」，
    // 而盘上真的只有一个。⇒ 换成明写的排除（摘不到它，`scan_tree_excluding` 当场红）。
    let others = guard_core::scan_tree_excluding(
        &root.join("src/bridge/src"),
        &["rs"],
        &["src/bridge/src/tmux.rs"],
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

    // ── ② 与 daemon 那个家逐字相等（值现取，不写死） ──────────────────
    let home_path = root.join(DAEMON_KOU_JING_HOME);
    let home = std::fs::read_to_string(&home_path).unwrap_or_else(|e| {
        panic!(
            "读不到 daemon 侧那个家 {home_path:?}：{e}\n\
                 它是 K-R12 下一拍建的「一个口径一个家」。文件被搬了 ⇒ 改本文件那个常量；\
                 家被删了 ⇒ 那个口径退回三份靠人对齐，先回件文件。"
        )
    });
    assert!(
        home.len() > 2_000,
        "daemon 那个家只有 {} 字节 —— 没读到内容，下面的对拍是空转的",
        home.len()
    );
    let want_flag = format!("{}: &str = {UTF8_CLIENT_FLAG:?};", "UTF8_CLIENT_FLAG");
    assert!(
        home.contains(&want_flag),
        "跨仓漂移：本侧的旗是 {UTF8_CLIENT_FLAG:?}，而 daemon 那个家里找不到 `{want_flag}`。\n\
             两侧漂开时**两边都不会因为别的判据变红** —— 那正是本件立件时的那个形状。"
    );
    // 段数下溢那个谓词是同一族的第二个口径：比的是**函数体**，不是名字。
    let body = format!("{}.count() < expected", ".split('\\t')");
    guard_core::find_pinned(&prod, &body)
        .unwrap_or_else(|e| panic!("本侧那个下溢谓词的体不是恰好一处：{e}"));
    assert!(
        home.contains(&body),
        "跨仓漂移：下溢谓词的体两侧不一致（本侧是 `{body}`，daemon 家里找不到）。\n\
             口径一致本身就是要买的东西：一侧改成 `!=` 就会开始误伤合法内容。"
    );

    // ── ③ daemon 家里两种表示都还在 ───────────────────────────────────
    // ⚠ 锚点只钉「那里有一个声明」（`const <名>:`），**不钉类型写法** ——
    //   带类型标注的锚点实测会被 `(&'static str, &'static str)` 这种合法写法误伤，
    //   而它印出来的话是「家里少了 env 形」：一句指向完全错误方向的诊断。
    //   ⚠ 同时**刻意收在 `:` 上**：收在标识符上时 `const <名>X:` 会被裸 `contains`
    //   当成命中（「匹配单位比事实小」那一族），于是「家改名了」这一形看不见。
    // 表里存**标识符**，锚点现拼 —— 反向自检那份「改了名」的夹具必须从标识符派生，
    // 从锚点文本派生的夹具会跟着锚点一起变松，于是「锚点变松了」这件事自己看不见
    // （daemon 那侧的同职判据实测栽过这一形，头注里逐字记着）。
    let anchor = |ident: &str| format!("const {ident}:");
    for (label, ident) in [
        ("argv 形（旗）", "UTF8_CLIENT_FLAG"),
        ("env 形", "UTF8_CLIENT_ENV"),
    ] {
        let needle = anchor(ident);
        assert!(
            home.contains(&needle),
            "daemon 那个家里少了**{label}**（找不到 `{needle}`）—— \
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
        "喂一个**漂了的**值居然也在 daemon 那个家里命中（`{drifted}`）—— \
             ② 那条对拍此刻恒真，它什么都没在守"
    );
    assert!(
        !home.contains(&format!("{}.count() != expected", ".split('\\t')")),
        "daemon 家里同时存在 `!=` 那一版下溢谓词 —— 口径不一致，且 `!=` 会误伤合法内容"
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

    let prod = guard_core::production_code(include_str!("../../src/bridge/src/tmux.rs"));
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
             真要新增一处，`exact_target` 今天只住 daemon 侧\n\
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
// `K-R112`（09-13）：抓屏改走 `capture-pane` 帧。下面两条是 `KR112D2` 的机检。
// ════════════════════════════════════════════════════════════════════════

/// 「这段代码在**拼 / 跑一条 tmux shell 串**吗」—— 认形态，不认某一个符号。
///
/// 🔴 失效方向（`KR112D2` 逐字点名的那个）：判「源码里还有没有 `Command::new`」是**判写法**，
/// 它挡不住换个写法再拼一遍。⇒ 本谓词认的是「命令串」这件事的几种形态，
/// 而它对每一种真的会响这件事，由 [`the_tmux_shell_line_detector_really_sees_each_shape`]
/// 用活体语料证明。
fn tmux_shell_line_markers(body: &str) -> Vec<&'static str> {
    // 逐条：送进远端 shell · 经命令构造器 · 直接写 tmux 命令 · 那两个老哨兵 ·
    // 为了拼进 shell 才需要的引用 · 问「这台远端怎么连」。
    [
        "connect_and_exec_cmd(",
        "_cmd(",
        "tmux capture-pane",
        "command -v tmux",
        "NO_PANE",
        "shell_quote(",
        "load_remote_config_by_label(",
    ]
    .into_iter()
    .filter(|m| body.contains(m))
    .collect()
}

/// ★ 上面那个谓词的**活体夹具**：它对每一种形态都得真的响。
#[test]
fn the_tmux_shell_line_detector_really_sees_each_shape() {
    // 形态一律**现拼**，免得夹具自己被真树上的扫描收进人群。
    let old = format!(
        "let cmd = build{u}capture{u}pane{u}cmd(&target)?;\n\
             let cfg = crate::load{u}remote{u}config{u}by{u}label(&origin)?;\n\
             let stream = ssh{u}source::connect{u}and{u}exec{u}cmd(&cfg, &cmd).await?;",
        u = "_"
    );
    assert!(
        tmux_shell_line_markers(&old).len() >= 3,
        "老那条路（构造器 + 远端配置 + 一次性 exec）没被认出来：{:?}",
        tmux_shell_line_markers(&old)
    );
    let inlined = format!(
        "let c = format!(\"if command {v} tmux; then tmux capture{d}pane -p -t {{t}}; fi\");",
        v = "-v",
        d = "-"
    );
    assert!(
        !tmux_shell_line_markers(&inlined).is_empty(),
        "**换个写法内联拼一份**没被认出来 —— 那正是「判写法」买不到的那一格"
    );
    // 反向：一段真的只调帧面原语的代码不许被误判。
    let clean = "let reply = client.call(CAPTURE_PANE, capture_pane_args(target), d).await";
    assert!(
        tmux_shell_line_markers(clean).is_empty(),
        "只调原语的代码被误判成拼串：{:?}",
        tmux_shell_line_markers(clean)
    );
}

/// ★★ `KR112D2` 刀①：**抓一屏走的是 `capture-pane` 帧，不是一次性 SSH。**
///
/// 判的是**这条路**（`capture_remote_pane` → `capture_via_daemon`）上有没有命令串。
#[test]
fn the_capture_path_asks_the_backend_instead_of_composing_a_shell_line() {
    let prod = guard_core::production_code(include_str!("../../src/bridge/src/tmux.rs"));
    let body_of = |sig: &str| -> String {
        let at = guard_core::find_pinned(&prod, sig)
            .unwrap_or_else(|e| panic!("生产段找不到 {sig}（{e}）—— 判据在空转"));
        prod[at..]
            .lines()
            .skip(1)
            .take_while(|l| *l != "\u{7d}")
            .collect::<Vec<_>>()
            .join("\n")
    };
    let mut checked = 0usize;
    for sig in [
        "pub async fn capture_remote_pane(",
        "async fn capture_via_daemon(",
    ] {
        let body = body_of(sig);
        assert!(
            body.chars().count() > 60,
            "`{sig}` 的体只切出 {} 字 —— 抽取器坏了，本条在空转",
            body.chars().count()
        );
        let hits = tmux_shell_line_markers(&body);
        assert!(
            hits.is_empty(),
            "`{sig}` 这条路上又出现了命令串的痕迹 {hits:?}。\n\
                 抓一屏归 daemon 的 `capture-pane` 原语（`K-R86` 出、`K-R104` 上帧面）——\n\
                 拼一条 shell 串走 SSH 就是同一件事的第二份实现（`K33`「所有命令只许有一处」）。"
        );
        checked += 1;
    }
    assert_eq!(checked, 2, "只核到 {checked} 段 —— 本断言在空转");
    // 它真的调了那条原语，而且能力协商排在抓之前。
    let via = body_of("async fn capture_via_daemon(");
    let ask = via
        .find("accepts(CAPTURE_PANE)")
        .expect("`capture_via_daemon` 没有先问一句能力 —— 「这台后端太旧」永远说不出口");
    let call = via
        .find("CAPTURE_PANE,")
        .expect("`capture_via_daemon` 没在调那条原语 —— 判据的参照物没了");
    assert!(ask < call, "能力协商排在真抓之后 —— 那就永远走不到");
    assert!(
        via.contains("capture_pane_args(target)"),
        "参数不是走那份共用的构造器 —— 字段名一漂，症状是「命令发出去了、对面说缺字段」"
    );
    // 分流走那唯一的一份（`daemon_route` 的登记表逐字要求每个发送端表态）。
    assert!(
        via.contains("route_call_error"),
        "抓屏这个发送端自己在判「要不要回落」—— 那是分流规则的第二份实现"
    );
}

/// ★★ `KR112D2` 刀②：**本机那一支从「回一句还看不了」变成真去抓。**
///
/// 老行为逐字是：`<local>` 直接早退，回一句「本机还看不了 …的画面预览」。
/// 那句话当时诚实（daemon 没有抓屏原语），今天**前提到期**（`K-R86`/`K-R104`）。
///
/// ⚠ **射程写清楚**：本条不证明「本机真抓得到一屏」—— 那要后端在、且有一个真 tmux 会话，
/// 而沙箱里 `<local>` 上没有入方向通道。本条证的是**两件可判的事**：
/// ① 那条早退（连同那句话）在盘上没了；② 本机与远端**走的是同一段代码**，
/// 通道不在时两边只差一个称呼 —— 那正是「本机不再是死胡同」的可判形式。
#[test]
fn the_local_capture_is_no_longer_a_dead_end() {
    let prod = guard_core::production_code(include_str!("../../src/bridge/src/tmux.rs"));
    let at = guard_core::find_pinned(&prod, "pub async fn capture_remote_pane(")
        .expect("抓屏入口不在了");
    let body: String = prod[at..]
        .lines()
        .skip(1)
        .take_while(|l| *l != "\u{7d}")
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        body.contains("capture_via_daemon("),
        "抽到的 `capture_remote_pane` 体里连主路都没有 —— 抽取器坏了，本条空转。实得 {} 字节",
        body.len()
    );
    // ① 那条本机早退没了：`origin` 是入参，不是分支。
    for forbidden in ["LOCAL_ORIGIN", "origin =="] {
        assert!(
            !body.contains(forbidden),
            "`capture_remote_pane` 里又出现了 `{forbidden}` —— 本机那条早退回潮了。\n\
                 它回的那句「本机还看不了」在 `K-R86`/`K-R104` 之后是**假话**：\n\
                 daemon 有 `capture-pane` 了，`<local>` 也是一个 origin。"
        );
    }
    // ② 两侧同一段代码：通道不在时只差一个称呼。
    let _guard = crate::inbound_client::local_origin_test_lock();
    assert!(
        crate::inbound_client::client_for(crate::inbound_client::LOCAL_ORIGIN).is_none(),
        "测试进程里 `<local>` 上居然有入方向通道 —— 本条的前提不成立，下面几句会空转"
    );
    let rt = tokio::runtime::Builder::new_current_thread()
        .build()
        .expect("建不出 runtime —— 本条无从判断，别读成绿");
    let local = rt
        .block_on(capture_remote_pane(
            crate::inbound_client::LOCAL_ORIGIN.to_string(),
            "cc-abc12345".to_string(),
        ))
        .expect_err("本机后端通道不在，这一趟不该报成功");
    let remote = rt
        .block_on(capture_remote_pane(
            "kr112-remote-label".to_string(),
            "cc-abc12345".to_string(),
        ))
        .expect_err("那台远端没配过，这一趟不该报成功");
    // 🔴 老那句话必须不在了 —— 它是「本机做不到」的字面形式。
    assert!(
        !local.contains("还看不了"),
        "本机仍在回那句「还看不了」—— 前提到期了，那句话今天是假的：{local}"
    );
    // 也不许退回那句与真实原因毫无关系的「未找到远端配置」（`P4d-Y5` 收口的那一族）。
    for e in [&local, &remote] {
        assert!(
            !e.contains("未找到远端配置"),
            "抓屏又报「未找到远端配置」—— 那是 `P4d-Y5` 收口的那一族假话：{e}"
        );
    }
    assert_ne!(
        local, remote,
        "本机与远端的「通道不在」共用了同一句话 —— 下一步不同却说同一句，\n\
             就等于把两件事压成一个读数"
    );
    assert!(
        remote.contains("kr112-remote-label") && !remote.contains("本机"),
        "远端那句话没点出是哪台机器、或者错用了本机那半。实得：{remote}"
    );
    // 空目标那道 Gate 1 仍在本地就地判（不该先花一次往返）。
    let empty = rt
        .block_on(capture_remote_pane(
            "kr112-remote-label".to_string(),
            String::new(),
        ))
        .expect_err("空目标必须被 Gate 1 拒");
    assert!(
        empty.contains("非法 tmux 目标"),
        "空目标不是被 Gate 1 拒的（`=:` 会被 tmux 解析成「当前会话」）：{empty}"
    );
}

/// ★ **P3 刀 2：本机 kill 不许回落到 SSH**〔08-11〕。
///
/// # ⚠ `K-R72`（09-12）：性质**变强了**，判法跟着换 —— 不是这一条死了
///
/// 它原来钉的是「那条 SSH 回落**之前**有本机的早退」（比的是两个位置的先后）。
/// 今天那条 SSH 回落整个没了 ⇒ **「本机不许回落到 SSH」从一条纪律变成一条结构事实**。
/// 位置判据在一个不存在的东西上无从谈起，但它买的那两件事一件都不许丢：
/// ① **盘上没有第二条路** —— 生产段里再出现 `connect_and_exec_cmd` 就红（**回潮闸**）；
/// ② **说的是真实原因** —— 对 `<local>` 报的不许是「未找到远端配置」那句与真实原因
///    毫无关系的话。**错的诊断比没有诊断更贵。**
///
/// ⚠ 射程：本条**不证明**本机 kill 真的杀得掉（那要后端在、且有一个真 tmux 会话）。
/// 后者今天没有 UI 入口（见 `K-R56#§0j`），所以也没有实测。
#[test]
fn the_local_kill_never_falls_back_to_ssh() {
    let prod = guard_core::production_code(include_str!("../../src/bridge/src/tmux.rs"));
    let at =
        guard_core::find_pinned(&prod, "pub async fn kill_remote_tmux(").expect("kill 入口不在了");
    let body: String = prod[at..]
        .lines()
        .skip(1)
        .take_while(|l| *l != "\u{7d}")
        .collect::<Vec<_>>()
        .join("\n");
    // 抽取器自检：抽空了下面那条就恒绿。
    assert!(
        body.contains("daemon_kill::daemon_kill("),
        "抽到的 `kill_remote_tmux` 函数体里连主路都没有 —— 抽取器坏了，本条此刻空转。\n\
             实得 {} 字节",
        body.len()
    );
    // ① 回潮闸：这条命令里**不许再有** SSH 那条路。
    assert!(
        !body.contains("connect_and_exec_cmd"),
        "`kill_remote_tmux` 里又出现了 `connect_and_exec_cmd` —— 那条一次性 SSH 回落回潮了。\n\
             `K-R54` 表第 2 处判它删：daemon 那条先 `admit_destructive` 拿 `#{{session_id}}`\n\
             **句柄**再杀，而 SSH 那条杀的是 `=name:`（**名字**）—— 破坏性动作对名字下手\n\
             就把 TOCTOU 窗口留着。要恢复它先回 `K-R54` 重新裁定。"
    );
    // ② 真实原因：本机那句话不许说成「未找到远端配置」。
    let _guard = crate::inbound_client::local_origin_test_lock();
    assert!(
        crate::inbound_client::client_for(crate::inbound_client::LOCAL_ORIGIN).is_none(),
        "测试进程里 `<local>` 上居然有入方向通道 —— 本条的前提不成立，下面那句会空转"
    );
    let rt = tokio::runtime::Builder::new_current_thread()
        .build()
        .expect("建不出 runtime —— 本条无从判断，别读成绿");
    let err = rt
        .block_on(kill_remote_tmux(
            crate::inbound_client::LOCAL_ORIGIN.to_string(),
            "cc-abc12345".to_string(),
        ))
        .expect_err("本机后端通道不在，这一趟不该报成功");
    assert!(
        !err.contains("未找到远端配置"),
        "本机 kill 报的是「未找到远端配置」—— 那是 SSH 回落那条路的话，\n\
             而真实原因是本机后端通道不在。实得：{err}"
    );
    assert!(
        err.contains("本机后端通道不在"),
        "本机那条早退在，但它没说出真实原因。实得：{err}"
    );
}

/// ★★ **`send-keys` 这条路上，本机也不许悄悄回落到一次性 SSH**（`K-R56`，09-11 买到）。
///
/// # ⚠ `K-R72`（09-12）：性质**变强了**，判法跟着换 —— 不是这一条死了
///
/// `K-R56` 立它时，`tmux_send_keys` 有一条 SSH 回落，而 `<local>` 会掉进
/// `load_remote_config_by_label("<local>")`，报 **「未找到远端配置: `"<local>"`」** ——
/// 一句与真实原因（本机后端通道不在）毫无关系的话。**错的诊断比没有诊断更贵。**
/// 今天那条回落整个删了 ⇒ 「本机不许回落到 SSH」从一条纪律变成一条**结构事实**。
/// 本条因此加一格、并把原来那格保住：
/// ① **回潮闸**（新）：生产段里再出现 `connect_and_exec_cmd` 就红；
/// ② **说的是真实原因**（原有那格，一个字没改判法）：真调生产入口 [`tmux_send_keys`]，
///    看它到底报了哪句话；
/// ③ **本机与远端的话不许一样**（新）：两条路的下一步不同 —— 一个是「先让本机后端跑起来」，
///    另一个是「先让那台机器上的后端连上」。压成一句就等于把两个处置合并成一个读数。
///
/// # ⚠ 射程，逐条说清（`brief` 12：报一个性质就要说清尺子）
///
/// - 钉的是「它报的是真实原因、而且盘上没有第二条路」。
///   **不证明**本机 send-keys 真的送得到 —— 那要后端在 + 一个真 tmux 会话，
///   而真 tmux 本区口径禁（`K-R56#§0d`）。
/// - 🔴 **②③ 比的是 `origin`，不是 `target` 会话名**：判据逐字是
///   `origin == LOCAL_ORIGIN`（**逐字节相等**）。⇒
///   · **拦得住**：唯一那个前端/后端约定的哨兵串（`inbound_client::LOCAL_ORIGIN`，
///     由 `inbound_client_tests.rs::the_local_origin_is_the_same_string_on_both_sides`
///     钉着它与前端 `daemon-policy.ts` 那份逐字相同）。
///   · **拦不住**：一台 label 起成 `localhost` / `127.0.0.1` / 本机主机名的**远端**
///     （即便它就是这台机器）—— 走的是远端那句话。⚠ 那**是对的**：它确实是一条远端传输。
///   · **也拦不住**：大小写 / 前后空白不同的写法（`<LOCAL>`、`" <local>"`）——
///     但那些今天进不来，`LOCAL_ORIGIN` 是常量、不是用户输入。
///     真正的撞名口子是 `LOCAL_ORIGIN` 自己头注逐字承认的那条：
///     「用户理论上可以把某台远端机器的 label 起成这个名字……**不做防御**」。
///   ⚠ **09-11 自查回打（`K-R56`）**：这一段第一版点的是一个**编出来的**判据名（盘上零处），
///     被 `structural_scan.rs` 那条「散文点名的名字必须在代码里」的机检当场逮住。
///     🔴 那个假名字与两趟判定行逐字抄在 `tests/evidence/K-R56-deathvalue.md`，
///     刻意不抄在这里：抄回来就又是一处「散文点名一个不存在的名字」。
#[test]
fn the_local_send_keys_never_falls_back_to_ssh() {
    // ① 回潮闸：这条命令里**不许再有** SSH 那条路。
    let prod = guard_core::production_code(include_str!("../../src/bridge/src/tmux.rs"));
    let at = guard_core::find_pinned(&prod, "pub async fn tmux_send_keys(")
        .expect("send-keys 入口不在了");
    let body: String = prod[at..]
        .lines()
        .skip(1)
        .take_while(|l| *l != "\u{7d}")
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        body.contains("daemon_send_keys::daemon_send_keys("),
        "抽到的 `tmux_send_keys` 函数体里连主路都没有 —— 抽取器坏了，本条此刻空转。\n\
             实得 {} 字节",
        body.len()
    );
    assert!(
        !body.contains("connect_and_exec_cmd"),
        "`tmux_send_keys` 里又出现了 `connect_and_exec_cmd` —— 那条一次性 SSH 回落回潮了。\n\
             `K-R54` 表第 1 处判它删（`K-R56` 先把两条路的「探了没有」补齐才删得掉）。\n\
             要恢复它先回 `K-R54` 重新裁定。"
    );

    // ②③ 行为：真调生产入口，看它报了哪句话。
    // 登记表是**进程内全局**的 ⇒ 与别的会在 `<local>` 键上登记通道的用例串起来跑。
    let _guard = crate::inbound_client::local_origin_test_lock();
    // 前提自检：本条靠「`<local>` 上没有通道」才走得到 `NoChannel` 那一臂。
    assert!(
        crate::inbound_client::client_for(crate::inbound_client::LOCAL_ORIGIN).is_none(),
        "测试进程里 `<local>` 上居然有入方向通道 —— 本条的前提不成立，下面那句会空转"
    );
    let rt = tokio::runtime::Builder::new_current_thread()
        .build()
        .expect("建不出 runtime —— 本条无从判断，别读成绿");
    let send = |origin: &str| {
        rt.block_on(tmux_send_keys(
            origin.to_string(),
            "cc-abc12345".to_string(),
            "/compact".to_string(),
            Some(true),
        ))
        .expect_err("后端通道不在，这一趟不该报成功")
    };
    let local = send(crate::inbound_client::LOCAL_ORIGIN);
    assert!(
        !local.contains("未找到远端配置"),
        "本机 send-keys 报的是「未找到远端配置」—— 那是 SSH 回落那条路的话，\n\
             而真实原因是本机后端通道不在。**错的诊断比没有诊断更贵。**\n\
             实得：{local}"
    );
    assert!(
        local.contains("本机后端通道不在"),
        "本机那条早退在，但它没说出真实原因。实得：{local}"
    );
    // ③ 远端那条：话必须不一样，而且也说得出下一步。
    let remote = send("some-remote-label");
    assert_ne!(
        local, remote,
        "本机与远端的「通道不在」共用了同一句话 —— 处置相同没问题，\n\
             **下一步不同却说同一句**就等于把两件事压成一个读数（本机是「本机后端没起来」，\n\
             远端是「那台机器上的后端没连上」）。"
    );
    assert!(
        remote.contains("some-remote-label") && !remote.contains("本机"),
        "远端那句话没点出是哪台机器、或者错用了本机那半。实得：{remote}"
    );
}

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
/// 判的是 daemon 那棵树（读文件，不跨 crate 调用）—— 同 `tmux_daemon_gate_guard`
/// 那几条两树对拍的做法。
///
/// ⚠ **它买不到什么**：只证明那两处**调了** `exact_target`，不证明 `exact_target`
/// 自己产的形状对 —— 那由 daemon 那棵树自己的判据钉（本条够不着它的运行期）。
#[test]
fn tmux_targets_use_exact_match() {
    // ① monitor 侧：一处裸目标都不许再有（本件之后这一侧连命令串都没有了）。
    let mine = guard_core::production_code(include_str!("../../src/bridge/src/tmux.rs"));
    assert!(
        !mine.contains("-t {"),
        "monitor 的 `tmux.rs` 生产段又出现了 `-t {{…}}` —— 那条路已经收干净了"
    );
    // ①b [`exact_target`] 自己产的形状 —— 它今天零生产调用方，但**是 daemon 那条
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
    // ② 那条性质的新住址：daemon 侧抓屏与杀会话**都**过 `exact_target`。
    // 〔搬树 2026-09-17〕后端树从 `remote-daemon-proto/src/` 搬到 `<repo>/src/backend/`
    // ⇒ **中间那层 `src` 没了**。原来是 `.join("src/backend").join("src").join("control")`。
    let daemon = crate::guard_support::backend_src_root().join("control");
    let mut checked = 0usize;
    for (file, why) in [("capture_pane.rs", "抓屏"), ("kill.rs", "杀会话")] {
        let p = daemon.join(file);
        let raw = std::fs::read_to_string(&p)
            .unwrap_or_else(|e| panic!("读不到 {p:?}：{e} —— 本条的被测对象没了，它此刻在空转"));
        let prod = guard_core::production_code(&raw);
        assert!(
            prod.contains("exact_target("),
            "daemon 的 `control/{file}`（{why}）生产段里没有 `exact_target(` —— \n\
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

/// ★★ `KR112D2`：**抓不到的五档分得开**，而且「认不出的码」不许被猜成某一档。
///
/// ⚠〔`K-R112` 09-13〕它替掉的是原来那条按两个哨兵（`NO_TMUX` / `NO_PANE`）
/// 判定的测试。那条测的东西今天**不存在**：
/// 答案不再编码在 stdout 里，所以「屏幕内容恰好等于哨兵串」这个误判形状也随之消失
/// —— 那正是这一刀买到的东西，不是判据被放宽。
#[test]
fn the_five_capture_refusals_stay_apart() {
    let msgs: Vec<String> = [
        "no_tmux",
        "no_server",
        "no_such_session",
        "invalid_args",
        "capture_failed",
    ]
    .iter()
    .map(|c| describe_capture_refusal("cc-x", c, "原话"))
    .collect();
    // ① 五句两两不同 —— 一句都不许被另一句吸收掉（老路把其中三件压成 `NO_PANE` 一句）。
    for (i, a) in msgs.iter().enumerate() {
        for b in msgs.iter().skip(i + 1) {
            assert_ne!(a, b, "两档被压成了同一句话");
        }
    }
    // ② 每一句都说得出是哪个会话，而且带上 daemon 的原话（诊断不许被吃掉）。
    for m in &msgs {
        assert!(m.contains("cc-x"), "没说是哪个会话：{m}");
        assert!(m.contains("原话"), "daemon 的原话被吃掉了：{m}");
    }
    // ③ 🔴 **认不出的码不许猜**：原样带出去，且不许长成任何一句已知档的样子。
    let unknown = describe_capture_refusal("cc-x", "zzz_new_code", "原话");
    assert!(
        unknown.contains("zzz_new_code"),
        "认不出的码没被原样带出去：{unknown}"
    );
    for m in &msgs {
        assert_ne!(
            &unknown, m,
            "认不出的码被猜成了一个已知档 —— 那是拿具体而错误的答案冒充知识"
        );
    }
}

#[test]
fn fmt_uses_real_tab_not_literal_backslash_t() {
    // 回归调研 03 §3.1 坑:格式串里必须是真 TAB 字节,不能是字面 \t。
    assert!(TMUX_LS_FMT.contains('\t'), "格式串须含真 TAB");
    assert!(!TMUX_LS_FMT.contains("\\t"), "格式串不得含字面反斜杠-t");
}

#[test]
fn tmux_ls_fmt_double_write_point_stays_in_sync() {
    // F08a：TMUX_LS_FMT 双写点断言（红线 I8 的机器化护栏）。monitor(本 const) 与 daemon
    // (`src/backend/observe/watcher.rs`) 分属两个独立 crate、不能共享 const，但两侧
    // `tmux ls -F` 格式串**必须逐字一致**（否则 daemon 推的列 monitor 解错位）。编译期
    // include_str! 读 daemon 源，把本 const 的真 TAB 折回源码里的 `\t` 转义再断言 daemon 源
    // 含该带引号字面量——**双向**：改 monitor 或 daemon 任一侧忘同步，本测即红。
    let daemon_src = include_str!(daemon_watcher_src!());
    let source_literal = TMUX_LS_FMT.replace('\t', "\\t");
    // 锚定到 const 定义行（非裸字面量）——否则该字面量若也出现在某条注释里，会掩盖真 const 漂移
    // （假阴性）。daemon 侧常量名同为 TMUX_LS_FMT（红线 I8 不许改），故按定义行精确比对。
    let expected_def = format!("const TMUX_LS_FMT: &str = \"{source_literal}\";");
    assert!(
        daemon_src.contains(&expected_def),
        "TMUX_LS_FMT 双写点漂移：daemon watcher.rs 不含与 monitor 侧一致的定义 {expected_def:?}\n\
             （改了 tmux ls 格式串就得两侧同步——红线 I8）"
    );
}
