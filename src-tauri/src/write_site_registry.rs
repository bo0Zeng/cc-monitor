//! ★ **每一个会写用户机器的落点都要申报**〔audit-0805 08-07，Phase G 第 38 件〕。
//!
//! # 它关掉的是 `ROADMAP §5 4b` 那条诚实边界的一半
//!
//! 4b 逐字写着：`tool_registry` 的 15 条判据全是**声明表内部的自洽**，
//! **人群是「声明了的工具」，不是「真实发生的安装动作」** ——
//! 「今天表里 6 条看着全，那是**人现在记得**，不是有东西钉着」。
//!
//! 它当初还写明了为什么不做机检：试过一条口径——**扫生产段的安装落点字面量**，
//! 实测 21 个命中里绝大多数是用户可见文案与探测命令，真落点只有两三个
//! ⇒ 噪声压过信号。那个判断当时是对的。
//!
//! ## 但那是**错的人群**
//!
//! 「路径字面量」是按**怎么写的**取样（一个字符串长得像不像路径），
//! 而要钉的事实是**做了什么**（这段代码有没有往用户机器上写东西）。
//! 换成后者之后人群当场干净：08-07 实测**生产段写盘调用 36 处、分布在 9 个文件**，
//! 每一处都机器可判、零文案噪声。
//!
//! ⇒ 这正是本工作区反复收敛出的那条：**判据锚在「怎么写」上就会又漏又吵；
//! 换成「做了什么」之后，原本被判为「做不了」的机检往往当场可行。**
//! ⚠ 教训的另一半：**「刻意不做」也会过期**。4b 的解锁条件写的是
//! 「安装动作先收敛到一个可枚举的落点」——今天回头量，它**早就成立了**，
//! 只是没人回来重量一次。
//!
//! # 它守什么、不守什么
//!
//! **守**：新增一个写盘落点（新文件、新函数）而不申报 ⇒ 当场红。
//! 申报为「安装动作」的必须点名 `tool_registry::TOOLS` 里**真实存在**的 `id`，
//! 于是「真实动作 ↔ 声明表」这条连线第一次有东西钉着。
//!
//! **不守**：① 一个已申报函数**内部**多写一个文件（人群键是「文件::函数」，
//! 不是逐次调用）——那要判语义，且同一个函数里多一次 `create_dir_all` 是常态；
//! ② 写的**内容**对不对（那是各模块自己的行为判据）；
//! ③ 通过 `Command` 起外部进程间接写盘（本条只看 Rust 侧的 `fs::` 调用面）。
//! ⚠ ③ 是真缺口，不是措辞：`ccm` 的部署有一部分走 shell。已进 `ROADMAP §5`。

/// ★ **「谁在写」的唯一权威源**〔audit-0805 08-07〕，供只读模块的守卫复用。
///
/// 只读模块（`config_surface` / `hooks_diag`）的守卫扫的是 `fs::` 前缀 ——
/// 那只认「自己写」，认不出**把写交给别人**：08-07 实测，两个模块各加一句
/// `crate::utils::atomic_write_json(p, v)`，**全仓 982 条判据一条不红**，
/// 而它们的判据名逐字写着 `only_reads` / `never_writes`。
///
/// ⇒ 与其在每个只读模块里各写一张「已知写者」清单（那是下一个漂移源），
/// 不如让它们都问同一张表：**本模块的 `WRITE_SITES` 就是那张表**。
/// ★ **本机起进程的落点也要申报**〔audit-0805 08-08，Phase G 第 52 件〕。
///
/// 写盘那一侧（本模块正题）与远端执行那一侧（`exec_site_registry`）都已经有人数了，
/// **本机 `Command::new` 这一侧没有** —— 08-08 实测：往 `session_map.rs` 加一句
/// `Command::new("sh").arg("-c").arg(arg)`，**全仓 983 条判据一条不红**。
///
/// ⚠ 先核查出关键一半：**daemon 侧早就有这张表**（`readonly_guard` 的 `ALLOWED` +
/// `SPAWN_SITES_TODAY`），monitor 侧从来没有。又是「同一形态在另一半原样存在」，
/// 只是这次缺的是 monitor（第 34、44 件是反过来）。
///
/// 两半各有一张表**不违反 E3**：「谁在本半起进程」本来就是两个事实，
/// 各自的权威源在各自那一半。这里只对齐**形状**，不共享清单。
#[cfg(test)]
mod spawn_sites {
    use std::path::{Path, PathBuf};

