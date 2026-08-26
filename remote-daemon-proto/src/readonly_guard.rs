//! F08a：daemon 只读机器护栏（主计划红线 I7 的机器化守护）。
//!
//! daemon 对被观测文件系统（`~/.claude` 等）**必须只读**——只 watch/scan/read，绝不写。
//! 唯一合法的「写」是把 wire 帧写 **stdout**（`main.rs` 的 `AsyncWriteExt::write_all`，非 FS）。
//! 本护栏遍历 daemon 生产源码，剥掉 `#[cfg(test)]` 块（测试夹具可用 temp 目录）后，断言不含任何
//! **文件系统变更**调用。加只读测试是红线 I7 明确允许的（「daemon 只准加只读测试/门禁」）。
//!
//! 注：本模块整体在 `#[cfg(test)]` 内，非测试构建为空、零运行期开销、不改 daemon 行为。

#[cfg(test)]
mod tests {
    /// U-1（2026-08-01）修掉两条**过剥**（= fail-open，静默删掉扫描面，比假阳性危险得多）。
    /// 两条都由 Phase E 工程审计逮出，并各自实测确认：
    ///
    /// ① **锚点必须钉在行首。** 原来是裸 `find("#[cfg(test)]")`，于是**注释里**逐字写出这个属性
    ///    也会起跳。`main.rs:23` 的行尾注释正是这个形状（「内部整体 #[cfg(test)]，生产构建为空」），
    ///    起跳后括号配平一路吃到 `:40` 的 `use tokio::io::{…}` 收尾 ⇒ **`main.rs:23–40`
    ///    （15 条 `mod` 声明 + 2 条 `use`）从来不在本护栏的扫描面里**。这与 §41.4 第 1 条纪律
    ///    「护栏连注释一起扫，是 fail-closed 的设计」正好相反 —— 对本函数而言，注释里出现这个
    ///    属性是 **fail-open**。
    ///
    /// ② **无花括号体的声明不许吃掉后文。** `#[cfg(test)] mod x;` 底下没有块，
    ///    `after.find('{')` 会一路找到**后面某个不相干 item** 的左大括号并从那里配平。
    ///    `guard_support.rs` 落地时新加的 `#[cfg(test)] mod guard_support;`（`main.rs:26`）
    ///    当场把洞从 429 B 撑到 497 B。判据：属性与第一个左大括号之间若先出现 `;`，那就是声明。
    ///
    /// 修完扫描面 **217_853 → 221_928 字节**（+4_075）——是**扩大**不是收窄（红线 I7 只禁收窄）。
    ///
    /// 剥掉所有 `#[cfg(test)]` 属性修饰的花括号块（按括号配平跳过其后第一个块）。
    /// 不能简单「从首个 `#[cfg(test)]` 截断到 EOF」——`main.rs` 的测试模块在文件**中部**，
    /// 其后仍有生产代码（`main`/`writer_task`/`write_frame`）。按块剥除才不误伤生产段。
    /// 字节索引均落在 `#`/`{`/`}`/`;`/`\n` 这些 ASCII 边界上，切片对 UTF-8（中文注释）安全。
    ///
    /// **已知局限**（本护栏是纵深防御、非严格证明，不值当为它塞个 Rust 词法器）：括号配平不识别
    /// 字符串/注释里的大括号，若某 `#[cfg(test)]` 块内有含不配对大括号的字符串字面量，剥除边界会
    /// 偏。偏向**保守**（少剥）→ 残留测试代码进扫描 → 顶多假阳性（CI 红、人一看是测试代码即排除，
    /// fail-closed 安全）。这条局限**已被 `no_test_code_leaks_into_any_production_section` 钉住**：
    /// 剥完全 crate 不许残留 `#[test]`，撞了就**改注释措辞**（§41.4 第 1 条纪律），别改本函数。
    fn strip_cfg_test(src: &str) -> String {
        const ATTR: &str = "#[cfg(test)]";
        // 只认**行首**的属性；文件开头那一处没有前导换行，单独放行。
        fn anchor(hay: &str, at_file_start: bool) -> Option<usize> {
            if at_file_start && hay.starts_with(ATTR) {
                return Some(0);
            }
            let mut pat = String::with_capacity(ATTR.len() + 1);
            pat.push('\n');
            pat.push_str(ATTR);
            hay.find(&pat).map(|i| i + 1)
        }
        let mut out = String::new();
        let mut rest = src;
        let mut first = true;
        while let Some(pos) = anchor(rest, first) {
            first = false;
            out.push_str(&rest[..pos]);
            let after = &rest[pos..];
            let brace = after.find('{');
            let semi = after.find(';');
            // 先遇到 `;` ⇒ 是**声明**（`#[cfg(test)] mod x;` / `use …;`），没有块可剥。
            let is_block = match (brace, semi) {
                (Some(b), Some(s)) => s > b,
                (Some(_), None) => true,
                (None, _) => false,
            };
            if is_block {
                let brace = brace.expect("is_block 为真时必有左大括号");
                let bytes = after.as_bytes();
                let mut depth: i32 = 0;
                let mut end = brace;
                while end < after.len() {
                    match bytes[end] {
                        b'{' => depth += 1,
                        b'}' => {
                            depth -= 1;
                            if depth == 0 {
                                end += 1;
                                break;
                            }
                        }
                        _ => {}
                    }
                    end += 1;
                }
                rest = &after[end..]; // 跳过整个 cfg(test) 块
            } else {
                // 声明或 `#[cfg(test)]` 修饰的非块 item——只跳过属性本身，保留其余。
                rest = &after[ATTR.len()..];
            }
        }
        out.push_str(rest);
        out
    }

