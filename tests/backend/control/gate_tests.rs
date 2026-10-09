use super::*;

/// ★★ **`KR96D1` 死值验第一刀：Gate 判活不许自己起 `tmux list-sessions`。**
///
/// 「列全部会话」这件事**只剩快照那一处**（`common/session_snapshot.rs`）。
/// 本模块回潮（把那条 argv 抄回来）时这一条要红。
///
/// 🔴 **它守的是「面没变大」，不是「快照真被走到」** —— 后面那条由
/// `the_liveness_answer_comes_from_this_moment_not_from_a_cache` 的行为夹具钉。
/// 两条缺一不可：只有结构那条，「读快照但读的是陈值」照样绿；
/// 只有行为那条，「另起一条 argv 自己去问」在没有真 tmux 的沙箱里也可能碰巧绿。
#[test]
fn listing_every_session_is_no_longer_this_modules_job() {
    let prod =
        crate::guard_support::production_code(include_str!("../../../src/backend/control/gate.rs"));
    crate::guard_support::assert_no_test_code("control/gate.rs", &prod);
    assert!(
        !prod.contains("list-sessions"),
        "本模块生产段里又出现了 `list-sessions` —— 判活回去自己起 tmux 了。\n\
             ★ `R52` 裁定一：Gate 判活**读那份快照**（`common/session_snapshot`），\n\
               而且「向快照发一次询问，快照更新一次」。"
    );
    // 反向自检：这一条不是靠「本文件恰好不含那个词」空转的 —— 快照那边必须有。
    let owner = include_str!("../../../src/backend/common/session_snapshot.rs");
    assert!(
        crate::guard_support::production_code(owner).contains("\"list-sessions\""),
        "`common/session_snapshot.rs` 的生产段里找不到 `list-sessions` ——\n\
             那处调用点搬走/改名了，本条此刻在空转（它只会证明「谁都没有」）。"
    );
}

/// ★★ **`KR96D1` 死值验第二刀（Gate 这一侧）：判活拿到的值必须是「这一刻的」。**
///
/// 🔴 **失效方向（件文件逐字）：只判「有没有调那个快照函数」。**
/// ⇒ 夹具是一个**会变的世界**（探测器第 1 次回 A、第 2 次回 B），
/// 判的是 `list_sessions_from` 第二次交出来的是 **B**。
/// 把 `SessionSnapshot::query` 改成「先看缓存」⇒ 这里当场红。
#[test]
fn the_liveness_answer_comes_from_this_moment_not_from_a_cache() {
    use crate::common::session_snapshot::SessionRow;
    let calls = std::sync::atomic::AtomicUsize::new(0);
    let snap = SessionSnapshot::with_prober(move || {
        let n = calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        Ok(if n == 0 {
            vec![
                SessionRow {
                    name: "alive-cc".into(),
                    ccm_sid: "sid-a".into(),
                },
                SessionRow {
                    name: "doomed-cc".into(),
                    ccm_sid: "sid-d".into(),
                },
            ]
        } else {
            // `doomed-cc` 这一刻死了。
            vec![SessionRow {
                name: "alive-cc".into(),
                ccm_sid: "sid-a".into(),
            }]
        })
    });

    let before = list_sessions_from(&snap).expect("问得到");
    assert_eq!(
        before,
        vec![
            ("alive-cc".to_string(), "sid-a".to_string()),
            ("doomed-cc".to_string(), "sid-d".to_string()),
        ]
    );
    let after = list_sessions_from(&snap).expect("问得到");
    assert_eq!(
        after,
        vec![("alive-cc".to_string(), "sid-a".to_string())],
        "`doomed-cc` 已经死了，判活却还把它报成活的 —— 拿的是陈值。\n\
             ★ 这就是 08-13 实测过的那个后果：敲门文字被打进**陌生占用者**的屏幕。"
    );
}

