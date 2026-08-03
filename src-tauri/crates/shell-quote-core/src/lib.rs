//! **POSIX 单引号 quote** —— Rust 侧唯一的一份实现。本 crate 只剩这一件事。
//!
//! # 它为什么只剩一件事（P4b，§1.4b）
//!
//! U8c-1 建这个 crate 是为了给「起会话的渲染」找个家 —— 而当时 monitor 侧**一个边界都没有**
//! （平铺五十多个 `.rs`），于是共享 crate 成了唯一的落点。架构审计 2026-08-03 点破：
//! 那批东西（维度注册表 + `render_ccm_invocation` + 载荷编译）**就是决策内核**，
//! 而 daemon 对整个 crate 的用量只有一行 `posix_quote`。**它不是共享的，是没处放的。**
//!
//! P4a 给 monitor 划出了 `src-tauri/src/backend/control/`，P4b 把那批东西搬了进去。
//! 留在这里的判据是**「daemon 真的在用」**：
//!
//! | 项 | daemon 用量 | 结论 |
//! |---|---|---|
//! | `posix_quote` | `control/tmux_hook.rs::sq` 一处 | **留** |
//! | `config_dir_command_safe` / `UNSET_CONFIG_DIR_PREFIX` / 载荷一族 / `cli` 决策内核 | **零** | 搬进 `backend/control/`（P4b） |
//!
//! ⚠ **「渲染一条 shell 命令串」永远属于开终端的那一侧**：§1.3 把最终 exec 钉在用户自己的
//! 终端进程里，而 U8a-2b 把 daemon 的执行面定成 **argv 直传、不过 shell**。
//! 所以那批东西搬去 monitor 不是权宜，是归属地 —— **不会再搬回来**。
//!
//! # 为什么这一件仍然值得一个共享 crate
//!
//! 收口前全仓有**五份逐字节相同**的实现，靠巧合保持一致、从来没红过（账本 S5）。
//! 两侧（monitor 5 处 + daemon 1 处）都要它，而两个二进制不共享源码树 ⇒ 共享 crate 是唯一载体。
//! 「只许一个实现」由 monitor 侧的 `quote_singleton_guard` 机检（它把本文件钉为 `SOLE_HOME`）。
//!
//! # 名字（P4c）
//!
//! **P4b 之前它叫 `launch-core`** —— 那时它持有决策内核，名字还说得过去。缩到只剩 quote 之后
//! 那个名字就成了说谎，P4c 改成 `shell-quote-core`：与 TS `src/shell-quote.ts`、
//! `shared/ccm::sq` 同族，**一眼看出这三份是同一件事**（跨语言那两份由黄金串夹具对拍）。
//! ⚠ 计划文档（`.claude/planned-build/`）里的 `launch-core` 是当时的实况，刻意没改。

/// POSIX 单引号 quote：整体 `'…'` 包裹，内部 `'` 断开为 `'\''`。
///
/// 与 TS `shell-quote.ts::posixQuote` 逐字节同义（对拍夹具里有带引号的样本）。
pub fn posix_quote(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('\'');
    for c in s.chars() {
        if c == '\'' {
            out.push_str("'\\''");
        } else {
            out.push(c);
        }
    }
    out.push('\'');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn posix_quote_breaks_single_quotes_the_posix_way() {
        assert_eq!(posix_quote("/p"), "'/p'");
        assert_eq!(posix_quote("a'b"), "'a'\\''b'");
        assert_eq!(posix_quote(""), "''");
    }
}