    /// 文件系统**变更**模式。用 `fs::`/`File::`/`OpenOptions` 命名空间锚定，故 stdout 的
    /// `AsyncWriteExt::write_all`（trait 方法、非 `fs::`）天然不匹配 = 合法放行。
    const FS_MUTATION_PATTERNS: &[&str] = &[
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

    /// ★ **唯一**被允许写文件系统的模块（G2，branch-anywhere）。
    ///
    /// 收窄而非放开：见下面 `daemon_write_capability_is_confined_to_one_module` 的头注。
    /// 唯一被允许写文件系统的模块 —— **按仓库相对路径钉，不是按裸文件名**。
    ///
    /// # U3（2026-08-01）从裸文件名改成路径，理由是一次「该红没红」
    ///
    /// U3 把 `fork_write.rs` 从 `src/` 搬进 `src/control/`。功能计划**预言**这会让
    /// `whitelisted == 1` 当场红（逼出 control 侧护栏），**结果它没红** ——
    /// 因为匹配用的是 `path.file_name()`，文件名没变，护栏对整个分层重组**毫无察觉**。
    ///
    /// 「没红」在这里不是好消息，是缺陷的证据：同样的逻辑意味着**将来任何目录下的
    /// `fork_write.rs` 都会被当白名单放行** —— 而白名单层比默认层松（它允许 `O_EXCL` 新建），
    /// 放行错文件 = 给写盘能力开一个没人知道的第二个洞。U2 的 Phase D 审计已经点名过这条。
    ///
    /// 改成路径之后，**再搬一次家就会红**，而那正是应该有人看一眼的时刻。
    const WRITE_WHITELIST_MODULE: &str = "control/fork_write.rs";

    /// 白名单模块**仍然不许**出现的东西 —— 这一层比默认层**更严**。
    ///
    /// 判据是「不许改动**既有**数据」：新建一个此前不存在的文件不违反它，
    /// 但删除 / 改名 / 截断 / 追加 / 覆盖写**都会**。
    const WHITELIST_STILL_FORBIDDEN: &[&str] = &[
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

    /// 白名单模块**必须**出现的东西：`O_EXCL` 新建。
    /// 少了它说明写盘方式被换掉了（比如换成 `File::create`），那就不再是「只新增」。
    ///
    /// **带前导点**是有意的：护栏是子串扫描、**不剥注释**。若只要求裸 token，
    /// 模块文档里那句「`create_new(true)` = O_EXCL」就能把这条要求喂饱 ——
    /// 实测过（N5）：把代码换成 `.create(true)` 之后本条**照样通过**，只有行为测试红。
    /// 带上点就只能由**调用**满足。
    const WHITELIST_REQUIRED: &str = ".create_new(true)";

    /// ★ Phase G 审计补的一条：**`.open(` 出现几次，`.create_new(true)` 就必须出现几次。**
    ///
    /// 原来白名单层禁了 `File::create` / `truncate(true)` / `append(true)` / `create(true)`，
    /// 却**没有**禁 `OpenOptions` / `File::options` / 裸 `.open(`（那三个在默认层是禁的，
    /// 白名单层反而放开了）。而 `WHITELIST_REQUIRED` 是**文件级子串**判定：
    /// 只要文件里某一处有 `.create_new(true)`，另一处写
    /// `OpenOptions::new().write(true).open(既有文件)` —— 不截断、不追加、从 0 偏移覆写
    /// 用户既有 jsonl —— **两层判据全绿**。那正好落在「不许改动既有数据」的反面。
    ///
    /// 配对计数把这个洞堵上：想再开一个写句柄，就必须再配一个 `O_EXCL`。
    fn open_calls_are_all_exclusive(prod: &str) -> Result<(), String> {
        let opens = prod.matches(".open(").count();
        let excl = prod.matches(".create_new(true)").count();
        if opens != excl {
            return Err(format!(
                "白名单模块里 `.open(` 出现 {opens} 次、`.create_new(true)` 出现 {excl} 次 —— \
                 每一个写句柄都必须是 O_EXCL 新建。不配对的那个可能在覆写既有文件。"
            ));
        }
        Ok(())
    }

    /// 默认层判据：这段源码有没有文件系统写操作。抽成纯函数，供反向自检直接喂字符串。
    fn violates_default_layer(prod: &str) -> Option<&'static str> {
        FS_MUTATION_PATTERNS
            .iter()
            .find(|pat| prod.contains(**pat))
            .copied()
    }

    /// 白名单层判据：白名单模块里有没有「改动既有数据」的写法。
    fn violates_whitelist_layer(prod: &str) -> Option<&'static str> {
        WHITELIST_STILL_FORBIDDEN
            .iter()
            .find(|pat| prod.contains(**pat))
            .copied()
    }

