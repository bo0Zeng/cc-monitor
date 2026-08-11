//! **原子替换的两套 Win32 语义，谁用哪一套**〔audit-0805 F13 下半，报告 I-15〕。
//!
//! # 报告说「4 份 / 两种语义」，核实之后那不是漂移
//!
//! `doc/INVARIANTS.md §4` 逐字写着「**profile 等用户文件**写入 = ReplaceFileW + backup + 写后校验」，
//! 而两处 `MoveFileExW` 写的都是 **monitor 自己的文件**（`config.json` / 日志轮转）。
//! `mcp.rs:320-322` 还专门写着「**不**用 config 的 `MoveFileExW`（§4 明令）」。
//! ⇒ **这是「两类文件两种语义」的刻意分工，不是无人察觉的漂移。**
//!
//! # 那真缺口在哪
//!
//! **没有任何东西让下一处新写点选对。** 实测（08-05）：
//! - 全仓**没有一条判据扫这两个符号**（`profile_installer.rs` 那三处 `cfg(test)` 提及是
//!   真机 `icacls` ACL 断言，不是扫描面）；
//! - `fenced_block.rs:5-12` 那张「哪些范式已经共享」的清单里，**原子替换这一族根本没列**。
//!
//! # ⚠ 08-06 订正：这张表此前只覆盖了 §4 规则的三分之二
//!
//! `doc/INVARIANTS.md §4` 那句话的原文是「不能用 **`std::fs::rename`** / `MoveFileExW`
//! 直接覆盖用户文件」，而本登记表原来只扫两个 **Win32** 符号 ——
//! **规则里第一个被点名的写法根本不在人群里**。
//! 于是「把 §4 从散文变成机检」（定框 E12）这件事，只做到了它自己声称的一部分。
//!
//! 实测补上之后多出 **11 处 / 6 个文件**（含两份 `atomic_replace_path` 副本的 POSIX 分支、
//! config/logging 的 `cfg(not(windows))` 对侧、SFTP 的远端 rename、下载落盘）。
//! ⇒ 这些**没有一处是错的**，但此前**没有一处被看住** ——
//! 而「本机随手 rename 一个用户文件」与它们在文本上一模一样。
//!
//! ⇒ 修法从「收成一份」改成「**登记表 + 扫描守卫**」：
//! 新增一处没登记的调用点就红，而**红的时候把选择规则原样打给写代码的人看**。
//! 这同时把 `INVARIANTS §4` 从散文变成机检（定框 **E12**：判准是「有没有一条会红的判据读它」）。
//!
//! ⚠ **刻意不建「统一原子写入器」**：`fenced_block.rs` 头注对同类冲动已经论证过一次 ——
//! 两类文件的正确行为**本来就不同**，套一层 `atomic_write(kind)` 分派器只是把
//! 「选哪套语义」这个决定藏进参数里，选错照样没人拦。**登记表让这个决定留在明处。**

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::{Path, PathBuf};

    /// 选择规则 —— 判据红的时候原样打给写代码的人。
    const RULE: &str = "\
选哪一套（`doc/INVARIANTS.md §4`）：\n\
  · 写**用户的**文件（PowerShell profile / MCP 配置 / auto-launch.json …）\n\
    ⇒ `ReplaceFileW(dst, tmp, NULL, REPLACEFILE_WRITE_THROUGH, …)`\n\
      理由：它**保留 dst 原有的 ACL/ADS/创建时间**。`MoveFileExW` 会把 tmp 的\n\
      继承 ACL 覆盖上去 ⇒ 用户 explicit ACE 丢失 ⇒ Documents 重定向到非默认盘的\n\
      用户读不了自己的 profile（v1.7.10 真踩过）。\n\
  · 写 **monitor 自己的**文件（config.json / 日志轮转）\n\
    ⇒ `MoveFileExW(tmp, dst, MOVEFILE_REPLACE_EXISTING)` 就够 —— 文件是我们自己的，\n\
      tmp 的 ACL 覆盖上去没有受害者。§4 的要求**只限定在用户文件**。\n\
拿不准就当成用户文件（两种错判的代价不对称：多保留一次 ACL 无害，丢一次 ACL 用户读不了文件）。";

    /// 每个生产调用点登记：`(相对 src-tauri/src 的路径, 符号, 处数, 写的是哪类文件, 为什么是这一套)`。
    ///
    /// ⚠ **登记表不是豁免清单**：新增一处没登记的 ⇒ 下面那条红，并把 `RULE` 原样打出来。
    const SITES: &[(&str, &str, usize, &str, &str)] = &[
        (
            "backend/control/local_backend.rs",
            "rename",
            1,
            "monitor 自己的缓存（自释放出来的 daemon 二进制）",
            "P2z 的自释放：先写 `.partial` 再 rename，防的是**半截文件被当成可执行的 daemon 起起来**。\
             §4 把 `ReplaceFileW` 的要求限定在**用户文件**（要保 ACL/ADS），这里写的是 monitor 自己\
             刚建的新文件、dst 通常压根不存在 ⇒ 没有要保留的 ACL，`rename` 的语义正合适。",
        ),
        (
            "config.rs",
            "MoveFileExW",
            1,
            "monitor 自己的 config.json",
            "写的是我们自己的配置文件，tmp 的 ACL 覆盖到 dst 没有受害者。\
             §4 把 ReplaceFileW 的要求**限定在用户文件**，这里不在其中。",
        ),
        (
            "logging.rs",
            "MoveFileExW",
            1,
            "monitor 自己的日志轮转",
            "同 config.rs（该处头注自陈「是从 config.rs 复制的」）。\
             ⚠ 复制而来这件事本身没问题 —— 有问题的是此前**没有任何东西记着它为什么可以照抄**。",
        ),
        (
            "utils.rs",
            "ReplaceFileW",
            1,
            "用户文件（`atomic_write_json` 的所有调用方，如 auto-launch.json）",
            "必须保留 dst 原有 ACL。dst 不存在时 fallback 到 rename（首次写，新文件本来就继承父目录 ACL）。",
        ),
        (
            "profile_installer.rs",
            "ReplaceFileW",
            1,
            "用户文件（PowerShell profile；`mcp.rs` 也复用它写 MCP 配置）",
            "同上，且**ACL 真的被保留**这一条由同文件 `cfg(test)` 里那条真机 `icacls` 断言钉着 —— \
             那是本族唯一一条能证明「语义选对了」的判据，别删。",
        ),
        // ── 〔audit-0805 08-06〕补上 `rename` 那一半（§4 规则原话里第一个被禁的写法）
        (
            "profile_installer.rs",
            "rename",
            2,
            "**用户文件**（PowerShell profile）—— 但这两处**就是正确路径本身**",
            "581：Windows 上 `dst` 不存在时 `ReplaceFileW` 会失败，首次安装直接 rename\
             （新文件 ACL 继承父目录，没有 explicit ACE 可保留）；599：`cfg(not(windows))` 分支，\
             POSIX 没有 §4 要防的那个 ACL 丢失问题。⇒ 登记它们不是因为它们错，\
             而是因为**它们与「别处随手 rename 一个用户文件」在文本上一模一样** —— \
             没有登记表就分不出哪一处是深思熟虑、哪一处是图省事。",
        ),
        (
            "utils.rs",
            "rename",
            2,
            "**用户文件**（`atomic_replace_path` 的第二份副本）",
            "与 profile_installer 那两处逐行同形（Windows 首装分支 + POSIX 分支）。\
             ⚠ 副本是**刻意**的（模块头注论证过不建统一写入器：两类文件的正确行为本来就不同），\
             但刻意复制的代价就是**两处都得被看住** —— 这正是登记表存在的理由。",
        ),
        (
            "config.rs",
            "rename",
            1,
            "**monitor 自己的** config.json（POSIX 分支）",
            "同文件那条 `MoveFileExW` 的 `cfg(not(windows))` 对侧。写的是我们自己的配置，\
             POSIX 上 rename 即原子替换，无 ACL 顾虑。",
        ),
        (
            "logging.rs",
            "rename",
            1,
            "**monitor 自己的** 日志轮转（POSIX 分支）",
            "同文件那条 `MoveFileExW` 的 `cfg(not(windows))` 对侧，理由同 config.rs。",
        ),
        (
            "sftp.rs",
            "rename",
            2,
            "**远端主机上的用户文件**（不是本机，§4 的 ACL 论证不适用）",
            "上传落地的两步：先把旧文件 rename 成 `.bak`（备份），再把 `.tmp` rename 成正名。\
             §4 讲的是 Windows 本机 ACL/ADS 保留，而这两处走的是 SFTP 协议、写的是远端文件系统\
             ⇒ **不属本机原子替换那条分工**。登记它是为了让下一个人不必再判一次。",
        ),
        (
            "sftp_pool.rs",
            "rename",
            3,
            "**monitor 自己的**下载落盘 + **远端主机上的用户文件**",
            "415：下载先写 `<local>.part` 再 rename 落地（半成品不冒充成品，失败即删）；\
             529/641：远端 SFTP rename，同 sftp.rs 那两处。\
             ⚠ 本行是**混类**的，刻意不拆：拆了会让「按文件求和」的对拍口径与实测那侧不一致，\
             而这条登记的用途是「让人看懂为什么这里可以 rename」，不是精确到行。",
        ),
    ];

    fn src_root() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("src")
    }

    /// 剥掉整行注释 —— 本文件与 `mcp.rs` / `profile_installer.rs` 的**头注里就写着这两个符号**。
    fn collect_rs(dir: &Path, out: &mut Vec<PathBuf>) {
        let Ok(rd) = fs::read_dir(dir) else { return };
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() {
                collect_rs(&p, out);
            } else if p.extension().is_some_and(|x| x == "rs") {
                out.push(p);
            }
        }
    }

    /// 本文件自己要排除 —— `RULE` 里写着两个符号的**调用示例**，那是字符串不是注释，
    /// 剥注释剥不掉。⚠ 第一次跑就是被这个咬红的（判据匹配到自己的文本，与 F12 那次同族）。
    /// 改名了也不会静默失效：新名字会以「未登记」的身份出现在下面那条里。
    /// 被扫的原子替换原语：`(显示名, 匹配串)`。
    ///
    /// 〔audit-0805 08-06〕**补上 `rename`** —— `doc/INVARIANTS.md §4` 那条规则的原话是
    /// 「不能用 `std::fs::rename` / `MoveFileExW` 直接覆盖用户文件」，
    /// 而本登记表此前只扫两个 **Win32** 符号 ⇒ **规则里第一个被点名的写法根本不在人群里**。
    /// 于是「把 §4 从散文变成机检」（定框 E12）这件事只做到了三分之二。
    ///
    /// ⚠ 匹配串刻意是**调用形态**而不是裸 `rename(`：后者会命中函数**定义**
    /// `pub async fn sftp_rename(` —— 摸底时实测多数出一处，正是本区反复记的 F24
    /// （匹配单位比事实小/大）的镜像半。
    const SCANNED: &[(&str, &str)] = &[
        ("MoveFileExW", "MoveFileExW("),
        ("ReplaceFileW", "ReplaceFileW("),
        ("rename", "::rename("),
        ("rename", ".rename("),
    ];

    const SELF: &str = "atomic_replace_registry.rs";

    /// 生产段（剥注释后）里**调用形态**的命中：`Symbol(`。`use` 那行没有括号，不算。
    fn call_sites() -> Vec<(String, String, usize)> {
        let root = src_root();
        let mut files = Vec::new();
        collect_rs(&root, &mut files);
        files.sort();
        let mut out = Vec::new();
        for f in files {
            let rel = f
                .strip_prefix(&root)
                .unwrap_or(&f)
                .to_string_lossy()
                .replace('\\', "/");
            if rel == SELF {
                continue;
            }
            let src = guard_core::strip_comment_lines(&fs::read_to_string(&f).unwrap_or_default());
            for (name, pat) in SCANNED {
                let n = src.matches(pat).count();
                if n > 0 {
                    match out.iter_mut().find(|(f, s, _)| f == &rel && s == name) {
                        Some((_, _, acc)) => *acc += n,
                        None => out.push((rel.clone(), (*name).to_string(), n)),
                    }
                }
            }
        }
        out.sort();
        out
    }

    /// ★ 正题：**每一处原子替换调用点都得登记它为什么选这一套语义**。
    #[test]
    fn every_atomic_replace_call_site_says_which_semantics_and_why() {
        let found = call_sites();
        let total: usize = found.iter().map(|(_, _, n)| *n).sum();
        // 抽取器自检：扫不到东西时下面的对拍会两边都空、静默变绿。
        assert!(
            total >= 4,
            "全仓只扫到 {total} 个原子替换调用点（08-05 实测 4）—— 抽取器坏了，\
             下面的对拍会在两边都空的情况下变绿"
        );

        // 排除项自检：`SELF` 排掉的必须**真的**是一个会命中的文件，否则这条排除是死规则，
        // 而死规则会在下一次有人真往这里写调用点时悄悄放行。
        let own = fs::read_to_string(src_root().join(SELF)).unwrap_or_default();
        assert!(
            own.contains("MoveFileExW(") && own.contains("ReplaceFileW("),
            "`{SELF}` 里已经没有那两个符号的示例了 —— 那条排除成了死规则，删掉它"
        );

        let mut missing = Vec::new();
        let mut drifted = Vec::new();
        for (f, sym, n) in &found {
            match SITES.iter().find(|(g, s, _, _, _)| g == f && s == sym) {
                None => missing.push(format!("  {f}  [{sym}]  {n} 处")),
                Some((_, _, want, _, _)) if want != n => {
                    drifted.push(format!("  {f}  [{sym}]  表里 {want} 处，实测 {n} 处"))
                }
                Some(_) => {}
            }
        }
        assert!(
            missing.is_empty(),
            "有原子替换调用点没登记：\n{}\n\n{RULE}",
            missing.join("\n")
        );
        assert!(
            drifted.is_empty(),
            "原子替换调用点的处数变了 —— 那正是该重新判定语义的时刻：\n{}\n\n{RULE}",
            drifted.join("\n")
        );
        // 反向：登记的还得真在（搬走/换实现了就该删条目，别留僵尸账）。
        for (f, sym, _, _, _) in SITES {
            assert!(
                found.iter().any(|(g, s, _)| g == f && s == sym),
                "登记表里的 `{f} [{sym}]` 已经不在了 —— 删掉这条"
            );
        }
    }

    /// 每条登记都得说清**写的是哪类文件**（那才是选语义的依据），不许只写「原子替换」。
    #[test]
    fn each_entry_names_the_file_class_it_writes() {
        for (f, sym, _, class, why) in SITES {
            assert!(
                class.contains("用户") || class.contains("monitor 自己"),
                "{f} [{sym}] 的文件类别写的是「{class}」—— 必须明说是**用户文件**还是\
                 **monitor 自己的文件**，那是 §4 里唯一的分界"
            );
            assert!(
                why.chars().count() > 20,
                "{f} [{sym}] 的理由太短，像是占位：「{why}」"
            );
        }
        // 两套都必须真的有人用 —— 只剩一套时这张表就退化成一句废话，该回来重读 §4。
        let mv = SITES
            .iter()
            .filter(|(_, s, ..)| *s == "MoveFileExW")
            .count();
        let rp = SITES
            .iter()
            .filter(|(_, s, ..)| *s == "ReplaceFileW")
            .count();
        assert!(
            mv > 0 && rp > 0,
            "登记表里只剩一套语义（MoveFileExW {mv} 条 / ReplaceFileW {rp} 条）—— \
             「两类文件两种语义」这条分工是否还成立？回去重读 `doc/INVARIANTS.md §4`"
        );
    }

    /// ★ 把 `INVARIANTS §4` 从散文变成机检（定框 **E12**）：
    /// 它逐字要求的那三件（备份 / `ReplaceFileW` / 回读校验）必须在文档里仍然写着，
    /// 且 `MoveFileExW` 仍被明令排除 —— 否则本模块整套论证的前提就没了。
    #[test]
    fn the_doc_rule_this_registry_rests_on_is_still_there() {
        let doc = fs::read_to_string(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .parent()
                .expect("仓根")
                .join("doc/INVARIANTS.md"),
        )
        .expect("doc/INVARIANTS.md 读不到 —— 路径变了就把这条一起改");
        let sec = doc
            .split("## 4. ")
            .nth(1)
            .expect("`doc/INVARIANTS.md` 里找不到 §4 —— 本注册表的全部依据都在那一节");
        let sec = sec.split("\n---").next().unwrap_or(sec);
        for want in ["ReplaceFileW", "MoveFileExW", "backup", "ACL"] {
            assert!(
                sec.contains(want),
                "`INVARIANTS §4` 里已经没有 `{want}` 了 —— 本注册表按它的分界登记语义，\
                 §4 一改就该回来重判。§4 现文：\n{sec}"
            );
        }
        assert!(
            sec.contains("用户文件"),
            "`INVARIANTS §4` 不再把要求限定在**用户文件** —— 那正是本表「两类文件两种语义」\
             的唯一依据，改了就该回来重判整张表。§4 现文：\n{sec}"
        );
    }
}
