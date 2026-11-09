//! backend 只读机器护栏（红线 I7 的机器化守护）。
//!
//! # 性质与人群，两行逐字
//!
//! - **它守的性质是**：backend **进程自身**不许改动用户既有数据 —— 碰盘只经 `platform/fs/` 交出去的句柄：读经 `Ro`（没有写方法）· 改用户文件只经文件管理面的 `Face`（写方法只收已过路径解析的 `Resolved`，只递给命令表文件管理那一族）· 分叉只经只会 `O_EXCL` 新建的 `NewFile` · 后端自家状态只经 `Own`（只写家目录的 `.cc-monitor` 与数据目录底下）。
//! - **它扫的人群是**：`src/backend/` 底下各包里调碰盘标准库与系统调用的地方（`platform/fs/` 之外一处都不许，由 clippy 按解析后的路径认）·
//!   造写句柄 · 拿跨进程锁的地方（同一份名单，逐处放行）· 要文件管理写句柄的处理器（同一份名单）· 起进程的地方（`Child::new` · `host::tmux::child` 同一份名单逐处放行、写清起什么；`Command::new` 只许住起子进程原语 `platform/child.rs`）。
//!
//! # 怎么挡：一处实现，四种句柄
//!
//! 挡在类型上，不靠扫每一处 `fs::` 调用再逐张表登记：
//!
//! - 碰盘的标准库与系统调用（`std::fs` · `tokio::fs` 的各函数与开法、`os::unix::fs` 的建链接 · 改属主与扩展特征上那几样、`libc` 的 `*at` 一族）
//!   列在后端 `clippy.toml` 碰盘那一组（deny），只在 `platform/fs/` 的 `ro` · `face` · `own` 逐处 `expect` 放行、写明理由；
//!   起别名 · 分组导入照认；放行写在用不到它的地方 ⇒「期望没满足」（警告，不红）。⚠ 只在 clippy 里红、`cargo test` 不红；Windows 那几臂只有门禁 `winchk-backend` 看得见。
//!   名单认不出的几形写在那份 toml 的头注里（`Path` 上的读方法 · 经 `From` 的换法 · `libc::syscall` 裸调 · 手写 `extern`）。
//! - 读不用递句柄：`Ro` 没有写方法，`Ro::open` 交回的那一份不实现 `Write`（[`tests::the_read_only_file_has_no_write`]）。
//! - 写要拿到句柄，句柄只在该造的那几处造：`Own` · `DiagLog` 只在启动入口 · `Face` 只在分派那一层 · `NewFile` 只在分叉那两条命令 ·
//!   `Resolved` 只由路径解析与删会话那一道担保；跨进程锁只在写自家状态的句柄与账号库那两趟读—改—写里拿。
//!   这几个口列在后端 `clippy.toml`（deny），每一处在调用点 `expect` 写理由；
//!   `Face` 只递给文件管理那一族（`needs::Face` 进同一份名单的 `disallowed-types`，那两扇门的挂载行放行），写方法不收 `&Path`（[`tests::the_face_writes_take_only_resolved_paths`]）。
//!
//! 今后加一处读，不用改护栏；加一处写，只要拿到对的句柄。只有新开一种写能力、或者多一个造句柄的地方，才回来改这里。
//!
//! ⚠ **这两行今天仍不是同一件事，而「它们是同一件事」这一格钉不住 —— 靠纪律**：
//! 「用户既有数据」是**语义**命题，人群是**文本形状 ＋ 类型**，两者之间没有可机检的桥。
//! 人群比性质**小**：不含依赖 crate 的写（见下：签字表守）· 不含被起进程的写面（起进程那一处的放行理由写着它归谁）·
//! 不含名单认不出的写（`libc::syscall` 裸调那一形 · 手写 `extern` 声明：`platform/fs/` 外今天一处没有，但没有一条判据钉它）。
//!
//! # 远端那一半另有一层
//!
//! **根本不在本机的写**（SFTP 那一套）不是这条护栏的一个漏项，是它**从来没有过的一维**。
//! ⇒ 远端那一半立在 [`remote_write_layer`]，两行性质 / 人群与盲区读数都在那里。
//! （本机换个命名空间的写，`os::unix::fs::symlink` 一族，由 `clippy.toml` 碰盘那一组一并拦着。）
//!
//! # 依赖 crate 的写面：不经 `platform/fs`，类型挡管不到它 —— 照旧由依赖签字那张表守
//!
//! 依赖 crate（如 `creds-core` 的 `perm.rs`）里的写不经 `platform/fs`，上面那几种句柄挡不住它。这里**不补人群**，
//! 而是让它「有人签过字」：清单上每一条依赖都要有一行登记（有没有写面 · 依据是什么 · 判档），**新加一条而没签字 ⇒ 当场红**。
//! 表与判据住 [`g6_dependency_signoff`]。
//!
//! 🔴 立表顺手量出一件事：那张表里**真有一条有写面** —— `creds-core` 的 `perm.rs` 里两处，
//! 而它们进不进发布二进制取决于**一个 feature**，那张表钉着这件事。
//!
//! 性质那一句的来历与原措辞住 `src/doc/INVARIANTS.md` §41.6。
//!
//! 本护栏遍历后端生产源码，剥掉 `#[cfg(test)]` 块（测试夹具可用 temp 目录）后，断言写句柄只在登记的那几处造
//! （`platform/fs/` 之外不碰盘由 clippy 名单守，见上）。加只读测试是红线 I7 明确允许的（「backend 只准加只读测试/门禁」）。
//!
//! 注：本模块整体在 `#[cfg(test)]` 内，非测试构建为空、零运行期开销、不改后端行为。

#[cfg(test)]
mod tests {
    //
    // 它做的事（「剥掉测试段，只留生产段」）在本仓**早就有唯一住址**：
    // [`guard_core::production_source`]（`src/common/guard-core`），
    // 本 crate 直接用它。`guard_core` 的模块头注逐字记着搬家的理由：
    // 「monitor 侧够不着后端的 `cfg(test)` 模块，于是它的守卫各自写了便宜近似
    // （`src.split("\n#[cfg(test)]").next()`）—— 那个近似……**把扫描面砍掉三分之二**」。
    // 那一族近似都是启发式（括号配平不认字符串/注释里的大括号），并且已经造成过 ≥4 次事故。
    //
    // 🔴 **退役的理由不是「它变成恒等函数了」** —— 当初那条预言
    // **今天还不成立**，现打读数进了：`src/backend` 上它剥掉 1 081 538 字节
    // （64 份里 51 份被它动过），`src/frontend/shell/src` 上 1 429 612 字节。
    // 实测把它改成恒等函数，后端当场红 3 条，`no_test_code_leaks_into_any_production_section`
    // 逐字报「49 份文件、533 个残留测试属性」。⇒ 退役的理由是
    // **一条形状只许有一个住址**，不是「它没用了」。
    //
    // 两者的差别现打过（表）：换成 `production_source` 之后，
    // `src/frontend/shell/src` 上的残留测试属性从 **42 → 4**（它认得原始字符串与可见性修饰，
    // 朴素括号配平认不得），`src/backend` 上两者同为 **0**；
    // 两个 workspace 的用例数与红绿**一格没动**。

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
    /// 碰盘挡在类型上 ⇒ 人群就是整个 crate，不分区。
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