    /// 扫 daemon 生产源码，按文件分流到两层判据。返回 (默认层文件数, 命中的白名单模块数)。
    fn scan(src_dir: &std::path::Path) -> (usize, usize) {
        let mut default_scanned = 0usize;
        let mut whitelisted = 0usize;
        // Phase G 审计：**递归**。原来是 `read_dir`（只看顶层）——今天 daemon src 是平的
        // 所以尚未失效，但「写盘能力不可能悄悄扩散到第二个模块」这句承诺对
        // `src/<subdir>/x.rs` 是不成立的：那种文件既不进默认层也不进白名单层，
        // 而 `default_scanned >= 5` 与 `whitelisted == 1` 照样满足 ⇒ 护栏静默失效。
        let mut stack = vec![src_dir.to_path_buf()];
        let mut files = Vec::new();
        while let Some(dir) = stack.pop() {
            for entry in std::fs::read_dir(&dir).expect("read src dir") {
                let path = entry.expect("dir entry").path();
                if path.is_dir() {
                    stack.push(path);
                } else if path.extension().and_then(|e| e.to_str()) == Some("rs") {
                    files.push(path);
                }
            }
        }
        for path in files {
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            // 跳过本护栏文件自身——它的模式字面量数组含这些子串。
            if name == "readonly_guard.rs" {
                continue;
            }
            // U3：白名单按**仓库相对路径**判（见 `WRITE_WHITELIST_MODULE` 头注）。
            let rel = path
                .strip_prefix(src_dir)
                .unwrap_or(&path)
                .to_string_lossy()
                .replace('\\', "/");
            let src = std::fs::read_to_string(&path).expect("read rs file");
            let prod = strip_cfg_test(&src);

            if rel == WRITE_WHITELIST_MODULE {
                whitelisted += 1;
                assert!(
                    prod.contains(WHITELIST_REQUIRED),
                    "白名单模块 {} 里找不到 `{}` —— 写盘方式被换掉了？\n\
                     只准 O_EXCL 新建；`File::create` 之类会截断既有文件。",
                    path.display(),
                    WHITELIST_REQUIRED
                );
                if let Err(why) = open_calls_are_all_exclusive(&prod) {
                    panic!("白名单模块 {}：{why}", path.display());
                }
                if let Some(pat) = violates_whitelist_layer(&prod) {
                    panic!(
                        "白名单模块 {} 含 `{pat}`（红线 I7 白名单层）。\n\
                         这一层比默认层更严：只准新增，**不许改动既有数据**\n\
                         （删除 / 改名 / 截断 / 追加 / 覆盖写都不行）。",
                        path.display()
                    );
                }
                continue;
            }

            if let Some(pat) = violates_default_layer(&prod) {
                panic!(
                    "daemon 写盘护栏违规（红线 I7 默认层）：生产代码 {} 含 `{pat}`。\n\
                     daemon 只有 {WRITE_WHITELIST_MODULE} 一个模块可以写，且只准 O_EXCL 新建；\n\
                     如确需临时文件，放进 #[cfg(test)] 块内。",
                    path.display()
                );
            }
            default_scanned += 1;
        }
        (default_scanned, whitelisted)
    }

