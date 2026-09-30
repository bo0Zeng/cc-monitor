//! **POSIX 单引号 quote** —— Rust 侧唯一的一份实现；〔TL3〕外加它的伴生件：自由文本值在 quote 之前的拒绝集；
//! 〔DUP1〕以及标识符类（sid · 模型名 · 账号名）的放行判定（`INVARIANTS §47` 的 ① ② 两层都住这里）；
//! 〔DUP3〕外加启动期令牌的形状（① 那一层）与命令片段类（启动器，§47 ③）那一张白名单。
//!
//! # 它为什么只剩一件事（P4b，§1.4b）
//!
//! U8c-1 建这个 crate 是为了给「起会话的渲染」找个家 —— 而当时 monitor 侧**一个边界都没有**
//! （平铺五十多个 `.rs`），于是共享 crate 成了唯一的落点。架构审计 2026-08-03 点破：
//! 那批东西（维度注册表 + `render_ccm_invocation` + 载荷编译）**就是决策内核**，
//! 而后端对整个 crate 的用量只有一行 `posix_quote`。**它不是共享的，是没处放的。**
//!
//! P4a 给 monitor 划出了 `src/frontend/shell/src/`，P4b 把那批东西搬了进去。
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
//! 那个名字就成了说谎，P4c 改成 `shell-quote-core`：与 TS `src/frontend/ui/shell-quote.ts`、
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
/// 交给 agent 的参数 · 登记备注也不走这一条：它们可以跨行，走 [`ARG_TEXT_REFUSED`]。
pub const FREE_TEXT_REFUSED: [char; 3] = ['\0', '\r', '\n'];

/// 见 [`FREE_TEXT_REFUSED`]：这个自由文本值能不能交给唯一的 quote 拼进 shell。
pub fn free_text_ok(s: &str) -> bool {
    !s.contains(FREE_TEXT_REFUSED)
}

/// 〔`INVARIANTS §47` ② · V138「位置参数原样交给 claude」〕**交给 agent 的参数 · 登记备注**这一种自由文本可以跨行：
/// 多行初始任务是真实能力（`cc-spawn <目录> "$(cat 任务.md)"`；后端 `control/launch.rs::check_typed_payload` 为它放行 `\n`）。
/// LF 在单引号里 quote 挡得住 —— 键进交互 shell 是续行、值原样，`sh -c` 里本来就原样；拒它就是「拒过头」。
/// 只拒 NUL（截断参数）与 CR（键进终端就是回车键，值里变成 LF，原样不了）。
pub const ARG_TEXT_REFUSED: [char; 2] = ['\0', '\r'];