/// ★★ **K-R12：本模块起 tmux 的地方，`-u` 必须在子命令之前 —— 一处都不许漏。**
///
/// # 为什么这一条只能是「扫源码」，以及它守不住什么
///
/// 本模块的两处是 **argv 直传**（`Command::new("tmux")`），没有 builder 能把命令行取回来，
/// 也没有办法在不污染整个测试进程 `PATH` 的前提下把它指向一个假 tmux
/// （`Command::new` 走进程级 `PATH`，`std::env::set_var` 会波及并行跑的别的测试）。
/// ⇒ **行为那一半的死值不在 cargo 里**：
/// 同样这两条 argv 对真 tmux 3.4 私有 socket 打过，改前 `段数=1`、改后 `段数=3`。
///
/// 🔴 **本条守的是「别漏、别搬错位置」，不是「它真的生效了」**（「盘上有 ≠ 被走到」）。
/// 位置这一维值得单独钉：`-u` 放到子命令**后面**是 `rc=1 + unknown flag -u`
/// （实测），而本模块两处都**刻意不看退出码** ⇒ 那个响错在这里会退化成
/// 「一个会话都没有」/「探不到」，**又变回一次静默失效**。
///
/// # ★★ 09-09：这条判据原先把**排版**也一起断言了进去
///
/// 原版的针是一整串跨元素的字面量：`.args([UTF8_CLIENT_FLAG, "<verb>"`。
/// 那串里有一个空格与一个逗号 —— 也就是说它顺带断言了
/// 「`-u` 与子命令必须在源码的**同一行**上」。而那一维**由 rustfmt 说了算**：
/// 09-09 那趟 `cargo fmt --all` 把 `probe` 里的 `.args([…])` 拆成每元素一行，
/// 本条当场红，判定行逐字「`display-message` 那一处没有把 `-u` 放在子命令**之前**
/// （找不到 `.args([UTF8_CLIENT_FLAG, "display-message"`）」——
/// **而 `-u` 一个字节都没挪过**。反向那两根针同时**静默失效**：
/// 它们也是带空格的跨元素字面量，拆行之后永远零命中，
/// 于是「有人把 `-u` 塞到子命令后面」这一格从此不会红。
///
/// ⇒ 今天断言的是**次序关系本身**，与排版无关：
///
/// - 正向：子命令那个字面量，与**它所属的那个 `.args([`** 之间，必须出现 `UTF8_CLIENT_FLAG`
///   （中间不许隔着 `]` —— 隔着就说明它压根不在那个数组里，本条在空转）；
/// - 反向：把源码**全部空白删掉**之后，不许出现 `"<verb>",UTF8_CLIENT_FLAG`。
///
/// 要求住址：`INVARIANTS §49`（tmux 打印通道必须是 UTF-8，段数下溢出声）。
#[test]
fn both_tmux_call_sites_ask_for_a_utf8_client_before_the_subcommand() {
    // 本条数的是**源码文本**，所以要的是那个常量的**名字**，不是它的值。
    const FLAG_IDENT: &str = "UTF8_CLIENT_FLAG";
    const ARGS_OPEN: &str = ".args([";

    let prod =
        crate::guard_support::production_code(include_str!("../../../src/backend/control/gate.rs"));
    crate::guard_support::assert_no_test_code("control/gate.rs", &prod);

    let starts = prod.matches("Child::new(\"tmux\")").count();
    assert_eq!(
        starts, 1,
        "本模块起 tmux 的处数变了（实得 {starts}，登记 1）—— 新增的那一处也要带 `-u`，\
             并把这条判据的数一起改。**这张表不是豁免清单。**\n\
             〔`K-R96` 09-12：2 → 1。`list-sessions` 那一处搬进了 `common/session_snapshot.rs`\
             （Gate 判活改成问那张快照，`R52` 裁定一），判据也随调用点搬了过去。〕"
    );
    // 反向那一针用的人群：把排版这一维抹掉（`-u` 与子命令同不同行由 rustfmt 说了算）。
    let flat: String = prod.chars().filter(|c| !c.is_whitespace()).collect();
    for verb in ["display-message", "list-panes"] {
        let quoted = format!("\"{verb}\"");
        // ── 正向：`-u` 排在子命令**之前** ──────────────────────────────
        let at = guard_core::find_pinned(&prod, &quoted).unwrap_or_else(|e| {
            panic!(
                "`{verb}` 这个子命令在本模块生产段里定不了位（{e}）。\n\
                     ★ 起 tmux 那两处换了写法就来改本条 —— 别让它零命中地绿。"
            )
        });
        let open = prod[..at].rfind(ARGS_OPEN).unwrap_or_else(|| {
            panic!(
                "`{verb}` 前面一个 `{ARGS_OPEN}` 都没有 —— 它已经不是 argv 直传了。\n\
                     ★ 换成别的传法（`arg()` 逐个加 / 拼 shell 串）本条就管不着了，回来改。"
            )
        });
        let before = &prod[open + ARGS_OPEN.len()..at];
        assert!(
            !before.contains(']'),
            "`{verb}` 与它前面那个 `{ARGS_OPEN}` 之间隔着一个右方括号 ——\n\
                 它根本不在那个数组里，本条此刻断言的是**别人的 argv**。"
        );
        assert!(
            before.contains(FLAG_IDENT),
            "`{verb}` 那一处没有把 `-u` 放在子命令**之前**\
                 （`{ARGS_OPEN}` 与它之间找不到 `{FLAG_IDENT}`）。\n\
                 放到后面是 rc=1 的响错，而本模块不看退出码 ⇒ 会退化成又一次静默失效。"
        );
        // ── 反向：也不许在子命令**后面**再塞一个（`tmux -u ls -u` 同样 rc=1）──
        let bad = format!("{quoted},{FLAG_IDENT}");
        assert!(!flat.contains(&bad), "`-u` 被放到了子命令后面：{bad}");
    }
}

