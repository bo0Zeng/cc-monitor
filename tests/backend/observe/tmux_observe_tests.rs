//! `observe/tmux_observe.rs` 的测试（随 A 块从 `watcher_tests.rs` 逐字搬来）。

use super::*;

// ---------- P5（zero-poll-liveness）：快照差分 → 正向死亡帧 ----------

use std::collections::BTreeSet;

fn names(v: &[&str]) -> Option<BTreeSet<String>> {
    Some(v.iter().map(|s| s.to_string()).collect())
}

/// ★ `K-R96`：切给那张唯一会话快照的行 —— 名字取第 0 列、`@ccm_sid` 取**末**列。
///
/// 顺带钉住两条：段数不等于 `TMUX_LS_FMT_FIELDS` 的行**整行丢掉**（下溢是通道被改写、
/// 过溢是有人往 `@ccm_sid` 里塞了 TAB —— 两种都不许当好数据）；`NO_TMUX` 哨兵不是会话。
#[test]
fn session_rows_carry_the_name_and_the_ccm_sid_and_nothing_else() {
    use crate::common::session_snapshot::SessionRow;
    let raw = "s1\t/p\tclaude\t1\t2\tsid-a\ns2\t/q\tbash\t0\t1\t\n";
    assert_eq!(
        session_rows(raw),
        vec![
            SessionRow {
                name: "s1".into(),
                ccm_sid: "sid-a".into()
            },
            SessionRow {
                name: "s2".into(),
                ccm_sid: String::new()
            },
        ]
    );
    assert!(session_rows("NO_TMUX\n").is_empty(), "哨兵不是会话");
    assert!(session_rows("只有一段\n").is_empty(), "下溢的行不当好数据");
    assert!(
        session_rows("s\t/p\tc\t1\t2\tsid\t多出来一段\n").is_empty(),
        "过溢的行不当好数据（`last()` 那种写法会在这里取到半截）"
    );
}

/// ★★ `K-R96` 死值验（observe 这一侧）：**观测无效时快照一个字都不许动。**
///
/// 把 `NoTmux`/`Unobservable` 那一支改成 `publish(Vec::new())`（= 「都没了」），
/// 本条当场红 —— 那正是「观测失败被读成零会话，把活会话全部误 retire」的那一下，
/// 只不过这一回它会顺着快照传染到 control 侧的判活。
#[test]
fn an_invalid_observation_leaves_the_shared_snapshot_untouched() {
    let snap = crate::common::session_snapshot::SessionSnapshot::with_prober(|| {
        panic!("本条一次都不该去探 —— 它量的是 `publish` 那一侧")
    });
    let mut prev: Option<BTreeSet<String>> = None;
    // 先让快照里有点东西（走 `Sessions` 那一支发布）。
    let _ = diff_closed_into(
        &mut prev,
        &TmuxObservation::Sessions("keep-cc\t/p\tclaude\t1\t1\tsid-k\n".into()),
        &snap,
    );
    let warmed = snap.peek();
    assert!(
        warmed.iter().any(|r| r.name == "keep-cc"),
        "`Sessions` 那一支没往快照里发布（实得 {warmed:?}）—— 本条此刻在空转"
    );
    for obs in [TmuxObservation::NoTmux, TmuxObservation::Unobservable] {
        let mut p = prev.clone();
        assert!(diff_closed_into(&mut p, &obs, &snap).is_empty());
        assert_eq!(
            snap.peek(),
            warmed,
            "观测无效那一支动了共享快照 —— 「不知道」被写成了「都没了」"
        );
    }
    // 而「server 没了」是**有效观测**：那一支必须把表清空（不是「不知道」）。
    let mut p = prev.clone();
    let _ = diff_closed_into(&mut p, &TmuxObservation::NoServer, &snap);
    assert!(
        snap.peek().is_empty(),
        "server 没了却还在表里留着会话 —— 判活会把它们报成活的"
    );
}

#[test]
fn session_names_takes_first_column_only() {
    let raw = "s1\t/p\tclaude\t1\t2\tsid-a\ns2\t/q\tbash\t0\t1\t\n";
    assert_eq!(
        session_names(raw),
        ["s1", "s2"].iter().map(|s| s.to_string()).collect()
    );
}

#[test]
fn session_names_ignores_blank_lines_and_no_tmux_sentinel() {
    assert!(session_names("\n\n").is_empty());
    assert!(session_names("NO_TMUX\n").is_empty());
}

#[test]
fn diff_reports_only_the_disappeared_one() {
    let mut prev = names(&["a", "b", "c"]);
    let closed = diff_closed(
        &mut prev,
        &TmuxObservation::Sessions("a\t/p\tsh\t0\t1\t\nc\t/p\tsh\t0\t1\t\n".into()),
    );
    assert_eq!(closed, vec!["b".to_string()]);
    assert_eq!(prev, names(&["a", "c"]));
}