    /// 「改动既有数据」那几形的词表（给依赖签字与远端那一层量别的树用）。
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

    // ═══════════════════ 删历史会话那一条═══════════════════

    /// 🔴🔴 **删历史会话那一条自己的那一道 —— 它恰好住一处。**
    ///
    /// 只有后端的文件管理部分写文件（本机也算）⇒ 删历史会话（本机此前是 monitor
    /// 进程直删、远端此前是 SFTP 直删）改成后端**一条明确的命令** `files-delete-session`，**只收 sid**。
    /// 用户定「文件管理器全部都可以改. 不需要任何围栏」⇒ 文件管理写面也删得掉会话文件；
    /// 本条钉的是删会话**那一条**的形状（只收 sid、自己那一道恰好一处）。
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
                "files/write.rs".to_string(),
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
            crate::stream::inbound::REGISTRY.iter().any(|s| s.name
                == <cmd_wire::files::FilesDeleteSessionReq as cmd_wire::Call>::NAME
                && s.run.face),
            "命令表上没有拿写句柄的 `files-delete-session` —— 删历史会话那条路断了"
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
        match crate::stream::inbound::files_write_face::write_tests::ask(
            "files-delete-session",
            &serde_json::json!({"sid": "abc", "path": "/tmp/x.jsonl"}),
        ) {
            Err(e) => {
                assert_eq!(
                    e.code.as_deref(),
                    Some("bad_args"),
                    "多给一个 `path` 该回 bad_args"
                )
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
            session_shape_calls("files/write.rs", &fake),
            vec![("files/write.rs".to_string(), "make_dir".to_string())],
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
            ("files/write.rs", "fenced_session_file"),
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

    // ═══════════════ 碰盘只经 `platform/fs/` ═══════════════
    //
    // 谁能调碰盘的标准库与系统调用，不在这里扫文本：后端 `clippy.toml` 碰盘那一组（deny）按解析后的路径认，
    // 只在 `platform/fs/` 的 `ro` · `face` · `own` 逐处 `expect` 放行（理由写在那个函数上：读那一份是「只读」）。
    // 谁能造写句柄、谁能拿跨进程锁，也由同一份 `clippy.toml` 那一组认（`Own` · `DiagLog` · `Face` · `NewFile` 的 `issue`、
    // `Resolved::walk` · `vouch`、`platform::lock::hold`），每一处在调用点 `expect` 写理由；
    // 要文件管理写句柄的处理器（`type Needs = needs::Face`）由同一份的 `disallowed-types` 认，只在文件管理那两扇门的挂载行放行。
    // 下面几条只管 clippy 名单管不到的：`Face` 的签名 · `RoFile` 不能写。

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
    #[expect(
        clippy::assertions_on_constants,
        reason = "探针答的就是一个编译期常量：判据要红在这一条上，不是整个测试档编不过"
    )]
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

/// 🔴 **那一处抓屏起进程点「只读」这件事，逐元素钉住。**
///
/// # 为什么调用点的放行买不到它
///
/// 起进程在后端 `clippy.toml` 的名单上、每处在调用点放行写理由 —— 但放行**分不出被调的是哪条
/// tmux 子命令**。同一份文件里把 `capture-pane -p` 改成别的动词、或多塞一个旗，
/// 放行一个字都不变 ⇒ **照常绿**。本模块钉的是这一处的「只读」。
///
/// # 它盖多大（⚠ 别读大）
///
/// **只盖 `control/capture_pane.rs` 这一份文件。** 它不是通用判据
/// ⇒ `control/gate.rs` / `host/tmux.rs` 那两处「只读 tmux」今天仍然只有调用点那一句放行理由看着。
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
    /// （判据住 `tests/backend/`、扫 `src/backend`，靠住址不在自己的语料里），但同族坑本仓记过四次，照纪律拼。
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
            prod.contains(&format!("host::tmux::{}", "child(")),
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
             · `-u` 必须排在子命令**之前**（放后面是 rc=1 + unknown flag，\n\
               而那个响错会被判法读成「抓不到」）；\n\
             · 子命令必须是抓屏那一条 —— 换成任何会改状态的动词，\n\
               调用点那一句放行**不会红**（放行分不出子命令），\n\
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
             这一处的立身之本就是「只读」——调用点那一句放行的理由\n\
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

