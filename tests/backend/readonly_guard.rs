//! F08a：backend 只读机器护栏（红线 I7 的机器化守护）。
//!
//! # `K-G6` `KG62`：性质与人群，两行逐字（**这两行各自只许有一句**，`g6_scope_pins` 钉着）
//!
//! - **它守的性质是**：backend **进程自身**不许改动用户既有数据 —— 碰盘只经 `platform/fs/` 交出去的句柄：读经 `Ro`（没有写方法）· 改用户文件只经文件管理面的 `Face`（写方法只收已过路径解析的 `Resolved`，只递给命令表文件管理那一族）· 分叉只经只会 `O_EXCL` 新建的 `NewFile` · 后端自家状态只经 `Own`（只写家目录的 `.cc-monitor` 与数据目录底下）。
//! - **它扫的人群是**：本 crate `src/` 递归全部 `.rs` 的生产段源码文本里点名文件系统命名空间的地方（`platform/fs/` 之外一处都不许）与造写句柄的地方（只许登记的那几处），外加另一张表：`Child::new` 的起进程点（生产段 `Command::new` 只许住起子进程原语 `platform/child.rs`）。
//!
//! # 怎么挡：一处实现，四种句柄
//!
//! 从前这份护栏扫源码文本：每一处 `fs::` 调用都得落在某张表里（读动词表 · 白名单层 · 文件管理面 · 自家状态），
//! 每写一行读盘、每多一个写的地方都要回来登记一次。今天挡在类型上：
//!
//! - 只有 `platform/fs/` 许点名 `std::fs` · `tokio::fs` · `os::unix::fs` · `OpenOptions` · `DirBuilder`（[`tests::only_the_fs_platform_module_names_the_filesystem`]）；
//!   那里点名的每一样也逐份登记（[`tests::the_fs_home_names_only_its_registered_entries`]）。
//! - 读不用递句柄：`Ro` 没有写方法，`Ro::open` 交回的那一份不实现 `Write`（[`tests::the_read_only_file_has_no_write`]）。
//! - 写要拿到句柄，句柄只在登记的那几处造（[`tests::the_write_handles_are_issued_only_at_startup`]：`Own` 只在启动入口 ·
//!   `Face` 只在分派那一层 · `NewFile` 只在分叉那两条命令 · `Resolved` 只由路径解析担保）；
//!   `Face` 只递给文件管理那一族（[`tests::only_the_file_manager_commands_are_handed_the_face`]），写方法不收 `&Path`（[`tests::the_face_writes_take_only_resolved_paths`]）。
//!
//! 今后加一处读，不用改护栏；加一处写，只要拿到对的句柄。只有新开一种写能力、或者多一个造句柄的地方，才回来改这里。
//!
//! ⚠ **这两行今天仍不是同一件事，而「它们是同一件事」这一格钉不住 —— 靠纪律**（`KG62` 如实登记）：
//! 「用户既有数据」是**语义**命题，人群是**文本形状 ＋ 类型**，两者之间没有可机检的桥。
//! 人群比性质**小**：不含依赖 crate 的写（见下：签字表守）· 不含被起进程的写面（`spawn_registry` 逐条登记）·
//! 不含不经那几个命名空间的写（裸系统调用：`platform/fs/` 外今天一处没有，但没有一条判据钉它）。
//! ⇒ **它今天真能拦住的形状与一个今天在盘上、形状相同却通过了的反例都在 [`g6_reach`]。**
//!
//! # 远端那一半另有一层
//!
//! **根本不在本机的写**（SFTP 那一套）不是这条护栏的一个漏项，是它**从来没有过的一维**。
//! ⇒ 远端那一半立在 [`remote_write_layer`]，两行性质 / 人群与盲区读数都在那里。
//! （本机换个命名空间的写，`os::unix::fs::symlink` 一族，今天由命名空间那一条一并拦着。）
//!
//! # 依赖 crate 的写面：不经 `platform/fs`，类型挡管不到它 —— 照旧由依赖签字那张表守
//!
//! 依赖 crate（如 `creds-core` 的 `perm.rs`）里的写不经 `platform/fs`，上面那几种句柄挡不住它。这里**不补人群**，
//! 而是让它「有人签过字」：清单上每一条依赖都要有一行登记（有没有写面 · 依据是什么 · 判档），**新加一条而没签字 ⇒ 当场红**。
//! 表与判据住 [`g6_dependency_signoff`]。
//!
//! 🔴 立表顺手量出一件事：那张表里**真有一条有写面** —— `creds-core` 的 `perm.rs` 里两处，
//! 而它们今天进不了发布二进制靠的是**一个 feature 没开**。
//! 从前盘上没有任何东西钉着那件事。
//!
//! # 收窄前那句绝对话今天是假的，**两处一次改完**
//!
//! `D1` 之后 `src/doc/INVARIANTS.md` §41.6 已经把铁律改成上面那句，并把收窄前那句绝对话
//! 逐字标成**原措辞**（要看原文去那里 —— 本文件刻意**不再抄一遍**：抄一遍就等于把那句假话
//! 留在这里，而这正是这里要治的病）。
//! 而这个文件里**同时**留着收窄前的绝对句两处（本头注一处 + 只读白名单那条的报错文案一处），
//! 判据兑现的却是收窄后那句 ⇒ 典型的「**只修一半**」，且盘上至少两件逐字引用了这句假话。
//! ⇒ 两处一并改，并由 `platform/fallback_guard.rs::g6_scope_pins` 立一条**反向棘轮**
//! 钉住那两个承重词从此不许回来。
//!
//! 本护栏遍历后端生产源码，剥掉 `#[cfg(test)]` 块（测试夹具可用 temp 目录）后，断言 `platform/fs/` 之外不点名文件系统命名空间、
//! 写句柄只在登记的那几处造。加只读测试是红线 I7 明确允许的（「backend 只准加只读测试/门禁」）。
//!
//! # ⚠ 本文件底部三个 `g6_*` 模块的住址是**当时改动范围限制的结果**，不是设计
//!
//! [`g6_doctrine`]（`§0a` 四情形表）与 [`g6_staged_zero`]（「今天零使用」那一族的指路判据）
//! 都是**跨护栏**的东西，正确落点是新立一份 `guard_doctrine.rs`；
//! 当时只许改三份护栏文件，新建文件与改 `main.rs` 都不在范围内。
//! ⇒ 暂住这里，**已上报 PM**。搬家那天把这段一起删掉。
//!
//! 注：本模块整体在 `#[cfg(test)]` 内，非测试构建为空、零运行期开销、不改后端行为。

#[cfg(test)]
mod tests {
    ///
    /// 它做的事（「剥掉测试段，只留生产段」）在本仓**早就有唯一住址**：
    /// [`guard_core::production_source`]（`src/common/guard-core`），
    /// 本 crate 直接用它。`guard_core` 的模块头注逐字记着搬家的理由：
    /// 「monitor 侧够不着后端的 `cfg(test)` 模块，于是它的守卫各自写了便宜近似
    /// （`src.split("\n#[cfg(test)]").next()`）—— 那个近似……**把扫描面砍掉三分之二**」。
    /// 那一族近似都是启发式（括号配平不认字符串/注释里的大括号），并且已经造成过 ≥4 次事故。
    ///
    /// 🔴 **退役的理由不是「它变成恒等函数了」** —— 当初那条预言
    /// **今天还不成立**，现打读数进了：`src/backend` 上它剥掉 1 081 538 字节
    /// （64 份里 51 份被它动过），`src/frontend/shell/src` 上 1 429 612 字节。
    /// 实测把它改成恒等函数，后端当场红 3 条，`no_test_code_leaks_into_any_production_section`
    /// 逐字报「49 份文件、533 个残留测试属性」。⇒ 退役的理由是
    /// **一条形状只许有一个住址**，不是「它没用了」。
    ///
    /// 两者的差别现打过（表）：换成 `production_source` 之后，
    /// `src/frontend/shell/src` 上的残留测试属性从 **42 → 4**（它认得原始字符串与可见性修饰，
    /// 朴素括号配平认不得），`src/backend` 上两者同为 **0**；
    /// 两个 workspace 的用例数与红绿**一格没动**。

    /// 文件系统**变更**模式的词表（`fs::`/`File::`/`OpenOptions` 锚定）。本机那一半今天挡在类型上、不再拿它扫；
    /// 留着是给依赖签字与远端那一层量**别的树**用的（[`g6_dependency_signoff`] · [`remote_write_layer`]）。
    pub(super) const FS_MUTATION_PATTERNS: &[&str] = &[
        "fs::write",
        "fs::create_dir",
        "fs::remove_file",
        "fs::remove_dir",
        "fs::rename",
        "fs::copy",
        "fs::hard_link",
        "fs::soft_link",
        "File::create",
        "File::options",
        "OpenOptions",
    ];

    /// 本护栏各条判据的人群：后端生产树 `src/backend/` 递归全部 `.rs`（`lib.rs` · `main.rs` 在内）。
    /// 从前由一张模块分区表派生；碰盘挡在类型上之后，人群就是整个 crate，分区表删了。
    pub(super) fn core_files() -> Vec<std::path::PathBuf> {
        let root = guard_core::addr::backend_src_root();
        let mut out: Vec<std::path::PathBuf> = guard_core::scan_tree!(&root, &["rs"])
            .into_iter()
            .map(|(p, _)| p)
            .collect();
        out.sort();
        assert!(
            out.len() >= 100,
            "只扫到 {} 份后端源码 —— 遍历坏了",
            out.len()
        );
        out
    }

    /// 「改动既有数据」那几形的词表（从前写盘白名单层的禁表；那一层删了，词表留给依赖签字与远端那一层量别的树）。
    ///
    /// 判据是「不许改动**既有**数据」：新建一个此前不存在的文件不违反它，
    /// 但删除 / 改名 / 截断 / 追加 / 覆盖写**都会**。
    pub(super) const WHITELIST_STILL_FORBIDDEN: &[&str] = &[
        "fs::write",      // 覆盖写既有文件
        "fs::create_dir", // 连目录都不建（projects 目录本来就在）
        "fs::remove_file",
        "fs::remove_dir",
        "fs::rename",
        "fs::copy",
        "fs::hard_link",
        "fs::soft_link",
        "File::create", // 无 O_EXCL 的建：已存在会被截断
        "truncate(true)",
        "append(true)",
        "set_len",
        // `.create(true)` 不截断，但对**已存在**的文件会从头写花它 —— 同样是改动既有数据。
        // 安全性：`create_new(true)` **不含**子串 `create(true)`，不会自伤。
        "create(true)",
    ];

    /// 拿这把跨进程锁的（`(模块, 为什么)`）：自家状态的写句柄本身 ＋ 两份不经它、却同样是一趟读—改—写的账号库那一面。
    /// 锁只是只读打开那个目录，不添任何写动词。
    const LOCK_HOLDERS: &[(&str, &str)] = &[
        (
            "platform/fs/own.rs",
            "自家状态写句柄 `Own::lock` 与凭据窄句柄 `AccountCreds::lock` 的本体（同一处拿）：原子写的签名要这把锁的守卫 ⇒ \
             写自家状态的每一份都在锁里写（类型管，不再逐份登记）；它自己不读—改—写任何一份",
        ),
        (
            "accounts/manage/wire.rs",
            "账号库的读—改—写（读清单与各号目录 → 算计划 → 经文件管理面落盘）：本机常驻后端与一次性 CLI 同时改，\
             后写的会把先写的那个号从清单里抹掉。锁的是账号库目录本身",
        ),
        (
            "accounts/manage/mcp_share_exec.rs",
            "各号共用的用户级 MCP 那一趟读—改—写（读共享集合与各号配置 → 对照 → 经文件管理面写各号与共享集合）：\
             监听器那一路、帧命令那一路、改账号库的命令同时来，后写的会拿旧的对照底算错。与账号库同一把锁",
        ),
    ];

    /// 🔴 **拿跨进程锁的只有登记的那几处 —— 人群两向相等。**
    ///
    /// 要求：「后端自有状态文件跨进程锁（`flock` 一类，Windows 对应）」。写自家状态那一半由类型保证：`Own::write` 的签名要
    /// 那个目录的锁的守卫（拿不到守卫就写不了）。本条管另一向：生产段里拿 `platform::lock::hold` 的文件 == [`LOCK_HOLDERS`]
    /// （两向；每份**恰好一处**）—— 多出来的 ⇒ 有人绕过写句柄自己拿锁、多半也在自己写。
    #[test]
    fn hx2_the_cross_process_lock_is_taken_only_where_registered() {
        let root = guard_core::addr::backend_src_root();
        let needle = "platform::lock::hold(";
        let mut holders: std::collections::BTreeMap<String, usize> = Default::default();
        for path in core_files() {
            let rel = path
                .strip_prefix(&root)
                .unwrap_or(&path)
                .to_string_lossy()
                .replace('\\', "/");
            let src = std::fs::read_to_string(&path).expect("read rs file");
            let n = guard_core::production_code(&src).matches(needle).count();
            if n > 0 {
                holders.insert(rel, n);
            }
        }
        let want: std::collections::BTreeSet<String> =
            LOCK_HOLDERS.iter().map(|(p, _)| p.to_string()).collect();
        let got: std::collections::BTreeSet<String> = holders.keys().cloned().collect();
        assert_eq!(
            got, want,
            "拿跨进程锁的模块 ≠ 登记的那几处。\n\
             多了 ⇒ 有人绕过写句柄自己拿锁（写自家状态经 `platform::fs::Own`，锁随它拿）；\n\
             少了 ⇒ 登记挂空号（搬走 / 改名了，这张表没跟）。"
        );
        for (m, n) in &holders {
            assert_eq!(*n, 1, "`{m}` 里拿了 {n} 处锁 —— 每份一处");
        }
    }

    // ═══════════════════ 删历史会话那一条═══════════════════

    /// 🔴🔴 **删历史会话那一条自己的那一道 —— 它恰好住一处。**
    ///
    /// 只有后端的文件管理部分写文件（本机也算）⇒ 删历史会话（本机此前是 monitor
    /// 进程直删、远端此前是 SFTP 直删）改成后端**一条明确的命令** `files-delete-session`，**只收 sid**。
    /// 从前这里写「写面其余每一条都被会话文件围栏挡在那几份文件外面，只有这一条能删会话文件」——
    /// 用户「文件管理器全部都可以改. 不需要任何围栏」之后，文件管理写面也删得掉会话文件；
    /// 本条钉的仍是删会话**那一条**的形状（只收 sid、自己那一道恰好一处），它一个字节没动。
    ///
    /// 四件，各一刀（两侧异源：一侧是后端**源码文本**，一侧是写面的**常量表**与**真跑一趟**）：
    ///
    /// 1. 例外那根围栏针 `fenced_session_file(` 在后端生产树里**恰好一处调用**（不算它自己的定义），
    ///    而且那一处住在 `delete_session_with` 的函数体里；
    /// 2. 适配层那个「按 sid 找要删的那一份」的入口，后端生产树里**只有**适配层自家注册表引用它，注册表的窄口只有门引用（改经门）；
    /// 3. 写面登记里那条命令的 `args` **恰好是** `["sid"]`；
    /// 4. 真跑：多给一个 `path` ⇒ `bad_args`（「只收 sid」是行为，不只是登记）。
    #[test]
    fn the_session_file_exception_lives_in_exactly_one_place() {
        let root = guard_core::addr::backend_src_root();
        let call = format!("fenced_session_file{}", "(");
        let def = format!("fn {call}");
        let locate = format!("session_file_for_delete{}", "");
        let port = format!("locate_session_for_{}", "delete");
        let mut calls: Vec<(String, String)> = Vec::new();
        let mut locators: std::collections::BTreeSet<String> = Default::default();
        let mut port_users: std::collections::BTreeSet<String> = Default::default();
        let mut scanned = 0usize;
        for path in core_files() {
            if path.file_name().and_then(|n| n.to_str()) == Some("readonly_guard.rs") {
                continue;
            }
            scanned += 1;
            let rel = path
                .strip_prefix(&root)
                .unwrap_or(&path)
                .to_string_lossy()
                .replace('\\', "/");
            let src = std::fs::read_to_string(&path).expect("read rs file");
            let prod = guard_core::production_code(&src);
            let mut cur_fn = String::new();
            for row in prod.lines() {
                let t = row.trim_start();
                if let Some(rest) = t
                    .strip_prefix("pub fn ")
                    .or_else(|| t.strip_prefix("fn "))
                    .or_else(|| t.strip_prefix("pub(crate) fn "))
                {
                    cur_fn = rest
                        .chars()
                        .take_while(|c| c.is_alphanumeric() || *c == '_')
                        .collect();
                }
                if row.contains(call.as_str()) && !row.contains(def.as_str()) {
                    calls.push((rel.clone(), cur_fn.clone()));
                }
            }
            if prod.contains(locate.as_str()) && rel != "agents/claudecode/paths.rs" {
                locators.insert(rel.clone());
            }
            if prod.contains(port.as_str()) && rel != "agents/mod.rs" {
                port_users.insert(rel);
            }
        }
        assert!(scanned >= 60, "只扫到 {scanned} 份后端源文件 —— 遍历坏了");
        assert_eq!(
            calls,
            vec![(
                "control/files_write.rs".to_string(),
                "delete_session_with".to_string()
            )],
            "\n删会话那一道（`{call}`）必须**恰好一处调用**、住 `delete_session_with`。\n\
             多出来的每一处都是把「能删会话文件」借给了第二个函数。"
        );
        // 落点那一问改经门：适配层那个入口只有它自家注册表那一行引用，
        //   注册表的窄口（`agents::locate_session_for_delete`）只有门（命令注册那一处）引用、递给写面。
        assert_eq!(
            locators.into_iter().collect::<Vec<_>>(),
            vec!["agents/claudecode/mod.rs".to_string()],
            "「按 sid 找要删的那一份」只许适配层自家注册表那一行引用 —— 别的面拿到它就等于拿到了删会话的落点"
        );
        assert_eq!(
            port_users.into_iter().collect::<Vec<_>>(),
            vec!["stream/inbound/files_session_face.rs".to_string()],
            "删会话落点的窄口只许门引用（它递给写面）—— 别的面拿到它就等于拿到了删会话的落点"
        );
        assert!(
            crate::control::files_write::MANAGE_COMMANDS
                .iter()
                .any(|c| c.name == "files-delete-session"),
            "写面登记里没有 `files-delete-session` —— 删历史会话那条路断了"
        );
        let keys: Vec<String> = serde_json::to_value(cmd_wire::files::FilesDeleteSessionReq {
            sid: "abc".to_string(),
        })
        .expect("入参总能序列化")
        .as_object()
        .expect("对象")
        .keys()
        .cloned()
        .collect();
        assert_eq!(
            keys,
            vec!["sid".to_string()],
            "删历史会话那条命令**只收 sid** —— 多一个入参就多一种表达「另一份文件」的办法"
        );
        match crate::control::files_write::tests::ask(
            "files-delete-session",
            &serde_json::json!({"sid": "abc", "path": "/tmp/x.jsonl"}),
        ) {
            Err(crate::control::files_write::WriteFail { code, .. }) => {
                assert_eq!(code, "bad_args", "多给一个 `path` 该回 bad_args")
            }
            Ok(v) => panic!("🔴 多给了一个 `path`，删会话那条竟然答了：{v}"),
        }
    }

    /// 🔴🔴 **文件管理写面不再问「这是不是一份会话记录」。**
    ///
    /// 要求「**文件管理器全部都可以改. 不需要任何围栏**」；
    /// 文件管理面写的路径因此只过路径解析（`Face` 只收它交回的 `Resolved`），不问会话形状。
    ///
    /// # 两向相等，人群按事实取样
    ///
    /// 人群：后端生产树里**调**会话形状判定（`is_session_record_path(` / `is_session_record_file(`，
    /// 不算定义行）的 `(文件, 所在函数)`，逐行现打。
    /// 期望：删历史会话那一条要的三处 —— 写面 `fenced_session_file`（「要删的必须**是**会话」；经门递进来的 `SessionPort.is_record` 问）·
    /// 适配层 `session_file_for_delete_in`（按 sid 找到之后再判一次形状）· 适配层 `is_session_record_path`
    /// （`&Path` 门面，调字符串那一份）。期望取自 `files-delete-session` 那条既有裁决（RW1「只收 sid」），
    /// 不从判定本家现推（异源）。
    /// 多一处 ⇒ 有人把会话文件围栏接回了文件管理写面（或别的面）；少一处 ⇒ 删会话那一条的形状变了，
    /// 那条命令不在 FN1 射程里，先去看是谁动的。
    ///
    /// 正控：同一个抽取器在一段合成的「写面又问了一次」上必须认出 `(文件, 函数)`；定义行不算调用。
    #[test]
    fn the_file_manager_face_never_asks_the_session_shape() {
        fn session_shape_calls(rel: &str, prod: &str) -> Vec<(String, String)> {
            // 写面今天经门递进来的窄口问（`SessionPort.is_record`），不再直呼适配层那个名字 ⇒ 第三根针。
            let needles = [
                format!("is_session_record_{}(", "path"),
                format!("is_session_record_{}(", "file"),
                format!("is_{}(", "record"),
            ];
            let mut cur_fn = String::new();
            let mut out = Vec::new();
            for row in prod.lines() {
                let t = row.trim_start();
                let is_def = t
                    .strip_prefix("pub fn ")
                    .or_else(|| t.strip_prefix("fn "))
                    .or_else(|| t.strip_prefix("pub(crate) fn "));
                if let Some(rest) = is_def {
                    cur_fn = rest
                        .chars()
                        .take_while(|c| c.is_alphanumeric() || *c == '_')
                        .collect();
                    continue; // 定义行（签名）不算调用
                }
                if needles.iter().any(|n| row.contains(n.as_str())) {
                    out.push((rel.to_string(), cur_fn.clone()));
                }
            }
            out
        }
        // 正控：抽取器认得出合成的一处调用，也认得出定义行不是调用。
        let fake = format!(
            "pub fn is_session_record_{p}(t: &Path) -> bool {{\n    true\n}}\n\
             pub fn make_dir(root: &Path) {{\n    if is_session_record_{p}(root) {{}}\n}}\n",
            p = "path"
        );
        assert_eq!(
            session_shape_calls("control/files_write.rs", &fake),
            vec![("control/files_write.rs".to_string(), "make_dir".to_string())],
            "抽取器在合成语料上认不出那一处调用 / 把定义行当成了调用 —— 下面那个集合不可信"
        );

        let root = guard_core::addr::backend_src_root();
        let mut found: std::collections::BTreeSet<(String, String)> = Default::default();
        let mut scanned = 0usize;
        for path in core_files() {
            if path.file_name().and_then(|n| n.to_str()) == Some("readonly_guard.rs") {
                continue;
            }
            scanned += 1;
            let rel = path
                .strip_prefix(&root)
                .unwrap_or(&path)
                .to_string_lossy()
                .replace('\\', "/");
            let src = std::fs::read_to_string(&path).expect("read rs file");
            found.extend(session_shape_calls(
                &rel,
                &guard_core::production_code(&src),
            ));
        }
        assert!(scanned >= 60, "只扫到 {scanned} 份后端源文件 —— 遍历坏了");
        let want: std::collections::BTreeSet<(String, String)> = [
            ("agents/claudecode/paths.rs", "is_session_record_path"),
            ("agents/claudecode/paths.rs", "session_file_for_delete_in"),
            ("control/files_write.rs", "fenced_session_file"),
        ]
        .iter()
        .map(|(a, b)| (a.to_string(), b.to_string()))
        .collect();
        assert_eq!(
            found,
            want,
            "\n🔴 后端里问「这是不是一份会话记录」的地方，不再恰是删历史会话那一条要的三处。\n\
             多出来的（⇒ 有人把会话文件围栏接回了文件管理写面或别处，而要求是\n\
             「文件管理器全部都可以改. 不需要任何围栏」）：{:?}\n\
             少了的（⇒ 删会话那一条的形状变了；它不在 FN1 射程里，先去看是谁动的）：{:?}",
            found.difference(&want).collect::<Vec<_>>(),
            want.difference(&found).collect::<Vec<_>>()
        );
    }

    /// U-1（2026-08-01）：**剥法的欠剥方向也要机器钉住。**
    ///
    /// 剥法的已知局限（早年那份朴素括号配平不识别字符串/注释里的大括号）此前只写在散文里。
    /// 散文挡不住事：`guard_support.rs` 落地时，我自己注释里一个孤立的右大括号就把配平提前收尾，
    /// 让那个测试模块的 5 个 `#[test]` 整段留在「生产段」里 —— 而这**不会红**（那些测试只读文件、
    /// 不含写模式），是静默的。下一个往 `guard_support.rs` 加 tempdir 测试的人才会撞上
    /// 一条说「生产代码 guard_support.rs 含 fs::write」的**误导性**诊断。
    ///
    /// 处置遵循 §41.4 第 1 条纪律：**撞了改注释措辞，别改护栏**（本次就是把注释里的
    /// 孤立大括号改成中文名词）。
    ///
    /// ⚠ 剥法本体已收进 [`guard_core::production_source`]
    /// （本文件那份便宜近似退役了，理由住本模块头注）。**本条一个字都没放宽** ——
    /// 它量的仍然是「剥完的生产段里不许残留测试属性」，只是尺子换成了那个唯一住址。
    #[test]
    fn no_test_code_leaks_into_any_production_section() {
        let src_dir = guard_core::addr::backend_src_root();
        let mut stack = vec![src_dir];
        let mut leaks: Vec<(String, usize)> = Vec::new();
        while let Some(dir) = stack.pop() {
            for entry in std::fs::read_dir(&dir).expect("read src dir") {
                let path = entry.expect("dir entry").path();
                if path.is_dir() {
                    stack.push(path);
                    continue;
                }
                if path.extension().and_then(|e| e.to_str()) != Some("rs") {
                    continue;
                }
                let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
                if name == "readonly_guard.rs" {
                    continue; // 与 `scan` 同款跳过：本文件的模式字面量必然含这些子串
                }
                let prod = guard_core::production_source(
                    &std::fs::read_to_string(&path).expect("read rs file"),
                );
                // 用拼接写法，免得本行自己被数进去。
                let attr = format!("#[{}]", "test");
                let n = prod.matches(attr.as_str()).count();
                if n > 0 {
                    leaks.push((name.to_string(), n));
                }
            }
        }
        leaks.sort();
        assert!(
            leaks.is_empty(),
            "剥完仍有测试属性残留在生产段里：{leaks:?}\n\
             多半是某个注释/字符串里有**不配对的大括号**，把括号配平提前收尾了。\n\
             ⇒ 改那处措辞（§41.4 第 1 条纪律），**不要**动剥法本体
             （`guard_core::production_source`）。"
        );
    }

    // ═══════════════ 碰盘只经 `platform/fs/`（类型挡第一刀：读）═══════════════

    /// 唯一允许点名文件系统命名空间的地方。别处要读盘只能经它交出去的 [`crate::platform::fs::Ro`]。
    const FS_HOME: &str = "platform/fs/";

    /// 带路径的那几个命名空间：点名一处就红（前后都要是边界，`std::fsx` 不算）。
    const FS_PATHS: &[&str] = &["std::fs", "tokio::fs", "os::unix::fs", "os::windows::fs"];
    /// 只认整词的那两个类型名（`OpenOptionsLike` 不算）。
    /// ⚠ 裸 `File` 不在这里：它不在前导里，要用 `std` / `tokio` 的那个 `File` 先得点名上面某个路径
    ///   （全路径 · 导入 · 分组导入 · 起别名都是），而裸词会撞上一堆叫 `File` 的枚举臂（`Item::File` · `What::File` …）。
    const FS_WORDS: &[&str] = &["OpenOptions", "DirBuilder"];
    /// 分组导入的根：`use std::{fs, io}` 这种写法里没有 `std::fs` 这串字，要按组认。
    const FS_GROUP_ROOTS: &[&str] = &["std::", "tokio::", "os::", "os::unix::", "os::windows::"];

    fn word_char(c: char) -> bool {
        c.is_ascii_alphanumeric() || c == '_'
    }

    fn word_at(code: &str, at: usize, needle: &str) -> bool {
        let before = code[..at].chars().next_back().is_some_and(word_char);
        let after = code[at + needle.len()..]
            .chars()
            .next()
            .is_some_and(word_char);
        !before && !after
    }

    /// 一段生产代码（已剥测试段与注释）里点名文件系统命名空间的每一处：`(行号, 命中的那一形)`。
    pub(super) fn fs_namespace_hits(code: &str) -> Vec<(usize, String)> {
        let line_of = |at: usize| code[..at].matches('\n').count() + 1;
        let mut hits = Vec::new();
        for needle in FS_PATHS.iter().chain(FS_WORDS) {
            let mut from = 0;
            while let Some(k) = code[from..].find(needle) {
                let at = from + k;
                if word_at(code, at, needle) {
                    hits.push((line_of(at), needle.to_string()));
                }
                from = at + needle.len();
            }
        }
        for root in FS_GROUP_ROOTS {
            let open = format!("{root}{{");
            let mut from = 0;
            while let Some(k) = code[from..].find(open.as_str()) {
                let at = from + k;
                let body_at = at + open.len();
                let mut depth = 1usize;
                let mut end = code.len();
                for (i, c) in code[body_at..].char_indices() {
                    match c {
                        '{' => depth += 1,
                        '}' => {
                            depth -= 1;
                            if depth == 0 {
                                end = body_at + i;
                                break;
                            }
                        }
                        _ => {}
                    }
                }
                let body = &code[body_at..end];
                let mut f = 0;
                while let Some(j) = body[f..].find("fs") {
                    let p = f + j;
                    if word_at(body, p, "fs") {
                        hits.push((line_of(at), format!("{open}…fs…}}")));
                        break;
                    }
                    f = p + 2;
                }
                from = body_at;
            }
        }
        hits.sort();
        hits.dedup();
        hits
    }

