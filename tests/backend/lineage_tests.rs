//! 会话血缘（`lineage.rs`）的判据：中转那一处怎么认「谁起的谁」· 只记第一次 · 不写盘的那几种 · 清旧。

use super::*;

const T: &str = "0a1b2c3d4e5f6071";

fn o<'a>(token: &'a str, parent: Option<&'a str>) -> Option<Origin<'a>> {
    Some(Origin { token, parent })
}

/// 来处第一次被用 ⇒ 绑给发这一发的会话（它就是这条地址交给的那个），自己不记父；之后别的会话拿同一条地址来 ⇒ 父 ＝ 绑的那个。
#[test]
fn the_first_session_on_an_origin_owns_it_and_later_ones_are_its_children() {
    let mut b = Book::default();
    assert!(b.saw(o(T, None), "s-owner", "claude-code", 100));
    assert_eq!(
        b.parent_of("s-owner"),
        None,
        "地址交给的那个会话自己不是谁的孩子"
    );
    assert!(b.saw(o(T, None), "s-kid", "claude-code", 101));
    assert_eq!(b.parent_of("s-kid"), Some("s-owner"));
    assert_eq!(b.children_of("s-owner"), vec!["s-kid"]);
}

/// 同一个会话再发（subagent 带的也是父会话的编号，同一格）⇒ 不改、不写盘。
#[test]
fn the_owner_sending_again_changes_nothing() {
    let mut b = Book::default();
    b.saw(o(T, None), "s-owner", "claude-code", 100);
    let before = b.clone();
    assert!(!b.saw(o(T, None), "s-owner", "claude-code", 102));
    assert_eq!(b, before);
}

/// 地址里带了父（`ccm` 在一个会话的 shell 里起的）⇒ 绑来处的那个会话的父 ＝ 它。
#[test]
fn a_parent_in_the_address_is_recorded_for_the_owner() {
    let mut b = Book::default();
    assert!(b.saw(o(T, Some("s-p")), "s-new", "claude-code", 100));
    assert_eq!(b.parent_of("s-new"), Some("s-p"));
    assert_eq!(b.agent_of("s-new"), Some("claude-code"));
}

/// 父只记第一次：之后换一条地址（续接时 `ccm` 新铸的来处）再来，父不改。
#[test]
fn a_parent_is_recorded_once_and_never_rewritten() {
    let mut b = Book::default();
    b.saw(o(T, Some("s-p")), "s-new", "claude-code", 100);
    assert!(b.saw(
        o("ffff000011112222", Some("s-q")),
        "s-new",
        "claude-code",
        200
    ));
    assert_eq!(b.parent_of("s-new"), Some("s-p"), "父被改写了");
}

/// 自己当自己的父 · 没有会话头 · 没有来处 ⇒ 不记、不写盘。
#[test]
fn self_parent_no_sid_and_no_origin_record_nothing() {
    let mut b = Book::default();
    assert!(
        b.saw(o(T, Some("s-x")), "s-x", "claude-code", 100),
        "来处照绑"
    );
    assert_eq!(b.parent_of("s-x"), None, "自己当自己的父");
    let mut c = Book::default();
    assert!(!c.saw(o(T, None), "", "claude-code", 100));
    assert!(!c.saw(None, "s-a", "claude-code", 100));
    assert_eq!(c, Book::default());
}

/// 每天头一次被用到刷新时刻（写一次盘），同一天不写；90 天没被用到的整条清掉。
#[test]
fn entries_are_refreshed_daily_and_dropped_after_ninety_days() {
    let mut b = Book::default();
    b.saw(o(T, None), "s-owner", "claude-code", 0);
    b.saw(o(T, None), "s-kid", "claude-code", 0);
    assert!(!b.saw(o(T, None), "s-kid", "claude-code", SEEN_REFRESH - 1));
    assert!(
        b.saw(o(T, None), "s-kid", "claude-code", SEEN_REFRESH),
        "隔一天第一发该刷新"
    );
    // 另一条地址的会话在很久以后被看见 ⇒ 顺手清掉 90 天没被用到的。
    let late = SEEN_REFRESH + DROP_AFTER + 1;
    b.saw(o("ffff000011112222", None), "s-late", "claude-code", late);
    assert_eq!(b.parent_of("s-kid"), None, "90 天没被用到的父子没清");
    assert!(
        b.saw(o(T, None), "s-again", "claude-code", late),
        "来处没清：又能绑"
    );
    assert_eq!(
        b.parent_of("s-again"),
        None,
        "清掉的来处被当成新的：绑给它自己"
    );
}

/// 盘上那一份：写 → 读回同样的；读不懂的那一份不覆盖。
#[test]
fn the_store_round_trips_and_does_not_overwrite_an_unreadable_file() {
    let dir = std::env::temp_dir().join(format!("ccm-lineage-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join(FILE_NAME);
    let store = LineageStore::at(Some(path.clone()));
    store.saw(o(T, Some("s-p")), "s-new", "claude-code", 100);
    let again = LineageStore::at(Some(path.clone()));
    assert_eq!(again.now().parent_of("s-new"), Some("s-p"));
    std::fs::write(&path, b"{not json").unwrap();
    let broken = LineageStore::at(Some(path.clone()));
    broken.saw(o("ffff000011112222", None), "s-b", "claude-code", 100);
    assert_eq!(
        std::fs::read(&path).unwrap(),
        b"{not json",
        "读不懂的被覆盖了"
    );
    let _ = std::fs::remove_dir_all(&dir);
}