    /// **红线 I7 的机器化护栏**（G2 起分两层）。
    ///
    /// # 为什么是「收窄」而不是「放开」
    ///
    /// 这条护栏的真实意图从来不是「daemon 不许碰文件系统」，而是
    /// **「daemon 不许改动用户既有数据」**。此前 daemon 一个字都不用写，
    /// 于是用「全面禁写」来近似它 —— 够用，且实现简单。
    ///
    /// `--fork-session` 要加的能力恰好落在这个近似的**误差**里：
    /// **用 `O_EXCL` 新建一个此前不存在的文件**，不修改、不覆盖、不删除任何既有文件。
    ///
    /// ⇒ 拆成两层，而且**整体比原来更强**：原来对「daemon 将来要写盘」没有任何设计，
    /// 一旦有人要写就只能整条删掉护栏；现在写的能力被钉死在**一个**可审计的洞里，
    /// 洞口还额外挡住了截断 / 追加 / 改名 / 删除。
    #[test]
    fn daemon_write_capability_is_confined_to_one_module() {
        let src_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let (default_scanned, whitelisted) = scan(&src_dir);
        assert!(
            default_scanned >= 5,
            "扫描到的 daemon 源文件过少（{default_scanned}），护栏可能没生效"
        );
        assert_eq!(
            whitelisted, 1,
            "白名单模块必须**恰好一个**（找到 {whitelisted} 个）。\n\
             多一个 = 写盘能力扩散；零个 = {WRITE_WHITELIST_MODULE} 被改名/删除而护栏没跟上。"
        );
    }

