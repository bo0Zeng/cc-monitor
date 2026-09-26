//! T04 第二步：**围栏块配对判定**——本机 profile 与远端 profile 共用同一条规则。
//!
//! ## 为什么只抽这一条，而不是"统一部署器"
//!
//! T04 第二步原计划是"五套机制的装/升/卸真正走注册表"。**数下来那个抽象不该建**：
//!
//! | 范式 | 使用者 | 现状 |
//! |---|---|---|
//! | 指纹判过期 → 决定装/升/跳过 | backend + cc-acct-iso（2） | **已共享** `sftp::deploy_decision` |
//! | 备份 → 写 → 读回比对 → 回滚 | 5 处 | **已共享** `verified_write::verify_readback` |
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
//! - 远端（`profile_installer::merge_profile_block`，〔W5-ALIAS〕从前住 `sftp.rs`）：**Err 中止**。这是 F10 审计 B1 专门加的——
//!   原话「绝不用独立 `find` 误配前面的 END 而吞掉用户内容；宁可报错让用户手修，
//!   也不破坏文件」。
//! - 本机（`profile_installer::find_block_range`〔散文墓碑〕）：返回 `None` → 走**追加**分支。
//!
//! 本机那条的后果我实测过（`profile_installer` 里留着那条复现测试）：
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
//! 所以本模块取**两者中最强的那一档**（同 T01 对 `verified_write` 的做法：
//! 四处实现里本机侧只比长度，统一到内容级比对）。

use crate::copy_table::copy_text;

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
pub fn find_pair(
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
// `verified_write` 头注自己记着为什么远端那几处没收进去：「回滚是 `async` SFTP 操作，塞不进 `impl FnOnce()`」。
//
// ⇒ 按 `71 §12.3` 第 4、5 格切：**拼接是规则（留这里，一份）**；「排版」随目标文件的方言走（[`Layout`]）。
// 〔RW1 · 第四波 09-24〕**序列那一半搬去了后端**（`control/files_write.rs::put_text`）：用户文件（rc ·
//   `$PROFILE` · 别名文件 · 远端 rc）从此经那台机器的后端写，本机那一份原语 `LocalFile`〔散文墓碑〕随之走了。
//   [`apply`] ＋ [`Store`] 只剩 F08 部署物那一个用户（`sftp::SftpFile`，见 `put_ccm_entry`）；
//   方法名刻意不叫 `replace` / `remove` —— 那两个词与 `str::replace`、集合的 `remove` 同名，按名字认写者的判据会把它们认成同一个。

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

/// 落点给的四个原语 —— **规则一个字都不住在实现里**，它们只回答「这台机器上怎么做这件事」。
///
/// ⚠ 读必须 fail-closed：说不清「不存在」还是「读不出来」⇒ `Err`。把读不出来当成空文件，
/// 正是 `sftp::interpret_profile_read` 头注记的那两个数据丢失口（跳过备份 ＋ 整份覆盖）。
pub(crate) trait Store {
    /// 给人看的名字（报错里用）。
    fn label(&self) -> String;
    /// 原样读。`Ok(None)` = 确定不存在。
    async fn read(&self) -> Result<Option<String>, String>;
    /// 把原文另存一份，返回备份的名字（给人看）。
    async fn save_backup(&self, original: &str) -> Result<String, String>;
    /// 原子替换成 `content`；落点不存在就新建（含上级目录）。
    async fn put_atomic(&self, content: &str) -> Result<(), String>;
    /// 删掉 —— 只在「原本不存在、是我们刚建出来的」那一档回滚时用。
    async fn delete_created(&self) -> Result<(), String>;
}

/// 一次 [`apply`] 做了什么。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Applied {
    /// 算出来的内容与盘上逐字相同（或计划说「没事可做」）⇒ **一个字节都没写**。
    Unchanged,
    /// 写了。`backup` = 原文另存在哪（原文为空 / 不存在 / 调用方不要备份时为 `None`）；
    /// `created` = 这份文件是这一次新建的。
    Written {
        backup: Option<String>,
        created: bool,
    },
}

/// 回滚之后那半句话 —— **说的必须是真发生了的事**（从前远端 `existing` 为空时一个回滚都没做，
/// 文案却无条件说「已尝试回滚原文件」；本机那一侧没有备份时干脆什么都不说）。
fn undo_note(existed: bool, undone: bool, backup: Option<&str>) -> String {
    match (existed, undone, backup) {
        (true, true, _) => copy_text("rsFencedBlock.undo.restored", &[]),
        (false, true, _) => copy_text("rsFencedBlock.undo.createdRemoved", &[]),
        (true, false, Some(b)) => copy_text(
            "rsFencedBlock.undo.restoreFailedBackup",
            &[("backup", &b.to_string())],
        ),
        (true, false, None) => copy_text("rsFencedBlock.undo.restoreFailed", &[]),
        (false, false, _) => copy_text("rsFencedBlock.undo.createdLeft", &[]),
    }
}