/// 信号会合并 ⇒ 一次差分要能报出**所有**消失的（逐事件必漏）。
#[test]
fn diff_reports_all_disappeared_at_once() {
    let mut prev = names(&["a", "b", "c", "d"]);
    let closed = diff_closed(
        &mut prev,
        &TmuxObservation::Sessions("b\t/p\tsh\t0\t1\t\n".into()),
    );
    assert_eq!(
        closed,
        vec!["a".to_string(), "c".to_string(), "d".to_string()]
    );
}

#[test]
fn server_gone_closes_everything() {
    let mut prev = names(&["a", "b"]);
    let closed = diff_closed(&mut prev, &TmuxObservation::NoServer);
    assert_eq!(closed, vec!["a".to_string(), "b".to_string()]);
    assert_eq!(prev, Some(BTreeSet::new()));
}

/// ★ 最要紧的一条：**观测失败 ≠ 都没了**。
/// 报一堆死亡帧会把活着的会话全部误 retire —— 这正是 P1 当年那条
/// 「空 `raw` 同时意味着零会话和出错」的教训在死亡帧这条路上的复发点。
#[test]
fn unobservable_never_reports_deaths_and_keeps_snapshot() {
    for obs in [TmuxObservation::Unobservable, TmuxObservation::NoTmux] {
        let mut prev = names(&["a", "b"]);
        assert!(
            diff_closed(&mut prev, &obs).is_empty(),
            "{obs:?} 不该报死亡"
        );
        assert_eq!(prev, names(&["a", "b"]), "{obs:?} 不该动快照");
    }
}

/// 第一次观测没有「上一份」可比 ⇒ 不报任何死亡（否则后端一启动就诬告一批）。
#[test]
fn first_observation_reports_nothing() {
    let mut prev = None;
    let closed = diff_closed(
        &mut prev,
        &TmuxObservation::Sessions("a\t/p\tsh\t0\t1\t\n".into()),
    );
    assert!(closed.is_empty());
    assert_eq!(prev, names(&["a"]));
}

/// 幂等：同一份观测再来一次，不该重复报死亡。
#[test]
fn repeated_identical_observation_reports_nothing() {
    let mut prev = names(&["a"]);
    let obs = TmuxObservation::Sessions("a\t/p\tsh\t0\t1\t\n".into());
    assert!(diff_closed(&mut prev, &obs).is_empty());
    assert!(diff_closed(&mut prev, &obs).is_empty());
}

/// 新会话出现不该被当成死亡（差分方向别搞反）。
#[test]
fn new_session_is_not_a_death() {
    let mut prev = names(&["a"]);
    let closed = diff_closed(
        &mut prev,
        &TmuxObservation::Sessions("a\t/p\tsh\t0\t1\t\nb\t/p\tsh\t0\t1\t\n".into()),
    );
    assert!(closed.is_empty());
    assert_eq!(prev, names(&["a", "b"]));
}

// ---------- P3（zero-poll-liveness）：tmux server 生 / 死 / 复活 ----------

/// ★ P3 收紧判据：`tmux ls` rc=1 时，**只有在我们记着的 server pid 确实已经不在**
/// 才认"零会话"；pid 还在 = 真异常 ⇒ `Unobservable`（保守跳过，不误 retire）。
///
/// **刻意不依赖"pidfd 是否已经醒过"**——那会有个危险失效模式：pidfd 路万一没醒，
/// 状态永停 `Alive`，rc=1 被永久压成 `Unobservable` ⇒ 永不 retire。改成直接查 `/proc`。
#[test]
fn no_server_is_tightened_only_when_the_pid_is_really_still_alive() {
    // ① 记着的 server 是**自己**（铁定活着）+ rc=1 ⇒ 真异常 ⇒ Unobservable
    let me = std::process::id();
    assert_eq!(
        classify_with_server_state(TmuxObservation::NoServer, ServerState::Alive(me)),
        TmuxObservation::Unobservable,
        "server 明明活着而 tmux ls 连不上 = 真异常，不该当成零会话"
    );

    // ② 记着的 server 已死 + rc=1 ⇒ 原样通过（真的没 server）
    let mut child = std::process::Command::new("sleep")
        .arg("30")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .expect("spawn");
    let dead = child.id();
    child.kill().expect("kill");
    child.wait().expect("reap");
    assert_eq!(
        classify_with_server_state(TmuxObservation::NoServer, ServerState::Alive(dead)),
        TmuxObservation::NoServer,
        "server 真没了就该照常判零会话"
    );

    // ③ Unknown / Gone 一律不收紧（还没探过、或已知没了）
    for st in [ServerState::Unknown, ServerState::Gone] {
        assert_eq!(
            classify_with_server_state(TmuxObservation::NoServer, st),
            TmuxObservation::NoServer,
            "{st:?} 下不该收紧"
        );
    }

    // ④ **收紧只作用于 NoServer**（守卫范围必须等于性质范围）：别的观测原样穿过，
    //    尤其 `ServerEmpty`（exit-empty off 下 server 活着 + 零会话，是合法观测）。
    for obs in [
        TmuxObservation::Sessions("x".into()),
        TmuxObservation::ServerEmpty,
        TmuxObservation::NoTmux,
        TmuxObservation::Unobservable,
    ] {
        assert_eq!(
            classify_with_server_state(obs.clone(), ServerState::Alive(me)),
            obs,
            "{obs:?} 不该被 server 状态改写"
        );
    }
}

