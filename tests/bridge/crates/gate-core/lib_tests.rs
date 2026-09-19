use super::*;

#[test]
fn old_cc_prefix_is_still_recognised() {
    // 删掉这一支 = 把用户正在跑的老会话变成失管会话（见 `is_ccm_tmux_name` 头注）。
    assert!(is_ccm_tmux_name("cc-abc12345"));
    assert!(is_ccm_tmux_name("cc-proj"));
    assert!(is_ccm_tmux_name("cc-abc12345-2")); // pickFreshTmuxName 的 -N 变体
    assert!(!is_ccm_tmux_name("cc-")); // 只前缀无体
}

#[test]
fn new_cc_suffix_shape_is_recognised() {
    assert!(is_ccm_tmux_name("abc12345-cc"));
    assert!(is_ccm_tmux_name("abc12345-cc-2"));
    assert!(is_ccm_tmux_name("my-proj-cc"));
    assert!(!is_ccm_tmux_name("-cc")); // `<X>` 为空的退化名
    assert!(!is_ccm_tmux_name("foo-ccx"));
    assert!(!is_ccm_tmux_name("foo-cc-bar")); // `-cc-` 后面不是纯数字
}

#[test]
fn other_peoples_sessions_are_not_ours() {
    assert!(!is_ccm_tmux_name("web")); // 用户自己的会话
    assert!(!is_ccm_tmux_name("mycc-x")); // 非前缀
    assert!(!is_ccm_tmux_name("foo_cc")); // cc-bus 的 `_cc` 命名空间（ROADMAP §6 待决 #2）
}

#[test]
fn shell_metacharacters_never_pass_the_name_half() {
    // 这几条守的是「名字命中 ⇒ 跳过远端核验」那条零 IO 快路。
    assert!(!is_ccm_tmux_name("cc-a b")); // 空格
    assert!(!is_ccm_tmux_name("cc-a;rm")); // 分号
    assert!(!is_ccm_tmux_name("cc-a$x")); // 元字符
    assert!(!is_ccm_tmux_name("cc-a:b")); // tmux 目标语法
    assert!(!is_ccm_tmux_name("=cc-a")); // tmux 精确匹配前缀
}

#[test]
fn the_union_allows_by_name_without_asking_the_remote() {
    // 名字命中时**连 remote_sid 都不看** —— 三种取值结论必须一致。
    for sid in [None, Some(""), Some("whatever")] {
        assert_eq!(gate2("cc-abc12345", sid), Gate2::AllowedByName);
    }
    assert!(!needs_remote_sid("cc-abc12345"));
}

#[test]
fn the_union_allows_a_custom_name_only_when_the_remote_sid_is_set() {
    assert!(needs_remote_sid("e2e-custom"));
    assert_eq!(
        gate2("e2e-custom", Some("abc123")),
        Gate2::AllowedByRemoteSid
    );
    assert_eq!(gate2("e2e-custom", Some("")), Gate2::Rejected);
    assert_eq!(gate2("e2e-custom", None), Gate2::Rejected);
}

#[test]
fn unknown_state_fails_closed() {
    // 探测失败（`None`）绝不能等价于「允许」。这条是 fail-closed 的落点。
    assert!(!gate2("someones-session", None).allowed());
    assert!(!gate2("someones-session", Some("")).allowed());
}

#[test]
fn the_wire_names_are_pinned_not_derived_from_debug() {
    // 夹具与 e2e 都比这几个字面量；跟着 `Debug` 漂就成了「两侧一起漂」。
    assert_eq!(Gate2::AllowedByName.as_str(), "allowed_by_name");
    assert_eq!(Gate2::AllowedByRemoteSid.as_str(), "allowed_by_remote_sid");
    assert_eq!(Gate2::Rejected.as_str(), "rejected");
}