/// 🔴 **那一个序列**：读 → 计划 → 相同就不写 → 备份 → 原子替换 → 回读逐字比对 → 不符就回滚。
///
/// `plan` 拿到原样读到的文本（`None` = 文件不存在），回 `Some(新内容)` 或 `None`（没事可做）。
/// 方言相关的一切（BOM、围栏、排版、「不存在算不算错」）都在 `plan` 里答，本函数不认识任何一种。
/// `keep_backup`：用户的文件（rc / profile）要一份给人看的备份；我们自己的文件
/// （整份由我们拥有、下一次生成会原样覆盖）不留 —— 回滚用的是内存里那份原文，不靠备份文件。
pub(crate) async fn apply<S: Store>(
    store: &S,
    keep_backup: bool,
    plan: impl FnOnce(Option<&str>) -> Result<Option<String>, String>,
) -> Result<Applied, String> {
    let original = store.read().await?;
    let Some(next) = plan(original.as_deref())? else {
        return Ok(Applied::Unchanged);
    };
    if original.as_deref() == Some(next.as_str()) {
        return Ok(Applied::Unchanged);
    }
    let label = store.label();
    let backup = match original.as_deref() {
        Some(o) if keep_backup && !o.is_empty() => {
            Some(store.save_backup(o).await.map_err(|e| {
                copy_text(
                    "rsFencedBlock.apply.backupFailed",
                    &[("label", &label.to_string()), ("e", &e.to_string())],
                )
            })?)
        }
        _ => None,
    };
    let existed = original.is_some();
    let undo = || async {
        match original.as_deref() {
            Some(o) => store.put_atomic(o).await.is_ok(),
            None => store.delete_created().await.is_ok(),
        }
    };
    if let Err(e) = store.put_atomic(&next).await {
        let undone = undo().await;
        return Err(copy_text(
            "rsFencedBlock.apply.writeFailed",
            &[
                ("label", &label.to_string()),
                ("e", &e.to_string()),
                (
                    "undo",
                    &(undo_note(existed, undone, backup.as_deref())).to_string(),
                ),
            ],
        ));
    }
    let verdict = match store.read().await {
        Ok(Some(back)) => crate::verified_write::verify_readback(&next, &back),
        Ok(None) => crate::verified_write::WriteVerdict::Mismatch {
            detail: copy_text("rsFencedBlock.verify.missing", &[]),
        },
        Err(e) => crate::verified_write::WriteVerdict::Mismatch {
            detail: copy_text("rsFencedBlock.verify.unreadable", &[("e", &e.to_string())]),
        },
    };
    if let crate::verified_write::WriteVerdict::Mismatch { detail } = verdict {
        let undone = undo().await;
        return Err(copy_text(
            "rsFencedBlock.verify.failed",
            &[
                ("detail", &detail.to_string()),
                (
                    "undo",
                    &(undo_note(existed, undone, backup.as_deref())).to_string(),
                ),
            ],
        ));
    }
    Ok(Applied::Written {
        backup,
        created: !existed,
    })
}

// 〔RW1 · 第四波 09-24〕这里原来住着本机那一份原语 `LocalFile`〔散文墓碑〕与它的同步门面
// `apply_local`〔散文墓碑〕：本机 rc / `$PROFILE` / 别名文件经它们在 monitor 进程里直写。
// 用户裁「只允许后端的文件管理部分写文件」**也管本机** ⇒ 那几处改走本机后端
// （`user_files::edit` → `files-peek` / `files-put`），序列（备份 · 原子替换 · 回读 · 回滚）住后端
// `control/files_write.rs::put_text`，两件零调用方 ⇒ 整块走。
// ⚠ [`apply`] 与 [`Store`] **还剩一个用户**：F08 部署那一族的 `sftp::put_ccm_entry`（`~/.local/bin/ccm`
// 那三行入口，用户裁「F08 部署后端留在 SFTP」）—— 它是我们的部署物，不是用户文件，本路不改它的行为。

// ═══════════════════════════════════════════════════════════════════════════
// `KR62D3`：**同一件事今天有几套形状 —— 一条有住址的账**
// ═══════════════════════════════════════════════════════════════════════════