/// ★★ **K-R12 `J1` 死值验（本模块这一侧）：段数下溢必须红。**
///
/// 死值取自真 tmux 3.4 + POSIX 客户端下现打：
/// `list-sessions` 那三列打出来是 `kr12_$0_cc-deadval1`、
/// `display-message` 那三列打出来是 `$0_cc-deadval1_1` —— **TAB 全没了，段数 1**。
///
/// ⚠ 过溢那一档**不在这里**：`PROBE_FMT`/`LIST_FMT` 的列里没有路径，
/// 三列的取值域都排除真 TAB ⇒ 合法内容推不高段数（理由见 `PROBE_FMT_FIELDS` 头注）。
/// 但判据仍写成「下溢」而不是「不等于」，与另外两处同一口径 —— **口径一致本身是要买的东西**。
///
/// 要求住址：`INVARIANTS §49`（tmux 打印通道必须是 UTF-8，段数下溢出声）。
#[test]
fn the_underflow_predicate_catches_the_real_dirty_bytes() {
    assert!(
        tab_underflow("kr12_$0_cc-deadval1", 3),
        "真 tmux 打出来的脏字节必须判下溢"
    );
    assert!(tab_underflow("$0_cc-deadval1_1", 3), "同上（probe 那一条）");
    assert!(
        !tab_underflow("kr12\t$0\tcc-deadval1", 3),
        "干净的三段必须放行"
    );
    assert!(
        !tab_underflow("$0\t\t1", 3),
        "🔴 `@ccm_sid` **没设**是合法的（中间那段是空串）—— 它与「拆不出」是两件事，不许判红"
    );
    assert!(
        !tab_underflow("a\tb\tc\td", 3),
        "过溢不许红（口径与另外两处一致：判的是下溢，不是不等于）"
    );
}

/// 判定表的**唯一源头**：后端这一轨与 e2e 各自独立读它（monitor 那一轨随 monitor 侧的门删了）。
const GOLDEN: &str = include_str!("../../__fixtures__/gate2-golden.tsv");

fn golden_rows() -> Vec<(String, String, Option<String>, String)> {
    GOLDEN
        .lines()
        .filter(|l| !l.trim_start().starts_with('#') && !l.trim().is_empty())
        .map(|l| {
            let f: Vec<&str> = l.split('\t').collect();
            assert_eq!(f.len(), 4, "夹具行不是 4 列：{l:?}");
            let sid = match f[2] {
                "<none>" => None,
                "<unset>" => Some(String::new()),
                v => Some(v.to_string()),
            };
            (f[0].to_string(), f[1].to_string(), sid, f[3].to_string())
        })
        .collect()
}

/// ★ 抽取器自检：夹具读不到 / 解析空时，下面那条会零命中地绿。
#[test]
fn the_golden_table_is_actually_read_from_the_monitor_side_fixture() {
    let rows = golden_rows();
    assert!(
        rows.len() >= 20,
        "只解析出 {} 行夹具 —— 路径或解析坏了（这份夹具住在 monitor 那边，跨仓相对路径）",
        rows.len()
    );
    // 三种结论都必须在表里出现，否则表本身是偏的。
    for want in ["allowed_by_name", "allowed_by_remote_sid", "rejected"] {
        assert!(
            rows.iter().any(|r| r.3 == want),
            "夹具里一行 `{want}` 都没有 —— 表偏了，下面那条测不到那一支"
        );
    }
}

/// ★ backend 这一侧对同一张表给出同样的判定。
///
/// ⚠ **不许改成「调 monitor 的实现来对拍」** —— 两侧一起错就全绿了。
/// 两侧各自独立读这张表，才叫跨轨。
#[test]
fn the_backend_side_agrees_with_the_golden_table() {
    let mut bad = Vec::new();
    for (id, name, sid, want) in golden_rows() {
        let got = crate::control::gate_rules::gate2(&name, sid.as_deref()).as_str();
        if got != want {
            bad.push(format!(
                "  {id}: name={name:?} sid={sid:?} 期望={want} 实得={got}"
            ));
        }
    }
    assert!(
        bad.is_empty(),
        "backend 侧与判定表不一致：\n{}",
        bad.join("\n")
    );
}