    /// 🔴 **碰盘只经 `platform/fs/`。** 后端生产段除了 `platform/fs/`，一处都不许点名
    /// `std::fs` · `tokio::fs` · `os::unix::fs` · `OpenOptions` · `DirBuilder`（分组导入、起别名一样算；`std::fs::File` 只能经这些路径点名）。
    /// 读盘走 `platform::fs::Ro`（只读：没有写方法，`open` 交回的只能读和挪读写位置）；
    /// 今后加一个读动词只改 `platform/fs/` 一处，本条不用动。
    #[test]
    fn only_the_fs_platform_module_names_the_filesystem() {
        let root = guard_core::addr::backend_src_root();
        let mut bad = Vec::new();
        let (mut scanned, mut home_hits) = (0usize, 0usize);
        for (path, src) in guard_core::scan_tree!(&root, &["rs"]) {
            let rel = path
                .strip_prefix(&root)
                .unwrap_or(&path)
                .to_string_lossy()
                .replace('\\', "/");
            let code = guard_core::production_code(&src);
            let hits = fs_namespace_hits(&code);
            scanned += 1;
            if rel.starts_with(FS_HOME) {
                home_hits += hits.len();
                continue;
            }
            // 行号是剥过注释的那份里的，不对盘 ⇒ 报那一行的原文。
            for (line, what) in hits {
                let text = code.lines().nth(line - 1).unwrap_or("").trim();
                bad.push(format!("  {rel}: {what} —— {text}"));
            }
        }
        assert!(
            scanned >= 100,
            "只扫到 {scanned} 份后端源码 —— 遍历坏了，本条会空转地绿"
        );
        assert!(
            bad.is_empty(),
            "后端生产段在 `{FS_HOME}` 之外点名了文件系统：\n{}\n\n\
             读盘走 `crate::platform::fs::Ro`（`Ro::metadata` · `Ro::read_to_string` · `Ro::open` …）；\n\
             缺一个读动词就加进 `platform/fs/ro.rs`。写盘不在这里开口子 —— 那是写的那几层的事。",
            bad.join("\n")
        );
        assert!(
            home_hits > 0,
            "`{FS_HOME}` 里一处命名空间都没扫到 —— 针坏了（那里本该是唯一点名 `std::fs` 的地方）"
        );
    }

    /// 上一条的针真咬得住：该红的那几形红、该绿的那几形绿（a9b661c1a 那一形就是该绿的那一句）。
    #[test]
    fn the_fs_namespace_needle_bites_and_lets_the_typed_reads_through() {
        for red in [
            "fn f(p: &Path) { let _ = std::fs::metadata(p); }",
            "fn f(p: &Path) { let _ = std::fs::symlink_metadata(p); }",
            "use std::{fs, io};",
            "use std::{io::{self, Read}, fs::File};",
            "async fn f(p: &Path) { tokio::fs::read(p).await; }",
            "fn f(r: BufReader<std::fs::File>) {}",
            "use std::fs as f;",
            "let p = ::std::fs::read_link(p);",
            "let o = OpenOptions::new();",
            "let b = DirBuilder::new();",
            "use std::os::unix::fs::PermissionsExt;",
            "use std::os::{unix::fs::MetadataExt};",
        ] {
            assert!(!fs_namespace_hits(red).is_empty(), "该红没红：{red}");
        }
        for green in [
            "fn f(p: &Path) { let _ = crate::platform::fs::Ro::symlink_metadata(p); }",
            "use crate::platform::fs::{Ro, RoFile};",
            "struct FileKind; let x = Profile::new(); let y = files::list();",
            "match i { Item::File { .. } => {} }",
            "use std::{io, path::Path};",
            "let s = std::fsync;",
        ] {
            assert_eq!(fs_namespace_hits(green), vec![], "该绿没绿：{green}");
        }
    }
    /// `platform/fs/` 里点名标准库文件系统的只许两份：`ro.rs` 点名的每一样都是读（本表）；`own.rs` 的见 [`OWN_NAMES`]
    /// （文件管理面的写原语住 `mod.rs`，走系统调用、一个命名空间都不点名，谁调得到由写的那几层管）。
    /// 今后加一个读动词，改的是 `ro.rs` 与这里，别处一行不动。
    const RO_NAMES: &[&str] = &[
        "std::fs::read(",
        "std::fs::read_to_string(",
        "std::fs::read_dir(",
        "std::fs::metadata(",
        "std::fs::canonicalize(",
        "std::fs::symlink_metadata(",
        "std::fs::read_link(",
        "std::fs::File::open(",
        "std::fs::File)",
        "std::fs::File,",
        "std::fs::Metadata;",
        "std::fs::ReadDir;",
        "std::os::unix::fs::PermissionsExt",
        "std::os::unix::fs::MetadataExt",
        // 不跟链接的只读开法：那份开法只 `.read(true)`，交出去的仍是 `RoFile` / `RoAsyncFile`。
        "std::fs::OpenOptions::new()",
        "std::fs::OpenOptions {",
        "std::os::unix::fs::OpenOptionsExt",
        "std::fs::Permissions;",
        // 异步读（传输台读本机那一份）：问元数据 · 只读打开，交回的只能读和挪位置。
        "tokio::fs::metadata(",
        "tokio::fs::File::open(",
        "tokio::fs::OpenOptions::from(",
        "RoAsyncFile(tokio::fs::File)",
    ];
    /// `platform/fs/face.rs`（文件管理面 `Face` · 分叉那只手 `NewFile`）点名的标准库条目：写面动词闭集
    /// （unix 上是 `*at` 一族，经 `libc`；Windows 臂按路径：新建 · 就地覆盖写 · 建目录 · 改名 · 删文件 · 删空目录 · 改权限）、
    /// unix 上从描述符包成文件的那一下、读权限位的那一个特征，与开出来那一份的类型。
    /// 谁能调由类型管：方法都在 `Face` / `NewFile` 上（写方法只收 `Resolved`），两者只有登记的那几处造（[`ISSUERS`]）。
    const FACE_NAMES: &[&str] = &[
        "std::fs::OpenOptions {",
        "std::fs::OpenOptions::new()",
        "std::fs::create_dir(",
        "std::fs::rename(",
        "std::fs::remove_file(",
        "std::fs::remove_dir(",
        "std::fs::set_permissions(",
        "FaceFile(std::fs::File)",
        "FaceAsyncFile(tokio::fs::File)",
        "tokio::fs::File::from_std(",
        "io::Result<std::fs::File>",
        "std::fs::File::from(",
        "std::os::unix::fs::PermissionsExt",
    ];
    /// `platform/fs/own.rs`（自家状态写句柄 `Own`）点名的标准库条目：它的写只有这几样（删文件 · 改名 · 带权限位建一层目录 · 自家目录收回 0700）；
    /// 原子写的旁名经 `creds_core::perm::create_private` 建（依赖签字那一侧管）。谁能调由类型管：方法都在 `Own` 上，`Own` 只在启动时造。
    const OWN_NAMES: &[&str] = &[
        "std::fs::remove_file(",
        "std::fs::rename(",
        // 订阅号凭据窄句柄（`AccountCreds`）续期那两把 `mkdir` 锁：建 · 删空目录（那一家定的锁协议，与它互斥只能照它的做法）。
        "std::fs::create_dir(",
        "std::fs::remove_dir(",
        "std::fs::DirBuilder::new()",
        "std::os::unix::fs::DirBuilderExt",
        // 诊断文件窄句柄（`DiagLog`）：`O_EXCL` 新建当前那份 · 只追加一整行（权限位 0600）两个打开法，交出去的那一份只会写、交得出 fd。
        "std::fs::OpenOptions::new()",
        "std::os::unix::fs::OpenOptionsExt",
        "DiagFile(std::fs::File)",
        // 后端自家目录（`own_dir`）组 / 他人有权 ⇒ 收回 0700 那一下。
        "std::fs::set_permissions(",
    ];

    #[test]
    fn the_fs_home_names_only_its_registered_entries() {
        let root = guard_core::addr::backend_src_root();
        let mut named = 0usize;
        let mut bad = Vec::new();
        for (path, src) in guard_core::scan_tree!(&root.join("platform/fs"), &["rs"]) {
            let rel = path
                .strip_prefix(&root)
                .unwrap_or(&path)
                .to_string_lossy()
                .replace('\\', "/");
            let code = guard_core::production_code(&src);
            let names: &[&str] = match rel.as_str() {
                "platform/fs/ro.rs" => RO_NAMES,
                "platform/fs/own.rs" => OWN_NAMES,
                "platform/fs/face.rs" => FACE_NAMES,
                _ => &[],
            };
            if names.is_empty() {
                for (line, what) in fs_namespace_hits(&code) {
                    let text = code.lines().nth(line - 1).unwrap_or("").trim();
                    bad.push(format!(
                        "  {rel}: {what} —— {text}（只许 `ro.rs` · `own.rs` · `face.rs` 点名）"
                    ));
                }
                continue;
            }
            // 认得的那几形整段摘掉，剩下的里面再有一处点名就是读以外的东西。
            let mut rest = code.clone();
            for ok in names {
                named += rest.matches(ok).count();
                rest = rest.replace(ok, "");
            }
            for (line, what) in fs_namespace_hits(&rest) {
                let text = rest.lines().nth(line - 1).unwrap_or("").trim();
                bad.push(format!("  {rel}: {what} —— {text}"));
            }
        }
        assert!(
            named >= 8,
            "`platform/fs/ro.rs` 只扫到 {named} 处点名 —— 针或住址坏了，本条会空转地绿"
        );
        assert!(
            bad.is_empty(),
            "`platform/fs/` 里点名了读以外的东西：\n{}\n\n\
             这里是全后端唯一能碰盘的地方：读那一半只许读（`RO_NAMES`），自家状态那一半只许那几样（`OWN_NAMES`）；别的写的能力不从这里开口子。",
            bad.join("\n")
        );
    }

    /// 自家状态写句柄的造法（`(针, 只许出现在哪几份)`）：[`crate::platform::fs::Own`] 启动时造一次、顺着入口递下去；
    /// 别处一造就红（类型挡管「拿着它才写得出来」，这一条管「谁能凭空造一个」）。定义所在那一份不算。
    // 针不带括号：`Own::issue` 一族（照进程环境 · 照注入的环境）都只许启动入口造。
    const ISSUERS: &[(&str, &[&str])] = &[
        ("Own::issue", &["main.rs"]),
        ("DiagLog::issue", &["main.rs"]),
        // 文件管理面的写句柄：只有分派那一层造 —— `spec.rs::with_face`（按档装处理器的 `prepare` 那一档 · 资产域那扇门的进程内分派都经它，句柄只活那一问），
        //   按命令表那一族（typed 的 `Facing` 那一档）递下去；传输台那四条硬臂的票表在帧面那一处造的时候领一份。
        ("Face::issue", &["stream/inbound/mod.rs", "wire/spec.rs"]),
        // 分叉那只只会新建的手：只有分叉那两条命令造 —— 都是 typed，各在它自己那份实现里。
        (
            "NewFile::issue",
            &["faces/fork_face.rs", "faces/session_open_face.rs"],
        ),
        // 「已过路径解析」只有路径解析本身造得出：从根句柄逐段开（`common/root_path.rs`，文件管理写面与传输台共用）；
        // 按路径担保的那一形只给删会话那一道（写面）。
        ("Resolved::walk", &["common/root_path.rs"]),
        ("Resolved::vouch", &["control/files_write.rs"]),
    ];

    #[test]
    fn the_write_handles_are_issued_only_at_startup() {
        let root = guard_core::addr::backend_src_root();
        for (needle, want) in ISSUERS {
            let mut got: Vec<String> = Vec::new();
            for (path, src) in guard_core::scan_tree!(&root, &["rs"]) {
                let rel = path
                    .strip_prefix(&root)
                    .unwrap_or(&path)
                    .to_string_lossy()
                    .replace('\\', "/");
                if rel.starts_with(FS_HOME) {
                    continue;
                }
                if guard_core::production_code(&src).contains(needle) {
                    got.push(rel);
                }
            }
            got.sort();
            assert_eq!(
                got, *want,
                "`{needle}` 出现的地方不是启动入口 —— 写句柄只许在启动时造一次、顺着入口递下去；\n\
                 要写自家状态的那一处改成收一个 `&Own` 参数，由调它的那一层递进来"
            );
        }
        // 正控：针认得出合成语料里的那一造。
        assert!(
            guard_core::production_code("fn f() { let o = Own::issue(); }").contains(ISSUERS[0].0)
        );
    }

    /// 递 [`crate::platform::fs::Face`] 的那一种能力（处理器写 `type Needs = needs::Face`）只有文件管理那一族用；用它的命令恰好是
    /// 文件管理写面登记的那几张命令表的并（写面 · 上传提交 · 解压）—— 两侧异源：一侧是命令表里的档位（现打），一侧是写面自己的表。
    #[test]
    fn only_the_file_manager_commands_are_handed_the_face() {
        use crate::stream::inbound::REGISTRY;
        let got: std::collections::BTreeSet<&str> = REGISTRY
            .iter()
            .filter(|s| s.run.face)
            .map(|s| s.name)
            .collect();
        let want: std::collections::BTreeSet<&str> = crate::control::files_write::MANAGE_COMMANDS
            .iter()
            .chain(crate::control::files_commit::COMMIT_COMMANDS)
            .chain(crate::control::files_extract::EXTRACT_COMMANDS)
            .map(|c| c.name)
            .collect();
        assert!(
            want.len() >= 10,
            "写面的命令表只并出 {} 条 —— 表读坏了",
            want.len()
        );
        assert_eq!(
            got, want,
            "拿得到文件管理写句柄的命令与写面登记的命令对不上（左 = 命令表里要 `needs::Face` 的那几条）"
        );
    }

    /// `Face` 的写方法只收 [`crate::platform::fs::Resolved`]：`impl Face` 里每个方法的参数表都不点名 `Path`
    /// （建链接那一处的链接内容是原样写的文本，不是落点 ⇒ 登记的唯一例外）。拿 `&Path` 调它编不过 —— 本条钉的是签名没被放宽。
    #[test]
    fn the_face_writes_take_only_resolved_paths() {
        let src = std::fs::read_to_string(
            guard_core::addr::backend_src_root().join("platform/fs/face.rs"),
        )
        .expect("读 face.rs");
        let code = guard_core::production_code(&src);
        let body = code
            .split("impl Face {")
            .nth(1)
            .and_then(|b| b.split("\n}\n").next())
            .expect("找不到 `impl Face {` 那一段");
        let sigs = face_method_params(body);
        let mut bad = Vec::new();
        for (name, params) in &sigs {
            // 建链接：链接内容 `target: &Path` 是登记的那一处，落点照样只收 `Resolved`。
            let rest = if name == "make_link" {
                params.replacen("target: &Path", "", 1)
            } else {
                params.clone()
            };
            if guard_core::contains_word(&rest, "Path") {
                bad.push(format!("{name}({params})"));
            }
        }
        assert!(
            sigs.len() >= 10,
            "`impl Face` 只认出 {} 个方法 —— 切法坏了",
            sigs.len()
        );
        assert!(
            bad.is_empty(),
            "`Face` 的写方法收了裸路径：\n  {}",
            bad.join("\n  ")
        );
        // 正控：同一切法认得出一个收 `&Path` 的方法（名字 · 参数表都切对）。
        let probe = face_method_params("    fn delete_file(&self, at: &Path) -> io::Result<()> {}");
        assert_eq!(probe.len(), 1);
        assert_eq!(probe[0].0, "delete_file");
        assert!(guard_core::contains_word(&probe[0].1, "Path"));
    }

    /// 一段 `impl` 体里每个方法的 `(名字, 参数表)`（到第一个 `)` 为止）。
    fn face_method_params(body: &str) -> Vec<(String, String)> {
        const FN_KW: &str = "fn ";
        body.split(FN_KW)
            .skip(1)
            .map(|chunk| {
                let name: String = chunk
                    .chars()
                    .take_while(|c| c.is_alphanumeric() || *c == '_')
                    .collect();
                (name, chunk.split(')').next().unwrap_or("").to_string())
            })
            .collect()
    }

    /// `Ro::open` 交回的那一份没有 `Write`：拿着它写不出来（现打，不是读源码）。
    #[test]
    fn the_read_only_file_has_no_write() {
        use std::marker::PhantomData;
        trait NoWrite {
            const WRITES: bool = false;
        }
        struct Probe<T>(PhantomData<T>);
        impl<T> NoWrite for Probe<T> {}
        #[allow(dead_code)]
        impl<T: std::io::Write> Probe<T> {
            const WRITES: bool = true;
        }
        assert!(
            !<Probe<crate::platform::fs::RoFile>>::WRITES,
            "`RoFile` 实现了 `Write` —— 只读打开交回的东西能写了"
        );
        // 正控：同一根探针在一个真能写的类型上答「能写」，否则上面那条恒绿。
        assert!(<Probe<Vec<u8>>>::WRITES, "探针坏了：`Vec<u8>` 明明能写");
    }
}

/// U8a-2 / **D1 裁决的代码强制**：backend 起进程的**受管例外清单**。
///
/// # 为什么要这条
///
/// `readonly_guard` 的既有判据只认**文件系统写模式**，**它不认 `Command` / `spawn`**
/// （`§0.2` 早就登记了这件事：「『backend 只读』这个词今天已经在骗人」）。
/// 于是「起一个会写用户数据的进程」这条路，**机器护栏永远不会红**。
///
/// D1 的裁决选了①：**铁律收窄为「backend 进程自身不许写用户既有数据」**，
/// 间接写不算 —— 但推荐里带一个**强制条件**：
///
/// > 必须同时：在 §41.6 写下「间接写的责任在被起的那个程序，backend 的责任是不越权
/// > 替它决定写什么」+ 把预信任那条单列为**受管的例外**，**逐条列举写面**。
///
/// 「逐条列举」不能只是散文里列一遍 —— 那正是 §0.2 批评的「护栏与散文说的不是一件事」。
/// 本护栏就是那份清单的机器形态：**生产段每一处起进程都必须在这里登记，并写明它做什么。**
///
/// # 它挡什么、不挡什么（如实登记）
///
/// - **挡**：悄悄新增一个起进程点。新增而不登记 ⇒ 红。
/// - **不挡**：已登记的那条改成起别的东西（登记的是**文件名**，不是完整 argv）。
///   完整 argv 里有格式化变量（`tmux_probe_script()` 拼的脚本），钉不住也不该钉死。
///   这条边界写在这里，免得下一个人以为它保证了更多。
#[cfg(test)]
mod spawn_registry {
    /// 生产段允许起的进程，**逐条登记**。
    ///
    /// # 三元组扩成**五元组**：多出来的两栏是「走的是哪一格」与「解锁条件」
    ///
    /// `(文件, 起什么, why——做什么、为什么不算违反收窄后的铁律, 格——`§0a` 四情形表里的哪一格, unlock——什么条件满足之后这一条就能删)`
    ///
    /// 形状照 `agent_locality_guard::AGENT_NAMED_WIRE_FIELDS`（本仓已有的活体：四元组 + 长度地板）。
    /// 第四栏由 [`super::g6_doctrine::is_cell`] 做**枚举比对**（不是子串），
    /// ⇒ **加一条登记却说不出它走的是哪一格，当场红** —— 那正是 `KG64` 要的那个时刻。
    ///
    /// ⚠ 本表**每一条**今天都落在同一格（`缩性质`）：`D1` 把铁律从收窄前那句绝对话缩成
    /// 〔`K-R79` 09-12 订正：这一行原文逐字写的是「**本表七条**」，而现打 `ALLOWED.len()` 是 **9** ——
    ///  那是一处**写死的基数**（派工单固定项 13b：报一个基数也是复述，要现算）。
    ///  今天要那个数就让机器印：`readonly_guard::remote_write_layer` 的盲区读数里 `L2` 那一行现算它。〕
    /// 「backend **进程自身**不许改动用户既有数据」，而「缩掉的那一半从此归谁」的答案就是本表 ——
    /// 归**被起的那个程序**。这不是巧合，是这张表存在的理由。
    pub(super) const ALLOWED: &[(&str, &str, &str, &str, &str)] = &[
        (
            "dial/ssh_config.rs",
            "ssh",
            "`ssh -G <别名>`：OpenSSH 客户端**只解析** `~/.ssh/config` 打出有效参数、不建连接、不写任何文件。\
             别名先过 allowlist、`-` 开头另挡，argv 直传不过 shell。从 monitor 搬来（解读与拨号同一个家）。",
            "缩性质",
            "自己解析 ssh_config（Include / Match / 通配 / 默认值）的那天 —— 今天交给 `ssh -G` 是因为它最准。",
        ),
        (
            "control/ccm/mod.rs",
            "sh",
            "`K-R48`：一次性 `ccm` 模式把**一条已经渲好的命令串**交给 POSIX shell 并 `exec` 掉自己\
             （attach 那条、容器路的收尾那条、以及带 `CCM_ENV` / cc-bus 配方的那条）。\
             这与用户在自己终端里手敲同一条命令**没有区别** —— 它跑在用户的进程里、\
             做的是用户这一趟本来就要做的事（D1 裁决的正例，同 `control/launch.rs` 那条）。\
             ⚠ **backend 常驻那条路走不到这里**：入口在 `main()` 最前面按 `argv[0]` / 子命令词分出去，\
             常驻模式一个字节都不经过本模块。",
            "缩性质",
            "把收尾那几段（兜底轮询 / attach / cc-bus 登记）逐条做成原生动作、不再经 shell 的那天。\
             ⚠ 在那之前**不许**因为「反正已经登记了」而往这一条底下加第二种用途。",
        ),
        (
            "control/ccm/mod.rs",
            "<非字面量>",
            "`K-R48`：一次性 `ccm` 模式最后那一下 —— `exec` 用户要起的那个 agent\
             （`claude` / `codex` / `--launcher` 指定的任意命令），argv 直传**不过 shell**。\
             被起的程序名来自用户这一趟的参数，**按设计就不是字面量**。\
             它就是「用户敲 `cc` 想要发生的那件事」本身。",
            "缩性质",
            "同上那条：一次性模式整个搬走、或起 agent 不再由本进程 `exec` 的那天。",
        ),
        (
            "control/tmux_hook.rs",
            "tmux",
            "装 tmux hook（`set-hook -g`）。改的是 **tmux server 的运行期状态**，\
             不是用户既有数据；P4b 的零轮询判活靠它",
            "缩性质",
            "判活不再需要后端自己去装 hook 的那天（换成别的内核事件源，\
             或 hook 由用户侧一次性装好而后端只读）——那时这一条摘掉。",
        ),
        (
            "control/launch.rs",
            "tmux",
            "U8a-2b 平面 ②：建 tmux 会话 / 往已有会话 send-keys（argv 直传，不过 shell）。\
             改的是 **tmux server 的运行期状态** + 起一个用户自己要起的 claude 进程，\
             **不是后端进程自身写用户既有数据** —— 载荷落盘由那个 claude 进程负责，\
             与用户在终端里手敲同一条命令没有区别（D1 裁决的正例）",
            "缩性质",
            "「起会话」这条路整个搬出后端（或改成由 monitor 侧起、backend 只观测）的那天。\
             ⚠ 在那之前**不许**因为「反正已经登记了」而往这一条底下加第二种被起的程序。",
        ),
        (
            "control/kill.rs",
            "tmux",
            "F04a：`kill-session` / `kill-pane`（argv 直传；会话里还有别的 claude 窗格时只结束挂着那个 sid 的窗格）。\
             **破坏性**，但改的是 **tmux server 的运行期状态**，不是后端自己写用户既有数据；且必须先过 §34 三道门（Gate 3 = 单窗口）。\
             杀之前读 pane 根进程 pid（杀成之后按它从 cc-bus 名册认人 ⇒ `cc-kill` 经 `plugin/invoke.rs` 那条转调，写面记在那一条里）\
             走 `control/gate.rs` 那一处只读的 `list-panes`，本文件只这一处起进程",
            "缩性质",
            "§34 那三道门有任何一道被拆掉、或「杀会话」不再由后端发起的那天，\
             这一条要回来重判（它是本表里唯一**破坏性**的 tmux 动作）。",
        ),
        (
            "control/gate.rs",
            "tmux",
            "F03：§34 Gate 2 的探测（`display-message -p` 取 `@ccm_sid` + `#{session_id}`）。\
             **只读 tmux**，不改任何状态；登记在 control/ 是因为它是「能不能改这个会话」\
             这个决策的一部分（定框 C13）。\
             ⚠ **`K-R96`（09-12）：这一条从前还盖着第二处** —— P4f 续刀那条 \
             `list-sessions -F`（一次列全部会话）。用户 `R52` 裁定一之后，\
             判活改成向 `common/session_snapshot.rs` 那张快照发一次询问 \
             ⇒ 那处调用点搬去了那边。\
             今天两条子命令共用本文件**一处**起进程：探测那条 `display-message`，以及按 sid 找窗格那条只读的 \
             `list-panes -s -F '#{pane_id}…#{@ccm_sid}…'`（一个 tmux 会话里可以有几个 claude 窗格；`kill.rs` 杀之前读 pane pid 也走它）。",
            "缩性质",
            "这一条是**只读 tmux**，本来就落在收窄后的性质之内；\
             等哪天有一条判据能机检「这个起进程点只读」，它就该从受管例外里摘出去、不再占一格。",
        ),
        (
            "control/capture_pane.rs",
            "tmux",
            "**全 crate 唯一一处 `capture-pane`** —— \
             `tmux -u capture-pane -p -t <窗格>`（argv 直传不过 shell），\
             把名单里那个终端此刻那一屏的文本打到 stdout（终端预览 · 送前比指纹）。\
             **只读 tmux**：不 attach、不落 tmux buffer（`-p` = 打到 stdout，\
             换成落 buffer 就是改 tmux 状态了）、不写任何文件。\
             ⚠ **这张表的键分不出被调的是哪条 tmux 子命令**（同 `plugin/invoke.rs` 那条\
             自陈的 `K6b` 盲区）⇒ 光靠这一行**买不到「只读」**。\
             真正钉住它的是本文件的 [`super::capture_is_read_only`]：\
             值级（argv 逐元素）＋ 文本级（生产段零改状态动词）＋ 反向自检。\
             ⚠ **它只抓一次就返回**，轮询归 `K-R87`（`KR86D3`）—— 同一个模块里\
             「抓一次」被改成「循环抓」时，`no_timer_guard` 与 `the_capture_site_is_one_shot` \
             各红一条。",
            "缩性质",
            "这一条是**只读 tmux**，本来就落在收窄后的性质之内 —— 同 `control/gate.rs` 那条：\
             等哪天有一条**通用**判据能机检「这个起进程点只读」，它就该从受管例外里摘出去。\
             ⚠ [`super::capture_is_read_only`] **不是**那条通用判据：它只盖本文件这一处 \
             ⇒ 不许拿它去把 `control/gate.rs` / `common/session_snapshot.rs` 那两条也摘掉。\
             ⚠ 在那之前**不许**因为「反正已经登记了」而往这一条底下加第二个 tmux 子命令 —— \
             这一处的立身之本就是「抓一屏只有一处、而且只抓一屏」。\
             终端预览要的颜色（`-e`）与往回几行（`-S -N`）是同一条子命令上的两类只读旗（`capture_argv_with`，值级同样钉着）。",
        ),
        (
            "control/terminals.rs",
            "tmux",
            "终端管理 L1：逐窗格列名单（`list-panes -a -F`）· 列客户端（`list-clients -F`）· 问尺寸光标（`display-message -p`）三处只读，\
             外加经同一个命令构造器交给 `control/launch.rs` 的送字送键（`send-keys` / 粘贴）与 `control/gate.rs` 的身份探测 \
             —— 写的那一半只改 **tmux server 的运行期状态**（往用户明确要送的那个终端里打字），\
             不写任何文件；送之前过身份门（同 `launch` 的 `send-into`），只认名单里的终端。",
            "缩性质",
            "终端管理换成托管终端（宿主不再是 tmux）的那天，这一条随实现一起换掉。",
        ),
        (
            "control/terminal_follow.rs",
            "tmux",
            "终端实时预览：起一个 tmux **控制模式**客户端（`-C attach-session -f read-only,ignore-size`）挂在那个终端所在的会话上，\
             只读它报的输出事件当「画面变了」的信号（画面本身照旧经 `control/terminals.rs` 抓）；只读客户端不能往窗格里打字、\
             不改窗格尺寸，不写任何文件。订阅一停（退订 · 连接走 · 终端没了）就杀组收尸（`Child::stream` 放手即收）。",
            "缩性质",
            "终端管理换成托管终端（宿主不再是 tmux）的那天，这一条随实现一起换掉（托管进程自己推画面）。",
        ),
        (
            "common/session_snapshot.rs",
            "tmux",
            "`K-R96`（09-12）：**全 crate 唯一一处「一次列全部 tmux 会话」的探测点**\
             （`list-sessions -F '#{session_name}\t#{@ccm_sid}'`，argv 直传不过 shell）。\
             用户 `R52` 裁定一逐字：「Gate 能不能改成直接读那份快照. **可以** / \
             改为**向快照发一次询问, 快照更新一次**」⇒ 这一处就是那「一次询问」。\
             两个消费者：① Gate 判活（`bus-list` 的「这个总线成员今天还活着吗」——\
             **总线成员 ⊆ 活着的会话**，谁活着由身份空间说了算，不由 cc-bus 那份会过期的 \
             agents.tsv 说了算）；② `ccm` 铸名避让（`R52` 裁定二那张 hash 表）。\
             **只读 tmux**，不改任何状态。\
             ⚠ **一次调用列全部**，不是每个成员探一次（用户那台的总线有 86 行）。\
             ⚠ **这一条从 `control/gate.rs` 搬来，不是新增的面**：净处数不变。",
            "缩性质",
            "同 `control/gate.rs` 那条：只读，等有判据能机检「这个起进程点只读」时摘出去。\
             ⚠ 在那之前**不许**因为「反正已经登记了」而往这一条底下加第二个 tmux 子命令 —— \
             这一处的立身之本就是「列全部会话只有一处」，加一个子命令就要回来重判。",
        ),
        (
            // 搬家不是新增：`Command::new("tmux")` 从 `identity_tag.rs` 挪进它的 `door`
            //   （测试构建整个换成假 tmux，`INVARIANTS §48.3`）；`set-option` 那条 argv 仍在 `identity_tag.rs::set_sid`。
            "control/identity_tag/door.rs",
            "tmux",
            "`U-NP④`：`set-option @ccm_sid`（argv 直传）—— 把「这个 tmux 会话在跑哪个 sid」\
             这条事实打上去。接的是 `shared/ccm` 那条**每会话一条、每秒一轮**的身份 poller 的班\
             （「不要轮询」「ccm 做到必须走后端」）。改的是 **tmux server 的\
             运行期状态**，不是后端自己写用户既有数据（同 `tmux_hook`）。\
             ⚠ **探测不在这里**：它复用 `control/gate.rs` 那一处 `display-message`，\
             所以本文件只有这一处起进程 —— 刻意不让面变大",
            "缩性质",
            "会话身份不再靠 tmux 变量承载的那天（`K-P5` 若把身份脱离 tmux，这一条随之消失）。\
             ⚠ 那一件要来动本护栏时，先读 `K-G6` 立的这套做法，别顺手加白名单。",
        ),
        (
            "observe/session_terminals.rs",
            "tmux",
            "点 ↗ 时答「此刻谁在显示这个会话」：`list-clients -t <pane> -F '#{client_pid} #{client_activity}'`（argv 直传，\
             socket 取自那个进程自己的 `TMUX`）。**只读 tmux**，不改任何状态、不读不设标题；只在被问时起一次。",
            "缩性质",
            "「谁连着这个会话」改由别的事实源答（不再问 tmux）的那天。",
        ),
        (
            "plugin/invoke.rs",
            "<非字面量>",
            "`K-W1A`（08-26）：**插件通用调用口**里唯一一处起进程 —— argv 直传不过 shell，\
             期限由起子进程原语执行（调用方给秒数，到点杀整组）。\
             程序名是**查出来的路径**（PATH 里未必有用户级 bin 目录）⇒ 非字面量。\
             ⚠⚠ **本条从 `control/cc_bus.rs` 搬来，同轮把它的理由订正了**：\
             旧理由逐字写着「转调 `cc-list` / `cc-send`，**两条命令**共用」，而今天真实转调的是\
             **三条** —— 08-13 当天稍晚进来的 `cc-kill` **从来没被写进这条豁免理由**\
             （那个 commit 动了 8 个文件，本文件不在其中），而三条判据全绿：\
             键里的程序名是 `<非字面量>`，三条命令**共用同一个键** ⇒ 加第三条不会红。\
             ⇒ 这条登记覆盖的写面**今天是这样**：`cc-list` 只读；`cc-send` 写收件人的收件箱；\
             ★ `cc-kill` **是破坏性的**（杀会话 + 清名册 + 清台账 + 清那个 id 的状态）。\
             ★★ **`K-R113` 09-13：第四条到了，逐条记在这里** —— `cc-agents`（`bus-state` 的\
             spawn 台账那一半）。写面：**只读** —— 它读 `spawned.tsv`、回头去 `agents.tsv` \
             借登记时记下的 pane 根进程 pid，再问一次 `tmux has-session` / `display-message`，\
             **一个字节都不写**。⚠ 这一次**不是**「加了才发现漏登」：加它的那一刀被下面那条\
             `the_non_literal_spawn_key_still_covers_exactly_three_commands` 当场红\
             （它把条数钉成相等），是被逼回来读这一段之后才写下的 —— 那正是 `cc-kill` \
             漏登 13 天之后补的这道闸要买的东西。\
             四者都是**被起的那个进程**在写，与用户自己在终端里敲同一条命令没有区别\
             （同 `launch` 起 claude 的 D1 正例：收窄后的铁律管的是 **backend 进程自身**\
             不写用户既有数据）。\
             ★★ **ccbus-spawn 09-24：第五条到了，逐条记在这里** —— `cc-spawn`（`bus-spawn` 那条原语，\
             **今天写好了、没登记进帧面**，登记要 bump `BUILD_ID`）。写面：**起一个真 agent 会话**\
             （tmux ＋ claude/codex 进程，烧额度）· 登记进名册与 spawn 台账 · 预信任那个目录。\
             仍是**被起的那个进程**在写（它内部再经 `ccm`），与用户在终端里敲 `cc-spawn` 没有区别；\
             加它的那一刀同样被下面那条相等断言当场红、被逼回来读这一段之后才写下。\
             ★★ **SH1 09-26：第六条到了** —— `cc-log`（`bus-inbox` 读收件箱的尾巴）。写面：**只读**\
             （共享锁，锁文件仍是 `<inbox>.lock`，收件箱初次被看时它会建出那个空锁文件 —— 与 `cc-peek` 同一格）；\
             另外 `cc-list` / `cc-agents` 多了 `--tsv` 这一印法（仍只读）。同样被下面那条相等断言红出来之后写下。\
             ⚠ 这一处口从此是**通用**的：将来经它起的每一个插件，写面都落在这一条理由底下，\
             而这条键**分不出**是哪个插件 —— 加一种新的被调命令时必须回来重读这一段，\
             没有任何机检会替你想起（`K6b` 那一族，本条就是它的活体标本）。\
             ★★ **`K-W2D` 09-10：那句话点名的第二个使用者到了，逐条记在这里** —— \
             已删的那条按需拉取路经这一处口起**按需拉回来的代码全景 local_backend**。\
             （那条路整棵删了，这一段是历史，别照它去找符号。）\
             写面：那个本机后端是**我们自己出、我们自己校验哈希之后落盘**的二进制，\
             它今天只被问查询语义（`KR24`「命令面只暴露查询语义」没被放宽）\
             ⇒ **就本层调它的这条路而言写面是「只读」**；\
             ⚠ 而「它自己会不会在用户机器上落一个索引库」**是另一件事、归 PM/用户**\
             （`K-R2` PM 审计 §六㈡ 已端上去），本条**不替它回答**。\
             ⚠⚠ 而这正是这条键的病在**第二个使用者**身上复发：\
             键仍是 `<非字面量>`、仍分不出是哪个插件 ⇒ **加这一条不会红**，\
             是人回来读了这一段才写下的。下一个使用者同理。\
             ★★ **第三个使用者到了，逐条记在这里** —— 上游选择 \
             `accounts/iso.rs`经这一处口起**本机 `cc-acct-iso shellinit`**（`--acct-iso-shellinit`）。\
             写面：**只读**。今天这个使用者已不在：账号库改由后端自己建，那份外部脚本与 `accounts/iso.rs` 一起删了。\
             ⚠ 同样**加这一条不会红**（键仍是 `<非字面量>`；下面那条「恰好四条」只数 \
             `control/cc_bus.rs`）—— 是人回来读了这一段才写下的。",
            "缩性质",
            "键能分得出「哪个插件、哪条被调命令」的那天（今天是 `<非字面量>`，四条命令共用一个键）。\
             ⚠ 在那之前，本条的覆盖面由 [`super::g6_reach`] 那一格钉着：\
             `control/cc_bus.rs` 今天经它转调**恰好四条**，加第五条会红。",
        ),
        (
            "platform/shell/mod.rs", // `platform/shell.rs` 成了目录模块
            "sh",
            "`K-R55`（09-11）：**全 crate 唯一一处把命令串交给 POSIX shell 的适配层口**。\
             今天经它送出去的是 `observe/watcher.rs` 那两跳（`command -v tmux && tmux ls` \
             与 `tmux display-message -p`）—— 两处都**只读**，`sh -c` 是为了让 \
             `command -v` 解析 PATH。\
             ⚠ **这一条从 `observe/watcher.rs` 搬来**：先前那两跳各自裸写 `Command::new(\"sh\")`，\
             是 `K-R52` 挂在 `cfgless_guard` A2「真漏」堆上的头两条（签字栏逐字写着\
             「该进适配层（`K33` 裁定二），今天没进」）。\
             ⚠⚠ **这一处口从此是通用的**，而这条键**分不出**是谁在用它 —— \
             与 `plugin/invoke.rs` 那条 `<非字面量>` 同一族病：\
             将来经它起的每一条脚本，写面都落在这条理由底下，而加一个新用户**不会红**。\
             ⇒ 加新用户时必须回来重读这一段，没有任何机检会替你想起。\
             今天经它送出去的**恰好两处**，两处都在 `observe/watcher.rs`。",
            "缩性质",
            "同 `control/gate.rs` 那条：今天经它送的两处都是只读，\
             等有判据能机检「这个起进程点只读」时就该摘出受管例外。\
             ⚠ 在那之前**不许**因为「反正已经登记了」而往这一条底下加第三种用途 —— \
             要加就先回来把这一栏的「恰好两处」重新数一遍。",
        ),
        (
            "platform/shell/mod.rs", // `platform/shell.rs` 成了目录模块
            "powershell.exe",
            "别名方言问 PowerShell **内建别名**（`Get-Alias`，固定脚本、不吃用户输入、`-NoProfile` \
             `-NonInteractive`、不弹窗）—— 撞了的名字定义了也敲不到。从 monitor `shell_dialect.rs` 搬来：\
             规则进了那台后端，问的是**那台自己**的 PowerShell。**只读**，只在说 PowerShell 的那台（Windows）上起，\
             进程内缓存一次。调用方 `platform/shell/dialect.rs::ask_get_alias`（方言住适配层）。\
同一处口（`platform/shell/mod.rs::powershell_command`）又多两种**固定脚本**的用途：\
             ① 现问执行策略（`Get-ExecutionPolicy` 三行，只读；`aliases-read` 每份 5.1 的 `$PROFILE` 候选）；\
             ② **写一格用户设置**：`Set-ExecutionPolicy -Scope CurrentUser -ExecutionPolicy RemoteSigned`（HKCU 那一档，值写死），\
             只由 `powershell-policy-set` 跑 —— 界面在用户点了、确认了之后才发（「只在用户点击并确认后执行，不代改」）；\
             ③ 现问连接表与进程表（`Get-NetTCPConnection -State Established` ＋ `Win32_Process` 只取进程号 · 父进程号 · 名字 · 启动时刻四格，\
             只读、不吃入参；`terminal-processes`，点 ↗ 时问一次）。",
            "缩性质",
            "有别的办法读到那台 PowerShell 的内建别名表与执行策略（不起进程）的那天摘掉只读那两种。\
             ⚠ 写的那一种只许是上面那一句固定脚本，不许收界面给的策略值。",
        ),
        (
            "platform/shell/mod.rs",
            "pwsh.exe",
            "PowerShell 7 那一代（它的 `$PROFILE` 在 `Documents/PowerShell/`，执行策略与 5.1 分开存）：\
             同上一条的 ① ② 两种固定脚本（现问执行策略 · 用户确认后设当前用户 `RemoteSigned`），只在 7 的 profile 目录在时问。",
            "缩性质",
            "有别的办法不起进程就读到 PowerShell 7 的执行策略的那天摘掉只读那一种；\
             ⚠ 写的那一种只许是上一条那一句固定脚本，不许收界面给的策略值。",
        ),
        (
            "platform/shell/mod.rs",
            "<非字面量>",
            "问**起会话那个 shell** 的 `PATH`（`session_shell_path`）：用户的 `$SHELL` 起一次 `-l -i -c`，固定脚本只 `printf` 出 `PATH`，\
             不吃任何输入、有期限。被起的是用户自己的登录 shell，它读不读 / 写不写什么由用户的 rc 文件定；后端进程自身不写用户数据。\
             按后端生命周期只问一次（缓存）；只从足迹报告（`footprint-report` · `data-report`）判 agent 命令行装没装、起新会话框（`session-open-facts`）判这台能起哪几家时进来。",
            "缩性质",
            "有别的办法拿到用户登录环境的 `PATH` 而不起他的 shell 的那天摘掉。⚠ 脚本只许是打出 `PATH` 那一句。",
        ),
        // `relay/machine.rs` 那一行（远端起一个脱离的 `--relay`）摘了：中转只住常驻后端进程里，那一处起法随之删。
        (
            "control/session_open/mod.rs",
            "<非字面量>",
            "在 tmux 里后台起会话：**本后端这个二进制自己**（`current_exe`）当 ccm 跑界面那一行（同一个渲染器、只多 `--detach`），\
             argv 不过 shell、stdin 空、等它退出收话。被起的是 ccm 那一趟，它自己的起法（建 tmux 会话、键入、收尾）由本护栏照样管；\
             **不是**后端进程自身写用户既有数据。只从 `session-open`（tmux 那一形、要新建会话）· 换号重启 · 说一句恢复进来。",
            "缩性质",
            "起会话改由别的入口在后端起的那天摘掉。⚠ 不许往这一处底下加第二种用途 —— 它起的永远是本二进制的 ccm 那一趟。",
        ),
        (
            "accounts/sign_in.rs",
            "<非字面量>",
            "在 app 里登录一个号：起那一家的官方登录子命令（Claude：`claude auth login --claudeai`），程序名与参数来自适配层登记的那一形\
             （`AccountsFace::sign_in`）、argv 直传不过 shell；stdin 只写人从登录页粘回来的那一段码。令牌由官方客户端自己换、自己写进\
             **那个号自己的**配置目录（照它自己的样子用它自己的文件），后端一个字节不写。**不是**后端进程自身写用户既有数据。",
            "缩性质",
            "官方客户端改成别的登录方式（或后端改走别的路登录）的那天摘掉。",
        ),
        (
            "faces/quota_probe_face.rs",
            "<非字面量>",
            "`quota-probe`：起那一家的官方客户端报一次某个号的用量（Claude：`claude -p /usage`），程序名与参数来自适配层登记的那一形\
             （`agents::usage_of`）、argv 直传不过 shell、stdin 空、期限 30 秒。它只读那个号的登录、问一次上游的用量，不发模型请求；\
             工作目录是家里不出会话的那一个。**不是**后端进程自身写用户既有数据（官方客户端照它自己的样子用它自己的配置目录）。",
            "缩性质",
            "用量改由别的来源（如回包头里已经带着按模型的那几档）读得全、不再需要问官方客户端的那天摘掉。\
             ⚠ 不许往这一处底下加第二种用途 —— 它起的永远是适配层登记的那一条报用量的命令。",
        ),
        (
            "control/resident.rs",
            "<非字面量>",
            "远端那台的常驻后端由那台的 `--resident-ensure` 起：**本后端这个二进制自己**（`current_exe`）\
             以常驻载体起（stdio 全空、自成进程组、钥匙经文件交）。被起的就是流模式那一臂，它自己的写面由本护栏照样管；\
             **不是**后端进程自身写用户既有数据。只从 CLI 子命令 `--resident-ensure` 一条进来（monitor 经链路 capture 跑）。",
            "缩性质",
            "远端常驻后端改由别的东西起的那天摘掉。⚠ 在那之前**不许**往这一处底下加第二种用途 —— 它起的永远是本二进制的常驻载体。",
        ),
    ];