/// 见 [`ARG_TEXT_REFUSED`]：这个交给 agent 的参数 / 登记备注能不能交给唯一的 quote 拼进 shell。
pub fn arg_text_ok(s: &str) -> bool {
    !s.contains(ARG_TEXT_REFUSED)
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
// 每一条都是**全仓唯一的一份**（`设计/01 §5` D1；登记表 `tests/frontend/ui/judgment-single-home.vitest.ts`）：
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
/// 规则从这里现生成到 `src/frontend/ui/generated/judgment-rules.ts`（不手抄）。
pub const ACCOUNT_NAME_EXTRA: &str = "_-";

/// **账号名**（`--account <名>`）：1..=[`ACCOUNT_NAME_MAX`] 位 · 首字符 ASCII 字母数字 · 其余字母数字或 [`ACCOUNT_NAME_EXTRA`]。
///
/// 建账号库的后端（`accounts/manage/model.rs::name_ok`）直接用这一条（「`[A-Za-z0-9_-]`，不以 `-` 或 `_` 开头，≤32」）——
/// 盘上每个具名账号都过过它 ⇒ 真实账号名全过。
pub fn account_name_ok(s: &str) -> bool {
    let mut cs = s.chars();
    matches!(cs.next(), Some(c) if c.is_ascii_alphanumeric())
        && s.len() <= ACCOUNT_NAME_MAX
        && cs.all(|c| c.is_ascii_alphanumeric() || ACCOUNT_NAME_EXTRA.contains(c))
}

/// **cc-bus agent id**（发消息的收件人 · 收掉的那个 · 派生时的账号名 · 读收件箱时的文件名）：非空 · 不以 `-` 开头 ·
/// 只含 `[A-Za-z0-9_-]`。**没有上界**（今天就没有；照原样搬，不顺手加）。
///
/// 〔DUP2 · 主会话 09-26 裁 J12 · `INVARIANTS §47` ①〕从 monitor `backend/control/cc_bus.rs` 里的 `is_valid_bus_id` 搬来（规则逐字不变；那个名字今天是本函数的再导出），
/// 住这里是因为两半都要它：monitor 读收件箱 · 后端 `bus-send` / `bus-kill` / `bus-spawn` 在把 id 交给 `cc-send` / `cc-kill` /
/// `cc-spawn` **之前**先判（界面那一份删了，`设计/90 §3` 判据 2）。关键的一条是拒前导 `-`：`--help` 在盘上真出现过
/// （`~/.cc-bus/inbox/--help.jsonl`），拼进 `cc-send` 就被当成一个 flag。
pub fn bus_id_ok(s: &str) -> bool {
    !s.is_empty()
        && !s.starts_with('-')
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

/// **启动期令牌**（`CCM_RBIND_TOKEN`）的长度 —— 32 个字符。
///
/// 〔DUP3 · 主会话 09-26 裁 · `设计/01 §5` D1〕令牌形状全仓只有这一份（先前两半各一份、规则逐字同：
/// monitor `backend/control/payload.rs` 的 `rbind_token_shape_ok` · 后端 `control/identity_tag.rs` 的 `token_is_safe`，两个名字今天都是本组的再导出）。
/// 核过是**同一个令牌**：写侧 monitor 渲 `export CCM_RBIND_TOKEN=<令牌>`（`payload.rs`）、拉起时按它登记窗口（`launch.rs` 握手前奏）；
/// 读侧后端从 `/proc/<pid>/environ` 读**同一个变量**（`identity_tag.rs::rbind_token_of`）。两半都要 ⇒ 共享 crate；
/// 形状是闭集字母表 ＋ 定长 = `INVARIANTS §47` ① 标识符那一形 ⇒ 与 sid / 模型名 / 账号名 / bus id 同住。
/// 前端铸币口按 [`RBIND_TOKEN_ALPHABET`] × 本常量**造**令牌（两个值由 monitor 现生成进 `src/frontend/ui/generated/judgment-rules.ts`）。
pub const RBIND_TOKEN_LEN: usize = 32;

/// 启动期令牌的字母表 —— **小写**十六进制（大写 `A`–`F` 刻意不在里面，见 [`rbind_token_ok`]）。
pub const RBIND_TOKEN_ALPHABET: &str = "0123456789abcdef";

/// 令牌形状：恰好 [`RBIND_TOKEN_LEN`] 个 [`RBIND_TOKEN_ALPHABET`] 里的字符（32 个**小写**十六进制）。
///
/// **fail closed 到这个地步**（不 trim、不认大写、不认长度相近）：令牌的下游用途是**跨机器的 join 键** ——
/// 本地那张 `token → 窗口` 表与从 `environ` 读回来的串直接相等比较，中间不留归一化步骤（归一化是「两侧各写一遍、
/// 各写错一遍」的经典落点）；一个「差不多对」的串查不到，与查错一样糟、还更难归因（`设计/80 §8.5 ②`）。
///
/// ⚠ 这条**不是转义**：写侧渲染时照样过 [`posix_quote`]。「值的形状」与「拼进 shell 安不安全」是两道闸。
pub fn rbind_token_ok(token: &str) -> bool {
    token.len() == RBIND_TOKEN_LEN
        && token
            .bytes()
            .all(|b| RBIND_TOKEN_ALPHABET.as_bytes().contains(&b))
}

/// 〔P5 · `设计/80 §8.2`「marker = token」〕令牌握手里那个 marker 的前缀：`ccm-rbind-token-<32hex>`。
/// 写侧是后端渲的开终端前奏（`platform/shell/powershell.rs::rbind_bind_prelude`），读侧是 monitor `bind.rs` 那张表 ⇒ 契约住这里。
/// 与 Era 2 的 `ccm-bind-<PID>-<8hex>`、标题路的 `ccm-rbind-<sid>` 互不误命中：解码要求前缀后**恰好**一个合格令牌。
pub const RBIND_TOKEN_MARKER_PREFIX: &str = "ccm-rbind-token-";

/// 握手目录名（相对 monitor 数据目录）。已装的 `__ccm_bind`（`src/shared/cc.ps1.tpl`）也写它 ⇒ **不许改值**。
pub const AWAIT_SUBDIR: &str = "ps-await";

/// 带令牌的 marker。形状不对 ⇒ `None`（不产一个解不回来的 marker）。与 [`rbind_token_from_marker`] 互逆。
pub fn rbind_token_marker(token: &str) -> Option<String> {
    rbind_token_ok(token).then(|| format!("{RBIND_TOKEN_MARKER_PREFIX}{token}"))
}

/// 从 marker 里解出令牌；前缀对了而后面形状不对也当没有（fail closed：可疑的键会把「拉错窗口」伪装成「拉不到」）。
pub fn rbind_token_from_marker(marker: &str) -> Option<&str> {
    let rest = marker.strip_prefix(RBIND_TOKEN_MARKER_PREFIX)?;
    rbind_token_ok(rest).then_some(rest)
}

// ═══════════════════════════════════════════════════════════════════════════
// 〔DUP3 · 主会话 09-26 裁（乙；主会话代用户裁，用户可推翻）· `INVARIANTS §47` ③〕**命令片段类**：启动器。
//
// 启动器**不是一个词**：`ccr code`（带参数）· `claude --dangerously-skip-permissions` · `cct`（alias）· `/usr/local/bin/claude`（带路径）·
// `~/bin/claude`（家目录下）都是真实用法 —— 它要被 shell **拆成词、按 alias / PATH 解析**，所以**不 quote**（quote 起来
// `'ccr code'` 找不到命令、alias 不展开、PowerShell 要 `& '…'`）。不 quote 就只能**白名单**：同一个字符串原样拼进
// bash（远端载荷 · 本机 POSIX）与 PowerShell（本机 Windows），白名单里每一个字符在两种 shell 里都不是元字符
// （`;` `|` `&` `$` `(` `)` `<` `>` `{` `}` `@` `#` `,` 反引号 · 引号 · 换行一个都不在），空格就是要拆的词界。
// `~` 只许打头、紧跟 `/`：POSIX 展开成家目录，PowerShell 的文件系统路径也认它；别处的 `~` 两种 shell 语义不同，拒。
//
// 先前三处三条规则（本机 `history.rs` 白名单不许 `/` · 远端载荷 `payload.rs` 拒绝集 · 后端 ccm `free_text_gate` 只拒 NUL / CR / LF），
// 今天三处都调这一个（`设计/01 §6.8`：本机远端同一条）。空串不在这里判：各调用处「空 ⇒ 默认启动器」是 D3 缺省，不是判定。
// ═══════════════════════════════════════════════════════════════════════════

/// 启动器在 ASCII 字母数字之外还放行的字符（空格是词界）。
pub const LAUNCHER_EXTRA: &str = " -_./";

/// 启动器里 `~` 唯一合法的位置：打头、紧跟 `/`（家目录下的路径）。
pub const LAUNCHER_HOME_PREFIX: &str = "~/";

/// 启动器（命令片段）里**第一个不许有的字符**；`None` = 整串都在白名单里（ASCII 字母数字 ∪ [`LAUNCHER_EXTRA`]，
/// 外加打头的 [`LAUNCHER_HOME_PREFIX`]）。回那个字符，好让调用处**说清是哪一个**（拒就说清、不静默换）。
pub fn launcher_refused_char(s: &str) -> Option<char> {
    let body = s.strip_prefix(LAUNCHER_HOME_PREFIX).unwrap_or(s);
    body.chars()
        .find(|c| !(c.is_ascii_alphanumeric() || LAUNCHER_EXTRA.contains(*c)))
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

/// 〔FILES2 · `INVARIANTS §47` ②〕[`posix_free_path_ok`] 的**字节形**：路径不是合法 UTF-8（远端的乱码目录名）时用它判，
/// 规则逐条相同（绝对 · 没有 `..` 段 · 不含 NUL / CR / LF）。合法 UTF-8 的字节与字符串形答得一样。
pub fn posix_free_path_bytes_ok(p: &[u8]) -> bool {
    p.first() == Some(&b'/')
        && !p.split(|b| *b == b'/').any(|seg| seg == b"..")
        && !p.iter().any(|b| matches!(b, b'\0' | b'\r' | b'\n'))
}

/// 〔FILES2 · 唯一的 quote 的字节形〕POSIX 的 ANSI-C 引号 `$'…'`：可打印 ASCII 原样（`\` 与 `'` 前面加 `\`），
/// 其余每个字节写 `\xNN` —— 名字不是合法 UTF-8 时单引号那一形写不出来（Rust 的串装不下那几个字节）。
/// ⚠ `$'…'` 是 bash / zsh / ksh 的形，POSIX 2024 才收进标准；老 dash 不认（`cd` 失败、出声，不猜）。
pub fn posix_quote_bytes(b: &[u8]) -> String {
    let mut out = String::with_capacity(b.len() + 3);
    out.push_str("$'");
    for &c in b {
        match c {
            b'\\' => out.push_str("\\\\"),
            b'\'' => out.push_str("\\'"),
            0x20..=0x7e => out.push(c as char),
            _ => out.push_str(&format!("\\x{c:02x}")),
        }
    }
    out.push('\'');
    out
}

#[cfg(test)]
#[path = "../../../../tests/common/shell-quote-core/lib_tests.rs"]
mod tests;
