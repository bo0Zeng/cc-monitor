//! **每个 shell 脚本要么进 shellcheck，要么登记为什么不进**〔audit-0805 08-08，Phase G 第 71 件〕。
//!
//! # 洞：人群是手写分组
//!
//! shellcheck 的人群是一行手写的 glob 分组（今天住门禁 `tests/scripts/gate.sh` 的 `GATE_SHELLCHECK_GLOBS`，
//! CI 那个 job 调门禁的 `shellcheck` 格）。**新脚本落在任何一个分组之外 ⇒ 静默不被 lint**。
//! 08-08 实测：全仓 46 个 shell 脚本，那条表达式覆盖 44 —— 漏的是 `tests/e2e/fake-claude`
//! （那一组的 glob 是 `tests/e2e/*.sh`，它没有后缀）与 `src/shared/ccm-aliases.sh`。
//! ⇒ 本模块判「盘上每个 shell 脚本要么被某个 glob 盖住、要么登记豁免」；
//! 原先另有一条「覆盖份数 == CI 里那条计数地板」，人群对拍之后那个数不再承重，连同地板一起删了。
//!
//! # 「刻意不含」不能只是散文（E12）
//!
//! `src/shared/ccm-aliases.sh` 的排除是**有理由的、先核过的**：它是供 `source` 的片段、
//! 没有 shebang（SC2148 是它的构造性属性），而它会被写进用户 shell profile、
//! 还在 UI 面板里展示供手动复制 —— 为过 lint 往里塞 `# shellcheck shell=bash`
//! 等于往用户配置和界面文案里掺 lint 噪音。理由成立，**但当时它只写在 CI 配置的注释里**。
//! 本模块把它登记成一条**豁免**：默认拒绝，豁免要写理由，且豁免行不许变成死行。
//!
//! ⚠ 如实记一笔量到的事：`shellcheck -s bash src/shared/ccm-aliases.sh` 今天**零 error**
//! —— 也就是说那条豁免是**可以撤销**的（代价是上面说的用户可见噪音）。
//! 写在这里是为了让下一个人不必重量一次，**不是**在建议撤销。
//!
//! # 它服务哪条要求：**`INVARIANTS §46`**
//!
//! 违反此约束见 `src/doc/INVARIANTS.md` § 46（与 `e2e_gate_registry` 同一条）。下面那段是升格**之前**的缺条原账，留着是账：
//!
//! ## 升格前：**这里没有条**〔`P20` 第二刀 2026-09-22 核过原文〕
//!
//! 同 `crate::e2e_gate_registry` —— **两族缺的是同一条**（「一套检查要么进门禁，
//! 要么登记为什么不进；人群从清单全集派生，默认拒绝」）。逐条核原文的读数与缺口
//! 登记在，升格与否要用户拍（`§4.11.7 ⑤`）。
//!
//! ⚠ 这条性质**已被违反过两次，两次都是人发现的**：本模块头上那条 08-08 实测
//! （46 个脚本只覆盖 44），以及 `ci.yml` 自陈 G2 那套 e2e「当时忘了接线」。

#[cfg(test)]
#[path = "../../../../tests/frontend/shell/shell_lint_registry_tests.rs"]
mod tests;
