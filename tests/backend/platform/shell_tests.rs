//! # 要求住址：`设计/00 §1.5.0`（平台原语只住 `platform/`）＋ `设计/01 §5 D7`（本平台没有的那一格如实说没有）
//!
//! 核原文：`设计/00 §1.5.0` 逐字「`platform/` —— 唯一允许出现平台原语与平台 `cfg` 的地方」—— `shell/mod.rs::posix_shell` 是「把命令串交给
//! POSIX shell」这一格在那里的住址，本族判它在 unix 上原样带出脚本；非 unix 回 `None`、不编一个 `cmd /C` 冒充，对 `D7`「失败要显式」。
//! ⚠ 主住址偏弱：原文讲的是「只许住在这里」，本族判的是原语的行为。〔JA1 点址 2026-09-24〕

/// ★ unix 上：真的备出了一条 `sh -c <脚本>`，而且脚本是**传进去的那一份**。
///
/// ⚠ 断的是 `get_args()` 里那两段（`-c` 与脚本本身），不是「程序名叫 sh」——
/// 后者在这个仓里是写死的字面量，断它等于断一句源码。
#[test]
#[cfg(unix)]
fn the_posix_shell_carries_the_script_it_was_given() {
    let c = super::posix_shell("echo 甲乙丙").expect("unix 上必须备得出来");
    let args: Vec<String> = c
        .get_args()
        .map(|a| a.to_string_lossy().into_owned())
        .collect();
    assert_eq!(
        args,
        vec!["-c".to_string(), "echo 甲乙丙".to_string()],
        "备出来的 argv 不是 `-c <传进去的那份脚本>` —— 脚本被改写或被丢了"
    );
    // 反空真：换一份脚本，argv 跟着变（否则上面那条可能断在一个常量上）。
    let other = super::posix_shell("true")
        .expect("unix 上必须备得出来")
        .get_args()
        .map(|a| a.to_string_lossy().into_owned())
        .collect::<Vec<_>>();
    assert_ne!(args, other, "换了脚本 argv 却没变 —— 脚本那一维是个常量");
}

/// ★ 非 unix 上：**说得出「本平台没有这一格」**，不是编一个 `cmd /C` 出来。
#[test]
#[cfg(not(unix))]
fn there_is_no_posix_shell_off_unix() {
    assert!(
        super::posix_shell("echo x").is_none(),
        "非 unix 上竟然备出了一条 shell 命令 —— 那只能是编出来的"
    );
}
