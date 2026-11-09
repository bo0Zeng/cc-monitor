//! tmux 那一形的原语（挑窗格 · 列整台会话的格式与切行 · pane id 形状 · `$TMUX` 取 socket）只住 `host/tmux.rs`。

use super::*;
use crate::common::session_snapshot::SessionRow;

fn row(name: &str, sid: &str) -> SessionRow {
    SessionRow {
        name: name.to_string(),
        ccm_sid: sid.to_string(),
    }
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

/// 名单那一趟的去重照同一个挑法：同一个 sid 挂在几个窗格上 ⇒ 当前窗格那一行。
#[test]
fn the_list_picks_the_same_carrier_as_the_gate() {
    let (rows, odd) = parse_rows(
        "$1\t%1\t0\t1\tw-cc\t1\t5\tsid-l\t\t\tclaude\t/a\tt\n\
         $1\t%2\t1\t1\tw-cc\t1\t5\tsid-l\t\t\tclaude\t/a\tt\n\
         $1\t%3\t0\t1\tw-cc\t1\t5\tsid-m\t\t\tclaude\t/a\tt\n",
    );
    assert!(!odd);
    assert_eq!(
        rows.iter()
            .map(|r| (r.sid.as_str(), r.pane.as_deref()))
            .collect::<Vec<_>>(),
        [("sid-l", Some("%2")), ("sid-m", Some("%3"))]
    );
}

/// ★ 列整台会话（会话快照的探测器）只用一份格式串：`list_sessions` 那一发的 `-F` 就是 [`TMUX_LS_FMT`]，
/// 切行照 [`session_rows`]（观测那一跳读的也是它）。
#[test]
fn list_sessions_asks_with_the_one_ls_format() {
    let prod = guard_core::production_code(include_str!("../../../src/backend/host/tmux.rs"));
    let flat: String = prod.chars().filter(|c| !c.is_whitespace()).collect();
    assert!(
        flat.contains("\"list-sessions\",\"-F\",TMUX_LS_FMT]"),
        "`list_sessions` 的 `-F` 不是 `TMUX_LS_FMT` —— 列整台会话又长出了第二份格式串"
    );
    assert!(
        !flat.contains("constLIST_FMT"),
        "`LIST_FMT` 还在 —— 两份格式串"
    );
}

/// ★ 段数下溢 ⇒ 整行不当好数据；好行一个会话 × 挂着的每个 sid 一行（活动窗格那个在前）。
///
/// 要求住址：`INVARIANTS §49`（tmux 打印通道必须是 UTF-8，段数下溢出声）。
#[test]
fn a_tab_starved_line_is_dropped_instead_of_becoming_a_session() {
    assert!(
        session_rows("kr96_cc-deadval1\n").is_empty(),
        "下溢的行被当成了一个会话名"
    );
    assert_eq!(
        session_rows(
            "a-cc\t/p\tclaude\t1\t1\tsid-a\t$1\tsid-a \n\
             b-cc\t/p\tbash\t0\t1\t\t$2\t \n\
             c-cc\t/p\tclaude\t0\t2\t\t$3\tsid-c  sid-d \n\n"
        ),
        vec![
            row("a-cc", "sid-a"),
            row("b-cc", ""),
            row("c-cc", "sid-c"),
            row("c-cc", "sid-d")
        ]
    );
}

/// pane id 是 `%` ＋ 十进制数字；空串、只有 `%`、别的形状一律不认（空 target 会被 tmux 静默解析成「某个会话」）。
#[test]
fn a_pane_id_is_percent_and_digits() {
    for ok in ["%0", "%12"] {
        assert!(pane_id_ok(ok), "{ok}");
    }
    for bad in ["", "%", "12", "%1a", "$1", "@0", " %1", "%1 ", "%-1"] {
        assert!(!pane_id_ok(bad), "{bad:?}");
    }
}

/// `$TMUX` 的值是 `socket,pid,session`：socket 取第一段；空 ⇒ 没有。
#[test]
fn the_socket_is_the_first_field_of_the_tmux_env() {
    assert_eq!(
        socket_of("/tmp/tmux-1000/default,123,0"),
        Some("/tmp/tmux-1000/default")
    );
    assert_eq!(socket_of("/s"), Some("/s"));
    assert_eq!(socket_of(""), None);
    assert_eq!(socket_of(",1,2"), None);
}

/// 宿主口里每一发 `list-panes`（名单 · 一个会话的窗格）都把 `-u` 排在子命令之前（`INVARIANTS §49`）：
/// 放到后面是 `rc=1 + unknown flag -u`，列窗格那一处不看退出码 ⇒ 会退化成「一个窗格都没有」。
#[test]
fn every_list_panes_asks_for_a_utf8_client_before_the_subcommand() {
    let prod = guard_core::production_code(include_str!("../../../src/backend/host/tmux.rs"));
    let sites: Vec<usize> = prod
        .match_indices("\"list-panes\"")
        .map(|(i, _)| i)
        .collect();
    assert_eq!(
        sites.len(),
        2,
        "`list-panes` 的处数变了（名单 · 一个会话的窗格）"
    );
    for at in sites {
        let open = prod[..at].rfind('[').expect("它在一个数组里");
        let before = &prod[open..at];
        assert!(
            before.contains("UTF8_CLIENT_FLAG") && !before.contains(']'),
            "有一发 `list-panes` 没把 `-u` 放在子命令之前：{before:?}"
        );
    }
    let flat: String = prod.chars().filter(|c| !c.is_whitespace()).collect();
    assert!(!flat.contains("\"list-panes\",UTF8_CLIENT_FLAG"));
}
