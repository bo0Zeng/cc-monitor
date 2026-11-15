//! 账号那几条别名的名字怎么起（shell 函数名的规矩）。

use super::*;

/// 缺省那一家的号（清单那一格缺席）。
fn own(name: &str) -> AliasOwner {
    AliasOwner {
        name: name.into(),
        agent: None,
    }
}

#[test]
fn alias_names_follow_the_shell_function_rules() {
    assert_eq!(
        alias_name(&own("work"), Launch::Direct).as_deref(),
        Some("workcc")
    );
    assert_eq!(
        alias_name(&own("work"), Launch::Tmux).as_deref(),
        Some("workcct")
    );
    assert_eq!(
        alias_name(&own("work"), Launch::Managed).as_deref(),
        Some("workcch")
    );
    assert_eq!(
        alias_name(&own("side-2"), Launch::Direct).as_deref(),
        Some("side2cc")
    );
    assert_eq!(
        alias_name(&own("a_b"), Launch::Tmux).as_deref(),
        Some("a_bcct")
    );
    assert_eq!(
        alias_name(&own("2x"), Launch::Managed).as_deref(),
        Some("_2xcch")
    );
    assert_eq!(alias_name(&own("---"), Launch::Tmux), None);
}

/// 每个号那几条别名照那个号那一家的 wrapper 起：两家替身各一个号 ⇒ 各用各家的 wrapper；那一家没有 wrapper ⇒ 不起。
#[test]
fn alias_names_follow_the_family_of_the_account() {
    let reg = [
        crate::agents::fake::family_row("stand-in", "CCM_STAND_IN_DIR", "si", true, || None),
        crate::agents::fake::family_row("stand-two", "CCM_STAND_TWO_DIR", "st", false, || None),
        crate::agents::fake::full_row(|| None),
    ];
    assert_eq!(
        alias_name_among(&reg, "stand-in", "work", Launch::Direct).as_deref(),
        Some("worksi")
    );
    assert_eq!(
        alias_name_among(&reg, "stand-two", "work", Launch::Tmux).as_deref(),
        Some("workstt")
    );
    assert_eq!(
        alias_name_among(
            &reg,
            crate::agents::fake::AGENT_KIND,
            "work",
            Launch::Direct
        ),
        None,
        "那一家没有 wrapper ⇒ 不起"
    );
}
