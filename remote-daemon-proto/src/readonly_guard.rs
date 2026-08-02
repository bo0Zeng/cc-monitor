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
            "observe/watcher.rs",
            "sh",
            "跑 `command -v tmux && tmux ls`（两处：探测 + 取观测）。**只读**，\
             `sh -c` 是为了让 `command -v` 解析 PATH",
        ),
    ];

    /// ★ 生产段的每一处起进程都必须在 [`ALLOWED`] 里。
    #[test]
    fn every_process_spawn_in_production_is_registered() {
        let files: &[(&str, &str)] = &[
            ("main.rs", include_str!("main.rs")),
            ("wire.rs", include_str!("wire.rs")),
            ("inbound.rs", include_str!("inbound.rs")),
            ("observe/watcher.rs", include_str!("observe/watcher.rs")),
            (
                "observe/history_query.rs",
                include_str!("observe/history_query.rs"),
            ),
            (
                "observe/search_query.rs",
                include_str!("observe/search_query.rs"),
            ),
            (
                "observe/usage_query.rs",
                include_str!("observe/usage_query.rs"),
            ),
            (
                "observe/accounts_query.rs",
                include_str!("observe/accounts_query.rs"),
            ),
            ("observe/codex.rs", include_str!("observe/codex.rs")),
            (
                "observe/turn_detect.rs",
                include_str!("observe/turn_detect.rs"),
            ),
            ("control/tmux_hook.rs", include_str!("control/tmux_hook.rs")),
            (
                "control/fork_write.rs",
                include_str!("control/fork_write.rs"),
            ),
            (
                "control/resolve_query.rs",
                include_str!("control/resolve_query.rs"),
            ),
            ("platform/proc.rs", include_str!("platform/proc.rs")),
            ("platform/signal.rs", include_str!("platform/signal.rs")),
            ("platform/liveness.rs", include_str!("platform/liveness.rs")),
        ];
        let mut found: Vec<(String, String)> = Vec::new();
        for (name, raw) in files {
            let prod = crate::guard_support::production_code(raw);
            let mut from = 0usize;
            while let Some(rel) = prod[from..].find("Command::new(") {
                let at = from + rel + "Command::new(".len();
                let tail = &prod[at..];
                if let Some(q) = tail.find('"') {
                    if let Some(e) = tail[q + 1..].find('"') {
                        found.push((name.to_string(), tail[q + 1..q + 1 + e].to_string()));
                    }
                }
                from = at;
            }
        }
        assert!(
            found.len() >= 3,
            "只扫到 {} 处起进程 —— 抽取坏了，本断言在空转：{found:?}\n\
             （实测生产段今天有 3 处：tmux_hook 的 tmux + watcher 的两处 sh）",
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
}
