//! U3（2026-08-01）：**分层护栏** —— §1.1 第二条解耦线的机器判据。
//!
//! # 它守的是什么
//!
//! `observe/`（读）与 `control/`（改变世界）之间**只许一个方向**：
//! `observe → control`，且**接口面必须显式列举、条数被钉住**；反向一条都不许。
//!
//! 这条性质没有护栏的话会以最不起眼的方式退化：某天 `fork_write` 需要读一份账号信息，
//! 顺手 `use crate::observe::accounts_query::...` —— 编译通过、测试全绿，而两层从此互相咬死。
//! U3 摸底时**真的就有这么一条**（`fork_write` → `accounts_query::read_regular_capped`），
//! 处置不是开例外，是把那个函数搬进 `common/`（它本来就不是 observe 的域逻辑）。
//!
//! # 为什么正向要**钉条数**而不是「随便跨」
//!
//! 允许跨层的边今天**恰好两个符号**，都由 `watcher` 发起、都有具体说得清的理由：
//! `control::tmux_hook::install_hooks`（tmux hook 活在 server 内存里、每次 server 重起要重装，
//! 而「server 起来了」只有 observe 知道）与 `control::identity_tag::tag`（`U-NP④`：
//! `(pid, sid)` 只在 pidfile 事件那一刻同时在手，让 control 自己去发现只能靠轮询，
//! 而消灭轮询正是那件事的全部目的）。
//! **「有正当例外」与「这条线随便穿」是两回事**，中间隔着的就是这个计数。
//! 多一个就红，逼下一个人把他的理由也写出来。
//!
//! 注：本模块整体在 `#[cfg(test)]` 内，非测试构建为空。

#![cfg(test)]

#[cfg(test)]
mod tests {
    use crate::guard_support::production_code;

    /// 允许的 `observe → control` 跨层引用，逐条列举。
    ///
    /// **加一条之前先回答**：为什么这件事非得由观测侧发起？能不能反过来由 control 主动做？
    /// （`install_hooks` 的答案：不能 —— 触发时机是「tmux server 起来了」，
    /// 那是 socket 目录 inotify 观测到的事实，control 侧没有这个信号，
    /// 硬要它自己发现只能靠轮询，与 §41 零定时器铁律正面冲突。）
    ///
    /// `identity_tag::tag`（`U-NP④`，08-14）的答案**同型**：触发时机是「某个 pidfile 出现/
    /// 原地换了 sid」，那是 `sessions/` inotify 观测到的事实（`(pid, sid)` 也只有那一刻同时在手）；
    /// control 侧没有这个信号，硬要它自己发现只能靠轮询 —— 而本件的**全部目的**就是
    /// 把 `shared/ccm` 那条每秒轮询消掉（用户 08-14：「不要轮询」「ccm 做到必须走 daemon」）。
    /// 反过来做只是把轮询从 ccm 搬到 daemon。
    const ALLOWED_OBSERVE_TO_CONTROL: &[&str] = &[
        "crate::control::identity_tag::tag",
        "crate::control::tmux_hook::install_hooks",
    ];

    /// ★★ `KY5`〔`K-W1A` 08-26〕：**谁能碰新立的 `plugin/` 层，逐条登记**（`(哪一层, 符号, 为什么)`）。
    ///
    /// # 为什么新层非要立判据不可 —— 它不是「不撞所以安全」
    ///
    /// 本护栏的 `layer_sources` 是**按层名拼路径**的，定义域此前只有 `observe/` 与 `control/`。
    /// ⇒ `src/plugin/` 建出来的那一刻，`control → plugin` · `plugin → control` ·
    /// `plugin → observe` **三个方向零判据** —— 那不是「撞不着」，那是**覆盖面被绕开**
    ///（本仓「守卫范围 ≠ 性质范围」那一族的又一形）。新层一天没有方向判据，
    /// 它就是一条**没人守的层间边**：某天调用口顺手 `use crate::control::gate;` 去问一句 tmux，
    /// 编译通过、全绿，而「通用调用口」从此变成 control 的私有助手。
    ///
    /// # 加一条之前先回答
    ///
    /// 这个符号是**调用口的形状**，还是**某个插件的语义**？后者不该跨过来 ——
    /// 它该住在调用方自己那一侧（`E6`）。今天这 5 条全是前者，而且全部由
    /// `control/cc_bus.rs` 一个文件发起：② 那一段（协商）今天零生产调用方，所以不在表里。
    ///
    /// ⚠ **类型也要登记，不只是函数**：`Done` / `NotRun` 出现在调用方的签名与 `match` 里，
    /// 它们和函数一样是接口面。漏登记等于「接口只算函数」——那是个会腐的口径。
    const ALLOWED_INTO_PLUGIN: &[(&str, &str, &str)] = &[
        (
            "control",
            "crate::plugin::discover::find",
            "① 找它：候选顺序、要不要兜 PATH、找不到那句话的尾巴**全是入参**，\
             调用口只负责按顺序走一遍并把「查过哪儿」拼成一句能自证的话",
        ),
        (
            "control",
            "crate::plugin::invoke::run",
            "③ 传 argv 起它：**全 crate 唯一一处**起进程口（`readonly_guard::ALLOWED` 里登记的那一处），\
             期限走 `timeout` 前缀交给子进程 —— 零定时器铁律的落点",
        ),
        (
            "control",
            "crate::plugin::invoke::Done",
            "④ 骨架的**返回类型**：`code`（`None` = 被信号打断）+ 两条流 + `diagnosis()`。\
             ⚠ 码→语义的**映射表刻意不在这里** —— 同一个码在不同插件里语义互斥",
        ),
        (
            "control",
            "crate::plugin::invoke::NotRun",
            "「根本没跑起来」那一类的类型：调用方要 `match` 它，才分得出\
             「我给的参数太大」（自己能修）与「那个程序坏了」（自己修不了）",
        ),
        (
            "control",
            "crate::plugin::invoke::TIMED_OUT_CODE",
            "期限命令超时时的那个退出码。它是**那条命令的事实**、不是某个插件的语义 ⇒ 住通用层；\
             而「超时之后跟人怎么说」是插件自己的话，留在调用方",
        ),
    ];

    /// 收集某一层下所有 `.rs` 的 `(相对路径, 生产段)`。
    fn layer_sources(layer: &str) -> Vec<(String, String)> {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("src")
            .join(layer);
        layer_sources_at(&root, layer)
    }

