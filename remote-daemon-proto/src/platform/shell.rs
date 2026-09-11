//! `K-R55`（2026-09-11）：**「把一条命令串交给 POSIX shell」这一族平台原语**。
//!
//! # 它从哪来
//!
//! `K-R52` 立 [`super::cfgless_guard`] 那一拍，A2（编得过、跑不对）堆里挂着
//! `observe/watcher.rs` 的两处 `Command::new("sh")`，签字栏逐字写着
//! 「该进适配层（`K33` 裁定二），今天没进 …… `K-R52` 的写区不含 `observe/`」。
//! 本模块就是那两处的新住址。
//!
//! # 为什么非 unix 那一臂是 `None`，不是 `cmd /C`
//!
//! 经这条口送出去的是**POSIX shell 脚本**（`command -v` 门控、`exec`、`2>/dev/null`），
//! 交给 `cmd.exe` / PowerShell 不是「另一种写法」，是**另一种语言** ——
//! 编一个 `cmd /C` 出来等于给一个答不上来的问题编一个看起来无害的答案
//! （`fallback_guard` 头注里 `pid_alive` 那个地雷的同一形）。
//! ⇒ 这一臂诚实地说「本平台没有这一格」，由调用方决定怎么降级。
//!
//! # 行为面：**这一拍在 unix 上逐字节不变，在非 unix 上也不变**
//!
//! 搬之前，非 unix 上 `Command::new("sh").output()` 会返回 `Err`（找不到 `sh`），
//! 两个调用方各自落进自己的 `Err` 臂（`Unobservable` / `(None, None)`）。
//! 搬之后那两条路走的是 `None` 臂，**落点逐字相同** ——
//! 变的只有一件事：先前那是「碰巧撞出来的」，现在是**写出来的**。

/// 备一条 `sh -c <脚本>`。**非 unix 上回 `None`** —— 那里没有 `sh`。
///
/// 只负责「备好这条命令」，不 `spawn`、不 `output` —— 送出去那一下归调用方，
/// 它还要往上挂环境变量（`UTF8_CLIENT_ENV` 那一族）与决定怎么读回来。
///
/// ⚠ 本函数**不判脚本内容**：脚本是不是 POSIX 语法、跑不跑得动，那是调用方的事。
pub(crate) fn posix_shell(script: &str) -> Option<std::process::Command> {
    #[cfg(unix)]
    {
        let mut c = std::process::Command::new("sh");
        c.arg("-c").arg(script);
        Some(c)
    }
    #[cfg(not(unix))]
    {
        let _ = script;
        None
    }
}

#[cfg(test)]
mod tests {
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
}
