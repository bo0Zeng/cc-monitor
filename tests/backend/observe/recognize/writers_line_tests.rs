//! 写入进程那一句（`SessionFacts::writers_line`）由后端写好：不止一个进程在写 ⇒「写入进程 ×n」；一个或没有 ⇒ 不带。

use super::*;
use copy_core::copy_text;

#[test]
fn more_than_one_writer_says_how_many() {
    assert_eq!(
        writers_line(&[4242, 5252]),
        Some(Words(copy_text("tabBarView.tab.writers", &[("n", "2")])))
    );
    assert_eq!(writers_line(&[4242]), None);
    assert_eq!(writers_line(&[]), None);
}