    /// [`layer_sources`] 的**根可注入**版本。
    ///
    /// 抽出这一层只为一件事：`K-G4` 的活体夹具要让**真判据本身**（不是它的复刻）
    /// 跑在一棵真的、盘上存在的小树上。根写死在函数里的话，夹具只能另写一份扫描，
    /// 而「另写一份」证明的是那一份、不是护栏〔`brief` 第 9 条：空真要用**活体**夹具治〕。
    ///
    /// `label` 只进报错文本里的相对路径前缀，**不进任何断言**〔`6g`：断言别取自夹具的名字〕。
    fn layer_sources_at(root: &std::path::Path, label: &str) -> Vec<(String, String)> {
        let layer = label;
        let root = root.to_path_buf();
        let mut out = Vec::new();
        let mut stack = vec![root.clone()];
        while let Some(dir) = stack.pop() {
            for entry in std::fs::read_dir(&dir).expect("read layer dir") {
                let path = entry.expect("dir entry").path();
                if path.is_dir() {
                    stack.push(path);
                    continue;
                }
                if path.extension().and_then(|e| e.to_str()) != Some("rs") {
                    continue;
                }
                let rel = path
                    .strip_prefix(&root)
                    .unwrap_or(&path)
                    .to_string_lossy()
                    .replace('\\', "/");
                let src = std::fs::read_to_string(&path).expect("read rs");
                out.push((format!("{layer}/{rel}"), production_code(&src)));
            }
        }
        out.sort();
        out
    }

    /// 抽出生产段里所有指向 `layer` 的引用（去重、排序）。
    ///
    /// # 判据必须同时认三种拼法 —— 少认一种就是安慰剂
    ///
    /// 初版只扫 `crate::<layer>::` 这一种，Phase D 审计当场用三个变异证伪：
    ///
    /// | 拼法 | 初版 | 现在 |
    /// |---|---|---|
    /// | `use crate::observe::accounts_query;` + 短名调用 | 抓到（`use` 行本身含完整路径） | 抓到 |
    /// | **`use crate::observe as ob;`** + `ob::accounts_query::run(..)` | **全绿** | 抓到 |
    /// | **`super::super::observe::accounts_query::run(..)`** | **全绿** | 抓到 |
    ///
    /// # 而那次修法只补了两根针，没有把人群圈出来 —— 于是漏的是**最朴素的一种**
    ///
    /// 〔audit-0805 08-06〕接着上表往下变异，三条里两条**全绿**：
    ///
    /// | 拼法 | 上表那版 | 现在 |
    /// |---|---|---|
    /// | **`use crate::observe;`** + `observe::accounts_query::run(..)` | **全绿** | 抓到 |
    /// | **`use crate::{observe, wire};`** | **全绿** | 抓到 |
    /// | `use crate::observe as obs;` | 抓到 | 抓到 |
    ///
    /// 讽刺的地方在于：上一版**专门禁了层别名**，理由逐字写着「建立之后所有用法都绕过本护栏」——
    /// 而 `use crate::observe;` **性质完全一样**（后面全是裸 `observe::…`），只是不用起别名，
    /// 更常见、更省事，却不在针里。⇒ 手写「拼法清单」这件事本身就是漏洞来源：
    /// 补一条只是把清单变长，下一种写法照样在清单外。
    ///
    /// 本轮改成**从层名派生**：`crate::<layer>` / `super::super::<layer>` 每一处出现都要分类
    ///（后面是 `::` ⇒ 符号路径；否则 ⇒ 模块级引入，与别名同罪），外加成组导入单独一路。
    ///
    /// 中间那一栏不是理论风险：审计在**真放进一条反向边**（control 调 observe 的 `run`）的状态下
    /// 跑了全量 `cargo test`，**199 passed / RC=0**，没有一条门禁叫。
    ///
    /// **这与本轮判 `readonly_guard` 裸文件名匹配有罪是同一类问题** —— 护栏对一种
    /// 完全合法、编译器认账的写法视而不见。自己刚批评过的形状不能自己再犯一遍。
    ///
    /// # 它仍然挡不住什么（如实登记，别再宣称「恰好」而不加限定）
    ///
    /// - **测试段不受管**（下面走 `production_code` 剥掉）。这是**有意**的：分层是生产架构的性质，
    ///   测试跨层构造夹具是正常的。但这个取舍此前一个字都没写 —— 审计变异 M6 证实测试段里
    ///   放一条真反向边全绿。
    /// - 更曲折的间接（把符号先 `pub use` 到第三个模块再引）扫不到。
    ///   真判据得上 `syn` 级解析，成本远超本仓需要；写在这里，别让人以为它是完备的。
    fn refs_to_layer(code: &str, layer: &str) -> Vec<String> {
        let mut hits: Vec<String> = Vec::new();
        // 〔audit-0805 08-06〕**不再一种拼法一根针，改成从层名派生**（理由见上面那张表末行）。
        //
        // 做法：找出每一处 `crate::<layer>` / `super::super::<layer>`，**按紧随其后的字符分类** ——
        // 后面是 `::` 就是符号路径（照旧抠符号），否则就是**模块级引入**（`;` / `,` / ` as ` / `}`）。
        // 这样「怎么把这一层弄进来」的写法是被**枚举**出来的，不靠人再想起第四种。
        for root in ["crate::", "super::super::"] {
            let anchor = format!("{root}{layer}");
            let mut from = 0usize;
            while let Some(rel) = code[from..].find(&anchor) {
                let i = from + rel;
                from = i + anchor.len();
                let tail = &code[from..];
                // `crate::observed` / `crate::observe_helpers` 不是本层，别误命中。
                if tail.starts_with(|c: char| c.is_ascii_alphanumeric() || c == '_') {
                    continue;
                }
                if let Some(rest) = tail.strip_prefix("::") {
                    let end = rest
                        .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_' || c == ':'))
                        .unwrap_or(rest.len());
                    let mut sym = format!("crate::{layer}::{}", &rest[..end]);
                    while sym.ends_with(':') {
                        sym.pop();
                    }
                    if !hits.contains(&sym) {
                        hits.push(sym);
                    }
                } else {
                    let mark = format!(
                        "{anchor}（**模块级引入**：引进来之后用法都是裸 `{layer}::…`，\
                         本护栏再也看不见 ⇒ 与层别名同性质，一样禁）"
                    );
                    if !hits.contains(&mark) {
                        hits.push(mark);
                    }
                }
            }
        }
        // 成组导入 `use crate::{observe, wire};` —— 层名被包进花括号，上面的锚点一个都对不上。
        // 实测：这一行放进 `control/gate.rs`，三条判据全绿。
        for prefix in ["use crate::{", "use super::super::{"] {
            let mut from = 0usize;
            while let Some(rel) = code[from..].find(prefix) {
                let i = from + rel;
                from = i + prefix.len();
                let stmt = &code[i..];
                let end = stmt.find(';').map(|e| e + 1).unwrap_or(stmt.len());
                let group = &stmt[..end];
                if group_names_layer(group, layer) {
                    let mark = format!(
                        "{}（**成组导入**：层名在花括号里 ⇒ 按层拆成一行一个，别让它藏在组里）",
                        group.replace('\n', " ")
                    );
                    if !hits.contains(&mark) {
                        hits.push(mark);
                    }
                }
            }
        }
        hits.sort();
        hits
    }

