use super::*;

/// ★★ **本件的核心决策**：`enter=false` 必须走**新** mode 名。
///
/// 反过来（两支都发 `send-into`）就是 F04c 存在的理由本身被抹掉：
/// `Escape` 会被 daemon 补上 `Enter` ⇒ 提交用户排队的文本。
#[test]
fn the_no_enter_case_uses_the_new_mode_name_and_the_other_reuses_the_old_one() {
    assert_eq!(mode_for(true), "send-into", "带回车那支应当复用既有 mode");
    assert_eq!(mode_for(false), "send-keys-raw", "裸键那支必须是新 mode 名");
    assert_ne!(
        mode_for(true),
        mode_for(false),
        "两支用了同一个 mode —— 那 `enter` 就没被表达出去，\
             `Escape` 会被 daemon 补上 `Enter`（= 提交用户输入框里排队的文本）"
    );
}

/// ★ 跨轨钉：这两个 mode 名 daemon 侧**真的认**。
///
/// 「monitor 发一个 daemon 不认的 mode」的后果是每次都 `invalid_args` 然后回落 SSH ——
/// **功能看着正常**（回落能用），而 daemon 那条路事实上从没被走过。
/// 那正是这一整族「切了路由但其实没切」最难发现的形状。
#[test]
fn both_mode_names_are_ones_the_daemon_actually_parses() {
    let daemon =
        guard_core::production_code(include_str!("../../../../src/backend/control/launch.rs"));
    for m in [mode_for(true), mode_for(false)] {
        let needle = format!("\"{m}\" => Some(Mode::");
        assert!(
            daemon.contains(needle.as_str()),
            "daemon 的 `Mode::parse` 里没有 `{m}` —— monitor 会一直拿 `invalid_args` \n\
                 然后回落 SSH：**功能正常、而 daemon 那条路从没走过**"
        );
    }
}

/// 与**兄弟命令**（`daemon_kill`）的拒绝文案对齐，含反向锚点：那一侧的生产段里那些串还在。
///
/// ⚠ `K-R72`（09-12）：本条原名 `the_refusal_wording_matches_the_ssh_path`，  〔散文墓碑〕
/// 反向锚点原先指 monitor 侧那条一次性 SSH 回落。**那条路整块删了**，
/// 而它守的性质（同一个拒绝只许有一种说法）没有消失 ⇒ 对照面换成 `daemon_kill.rs`，
/// 并收紧成只看**生产段**（原来是整份源码 `contains`，对面的测试里抄一份就能糊弄过去）。
/// 理由与对照关系的全文住 `daemon_kill.rs::the_refusal_wording_matches_the_sibling_command`。
#[test]
fn the_refusal_wording_matches_the_sibling_command() {
    let sibling = guard_core::production_code(include_str!("../../../../src/bridge/src/backend/control/daemon_kill.rs"));
    for (code, needle) in [
        ("no_tmux", "远端未安装 tmux"),
        ("no_such_session", "远端会话已不存在（可能已被终止）"),
        ("wrong_owner", "可能不是本工具管理的会话"),
    ] {
        assert!(
            refusal_text(code, "m").contains(needle),
            "`{code}` 的文案里没有 {needle:?}"
        );
        assert!(
            sibling.contains(needle),
            "`daemon_kill.rs` 的生产段里已经没有 {needle:?} 了 —— 两条后端命令的文案漂了"
        );
    }
    // `typed_unconfirmed` 是 daemon 独有的一档（SSH 那条路分不出来）⇒ 只要求它不被吞掉。
    assert!(
        refusal_text("typed_unconfirmed", "x").contains("未必"),
        "`typed_unconfirmed` 被说成了确定的成功或确定的失败 —— 它是「不确定」那一档"
    );
}
