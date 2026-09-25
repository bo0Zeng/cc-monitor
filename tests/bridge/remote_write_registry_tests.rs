use std::path::{Path, PathBuf};

fn repo_root() -> PathBuf {
    // 住址唯一源：`crate::guard_support`（头注写着 24 份副本怎么一起漂的）。
    crate::guard_support::repo_root()
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
             **路径由调用方给**，而调用方全是「代码定路径」那一族（backend 落点 / `.mcp.json` /\
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
        "uninstall_remote_backend",
        "远端",
        "删远端后端二进制。路径**由代码定**（`backend_binary()` 的落点），不接受用户输入。",
    ),
    // 〔RW1 · 第四波 09-24〕这里原来有 `sftp.rs` 的 `remove_remote_file`〔散文墓碑〕一行（F11：删一份远端会话 jsonl，自带一道
    //   方向相反的结构守卫）。F11 改经远端后端删（`files-delete-session`，只收 sid）之后它零调用方、删了 ⇒ 摘行。
    // 〔AL1 · 2026-09-24〕从前这里是 `install_remote_ccm_helper`〔散文墓碑〕一行（它自己逐级 `create_dir`）。
    // 「备份 → 原子写 → 回读 → 回滚」收成 `fenced_block::apply` 一份之后，远端 rc / 入口的
    // 写盘只剩 `SftpFile` 的两个原语 —— 装/卸两个命令一个裸写原语都不再有。
    (
        "sftp.rs",
        "put_atomic",
        "远端",
        "`SftpFile` 的原子替换：逐级 `create_dir` 上级目录（相对远端 home，已存在就忽略）\
             ＋ `upload_atomic`。**路径由代码定**：远端 rc（`.bashrc` 这类，只许 home 下的一个文件名，\
             `remote_profile_name` 拒 `/`、`\\`、`..`）或入口的固定落点。回滚也走它（把原文写回）。",
    ),
    (
        "sftp.rs",
        "delete_created",
        "远端",
        "`SftpFile` 的删除：只在「这份文件原本不存在、这一次新建的、写完读回来不对」时删掉刚建的那份。\
             **路径由代码定**（同上两种落点）。",
    ),
    // ---- sftp_pool.rs：文件面板，路径**由用户选** ⇒ 全部要过 Claude 数据围栏 ----
    // 〔F7c 收尾 09-24〕这一段走了五行：老上传核心〔已删：`upload_inner`〕与四条写命令
    //   〔已删：`sftp_mkdir` · `sftp_rename` · `sftp_delete` · `sftp_chmod`〕（老面板删了、窗口改走后端写面）。
    (
        "sftp_pool.rs",
        "copy_remote_path",
        "远端",
        "★〔步 23b · 09-20〕**零流量复制的核心** —— 写面在退路那一支上：\
             逐块 `read` → `write` 中转（快路那一支一个字节都不经过这里，\
             服务端自己搬，所以它在本表里的写面是**条件性**的）。\
             另有一条 `rename`：先写 `<to>.part`（**EXCLUDE** 创建）再换名上位，同 \
             `upload_inner` 那条纪律。**路径由用户选**（面板里点的源与目标），\
             围栏在入口 `sftp_copy` 的两次 `guard_write`（`from` 与 `to` **各过一次**）。\
             ⚠ 本函数自己**不设围栏** —— 它的语料是一个裸会话 ＋ 两个路径字符串，\
             刻意做成这样好让秤 F3 两个方向都喂得进去（`tests/bridge/sftp_copy_f3_tests.rs`）；\
             围栏在命令入口上，与 `upload_atomic` 那一条同形。",
    ),
    // ---- 〔F7c · 第三波 09-24〕`设计/60 §13`：上传**只写暂存区**，路径**由代码定** ----
    (
        "sftp_pool.rs",
        "upload_to_staging",
        "远端",
        "上传的唯一新形状：本机文件 → `~/.cc-monitor/staging/<key>.part`（相对 SFTP 起始目录）。\
             **路径由代码定**（`staging_part(key)`，`key` 由本机路径 · 大小 · 修改时间派生；\
             函数签名里**没有目标路径**）⇒ 不需要 Claude 数据围栏 —— 它写不到用户目录里去。\
             撤 ⇒ 删自己那份暂存件；失败 ⇒ 留着给续传。落进用户目标的那一下归后端 \
             `files-commit-upload`（先过围栏）。「暂存区之外零写」另有一条行为判据\
             （`sftp_staging_tests::a_staging_upload_writes_nothing_outside_the_staging_area`）。",
    ),
    (
        "sftp_pool.rs",
        "ensure_staging_dir",
        "远端",
        "建暂存区**最后那一段**（`.cc-monitor/staging`）。**路径由代码定**；上一级 \
             `~/.cc-monitor`（后端的家）不在 ⇒ 报错，**不顺手建**（`D11`：后端是给定的）。",
    ),
    (
        "sftp_pool.rs",
        "download_inner",
        "本机",
        "★ **这是超集里唯一的本机项**，也正是「机器分不清 handle 来历」的实例：\
             它和 `upload_inner` 在同一个文件、用同一个方法名 `.write_all(`，\
             但 handle 来自 `tokio::fs::File`（下载落地）而不是 `sftp.create` ⇒ 写的是**本机**。\
             ⇒ 不需要**远端**围栏。\
             🔴〔订正 2026-09-21〕这里原先接着写「它另有一个本机侧的问题（用户选的本机落点\
             由前端对话框给），那属 `write_site_registry` 的管辖面，不在本表」——\
             **那是这条推诿链的第二站**（第一站在 `NON_WRITING_COMMANDS` 的墓碑里，\
             第三站是 `write_site_registry` 里那句「本地缓存 / 不是用户既有环境」，假的）。\
             ⇒ 本机落点现在**真的有围栏**了：`sftp_download` 第一行 `guard_write(&local_path)`，\
             与远端那七条同一个家、同一道判定。本表这一行说的仍然只是「它写本机」这个事实。",
    ),
];

