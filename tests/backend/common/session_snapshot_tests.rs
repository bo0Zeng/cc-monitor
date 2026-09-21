use super::*;

fn row(name: &str, sid: &str) -> SessionRow {
    SessionRow {
        name: name.to_string(),
        ccm_sid: sid.to_string(),
    }
}

/// ★★ **`KR96D1` 死值验第二刀：读快照必须触发更新，拿到的值是「这一刻的」。**
///
/// 🔴 **失效方向（件文件逐字）：只判「有没有调那个快照函数」。**
/// 那种判据在「`query` 被改成先看缓存」时**照样绿** —— 而那正是本件要挡的那一下。
/// ⇒ 这里的夹具是一个**会变的世界**：探测器第 1 次回 A、第 2 次回 B。
/// 判的是**第二次问到的是 B**，不是「问了」。
#[test]
fn asking_the_snapshot_refreshes_it_so_the_answer_is_from_this_moment() {
    let calls = std::sync::atomic::AtomicUsize::new(0);
    let snap = SessionSnapshot::with_prober(move || {
        let n = calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        Ok(vec![row(&format!("world-{n}-cc"), "sid-x")])
    });

    let first = snap.query().expect("第一次问得到");
    assert_eq!(first, vec![row("world-0-cc", "sid-x")]);

    // 世界变了（探测器的下一次回的是另一份）。**没有任何人再调 publish**。
    let second = snap.query().expect("第二次问得到");
    assert_eq!(
        second,
        vec![row("world-1-cc", "sid-x")],
        "第二次问回来的还是第一次那份 —— `query` 退化成了「读缓存」。\n\
             ★ `R52` 裁定一允许 Gate 读快照的**前提**就是「问一次、更新一次」；\n\
               读陈值判活 = 把一个刚死的会话报成活的。"
    );
}

/// ★★ **焐热不等于问过了** —— `publish` 进去的值，`query` 不许交出去。
///
/// 这一条挡的是另一种改法：「`query` 先看有没有人 publish 过，有就直接回」。
/// 它比上一条更隐蔽 —— 有 watcher 在跑的时候那个值**看起来**是新的
/// （watcher 每次观测都焐一遍），于是单测里一切正常，
/// 而 watcher 观测不到（`NoTmux` / `Unobservable`）的那些时刻它就是陈的。
#[test]
fn a_warmed_value_is_never_what_the_query_hands_back() {
    let snap = SessionSnapshot::with_prober(|| Ok(vec![row("probed-cc", "sid-p")]));
    snap.publish(vec![row("warmed-cc", "sid-w")]);
    assert_eq!(snap.peek(), vec![row("warmed-cc", "sid-w")], "焐进去了");
    assert_eq!(
        snap.query().expect("问得到"),
        vec![row("probed-cc", "sid-p")],
        "`query` 把 `publish` 焐进来的那份交了出去 —— 那是陈值。"
    );
    assert_eq!(
        snap.peek(),
        vec![row("probed-cc", "sid-p")],
        "问完之后焐着的那份也该是刚探到的（同一张表，不是两份）"
    );
}

/// ★ `taken_names` 是 `query` 的投影 ⇒ 它同样会触发更新（避让不许拿陈名单）。
#[test]
fn the_taken_name_table_is_also_refreshed_by_the_asking() {
    let calls = std::sync::atomic::AtomicUsize::new(0);
    let snap = SessionSnapshot::with_prober(move || {
        let n = calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        Ok((0..=n).map(|k| row(&format!("p-{k}-cc"), "")).collect())
    });
    assert_eq!(snap.taken_names().expect("问得到").as_slice(), ["p-0-cc"]);
    assert_eq!(
        snap.taken_names().expect("问得到").as_slice(),
        ["p-0-cc", "p-1-cc"],
        "避让拿到的名单是陈的 —— 它会让新会话撞上刚建出来的那个"
    );
}