    /// ★ 生产段的每一处起进程都必须在 [`ALLOWED`] 里。
    #[test]
    fn every_process_spawn_in_production_is_registered() {
        // ★ **递归遍历，不是硬编码文件表**。
        //
        // 第一版是一张手写的 `include_str!` 清单。U8a-2b 新增 `control/launch.rs`（起 tmux）时
        // 实测：**它不在表里 ⇒ 这条护栏根本扫不到它**，加一个未登记的起进程点全绿。
        // 这正是本仓「扫描面画小了」那一族的第五次，而且是**我自己**在 D1 那轮埋的。
        // 同文件上方 `scan()` 早就因为同样的理由改成递归了（Phase G 审计），这里没跟。
        let src_dir = guard_core::addr::backend_src_root();
        let mut stack = vec![src_dir.clone()];
        let mut files: Vec<(String, String)> = Vec::new();
        while let Some(dir) = stack.pop() {
            for entry in std::fs::read_dir(&dir).expect("read src dir") {
                let path = entry.expect("dir entry").path();
                if path.is_dir() {
                    stack.push(path);
                    continue;
                }
                if path.extension().and_then(|e| e.to_str()) != Some("rs") {
                    continue;
                }
                let rel = path
                    .strip_prefix(&src_dir)
                    .unwrap_or(&path)
                    .to_string_lossy()
                    .replace('\\', "/");
                // 跳过本护栏文件自身：它的清单里逐字写着 `Command::new("tmux")` 这类字面量。
                if rel == "readonly_guard.rs" {
                    continue;
                }
                files.push((rel, std::fs::read_to_string(&path).expect("read rs file")));
            }
        }
        assert!(
            files.len() >= 20,
            "只遍历到 {} 个源文件 —— 遍历坏了，本断言在空转",
            files.len()
        );
        let mut found: Vec<(String, String)> = Vec::new();
        for (name, raw) in &files {
            let prod = guard_core::production_code(raw);
            let mut from = 0usize;
            while let Some(rel) = prod[from..].find("Child::new(") {
                let at = from + rel + "Child::new(".len();
                let tail = &prod[at..];
                // ★★**非字面量也要记**。
                //
                // 原来这里是「往后找第一个引号」——`Command::new(&bin)` 这种写法下，
                // 那个引号可能在**几十行之外**的某个无关字符串上，于是：
                // ① 记下来的"程序名"是假的；② 更坏的是它**可能与某条已登记的 pair 撞上**
                //   （同文件里已登记 `tmux`，而后面某处正好有 `"tmux"` 字面量）⇒ 静默放行。
                // ⇒ 认准紧跟其后的那个字符：是引号才当字面量，否则记成 `<非字面量>`，逼它单独登记。
                let prog = if tail.starts_with('"') {
                    match tail[1..].find('"') {
                        Some(e) => tail[1..1 + e].to_string(),
                        None => "<未闭合的字面量>".to_string(),
                    }
                } else {
                    "<非字面量>".to_string()
                };
                found.push((name.clone(), prog));
                from = at;
            }
        }
        // 处数不再写死：键集与 `ALLOWED` 两向相等（这一条管正向，下面 `every_registered_entry_is_backed_by_a_real_spawn_site` 管反向）。
        let unregistered: Vec<&(String, String)> = found
            .iter()
            .filter(|(f, p)| !ALLOWED.iter().any(|(af, ap, ..)| af == f && ap == p))
            .collect();
        assert!(
            unregistered.is_empty(),
            "这些起进程点没在受管例外清单里：{unregistered:?}\n\
             D1 把铁律收窄成「backend **进程自身**不许写用户既有数据」，代价是**必须逐条列举**\n\
             起进程的写面 —— 否则收窄就退化成「隔一层 exec 就绕过」。\n\
             把它加进 `ALLOWED` 并**写明它做什么、为什么不违反收窄后的铁律**。"
        );
    }

    /// ★ 生产段 `Command::new(` **只许**出现在起子进程原语里（期限必填 · 杀整组 · 自有环境无条件摘都在那里做）。
    /// 别处零命中；正控：原语那一份恰好一处（抽取没坏）。
    #[test]
    fn command_new_lives_only_in_the_spawn_primitive() {
        let src = guard_core::addr::backend_src_root();
        let mut hits: Vec<(String, usize)> = Vec::new();
        for (p, raw) in guard_core::scan_tree_excluding(&src, &["rs"], &[]) {
            let rel = p
                .strip_prefix(&src)
                .unwrap_or(&p)
                .to_string_lossy()
                .replace('\\', "/");
            let n = guard_core::production_code(&raw)
                .matches("Command::new(")
                .count();
            if n > 0 {
                hits.push((rel, n));
            }
        }
        assert_eq!(
            hits,
            vec![("platform/child.rs".to_string(), 1)],
            "生产段 `Command::new(` 只许住 `platform/child.rs`（恰好一处）。别处起子进程改走 `Child::new(…).run(期限)` / `detach` / `exec_replace`。"
        );
    }

    /// ★ 清单里不许有**幽灵条目**（登记了但生产段已经没有了）。
    ///
    /// 否则清单会越攒越松，上面那条的判据跟着变松。
    #[test]
    fn the_registry_has_no_ghost_entries() {
        for (f, p, why, cell, unlock) in ALLOWED {
            assert!(
                !why.is_empty(),
                "{f} 起 {p} 没写理由 —— 「逐条列举」列的是写面与理由，不是文件名清单"
            );
            // **枚举比对**（判据规范规则 3：闭集比对，不是子串）。
            assert!(
                super::g6_doctrine::is_cell(cell),
                "{f} 起 {p} 的第四栏是 `{cell}` —— 它不在 `§0a` 四情形表的闭集里。\n\
                 加一条受管例外**必须说得出它走的是哪一格**（`{}`）。\n\
                 ⚠ 「加白名单」**不是**那四格里的任何一格 —— 这条判据存在的全部理由\n\
                 就是那句承重的话：**放宽 ≠ 加白名单**。",
                super::g6_doctrine::cell_names().join(" / ")
            );
            assert!(
                unlock.trim().chars().count() >= 20,
                "{f} 起 {p} 没写**解锁条件**（实得 {} 字）—— 要写的是「什么条件满足之后\
                 这一条就能删」，不是「为什么现在不能删」。\
                 没有解锁条件的受管例外只是「永久豁免」的好听说法\
                 （形状照 `agent_locality_guard::AGENT_NAMED_WIRE_FIELDS`）。",
                unlock.trim().chars().count()
            );
        }
        let hook =
            guard_core::production_code(include_str!("../../src/backend/control/tmux_hook.rs"));
        // 🔴 `K-R55`（09-11）：这一处的住址从 `observe/watcher.rs` 换成了
        //    `platform/shell.rs` —— 起 shell 那一跳搬进适配层（`K33` 裁定二）。
        //    ⚠ 换的是**住址**，不是判据：核的仍是「登记的那一条在生产段里真的找得到」。
        let shell =
            guard_core::production_code(include_str!("../../src/backend/platform/shell/mod.rs"));
        assert!(
            hook.contains("Child::new(\"tmux\")"),
            "清单登记了 tmux_hook 起 tmux，但生产段里找不到了 —— 幽灵条目"
        );
        assert!(
            shell.contains("Child::new(\"sh\")"),
            "清单登记了 platform/shell.rs 起 sh，但生产段里找不到了 —— 幽灵条目"
        );
    }

    /// ★★ `KY1′-b`：**全表反向核** —— `ALLOWED` 的每一条都得对得上一处真的起进程。
    ///
    /// # 它补的是上面那条的哪一个洞（实测出来的，不是设想）
    ///
    /// [`the_registry_has_no_ghost_entries`] 只做两件事：断言每条 `why` 非空 +
    /// **硬编码**核 `tmux_hook.rs` / `watcher.rs` 那两条。**它不核其余五条。**
    /// ⇒ 搬家时如果**加**了 `plugin/invoke.rs` 的登记却**忘了删**
    /// `control/cc_bus.rs` 那条：`ALLOWED` 变 8 条、扫到的仍是 9 处、`unregistered` 仍空
    /// ⇒ **三条判据全绿，而表里躺着一条幽灵**。
    ///
    /// 本条把「硬编码那两条」换成**全表**：登记表与扫描结果做**双向**对账 ——
    /// 正向（每处起进程都在表里）由上面那条管，反向（每条登记都对得上起进程）由本条管。
    ///
    /// # 它**仍然**不检查什么（射程说清，别读大一格）
    ///
    /// - 不检查那条 `why` **说得对不对**（`K6b` 原样保留 —— 上面那条 `ALLOWED` 里
    ///   `cc-kill` 漏了 13 天就是这个洞的活体标本，本条也逮不住它）；
    /// - 不检查一条登记**覆盖了几处** —— 键是 `(文件, 程序名)`，同一个键下加第二种被调命令
    ///   不会红（同一个洞的另一面）。
    #[test]
    fn every_registered_entry_is_backed_by_a_real_spawn_site() {
        let src_dir = guard_core::addr::backend_src_root();
        // 与上面那条**分开走一遍树**：那边还要分类、比数，这边只回答「表里这一条今天还在吗」。
        let mut stack = vec![src_dir.clone()];
        let mut found: Vec<(String, String)> = Vec::new();
        while let Some(dir) = stack.pop() {
            for entry in std::fs::read_dir(&dir).expect("read src dir") {
                let path = entry.expect("dir entry").path();
                if path.is_dir() {
                    stack.push(path);
                    continue;
                }
                if path.extension().and_then(|e| e.to_str()) != Some("rs") {
                    continue;
                }
                let rel = path
                    .strip_prefix(&src_dir)
                    .unwrap_or(&path)
                    .to_string_lossy()
                    .replace('\\', "/");
                if rel == "readonly_guard.rs" {
                    continue;
                }
                let prod =
                    guard_core::production_code(&std::fs::read_to_string(&path).expect("read rs"));
                let mut from = 0usize;
                while let Some(at) = prod[from..].find("Child::new(") {
                    let i = from + at + "Child::new(".len();
                    let tail = &prod[i..];
                    let prog = if tail.starts_with('"') {
                        match tail[1..].find('"') {
                            Some(e) => tail[1..1 + e].to_string(),
                            None => "<未闭合的字面量>".to_string(),
                        }
                    } else {
                        "<非字面量>".to_string()
                    };
                    found.push((rel.clone(), prog));
                    from = i;
                }
            }
        }
        assert!(
            found.len() >= 5,
            "只扫到 {} 处起进程 —— 遍历坏了，本断言在空转",
            found.len()
        );
        let ghosts: Vec<String> = ALLOWED
            .iter()
            .filter(|(af, ap, ..)| !found.iter().any(|(f, p)| f == af && p == ap))
            .map(|(af, ap, ..)| format!("{af} 起 {ap}"))
            .collect();
        assert!(
            ghosts.is_empty(),
            "这些登记在生产段里**已经找不到对应的起进程点**了：{ghosts:?}\n\
             ⇒ 搬走了/删掉了就**同轮把登记摘掉**。留着的后果不是「多一行没用的字」——\n\
             它会让下一个人以为那个文件还在起进程，而真正的那一处在别处、\n\
             理由却还挂在旧地址上（`ALLOWED` 里那条漏了 `cc-kill` 13 天，就是这么来的）。"
        );
    }
}

/// 🔴 `K-R86`（09-13）：**那一处新的抓屏起进程点「只读」这件事，逐元素钉住。**
///
/// # 为什么 `spawn_registry` 那一行买不到它
///
/// [`spawn_registry::ALLOWED`] 的键是 `(文件, 起什么程序)` —— 它**分不出被调的是哪条
/// tmux 子命令**。同一份文件里把 `capture-pane -p` 改成别的动词、或多塞一个旗，
/// 键一个字都不变 ⇒ **那条登记照常绿**。这不是新发现的洞：`ALLOWED` 里
/// `plugin/invoke.rs` 那条自己就写着这句话（`K6b` 那一族，它是活体标本），
/// 而 `control/gate.rs` 那条的 `unlock` 栏逐字写着「等哪天**有一条判据能机检
/// 「这个起进程点只读」**」——本模块就是那条判据的**第一格**。
///
/// # 它盖多大（⚠ 别读大）
///
/// **只盖 `control/capture_pane.rs` 这一份文件。** 它不是通用判据
/// ⇒ **不许**拿它去把 `control/gate.rs` / `common/session_snapshot.rs` 那两条
/// 「只读 tmux」的受管例外摘掉 —— 那两处今天仍然只有 `ALLOWED` 那一行看着。
///
/// # 两层 ＋ 一条反向自检，缺一不可
///
/// - **值级**：这一处发出去的 argv **逐元素**就是那条只读形（顺序也算）。
///   它挡的是「顺手把 `-p` 换成落 buffer」「顺手多塞一个旗」。
/// - **文本级**：生产段里**不出现**任何会改 tmux 状态的动词。
///   它挡的是「另起一条 argv、绕过上面那个构造器」。
///   ⚠ 文本级是**黑名单**，列不全（同 `platform/cfgless_guard` 的信号表）——
///   所以它是第二道，不是第一道；每一条针都带「为什么它算改状态」。
/// - **反向自检**：逐条针各喂一个合成样本，同一把尺子必须逮到。
///   没有它，上面那条「零命中」是空真。
#[cfg(test)]
mod capture_is_read_only {
    use guard_core::production_code;

    /// 被测对象：那一处抓屏的**生产段**。
    fn site() -> String {
        production_code(include_str!("../../src/backend/control/capture_pane.rs"))
    }

    /// 会改 tmux 状态的动词 / 旗，`(针, 为什么它算改状态)`。
    ///
    /// 🔴 **运行时拼**，不写字面量：本模块与被扫的文本今天不在同一份文件里
    /// （那是刻意的，见 `ratchet_guard.rs` 头注），但同族坑本仓记过四次，照纪律拼。
    fn mutating_verbs() -> Vec<(String, &'static str)> {
        vec![
            (
                format!("send{}", "-keys"),
                "往别人的键盘缓冲里打字 —— 破坏性，而且它有自己的门（`control/launch.rs`）",
            ),
            (
                format!("kill{}", "-session"),
                "杀会话 —— 全 crate 只许 `control/kill.rs` 过完三道门再做",
            ),
            (
                format!("new{}", "-session"),
                "建会话 —— 改 tmux server 的运行期状态",
            ),
            (
                format!("set{}", "-option"),
                "写 tmux 变量（`@ccm_sid` 那一族）—— 只许 `control/identity_tag.rs`",
            ),
            (
                format!("set{}", "-hook"),
                "装 hook —— 只许 `control/tmux_hook.rs`",
            ),
            (
                format!("run{}", "-shell"),
                "让 tmux 去跑一条命令 —— 那是把执行权交出去，不是抓一屏",
            ),
            (
                format!("respawn{}", "-pane"),
                "重起 pane 里的进程 —— 破坏性",
            ),
            (
                format!("pipe{}", "-pane"),
                "把 pane 的输出接到一条命令上 —— 常驻副作用，而且不是「抓一次」",
            ),
            (
                format!("load{}", "-buffer"),
                "写 tmux 的粘贴缓冲 —— 改 tmux server 的状态",
            ),
            (
                format!("copy{}", "-mode"),
                "把 pane 切进 copy-mode —— 改的是 pane 的模式，用户会看见",
            ),
            (
                format!("\"{}\"", "-X"),
                "copy-mode 的命令派发口（`-X cancel` 那一族）—— 抓一屏用不到它",
            ),
        ]
    }

    /// 一把尺子：某段文本里命中了哪几条改状态动词。
    fn hits(text: &str) -> Vec<String> {
        mutating_verbs()
            .into_iter()
            .filter(|(n, _)| text.contains(n.as_str()))
            .map(|(n, _)| n)
            .collect()
    }

    /// ★ 抽取器自检：剥完还得剩下真代码，否则下面几条是空转。
    #[test]
    fn the_site_is_really_being_read() {
        let prod = site();
        assert!(
            prod.len() > 800,
            "剥完 `control/capture_pane.rs` 只剩 {} 字节 —— 读错文件或剥过头了，\
             本模块此刻是无效的",
            prod.len()
        );
        guard_core::assert_no_test_code("capture_is_read_only", &prod);
        assert!(
            prod.contains(&format!("Child::{}", "new(\"tmux\")")),
            "那一处起进程不见了 —— 本模块声称盖的东西已经不在它声称的地方了"
        );
    }

