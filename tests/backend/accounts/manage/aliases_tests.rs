//! 账号那几条别名的名字怎么起（shell 函数名的规矩）。

use super::*;

#[test]
fn alias_names_follow_the_shell_function_rules() {
    assert_eq!(alias_name("work", Form::Plain).as_deref(), Some("workcc"));
    assert_eq!(alias_name("work", Form::Tmux).as_deref(), Some("workcct"));
    assert_eq!(alias_name("work", Form::Hosted).as_deref(), Some("workcch"));
    assert_eq!(
        alias_name("side-2", Form::Plain).as_deref(),
        Some("side2cc")
    );
    assert_eq!(alias_name("a_b", Form::Tmux).as_deref(), Some("a_bcct"));
    assert_eq!(alias_name("2x", Form::Hosted).as_deref(), Some("_2xcch"));
    assert_eq!(alias_name("---", Form::Tmux), None);
}