/// ★ 生产接线：`admit` 通过之后回的是**句柄**，不是名字 —— TOCTOU 那条的落点。
///
/// 这里只钉「解析出来的形状」；真 tmux 上的行为由
/// `tests/e2e/backend-gate2-acceptance.sh` 钉（那才是真二进制那一轨）。
#[test]
fn a_probe_line_parses_into_a_handle_and_a_sid() {
    // 直接构造探测输出的解析结果，不起进程（起进程是 e2e 的事）。
    let line = "$3\tabc123\t1\t";
    let mut it = line.split('\t');
    let p = Probed {
        session_id: it.next().unwrap().to_string(),
        ccm_sid: it.next().unwrap().to_string(),
        // F04a：第三段是 `#{session_windows}`。**解析不出来 ⇒ 0**，而 Gate 3 要求恰好 1
        // ⇒ fail closed（拿不到窗口数就不许杀）。
        windows: it.next().unwrap_or_default().parse().unwrap_or(0),
        // 第四段是 `@ccm_client`（没设 ⇒ 空串）。
        client: it.next().unwrap_or_default().to_string(),
    };
    assert_eq!(p.client, "");
    assert_eq!(p.session_id, "$3");
    assert_eq!(p.ccm_sid, "abc123");
    assert_eq!(p.windows, 1);
    assert!(
        p.session_id.starts_with('$'),
        "tmux 的 session_id 恒是 `$N` —— 不是这个形状就说明格式串被动过了"
    );
}

/// 按 sid 找窗格：一个会话里几个窗格各挂一个 sid ⇒ 挂着它的那个，不看哪个是当前窗格；
/// 同一个 sid 挂在几个窗格上（会话那一级的旧标签往下透）⇒ 当前窗格那个，否则第一个；谁都不挂 ⇒ 没有。
/// 读不懂的行（段数不对 = 打印通道被改写、句柄不是 `%N`）丢掉，不猜。
#[test]
fn the_pane_that_carries_a_sid_is_found_whichever_pane_is_active() {
    let panes = parse_panes(
        "%1\t101\tsid-a\t00\n\
         %2\t102\tsid-b\t10\n\
         %3\t103\t\t11\n\
         %4\t104\tsid-l\t01\n\
         %5\t105\tsid-l\t11\n\
         %6\t106\tsid-l\t00\n\
         %7_107_sid-x_00\n\
         $8\t108\tsid-y\t00\n\
         %9\tpid\tsid-z\t00\n",
    );
    assert_eq!(
        panes.iter().map(|p| p.pane.as_str()).collect::<Vec<_>>(),
        ["%1", "%2", "%3", "%4", "%5", "%6"]
    );
    assert_eq!(panes[0].pid, 101);
    assert!(panes[4].current && !panes[1].current && !panes[3].current);
    let at = |sid: &str| carrier(&panes, sid).map(|p| p.pane.as_str());
    assert_eq!(
        at("sid-a"),
        Some("%1"),
        "活动的是别的窗格也照样找到挂着它的那个"
    );
    assert_eq!(at("sid-b"), Some("%2"));
    assert_eq!(
        at("sid-l"),
        Some("%5"),
        "几个窗格挂着同一个 sid ⇒ 当前窗格那个"
    );
    assert_eq!(
        carrier(&panes[3..4], "sid-l").map(|p| p.pane.as_str()),
        Some("%4")
    );
    assert_eq!(at("sid-x"), None);
    assert_eq!(at(""), None, "没挂 sid 的窗格不是「挂着空 sid」");
}

/// ★ F04a：**Gate 3 拿不到窗口数时 fail closed**（解析失败 ⇒ 0 ⇒ 拒绝）。
///
/// 反向的错法（解析失败当 1）会把「探测被截断」变成「放行一次 kill」——
/// 那是本仓最不能接受的一类默认值。
#[test]
fn gate3_fails_closed_when_the_window_count_is_unreadable() {
    for line in ["$1\tsid", "$1\tsid\t", "$1\tsid\tnot-a-number"] {
        let mut it = line.split('\t');
        it.next();
        it.next();
        let w: u32 = it.next().unwrap_or_default().trim().parse().unwrap_or(0);
        assert_ne!(
            w, 1,
            "{line:?} 解析出的窗口数不该等于 1（那会放行一次 kill）"
        );
    }
    let mut it = "$1\tsid\t1".split('\t');
    it.next();
    it.next();
    assert_eq!(it.next().unwrap().parse::<u32>().unwrap(), 1);
}

/// ★ 格式串里必须**同时**有句柄、sid 与窗口数：少了句柄就退回「对名字下手」＝TOCTOU 回归，
/// 少了 sid 就等于没有 Gate 2，少了窗口数就等于没有 Gate 3。
#[test]
fn the_probe_format_asks_for_both_fields() {
    assert!(
        PROBE_FMT.contains("#{session_id}"),
        "少了句柄 ⇒ TOCTOU 窗口回来了"
    );
    assert!(
        PROBE_FMT.contains("#{@ccm_sid}"),
        "少了 sid ⇒ 这道门就是空的"
    );
    assert!(
        PROBE_FMT.contains("#{session_windows}"),
        "少了窗口数 ⇒ Gate 3 没有输入，破坏性动作会误杀多窗口会话"
    );
    assert!(
        !PROBE_FMT.contains("@ccm_sid_expect"),
        "**只认 `@ccm_sid`** —— `_expect` 是「声明了但未必跑起来」的意图，不是事实"
    );
}

