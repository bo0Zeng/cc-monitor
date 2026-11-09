use super::*;

/// ★★ **死值验：Gate 判活不许自己起 `tmux list-sessions`。**
///
/// 「列全部会话」这件事**只剩快照那一处**（探测器 `host/tmux.rs::list_sessions`）。
/// 本模块回潮（把那条 argv 抄回来）时这一条要红。
///
/// 它守的是「面没变大」：本模块不再列全部会话（谁活着由后端判活那一处答，cc-bus 的名单不再问 tmux）。
#[test]
fn listing_every_session_is_no_longer_this_modules_job() {
    let prod = guard_core::production_code(include_str!("../../../src/backend/control/gate.rs"));
    guard_core::assert_no_test_code("control/gate.rs", &prod);
    assert!(
        !prod.contains("list-sessions"),
        "本模块生产段里又出现了 `list-sessions` —— 判活回去自己起 tmux 了。\n\
             ★ Gate 判活**读那份快照**（`common/session_snapshot`），\n\
               而且「向快照发一次询问，快照更新一次」。"
    );
    // 反向自检：这一条不是靠「本文件恰好不含那个词」空转的 —— 快照那边必须有。
    let owner = include_str!("../../../src/backend/host/tmux.rs");
    assert!(
        guard_core::production_code(owner).contains("\"list-sessions\""),
        "`host/tmux.rs` 的生产段里找不到 `list-sessions` ——\n\
             那处调用点搬走/改名了，本条此刻在空转（它只会证明「谁都没有」）。"
    );
}

/// ★★ **本模块起 tmux 的地方，`-u` 必须在子命令之前 —— 一处都不许漏。**
///
/// # 为什么这一条只能是「扫源码」，以及它守不住什么
///
/// 本模块起 tmux 的那一处（`display-message`；列窗格那一发已挪进宿主口）是 **argv 直传**（`Command::new("tmux")`），没有 builder 能把命令行取回来，
/// 也没有办法在不污染整个测试进程 `PATH` 的前提下把它指向一个假 tmux
/// （`Command::new` 走进程级 `PATH`，`std::env::set_var` 会波及并行跑的别的测试）。
/// ⇒ **行为那一半的死值不在 cargo 里**：
/// 同样的 argv 对真 tmux 3.4 私有 socket 打过，改前 `段数=1`、改后 `段数=3`。
///
/// 🔴 **本条守的是「别漏、别搬错位置」，不是「它真的生效了」**（「盘上有 ≠ 被走到」）。
/// 位置这一维值得单独钉：`-u` 放到子命令**后面**是 `rc=1 + unknown flag -u`
/// （实测），而这一处**刻意不看退出码** ⇒ 那个响错在这里会退化成
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
fn the_gate_tmux_call_asks_for_a_utf8_client_before_the_subcommand() {
    // 本条数的是**源码文本**，所以要的是那个常量的**名字**，不是它的值。
    const FLAG_IDENT: &str = "UTF8_CLIENT_FLAG";
    const ARGS_OPEN: &str = ".args([";

    let prod = guard_core::production_code(include_str!("../../../src/backend/control/gate.rs"));
    guard_core::assert_no_test_code("control/gate.rs", &prod);

    let starts = prod.matches("host::tmux::child(").count();
    assert_eq!(
        starts, 1,
        "本模块起 tmux 的处数变了（实得 {starts}，登记 1）—— 新增的那一处也要带 `-u`，\
             并把这条判据的数一起改。**这张表不是豁免清单。**\n\
             〔起 tmux 经宿主口 `host::tmux::child`；`list-sessions` 那一处在 `host/tmux.rs`（快照的探测器）。〕"
    );
    // 反向那一针用的人群：把排版这一维抹掉（`-u` 与子命令同不同行由 rustfmt 说了算）。
    let flat: String = prod.chars().filter(|c| !c.is_whitespace()).collect();
    // 列窗格那一发住宿主口（`host::tmux::panes_on`），它的 `-u` 由 `host/tmux_tests.rs` 钉。
    for verb in ["display-message"] {
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

/// ★★ **死值验（本模块这一侧）：段数下溢必须红。**
///
/// 死值取自真 tmux 3.4 + POSIX 客户端下现打：
/// `list-sessions` 那三列打出来是 `kr12_$0_cc-deadval1`、
/// `display-message` 那三列打出来是 `$0_cc-deadval1_1` —— **TAB 全没了，段数 1**。
///
/// ⚠ 过溢那一档**不在这里**：`PROBE_FMT` 的列里没有路径，
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

/// 夹具 `expect` 那一列的名字：显式写字面量，不跟着 `Debug` 漂。
fn gate2_wire_name(g: crate::control::gate_rules::Gate2) -> &'static str {
    use crate::control::gate_rules::Gate2;
    match g {
        Gate2::AllowedByName => "allowed_by_name",
        Gate2::AllowedByRemoteSid => "allowed_by_remote_sid",
        Gate2::Rejected => "rejected",
    }
}

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
        let got = gate2_wire_name(crate::control::gate_rules::gate2(&name, sid.as_deref()));
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
        // 第三段是 `#{session_windows}`。**解析不出来 ⇒ 0**，而 Gate 3 要求恰好 1
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

/// ★ **Gate 3 拿不到窗口数时 fail closed**（解析失败 ⇒ 0 ⇒ 拒绝）。
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

// ── 把「skip 不是通过」那个刻意决定钉住 ──────────────
//
// 处境：`meta_dollar`（会话名 `cc-a$x`）在 CI 上报 `no_such_session`。e2e 脚本建完用 `=name:` 复核，
// 找不到就 **skip 并打出 tmux 实况** —— 一条**会自证**的失败，不是误导性的那种。
//
// ★★ **根因**：**tmux ≤3.4 会在会话名里给 `$` 前面插一个反斜杠**，只要 `$` 后面跟的是字母 / `_` / `{`：
// tmux 的 session_check_name() 把新名字过 utf8_stravis()（`:`→`_` 也是这儿来的，那正是
// `meta_colon`），而 3.4 的 `utf8.c` 里那条 `$` 规则**不看任何 flag**。
// ⇒ `cc-a$x` 存进 server 的真名是 `cc-a` + 反斜杠 + `$x`，`=cc-a$x:` 当然找不到
// ⇒ 正是那条复核 skip。上游 `692ce59bcef5`（2024-05-24）给它加了 `VIS_DQ` 门，
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
    guard_core::addr::repo_root()
}