    /// 成组导入里，`layer` 是不是**顶层的一个组员**（`{observe, wire}` 里的 `observe`）。
    ///
    /// 只认前面紧挨着 `{` / `,` / 空白的那种，免得 `crate::{common::observe_helpers}` 误命中。
    fn group_names_layer(group: &str, layer: &str) -> bool {
        let mut from = 0usize;
        while let Some(rel) = group[from..].find(layer) {
            let i = from + rel;
            from = i + layer.len();
            let before_ok = group[..i]
                .chars()
                .next_back()
                .is_some_and(|c| c == '{' || c == ',' || c.is_whitespace());
            let after_ok =
                !group[from..].starts_with(|c: char| c.is_ascii_alphanumeric() || c == '_');
            if before_ok && after_ok {
                return true;
            }
        }
        false
    }

    /// 采集面自检 —— **照抄 `no_timer_guard` 的做法，不自己另发明一套弱的**。
    ///
    /// # 为什么不用「≥3 文件 / ≥10_000 字节」
    ///
    /// 初版就是那样写的，Phase D 审计两头都点了：
    ///
    /// - **`observe/` 侧太松**：实测 9 文件 / 90_618 字节，余量 9.1 倍 ⇒
    ///   `accounts_query.rs`（16.5KB）**整个掉出采集面，地板照样绿**，反向判据静默失效。
    ///   而同一个仓的 `no_timer_guard` 早就论证过这一点并给了正解，我没沿用。
    /// - **`control/` 侧太紧**：实测 4 文件 / 11_863 字节，余量只有 **1.19 倍**。
    ///   而账本 S14 写明 `resolve_query` 要在 U6/U8 被吸收进计划面 —— 它一走 control 只剩
    ///   6_522 字节，断言当场红，报的却是「**采集坏了，下面的断言是空转**」：
    ///   **一条指向完全错误方向的诊断**，正是本轮在 `matches_registered` 那里刚批评过的形状。
    ///
    /// ⇒ 改成**数量相等**：独立走一遍目录树数 `.rs`，与采集到的条数比。
    /// 它对「文件增删」免疫（那是正常演进），只对「采集漏了」敏感 —— 而后者才是要防的。
    fn assert_collection_is_complete(layer: &str, files: &[(String, String)]) {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("src")
            .join(layer);
        assert_collection_is_complete_at(&root, layer, files);
    }

    /// [`assert_collection_is_complete`] 的**根可注入**版本（理由同 [`layer_sources_at`]）。
    fn assert_collection_is_complete_at(
        root: &std::path::Path,
        layer: &str,
        files: &[(String, String)],
    ) {
        let root = root.to_path_buf();
        // 刻意与 `layer_sources` 分开写：那边还要读文件、剥生产段，这边只数个数。
        let mut tree = 0usize;
        let mut stack = vec![root];
        while let Some(dir) = stack.pop() {
            for entry in std::fs::read_dir(&dir).expect("read layer dir") {
                let path = entry.expect("dir entry").path();
                if path.is_dir() {
                    stack.push(path);
                } else if path.extension().and_then(|e| e.to_str()) == Some("rs") {
                    tree += 1;
                }
            }
        }
        assert_eq!(
            files.len(),
            tree,
            "{layer}/ 采集到 {} 个 .rs，而树上有 {tree} 个 —— **采集漏了文件**，\
             下面的分层判据对漏掉的那些是瞎的。",
            files.len()
        );
        assert!(
            tree >= 2,
            "{layer}/ 只有 {tree} 个 .rs —— 这一层是不是已经名存实亡了？"
        );
        // 剥法自检：剥完不许残留测试属性。`no_timer_guard`/`build_id_guard`/`readonly_guard`
        // 都有这条，本护栏初版是唯一没有的（Phase D 审计指出）。
        // 它挡的是「剥少了 ⇒ 护栏开始扫测试代码 ⇒ 被夹具打红 ⇒ 有人顺手放宽护栏」。
        for (name, prod) in files {
            crate::guard_support::assert_no_test_code(&format!("{layer}/{name}"), prod);
        }
    }

    /// ★ **反向零容忍**：`control/` 不许引用 `crate::observe`。
    #[test]
    fn control_layer_must_not_reference_observe() {
        let files = layer_sources("control");
        assert_collection_is_complete("control", &files);
        let mut bad: Vec<String> = Vec::new();
        for (name, code) in &files {
            for sym in refs_to_layer(code, "observe") {
                bad.push(format!("{name} → {sym}"));
            }
        }
        assert!(
            bad.is_empty(),
            "control/ 引用了 observe（§1.1-2 反向不许）：\n  {}\n\
             **先别急着加例外** —— U3 摸底时那条反向边（fork_write → accounts_query::read_regular_capped）\
             的正解是「那个函数根本不属于 observe」，搬进 common/ 之后边就没了。\n\
             先问：被引用的那个东西，是不是也只是个放错地方的通用工具？",
            bad.join("\n  ")
        );
    }