/// ★★ **`K-R72`（09-12）接手 `K-R56`（09-11）买的那条性质：两道门都**恒先探会话**。**
///
/// # 它从哪来 —— 这是一条**搬家**，不是一条新判据
///
/// `K-R56` 在 monitor 侧立了 `tmux.rs` 里那条
/// `tests::the_ssh_fallback_always_probes_before_it_acts`〔散文墓碑〕，
/// 守的是「那条一次性 SSH 回落上，四种守护形态一个不漏地**先探会话、探不到就不动手**」。
/// 立它的理由逐字是：backend 的 [`admit`] **恒先 `probe`**，而 monitor 那条退化分支
/// （`cc-*` 名的 send-keys）**一次探测都没有** ⇒ 两条路的门不等价（`K-R54` 表第 1 处）。
///
/// 🔴 `K-R72` 把 monitor 那条路整个删了 ⇒ **那条判据的被测对象没有了，而它守的性质还在**
/// —— 只是今天只剩这一处实现。按 `KR72D2` 的三类去向，它是 ①「性质还成立 ⇒ 在新住址
/// 上重新钉住」。**这里就是那个新住址。**
///
/// # 它判什么 · 不判什么（`brief` 12：报一个性质就要说清尺子）
///
/// - **判**：两个门函数的生产段里，会话是**先探出来**的（`let Some(p) = probe(target)?`
///   这个绑定 —— 后面每一句都在它之后，所以不必再比位置），探不到那一支回
///   `no_such_session`，而放行交出的是**探回来的句柄** `p.session_id`，
///   不是调用方给的名字。
/// - **不判**：「探对了」。那要真 tmux，本区口径禁（`K-R56#§0d`）。
///   行为那一半在 `tests/e2e/backend-gate2-acceptance.sh`（真后端二进制 + 真 tmux server，
///   用例逐行来自同一张 `gate2-golden.tsv`）。
/// - ⚠ **约定型守卫**：扫的是本文件自己的源码形态，挡得住「顺手把 probe 挪到动作后面 /
///   删掉 `else` 那一支」，挡不住「换个名字继续错」。**比没有强，别读成证明。**
#[test]
fn both_gates_always_probe_before_they_act() {
    // 探测必须是**那个绑定**：`let Some(p) = probe(target)?` 一旦在，后面每一句都在它之后
    // ⇒ 不必比位置（那会踩 `structural_scan` 那条「位置比较型判据」的三条纪律）。
    const PROBE_BINDING: &str = "let Some(p) = probe(target)?";
    const HANDLE_OUT: &str = "Ok(p.session_id)";
    let prod = guard_core::production_code(include_str!("../../../src/backend/control/gate.rs"));
    // 抽取器自检：剥生产段塌了下面几条就零命中地绿。
    assert!(
        prod.len() > 3_000,
        "本文件生产段只剩 {} 字节 —— 剥法坏了，本条此刻量不到东西",
        prod.len()
    );
    // 两道门各一份 —— 数量核在这里，下面按签名切段就不会比到隔壁去。
    assert_eq!(
        prod.matches(PROBE_BINDING).count(),
        2,
        "生产段里 `{PROBE_BINDING}` 不是恰好两处（`admit` 与 `admit_destructive` 各一）"
    );
    // 放行时交出来的句柄：`admit` 交会话句柄，或那个会话里挂着请求 sid 的窗格句柄；
    // `admit_destructive` 交「整个会话」（会话句柄）或「这几个窗格」（同会话句柄 ＋ 窗格句柄）。
    for (sig, handle_out) in [
        ("pub(crate) fn admit(", "Ok(at.unwrap_or(p.session_id))"),
        (
            "pub(crate) fn admit_destructive(",
            "Ok(EndAt::Session(p.session_id))",
        ),
    ] {
        let at = guard_core::find_pinned(&prod, sig).unwrap_or_else(|e| {
            panic!("`{sig}` 不是恰好一处（{e}）—— 签名变了就把本条一起改，别让它零命中地绿")
        });
        let rest = &prod[at..];
        let end = rest.find("\n\u{7d}\n").map(|k| k + 3).unwrap_or(rest.len());
        let body = &rest[..end];
        assert!(
            body.contains(PROBE_BINDING),
            "`{sig}` 的生产段里没有 `{PROBE_BINDING}` —— 它会对一个可能不存在、\n\
                 也可能不是本工具的会话直接动手。实得这一段：{body:?}"
        );
        assert!(
            body.contains("no_such_session"),
            "`{sig}` 里探不到会话时没有回 `no_such_session` —— 那一档被吞掉了"
        );
        assert!(
            body.contains(handle_out),
            "`{sig}` 放行时交出来的不是探回来的句柄 `{handle_out}` ——\n\
                 对**名字**下手就把 TOCTOU 窗口留着（`K-R54` 表第 2 处判「留后端」\n\
                 的理由逐字就是这一条）。实得这一段：{body:?}"
        );
    }
    // ★ 反向自检：同一把尺子在一份「不先探就动手」的合成语料上必须分得出来。
    //   ⚠ 语料里的名字刻意取中性名，断言不取自夹具的名字（`6g` 那一族）。
    const SYNTHETIC_BAD: &str = "fn zzz(t: &str) -> u8 { act(t); 0 }";
    assert!(
        !SYNTHETIC_BAD.contains(PROBE_BINDING) && !SYNTHETIC_BAD.contains(HANDLE_OUT),
        "本条用来自检的那份坏语料本身就不坏 —— 反向自检此刻什么都没在证明"
    );
}