/// ★ **未登记的 skip 仍然让 `backend-gate2` 整套红**：
/// ① `waiver_reason` 的兜底分支整行还是 `*) echo "" ;;` —— 兜底一回非空串，每一条未登记的 skip
///    都会被当成豁免，收尾那道门当场静默失效；
/// ② `skipped()` 没登记豁免的那一支记的是一条败（`bad`，共用计数 `tests/e2e/assert.sh`）：有败 ⇒ `finish` 退 1。
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
    let s_at = guard_core::find_pinned(&sh, "skipped() {")
        .unwrap_or_else(|e| panic!("e2e 脚本里的 `skipped()` 定义钉不住：{e}"));
    let s_rel = sh[s_at..]
        .find("\n}")
        .expect("`skipped()` 找不到收尾的 `}` —— 函数形状变了，切不出函数体");
    let s_body = &sh[s_at..s_at + s_rel];
    let (registered, unregistered) = s_body
        .split_once("\n  else\n")
        .expect("`skipped()` 里没有 `else` 那一支 —— 「登记了 ⇒ 跳过 · 没登记 ⇒ 败」切不出两支");
    assert!(
        registered.contains("skip \"") && !guard_core::contains_word(registered, "bad"),
        "`skipped()` 登记了豁免的那一支不再记跳过：{registered}"
    );
    assert!(
        unregistered.trim_start().starts_with("bad \""),
        "`skipped()` 没登记豁免的那一支不再记一条败：{unregistered}\n\
             🔴 记成跳过 ⇒ 未登记的 skip 被当成通过，整套照绿。"
    );
}

