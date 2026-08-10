//! ★ **每一处会写到别人机器上的落点都要申报**〔devbench F10c〕。
//!
//! # 它接的是三张表各自划出去、然后没人接的那道缝
//!
//! | 表 | 它自己写的边界（逐字） |
//! |---|---|
//! | `exec_site_registry` | 「**不守**…⑤ 走 SFTP / 本机 `Command` 的路」 |
//! | `write_site_registry` | 「**不守**…③ 通过 `Command` 起外部进程间接写盘（本条只看 Rust 侧的 `fs::` 调用面）」 |
//! | `readonly_guard` | 只扫 daemon 生产源码 |
//!
//! ⇒ 本机写面有 `write_site_registry` 数着 36 处、「新增写盘落点不申报当场红」；
//! **远端写面（写的是别人机器上的 `~/.bashrc` / `~/.claude/` / 任意用户选的路径）零登记**。
//! 三张表都不是「忘了」，是各自划出去之后没人接。
//!
//! # 人群怎么取（这是本件唯一真正难的地方）
//!
//! 〔audit-0805 V7-2〕给过一个候选口径并**自己判它不可用**：扫写原语的方法名
//! （`.write(` / `.create(` / `.remove_file(` / `.rename(` / `.create_dir(`）。
//! 08-10 复量那个口径：**36 处、8 个文件**，而其中 6 个文件
//! （`bind.rs` 6 · `history.rs` 2 · `inbound_client.rs` 2 · `logging.rs` 2 · `search.rs` 3 ·
//! `session_map.rs` 2）**压根不碰远端** —— 它们写的是本机文件或一条流。
//!
//! ★ 那是**按怎么写取样**（一个方法叫什么名字），而要钉的事实是**写到谁的机器上**。
//! 换成后者：**远端写必须先拿到一个 SFTP 会话**，而会话对象只出现在 **3 个文件**里
//! （`sftp.rs` · `sftp_pool.rs` · `mcp.rs`）。人群当场从 8 个文件收到 3 个、零本机噪声。
//!
//! ⇒ 与 `write_site_registry` 头注收敛出的同一句：**判据锚在「怎么写」上就会又漏又吵；
//! 换成「做了什么」之后，原本被判为「做不了」的机检往往当场可行。**
//!
//! ## 但「哪个 handle 是远端的」机器判不了 ⇒ 超集 + 人的答案
//!
//! 同一个文件里 `rf.write_all()` 写的是远端（handle 来自 `sftp.create`）、
//! `lf.write_all()` 写的是本机（下载落地）。**从方法调用那一行看不出 handle 的来历**，
//! 而按 binding 名字（`rf`/`lf`）判就又回到了「按拼法取样」。
//!
//! ⇒ 照 `byte_cap_registry` 那条既有形状办：**人群取可机判的超集**
//! （持有 SFTP 会话的文件里每一个调写原语的函数），
//! 「落地何方」这个语义判断**登记成人的答案**，而不是猜。
//!
//! # 它守什么、不守什么
//!
//! **守**：① 那 3 个文件里新增一个写原语调用点而不申报 ⇒ 当场红；
//! ② 申报为**用户选路径**的远端写必须过 Claude 数据防误伤围栏
//! （★ 今天五个入口全过，而**在本表之前零判据钉着这件事**）；
//! ③ 对外 IPC 入口必须真的转发到一个已登记的原语点（接线层）。
//!
//! **不守**：① **exec 那条路**（`connect_and_exec_cmd` 发一条会写的 shell 命令）——
//!    判「一条 shell 命令会不会写」**没有可靠语法特征**：`2>/dev/null` 就带 `>`，
//!    而 `install_remote_ccm_helper` 真正的写又是走 SFTP 的。那条路的扼流点是
//!    `exec_site_registry`（它数着每一处执行、每条命令的来历），
//!    **在那张表上加一格「这条命令写不写远端」是另一件事**。如实登记，不假装覆盖。
//! ② 写的**内容**对不对（各模块自己的行为判据）；
//! ③ 人群键是「文件::函数」，同一个函数内部多写一个文件看不见（同 `write_site_registry`）。

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    fn repo_root() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("仓根")
            .to_path_buf()
    }

    /// 写原语。**刻意含本机也会用的那几个** —— 人群是超集，分类靠登记。
    ///
    /// ⚠ **运行时拼**，照 `local_read_surface_registry::needles()` 的既有先例。
    /// 首跑实测：写成 `const &[&str]` 字面量之后，`atomic_replace_registry` 当场把本文件
    /// 报成「有原子替换调用点没登记：`remote_write_registry.rs [rename] 1 处`」——
    /// 它用 `strip_comment_lines` 剥注释（所以头注里那串没事），但**不剥 `cfg(test)` 段**，
    /// 于是这里的字符串字面量被当成了一处真实 `.rename(` 调用。
    /// ⇒ 判据的针写成字面量，就会被**别的判据**当成靶子。
    fn write_prims() -> Vec<String> {
        let dot = ".";
        [
            "write",
            "create",
            "remove_file",
            "remove_dir",
            "rename",
            "create_dir",
            "set_metadata",
            "write_all",
        ]
        .iter()
        .map(|m| format!("{dot}{m}("))
        .collect()
    }

    /// 落地何方。**刻意是封闭集合** —— 多出第三种就得回来论证。
    const LANDINGS: &[&str] = &["远端", "本机"];

    /// `(相对 src 的文件, 函数, 落地何方, 写什么 + 路径由谁定 + 什么围栏着)`
    ///
    /// ⚠ **登记表不是豁免清单**：新增一处没登记的 ⇒ 下面第一条红。
    #[allow(clippy::type_complexity)]
    const REMOTE_WRITES: &[(&str, &str, &str, &str)] = &[
        // ---- sftp.rs：路径全部由**代码**定（部署落点 / 已知配置文件），不是用户选的 ----
        (
            "sftp.rs",
            "upload_atomic",
            "远端",
            "近原子替换的核心：写 `.tmp` → 备份旧文件为 `.bak` → rename 上位。\
             **路径由调用方给**，而调用方全是「代码定路径」那一族（daemon 落点 / `.mcp.json` /\
             `sftp_write_text` 转发过来的用户选路径）。⚠ 本函数自己**不设围栏** ——\
             用户选路径那条链的围栏在入口 `sftp_pool::sftp_write_text` 上（见下面第三条判据）。",
        ),
        (
            "sftp.rs",
            "ensure_dir_all",
            "远端",
            "逐级 `create_dir`（best-effort，已存在就忽略）。路径由调用方给，\
             用在部署与 helper 安装的固定落点上。",
        ),
        (
            "sftp.rs",
            "uninstall_remote_daemon",
            "远端",
            "删远端 daemon 二进制。路径**由代码定**（`daemon_binary()` 的落点），不接受用户输入。",
        ),
        (
            "sftp.rs",
            "remove_remote_file",
            "远端",
            "删一份远端会话 jsonl。★ 它有**自己的**围栏 `is_safe_remote_jsonl`\
             （`projects/` 前缀 + `.jsonl` 后缀 + 无 `..`），而**不是** `is_protected_claude_data_path` ——\
             因为它的正题恰恰是「删 Claude 的会话文件」，那是历史浏览器的功能。\
             **路径由用户选**（在历史浏览器里点某一份会话），但选的范围被那道围栏收在\
             `projects/**/*.jsonl` 之内。⚠ 两道围栏方向相反，别互相替代。",
        ),
        (
            "sftp.rs",
            "install_remote_ccm_helper",
            "远端",
            "建 helper 目录（`create_dir`）；真正写文件走 `upload_atomic`。\
             路径**由代码定**（helper 的固定安装位）。",
        ),
        // ---- sftp_pool.rs：文件面板，路径**由用户选** ⇒ 全部要过 Claude 数据围栏 ----
        (
            "sftp_pool.rs",
            "upload_inner",
            "远端",
            "上传：写远端 `.tmp` → 删旧 → rename。**路径由用户选**（面板里点的目标目录），\
             围栏在入口 `sftp_upload` 的 `guard_write`。",
        ),
        (
            "sftp_pool.rs",
            "sftp_mkdir",
            "远端",
            "新建远端目录。**路径由用户选**，`guard_write` 在函数第一行。",
        ),
        (
            "sftp_pool.rs",
            "sftp_rename",
            "远端",
            "重命名远端文件。**路径由用户选**，且 `from` 与 `to` **各过一次** `guard_write`\
             （既不许把 Claude 文件改走，也不许改成 Claude 数据源名）。",
        ),
        (
            "sftp_pool.rs",
            "sftp_delete",
            "远端",
            "删远端文件/目录。**路径由用户选**，`guard_write` 在函数第一行。",
        ),
        (
            "sftp_pool.rs",
            "download_inner",
            "本机",
            "★ **这是超集里唯一的本机项**，也正是「机器分不清 handle 来历」的实例：\
             它和 `upload_inner` 在同一个文件、用同一个方法名 `.write_all(`，\
             但 handle 来自 `tokio::fs::File`（下载落地）而不是 `sftp.create` ⇒ 写的是**本机**。\
             ⇒ 不需要远端围栏。⚠ 它另有一个**本机**侧的问题（用户选的本机落点由前端对话框给），\
             那属 `write_site_registry` 的管辖面，不在本表。",
        ),
    ];

    /// 从三个「持有 SFTP 会话」的文件里抠出 `(文件, 函数)` —— 函数体内调了写原语。
    fn write_sites() -> Vec<(String, String)> {
        let root = repo_root();
        let prims = write_prims();
        let mut out = Vec::new();
        for f in capability_holders() {
            let raw = std::fs::read_to_string(root.join("src-tauri/src").join(&f))
                .unwrap_or_else(|e| panic!("{f} 读不到：{e} —— 文件搬了就把登记一起改"));
            let prod = guard_core::production_code(&raw);
            let mut cur = String::new();
            for line in prod.lines() {
                let t = line.trim_start();
                // `fn 名(` 起一个新作用域。闭包不算（闭包里的写归外层函数）。
                //
                // ⚠ **不许用前缀白名单**〔本条首跑就红在这里〕：第一版列了
                // `["pub async fn ", "pub fn ", "async fn ", "fn "]`，**漏掉 `pub(crate) async fn`** ——
                // 于是 `sftp.rs::ensure_dir_all`（正是这个可见性）没起新作用域，
                // 它那处 `create_dir` 被记到**上一个函数** `read_profile_text` 头上，
                // 而那个函数一个写原语都没有。诊断当场点名了 `read_profile_text`。
                // ⇒ 改成**按结构判**：在 `fn ` 之前的东西必须全是可见性/修饰符 token。
                // 这样 `pub(in crate::x) const unsafe fn` 之类的写法也认得。
                if let Some((head, rest)) = t.split_once("fn ") {
                    let modifiers_only = head.split_whitespace().all(|w| {
                        w == "pub"
                            || w.starts_with("pub(")
                            || w == "async"
                            || w == "const"
                            || w == "unsafe"
                            || w == "extern"
                            || w.starts_with('"')
                    });
                    if modifiers_only {
                        if let Some(name) = rest.split(['(', '<']).next() {
                            cur = name.trim().to_string();
                        }
                    }
                }
                if cur.is_empty() {
                    continue;
                }
                if prims.iter().any(|p| line.contains(p.as_str()))
                    && !out.iter().any(|(g, m)| *g == f && *m == cur)
                {
                    out.push((f.clone(), cur.clone()));
                }
            }
        }
        out.sort();
        out
    }

    /// 「持有远端写能力」= **拿到过一个 SFTP 会话**。这是人群的根。
    ///
    /// ★★〔G 审计逮到的〕**第一版按「源码里出现 SFTP 会话类型名」判，恒绿。**
    ///
    /// 漏掉的是 `acct_iso_deploy.rs`：它 `let conn = connect_sftp(&cfg).await?;`
    /// `let sftp = &conn.sftp;` 然后往**用户给的 `dest_dir`** 写文件
    /// （`#[tauri::command] deploy_remote_acct_iso`）——**类型靠推断，那个名字一次都没写出来**。
    ///
    /// ⇒ 又一次「按拼法取样」，而本条的 doc 注释里逐字写着
    /// 「**凡「本条是某一族唯一的哨兵」的判据，人群必须按事实取样，因为它没有第二道网**」。
    /// **写下那句话的判据，自己犯了那句话说的错** —— 本区第三次量到同一条。
    ///
    /// ⇒ 改成按**能力从哪来**取样：SFTP 会话在本仓只有两个出处 ——
    /// `sftp::connect_sftp()`（自己开一条）与 `sftp_pool::with_sftp()`（从池里借）。
    /// 拿不到会话就写不了远端，这是结构事实，绕不过去。
    /// 类型名仍然留在针里（`sftp.rs`/`sftp_pool.rs` 自己要靠它进人群）。
    ///
    /// ⚠ 仍然失效的形态如实登记：把会话再包一层自己的 newtype 传出去，本条看不见那一层。
    /// 真要堵死得走可见性（`pub(in crate::sftp) struct RemoteWriter(SftpSession)`），
    /// 那样「远端写能力在哪几个模块」就是**编译器答案**而不是 grep 答案。已登记，未做。
    fn capability_holders() -> Vec<String> {
        let root = repo_root().join("src-tauri/src");
        let mut out = Vec::new();
        for (path, src) in guard_core::scan_tree!(&root, &["rs"]) {
            let prod = guard_core::production_code(&src);
            // 拼出来的，免得命中本文件自己的说明。
            let ty = format!("Sftp{}", "Session");
            // 会话的两个出处：自己开一条 / 从池里借。拿不到会话就写不了远端。
            let opens = format!("connect_{}(", "sftp");
            let borrows = format!("with_{}(", "sftp");
            if prod.contains(&ty)
                || prod.contains("russh_sftp")
                || prod.contains(&opens)
                || prod.contains(&borrows)
            {
                out.push(
                    path.strip_prefix(&root)
                        .unwrap_or(&path)
                        .to_string_lossy()
                        .replace('\\', "/"),
                );
            }
        }
        out.sort();
        out
    }

    /// ★ 正题一：**默认拒绝** —— 那三个文件里每一处写原语调用都得说清落地何方。
    #[test]
    fn every_write_primitive_in_a_capability_holder_is_classified() {
        let sites = write_sites();
        assert!(
            sites.len() >= 8,
            "只抠到 {} 处写点（08-10 实测 10）—— 抽取器坏了，本条此刻是空转的",
            sites.len()
        );
        let missing: Vec<String> = sites
            .iter()
            .filter(|(f, n)| !REMOTE_WRITES.iter().any(|(g, m, ..)| g == f && m == n))
            .map(|(f, n)| format!("  {f}::{n}"))
            .collect();
        assert!(
            missing.is_empty(),
            "这些写点没登记：\n{}\n\n\
             ★ 它们在一个**持有 SFTP 会话**的文件里 —— 也就是说这段代码手上有\n\
             「往别人机器上写」的能力。要回答三件事，都不许留空：\n\
             ① **落地何方**（{LANDINGS:?}）—— 机器分不清 handle 来历，这是人的答案；\n\
             ② **路径由谁定**（代码定的固定落点？还是用户在面板里选的？）；\n\
             ③ **什么围栏着**（用户选路径的必须过 Claude 数据防误伤围栏，见第三条判据）。",
            missing.join("\n")
        );
        // 反向：登记的必须真在（改名/删了就该删条目）。
        for (f, n, ..) in REMOTE_WRITES {
            assert!(
                sites.iter().any(|(g, m)| g == f && m == n),
                "登记表里的 `{f}::{n}` 已经不在源码里了 —— 删掉这条，别留僵尸账"
            );
        }
    }

    /// ★ 正题二：落地何方在封闭集合里，且说明不许是占位。
    #[test]
    fn a_registered_write_says_where_it_lands_and_who_picks_the_path() {
        for (f, n, landing, why) in REMOTE_WRITES {
            assert!(
                LANDINGS.contains(landing),
                "`{f}::{n}` 的落地写的是「{landing}」，不在 {LANDINGS:?} 里。\
                 多出一种就得回来论证 —— 本表的全部意义是分清「写谁的机器」。"
            );
            assert!(
                why.chars().count() > 20,
                "`{f}::{n}` 的说明太短，像是占位：「{why}」"
            );
            // 「路径由谁定」是本表的核心信息之一，措辞可以变，但必须提到「路径」。
            assert!(
                why.contains("路径") || why.contains("落点"),
                "`{f}::{n}` 没说**路径由谁定**（代码定的固定落点 / 用户选的）。\
                 那决定了它要不要围栏 —— 说明里必须提到「路径」或「落点」。"
            );
        }
        // 封闭集合不许长草。
        for l in LANDINGS {
            assert!(
                REMOTE_WRITES.iter().any(|(_, _, x, _)| x == l),
                "落地集合里的「{l}」今天一处都没人用 —— 删掉它"
            );
        }
    }

    /// 一个顶层 `fn` 的函数体到哪一行为止。
    ///
    /// ★ 用**第一行行首的 `}`** 判 —— 那是 rustfmt 下顶层项的真实收尾。
    /// ⚠ 第一版用「下一行以 `pub ` 或 `#[tauri::command]` 开头」，两个洞：
    /// ① 非 `pub` 的顶层 `fn`（本仓的 `fn guard_write` / `async fn upload_inner`）不终止窗口
    ///    ⇒ `sftp_write_text` 的窗口吞进了 `guard_write` 的**定义**，判据当场假绿；
    /// ② `sftp_upload` 的窗口宽到 109 行、把整个 `upload_inner` 吞了进去。
    /// ⇒ 又一次「终止条件按拼法列白名单」。
    fn fn_body_end(lines: &[&str], start: usize) -> usize {
        lines
            .iter()
            .enumerate()
            .skip(start + 1)
            .find(|(_, l)| l.starts_with('}'))
            .map(|(i, _)| i + 1)
            .unwrap_or(lines.len())
    }

    /// ★ 正题三：**用户选路径的远端写必须过 Claude 数据防误伤围栏**。
    ///
    /// # 它补的洞
    ///
    /// `sftp_pool.rs` 头注逐字写着「防误伤守卫见 `is_protected_claude_data_path`：
    /// **SFTP 写命令拒碰 Claude 数据源文件**（往正被 Claude 打开的 jsonl 写会损坏会话）」。
    ///
    /// 08-10 实测：那句话今天**是真的** —— `sftp_write_text` / `sftp_upload` / `sftp_mkdir` /
    /// `sftp_rename` / `sftp_delete` 五个入口全都调了 `guard_write`。
    /// ⚠ 但**在本条之前没有任何判据钉着它**：删掉任一处 `guard_write?`，
    /// 全仓判据一条不红，而那台远端机上正被 Claude 打开的 jsonl 就能被面板删掉。
    ///
    /// ⚠ 失效模式如实登记：本条钉的是「函数体里出现 `guard_write`」这个**形态**，
    /// 钉不了「围栏用对了路径」（比如过的是 `from` 而写的是 `to`）。
    /// `sftp_rename` 那处「两个参数各过一次」是靠**登记说明**写清的，不是机检。
    #[test]
    fn a_user_chosen_remote_write_passes_the_claude_data_fence() {
        let root = repo_root();
        let raw = std::fs::read_to_string(root.join("src-tauri/src/sftp_pool.rs"))
            .expect("sftp_pool.rs 读不到");
        let prod = guard_core::production_code(&raw);
        // 用户选路径的写入口 = 那五条 `#[tauri::command]` 写命令。
        // ⚠ 人群写死在这里是**刻意的**：「哪些路径是用户选的」没有语法特征，
        //   而这五条是 F47 文件面板的全部写口。第五条判据盯着这个前提别变。
        const USER_CHOSEN_ENTRIES: &[&str] = &[
            "sftp_write_text",
            "sftp_upload",
            "sftp_mkdir",
            "sftp_rename",
            "sftp_delete",
        ];
        let lines: Vec<&str> = prod.lines().collect();
        let mut unfenced = Vec::new();
        let mut checked = 0usize;
        for entry in USER_CHOSEN_ENTRIES {
            let Some(start) = lines.iter().position(|l| {
                let t = l.trim_start();
                t.starts_with(&format!("pub async fn {entry}("))
                    || t.starts_with(&format!("pub fn {entry}("))
            }) else {
                panic!(
                    "找不到写入口 `{entry}` —— 它改名或搬走了。\
                     人群写死在本条里（见头注），改名就得把这里一起改。"
                );
            };
            checked += 1;
            // 函数体到下一个顶层 `fn` / `#[tauri::command]` 为止。
            let end = fn_body_end(&lines, start);
            let body = lines[start..end].join("\n");
            // ⚠ **必须排除定义行**〔G 审计逮到的〕：`fn guard_write(path: &str) …`
            // 里也含 `guard_write`。第一版只判 `body.contains("guard_write")`，
            // 而窗口终止条件又漏掉了非 `pub` 的顶层 `fn` ⇒ `sftp_write_text` 的窗口
            // 把 `guard_write` 的**定义**吞了进来 ⇒ 删掉它真正那次调用，本条**照样绿**。
            // 那正是本条自称要消灭的假绿，五处里有一处没修上。
            let called = body
                .lines()
                .any(|l| l.contains("guard_write(") && !l.trim_start().starts_with("fn "));
            if !called {
                unfenced.push(format!("  sftp_pool.rs::{entry}"));
            }
        }
        assert_eq!(
            checked,
            USER_CHOSEN_ENTRIES.len(),
            "只找到 {checked} 个写入口 —— 抽取器坏了"
        );
        assert!(
            unfenced.is_empty(),
            "这些**用户选路径**的远端写没过 Claude 数据防误伤围栏：\n{}\n\n\
             ★ `sftp_pool.rs` 头注逐字承诺「**SFTP 写命令拒碰 Claude 数据源文件**\n\
             （往正被 Claude 打开的 jsonl 写会损坏会话）」。\n\
             少一道 `guard_write?` 的后果是：那台远端机上**正在跑的会话**的 jsonl\n\
             能被文件面板删掉/覆盖掉，而 monitor 这边只会看到会话突然坏了。\n\
             ⚠ 这道围栏是**防手滑不是合规**（头注原话）—— 但它是唯一一道。",
            unfenced.join("\n")
        );
    }

    /// ★ 正题四：**接线层** —— 对外 IPC 入口必须真的转发到一个已登记的原语点。
    ///
    /// # 为什么单列
    ///
    /// 〔audit-0805 V7-2〕当初点名的十个名字里有五个是**IPC 包装**
    /// （`sftp_write_text` / `sftp_upload` / `deploy_remote_daemon` /
    /// `write_remote_mcp_server` / `delete_remote_history_session`）——
    /// 它们自己**不调写原语**，所以按能力边界派生的人群里**一个都没有**。
    ///
    /// ⇒ 两份名单只重合五个。那不是谁错了，是**两个不同的层**：
    /// 「谁按下按钮」与「谁真的写」。本表的人群是后者，而前者是外面看得见的那一层
    /// ⇒ 必须有一条边把它们连起来，否则改了转发目标没人会红。
    #[test]
    fn the_ipc_entry_points_route_through_a_registered_write_site() {
        let root = repo_root();
        // `(入口所在文件, 入口名, 它该转发到的已登记写点)`
        const ROUTES: &[(&str, &str, &str)] = &[
            ("sftp_pool.rs", "sftp_write_text", "upload_atomic"),
            ("sftp_pool.rs", "sftp_upload", "upload_inner"),
            ("sftp.rs", "deploy_remote_daemon", "upload_atomic"),
            ("mcp.rs", "write_remote_mcp_server", "upload_atomic"),
            (
                "remote_history.rs",
                "delete_remote_history_session",
                "remove_remote_file",
            ),
            // ★〔G 审计补的两条〕它们都持会话 / 往用户给的路径写远端，却因为
            // 「自己不调裸写原语」而进不了按能力边界派生的人群 ——
            // **正是本条（接线层）存在的理由**：两个层，一条边。
            (
                "acct_iso_deploy.rs",
                "deploy_remote_acct_iso",
                "ensure_dir_all",
            ),
            ("sftp.rs", "uninstall_remote_ccm_helper", "upload_atomic"),
        ];
        for (file, entry, target) in ROUTES {
            // 转发目标必须是本表登记过的写点 —— 否则这条边指向账外。
            assert!(
                REMOTE_WRITES.iter().any(|(_, n, ..)| n == target),
                "路由表说 `{entry}` 转发到 `{target}`，可 `{target}` 不在 `REMOTE_WRITES` 里 —— \
                 那这条边指向账外，等于没连"
            );
            let raw = std::fs::read_to_string(root.join("src-tauri/src").join(file))
                .unwrap_or_else(|e| panic!("{file} 读不到：{e}"));
            let prod = guard_core::production_code(&raw);
            let lines: Vec<&str> = prod.lines().collect();
            let Some(start) = lines
                .iter()
                .position(|l| l.trim_start().contains(&format!("fn {entry}(")))
            else {
                panic!("`{file}` 里找不到入口 `{entry}` —— 改名/搬走了就把路由表一起改");
            };
            let end = fn_body_end(&lines, start);
            let body = lines[start..end].join("\n");
            assert!(
                guard_core::contains_word(&body, target),
                "`{file}::{entry}` 不再转发到 `{target}` 了。\n\
                 ★ 它是**用户按得到**的那一层，而真正写盘的是被转发的那一层。\n\
                 换了转发目标就要把路由表改对 —— 否则「按钮 ↔ 真实写点」这条边断了没人知道。"
            );
        }
    }

    /// ★ 正题五：**前提触发器** —— 拿得到远端写能力的文件仍然只有那四个。
    ///
    /// 本表的人群根是「谁拿得到一个 SFTP 会话」。那个根一旦长大，
    /// 上面四条判据的覆盖面就跟着变，而**没有任何东西会提醒**。
    ///
    /// ⚠⚠ **第一版按「源码里出现会话类型名」判，恒绿，被 G 审计逮到**
    /// （`acct_iso_deploy.rs` 靠类型推断，那个名字一次都没写出来，
    /// 而它往用户给的 `dest_dir` 写 8 个文件 + 2 个目录）。
    /// 而本条的注释当时就逐字写着「凡『唯一的哨兵』的判据人群必须按事实取样」——
    /// **写下那句话的判据自己犯了那句话说的错**，本区第三次。
    /// ⇒ 现在按**能力从哪来**取样（`connect_sftp` / `with_sftp` 两个出处），见
    /// [`capability_holders`] 的头注。
    #[test]
    fn the_remote_write_capability_is_still_confined_to_three_files() {
        let holders = capability_holders();
        assert_eq!(
            holders,
            vec![
                "acct_iso_deploy.rs".to_string(),
                "mcp.rs".to_string(),
                "sftp.rs".to_string(),
                "sftp_pool.rs".to_string()
            ],
            "持有 SFTP 会话的文件变了（实得 {holders:?}）。\n\n\
                     ★ **那是本表人群的根** —— 多一个文件就意味着「往别人机器上写」这个能力\n\
                     扩散到了一个本表没在看的地方。请：\n\
                     ① 把新文件加进这里，并让它的写点进 `REMOTE_WRITES`；\n\
                     ② 如果它只**读**（`mcp.rs` 就是这样，它持会话但零写原语），\n\
                        在登记里写明这一点 —— 「持有能力但不用它写」本身是个值得记的事实。\n\
                     ⚠ 少一个也红：那说明某处远端写退役了，登记要跟着删（别留僵尸账）。"
        );
    }
}