/// `query_tmux_server`：**死 socket 上不该把 server 拉活**、且拿不到 pid/socket。
///
/// 这里不能直接调 `query_tmux_server()`（它走默认 socket = 用户实况），所以只钉住
/// 「探测脚本对没有 server 的情形返回全 None」这条语义——用 PATH 前置一个 rc=1 的假 tmux。
#[test]
fn tmux_server_query_yields_nothing_without_a_server() {
    let dir = std::env::temp_dir().join(format!("ccm-p3-q-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("mkdir");
    let fake = dir.join("tmux");
    std::fs::write(&fake, "#!/bin/sh\necho 'error connecting' >&2\nexit 1\n").expect("write");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perm = std::fs::metadata(&fake).expect("stat").permissions();
        perm.set_mode(0o755);
        std::fs::set_permissions(&fake, perm).expect("chmod");
    }
    // 跑与生产同一段脚本，只把 PATH 指向假 tmux。
    let script = "if command -v tmux >/dev/null 2>&1; then exec tmux display-message -p '#{pid}\t#{socket_path}' 2>/dev/null; else exit 97; fi";
    let out = std::process::Command::new("/bin/sh")
        .arg("-c")
        .arg(script)
        .env("PATH", dir.display().to_string())
        .output()
        .expect("spawn");
    assert_ne!(out.status.code(), Some(0), "没有 server 时脚本不该 rc=0");
    assert!(
        String::from_utf8_lossy(&out.stdout).trim().is_empty(),
        "没有 server 时不该有 stdout"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

// ---------- P1（zero-poll-liveness）：tmux 观测四态 ----------

/// 纯分类：四态各自的判据。**P0 实测的状态空间**（见
/// `.claude/planned-build/zero-poll-liveness/features/P0-machine-facts.md` §3 ④）。
#[test]
fn tmux_probe_classifies_four_states() {
    // rc=0 + 非空 → 有会话
    assert_eq!(
        classify_tmux_probe(Some(0), "s1\t/p\tclaude\t1\t1\tsid-a\n"),
        TmuxObservation::Sessions("s1\t/p\tclaude\t1\t1\tsid-a\n".to_string())
    );
    // rc=0 + 空 → **server 活但零会话**（exit-empty off）。P3 起与 rc=1 分开。
    assert_eq!(
        classify_tmux_probe(Some(0), ""),
        TmuxObservation::ServerEmpty
    );
    assert_eq!(
        classify_tmux_probe(Some(0), "  \n"),
        TmuxObservation::ServerEmpty,
        "只有空白也算空"
    );
    // rc=1 → **server 不在**（两种 stderr 措辞都走这里，刻意不看 stderr）
    assert_eq!(classify_tmux_probe(Some(1), ""), TmuxObservation::NoServer);
    // 约定 rc → 无 tmux
    assert_eq!(
        classify_tmux_probe(Some(TMUX_PROBE_NO_TMUX_RC), ""),
        TmuxObservation::NoTmux
    );
    // 其他 rc / 被信号杀 → 观测无效（**绝不当零会话**）
    assert_eq!(
        classify_tmux_probe(Some(2), ""),
        TmuxObservation::Unobservable
    );
    assert_eq!(
        classify_tmux_probe(Some(127), ""),
        TmuxObservation::Unobservable
    );
    assert_eq!(classify_tmux_probe(None, ""), TmuxObservation::Unobservable);
}

/// ★★ **K-R12 `J1` 死值验：让段数真的下溢一次，它必须红。**
///
/// # 这一条钉的到底是什么
///
/// 不是「有没有那行 `if`」，是**下溢那一档的处置**。今天（改之前）rc=0 + 非空 ⇒ 一律
/// `Sessions(raw)`，于是脏输入会被**当成好数据往下游送**，而下游的伤害是最大的那一档：
/// [`session_names`] 只取第 1 段、永远取得到 ⇒ 它把**整行**当会话名
/// ⇒ [`diff_closed`] 下一轮把**所有真会话**算成「消失了」⇒ 一批活着的会话被 retire。
/// ⇒ 所以本条第二段量的是**那个后果**，不是那个分支。
///
/// # 死值从哪来（不是我编的）
///
/// `DIRTY` 是 09-04 在**零挂载容器**里对真 tmux 3.4 私有 socket 打出来的字节
/// （`tests/evidence/K-R12-deathvalue.md` ①/S5，`od -c` 逐字节复核）：POSIX 客户端下
/// 六个真 TAB 全变 `_`，连 `文档`（3 字节/字）都按**显示宽度**变成了 `____`。
/// `CLEAN` 是同一台 server、同一条命令、只加了本拍那条口径之后的输出（同文件 ②/S5）。
///
/// # 正对照不能省 —— 两个方向都要钉
///
/// 只钉「脏的被拒」的话，把 `classify` 焊死成「永远 Unobservable」也能绿。所以：
/// ① 干净的 6 段必须照常 `Sessions`；② **7 段（过溢）也必须照常 `Sessions`** ——
/// 那是**合法内容**（cwd 里带真 TAB 的会话，实测切出 7 段），
/// 判据写成 `!= 6` 就会在这里误伤。**这一格就是「下溢而不是不等于」那个选择的死值。**
#[test]
fn a_dirty_tmux_channel_is_unobservable_never_sessions() {
    // 真 tmux 3.4 + POSIX 客户端打出来的字节（见头注）。六列塌成 1 段。
    const DIRTY: &str = "kr12_/tmp/kr12dv/____/proj_bash_0_1_cc-deadval1\n";
    // 同一台 server、加了 `-u`/`LC_ALL` 之后的同一行。
    const CLEAN: &str = "kr12\t/tmp/kr12dv/文档/proj\tbash\t0\t1\tcc-deadval1\n";
    // 合法的**过溢**：cwd 里有一个真 TAB ⇒ 7 段。`!= 6` 会误伤它，`< 6` 不会。
    const OVERFLOW: &str = "kr12\t/tmp/a\tb\tbash\t0\t1\tcc-deadval1\n";

    assert_eq!(
        classify_tmux_probe(Some(0), DIRTY),
        TmuxObservation::Unobservable,
        "通道脏（段数下溢）必须判观测无效；判成 Sessions 就是把垃圾当好数据送下游"
    );
    assert!(
        matches!(classify_tmux_probe(Some(0), CLEAN), TmuxObservation::Sessions(ref s) if s == CLEAN),
        "正对照：干净的六段必须照常放行，否则买到的是「门坏了」而不是「门对了」"
    );
    assert!(
        matches!(
            classify_tmux_probe(Some(0), OVERFLOW),
            TmuxObservation::Sessions(_)
        ),
        "过溢是**合法内容**（cwd 里带真 TAB）⇒ 必须放行。这一格钉的是「下溢」而不是「不等于 6」"
    );

    // ── 第二段：量**后果**，不是量分支 ────────────────────────────────
    // 先用一份干净观测建立快照，再喂一份脏的，断言**一个会话都没被报死**。
    let mut prev = None;
    let first = "s1\t/p\tclaude\t1\t1\tsid-a\ns2\t/q\tbash\t0\t1\t\n";
    let closed = diff_closed(&mut prev, &classify_tmux_probe(Some(0), first));
    assert!(closed.is_empty(), "第一次观测不该报任何死亡");
    assert_eq!(
        prev.as_ref().map(|s| s.len()),
        Some(2),
        "快照该记住两个会话"
    );

    let dirty_two = "s1_/p_claude_1_1_sid-a\ns2_/q_bash_0_1_\n";
    let closed = diff_closed(&mut prev, &classify_tmux_probe(Some(0), dirty_two));
    assert!(
        closed.is_empty(),
        "🔴 通道一脏就把**全部活会话**报成消失 —— 这才是本件真正的伤害。实得：{closed:?}"
    );
    assert_eq!(
        prev.as_ref().map(|s| s.len()),
        Some(2),
        "观测无效时快照必须原样保留，等下一次成功观测"
    );
}

/// K-R12 `J1` 的判据本体：**下溢红、恰好绿、过溢绿**。
///
/// 与上一条分开写，是因为上一条量的是「处置对不对」，这一条量的是「那条不等号的方向」。
///
/// 要求住址：`INVARIANTS §49`（tmux 打印通道必须是 UTF-8，段数下溢出声）。
#[test]
fn the_underflow_predicate_only_fires_downward() {
    assert!(
        tab_underflow("一段而已", TMUX_LS_FMT_FIELDS),
        "1 < 6 ⇒ 下溢"
    );
    assert!(
        tab_underflow("a\tb\tc\td\te", TMUX_LS_FMT_FIELDS),
        "5 < 6 ⇒ 下溢"
    );
    assert!(
        !tab_underflow("a\tb\tc\td\te\tf", TMUX_LS_FMT_FIELDS),
        "恰好 6 ⇒ 不红"
    );
    assert!(
        !tab_underflow("a\tb\tc\td\te\tf\tg", TMUX_LS_FMT_FIELDS),
        "7 段是合法内容（路径里有真 TAB）⇒ **不许红**，否则就成了 `!= 6` 那个误伤"
    );
    assert!(
        tab_underflow("15_/tmp/x/sock", 2),
        "query_tmux_server 那条 N=2 的同理"
    );
    assert!(!tab_underflow("15\t/tmp/x/sock", 2));
}

/// ★★ **K-R12 下一拍（09-04）：本模块每一处起 `sh` 的地方都必须挂上 UTF-8 那个 env
/// —— 一处都不许漏。**
///
/// # 🔴 它补的是上一拍留下的一个真洞（现打）
///
/// 上一拍把 `.env(…)` 挂上去了，却**没有任何判据看得见它**：
/// 全仓（两棵树）搜 `TMUX_UTF8_ENV` / `LC_ALL` 的命中，除了本文件生产段那三行之外
/// **一条都不在测试段里**。⇒ 那时把其中一处的 `.env(…)` 删掉，
/// cargo 一条都不会红（两处都删掉才会有个 unused 的**告警**，而本仓不 deny warnings）
/// ⇒ **静默回到病态**，而这一件治的就是静默。
///
/// 上一拍在 `control/gate.rs` 那侧装了同职的一条（`both_tmux_call_sites_…`），
/// **这一侧漏了** —— 正是「只覆盖了那条病的一个动词」那一族。
///
/// # 它守的是「每一处」，不是「有没有」
///
/// 逐处查（不只比总数）：从每一个起 `sh` 的地方到它那句 `.output()` 之间
/// 必须出现那行 `.env(…)`。只比总数的话，「一处挂了两遍、另一处零」照样绿。
///
/// # 🔴 `K-R55`（09-11）：**锚点换了** —— 起 `sh` 这一跳搬进了适配层
///
/// 上一版的锚点是本模块里的裸 `Command::new("sh")`。那两处今天住在
/// [`crate::platform::shell::posix_shell`]（`K33` 裁定二：平台差异只许住适配层），
/// 本模块留下的是**调用点** ⇒ 锚点跟着换成 `posix_shell(`。
/// ⚠ **换锚点不是把红的那条删掉了事**（`guard_support` 那条纪律）：
/// 本条要守的性质一个字没变 —— 「本模块每一处起 `sh` 的地方都挂了那个 env」，
/// 而挂 env 的仍然是**这一侧**（适配层只负责备命令，不碰 env）。
/// 🔴 它因此**没有**跟着搬走：`posix_shell` 有第二个使用者的那天，
/// 那一处的 env 归那一处自己管，本条**看不见它** —— 如实登记，别读宽。
///
/// 🔴 **本条守的是「别漏」，不是「它真的生效了」**（「盘上有 ≠ 被走到」）。
/// 行为那一半的死值在 `tests/evidence/K-R12-deathvalue.md`：同样这两条脚本对真 tmux 3.4
/// 私有 socket 打过，改前段数 1、改后各回各的 N。
///
/// 要求住址：`INVARIANTS §49`（tmux 打印通道必须是 UTF-8，段数下溢出声）。
#[test]
fn every_sh_call_site_in_this_module_carries_the_utf8_env() {
    // A 块搬出之后，「本模块」仍是原 `watcher.rs` 那一份 = 两份生产段拼起来（人群不缩，
    // 下面的登记值与地板一个不动）；起 `sh` 的两处今天都住 `tmux_observe.rs`。
    let prod = crate::guard_support::production_code(include_str!(
        "../../../src/backend/observe/watcher.rs"
    ));
    crate::guard_support::assert_no_test_code("observe/watcher.rs", &prod);
    let moved = crate::guard_support::production_code(include_str!(
        "../../../src/backend/observe/tmux_observe.rs"
    ));
    crate::guard_support::assert_no_test_code("observe/tmux_observe.rs", &moved);
    let prod = format!("{prod}{moved}");
    // 非空对照：剥过头 / 没读到 ⇒ 下面全是 0 == 0 的空真。
    // 地板 = 实测值的一半（09-04 现打 41_149 字节）。
    assert!(
        prod.len() > 20_000,
        "生产段只有 {} 字节 —— 没读到或剥过头，本条此刻在空转",
        prod.len()
    );
    /// 本模块起 `sh` 的处数 —— **登记值**。这张表不是豁免清单：
    /// 新增一处 ⇒ 它也要挂 env，并把这个数一起改。
    const SH_CALL_SITES: usize = 2;
    let starts = prod.matches("posix_shell(").count();
    assert_eq!(
        starts, SH_CALL_SITES,
        "本模块起 `sh` 的处数变了（实得 {starts}，登记 {SH_CALL_SITES}）—— \
             新增的那一处也要挂 UTF-8 那个 env（家在 `common::tmux_utf8`），\
             并把这条判据的数一起改。**这张表不是豁免清单。**"
    );
    let env_call = format!(".env({}.0, {}.1)", "UTF8_CLIENT_ENV", "UTF8_CLIENT_ENV");
    // 逐处查：每个调用点到它那句 `.output()` 之间必须有那行 `.env(…)`。
    let mut checked = 0usize;
    for seg in prod.split("posix_shell(").skip(1) {
        let head = seg.split(".output()").next().unwrap_or(seg);
        assert!(
            head.contains(&env_call),
            "第 {} 处起 `sh` 的地方没挂 `{env_call}` —— 那一处的 tmux 客户端会退回\
                 非 UTF-8，输出里的 TAB 与非 ASCII 全变 `_`，而 rc 仍是 0、\
                 段数下溢那条只会把整趟观测判成「观测无效」：**看起来像远端没事**。\n\
                 这一段是：{head:?}",
            checked + 1
        );
        checked += 1;
    }
    assert_eq!(
        checked, SH_CALL_SITES,
        "逐处查只走到 {checked} 处 —— 切法坏了，上面那条等号是空转的"
    );
    // 反向：env 那一行不许被换成「往脚本串里插旗」。那一改会让 `query_tmux_server`
    // 的脚本与测试里那份逐字复制漂开，而 `tmux_probe_script` 的两条 `exec` 分支
    // 也会退回「要改两处、漏一处永远看不见」。
    assert!(
        !prod.contains("tmux -u ") && !prod.contains(" -u ls "),
        "本模块的脚本串里出现了 argv 形的旗 —— 这一侧按调用点形态该用 env 形，\
             理由（两条 `exec` 分支 + 那份逐字复制）在文件上方那段注释里"
    );
}

/// 四态观测 → 会话账本读的那两格（原 `tmux_sessions` 帧载荷，那一帧不再上线）：有会话 ⇒ 原文、不带取值；
/// P3 那两个细分（`ServerEmpty` / `NoServer`）**必须落同一个取值**（对收割完全等价）；没装 tmux / 观测无效各有自己的取值，
/// 账本据此「不知道」绝不当成「都没了」。
#[test]
fn observation_parts_keep_the_four_states_apart_for_the_ledger() {
    assert_eq!(
        observation_parts(TmuxObservation::Sessions("s1\t/p\tclaude\t1\t1\tx".into())),
        ("s1\t/p\tclaude\t1\t1\tx".to_string(), None)
    );
    for (obs, token) in [
        (TmuxObservation::ServerEmpty, OBS_ZERO_SESSIONS),
        (TmuxObservation::NoServer, OBS_ZERO_SESSIONS),
        (TmuxObservation::NoTmux, OBS_NO_TMUX),
        (TmuxObservation::Unobservable, OBS_UNOBSERVABLE),
    ] {
        let (raw, o) = observation_parts(obs);
        assert_eq!((raw.as_str(), o), ("", Some(token)));
        assert_eq!(tmux_view_is_observable(&raw, o), token == OBS_ZERO_SESSIONS);
    }
}

/// ★ **真跑那段 shell 脚本**（拿假 tmux 喂各种 rc），不只做字符串断言。
///
/// 为什么必须这样测：P1 的关键改动是把 `tmux ls … || true` 换成 `exec tmux …` 让 rc
/// 透出。`|| true` 与 `exec` 的差别**在字符串断言里看不出来**——只有真执行才知道 rc
/// 有没有传出来。（同 `control/ccm/plan.rs::render_container` 那一族的教训：
/// 门禁只锁字符串形状不锁行为。⚠ 这句话点名的活体**换过两次**：`K-R72` 09-12 之前指
/// `tmux.rs` 那一份（随桌面侧 SSH 回落一起走了），之后指用量探针那份
/// （`K-R104` 09-13 随编排搬上帧面一起走了）—— **教训没变，每次换指今天真在的那个。**）
#[test]
fn probe_script_propagates_rc_with_fake_tmux() {
    let dir = std::env::temp_dir().join(format!("ccm-p1-probe-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("mkdir");
    let fake = dir.join("tmux");

    let run = |path_value: &str| -> TmuxObservation {
        let out = std::process::Command::new("/bin/sh")
            .arg("-c")
            .arg(tmux_probe_script())
            .env("PATH", path_value)
            .output()
            .expect("spawn /bin/sh");
        classify_tmux_probe(out.status.code(), &String::from_utf8_lossy(&out.stdout))
    };
    let write_fake = |body: &str| {
        std::fs::write(&fake, body).expect("write fake tmux");
        let mut perm = std::fs::metadata(&fake).expect("stat").permissions();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            perm.set_mode(0o755);
        }
        std::fs::set_permissions(&fake, perm).expect("chmod");
    };
    let path_with_fake = format!("{}:/usr/bin:/bin", dir.display());

    // ① 假 tmux 打印一行会话、rc=0 → Sessions
    write_fake("#!/bin/sh\nprintf 's1\\t/p\\tclaude\\t1\\t1\\tsid-a\\n'\nexit 0\n");
    assert!(matches!(
        run(&path_with_fake),
        TmuxObservation::Sessions(ref s) if s.contains("sid-a")
    ));

    // ② rc=0 但不输出 → ServerEmpty（exit-empty off 那格）
    write_fake("#!/bin/sh\nexit 0\n");
    assert_eq!(run(&path_with_fake), TmuxObservation::ServerEmpty);

    // ③ rc=1（真 tmux 在 server 不在时就是这个）→ ZeroSessions
    //    **这一格是 P1 的核心**：改回 `|| true` 会让它变成 rc=0+空 ⇒ 仍是 ZeroSessions，
    //    所以本格单独看不出回归；真正钉住 `exec` 的是 ④。
    write_fake("#!/bin/sh\necho 'no server running on /tmp/x' >&2\nexit 1\n");
    assert_eq!(run(&path_with_fake), TmuxObservation::NoServer);

    // ④ ★ rc=2（观测无效）→ 必须是 Unobservable，**绝不能被折成零会话**。
    //    这一格就是 `|| true` 的变异检测点：加回 `|| true` 会把 rc=2 吞成 rc=0+空
    //    ⇒ 误判成 ZeroSessions ⇒ 本断言红。
    write_fake("#!/bin/sh\necho boom >&2\nexit 2\n");
    assert_eq!(
        run(&path_with_fake),
        TmuxObservation::Unobservable,
        "观测失败被折成零会话会批量误灰——这里红说明 rc 没有真的透出来"
    );

    // ⑤ PATH 里没有 tmux → NoTmux（command -v 门控）。
    //    **必须用一个确实没有 tmux 的空目录**：初版这里写的是 `/usr/bin:/bin`，而真 tmux
    //    就在 `/usr/bin` ⇒ 测试真跑了**默认 socket** 上的 `tmux ls`（只读、无损，但违反
    //    "tmux 一律走隔离 socket"的纪律，且在别人机器上结果不可预测）。断言当场红是因为
    //    它列出了真实会话而不是 NoTmux —— 算这条测试自己抓到的第一个问题。
    let empty = dir.join("no-tmux-here");
    std::fs::create_dir_all(&empty).expect("mkdir empty");
    assert_eq!(
        run(&empty.display().to_string()),
        TmuxObservation::NoTmux,
        "PATH 里没有 tmux 时必须走 command -v 门控那支"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

/// ★ `P0b-Y2` 第十六拍：**socket 目录按 `TMUX_TMPDIR` 推，不许硬编码 `/tmp`。**
///
/// # 为什么这条是单测而不是 e2e
///
/// 它是个**纯函数**（env → 路径），单测才是对的 acceptor。
/// ⚠ 第一版把它写进 e2e 的第三格，逼得那个套件自己设 `TMUX_TMPDIR` ——
/// 而 `C7i` **零例外**禁止 e2e 靠它做隔离，`e2e_gate_registry` 那条守卫当场拦下。
/// **它报得对**：判据要钉的性质与套件要用的隔离手段撞在同一个变量上时，
/// 该换的是**判据落在哪一层**，不是给红线开例外。
///
/// ⚠ 硬编码 `/tmp` 的后果是**静默失效**：在设了 `TMUX_TMPDIR` 的机器上，
/// backend 会去监视一个永远不会有动静的目录 —— 与修之前一模一样，且没有任何错误。
#[test]
fn tmux_socket_dir_follows_tmux_tmpdir() {
    // ⚠ env 是进程全局的：设完必须还原，否则会污染同进程里别的测试。
    let saved = std::env::var_os("TMUX_TMPDIR");
    // SAFETY: 单线程内设/取环境变量；本测试跑完立即还原。
    unsafe { std::env::set_var("TMUX_TMPDIR", "/x/y") };
    let d = tmux_socket_dir();
    unsafe {
        match &saved {
            Some(v) => std::env::set_var("TMUX_TMPDIR", v),
            None => std::env::remove_var("TMUX_TMPDIR"),
        }
    }
    let s = d.to_string_lossy();
    assert!(
        s.starts_with("/x/y/tmux-"),
        "socket 目录没跟着 `TMUX_TMPDIR` 走（实得 {s}）—— \
             硬编码 `/tmp` 会让后端监视一个永远没动静的目录，且**没有任何错误**。"
    );
    assert!(
        !s.starts_with("/tmp/"),
        "socket 目录仍落在 `/tmp` 下（实得 {s}）"
    );
}

/// ★★ **超时必须落成「观测无效」，绝不能落成「零会话」**。
///
/// 这是本件最要命的一格：`ServerEmpty`（rc=0 且 stdout 空）会让上层认为
/// **那台机器上一个会话都没有** ⇒ 活着的会话被 retire。
/// 而超时是「**我没看清**」，不是「**我看清了，是空的**」。
///
/// `timeout -s KILL` 杀掉子进程后 rc 是 137（128+9）；有些实现/路径下是 124；
/// 被信号直接杀时 `code` 是 `None`。三种都必须落 `Unobservable`。
#[test]
fn a_timed_out_probe_is_unobservable_never_zero_sessions() {
    for code in [Some(124), Some(137), None] {
        let got = classify_tmux_probe(code, "");
        assert!(
            matches!(got, TmuxObservation::Unobservable),
            "rc={code:?} 被判成了 {got:?} —— 超时是「我没看清」，不是「我看清了，是空的」。\n\
                 判成 ServerEmpty 会让上层认为那台机器零会话 ⇒ **活着的会话被 retire**。"
        );
    }
    // 对照：真正的「server 在、但零会话」仍然要判 ServerEmpty（防把上面写成恒真）。
    assert!(
        matches!(
            classify_tmux_probe(Some(0), ""),
            TmuxObservation::ServerEmpty
        ),
        "rc=0 且空 stdout 该是 ServerEmpty —— 上面那条不许把它一起吞了"
    );
}

/// 探测脚本必须**带上界**，且 `timeout` 缺席时诚实退回〔audit-0805 F09，承接 C7〕。
#[test]
fn the_tmux_probe_is_bounded_and_degrades_honestly() {
    let script = tmux_probe_script();
    assert!(
        script.contains("tmux ls"),
        "抽取器自检：脚本里连 `tmux ls` 都没有 —— 拿错东西了：{script}"
    );
    assert!(
        script.contains("timeout"),
        "★ 探测没有上界。`run_tmux_ls` 的 `output()` 无超时，而 `watch_loop` 的 `tmux_inflight`\n\
             只在收到 `TmuxObserved` 时清 —— 探测永不返回 ⇒ 标志永远为真 ⇒ **此后一次 tmux 探测\n\
             都不会再发起，且不发任何理由帧**（报告 I-2）。实得：{script}"
    );
    assert!(
        script.contains("command -v timeout"),
        "★ `timeout` 必须门控。硬用它会在没有 coreutils 的系统上让整条探测直接失败 ——\n\
             那是把一个「偶发卡死」换成「必然不可用」。要诚实降级（C7），不是赌它存在。实得：{script}"
    );
}

/// `tmux-list` 的四态折叠：没装 ≠ 零会话 ≠ 看不清（`list_remote_tmux` 头注那三档，搬到这一侧）。
/// 要求：`INVARIANTS §49`「下溢必须出声 ＋ 这一行不许当好数据」·「`list_remote_tmux` 改后端新帧命令 `tmux-list`」。
#[test]
fn the_tmux_list_query_keeps_not_installed_empty_and_unobservable_apart() {
    assert_eq!(query_reply(TmuxObservation::NoTmux), Ok((false, vec![])));
    assert_eq!(query_reply(TmuxObservation::NoServer), Ok((true, vec![])));
    assert_eq!(
        query_reply(TmuxObservation::ServerEmpty),
        Ok((true, vec![]))
    );
    assert_eq!(
        query_reply(TmuxObservation::Sessions("a\tb\n".to_string() + "c\td")),
        Ok((true, vec!["a\tb".to_string(), "c\td".to_string()]))
    );
    assert!(
        query_reply(TmuxObservation::Unobservable).is_err(),
        "看不清绝不当成零会话"
    );
}