// ── audit-0805 F20 下半：把「skip 不是通过」那个刻意决定钉住 ──────────────
//
// F20 的处境：`meta_dollar`（会话名 `cc-a$x`）在 CI 上报 `no_such_session`，
// 当时**根因未知** —— 复现要那台机器的 tmux，红线禁真 tmux、且已裁定不再推 CI。
// 上半做的是「把一条误导性的失败改成一条**会自证**的失败」：建完用 `=name:` 复核，
// 找不到就 **skip 并打出 tmux 实况**。
//
// ★★ **2026-09-09：根因查出来了。这段话原来写着「根因至今未知」，那句今天不成立了。**
//
// **tmux ≤3.4 会在会话名里给 `$` 前面插一个反斜杠**，只要 `$` 后面跟的是字母 / `_` / `{`：
// `session_check_name()` 把新名字过 `utf8_stravis(…)`（`:`→`_` 也是这儿来的，那正是
// `meta_colon`），而 3.4 的 `utf8.c` 里那条 `$` 规则**不看任何 flag**。
// ⇒ `cc-a$x` 存进 server 的真名是 `cc-a` + 反斜杠 + `$x`，`=cc-a$x:` 当然找不到
// ⇒ 正是 F20 那条复核 skip。上游 `692ce59bcef5`（2024-05-24）给它加了 `VIS_DQ` 门，
// **首次随 3.5 出货**；runner 的 ubuntu-24.04 装的是 **3.4**（日志逐字 `tmux 3.4-1ubuntu0.1`），
// 而开发机是 **3.6** ⇒ **这个病在本地物理上看不见**，这就是它挂了一个月的原因。
//
// ⚠ **一个会让人改错东西的坑，写下来**：日志里印的「实际会话 `[cc-a\\$x ]`」
//   **本身是二次转义的**（打印那条路又过了一次 vis）—— **真名只有一个反斜杠**。
//
// ⇒ 处置：进 `waiver_reason` 成为**第二条登记豁免**（理由里带机制、版本边界与出处）。
//   **不是「不验了」**：判定表被三方独立读，另两条纯函数轨照常验它，本轨欠的只是「真会话」这一层。
//
// 「skip 不是通过」由套件自己扛：收尾 `[ "$skip" -eq 0 ] || … exit 1`（**未登记**的 skip 让整套 RC=1），
// 而登记的豁免进 `waiver_reason()`，兜底分支回空串（不算登记）。门禁只判退出码 ＋ `FAIL=0` ＋ `PASS > 0`，
// 不钉断言条数 ⇒ 这两处就是那个决定的全部落点，下面那条把它们钉住。

fn repo_root() -> std::path::PathBuf {
    crate::guard_support::repo_root()
}

/// ★ **未登记的 skip 仍然让 `backend-gate2` 整套红**：
/// ① `waiver_reason` 的兜底分支整行还是 `*) echo "" ;;` —— 兜底一回非空串，每一条未登记的 skip
///    都会被当成豁免，收尾那道门当场静默失效；
/// ② 收尾那道门还在：`[ "$skip" -eq 0 ] ||` 之后 `exit 1`。
#[test]
fn an_unregistered_skip_still_fails_the_gate2_suite() {
    let sh = std::fs::read_to_string(repo_root().join("tests/e2e/backend-gate2-acceptance.sh"))
        .expect("e2e 脚本读不到");
    let w_at = guard_core::find_pinned(&sh, "waiver_reason() {")
        .unwrap_or_else(|e| panic!("e2e 脚本里的 `waiver_reason()` 定义钉不住：{e}"));
    let w_rel = sh[w_at..]
        .find("esac")
        .expect("`waiver_reason` 里找不到 `esac` —— 函数形状变了，切不出函数体");
    let body = &sh[w_at..w_at + w_rel];
    assert!(
        guard_core::contains_word(body, "case \"$1\" in"),
        "切出来的不是 `waiver_reason` 的 `case` 块（{} 字节）—— 抽取器坏了",
        body.len()
    );
    if let Err(e) = guard_core::pin_line(body, "*) echo \"\" ;;") {
        panic!(
            "`waiver_reason` 的兜底分支不再是整行 `*) echo \"\" ;;`：{e}\n\
                 🔴 兜底回非空串 ⇒ 每一条**未登记**的 skip 都会被当成豁免 ⇒ 收尾那道门静默失效。"
        )
    }
    let gate_line = sh
        .lines()
        .filter(|l| l.trim_start().starts_with("[ \"$skip\" -eq 0 ] ||"))
        .collect::<Vec<_>>();
    assert_eq!(
        gate_line.len(),
        1,
        "收尾那道「未登记的 skip ⇒ 整套红」不是恰好一行：{gate_line:?}"
    );
    assert!(
        gate_line[0].contains("exit 1"),
        "收尾那道门不再 `exit 1`：{} —— skip 会被当成通过",
        gate_line[0]
    );
}