    /// ★ **正向要显式列举且条数钉死**：`observe/` 只许用登记过的那几个 control 符号。
    #[test]
    fn observe_to_control_interface_is_exactly_the_registered_set() {
        let files = layer_sources("observe");
        assert_collection_is_complete("observe", &files);
        let mut found: Vec<String> = Vec::new();
        for (_, code) in &files {
            for sym in refs_to_layer(code, "control") {
                if !found.contains(&sym) {
                    found.push(sym);
                }
            }
        }
        found.sort();
        let mut want: Vec<String> = ALLOWED_OBSERVE_TO_CONTROL
            .iter()
            .map(|s| s.to_string())
            .collect();
        want.sort();
        // S1（Phase D 审计）：**登记项必须钉到函数级**。
        //
        // 审计的变异 M5 显示：`use crate::control::tmux_hook;`（模块级）会被记成一个新条目
        // `crate::control::tmux_hook`。下一个人「修红」最省事的办法就是把它加进表里 ——
        // 从此 `tmux_hook` 的**任意函数**都能被 observe 调，而计数仍是「2 条」、护栏再无信号。
        // ⇒ 直接禁掉模块级登记：`crate::<layer>::` 之后必须有 ≥2 段。
        for e in ALLOWED_OBSERVE_TO_CONTROL {
            let tail = e
                .strip_prefix("crate::control::")
                .unwrap_or_else(|| panic!("登记项必须以 `crate::control::` 开头：{e}"));
            assert!(
                tail.contains("::"),
                "登记项 `{e}` 只钉到**模块级** —— 那等于把整个模块的接口面都放开，\
                 而计数看不出区别。必须钉到函数：`crate::control::<模块>::<函数>`。"
            );
        }
        assert_eq!(
            found, want,
            "observe → control 的接口面与登记表对不上。\n\
             **多出来的**：加进 `ALLOWED_OBSERVE_TO_CONTROL` 之前先回答「为什么这件事非得由观测侧发起、\
             control 能不能自己做」——那张表的头注写着 `install_hooks` 的答案长什么样。\n\
             **少了的**：说明那条跨层调用没了，清理登记（别留着，登记表腐烂比没有登记更糟）。"
        );
    }

    /// ★★ `KY5` 正题之一：**`plugin/` 不许反过来引用 `control/` 或 `observe/`**。
    ///
    /// 通用调用口一旦认识控制面或观测面，它就不再是「谁都能用的口」——
    /// 它变成 control 的一个私有助手，而下一个插件接进来时会发现自己继承了一堆不相干的知识。
    /// 两个方向都是**零容忍**：本层要的东西一律走**入参**（`E6` 的通则）。
    #[test]
    fn plugin_layer_must_not_reference_control_or_observe() {
        let files = layer_sources("plugin");
        assert_collection_is_complete("plugin", &files);
        let mut bad: Vec<String> = Vec::new();
        for (name, code) in &files {
            for other in ["control", "observe"] {
                for sym in refs_to_layer(code, other) {
                    bad.push(format!("{name} → {sym}"));
                }
            }
        }
        assert!(
            bad.is_empty(),
            "plugin/ 反过来引用了 control/ 或 observe/：\n  {}\n\
             **先别急着加例外** —— 通用调用口需要的每一样东西都该由**调用方传进来**\n\
             （候选路径、期限秒数、环境变量、必需能力清单，今天全是入参）。\n\
             先问：跨过来的那个东西，是不是其实是**某一个插件的语义**放错了地方？",
            bad.join("\n  ")
        );
    }

    /// ★★ `KY5` 正题之二：**进 `plugin/` 的边逐条登记、条数钉死**。
    ///
    /// 形状照 [`ALLOWED_OBSERVE_TO_CONTROL`]：不是禁绝（调用口本来就是给人用的），
    /// 是**让每一条边被人看见一次**。
    ///
    /// ⚠ 扫的是 **`control/` 与 `observe/` 两层**，不只是 control ——
    /// 「今天只有 control 在用」是**读数**，不是性质。观测层哪天伸手过来（它一旦这么做，
    /// 就等于在只读层起进程），这条会红并逼人先把那条边写进表里。
    #[test]
    fn the_interface_into_plugin_is_exactly_the_registered_set() {
        let mut found: Vec<(String, String)> = Vec::new();
        for layer in ["control", "observe"] {
            let files = layer_sources(layer);
            assert_collection_is_complete(layer, &files);
            for (_, code) in &files {
                for sym in refs_to_layer(code, "plugin") {
                    let e = (layer.to_string(), sym);
                    if !found.contains(&e) {
                        found.push(e);
                    }
                }
            }
        }
        found.sort();
        let mut want: Vec<(String, String)> = ALLOWED_INTO_PLUGIN
            .iter()
            .map(|(l, s, _)| (l.to_string(), s.to_string()))
            .collect();
        want.sort();
        // 同 `S1`（Phase D 审计）那条纪律：**登记项必须钉到符号级**，模块级等于把整层放开。
        for (_, e, why) in ALLOWED_INTO_PLUGIN {
            let tail = e
                .strip_prefix("crate::plugin::")
                .unwrap_or_else(|| panic!("登记项必须以 `crate::plugin::` 开头：{e}"));
            assert!(
                tail.contains("::"),
                "登记项 `{e}` 只钉到**模块级** —— 那等于把整个模块的接口面都放开，\
                 而计数看不出区别。必须钉到符号：`crate::plugin::<模块>::<符号>`。"
            );
            assert!(
                why.trim().len() >= 20,
                "登记项 `{e}` 没写清「为什么这条边是调用口的**形状**而不是某个插件的**语义**」"
            );
        }
        assert_eq!(
            found, want,
            "进 plugin/ 的接口面与登记表对不上。\n\
             **多出来的**：加进 `ALLOWED_INTO_PLUGIN` 之前先回答「这个符号是调用口的形状，\
             还是某个插件的语义」——后者该留在调用方那一侧（`E6`）。\n\
             **少了的**：那条边没了就把登记摘掉（登记表腐烂比没有登记更糟 —— \
             `readonly_guard::ALLOWED` 里那条漏了 `cc-kill` 13 天就是活标本）。\n\
             ⚠ 若「多出来的」那条来自 `observe`：那等于**只读层开始起进程**，先回定框。"
        );
    }

