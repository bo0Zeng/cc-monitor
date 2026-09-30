//! T04 第二步：**围栏块配对判定**——本机 profile 与远端 profile 共用同一条规则。
//!
//! 〔MIG-3a · `设计/99 §2.1 ⑬` · 主会话 09-27 裁〕从 monitor `fenced_block.rs` 搬来：别名块装 / 卸今天在那台后端里算、经它自己的
//! 文件管理面写（[`super`]），配对与拼接这份纯规划跟着住这里；一个字节的规则没变。
//!
//! ## 为什么只抽这一条，而不是"统一部署器"
//!
//! T04 第二步原计划是"五套机制的装/升/卸真正走注册表"。**数下来那个抽象不该建**：
//!
//! | 范式 | 使用者 | 现状 |
//! |---|---|---|
//! | 指纹判过期 → 决定装/升/跳过 | backend（后端二进制自部署） | backend 那条住 `control/deploy_plan.rs` 的 `identity_decision` |
//! | 备份 → 写 → 读回比对 → 回滚 | 5 处 | **已共享**（〔W5-ALIAS〕今天只住后端 `files_write.rs::put_text`） |
//! | 围栏块插入/替换/剥离 | ccm 远端 profile + PowerShell 本机 profile（2） | **两套独立实现** ← 本模块 |
//! | 整份 JSON 覆写 | 项目 MCP（1） | 单例，不抽 |
//!
//! 三个范式里两个早就共享了，第四个只有一个使用者。**再套一层 `install(tool_id)` 分派器
//! 只会把五件形状不同的事装进一个盒子**——正是本工作区反复拒绝的形状。
//! 真正剩下的重复只有围栏块这一族，而它藏着一个**真的数据丢失 bug**（见下）。
//!
//! ## 这里修的是一个会吃掉用户内容的 bug
//!
//! 两侧对「有 BEGIN 但找不到配对的 END」（上次安装中断 / 用户手改坏）处置**不一致**：
//!
//! - 远端（`block::merge_profile_block`，〔W5-ALIAS〕从前住 `sftp.rs`，〔MIG-3a〕从 monitor 搬进后端）：**Err 中止**。这是 F10 审计 B1 专门加的——
//!   原话「绝不用独立 `find` 误配前面的 END 而吞掉用户内容；宁可报错让用户手修，
//!   也不破坏文件」。
//! - 本机（`profile_installer::find_block_range`〔散文墓碑〕）：返回 `None` → 走**追加**分支。
//!
//! 本机那条的后果我实测过（`block_tests.rs` 里留着那条复现测试）：
//!
//! ```text
//! 原始：  # my stuff / # === cc-monitor BEGIN v1 === / function cc { }        ← 损坏 + 用户代码
//! 装一次：…BEGIN… / function cc { } / …BEGIN… / NEW / …END…                  ← 追加，用户代码还在
//! 装两次：# my stuff / …BEGIN… / NEW / …END…                                 ← **function cc { } 没了**
//! ```
//!
//! 第二次安装时，**损坏的那个 BEGIN 与新块的 END 配上了对**，于是两者之间的东西
//! ——包含用户自己的代码——被整段替换掉。写的是用户的 PowerShell `$PROFILE`，
//! 和远端 `.bashrc` 同性质：写坏了下次开终端就炸。
//!
//! 所以本模块取**两者中最强的那一档**（同 T01 对写后回读的做法：
//! 四处实现里本机侧只比长度，统一到内容级比对）。

use copy_core::copy_text;