    /// ★★ 正题①（**值级**）：这一处发出去的 argv 逐元素就是那条只读形。
    ///
    /// `INVARIANTS §49`（tmux 打印通道必须是 UTF-8）的邻居：`capture-pane` 实测不在那一条的人群里，这里钉着的第一个元素（那个旗）是额外的；本条的主住址仍是只读铁律。
    #[test]
    fn the_argv_this_site_emits_is_read_only_element_by_element() {
        let argv = crate::control::capture_pane::capture_argv_with("=某会话:", false, 0);
        assert_eq!(
            argv,
            ["-u", "capture-pane", "-p", "-t", "=某会话:"],
            "抓屏那一处的 argv 变了。三条硬约束逐条：\n\
             · `-u` 必须排在子命令**之前**（`K-R12`：放后面是 rc=1 + unknown flag，\n\
               而那个响错会被判法读成「抓不到」）；\n\
             · 子命令必须是抓屏那一条 —— 换成任何会改状态的动词，\n\
               `readonly_guard` 的 `ALLOWED` **不会红**（那张表的键分不出子命令），\n\
               本条是唯一会红的；\n\
             · `-p` 必须是「打到 stdout」—— 换成落 tmux buffer 就是改 tmux 状态。"
        );
        assert_eq!(argv[4], "=某会话:", "目标那一格被改写了");
        // 预览那一形：同一条子命令，只多 `-e`（带颜色）与 `-S -N`（往回几行）两类只读旗；都不要时与上面逐字相同。
        assert_eq!(
            crate::control::capture_pane::capture_argv_with("=某会话:", true, 5),
            [
                "-u",
                "capture-pane",
                "-p",
                "-e",
                "-S",
                "-5",
                "-t",
                "=某会话:"
            ]
        );
    }

    /// ★★ 正题②（**文本级**）：生产段里不出现任何会改 tmux 状态的动词。
    #[test]
    fn the_site_names_no_state_changing_tmux_verb() {
        let bad = hits(&site());
        assert_eq!(
            bad,
            Vec::<String>::new(),
            "抓屏那一处的生产段里出现了会改 tmux 状态的动词：{bad:?}\n\
             ⇒ 要么有人绕过 `capture_argv_with` 另起了一条 argv，要么那个构造器被改了。\n\
             这一处的立身之本就是「只读」——`readonly_guard::ALLOWED` 那一行的理由\n\
             逐字写着它不改任何 tmux 状态；写不成真话就别写。"
        );
    }

    /// ★ 反向自检：**那把尺子真的会咬人**。
    ///
    /// 没有它，上面那条「零命中」与「针全拼错了」长得一模一样（
    /// 零违例要先证够得到）。逐条喂，不是喂一条了事 —— 一条针拼错也要报出来。
    #[test]
    fn the_verb_scan_actually_bites_on_every_needle() {
        let verbs = mutating_verbs();
        assert!(
            verbs.len() >= 8,
            "改状态动词表只剩 {} 条 —— 被掏瘪了，正题此刻在空转",
            verbs.len()
        );
        for (needle, _) in &verbs {
            let sample = format!("let _ = tmux_argv({needle}, target);");
            assert_eq!(
                hits(&sample),
                vec![needle.clone()],
                "针 {needle:?} 喂自己的样本都咬不到 —— 它此刻是一条死针"
            );
        }
        assert!(
            hits("let out = tmux(\"-u\", \"capture-pane\", \"-p\", \"-t\", t);").is_empty(),
            "干净样本被误咬 —— 假阳会训练人绕过判据，比没有判据更坏"
        );
    }

    /// ★★ `KR86D3`：**这一处只抓一次就返回。**
    ///
    /// 两个数一起断：起进程**恰好一处**（多一处 = 有人在这里连着抓第二屏），
    /// 循环构件**零处**（那是「靠节拍反复问」的形态，归 `K-R87`，不许在这里做掉）。
    ///
    /// ⚠ 与 `no_timer_guard` **不重**：那一条扫的是全 crate 的「会让线程自己醒来的构件」
    /// （`sleep` / `interval` / `Duration::from_secs`），认不出一个**忙循环**；
    /// 本条盖的是这一份文件里「循环」这个形状本身。
    #[test]
    fn the_capture_site_is_one_shot() {
        let prod = site();
        let spawns = prod.matches(&format!("Child::{}", "new(")).count();
        assert_eq!(
            spawns, 1,
            "抓屏那一处的生产段里有 {spawns} 处起进程 —— 该恰好 1 处。\n\
             多出来的那一处多半就是「再抓一次」：轮询归 `K-R87`（`KR86D3`），\n\
             本模块只出原语。"
        );
        let loops: Vec<String> = [("lo", "op {"), ("whi", "le "), ("fo", "r ")]
            .iter()
            .map(|(a, b)| format!("{a}{b}"))
            .filter(|w| prod.contains(w.as_str()))
            .collect();
        assert_eq!(
            loops,
            Vec::<String>::new(),
            "抓屏那一处的生产段里出现了循环构件：{loops:?}\n\
             ⇒ 「抓几次」是**调用方**的事（`KR86D3` 逐字：只出原语，不出轮询）。"
        );
    }
}

// **这段话原先给的那个理由，今天已经不成立了**。原文逐字留档
// （它下过一次结论、被引过，所以留着）：
//   「这条再导出**不是为了好看**：`g6_doctrine` 的声明行必须逐字是 `mod g6_doctrine {`，
//    不许写成 `pub(crate) mod` —— `guard_core::test_module_ranges` 认「测试模块」的判据是
//    「属性的下一行以 `mod ` 打头、以 `{` 收尾」，写成 `pub(crate) mod` 就**认不出来**，
//    整段测试代码会留在生产段里被别的守卫扫。〔实测：写成 `pub(crate) mod` 时
//    `every_backend_file_strips_clean` 当场红，逐字「剥完仍残留 2 个测试属性」。〕
//    ⇒ 跨模块可见性只能走这条再导出。」
//
// **那个前提被 `K-R75`（09-12）拆掉了**：剥法不再按字面前缀认 `mod `，改成**按形状**
// 先剥可见性修饰（`guard_core::strip_visibility`）⇒ `pub(crate) mod` 今天认得出来，
// 上面那句「只能走这条再导出」**是假的**。
// ⇒ 今天这条再导出**不是被逼出来的**，它只是一个写法；`mod g6_doctrine {` 那一行
//   **也不再必须逐字长成那样**。
//
// 🔴 **`K-R77`（09-12）把那道再导出拆了**（——
//   `K-R76` 当时刻意不自批，`§0d` 写死「只改那段过期的话」）。
//   今天盘上：`g6_doctrine` 直接写成 `pub(crate) mod`，跨模块的取名走
//   `crate::readonly_guard::g6_doctrine::{cell_names, is_cell}`（唯一的跨模块消费者是
//   `no_timer_guard`），**顶层那一行再导出没有了**。
//   ⚠ **别再写回来**：这一族今天有闸了 —— monitor 侧
//   `structural_scan.rs::the_cfg_test_reexport_detour_stays_extinct` 是一条**恒零棘轮**，
//   语料面含本文件所在的这棵树，写回一行当场红并点名这份文件。

/// **`§0a` 四情形表的代码形态** —— 一条护栏的人群与性质对不上时该怎么处置。
///
/// # 承重的那句话：**放宽 ≠ 加白名单**
///
/// 本仓铁律 16 写着「一条判据的白名单被反复放宽 ⇒ 它没有真实消费者，删掉它」。
/// 而护栏这一族的常见情形**方向相反**：性质仍然要，只是人群画错了。
/// 两者必须分开处置，而分开的判准就是这张表。
/// ⇒ 判据 [`adding_a_whitelist_entry_is_not_one_of_the_four_cells`] 钉的正是这一点：
/// **「加白名单」不是这四格里的任何一格。**
///
/// # 它落在这里、而不是注释里，是刻意的
///
/// 派工单原写「落进护栏头注」，`Bx` 顶回来了：「写在**源码注释**里的
/// 自证等于埋掉了」，而 `ratchet_guard.rs` 头注另有一条 ——
/// 判据不许在自己的文本里找到自己。⇒ 四情形表必须是**可被判据读的结构**，
/// 形状照 `agent_locality_guard::AGENT_NAMED_WIRE_FIELDS`（四元组 + 长度地板）。
///
/// # 它挡不住什么（如实登记，别读成证明）
///
/// - 挡不住**抄一个格名过关**：它钉的是「说得出走的是哪一格」，不是「真的想过」。
/// - 挡不住**既有条目的覆盖面悄悄变大**：`ALLOWED` 里 `plugin/invoke.rs` 那个
///   `<非字面量>` 键覆盖三条命令、第三条漏登 13 天而三条判据全绿 —— 本条逮不到它，
///   逮它的是 [`g6_reach`] 的反例表（那一格钉的是「恰好三条」）。
#[cfg(test)]
pub(crate) mod g6_doctrine {
    /// 四情形**闭集**。五元组：
    /// `(格, 什么时候落在这一格, 正确处置, why——为什么是这个处置, unlock——这一格自己什么时候要被重新裁定)`
    pub(crate) const CELLS: &[(&str, &str, &str, &str, &str)] = &[
        (
            "收窄人群",
            "性质还要，而人群画大了 —— 判据扫到了不该扫的",
            "把人群收窄到性质上",
            "这是**收紧**不是放宽：收完之后判据说的话变少了，而它说的每一句都是真的。\
             `K-G2` 已有先例。误红消失换来的不是「护栏变松」，是「护栏不再撒谎」。",
            "哪天性质本身扩了（真的要管更大那一片），这一格就不适用，改走 `补齐人群`。",
        ),
        (
            "补齐人群",
            "性质还要，而人群画小了 —— 判据漏了该扫的",
            "补齐人群，**同一刀做完**",
            "`K-G2` 的教训逐字：分开做必漏。补人群那一刀必须与「确认新人群不误红」同轮，\
             否则第二刀永远排在后面，而第一刀已经把护栏的名声用掉了。",
            "补到「人群 == 性质」那天这一格就关了；在那之前每补一次都要重新量误红面。",
        ),
        (
            "重做或删",
            "性质今天已经守不住 —— 盘上有一个形状相同、未经放宽就通过了的反例",
            "★★ **先判它今天守住了什么**，再裁重做还是删掉",
            "这一格是本表最要紧的一格：**不许在一个拦不住的东西上讨论怎么放宽它**。\
             「放宽之后没红」既可能是放宽对了，也可能是它本来就不响 —— \
             两者在终端上一模一样，而只有先摆出反例才分得开。",
            "反例被修掉、或人群补齐到能覆盖它之后，这一条回到 `收窄人群` / `补齐人群`。",
        ),
        (
            "缩性质",
            "性质本身该缩 —— 它当初是一个更小性质的**粗近似**",
            "**单独论证**，并写清**缩掉的那一半从此归谁**",
            "`D1` 把后端那条写盘铁律从收窄前那句绝对话（原文见 `src/doc/INVARIANTS.md` §41.6 的\
             「原措辞」，本文件刻意不抄）缩成「backend 进程自身不许改动用户既有数据」就是这一格：\
             缩掉的那一半（间接写）归**被起的那个程序**，而代价是那条路必须逐条登记 —— \
             登记表就是「归谁」的落点。**只缩不写归属 = 把那一半丢了。**",
            "缩掉的那一半有了自己的判据（或那条路整个消失）之后，登记表随之摘掉。",
        ),
    ];

    /// 闭集比对（**枚举，不是子串** —— 判据规范规则 3）。
    pub(crate) fn is_cell(tag: &str) -> bool {
        CELLS.iter().any(|(c, ..)| *c == tag)
    }

    /// 报错文案要印出闭集全体，否则撞上的人得回来翻源码。
    pub(crate) fn cell_names() -> Vec<&'static str> {
        CELLS.iter().map(|(c, ..)| *c).collect()
    }

    /// ★ 闭集是**恰好四格**，每格的 why / unlock 都有长度地板。
    #[test]
    fn the_doctrine_is_a_closed_set_of_exactly_four_cells() {
        assert_eq!(
            CELLS.len(),
            4,
            "四情形表变成 {} 格了 —— **相等断言，不是地板**。\n\
             加一格 = `§0a` 那张表被改了，那是件计划级的事，不是顺手能加的；\n\
             少一格 = 有人把某一种处置从判准里拿掉了，那正是要有人看一眼的时刻。",
            CELLS.len()
        );
        let mut names = cell_names();
        let before = names.len();
        names.sort_unstable();
        names.dedup();
        assert_eq!(before, names.len(), "四情形表里有重名的格：{names:?}");
        for (cell, when, how, _, unlock) in CELLS {
            assert!(
                !when.trim().is_empty() && !how.trim().is_empty(),
                "`{cell}` 这一格没写「什么时候落在这一格」或「正确处置」"
            );
            assert!(
                unlock.trim().chars().count() >= 20,
                "`{cell}` 没写**这一格自己什么时候要被重新裁定**（实得 {} 字）—— \
                 一张没有解锁条件的判准表，用不了几轮就会变成装饰",
                unlock.trim().chars().count()
            );
        }
    }

    /// ★★ 承重的那一条：**「加白名单」不是四格里的任何一格。**
    ///
    /// 这条是本模块存在的理由的机器形态。它今天绿，而它的牙在**将来**：
    /// 谁想把「加白名单」当成一种合法处置塞进闭集，这一条当场红。
    #[test]
    fn adding_a_whitelist_entry_is_not_one_of_the_four_cells() {
        for forbidden in [
            format!("加{}", "白名单"),
            format!("白名单{}", "放宽"),
            format!("{}{}", "放", "宽"),
        ] {
            assert!(
                !is_cell(&forbidden),
                "`{forbidden}` 被当成了四情形表里的一格。\n\
                 ★★ **放宽 ≠ 加白名单**：铁律 16 说的是「白名单被反复放宽 ⇒ 删掉它」，\n\
                 而这四格治的是**方向相反**的病（性质还要、人群画错了）。\n\
                 把「加白名单」写成一种处置，等于把这张表要分开的两件事又合回去了。"
            );
        }
        // 反向：闭集里那四个名字必须**真的**被认出来，否则上面那条靠「什么都不是格」恒真。
        for (cell, ..) in CELLS {
            assert!(
                is_cell(cell),
                "闭集自己的 `{cell}` 都认不出来 —— `is_cell` 坏了，上面那条在空转"
            );
        }
    }
}

/// **本护栏今天真能拦住的形状全表 + 一个今天就通过了的反例。**
///
/// # 为什么先列全表，再谈放宽（`§0c 裁五`）
///
/// 摸底那三条反例**全部**落在「它本来就拦不住」那一侧 —— 存量、未经放宽就通过。
/// ⇒ **不许在一个拦不住的东西上讨论怎么放宽它。** 本模块交两样：
/// ① 它今天真能拦住的形状**全表**（逐形一刀读数，分母用**相等断言**钉住）；
/// ② 一个今天在盘上、形状与它声称要拦的相同、却通过了的**反例**（形态照 `§0b-5` 的形态二：
///    表 + 每条理由 + 幽灵检查 —— 它买的是「**射程被写下来且不许腐烂**」）。
///
/// ⚠ **形态二挡不住什么**：它不会因为**新出现**一个反例而红（那需要能自动发现反例，做不到）。
#[cfg(test)]
mod g6_reach {

    /// 反例表的语料。**住址与语料在这里对死** —— 加一行反例而不接语料，这里当场 panic。
    fn source_of(rel: &str) -> &'static str {
        match rel {
            "control/cc_bus.rs" => include_str!("../../src/backend/control/cc_bus.rs"),
            other => panic!("反例表里出现了没接语料的住址：{other}"),
        }
    }

    /// **今天在盘上、形状与本护栏声称要拦的相同、而它放过了的那一处。**
    ///
    /// `(住址, 生产段里的片段, why——形状为什么对得上、而它为什么看不见, unlock)`
    const KNOWN_PASSING_COUNTEREXAMPLES: &[(&str, &str, &str, &str)] = &[(
        "control/cc_bus.rs",
        "run(\"cc-kill\"",
        "生产段起一个外部程序去**删除 / 覆盖用户既有数据**：被起的那个脚本会覆盖两份名册、\
         删掉一个 id 的收件箱与它的状态文件。**形状对得上** —— backend 若把同一件事写成\
         `remove_file` / `rename` 那两个动词，命名空间那一条当场红。\
         **而它今天通过**：命名空间那一条的人群是本 crate 生产段里点名文件系统命名空间的地方，起进程一个都不匹配（`spawn_registry` 头注逐字承认「它不认 \
         `Command` / `spawn`」）。⇒ 这不是漏洞，是**一次没有落到判据上的裁定**（`D1` 缩性质），\
         而缩掉的那一半由 `ALLOWED` 接着 —— 那张表的键**分不出被调命令**，所以本条另钉一格：\
         经那个键转调的命令**恰好三条**。",
        "命名空间那一条能顺着起进程点读到被起程序的写面那天（或那条路改成后端自己写、\
         从而落回命名空间那一条射程内）——那时这一条摘掉，并回来重判 `ALLOWED` 还需不需要。",
    )];

    /// ★ 反例仍在盘上、仍然通过；同形的**直写**仍然会红。
    ///
    /// ★★ 专属陷阱在这一格的答案：这条路**未经任何放宽就已经通过**
    /// —— 命名空间那一条（[`tests::only_the_fs_platform_module_names_the_filesystem`]）只认文件系统那几个命名空间的文本，
    /// 起进程一个都不匹配。
    /// ⇒ 它属于「**它本来就拦不住**」那一侧，而不是「放宽之后没红」。
    /// ⚠ 我**没有**去查历史上它有没有因为别的原因红过 —— 上面那句是对**今天的判据**说的，不是对历史说的。
    /// 两句话在终端上长得一样，分开它们的正是下面第二个断言 —— 同形直写会红。
    #[test]
    fn the_counterexample_is_still_on_the_board_and_still_passes() {
        assert_eq!(
            KNOWN_PASSING_COUNTEREXAMPLES.len(),
            1,
            "反例表从 1 条变成 {} 条了 —— 加反例是好事，但要连它的语料与断言一起加",
            KNOWN_PASSING_COUNTEREXAMPLES.len()
        );
        for (rel, frag, _, unlock) in KNOWN_PASSING_COUNTEREXAMPLES {
            let prod = guard_core::production_source(source_of(rel));
            assert!(
                prod.contains(frag),
                "反例 `{rel}` 里找不到 `{frag}` 了 —— 修掉了就**同轮摘登记**，\
                 并回来重判本护栏的射程（这正是形态二要买的东西）"
            );
            assert_eq!(
                super::tests::fs_namespace_hits(&prod),
                vec![],
                "反例 `{rel}` 今天**红了** —— 那说明本护栏的射程变了，\
                 这张表说的话已经不成立，回来重判"
            );
            assert!(
                unlock.trim().chars().count() >= 20,
                "`{rel}` 没写解锁条件 —— 没有解锁条件的反例登记会一直躺着"
            );
        }
        // ★ 分开「它本来就拦不住」与「放宽之后没红」：**同一件事直写**，命名空间那一条当场红。
        assert!(
            !super::tests::fs_namespace_hits(
                "std::fs::remove_file(bus.join(\"inbox\").join(name))?;"
            )
            .is_empty(),
            "同形的直写都不红了 —— 那就不是「人群够不着」，是判据本身坏了"
        );
    }

    /// ★ 那条 `<非字面量>` 键今天覆盖了**恰好四条**被调命令。
    ///
    /// ⚠ **判据的名字里那个「three」是历史，不是今天的读数** —— 09-13（`K-R113`）加了
    /// 第四条 `cc-agents`，条数由下面那个 `vec![…]` 现算着钉。改名要动的是判据名，
    /// 而判据名一改就要同轮跑 `pb doc`（生成区会连带打红）⇒ 那笔账这里不改，
    /// **如实留在这里，不假装它读得通**。
    ///
    /// # 它钉的是 `ALLOWED` 逮不到的那一面（活体标本，不是设想）
    ///
    /// `spawn_registry` 的三条判据用的键是 `(文件, 程序名)`，而 `plugin/invoke.rs` 那条的
    /// 程序名是 `<非字面量>` ⇒ **同一个键下加第二种被调命令不会红**。
    /// 08-13 加进来的那条**破坏性**命令因此漏登 13 天，三条判据全绿。
    /// 本条把「今天是三条」钉成**相等**：加第四条就必须回来重读那条豁免理由。
    #[test]
    fn the_non_literal_spawn_key_still_covers_exactly_three_commands() {
        let prod = guard_core::production_source(source_of("control/cc_bus.rs"));
        let mut cmds: Vec<&str> = Vec::new();
        // 只读三条经 `read_via("…"` 转调（同一个起进程口）。
        for opener in ["run(\"", "run_as(\"", "read_via(\""] {
            let mut from = 0usize;
            while let Some(k) = prod[from..].find(opener) {
                let at = from + k + opener.len();
                let end = prod[at..]
                    .find('"')
                    .expect("被调命令的字面量没有闭合 —— 抽取坏了");
                cmds.push(&prod[at..at + end]);
                from = at + end;
            }
        }
        cmds.sort_unstable();
        cmds.dedup();
        assert_eq!(
            cmds,
            vec![
                "cc-agents",
                "cc-kill",
                "cc-list",
                "cc-log",
                "cc-send",
                "cc-spawn"
            ],
            "经 `plugin/invoke.rs` 那个 `<非字面量>` 键转调的命令变了：{cmds:?}\n\
             ⇒ 回 `ALLOWED` 里 `plugin/invoke.rs` 那条**重读它的豁免理由**，\n\
             把新命令的写面写进去。**不许只改这个断言。**\n\
             （那条键分不出是哪个插件，所以这一格是它唯一的机器提醒。）"
        );
        let invoke_keys = super::spawn_registry::ALLOWED
            .iter()
            .filter(|(f, ..)| *f == "plugin/invoke.rs")
            .count();
        assert_eq!(
            invoke_keys, 1,
            "`ALLOWED` 里 `plugin/invoke.rs` 有 {invoke_keys} 条键 —— \
             本条的前提是「那几条命令共用**一个**键」，键数变了这格就该重判"
        );
    }
}

/// **「这个能力今天在生产里零使用，而『零』本身要被钉住、接线那天要故意变红」这一族的指路判据。**
///
/// # 族名是重新起的（`§0c 裁二`）
///
/// 派工单原叫它「某符号**生产调用数** = 0」，照那个名字在盘上找不到成员：
/// 现有那两条钉的一条是「生产段不发某个 mode 串」、一条是「赋值处逐字是空构造 + 反向能力锚」，
/// **都不是符号调用数**。按它们真正断言的东西重新命名之后，这一族才找得到。
///
/// # 为什么它与 `*_guard.rs` 家族是两个不相交的人群
///
/// 守卫家族（现打 15 份）里的零断言**全部**是「违规列表为空」（fail-closed，**永远**该是 0）；
/// 而这一族是「**今天**是 0，接线那天该红」（fail-open-once-wired）。
/// 两者结构长得一模一样（都是某个 `is_empty()`），**语义相反** ——
/// 这就是为什么来找的人翻遍守卫文件一无所获。
///
/// # 走「指路判据」，不搬家（`Bx` 的建议，`§0c 裁二` 采纳）
///
/// 搬家要同轮改上百处硬编码的路径字面量 —— 那是**用封装换编译单元**。
/// 这里只留一张表：把成员与**两笔今天连判据都没有的欠账**收在一处，让找的人搜得到。
#[cfg(test)]
mod g6_staged_zero {
    /// `(住址, 符号或判据名, 被钉的那个「零」逐字是什么, 今天钉它的判据（`—` = 今天没有）, 本 crate 够不够得着)`
    const STAGED_ZERO: &[(&str, &str, &str, &str, &str)] = &[
        (
            // 判据改名（原名说的「U8c-3 删不得」那半随 TS 兜底一族删了，只剩 `create-or-attach` 这一半）、
            //   住址跟着判据走（剖分之后它就住 `tests/` 这份，旧住址是它当年的生产段宿主）。
            "tests/frontend/shell/launch_wire_f07_main_path_tests.rs",
            "the_create_or_attach_mode_is_sent_only_by_the_ccm_container_path",
            "生产段**不发** `create-or-attach` 这个 mode 串（运行时拼串防自指，配抽取器自检）",
            "自己就是那条判据",
            "跨 crate",
        ),
        // `agents/codex/parse.rs` 的 turn-end 那两行**摘了**：函数连同判据删了（零读者判据那次）。
        // `plugin/probe.rs::negotiate` 那一行**摘了**：接线那天到了 ——
        //   `control/panorama.rs`（代码全景小程序的能力协商）是它第一个跨文件生产消费者，
        //   下面那条判据当场红，正是这张表承诺的「接线那天该红」。条数 7 → 6。
        // 🔴 **这一条是刚刚变成零的，不是一直是零。**
        //
        // 「文件管理器该不该能改 `~/.claude` 里的东西. **可以.**」
        // ⇒ 那道「两道栅栏宽窄不同」按丙（统一）裁，统一到**窄的那一档**
        // ⇒ `control/files_write.rs` 的两道围栏从 `is_inside_tree`（拒**整棵 `~/.claude*` 树**）
        //   换成 `is_session_record_path`（只拒那几份具体的会话文件），
        //   而 `is_inside_tree` 的**唯一生产消费者**就是那两处。
        //
        // ⚠ **为什么不删它**（本仓「不为旧配置留兼容」那条纪律这里不适用）：
        //   它是「`~/.claude*` 那个星号包含哪些树」这条知识的**唯一住址**，
        //   而那条知识本身没有过期 —— 过期的是「写侧要不要用它」这个**产品裁决**。
        //   删掉它等于把一条仍然正确的布局知识连同那次裁决一起丢掉，
        //   下一次有人要「整棵树」那一档（比如给另一家 agent 立围栏）就得重写一遍。
        // ⚠ 而它留着**必须有人数着**：一个零消费者的 `pub fn` 与一个「已经接上的」
        //   长得一模一样，这条登记就是那个数。谁把它接回生产路径，
        //   下面那条判据当场红 —— 而那一红正是「有人重新裁了这一格」该出现的地方。
        (
            "agents/claudecode/paths.rs",
            "is_inside_tree",
            "backend 生产段里**跨文件消费者 0 个**（09-23 之前是 `control/files_write.rs` 两处）",
            "—",
            "本 crate",
        ),
    ];

    /// `(相对路径, 原文, 生产段)`。
    ///
    /// 🔴 **两件一起改。**
    ///
    /// · **射程**：人群从「只有 `src/backend`」扩到**两棵树**。理由：`STAGED_ZERO` 那张表
    ///   第二栏登记的是**判据的名字**，而判据这一轮整批搬进了 `<repo>/tests/backend/`
    ///   ⇒ 只扫生产树时「那条判据还在不在」这一格必然报「找不到」
    ///   （现打红：`wire.rs` 里已经找不到 `the_backend_can_already_discover_homes_…`）。
    /// · **自摘**：原来靠 `scan_tree!` 的 `file!()`。本文件住 `tests/backend/`，
    ///   在旧射程（生产树）里本来就不在人群中，所以自摘失效一直**无害**；
    ///   射程一扩到 `tests/backend` 就**立刻有害**了 ——
    ///   本表里逐字写着那些名字，不摘就人人都「有人指向它」（记过五次的恒绿形状）。
    ///   ⇒ 排除明写成名单，`scan_tree_excluding` 摘不到就 panic。
    ///
    /// ⚠ 住址口径：生产树的文件相对 `src/backend`（表里旧行**一个字没改**），
    ///   测试树的文件写成 `tests/backend/…`（仓根相对）—— 两种靠前缀就分得开。
    fn backend_files() -> Vec<(String, String, String)> {
        let src_root = guard_core::addr::backend_src_root();
        let tests_root = guard_core::addr::backend_tests_root();
        // 中转 crate 的单测住 `tests/comms/outward/`（生产那一半经后端的人群声明收进 `src_root` 的人群）。
        let comms_tests = guard_core::addr::comms_tests_root();
        let mut out = Vec::new();
        for (root, excluded, prefix) in [
            (&src_root, &[] as &[&str], ""),
            (&tests_root, &["readonly_guard.rs"], "tests/backend/"),
            (&comms_tests, &[], "tests/comms/outward/"),
        ] {
            for (path, src) in guard_core::scan_tree_excluding(root, &["rs"], excluded) {
                let rel = format!(
                    "{prefix}{}",
                    path.strip_prefix(root)
                        .unwrap_or(&path)
                        .to_string_lossy()
                        .replace('\\', "/")
                );
                // 生产段按「住哪棵树」分流：测试树的文件贡献 0 字节生产代码
                // （否则下面那条「跨文件生产消费者为 0」会把判据夹具算成消费者）。
                let prod = guard_core::addr::production_side_of(&path, &src);
                out.push((rel, src, prod));
            }
        }
        out.sort();
        out
    }

    /// ★ 正题一：表里每条的住址今天真的在，且那个名字真的还在那份文件里（**幽灵检查**）。
    #[test]
    fn the_staged_zero_registry_has_no_ghost_entries() {
        assert_eq!(
            STAGED_ZERO.len(),
            2,
            "这一族的登记表从 2 条变成 {} 条了 —— 加成员是好事，\
             但每加一条都要说清「被钉的那个零逐字是什么」与「接线那天为什么该红」",
            STAGED_ZERO.len()
        );
        let files = backend_files();
        assert!(
            files.len() >= 60,
            "只扫到 {} 个后端源文件 —— **遍历坏了**，本条此刻在空转",
            files.len()
        );
        let mut out_of_reach = 0usize;
        for (rel, name, zero, pinned_by, reach) in STAGED_ZERO {
            assert!(
                zero.trim().chars().count() >= 10 && !pinned_by.trim().is_empty(),
                "`{rel}` / `{name}` 没写清被钉的那个「零」，或没写今天有没有判据钉它"
            );
            if *reach == "跨 crate" {
                out_of_reach += 1;
                continue;
            }
            assert_eq!(
                *reach, "本 crate",
                "`{rel}` / `{name}` 的第五栏是 `{reach}` —— 只有 `本 crate` / `跨 crate` 两种"
            );
            let owner = files
                .iter()
                .find(|(f, ..)| f.as_str() == *rel)
                .unwrap_or_else(|| panic!("登记的住址 `{rel}` 今天不在后端树上了 —— 幽灵条目"));
            assert!(
                owner.1.contains(name),
                "`{rel}` 里已经找不到 `{name}` 了 —— 删掉了就**同轮摘登记**"
            );
        }
        assert!(
            out_of_reach <= 1,
            "有 {out_of_reach} 条标着 `跨 crate` —— 本判据住 backend crate（它刻意不属于 workspace），\
             够不着 `src/frontend/shell`。标的条数超过 1 就说明这张表的家选错了，\
             该按 `Bx` 说的另立一处两侧都够得着的落点。"
        );
    }