/// 从三个「持有 SFTP 会话」的文件里抠出 `(文件, 函数)` —— 函数体内调了写原语。
fn write_sites() -> Vec<(String, String)> {
    let root = repo_root();
    let prims = write_prims();
    let mut out = Vec::new();
    for f in capability_holders() {
        let raw = std::fs::read_to_string(root.join("src/bridge/src").join(&f))
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
    let root = repo_root().join("src/bridge/src");
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
        "只抠到 {} 处写点（08-10 实测 10；〔步 23b 09-20〕+1 = 11；\
             〔`设计/60 §5.4c` 09-20〕`sftp_chmod` +1 = **12**）—— 抽取器坏了，本条此刻是空转的",
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

/// `sftp_pool.rs` 今天对外开了哪几条 `#[tauri::command]`（已排序）。
///
/// 🔴 **这个数本身就是一条现打订正**，所以它只许有一个住址：
/// `设计/99 §4.6.4` 写的是 14，而那第 14 个匹配住在 `sftp_chmod` 的头注里
/// —— 「错的 grep 与截断的 grep 是同一种失败」。
///
/// 〔步 H2 09-21〕从 [`the_file_window_uses_exactly_the_pool_commands_it_registers`]
/// 的函数体里提出来，**一行逻辑没改**。提出来的唯一理由：
/// [`every_pool_command_is_either_a_registered_write_or_a_registered_read`] 要拿
/// **同一份**人群去做「写 / 读 两分」那条相等断言，而一个闭集只许有一个住址。
fn pool_commands() -> Vec<String> {
    let pool = std::fs::read_to_string(repo_root().join("src/bridge/src/sftp_pool.rs"))
        .expect("sftp_pool.rs 读不到");
    let pool_prod = guard_core::production_code(&pool);
    let attr = format!("#[tauri::{}]", "command");
    let mut commands: Vec<String> = Vec::new();
    let lines: Vec<&str> = pool_prod.lines().collect();
    for (i, l) in lines.iter().enumerate() {
        if l.trim() != attr {
            continue;
        }
        // 属性的下一行就是那条命令的签名（rustfmt 下如此）。
        let Some(sig) = lines.get(i + 1) else {
            continue;
        };
        let Some((_, rest)) = sig.split_once("fn ") else {
            continue;
        };
        if let Some(name) = rest.split(['(', '<']).next() {
            commands.push(name.trim().to_string());
        }
    }
    commands.sort();
    // 抽取器自检：`sftp_pool.rs` 到底有几条对外命令。
    assert_eq!(
        commands.len(),
        // 〔F7c 收尾 09-24〕13 → 1：只剩 `sftp_copy`（门禁 `f3-copy` 那一格还在量它的核心；
        //   其余十二条随老面板与窗口改走通道删了，`设计/60 §13b`）。
        1,
        "`sftp_pool.rs` 现打 {} 条 `#[tauri::command]`（〔F7c 收尾〕现打 **1**；2026-09-21 现打 13；\
         `设计/99 §4.6.4` 写的是 14，而含注释的 grep 数出来正是 14 —— \
         「错的 grep 与截断的 grep 是同一种失败」）。实得：{commands:?}",
        commands.len()
    );
    commands
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
/// `sftp_pool.rs` 头注逐字承诺「**SFTP 写命令拒碰 Claude 数据源文件**
/// （往正被 Claude 打开的 jsonl 写会损坏会话）」。
/// 〔步 H2 09-21〕那句话里的**住址**变了（守卫搬去了 `crate::claude_data_fence`，
/// 池子这边只剩一行 `pub use`），而**承诺一个字没动** —— 本条钉的也一直是那个承诺，
/// 不是那个住址：它判的是「函数体里出现 `guard_write`」这个形态，
/// 而 `guard_write` 现在是一行 `use` 引进来的同一个函数。
///
/// 08-10 实测：那句话今天**是真的** —— `sftp_write_text` / `sftp_upload` / `sftp_mkdir` /
/// `sftp_rename` / `sftp_delete` 五个入口全都调了 `guard_write`
/// （〔步 23b 09-20〕**+ `sftp_copy` = 六个**，同样两个参数各过一次；
/// 〔`设计/60 §5.4c` 09-20〕**+ `sftp_chmod` = 七个**，它只有一个路径参数）。
/// ⚠ 但**在本条之前没有任何判据钉着它**：删掉任一处 `guard_write?`，
/// 全仓判据一条不红，而那台远端机上正被 Claude 打开的 jsonl 就能被面板删掉。
///
/// ⚠ 失效模式如实登记：本条钉的是「函数体里出现 `guard_write`」这个**形态**，
/// 钉不了「围栏用对了路径」（比如过的是 `from` 而写的是 `to`）。
/// `sftp_rename` 那处「两个参数各过一次」是靠**登记说明**写清的，不是机检。
#[test]
fn a_user_chosen_remote_write_passes_the_claude_data_fence() {
    let root = repo_root();
    let raw = std::fs::read_to_string(root.join("src/bridge/src/sftp_pool.rs"))
        .expect("sftp_pool.rs 读不到");
    let prod = guard_core::production_code(&raw);
    // 用户选路径的写入口 = 那六条 `#[tauri::command]` 写命令。
    // ⚠ 人群写死在这里是**刻意的**：「哪些路径是用户选的」没有语法特征，
    //   而这几条是 F47 文件面板的全部写口。第五条判据盯着这个前提别变。
    // 🔴 〔步 23b · 09-20〕**五 → 六**：`sftp_copy`（零流量复制）。它的 `from` 与 `to`
    //   **各过一次** `guard_write`，照 `sftp_rename` 的先例 —— 既不许把 Claude 的会话文件
    //   复制走，也不许复制成一个 Claude 数据源名（往正被 Claude 打开的 jsonl 上盖一份
    //   复制品，与覆写它一样会损坏会话）。
    //   ⚠ **本条只钉「函数体里出现 `guard_write`」这个形态**（头注里那条失效模式），
    //   所以「两个参数各过一次」这件事在 `sftp_copy` 上同样**不是机检**，是登记说明。
    // 🔴 〔`设计/60 §5.4c` · 09-20〕**六 → 七**：`sftp_chmod`（改权限位）。
    //   它只有**一个**路径参数 ⇒ 归本条（单路径那一档），`guard_write` 在函数第一行。
    //   ⚠ **它不进下面那条双路径判据的人群**，而那不是漏掉：那一条的人群逐字是
    //   「签名里有两个路径参数」的写入口，`sftp_chmod` 按构造不满足。
    //   把它塞进去会让那一条去找一个叫 `from`/`to` 的参数、当场 `panic!` ——
    //   那是**误伤**，不是覆盖。两条判据各管各的那一半，这里写死，免得下一个人来「补全」。
    const USER_CHOSEN_ENTRIES: &[&str] = &[
        // 〔F7c 收尾 09-24〕八 → 二。走掉的七条〔已删：`sftp_download` · `sftp_write_text` · `sftp_upload` ·
        //   `sftp_mkdir` · `sftp_rename` · `sftp_delete` · `sftp_chmod`〕随老面板与窗口改走通道删了。
        //   🔴 下载那道**本机落点**围栏没有跟着走：它今天住传输台的开单口 `transfer_call`
        //   （窗口经通道说 `transfer-download`，开单时就过 `guard_write(&local)`），所以那一格换成它。
        "sftp_copy",
        "transfer_call",
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

/// ★ 正题三·下半：**两个路径参数的写入口，两个参数各自过一次围栏。**
///
/// # 🔴 它补的洞是死值验现打出来的，不是想出来的
///
/// 上面那一条的头注逐字登记了自己的失效模式：
///
/// > 本条钉的是「函数体里出现 `guard_write`」这个**形态**，钉不了「围栏用对了路径」
/// > （比如过的是 `from` 而写的是 `to`）。`sftp_rename` 那处「两个参数各过一次」
/// > 是靠**登记说明**写清的，不是机检。
///
/// 〔步 23b 死值验 `M7` 现打〕把 `sftp_copy` 里的 `guard_write(&to)?` **整行删掉**
/// ⇒ 上面那一条 **rc=0、5 passed，一条没红**。后果是具体的：
/// 用户可以把任意文件**复制成** `<远端>/projects/<proj>/<sid>.jsonl`，
/// 盖掉那台机器上**正被 Claude 打开**的会话文件 —— 与覆写它一样会损坏会话，
/// 而那正是 `guard_write` 存在的全部理由。
///
/// ⇒ 那条登记说明从此**有牙**：人群是「签名里有两个路径参数」的那几条写入口，
/// 逐条要求它们的函数体里**两个参数名各自**出现在一次 `guard_write(` 里。
///
/// ⚠ **它仍然钉不了什么**（如实登记，不假装覆盖）：
/// ① 参数名换了（`from`/`to` → `src`/`dst`）要回来改这张表 —— 人群按参数名取样，
///    没有别的可机判特征；改名会让本条**红**（`panic!` 点名），不会静默；
/// ② 它判「那个名字出现在 `guard_write(` 这一行里」，判不了**求值顺序**
///    （先写后判那种写法它看不见）。那一维要的是数据流分析，本仓没有。
#[test]
fn a_two_path_write_entry_fences_both_of_its_paths() {
    let root = repo_root();
    let raw = std::fs::read_to_string(root.join("src/bridge/src/sftp_pool.rs"))
        .expect("sftp_pool.rs 读不到");
    let prod = guard_core::production_code(&raw);
    let lines: Vec<&str> = prod.lines().collect();
    // `(入口名, 那两个路径参数)`。**两条都是「源与目标」那一形**：
    // 既不许把 Claude 的会话文件搬走/复制走，也不许搬成/复制成一个 Claude 数据源名。
    const TWO_PATH_ENTRIES: &[(&str, [&str; 2])] = &[
        // 〔F7c 收尾 09-24〕`sftp_rename`〔散文墓碑〕那一行随它走了（改名今天是后端 `files-rename`）。
        ("sftp_copy", ["from", "to"]),
    ];
    let mut checked = 0usize;
    let mut bad = Vec::new();
    for (entry, params) in TWO_PATH_ENTRIES {
        let Some(start) = lines.iter().position(|l| {
            let t = l.trim_start();
            t.starts_with(&format!("pub async fn {entry}("))
                || t.starts_with(&format!("pub fn {entry}("))
        }) else {
            panic!(
                "找不到写入口 `{entry}` —— 它改名或搬走了。\
                 人群与参数名都写死在本条里（见头注失效模式①），改名就得把这里一起改。"
            );
        };
        checked += 1;
        let end = fn_body_end(&lines, start);
        let body = lines[start..end].join("\n");
        for p in params {
            // 必须是**调用**，不是定义行（同上面那一条踩过的坑）。
            let fenced = body.lines().any(|l| {
                !l.trim_start().starts_with("fn ")
                    && l.contains("guard_write(")
                    && l.contains(&format!("&{p}"))
            });
            if !fenced {
                bad.push(format!("  sftp_pool.rs::{entry} 的 `{p}`"));
            }
        }
    }
    assert_eq!(
        checked,
        TWO_PATH_ENTRIES.len(),
        "只找到 {checked} 个双路径写入口 —— 抽取器坏了"
    );
    assert!(
        bad.is_empty(),
        "这几个路径参数**没有各自**过一次 `guard_write`：\n{}\n\n\
         ★ 「函数体里有 `guard_write`」不等于「每一条路径都过了」。\n\
         少的那一侧的后果是具体的：`to` 没过 ⇒ 能把任意文件改名/复制成\n\
         `<远端>/projects/<proj>/<sid>.jsonl`，盖掉那台机器上**正被 Claude 打开**的会话；\n\
         `from` 没过 ⇒ 能把正在用的会话文件从 Claude 底下搬走。\n\
         ⚠ 这一条是死值验 `M7` 逼出来的：在它之前，删掉 `guard_write(&to)?` **全仓一条不红**。",
        bad.join("\n")
    );
}

/// ★ 正题四：**接线层** —— 对外 IPC 入口必须真的转发到一个已登记的原语点。
///
/// # 为什么单列
///
/// 〔audit-0805 V7-2〕当初点名的十个名字里有五个是**IPC 包装**
/// （`sftp_write_text` / `sftp_upload` / `deploy_remote_backend` /
/// `write_remote_mcp_server` / `delete_remote_history_session`）——
/// 它们自己**不调写原语**，所以按能力边界派生的人群里**一个都没有**。
///
/// ⚠ 〔步 12·C 收尾 09-20〕上面那句话是 audit-0805 当时的**现打**，留着不改；
/// 但今天它里面的 `write_remote_mcp_server` 与 `delete_remote_history_session`
/// **已经不是 IPC 命令**了（`origin` 归一把它们并进了本机同族那条）。
/// 它们仍留在下面那张路由表里，而且**必须留** —— 本表这一条判的是
/// 「**按得到的那一层 ↔ 真正写盘的那一层**」这条边，而合并之后那条边是
/// `write_project_mcp_server`（命令）→ `write_remote_mcp_server`（远端分支）
/// → `upload_atomic`（写点）。本表是**一跳**的（理由见 `sftp_copy` 那一条），
/// 所以它钉的是后半跳；前半跳由 `origin_tests::every_origin_taking_command_splits_local_through_route`
/// 钉着（吃 `Origin` 的命令 ↔ 走 `route` 的命令，两向集合相等）。
///
/// ⇒ 两份名单只重合五个。那不是谁错了，是**两个不同的层**：
/// 「谁按下按钮」与「谁真的写」。本表的人群是后者，而前者是外面看得见的那一层
/// ⇒ 必须有一条边把它们连起来，否则改了转发目标没人会红。
#[test]
fn the_ipc_entry_points_route_through_a_registered_write_site() {
    let root = repo_root();
    // `(入口所在文件, 入口名, 它该转发到的已登记写点)`
    const ROUTES: &[(&str, &str, &str)] = &[
        // 〔F7c 收尾 09-24〕池子那两条 IPC 写入口〔已删：`sftp_write_text` · `sftp_upload`〕删了；
        //   上传今天的入口是传输台的开单口（窗口经通道说 `transfer-upload`），它转发到只写暂存区的那一个写点。
        ("sftp_pool.rs", "transfer_call", "upload_to_staging"),
        ("sftp.rs", "deploy_remote_backend", "upload_atomic"),
        // 〔RW1 · 第四波 09-24〕`mcp.rs` 远端写那一行走了：F89a 按用户裁「按推荐改」改经远端后端写，
        //   那个分支函数（`write_remote_mcp_server`）今天一个 SFTP 会话都不拿 ⇒ 不再是本表的人群。
        // 〔RW1 · 第四波 09-24〕`remote_history.rs::delete_remote_history_session` 那一行走了：它今天经远端后端删
        //   （`files-delete-session`），一个 SFTP 会话都不拿 ⇒ 不再是本表的人群。
        // ★〔G 审计补的两条〕它们都持会话 / 往用户给的路径写远端，却因为
        // 「自己不调裸写原语」而进不了按能力边界派生的人群 ——
        // **正是本条（接线层）存在的理由**：两个层，一条边。
        (
            "acct_iso_deploy.rs",
            "deploy_remote_acct_iso",
            "ensure_dir_all",
        ),
        // 〔RW1 · 第四波 09-24〕装 / 卸远端 rc 两条命令（F10）从这里走了：它们今天经那台远端的**后端**写
        //   （`user_files::BackendDoor` → `files-peek` / `files-put`），一个 SFTP 会话都不拿 ⇒ 不再是本表的人群。
        //   `SftpFile` 只剩 F08 部署那一个用户（`put_ccm_entry`，入口 `deploy_remote_backend` 那一行上面已经在）。
        // ★〔步 23b · 09-20〕零流量复制。**为了这条边，`sftp_copy` 刻意没抽 `copy_inner`** ——
        // 本表是**一跳**的，中间垫一层，「按钮 ↔ 真实写点」这条边就表达不出来；
        // 理由逐字写在 `sftp_pool.rs::sftp_copy` 的头注上。
        ("sftp_pool.rs", "sftp_copy", "copy_remote_path"),
    ];
    // 〔AL1〕**落点类型**：一个 `fenced_block::Store` 的远端实现，它的写原语方法全在 `REMOTE_WRITES` 里。
    // 入口「造了它」＝ 入口把写交给了它（序列 `fenced_block::apply` 不认识任何落点）。
    const STORES: &[(&str, &[&str])] = &[("SftpFile", &["put_atomic", "delete_created"])];
    for (ty, methods) in STORES {
        for m in *methods {
            assert!(
                REMOTE_WRITES
                    .iter()
                    .any(|(f, n, ..)| *f == "sftp.rs" && n == m),
                "落点类型 `{ty}` 的写原语 `{m}` 不在 `REMOTE_WRITES` 里 —— 这条边指向账外"
            );
        }
    }
    for (file, entry, target) in ROUTES {
        // 转发目标必须是本表登记过的写点（或登记过的落点类型）—— 否则这条边指向账外。
        assert!(
            REMOTE_WRITES.iter().any(|(_, n, ..)| n == target)
                || STORES.iter().any(|(t, _)| t == target),
            "路由表说 `{entry}` 转发到 `{target}`，可 `{target}` 不在 `REMOTE_WRITES` 里 —— \
                 那这条边指向账外，等于没连"
        );
        let raw = std::fs::read_to_string(root.join("src/bridge/src").join(file))
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

/// ★ 正题六：🔴 **原生文件窗口用了池子哪几条 —— 逐条登记，两向相等。**
///
/// # 它补的洞
///
/// 前五条判据管的是「**池子**那一侧每一处写盘有没有申报、有没有围栏」。
/// 它们**一条都不管「谁按得下去」**：`filewin/` 那棵树把哪几条命令接到了界面上，
/// 在本条之前**零判据**。后果有两个方向，都出现过：
///
/// - **变多**：往窗口上接一条新的写命令而不申报 ⇒ 没人回来读一遍
///   「它的路径由谁定 · 什么围栏着」（第三条判据只看池子那一侧的入口，
///   看不见窗口新长出来的按钮）。
/// - 🔴 **变少**：某条命令的接线被顺手删掉 ⇒ 用户当场少一件事，而**一条判据都不会红**。
///   `设计/99 §4.6.5` 逐字记着这一形的代价：老面板先退役就会「用户这六件事当场没了」。
///
/// ⇒ 本条是一条**相等**断言（两向差集空 ＋ 处数相等）。**地板不行** ——
/// 地板在「变少」那个方向上是瞎的，而那正是贵的那个方向。
///
/// # 🔴 现打订正了 `设计/99 §4.6.4` 那张表两处（2026-09-21，接线前现打）
///
/// 那一节写着「池子 **14** 条命令，窗口只接了 **6** 条」，接上了那一栏点名
/// `sftp_list_dir` · `sftp_stat` · **`sftp_realpath`** · `sftp_upload` · `sftp_copy` ·
/// **`copy_remote_path`**。逐条现打：
///
/// | 那张表 | 现打 | 差在哪 |
/// |---|---|---|
/// | 池子 14 条 | **13** 条 | 第 14 个匹配是 `sftp_chmod` 头注里提到 `#[tauri::command]` 那句**散文** |
/// | 窗口接了 6 条 | **4** 条 | `sftp_realpath` 在 `filewin/` 里**零处**（只有老面板 `panel.ts` 在用）；`copy_remote_path` **不是命令**，而且 `filewin/` 被判据明禁调它（理由住 `copy.rs` 头注：它的第一个参数是一条裸会话，绕过 `sftp_copy` 就同时丢掉围栏 ＋ 取消登记 ＋ 车道预算） |
///
/// ⇒ 那张表的「6 → 11」这个算式两侧都错。**真实读数是 4 → 9**（第七刀之后 **10**），
/// 而本条就是那个数的住址。
/// 🔴 〔第七刀 09-21 补记〕上面那张表里「`sftp_realpath` 在 `filewin/` 里**零处**」
/// 这句**已经不是实况了** —— 那一格正是第七刀补上的（落点是 `source.rs` 里一个问 home 的 async 函数；
/// 〔F7a · 第三波 09-24〕那一问又换成了后端 `files-home`，`sftp_realpath` 从窗口那棵树上又没了，见下表）。
/// 那句话留着是因为它说清了**当时为什么是零**（窗口寄生在老面板的寻址上），
/// 而它今天的真伪由本条的相等断言替它保鲜。
/// ⚠ `设计/` 是唯一真相源、不是 git 仓 ⇒ 这条订正只落在本判据的诊断里，
/// 由本条的相等断言替它保鲜。
#[test]
fn the_file_window_uses_exactly_the_pool_commands_it_registers() {
    /// `(池子里的名字, 它是什么, 窗口为什么要它)`。
    ///
    /// ⚠ 「它是什么」那一栏**不是人的答案** —— 下面按 `sftp_pool.rs` 里
    /// 有没有那个属性逐条核（写错了当场红）。
    // 〔F2 · 2026-09-24 · 文件窗口只经通道说 `call`〕走掉 8 行：`sftp_list_dir` · `SftpEntry`（列目录改问后端
    //   `files-ls`）· `sftp_stat`（「那儿有没有东西」改问 `files-stat`）· `sftp_mkdir` / `sftp_delete` /
    //   `sftp_rename` / `sftp_chmod` / `sftp_write_text`（改走后端写面那五条）。留下的 10 行里 9 行是
    //   窗口进程那一侧**还不是通道**的欠账（跨机传输 §8.4 未拍 · 后端缺 `files-copy` · 本地预判围栏），
    //   逐条住 `tests/bridge/filewin/boundary_tests.rs::WINDOW_SIDE`；〔F7a 09-24〕`sftp_realpath` 那一行已搬去后端 `files-home`（见 `MOVED_TO_BACKEND`）。
    // 〔F7c 收尾 09-24〕走掉三行〔已删：`sftp_download` · `TRANSFER_LANE_CAP` · `sftp_cancel_transfer`〔散文墓碑〕〕：
    //   上传 / 下载经通道开单、订阅进度（`设计/60 §13`），复制那一腿的取消随复制走后端一起删了。
    //   ⇒ 窗口接的池子**命令**从此是 0；剩下这一行是那道围栏判定的旧住址（一条函数，不是命令）。
    const SITES: &[(&str, &str, &str)] = &[(
        "is_protected_claude_data_path",
        "函数",
        "🔴 **那道围栏的判定本身。** 窗口拿它在发往返**之前**就把踩线的挑出去并出声。\
             ⚠ 〔步 H2 09-21〕`设计/99 §2 Q2` **用户拍了「拆」** ⇒ 判定的家已经搬去\
             `crate::claude_data_fence`，池子这边只剩一行 `pub use`（转出住址，不是第二个家）。\
             窗口那棵树本轮不在写区里 ⇒ 它这一处仍写着旧写法，而那是**唯一**还这么写的一处，\
             由 `claude_data_fence_tests::the_old_address_is_down_to_its_last_consumer` \
             钉成相等断言（改过来的那天连那行 `pub use` 一起删）。\
             ⚠ 两道围栏问的仍是这**一个**函数 ⇒ 判定不会漂；漂得动的只有文案。\
             ★ 本条的针是 `sftp_pool::` 前缀 ⇒ 它数的正是「还在走旧住址的那一处」，\
             与那条棘轮同一个事实、两个方向",
    )];
    /// 「它是什么」那一栏的**封闭集合**。多出第四种就得回来论证。
    /// 〔F7a · 第三波 09-24〕原来还有「类型」一档（唯一一条是复制那一趟的裁决类型），
    /// 复制换到后端之后那一条走了 ⇒ 这一档没人用，删掉（下面那条「每一档都有人用」逐字要求）。
    // 〔F7c 收尾 09-24〕「命令」「常量」两档今天都没人用了（窗口接的池子命令是 0）⇒ 只剩「函数」。
    const KINDS: &[&str] = &["函数"];

    let root = repo_root();
    let dir = root.join("src/bridge/src/filewin");
    // 🔴 走 `guard_core` 而不是裸 `read_dir`（`scanning_guard_registry` 那条纪律：
    //    扫描型判据不许自己遍历）。本条不需要摘掉自己 —— 调用者住
    //    `tests/bridge/`，压根不在被扫的那棵树里。
    let files = guard_core::scan_tree!(&dir, &["rs"]);
    // 抽取器自检①：**采集量**。射程被改窄在本条上是静默的（少扫一份 ⇒ 那一份
    // 接的命令从 `used` 里消失 ⇒ 差集非空 ⇒ 会红；但少扫**全部**就两边都空了）。
    assert_eq!(
        files.len(),
        // 〔F7c · 合主线 09-24〕主线 19 → 20，多的是 `upload.rs`（工具栏「上传」那一问；一个池子符号都不碰）。
        20,
        "`filewin/` 那棵树现扫到 {} 份 `.rs`（2026-09-22 现打 14：copy · corpus · **download** · **editor** · entry · \
         find · fonts · mod · rows · scale · shell · source · transfer · writeops）\
         〔第十三刀 09-23：14 → 16，多的是 **proc** 与 **win_main**（窗口改独立进程：\
          一个窗口一个进程）。⚠ `win_main.rs` **不是一个模块** —— 它是那个 `[[bin]]` 的 \
          crate 根，`mod.rs` 刻意不 `mod` 它；它照旧进本条的射程，因为本条问的是\
          「`filewin/` 这棵树上谁在碰池子那几条命令」，而那个问题对一份 bin 入口一样要问〕\
         〔F7b 09-24：16 → 18，多的是 **create** 与 **bigfile**（「新建空文件」；它走后端 `files-create`，\
          一条池子命令都不碰 ⇒ 下面 `used` 那一摞不因它变）〕—— \
         〔F9 09-24（与 F7b 同拍合并，现打 18）多的是 **bigfile**（大文件模式；它一条池子命令都不碰 ——\
          读上限仍经 `editor.rs` 那一处 `MAX_EDIT_BYTES`）〕—— \
         〔FW1+FW2 09-24：18 → 19（与 F7b / F9 同拍合并，现打 19），多的是 **select**（选中态 · 键位 · \
          右键菜单那张表；它一条池命令都不碰，写操作经 `shell.rs` 那几个 `begin_*` 走）〕—— \
         不等就是射程变了，先查扫描面再改这个数",
        files.len()
    );
    let mut used: Vec<String> = Vec::new();
    let mut where_of: Vec<(String, String)> = Vec::new();
    // 拼出来的针，免得命中本文件自己的说明（同 `capability_holders` 那条手法）。
    let needle = format!("sftp_{}::", "pool");
    for (path, src) in &files {
        let prod = guard_core::production_code(src);
        let rel = path
            .file_name()
            .expect("扫到的每一项都是文件")
            .to_string_lossy()
            .to_string();
        let mut from = 0usize;
        while let Some(k) = prod[from..].find(needle.as_str()) {
            let at = from + k + needle.len();
            let end = prod[at..]
                .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
                .map_or(prod.len(), |d| at + d);
            let name = prod[at..end].to_string();
            if !name.is_empty() {
                if !used.contains(&name) {
                    used.push(name.clone());
                }
                where_of.push((name, rel.clone()));
            }
            from = at;
        }
    }
    used.sort();
    // 抽取器自检②：**处数地板**。剥法把生产段剥没了 ⇒ 两边都空 ⇒ 相等断言恒真。
    // 〔F7a · 第三波 09-24〕地板改成相等：10 → 9（少了 `sftp_realpath`：开窗前解 home 换成后端 `files-home`）
    //   → 7（少了读文本那条命令与它的上限常量：换成后端 `files-read-text`，常量搬回 `editor.rs`）
    //   → 5（少了复制那条命令与它的裁决类型：换成后端 `files-copy`）。
    assert_eq!(
        used.len(),
        // 〔F7c · 合主线 ＋ 收尾 09-24〕5 → 1：`sftp_download` · `sftp_upload` · `TRANSFER_LANE_CAP`（经通道）·
        //   `sftp_cancel_transfer`〔散文墓碑〕（复制那一腿的取消，随复制走后端删了）走掉；剩那道围栏判定的旧住址。
        1,
        "抠到 {} 处 `sftp_pool::…` 引用 —— 与现打的条数不等：抽取器坏了，或接线变了（实得 {used:?}）",
        used.len()
    );

    let mut declared: Vec<String> = SITES.iter().map(|(n, ..)| n.to_string()).collect();
    declared.sort();
    // 🔴 **相等断言，两向**（处数相等 ＋ 双向差集空，由 `assert_eq!` 一并给出）。
    assert_eq!(
        declared,
        used,
        "「窗口那棵树用到的池子那几样」与登记表对不上。\n  \
         登记了而树上没有（接线被删了？那用户当场少一件事）：{:?}\n  \
         树上有而没登记（新接了一条却没人回来读它的围栏与路径归属）：{:?}\n\n\
         ★ 这一条**不是地板**：在「变少」那个方向上地板是瞎的，\n\
         而 `设计/99 §4.6.5` 逐字记着那个方向的代价。",
        declared
            .iter()
            .filter(|d| !used.contains(d))
            .collect::<Vec<_>>(),
        used.iter()
            .filter(|u| !declared.contains(u))
            .collect::<Vec<_>>()
    );

    // 说明不许是占位，落地分类在封闭集合里。
    for (name, kind, why) in SITES {
        assert!(
            KINDS.contains(kind),
            "`{name}` 的分类写的是「{kind}」，不在 {KINDS:?} 里"
        );
        assert!(
            why.chars().count() > 20,
            "`{name}` 的说明太短，像是占位：「{why}」"
        );
    }
    for k in KINDS {
        assert!(
            SITES.iter().any(|(_, x, _)| x == k),
            "分类集合里的「{k}」今天一处都没人用 —— 删掉它"
        );
    }

    // 🔴 「它是什么」那一栏**由机器核**，不是人说了算。住址唯一源：[`pool_commands`]。
    let commands = pool_commands();
    let declared_cmds: Vec<String> = SITES
        .iter()
        .filter(|(_, k, _)| *k == "命令")
        .map(|(n, ..)| n.to_string())
        .collect();
    for n in &declared_cmds {
        assert!(
            commands.contains(n),
            "登记表说 `{n}` 是一条命令，可 `sftp_pool.rs` 里没有这么一条 \
             `#[tauri::command]` —— 那这一栏是假的"
        );
    }
    for (n, k, _) in SITES.iter().filter(|(_, k, _)| *k != "命令") {
        assert!(
            !commands.contains(&n.to_string()),
            "登记表说 `{n}` 是「{k}」，可它其实是一条 `#[tauri::command]`"
        );
    }
    // 🔴〔F2 · 2026-09-24〕**从「13 条全接上」变成「6 条 ＋ 7 条搬去后端」**。
    //   窗口成了独立进程、只经通道说 `call` 之后，池子那 13 条里有 7 条在后端有了对应命令，
    //   窗口不再碰它们（`D11`：没有退路）。剩下 6 条是窗口进程那一侧**还不是通道**的欠账。
    //   ⇒ 两件事各一条**两向相等**：窗口还用的 == 6 条；搬走的 == 下表 7 条（逐条写清搬去了哪）。
    // 〔F7c 收尾 09-24〕这张表的人群是「池子里**还在**、而窗口不再用」的命令 ⇒ 池子收到只剩 `sftp_copy` 之后
    //   它只剩一行。搬走过的十二条〔已删：`sftp_list_dir`→`files-ls` · `sftp_stat`→`files-stat` ·
    //   `sftp_mkdir` · `sftp_delete` · `sftp_rename` · `sftp_chmod` · `sftp_write_text`→写面 ·
    //   `sftp_realpath`→`files-home` · `sftp_read_text_for_edit`〔散文墓碑〕→`files-read-text`（F7a）·
    //   `sftp_upload` / `sftp_download`→通道后面的传输台 · `sftp_cancel_transfer`〔散文墓碑〕→停订（F7c）〕连本体一起删了。
    const MOVED_TO_BACKEND: &[(&str, &str)] = &[
        // 〔F7a · 第三波 09-24〕同机复制（此前走池子那条零流量复制）。池子那条命令还在，只剩门禁 `f3-copy` 在量它。
        ("sftp_copy", "files-copy"),
    ];
    assert_eq!(
        declared_cmds.len(),
        // 〔F7c 收尾 09-24〕3 → 0：窗口一条池子命令都不接了（上传 · 往外拖经通道；取消随复制走后端）。
        0,
        "窗口今天接了 {} 条池子命令（〔F2 09-24〕13 → 6：上传 · 往外拖 · 取消 · 读文本进编辑器 ·\
         同机复制 · 开窗前解 home；〔F7a 09-24〕6 → 5：开窗前解 home 换成后端 `files-home`；\
         5 → 4：读文本进编辑器换成后端 `files-read-text`；4 → 3：同机复制换成后端 `files-copy`）",
        declared_cmds.len()
    );
    let missing: std::collections::BTreeSet<&str> = commands
        .iter()
        .filter(|c| !declared_cmds.contains(c))
        .map(String::as_str)
        .collect();
    let moved: std::collections::BTreeSet<&str> =
        MOVED_TO_BACKEND.iter().map(|(c, _)| *c).collect();
    assert_eq!(
        missing, moved,
        "窗口不再用的池子命令 ≠ 「搬去后端的那几条」（F2 7 条 ＋ F7a 逐条加的）。多出来的 ＝ 一条接线掉了而没有后端对应；\
         少了的 ＝ 窗口又回头用池子了（`D11` 不许）"
    );
    // 反空真：`copy_remote_path` 真的**不在**窗口那棵树上。
    // 🔴 〔第七刀 09-21〕这一圈原先还含 `sftp_realpath` —— 它现在**真的接上了**
    //（落点当时是 `source.rs` 里一个问 home 的 async 函数）⇒ 从这一圈里拿掉，并进了上面 `SITES`。
    //    〔F7a · 第三波 09-24〕那一问又换成了后端 `files-home` ⇒ 它从 `SITES` 出来、进了 `MOVED_TO_BACKEND`。
    //    ⚠ 那不是把一条判据放松了：它换了方向 —— 从「钉住它别悄悄回来」变成
    //      「钉住它恰好一处、且排在列目录前面」（住 `entry_tests` 那条）。
    //    `copy_remote_path` 留着，理由没变（`filewin/` 被判据明禁调它）。
    for gone in ["copy_remote_path"] {
        assert!(
            !used.iter().any(|u| u == gone),
            "`{gone}` 出现在窗口那棵树上了 —— 它是上面那条现打订正点名的两处之一，\
             要真接上就把登记与那条订正一起改"
        );
    }
    // 🔴 把「谁在哪儿用」印出来（`--nocapture` 下可见）——
    //    本条对「采到了它而它过了」与「压根没扫到」原本输出相同，那正是静默的绿。
    where_of.sort();
    where_of.dedup();
    println!("窗口 ↔ 池子的接线（{} 处）：{where_of:?}", where_of.len());
}

// ════════════════════════════════════════════════════════════════════════════
//  〔步 H2 · 2026-09-21〕围栏拆成独立一族那一刀留下的两条
// ════════════════════════════════════════════════════════════════════════════

/// 池子里**不写任何路径**的那几条命令 —— `(命令, 它做什么, 为什么不需要围栏)`。
///
/// 🔴 **它是「默认拒绝」的另一半。** 上面第三条判据的人群
/// （`USER_CHOSEN_ENTRIES`，七条）是**写死**的 —— 那是刻意的（「哪些路径是用户选的」
/// 没有语法特征），但它单独存在时有一个洞：
/// **往 `sftp_pool.rs` 加第 14 条命令、而它是一条写命令，那七个名字里没有它 ⇒ 一条不红。**
///
/// ⇒ 本表把那个洞封上：命令的全集由 [`pool_commands`] 从文件里现打，
/// 而全集必须**恰好**分成「已登记的写」与「已登记的读」两堆，一条都不许落在外面。
const NON_WRITING_COMMANDS: &[(&str, &str, &str)] = &[
    // 〔F7c 收尾 09-24〕这一堆五行全走了〔已删：`sftp_realpath` · `sftp_list_dir` · `sftp_stat` ·
    //   `sftp_read_text_for_edit` · `sftp_cancel_transfer`〔散文墓碑〕〕（读侧今天是后端 `files-*`；取消是停订）。
    //   表留着、今天是空的：它是「默认拒绝」的另一半 —— 池子里哪天再长出一条读命令，它得来这里写清理由。
];

/// 🔴 **池子里每一条命令，要么是已登记的写、要么是已登记的读 —— 相等断言，两向。**
///
/// # 它补的洞：「现打有几处写操作」此前是**写死**的
///
/// 第三条判据（`a_user_chosen_remote_write_passes_the_claude_data_fence`）的人群是
/// 七个写死的名字。它钉得住「这七条各自有围栏」，钉不住「今天恰好就是这七条」。
/// ⇒ 加第 14 条命令而它是写命令 ⇒ 那七个名字里没有它 ⇒ **全仓一条不红**，
/// 而它写的是用户选的远端路径。
///
/// 本条把两个数对起来：
///
/// ```text
/// 从文件现打的命令全集（13）  ==  带围栏的写（7）  +  已登记的读（6）
/// ```
///
/// 三个数**都不是人手填的**：全集来自 [`pool_commands`]，
/// 「写」那一堆从函数体里现打（带 `guard_write(` 调用的那几条），
/// 「读」那一堆来自 [`NON_WRITING_COMMANDS`]（每一行要写清**为什么不需要围栏**）。
///
/// ⚠ 它钉不了什么（如实登记）：**一条新命令被归进「读」那一堆而其实它写**。
/// 那一栏是人的答案（同 `REMOTE_WRITES` 的「落地何方」）—— 挡这一形的是
/// 第一条判据（写原语调用点默认拒绝）与第四条判据（IPC 入口必须转发到已登记的写点）。
/// 本条挡的是**落在两堆之外**那一形，那是「加了一条命令而没人回来读它的围栏」的痕迹。
#[test]
fn every_pool_command_is_either_a_registered_write_or_a_registered_read() {
    let pool = std::fs::read_to_string(repo_root().join("src/bridge/src/sftp_pool.rs"))
        .expect("sftp_pool.rs 读不到");
    let prod = guard_core::production_code(&pool);
    let lines: Vec<&str> = prod.lines().collect();
    let all = pool_commands();
    // 「写」那一堆**从函数体里现打**，不抄第三条判据那七个名字 ——
    // 抄一份的话，「人群少一个」与「登记少一行」会被同一次编辑一起改掉 ⇒ 恒真。
    let mut fenced: Vec<String> = Vec::new();
    for cmd in &all {
        let Some(start) = lines.iter().position(|l| {
            let t = l.trim_start();
            t.starts_with(&format!("pub async fn {cmd}("))
                || t.starts_with(&format!("pub fn {cmd}("))
        }) else {
            panic!("命令 `{cmd}` 的签名找不到 —— 抽取器坏了");
        };
        let end = fn_body_end(&lines, start);
        let body = lines[start..end].join("\n");
        if body
            .lines()
            .any(|l| l.contains("guard_write(") && !l.trim_start().starts_with("fn "))
        {
            fenced.push(cmd.clone());
        }
    }
    let mut readers: Vec<String> = NON_WRITING_COMMANDS
        .iter()
        .map(|(n, ..)| n.to_string())
        .collect();
    readers.sort();
    fenced.sort();

    // 🔴 两个数相等 —— 全集 = 写 + 读，一条都不许落在两堆之外。
    let mut covered: Vec<String> = fenced.iter().chain(readers.iter()).cloned().collect();
    covered.sort();
    assert_eq!(
        covered,
        all,
        "`sftp_pool.rs` 的命令全集与「写 / 读」两堆对不上。\n  \
         全集里没归档的（🔴 **最贵的就是这一格**）：{:?}\n  \
         归档了而全集里没有（改名 / 退役了 ⇒ 把登记一起改）：{:?}\n\n\
         ★ 落在两堆之外意味着：它要么是一条**没人回来读它围栏**的写命令\n\
         （后果：那台远端机上正被 Claude 打开的 jsonl 能被它删掉 / 改走 / 改成不可读），\n\
         要么是一条读命令而没人写下「为什么它不需要围栏」。\n\
         ⇒ 处置：写命令 ⇒ 加进 `a_user_chosen_remote_write_passes_the_claude_data_fence`\n\
         的 `USER_CHOSEN_ENTRIES` 并在函数第一行加 `guard_write`（两个路径参数的加两次）；\n\
         读命令 ⇒ 加进 `NON_WRITING_COMMANDS` 并写清理由。",
        all.iter()
            .filter(|c| !covered.contains(c))
            .collect::<Vec<_>>(),
        covered
            .iter()
            .filter(|c| !all.contains(c))
            .collect::<Vec<_>>()
    );
    // 两堆不许重叠（一条命令不能既是写又是读 —— 那说明有一栏是假的）。
    let both: Vec<&String> = fenced.iter().filter(|f| readers.contains(f)).collect();
    assert!(
        both.is_empty(),
        "这几条命令同时出现在两堆里：{both:?} —— \
         `NON_WRITING_COMMANDS` 说它不写，可它函数体里有 `guard_write`。哪一栏是假的？"
    );
    // 现打读数：两个数各自钉住，别只钉和。
    assert_eq!(
        (fenced.len(), readers.len()),
        // 〔F7c 收尾 09-24〕(8, 5) → (1, 0)：池子只剩 `sftp_copy` 一条带围栏的写命令，读命令一条都不剩。
        (1, 0),
        "现打：写 {} 条 · 读 {} 条（2026-09-21 现打 **8 ＋ 5 = 13**；\
         当日早先是 7 ＋ 6 —— `sftp_download` 从「读」那一堆搬到了「写」那一堆，\
         它一直在写盘，只是写的是**用户选的本机路径**而没人守，逐条来历见\
         `NON_WRITING_COMMANDS` 上方那段墓碑）。\
         实得写 {fenced:?} · 读 {readers:?}",
        fenced.len(),
        readers.len()
    );
    for (n, what, why) in NON_WRITING_COMMANDS {
        assert!(
            why.chars().count() > 10,
            "`{n}`（{what}）没写清为什么不需要围栏，像是占位：「{why}」"
        );
    }
}

/// 🔴 **围栏在「发往返之前」就出声 —— 这一条判的是顺序，不是存在。**
///
/// # 它补的洞
///
/// 第三条判据的头注逐字登记着它自己的失效模式，其中一条是：
///
/// > 它判「那个名字出现在 `guard_write(` 这一行里」，判不了**求值顺序**
/// > （先写后判那种写法它看不见）。那一维要的是数据流分析，本仓没有。
///
/// 完整的数据流分析本仓确实没有，而**这一刀要的那一维不需要它**：
/// 围栏要的是「拒绝发生在**拿到连接之前**」，而「拿连接」在这个文件里只有两个出处
/// （`pool_for` 自己拨/取那条 SSH 连接 · `with_sftp` 从池里借）——
/// 那是 `sftp_pool.rs` 头注写着的结构事实，不是一张按名字列的白名单。
/// ⇒ 逐条比**行号**：`guard_write` 第一次出现必须早于它俩第一次出现。
///
/// # 为什么这一维值钱
///
/// 把 `guard_write(&path)?` 挪到拿连接那一行之后，第三条判据**照样绿**
/// （那一行还在函数体里），而行为变了：连接拨出去了、通道借走了、
/// 甚至 `metadata` 那一问已经上过线了，踩线的那条路径才被拒。
/// `src/doc/INVARIANTS.md` `§1` 的 F47 澄清段逐字要的是「绝无自动/后台写」——
/// 一条已经上了线的请求，事后再说「我拒绝」不是同一件事。
///
/// ⚠ 它钉不了什么：① 同一行里的求值顺序；
/// ② 围栏与拿连接**之间**新插进来的别的网络动作（本条只比这两个锚点的先后）。
/// ⇒ 两条都是「行号锚点」这个量具的上限，不是漏掉。
#[test]
fn a_fenced_write_refuses_before_it_touches_the_wire() {
    let pool = std::fs::read_to_string(repo_root().join("src/bridge/src/sftp_pool.rs"))
        .expect("sftp_pool.rs 读不到");
    let prod = guard_core::production_code(&pool);
    let lines: Vec<&str> = prod.lines().collect();
    // 「拿连接」的两个出处（运行时拼，免得命中本文件自己的说明）。
    let wire: Vec<String> = [("with_", "sftp("), ("pool_", "for(")]
        .iter()
        .map(|(a, b)| format!("{a}{b}"))
        .collect();
    let mut checked = 0usize;
    let mut late = Vec::new();
    let mut unreached = Vec::new();
    for cmd in pool_commands() {
        let Some(start) = lines.iter().position(|l| {
            let t = l.trim_start();
            t.starts_with(&format!("pub async fn {cmd}("))
                || t.starts_with(&format!("pub fn {cmd}("))
        }) else {
            panic!("命令 `{cmd}` 的签名找不到 —— 抽取器坏了");
        };
        let end = fn_body_end(&lines, start);
        let body: Vec<&str> = lines[start..end].to_vec();
        let at_guard = body
            .iter()
            .position(|l| l.contains("guard_write(") && !l.trim_start().starts_with("fn "));
        let at_wire = body
            .iter()
            .position(|l| wire.iter().any(|w| l.contains(w.as_str())));
        let Some(g) = at_guard else {
            continue; // 不带围栏的是读命令，归上面那条两分判据
        };
        checked += 1;
        match at_wire {
            None => unreached.push(format!("  sftp_pool.rs::{cmd}")),
            Some(w) if g >= w => late.push(format!(
                "  sftp_pool.rs::{cmd} —— 围栏在函数体第 {} 行，拿连接在第 {} 行",
                g + 1,
                w + 1
            )),
            Some(_) => {}
        }
    }
    // 抽取器自检：人群必须恰好是那七条带围栏的写命令（与两分那条判据同一个数）。
    assert_eq!(
        // 〔F7c 收尾 09-24〕8 → 1：只剩 `sftp_copy`。
        checked,
        1,
        "只找到 {checked} 条带围栏的命令（2026-09-21 现打 **8**：远端那七条 ＋ `sftp_download`\n\
         那一条**本机**落点〔当日补，来历见它的函数注释：三张账首尾相接推诿，末端一句假话〕）\n\
         —— 抽取器坏了，本条此刻在空转"
    );
    // 反空真：「拿连接」那个锚点必须真的在每一条里命中，否则 `g < w` 恒真地过。
    assert!(
        unreached.is_empty(),
        "这几条写命令的函数体里**找不到「拿连接」那个锚点**：\n{}\n\n\
         ★ 那不是「它不上网」，更可能是本条的锚点过期了 —— \n\
         池子换了拿连接的写法之后，本条会对**每一条**都恒真地绿。\n\
         ⇒ 先读 `sftp_pool.rs` 头注那一节（连接分离 + 通道预算），再改锚点。",
        unreached.join("\n")
    );
    assert!(
        late.is_empty(),
        "这几条写命令**先拨了线才判围栏**：\n{}\n\n\
         ★ 第三条判据对这一形是**瞎的**（`guard_write` 那一行还在函数体里，它照样绿）。\n\
         而行为变了：连接拨出去了、通道借走了、甚至已经问过一次 `metadata`，\n\
         踩线的那条路径才被拒。\n\
         ⚠ `src/doc/INVARIANTS.md` `§1` 的 F47 澄清段逐字要的是「绝无自动/后台写」——\n\
         一条已经上了线的请求，事后再说「我拒绝」不是同一件事。\n\
         ⇒ 处置：把 `guard_write` 放回函数第一行（两个路径参数的两行都放前面）。",
        late.join("\n")
    );
}

// ══════════════════════════════════════════════════════════════════════════
// 🔴 〔RW1 · 第四波 · 2026-09-24〕**monitor 进程不经 SFTP 直写用户文件** —— 远端那一半的分类闭集
// ══════════════════════════════════════════════════════════════════════════
//
// 用户裁远端三处（F10 别名块 · F11 删会话 · F89a `.mcp.json`）「按推荐改」经远端后端写，F08 部署后端留在 SFTP。
// ⇒ 本表剩下的每一处 SFTP 写，都必须落在**不是用户文件**的那几类里（同本机那一半
//   `write_site_registry_tests::every_monitor_write_site_lands_outside_the_users_files`）：
//   闭集**没有「用户文件」这一档**；真是用户文件又一时搬不走的，记待收并指名谁来收。两向相等。

/// 一处远端写写的是什么。**闭集，刻意没有「用户文件」这一档。**
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RemoteLands {
    /// F08：我们的部署物（后端二进制 · 标记 · 入口 shim · 我们的脚本目录）—— 用户裁「留在 SFTP」。
    OwnDeployment,
    /// 我们自己的暂存区（`~/.cc-monitor/staging/`），落进用户目标的那一下在后端提交。
    OwnStaging,
    /// **是**用户文件，但有主、在别的路收：`(谁来收, 为什么不在本路)`。
    Pending(&'static str),
}

const REMOTE_CLASS: &[(&str, &str, RemoteLands)] = &[
    ("sftp.rs", "upload_atomic", RemoteLands::OwnDeployment),
    ("sftp.rs", "ensure_dir_all", RemoteLands::OwnDeployment),
    ("sftp.rs", "uninstall_remote_backend", RemoteLands::OwnDeployment),
    // `SftpFile` 今天只剩 F08 那一个用户（`put_ccm_entry`：`~/.local/bin/ccm` 那三行入口）。
    ("sftp.rs", "put_atomic", RemoteLands::OwnDeployment),
    ("sftp.rs", "delete_created", RemoteLands::OwnDeployment),
    (
        "sftp_pool.rs",
        "copy_remote_path",
        RemoteLands::Pending(
            "S4：`sftp_copy` 那套裸通道件退役（复制已走后端 `files-copy`，门禁 `f3-copy` 格随之退役）",
        ),
    ),
    ("sftp_pool.rs", "upload_to_staging", RemoteLands::OwnStaging),
    ("sftp_pool.rs", "ensure_staging_dir", RemoteLands::OwnStaging),
    (
        "sftp_pool.rs",
        "download_inner",
        RemoteLands::Pending("SR1b：SFTP 进本机常驻后端，下载的本机落地改由本机后端提交"),
    ),
];

#[test]
fn every_remaining_sftp_write_lands_outside_the_users_files() {
    let registered: Vec<(&str, &str)> = REMOTE_WRITES.iter().map(|(f, n, _, _)| (*f, *n)).collect();
    assert!(
        !registered.is_empty(),
        "远端写表空了 —— 下面那条相等在空集上成立"
    );
    let unclassified: Vec<String> = registered
        .iter()
        .filter(|(f, n)| !REMOTE_CLASS.iter().any(|(cf, cn, _)| cf == f && cn == n))
        .map(|(f, n)| format!("{f}::{n}"))
        .collect();
    let ghosts: Vec<String> = REMOTE_CLASS
        .iter()
        .filter(|(cf, cn, _)| !registered.iter().any(|(f, n)| f == cf && n == cn))
        .map(|(f, n, _)| format!("{f}::{n}"))
        .collect();
    assert!(
        unclassified.is_empty() && ghosts.is_empty(),
        "远端写与「它写的是什么」那张分类表对不上。\n  \
         没分类的（🔴 新的一处 SFTP 写先回答它是不是用户文件）：{unclassified:?}\n  \
         分类表里的幽灵（那一处没了，同轮摘行）：{ghosts:?}\n\n\
         用户裁远端三处（别名块 · 删会话 · `.mcp.json`）经远端后端写；F08 部署留在 SFTP。\n\
         ⇒ 分类是闭集，**没有「用户文件」这一档**：用户文件经那台远端的后端写（`user_files`）。"
    );
    for (f, n, c) in REMOTE_CLASS {
        if let RemoteLands::Pending(owner) = c {
            assert!(
                owner.chars().count() >= 10,
                "`{f}::{n}` 记成待收，却没写清谁来收"
            );
        }
    }
    // 远端三处用户文件的写，今天一处都不在本表的人群里（它们不再拿 SFTP 会话）。
    for gone in [
        "install_remote_alias_block",
        "uninstall_remote_alias_block",
        "remove_remote_file",
        "write_remote_mcp_server",
        "remove_remote_mcp_server",
    ] {
        assert!(
            !write_sites().iter().any(|(_, n)| n == gone),
            "`{gone}` 又经 SFTP 写了 —— 用户裁它经远端后端写"
        );
    }
}
