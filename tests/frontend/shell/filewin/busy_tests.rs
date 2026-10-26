//! 各文件窗口「会打断什么」那张表：窗口说了就记、管子断了就删、现读时加起来。

use super::busy::*;
use filewin_contract::Busy;

fn b(unsaved: u32, running: u32) -> Busy {
    Busy { unsaved, running }
}

/// 两扇窗各说一次 ⇒ 现读的是两扇加起来；一扇断了 ⇒ 它那一份不再算。
#[test]
fn the_book_sums_what_each_window_said_and_drops_a_gone_window() {
    let book = Book::default();
    let (one, two) = (book.enrol(), book.enrol());
    assert_ne!(one, two);
    assert_eq!(book.total(), FilewinTotals::default(), "还没人说，就有数了");
    book.set(one, b(1, 0));
    book.set(two, b(2, 3));
    assert_eq!(
        book.total(),
        FilewinTotals {
            windows: 2,
            unsaved: 3,
            running: 3
        }
    );
    book.set(two, b(0, 0));
    assert_eq!(
        book.total(),
        FilewinTotals {
            windows: 1,
            unsaved: 1,
            running: 0
        },
        "说回 0 的那扇窗还算成手上有事"
    );
    book.forget(one);
    assert_eq!(book.total(), FilewinTotals::default(), "管子断了，那扇窗的数还留着");
}

/// 窗口说的那一句解不出来 ⇒ 用错了（不当成 0 记下）。
#[test]
fn a_garbled_report_is_misuse_and_changes_nothing() {
    let book = Book::default();
    let id = book.enrol();
    book.set(id, b(1, 0));
    assert!(book.said(id, b"not json").is_err());
    assert_eq!(book.total().unsaved, 1);
    assert!(book.said(id, br#"{"unsaved":0,"running":2}"#).is_ok());
    assert_eq!(
        book.total(),
        FilewinTotals {
            windows: 1,
            unsaved: 0,
            running: 2
        }
    );
}