/// ★ 那个决定的**前提**：脚本里那条「建完复核、找不到就 skip 并打实况」还在。
///
/// 它一没，失败就退回**误导性**的那种（看起来像 Gate 2 判错，实际是夹具没准备好）——
/// 那条复核就是为它设的。
#[test]
fn the_selfevidencing_skip_branch_is_still_there() {
    let sh = std::fs::read_to_string(repo_root().join("tests/e2e/backend-gate2-acceptance.sh"))
        .expect("e2e 脚本读不到");
    for needle in ["has-session -t \"=$name:\"", "实际会话："] {
        assert!(
            sh.contains(needle),
            "e2e 脚本里找不到 `{needle}` —— 「建完复核 + skip 时打出 tmux 实况」那段没了。\n\
                 ★ 它一没，`meta_dollar` 的失败就退回**误导性**的那种：\n\
                 看起来像「Gate 2 判错了」，实际是「夹具没准备好」。\n\
                 ⚠ 同时上面那条地板判据也失去意义 —— 它保护的正是这条 skip 的可见性。"
        );
    }
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
        ("phonepipe-x", p("", ""), None, Who::NotOurs),
        ("phonepipe-x", p("", ""), Some("mobile"), Who::NotOurs),
        ("proj-cc", p("", ""), None, Who::Pass),
        ("phonepipe-x", p("s1", ""), None, Who::Pass),
        ("proj-cc", p("s1", "ccm"), None, Who::Pass),
        ("proj-cc", p("s1", "ccm"), Some("mobile"), Who::Pass),
        ("phonepipe-x", p("", "ccm"), Some("mobile"), Who::NotOurs),
        ("phonepipe-x", p("s1", "mobile"), None, Who::OtherClient),
        (
            "proj-cc",
            p("s1", "mobile"),
            Some("desktop"),
            Who::OtherClient,
        ),
        ("phonepipe-x", p("", "mobile"), Some("mobile"), Who::Pass),
    ] {
        assert_eq!(
            identity(name, &probed.client, &probed.ccm_sid, requester),
            want,
            "{name} · {probed:?} · 请求自报 {requester:?}"
        );
    }
}

/// 请求里的 `client`：没给 ⇒ 没报；给了就得合形状（会进 tmux 会话选项），不合 ⇒ `bad_args`。
#[test]
fn the_requester_is_optional_and_shaped() {
    // 类型不对（`client: 1`）在解入参那一步就回 `bad_args`（`inbound_tests::the_terminal_trio_refuses_fields_it_does_not_use`）。
    assert_eq!(requester(None), Ok(None));
    assert_eq!(requester(Some("mobile")), Ok(Some("mobile".into())));
    let long = "x".repeat(33);
    for bad in ["", "Mobile", "-x", "a b", long.as_str()] {
        assert_eq!(
            requester(Some(bad)).map_err(|s| s.code),
            Err(Some("bad_args".to_string())),
            "{bad}"
        );
    }
}

/// 门那几句里的 tmux 会话名直接填进句子：`format!("{name:?}")` 会让界面上出现 Rust 调试形的引号与转义（手机端键名对账 10-09 第四节第 4 条）。
#[test]
fn the_gate_puts_the_session_name_in_as_is() {
    let prod = guard_core::production_code(include_str!("../../../src/backend/control/gate.rs"));
    assert!(
        !prod.contains("{name:?}"),
        "gate.rs 生产段里又用 `{{name:?}}` 填会话名 —— 界面上会带调试引号"
    );
    // 反向自检：门的那几句确实还从这里取文、还填 name（不是改名后空转）。
    assert!(prod.contains("\"beGate.admit.noSession\"") && prod.contains("(\"name\", "));
}

/// tmux 起不来：句子只说原因词，系统原话（`No such file or directory (os error 2)` 一类）不上句子。
#[test]
fn a_missing_tmux_is_said_with_a_reason_word_not_the_os_words() {
    let dir = std::env::temp_dir().join(format!("ccm-gate-gone-{}", std::process::id()));
    let failed = probe_with(Child::new(dir.join("gone")), "=x:").unwrap_err();
    assert_eq!(failed.code.as_deref(), Some("no_tmux"));
    let said = failed.said;
    assert_eq!(
        said,
        copy_text(
            "reason.tmux.startFailed",
            &[(
                "why",
                &copy_core::spawn_reason(std::io::ErrorKind::NotFound)
            )]
        )
    );
}