    /// ★ 正题二：**那个「零」今天真的是零** —— 标着「今天没有判据」的那几条，由本条替它们钉住。
    ///
    /// ★★ 这一条**接线那天会故意变红**，那正是它存在的理由：
    /// 谁把 `codex` 的 turn-end 或插件协商接上生产路径，就必须回到这里把那一条摘掉，
    /// 而不是让一个「本来说好是零」的事实悄悄变成非零。
    #[test]
    fn the_symbols_registered_as_zero_have_no_cross_file_production_consumer() {
        let files = backend_files();
        let mut checked = 0usize;
        let mut offenders: Vec<String> = Vec::new();
        for (rel, name, _zero, pinned_by, reach) in STAGED_ZERO {
            if *pinned_by != "—" || *reach != "本 crate" {
                continue;
            }
            checked += 1;
            for (f, _raw, prod) in &files {
                if f.as_str() == *rel {
                    continue;
                }
                if prod.contains(name) {
                    offenders.push(format!("  {f} 里出现了 {name}"));
                }
            }
        }
        assert_eq!(
            checked, 1,
            "只核了 {checked} 条「今天没有判据」的欠账（登记时是 3 条，\
             波 5 ㈢ 09-23 加了 `is_inside_tree` 之后是 4 条，`negotiate` 接上生产摘掉之后回到 3 条，\
             codex turn-end 那两个连同判据删了之后剩 `is_inside_tree` 1 条）—— \
             筛选条件与登记表脱节了，本条在空转"
        );
        assert!(
            offenders.is_empty(),
            "下面这些符号**已经有跨文件的生产消费者了**，而它们还登记在「今天零使用」这一族里：\n{}\n\n\
             ⇒ 这不是坏事，这是**接线发生了**。处置：把它从 `STAGED_ZERO` 里摘掉，\n\
             并给它补一条真正的判据（它现在承载生产行为了）。\n\
             ★ 本条红在「说好是零的东西变成非零」那一刻 —— 那正是这一族要买的东西。",
            offenders.join("\n")
        );
    }
}

/// **钉住另外两道护栏的「性质行 / 人群行」各自只有一句。**
///
/// # 为什么钉在这里，而不是各自的文件里
///
/// `ratchet_guard.rs` 头注逐字给过理由：判据**不许与被扫的文本同住一个文件**
/// （它会在自己的注释里找到自己 ⇒ 恒绿）。
/// ⇒ 本模块钉 `platform/fallback_guard.rs`；本文件自己那两行由 `platform/fallback_guard.rs::g6_scope_pins` 钉 —— 两道互钉
/// （零定时器那一道换成了 clippy，没有性质行可钉了）。
///
/// # 它挡的是哪一种事故
///
/// `readonly_guard` 与 `no_timer_guard` 今天各有**两句**性质声明，一宽一窄，
/// 而判据只兑现窄的那句 ⇒ 宽的那句是假话，且下游件在逐字引用它。
/// 钉「只有一句」挡的正是这个：想再写一句更好听的，就得先把旧的那句处理掉。
///
/// ⚠ **它钉不住的**：换个措辞说同一件事它看不见（`ratchet_guard` 登记过这条同族边界）。
/// 它也**不检查那一句说得对不对** —— 那是语义判断，机器认不了。
#[cfg(test)]
mod g6_scope_pins {
    /// 两行的标记。**运行时拼**，免得本文件被自己数进去。
    fn marks() -> (String, String) {
        (
            format!("//! - **它守的{}**", "性质是"),
            format!("//! - **它扫的{}**", "人群是"),
        )
    }

    /// `(住址, 语料, 收窄前那句假话的承重词——今天必须零命中, why_empty)`
    fn pinned() -> Vec<(&'static str, &'static str, Vec<String>, &'static str)> {
        vec![(
            "platform/fallback_guard.rs",
            include_str!("platform/fallback_guard.rs"),
            Vec::new(),
            "这一道的性质行今天**只有一句**、也没有收窄史 —— 它自曝的是射程\
                 （只看 `platform/`、等价改写绕得过），那是诚实登记，不是过期的绝对话。\
                 ⇒ 没有承重词要钉，如实写在这里而不是留一个空表。",
        )]
    }

    /// ★ 正题：每份文件里，性质行与人群行**各自恰好一行**，且各自有内容。
    #[test]
    fn each_guard_states_its_property_and_its_population_exactly_once() {
        let (prop, popu) = marks();
        for (rel, src, _forbidden, _why_empty) in pinned() {
            for (what, mark) in [("性质行", &prop), ("人群行", &popu)] {
                let hits: Vec<&str> = src
                    .lines()
                    .filter(|l| l.starts_with(mark.as_str()))
                    .collect();
                assert_eq!(
                    hits.len(),
                    1,
                    "`{rel}` 里以 `{mark}` 打头的{what}有 {} 行（应恰好 1 行）。\n\
                     **少了**：`KG62` 要的两行不在场，读的人没有一句可引的话；\n\
                     **多了**：同一道护栏有两句性质声明 —— 这正是这里逮到的那个病\n\
                     （一宽一窄，判据只兑现窄的那句，而宽的那句被下游逐字引用）。",
                    hits.len()
                );
                let body = hits[0].trim_start_matches(mark.as_str());
                assert!(
                    body.trim().chars().count() >= 20,
                    "`{rel}` 的{what}只有 {} 字 —— 一句写不下去的性质声明等于没写",
                    body.trim().chars().count()
                );
            }
        }
    }
}

/// **失败信封只有一份**：CLI 面与帧面的失败都经 `wire::detail::Failed` 序列化（`{code, message, detail, data?}`）；
/// 后端生产段里手拼 `{"code": …, "message": …}` 的地方 == 零处。键名是契约（`src/doc/IPC-PROTOCOL.md`），
/// 手拼的一份改了键名门禁看不见 ⇒ 不许再长出来。按文本认，不跑命令；退出码不在射程里。
#[cfg(test)]
mod error_envelope_registry {
    /// 信封那两个键。运行时拼进两种写法，见 [`key_forms`]。
    const CODE_KEY: &str = "code";
    /// 见 [`CODE_KEY`]。
    const MESSAGE_KEY: &str = "message";

    /// 一个键的两种写法：普通字面量，与**转义进另一层串**里的那一形（`eprintln!("{{\"code\":…")`）。
    fn key_forms(name: &str) -> [String; 2] {
        [format!("\"{name}\""), format!("\\\"{name}\\\"")]
    }

    /// 一行里 `code` 这个键出现几次（两种写法都算）—— [`envelope_sites`] 的纯函数那一半，正控量的是同一把尺子。
    fn code_key_hits(line: &str) -> usize {
        key_forms(CODE_KEY)
            .iter()
            .map(|k| line.matches(k.as_str()).count())
            .sum()
    }

    /// 全树一趟：扫了几份文件，与每一处信封的 `(文件相对路径, 那一行逐字)`。
    fn envelope_sites() -> (usize, Vec<(String, String)>) {
        let root = guard_core::addr::backend_src_root();
        let (mut scanned, mut out) = (0, Vec::new());
        for (f, src) in guard_core::scan_tree!(&root, &["rs"]) {
            scanned += 1;
            let rel = f
                .strip_prefix(&root)
                .unwrap_or(&f)
                .to_string_lossy()
                .replace('\\', "/");
            for line in guard_core::production_code(&src).lines() {
                if code_key_hits(line) > 0 {
                    out.push((rel.clone(), line.trim().to_string()));
                }
            }
        }
        (scanned, out)
    }

    /// ★★ 正题：后端生产段里手拼的失败信封 == 零处（人群非空）。
    #[test]
    fn no_hand_built_error_envelope_is_left_in_backend_production() {
        let (scanned, sites) = envelope_sites();
        assert!(scanned > 0, "一份 `.rs` 都没扫到 —— 遍历塌了，零命中在空转");
        let hits: Vec<String> = sites.iter().map(|(f, l)| format!("  {f}: {l}")).collect();
        assert_eq!(
            hits,
            Vec::<String>::new(),
            "这几处手拼了失败信封 —— 改由 `wire::detail::Failed` 序列化，键只许一份：\n{}",
            hits.join("\n")
        );
    }

    /// ★ 正控：两种写法都认得出，而 `code` 这个词本身不算（裸词会把 `let code = …` 全数进来）。
    #[test]
    fn the_needle_bites_on_both_writings_and_not_on_the_bare_word() {
        let plain = format!("json!({{{CODE_KEY:?}: c, {MESSAGE_KEY:?}: m}})");
        assert_eq!(code_key_hits(&plain), 1, "普通写法认不出来：{plain}");
        let escaped = format!("eprintln!(\"{{\\\"{CODE_KEY}\\\":1}}\")");
        assert_eq!(code_key_hits(&escaped), 1, "转义写法认不出来：{escaped}");
        let bare = format!("let {CODE_KEY} = out.status.{CODE_KEY}();");
        assert_eq!(
            code_key_hits(&bare),
            0,
            "裸词 `{CODE_KEY}` 被数进来了：{bare}"
        );
    }
}

/// 〔`K-R2` 09-04，兑现 `K-W2D` `KW2D5` 的一半〕**依赖 crate 的写面：从「判据看不见」
/// 变成「有人签过字」。**
///
/// # 洞在哪 —— 本文件的人群行自己承认的那一句
///
/// 人群行逐字写着人群比性质**小**，而它列的第一条就是「不含依赖 crate 的写」。
/// ⇒ 一个依赖 crate 在**它自己的**代码里写盘 / 起进程，本护栏两层判据一层都不会响：
/// 它扫的是**本 crate 源码文本**里那三个命名空间的调用。
///
/// 🔴 **这不是假想形态**：`creds-core` 是本清单上的直接依赖，它自己的 `perm.rs` 里
/// 就有写面（`create_private` 建私有文件）。
/// 它们今天进不了本 crate 的依赖树，靠的是**那个 feature 没开** ——
/// 而在本模块之前，盘上没有任何东西钉着「那个 feature 不许开」。
///
/// # 本模块**不补人群**（归 PM，按逐条过），它买的是另一格
///
/// 补人群 = 去扫依赖 crate 的源码树，那是另一件事的规模。本模块只做一件机器判得了的事：
/// **清单上每一条依赖都得有一行签字**，新加一条而没签字 ⇒ 当场红。
/// 签字里写清「有没有写面 · 依据是什么 · 落哪一档」。
///
/// # 它买不到什么（逐条写明，别读大一格）
///
/// - **不检查那行签字说得对不对** —— 同 [`spawn_registry`] 那条登记过的边界。
///   它钉的是「说得出来」，不是「真的想过」。
/// - **分母是清单上直接声明的那几条**，传递依赖不在里面 ⇒ 一条依赖的依赖在写盘，本条一声不吭。
///   ⚠⚠ **订正（09-04 同轮，别读成「看不见」）**：本条初版逐字写的是
///   「那个分母要起子进程去问 cargo 才数得出来」—— **那句话是假的**：`Cargo.lock` 就在盘上、
///   纯文本、解析得动（同一天另一道判据现打就是读它来判「谁链了引擎」）。
///   ⇒ 准确的说法是**口径选择**，不是能力边界：签字要签在「**我们自己写下的那一条依赖**」上，
///   那才是有人做过决定的地方；锁文件那一面是另一张表、另一件事。
///   〔这一条自己就是本仓最高频那族的活体：**把「我没做」写成了「做不到」**。〕
/// - **按「crate 名 + 段」认**：同一条依赖换版本 / 换 feature 集合不会红。
///   唯一的例外是那条**有写面**的（它的签字前提由本模块单独一条判据钉着）。
/// - 它读的是**本半自己那份清单**（编译期 `include_str!`），不跨半边、不新增跨界编译边。
#[cfg(test)]
mod g6_dependency_signoff {
    /// 本 crate 的清单。**编译期读**，而且是本半自己那一份。
    const MANIFEST: &str = include_str!("../../src/backend/Cargo.toml");

    /// 清单里**带依赖的段**，逐段登记（**相等**对拍，不是子集）。
    ///
    /// 一整段依赖溜出分母是这一族最省事的漏法：`[build-dependencies]` 与
    /// `[target.'…'.dependencies]` 在隔壁那份清单上真的有，本清单今天没有 ——
    /// 哪天有了，本模块要先出声。
    const DEPS: &str = "[dependencies]";
    const DEV_DEPS: &str = "[dev-dependencies]";
    /// 只在远端 musl 版才链的那一段（今天只有分配器 `mimalloc`，判据 `allocator_guard`）。
    const MUSL_DEPS: &str = "[target.'cfg(target_env = \"musl\")'.dependencies]";
    const DEP_SECTIONS: &[&str] = &[DEPS, DEV_DEPS, MUSL_DEPS];

    /// 判档：**闭集**，名字只有这一处住址（表里与签字行都引这几个常量，不写第二遍字面量）。
    const MEASURED_WRITES: &str = "已量·有写面";
    const MEASURED_CLEAN: &str = "已量·未见写面";
    const UNMEASURED: &str = "未量·靠用法签字";
    /// 〔`K-R79` 09-12〕**第四档。**
    ///
    /// 它与 [`MEASURED_WRITES`] 的差别**不是程度，是那句「凭什么进不来」有没有**：
    /// 前者的签字要答「它的写面进不了发布二进制」，本档答的是「**进得来，而且就是要它写**」。
    ///
    /// 🔴 **它不是「例外」那一档** —— 件计划逐字点名的失效方向就是「加一档例外而不写它的边界，
    /// 那等于把闭集拆了」。本档的边界是**机检的**，见 [`BOUNDARY_TAG`]。
    const MEASURED_WRITES_ON_PURPOSE: &str = "已量·有写面·就是要它写";

    /// 第四档的**边界记号**：落这一档的签字里必须逐字出现它，后面跟一条**本文件里真存在**的
    /// 判据名（反引号括起来）。三处对拍住
    /// [`the_purposeful_write_verdict_names_a_boundary_judge_that_really_exists`]。
    ///
    /// 为什么边界要长成「点名一条判据」而不是「写一句话」：本表其余各列的地板都是**字数**，
    /// 而字数挡不住「写得挺像那么回事」。一条判据名挡得住 —— 它要么在盘上，要么不在。
    const BOUNDARY_TAG: &str = "边界判据：";

    /// `(判档, 什么时候落在这一档, 这一档自己的解锁条件)`。
    ///
    /// 解锁条件按**档**给一次，不在每条签字里抄一遍 —— 抄一遍就会漂（本仓 E12）。
    const VERDICTS: &[(&str, &str, &str)] = &[
        (
            MEASURED_WRITES,
            "读过它的源码、并在里面找到了文件系统变更或起进程的调用，\
             **而它的写面进不了发布二进制** —— 凭什么进不来，要写在签字里。\
             〔`K-R79` 09-12 补一句射程：**进得来**的那一形不落本档，\
             落 `已量·有写面·就是要它写`〕",
            "那条「凭什么进不来」的前提没了（feature 开了 / 调用路接上了）⇒ 这一条回来**重签**，\
             不是改个字。而那个前提本身必须有一条判据钉着，否则这一档只是好听的说法",
        ),
        (
            MEASURED_WRITES_ON_PURPOSE,
            "读过它的源码、找到了写面，**而发布二进制里就是要它写** —— \
             它的写面就是后端要用的那个功能本身（远端部署 / 远端文件操作那一族）。\
             ⇒ 本档**不问**「凭什么进不来」，改问「**它写的那一面由谁划边界**」",
            "🔴 **本档自带边界，它不是例外**：签字里必须逐字出现 `边界判据：` \
             ＋ 一条**本文件里真存在**的判据名（反引号括起来），由 \
             `the_purposeful_write_verdict_names_a_boundary_judge_that_really_exists` 三处对拍。\
             降档 / 摘掉的条件是「backend 不再需要它写」—— 那时它回到 `已量·有写面`\
             （并重新答「凭什么进不来」），或者直接从清单上摘掉",
        ),
        (
            MEASURED_CLEAN,
            "仓内 crate —— 整棵 `src` 按本护栏那两张模式表加起进程点现打，命中 0 处\
             （量具是 `evidence/` 下那份 `.py`，交回里给了住址与量于哪个提交）",
            "它长出第一处写面那天 —— 而那一刻**有一条常驻判据会自动红**：\
             `the_clean_verdict_is_re_measured_on_the_tree_every_run`（`K-R29` 09-06 立）。\
             它每一趟拿本护栏那两张模式表加起进程点，重扫本档里**仓内**那几棵 crate 的 `src`；\
             那份清单**现算自 `SIGNED`**，不手抄第二份。\
             ⚠ 它**扫不了**的：判档落在本档、而清单那一行没有 `path =`（源码不在树里）的那种 —— \
             那时它**点名报红**，不静默跳过。`未量·靠用法签字` 整档也不在它的射程里",
        ),
        (
            UNMEASURED,
            "**没读它的源码。** 签字依据只有「backend 在它身上的用法不需要它自己写盘」——\
             那是**用法**判断，不是对它源码的读数",
            "有人真去读了它的源码（或它进了一次真的依赖审计）⇒ 那一条升到已量的两档之一；\
             在那之前如实标着「未量」，不许因为「看起来不会写」就升档",
        ),
    ];

    /// 判档闭集比对（**枚举，不是子串** —— 判据规范规则 3）。
    fn is_verdict(tag: &str) -> bool {
        VERDICTS.iter().any(|(v, ..)| *v == tag)
    }

