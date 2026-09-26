//! **POSIX 单引号 quote** —— Rust 侧唯一的一份实现；〔TL3〕外加它的伴生件：自由文本值在 quote 之前的拒绝集；
//! 〔DUP1〕以及标识符类（sid · 模型名 · 账号名）的放行判定（`INVARIANTS §47` 的 ① ② 两层都住这里）。
//!
//! # 它为什么只剩一件事（P4b，§1.4b）
//!
//! U8c-1 建这个 crate 是为了给「起会话的渲染」找个家 —— 而当时 monitor 侧**一个边界都没有**
//! （平铺五十多个 `.rs`），于是共享 crate 成了唯一的落点。架构审计 2026-08-03 点破：
//! 那批东西（维度注册表 + `render_ccm_invocation` + 载荷编译）**就是决策内核**，
//! 而后端对整个 crate 的用量只有一行 `posix_quote`。**它不是共享的，是没处放的。**
//!
//! P4a 给 monitor 划出了 `src/bridge/src/backend/control/`，P4b 把那批东西搬了进去。
//! 留在这里的判据是**「backend 真的在用」**：
//!
//! | 项 | backend 用量 | 结论 |
//! |---|---|---|
//! | `posix_quote` | `control/tmux_hook.rs::sq` 一处 | **留** |
//! | `config_dir_command_safe` / `UNSET_CONFIG_DIR_PREFIX` / 载荷一族 / `cli` 决策内核 | **零** | 搬进 `backend/control/`（P4b） |
//!
//! ⚠ **「渲染一条 shell 命令串」永远属于开终端的那一侧**：§1.3 把最终 exec 钉在用户自己的
//! 终端进程里，而 U8a-2b 把后端的执行面定成 **argv 直传、不过 shell**。
//! 所以那批东西搬去 monitor 不是权宜，是归属地 —— **不会再搬回来**。
//!
//! # 为什么这一件仍然值得一个共享 crate
//!
//! 收口前全仓有**五份逐字节相同**的实现，靠巧合保持一致、从来没红过（账本 S5）。
//! 两侧（monitor 5 处 + backend 1 处）都要它，而两个二进制不共享源码树 ⇒ 共享 crate 是唯一载体。
//! 「只许一个实现」由 monitor 侧的 `quote_singleton_guard` 机检（它把本文件钉为 `SOLE_HOME`）。
//!
//! # 名字（P4c）
//!
//! **P4b 之前它叫 `launch-core`** —— 那时它持有决策内核，名字还说得过去。缩到只剩 quote 之后
//! 那个名字就成了说谎，P4c 改成 `shell-quote-core`：与 TS `src/shell-quote.ts`、
//! `shared/ccm::sq` 同族，**一眼看出这三份是同一件事**（跨语言那两份由黄金串夹具对拍）。
//! ⚠ 计划文档（`.claude/planned-build/`）里的 `launch-core` 是当时的实况，刻意没改。

/// 〔TL3 · `INVARIANTS §47` ② · 主会话 09-26 按 V131 裁〕**自由文本**值（cwd · 目录 · 远端子命令的 argv · 别名词 …）
/// 在唯一的 quote 之前的**拒绝集**：只收 NUL / CR / LF。
///
/// 为什么只收这三个、**不拒 shell 元字符**：`Bob's notes` · `照片 (2019)` 这类真实名字里 `'` `(` `)` `&` 都合法，
/// 元字符交给 [`posix_quote`]（单引号里没有一个会被解释）；拒它们就是 §47 自己写的「拒过头也算违反」。
/// 而这三个字符 quote 挡不住它们的后果：NUL 截断参数、CR / LF 在交互 shell 里（`tmux send-keys` 那一跳）等于按了回车。
/// 形式判定（绝对路径 · 无 `..` 段 …）**按各自语境**写在调用处（本机 / 远端、POSIX / Windows 的「绝对」不是同一件事）。
/// 本仓自管的值（配置目录 · 后端落点）不走这一条，走全表（`payload.rs::config_dir_command_safe` 那一族）。
pub const FREE_TEXT_REFUSED: [char; 3] = ['\0', '\r', '\n'];

/// 见 [`FREE_TEXT_REFUSED`]：这个自由文本值能不能交给唯一的 quote 拼进 shell。
pub fn free_text_ok(s: &str) -> bool {
    !s.contains(FREE_TEXT_REFUSED)
}

/// **POSIX 语境下的自由文本路径**（远端 / 本机 POSIX 的工作目录 · 文件窗口的当前目录）能不能交给 [`posix_quote`]：
/// 形式 = 绝对（`/` 开头）· 没有 `..` 段；拒绝集 = [`free_text_ok`]。**不拒 shell 元字符**。
/// 空串不在这里判：各调用处对「空」各有自己的话（载荷「空值不等于没设」· 文件窗口「不带 cd」）。
/// 住这里而不住 monitor 的载荷模块：文件窗口进程也要它，而那个进程够到 app 侧只许走通道（`filewin/boundary_tests.rs`）。
pub fn posix_free_path_ok(p: &str) -> bool {
    p.starts_with('/') && !p.split('/').any(|seg| seg == "..") && free_text_ok(p)
}

