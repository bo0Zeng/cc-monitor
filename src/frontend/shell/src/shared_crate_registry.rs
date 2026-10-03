//! U8c-1：把「新增共享 crate 时别漏跑」从**散文**变成机检。
//!
//! ⚠⚠ **G2（2026-08-04）换了机制，本模块整体改写** —— 原来的形态是「每个 crate 必须在
//! `ci.yml` 里出现在 test / fmt / clippy **三处**」。那条纪律存在的唯一原因是
//! **`src/frontend/shell/Cargo.toml` 当时没有 `[workspace]` 表**：六个 crate 只是 path 依赖，
//! `--all` 覆不到，只能一个个手工列。
//! **现在它们是真 workspace member**，`cargo fmt --all` / `--workspace` 自动覆盖 ⇒
//! 「补三处」这条纪律**消失了**，取而代之的是「**必须在 members 里**」。
//! ⇒ 判据跟着换靶：不再数 CI 步骤，改钉 `[workspace] members`。
//! ★ 这两条**不是被删的**（铁律 13：删判据前先证明它恒绿）——它们是**被改写成继任者**的：
//! 同一个失效模式（「静默少跑」），换了一个载体。
//!
//! # 为什么这条值得一个判据
//!
//! `ci.yml` 里那句纪律已经被违反过**两次**，而且都是事后补账发现的：
//! `branch-core` 当初漏了 fmt/clippy；`usage-core`/`acct-core` 三样**全漏**、漏了两轮
//! ——那段补账注释自己写着「它们的测试在 CI 里等于不存在」。
//!
//! 违反它不会红，只会**静默少跑**。这正是本工作区在治的那个病的形状。
//!
//! # 它服务哪条要求：`INVARIANTS §46`〔D0b 2026-09-24 升格〕
//!
//! 违反此约束见 `src/doc/INVARIANTS.md` § 46 —— 「每一套检查要么进门禁，要么登记为什么不进」。
//! 本模块的测试文件里有那一族的四格（共享 crate · CI 每一步 · `package.json` 测试脚本 · `#[ignore]`），
//! 逐条函数名列在那一条里。
//!
//! # 判据形态
//!
//! 遍历 `crates/*/Cargo.toml` 拿包名（**不是手写清单** —— 手写清单本身就是下一个漂移源），
//! 然后要求每个包名在 `ci.yml` 里同时出现在 `cargo test -p <名>`、
//! `cargo fmt --check --manifest-path crates/<名>/Cargo.toml`、
//! `cargo clippy --manifest-path crates/<名>/Cargo.toml` 三处。
//!
//! ⚠ **`vendor/` 下的不算** —— 那是 vendored 第三方（`code-picture-core`），
//! 有自己的一套（`ci.yml` 单独一步），不受本约定管。
//!
//! # ★★ 新增一个共享 crate，要补的**全部**地方（`E` 阻-1 回修补全，08-27）
//!
//! 这张清单存在的理由是它**已经漏过两次**，两次都是同一个形状：
//! 「加了东西，但没回来改那个记着『今天是几』的数」。
//!
//! | # | 要补哪儿 | 漏了会怎样 | 谁在钉 |
//! |---|---|---|---|
//! | 1 | `src/frontend/shell/Cargo.toml` 的 `[workspace] members` | **静默少跑**（`--workspace` 覆不到非成员，那个 crate 的测试从门禁里消失，不是失败是不存在） | `every_shared_crate_is_a_workspace_member` |
//! | 2 | `git add` 那个 crate 的 `Cargo.toml` | 别人（和 CI）检出会直接编不过，而你的工作树一切正常 | `every_path_dependency_is_actually_committed` |
//! | 3 | ~~`tests/scripts/gate.sh` 里手抄的包数~~ | ~~加了 crate 没回来改那个数 ⇒ 门禁红在错误的方向上~~ | **已消掉**：门禁 `cargo` 那一格从 `cargo metadata` 现取成员集合，与真跑到的包两向相等 |
//! | 4 | ~~两条自检的地板~~ | ~~余量被撑大，「少认一个 crate」不会红~~ | **已消掉**：两条自检改成**两个独立来源对拍**，自动跟上 |
//!
//! ★ 第 4 行**划掉**是本轮最要紧的一格：它原来是「要记得回来 +1」，
//! 而**忘记正是这个病本身** ⇒ 与其把它留在清单上，不如让它不再需要被记住。
//! （`F03` 漏过一次、`K-H2a` 漏过第二次，两次都是同一条断言。）
//! ⚠ 第 3 行**还是「要记得」那一类** —— 它没消掉，只是从「漏了会指错方向」变成「漏了会被点名」。

/// ★ `ci.yml` 的**读取与切块只有一个家**〔audit-0805 08-07，定框 E3〕。
///
/// 抽出来的原因是实测撞见的：`lockfile_conflict_guard` 的前提判据要问的是
/// 「跨 target check **和** `working-directory: src/backend` 在不在同一个 job」，
/// 而它当时只能在整份文件里各找一次字符串 ⇒ 两件事各自成立、关系没人钉。
/// 要钉那个关系就得会切 job 块，而切块的实现当时住在本文件的 `mod tests` 里、别人够不着 ——
/// **判据之间借不到量具，就会各写一份近似的**，那正是 E3 要防的。
#[cfg(test)]
#[path = "../../../../tests/frontend/shell/shared_crate_registry_ci_yaml.rs"]
pub(crate) mod ci_yaml;

#[cfg(test)]
#[path = "../../../../tests/frontend/shell/shared_crate_registry_tests.rs"]
mod tests;