/// 找配对的围栏块，返回**行下标**区间（含两端）。
///
/// - `Ok(None)`：没有 BEGIN → 调用方追加。
/// - `Ok(Some((b, e)))`：找到配对 → 调用方整块替换。
/// - `Err(_)`：**有 BEGIN 但其后没有 END** → 调用方必须中止，绝不猜。
///
/// 匹配用 `trim_start().starts_with(..)`：两侧的标记都允许行内缩进，
/// 且本机侧的 BEGIN 带版本后缀（`# === cc-monitor BEGIN v1 ===`）所以只能前缀匹配。
///
/// **只找 BEGIN 之后的 END**——独立 `find` 会误配 BEGIN 前面的 END。
pub(crate) fn find_pair(
    text: &str,
    begin_marker: &str,
    end_marker: &str,
    what: &str,
) -> Result<Option<(usize, usize)>, String> {
    let mut begin: Option<usize> = None;
    for (idx, line) in text.lines().enumerate() {
        let l = line.trim_start();
        if begin.is_none() && l.starts_with(begin_marker) {
            begin = Some(idx);
            continue;
        }
        if begin.is_some() && l.starts_with(end_marker) {
            return Ok(Some((begin.unwrap(), idx)));
        }
    }
    match begin {
        None => Ok(None),
        Some(b) => Err(copy_text(
            "rsFencedBlock.pair.noEnd",
            &[("what", &what.to_string()), ("line", &(b + 1).to_string())],
        )),
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// 〔AL1 · 2026-09-24〕`设计/71 §12.5`：**规则只有一份** —— 拼接一处，落盘序列一处
// ═══════════════════════════════════════════════════════════════════════════
//
// 立件时现打（`tests/evidence/MC1-AL1-摸底.md` 第四节）：配对判定早就只有 [`find_pair`] 一份，
// 但「配对之后怎么拼」写了三份（`sftp::merge/strip_profile_block` · `profile_installer` 的
// `replace_or_append_block/strip_block` · `account_aliases::ensure_rc_source_line` 里内联的那一段 —— 〔TL1〕那一跳后来整个退役了），〔散文墓碑〕
// 「备份 → 原子写 → 回读比对 → 回滚」这个序列写了五个函数体（本机三处 ＋ 远端装/卸）。
// 当年的回滚写入器头注自己记着为什么远端那几处没收进去：「回滚是 `async` SFTP 操作，塞不进 `impl FnOnce()`」。
//
// ⇒ 按 `71 §12.3` 第 4、5 格切：**拼接是规则（留这里，一份）**；「排版」随目标文件的方言走（[`Layout`]）。
// 〔RW1 · 第四波 09-24〕**序列那一半搬去了后端**（`control/files_write.rs::put_text`）：用户文件（rc ·
//   `$PROFILE` · 别名文件 · 远端 rc）从此经那台机器的后端写，本机那一份原语 `LocalFile`〔散文墓碑〕随之走了。
//   〔W5-ALIAS〕留在这里的那一份序列后来也删了（见下面 `KR62D3` 那一节之前的墓碑）。

/// 排版方言。**规则不分方言**（配对 → 整块替换 / 追加 / 悬空中止；剥离 → 删 / 原样 / 悬空中止），
/// 分方言的只有排版这一层 —— 由目标文件决定（`profile_installer::flavor_of` 按扩展名答），
/// 不由宿主平台决定。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Layout {
    /// POSIX rc：块原样放（调用方给的块以 `\n` 结尾）；追加时只补一个缺的换行；剥离后前后原样拼。
    Posix,
    /// PowerShell profile：**保住原文件的行尾**（默认 CRLF —— notepad / VSCode / autocrlf 三大来源）；
    /// 追加前空一行；剥离后尾部至多留一个行尾。
    PowerShell,
}

/// 原文件用的行尾：含任何 `\r\n` 就算 CRLF。
fn detect_eol(s: &str) -> &'static str {
    if s.contains("\r\n") {
        "\r\n"
    } else {
        "\n"
    }
}

/// 把文本的行尾统一成 `eol`（先全归 LF，再按需换成 CRLF）。
fn rewrite_eol(content: &str, eol: &str) -> String {
    let lf = content.replace("\r\n", "\n");
    if eol == "\n" {
        lf
    } else {
        lf.replace('\n', "\r\n")
    }
}

/// 把 `[b, e]`（行下标，含两端）切出来，返回前后两段（各自带着原来的行尾）。
fn cut(existing: &str, b: usize, e: usize) -> (String, String) {
    let lines: Vec<&str> = existing.split_inclusive('\n').collect();
    let before = lines[..b].concat();
    let after = if e + 1 < lines.len() {
        lines[(e + 1)..].concat()
    } else {
        String::new()
    };
    (before, after)
}