// ═══════════════════════════════════════════════════════════════════════════
// 〔DUP1 · `INVARIANTS §47` ①〕**标识符类**的放行判定：闭集白名单 ＋ 不许 `-` 开头 ＋ 钉上界。
//
// 与上面 ② 自由文本那一层、下面唯一的 quote 同住（TL3 的先例：判定与 quote 同住）。
// 每一条都是**全仓唯一的一份**（`设计/01 §5` D1；登记表 `tests/judgment-single-home.vitest.ts`）：
// 前端不判（线上校验交后端判，`设计/90 §3` 判据 2），monitor 渲染 · 后端 ccm 都调这里。
// 首字符一律要 ASCII 字母数字：`-` 开头会被下游当选项解析（`--model -x` · `resume --x`），quote 挡不住。
// ═══════════════════════════════════════════════════════════════════════════

/// session id 的上界（与分叉找文件那条路逐字同，见 [`session_id_ok`]）。
pub const SESSION_ID_MAX: usize = 64;

/// **session id**（resume 的 sid · `--ccm-sid` / `@ccm_sid` 身份标记 · 分叉的 sid 与消息 uuid · 按 sid 找会话文件）。
///
/// 规则 = 1..=[`SESSION_ID_MAX`] 位 · 首字符 ASCII 字母数字 · 其余 `[A-Za-z0-9-]`。
/// 取的是今天各处规则的**交集**（原 `branch_core::is_plain_sid` 的字符集与上界 ＋ 原 TS `isValidSessionId` 的「不许 `-` 开头」）⇒
/// 没有一处因此放宽；真实 sid 是 UUID（Claude / Codex 同形，36 位），全过。
/// 它挡掉 `..` `/` `\` 与任何能拼出别处路径的字符（`INVARIANTS §41.6` 收窄第 3 条），也挡掉选项注入。
/// ⚠ 后端 `resolve` 那条（`resolve_query.rs`）**刻意不接**：它的行为冻结给仓外 aterm（`V126`，改一格 = 跨仓契约变更）。
pub fn session_id_ok(s: &str) -> bool {
    let mut cs = s.chars();
    matches!(cs.next(), Some(c) if c.is_ascii_alphanumeric())
        && s.len() <= SESSION_ID_MAX
        && cs.all(|c| c.is_ascii_alphanumeric() || c == '-')
}

/// 模型名的上界：盖得住 Bedrock 推理配置的 ARN。
pub const MODEL_NAME_MAX: usize = 256;
/// 模型名在字母数字之外还放行的字符（首字符除外）：`.` `_` `-`（完整 id）· `:`（Bedrock `…-v1:0`、ARN）·
/// `@`（Vertex `…@20250929`）· `/`（网关 `anthropic/…`、ARN）· `[` `]`（`sonnet[1m]`）。
pub const MODEL_NAME_EXTRA: &str = "._-:@/[]";

/// **模型名**（`ANTHROPIC_MODEL` · `--model`）：1..=[`MODEL_NAME_MAX`] 位 · 首字符 ASCII 字母数字 · 其余字母数字或 [`MODEL_NAME_EXTRA`]。
///
/// 只判「能不能安全地交出去」，不判「这个模型存不存在」（那是 agent 自己的事）。
pub fn model_name_ok(s: &str) -> bool {
    let mut cs = s.chars();
    matches!(cs.next(), Some(c) if c.is_ascii_alphanumeric())
        && s.len() <= MODEL_NAME_MAX
        && cs.all(|c| c.is_ascii_alphanumeric() || MODEL_NAME_EXTRA.contains(c))
}

/// 账号名的上界（与建账号的那个工具逐字同，见 [`account_name_ok`]）。
pub const ACCOUNT_NAME_MAX: usize = 32;

/// 账号名在字母数字之外还放行的字符（首字符除外）。〔DUP2〕单独提成常量：新建账号表单要在写入点先说一句，
/// 规则从这里现生成到 `src/generated/judgment-rules.ts`（不手抄）。
pub const ACCOUNT_NAME_EXTRA: &str = "_-";

/// **账号名**（`--account <名>`）：1..=[`ACCOUNT_NAME_MAX`] 位 · 首字符 ASCII 字母数字 · 其余字母数字或 [`ACCOUNT_NAME_EXTRA`]。
///
/// 与建账号的那个工具（随包 `cc-acct-iso` 的 `name_check`：「`[A-Za-z0-9_-]`，不以 `-` 或 `_` 开头，≤32」）逐字同 ——
/// 盘上每个具名账号都是它建的 ⇒ 真实账号名全过。
pub fn account_name_ok(s: &str) -> bool {
    let mut cs = s.chars();
    matches!(cs.next(), Some(c) if c.is_ascii_alphanumeric())
        && s.len() <= ACCOUNT_NAME_MAX
        && cs.all(|c| c.is_ascii_alphanumeric() || ACCOUNT_NAME_EXTRA.contains(c))
}

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
#[path = "../../../../../tests/bridge/crates/shell-quote-core/lib_tests.rs"]
mod tests;