    /// U-1（2026-08-01）：**剥法的欠剥方向也要机器钉住。**
    ///
    /// `strip_cfg_test` 的已知局限（括号配平不识别字符串/注释里的大括号）此前只写在散文里。
    /// 散文挡不住事：`guard_support.rs` 落地时，我自己注释里一个孤立的右大括号就把配平提前收尾，
    /// 让那个测试模块的 5 个 `#[test]` 整段留在「生产段」里 —— 而这**不会红**（那些测试只读文件、
    /// 不含写模式），是静默的。下一个往 `guard_support.rs` 加 tempdir 测试的人才会撞上
    /// 一条说「生产代码 guard_support.rs 含 fs::write」的**误导性**诊断。
    ///
    /// 处置遵循 §41.4 第 1 条纪律：**撞了改注释措辞，别改护栏**（本次就是把注释里的
    /// 孤立大括号改成中文名词）。
    #[test]
    fn no_test_code_leaks_into_any_production_section() {
        let src_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
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
                let prod = strip_cfg_test(&std::fs::read_to_string(&path).expect("read rs file"));
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
             ⇒ 改那处措辞（§41.4 第 1 条纪律），**不要**动 `strip_cfg_test`。"
        );
    }

    /// 反向自检：两层判据**真的会抓人**。
    ///
    /// 直接喂字符串给判据函数，而不是去改真文件 —— 后者要么污染工作区，
    /// 要么因为改不进去而**假绿**（本会话已栽过两次「变异没落地却当成没覆盖」）。
    #[test]
    fn both_layers_actually_catch_violations() {
        // 默认层：白名单之外写一句 fs::write 要被抓
        assert_eq!(
            violates_default_layer("fn f() { std::fs::write(p, b).unwrap(); }"),
            Some("fs::write")
        );
        // 白名单层：白名单**之内**写一句 fs::remove_file 也要被抓
        assert_eq!(
            violates_whitelist_layer("fn f() { std::fs::remove_file(p).unwrap(); }"),
            Some("fs::remove_file")
        );
        // 白名单层挡住「能改到既有文件」的开关
        assert_eq!(
            violates_whitelist_layer(".truncate(true)"),
            Some("truncate(true)")
        );
        assert_eq!(
            violates_whitelist_layer(".append(true)"),
            Some("append(true)")
        );
        // 反向的反向：干净代码不许被误判（否则护栏会因假红被人放宽）
        assert_eq!(
            violates_default_layer("let s = std::fs::read_to_string(p)?;"),
            None
        );
        assert_eq!(
            violates_whitelist_layer("OpenOptions::new().write(true).create_new(true).open(p)?"),
            None,
            "O_EXCL 新建是白名单层**唯一允许**的写法，不能被自己挡掉"
        );
        // `.create(true)` 会写花既有文件 —— 必须被挡，且不能误伤 `.create_new(true)`
        assert_eq!(
            violates_whitelist_layer(".create(true)"),
            Some("create(true)")
        );
        // ★ 必需 token 的形状：注释里提一嘴**不算**满足，只有真调用才算（N5 实测暴露的洞）
        assert!(
            !"只准 `create_new(true)` 新建".contains(WHITELIST_REQUIRED),
            "注释里的裸 token 不该满足必需项"
        );
        assert!(
            ".create_new(true)".contains(WHITELIST_REQUIRED),
            "真调用必须满足必需项"
        );
    }
    /// ★★ 〔audit-0805 08-06〕**把只读红线从「列坏 API」翻成「列好 API」。**
    ///
    /// # 原来那条漏了什么（实测，不是设想）
    ///
    /// `FS_MUTATION_PATTERNS` 是**固定 11 项**的黑名单。08-06 在 daemon 生产段里写下
    /// `std::os::unix::fs::symlink(a, b)` 与 `std::fs::set_permissions(b, …)` ——
    /// 两个货真价实的文件系统变更 —— **daemon 281 条全过**。
    /// 原因：表里写的是 `fs::soft_link`（那是**早已废弃的旧名**），而真 API 叫 `symlink`；
    /// `set_permissions` 则**根本没列**。
    /// ⇒ 它守的是「daemon 对 `~/.claude` 只读」这条**用户级红线**，而绕过它只需要用一个
    /// 没被想到的 API 名字 —— 这正是本仓 `structural_scan.rs` 头注说的黑名单通病。
    ///
    /// # 改法：枚举生产段里**每一处** `fs::` / `File::` 调用，要求它在只读白名单里
    ///
    /// 同 `config_surface` 那条已验证的形态（08-06 实测：往那边加一处 `std::fs::write`
    /// 当场红）。白名单的好处是**新增的写 API 自动被挡**，不需要有人先想到它的名字。
    ///
    /// 允许集合来自实测：`File` / `File::open` / `metadata` / `read` / `read_dir` /
    /// `read_to_string`（纯只读）+ 两个仓内 helper（`mtime_ms` / `read_regular_capped`），
    /// 以及**只准出现在 [`WRITE_WHITELIST_MODULE`] 里**的 `OpenOptions`。
    #[test]
    fn every_fs_call_in_daemon_production_is_read_only() {
        /// 只读动词 + 仓内只读 helper。**新增写 API 不在这里 ⇒ 自动红。**
        const READ_ONLY: &[&str] = &[
            "File",
            "File::open",
            "metadata",
            "read",
            "read_dir",
            "read_to_string",
            "mtime_ms",
            "read_regular_capped",
            // P4f：`PermissionsExt` 只用来**读** `mode()`（判可执行位，找 cc-bus 命令用）。
            // ⚠ 与它同族的 `set_permissions` **不在**表里，那条仍然是写、仍然会红。
            "PermissionsExt",
        ];
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut bad: Vec<String> = Vec::new();
        // ★〔audit-0805 08-06〕**先堵逃生口**：把 `std::fs` 的条目导入进作用域，
        // 调用点就不再带 `fs::` 前缀，下面那套按 `fs::` / `File::` 锚定的白名单**整条看不见**。
        //
        // 实测：往 `inbound.rs` 写
        //   `use std::fs::{self as _f, write};`
        //   `fn probe(p: &Path) -> io::Result<()> { write(p, b"x") }`
        // ——一次货真价实的写盘，**六条判据全绿**。而这守的是用户级只读红线。
        //
        // 分类法照搬 `layering_guard`（那里禁的是层别名，同一个道理）：
        // `use std::fs;` **合法**（调用点仍写 `fs::read_to_string`，白名单看得见）；
        // `use std::fs::<条目>` / `use std::fs::{..}` / `use std::fs::*` / `use std::fs as X`
        // **一律禁** —— 它们把动词从调用点上摘掉了。
        //
        // ⚠ 量过误红面：daemon 生产段今天只有 3 处 `use ... fs ...`，
        // 全是 crate 内部的只读助手（`crate::common::fs::read_regular_capped` /
        // `crate::observe::fs::mtime_ms`），没有一处 `std::fs` 或 `tokio::fs` 导入 ⇒ 零误红。
        // 分类逻辑的常驻自检：真代码里今天**没有** `use std::fs;` 这种样本，
        // 于是「模块导入放行」那一支平时没人行使 —— 不钉住的话它可以被改成「一律放行」
        // 而不会有任何信号（本区第五次踩「负向断言没有输入就等于没有」）。
        fn is_hatch(line: &str) -> bool {
            let l = line.trim();
            if !l.starts_with("use ") {
                return false;
            }
            ["std::fs", "tokio::fs"].iter().any(|base| {
                l.find(base)
                    .is_some_and(|k| !l[k + base.len()..].trim_start().starts_with(';'))
            })
        }
        assert!(
            is_hatch("use std::fs::{self as _f, write};"),
            "条目导入没被判成逃生口"
        );
        assert!(is_hatch("use std::fs::write;"), "单条目导入没被判成逃生口");
        assert!(is_hatch("use std::fs as f;"), "别名没被判成逃生口");
        assert!(is_hatch("use tokio::fs::write;"), "tokio 那侧同样要认");
        assert!(
            !is_hatch("use std::fs;"),
            "模块导入被误判 —— 调用点仍带 `fs::`，白名单看得见"
        );
        assert!(
            !is_hatch("use crate::common::fs::read_regular_capped;"),
            "crate 内部的只读助手被误判"
        );

        let mut hatches: Vec<String> = Vec::new();
        for (path, src) in guard_core::scan_tree!(&root, &["rs"]) {
            let rel = path
                .strip_prefix(&root)
                .unwrap_or(&path)
                .to_string_lossy()
                .replace('\\', "/");
            let prod = guard_core::production_code(&src);
            for line in prod.lines() {
                let l = line.trim();
                if !l.starts_with("use ") {
                    continue;
                }
                if is_hatch(l) {
                    hatches.push(format!("  {rel}: {l}"));
                }
            }
        }
        assert!(
            hatches.is_empty(),
            "daemon 生产段把 `std::fs` / `tokio::fs` 的条目导入了作用域：\n{}\n\
             ⚠ 这会让调用点不再带 `fs::` 前缀，于是下面那套只读白名单**整条看不见** ——\n\
             实测一次 `use std::fs::{{self as _f, write}};` + 裸 `write(..)` 就绕过了六条判据。\n\
             写法要求：`use std::fs;` 可以（调用点写 `fs::read_to_string`），\n\
             导入条目 / 起别名 / glob 一律不行。真要写盘只有一条路，见下面那条诊断。",
            hatches.join("\n")
        );

        let mut seen = 0usize;
        for (path, src) in guard_core::scan_tree!(&root, &["rs"]) {
            let rel = path
                .strip_prefix(&root)
                .unwrap_or(&path)
                .to_string_lossy()
                .replace('\\', "/");
            let prod = guard_core::production_code(&src);
            for line in prod.lines() {
                let l = line.trim();
                if l.starts_with("//") {
                    continue;
                }
                for (pat, kind) in [("fs::", "fs"), ("File::", "File")] {
                    let mut from = 0usize;
                    while let Some(k) = l[from..].find(pat) {
                        let at = from + k + pat.len();
                        let verb: String = l[at..]
                            .chars()
                            .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
                            .collect();
                        from = at.max(from + 1);
                        if verb.is_empty() {
                            continue;
                        }
                        let full = if kind == "File" {
                            format!("File::{verb}")
                        } else {
                            verb.clone()
                        };
                        seen += 1;
                        if READ_ONLY.contains(&full.as_str()) {
                            continue;
                        }
                        // 唯一的写口，且只准住在那一个模块里。
                        if full == "OpenOptions" && rel == WRITE_WHITELIST_MODULE {
                            continue;
                        }
                        bad.push(format!("  {rel}: {pat}{verb}"));
                    }
                }
            }
        }
        // ★ 枚举自检：扫不到足够多的 fs 调用 ⇒ 剥法或遍历坏了，下面是空转的。
        assert!(
            seen >= 25,
            "daemon 生产段只扫到 {seen} 处 `fs::`/`File::` 调用 —— 枚举坏了（08-06 实测 39 处）"
        );
        bad.sort();
        bad.dedup();
        assert!(
            bad.is_empty(),
            "daemon 生产段出现了**不在只读白名单里**的文件系统调用：\n{}\n\n\
             ⚠ 红线（主计划 I7）：daemon 对被观测文件系统**必须只读**。\n\
             ★ 本条是白名单 —— 它挡的不只是已知的写 API，也挡**没人想到过**的那些：\n\
             08-06 实测，上面那条黑名单放过了 `os::unix::fs::symlink` 与 `fs::set_permissions`\n\
             （表里写的是早已废弃的 `soft_link`，而 `set_permissions` 根本没列）。\n\
             要新增只读调用就把动词加进 `READ_ONLY`；要写盘只有一条路：\n\
             进 `{}`（那条路自己另有护栏）。",
            bad.join("\n"),
            WRITE_WHITELIST_MODULE
        );
    }
}