/// 「把 cc-monitor 的一块东西装进一份 shell 配置」这件事的**一个形状**。
///
/// # 为什么要有这张表（而不是把它写进某段注释里）
///
/// `K-R62 §0c` 现打到的那条：同一件事按宿主分了三套形状，判定那一半 `fenced_block`
/// 已经收了（三套全走 [`find_pair`]），**装与卸那一半没收**。
/// 那段话本来只活在件文件的正文里 —— 而 `K-R60` / `K-R61` 两件已经连着证明：
/// **写在注释里而字段 / 判据看不见，等于没写**（一句真话摆错了格，和假话一样是假举证，`K29`）。
///
/// ⇒ `K-R62` **不收敛它们**（那是另一个量级，见件文件 `§0e`），只把它**登记成一条有住址的账**，
/// 然后由 PM 裁收不收、什么时候收。这张表买到三件下面各有判据看着的东西：
///   ① 每一行都指得出**代码住址**（`<文件>.rs::<符号>`），`structural_scan` 会去验它解析得到；
///   ② 围栏标记**指**各自那个常量，不在这里抄字面量（抄一份就是第二个住址）；
///   ③ 「判定已收 / 装卸未收」这句话是**数出来的**，不是形容出来的。
pub struct FenceShape {
    /// 稳定 id。
    pub id: &'static str,
    /// 哪台机器上的哪份文件。
    pub host: &'static str,
    /// 往里放什么。
    pub what_goes_in: &'static str,
    /// 这一套认哪一对围栏的 BEGIN。**指常量，不抄字面量。**
    pub begin_marker: &'static str,
    /// 装那一半住哪（`<文件>.rs::<符号>`）。
    pub install_site: &'static str,
    /// 卸那一半住哪；`None` = **今天没有卸口**（那本身就是一条账）。
    pub uninstall_site: Option<&'static str>,
    /// 配对判定走哪一份。三套今天是同一份 —— 这一格就是「已经收了哪一半」的读数。
    pub pairing: &'static str,
    /// 它与别的形状**差在哪**。
    pub differs_in: &'static str,
}

/// 🔴 **那三套形状 + `K-R62` 新加的那一条，唯一一份账。**
///
/// ⚠ 它**不判对错**，只记「今天是什么样」。要不要收敛由 PM 裁。
pub const FENCE_SHAPES: &[FenceShape] = &[
    FenceShape {
        id: "remote-posix-block",
        host: "远端 POSIX 的 ~/<用户选的那份 rc>",
        what_goes_in: "整块别名 snippet（src/shared/ccm-aliases.sh）",
        begin_marker: crate::profile_installer::CCM_PROFILE_BEGIN,
        install_site: "profile_installer.rs::install_remote_alias_block",
        uninstall_site: Some("profile_installer.rs::uninstall_remote_alias_block"),
        pairing: "fenced_block.rs::find_pair",
        differs_in: "落盘走 SFTP（upload_atomic + 远端备份），本机那两套走本地原子替换",
    },
    FenceShape {
        id: "local-windows-ps",
        host: "本机 Windows 的 PowerShell profile",
        what_goes_in: "整块 PowerShell 代码（scripts/cc.ps1.tpl 渲染）",
        begin_marker: crate::profile_installer::BEGIN_MARKER,
        install_site: "profile_installer.rs::install_to_profile",
        uninstall_site: Some("profile_installer.rs::uninstall_from_profile"),
        pairing: "fenced_block.rs::find_pair",
        differs_in: "内容是**现渲**的（命令名与要不要带 cc 函数由界面给），另两套写的是仓里那份文件本身；\
                     而且它要保住 CRLF（fenced_block.rs::detect_eol，`Layout::PowerShell` 那一臂）",
    },
    // 〔TL1 · 4C〕墓碑：这里从前还有一行 `local-posix-source-line`（本机 rc 里包着「一行 source」的那一对围栏，
    //   唯一**没有卸口**的一套）。那一步退役了（`设计/71 §6.1`「source 那一行只许一处装」：接上别名文件的那一行只住别名块里，
    //   选了 rc 只查不装）⇒ 这一套不再存在，账降一行；「装得进去卸不掉」那一格随之清零。盘上已有的那一块不读不删。
    // ★★ 〔`K-R62` 09-11〕**本件新加的那条路，就是这一行。**
    FenceShape {
        id: "local-posix-block",
        host: "本机 POSIX 的 ~/<用户选的那份 rc>",
        what_goes_in: "整块别名 snippet —— **与 `remote-posix-block` 同一个常量**（profile_installer.rs::CCM_WRAPPER_SNIPPET）",
        // 与远端那一套**同一个常量**：本机与远端装进 rc 的是同一个东西（`K15` / `K36`）。
        begin_marker: crate::profile_installer::CCM_PROFILE_BEGIN,
        // 🔴 **落盘那一跳与 `local-windows-ps` 是同一处** —— 这一行的 `install_site`
        //    与它逐字相同，不是笔误：补这一格没有多出第四台安装器，多出来的只是
        //    那一台安装器的第二种方言（分岔在 profile_installer.rs::plan_install）。
        install_site: "profile_installer.rs::install_to_profile",
        uninstall_site: Some("profile_installer.rs::uninstall_from_profile"),
        pairing: "fenced_block.rs::find_pair",
        differs_in: "与 `remote-posix-block` **只差落盘那一跳**（本地原子替换 vs SFTP）：\
                     内容、围栏、合块与剥块的实现全共用；与 `local-windows-ps` 只差**方言**\
                     （分岔在 profile_installer.rs::plan_install / \
                     profile_installer.rs::plan_uninstall，落盘与备份回滚那一整套共用）。\
                     ⇒ 补这一格没有把三套变成四套。判据 \
                     profile_installer_tests.rs::the_local_posix_port_is_byte_for_byte_the_remote_one",
    },
];

#[cfg(test)]
#[path = "../../../tests/bridge/fenced_block_tests.rs"]
mod tests;
