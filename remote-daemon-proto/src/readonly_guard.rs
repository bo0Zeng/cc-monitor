//! F08a：daemon 只读机器护栏（主计划红线 I7 的机器化守护）。
//!
//! # `K-G6` `KG62`：性质与人群，两行逐字（**这两行各自只许有一句**，`g6_scope_pins` 钉着）
//!
//! - **它守的性质是**：daemon **进程自身**不许改动用户既有数据；新增文件须 `O_EXCL` 且只许在白名单模块里（`D1` 08-01 收窄后的铁律，与 `doc/INVARIANTS.md` §41.6 的「现措辞」同一句）。
//! - **它扫的人群是**：本 crate `src/` 递归全部 `.rs` 的生产段**源码文本**里 `fs::` / `File::` / `OpenOptions` 命名空间的调用（默认层 + 只读白名单 + 逃生口），外加另一张表：`Command::new` 的起进程点。
//!
//! ⚠ **这两行今天不是同一件事，而「它们是同一件事」这一格钉不住 —— 靠纪律**（`KG62` 如实登记）：
//! 「用户既有数据」是**语义**命题，人群是**文本形状**，两者之间没有可机检的桥。
//! 人群比性质**小**（不含依赖 crate 的写、不含被起进程的写面、不含非 `fs::` 命名空间的写路径），
//! 同时又比性质**大**（daemon 写一个与用户无关的自己的文件也会红）。
//! ⇒ **它今天真能拦住的形状全表在 [`g6_reach`]，一个今天在盘上、形状相同却通过了的反例也在那里。**
//!
//! # 〔`K-R79` 09-12〕上面那三条「小」里的第三条：**远端那一半今天有一层了**
//!
//! 「不含非 `fs::` 命名空间的写路径」这句话此前把两件事装在一起：
//! **本机换个命名空间的写**（`os::unix::fs::symlink` 一族）与**根本不在本机的写**（SFTP 那一套）。
//! 后者不是这条护栏的一个漏项，是它**从来没有过的一维** —— 而 C 类要走的正是那条路。
//! ⇒ 远端那一半立在 [`remote_write_layer`]，两行性质 / 人群与盲区读数都在那里；
//! **本机换命名空间那一半仍然没有守卫，那句话对它照旧成立。**
//!
//! # 〔`K-R2` 09-04〕人群那三条「小」里的第一条：**依赖 crate 的写面，今天有人签字了**
//!
//! 上面那一行逐字承认人群「不含依赖 crate 的写」。本轮**不补人群**（归 PM，按 `MASTERPLAN §2b` 逐条过），
//! 而是把这一条从「判据看不见」变成「有人签过字」：清单上每一条依赖都要有一行登记
//! （有没有写面 · 依据是什么 · 判档），**新加一条而没签字 ⇒ 当场红**。
//! 表与判据住 [`g6_dependency_signoff`]。
//!
//! 🔴 立表顺手量出一件事：那张表里**真有一条有写面** —— `creds-core` 的 `perm.rs` 里两处，
//! 而它们今天进不了发布二进制靠的是**一个 feature 没开**。
//! 在本轮之前，盘上没有任何东西钉着那件事。
//!
//! # 〔`K-G6` 订正〕收窄前那句绝对话今天是假的，本轮**两处一次改完**
//!
//! `D1` 之后 `doc/INVARIANTS.md` §41.6 已经把铁律改成上面那句，并把收窄前那句绝对话
//! 逐字标成**原措辞**（要看原文去那里 —— 本文件刻意**不再抄一遍**：抄一遍就等于把那句假话
//! 留在这里，而这正是本轮要治的病）。
//! 而这个文件里**同时**留着收窄前的绝对句两处（本头注一处 + 只读白名单那条的报错文案一处），
//! 判据兑现的却是收窄后那句 ⇒ 典型的「**只修一半**」，且盘上至少两件逐字引用了这句假话。
//! ⇒ 两处一并改，并由 `platform/fallback_guard.rs::g6_scope_pins` 立一条**反向棘轮**
//! 钉住那两个承重词从此不许回来。
//!
//! 唯一合法的「写」是把 wire 帧写 **stdout**（`main.rs` 的 `AsyncWriteExt::write_all`，非 FS）。
//! 本护栏遍历 daemon 生产源码，剥掉 `#[cfg(test)]` 块（测试夹具可用 temp 目录）后，断言不含任何
//! **文件系统变更**调用。加只读测试是红线 I7 明确允许的（「daemon 只准加只读测试/门禁」）。
//!
//! # ⚠ 本文件底部三个 `g6_*` 模块的住址是**写区限制的结果**，不是设计
//!
//! [`g6_doctrine`]（`§0a` 四情形表）与 [`g6_staged_zero`]（「今天零使用」那一族的指路判据）
//! 都是**跨护栏**的东西，正确落点是新立一份 `guard_doctrine.rs`；
//! `K-G6` `C` 拍的写区只有三份护栏文件 + 件文件，新建文件与改 `main.rs` 都在写区外。
//! ⇒ 暂住这里，**已上报 PM**。搬家那天把这段一起删掉。
//!
//! 注：本模块整体在 `#[cfg(test)]` 内，非测试构建为空、零运行期开销、不改 daemon 行为。

#[cfg(test)]
mod tests {
    /// U-1（2026-08-01）修掉两条**过剥**（= fail-open，静默删掉扫描面，比假阳性危险得多）。
    /// 两条都由 Phase E 工程审计逮出，并各自实测确认：
    ///
    /// ① **锚点必须钉在行首。** 原来是裸 `find("#[cfg(test)]")`，于是**注释里**逐字写出这个属性
    ///    也会起跳。`main.rs::build_id_guard` 那条 `mod` 声明的行尾注释正是这个形状
    ///    （「内部整体 #[cfg(test)]，生产构建为空」），起跳后括号配平一路吃到本文件
    ///    **第一条 `use tokio::io::{…}`** 收尾 ⇒ **夹在这两者之间的那一整段 `mod` 声明
    ///    与 `use` 从来不在本护栏的扫描面里**。这与 §41.4 第 1 条纪律
    ///    「护栏连注释一起扫，是 fail-closed 的设计」正好相反 —— 对本函数而言，注释里出现这个
    ///    属性是 **fail-open**。
    ///
    /// ② **无花括号体的声明不许吃掉后文。** `#[cfg(test)] mod x;` 底下没有块，
    ///    `after.find('{')` 会一路找到**后面某个不相干 item** 的左大括号并从那里配平。
    ///    `guard_support.rs` 落地时新加的 `#[cfg(test)] mod guard_support;`（`main.rs::guard_support`）
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
    pub(super) fn strip_cfg_test(src: &str) -> String {
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

    /// ★ 被允许写文件系统的模块，**逐条登记**（G2，branch-anywhere）。
    ///
    /// `(仓库相对路径, why —— 它写什么、为什么落在「只准新增、不许改动既有数据」之内)`
    ///
    /// 收窄而非放开：见下面 [`daemon_write_capability_is_confined_to_the_registered_modules`]
    /// 的头注。**按仓库相对路径钉，不是按裸文件名。**
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
    ///
    /// # `K-W2D`（2026-09-10）从**一个 `&str`** 变成**一张表** —— 接线那一拍真的来了
    ///
    /// 件文件 `KW2D5` 逐字预言过这一刻：sidecar 落地当天这条判据「**必然红，红得对** ——
    /// 那一红就是**有人在这张表上签了字**」。签的字就是下面第二条。
    ///
    /// ⚠ **表不是豁免清单，三条性质一条没松**：
    /// ① 它仍然是**相等断言**（扫到的白名单模块数 == 本表条数），多一个没登记的照样红；
    /// ② 每一条都得写清它写什么（`why` 有长度地板）；
    /// ③ 每一条都得**真的在盘上、真的在写** —— 幽灵条目由
    ///    [`every_registered_write_module_is_really_on_the_tree_and_really_writes`] 逮。
    ///
    /// ⚠ **它仍然不检查那行 `why` 说得对不对** —— 同 [`spawn_registry`] 那条登记过的边界：
    /// 它钉的是「说得出来」，不是「真的想过」。
    const WRITE_WHITELIST_MODULES: &[(&str, &str)] = &[
        (
            "control/fork_write.rs",
            "G2 / `--fork-session`：用 `O_EXCL` 在 projects 目录里新建一份**此前不存在**的 \
             jsonl，不改、不覆盖、不删任何既有文件 —— 这正是 `D1` 收窄后那条铁律的误差项",
        ),
        (
            "platform/landing.rs",
            "`K-W2D` 接线：把按需拉回来的代码全景 sidecar 用 `O_EXCL` 新建到落点目录里，\
             并在**新建那一刻**给可执行位（事后改权限那个动词在只读白名单里根本不存在）。\
             ★ 身份进**文件名**（`fetch::landing_name` 带 `build_id`），\
             **一个目录级标记文件都不写** —— 那个目录已经有两个写入方，\
             第三份标记会互相覆盖（`R3` 记的 08-11 那次无限重装循环）。\
             ⇒ 它落在「只准新增」之内：撞上同名既有文件就失败并出声，不覆盖。\
             ⚠⚠ **`K-R55`（09-11）：这一条从 `sidecars/codepicture/acquire.rs` 改钉到这里。**\
             搬的是**平台原语**（给可执行位那个动词是 unix 专有的名字），\
             `K-R52` 当时在 `acquire.rs::land` 的头注里逐字登记过「搬要同拍改本文件，\
             那不在它写区」——本件就是那一拍。\
             🔴 **仍然按文件认**：`KR55D2` 点名的失效方向就是「为了搬得动而放宽成按目录认」，\
             一放宽，`platform/` 底下**每一份**文件都白拿写盘能力。\
             ⇒ 写盘那一跳今天全 crate 只有这一份，`acquire.rs` 剩下的是纯映射（零 fs 动词）。",
        ),
    ];

    /// 这个路径在白名单上吗。
    fn is_write_whitelisted(rel: &str) -> bool {
        WRITE_WHITELIST_MODULES.iter().any(|(p, _)| *p == rel)
    }

    /// 白名单模块的名字，拼给报错文案用（**现算，不写第二份字面量**）。
    fn write_whitelist_names() -> String {
        WRITE_WHITELIST_MODULES
            .iter()
            .map(|(p, _)| *p)
            .collect::<Vec<_>>()
            .join(" / ")
    }

    /// 白名单模块**仍然不许**出现的东西 —— 这一层比默认层**更严**。
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

    /// 白名单模块**必须**出现的东西：`O_EXCL` 新建。
    /// 少了它说明写盘方式被换掉了（比如换成 `File::create`），那就不再是「只新增」。
    ///
    /// **带前导点**是有意的：护栏是子串扫描、**不剥注释**。若只要求裸 token，
    /// 模块文档里那句「`create_new(true)` = O_EXCL」就能把这条要求喂饱 ——
    /// 实测过（N5）：把代码换成 `.create(true)` 之后本条**照样通过**，只有行为测试红。
    /// 带上点就只能由**调用**满足。
    const WHITELIST_REQUIRED: &str = ".create_new(true)";

    /// `fs::` / `File::` 那套白名单里，**只准出现在登记过的白名单模块里**的那几个动词。
    ///
    /// # 为什么这里是个表，而不是一个 `OpenOptions`〔`K-W2D` 09-10〕
    ///
    /// `OpenOptionsExt` 是「**新建那一刻就把权限给对**」的唯一入口（`.mode(…)`）。
    /// 它**不能**进 `READ_ONLY`（那是只读动词表，它是货真价实的写能力），
    /// 也**不能**放任它出现在别处 —— 于是它与 `OpenOptions` 落在同一档：
    /// **能力本身不否认，但它只许住在签过字的那几个模块里。**
    ///
    /// ⚠ 反过来说清：这一档**没有**放宽任何东西。事后改权限那个动词
    /// （`set_permissions`）仍然**一个模块都不许有** —— 它不在这张表上，
    /// 也不在 `READ_ONLY` 上，任何地方出现都红。**让它写不出来，比事后检测强一档。**
    const WRITE_ONLY_IN_WHITELIST: &[&str] = &["OpenOptions", "OpenOptionsExt"];

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
    pub(super) fn violates_default_layer(prod: &str) -> Option<&'static str> {
        FS_MUTATION_PATTERNS
            .iter()
            .find(|pat| prod.contains(**pat))
            .copied()
    }