    /// ★★ **这一处只抓一次就返回。**
    ///
    /// 两个数一起断：起进程**恰好一处**（多一处 = 有人在这里连着抓第二屏），
    /// 循环构件**零处**（那是「靠节拍反复问」的形态，不许在这里做）。
    ///
    /// ⚠ 与 `no_timer_guard` **不重**：那一条扫的是全 crate 的「会让线程自己醒来的构件」
    /// （`sleep` / `interval` / `Duration::from_secs`），认不出一个**忙循环**；
    /// 本条盖的是这一份文件里「循环」这个形状本身。
    #[test]
    fn the_capture_site_is_one_shot() {
        let prod = site();
        let spawns = prod.matches(&format!("host::tmux::{}", "child(")).count();
        assert_eq!(
            spawns, 1,
            "抓屏那一处的生产段里有 {spawns} 处起进程 —— 该恰好 1 处。\n\
             多出来的那一处多半就是「再抓一次」：轮询不在这里，\n\
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
             ⇒ 「抓几次」是**调用方**的事（只出原语，不出轮询）。"
        );
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

/// **依赖 crate 的写面：不是「判据看不见」，是「有人签过字」。**
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
/// - **不检查那行签字说得对不对** —— 同起进程那几处的放行理由。
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
    // Windows 版那一段（`[target.'cfg(windows)'.dependencies]`）今天没有：系统时区名 → IANA 名那一样 `iana-time-zone`
    // 随 `platform_core::tz` 进了仓内 crate（它那一行签字在 `platform-core` 里写）。
    const DEP_SECTIONS: &[&str] = &[DEPS, DEV_DEPS, MUSL_DEPS];
    /// 「我们写下的依赖」的版本与缺省 feature 住的那一段（根清单）：各成员段里只写 `<名>.workspace = true`。
    const WS_DEPS: &str = "[workspace.dependencies]";
    /// 同一棵 workspace 里不归本判据管的成员（目录, 为什么）：它们的清单手机那一侧也单读，版本照旧自己写、与根清单那一段对齐。
    const NOT_COVERED: &[(&str, &str)] = &[
        ("../comms/ssh-dial", "拨号核心：手机的 `mobcore` 也链它，清单自带版本，由 `dial_compress_tests` 钉 russh 那一行与各清单对齐"),
        ("deploy", "装 / 升级远端后端的编排：手机同链，清单自带版本"),
        ("../comms/egress", "出网口：手机同链，清单自带版本"),
        ("../mobile/core-rs/rust", "手机嵌的 Rust 核心（`mobcore`）：Gradle 那一侧单编它，清单自带版本"),
    ];
    /// 只在 dev 侧的成员（包名）：它们的 `[dependencies]` 只进测试二进制，签字记在 [`DEV_DEPS`] 那一档。
    const DEV_ONLY_MEMBERS: &[&str] = &["test-meters"];
    /// 仓内成员只给上层测试档开的 feature：只许出现在 `[dev-dependencies]`。
    const TEST_ONLY_FEATURES: &[&str] = &["test-support", "ts"];

    /// 判档：**闭集**，名字只有这一处住址（表里与签字行都引这几个常量，不写第二遍字面量）。
    const MEASURED_WRITES: &str = "已量·有写面";
    const MEASURED_CLEAN: &str = "已量·未见写面";
    const UNMEASURED: &str = "未量·靠用法签字";
    /// **第四档。**
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
             〔射程：**进得来**的那一形不落本档，落 `已量·有写面·就是要它写`〕",
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
             `the_clean_verdict_is_re_measured_on_the_tree_every_run`。\
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
            "face-core",
            DEPS,
            MEASURED_CLEAN,
            "契约类型的写字一侧（↗ 浮层那一张：照结局与原因从文案表取字），纯字符串变换；与壳共用同一份；仓内 crate，现打 0 处写面",
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
            "拨号核心（后端与手机同一份）：仓内 crate，住 `src/comms/ssh-dial/`，本 crate 清单的 `population` 写着它 ⇒ 它的源码就在本护栏的人群里\
             （`src/backend` 下全部 `.rs` ＋ 人群里的兄弟包），与本 crate 自己的代码同一套判准。本机写面现打 0 处：认下的 host key 交回宿主\
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
        (
            "egress",
            DEPS,
            MEASURED_CLEAN,
            "出网口（这台所有出站 TCP 只在那里开：直连 · HTTP CONNECT · SOCKS5）：仓内 crate、只依赖标准库，住 `src/comms/egress/`，本 crate 清单的 `population` 写着它 ⇒ \
             源码在本护栏的人群里。现打 0 处写面：不开文件、不碰盘、不读环境（描述由宿主照它的环境造好装进来）",
        ),
        (
            "egress",
            DEV_DEPS,
            MEASURED_CLEAN,
            "同上一行那个仓内 crate，测试构建多开 `test-support`（测试档不许出网那句断言住它真开 TCP 的那一处），多出来的 0 处写面",
        ),
        (
            "resident-client",
            DEPS,
            MEASURED_CLEAN,
            "连远端常驻后端的客户端那一侧（桌面 · 手机同一份；本后端只在测试连接 `dial/probe.rs` 里跑一趟它的常驻流）：仓内 crate，\
             连它引的 `comms-inward` 现打 0 处写面（`std::fs` · `tokio::fs` · 起进程一处都没有）；字节只经宿主交来的那条链路进出（`feed::Transport`，本后端那份走拨号核心）",
        ),
        (
            "comms-inward",
            DEPS,
            MEASURED_CLEAN,
            "通信层（仓内 crate）：本后端只取它那一句（`chan::said::offline`，那台离线那一句由连接层写）；\
             现打 0 处写面（`std::fs` · `tokio::fs` · 起进程一处都没有）",
        ),
        (
            "resident-client",
            DEV_DEPS,
            MEASURED_CLEAN,
            "同上面 `[dependencies]` 那一行的仓内 crate：帧面零件那个包（`backend-wire`）的判据拿它对拍客户端那一侧（握手行的读法），多出来的 0 处写面",
        ),
        (
            "face-core",
            DEV_DEPS,
            MEASURED_CLEAN,
            "同上面 `[dependencies]` 那一行的仓内 crate：帧面零件那个包（`backend-wire`）的判据拿它对拍会话簿「那台看不见」那一张（桌面与手机核心同一处），多出来的 0 处写面",
        ),
        (
            "comms-inward",
            DEV_DEPS,
            MEASURED_CLEAN,
            "同上面 `[dependencies]` 那一行的仓内 crate：帧面零件那个包的判据拿它对拍握手行的读法与离线那一句，多出来的 0 处写面",
        ),
        (
            GATED_CRATE,
            DEPS,
            MEASURED_WRITES_ON_PURPOSE,
            "第三方 API key 的唯一住址（装它的类型 / 落盘格式 / 权限判断）。\
             ★ **它自己有一处写面**：`perm.rs` 的 `create_private`（建一个只给本人的新文件），在那个 feature 后面。\
本清单**开了**它：远端那台机器上的 key 只能由那台的后端写\
             （上游选择自己的状态文件，`accounts/upstream_select/file_face.rs` 经 `platform::fs::Own` 写），\
             「出生即只给本人」只有 `create_private` 这一份实现 ⇒ 就是要它写。\
             「写不了」不由编译器保证，由下面这条判据保证：写半边在本 crate 生产段的引用处 == 那一份。\
             边界判据：`the_credentials_write_half_is_reached_only_from_the_account_file_face`",
        ),
        (
            "copy-core",
            DEPS,
            MEASURED_CLEAN,
            "对外文案表的 Rust 取文口（编译期内嵌 `src/shared/copy/table.json` ＋ 具名占位符替换，纯字符串变换）；\
             与 monitor、creds-core 共用同一份；仓内 crate，现打 0 处写面",
        ),
        (
            "batch-core",
            DEPS,
            MEASURED_CLEAN,
            "一组一条命令的全过程与它的字（结局那一句 `said_of` · 标题 · 没成的那几行 · 同名那一行的说明）：\
             与文件窗口、手机嵌的核心共用同一份；纯计算 ＋ 问法由调用方给，仓内 crate，现打 0 处写面",
        ),
        (
            "chunk-core",
            DEPS,
            UNMEASURED,
            "分块传的循环（逐块送进那台的暂存区 ＋ 整份摘要 · 逐块读回）：与文件窗口、手机嵌的核心共用同一份；\
             问那台 · 读本机 · 写本机都由调用方给（`Ask` · `Read` · `Sink`），它自己不碰盘。\
             只由 `backend-control` 引（根清单里没有它那一行 ⇒ 本判据重扫够不着，先按用法签）",
        ),
        (
            "md-core",
            DEPS,
            MEASURED_CLEAN,
            "助手正文的 Markdown 解析成块树（`md_core::parse` 出 `cmd_wire::md` 的 `Doc`）＋ 代码高亮 ＋ 长正文分段：\
             纯函数、不碰盘、不起进程、不读环境；仓内 crate，现打 0 处写面",
        ),
        // `gate-core` 那一行摘了：它收成本 crate 的模块 `control/gate_rules.rs`（`clippy.toml` 碰盘名单管它的写面）。
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
             写不写盘看调用点，而调用点在本 crate 里、由 `clippy.toml` 碰盘名单管",
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
            "vt100",
            DEPS,
            UNMEASURED,
            "终端屏的模型：托管终端（`host/managed/screen.rs` · `attach.rs`）把伪终端吐的字节喂给它、读回格子与整屏字节。\
             纯内存里的解析与格子，backend 在它身上的用法不需要它碰盘、也不起进程",
        ),
        (
            "flate2",
            DEPS,
            UNMEASURED,
            "解压 tar.gz / tgz 与 zip 的 deflate 那一支（`files/extract.rs`）：纯内存解码器（`rust_backend` ＝ `miniz_oxide`）。\
             它本来就在发布二进制里（`russh` 的 `flate2` feature 那一棵），这一行只是把间接依赖提成直接依赖、零新包。\
             写不写盘：本 crate 对它的用法只有 `read::GzDecoder` 包一个读句柄（用法签字，没扫它的源码）",
        ),
        (
            "tar",
            DEPS,
            UNMEASURED,
            "解压 tar / tar.gz（`files/extract.rs`，「用 Rust 库、不调外部命令」）：缺省 feature 关了（不带 `xattr`）。\
             ⚠ 它**有**一步解到盘上的 API（`unpack` 一族，会自己建文件、设权限与时间）—— 本 crate **不调它**：只用 `Archive::entries` 逐条读头与正文，\
             落盘由 `files::extract` 自己逐条过路径解析再写。本档是用法签字（没扫它的源码）",
        ),
        (
            "zip",
            DEPS,
            UNMEASURED,
            "解压 zip（`files/extract.rs`）：缺省 feature 全关（aes / bzip2 / lzma / xz / zstd / zopfli 不进来），只开读 deflate 那一支。\
             ⚠ 它**有**一步解到盘上的 API（`ZipArchive::extract`）—— 本 crate **不调它**：只用 `by_index` 逐条读名字、种类、权限位与正文，\
             落盘由 `files::extract` 自己逐条过路径解析再写。本档是用法签字（没扫它的源码）",
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
            "ring",
            DEV_DEPS,
            UNMEASURED,
            "同上一行那一份，只在测试档：足迹判据照中转的派生法现算直通那一把（`ring::hmac` 纯内存算 HMAC-SHA256）。\
             写不写盘：判据对它的用法一条写路径都不经它（用法签字，没扫它的源码）",
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
            "`--dial` 那条代理臂的 SSH 客户端（传输层握手 · publickey 鉴权 · \
             开 channel）。与 ssh-dial · 手机核心**同版本同 feature 集**（`0.65` / `ring` / \
             `flate2` / `rsa`），provider 就是本表上面 `rustls` 那条已经在用的 `ring`。\
             ⚠ **本档是「未量」不是「没写面」**：私钥由后端经 `Ro` 读成文本、交 `decode_secret_key` \
             解码（russh 不再碰路径），而它建不建缓存 / 写不写 known_hosts \
             **我没有扫过它的源码** —— backend 侧这条路不传 known_hosts 路径、\
             host key 校验由 `dial::DialHandler` 自己在内存里比指纹，\
             但那是**用法**上的签字，不是对它源码的读数。\
             ⇒ 要升到 `已量·未见写面` 得真去扫它那棵树，还没做。\
             链的是 crates.io 上游那一份（仓内不留副本）",
        ),
        (
            "russh",
            DEV_DEPS,
            UNMEASURED,
            "同上面 `[dependencies]` 那一行：拨号那一侧的包（`backend-dial`）的判据拿它造替身服务端\
             （`russh::server::Handler`）、读它的错误类型与鉴权结果，只在测试档，写面同上",
        ),
        (
            "russh-sftp",
            DEPS,
            MEASURED_WRITES_ON_PURPOSE,
            "SFTP 客户端：用户「SFTP 进本机常驻后端，只写暂存区」（＋ 自部署目录）。\
             **本机**写面现打 0 处（3.0.0 源码里 `std::fs` 只出现在 `protocol/open.rs` 一个 `From<OpenFlags> for \
             fs::OpenOptions` 的转换里 —— 造选项、不开文件；它的 `tokio` 没开 `fs` / `net`）；\
             **远端**写面就是它的本职（`SSH_FXP_OPEN` 带写标志 · `REMOVE` · `RENAME` · `MKDIR` …）⇒ 就是要它写。\
             谁划边界：只许 `ssh-dial/sftp.rs` 一份文件持有它的会话、写根 == 暂存区与部署目录、每处先过 `fenced_remote`。\
             边界判据：`the_remote_write_lives_in_exactly_one_file_and_its_root_is_exactly_staging`",
        ),
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
            "test-home",
            DEPS,
            UNMEASURED,
            "同上面 `[dev-dependencies]` 那一行：宿主口那个包（`backend-host`）的 `[dependencies]` 里是 optional，\
             只由它的 `test-support` 开（托管终端的夹具把数据目录建在测试进程的沙箱根底下）；\
             开它的只有上层的 `[dev-dependencies]` ⇒ 照样只在测试期链接，不进发布二进制",
        ),
        (
            "sha2",
            DEV_DEPS,
            UNMEASURED,
            "判据里异源算 sha256（夹具的摘要 · 传输与写盘的整份摘要对拍）。\
             **只在测试期链接**，不进发布二进制；版本是 russh 那棵树早已锁着的 `0.11.0`（不新增包）。纯内存摘要 —— 它不开文件，\
             读文件的是判据自己",
        ),
        (
            "tar",
            DEV_DEPS,
            UNMEASURED,
            "判据手铺 tar 包（解压的命令面 · 文件管理帧面）：只用 `Builder` 往内存里的 `Vec` 写。**只在测试期链接**，\
             与发布那一份同一个包（文件管理那一块解压用它），不新增包",
        ),
        (
            "libc",
            DEV_DEPS,
            UNMEASURED,
            "覆盖写写到一半被收掉那一条认子进程收到的 `SIGXFSZ`（只读一个常量）。**只在测试期链接**，与平台层那一份同一个包，不新增包",
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
            "ts-rs",
            DEPS,
            UNMEASURED,
            "拆出去的那几层（`backend-*`）被上层线上类型嵌着的类型导出 TS 用：`optional`，只在它们自己的 `ts` feature 下编进来，\
             而开 `ts` 的只有上层的 `[dev-dependencies]`（`members_write_third_party_dependencies_only_through_the_workspace_table` 钉）\
             ⇒ 不进发布二进制；写面同上一行（`cargo test` 往 `src/frontend/ui/generated/` 写生成物）",
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
            "pidwatch-core",
            DEPS,
            MEASURED_CLEAN,
            "等一个进程退出（Linux `pidfd_open` ＋ 不带期限的 `poll` · Windows 开进程句柄 ＋ 一直等）；仓内 crate、只依赖 tracing 与 libc。\
             现打 0 处写面：不开文件、不碰盘",
        ),
        (
            "platform-core",
            DEPS,
            MEASURED_WRITES_ON_PURPOSE,
            "平台原语（monitor 同一份）：目录锁 · 只给本人的目录 · 自家状态的原子写 · /proc stat · 进程表 · 本机时区；仓内 crate。\
             本机时区那一样只读（`localtime_r` · 读 `/etc/localtime` 那条链与 `/etc/timezone` · Windows 经系统时间接口与 `iana-time-zone` 问），不写盘。\
             写面就是 `Own` 要的那两样本身（`dir::private_dir*` 建一层只给本人的目录 · `atomic::write_atomic` 旁名 → 落盘 → 换上去），\
             后端只经 `platform/fs/own.rs` 一处叫它们（锁那一样不写）。\
             边界判据：`the_platform_primitive_writes_are_reached_only_from_the_own_state_layer`",
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
            "中转门牌：端口 · 钥匙文件相对路径两个 const ＋ 路由语法（拼 / 拆 / 段闸，纯字符串）；仓内 crate、只依赖 `cap-core`，现打 0 处写面、0 处 I/O",
        ),
        (
            "plain-path-core",
            DEPS,
            UNMEASURED,
            "相对路径拆成普通段的词法判定（交进来一串字节 ⇒ 回剩下的段或哪一形不对），后端只这样用它、不要它碰盘。\
             仓内 crate（`src/common/plain-path-core`），可只经成员 `backend-common` · `backend-files` 的清单链进来、根清单里没有它那一行 ⇒ \
             `已量·未见写面` 那一档的常驻重量够不着它，先落本档",
        ),
        (
            "cap-core",
            DEPS,
            MEASURED_CLEAN,
            "有上限读：读上限 `Cap` · `Over` 与同步整读（读到流尽 · 读一个窗口 · 读一份本机文件，读的都是调用方交进来的句柄）；仓内 crate、零依赖，现打 0 处写面",
        ),
        (
            "tokio",
            DEPS,
            UNMEASURED,
            "运行时 + io。⚠ **本表唯一「写面在我们自己手上」的一条**：`fs` feature 开着 ⇒ \
             它**提供**写 API，而调用点全在本 crate 里 —— 那一面由碰盘名单与那几种写句柄\
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
        (
            "tracing",
            DEV_DEPS,
            UNMEASURED,
            "量具包 `test-meters` 的 `heard`（听本线程上一段代码期间说了什么）用它装一个临时的订阅者。\
             量具包只在各成员的 `[dev-dependencies]`，**只在测试期链接**；与 `[dependencies]` 里那一条同一个包",
        ),
        (
            "tracing-subscriber",
            DEV_DEPS,
            UNMEASURED,
            "同上一行（`heard` 的订阅者把字写进内存里的一块缓冲，不落盘）。**只在测试期链接**；与 `[dependencies]` 里那一条同一个包",
        ),
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
            // 交给 Win32 的路径（长路径前缀）的唯一一份：不覆盖改名那一处用；句子里给人看的路径摘掉前缀也住它。
            "win-path-core",
            DEPS,
            MEASURED_CLEAN,
            "路径编成 UTF-16 并补长路径前缀 · 给人看时摘掉前缀（纯变换；Windows 那一半只取绝对路径）；仓内 crate、零依赖，现打 0 处写面、0 处 I/O",
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
    ///
    /// 条目认两种写法：`<名> = …` 与 `<名>.workspace = true`（版本在根清单的 [`WS_DEPS`] 那一段）。
    /// [`WS_DEPS`] 那一段本身不在这里出现：它只记版本与缺省 feature，链不链、在哪一段链看各段里那一行（[`ws_dep_names`] 读它）。
    fn dep_entries(manifest_text: &str) -> Vec<(String, String, String)> {
        sectioned_entries(manifest_text)
            .into_iter()
            .filter(|(s, ..)| s != WS_DEPS)
            .collect()
    }

    /// 所有以 `dependencies]` 收尾的段（连 [`WS_DEPS`]）里的条目：`(段名, crate 名, 那一行原文)`。
    fn sectioned_entries(manifest_text: &str) -> Vec<(String, String, String)> {
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
            let key = key.trim();
            let name = key.strip_suffix(".workspace").unwrap_or(key);
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

    /// 根清单 [`WS_DEPS`] 那一段里记了版本的名字。
    fn ws_dep_names(manifest_text: &str) -> std::collections::BTreeSet<String> {
        sectioned_entries(manifest_text)
            .into_iter()
            .filter(|(s, ..)| s == WS_DEPS)
            .map(|(_, n, _)| n)
            .collect()
    }

    /// 一条依赖那一行引的是 [`WS_DEPS`] 那一段（`<名>.workspace = true` · `<名> = { workspace = true, … }`）。
    fn refers_to_workspace(line: &str) -> bool {
        let (key, value) = line.split_once('=').unwrap_or((line, ""));
        key.trim().ends_with(".workspace")
            || value
                .split(['{', ',', '}'])
                .any(|kv| kv.split_whitespace().collect::<String>() == "workspace=true")
    }

    /// 一条依赖那一行是仓内 crate（`path = "…"`）。
    fn is_path_dep(line: &str) -> bool {
        line.split_once('=').is_some_and(|(_, v)| {
            v.split(['{', ',', '}'])
                .any(|kv| kv.trim_start().starts_with("path"))
        })
    }

    /// 本判据管的成员：后端那棵 workspace 的 `members` 去掉 [`NOT_COVERED`]（主 crate 与拆出来的 `backend-*` · 量具包）。
    /// `(目录（相对根清单）, 包名, 只在 dev 侧, 清单原文)`；读不到就点名红，不静默少一份。
    fn covered_members() -> Vec<(String, String, bool, String)> {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let members_line = guard_core::strip_hash_comment_lines(MANIFEST)
            .lines()
            .find(|l| l.trim_start().starts_with("members"))
            .map(str::to_string)
            .expect("根清单里没有 `members`");
        members_line
            .split('"')
            .skip(1)
            .step_by(2)
            .filter(|m| !NOT_COVERED.iter().any(|(d, _)| d == m))
            .map(|m| {
                let text = if m == "." {
                    MANIFEST.to_string()
                } else {
                    let p = root.join(m).join("Cargo.toml");
                    std::fs::read_to_string(&p)
                        .unwrap_or_else(|e| panic!("读不到成员清单 {}：{e}", p.display()))
                };
                let name =
                    package_name(&text).unwrap_or_else(|| panic!("成员 `{m}` 的清单里抽不出包名"));
                let dev_only = DEV_ONLY_MEMBERS.contains(&name.as_str());
                (m.to_string(), name, dev_only, text)
            })
            .collect()
    }

    /// `[package]` 那一段的 `name = "…"`。
    fn package_name(manifest_text: &str) -> Option<String> {
        let mut in_package = false;
        for line in guard_core::strip_hash_comment_lines(manifest_text).lines() {
            let entry = line.trim();
            if entry.starts_with('[') {
                in_package = entry == "[package]";
                continue;
            }
            match entry.split_once('=') {
                Some((key, value)) if in_package && key.trim() == "name" => {
                    return Some(value.trim().trim_matches('"').to_string());
                }
                _ => {}
            }
        }
        None
    }

    /// 签字的分母：本判据管的每个成员各段的条目并起来。成员之间的互引（`backend-*` · 量具包）不算依赖，不签字；
    /// 只在 dev 侧的成员（量具包）的 `[dependencies]` 记进 [`DEV_DEPS`] —— 它自己只许被别的成员写在 `[dev-dependencies]` 里
    /// （[`members_write_third_party_dependencies_only_through_the_workspace_table`] 钉）。
    fn declared_all() -> Vec<(String, String, String)> {
        let members = covered_members();
        let names: Vec<&str> = members.iter().map(|(_, n, ..)| n.as_str()).collect();
        let mut out: Vec<(String, String, String)> = members
            .iter()
            .flat_map(|(_, _, dev_only, text)| {
                dep_entries(text).into_iter().map(move |(s, n, l)| {
                    let s = if *dev_only && s == DEPS {
                        DEV_DEPS.to_string()
                    } else {
                        s
                    };
                    (s, n, l)
                })
            })
            .filter(|(_, n, _)| !names.contains(&n.as_str()))
            .collect();
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

    /// 一份成员清单里不经 [`WS_DEPS`]、自己写了版本的第三方依赖（`段 · 那一行`）。**纯函数**，能喂合成清单。
    fn bare_third_party(manifest_text: &str) -> Vec<String> {
        dep_entries(manifest_text)
            .into_iter()
            .filter(|(_, _, l)| !refers_to_workspace(l) && !is_path_dep(l))
            .map(|(s, _, l)| format!("  {s}：{l}"))
            .collect()
    }

    /// 一份成员清单的 `[dependencies]`（含按平台分的那几段）里给仓内成员开了只给测试用的 feature 的那几行。**纯函数**。
    fn test_features_in_release_deps(manifest_text: &str) -> Vec<String> {
        dep_entries(manifest_text)
            .into_iter()
            .filter(|(s, _, l)| {
                s != DEV_DEPS
                    && TEST_ONLY_FEATURES
                        .iter()
                        .any(|f| l.contains(&format!("\"{f}\"")))
            })
            .map(|(s, _, l)| format!("  {s}：{l}"))
            .collect()
    }

    /// ★ 第三方依赖的版本与缺省 feature 只住根清单的 [`WS_DEPS`]：本判据管的成员里，不是仓内 crate 的依赖一律
    /// `.workspace = true`（子清单里偷写一条带版本的 ⇒ 红）；那一段与各成员引到的名字两向相等；
    /// 只在 dev 侧的成员只出现在别人的 `[dev-dependencies]`；只给测试用的 feature 不进 `[dependencies]`。
    #[test]
    fn members_write_third_party_dependencies_only_through_the_workspace_table() {
        let members = covered_members();
        assert!(
            members.len() >= 3 && members.iter().any(|(d, ..)| d == "."),
            "本判据管的成员只认出 {}（至少主 crate · 平台层 · 量具包）—— 读 `members` 坏了，本条在空转",
            members.len()
        );
        let mut bare = Vec::new();
        let mut gated = Vec::new();
        let mut referenced = std::collections::BTreeSet::new();
        let dev_only: Vec<&str> = members
            .iter()
            .filter(|(_, _, d, _)| *d)
            .map(|(_, n, ..)| n.as_str())
            .collect();
        let mut dev_only_misplaced = Vec::new();
        for (dir, _, _, text) in &members {
            bare.extend(
                bare_third_party(text)
                    .into_iter()
                    .map(|l| format!("{dir}{l}")),
            );
            gated.extend(
                test_features_in_release_deps(text)
                    .into_iter()
                    .map(|l| format!("{dir}{l}")),
            );
            for (s, n, l) in dep_entries(text) {
                if refers_to_workspace(&l) {
                    referenced.insert(n.clone());
                }
                if dev_only.contains(&n.as_str()) && s != DEV_DEPS {
                    dev_only_misplaced.push(format!("  {dir} {s}：{l}"));
                }
            }
        }
        assert!(
            bare.is_empty(),
            "这几条第三方依赖在成员清单里自己写了版本（要写成 `<名>.workspace = true`，版本与缺省 feature 只住根清单的 `{WS_DEPS}`）：\n{}",
            bare.join("\n")
        );
        assert!(
            gated.is_empty(),
            "只给测试用的 feature（{}）开在了发布构建也链的段里 —— 测试专用的那几样会进发布二进制：\n{}",
            TEST_ONLY_FEATURES.join(" · "),
            gated.join("\n")
        );
        assert!(
            dev_only_misplaced.is_empty(),
            "只在 dev 侧的成员（{}）出现在了 `[dev-dependencies]` 以外 —— 它那几条依赖记在 dev 那一档签字，前提没了：\n{}",
            dev_only.join(" · "),
            dev_only_misplaced.join("\n")
        );
        let in_table = ws_dep_names(MANIFEST);
        assert!(
            in_table.len() >= 15,
            "根清单 `{WS_DEPS}` 里只抽出 {} 条 —— 抽取坏了，本条在空转",
            in_table.len()
        );
        assert_eq!(
            in_table, referenced,
            "根清单 `{WS_DEPS}` 里记了版本的 ≠ 各成员引到的（多出来的是没人用的版本；少的那条 cargo 会报找不到）"
        );
        // 反空真：合成清单上，偷写版本的 · 开测试 feature 的各认出一条；经表的 · 仓内的不报。
        let fake = format!(
            "[package]\nname = \"backend-某层\"\n\n{DEPS}\nserde.workspace = true\nlibc = \"0.2\"\n\
             tokio = {{ workspace = true, features = [\"sync\"] }}\nbackend-lower = {{ path = \"../lower\", features = [\"{}\"] }}\n\n\
             {DEV_DEPS}\nbackend-lower = {{ path = \"../lower\", features = [\"{}\"] }}\n",
            TEST_ONLY_FEATURES[0], TEST_ONLY_FEATURES[0]
        );
        assert_eq!(
            bare_third_party(&fake),
            vec![format!("  {DEPS}：libc = \"0.2\"")],
            "合成清单里偷写版本的那一条没认准"
        );
        assert_eq!(
            test_features_in_release_deps(&fake).len(),
            1,
            "合成清单里 `[dependencies]` 开测试 feature 的那一条没认出（`[dev-dependencies]` 那条不该报）"
        );
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

    // ══════ `已量·未见写面` 那一档的判决是**常驻**的，不是一次性的 ══════
    //
    // # 它治的那一格
    //
    // `MEASURED_CLEAN` 那一档的判决若只在签字那一刻量，就只在那一秒为真：之后给本档哪棵仓内 crate
    // 加一处货真价实的 `fs::write`，别的门禁一格都不红。⇒ 下面这几条每趟照树重量。
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
    //   〔尺子里还有第三样 —— [`super::remote_write_layer::all_remote_needles`]
    //    那两张远端写的网。**同一句话照样成立**：那两张网漏掉的写法，这里一样漏，
    //    它们各自漏什么逐条写在 `remote_write_layer` 的头注里。〕
    // - 口径是**整棵 `src`、不剥 `#[cfg(test)]`、连注释一起扫**（fail-closed，§41.4 第 1 条纪律）。
    //   ⇒ 那几棵 crate 的**测试代码**里出现写面也会红。那是有意的：本档的签字逐字写的是
    //   「整棵 `src` … 命中 0 处」，而剥掉测试段会让这个数比签字上的那个数**小**。

    /// 那条常驻判据的**名字只有这一处住址**。
    ///
    /// 三处必须对得上，任一处漂了就红（[`the_clean_verdict_unlock_text_names_the_guard_that_now_watches_it`]）：
    /// ① 本常量；② `VERDICTS` 里 `MEASURED_CLEAN` 那一档的解锁条件文字；③ 本文件里真有这么一条 `fn`。
    const RESIDENT_GUARD: &str = "the_clean_verdict_is_re_measured_on_the_tree_every_run";

    /// 这里那把尺子：**护栏自己那两张模式表 ∪ 起进程点 ∪ 远端写那两张网**（现算，不手抄第二份）。
    ///
    /// ⚠ 起进程点那一项**运行时拼** —— 本文件里别再多一处那个字面量：跨文件数它的那些判据不一定跳过本文件。
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
        let declared = declared_all();
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
        let declared = declared_all();
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
    /// 前提是「开了、写面就是账号域那一份要用的」（`已量·有写面·就是要它写`），
    /// 不是「那个 feature 本清单没开 ⇒ 编不进来」（`已量·有写面`）。
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
        // +`platform-core`（本机自家状态的建目录与原子写）：前提是「后端只经 `platform/fs/own.rs` 叫它」，由它签字里点名的边界判据钉。
        assert_eq!(
            purposeful,
            vec!["ssh-dial", "ssh-dial", GATED_CRATE, "russh-sftp", "platform-core"],
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
    /// feature 不开时「backend 写不了那份凭据文件」由**编译器**兜（那个函数不存在）。
    /// 远端那台机器上的 key 只能由那台的后端写 ⇒ feature 开着，编译器那一格不在，由本条接住：
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

    /// 🔴 **`platform-core` 那条「就是要它写」的边界**：它的两样写面（建只给本人的目录 · 原子写）在后端生产段里
    /// 只经 `platform/fs/own.rs`（`Own` 与两个窄句柄的本体）一处叫；多出一处 = 本机写自家状态的口子扩散到 `Own` 之外。
    #[test]
    fn the_platform_primitive_writes_are_reached_only_from_the_own_state_layer() {
        let root = guard_core::addr::backend_src_root();
        let needles = ["platform_core::atomic::", "platform_core::dir::private_dir"];
        let mut found: std::collections::BTreeSet<String> = Default::default();
        let mut scanned = 0usize;
        for path in super::tests::core_files() {
            if path.file_name().and_then(|n| n.to_str()) == Some("readonly_guard.rs") {
                continue;
            }
            scanned += 1;
            let src = std::fs::read_to_string(&path).expect("read rs file");
            let prod = guard_core::production_code(&src);
            if needles.iter().any(|n| prod.contains(n)) {
                found.insert(
                    path.strip_prefix(&root)
                        .unwrap_or(&path)
                        .to_string_lossy()
                        .replace('\\', "/"),
                );
            }
        }
        assert!(scanned >= 60, "只扫到 {scanned} 份后端源文件 —— 遍历坏了");
        let want: std::collections::BTreeSet<String> =
            ["platform/fs/own.rs".to_string()].into_iter().collect();
        assert_eq!(
            found, want,
            "`platform-core` 的写面（建只给本人的目录 · 原子写）在后端生产段里的引用处对不上：应当恰好是 `platform/fs/own.rs`"
        );
        let sample = "platform_core::atomic::write_atomic(path, &tmp, open, fill)";
        assert!(
            needles.iter().any(|n| sample.contains(n)),
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
            "★ 这正是这里的形状：**引擎被编进 backend** 那天，它会作为一条新依赖出现，\
             而它自己就是「在 vendor 里写盘、判据看不见」的那一个 —— 本条要求那时候当场点名它。\
             同时它反向证明另一半：已经签过字的 `serde` / `guard-core` 不许被误报成没签字。"
        );
    }

    /// ★★ 正题：`已量·未见写面` 那一档的判决，**每一趟都重打一遍**。
    ///
    /// 只量一次的话，那一档的判决只在**签字那一秒**为真。
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
            "尺子只现算出 {} 项（两张模式表并集 ＋ 起进程点 ＋ 远端写那两张网，现打 26）—— \
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
        // 🔴 反空真②：**棵数 > 0，并把棵数印出来**。
        assert!(
            !scannable.is_empty(),
            "本判据这一趟真扫了 0 棵 crate —— 零命中什么也不说明"
        );
        let names: Vec<&str> = scannable.iter().map(|(n, ..)| *n).collect();
        println!(
            "常驻重量：本趟重扫 {} 棵仓内 crate（{}），尺子 {} 项",
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

    /// ★★ 非空对照：**同一把尺子**必须在真有写面的那一条上亮。
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

    /// ★★ 那一档的解锁条件**不许说自己没人盯着**，而且要点名盯着它的那一条。
    ///
    /// 本条钉的是一句**诚实边界的时效**：常驻判据在，「那一档没有任何常驻判据」就是假话，而
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
            "`{MEASURED_CLEAN}` 的解锁条件里又出现了 `{retired}` —— 那句话不成立。\n\
             盯着这一档的是 `{RESIDENT_GUARD}`。\n\
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

    // ══════ 第四档的**边界**，以及「能力供应者不许躲进未量档」 ══════
    //
    // # 这两条各接哪一格
    //
    // 裁定二只说了「三档缺一档 ⇒ 加档，不是硬塞」。加一档本身是**一行数据**，
    // 而要防的失效方向是「**加一档「例外」而不写它的边界**」。
    // ⇒ 下面两条就是那条边界的两半：
    //
    // | 判据 | 它挡的那一形 |
    // |---|---|
    // | [`capability_supplier_misfits`] | 一条**能远端写**的依赖被判成「没量过 / 没见写面」—— 那是**绕过新档**，闭集等于白加 |
    // | [`boundary_violations`]         | 一条真落在新档里的依赖，签字里**说不出谁在划它的边界** —— 那才是「例外」 |
    //
    // ⚠ **三档各自怎么红，逐档写清**（死值验口径）：
    // 一条「有写面而且就是要它写」的依赖塞进现有三档中的任何一档，红的不是同一条判据——
    // 塞进 `已量·有写面` ⇒ [`the_credentials_crate_is_signed_as_a_purposeful_writer_and_its_feature_is_really_on`]
    // （那一档的成员集是**相等**断言）；塞进 `已量·未见写面` ⇒
    // [`the_clean_verdict_is_re_measured_on_the_tree_every_run`]（第三方 crate 没有 `path =`，
    // 落进「本判据扫不了」那一格）；塞进 `未量·靠用法签字` ⇒ 由 [`capability_supplier_misfits`] 红，
    // 没有它那一形一条都不红。

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

    /// ★★ **一条能远端写的依赖，不许躲进「没量过 / 没见写面」那两档。**
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
                "★ 这正是要防的形状：**SFTP 那条 crate 被判成 `{hiding}`** 那天，\
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

    /// ★★ 边界那一半：**落第四档的每一条，都得点名一条真存在的判据。**
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
                     而那正是要防的失效方向。实得：{got:?}"
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
    //! **本护栏在远端写那条路上的那一层**（本机那几层看不见它）。
    //!
    //! # 它守的性质 · 它扫的人群（两行，各只许有一句 —— 同本文件顶上 `KG62` 那对）
    //!
    //! - **它守的性质是**：backend **进程自身**不许改动**别人机器上**的用户既有数据 —— 远端写只许住 `ssh-dial/sftp.rs` 一份、只许落在 `~/.cc-monitor/staging/`（我们自己的目录）、每一处先过 `fenced_remote`。
    //!   （本机那一半由 `clippy.toml` 碰盘名单与那几种写句柄守；本层守的是**远端**那一半。）
    //! - **它扫的人群是**：本 crate `src/` 递归全部 `.rs` 的生产段源码文本里，
    //!   **把一条远端通道变成文件系统的那一步**（[`REMOTE_CAPABILITY_ANCHORS`]）
    //!   ＋ **SFTP v3 协议自己那几个改动操作的方法形**（[`REMOTE_MUTATION_VERBS`]）。
    //!
    //! # 🔴 为什么锚在这两处，而不是「`russh-sftp` 今天那几个方法名」
    //!
    //! 要防的失效方向就是后者 ——「**把同一个洞换个命名空间再挖一遍**」。
    //! 两张表各自的理由逐条写在表里，这里只说那条总的：
    //!
    //! **远端写不是一个动词，是一条链**：先取得一个能改远端文件系统的**能力**，再拿它发**操作**。
    //! 按写原语的方法名取样会混进本机写的噪声；按「谁持有 SFTP 会话」取样零噪声。
    //! ⇒ **能力那一端是扼流点，动词那一端不是。** 本层因此把能力网排在第一位，
    //! 动词网只作第二道（能力取得点住在别的文件时，动词那一处仍然点得出名）。
    //!
    //! # ⚠ 它认不出什么 —— **口径与射程同句写清**（盲区读数的落点是 [`BLIND_SPOT_LAYERS`]）
    //!
    //! - **远端 exec**：`channel_open_session` ＋ 把一条会写的 shell 命令交给远端。
    //!   一条命令写不写，**源码文本上判不了**（`2>/dev/null` 就带 `>`）。这与碰盘名单认不出 `Command::new`
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
    ///   碰盘名单已经在管；`sftp.remove_file(` 是第三方的方法，碰盘名单一条都不匹配 —— 那正是要修的。
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
    /// ⚠ `scan_tree!` 那一步「**按构造摘掉调用者自己那一份**」**在这一处不生效**
    /// （判据由 `#[path]` 挂载 ⇒ `file!()` 是折返路径 ⇒ 后缀比不命中），
    /// 而本护栏文件住 `tests/backend/`、本来就不在这里扫的 `src/backend` 那棵树里。
    /// ⇒ 不另补「摘掉自己」，理由与 `super::tests::scan` 跳过本文件逐字同一条：
    /// 本文件整体在 `#[cfg(test)]` 内、生产段是空的，而它的锚点表本身就是一串会自匹配的字面量。
    fn production_tree() -> Vec<(String, String)> {
        let root = guard_core::addr::backend_src_root();
        let mut out: Vec<(String, String)> = Vec::new();
        for (path, src) in guard_core::scan_tree!(&root, &["rs"]) {
            // 模块住址：人群里的兄弟包（拨号核心住 `src/comms/ssh-dial`）认作 `ssh-dial/<文件>`。
            let rel = guard_core::module_address(&root, &path);
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

    // ── **盲区读数** ──────────────────────────────────────────────────────

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
            // 落在两张模式表射程里的每一处，碰盘名单都看得见（看得见 ≠ 会红：
            // 白名单那两份模块里的 `O_EXCL` 新建是**看得见并且被允许**的）。
            seen: Denominator::Counted(n),
            blind: Denominator::Counted(0),
        }
    }

    /// L2 本机代理写（起进程）：分母 = 生产段里经原语起（`Child::new`）与经宿主口起 tmux（`host::tmux::child`）的处数，注释行不算。
    /// 每一处都在后端 `clippy.toml` 的名单上、调用点放行（看得见）；被起的那个程序写什么，放行理由里写着、机器判不了。
    fn layer_local_proxy(corpus: &Corpus) -> LayerReading {
        let needles = [
            format!("Child::{}(", "new"),
            format!("host::tmux::{}(", "child"),
        ];
        let n: usize = corpus
            .iter()
            .flat_map(|(_, c)| c.lines())
            .filter(|l| !l.trim_start().starts_with("//"))
            .map(|l| {
                needles
                    .iter()
                    .map(|k| l.matches(k.as_str()).count())
                    .sum::<usize>()
            })
            .sum();
        LayerReading {
            total: Denominator::Counted(n),
            seen: Denominator::Counted(n),
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
            "放行理由写的是「起什么」，**判不了被起的那个程序写了什么**",
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
            "**那条命令写不写，源码文本上判不了** —— 与碰盘名单认不出 `Command::new` 同一个洞的远端孪生",
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
    /// （按位置判的形状；漏判面：判顺序，判不了「动的就是判过的那一个」——
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
        //   那一臂此前在界面进程里（界面那份收全 exec 的原语，已删），是搬家，不是新长的能力。
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

    /// ★★ **这道闸的盲区，逐层带分母** —— 报读数，不报「已守住」。
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
             「拿命中当全集」**，那正是要防的失效方向。"
        );
        // 🔴 反空真：数得出的四层里，**至少一层今天真有处数**，否则整张读数是在空集上说话。
        assert!(
            total > 0,
            "数得出的四层加起来是 0 处 —— 尺子全瞎了，这张读数此刻什么也不说明"
        );
        // 🔴 **判不了的那几处不许是 0** —— 它是 0 就说明这张表把自己粉饰过了
        //    （盘上今天真有：起进程那几十处 ＋ 远端 exec 1 处）。
        assert!(
            blind > 0,
            "「看得见却判不了它写什么」合计 0 处 —— 盘上明明有起进程与远端 exec 两族，\
             这张读数正在把自己说得比实际干净"
        );
        println!(
            "远端写那一层的盲区读数（量于本趟 `cargo test`；口径 = 本 crate `src/` 递归全部 `.rs` 的\
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
