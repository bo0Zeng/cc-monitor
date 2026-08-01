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
    /// 剥掉所有 `#[cfg(test)]` 属性修饰的花括号块（按括号配平跳过其后第一个 `{...}`）。
    /// 不能简单「从首个 `#[cfg(test)]` 截断到 EOF」——`main.rs` 的测试模块在文件**中部**，
    /// 其后仍有生产代码（`main`/`writer_task`/`write_frame`）。按块剥除才不误伤生产段。
    /// 字节索引均落在 `#`/`{`/`}` 这些 ASCII 边界上，切片对 UTF-8（中文注释）安全。
    ///
    /// **已知局限**（本护栏是纵深防御、非严格证明，不值当为它塞个 Rust 词法器）：括号配平不识别
    /// 字符串/注释里的 `{`/`}`，若某 `#[cfg(test)]` 块内有含不配对花括号的字符串字面量，剥除边界会
    /// 偏。偏向**保守**（少剥）→ 残留测试代码进扫描 → 顶多假阳性（CI 红、人一看是测试代码即排除，
    /// fail-closed 安全）；真正危险的假阴性（多剥、吞掉生产 `fs::write`）需要生产 `fs::write` 紧跟在
    /// 一个花括号不配对的 cfg(test) 块之后——现有 daemon 源无此形态，且真加生产写操作时该模式亦罕见。
    fn strip_cfg_test(src: &str) -> String {
        let mut out = String::new();
        let mut rest = src;
        while let Some(pos) = rest.find("#[cfg(test)]") {
            out.push_str(&rest[..pos]);
            let after = &rest[pos..];
            match after.find('{') {
                Some(brace) => {
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
                }
                None => {
                    // `#[cfg(test)]` 修饰的不是块（如 `use`）——只跳过属性本身，保留其余。
                    rest = &after["#[cfg(test)]".len()..];
                }
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
    const WRITE_WHITELIST_MODULE: &str = "fork_write.rs";

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
            let src = std::fs::read_to_string(&path).expect("read rs file");
            let prod = strip_cfg_test(&src);

            if name == WRITE_WHITELIST_MODULE {
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