    /// 报错文案要印出闭集全体（**现算**，不写基数）。
    fn verdict_names() -> Vec<&'static str> {
        VERDICTS.iter().map(|(v, ..)| *v).collect()
    }

    /// 有写面那一档今天唯一的成员，与它那个被关着的 feature。
    const GATED_CRATE: &str = "creds-core";

    /// **签字表**：`(crate 名, 段, 判档, 签字——它在后端里做什么 · 凭什么落这一档)`。
    ///
    /// 谁签的：`实@09-04`（实现方）。整表一个签字人，所以不占一列 ——
    /// 哪天有第二个人往里加行，那一列再立。
    const SIGNED: &[(&str, &str, &str, &str)] = &[
        (
            "acct-core",
            DEPS,
            MEASURED_CLEAN,
            "账号记录变换（纯数据），与 monitor 共用同一份；仓内 crate，现打 0 处写面",
        ),
        (
            // 同一个依赖在测试构建里多开 `fixtures`（跨生产者对拍夹具，`accounts_query_tests.rs` 用）。
            "acct-core",
            DEV_DEPS,
            MEASURED_CLEAN,
            "同上一行那个仓内 crate，测试构建多开 `fixtures` feature（多出来的只是一张静态夹具表 ＋ 渲染它的纯函数），0 处写面",
        ),
        (
            "cmd-wire",
            DEPS,
            MEASURED_CLEAN,
            "帧命令两端对上的形状（机器名 `Machine` · typed 命令的形状指纹表 · 账号库那几条的入参与成品），纯数据；仓内 crate，现打 0 处写面",
        ),
        (
            // 同一个依赖在测试构建里多开 `ts`（入参 · 成品的 TS 派生，形状指纹与生成类型用）。
            "cmd-wire",
            DEV_DEPS,
            MEASURED_CLEAN,
            "同上一行那个仓内 crate，测试构建多开 `ts` feature（多出来的只是类型的 TS 派生），0 处写面",
        ),
        (
            "cmd-wire",
            DEV_DEPS,
            MEASURED_CLEAN,
            "同一个 crate 在测试档多开 `ts`（typed 命令线上类型的 TS 派生 · 形状指纹），只多一个 codegen 派生；现打 0 处写面",
        ),
        (
            "comms-outward",
            DEPS,
            MEASURED_CLEAN,
            "中转（通信层面 B，搬字节那一层）：仓内 crate，现打生产段 0 处文件系统调用、0 处起进程、0 处绑口 \
             —— 钥匙文件 · 绑口 · 期限都在本 crate 的 `relay/`（宿主）里；它只写交给它的 socket",
        ),
        (
            "comms-outward",
            DEV_DEPS,
            MEASURED_CLEAN,
            "同上一行那个仓内 crate，测试构建多开 `test-support`（只把内部件再导出给组合判据），0 处写面",
        ),
        (
            "ssh-dial",
            DEPS,
            MEASURED_WRITES_ON_PURPOSE,
            "拨号核心（后端与手机同一份）：仓内 crate，**住本 crate 目录底下**（`ssh-dial/`）⇒ 它的源码就在本护栏的人群里\
             （`src/backend` 下全部 `.rs`），与本 crate 自己的代码同一套判准。本机写面现打 0 处：认下的 host key 交回宿主\
             （`dial/known_hosts.rs`，经启动时装的写句柄 `known_hosts::Writer` 记）。**远端**写面就是它的本职（`russh-sftp` 那一格搬到这里）⇒ 就是要它写。\
             谁划边界：只住它的 `sftp.rs` 一份、规矩由宿主装（本后端装部署那两处，`dial/mod.rs::host`）、每处先过 `fenced_remote`。\
             边界判据：`the_remote_write_lives_in_exactly_one_file_and_its_root_is_exactly_staging`",
        ),
        (
            "ssh-dial",
            DEV_DEPS,
            MEASURED_WRITES_ON_PURPOSE,
            "同上一行那个仓内 crate，测试构建多开 `test-support`（测试档不许出网那句断言 · 拨了必被拒的本机口 · 合成 SFTP 服务端），\
             多出来的 0 处写面；远端写面同上一行。\
             边界判据：`the_remote_write_lives_in_exactly_one_file_and_its_root_is_exactly_staging`",
        ),
        (
            "deploy",
            DEPS,
            MEASURED_CLEAN,
            "装 / 升级远端后端的判定与编排（与 ssh-dial 并列、后端与手机同一份）：仓内 crate，住本 crate 目录底下（`deploy/`）⇒ \
             源码在本护栏的人群里。本机写面现打 0 处（要放的字节由宿主经 `Load` 读来，本 crate 不碰本机文件系统）；\
             远端那几下（建目录 · 放 ＋ 读回 · 删残件）全经 `ssh-dial/sftp.rs` 的原语，写面记在 ssh-dial 那一行",
        ),
        // `branch-core` 那一行摘了：它收进本 crate 的适配层（`agents/claudecode/branch.rs`；IO 只有一处只读的目录枚举，0 处写面）。
        (
            GATED_CRATE,
            DEPS,
            MEASURED_WRITES_ON_PURPOSE,
            "第三方 API key 的唯一住址（装它的类型 / 落盘格式 / 权限判断）。\
             ★ **它自己有一处写面**：`perm.rs` 的 `create_private`（建一个只给本人的新文件），在那个 feature 后面。\
本清单**开了**它：远端那台机器上的 key 只能由那台的后端写\
             （上游选择自己的状态文件，`accounts/upstream_select/file_face.rs` 经 `platform::fs::Own` 写），\
             「出生即只给本人」只有 `create_private` 这一份实现 ⇒ 就是要它写。\
             先前「编译器保证写不了」那一格换成下面这条判据：写半边在本 crate 生产段的引用处 == 那一份。\
             边界判据：`the_credentials_write_half_is_reached_only_from_the_account_file_face`",
        ),
        (
            "copy-core",
            DEPS,
            MEASURED_CLEAN,
            "对外文案表的 Rust 取文口（编译期内嵌 `src/shared/copy/table.json` ＋ 具名占位符替换，纯字符串变换）；\
             与 monitor、creds-core 共用同一份；仓内 crate，现打 0 处写面",
        ),
        // `gate-core` 那一行摘了：它收成本 crate 的模块 `control/gate_rules.rs`（命名空间那一条那条判据数它的写面）。
        (
            "guard-core",
            DEV_DEPS,
            MEASURED_WRITES,
            "源码扫描型判据的共用剥法与住址（`addr`）。现打 0 处写盘、1 处起进程：`addr::tracked_under` 起一个只读的 \
             `git ls-files` 问索引。进不了发布二进制：它只在各包的 `[dev-dependencies]`，本行的段与清单逐条对拍 \
             （`every_dependency_this_manifest_declares_carries_a_signature`），挪进 `[dependencies]` 就红",
        ),
        (
            "libc",
            DEPS,
            UNMEASURED,
            "只用 pidfd 那三样（清单那段注释逐字），零 feature。它是 syscall 绑定 ——\
             写不写盘看调用点，而调用点在本 crate 里、由命名空间那一条那条判据数",
        ),
        (
            "notify",
            DEPS,
            UNMEASURED,
            "inotify 观测：backend 只拿它订阅被观测目录底下的变化，一条写路径都不经它",
        ),
        (
            "portable-pty",
            DEPS,
            UNMEASURED,
            "伪终端：只在 `platform/child.rs::converse_in_terminal` 开一对终端、在里面起子进程（要终端才肯问的官方登录子命令）。\
             它自己不写盘：只开系统给的伪终端设备、读写那一对终端；子进程写什么归子进程（同 `Child` 别的几种用法）",
        ),
        (
            "notify-debouncer-mini",
            DEPS,
            UNMEASURED,
            "把上面那条的事件去抖之后再交出来 —— 同一条路上的第二段，同样只在读侧",
        ),
        (
            "flate2",
            DEPS,
            UNMEASURED,
            "解压 tar.gz / tgz 与 zip 的 deflate 那一支（`control/files_extract.rs`）：纯内存解码器（`rust_backend` ＝ `miniz_oxide`）。\
             它本来就在发布二进制里（`russh` 的 `flate2` feature 那一棵），这一行只是把间接依赖提成直接依赖、零新包。\
             写不写盘：本 crate 对它的用法只有 `read::GzDecoder` 包一个读句柄（用法签字，没扫它的源码）",
        ),
        (
            "tar",
            DEPS,
            UNMEASURED,
            "解压 tar / tar.gz（`control/files_extract.rs`，「用 Rust 库、不调外部命令」）：缺省 feature 关了（不带 `xattr`）。\
             ⚠ 它**有**一步解到盘上的 API（`unpack` 一族，会自己建文件、设权限与时间）—— 本 crate **不调它**：只用 `Archive::entries` 逐条读头与正文，\
             落盘由 `files_extract` 自己逐条过路径解析再写。本档是用法签字（没扫它的源码）",
        ),
        (
            "zip",
            DEPS,
            UNMEASURED,
            "解压 zip（`control/files_extract.rs`）：缺省 feature 全关（aes / bzip2 / lzma / xz / zstd / zopfli 不进来），只开读 deflate 那一支。\
             ⚠ 它**有**一步解到盘上的 API（`ZipArchive::extract`）—— 本 crate **不调它**：只用 `by_index` 逐条读名字、种类、权限位与正文，\
             落盘由 `files_extract` 自己逐条过路径解析再写。本档是用法签字（没扫它的源码）",
        ),
        (
            "ring",
            DEPS,
            UNMEASURED,
            "只用 `ring::digest::SHA256`：CAS 摘要形的唯一算法住址 \
             `files/mod.rs::content_sha256`（纯内存算摘要）。它本来就在发布二进制里 \
             （上面 `rustls` 的 provider · 下面 `russh` 同一棵），这一行只是把间接依赖提成直接依赖、零新包。\
             写不写盘：本 crate 对它的用法一条写路径都不经它（用法签字，没扫它的源码）",
        ),
        (
            "rustls",
            DEPS,
            UNMEASURED,
            "TLS 握手与记录层（provider 钉了 `ring`）；证书链由下面那条 crate 以常量表给出，\
             本机不落盘、不建缓存目录",
        ),
        (
            "russh",
            DEPS,
            UNMEASURED,
            "`K-P6b`：`--dial` 那条代理臂的 SSH 客户端（传输层握手 · publickey 鉴权 · \
             开 channel）。与 monitor 侧**同版本同 feature 集**（`0.61.1` / `ring` / \
             `flate2` / `rsa`），provider 就是本表上面 `rustls` 那条已经在用的 `ring`。\
             ⚠ **本档是「未量」不是「没写面」**：私钥由 `russh::keys::load_secret_key` \
             **读**一个路径（读不是写），而它建不建缓存 / 写不写 known_hosts \
             **我没有扫过它的源码** —— backend 侧这条路不传 known_hosts 路径、\
             host key 校验由 `dial::DialHandler` 自己在内存里比指纹，\
             但那是**用法**上的签字，不是对它源码的读数。\
             ⇒ 要升到 `已量·未见写面` 得真去扫它那棵树，还没做。\
今天链的是仓内补过的副本（`[patch.crates-io]` → `src/vendor/russh`，只改了 \
             `compression.rs` 解压收尾那一段、没碰任何 IO）⇒ 源码进了树、**可以**量了，但本档仍是「未量」：还没扫它的写面",
        ),
        (
            "russh-sftp",
            DEPS,
            MEASURED_WRITES_ON_PURPOSE,
            "SFTP 客户端：用户「SFTP 进本机常驻后端，只写暂存区」（＋ F08 自部署目录）。\
             **本机**写面现打 0 处（3.0.0 源码里 `std::fs` 只出现在 `protocol/open.rs` 一个 `From<OpenFlags> for \
             fs::OpenOptions` 的转换里 —— 造选项、不开文件；它的 `tokio` 没开 `fs` / `net`）；\
             **远端**写面就是它的本职（`SSH_FXP_OPEN` 带写标志 · `REMOVE` · `RENAME` · `MKDIR` …）⇒ 就是要它写。\
             谁划边界：只许 `ssh-dial/sftp.rs` 一份文件持有它的会话、写根 == 暂存区与部署目录、每处先过 `fenced_remote`。\
             边界判据：`the_remote_write_lives_in_exactly_one_file_and_its_root_is_exactly_staging`",
        ),
        // `search-core` 那一行出列：crate 删了（通用口径收成 `history/search_rules.rs`，记录文本进 `agents/claudecode/text.rs`）。
        (
            "serde",
            DEPS,
            UNMEASURED,
            "序列化派生；backend 只拿它把 wire 帧与结构体互转，落盘那一步不经它",
        ),
        (
            "serde_json",
            DEPS,
            UNMEASURED,
            "JSON 编解码；同 `serde`，只在内存里把字节变成结构体、再变回去",
        ),
        (
            "rhai",
            DEPS,
            UNMEASURED,
            "轮换策略文件的沙箱（`accounts/quota/strategy/`）：`Engine::new_raw()` ＋ 标准包，**没注册**读写文件 · 连网 · 起进程的函数，\
             不给模块解析器（`import` 不可用），开 `no_time`（连取时间都没有）。它只在内存里编、跑脚本，交回一个值；\
             策略文件由后端自己读成字串交给它（`strategy::files_in`），它自己不开文件",
        ),
        (
            "test-home",
            DEV_DEPS,
            UNMEASURED,
            "测试档一起来就把真家换成沙箱（仓内 crate `src/common/test-home`，`main` 之前设 `HOME` 等变量、建一个临时目录）。\
             **只在测试期链接**，不进发布二进制",
        ),
        (
            "sha2",
            DEV_DEPS,
            UNMEASURED,
            "只给 vendored russh 副本算指纹（`dial_compress_tests`：盘上每一份 == `VENDOR.md` 登记的 sha256）。\
             **只在测试期链接**，不进发布二进制；版本是 russh 那棵树早已锁着的 `0.11.0`（不新增包）。纯内存摘要 —— 它不开文件，\
             读副本的是判据自己（经 `guard_core::scan_tree_excluding`）",
        ),
        (
            "ts-rs",
            DEV_DEPS,
            UNMEASURED,
            "记录的线上形状（`agents/claudecode/schema.rs`）导出 TS 类型用（`#[cfg_attr(test, derive(ts_rs::TS))]`）。\
             **只在测试期链接**，不进发布二进制；写面只有 `cargo test` 里 `export_bindings_*` 往 `src/frontend/ui/generated/` 写生成物 —— \
             与 monitor 那一侧同一个用法（C01），版本是 monitor 那份 lock 早已解析的 `12.0.1`",
        ),
        (
            "tokio",
            DEV_DEPS,
            UNMEASURED,
            "测试档多开 `test-util`（停着的钟，`advance` 才走）：判「挂着的提问自己带的期限」· 保活几轮判死不按墙钟等。\
             同一个包（`[dependencies]` 里那一条），**只在测试期多一个特性**，不进发布二进制",
        ),
        (
            "shell-quote-core",
            DEPS,
            MEASURED_CLEAN,
            "POSIX 单引号 quote 的唯一实现（纯字符串变换）；仓内 crate，现打 0 处写面",
        ),
        (
            "own-chan",
            DEPS,
            MEASURED_CLEAN,
            "本人通道：常驻后端听的那个 Unix 套接字（目录独占锁 `flock` · 绑 · 收连接核对端 uid · 连）；仓内 crate、只依赖 tokio 与 libc。\
             现打 0 处写面：绑套接字会在那个目录里建一个套接字文件（内核做），删陈旧文件与建目录都在调用方 `control/resident.rs`（经 `Own`）",
        ),
        (
            "relay-route-core",
            DEPS,
            MEASURED_CLEAN,
            "中转门牌：端口 · 钥匙文件相对路径两个 const ＋ 路由语法（拼 / 拆 / 段闸，纯字符串）；仓内 crate、零依赖，现打 0 处写面、0 处 I/O",
        ),
        (
            "tokio",
            DEPS,
            UNMEASURED,
            "运行时 + io。⚠ **本表唯一「写面在我们自己手上」的一条**：`fs` feature 开着 ⇒ \
             它**提供**写 API，而调用点全在本 crate 里 —— 那一面由命名空间那一条与那几种写句柄\
             那两条判据数，不由本表。本表管的是「它自己会不会写」",
        ),
        (
            "tracing",
            DEPS,
            UNMEASURED,
            "日志门面：它自己不选落点，落点由 subscriber 那一条决定",
        ),
        (
            "jiff",
            DEPS,
            UNMEASURED,
            "时刻按看的那一台的时区写字（`common/time.rs::Tz`：IANA 名 ⇒ 那一刻的偏移）：缺省 feature 关了，只开 `std` 与 `tzdb-bundle-always`，\
             时区库编进二进制（`jiff-tzdb`，一份静态字节），**不开** `tz-system` ⇒ 不读 `/usr/share/zoneinfo`、不读 `TZ`、不读 Windows 注册表。\
             写不写盘：本 crate 对它的用法只有 `TimeZone::get`（查内嵌那份表）与 `to_offset`（纯计算），不碰盘、不起进程（用法签字，没扫它的源码）",
        ),
        (
            "toml_edit",
            DEPS,
            UNMEASURED,
            "配置文件（`~/.cc-monitor/profiles.toml`）的解析与按条目改（`assets/aliases/profile.rs`）：缺省 feature 关了，只开 `parse` 与 `display`，\
             纯内存把文本变成文档树、再变回文本（保留注释与排版）。读盘与落盘都在本 crate：读是 ccm 那一趟的 `read_to_string` 与经文件管理面的 `files-peek`，\
             写经文件管理面的 `Door::put`。写不写盘：本 crate 对它的用法只有 `Document::parse` / `DocumentMut` 的内存编辑（用法签字，没扫它的源码）",
        ),
        (
            "tracing-subscriber",
            DEPS,
            UNMEASURED,
            "只开 `env-filter`。日志去向由本 crate 自己给的 writer 定（今天是标准错误）——\
             落盘那一形要另一条 crate，而本清单上没有",
        ),
        // `codex-token-core` 那一行删了：它搬进本 crate（`agents/codex/token.rs`），不再是依赖 ——
        //   它那几行从此是本 crate 的生产段，由上面的写面分层照常扫（纯数据映射，0 处写面）。
        // `agent-tools-core` 那一行摘了：它收进本 crate 的适配层（`agents/claudecode/cards.rs`，纯数据映射，0 处写面）。
        (
            "mimalloc",
            MUSL_DEPS,
            UNMEASURED,
            "远端 musl 版的全局分配器（`main.rs` 那一处 `#[global_allocator]`）：它只管进程自己的堆 —— \
             向内核要页、还页（mmap / madvise），不开文件、不起进程；缺省 feature 为空（不开 override / secure）。\
             写不写盘：分配器的接口里没有路径这一样东西（用法签字，没扫它带的 C 源）",
        ),
        (
            "walkdir",
            DEPS,
            UNMEASURED,
            "目录遍历，纯只读：它交出的是路径，读不读、写不写由调用点决定",
        ),
        // `webpki-roots` 那一行摘了：根证书表只剩中转 crate `comms-outward` 用（它那一行签着）。
        (
            // 上游 base URL 能不能用的唯一一份（中转解析 · 上游选择装表 · 写口）。
            "upstream-url-core",
            DEPS,
            MEASURED_CLEAN,
            "上游 base URL 的形状 ＋ 明文只许回环（纯字符串判定）；仓内 crate、零依赖，现打 0 处写面、0 处 I/O",
        ),
        (
            // 交给 Win32 的路径（长路径前缀）的唯一一份：不覆盖改名那一处用。
            "win-path-core",
            DEPS,
            MEASURED_CLEAN,
            "路径编成 UTF-16 并补长路径前缀（纯变换；Windows 那一半只取绝对路径）；仓内 crate、零依赖，现打 0 处写面、0 处 I/O",
        ),
        (
            // 部署那一族的契约（表 A 的键与行 · 戳格式 · 答话形状 · 路径）；判定那一半住后端 `deploy` crate。
            "deploy-contract",
            DEPS,
            MEASURED_CLEAN,
            "部署契约（纯形状：键 · 表 A 的行 · 身份戳的读法 · 答话形状 · 路径）；仓内 crate、只依赖 copy-core / shell-quote-core，现打 0 处写面、0 处 I/O",
        ),
    ];

    /// 清单的**依赖段**逐条：`(段名, crate 名, 那一行原文)`。
    ///
    /// 两条判据：段名以 `dependencies]` 收尾 · 条目从**列 0 起**
    /// （多行内联表的续行 —— 那些 feature 字面量 —— 不是条目）。
    ///
    /// ⚠ 剥 `#` 整行注释走 `guard_core` 的**共享原语**，不在这里自己写第二份 ——
    /// 本函数第一版内联了一个 `#` 过滤，而 monitor 侧那张「剥注释实现只许一份」的登记表
    /// **当场逮住了它**（09-04 现打；它的人群跨三棵树，backend 这一侧也在里面）。
    fn dep_entries(manifest_text: &str) -> Vec<(String, String, String)> {
        let mut out: Vec<(String, String, String)> = Vec::new();
        let mut section = String::new();
        let uncommented = guard_core::strip_hash_comment_lines(manifest_text);
        for line in uncommented.lines() {
            let entry = line.trim();
            if entry.starts_with('[') {
                section = if entry.ends_with("dependencies]") {
                    entry.to_string()
                } else {
                    String::new()
                };
                continue;
            }
            if section.is_empty() || line.starts_with(char::is_whitespace) {
                continue;
            }
            let Some((key, _)) = entry.split_once('=') else {
                continue;
            };
            let name = key.trim();
            if name.is_empty()
                || !name
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
            {
                continue;
            }
            out.push((section.clone(), name.to_string(), entry.to_string()));
        }
        out.sort();
        out.dedup();
        out
    }

    /// 清单的依赖段里某一条依赖的**那一行原文**（`None` = 那一段里没有这条依赖）。
    fn dep_line(manifest_text: &str, name: &str) -> Option<String> {
        dep_entries(manifest_text)
            .into_iter()
            .find(|(_, n, _)| n.as_str() == name)
            .map(|(_, _, line)| line)
    }

    /// 没签字的那些（诊断用的住址表）。**纯函数** —— 能直接喂合成清单，
    /// 否则「今天零条未签字」这个读数与「这把尺子根本不报」在终端上没有区别。
    fn unsigned_of(declared: &[(String, String, String)]) -> Vec<String> {
        declared
            .iter()
            .filter(|(sect, name, _)| {
                !SIGNED
                    .iter()
                    .any(|(n, s, ..)| *n == name.as_str() && *s == sect.as_str())
            })
            .map(|(sect, name, _)| format!("  {sect} 里的 {name}"))
            .collect()
    }

    // ══════ `K-R29`（09-06）：`已量·未见写面` 那一档的判决，从**一次性**改成**常驻** ══════
    //
    // # 它治的那一格
    //
    // `MEASURED_CLEAN` 那一档的解锁条件**自己写着**它没有守卫（逐字，改掉之前）：
    // 「⚠ 如实写明：**没有任何判据会在那一刻自动红** —— 这一档的尺子是签字那一刻现打的，
    //   不是常驻的」。⇒ 那个判决只在签字那一秒为真，之后无人再问。
    // `D1②` 现打过一刀确认这条读数：给 `branch-core` 加一处货真价实的 `fs::write`，
    // **门禁九格一格没红**（cargo 1450 · backend 587 · npm 1590 · e2e 12/8/264/72 · pb check 0，
    // 量于 `b4e289f` + 那一刀、未铺 `embedded-backends`）。
    //
    // # 为什么这一档做得起来，而 `UNMEASURED` 那一档做不起来
    //
    // 本档今天 6 条**全是仓内 crate**（清单那一行带 `path =`）⇒ **源码就在树里**，
    // 重扫是零边际成本。`UNMEASURED` 那一档的解锁条件是「有人真去读了它的源码」，
    // 而那些是第三方 crate、源码不在树里 ⇒ **这里的做法对它们不适用**。
    // 🔴 **这里覆盖 `MEASURED_CLEAN` 这一档，不覆盖 `UNMEASURED` 那一档**；
    //    下面这几条判据绿了，说明的只是前者，**不许把后者也算进这个绿里**。
    //
    // # 它买不到什么（`§4` 诚实边界逐字，别读大一格）
    //
    // - 买到的是「**写面出现的那一刻会红**」，**不是**「那几棵 crate 永远干净」。两句不许压成一句。
    // - **模式表本身有多准，这里不判** —— 它沿用护栏既有的那两张表（[`super::tests::FS_MUTATION_PATTERNS`]
    //   与 [`super::tests::WHITELIST_STILL_FORBIDDEN`]）加起进程点；**那两张表漏掉的写法，这里一样漏**。
    //   08-06 就实测过一次同族的漏：黑名单放过了 `os::unix::fs::symlink` 与 `fs::set_permissions`。
    //   〔`K-R79` 09-12 补：尺子里现在还有第三样 —— [`super::remote_write_layer::all_remote_needles`]
    //    那两张远端写的网。**同一句话照样成立**：那两张网漏掉的写法，这里一样漏，
    //    它们各自漏什么逐条写在 `remote_write_layer` 的头注里。〕
    // - 口径是**整棵 `src`、不剥 `#[cfg(test)]`、连注释一起扫**（fail-closed，§41.4 第 1 条纪律）。
    //   ⇒ 那几棵 crate 的**测试代码**里出现写面也会红。那是有意的：本档的签字逐字写的是
    //   「整棵 `src` … 命中 0 处」，而剥掉测试段会让这个数比签字上的那个数**小**。

    /// K-R29：那条常驻判据的**名字只有这一处住址**。
    ///
    /// 三处必须对得上，任一处漂了就红（[`the_clean_verdict_unlock_text_names_the_guard_that_now_watches_it`]）：
    /// ① 本常量；② `VERDICTS` 里 `MEASURED_CLEAN` 那一档的解锁条件文字；③ 本文件里真有这么一条 `fn`。
    const RESIDENT_GUARD: &str = "the_clean_verdict_is_re_measured_on_the_tree_every_run";

    /// 这里那把尺子：**护栏自己那两张模式表 ∪ 起进程点 ∪ 远端写那两张网**（现算，不手抄第二份）。
    ///
    /// ⚠ 起进程点那一项**运行时拼** —— 本文件里别再多一处那个字面量：
    /// [`super::spawn_registry`] 那条判据靠**按文件名跳过本文件**才不自匹配，
    /// 而跨文件数它的那些判据不一定跳。
    ///
    /// 并进了 [`super::remote_write_layer`] 那两张网。**理由是「同职」**：
    /// 这把尺子替 `已量·未见写面` 那一档回答「这几棵仓内 crate 今天干不干净」，
    /// 而「干净」如果只算本机写面，那么哪天 `acct-core` 里长出一处远端写，
    /// 这一档照样绿 —— 那正是在后端本体上治的同一个洞，换一棵树再挖一遍。
    /// ⚠ 并进来是**收紧**：今天这六棵 crate 上现打 0 处命中（量过，逐词都是 0）。
    fn resident_ruler() -> Vec<String> {
        let mut v: Vec<String> = super::tests::FS_MUTATION_PATTERNS
            .iter()
            .chain(super::tests::WHITELIST_STILL_FORBIDDEN.iter())
            .map(|p| (*p).to_string())
            .collect();
        v.push(format!("Command::{}(", "new"));
        v.extend(super::remote_write_layer::all_remote_needles());
        v.sort();
        v.dedup();
        v
    }

    /// 清单里那条依赖的 `path = "…"`（`None` = 它不是仓内 crate ⇒ 本尺子够不着它的源码）。
    fn dep_path_of(manifest_text: &str, name: &str) -> Option<String> {
        let line = dep_line(manifest_text, name)?;
        // 只在等号右边找：名字里也可能带 `path`（`win-path-core`）。
        let (_, value) = line.split_once('=')?;
        let at = value.find("path")?;
        let rest = value[at + "path".len()..].trim_start();
        let rest = rest.strip_prefix('=')?.trim_start();
        let rest = rest.strip_prefix('"')?;
        let end = rest.find('"')?;
        Some(rest[..end].to_string())
    }

    /// 判档为 `verdict` 的那几条 —— **现算自 [`SIGNED`]**，不手抄第二份（本仓 `E12`：
    /// 手抄那份会在下一次加签字时漂，而漂了不会有人知道）。
    fn crates_with_verdict(verdict: &str) -> Vec<&'static str> {
        SIGNED
            .iter()
            .filter(|(_, _, v, _)| *v == verdict)
            .map(|(n, ..)| *n)
            .collect()
    }

    /// 一棵仓内 crate 的源码绝对住址（相对本清单所在目录解析）：有 `src/` 取它；crate 根与清单同层的（通信层那两个）取包目录。
    fn crate_src_dir(rel: &str) -> std::path::PathBuf {
        let pkg = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(rel);
        if pkg.join("src").is_dir() || !pkg.join("lib.rs").is_file() {
            pkg.join("src")
        } else {
            pkg
        }
    }

    /// 把某一档拆成「扫得了的」与「扫不了的」两半。
    ///
    /// 🔴 **扫不了的不许静默跳过** —— 那正是这里要治的病的同族形状（判决没人盯着）。
    /// 返回 `(可扫: (crate 名, 清单里的 path, src 住址), 扫不了的诊断行)`。
    #[allow(clippy::type_complexity)]
    fn split_by_reachability(
        names: &[&'static str],
    ) -> (Vec<(&'static str, String, std::path::PathBuf)>, Vec<String>) {
        let mut ok = Vec::new();
        let mut out_of_reach = Vec::new();
        for name in names {
            match dep_path_of(MANIFEST, name) {
                None => out_of_reach.push(format!(
                    "  {name}：清单那一行没有 `path =` ⇒ 它不是仓内 crate，源码不在树里"
                )),
                Some(rel) => {
                    let dir = crate_src_dir(&rel);
                    if dir.is_dir() {
                        ok.push((*name, rel, dir));
                    } else {
                        out_of_reach.push(format!(
                            "  {name}：清单写着 `path = \"{rel}\"`，而 {} 不是个目录",
                            dir.display()
                        ));
                    }
                }
            }
        }
        (ok, out_of_reach)
    }

    /// 拿尺子扫一棵 crate 的 `src`，**逐处**给住址（`文件:行号: 命中的模式 ▸ 那一行`）。
    ///
    /// ⚠ **整棵树、不剥 `#[cfg(test)]`、连注释一起数** —— 见本段头注的口径说明。
    fn write_surface_hits(crate_src: &std::path::Path, ruler: &[String]) -> Vec<String> {
        let mut hits = Vec::new();
        for (path, src) in guard_core::scan_tree!(crate_src, &["rs"]) {
            let rel = path
                .strip_prefix(crate_src)
                .unwrap_or(&path)
                .to_string_lossy()
                .replace('\\', "/");
            for (n, line) in src.lines().enumerate() {
                if let Some(pat) = ruler.iter().find(|p| line.contains(p.as_str())) {
                    hits.push(format!("src/{rel}:{}: `{pat}` ▸ {}", n + 1, line.trim()));
                }
            }
        }
        hits.sort();
        hits
    }

    /// 本文件自己的源码，**运行时读**（住址由 `file!()` 给，改名了它自己会说不出话）。
    fn own_source() -> String {
        let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(file!());
        std::fs::read_to_string(&p).unwrap_or_else(|e| {
            panic!(
                "读不到本文件 {}：{e}\n\
                 `file!()` 给的住址与真实布局对不上了 —— 先修这个，别把下面那条断言绕过去",
                p.display()
            )
        })
    }

    /// ★ 正题：**清单上每一条依赖都得有一行签字**，而带依赖的段本身也钉住。
    #[test]
    fn every_dependency_this_manifest_declares_carries_a_signature() {
        let declared = dep_entries(MANIFEST);
        // 抽取器自检：分母塌了下面两条一起零命中地绿。
        assert!(
            declared.len() >= 15,
            "清单的依赖段里只抽出 {} 条依赖（09-04 现打 18）—— 抽取坏了，本条在空转：{declared:?}",
            declared.len()
        );
        let mut sections: Vec<String> = declared.iter().map(|(s, ..)| s.clone()).collect();
        sections.sort();
        sections.dedup();
        let mut registered: Vec<String> = DEP_SECTIONS.iter().map(|s| (*s).to_string()).collect();
        registered.sort();
        assert_eq!(
            sections, registered,
            "清单里**带依赖的段**变了。\n\
             **多一段**（`[build-dependencies]` / `[target.'…'.dependencies]` 那些）⇒ \
             那是一整段依赖从本表分母里溜走的入口，先把它登记进 `DEP_SECTIONS`；\n\
             **少一段** ⇒ 那一段的依赖没了，回来把 `SIGNED` 里对应的行摘掉。"
        );
        let unsigned = unsigned_of(&declared);
        assert!(
            unsigned.is_empty(),
            "这些依赖没有签字：\n{}\n\n\
             ⇒ 本护栏的人群行逐字承认它「不含依赖 crate 的写」——\
             一条新依赖在它自己的代码里写盘 / 起进程，两层判据一层都不会响。\n\
             **本表买的就是那一格**：加一条依赖，就在 `SIGNED` 里写一行\
             「它在后端里做什么 · 有没有写面 · 依据是什么」。\n\
             判档只有这几个（现算）：{}",
            unsigned.join("\n"),
            verdict_names().join(" / ")
        );
    }

    /// ★ 反向那半：签字表里不许有**幽灵条目**，每一档都要在闭集里、每一行都要有依据。
    #[test]
    fn the_signature_table_has_no_ghost_entries_and_every_verdict_is_a_registered_one() {
        assert_eq!(
            VERDICTS.len(),
            4,
            "判档闭集现在有 {} 格 —— **相等断言，不是地板**。\n\
             加一格 = 判档表被改了，那不是顺手能加的；少一格 = 有人把某一种判档拿掉了，\n\
             而那正是要有人看一眼的时刻。",
            VERDICTS.len()
        );
        let mut names = verdict_names();
        let before = names.len();
        names.sort_unstable();
        names.dedup();
        assert_eq!(before, names.len(), "判档闭集里有重名的档：{names:?}");
        for (verdict, when, unlock) in VERDICTS {
            assert!(
                when.trim().chars().count() >= 20,
                "`{verdict}` 没写清「什么时候落在这一档」（实得 {} 字）",
                when.trim().chars().count()
            );
            assert!(
                unlock.trim().chars().count() >= 20,
                "`{verdict}` 没写**这一档自己的解锁条件**（实得 {} 字）—— \
                 一张没有解锁条件的判档表，用不了几轮就会变成装饰",
                unlock.trim().chars().count()
            );
        }
        let declared = dep_entries(MANIFEST);
        for (name, sect, verdict, _) in SIGNED {
            assert!(
                is_verdict(verdict),
                "`{name}` 的判档是 `{verdict}` —— 它不在闭集里。闭集（现算）：{}",
                verdict_names().join(" / ")
            );
            assert!(
                declared
                    .iter()
                    .any(|(s, n, _)| n.as_str() == *name && s.as_str() == *sect),
                "签字表里的 `{sect}` / `{name}` 在清单上已经找不到了 —— 幽灵条目。\n\
                 依赖摘掉了就**同轮**把签字摘掉：留着的后果不是多一行没用的字，\
                 是下一个人以为这条依赖还在、而它的写面理由还挂在旧地址上。"
            );
        }
    }

    /// ★★ 那条**有写面**的签字，它的前提是「那个 feature 本清单开着、而且就是要它写」—— 把前提钉住。
    ///
    /// 这一条是本模块里唯一**不只钉「说得出来」**的判据：它钉的是那句话赖以成立的那个事实。
    /// 前提翻了一次：先前是「那个 feature 本清单没开 ⇒ 今天编不进来」
    /// （`已量·有写面`），今天是「开了、写面就是账号域那一份要用的」（`已量·有写面·就是要它写`）。
    /// ⇒ 两个方向都钉：feature 被关掉了而签字还说「就是要它写」⇒ 红；
    ///   `已量·有写面`（「凭什么进不来」那一档）今天只有 guard-core —— 有人把它签回那一档而 feature 还开着 ⇒ 红。
    /// 本仓最高频的病是「代码订正了，盘没跟着改」，这一条挡的正是它的反面：
    /// **盘上写着的前提被代码改掉了而盘不知道。**
    #[test]
    fn the_credentials_crate_is_signed_as_a_purposeful_writer_and_its_feature_is_really_on() {
        let gated: Vec<&str> = SIGNED
            .iter()
            .filter(|(_, _, v, _)| *v == MEASURED_WRITES)
            .map(|(n, ..)| *n)
            .collect();
        // 今天只有 guard-core：起一个只读的 `git ls-files`，进不来是因为它只在 `[dev-dependencies]`（签字里写着谁钉）。
        assert_eq!(
            gated,
            vec!["guard-core"],
            "「有写面、但进不了发布二进制」那一档的成员变了，实得 {gated:?}\n\
             新来一条这样的依赖 ⇒ 先回答「它凭什么进不来」「那个前提谁钉着」，再改这里。"
        );
        let purposeful: Vec<&str> = SIGNED
            .iter()
            .filter(|(_, _, v, _)| *v == MEASURED_WRITES_ON_PURPOSE)
            .map(|(n, ..)| *n)
            .collect();
        // +`russh-sftp`（SFTP 进本机常驻后端）。它的前提不是一个 feature，
        //   是「只许一份文件持有它的会话、写根 == 暂存区一处」—— 那一条由它签字里点名的边界判据钉（远端写那一层），
        //   本条只钉 `creds-core` 那一条的前提（feature 开着）。
        // +`ssh-dial` 两行（拨号核心，远端写那一格随 SFTP 客户端搬进去）：前提同 `russh-sftp`，由同一条边界判据钉。
        assert_eq!(
            purposeful,
            vec!["ssh-dial", "ssh-dial", GATED_CRATE, "russh-sftp"],
            "「就是要它写」那一档的成员变了：{purposeful:?}\n\
             本条只钉得住 `{GATED_CRATE}` 那一条的前提（feature 开着）；新来的成员要在签字里点名它自己的边界判据。"
        );
        // 针**运行时拼**：清单的注释里逐字写着这个词，而下面只看那一行、不看注释。
        let feature = format!("har{}", "den");
        let line = dep_line(MANIFEST, GATED_CRATE).unwrap_or_else(|| {
            panic!(
                "清单的依赖段里找不到 `{GATED_CRATE}` —— 签字的对象没了，回来重判 `SIGNED`\
                 （而不是把本条删掉）"
            )
        });
        assert!(
            guard_core::contains_word(&line, &feature),
            "本清单没给 `{GATED_CRATE}` 开 `{feature}`：{line}\n\
             ⇒ 签字说「就是要它写」（账号域那一份写口要 `create_private`），而 feature 关着 —— \
             要么写口已经退役（那就回来把签字降回 `已量·有写面` 并答「凭什么进不来」），\
             要么这一行被改坏了（那么后端根本编不过）。**这不是改个断言的事**：回来重签。"
        );
        // 反空真：探针对「开着」的写法必须认得出来，否则上面那条零命中断言什么也不说明。
        //
        // 🔴 **样本里那个词不许由 needle 拼出来** —— 那样这条控制**恒真**。
        // 09-04 现打的活体（死值验 M5 逮到的，就在这一行上）：第一版逐字是
        // `format!("… features = [\"{feature}\"] …")`，把 needle 改成一个盘上不存在的词之后
        // **控制照样绿** ⇒ 它当时什么都没在证明，而它的表现与真控制一模一样。
        // ⇒ 样本里是**另一处独立字面量**。两处「重复」是刻意的：
        //   一条控制要的正是**独立见证**，与 needle 同源就不是见证。
        let sample_with_it = format!(
            "{GATED_CRATE} = {{ path = \"…\", features = [\"{}\"] }}",
            "harden"
        );
        assert!(
            guard_core::contains_word(&sample_with_it, &feature),
            "探针连样本 `{sample_with_it}` 都认不出来 —— 上面那条零命中断言此刻是空转的。\
             （两处若漂开了，先核**清单里那个 feature 今天叫什么**，别顺手把针改成样本。）"
        );
    }

    /// 🔴 **`creds-core` 那条「就是要它写」的边界判据**：它的写半边
    /// （`perm::create_private`）在本 crate 生产段里的引用处，
    /// **恰好**是原子写那一处 `platform/fs/own.rs`（两向集合相等）。
    ///
    /// # 它顶替的是哪一格
    ///
    /// 先前「backend 写不了那份凭据文件」是**编译器**兜的（feature 没开 ⇒ 那个函数不存在）。
    /// 远端那台机器上的 key 只能由那台的后端写 ⇒ feature 开了，编译器那一格没了，由本条接住：
    /// 谁在别处顺手调一次 `create_private` 建个文件 ⇒ 当场红。
    ///
    /// # 反空真
    ///
    /// 期望集合非空（那一份自己必须真的引用它 —— 写口被换掉了本条也红）；
    /// 针在一份合成源码上认得出（样本是**独立字面量**，不由针拼出来）。
    ///
    /// # 买不到
    ///
    /// 经宏 / 别名间接够到那个函数的写法（`use creds_core::perm::create_private as c;` 之后只写 `c(`）——
    /// `use` 那一行本身带着名字，所以「引入」这件事照样看得见；再往外一层的重导出看不见。
    #[test]
    fn the_credentials_write_half_is_reached_only_from_the_account_file_face() {
        let root = guard_core::addr::backend_src_root();
        let needles = [format!("create_{}", "private")];
        let mut found: std::collections::BTreeSet<String> = Default::default();
        let mut scanned = 0usize;
        let files = super::tests::core_files();
        for path in files {
            let rel = path
                .strip_prefix(&root)
                .unwrap_or(&path)
                .to_string_lossy()
                .replace('\\', "/");
            if path.file_name().and_then(|n| n.to_str()) == Some("readonly_guard.rs") {
                continue;
            }
            scanned += 1;
            let src = std::fs::read_to_string(&path).expect("read rs file");
            let prod = guard_core::production_code(&src);
            if needles.iter().any(|n| guard_core::contains_word(&prod, n)) {
                found.insert(rel);
            }
        }
        assert!(scanned >= 60, "只扫到 {scanned} 份后端源文件 —— 遍历坏了");
        // 自家状态各份与两个窄句柄（凭据 · 中转钥匙 · 门牌 · 主机钥匙 …）都经原子写那一处（`platform/fs/own.rs`）出生即只给本人 ⇒ 引用处只有它。
        let want: std::collections::BTreeSet<String> =
            ["platform/fs/own.rs".to_string()].into_iter().collect();
        assert_eq!(
            found, want,
            "`creds-core` 的写半边（建私有文件 / 收窄权限）在本 crate 生产段里的引用处对不上：\n\
             应当**恰好**是账号域那一份（写经 `Own` 的订阅号凭据窄句柄）。多出来的 = 写凭据的能力扩散了；\n\
             少了 = 那一份不再用它（写口被换掉了？那就回来重签 `{GATED_CRATE}`）。"
        );
        let sample = "let f = creds_core::perm::create_private(&tmp)?;";
        assert!(
            needles.iter().any(|n| guard_core::contains_word(sample, n)),
            "针连样本 `{sample}` 都认不出 —— 上面那条相等此刻在空转"
        );
    }

    /// ★★ 「今天零条未签字」这个读数要先证明**尺子接上了** —— 喂一份合成清单，它必须点名。
    ///
    /// 合成的那条刻意就是这里的形状：**引擎作为一条新依赖进来**那天。
    #[test]
    fn an_unsigned_dependency_is_really_reported_so_that_the_zero_is_not_vacuous() {
        let engine = "some-engine";
        let fake = format!(
            "[package]\nname = \"某个壳\"\nversion = \"0.0.0\"\n\n\
             {DEPS}\nserde = \"1\"\n{engine} = {{ path = \"vendor/引擎\" }}\n\n\
             {DEV_DEPS}\nguard-core = {{ path = \"../共享/guard-core\" }}\n"
        );
        let declared = dep_entries(&fake);
        assert_eq!(
            declared.len(),
            3,
            "合成清单里抽出 {} 条依赖（应当恰好是 serde · 引擎 · guard-core）：{declared:?}\n\
             抽取器读不准这份合成清单 ⇒ 它读真清单的那个读数也说明不了什么",
            declared.len()
        );
        assert_eq!(
            unsigned_of(&declared),
            vec![format!("  {DEPS} 里的 {engine}")],
            "★ 这一刀正是这里的形状：**引擎被编进 backend** 那天，它会作为一条新依赖出现，\
             而它自己就是「在 vendor 里写盘、判据看不见」的那一个 —— 本条要求那时候当场点名它。\
             同时它反向证明另一半：已经签过字的 `serde` / `guard-core` 不许被误报成没签字。"
        );
    }

    /// ★★ `K-R29` 正题：`已量·未见写面` 那一档的判决，**每一趟都重打一遍**。
    ///
    /// 在本条之前，那一档的判决只在**签字那一秒**为真（它的解锁条件自己写着这件事）。
    /// 本条把同一把尺子做成常驻：那几棵 crate 长出第一处写面的那一趟，它就红。
    ///
    /// 覆盖面与买不到的东西，逐条写在本段上方那个头注里 —— 尤其是
    /// **不覆盖 `UNMEASURED` 那一档**、以及**模式表漏掉的写法本条一样漏**。
    #[test]
    fn the_clean_verdict_is_re_measured_on_the_tree_every_run() {
        let ruler = resident_ruler();
        // 尺子自检：表被掏空了下面整条就是零命中地绿。
        assert!(
            ruler.len() >= 10,
            "尺子只现算出 {} 项（09-06 现打 16 = 两张模式表并集 11 + 5 ＋ 起进程点 1；\
             `K-R79` 09-12 并进远端写那两张网之后 26）—— \
             那几张模式表被掏空了，本条在空转：{ruler:?}",
            ruler.len()
        );
        let clean = crates_with_verdict(MEASURED_CLEAN);
        // 🔴 反空真①：清单现算出**空集**（选择器写错 ⇒ 扫 0 棵 ⇒ 恒绿）。
        assert!(
            !clean.is_empty(),
            "签字表里判档为 `{MEASURED_CLEAN}` 的一条都没现算出来（`SIGNED` 共 {} 条）—— \
             选择器写错了，本条此刻在扫 0 棵、恒绿。\n\
             （判档闭集现算：{}）",
            SIGNED.len(),
            verdict_names().join(" / ")
        );
        let (scannable, out_of_reach) = split_by_reachability(&clean);
        // 🔴 扫不了的**要出声**，不许静默跳过 —— 今天没有这种条目，将来会有。
        assert!(
            out_of_reach.is_empty(),
            "`{MEASURED_CLEAN}` 这一档里有 {} 条**本判据扫不了**：\n{}\n\n\
             ⇒ 本判据的做法是「源码就在树里 ⇒ 重扫一遍」，它只对**仓内 crate** 成立。\n\
             回来做一个决定，别让它静默留在这一档里：\n\
             ① 那一条降到 `{UNMEASURED}`（那一档的解锁条件本来就是「有人真去读了它的源码」）；\n\
             ② 或者给这一档补一条够得着它的判据，并把解锁条件文字一起改掉。",
            out_of_reach.len(),
            out_of_reach.join("\n")
        );
        // 🔴 反空真②：**棵数 > 0，并把棵数印出来**（`KR29D2` 的 acceptor 失效口）。
        assert!(
            !scannable.is_empty(),
            "本判据这一趟真扫了 0 棵 crate —— 零命中什么也不说明"
        );
        let names: Vec<&str> = scannable.iter().map(|(n, ..)| *n).collect();
        println!(
            "K-R29：本趟重扫 {} 棵仓内 crate（{}），尺子 {} 项",
            scannable.len(),
            names.join(" · "),
            ruler.len()
        );
        let mut hits: Vec<String> = Vec::new();
        for (name, rel, dir) in &scannable {
            for h in write_surface_hits(dir, &ruler) {
                hits.push(format!("  {name}（{rel}）{h}"));
            }
        }
        assert!(
            hits.is_empty(),
            "签字表把这几棵判成 `{MEASURED_CLEAN}`，而今天它们身上有 {} 处写面 / 起进程点：\n{}\n\n\
             ⇒ 这正是那一档的解锁条件说的那一刻：**那一条要回来重签**，判档多半该换成 \
             `{MEASURED_WRITES}`，并在签字里回答「凭什么它进不了发布二进制」「那个前提谁钉着」。\n\
             ⚠ **不许**为了让本条绿而把那几处从尺子里排除掉 —— 尺子是护栏自己那两张模式表，\
             动它等于同时放宽后端本体那两层判据。\n\
             （本趟扫了 {} 棵：{}）",
            hits.len(),
            hits.join("\n"),
            scannable.len(),
            names.join(" · ")
        );
    }

    /// ★★ `K-R29` 非空对照：**同一把尺子**必须在真有写面的那一条上亮。
    ///
    /// 上面那条断言的是「命中 0」，而**零命中既可能是干净、也可能是尺子瞎了** ——
    /// 两者在终端上一模一样。本条把它们分开：同一把尺子扫**有写面**的那两档
    /// （`{MEASURED_WRITES}` ∪ `{MEASURED_WRITES_ON_PURPOSE}`；今天唯一成员 [`GATED_CRATE`]，
    /// 它的签字里点名了 `perm.rs` 那两处写面 —— 它换了档、写面一处没少），必须 > 0。
    ///
    /// ⚠ 哪天那一条也变干净了，本条会红 —— **那是对的**：回来重挑一个非空对照，
    /// 不许把本条删掉了事（删掉之后上面那条就退回成一句空真）。
    #[test]
    fn the_same_ruler_still_lights_up_on_the_crate_that_really_writes() {
        let ruler = resident_ruler();
        let mut with_surface = crates_with_verdict(MEASURED_WRITES);
        with_surface.extend(crates_with_verdict(MEASURED_WRITES_ON_PURPOSE));
        assert!(
            !with_surface.is_empty(),
            "`{MEASURED_WRITES}` / `{MEASURED_WRITES_ON_PURPOSE}` 这两档今天一条成员都没有 —— 上面那条零命中断言从此没有对照，\
             它是「真干净」还是「尺子瞎了」分不出来了。回来重挑对照。"
        );
        // 对照只取**仓内**那几条：`russh-sftp`（第四档，外部 crate）的源码不在树里，
        //   本尺子够不着 —— 它的写面由签字里点名的边界判据在**用法**一侧钉（只许 `ssh-dial/sftp.rs` 一份持有）。
        //   够不着的只许是这一形：第四档、名字落在远端写协议词表上；别的形照旧红。
        let (scannable, out_of_reach) = split_by_reachability(&with_surface);
        let external_ok: Vec<&str> = SIGNED
            .iter()
            .filter(|(n, _, v, why)| {
                *v == MEASURED_WRITES_ON_PURPOSE
                    && why.contains(BOUNDARY_TAG)
                    && super::remote_write_layer::REMOTE_WRITE_PROTOCOL_WORDS
                        .iter()
                        .any(|(w, _)| n.contains(w))
            })
            .map(|(n, ..)| *n)
            .collect();
        let stray: Vec<&String> = out_of_reach
            .iter()
            .filter(|line| {
                !(line.contains("没有 `path =`")
                    && external_ok
                        .iter()
                        .any(|n| line.trim_start().starts_with(&format!("{n}："))))
            })
            .collect();
        assert!(
            stray.is_empty(),
            "对照那一档里有本尺子够不着的条目：\n{}",
            stray
                .iter()
                .map(|s| s.as_str())
                .collect::<Vec<_>>()
                .join("\n")
        );
        assert!(
            !scannable.is_empty(),
            "有写面的那两档里一棵仓内 crate 都没有了 —— 对照没了，回来重挑"
        );
        for (name, rel, dir) in &scannable {
            let hits = write_surface_hits(dir, &ruler);
            assert!(
                !hits.is_empty(),
                "同一把尺子在 `{name}`（`{rel}`）上命中 **0** —— 而它的判档是有写面的那两档之一。\n\
                 两种可能，都得有人看一眼：\n\
                 ① 尺子瞎了（模式表被掏空 / 扫描面画错 / 路径解析歪了）⇒ 那么上面那条\n\
                 「六棵全 0」的绿**此刻什么也不说明**；\n\
                 ② 它真的变干净了 ⇒ 回来重判它的判档，并**另挑一个非空对照**，别把本条删掉。\n\
                 （本趟尺子 {} 项）",
                ruler.len()
            );
        }
    }

    /// ★★ `K-R29`：那一档的解锁条件**不许再说自己没人盯着**，而且要点名盯着它的那一条。
    ///
    /// 本条钉的是一句**诚实边界的时效**。它原来逐字写着那一档没有任何常驻判据 ——
    /// 那句话在 `K-R29` 落地的同一拍就变成了假话，而
    /// **留着一句已经不成立的诚实边界，比没写还坏**：下一个人会照它做决定。
    ///
    /// 三处对拍（任一处漂了就红）：本模块的 [`RESIDENT_GUARD`] 常量 ·
    /// 解锁条件文字里点的那个名 · 本文件里真有这么一条 `fn`。
    #[test]
    fn the_clean_verdict_unlock_text_names_the_guard_that_now_watches_it() {
        let (_, _, unlock) = VERDICTS
            .iter()
            .find(|(v, ..)| *v == MEASURED_CLEAN)
            .unwrap_or_else(|| panic!("`{MEASURED_CLEAN}` 这一档在判档表里没了 —— 回来重判"));
        // 承重词**运行时拼**：免得本断言在自己的报错文案里找到它、变成恒真。
        let retired = format!("没有任何{}会在那一刻自动红", "判据");
        assert!(
            !unlock.contains(retired.as_str()),
            "`{MEASURED_CLEAN}` 的解锁条件里又出现了 `{retired}` —— 那是 `K-R29` **之前**的实况。\n\
             今天盯着这一档的是 `{RESIDENT_GUARD}`。\n\
             ⇒ 要么把那条判据真的删了（那时这句话才重新为真，而删之前先回答「为什么不要它了」），\n\
             要么就别把这句话写回来。"
        );
        assert!(
            unlock.contains(RESIDENT_GUARD),
            "`{MEASURED_CLEAN}` 的解锁条件没有点名那条常驻判据 `{RESIDENT_GUARD}`。\n\
             一张判档表的解锁条件是下一个人唯一会读的东西：它必须说得出\n\
             「现在由哪条判据盯着 · 它扫的是哪几棵 · 它扫不了的是什么」。\n\
             解锁条件现文：{unlock}"
        );
        // 反向那半：那个名字必须**真是本文件里的一条判据**，不是一句好听的话。
        let own = own_source();
        let def = format!("fn {RESIDENT_GUARD}(");
        assert!(
            own.contains(def.as_str()),
            "解锁条件点名了 `{RESIDENT_GUARD}`，而本文件里根本没有 `{def}` —— \
             那条判据被改名或删掉了，而解锁条件还挂在旧地址上。\n\
             ⚠ 这正是本仓最高频那族的形状：**代码改了，写着它的那句话没跟着改**。"
        );
    }

    // ══════ `K-R79`（09-12）：第四档的**边界**，以及「能力供应者不许躲进未量档」 ══════
    //
    // # 这两条各接哪一格
    //
    // 裁定二只说了「三档缺一档 ⇒ 加档，不是硬塞」。加一档本身是**一行数据**，
    // 而件计划 `KR79D2` 逐字点名的失效方向是「**加一档「例外」而不写它的边界**」。
    // ⇒ 下面两条就是那条边界的两半：
    //
    // | 判据 | 它挡的那一形 |
    // |---|---|
    // | [`capability_supplier_misfits`] | 一条**能远端写**的依赖被判成「没量过 / 没见写面」—— 那是**绕过新档**，闭集等于白加 |
    // | [`boundary_violations`]         | 一条真落在新档里的依赖，签字里**说不出谁在划它的边界** —— 那才是「例外」 |
    //
    // ⚠ **三档各自怎么红，逐档写清**（`KR79D2` 的死值验口径）：
    // 一条「有写面而且就是要它写」的依赖塞进现有三档中的任何一档，红的不是同一条判据——
    // 塞进 `已量·有写面` ⇒ [`the_credentials_crate_is_signed_as_a_purposeful_writer_and_its_feature_is_really_on`]
    // （那一档的成员集是**相等**断言）；塞进 `已量·未见写面` ⇒
    // [`the_clean_verdict_is_re_measured_on_the_tree_every_run`]（第三方 crate 没有 `path =`，
    // 落进「本判据扫不了」那一格）；塞进 `未量·靠用法签字` ⇒ **从前一条都不红**，
    // 那正是 [`capability_supplier_misfits`] 补上的那一格。

    /// 能力供应者**许**落的判档：已量那两档。**闭集**。
    ///
    /// 为什么 `MEASURED_WRITES` 也在里面：一条 SFTP crate 完全可能像 [`GATED_CRATE`] 那样
    /// 被一个没开的 feature 关在门外 —— 那时「有写面、但进不了发布二进制」是**对的判档**。
    /// 本条挡的不是「它落哪一档」，是「它躲进**没量过**那两档」。
    const VERDICTS_OPEN_TO_CAPABILITY_SUPPLIERS: &[&str] =
        &[MEASURED_WRITES, MEASURED_WRITES_ON_PURPOSE];

    /// **纯函数**：签字表里，名字落在远端写协议表上、判档却是「没量过 / 没见写面」的那几条。
    ///
    /// 纯函数是有意的 —— 新档今天**零成员**，真表上恒空；只有喂合成行，
    /// 「今天零违规」与「这把尺子根本不报」才分得开。
    fn capability_supplier_misfits(rows: &[(&str, &str, &str, &str)]) -> Vec<String> {
        let words = super::remote_write_layer::REMOTE_WRITE_PROTOCOL_WORDS;
        let mut out = Vec::new();
        for (name, sect, verdict, _) in rows {
            let Some((word, proto)) = words.iter().find(|(w, _)| name.contains(w)) else {
                continue;
            };
            if VERDICTS_OPEN_TO_CAPABILITY_SUPPLIERS.contains(verdict) {
                continue;
            }
            out.push(format!(
                "  {sect} 里的 {name}（名字里带 `{word}` = {proto}）判成了 `{verdict}`"
            ));
        }
        out
    }

    /// **纯函数**：落在第四档、而签字里说不出边界的那几条。
    ///
    /// 边界的形状：签字里逐字出现 [`BOUNDARY_TAG`]，紧跟一个反引号括起来的判据名，
    /// 而那个名字必须**真是本文件里的一条 `fn`**（`own_source` 由调用方给 ⇒ 可喂合成源码）。
    fn boundary_violations(rows: &[(&str, &str, &str, &str)], own_source: &str) -> Vec<String> {
        let mut out = Vec::new();
        for (name, sect, verdict, why) in rows {
            if *verdict != MEASURED_WRITES_ON_PURPOSE {
                continue;
            }
            let Some(at) = why.find(BOUNDARY_TAG) else {
                out.push(format!(
                    "  {sect} 里的 {name}：签字里没有 `{BOUNDARY_TAG}` —— \
                     一条没有边界的「就是要它写」，就是件计划点名的那个「例外」"
                ));
                continue;
            };
            let rest = &why[at + BOUNDARY_TAG.len()..];
            let judge = rest
                .strip_prefix('`')
                .and_then(|r| r.split_once('`').map(|(j, _)| j));
            match judge {
                None => out.push(format!(
                    "  {sect} 里的 {name}：`{BOUNDARY_TAG}` 后面没有反引号括起来的判据名"
                )),
                Some(j) if !own_source.contains(&format!("fn {j}(")) => out.push(format!(
                    "  {sect} 里的 {name}：边界点名了 `{j}`，而本文件里根本没有 `fn {j}(` —— \
                     判据被改名或删掉了，而这一行的边界还挂在旧地址上"
                )),
                Some(_) => {}
            }
        }
        out
    }

    /// ★★ `KR79D2` 那一格：**一条能远端写的依赖，不许躲进「没量过 / 没见写面」那两档。**
    ///
    /// 真表今天零违规，而零违规**说明不了尺子接上没有** ⇒ 三份合成行把两个方向都切开。
    #[test]
    fn a_dependency_that_supplies_remote_write_cannot_hide_in_an_unmeasured_verdict() {
        assert!(
            !super::remote_write_layer::REMOTE_WRITE_PROTOCOL_WORDS.is_empty(),
            "远端写协议词表被掏空了 —— 本条此刻在空转"
        );
        assert_eq!(
            capability_supplier_misfits(SIGNED),
            Vec::<String>::new(),
            "真表上有依赖躲在未量档里 —— 逐条见上"
        );
        // 引擎那个名字**运行时拼**，理由同本模块下面那条合成清单判据。
        let sftp_crate = format!("russh{}sftp", "-");
        for hiding in [UNMEASURED, MEASURED_CLEAN] {
            let fake = [(
                sftp_crate.as_str(),
                DEPS,
                hiding,
                "随便写一句看起来很像那么回事的话",
            )];
            let got = capability_supplier_misfits(&fake);
            assert_eq!(
                got.len(),
                1,
                "★ 这一刀正是 `KR79D2` 的形状：**SFTP 那条 crate 被判成 `{hiding}`** 那天，\
                 本条要当场点名它。实得：{got:?}"
            );
            assert!(
                got[0].contains(&sftp_crate) && got[0].contains(hiding),
                "点名了，却没说清是谁、落错了哪一档：{}",
                got[0]
            );
        }
        // 反向那半：**许**落的两档不许被误报。
        for allowed in VERDICTS_OPEN_TO_CAPABILITY_SUPPLIERS {
            let fake = [(
                sftp_crate.as_str(),
                DEPS,
                *allowed,
                "它的写面被一个没开的 feature 关着 / 边界判据：`the_remote_write_lives_in_exactly_one_file_and_its_root_is_exactly_staging`",
            )];
            assert!(
                capability_supplier_misfits(&fake).is_empty(),
                "`{allowed}` 是许落的档，却被误报成躲藏"
            );
        }
    }

    /// ★★ `KR79D2` 的边界那一半：**落第四档的每一条，都得点名一条真存在的判据。**
    ///
    /// 🔴 新档今天**零成员** ⇒ 真表那一格是**空真**。本条因此以合成行为主：
    /// 三种坏形状各一刀（没写边界 / 边界不成形 / 点名了一个盘上没有的判据）＋ 一刀好形状。
    #[test]
    fn the_purposeful_write_verdict_names_a_boundary_judge_that_really_exists() {
        let own = own_source();
        assert!(
            own.contains(&format!("fn {RESIDENT_GUARD}(")),
            "本条拿 `{RESIDENT_GUARD}` 当「真存在的判据」样本，而它今天不在本文件里 —— 换一个样本"
        );
        assert_eq!(
            boundary_violations(SIGNED, &own),
            Vec::<String>::new(),
            "真表上有一条落在第四档、而说不出边界 —— 逐条见上"
        );
        let good = format!("backend 就是要它写远端。{BOUNDARY_TAG}`{RESIDENT_GUARD}`");
        let cases: [(&str, &str); 4] = [
            ("没写边界", "backend 就是要它写远端，反正已经登记了"),
            ("边界不成形", "backend 就是要它写远端。边界判据：那条常驻的"),
            (
                "点名了一个盘上没有的判据",
                "backend 就是要它写远端。边界判据：`a_judge_that_was_renamed_away`",
            ),
            ("好形状", good.as_str()),
        ];
        for (what, why) in cases {
            let fake = [("某条远端写依赖", DEPS, MEASURED_WRITES_ON_PURPOSE, why)];
            let got = boundary_violations(&fake, &own);
            if what == "好形状" {
                assert!(got.is_empty(), "好形状被误报：{got:?}");
            } else {
                assert_eq!(
                    got.len(),
                    1,
                    "「{what}」这一形没被逮到 —— 那么第四档此刻就是一条没有边界的例外，\
                     而那正是 `KR79D2` 逐字点名的失效方向。实得：{got:?}"
                );
            }
        }
        // 反向：**别的档不进本条的射程** —— 否则第四档的边界会变成全表的负担。
        for other in [MEASURED_WRITES, MEASURED_CLEAN, UNMEASURED] {
            let fake = [("某条依赖", DEPS, other, "一句没有边界记号的签字")];
            assert!(
                boundary_violations(&fake, &own).is_empty(),
                "`{other}` 不该被第四档的边界规矩管到"
            );
        }
    }
}

// ══════════════════════════════════════════════════════════════════════════════
// **远端写那一半**
// ══════════════════════════════════════════════════════════════════════════════
#[cfg(test)]
mod remote_write_layer {
    //! 〔`K-R79` 09-12〕
    //! **本护栏在 C 类要走的那条路上，此前是瞎的。**
    //!
    //! # 它守的性质 · 它扫的人群（两行，各只许有一句 —— 同本文件顶上 `KG62` 那对）
    //!
    //! - **它守的性质是**：backend **进程自身**不许改动**别人机器上**的用户既有数据 —— 远端写只许住 `ssh-dial/sftp.rs` 一份、只许落在 `~/.cc-monitor/staging/`（我们自己的目录）、每一处先过 `fenced_remote`。
    //!   （本机那一半由命名空间那一条与那几种写句柄守；本层守的是**远端**那一半。）
    //! - **它扫的人群是**：本 crate `src/` 递归全部 `.rs` 的生产段源码文本里，
    //!   **把一条远端通道变成文件系统的那一步**（[`REMOTE_CAPABILITY_ANCHORS`]）
    //!   ＋ **SFTP v3 协议自己那几个改动操作的方法形**（[`REMOTE_MUTATION_VERBS`]）。
    //!
    //! # 🔴 为什么锚在这两处，而不是「`russh-sftp` 今天那几个方法名」
    //!
    //! 件计划 `KR79D1` 逐字点名的失效方向就是后者 ——「**把同一个洞换个命名空间再挖一遍**」。
    //! 两张表各自的理由逐条写在表里，这里只说那条总的：
    //!
    //! **远端写不是一个动词，是一条链**：先取得一个能改远端文件系统的**能力**，再拿它发**操作**。
    //! 按写原语的方法名取样会混进本机写的噪声；按「谁持有 SFTP 会话」取样零噪声。
    //! ⇒ **能力那一端是扼流点，动词那一端不是。** 本层因此把能力网排在第一位，
    //! 动词网只作第二道（能力取得点住在别的文件时，动词那一处仍然点得出名）。
    //!
    //! # ⚠ 它认不出什么 —— **口径与射程同句写清**（`KR79D3` 的落点是 [`BLIND_SPOT_LAYERS`]）
    //!
    //! - **远端 exec**：`channel_open_session` ＋ 把一条会写的 shell 命令交给远端。
    //!   一条命令写不写，**源码文本上判不了**（`2>/dev/null` 就带 `>`）。这与命名空间那一条认不出 `Command::new`
    //!   是**同一个洞的远端孪生** —— 盘上今天就有一处，登记在
    //!   [`the_remote_exec_hole_is_a_registered_counterexample_and_still_passes`]。
    //! - **不经 SSH 的远端写协议**（手写 HTTP PUT / 云存储 SDK / 自己封的传输层）：
    //!   往一条 TCP 流 `write_all` 几行字节就是了，**没有可穷举的形状**。
    //!   这一半的一半由 [`super::g6_dependency_signoff`] 那张签字表接（新引一条 crate 要签字），
    //!   **另一半（拿 tokio 手写协议）两层都接不住**。
    //! - **本机那一侧非 `fs::` 命名空间的写**（`os::unix::fs::symlink` 一族，08-06 实测漏过）
    //!   **不在本层射程里** —— 本层只管远端。
    //!
    //! # 它与 [`super::tests`] 那两层的关系
    //!
    //! **不改那两张表一个字。** 往 `FS_MUTATION_PATTERNS` 里塞几个 SFTP 方法名，
    //! 就是件计划点名的那个失效方向；而且那张表的语义是「本机文件系统变更」，
    //! 混进远端的东西之后，`g6_reach` 那张全表说的话会当场变得说不清。

    /// 能力家族。**闭集**，名字只有这一处住址。
    const FAMILY_SUBSYSTEM: &str = "远端文件传输子系统";
    const FAMILY_TODAY_ONLY: &str = "今天那份实现的名字（不是形状）";
    const FAMILY_PROTOCOL_OP: &str = "SFTP v3 协议操作（方法形）";
    const FAMILIES: &[&str] = &[FAMILY_SUBSYSTEM, FAMILY_TODAY_ONLY, FAMILY_PROTOCOL_OP];

    /// ★ 第一张网：**取得一条远端写能力**的那几步。`(锚点, 家族, why —— 它凭什么是形状)`
    ///
    /// ⚠ **锚点一律运行时拼**（[`capability_anchors`]），这里只放家族与理由 ——
    /// 直接写字面量的话，跨文件数它的那些判据不一定跳过本文件（同
    /// [`super::g6_dependency_signoff::an_unsigned_dependency_is_really_reported_so_that_the_zero_is_not_vacuous`]
    /// 里那条把引擎名运行时拼起来的先例）。
    const REMOTE_CAPABILITY_ANCHORS: &[(&str, &str)] = &[
        (
            FAMILY_SUBSYSTEM,
            "在 SSH 连接上开一个**子系统** —— `ssh-connection` 协议里这是**唯一**一个动作，\
             而 SFTP 就是一个子系统名。换一份 crate、换一套方法名，这一步躲不掉；\
backend 今天**只在一处**开子系统（`ssh-dial/sftp.rs`，SFTP 住本机常驻后端）\
             ⇒ 这一条在那一份之外恒零，出现在第二份即越线（`REMOTE_WRITE_MODULE`）。",
        ),
        (
            FAMILY_SUBSYSTEM,
            "SFTP **会话类型名**：按写原语的方法名取样混进本机噪声，按「谁持有 SFTP 会话」零噪声 \
             ⇒ **会话对象是远端写那条路的扼流点**，方法名不是。",
        ),
        (
            FAMILY_SUBSYSTEM,
            "SFTP 协议自己的**打开标志集**（`SSH_FXF_WRITE` / `CREAT` / `TRUNC` / `APPEND`）。\
             要在远端建 / 截断 / 追加一个文件，协议层必须发这几位 —— \
             它是**协议常量**，不是某个 crate 的选择。",
        ),
        (
            FAMILY_TODAY_ONLY,
            "🔴 **本条如实标成「今天恰好如此」，它不是形状**：这是仓里那份 SFTP 实现今天的 \
             crate 路径（monitor 侧清单上的 `russh-sftp`）。留着它，是因为「照着 monitor \
             那份抄过来」是 C 类最可能的落法；**换一个 crate 它就瞎了** —— \
             顶上那三条才是躲不掉的那几步。",
        ),
    ];

    /// ★ 第二张网：**SFTP v3 协议的改动操作**，取**方法形**（带前导点）。
    ///
    /// `(锚点, 它对应协议里的哪一个包)`
    ///
    /// # 🔴 为什么这不是「列 `russh-sftp` 那几个方法名」
    ///
    /// 这几个名字**不是某份 crate 起的**，是 SFTP v3 协议自己的操作名
    /// （`SSH_FXP_REMOVE` / `RENAME` / `MKDIR` / `RMDIR` / `SETSTAT` / `SYMLINK`）——
    /// 任何一份 SFTP 客户端都会用它们，因为协议就是这么定的。
    ///
    /// # ⚠ 它的口径（两个方向都写出来）
    ///
    /// - **取方法形**（带前导点）是为了与本机那一层分开：`std::fs::remove_file(` 是路径形，
    ///   命名空间那一条已经在数；`sftp.remove_file(` 是方法形，命名空间那一条一条都不匹配 —— 那正是要修的。
    /// - 它是**超集**：本机某个**恰好同名**的方法也会红（今天树上 **0 处**，分母 = 本 crate
    ///   `src/` 生产段全部 `.rs`，由 [`the_verb_net_has_no_false_positive_on_this_tree_today`] 现打）。
    /// - 🔴 **`write_all(` 刻意不在表上**：backend 唯一合法的写就是把 wire 帧写 stdout
    ///   （`main.rs` / `wire.rs` / `listen.rs` 那三处），把它收进来等于把本层做成一条恒红的闸，
    ///   而一条恒红的闸活不过一轮 —— 它会被人关掉。**这一格是想过的，不是漏的。**
    const REMOTE_MUTATION_VERBS: &[(&str, &str)] = &[
        ("remove_file", "SSH_FXP_REMOVE —— 删远端一份文件"),
        ("rename", "SSH_FXP_RENAME —— 给远端一份文件改名 / 换位"),
        ("create_dir", "SSH_FXP_MKDIR —— 在远端建目录"),
        ("remove_dir", "SSH_FXP_RMDIR —— 删远端一个目录"),
        (
            "set_metadata",
            "SSH_FXP_SETSTAT —— 改远端一份文件的属性 / 权限 / 长度",
        ),
        ("symlink", "SSH_FXP_SYMLINK —— 在远端建符号链接"),
    ];

    /// ★ 第三张网，**它不扫源码，扫的是清单** —— `(词, 它是哪一个远端写协议)`。
    ///
    /// 一条依赖能不能远端写，最便宜的可判信号是**它的名字**：远端文件传输那几个协议
    /// 各自只有一个名字，而实现它们的 crate 几乎一定把那个名字放进包名里
    /// （今天仓里那份就叫 `russh-sftp`）。
    ///
    /// # ⚠ 口径与射程同句写清
    ///
    /// 它拦得住「照着 monitor 那份把 `russh-sftp` 抄过来」这一形（C 类最可能的落法）；
    /// **拦不住**一个不叫这些名字、却能远端写的 crate（云存储 SDK / 自己封的传输层 /
    /// 通用 HTTP 客户端）。那一半的落点是签字那一行本身（人写的），不是这张表。
    ///
    /// 用途住 [`super::g6_dependency_signoff::capability_supplier_misfits`] ——
    /// **名字落在这张表上的依赖，不许判成「没量过 / 没见写面」那两档。**
    pub(super) const REMOTE_WRITE_PROTOCOL_WORDS: &[(&str, &str)] = &[
        ("sftp", "SSH 文件传输子系统（SFTP v3）"),
        ("scp", "SSH 上的远端拷贝"),
        ("rsync", "增量同步协议，写面比 SFTP 还大（它会删）"),
        ("webdav", "HTTP 上的远端文件系统"),
    ];

    /// 能力网的锚点，**运行时拼**（理由见 [`REMOTE_CAPABILITY_ANCHORS`] 头注）。
    ///
    /// 次序与那张表**逐格对齐** —— 由 [`the_two_capability_tables_stay_aligned`] 钉着。
    fn capability_needles() -> Vec<String> {
        let sub = "sub";
        let ses = "Session";
        vec![
            format!("request_{sub}system("),
            format!("Sftp{ses}"),
            format!("Open{}::", "Flags"),
            format!("russh{}sftp", "_"),
        ]
    }

    /// 动词网的锚点：方法形 = `.` ＋ 协议操作名 ＋ `(`。**现算，不写第二份字面量。**
    fn verb_needles() -> Vec<String> {
        REMOTE_MUTATION_VERBS
            .iter()
            .map(|(op, _)| format!(".{op}("))
            .collect()
    }

    /// 两张网合起来 —— 给**本 crate 之外**的使用者（今天只有
    /// [`super::g6_dependency_signoff::resident_ruler`]）。
    ///
    /// 🔴 **它是本层唯一的对外口**：谁要拿这两张网去量别的树，走这里，
    /// 别在别处拷一份针（拷一份就会漂，而漂了不会有人知道 —— 本仓 `E12`）。
    pub(super) fn all_remote_needles() -> Vec<String> {
        let mut v = capability_needles();
        v.extend(verb_needles());
        v
    }

    /// 本层判据本体：这段生产段源码里有没有远端写。**纯函数** —— 两个方向都切得动。
    ///
    /// 返回 `(命中的锚点, 家族)`。
    pub(super) fn violates_remote_write_layer(prod: &str) -> Option<(String, &'static str)> {
        for (needle, (family, _)) in capability_needles()
            .into_iter()
            .zip(REMOTE_CAPABILITY_ANCHORS.iter())
        {
            if prod.contains(needle.as_str()) {
                return Some((needle, family));
            }
        }
        for needle in verb_needles() {
            if prod.contains(needle.as_str()) {
                return Some((needle, FAMILY_PROTOCOL_OP));
            }
        }
        None
    }

    /// 远端 **exec** 那条路的锚点 —— 它**不是**本层的判据，是本层**认不出的那一格**的量具。
    ///
    /// 取 `channel_open_session(`：SSH 里拿到一条能跑命令的远端通道，协议上只有这一步
    /// （`dial_locality` 那张表数的是「跑不跑传输层握手」，是另一维，两者刻意不合并）。
    fn remote_exec_needle() -> String {
        format!("channel_open_{}(", "session")
    }

    /// 本 crate `src/` 递归全部 `.rs` 的**生产段**：`(相对 src 的路径, 正文)`。
    ///
    /// ⚠ 先前这里写着「**`scan_tree!` 按构造摘掉调用者自己那一份**
    /// （就是本护栏文件）—— 这里刻意不补回来」。**那一刀在这一处不生效**
    /// （判据由 `#[path]` 挂载 ⇒ `file!()` 是折返路径 ⇒ 后缀比不命中），
    /// 而且本护栏文件住 `tests/backend/`、本来就不在这里扫的 `src/backend` 那棵树里。
    /// ⇒ **不补回来这件事照旧**，理由与 `super::tests::scan` 跳过本文件逐字同一条：
    /// 本文件整体在 `#[cfg(test)]` 内、生产段是空的，而它的锚点表本身就是一串会自匹配的字面量。
    fn production_tree() -> Vec<(String, String)> {
        let root = guard_core::addr::backend_src_root();
        let mut out: Vec<(String, String)> = Vec::new();
        for (path, src) in guard_core::scan_tree!(&root, &["rs"]) {
            let rel = path
                .strip_prefix(&root)
                .unwrap_or(&path)
                .to_string_lossy()
                .replace('\\', "/");
            out.push((rel, guard_core::production_source(&src)));
        }
        out.sort();
        out.dedup_by(|a, b| a.0 == b.0);
        out
    }

    /// 扫描面正控：入口 `main.rs` 在人群里且剥完不是空的 —— 挡「树没扫到」与「每一份都被剥空」
    /// （`guard_support` 那条头注记着这两者互相掩盖过）。
    fn assert_tree_is_real(tree: &[(String, String)]) {
        assert!(
            tree.iter()
                .any(|(rel, prod)| rel == "main.rs" && prod.contains("fn main")),
            "扫描面里没有剥完仍带 `fn main` 的 `main.rs`（扫到 {} 份）—— 抽取坏了，本条在空转",
            tree.len()
        );
    }

    // ── `KR79D3`：**盲区读数** ────────────────────────────────────────────────

    /// 一层的分母：**要么数得出来，要么说清为什么数不出来。**
    ///
    /// 🔴 这个 `enum` 就是件计划那条失效方向的机器化落点 ——
    /// 「报『已守住』而分母只算认得出的那些」在这里**写不出来**：
    /// 数不出来的那一层必须是 [`Denominator::Uncountable`]，而它带着一句为什么。
    #[derive(Debug)]
    enum Denominator {
        Counted(usize),
        Uncountable(&'static str),
    }

    impl Denominator {
        fn show(&self) -> String {
            match self {
                Denominator::Counted(n) => format!("{n} 处"),
                Denominator::Uncountable(why) => format!("**给不出** —— {why}"),
            }
        }
        fn counted(&self) -> Option<usize> {
            match self {
                Denominator::Counted(n) => Some(*n),
                Denominator::Uncountable(_) => None,
            }
        }
    }

    /// 一层的读数：**有几处 · 其中闸看得见几处 · 其中「看得见却判不了它写什么」几处**。
    struct LayerReading {
        total: Denominator,
        seen: Denominator,
        blind: Denominator,
    }

    impl LayerReading {
        /// 一行。数得出来的三格并排印；**数不出来的那一层只印一次那句为什么**
        /// （三格印三遍同一句话，读的人会以为是三件事）。
        fn show(&self) -> String {
            match (&self.total, &self.seen, &self.blind) {
                (Denominator::Counted(t), Denominator::Counted(s), Denominator::Counted(b)) => {
                    format!("处数 {t} 处 · 闸看得见 {s} 处 · 看得见却判不了它写什么 {b} 处")
                }
                _ => format!("处数 / 看得见 / 判不了 三格一律 {}", self.total.show()),
            }
        }
    }

    type Corpus = [(String, String)];

    /// 语料里某个针的**处数**合计（不是文件数）。
    fn hits(corpus: &Corpus, needle: &str) -> usize {
        corpus.iter().map(|(_, c)| c.matches(needle).count()).sum()
    }

    /// L1 本机直写：分母 = 本护栏那两张模式表 × 生产段。
    fn layer_local_direct(corpus: &Corpus) -> LayerReading {
        let n: usize = super::tests::FS_MUTATION_PATTERNS
            .iter()
            .chain(super::tests::WHITELIST_STILL_FORBIDDEN.iter())
            .map(|p| hits(corpus, p))
            .sum();
        LayerReading {
            total: Denominator::Counted(n),
            // 落在两张模式表射程里的每一处，命名空间那一条都看得见（看得见 ≠ 会红：
            // 白名单那两份模块里的 `O_EXCL` 新建是**看得见并且被允许**的）。
            seen: Denominator::Counted(n),
            blind: Denominator::Counted(0),
        }
    }

    /// L2 本机代理写（起进程）：分母 = `spawn_registry::ALLOWED` 的登记条数。
    fn layer_local_proxy(_corpus: &Corpus) -> LayerReading {
        let n = super::spawn_registry::ALLOWED.len();
        LayerReading {
            total: Denominator::Counted(n),
            seen: Denominator::Counted(n),
            // 每一条都登记了「起什么」，**一条都判不了被起的那个程序写了什么** ——
            // 盘上就有活体（`control/cc_bus.rs` 那条起 `cc-kill` 的，`g6_reach` 的反例表）。
            blind: Denominator::Counted(n),
        }
    }

    /// L3 远端能力写：分母 = 本层两张网 × 生产段。
    fn layer_remote_capability(corpus: &Corpus) -> LayerReading {
        let n: usize = capability_needles()
            .iter()
            .chain(verb_needles().iter())
            .map(|p| hits(corpus, p.as_str()))
            .sum();
        LayerReading {
            total: Denominator::Counted(n),
            seen: Denominator::Counted(n),
            blind: Denominator::Counted(0),
        }
    }

    /// L4 远端代理写（远端 exec）：分母 = 远端 session channel 的处数。
    fn layer_remote_proxy(corpus: &Corpus) -> LayerReading {
        let n = hits(corpus, remote_exec_needle().as_str());
        LayerReading {
            total: Denominator::Counted(n),
            seen: Denominator::Counted(n),
            blind: Denominator::Counted(n),
        }
    }

    /// L5 不经 SSH 的远端写协议：**分母给不出。**
    fn layer_other_protocol(_corpus: &Corpus) -> LayerReading {
        const WHY: &str = "手写一次 HTTP PUT 就是往一条 TCP 流 `write_all` 几行字节，\
             没有可穷举的文本形状；新引一条 crate 那一半由依赖签字表接得住，\
             拿 tokio 自己封传输层那一半两层都接不住";
        LayerReading {
            total: Denominator::Uncountable(WHY),
            seen: Denominator::Uncountable(WHY),
            blind: Denominator::Uncountable(WHY),
        }
    }

    /// `(层名, 现算那一层读数的函数, 这一层认不出什么)`。
    ///
    /// 🔴 **每一层的数都由一个函数现算** —— 表里一个手写的数都没有。
    #[allow(clippy::type_complexity)]
    const BLIND_SPOT_LAYERS: &[(&str, fn(&Corpus) -> LayerReading, &str)] = &[
        (
            "L1 本机直写",
            layer_local_direct,
            "分母按**文本形状**取，不按「真的会写」取 —— 换个命名空间就掉出分母\
             （`os::unix::fs::symlink` / `fs::set_permissions`，08-06 实测漏过）",
        ),
        (
            "L2 本机代理写（起进程）",
            layer_local_proxy,
            "登记的是「起什么」，**判不了被起的那个程序写了什么**",
        ),
        (
            "L3 远端能力写（新加）",
            layer_remote_capability,
            "能力网认的是**取得能力**那一步、动词网认的是**协议操作名的方法形**；\
             两者都是源码文本 ⇒ `use` 别名 · 宏里拼出来的调用 · 自己改名的封装，一律看不见",
        ),
        (
            "L4 远端代理写（远端 exec）",
            layer_remote_proxy,
            "**那条命令写不写，源码文本上判不了** —— 与命名空间那一条认不出 `Command::new` 同一个洞的远端孪生",
        ),
        (
            "L5 不经 SSH 的远端写协议",
            layer_other_protocol,
            "见 [`layer_other_protocol`] 里那一句 —— **这一层连分母都给不出**",
        ),
    ];

    // ── 判据 ─────────────────────────────────────────────────────────────────

    /// ★ 全表：**能力网 / 动词网的每一个形状，逐形喂一个合成样本，逐形要求它红。**
    ///
    /// 形状照本文件 [`super::g6_reach::every_default_layer_shape_reds_on_its_own_sample`]。
    #[test]
    fn every_remote_write_shape_reds_on_its_own_sample() {
        let caps = capability_needles();
        assert_eq!(
            caps.len(),
            REMOTE_CAPABILITY_ANCHORS.len(),
            "能力网的针 {} 条、理由 {} 条 —— 两张表脱钩了",
            caps.len(),
            REMOTE_CAPABILITY_ANCHORS.len()
        );
        assert!(
            caps.len() >= 3 && !REMOTE_MUTATION_VERBS.is_empty(),
            "两张网被掏空了（能力 {} 条 · 动词 {} 条）—— 本层此刻在空转",
            caps.len(),
            REMOTE_MUTATION_VERBS.len()
        );
        for needle in &caps {
            let sample = format!("async fn f(ch: C) {{ let s = {needle}x(ch).await?; }}");
            let got = violates_remote_write_layer(&sample);
            assert_eq!(
                got.as_ref().map(|(n, _)| n.as_str()),
                Some(needle.as_str()),
                "能力网对 `{needle}` 这个形状不响了 —— 全表里这一格今天是空的"
            );
        }
        for needle in verb_needles() {
            let sample = format!("async fn f(s: S, p: String) {{ let _ = s{needle}p).await; }}");
            let got = violates_remote_write_layer(&sample);
            assert_eq!(
                got.as_ref().map(|(n, _)| n.as_str()),
                Some(needle.as_str()),
                "动词网对 `{needle}` 这个形状不响了 —— 全表里这一格今天是空的"
            );
            assert_eq!(
                got.map(|(_, f)| f),
                Some(FAMILY_PROTOCOL_OP),
                "`{needle}` 的家族标错了"
            );
        }
        // 反向的反向：**远端只读**与**本机只读**都不许被误判成写（误红最省事的消法是把判据删掉）。
        for clean in [
            "let s = sftp.read(path.to_string()).await.ok();",
            "let m = sftp.metadata(p).await?;",
            "let s = std::fs::read_to_string(p)?;",
            "w.write_all(line.as_bytes()).await?;",
        ] {
            assert_eq!(
                violates_remote_write_layer(clean),
                None,
                "只读 / 写 stdout 被本层误判成远端写：{clean}"
            );
        }
    }

    /// ★ 两张能力表**逐格对齐**，家族在闭集里，理由有长度地板。
    #[test]
    fn the_two_capability_tables_stay_aligned() {
        for (family, _) in REMOTE_CAPABILITY_ANCHORS {
            assert!(
                FAMILIES.contains(family),
                "`{family}` 不在家族闭集里：{FAMILIES:?}"
            );
        }
        for (op, why) in REMOTE_MUTATION_VERBS {
            assert!(
                why.contains("SSH_FXP"),
                "动词 `{op}` 没说清它对应协议里哪一个包（现文：{why}）—— \
                 说不出协议名的动词，就是「今天那几个方法名」，正是件计划点名的失效方向"
            );
        }
    }

    // ═══ **远端写只住一份文件、只许落暂存区** ═══════════════
    //

    /// ★ **远端写的唯一住址**：`(仓库相对路径, why —— 它写什么、路径由谁定、哪一道围栏拦着)`。
    const REMOTE_WRITE_MODULE: (&str, &str) = (
        "ssh-dial/sftp.rs",
        "SFTP 住本机常驻后端：暂存区的上传件（`control/transfer.rs` 经它写）· \
         自部署的后端二进制 / `.build_id` / `ccm` 入口（部署链路 `use:\"files\"` 经它写）。\
         路径由 monitor 给、由本文件的 `fenced_remote` 判：词法（只许暂存区）＋ 父目录解链接仍在根下 ＋ \
         开写前 `lstat` 拒链接",
    );

    /// 🔴 **写根的期望值手写**（「只许往远端 `~/.cc-monitor/staging/` 写；换进安装位置 · 卸载由传上去的那一份在那台上自己做」），
    /// **不取自被判文件的常量**（两侧同源 = 恒真）。
    const EXPECTED_WRITE_ROOTS: [&str; 1] = [".cc-monitor/staging"];

    /// 被判文件里写根那一行的锚（运行时拼，免得本文件自己也命中）。
    fn roots_anchor() -> String {
        format!("const STAGING_{}: &str =", "ROOT")
    }

    /// 围栏调用的针（远端那一道）。
    fn fence_needle() -> String {
        format!("fenced_{}(", "remote")
    }

    /// 「开写」那一形的针：协议打开标志里会改东西的那三位（`SSH_FXF_CREAT` / `WRITE` / `TRUNC`）。
    fn write_open_needles() -> Vec<String> {
        ["CREATE", "WRITE", "TRUNCATE"]
            .iter()
            .map(|f| format!("Open{}::{f}", "Flags"))
            .collect()
    }

    /// 从一行 `… = ["a", "b"];` 里抠出双引号里的串（纯函数）。
    fn quoted(line: &str) -> Vec<String> {
        line.split('"')
            .enumerate()
            .filter(|(i, _)| i % 2 == 1)
            .map(|(_, t)| t.to_string())
            .collect()
    }

    /// 按**顶格**函数头切块，回 `(函数名, 块)`。认 `fn` / `pub fn` / `pub(crate) fn` / `async fn` 及其组合
    /// （本文件 `tests::fn_chunks` 只认前两形，而被判文件里全是 `pub(crate) async fn`）。
    pub(super) fn fn_blocks(prod: &str) -> Vec<(String, String)> {
        let mut out: Vec<(String, String)> = Vec::new();
        let mut cur: Option<(String, String)> = None;
        for src_row in prod.split_inclusive('\n') {
            let at_col0 = !src_row.starts_with(' ') && !src_row.starts_with('\t');
            let words: Vec<&str> = src_row.split_whitespace().collect();
            let fn_at = words.iter().position(|w| *w == "fn");
            let head = at_col0
                && fn_at.is_some_and(|k| {
                    words[..k]
                        .iter()
                        .all(|w| *w == "pub" || *w == "async" || w.starts_with("pub("))
                });
            if head {
                if let Some(done) = cur.take() {
                    out.push(done);
                }
                let name: String = words[fn_at.unwrap_or(0) + 1]
                    .chars()
                    .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
                    .collect();
                cur = Some((name, String::new()));
            }
            if let Some((_, body)) = cur.as_mut() {
                body.push_str(src_row);
            }
        }
        if let Some(done) = cur.take() {
            out.push(done);
        }
        out
    }

    /// 判据 ③ 的纯函数那一半：一个函数块里**第一个远端改动**（动词网 ∪ 开写标志）之前有没有围栏调用。
    /// 回没过围栏的那几处 `函数名 → 那个改动`。
    pub(super) fn unfenced_remote_mutations(prod: &str) -> Vec<String> {
        let mut muts = verb_needles();
        muts.extend(write_open_needles());
        let fence = fence_needle();
        let mut bad = Vec::new();
        for (name, body) in fn_blocks(prod) {
            let first = muts
                .iter()
                .filter_map(|m| body.find(m.as_str()).map(|k| (k, m.clone())))
                .min_by_key(|(k, _)| *k);
            let Some((mk, which)) = first else { continue };
            if !body.find(fence.as_str()).is_some_and(|fk| fk < mk) {
                bad.push(format!("{name}  →  {which}"));
            }
        }
        bad
    }

    /// ★★ 正题 ①②：**远端写只住一份文件**（两向相等），**它声明的写根 == 暂存区那一处**（相等，异源）。
    #[test]
    fn the_remote_write_lives_in_exactly_one_file_and_its_root_is_exactly_staging() {
        let tree = production_tree();
        assert_tree_is_real(&tree);
        let found: std::collections::BTreeSet<String> = tree
            .iter()
            .filter(|(_, prod)| violates_remote_write_layer(prod).is_some())
            .map(|(rel, _)| rel.clone())
            .collect();
        let want: std::collections::BTreeSet<String> =
            [REMOTE_WRITE_MODULE.0.to_string()].into_iter().collect();
        assert_eq!(
            found,
            want,
            "\n命中远端写能力网 / 动词网的后端文件，与登记的那一份对不上。\n  \
             盘上有、表里没有（🔴 **远端写长到了第二份文件里**）：{:?}\n  \
             表里有、盘上没有（那一份搬走了 / 不写了 ⇒ 同轮改登记）：{:?}\n\n\
             SFTP 进本机常驻后端、只写暂存区。写原语只许住一份文件、\
             每个都先过那一道围栏 —— 第二份文件里的远端写就是一个没人审过的洞。",
            found.difference(&want).collect::<Vec<_>>(),
            want.difference(&found).collect::<Vec<_>>()
        );
        assert!(
            REMOTE_WRITE_MODULE.1.trim().chars().count() >= 30,
            "远端写的住址没写清它写什么"
        );
        let prod = tree
            .iter()
            .find(|(rel, _)| rel == REMOTE_WRITE_MODULE.0)
            .map(|(_, p)| p.as_str())
            .expect("上面刚判过它在");
        let anchor = roots_anchor();
        let rows: Vec<&str> = prod
            .lines()
            .filter(|l| l.contains(anchor.as_str()))
            .collect();
        assert_eq!(
            rows.len(),
            1,
            "写根那一行在 `{}` 里应当恰好一处（锚 `{anchor}`），实得 {}",
            REMOTE_WRITE_MODULE.0,
            rows.len()
        );
        let got: std::collections::BTreeSet<String> = quoted(rows[0]).into_iter().collect();
        let want_roots: std::collections::BTreeSet<String> =
            EXPECTED_WRITE_ROOTS.iter().map(|r| r.to_string()).collect();
        assert_eq!(
            got, want_roots,
            "远端写根与「暂存区」那一处对不上 —— 多一处 = 写能力扩散；少了 = 上传 · 部署落不下去"
        );
        println!(
            "SR1b：本趟扫 {} 份生产段源文件，远端写只在 {:?}，写根 {:?}",
            tree.len(),
            found,
            got
        );
    }

    /// ★★ 正题 ③：**那一份文件里，每个含远端改动的函数，第一个改动之前先过 `fenced_remote`**
    /// （形状照从前本机文件管理面那条「先过路径解析」的按位置判据，那一条随类型挡删了；漏判面同它：判顺序，判不了「动的就是判过的那一个」——
    /// 那一半靠 `dial_sftp_tests` 的行为判据：合成服务端改动表的根集合 == 两处、越界零改动）。
    #[test]
    fn every_remote_mutation_in_that_file_is_fenced_first() {
        let tree = production_tree();
        let prod = tree
            .iter()
            .find(|(rel, _)| rel == REMOTE_WRITE_MODULE.0)
            .map(|(_, p)| p.clone())
            .unwrap_or_else(|| panic!("`{}` 不在盘上", REMOTE_WRITE_MODULE.0));
        // 反空真：被判文件里真有改动、真有函数块（否则下面那个空表在空人群上成立）。
        let blocks = fn_blocks(&prod);
        let mutating = blocks
            .iter()
            .filter(|(_, b)| {
                verb_needles()
                    .iter()
                    .chain(write_open_needles().iter())
                    .any(|m| b.contains(m.as_str()))
            })
            .count();
        assert!(
            blocks.len() >= 10 && mutating >= 3,
            "只切出 {} 个函数块、其中 {mutating} 个含远端改动 —— 切法坏了，本条在空转",
            blocks.len()
        );
        let bad = unfenced_remote_mutations(&prod);
        assert!(
            bad.is_empty(),
            "`{}` 里有远端改动没先过围栏：\n  {}\n\n\
             每一处远端改动之前必须先有一次 `fenced_remote(` —— 那是「只许两处」在调用点上的形状。",
            REMOTE_WRITE_MODULE.0,
            bad.join("\n  ")
        );
    }

    /// 判据 ③ 的切法与判定**逐形喂样本**（两向）：三种函数头都切得出；先动后判 ⇒ 红；先判后动 ⇒ 不红。
    #[test]
    fn the_fenced_first_judge_bites_on_samples() {
        let verb = verb_needles()[0].clone();
        let fence = fence_needle();
        let flag = write_open_needles()[1].clone();
        let sample = format!(
            "pub(crate) async fn good(s: &S) {{\n    let r = {fence}s, p).await?;\n    s.sftp{verb}r).await;\n}}\n\
             async fn bad(s: &S) {{\n    s.sftp{verb}p).await;\n    let _ = {fence}s, p);\n}}\n\
             pub fn bad_open(s: &S) {{\n    s.open(p, {flag});\n}}\n\
             fn reads_only(s: &S) {{\n    s.read(p);\n}}\n"
        );
        let names: Vec<String> = fn_blocks(&sample).into_iter().map(|(n, _)| n).collect();
        assert_eq!(
            names,
            ["good", "bad", "bad_open", "reads_only"],
            "切法认不全三种函数头"
        );
        let bad = unfenced_remote_mutations(&sample);
        assert_eq!(bad.len(), 2, "判定没咬住那两处：{bad:?}");
        assert!(
            bad[0].starts_with("bad ") && bad[1].starts_with("bad_open "),
            "{bad:?}"
        );
        assert_eq!(
            quoted("pub(crate) const X: [&str; 2] = [\".a/b\", \".c/d\"];"),
            [".a/b", ".c/d"]
        );
    }

    /// ★ 动词网是**超集** —— 它今天在这棵树上的误红处数，是一个要现打的读数，不是一句话。
    ///
    /// 上面那条把两张网合起来判「只在一份文件里」；本条把**动词网单独**拎出来、在**那一份之外**再数一遍，
    /// 是因为两者说的不是同一件事：合起来那个结论里，动词网那一半有可能靠
    /// 「本机恰好没有同名方法」撑着，而那是**今天的巧合**，不是本层的性质。
    /// ⇒ 哪天它红了，先读这条的报错：**它多半不是一次越线，是一次同名误伤**。
    /// 登记的那一份（`ssh-dial/sftp.rs`）里的动词是真的远端写，不算误伤 ⇒ 不在本条人群里。
    #[test]
    fn the_verb_net_has_no_false_positive_on_this_tree_today() {
        let tree = production_tree();
        let mut hit: Vec<String> = Vec::new();
        for (rel, prod) in &tree {
            if rel == REMOTE_WRITE_MODULE.0 {
                continue;
            }
            for needle in verb_needles() {
                let n = prod.matches(needle.as_str()).count();
                if n > 0 {
                    hit.push(format!("  src/{rel}：{n} 处 `{needle}`"));
                }
            }
        }
        assert!(
            hit.is_empty(),
            "动词网在本机代码上命中了 {} 处：\n{}\n\n\
             ⚠ **先判是哪一种**：\n\
             ① 真的长出了远端写 ⇒ 它只许住 `{}`、先过那一道围栏；\n\
             ② 本机某个方法**恰好同名**（动词网是超集，头注逐字写着这件事）\
             ⇒ 那要么给这一处一条**逐条登记的例外**（写清它写的是本机的什么），\
             要么把动词网那一条换成更窄的形状。**不许直接把那一条从表上删掉。**",
            hit.len(),
            hit.join("\n"),
            REMOTE_WRITE_MODULE.0
        );
    }

    /// ★★ **远端 exec 那个洞：它今天就在盘上，而本层认不出它** —— 登记，不假装覆盖。
    ///
    /// 形状照 [`super::g6_reach::the_counterexample_is_still_on_the_board_and_still_passes`]。
    /// 三件事一起断：① 它还在盘上（`dial/` 底下恰好一处）· ② 本层对它**不红**
    /// （它属「本来就拦不住」那一侧，不是「放宽之后没红」）· ③ **同形的子系统直写会红**
    /// —— 第三条就是把这两句话分开的那一刀。
    #[test]
    fn the_remote_exec_hole_is_a_registered_counterexample_and_still_passes() {
        let tree = production_tree();
        let needle = remote_exec_needle();
        let sites: Vec<&str> = tree
            .iter()
            .filter(|(_, c)| c.contains(needle.as_str()))
            .map(|(rel, _)| rel.as_str())
            .collect();
        // 住址 `dial/mod.rs` → `ssh-dial/uses.rs`：拨号代理拆成三份，开 channel 之后那一段
        //   住 `uses.rs`。处数仍是**一份文件**；它里面多了 `capture` 那一臂（同一条 exec，收全输出）——
        //   那一臂此前在界面进程里（`stream_source::connect_and_exec_capture`），是搬家，不是新长的能力。
        assert_eq!(
            sites,
            vec!["ssh-dial/uses.rs"],
            "远端 session channel 的处数 / 住址变了：{sites:?}\n\
             它是本层**认不出的那一格**的活体标本 —— 变了就回来重读这条登记，\
             别只改这个断言。（`dial_locality` 管的是「跑不跑传输层握手」，是另一维。）"
        );
        for (rel, prod) in &tree {
            if rel == "ssh-dial/uses.rs" {
                assert_eq!(
                    violates_remote_write_layer(prod),
                    None,
                    "反例 `ssh-dial/uses.rs` 今天**红了** —— 那说明本层的射程变了，\
                     这条登记说的话已经不成立，回来重判"
                );
            }
        }
        // ★ 把「它本来就拦不住」与「放宽之后没红」分开：**同一条通道上开子系统**，本层当场红。
        let same_thing_via_subsystem = format!(
            "let ch = session.channel_open_{}().await?; ch.request_{}system(true, \"sftp\").await?;",
            "session", "sub"
        );
        assert!(
            violates_remote_write_layer(&same_thing_via_subsystem).is_some(),
            "同一条通道上开子系统都不红了 —— 那就不是「人群够不着」，是本层坏了"
        );
    }

    /// ★★ `KR79D3`：**这道闸的盲区，逐层带分母** —— 报读数，不报「已守住」。
    #[test]
    fn the_blind_spot_reading_carries_a_denominator_for_every_layer_it_can_count() {
        let tree = production_tree();
        assert_tree_is_real(&tree);
        let mut lines: Vec<String> = Vec::new();
        let mut countable = 0usize;
        let mut uncountable = 0usize;
        let (mut total, mut seen, mut blind) = (0usize, 0usize, 0usize);
        for (name, read, cannot) in BLIND_SPOT_LAYERS {
            let r = read(&tree);
            match (r.total.counted(), r.seen.counted(), r.blind.counted()) {
                (Some(t), Some(s), Some(b)) => {
                    countable += 1;
                    total += t;
                    seen += s;
                    blind += b;
                    assert!(
                        s <= t && b <= t,
                        "`{name}` 的读数自相矛盾：处数 {t} · 看得见 {s} · 判不了 {b}"
                    );
                }
                _ => {
                    uncountable += 1;
                    assert!(
                        matches!(r.total, Denominator::Uncountable(w) if w.trim().chars().count() >= 20),
                        "`{name}` 说自己数不出来，却没写清为什么 —— \
                         「数不出来」是三值里最贵的一个，不许拿一句话换"
                    );
                }
            }
            lines.push(format!("  {name}：{} ｜ 认不出：{cannot}", r.show()));
        }
        assert_eq!(
            (countable, uncountable),
            (4, 1),
            "分层表的形状变了（数得出 {countable} 层 · 数不出 {uncountable} 层）—— \
             回来重读 `BLIND_SPOT_LAYERS`：**少掉那一层「数不出来」的，本张读数就退回成\
             「拿命中当全集」**，那正是 `KR79D3` 点名的失效方向。"
        );
        // 🔴 反空真：数得出的四层里，**至少一层今天真有处数**，否则整张读数是在空集上说话。
        assert!(
            total > 0,
            "数得出的四层加起来是 0 处 —— 尺子全瞎了，这张读数此刻什么也不说明"
        );
        // 🔴 **判不了的那几处不许是 0** —— 它是 0 就说明这张表把自己粉饰过了
        //    （盘上今天真有：起进程 7 处 ＋ 远端 exec 1 处）。
        assert!(
            blind > 0,
            "「看得见却判不了它写什么」合计 0 处 —— 盘上明明有起进程与远端 exec 两族，\
             这张读数正在把自己说得比实际干净"
        );
        println!(
            "K-R79 盲区读数（量于本趟 `cargo test`；口径 = 本 crate `src/` 递归全部 `.rs` 的\
             **生产段**，剥 `#[cfg(test)]`，{} 份 / {} 字节）：\n{}\n  \
             合计：数得出的四层共 {total} 处，其中闸看得见 {seen} 处、\
             看得见却判不了它写什么 {blind} 处；**另有 1 层连分母都给不出**（L5）。\n  \
             ⚠ 这个「{seen}/{total}」**不是覆盖率** —— 分母本身是按文本形状取的，\
             L5 那一层根本没进分母。",
            tree.len(),
            tree.iter().map(|(_, c)| c.len()).sum::<usize>(),
            lines.join("\n")
        );
    }
}
