//! 种类怎么画：每一种一个图标；隐藏文件按名字的前导点认。

use super::*;

#[test]
fn every_class_has_its_own_icon() {
    let icons: std::collections::BTreeSet<&str> = ALL.iter().map(|k| icon(*k)).collect();
    assert_eq!(icons.len(), ALL.len());
}

#[test]
fn a_leading_dot_is_hidden() {
    assert!(is_hidden(".bashrc") && !is_hidden("a.txt") && !is_hidden(".."));
}