    /// `(文件, 函数, 起的是什么, 为什么必须起进程)`。**默认拒绝**：人群从源码派生。
    const SPAWNS: &[(&str, &str, &str, &str)] = &[
        ("account_usage.rs", "run_local_probe", "`sh -c <载荷>`",
         "本机用量探针：载荷由 `probe_command_for` 构造并引用过（`exec_site_registry` 里那条 Builder 行管它）"),
        ("launch.rs", "launch_local_posix", "用户配置的终端 argv[0]",
         "在用户的终端里起会话 —— 承接 C13「最后那次 exec 在用户终端里」，这是本产品的主用途"),
        ("launch.rs", "launch_powershell_window", "`wt.exe` / `powershell.exe`",
         "Windows 侧同上；两个名字都是常量，不吃用户输入"),
        ("launch.rs", "ssh_client_available", "探测用的 `ssh`",
         "只探测「本机有没有 ssh」，不带用户参数"),
        ("lib.rs", "open_with_os", "`cmd` / `open` / `xdg-open`",
         "按平台打开日志目录：三个名字都是常量，路径是 monitor 自己的目录"),
        ("local_backend.rs", "supervise", "被监护的 daemon 二进制",
         "本机后端监护：二进制路径来自 `candidates`（有 `candidates_never_point_into_a_build_tree` 守着）"),
        ("local_query.rs", "run_query", "daemon 二进制 + 只读子命令",
         "本机只读查询：`bin` 同上来自候选表，`args` 是本模块构造的固定子命令"),
        ("ssh_source.rs", "resolve_ssh_host", "`ssh -G <host>`",
         "解析 ssh_config 的别名 —— 只读一次配置，不建连接"),
    ];

    fn src_root() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("src")
    }

    /// 某一行所在的函数名（往回找最近的 `fn`）。
    fn enclosing_fn(lines: &[&str], at: usize) -> String {
        for l in lines[..=at].iter().rev() {
            if let Some(rest) = l.split(" fn ").nth(1).or_else(|| l.strip_prefix("fn ")) {
                let n: String = rest
                    .chars()
                    .take_while(|c| c.is_alphanumeric() || *c == '_')
                    .collect();
                if !n.is_empty() {
                    return n;
                }
            }
        }
        "<找不到外层函数>".to_string()
    }

    #[test]
    fn every_local_spawn_is_declared() {
        let files = guard_core::scan_tree!(&src_root(), &["rs"]);
        let mut found: Vec<(String, String)> = Vec::new();
        for (path, src) in &files {
            let prod = guard_core::production_code(src);
            let lines: Vec<&str> = prod.lines().collect();
            let stem = path
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap()
                .to_string();
            for (i, l) in lines.iter().enumerate() {
                if l.contains(concat!("Command::", "new(")) {
                    found.push((stem.clone(), enclosing_fn(&lines, i)));
                }
            }
        }
        found.sort();
        found.dedup();
        assert!(
            found.len() >= 5,
            "全树只找到 {} 处本机起进程（08-08 实测 8 个「文件::函数」）—— 抽取器坏了，本条此刻无效",
            found.len()
        );

        let missing: Vec<String> = found
            .iter()
            .filter(|(f, n)| !SPAWNS.iter().any(|(sf, sn, _, _)| sf == f && sn == n))
            .map(|(f, n)| format!("  {f}::{n}"))
            .collect();
        assert!(
            missing.is_empty(),
            "这些地方**会在用户机器上起一个进程，但没人申报**：\n{}\n\n\
             ⚠ 08-08 实测：往生产段加一句 `Command::new(\"sh\").arg(\"-c\")`，全仓判据一条不红。\n\
             登记进 `SPAWNS`：写清**起的是什么**、**为什么必须起进程**。\n\
             daemon 侧同类表在 `readonly_guard`（`ALLOWED` + `SPAWN_SITES_TODAY`）。",
            missing.join("\n")
        );

        let stale: Vec<String> = SPAWNS
            .iter()
            .filter(|(f, n, _, _)| !found.iter().any(|(ff, nn)| ff == f && nn == n))
            .map(|(f, n, _, _)| format!("  {f}::{n}"))
            .collect();
        assert!(
            stale.is_empty(),
            "申报表里这些落点已经不起进程了（改名或删了）：\n{}\n\
             改名也要红 —— 名字变了就该有人重新看一眼它起的是什么。",
            stale.join("\n")
        );
    }
}