/// ★ 那个决定的**前提**：脚本里那条「建完复核、找不到就 skip 并打实况」还在。
///
/// 它一没，失败就退回**误导性**的那种（看起来像 Gate 2 判错，实际是夹具没准备好）——
/// 那正是 F20 上半修掉的东西。
#[test]
fn the_selfevidencing_skip_branch_is_still_there() {
    let sh = std::fs::read_to_string(repo_root().join("tests/e2e/backend-gate2-acceptance.sh"))
        .expect("e2e 脚本读不到");
    for needle in ["has-session -t \"=$name:\"", "实际会话："] {
        assert!(
            sh.contains(needle),
            "e2e 脚本里找不到 `{needle}` —— 「建完复核 + skip 时打出 tmux 实况」那段没了。\n\
                 ★ 它一没，`meta_dollar` 的失败就退回**误导性**的那种：\n\
                 看起来像「Gate 2 判错了」，实际是「夹具没准备好」。那是 F20 上半修掉的东西。\n\
                 ⚠ 同时上面那条地板判据也失去意义 —— 它保护的正是这条 skip 的可见性。"
        );
    }
}

// ════════════════════════════════════════════════════════════════════════
// 〔C4e 问 2〕过门的命令，登记表里的码必须盖住门会回的码
// ════════════════════════════════════════════════════════════════════════

/// 一段生产源码里「错误元组 / 错误出口」的码：`(` 之后（隔空白）紧跟一个蛇形字面量、再跟 `,`。
fn tl2_error_codes_in(src: &str) -> std::collections::BTreeSet<String> {
    let mut out = std::collections::BTreeSet::new();
    for (i, _) in src.match_indices('(') {
        let after = src[i + 1..].trim_start();
        let Some(rest) = after.strip_prefix('"') else {
            continue;
        };
        let Some(end) = rest.find('"') else { continue };
        let lit = &rest[..end];
        if rest[end + 1..].trim_start().chars().next() == Some(',')
            && lit.contains('_')
            && lit.chars().all(|c| c.is_ascii_lowercase() || c == '_')
        {
            out.insert(lit.to_string());
        }
    }
    out
}

/// `gate.rs` 生产段里 `fn <name>(` 的函数体（到顶层 `\n}` 为止）。
fn tl2_fn_body<'a>(src: &'a str, name: &str) -> &'a str {
    let at = src
        .find(&format!("fn {name}("))
        .unwrap_or_else(|| panic!("gate.rs 里找不到 `fn {name}(`"));
    let tail = &src[at..];
    &tail[..tail.find("\n}").expect("函数没收尾")]
}