/// ★ 陈值那个口子（`peek`）**只许测试用** —— 生产段一处都不许调。
///
/// 它是本模块唯一一个「读了不更新」的口，留着是为了让上面两条测得出来。
/// 漏进生产段就等于把 `R52` 的前提拆了，所以在这里数一遍。
#[test]
fn the_stale_read_door_never_appears_in_production_code() {
    // ⚠ **必须走 `scan_tree!`，不许自己 `read_dir`** —— 裸遍历会让判据在**自己的语料**
    //   里找到自己 ⇒ 恒绿；那条纪律由 monitor 侧 `scanning_guard_registry` 机检
    //   （本条第一版就是这么红的）。
    // ⚠ 〔`P4` 2026-09-21〕宏自称的「摘掉调用者自己那一份」**在这一处不生效**
    //   （判据由 `#[path]` 挂载 ⇒ `file!()` 是折返路径 ⇒ 后缀比不命中）。
    //   本条不在自己的语料里靠的是**住址**：它住 `tests/backend/common/`，扫的是 `src/backend`。
    let src_dir = crate::guard_support::src_root();
    let mut checked = 0usize;
    let mut hits: Vec<String> = Vec::new();
    for (path, raw) in guard_core::scan_tree!(&src_dir, &["rs"]) {
        checked += 1;
        if crate::guard_support::production_code(&raw).contains(".peek()") {
            hits.push(
                path.strip_prefix(&src_dir)
                    .unwrap_or(&path)
                    .to_string_lossy()
                    .replace('\\', "/"),
            );
        }
    }
    assert!(
        checked >= 20,
        "只扫到 {checked} 个源文件 —— 遍历坏了，本条在空转"
    );
    assert!(
        hits.is_empty(),
        "这些文件的**生产段**里调了 `.peek()`（快照的陈值口）：{hits:?}\n\
             ★ 陈值只许测试读。生产上要「这一刻」就调 `query()`。"
    );
}

/// ★★ **`K-R12`：这一处起 tmux，`-u` 必须排在子命令之前。**
///
/// 判据本体从 `control/gate.rs` 随 `list-sessions` 这处调用点一起搬来
/// （那边今天只剩 `display-message` 一处，它那条判据也随之收成一个动词）。
/// 为什么这一条只能是「扫源码」、以及它守不住什么，见 `control/gate.rs` 同名判据的头注
/// （行为那一半的死值在 `tests/evidence/K-R12-deathvalue.md`）。
#[test]
fn the_one_list_sessions_call_asks_for_a_utf8_client_before_the_subcommand() {
    const FLAG_IDENT: &str = "UTF8_CLIENT_FLAG";
    const ARGS_OPEN: &str = ".args([";
    let prod = crate::guard_support::production_code(include_str!(
        "../../../src/backend/common/session_snapshot.rs"
    ));
    crate::guard_support::assert_no_test_code("common/session_snapshot.rs", &prod);

    let starts = prod.matches("Command::new(\"tmux\")").count();
    assert_eq!(
        starts, 1,
        "本模块起 tmux 的处数变了（实得 {starts}，登记 1）—— 它是**唯一**一处\
             「一次列全部会话」的起进程点，多一处就说明快照又被绕开了。"
    );
    let flat: String = prod.chars().filter(|c| !c.is_whitespace()).collect();
    let quoted = "\"list-sessions\"";
    let at = guard_core::find_pinned(&prod, quoted).expect("`list-sessions` 定得了位");
    let open = prod[..at].rfind(ARGS_OPEN).expect("它前面有一个 `.args([`");
    let before = &prod[open + ARGS_OPEN.len()..at];
    assert!(
        !before.contains(']'),
        "`list-sessions` 与它前面那个 `{ARGS_OPEN}` 之间隔着一个右方括号 —— 本条在断言别人的 argv"
    );
    assert!(
        before.contains(FLAG_IDENT),
        "`list-sessions` 那一处没把 `-u` 放在子命令**之前**。\
             放到后面是 rc=1 的响错，而这里不看退出码 ⇒ 会退化成「一个会话都没有」"
    );
    assert!(
        !flat.contains(&format!("{quoted},{FLAG_IDENT}")),
        "`-u` 被放到了子命令后面（`tmux -u ls -u` 同样 rc=1）"
    );
}

/// ★ `K-R12 J1`：段数下溢 ⇒ 整行不当好数据（本处这一侧）。
#[test]
fn a_tab_starved_line_is_dropped_instead_of_becoming_a_session() {
    // 通道被改写：TAB 变 `_` ⇒ 整行只切出 1 段。
    let dirty = "kr96_cc-deadval1\n";
    assert!(parse_rows(dirty).is_empty(), "下溢的行被当成了一个会话名");
    // 好行照常出。
    assert_eq!(
        parse_rows("a-cc\tsid-a\nb-cc\t\n\n"),
        vec![row("a-cc", "sid-a"), row("b-cc", "")]
    );
}