/// **装**：`block`（含两个标记行）放进 `existing` —— 有配对块就整块替换，没有就追加，
/// 有 BEGIN 没 END 就 `Err` 中止（[`find_pair`]）。幂等：`splice_in(splice_in(x)) == splice_in(x)`。
pub(crate) fn splice_in(
    existing: &str,
    begin: &str,
    end: &str,
    block: &str,
    what: &str,
    layout: Layout,
) -> Result<String, String> {
    let pair = find_pair(existing, begin, end, what)?;
    Ok(match layout {
        Layout::Posix => match pair {
            Some((b, e)) => {
                let (before, after) = cut(existing, b, e);
                format!("{before}{block}{after}")
            }
            None => {
                let mut out = existing.to_string();
                if !out.is_empty() && !out.ends_with('\n') {
                    out.push('\n');
                }
                out.push_str(block);
                out
            }
        },
        Layout::PowerShell => {
            let eol = detect_eol(existing);
            let block = rewrite_eol(block.trim_end_matches(['\r', '\n']), eol);
            let mut out = String::new();
            match pair {
                Some((b, e)) => {
                    let (before, after) = cut(existing, b, e);
                    if !before.is_empty() {
                        out.push_str(&before);
                        if !before.ends_with('\n') {
                            out.push_str(eol);
                        }
                    }
                    out.push_str(&block);
                    out.push_str(eol);
                    if !after.is_empty() {
                        out.push_str(&after);
                        if !after.ends_with('\n') {
                            out.push_str(eol);
                        }
                    }
                }
                None => {
                    out.push_str(existing);
                    if !out.is_empty() {
                        if !out.ends_with('\n') {
                            out.push_str(eol);
                        }
                        out.push_str(eol);
                    }
                    out.push_str(&block);
                    out.push_str(eol);
                }
            }
            out
        }
    })
}

/// **卸**：删掉配对块，块外一个字节都不动；没有块 ⇒ 原样返回；悬空 BEGIN ⇒ `Err`
/// （「原样返回」看着无害，实则让人以为卸干净了，而那个 BEGIN 下次安装会吃掉它下面的内容）。
pub(crate) fn splice_out(
    existing: &str,
    begin: &str,
    end: &str,
    what: &str,
    layout: Layout,
) -> Result<String, String> {
    let Some((b, e)) = find_pair(existing, begin, end, what)? else {
        return Ok(existing.to_string());
    };
    let (before, after) = cut(existing, b, e);
    Ok(match layout {
        Layout::Posix => format!("{before}{after}"),
        Layout::PowerShell => {
            let eol = detect_eol(existing);
            let mut out = String::new();
            for part in [&before, &after] {
                if !part.is_empty() {
                    out.push_str(part);
                    if !part.ends_with('\n') {
                        out.push_str(eol);
                    }
                }
            }
            let double = format!("{eol}{eol}");
            while out.ends_with(&double) {
                out.truncate(out.len() - eol.len());
            }
            out
        }
    })
}

// 〔W5-ALIAS · 第五波先行〕这里原来是落盘那一个序列：`apply`〔散文墓碑〕（读 → 计划 → 相同不写 → 备份 → 原子替换 →
//   回读比对 → 回滚）＋ 它的四个原语 `Store`〔散文墓碑〕＋ 结局 `Applied`〔散文墓碑〕＋ 回滚措辞 `undo_note`〔散文墓碑〕。
//   RW1 之后用户文件（rc / `$PROFILE` / 别名文件 / 别名块）一律经那台机器的后端写（〔MIG-3a〕今天是 `door::edit` → 本进程 `files-put`，
//   序列住后端 `control/files_write.rs::put_text`），它只剩一个用户 —— 远端 `ccm` 入口那三行（部署物，不是用户文件）；
//   那一处改走部署那一族的 `sftp::upload_verified` 之后零调用方 ⇒ 删。本模块只剩**纯规划**（配对 · 拼接 · 账）。

// 〔MIG-3a〕`KR62D3` 那张「同一件事今天有几套形状」的账（`FenceShape` / `FENCE_SHAPES`）只有判据读它 ⇒ 随搬家住进判据文件
//   （`fence_tests.rs`；从前在 monitor 生产段里是一条死代码，占 `deadcode` 棘轮一格）。

#[cfg(test)]
#[path = "../../../../tests/backend/assets/aliases/fence_tests.rs"]
mod tests;