    /// ★ `KY5` 的反向自检：**新方向的扫描真的会抓人**。
    ///
    /// 没有这一格，上面两条就是空真 —— 本仓「负向断言没有输入就等于没有」已经踩过五次
    ///（`readonly_guard:416` 的注释逐字记着「本区第五次」）。
    /// 三种模块级拼法 + 成组导入**逐个喂**，一种都不许漏：`refs_to_layer` 的头注逐字写着
    /// 「判据必须同时认三种拼法 —— **少认一种就是安慰剂**」。
    #[test]
    fn the_plugin_layer_scan_actually_bites() {
        assert_eq!(
            refs_to_layer("let x = crate::plugin::invoke::run(&b, &[], 1, &[]);", "plugin"),
            vec!["crate::plugin::invoke::run"]
        );
        assert_eq!(
            refs_to_layer("super::super::plugin::discover::find(a);", "plugin"),
            vec!["crate::plugin::discover::find"],
            "`super::super::` 那种拼法没认出来"
        );
        for form in [
            "use crate::plugin;",
            "use crate::plugin as pg;",
            "use super::super::plugin;",
        ] {
            assert!(
                refs_to_layer(form, "plugin")
                    .iter()
                    .any(|h| h.contains("模块级引入")),
                "`{form}` 没被判成模块级引入 —— 引进来之后用法都是裸 `plugin::…`，扫不到"
            );
        }
        assert!(
            refs_to_layer("use crate::{plugin, wire};", "plugin")
                .iter()
                .any(|h| h.contains("成组导入")),
            "层名藏在花括号里没被抓到"
        );
        // 反向：**只是前缀相同**的名字不许误命中 —— 误伤会训练人绕过判据。
        assert!(refs_to_layer("use crate::plugins_registry;", "plugin").is_empty());
        assert!(refs_to_layer("use crate::{common::plugin_helpers};", "plugin").is_empty());
        assert!(refs_to_layer("crate::control::gate::x();", "plugin").is_empty());
        // 登记表不许空：空表 + 零引用 = 上面那条恒绿（空真）。
        assert!(
            !ALLOWED_INTO_PLUGIN.is_empty(),
            "登记表空了 —— 那条等号断言会变成「空 == 空」，恒绿"
        );
    }

    // ══════════════════════════════════════════════════════════════════════
    // ★★★ `K-G4`（09-02）：`relay/` 那一层的三条方向判据
    //
    // 摸底读数（`K-G4` 派工时 PM 复打、C 实现拍开工前自己重打）：
    // `grep -c relay layering_guard.rs` = **0** ⇒ `K-H1` 立起来的 `relay/`，
    // 「谁能引谁」三个方向**一条判据都没有**。这与 `KY5` 治过的是同一族病
    //（新立一层、分层护栏没跟上），**同区第二次**。
    //
    // ⚠ 分母先说清：`relay/` 今天是 **12 个 `.rs`**（09-02 现打，`ls -1 src/relay/*.rs | wc -l`）。
    // 件文件 `§0` 写的「8 个文件」量于 **08-26**，此后长了 4 个 ⇒ **那个数已经馊了，别沿用**。
    //
    // ⚠ **别与 `relay/bind_guard` · `relay/nodelay_guard` 混起来**（件文件 `§0a`）：
    // 那两条护的是**中转自己的行为**（绑哪个口、开不开 Nagle），
    // 与「**层与层之间谁能引谁**」不是一回事。拿它们答「已经有护栏了」，
    // 正是本区「量具的作用域对不上事实」那一族。
    // ══════════════════════════════════════════════════════════════════════

    /// `relay/` **不许**认识的那几层，逐条给理由。
    ///
    /// 判据 ① 的人群就是这张表。**它刻意不含 `plugin`** —— 那一支归判据 ③，
    /// 两支的人群不许重叠：重叠了就没有「**只由这一支挡住**」的探针，
    /// 而〔定框 `K22`〕要的正是 N 支独立信号配 N 个单断探针。
    const RELAY_MUST_NOT_KNOW: &[(&str, &str)] = &[
        (
            "observe",
            "中转是**搬字节**的，它不读世界。观测面一旦被它认识，\
             「一个进程服务 N 个会话」那条就会退化成「中转顺手替某个会话查点东西」",
        ),
        (
            "control",
            "中转**不改变世界**（除了把字节递过去）。认识控制面等于给它开一条\
             「转发的路上顺手 kill / launch 一下」的门，而那条门在 HTTP 处理线程上",
        ),
        (
            "agents",
            "`relay/mod.rs` 头注第一句逐字写着「**它不懂任何 agent 的语义**」。\
             今天这条是真的、而且**承重**：`run(home, args)` 的 `home` 是**入参**\
             （`main.rs` 的 `--relay` 分派臂传进来），不是中转自己去 `agents/` 里问出来的 —— \
             那正是 `E6`「本层要的东西一律走入参」的形状",
        ),
    ];

    /// 方向 ② 的**人群**：这两层不许伸手进 `relay/` 内部。
    ///
    /// `plugin/` 刻意不在这里 —— 它归方向 ③（人群不重叠，理由同 [`RELAY_MUST_NOT_KNOW`]）。
    const WHO_MAY_NOT_REACH_INTO_RELAY: &[&str] = &["observe", "control"];

    /// 方向 ② 扫的**目标层**。
    ///
    /// 🔴 这三个 `D*_` 常量**不是为了少打几个字** —— 它们是「判据与活体夹具共用同一份
    /// 权威源」的落点〔`E3`：一个事实恰好一个权威源〕。夹具若各写一份字面量，
    /// 那把某条方向的被禁层改坏，**夹具照样全绿** —— 它证明的是自己那份字面量。
    /// 现在改坏任何一条，对应探针**当场红**（变异表逐刀在件文件里）。
    const D2_REACHING_INTO_RELAY: &[&str] = &["relay"];

    /// 方向 ③ 的正向那一半：`relay/` 不许引 `plugin/`。
    const D3_RELAY_TO_PLUGIN: &[&str] = &["plugin"];

    /// 方向 ③ 的反向那一半：`plugin/` 不许引 `relay/`。
    const D3_PLUGIN_TO_RELAY: &[&str] = &["relay"];