/// U8a-2 / **D1 裁决的代码强制**：daemon 起进程的**受管例外清单**。
///
/// # 为什么要这条
///
/// `readonly_guard` 的既有判据只认**文件系统写模式**，**它不认 `Command` / `spawn`**
/// （`§0.2` 早就登记了这件事：「『daemon 只读』这个词今天已经在骗人」）。
/// 于是「起一个会写用户数据的进程」这条路，**机器护栏永远不会红**。
///
/// D1 的裁决（主计划 §5）选了①：**铁律收窄为「daemon 进程自身不许写用户既有数据」**，
/// 间接写不算 —— 但推荐里带一个**强制条件**：
///
/// > 必须同时：在 §41.6 写下「间接写的责任在被起的那个程序，daemon 的责任是不越权
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
    /// 生产段允许起的进程，**逐条登记**：(文件, 起什么, 做什么、为什么不算违反收窄后的铁律)。
    const ALLOWED: &[(&str, &str, &str)] = &[
        (
            "control/tmux_hook.rs",
            "tmux",
            "装 tmux hook（`set-hook -g`）。改的是 **tmux server 的运行期状态**，\
             不是用户既有数据；P4b 的零轮询判活靠它",
        ),
        (
            "control/launch.rs",
            "tmux",
            "U8a-2b 平面 ②：建 tmux 会话 / 往已有会话 send-keys（argv 直传，不过 shell）。\
             改的是 **tmux server 的运行期状态** + 起一个用户自己要起的 claude 进程，\
             **不是 daemon 进程自身写用户既有数据** —— 载荷落盘由那个 claude 进程负责，\
             与用户在终端里手敲同一条命令没有区别（D1 裁决的正例）",
        ),
        (
            "control/kill.rs",
            "tmux",
            "F04a：`kill-session`（argv 直传）。**破坏性**，但改的是 **tmux server 的运行期状态**，\
             不是 daemon 自己写用户既有数据；且必须先过 §34 三道门（Gate 3 = 单窗口）",
        ),
        (
            "control/gate.rs",
            "tmux",
            "F03：§34 Gate 2 的探测（`display-message -p` 取 `@ccm_sid` + `#{session_id}`）\
             ＋ P4f 续刀的 `list-sessions -F`（一次列全部会话的身份三元组，供 `bus-list` \
             判「这个总线成员的地址今天还活着吗」——**总线成员 ⊆ 活着的会话**，\
             谁活着由身份空间说了算，不由 cc-bus 那份会过期的 agents.tsv 说了算）。\
             **只读 tmux**，不改任何状态；登记在 control/ 是因为它是「能不能改这个会话」\
             这个决策的一部分（定框 C13）",
        ),
        (
            "control/identity_tag.rs",
            "tmux",
            "`U-NP④`：`set-option @ccm_sid`（argv 直传）—— 把「这个 tmux 会话在跑哪个 sid」\
             这条事实打上去。接的是 `shared/ccm` 那条**每会话一条、每秒一轮**的身份 poller 的班\
             （用户 08-14：「不要轮询」「ccm 做到必须走 daemon」）。改的是 **tmux server 的\
             运行期状态**，不是 daemon 自己写用户既有数据（同 `tmux_hook`）。\
             ⚠ **探测不在这里**：它复用 `control/gate.rs` 那一处 `display-message`，\
             所以本文件只有这一处起进程 —— 刻意不让面变大",
        ),
        (
            "plugin/invoke.rs",
            "<非字面量>",
            "`K-W1A`（08-26）：**插件通用调用口**里唯一一处起进程 —— argv 直传不过 shell，\
             期限靠 `timeout` 当前缀交给子进程（零定时器铁律的另一侧）。\
             程序名是**查出来的路径**（PATH 里未必有用户级 bin 目录）⇒ 非字面量。\
             ⚠⚠ **本条从 `control/cc_bus.rs` 搬来，同轮把它的理由订正了**：\
             旧理由逐字写着「转调 `cc-list` / `cc-send`，**两条命令**共用」，而今天真实转调的是\
             **三条** —— 08-13 当天稍晚进来的 `cc-kill` **从来没被写进这条豁免理由**\
             （那个 commit 动了 8 个文件，本文件不在其中），而三条判据全绿：\
             键里的程序名是 `<非字面量>`，三条命令**共用同一个键** ⇒ 加第三条不会红。\
             ⇒ 这条登记覆盖的写面**今天是这样**：`cc-list` 只读；`cc-send` 写收件人的收件箱；\
             ★ `cc-kill` **是破坏性的**（杀会话 + 清名册 + 清台账 + 清那个 id 的状态）。\
             三者都是**被起的那个进程**在写，与用户自己在终端里敲同一条命令没有区别\
             （同 `launch` 起 claude 的 D1 正例：收窄后的铁律管的是 **daemon 进程自身**\
             不写用户既有数据）。\
             ⚠ 这一处口从此是**通用**的：将来经它起的每一个插件，写面都落在这一条理由底下，\
             而这条键**分不出**是哪个插件 —— 加一种新的被调命令时必须回来重读这一段，\
             没有任何机检会替你想起（`K6b` 那一族，本条就是它的活体标本）",
        ),
        (
            "observe/watcher.rs",
            "sh",
            "跑 `command -v tmux && tmux ls`（两处：探测 + 取观测）。**只读**，\
             `sh -c` 是为了让 `command -v` 解析 PATH",
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
        let src_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
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
            let prod = crate::guard_support::production_code(raw);
            let mut from = 0usize;
            while let Some(rel) = prod[from..].find("Command::new(") {
                let at = from + rel + "Command::new(".len();
                let tail = &prod[at..];
                // ★★〔P4f 08-13〕**非字面量也要记**。
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
        // ★ **相等，不是地板**〔audit-0805 F18 下半〕。
        //
        // 原来这里是 `found.len() >= 4`。V6 逐行核出：**地板式判据在「数字变大」这个方向上
        // 不会红**，而这里恰恰是变大 —— 真值早已是 6，而它旁边那两段散文
        // （`INVARIANTS.md` 与本条报错文案）一直停在 4，两年没人发现。
        // 相等之后，加一处而不改这个数就会红；那正是「让人非看见不可」的地方。
        //
        // ⚠ 这个数**刻意不再枚举是哪几处** —— 那份清单的家是 `ALLOWED`，
        // 在报错文案里再抄一遍就是下一处会腐的散文（定框 E12）。
        // `U-NP④`（08-14）：8 → 9，新增 `control/identity_tag.rs` 的 `set-option @ccm_sid`。
        // 探测那半复用 `control/gate.rs` 已有的 `display-message` ⇒ 只 +1 不是 +2。
        const SPAWN_SITES_TODAY: usize = 9;
        assert_eq!(
            found.len(),
            SPAWN_SITES_TODAY,
            "生产段扫到 {} 处起进程，登记时是 {SPAWN_SITES_TODAY} 处。\n\
             变少 ⇒ 多半是**抽取坏了**，本断言在空转；变多 ⇒ 新增了起进程的面。\n\
             两种都要人来看：把它加进 `ALLOWED` 并写明「做什么、为什么不违反收窄后的铁律」，\n\
             然后把这个数一起改。**不许改回地板** —— 地板在变大方向上是瞎的。\n\
             实际扫到：{found:?}",
            found.len()
        );
        let unregistered: Vec<&(String, String)> = found
            .iter()
            .filter(|(f, p)| !ALLOWED.iter().any(|(af, ap, _)| af == f && ap == p))
            .collect();
        assert!(
            unregistered.is_empty(),
            "这些起进程点没在受管例外清单里：{unregistered:?}\n\
             D1 把铁律收窄成「daemon **进程自身**不许写用户既有数据」，代价是**必须逐条列举**\n\
             起进程的写面 —— 否则收窄就退化成「隔一层 exec 就绕过」。\n\
             把它加进 `ALLOWED` 并**写明它做什么、为什么不违反收窄后的铁律**。"
        );
    }

    /// ★ 清单里不许有**幽灵条目**（登记了但生产段已经没有了）。
    ///
    /// 否则清单会越攒越松，上面那条的判据跟着变松。
    #[test]
    fn the_registry_has_no_ghost_entries() {
        for (f, p, why) in ALLOWED {
            assert!(
                !why.is_empty(),
                "{f} 起 {p} 没写理由 —— 「逐条列举」列的是写面与理由，不是文件名清单"
            );
        }
        let hook = crate::guard_support::production_code(include_str!("control/tmux_hook.rs"));
        let watcher = crate::guard_support::production_code(include_str!("observe/watcher.rs"));
        assert!(
            hook.contains("Command::new(\"tmux\")"),
            "清单登记了 tmux_hook 起 tmux，但生产段里找不到了 —— 幽灵条目"
        );
        assert!(
            watcher.contains("Command::new(\"sh\")"),
            "清单登记了 watcher 起 sh，但生产段里找不到了 —— 幽灵条目"
        );
    }

    /// ★★ `KY1′-b`〔`K-W1A` 08-26〕：**全表反向核** —— `ALLOWED` 的每一条都得对得上一处真的起进程。
    ///
    /// # 它补的是上面那条的哪一个洞（实测出来的，不是设想）
    ///
    /// [`the_registry_has_no_ghost_entries`] 只做两件事：断言每条 `why` 非空 +
    /// **硬编码**核 `tmux_hook.rs` / `watcher.rs` 那两条。**它不核其余五条。**
    /// ⇒ 本件搬家时如果**加**了 `plugin/invoke.rs` 的登记却**忘了删**
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
        let src_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
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
                    crate::guard_support::production_code(&std::fs::read_to_string(&path).expect("read rs"));
                let mut from = 0usize;
                while let Some(at) = prod[from..].find("Command::new(") {
                    let i = from + at + "Command::new(".len();
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
            .filter(|(af, ap, _)| !found.iter().any(|(f, p)| f == af && p == ap))
            .map(|(af, ap, _)| format!("{af} 起 {ap}"))
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
