//! 账号那两条别名的名字怎么起（shell 函数名的规矩）。

use super::*;

#[test]
fn alias_names_follow_the_shell_function_rules() {
    assert_eq!(alias_name("work", false).as_deref(), Some("workcc"));
    assert_eq!(alias_name("work", true).as_deref(), Some("workcct"));
    assert_eq!(alias_name("side-2", false).as_deref(), Some("side2cc"));
    assert_eq!(alias_name("a_b", true).as_deref(), Some("a_bcct"));
    assert_eq!(alias_name("2x", false).as_deref(), Some("_2xcc"));
    assert_eq!(alias_name("---", true), None);
}