    /// 方向 ① 的被禁层名（从 [`RELAY_MUST_NOT_KNOW`] 派生，理由栏不参与判定）。
    fn d1_relay_must_not_know() -> Vec<&'static str> {
        RELAY_MUST_NOT_KNOW.iter().map(|(l, _)| *l).collect()
    }

    /// 三条 `relay/` 方向判据**共用的核**：给一份 `(名字, 生产段)` 表与一组被禁层，
    /// 数出所有违规边。
    ///
    /// # 为什么要抽出来 —— 它是活体夹具的落点
    ///
    /// `KG43` 摸底读数是 **0**（三个方向全 0，见下面那条自检的头注）⇒ 三条判据今天
    /// 全是 **`[] == []`** 的空真。〔`brief` 第 9 条〕治空真只能靠**活体夹具**，
    /// 而活体夹具必须跑**真判据本身**：夹具若另写一份扫描，它证明的是那一份、不是护栏。
    /// ⇒ 真判据与夹具都只经由这一个函数，中间没有第二份实现。
    fn violating_edges(files: &[(String, String)], forbidden: &[&str]) -> Vec<String> {
        let mut bad: Vec<String> = Vec::new();
        for (name, code) in files {
            for other in forbidden {
                for sym in refs_to_layer(code, other) {
                    bad.push(format!("{name} → {sym}"));
                }
            }
        }
        bad.sort();
        bad
    }

    /// ★★★ `K-G4` 方向 ①：**`relay/` 不许引 `observe/` · `control/` · `agents/`**。
    ///
    /// 零容忍，不设登记表。**为什么这一支不照 `ALLOWED_INTO_PLUGIN` 那种「逐条登记」的形状**：
    /// 登记表的价值在于「有正当例外、让每条边被人看见一次」，而今天这三条方向
    /// **一条边都没有**（现打 0）⇒ 建出来就是一张空表，而本文件自己在
    /// [`the_plugin_layer_scan_actually_bites`] 里逐字写着「登记表空了 ⇒ 那条等号断言会变成
    /// 「空 == 空」，恒绿」。空表比没表更糟：它长得像有护栏。
    /// ⇒ 先零容忍；真出现有理由的第一条边时，那一刻再把表建起来（那时它非空）。
    ///
    /// # ⚠ 诚实边界：`relay/` 的**测试段**确实引了 `agents/`，那不算违规
    ///
    /// `relay/server.rs::relay_child_process_entry_point`（子进程入口，`#[ignore]`）
    /// 走 `crate::agents::claudecode::paths::resolve_home()`。本判据扫的是
    /// `production_code`（测试段被剥掉）⇒ 看不见它，**这是有意的**：
    /// 分层是**生产架构**的性质，测试跨层构造夹具是正常的（`refs_to_layer` 头注同款取舍）。
    /// 写在这里免得下一个人 `grep` 到那一行、以为本判据坏了。
    #[test]
    fn relay_layer_must_not_reference_the_semantic_layers() {
        let files = layer_sources("relay");
        assert_collection_is_complete("relay", &files);
        let bad = violating_edges(&files, &d1_relay_must_not_know());
        assert!(
            bad.is_empty(),
            "relay/ 引用了它不该认识的层：\n  {}\n\
             **先别急着加例外** —— 中转要的每一样东西都该由**调用方传进来**\n\
             （`home` 今天就是这么来的：`main.rs` 的 `--relay` 臂把它当参数递进 `relay::run`）。\n\
             先问：跨过来的那个东西，是不是其实该走入参？理由逐条见 `RELAY_MUST_NOT_KNOW`。",
            bad.join("\n  ")
        );
    }

    /// ★★★ `K-G4` 方向 ②：**别处不许反过来伸手进 `relay/` 内部**。
    ///
    /// 人群是 `observe/` 与 `control/` 两层（`plugin/` 归判据 ③，人群不重叠）。
    /// 中转对外**只有一个口**：`relay/mod.rs` 里那一行 `pub(crate) use server::run;`。
    /// 谁绕过它去引 `crate::relay::table` / `crate::relay::upstream`，
    /// 中转的内部结构就变成了公共契约 —— 之后 `table.rs` 想换个形状都得先问一圈。
    ///
    /// # ⚠ 这一支够不到哪儿（如实登记，别读成「全体没有」）
    ///
    /// 它的人群是**两个层目录下的 18 个 `.rs`**（09-02 现打：`observe/` 8 + `control/` 10）。
    /// `src/` 顶层那几个文件（`main.rs` · `listen.rs` · `wire.rs` …）**不在人群里**，
    /// 而且就算放进来也扫不到：`mod relay;` 声明在 `main.rs`，它写的是**裸** `relay::run`，
    /// 而 `refs_to_layer` 的锚点是 `crate::relay` / `super::super::relay`。
    /// ⇒ 顶层文件伸手进 `relay::table::…` 这一形，**本判据看不见**。留作跟进件，不在本件买。
    #[test]
    fn no_layer_may_reach_into_relay_internals() {
        let mut bad: Vec<String> = Vec::new();
        for layer in WHO_MAY_NOT_REACH_INTO_RELAY {
            let files = layer_sources(layer);
            assert_collection_is_complete(layer, &files);
            bad.extend(violating_edges(&files, D2_REACHING_INTO_RELAY));
        }
        bad.sort();
        assert!(
            bad.is_empty(),
            "有人伸手进了 relay/ 内部（对外只有 `relay::run` 一个口）：\n  {}\n\
             **先别急着加例外** —— 先问那个东西是不是根本不属于中转：\n\
             `U3` 摸底时那条反向边的正解就是「被引的那个函数放错了地方」，搬进 `common/` 之后边就没了。\n\
             真要新开一个口，那个口该住 `relay/mod.rs` 的 `pub(crate) use`，并在这里配一张非空登记表。",
            bad.join("\n  ")
        );
    }

    /// ★★★ `K-G4` 方向 ③：**`relay/` 与 `plugin/` 互不认识**（两个方向都零容忍）。
    ///
    /// 这两层是**两条互不相干的基础设施**：一条搬 HTTP 字节，一条按 argv 起外部程序。
    /// 谁先认识谁都会把对方的知识继承过来 —— 中转认识调用口，就等于在 HTTP 处理线程上
    /// 长出一条起进程的路（而全 crate 唯一一处起进程口是
    /// `crate::plugin::invoke::run`，它在 `readonly_guard::ALLOWED` 里单独登记着）；
    /// 调用口认识中转，它就不再是「谁都能用的口」。
    ///
    /// ⚠ 本支**刻意与判据 ① 的人群不重叠**（`RELAY_MUST_NOT_KNOW` 里没有 `plugin`）：
    /// 重叠了就造不出「只由这一支挡住」的探针〔`K22`〕。
    #[test]
    fn relay_and_plugin_must_not_reference_each_other() {
        let relay = layer_sources("relay");
        assert_collection_is_complete("relay", &relay);
        let plugin = layer_sources("plugin");
        assert_collection_is_complete("plugin", &plugin);
        let mut bad = violating_edges(&relay, D3_RELAY_TO_PLUGIN);
        bad.extend(violating_edges(&plugin, D3_PLUGIN_TO_RELAY));
        bad.sort();
        assert!(
            bad.is_empty(),
            "relay/ 与 plugin/ 互相认识了（两条互不相干的基础设施）：\n  {}\n\
             **先别急着加例外** —— 两边要的东西都该由**调用方传进来**（`E6`）。\n\
             ⚠ 若这条边是 `relay → plugin::invoke`：那等于在 HTTP 处理线程上开了一条起进程的路，\n\
             先回定框，别在这里加例外。",
            bad.join("\n  ")
        );
    }

    /// ★★★ `K-G4` `KG43`：**上面三条今天全是空真 ⇒ 用活体夹具证明它们真会红**。
    ///
    /// # 先报摸底读数（`brief` 第 12 条：分母怎么数的一起写）
    ///
    /// 09-02 现打，量具住 `evidence/K-G4-C-relay-layer-census.py`（被测对象写死指向
    /// `worktrees/k-g4/remote-daemon-proto/src`）：
    ///
    /// | 方向 | 违规处数 | 分母 |
    /// |---|---|---|
    /// | ① `relay/` → `observe`·`control`·`agents` | **0** | `relay/` 的 **12** 个 `.rs` 的生产段 |
    /// | ② `observe`·`control` → `relay/` | **0** | 两层合计 **18** 个 `.rs` 的生产段 |
    /// | ③ `relay/` ↔ `plugin/` | **0** | 两层合计 **16** 个 `.rs` 的生产段 |
    ///
    /// 三个方向全 0 ⇒ 三条断言都是 **`[] == []`**，**闸死了照样绿**。
    /// 那份量具自己带**非空对照**（同一把尺子量 `control → plugin`，
    /// 现打 **5** 条边，与 `ALLOWED_INTO_PLUGIN` 的 5 条逐条对上）——
    /// 「差集为空」与「命令没跑」在终端上一模一样，必须有非空的那一格〔`brief` 12·14w②〕。
    ///
    /// # 夹具为什么造在**盘上**，而不是喂字符串
    ///
    /// 本文件已有的两条 `*_actually_bites` 喂的是字符串，它们证明的是
    /// [`refs_to_layer`] **这一个函数**会咬人 —— 那是**扫描器**的牙，不是**判据**的牙。
    /// 判据还有「走目录 → 剥生产段 → 采集面自检 → 汇总 → 断言」四段，
    /// 喂字符串一段都没盖到。⇒ 这里造真目录、真 `.rs` 文件，
    /// 让 [`layer_sources_at`] · [`assert_collection_is_complete_at`] · [`violating_edges`]
    /// **原封不动**跑一遍。
    ///
    /// # 四个探针，**每个只由一支挡住**〔定框 `K22`〕
    ///
    /// 拆掉任一支必有东西红，且红的只有那一支：
    ///
    /// | 探针 | ① | ② | ③ |
    /// |---|---|---|---|
    /// | relay 形状的文件引 `control` | 🔴 | 绿 | 绿 |
    /// | control 形状的文件引 `relay` | 绿 | 🔴 | 绿 |
    /// | relay 形状的文件引 `plugin` | 绿 | 绿 | 🔴 |
    /// | plugin 形状的文件引 `relay` | 绿 | 绿 | 🔴 |
    ///
    /// ⚠ 夹具的目录名 / 文件名一律**中性**，且下面每一条断言都只认**符号**
    ///（`crate::control::gate` 这一类，来自文件**内容**），不认路径 ——
    /// 〔`6g`〕断言取自夹具的名字会靠路径恒真。
    #[test]
    fn the_relay_direction_judgments_actually_bite_on_a_live_tree() {
        // 每棵树都放**两个** `.rs`：一个违规、一个干净。两个的理由有两条 ——
        // ① `assert_collection_is_complete_at` 的地板是 `tree >= 2`；
        // ② 顺带证明遍历真的走到了第二个文件，而不是撞见第一个就返回。
        let clean = "pub fn ok() -> usize { crate::common::fs::len() }\n";

        // 探针一：relay 形状的文件引 control。
        let t1 = write_probe_tree("a", "use crate::control::gate;\npub fn x() {}\n", clean);
        let f1 = layer_sources_at(&t1, "x");
        assert_collection_is_complete_at(&t1, "x", &f1);
        let forbidden1 = d1_relay_must_not_know();
        let hit1 = violating_edges(&f1, &forbidden1);
        assert!(
            hit1.iter().any(|h| h.contains("crate::control")),
            "判据 ① 对一条真的 relay→control 边没出声 —— 它此刻是空转的。实得：{hit1:?}"
        );
        // 单断：这条边只归 ①，②③ 的人群/被禁层都不该认它。
        assert!(
            violating_edges(&f1, D2_REACHING_INTO_RELAY).is_empty(),
            "② 认领了本该只归 ① 的那条边 —— 两支的信号串了，探针不再是单断的"
        );
        assert!(
            violating_edges(&f1, D3_RELAY_TO_PLUGIN).is_empty(),
            "③ 认领了本该只归 ① 的那条边 —— 两支的信号串了，探针不再是单断的"
        );

        // 探针二：control 形状的文件伸手进 relay 内部。
        let t2 = write_probe_tree("b", "pub fn y() { crate::relay::table::look(); }\n", clean);
        let f2 = layer_sources_at(&t2, "x");
        assert_collection_is_complete_at(&t2, "x", &f2);
        let hit2 = violating_edges(&f2, D2_REACHING_INTO_RELAY);
        assert!(
            hit2.iter().any(|h| h.contains("crate::relay::table")),
            "判据 ② 对一条真的 →relay 内部边没出声 —— 它此刻是空转的。实得：{hit2:?}"
        );
        assert!(
            violating_edges(&f2, &forbidden1).is_empty(),
            "① 认领了本该只归 ② 的那条边 —— 信号串了"
        );
        assert!(
            violating_edges(&f2, D3_RELAY_TO_PLUGIN).is_empty(),
            "③ 认领了本该只归 ② 的那条边 —— 信号串了"
        );

        // 探针三：relay 形状的文件引 plugin（③ 的正向那一半）。
        let t3 = write_probe_tree(
            "c",
            "pub fn z() { crate::plugin::invoke::run(&b, &[], 1, &[]); }\n",
            clean,
        );
        let f3 = layer_sources_at(&t3, "x");
        assert_collection_is_complete_at(&t3, "x", &f3);
        let hit3 = violating_edges(&f3, D3_RELAY_TO_PLUGIN);
        assert!(
            hit3.iter().any(|h| h.contains("crate::plugin::invoke")),
            "判据 ③ 对一条真的 relay→plugin 边没出声 —— 它此刻是空转的。实得：{hit3:?}"
        );
        assert!(
            violating_edges(&f3, &forbidden1).is_empty(),
            "① 认领了本该只归 ③ 的那条边 —— `RELAY_MUST_NOT_KNOW` 里混进了 `plugin`，\
             那样 ③ 就再也没有单断探针了〔`K22`〕"
        );
        assert!(
            violating_edges(&f3, D2_REACHING_INTO_RELAY).is_empty(),
            "② 认领了本该只归 ③ 的那条边 —— 信号串了"
        );

        // 探针四：plugin 形状的文件反过来引 relay（③ 的另一半 —— 少了这个，
        // ③ 就只买到了单向，而它的名字承诺的是「互不认识」）。
        let t4 = write_probe_tree("d", "use crate::relay as rl;\npub fn w() {}\n", clean);
        let f4 = layer_sources_at(&t4, "x");
        assert_collection_is_complete_at(&t4, "x", &f4);
        let hit4 = violating_edges(&f4, D3_PLUGIN_TO_RELAY);
        assert!(
            hit4.iter().any(|h| h.contains("模块级引入")),
            "③ 的反向那一半对层别名没出声 —— 引进来之后用法全是裸 `relay::…`，扫不到。实得：{hit4:?}"
        );
        assert!(
            violating_edges(&f4, &forbidden1).is_empty(),
            "① 认领了本该只归 ③ 的那条边 —— 信号串了"
        );

        // ② 的**人群**本身也要有牙 —— 上面四个探针盖的是「扫到的东西判得对不对」，
        // 一个都盖不到「**扫了谁**」。人群被缩空或改成不存在的层名，判据会静默变成空转，
        // 而它的输出与「跑了、没违规」在终端上一模一样〔`brief` 12·14w②〕。
        assert!(
            WHO_MAY_NOT_REACH_INTO_RELAY.len() >= 2,
            "② 的人群缩到了 {} 层 —— 少一层就是少一整面没人看着",
            WHO_MAY_NOT_REACH_INTO_RELAY.len()
        );
        for layer in WHO_MAY_NOT_REACH_INTO_RELAY {
            // 层名写错 ⇒ `layer_sources` 在 `read_dir` 上直接 panic，同样是红。
            let fs = layer_sources(layer);
            assert!(
                fs.len() >= 2,
                "② 的人群里 `{layer}` 只采到 {} 个 `.rs` —— 那一层此刻没人扫",
                fs.len()
            );
        }

        // 采集面自检本身也要有牙：树上有 2 个 `.rs`，采集表里塞回 1 个 ⇒ 必须红。
        // 没有这一格，`assert_collection_is_complete_at` 就是本护栏里唯一没人验过的那段。
        let shrunk = vec![f1[0].clone()];
        let r = std::panic::catch_unwind(|| assert_collection_is_complete_at(&t1, "x", &shrunk));
        assert!(
            r.is_err(),
            "采集面自检对「采集漏了一个文件」没出声 —— 那意味着上面三条判据可以被\
             「悄悄少扫几个文件」整个绕开"
        );

        for t in [t1, t2, t3, t4] {
            let _ = std::fs::remove_dir_all(&t);
        }
    }

    /// 给 [`the_relay_direction_judgments_actually_bite_on_a_live_tree`] 造一棵**真**小树。
    ///
    /// `tag` 只用来把四棵树的目录名岔开（配 pid 防并行撞车），**一律取中性名**，
    /// 且**不许**出现在任何断言里〔`6g`：断言取自夹具名字会靠路径恒真〕。
    fn write_probe_tree(tag: &str, dirty: &str, clean: &str) -> std::path::PathBuf {
        let root = std::env::temp_dir().join(format!("ccm-lg-{}-{}", tag, std::process::id()));
        // 先清一次：上一趟留下的文件会让「树上有几个 .rs」这个分母漂。
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("造夹具目录");
        std::fs::write(root.join("one.rs"), dirty).expect("写夹具文件");
        std::fs::write(root.join("two.rs"), clean).expect("写夹具文件");
        root
    }

    /// 反向自检：判据真的会抓人（喂字符串，不改真文件）。
    #[test]
    fn the_layer_scan_actually_bites() {
        assert_eq!(
            refs_to_layer("let x = crate::observe::watcher::foo();", "observe"),
            vec!["crate::observe::watcher::foo"]
        );
        // 同一个符号出现多次只记一次。
        assert_eq!(
            refs_to_layer("crate::control::a::b; crate::control::a::b;", "control"),
            vec!["crate::control::a::b"]
        );
        // 不该误命中别的层。
        assert!(refs_to_layer("crate::common::fs::read", "observe").is_empty());
        assert!(refs_to_layer("crate::platform::proc::x", "control").is_empty());
        // 〔audit-0805 08-06〕**模块级引入的三种写法都要认**（上面第二张表）。
        for form in [
            "use crate::observe;",
            "use crate::observe as ob;",
            "use super::super::observe;",
        ] {
            assert!(
                refs_to_layer(form, "observe")
                    .iter()
                    .any(|h| h.contains("模块级引入")),
                "`{form}` 没被判成模块级引入 —— 引进来之后用法都是裸 `observe::…`，扫不到"
            );
        }
        // 成组导入：层名在花括号里。
        assert!(
            refs_to_layer("use crate::{observe, wire};", "observe")
                .iter()
                .any(|h| h.contains("成组导入")),
            "`use crate::{{observe, wire}};` 没被抓到 —— 层名藏在组里，锚点对不上"
        );
        // 反向：名字**只是前缀相同**的模块不许误命中（否则新判据会把好代码判红）。
        assert!(refs_to_layer("use crate::observe_helpers;", "observe").is_empty());
        assert!(refs_to_layer("use crate::{common::observe_helpers};", "observe").is_empty());
    }
}
