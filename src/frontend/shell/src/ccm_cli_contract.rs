//! `cc-spawn` 怎么找到并使唤 `ccm` —— **那条边**的契约。
//!
//! # 🔴 `K-R48` 第二拍（09-11）：本模块从 2773 行砍到今天这个样子，理由写清楚
//!
//! 它原来的题目是「`shared/ccm` 这份 **bash 脚本**的强度契约」：一张 needle 表 ＋ 一个
//! [`Strength`] 读数 ＋ 一条基线，量的是「那份脚本里还有没有这几个串」「`-t` 目标扫到几处」。
//! 〔用@09-11 `K33`〕逐字「后端**只有一个**…**不要有什么 bash 脚本**，**不要有什么单独的 ccm**」
//! ⇒ **那份脚本删了，这些读数没有被测对象了**。
//!
//! 删掉的 22 条判据与三张表（`REQUIRED_NEEDLES` / `LEDGER` / `BACKEND_BACKED_PATHS`、
//! `measure` / `scan_t_targets` / `pin_t_def` 〔散文墓碑〕 / `BASELINE`）逐条判词住
//! `tests/evidence/K-R48-356-verdicts.tsv` 的同族条目；它们守的**性质**去了哪里，逐条写在
//! 件文件 `features/K-R48-…md` 的 `§8c`。
//!
//! # 留下来的这 7 条为什么留
//!
//! 它们**一条都不读那份脚本** —— 读的是 `src/shared/cc-bus/scripts/cc-spawn` 与
//! `src/shared/cc-bus/SKILL.md`。钉的是「**别人怎么找到并使唤 `ccm`**」这条边：
//! 解析错了、名字自己拍了、撞名乱重试了，后面整条 CLI 契约都无从谈起。
//! 那条边今天仍然存在，只是另一头从「一个 bash 脚本」换成了「后端本体」。
//!
//! ⚠ **住址**：`K-R48` 第一拍说「最自然的新家是 `src/frontend/shell/src/cc_bus.rs`」，
//! 而本拍写区里**没有** `cc_bus.rs` ⇒ 留在原文件，不擅自扩写区。搬不搬归 PM。
//!
//! # ★★ 诚实边界（原 `P4b-Y3`，逐字保留）：它钉的是仓内那份，而本机真正在跑的不是它
//!
//! 实测：`~/.local/bin/cc-*` 全是指向 `~/.claude/skills/cc-bus/scripts/` 的 symlink，
//! 而那份是 07-18 的；仓内这份是另一份。`tool_registry.rs` 声明了
//! `src/shared/cc-bus` → `.claude/skills/cc-bus` 的部署映射，但那是**纯声明表**，
//! **没有任何东西真的按它部署**。
//! ⇒ 本文件里所有判据**证明的是仓内那份的性质，不是本机行为**。别读成「机器上就是这样」。
//! 〔`K-R48` 第一拍 PM 审计逐字复核过这一格：「**工具是，文件不是**」。〕
//!
//! 注：本模块整体在 `#[cfg(test)]` 内，非测试构建为空、零运行期开销。

#![cfg(test)]

#[cfg(test)]
#[path = "../../../../tests/frontend/shell/ccm_cli_contract_tests.rs"]
mod tests;