    /// 白名单层判据：白名单模块里有没有「改动既有数据」的写法。
    pub(super) fn violates_whitelist_layer(prod: &str) -> Option<&'static str> {
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
            // U3：白名单按**仓库相对路径**判（见 `WRITE_WHITELIST_MODULES` 头注）。
            let rel = path
                .strip_prefix(src_dir)
                .unwrap_or(&path)
                .to_string_lossy()
                .replace('\\', "/");
            let src = std::fs::read_to_string(&path).expect("read rs file");
            let prod = strip_cfg_test(&src);

            if is_write_whitelisted(&rel) {
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
                     daemon 今天只有这几个模块可以写，且只准 O_EXCL 新建：{}；\n\
                     如确需临时文件，放进 #[cfg(test)] 块内。",
                    path.display(),
                    write_whitelist_names()
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
    /// 一旦有人要写就只能整条删掉护栏；现在写的能力被钉死在**逐条登记**的几个可审计的洞里，
    /// 洞口还额外挡住了截断 / 追加 / 改名 / 删除。
    ///
    /// ⚠ **名字里那个「一个」是 `K-W2D` 09-10 改掉的**：接线那一拍来了，
    /// 白名单从一个模块变成一张表（理由整段住 [`WRITE_WHITELIST_MODULES`] 头注）。
    /// 承重的性质**一个字没松** —— 它仍然是相等断言，只是分母改成现算的表长。
    #[test]
    fn daemon_write_capability_is_confined_to_the_registered_modules() {
        let src_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let (default_scanned, whitelisted) = scan(&src_dir);
        assert!(
            default_scanned >= 5,
            "扫描到的 daemon 源文件过少（{default_scanned}），护栏可能没生效"
        );
        assert_eq!(
            whitelisted,
            WRITE_WHITELIST_MODULES.len(),
            "白名单模块必须**恰好**是登记的那几个（登记 {} 个，扫到 {whitelisted} 个）：{}。\n\
             多一个 = 写盘能力扩散到了没签字的地方；\n\
             少一个 = 某一条被改名 / 删除而这张表没跟上。\n\
             ★ **相等断言，不许改回地板** —— 地板在「变多」这个方向上是瞎的。",
            WRITE_WHITELIST_MODULES.len(),
            write_whitelist_names()
        );
    }

    /// ★★ 反向那半：**表里每一条都得对得上一份真的在写的文件**（幽灵检查）。
    ///
    /// # 它补的是上面那条的哪个洞（形状照 [`spawn_registry`] 那对双向对账）
    ///
    /// 上面那条只比**数**。一条登记指到一个**不存在**的路径、而恰好另一处冒出一个
    /// 没签字的白名单模块 —— 两边数相等，**上面那条一声不吭**。
    /// ⇒ 本条逐条问：这个路径今天在盘上吗、它的生产段里**真的有那个唯一的写口**吗。
    ///
    /// ⚠ 它**不检查**那条 `why` 说得对不对（同 `spawn_registry` 那条边界）。
    #[test]
    fn every_registered_write_module_is_really_on_the_tree_and_really_writes() {
        let src_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        assert!(
            !WRITE_WHITELIST_MODULES.is_empty(),
            "白名单表空了 —— 那会让上面那条相等断言变成「0 == 0」，恒绿"
        );
        for (rel, why) in WRITE_WHITELIST_MODULES {
            assert!(
                why.trim().chars().count() >= 20,
                "`{rel}` 没写清它写什么（实得 {} 字）—— 一条没有理由的白名单就是永久豁免的好听说法",
                why.trim().chars().count()
            );
            let path = src_dir.join(rel);
            let src = match std::fs::read_to_string(&path) {
                Ok(s) => s,
                Err(e) => panic!(
                    "白名单登记了 `{rel}`，而它今天不在盘上（{e}）—— \
                     搬走 / 删掉就**同轮把这一条摘掉**，别留幽灵账"
                ),
            };
            let prod = strip_cfg_test(&src);
            assert!(
                prod.contains(WHITELIST_REQUIRED),
                "白名单登记了 `{rel}`，而它的生产段里找不到 `{WHITELIST_REQUIRED}` —— \
                 它今天根本不写盘（那这条登记该摘），或者写盘方式被换掉了（那更要有人看一眼）"
            );
        }
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
    /// 以及**只准出现在 [`WRITE_WHITELIST_MODULES`] 那几个模块里**的 [`WRITE_ONLY_IN_WHITELIST`]。
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
                        // 那几个写口，且只准住在登记过的白名单模块里。
                        if WRITE_ONLY_IN_WHITELIST.contains(&full.as_str())
                            && is_write_whitelisted(&rel)
                        {
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
             ⚠ 红线（主计划 I7，`D1` 收窄后）：daemon **进程自身**不许改动用户既有数据；\n\
             新增文件须 `O_EXCL` 且只许在白名单模块里。\n\
             ★ 本条是白名单 —— 它挡的不只是已知的写 API，也挡**没人想到过**的那些：\n\
             08-06 实测，上面那条黑名单放过了 `os::unix::fs::symlink` 与 `fs::set_permissions`\n\
             （表里写的是早已废弃的 `soft_link`，而 `set_permissions` 根本没列）。\n\
             要新增只读调用就把动词加进 `READ_ONLY`；要写盘只有一条路：\n\
             进已登记的白名单模块 `{}`（那条路自己另有护栏），并**同轮在那张表上签字**。",
            bad.join("\n"),
            write_whitelist_names()
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
    /// 生产段允许起的进程，**逐条登记**。
    ///
    /// # 〔`K-G6` `KG64`〕三元组扩成**五元组**：多出来的两栏是「走的是哪一格」与「解锁条件」
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
    /// 「daemon **进程自身**不许改动用户既有数据」，而「缩掉的那一半从此归谁」的答案就是本表 ——
    /// 归**被起的那个程序**。这不是巧合，是这张表存在的理由。
    pub(super) const ALLOWED: &[(&str, &str, &str, &str, &str)] = &[
        (
            "control/ccm/mod.rs",
            "sh",
            "`K-R48`：一次性 `ccm` 模式把**一条已经渲好的命令串**交给 POSIX shell 并 `exec` 掉自己\
             （attach 那条、容器路的收尾那条、以及带 `CCM_ENV` / cc-bus 配方的那条）。\
             这与用户在自己终端里手敲同一条命令**没有区别** —— 它跑在用户的进程里、\
             做的是用户这一趟本来就要做的事（D1 裁决的正例，同 `control/launch.rs` 那条）。\
             ⚠ **daemon 常驻那条路走不到这里**：入口在 `main()` 最前面按 `argv[0]` / 子命令词分出去，\
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
            "判活不再需要 daemon 自己去装 hook 的那天（换成别的内核事件源，\
             或 hook 由用户侧一次性装好而 daemon 只读）——那时这一条摘掉。",
        ),
        (
            "control/launch.rs",
            "tmux",
            "U8a-2b 平面 ②：建 tmux 会话 / 往已有会话 send-keys（argv 直传，不过 shell）。\
             改的是 **tmux server 的运行期状态** + 起一个用户自己要起的 claude 进程，\
             **不是 daemon 进程自身写用户既有数据** —— 载荷落盘由那个 claude 进程负责，\
             与用户在终端里手敲同一条命令没有区别（D1 裁决的正例）",
            "缩性质",
            "「起会话」这条路整个搬出 daemon（或改成由 monitor 侧起、daemon 只观测）的那天。\
             ⚠ 在那之前**不许**因为「反正已经登记了」而往这一条底下加第二种被起的程序。",
        ),
        (
            "control/kill.rs",
            "tmux",
            "F04a：`kill-session`（argv 直传）。**破坏性**，但改的是 **tmux server 的运行期状态**，\
             不是 daemon 自己写用户既有数据；且必须先过 §34 三道门（Gate 3 = 单窗口）",
            "缩性质",
            "§34 那三道门有任何一道被拆掉、或「杀会话」不再由 daemon 发起的那天，\
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
             ⇒ 那处调用点搬去了那边，本条今天**只盖 `display-message` 一处**。",
            "缩性质",
            "这一条是**只读 tmux**，本来就落在收窄后的性质之内；\
             等哪天有一条判据能机检「这个起进程点只读」，它就该从受管例外里摘出去、不再占一格。",
        ),
        (
            "control/capture_pane.rs",
            "tmux",
            "`K-R86`（09-13）：**全 crate 唯一一处 `capture-pane`** —— \
             `tmux -u capture-pane -p -t '=名:'`（argv 直传不过 shell），\
             把某个会话此刻那一屏的文本打到 stdout。\
             **只读 tmux**：不 attach、不落 tmux buffer（`-p` = 打到 stdout，\
             换成落 buffer 就是改 tmux 状态了）、不写任何文件。\
             它服务的是 monitor 侧账本 `parity_ledger` 的 `tmux.manage` 那格挂了一个月的\
             「画面预览」欠账 —— 那一格逐字写着「仍等 daemon 出原语」。\
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
             这一处的立身之本就是「抓一屏只有一处、而且只抓一屏」。",
        ),
        (
            "control/oneshot_session.rs",
            "tmux",
            "`K-R87`（09-13）：**一次性会话**那条路上全部 tmux 调用的唯一起进程点\
             （`new-session -d -P -F` 建会话并顺带拿回句柄 · `has-session` 判撞名 · \
             `kill-session` 回滚），argv 直传不过 shell。\
             改的是 **tmux server 的运行期状态**，不是 daemon 自己写用户既有数据 \
             —— 同 `control/launch.rs` / `control/kill.rs` 那两条（`D1` 裁决的正例）。\
             ⚠ **它里面有一个破坏性动作**（`kill-session`），而它**没有过 §34 那三道门**：\
             那三道门挡的是「往一个**不是本工具管理的**会话下手」，而本模块从头到尾只碰\
             自己刚铸出来的名字 —— 名字在 daemon 专属前缀 `ccm-oneshot-` 底下，\
             撞名那一档在建会话那一步就**拒掉**（`KR87D2`），结构上走不到下手那一步；\
             真下手时对的是 `#{session_id}` **句柄**，不是名字。\
             ⇒ 这一条**不许**被拿去给别处的 `kill-session` 免门。\
             ⚠ 这张表的键分不出被调的是哪条 tmux 子命令（同 `plugin/invoke.rs` 那条自陈的 \
             `K6b` 盲区）⇒ 光靠这一行买不到「它只碰自己的名字空间」——\
             买那件事的是本模块自己那几条判据（撞名拒绝 · 别人的会话过了 ttl 还在）。",
            "缩性质",
            "「一次性会话」这条路整个搬出 daemon、或它不再需要自己杀会话的那天。\
             ⚠ 在那之前**不许**因为「反正已经登记了」而往这一条底下加第四条 tmux 子命令 —— \
             今天恰好三条（建 · 问在不在 · 杀），加第四条就要回来重判\
             「它还只碰自己的名字空间吗」。",
        ),
        (
            "control/oneshot_session.rs",
            "<非字面量>",
            "`K-R87`（09-13）：**看门狗那条外部进程** —— `setsid sh -c '<常量脚本>' … N … \
             kill-session -t $N`，到点回来把那个一次性会话杀掉。\
             🔴 **它必须是外部进程，这不是实现口味**：零定时器铁律（`no_timer_guard`）的人群\
             逐字是「本 crate `src/` 的源码文本」，且明写**被起进程的行为不在里面** ——\
             把那个「等 N 秒」搬进 daemon 自己的代码当场撞铁律。\
             写面：它只对 tmux server 下一条 `kill-session`，**不写任何文件** \
             ⇒ 同上一条，落在收窄后的性质之内。\
             ⚠ **程序名是非字面量**：生产恒是 `setsid`（住 `control/oneshot_session.rs` 的 \
             `WATCHDOG_LAUNCHER`），可注入只为了让「它起不来时不许回成功」这一档\
             测得出来（`KR87D3`）—— 那**不是配置口**，同 `capture_pane` 那条 socket 的理由。\
             ⚠⚠ 与 `plugin/invoke.rs` / `platform/shell.rs` 那两条 `<非字面量>` 同族病：\
             这条键**分不出**被起的是谁 ⇒ 把 launcher 换成别的程序**不会红**。\
             逮那一格的是本模块 argv 逐元素那条判据（脚本是常量 · 变量全走位置参数）。",
            "缩性质",
            "哪天「一个会话到点自己没」这件事有了不必起外部进程的做法（tmux 自己出一条 \
             session TTL、或判活那套事件源能承载它），这一条就该摘掉。\
             ⚠ 在那之前**不许**因为「反正已经登记了」而往这一条底下加第二种被起的程序 —— \
             今天经它起的**恰好一种**（看门狗），加第二种就要回来重判。",
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
            "control/identity_tag.rs",
            "tmux",
            "`U-NP④`：`set-option @ccm_sid`（argv 直传）—— 把「这个 tmux 会话在跑哪个 sid」\
             这条事实打上去。接的是 `shared/ccm` 那条**每会话一条、每秒一轮**的身份 poller 的班\
             （用户 08-14：「不要轮询」「ccm 做到必须走 daemon」）。改的是 **tmux server 的\
             运行期状态**，不是 daemon 自己写用户既有数据（同 `tmux_hook`）。\
             ⚠ **探测不在这里**：它复用 `control/gate.rs` 那一处 `display-message`，\
             所以本文件只有这一处起进程 —— 刻意不让面变大",
            "缩性质",
            "会话身份不再靠 tmux 变量承载的那天（`K-P5` 若把身份脱离 tmux，这一条随之消失）。\
             ⚠ 那一件要来动本护栏时，先读 `K-G6` 立的这套做法，别顺手加白名单。",
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
             没有任何机检会替你想起（`K6b` 那一族，本条就是它的活体标本）。\
             ★★ **`K-W2D` 09-10：那句话点名的第二个使用者到了，逐条记在这里** —— \
             `sidecars/codepicture/acquire.rs::ask` 经这一处口起**按需拉回来的代码全景 sidecar**。\
             写面：那个 sidecar 是**我们自己出、我们自己校验哈希之后落盘**的二进制，\
             它今天只被问查询语义（`KR24`「命令面只暴露查询语义」没被放宽）\
             ⇒ **就本层调它的这条路而言写面是「只读」**；\
             ⚠ 而「它自己会不会在用户机器上落一个索引库」**是另一件事、归 PM/用户**\
             （`K-R2` PM 审计 §六㈡ 已端上去），本条**不替它回答**。\
             ⚠⚠ 而这正是这条键的病在**第二个使用者**身上复发：\
             键仍是 `<非字面量>`、仍分不出是哪个插件 ⇒ **加这一条不会红**，\
             是人回来读了这一段才写下的。下一个使用者同理。",
            "缩性质",
            "键能分得出「哪个插件、哪条被调命令」的那天（今天是 `<非字面量>`，三条命令共用一个键）。\
             ⚠ 在那之前，本条的覆盖面由 [`super::g6_reach`] 的反例表钉着：\
             `control/cc_bus.rs` 今天经它转调**恰好三条**，加第四条会红。",
        ),
        (
            "platform/shell.rs",
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
        // `K-R48`（09-11）：9 → 11，新增 `control/ccm/mod.rs` 两处
        //（`sh -c <渲好的命令串>` 与 `exec <用户要起的 agent>`）——
        // 那是 `shared/ccm` 那个 bash 脚本被删掉之后，它那两处 `exec` 的新住址。
        // `K-R55`（09-11）：11 → 10。`observe/watcher.rs` 那**两处** `Command::new("sh")`
        // 搬进了 `platform/shell.rs`，而那里**合成一处**（两跳共用同一条口）⇒ 净 −1。
        // `K-R96`（09-12）：10 → 10，**净零**。`control/gate.rs` 那两处里的
        // `list-sessions -F` 搬去了 `common/session_snapshot.rs`（`R52` 裁定一：
        // Gate 判活改成向那张快照发一次询问）—— 一处走、一处来，`ALLOWED` 多一条键。
        // ⚠ 这种「搬家」最容易在这里留下**两条都在**的痕迹（旧键还盖着已经不存在的调用点）。
        // 复核法：`ALLOWED` 里那条 `control/gate.rs` 的理由栏今天只该说 `display-message`。
        // ⚠ 这个数变小**不一定**是好事（它也可能是抽取坏了），所以顺带写清怎么复核：
        // `grep -c 'Command::new(' `，逐文件看，`platform/shell.rs` 那份是新住址。
        // `K-R86`（09-13）：10 → 11，新增 `control/capture_pane.rs` 的
        // `tmux -u capture-pane -p -t '=名:'`。⚠ **这一处是真的新面，不是搬家**：
        // 抓屏这件事此前 daemon 侧一处都没有（`ALLOWED` 里那条新登记逐字写了它做什么）。
        // ⚠ 它**只读**，而这张表的键分不出被调的子命令 ⇒ 「只读」由
        // [`super::capture_is_read_only`] 单独钉，别把这一格的 +1 读成「只读性质有人证了」。
        // `K-R87`（09-13）：11 → **13**，一次加两处，两处都是**真的新面**：
        // ① `control/oneshot_session.rs` 的 `Command::new("tmux")` —— 那条路上三次 tmux 调用
        //    （建 / 问在不在 / 杀）共用这**一处**，所以只 +1；
        // ② 同一份文件里 `Command::new(<看门狗 launcher>)` —— 生产恒是 `setsid`，
        //    程序名走变量 ⇒ 抽取器记成 `<非字面量>`，`ALLOWED` 里单独一条。
        // 🔴 **PM 单子上写的是「11 → 12」**（`§2` 的「`K-R86` 现打交出来的两条」①）——
        //    那是按「新增一处起进程点」估的，而本件真实新增的是**两处**：看门狗是另一个程序，
        //    抽取器按 `Command::new(` 的**出现次数**数，不是按 `(文件, 程序)` 去重后的键数。
        //    ⇒ 这里记 13，并把差额登记在件文件 `§8`。
        const SPAWN_SITES_TODAY: usize = 13;
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
            .filter(|(f, p)| !ALLOWED.iter().any(|(af, ap, ..)| af == f && ap == p))
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
        for (f, p, why, cell, unlock) in ALLOWED {
            assert!(
                !why.is_empty(),
                "{f} 起 {p} 没写理由 —— 「逐条列举」列的是写面与理由，不是文件名清单"
            );
            // 〔`K-G6` `KG64`〕**枚举比对**（判据规范规则 3：闭集比对，不是子串）。
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
        let hook = crate::guard_support::production_code(include_str!("control/tmux_hook.rs"));
        // 🔴 `K-R55`（09-11）：这一处的住址从 `observe/watcher.rs` 换成了
        //    `platform/shell.rs` —— 起 shell 那一跳搬进适配层（`K33` 裁定二）。
        //    ⚠ 换的是**住址**，不是判据：核的仍是「登记的那一条在生产段里真的找得到」。
        let shell = crate::guard_support::production_code(include_str!("platform/shell.rs"));
        assert!(
            hook.contains("Command::new(\"tmux\")"),
            "清单登记了 tmux_hook 起 tmux，但生产段里找不到了 —— 幽灵条目"
        );
        assert!(
            shell.contains("Command::new(\"sh\")"),
            "清单登记了 platform/shell.rs 起 sh，但生产段里找不到了 —— 幽灵条目"
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
                let prod = crate::guard_support::production_code(
                    &std::fs::read_to_string(&path).expect("read rs"),
                );
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
///   没有它，上面那条「零命中」是空真（`brief` 第 9 条）。
#[cfg(test)]
mod capture_is_read_only {
    use crate::guard_support::production_code;

    /// 被测对象：那一处抓屏的**生产段**。
    fn site() -> String {
        production_code(include_str!("control/capture_pane.rs"))
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
        crate::guard_support::assert_no_test_code("capture_is_read_only", &prod);
        assert!(
            prod.contains(&format!("Command::{}", "new(\"tmux\")")),
            "那一处起进程不见了 —— 本模块声称盖的东西已经不在它声称的地方了"
        );
    }

    /// ★★ 正题①（**值级**）：这一处发出去的 argv 逐元素就是那条只读形。
    #[test]
    fn the_argv_this_site_emits_is_read_only_element_by_element() {
        let argv = crate::control::capture_pane::capture_argv("=某会话:");
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
    }

    /// ★★ 正题②（**文本级**）：生产段里不出现任何会改 tmux 状态的动词。
    #[test]
    fn the_site_names_no_state_changing_tmux_verb() {
        let bad = hits(&site());
        assert_eq!(
            bad,
            Vec::<String>::new(),
            "抓屏那一处的生产段里出现了会改 tmux 状态的动词：{bad:?}\n\
             ⇒ 要么有人绕过 `capture_argv` 另起了一条 argv，要么那个构造器被改了。\n\
             这一处的立身之本就是「只读」——`readonly_guard::ALLOWED` 那一行的理由\n\
             逐字写着它不改任何 tmux 状态；写不成真话就别写。"
        );
    }

    /// ★ 反向自检：**那把尺子真的会咬人**。
    ///
    /// 没有它，上面那条「零命中」与「针全拼错了」长得一模一样（`brief` 第 9 条：
    /// 零违例要先证够得到）。逐条喂，不是喂一条了事 —— 一条针拼错也要报出来。
    #[test]
    fn the_verb_scan_actually_bites_on_every_needle() {
        let verbs = mutating_verbs();
        assert!(
            verbs.len() >= 8,
            "改状态动词表只剩 {} 条 —— 被掏瘪了，正题此刻在空转",
            verbs.len()
        );
        for (needle, why) in &verbs {
            assert!(
                why.chars().count() >= 12,
                "针 {needle:?} 没写清「为什么它算改状态」—— 黑名单里的每一条都要说得出理由"
            );
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
    /// 循环构件**零处**（那是「靠节拍反复问」的形态，归 `K-R87`，不许在本件里做掉）。
    ///
    /// ⚠ 与 `no_timer_guard` **不重**：那一条扫的是全 crate 的「会让线程自己醒来的构件」
    /// （`sleep` / `interval` / `Duration::from_secs`），认不出一个**忙循环**；
    /// 本条盖的是这一份文件里「循环」这个形状本身。
    #[test]
    fn the_capture_site_is_one_shot() {
        let prod = site();
        let spawns = prod.matches(&format!("Command::{}", "new(")).count();
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
             ⇒ 「抓几次」是**调用方**的事（`KR86D3` 逐字：本件只出原语，不出轮询）。"
        );
    }
}

// 〔`K-R76` 09-12〕**这段话原先给的那个理由，今天已经不成立了**。原文逐字留档
// （它下过一次结论、被引过，所以留着）：
//   「这条再导出**不是为了好看**：`g6_doctrine` 的声明行必须逐字是 `mod g6_doctrine {`，
//    不许写成 `pub(crate) mod` —— `guard_core::test_module_ranges` 认「测试模块」的判据是
//    「属性的下一行以 `mod ` 打头、以 `{` 收尾」，写成 `pub(crate) mod` 就**认不出来**，
//    整段测试代码会留在生产段里被别的守卫扫。〔本轮现打：写成 `pub(crate) mod` 时
//    `every_daemon_file_strips_clean` 当场红，逐字「剥完仍残留 2 个测试属性」。〕
//    ⇒ 跨模块可见性只能走这条再导出。」
//
// **那个前提被 `K-R75`（09-12）拆掉了**：剥法不再按字面前缀认 `mod `，改成**按形状**
// 先剥可见性修饰（`guard_core::strip_visibility`）⇒ `pub(crate) mod` 今天认得出来，
// 上面那句「只能走这条再导出」**是假的**。
// ⇒ 今天这条再导出**不是被逼出来的**，它只是一个写法；`mod g6_doctrine {` 那一行
//   **也不再必须逐字长成那样**。
//
// 🔴 **`K-R77`（09-12）把那道再导出拆了**（裁定住 `DECISIONS.md#R36` 裁定一 ——
//   `K-R76` 当时刻意不自批，`§0d` 写死「只改那段过期的话」）。
//   今天盘上：`g6_doctrine` 直接写成 `pub(crate) mod`，跨模块的取名走
//   `crate::readonly_guard::g6_doctrine::{cell_names, is_cell}`（唯一的跨模块消费者是
//   `no_timer_guard::registered_uses_all_have_reasons`），**顶层那一行再导出没有了**。
//   ⚠ **别再写回来**：这一族今天有闸了 —— monitor 侧
//   `structural_scan.rs::the_cfg_test_reexport_detour_stays_extinct` 是一条**恒零棘轮**，
//   语料面含本文件所在的这棵树，写回一行当场红并点名这份文件。

/// 〔`K-G6` `KG64`〕**`§0a` 四情形表的代码形态** —— 一条护栏的人群与性质对不上时该怎么处置。
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
/// 派工单原写「落进护栏头注」，`Bx` 顶回来了：`brief` 纪律 15 逐字「写在**源码注释**里的
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
            "`D1` 把 daemon 那条写盘铁律从收窄前那句绝对话（原文见 `doc/INVARIANTS.md` §41.6 的\
             「原措辞」，本文件刻意不抄）缩成「daemon 进程自身不许改动用户既有数据」就是这一格：\
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
        for (cell, when, how, why, unlock) in CELLS {
            assert!(
                !when.trim().is_empty() && !how.trim().is_empty(),
                "`{cell}` 这一格没写「什么时候落在这一格」或「正确处置」"
            );
            assert!(
                why.trim().chars().count() >= 20,
                "`{cell}` 的 why 太短（{} 字）—— 说不清为什么是这个处置，下一个人只会照抄格名",
                why.trim().chars().count()
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

/// 〔`K-G6` `KG61`〕**本护栏今天真能拦住的形状全表 + 一个今天就通过了的反例。**
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
    use super::tests::{
        strip_cfg_test, violates_default_layer, violates_whitelist_layer, FS_MUTATION_PATTERNS,
        WHITELIST_STILL_FORBIDDEN,
    };

    /// ★ 全表①：**默认层**的每一个形状，逐形喂一个合成样本，逐形要求它红。
    #[test]
    fn every_default_layer_shape_reds_on_its_own_sample() {
        assert_eq!(
            FS_MUTATION_PATTERNS.len(),
            11,
            "默认层的形状表从 11 条变成 {} 条了 —— **全表的分母变了**。\n\
             变多：新形状要在这里补一刀读数；变少：有人从黑名单里拿掉了一个形状，\n\
             那是放宽，必须先摆出「它今天拦得住什么」再谈。",
            FS_MUTATION_PATTERNS.len()
        );
        for pat in FS_MUTATION_PATTERNS {
            let sample = format!("fn f() {{ std::{pat}(p, b).unwrap(); }}");
            assert_eq!(
                violates_default_layer(&sample),
                Some(*pat),
                "默认层对 `{pat}` 这个形状不响了 —— 全表里这一格今天是空的"
            );
        }
        // 反向的反向：干净的只读代码不许被误判（误红最省事的消法是把判据删掉）。
        assert_eq!(
            violates_default_layer("let s = std::fs::read_to_string(p)?;"),
            None,
            "只读调用被默认层误判成写"
        );
    }

    /// ★ 全表②：**白名单层**（比默认层更严的那一层）的每一个形状同样逐形一刀。
    #[test]
    fn every_whitelist_layer_shape_reds_on_its_own_sample() {
        assert_eq!(
            WHITELIST_STILL_FORBIDDEN.len(),
            13,
            "白名单层的形状表从 13 条变成 {} 条了 —— 同上，全表的分母变了",
            WHITELIST_STILL_FORBIDDEN.len()
        );
        for pat in WHITELIST_STILL_FORBIDDEN {
            let sample = format!("let _ = handle.{pat};");
            assert_eq!(
                violates_whitelist_layer(&sample),
                Some(*pat),
                "白名单层对 `{pat}` 这个形状不响了 —— 全表里这一格今天是空的"
            );
        }
        assert_eq!(
            violates_whitelist_layer("OpenOptions::new().write(true).create_new(true).open(p)?"),
            None,
            "O_EXCL 新建是白名单层唯一允许的写法，不能被自己挡掉"
        );
    }

    /// 反例表的语料。**住址与语料在这里对死** —— 加一行反例而不接语料，这里当场 panic。
    fn source_of(rel: &str) -> &'static str {
        match rel {
            "control/cc_bus.rs" => include_str!("control/cc_bus.rs"),
            other => panic!("反例表里出现了没接语料的住址：{other}"),
        }
    }

    /// 〔`KG61` ②〕**今天在盘上、形状与本护栏声称要拦的相同、而它放过了的那一处。**
    ///
    /// `(住址, 生产段里的片段, why——形状为什么对得上、而它为什么看不见, unlock)`
    const KNOWN_PASSING_COUNTEREXAMPLES: &[(&str, &str, &str, &str)] = &[(
        "control/cc_bus.rs",
        "run(\"cc-kill\"",
        "生产段起一个外部程序去**删除 / 覆盖用户既有数据**：被起的那个脚本会覆盖两份名册、\
         删掉一个 id 的收件箱与它的状态文件。**形状对得上** —— daemon 若把同一件事写成\
         `remove_file` / `rename` 那两个动词，默认层当场红。\
         **而它今天通过**：默认层的人群是本 crate 源码文本里 `fs::` / `File::` / `OpenOptions` \
         这三个命名空间的调用，起进程一个都不匹配（`spawn_registry` 头注逐字承认「它不认 \
         `Command` / `spawn`」）。⇒ 这不是漏洞，是**一次没有落到判据上的裁定**（`D1` 缩性质），\
         而缩掉的那一半由 `ALLOWED` 接着 —— 那张表的键**分不出被调命令**，所以本条另钉一格：\
         经那个键转调的命令**恰好三条**。",
        "默认层能顺着起进程点读到被起程序的写面那天（或那条路改成 daemon 自己写、\
         从而落回默认层射程内）——那时这一条摘掉，并回来重判 `ALLOWED` 还需不需要。",
    )];

    /// ★ 反例仍在盘上、仍然通过；同形的**直写**仍然会红。
    ///
    /// ★★ 本件专属陷阱（件文件 `§3`）在这一格的答案：这条路**未经任何放宽就已经通过**
    /// —— 默认层的判据里没有任何一条能匹配到起进程（分母 = `FS_MUTATION_PATTERNS` 11 条 +
    /// 只读白名单那条，两者都只认 `fs::` / `File::` / `OpenOptions` 三个命名空间的文本）。
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
        for (rel, frag, why, unlock) in KNOWN_PASSING_COUNTEREXAMPLES {
            let prod = strip_cfg_test(source_of(rel));
            assert!(
                prod.contains(frag),
                "反例 `{rel}` 里找不到 `{frag}` 了 —— 修掉了就**同轮摘登记**，\
                 并回来重判本护栏的射程（这正是形态二要买的东西）"
            );
            assert_eq!(
                violates_default_layer(&prod),
                None,
                "反例 `{rel}` 今天**红了** —— 那说明本护栏的射程变了，\
                 这张表说的话已经不成立，回来重判"
            );
            assert!(why.trim().chars().count() >= 20, "`{rel}` 的 why 太短");
            assert!(
                unlock.trim().chars().count() >= 20,
                "`{rel}` 没写解锁条件 —— 没有解锁条件的反例登记会一直躺着"
            );
        }
        // ★ 分开「它本来就拦不住」与「放宽之后没红」：**同一件事直写**，默认层当场红。
        assert_eq!(
            violates_default_layer("std::fs::remove_file(bus.join(\"inbox\").join(name))?;"),
            Some("fs::remove_file"),
            "同形的直写都不红了 —— 那就不是「人群够不着」，是判据本身坏了"
        );
    }

    /// ★ 那条 `<非字面量>` 键今天覆盖了**恰好三条**被调命令。
    ///
    /// # 它钉的是 `ALLOWED` 逮不到的那一面（活体标本，不是设想）
    ///
    /// `spawn_registry` 的三条判据用的键是 `(文件, 程序名)`，而 `plugin/invoke.rs` 那条的
    /// 程序名是 `<非字面量>` ⇒ **同一个键下加第二种被调命令不会红**。
    /// 08-13 加进来的那条**破坏性**命令因此漏登 13 天，三条判据全绿。
    /// 本条把「今天是三条」钉成**相等**：加第四条就必须回来重读那条豁免理由。
    #[test]
    fn the_non_literal_spawn_key_still_covers_exactly_three_commands() {
        let prod = strip_cfg_test(source_of("control/cc_bus.rs"));
        let mut cmds: Vec<&str> = Vec::new();
        for opener in ["run(\"", "run_as(\""] {
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
            vec!["cc-kill", "cc-list", "cc-send"],
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
             本条的前提是「三条命令共用**一个**键」，键数变了这格就该重判"
        );
    }
}

/// 〔`K-G6` `KG63`〕**「这个能力今天在生产里零使用，而『零』本身要被钉住、接线那天要故意变红」这一族的指路判据。**
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
            "src-tauri/src/backend/control/launch_wire.rs",
            "the_two_reasons_u8c3_cannot_delete_the_ts_renderer_still_hold",
            "生产段**不发** `create-or-attach` 这个 mode 串（运行时拼串防自指，配抽取器自检）",
            "自己就是那条判据",
            "跨 crate",
        ),
        (
            "wire.rs",
            "production_hello_leaves_homes_empty_so_claude_bytes_stay_frozen",
            "`main.rs` 生产段给 `homes` 赋值**恰好 1 处**且逐字是空构造",
            "自己就是那条判据",
            "本 crate",
        ),
        (
            "wire.rs",
            "the_daemon_can_already_discover_homes_it_just_does_not_send_them",
            "反锚：发现能力**还在**（合成夹具走真发现路 + 精确字节断言）",
            "自己就是那条判据",
            "本 crate",
        ),
        (
            "agents/codex/parse.rs",
            "codex_turn_end_uuid",
            "daemon 生产段里**跨文件消费者 0 个**（今天只有它自己那份文件在提它）",
            "—",
            "本 crate",
        ),
        (
            "agents/codex/parse.rs",
            "is_codex_turn_end",
            "daemon 生产段里**跨文件消费者 0 个**（同上；它在自己文件内被兄弟函数调一次）",
            "—",
            "本 crate",
        ),
        (
            "plugin/probe.rs",
            "negotiate",
            "daemon 生产段里**跨文件消费者 0 个**（`main.rs` 那处是英文文档注释，不是调用）",
            "—",
            "本 crate",
        ),
    ];

    /// `(相对路径, 原文, 生产段)`。**走 `scan_tree!`** —— 它按构造摘除调用者自己那一份，
    /// 否则本表里逐字写着的那些名字会把自己算成「有人指向它」（本仓记过五次的恒绿形状）。
    fn daemon_files() -> Vec<(String, String, String)> {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut out = Vec::new();
        for (path, src) in guard_core::scan_tree!(&root, &["rs"]) {
            let rel = path
                .strip_prefix(&root)
                .unwrap_or(&path)
                .to_string_lossy()
                .replace('\\', "/");
            let prod = guard_core::production_code(&src);
            out.push((rel, src, prod));
        }
        out.sort();
        out
    }

    /// ★ 正题一：表里每条的住址今天真的在，且那个名字真的还在那份文件里（**幽灵检查**）。
    #[test]
    fn the_staged_zero_registry_has_no_ghost_entries() {
        assert_eq!(
            STAGED_ZERO.len(),
            6,
            "这一族的登记表从 6 条变成 {} 条了 —— 加成员是好事，\
             但每加一条都要说清「被钉的那个零逐字是什么」与「接线那天为什么该红」",
            STAGED_ZERO.len()
        );
        let files = daemon_files();
        assert!(
            files.len() >= 60,
            "只扫到 {} 个 daemon 源文件 —— **遍历坏了**，本条此刻在空转",
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
                .unwrap_or_else(|| panic!("登记的住址 `{rel}` 今天不在 daemon 树上了 —— 幽灵条目"));
            assert!(
                owner.1.contains(name),
                "`{rel}` 里已经找不到 `{name}` 了 —— 删掉了就**同轮摘登记**"
            );
        }
        assert!(
            out_of_reach <= 1,
            "有 {out_of_reach} 条标着 `跨 crate` —— 本判据住 daemon crate（它刻意不属于 workspace），\
             够不着 `src-tauri`。标的条数超过 1 就说明这张表的家选错了，\
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
        let files = daemon_files();
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
            checked, 3,
            "只核了 {checked} 条「今天没有判据」的欠账（登记时是 3 条）—— \
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

/// 〔`K-G6` `KG62`〕**钉住另外两道护栏的「性质行 / 人群行」各自只有一句。**
///
/// # 为什么钉在这里，而不是各自的文件里
///
/// `ratchet_guard.rs` 头注逐字给过理由：判据**不许与被扫的文本同住一个文件**
/// （它会在自己的注释里找到自己 ⇒ 恒绿）。
/// ⇒ 本模块钉 `no_timer_guard.rs` 与 `platform/fallback_guard.rs`；
/// 本文件自己那两行由 `platform/fallback_guard.rs::g6_scope_pins` 钉 —— **三道两两互钉**。
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
        vec![
            (
                "no_timer_guard.rs",
                include_str!("no_timer_guard.rs"),
                vec![format!("不许再有任何{}", "周期性唤醒")],
                "",
            ),
            (
                "platform/fallback_guard.rs",
                include_str!("platform/fallback_guard.rs"),
                Vec::new(),
                "这一道的性质行今天**只有一句**、也没有收窄史 —— 它自曝的是射程\
                 （只看 `platform/`、等价改写绕得过），那是诚实登记，不是过期的绝对话。\
                 ⇒ 没有承重词要钉，如实写在这里而不是留一个空表。",
            ),
        ]
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
                     **多了**：同一道护栏有两句性质声明 —— 这正是本轮逮到的那个病\n\
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

    /// ★ 反向棘轮：收窄前那句绝对话的**承重词**，今天起在那份文件里零命中。
    #[test]
    fn the_pre_narrowing_absolutes_do_not_come_back() {
        let mut with_words = 0usize;
        for (rel, src, forbidden, why_empty) in pinned() {
            if forbidden.is_empty() {
                assert!(
                    why_empty.trim().chars().count() >= 20,
                    "`{rel}` 的禁词表是空的，却没写清为什么空 —— 空表要么是事实，\
                     要么是没人填，两者在断言上一模一样"
                );
                continue;
            }
            with_words += 1;
            for word in forbidden {
                assert!(
                    !src.contains(word.as_str()),
                    "`{rel}` 里又出现了 `{word}` —— 那是**收窄前**的说法，判据从来没兑现过它。\n\
                     ⇒ 要么把判据的人群补齐到那句话上，要么就别写那句话。\n\
                     （`§0a` 四情形表：这一格叫 `补齐人群`，不叫「先把话说满」。）"
                );
            }
        }
        assert!(
            with_words >= 1,
            "禁词表全空 —— 本条此刻是空转的（每一条都走了 `why_empty` 那一支）"
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
/// 就有两处写面（`make_private` 收窄权限 · `create_private` 建私有文件）。
/// 它们今天进不了本 crate 的依赖树，靠的是**那个 feature 没开** ——
/// 而在本模块之前，盘上没有任何东西钉着「那个 feature 不许开」。
///
/// # 本模块**不补人群**（归 PM，按 `MASTERPLAN §2b` 逐条过），它买的是另一格
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
    const MANIFEST: &str = include_str!("../Cargo.toml");

    /// 清单里**带依赖的段**，逐段登记（**相等**对拍，不是子集）。
    ///
    /// 一整段依赖溜出分母是这一族最省事的漏法：`[build-dependencies]` 与
    /// `[target.'…'.dependencies]` 在隔壁那份清单上真的有，本清单今天没有 ——
    /// 哪天有了，本模块要先出声。
    const DEPS: &str = "[dependencies]";
    const DEV_DEPS: &str = "[dev-dependencies]";
    const DEP_SECTIONS: &[&str] = &[DEPS, DEV_DEPS];

    /// 判档：**闭集**，名字只有这一处住址（表里与签字行都引这几个常量，不写第二遍字面量）。
    const MEASURED_WRITES: &str = "已量·有写面";
    const MEASURED_CLEAN: &str = "已量·未见写面";
    const UNMEASURED: &str = "未量·靠用法签字";
    /// 〔`K-R79` 09-12，来路 `DECISIONS.md#R38` 裁定二〕**第四档。**
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
             它的写面就是 daemon 要用的那个功能本身（远端部署 / 远端文件操作那一族）。\
             ⇒ 本档**不问**「凭什么进不来」，改问「**它写的那一面由谁划边界**」",
            "🔴 **本档自带边界，它不是例外**：签字里必须逐字出现 `边界判据：` \
             ＋ 一条**本文件里真存在**的判据名（反引号括起来），由 \
             `the_purposeful_write_verdict_names_a_boundary_judge_that_really_exists` 三处对拍。\
             降档 / 摘掉的条件是「daemon 不再需要它写」—— 那时它回到 `已量·有写面`\
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
            "**没读它的源码。** 签字依据只有「daemon 在它身上的用法不需要它自己写盘」——\
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

    /// **签字表**：`(crate 名, 段, 判档, 签字——它在 daemon 里做什么 · 凭什么落这一档)`。
    ///
    /// 谁签的：`实@09-04`（本轮实现方）。整表一个签字人，所以不占一列 ——
    /// 哪天有第二个人往里加行，那一列再立。
    const SIGNED: &[(&str, &str, &str, &str)] = &[
        (
            "acct-core",
            DEPS,
            MEASURED_CLEAN,
            "账号记录变换（纯数据），与 monitor 共用同一份；仓内 crate，现打 0 处写面",
        ),
        (
            "branch-core",
            DEPS,
            MEASURED_CLEAN,
            "分叉的记录变换（纯数据），与 monitor 共用同一份；仓内 crate，现打 0 处写面",
        ),
        (
            GATED_CRATE,
            DEPS,
            MEASURED_WRITES,
            "第三方 API key 的唯一住址（装它的类型 / 落盘格式 / 权限判断）。\
             ★ **它自己有两处写面**：`perm.rs` 的 `make_private`（收窄既有文件的权限）与 \
             `create_private`（建一个只给本人的新文件）。两处**都在那个 feature 后面**，\
             而本清单**刻意不开**它（清单那段注释逐字写着理由：daemon 只许读那份文件）\
             ⇒ 今天编不进来。这条前提由本模块那条 feature 判据钉着，不靠纪律",
        ),
        (
            "gate-core",
            DEPS,
            MEASURED_CLEAN,
            "§34 Gate 2 的唯一实现（生产段真的在执行它）；仓内 crate，现打 0 处写面",
        ),
        (
            "guard-core",
            DEV_DEPS,
            MEASURED_CLEAN,
            "源码扫描型判据的共用剥法。**只在测试期链接**，不进发布二进制；\
             仓内 crate，现打 0 处写面",
        ),
        (
            "libc",
            DEPS,
            UNMEASURED,
            "只用 pidfd 那三样（清单那段注释逐字），零 feature。它是 syscall 绑定 ——\
             写不写盘看调用点，而调用点在本 crate 里、由默认层那条判据数",
        ),
        (
            "notify",
            DEPS,
            UNMEASURED,
            "inotify 观测：daemon 只拿它订阅被观测目录底下的变化，一条写路径都不经它",
        ),
        (
            "notify-debouncer-mini",
            DEPS,
            UNMEASURED,
            "把上面那条的事件去抖之后再交出来 —— 同一条路上的第二段，同样只在读侧",
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
             **我没有扫过它的源码** —— daemon 侧这条路不传 known_hosts 路径、\
             host key 校验由 `dial::DialHandler` 自己在内存里比指纹，\
             但那是**用法**上的签字，不是对它源码的读数。\
             ⇒ 要升到 `已量·未见写面` 得真去扫它那棵树，本轮没做",
        ),
        (
            "search-core",
            DEPS,
            MEASURED_CLEAN,
            "`K-R100`：历史全文搜索口径的唯一实现（12 个纯函数 + 4 个口径常量 + snippet 预算 \
             + 预算顺序），与 monitor 共用同一份。**纯函数，无 IO** —— 它连 `std::fs` 都不 use，\
             取数（读 jsonl）仍在本 crate 的 `observe/search_query.rs` 里；仓内 crate，现打 0 处写面",
        ),
        (
            "serde",
            DEPS,
            UNMEASURED,
            "序列化派生；daemon 只拿它把 wire 帧与结构体互转，落盘那一步不经它",
        ),
        (
            "serde_json",
            DEPS,
            UNMEASURED,
            "JSON 编解码；同 `serde`，只在内存里把字节变成结构体、再变回去",
        ),
        (
            "shell-quote-core",
            DEPS,
            MEASURED_CLEAN,
            "POSIX 单引号 quote 的唯一实现（纯字符串变换）；仓内 crate，现打 0 处写面",
        ),
        (
            "tokio",
            DEPS,
            UNMEASURED,
            "运行时 + io。⚠ **本表唯一「写面在我们自己手上」的一条**：`fs` feature 开着 ⇒ \
             它**提供**写 API，而调用点全在本 crate 里 —— 那一面由默认层与只读白名单\
             那两条判据数，不由本表。本表管的是「它自己会不会写」",
        ),
        (
            "tracing",
            DEPS,
            UNMEASURED,
            "日志门面：它自己不选落点，落点由 subscriber 那一条决定",
        ),
        (
            "tracing-subscriber",
            DEPS,
            UNMEASURED,
            "只开 `env-filter`。日志去向由本 crate 自己给的 writer 定（今天是标准错误）——\
             落盘那一形要另一条 crate，而本清单上没有",
        ),
        (
            "usage-core",
            DEPS,
            MEASURED_CLEAN,
            "用量记录变换（纯数据），与 monitor 共用同一份；仓内 crate，现打 0 处写面",
        ),
        (
            "walkdir",
            DEPS,
            UNMEASURED,
            "目录遍历，纯只读：它交出的是路径，读不读、写不写由调用点决定",
        ),
        (
            "webpki-roots",
            DEPS,
            UNMEASURED,
            "根证书**数据**（一张常量表）—— 它没有 IO 那条代码路径要谈",
        ),
    ];

    /// 清单的**依赖段**逐条：`(段名, crate 名, 那一行原文)`。
    ///
    /// 两条判据：段名以 `dependencies]` 收尾 · 条目从**列 0 起**
    /// （多行内联表的续行 —— 那些 feature 字面量 —— 不是条目）。
    ///
    /// ⚠ 剥 `#` 整行注释走 `guard_core` 的**共享原语**，不在这里自己写第二份 ——
    /// 本函数第一版内联了一个 `#` 过滤，而 monitor 侧那张「剥注释实现只许一份」的登记表
    /// **当场逮住了它**（09-04 现打；它的人群跨三棵树，daemon 这一侧也在里面）。
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
    // **门禁九格一格没红**（cargo 1450 · daemon 587 · npm 1590 · e2e 12/8/264/72 · pb check 0，
    // 量于 `b4e289f` + 那一刀、未铺 `embedded-daemons`）。
    //
    // # 为什么这一档做得起来，而 `UNMEASURED` 那一档做不起来
    //
    // 本档今天 6 条**全是仓内 crate**（清单那一行带 `path =`）⇒ **源码就在树里**，
    // 重扫是零边际成本。`UNMEASURED` 那一档的解锁条件是「有人真去读了它的源码」，
    // 而那些是第三方 crate、源码不在树里 ⇒ **本件的做法对它们不适用**。
    // 🔴 **本件覆盖 `MEASURED_CLEAN` 这一档，不覆盖 `UNMEASURED` 那一档**；
    //    下面这几条判据绿了，说明的只是前者，**不许把后者也算进这个绿里**。
    //
    // # 它买不到什么（`§4` 诚实边界逐字，别读大一格）
    //
    // - 买到的是「**写面出现的那一刻会红**」，**不是**「那几棵 crate 永远干净」。两句不许压成一句。
    // - **模式表本身有多准，本件不判** —— 它沿用护栏既有的那两张表（[`super::tests::FS_MUTATION_PATTERNS`]
    //   与 [`super::tests::WHITELIST_STILL_FORBIDDEN`]）加起进程点；**那两张表漏掉的写法，本件一样漏**。
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

    /// 本件那把尺子：**护栏自己那两张模式表 ∪ 起进程点 ∪ 远端写那两张网**（现算，不手抄第二份）。
    ///
    /// ⚠ 起进程点那一项**运行时拼** —— 本文件里别再多一处那个字面量：
    /// [`super::spawn_registry`] 那条判据靠**按文件名跳过本文件**才不自匹配，
    /// 而跨文件数它的那些判据不一定跳。
    ///
    /// 〔`K-R79` 09-12〕并进了 [`super::remote_write_layer`] 那两张网。**理由是「同职」**：
    /// 这把尺子替 `已量·未见写面` 那一档回答「这几棵仓内 crate 今天干不干净」，
    /// 而「干净」如果只算本机写面，那么哪天 `acct-core` 里长出一处远端写，
    /// 这一档照样绿 —— 那正是本件在 daemon 本体上治的同一个洞，换一棵树再挖一遍。
    /// ⚠ 并进来是**收紧**：今天这六棵 crate 上现打 0 处命中（本轮量过，逐词都是 0）。
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
        let at = line.find("path")?;
        let rest = line[at + "path".len()..].trim_start();
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

    /// 一棵仓内 crate 的 `src` 绝对住址（相对本清单所在目录解析）。
    fn crate_src_dir(rel: &str) -> std::path::PathBuf {
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join(rel)
            .join("src")
    }

    /// 把某一档拆成「扫得了的」与「扫不了的」两半。
    ///
    /// 🔴 **扫不了的不许静默跳过** —— 那正是本件要治的病的同族形状（判决没人盯着）。
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
             「它在 daemon 里做什么 · 有没有写面 · 依据是什么」。\n\
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
        for (name, sect, verdict, why) in SIGNED {
            assert!(
                is_verdict(verdict),
                "`{name}` 的判档是 `{verdict}` —— 它不在闭集里。闭集（现算）：{}",
                verdict_names().join(" / ")
            );
            assert!(
                why.trim().chars().count() >= 20,
                "`{name}` 的签字太短（实得 {} 字）—— 这一列的读者是下一个想加依赖的人，\
                 要写的是「它在 daemon 里做什么 · 凭什么落这一档」，不是一句「没问题」",
                why.trim().chars().count()
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

    /// ★★ 那条**有写面**的签字，它的前提是「那个 feature 本清单没开」—— 把前提钉住。
    ///
    /// 这一条是本模块里唯一**不只钉「说得出来」**的判据：它钉的是那句话赖以成立的那个事实。
    /// feature 一开，两处写面就真进了本 crate 的依赖树，而那时签字里那句
    /// 「今天编不进来」当场变成假话 —— 本仓最高频的病是「代码订正了，盘没跟着改」，
    /// 这一条挡的正是它的反面：**盘上写着的前提被代码改掉了而盘不知道。**
    #[test]
    fn the_only_signed_write_surface_still_rides_on_a_feature_this_manifest_leaves_off() {
        let with_surface: Vec<&str> = SIGNED
            .iter()
            .filter(|(_, _, v, _)| *v == MEASURED_WRITES)
            .map(|(n, ..)| *n)
            .collect();
        assert_eq!(
            with_surface,
            vec![GATED_CRATE],
            "「有写面」那一档的成员变了：{with_surface:?}\n\
             本条只钉得住 `{GATED_CRATE}` 那一条的前提（它的写面在一个 feature 后面）。\n\
             新来一条有写面的依赖 ⇒ 先回答「它凭什么进不来」「那个前提谁钉着」，再改这里。"
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
            !guard_core::contains_word(&line, &feature),
            "本清单给 `{GATED_CRATE}` 开了 `{feature}`：{line}\n\
             ⇒ 那个 feature 才带「把文件收窄 / 建私有文件」的平台原语，一开，\
             它那两处写面就真编进本 crate 的依赖树了。\n\
             **这不是改个断言的事**：`SIGNED` 里那一行的签字前提当场作废，回来重签，\
             并回答「daemon 现在算不算自己在写用户既有数据」。"
        );
        // 反空真：探针对「开着」的写法必须认得出来，否则上面那条零命中断言什么也不说明。
        //
        // 🔴 **样本里那个词不许由 needle 拼出来** —— 那样这条控制**恒真**。
        // 09-04 现打的活体（本轮死值验 M5 逮到的，就在这一行上）：第一版逐字是
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

    /// ★★ 「今天零条未签字」这个读数要先证明**尺子接上了** —— 喂一份合成清单，它必须点名。
    ///
    /// 合成的那条刻意就是本件的形状：**引擎作为一条新依赖进来**那天。
    #[test]
    fn an_unsigned_dependency_is_really_reported_so_that_the_zero_is_not_vacuous() {
        // 引擎那个 crate 名**运行时拼**：monitor 侧有一条判据在数「哪几棵树里出现过它」，
        // 别让本文件变成那张表里的第二处命中。
        let engine = format!("code-picture{}core", "-");
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
            "★ 这一刀正是本件的形状：**引擎被编进 daemon** 那天，它会作为一条新依赖出现，\
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
             动它等于同时放宽 daemon 本体那两层判据。\n\
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
    /// 两者在终端上一模一样。本条把它们分开：同一把尺子扫 `{MEASURED_WRITES}` 那一档
    /// （今天唯一成员 [`GATED_CRATE`]，它的签字里点名了 `perm.rs` 那两处写面），必须 > 0。
    ///
    /// ⚠ 哪天那一条也变干净了，本条会红 —— **那是对的**：回来重挑一个非空对照，
    /// 不许把本条删掉了事（删掉之后上面那条就退回成一句空真）。
    #[test]
    fn the_same_ruler_still_lights_up_on_the_crate_that_really_writes() {
        let ruler = resident_ruler();
        let with_surface = crates_with_verdict(MEASURED_WRITES);
        assert!(
            !with_surface.is_empty(),
            "`{MEASURED_WRITES}` 这一档今天一条成员都没有 —— 上面那条零命中断言从此没有对照，\
             它是「真干净」还是「尺子瞎了」分不出来了。回来重挑对照。"
        );
        let (scannable, out_of_reach) = split_by_reachability(&with_surface);
        assert!(
            out_of_reach.is_empty(),
            "对照那一档里有本尺子够不着的条目：\n{}",
            out_of_reach.join("\n")
        );
        for (name, rel, dir) in &scannable {
            let hits = write_surface_hits(dir, &ruler);
            assert!(
                !hits.is_empty(),
                "同一把尺子在 `{name}`（`{rel}`）上命中 **0** —— 而它的判档是 `{MEASURED_WRITES}`。\n\
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
    // # 这两条各接哪一格（`DECISIONS.md#R38` 裁定二）
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
    // 塞进 `已量·有写面` ⇒ [`the_only_signed_write_surface_still_rides_on_a_feature_this_manifest_leaves_off`]
    // （那一档的成员集是**相等**断言）；塞进 `已量·未见写面` ⇒
    // [`the_clean_verdict_is_re_measured_on_the_tree_every_run`]（第三方 crate 没有 `path =`，
    // 落进「本判据扫不了」那一格）；塞进 `未量·靠用法签字` ⇒ **在本件之前一条都不红**，
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
                "它的写面被一个没开的 feature 关着 / 边界判据：`the_daemon_tree_has_no_remote_write_today_and_the_scan_face_is_not_empty`",
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
        let good = format!("daemon 就是要它写远端。{BOUNDARY_TAG}`{RESIDENT_GUARD}`");
        let cases: [(&str, &str); 4] = [
            ("没写边界", "daemon 就是要它写远端，反正已经登记了"),
            ("边界不成形", "daemon 就是要它写远端。边界判据：那条常驻的"),
            (
                "点名了一个盘上没有的判据",
                "daemon 就是要它写远端。边界判据：`a_judge_that_was_renamed_away`",
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
// 〔`K-R79` 09-12〕**远端写那一半**
// ══════════════════════════════════════════════════════════════════════════════
#[cfg(test)]
mod remote_write_layer {
    //! 〔`K-R79` 09-12，来路 `DECISIONS.md#R38` 裁定一〕
    //! **本护栏在 C 类要走的那条路上，此前是瞎的。**
    //!
    //! # 它守的性质 · 它扫的人群（两行，各只许有一句 —— 同本文件顶上 `KG62` 那对）
    //!
    //! - **它守的性质是**：daemon **进程自身**不许改动**别人机器上**的用户既有数据。
    //!   （本机那一半由默认层与只读白名单守；本层守的是**远端**那一半。）
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
    //! monitor 侧 `remote_write_registry` 的头注现打过这条链的读数 —— 按写原语的方法名取样是
    //! 8 份文件、其中 6 份是本机噪声；换成「谁持有 SFTP 会话」立刻收到 3 份、零噪声。
    //! ⇒ **能力那一端是扼流点，动词那一端不是。** 本层因此把能力网排在第一位，
    //! 动词网只作第二道（能力取得点住在别的文件时，动词那一处仍然点得出名）。
    //!
    //! # ⚠ 它认不出什么 —— **口径与射程同句写清**（`KR79D3` 的落点是 [`BLIND_SPOT_LAYERS`]）
    //!
    //! - **远端 exec**：`channel_open_session` ＋ 把一条会写的 shell 命令交给远端。
    //!   一条命令写不写，**源码文本上判不了**（monitor 侧 `remote_write_registry` 头注
    //!   逐字给过同一条结论：`2>/dev/null` 就带 `>`）。这与默认层认不出 `Command::new`
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

    use super::tests::strip_cfg_test;

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
             反过来，daemon 今天**没有任何理由**去开子系统（它那条 `--dial` 臂开的是 \
             session channel，不是子系统）⇒ 这一条今天在树上恒零，出现即越线。",
        ),
        (
            FAMILY_SUBSYSTEM,
            "SFTP **会话类型名**。monitor 侧 `remote_write_registry` 头注现打过：\
             按写原语的方法名取样 = 8 份文件、6 份是本机噪声；换成「谁持有 SFTP 会话」\
             = 3 份、零噪声。⇒ **会话对象是远端写那条路的扼流点**，方法名不是。",
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
    ///   默认层已经在数；`sftp.remove_file(` 是方法形，默认层一条都不匹配 —— 那正是本件的题面。
    /// - 它是**超集**：本机某个**恰好同名**的方法也会红（今天树上 **0 处**，分母 = 本 crate
    ///   `src/` 生产段全部 `.rs`，由 [`the_verb_net_has_no_false_positive_on_this_tree_today`] 现打）。
    /// - 🔴 **`write_all(` 刻意不在表上**：daemon 唯一合法的写就是把 wire 帧写 stdout
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
    /// ⚠ **`scan_tree!` 按构造摘掉调用者自己那一份**（就是本护栏文件）——
    /// 这里**刻意不补回来**，理由与 `super::tests::scan` 跳过本文件逐字同一条：
    /// 本文件整体在 `#[cfg(test)]` 内、生产段是空的，而它的锚点表本身就是一串会自匹配的字面量。
    fn production_tree() -> Vec<(String, String)> {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut out: Vec<(String, String)> = Vec::new();
        for (path, src) in guard_core::scan_tree!(&root, &["rs"]) {
            let rel = path
                .strip_prefix(&root)
                .unwrap_or(&path)
                .to_string_lossy()
                .replace('\\', "/");
            out.push((rel, strip_cfg_test(&src)));
        }
        out.sort();
        out.dedup_by(|a, b| a.0 == b.0);
        out
    }

    /// 扫描面地板：份数与字节数各一条。**两条都要** —— 份数挡「树没扫到」，
    /// 字节数挡「每一份都被剥空」（`guard_support` 那条头注记着这两者互相掩盖过）。
    const TREE_FILE_FLOOR: usize = 37;
    const TREE_BYTE_FLOOR: usize = 80_000;

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
            // 落在两张模式表射程里的每一处，默认层 / 白名单层都看得见（看得见 ≠ 会红：
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
            "L3 远端能力写（本件新加）",
            layer_remote_capability,
            "能力网认的是**取得能力**那一步、动词网认的是**协议操作名的方法形**；\
             两者都是源码文本 ⇒ `use` 别名 · 宏里拼出来的调用 · 自己改名的封装，一律看不见",
        ),
        (
            "L4 远端代理写（远端 exec）",
            layer_remote_proxy,
            "**那条命令写不写，源码文本上判不了** —— 与默认层认不出 `Command::new` 同一个洞的远端孪生",
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
        for (family, why) in REMOTE_CAPABILITY_ANCHORS {
            assert!(
                FAMILIES.contains(family),
                "`{family}` 不在家族闭集里：{FAMILIES:?}"
            );
            assert!(
                why.trim().chars().count() >= 30,
                "有一条能力锚点没写清它凭什么是形状（实得 {} 字）—— \
                 这一栏的读者是下一个想往表里加一行的人",
                why.trim().chars().count()
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

    /// ★★ 正题：**整棵 daemon 树的生产段，今天一处远端写都没有** —— 而这个零不是空转。
    #[test]
    fn the_daemon_tree_has_no_remote_write_today_and_the_scan_face_is_not_empty() {
        let tree = production_tree();
        let bytes: usize = tree.iter().map(|(_, c)| c.len()).sum();
        assert!(
            tree.len() >= TREE_FILE_FLOOR,
            "只扫到 {} 份源文件（地板 {TREE_FILE_FLOOR}）—— 抽取坏了，本条在空转",
            tree.len()
        );
        assert!(
            bytes >= TREE_BYTE_FLOOR,
            "扫描面只有 {bytes} 字节（地板 {TREE_BYTE_FLOOR}）—— 每一份都被剥空了，本条在空转"
        );
        let mut hit: Vec<String> = Vec::new();
        for (rel, prod) in &tree {
            if let Some((needle, family)) = violates_remote_write_layer(prod) {
                hit.push(format!("  src/{rel} 含 `{needle}`（{family}）"));
            }
        }
        assert!(
            hit.is_empty(),
            "daemon 生产段里有 {} 处**远端写**：\n{}\n\n\
             红线 I7 守的性质是「daemon 进程自身不许改动用户既有数据」，\
             而**别人机器上的数据也是用户既有数据** —— 本层就是那一半。\n\
             ⇒ 真要让后端去写远端，那不是改这条判据的事：\n\
             ① 先把「远端 rc 能不能替用户写」那一问裁掉（`ROADMAP.md#KU31`，今天还没裁）；\n\
             ② 裁「许」之后，这里要的是**一张逐条登记的白名单**（形状照 \
             `WRITE_WHITELIST_MODULES`：写什么 · 路径由谁定 · 哪一道围栏拦着），\
             不是把本层删掉。",
            hit.len(),
            hit.join("\n")
        );
        println!(
            "K-R79：本趟扫 {} 份生产段源文件 / {bytes} 字节，能力网 {} 针 ＋ 动词网 {} 针，远端写命中 0",
            tree.len(),
            capability_needles().len(),
            verb_needles().len()
        );
    }

    /// ★ 动词网是**超集** —— 它今天在这棵树上的误红处数，是一个要现打的读数，不是一句话。
    ///
    /// 上面那条把两张网合起来断言「0 处」；本条把**动词网单独**拎出来再数一遍，
    /// 是因为两者说的不是同一件事：合起来那个 0 里，动词网那一半有可能靠
    /// 「本机恰好没有同名方法」撑着，而那是**今天的巧合**，不是本层的性质。
    /// ⇒ 哪天它红了，先读这条的报错：**它多半不是一次越线，是一次同名误伤**。
    #[test]
    fn the_verb_net_has_no_false_positive_on_this_tree_today() {
        let tree = production_tree();
        let mut hit: Vec<String> = Vec::new();
        for (rel, prod) in &tree {
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
             ① 真的长出了远端写 ⇒ 走上面那条判据的报错里写的两步；\n\
             ② 本机某个方法**恰好同名**（动词网是超集，头注逐字写着这件事）\
             ⇒ 那要么给这一处一条**逐条登记的例外**（写清它写的是本机的什么），\
             要么把动词网那一条换成更窄的形状。**不许直接把那一条从表上删掉。**",
            hit.len(),
            hit.join("\n")
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
        assert_eq!(
            sites,
            vec!["dial/mod.rs"],
            "远端 session channel 的处数 / 住址变了：{sites:?}\n\
             它是本层**认不出的那一格**的活体标本 —— 变了就回来重读这条登记，\
             别只改这个断言。（`dial_locality` 管的是「跑不跑传输层握手」，是另一维。）"
        );
        for (rel, prod) in &tree {
            if rel == "dial/mod.rs" {
                assert_eq!(
                    violates_remote_write_layer(prod),
                    None,
                    "反例 `dial/mod.rs` 今天**红了** —— 那说明本层的射程变了，\
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
        assert!(
            tree.len() >= TREE_FILE_FLOOR,
            "语料只有 {} 份 —— 下面整张读数在空转",
            tree.len()
        );
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