#[cfg(test)]
pub(crate) mod writers {
    /// 所有会写盘的函数名（去重）。**从 `WRITE_SITES` 派生**，不是手写。
    pub(crate) fn names() -> Vec<&'static str> {
        let mut v: Vec<&'static str> = super::tests::WRITE_SITES
            .iter()
            .map(|(_, f, _, _)| *f)
            .collect();
        v.sort_unstable();
        v.dedup();
        v
    }

    /// 一段生产代码里调到的写者。`self_file` 是调用方自己的文件名 ——
    /// 它自己的写点不算「委托」（那由 `WRITE_SITES` 直接管）。
    pub(crate) fn called_by(prod: &str, self_file: &str) -> Vec<&'static str> {
        super::tests::WRITE_SITES
            .iter()
            .filter(|(f, _, _, _)| *f != self_file)
            .map(|(_, fname, _, _)| *fname)
            .filter(|fname| prod.contains(&format!("{fname}(")))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    /// 写盘调用的**形态清单**。人群按「做了什么」取：这些是 Rust 侧真正落盘的动作。
    ///
    /// ⚠ 只读的 `fs::read*` / `metadata` / `exists` **不在其中** —— 本条钉的是「写」。
    const WRITE_CALLS: &[&str] = &[
        "fs::write(",
        "fs::create_dir_all(",
        "fs::create_dir(",
        "fs::copy(",
        "fs::remove_file(",
        "fs::remove_dir_all(",
        "fs::remove_dir(",
        // ⚠ 这一个**刻意不带左括号**。带上就成了 `::rename(`，而
        // `atomic_replace_registry` 正是按那个形态数「原子替换调用点」——
        // 本文件一加它当场红（实测）。往那张表里塞一条豁免是最省事的消法，
        // 但那会把它变成废纸；去掉括号后：我这边照样认得出真调用，
        // 而**哪天有人在本文件里真写了一次 `fs::rename(...)`，那张表仍会逮住它**。
        // 代价是这个 needle 稍宽（会命中 `fs::rename_xxx` 之类），今天全树无此形态。
        "fs::rename",
        "File::create(",
    ];

    /// 每个落点的申报：`(文件, 函数, 属于哪个已声明工具, 说法)`。
    ///
    /// 第三列 `Some(id)` = **这是那个工具的安装/卸载动作**，`id` 必须在
    /// `tool_registry::TOOLS` 里真实存在（下面有对拍）；`None` = **不是安装动作**，
    /// 第四列要写清它写的是什么。
    ///
    /// ⚠ **默认拒绝**：人群从源码派生，没在这张表里的落点当场红。
    /// 「谁进人群」由机器定，「它是不是安装动作」才是人的答案。
    #[allow(clippy::type_complexity)]
    pub(crate) const WRITE_SITES: &[(&str, &str, Option<&str>, &str)] = &[
        // ── 安装动作：写的是**用户既有的环境/配置**，且对应声明表里的一个工具
        ("profile_installer.rs", "install_to_profile", Some("ccm"),
         "往用户 shell profile 的 BEGIN/END 块里装 ccm 启动器（写前先备份）"),
        ("profile_installer.rs", "uninstall_from_profile", Some("ccm"),
         "从 profile 里摘掉那个块（同样先备份）"),
        ("profile_installer.rs", "atomic_write_string", Some("ccm"),
         "上面两个动作唯一的落盘漏斗：临时文件 + rename"),
        ("profile_installer.rs", "atomic_replace_path", Some("ccm"),
         "跨设备回退的 rename。⚠ 这是**四份平台原语副本之一**，四份都已登记在 `atomic_replace_registry`（承接 C10）——本条不重复判它，只记它是个写点"),
        ("mcp.rs", "write_json_atomic", Some("project-mcp"),
         "写项目级 MCP 服务器配置（`TOOLS` 里 `project-mcp` 那条的真落点）"),
        // ── 不是安装动作：写的是 monitor 自己的东西
        ("bind.rs", "spawn", None, "monitor 自己的运行时目录/落地文件"),
        ("bind.rs", "process_await_file", None, "monitor 自己的等待文件"),
        ("bind.rs", "cleanup_dead", None, "清理 monitor 自己留下的死文件"),
        ("config.rs", "save_config", None, "monitor 自己的配置文件"),
        ("config.rs", "atomic_replace", None, "原子替换原语的本地副本（同上，归 `atomic_replace_registry` 判）"),
        ("lib.rs", "open_log_dir", None, "打开日志目录前确保它存在"),
        ("logging.rs", "build_rolling_appender", None, "monitor 自己的滚动日志"),
        ("logging.rs", "write_diagnostics_to_config", None, "把诊断信息写进 monitor 自己的配置"),
        ("logging.rs", "atomic_replace", None, "原子替换原语的本地副本（头注自陈是从 config.rs 复制的）"),
        ("session_map.rs", "run_watcher", None, "monitor 自己的会话映射状态"),
        ("sftp_pool.rs", "download_inner", None, "把远端文件落到本地缓存；写的不是用户既有环境"),
        ("utils.rs", "atomic_write_json", None, "通用原子写原语，调用方各自申报"),
        ("utils.rs", "atomic_replace_path", None, "同上，原语的本地副本"),
        // ── 既不是安装、也不是「monitor 自己的」：**删用户数据**
        ("history.rs", "delete_history_session", None,
         "★ 删的是用户 `~/.claude/projects/**` 下的会话文件（用户主动发起）。\
          它不是安装动作，但也不是 monitor 自己的东西。⚠ **08-07 订正**：本行原写「围栏由 \
          `validate_delete_target` 与它自己的判据守着」—— 那句只对一半：围栏函数有五条穿越 \
          防护判据，但**没有任何东西钉住那条路真的过了围栏**（实测跳过围栏，全仓 978 条不红）。 \
          现由 `history.rs::the_delete_entry_point_actually_goes_through_the_fence` 端到端钉住"),
    ];

    /// 一行 `use ... fs ...` 该放行还是该拦。`Ok(())` = 放行。
    ///
    /// # 为什么要有这道
    ///
    /// 本模块的人群靠 **`fs::` 这个前缀**认写盘调用（`WRITE_CALLS` 每一条都带它），
    /// `hooks_diag::this_module_never_writes` 的白名单也是扫 `fs::`。
    /// 两条判据因此**共享同一个前提**：`std::fs` 只能以带前缀的形态出现。
    ///
    /// 08-07 实测这个前提没人守：往 `hooks_diag.rs` 里加
    /// `use std::fs as sysio;` + `sysio::write(p, s)`（一个名叫
    /// `this_module_never_writes` 的模块里真写一次盘），全仓 **974 条判据一条不红** ——
    /// 连本模块上一轮刚建的写点人群都漏掉了。⚠ 头一次变异我把别名取成 `ffs`，
    /// 而 `ffs::write(` 里**含有** `fs::write(` 子串 ⇒ 两条判据都红了，
    /// 差点被我读成「有人守着」。**变异要造得像，巧合的红比不红更骗人。**
    ///
    /// 修法照 daemon 侧 `readonly_guard` 的先例：**堵逃生口**，别去追那些改写形态。
    fn fs_import_verdict(line: &str) -> Result<(), String> {
        let s = line.trim();
        if !s.starts_with("use ") || !s.contains("fs") {
            return Ok(());
        }
        // 放行两种，两种今天都真实存在（各自的理由写在这里，不是「看着眼熟」）：
        // · `use std::fs;`      —— 调用处必然写成 `fs::xxx(`，前缀还在
        // · `use std::fs::File;` —— File 的写入口 `File::create(` 自己就在 `WRITE_CALLS` 里
        if s == "use std::fs;" || s == "use std::fs::File;" {
            return Ok(());
        }
        if s.contains("std::fs as ") || s.contains("fs as ") {
            return Err("别名导入：调用处不再带 `fs::` 前缀，两条判据同时瞎掉".into());
        }
        if s.contains("std::fs::*") {
            return Err("glob 导入：写函数变成裸名字，前缀扫描扫不到".into());
        }
        if s.contains("std::fs::") {
            return Err("条目导入：`use std::fs::write;` 之后 `write(..)` 不带前缀".into());
        }
        Ok(())
    }

    /// ★★ **没有一种导入形态能让写盘调用丢掉 `fs::` 前缀**〔audit-0805 08-07〕。
    ///
    /// 这是上面那条人群、以及 `hooks_diag::this_module_never_writes` 白名单的**共同前提**。
    /// 前提没人守的时候，两条判据会**同时**瞎掉且都保持绿色 —— 08-07 实测过（见
    /// [`fs_import_verdict`] 的头注）。
    #[test]
    fn no_alias_or_item_import_can_hide_a_write_call() {
        // ① 常驻正反例：真代码里今天**没有**违规样本，那一支平时没人行使 ——
        //    本仓已连着五次栽在「新分支平时没人走」上，所以样本写死在这里。
        for ok in [
            "use std::fs;",
            "use std::fs::File;",
            "use std::path::Path;",
            "let x = 1;",
        ] {
            assert!(
                fs_import_verdict(ok).is_ok(),
                "{ok:?} 该放行却被拦了 —— 分类器过严会逼人去改合法代码"
            );
        }
        for bad in [
            "use std::fs as sysio;",
            "use std::fs as f;",
            "use std::fs::*;",
            "use std::fs::write;",
            "use std::fs::{self, write};",
        ] {
            assert!(
                fs_import_verdict(bad).is_err(),
                "{bad:?} 该被拦却放行了 —— 这正是 08-07 那次真写盘溜过去的形态"
            );
        }

        // ② 全树扫描。
        let files = guard_core::scan_tree!(&src_root(), &["rs"]);
        let mut checked = 0usize;
        let mut offenders = Vec::new();
        for (path, src) in &files {
            let prod = guard_core::production_code(src);
            for line in prod.lines() {
                if line.trim().starts_with("use ") && line.contains("fs") {
                    checked += 1;
                    if let Err(why) = fs_import_verdict(line) {
                        offenders.push(format!("  {}: {} —— {why}", path.display(), line.trim()));
                    }
                }
            }
        }
        // 抽取器自检：一条 `use ... fs ...` 都没扫到 ⇒ 上面整段在空转。
        assert!(
            checked >= 3,
            "全树只扫到 {checked} 行含 fs 的 `use`（08-07 实测 5：`use std::fs;` ×1 + `use std::fs::File;` ×4）\
             —— 剥法或扫描面坏了，本条此刻无效"
        );
        assert!(
            offenders.is_empty(),
            "这些导入会让写盘调用**丢掉 `fs::` 前缀**：\n{}\n\n\
             ⚠ 前缀一没，两条判据同时瞎掉且都保持绿色：\n\
             · 本模块的写点人群（`WRITE_CALLS` 每一条都带 `fs::`）；\n\
             · `hooks_diag::this_module_never_writes` 的白名单（它扫的就是 `fs::`）。\n\
             08-07 实测：`use std::fs as sysio;` + `sysio::write(p, s)` 放进 `hooks_diag.rs`，\n\
             974 条判据一条不红 —— 而那个模块判据的名字逐字写着「never writes」。\n\
             修法：用 `use std::fs;` 走全前缀，或直接写 `std::fs::xxx(`。别改本条去迁就它。",
            offenders.join("\n")
        );
    }

    fn src_root() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("src")
    }

    /// 从一份源码的**生产段**里抠出「有写盘调用的 (函数名)」。
    ///
    /// ⚠ 归属按「上一处 `fn 名字`」判 —— 粗，但**只会把落点归给更靠前的函数**，
    /// 归错了会让申报表里出现一个对不上的名字，那是**会红**的方向（不是静默变绿）。
    fn write_fns(src: &str) -> Vec<String> {
        let prod = guard_core::production_code(src);
        let mut cur = String::new();
        let mut out: Vec<String> = Vec::new();
        for line in prod.lines() {
            if let Some(rest) = line
                .split(" fn ")
                .nth(1)
                .or_else(|| line.strip_prefix("fn "))
            {
                let name: String = rest
                    .chars()
                    .take_while(|c| c.is_alphanumeric() || *c == '_')
                    .collect();
                if !name.is_empty() {
                    cur = name;
                }
            }
            if WRITE_CALLS.iter().any(|c| line.contains(c))
                && !cur.is_empty()
                && !out.contains(&cur)
            {
                out.push(cur.clone());
            }
        }
        out
    }

    /// ★ 正题：**每个写盘落点都得申报**，安装动作那一类还要点名真实存在的工具。
    #[test]
    fn every_write_site_is_declared_and_installers_name_a_real_tool() {
        let files = guard_core::scan_tree!(&src_root(), &["rs"]);
        let mut found: Vec<(String, String)> = Vec::new();
        for (path, src) in &files {
            let stem = path
                .file_name()
                .and_then(|s| s.to_str())
                .expect("文件名")
                .to_string();
            for f in write_fns(src) {
                found.push((stem.clone(), f));
            }
        }
        // ★ 抽取器自检：扫不到东西时下面整条会零命中地绿。
        assert!(
            found.len() >= 15,
            "全树只找到 {} 个写盘落点（08-07 实测 19 个「文件::函数」）—— 抽取器坏了，本条此刻无效",
            found.len()
        );

        let missing: Vec<String> = found
            .iter()
            .filter(|(f, n)| !WRITE_SITES.iter().any(|(sf, sn, _, _)| sf == f && sn == n))
            .map(|(f, n)| format!("  {f}::{n}"))
            .collect();
        assert!(
            missing.is_empty(),
            "这些地方**会往盘上写东西，但没人申报它是不是安装动作**：\n{}\n\n\
             ⚠ `ROADMAP §5 4b` 记的正是这个缺口：`tool_registry` 只守声明表自洽，\n\
             人群是「声明了的工具」而不是「真实发生的安装动作」——\n\
             于是新增一个写点，声明表可以一直不知道。\n\
             登记进 `WRITE_SITES`：是某个工具的安装动作就点名它的 `id`（要在 `TOOLS` 里真实存在），\n\
             不是就写清它写的是什么（例如「monitor 自己的缓存」）。",
            missing.join("\n")
        );

        // ★ 反向锚点：申报了一个已经不存在的落点 ⇒ 它在替真判据挡枪。
        let stale: Vec<String> = WRITE_SITES
            .iter()
            .filter(|(f, n, _, _)| !found.iter().any(|(ff, nn)| ff == f && nn == n))
            .map(|(f, n, _, _)| format!("  {f}::{n}"))
            .collect();
        assert!(
            stale.is_empty(),
            "申报表里这些落点**已经不写盘了**（改名、收口或删掉了）：\n{}\n\
             改名也要红 —— 名字变了就该有人重新回答一次「它是不是安装动作」。",
            stale.join("\n")
        );

        // ★★ 4b 要的那条连线：安装动作必须点名 `TOOLS` 里真实存在的 id。
        let table = guard_core::production_code(include_str!("tool_registry.rs"));
        let mut checked = 0usize;
        for (f, n, tool, _) in WRITE_SITES {
            let Some(id) = tool else { continue };
            checked += 1;
            assert!(
                table.contains(&format!("id: \"{id}\"")),
                "`{f}::{n}` 申报成工具 `{id}` 的安装动作，但 `tool_registry::TOOLS` 里没有这个 id。\n\
                 要么 id 写错了，要么那个工具被删了而真实的安装动作还留着 —— 后者更值得查。"
            );
        }
        // 常驻自检：一条安装动作都没有时，上面那个循环空转，而它看起来照样绿。
        assert!(
            checked >= 3,
            "申报表里只有 {checked} 条「安装动作」（08-07 实测 5）—— \
             要么真收口了（那很好，把这个数调下来），要么有人把它们改成了 `None` 绕过对拍。"
        );
    }
}