/// 要求住址：`INVARIANTS §34`（tmux 破坏性 / 半破坏性命令三道门）· `INVARIANTS §42`（线上契约文档与代码不许漂）。
///
/// C4e 交上来的缺口逐字：「`launch` 那条后端登记表的 `codes` **没列 `wrong_owner`**，而 `control/launch.rs::run` 经 `gate::admit` 真会回它」。
/// 病根是「码表手写、没人从门那一侧核」。本条从**门的源码**派生：`admit` / `admit_destructive` 各自会回哪些码（连它们调的 `probe`），
/// 再从 `inbound::REGISTRY` 的每一格找出「实现模块调了哪道门」，那一格的 `codes` 必须 ⊇ 那道门的码。
/// 异源：码集合读 `gate.rs`，调用关系读各模块与 `stream/inbound/` 的源码，被比的是运行期的 `REGISTRY`。
#[test]
fn every_command_that_passes_the_gate_lists_the_gates_codes() {
    let gate =
        crate::guard_support::production_code(include_str!("../../../src/backend/control/gate.rs"));
    let probe = tl2_error_codes_in(tl2_fn_body(&gate, "probe"));
    let codes_of = |f: &str| -> std::collections::BTreeSet<String> {
        let body = tl2_fn_body(&gate, f);
        let mut c = tl2_error_codes_in(body);
        if body.contains("probe(") {
            c.extend(probe.iter().cloned());
        }
        c
    };
    let doors = [
        ("admit", codes_of("admit")),
        ("admit_destructive", codes_of("admit_destructive")),
    ];
    assert!(
        doors[0].1.contains("wrong_owner") && doors[1].1.contains("too_many_windows"),
        "门的码读错了：{doors:?}"
    );

    let families = crate::guard_support::registry_sources();
    let root = crate::guard_support::src_root();
    let mut checked = Vec::new();
    for spec in crate::stream::inbound::REGISTRY {
        let anchor = format!("name: \"{}\",", spec.name);
        let Some((family, at)) = families
            .iter()
            .find_map(|(_, prod)| prod.find(&anchor).map(|at| (prod, at)))
        else {
            continue;
        };
        let cell = &family[at..];
        let cell = &cell[..cell.find("\n    },").unwrap_or(cell.len())];
        for (i, _) in cell.match_indices("crate::control::") {
            let module: String = cell[i + "crate::control::".len()..]
                .chars()
                .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
                .collect();
            let Ok(src) =
                std::fs::read_to_string(root.join("control").join(format!("{module}.rs")))
            else {
                continue;
            };
            let prod = crate::guard_support::production_code(&src);
            for (door, want) in &doors {
                if !prod.contains(&format!("gate::{door}(")) {
                    continue;
                }
                let have: std::collections::BTreeSet<String> =
                    spec.codes.iter().map(|s| s.to_string()).collect();
                let missing: Vec<_> = want.difference(&have).collect();
                assert!(
                    missing.is_empty(),
                    "`{}` 的实现（control/{module}.rs）过 `gate::{door}`，那道门会回 {missing:?}，而 `inbound::REGISTRY` 那一格的 `codes` 没列 —— \
                     界面按登记的码逐码说人话，漏一个就是那一句永远说不出来（C4e 问 2 那一形）",
                    spec.name
                );
                checked.push((spec.name, *door));
            }
        }
    }
    checked.sort();
    checked.dedup();
    // 正控 ＋ 人群：今天过门的恰好是 launch（admit）与 kill（admit_destructive）。多了少了都要回来看一眼。
    assert_eq!(
        checked,
        vec![("kill", "admit_destructive"), ("launch", "admit")],
        "过门的命令变了"
    );
}

/// 「哪个前端的会话」那一维接在身份门上：声明了别的前端 ⇒ 只读（哪怕名字像我们铸的、`@ccm_sid` 也在）；
/// 声明归这次请求的前端 ⇒ 放（名字不必像我们铸的）；没声明 / 用户终端起的（`ccm`）⇒ 照名字规则。
#[test]
fn the_identity_door_reads_which_client_started_the_session() {
    let p = |sid: &str, client: &str| Probed {
        session_id: "$1".into(),
        ccm_sid: sid.into(),
        windows: 1,
        client: client.into(),
    };
    for (name, probed, requester, want) in [
        ("atermpipe-x", p("", ""), None, Who::NotOurs),
        ("atermpipe-x", p("", ""), Some("mobile"), Who::NotOurs),
        ("proj-cc", p("", ""), None, Who::Pass),
        ("atermpipe-x", p("s1", ""), None, Who::Pass),
        ("proj-cc", p("s1", "ccm"), None, Who::Pass),
        ("proj-cc", p("s1", "ccm"), Some("mobile"), Who::Pass),
        ("atermpipe-x", p("", "ccm"), Some("mobile"), Who::NotOurs),
        ("atermpipe-x", p("s1", "mobile"), None, Who::OtherClient),
        (
            "proj-cc",
            p("s1", "mobile"),
            Some("desktop"),
            Who::OtherClient,
        ),
        ("atermpipe-x", p("", "mobile"), Some("mobile"), Who::Pass),
    ] {
        assert_eq!(
            identity(name, &probed, requester),
            want,
            "{name} · {probed:?} · 请求自报 {requester:?}"
        );
    }
}

/// 请求里的 `client`：没给 ⇒ 没报；给了就得合形状（会进 tmux 会话选项），不合 ⇒ `bad_args`。
#[test]
fn the_requester_is_optional_and_shaped() {
    assert_eq!(requester_of(&serde_json::json!({})), Ok(None));
    assert_eq!(
        requester_of(&serde_json::json!({ "client": "mobile" })),
        Ok(Some("mobile".into()))
    );
    for bad in [
        serde_json::json!({ "client": "" }),
        serde_json::json!({ "client": "Mobile" }),
        serde_json::json!({ "client": "-x" }),
        serde_json::json!({ "client": "a b" }),
        serde_json::json!({ "client": 1 }),
        serde_json::json!({ "client": "x".repeat(33) }),
    ] {
        assert_eq!(
            requester_of(&bad).map_err(|(c, _)| c),
            Err("bad_args"),
            "{bad}"
        );
    }
}
