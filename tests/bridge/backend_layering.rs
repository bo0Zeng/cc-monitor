use std::path::{Path, PathBuf};

/// 允许的 `observe → control` 跨线引用，**逐条列举**。`(符号, 为什么这条边非有不可)`。
///
/// # 加一条之前先回答
///
/// 为什么这件事非得由读面发起？能不能反过来由控制面主动做？
/// —— 今天这三条的答案是同一个：读面要起本机后端做一次性查询，
/// 而「那个 sidecar 装在哪、算不算找得到」是**控制面立起来的事实**
/// （起它、看住它、判它崩得太频繁的那半都住 `control/local_backend.rs`）。
/// 读面自己再解析一遍路径 = 第二份路径解析，而两份必漂。
///
/// # 为什么正向要**钉条数**而不是「随便引」
///
/// 下面那条判据是 `assert_eq!(found, want)`：多一条、少一条都红。
/// **「有正当例外」与「这条线随便穿」是两回事**，中间隔着的就是这个等号。
/// 一条没人数的合法边会长成一张网 —— daemon 那份头注逐字记着这句话。
const ALLOWED_OBSERVE_TO_CONTROL: &[(&str, &str)] = &[
    (
        "crate::backend::control::local_backend::resolve_beside_this_exe",
        "读面起本机后端拿 stdout 之前，先要知道那份 sidecar 在哪。\
             「装在哪、找过哪儿、算不算找到」是控制面立的事实（起进程与看住它的那半住在那里），\
             读面自己再解析一份路径就是第二个权威源",
    ),
    (
        "crate::backend::control::local_backend::Resolved::Found",
        "上面那次解析的**返回类型**的变体。⚠ 类型也要登记，不只是函数 —— \
             它出现在读面的 `match` 里，和函数一样是接口面；漏登记等于「接口只算函数」",
    ),
    (
        "crate::backend::control::local_backend::Resolved::Missing",
        "同一个返回类型的另一支，带着 `reason` 与 `looked_at`。\
             读面把「找过哪儿」原样转给调用方，不自己改写 —— 那句话是控制面产的",
    ),
];

fn backend_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("src/backend")
}

/// 收集某一条能力线下所有 `.rs` 的 `(相对路径, 生产段)`。
fn layer_sources(layer: &str) -> Vec<(String, String)> {
    layer_sources_at(&backend_dir().join(layer), layer)
}

/// [`layer_sources`] 的**根可注入**版本。
///
/// 抽出这一层只为一件事：下面那条活体夹具要让**真判据本身**（不是它的复刻）
/// 跑在一棵真的、盘上存在的小树上 —— 夹具若另写一份扫描，它证明的是那一份、不是护栏。
/// `label` 只进报错文本里的路径前缀，**不进任何断言**。
fn layer_sources_at(root: &Path, label: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).expect("读能力线目录") {
            let path = entry.expect("目录项").path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            if path.extension().and_then(|e| e.to_str()) != Some("rs") {
                continue;
            }
            let rel = path
                .strip_prefix(root)
                .unwrap_or(&path)
                .to_string_lossy()
                .replace('\\', "/");
            let src = std::fs::read_to_string(&path).expect("读 .rs");
            out.push((format!("{label}/{rel}"), guard_core::production_code(&src)));
        }
    }
    out.sort();
    out
}

/// 采集面自检 —— 照 daemon 那份的做法：**数量相等**，不是「至少几个字节」。
/// 它对「文件增删」免疫（那是正常演进），只对「采集漏了」敏感 —— 后者才是要防的。
fn assert_collection_is_complete(layer: &str, files: &[(String, String)]) {
    assert_collection_is_complete_at(&backend_dir().join(layer), layer, files);
}

/// [`assert_collection_is_complete`] 的根可注入版本（理由同 [`layer_sources_at`]）。
fn assert_collection_is_complete_at(root: &Path, layer: &str, files: &[(String, String)]) {
    let mut tree = 0usize;
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).expect("读能力线目录") {
            let path = entry.expect("目录项").path();
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
             下面的方向判据对漏掉的那些是瞎的。",
        files.len()
    );
    assert!(
        tree >= 2,
        "{layer}/ 只有 {tree} 个 .rs —— 这条能力线是不是已经名存实亡了？"
    );
    // 剥法自检：剥完不许残留测试属性。挡的是「剥少了 ⇒ 护栏开始扫测试代码 ⇒
    // 被夹具打红 ⇒ 有人顺手放宽护栏」。
    for (name, prod) in files {
        guard_core::assert_no_test_code(&format!("{layer}/{name}"), prod);
    }
}

/// 抽出生产段里所有指向 `layer` 这条能力线的引用（去重、排序）。
///
/// # 三个锚点根，一种都不许少
///
/// daemon 那份的头注用两张表记着它被证伪过两轮：只认 `crate::<层>::` 时，
/// 层别名（`use crate::x as y;`）、`super::super::` 那种拼法、**最朴素的
/// `use crate::x;` 加裸调用**、以及成组导入 `use crate::{x, y};` 各自都能全绿过去。
/// ⇒ 这里同样**从层名派生**、不列拼法清单：每一处锚点按**紧随其后的字符**分类
/// （后面是 `::` ⇒ 符号路径；否则 ⇒ 模块级引入，引进来之后用法都是裸 `层::…`、
/// 本护栏再也看不见，与层别名同罪），成组导入单独走一路。
///
/// # 它仍然挡不住什么（如实登记，别当成完备）
///
/// - **测试段不受管**（下面走 `production_code` 剥掉）。这是**有意**的：
///   分层是生产架构的性质，测试跨线构造夹具是正常的。
/// - 更曲折的间接（先 `pub use` 到第三个模块再引）扫不到 —— 那要上 `syn` 级解析。
/// - **`backend/` 之外的文件不在人群里**：`local_accounts.rs` 等今天
///   在调 `backend::observe::local_query`，它们既不住 `control/` 也不住 `observe/`，
///   本护栏一个字都看不见它们。那是「前端那一半怎么用后端」的问题，另有其人。
fn refs_to_layer(code: &str, layer: &str) -> Vec<String> {
    let mut hits: Vec<String> = Vec::new();
    for root in ["crate::backend::", "super::super::", "super::"] {
        let anchor = format!("{root}{layer}");
        let mut from = 0usize;
        while let Some(rel) = code[from..].find(&anchor) {
            let i = from + rel;
            from = i + anchor.len();
            let tail = &code[from..];
            // `crate::backend::controller` 之类只是前缀相同，不是本层。
            if tail.starts_with(|c: char| c.is_ascii_alphanumeric() || c == '_') {
                continue;
            }
            if let Some(rest) = tail.strip_prefix("::") {
                let end = rest
                    .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_' || c == ':'))
                    .unwrap_or(rest.len());
                let mut sym = format!("crate::backend::{layer}::{}", &rest[..end]);
                while sym.ends_with(':') {
                    sym.pop();
                }
                if !hits.contains(&sym) {
                    hits.push(sym);
                }
            } else {
                // ⚠ 标记**刻意不带 root** —— 带上的话同一处 `use super::super::control;`
                // 会被 `super::super::` 与 `super::` 两个根各记一条，条数凭空翻倍。
                let mark = module_level_mark(layer);
                if !hits.contains(&mark) {
                    hits.push(mark);
                }
            }
        }
    }
    // 成组导入 `use crate::backend::{observe, control};` —— 层名被包进花括号，
    // 上面的锚点一个都对不上。
    for prefix in [
        "use crate::backend::{",
        "use super::super::{",
        "use super::{",
    ] {
        let mut from = 0usize;
        while let Some(rel) = code[from..].find(prefix) {
            let i = from + rel;
            from = i + prefix.len();
            let stmt = &code[i..];
            let end = stmt.find(';').map(|e| e + 1).unwrap_or(stmt.len());
            if group_names_layer(&stmt[..end], layer) {
                let mark = grouped_import_mark(layer);
                if !hits.contains(&mark) {
                    hits.push(mark);
                }
            }
        }
    }
    hits.sort();
    hits
}

fn module_level_mark(layer: &str) -> String {
    format!(
        "{layer}（**模块级引入**：引进来之后用法都是裸 `{layer}::…`，\
             本护栏再也看不见 ⇒ 与层别名同性质，一样禁）"
    )
}

fn grouped_import_mark(layer: &str) -> String {
    format!("{layer}（**成组导入**：层名藏在花括号里 ⇒ 按线拆成一行一个）")
}

/// 成组导入里，`layer` 是不是**顶层的一个组员**（`{observe, wire}` 里的 `observe`）。
/// 只认前面紧挨着 `{` / `,` / 空白的那种，免得 `{common::observe_helpers}` 误命中。
fn group_names_layer(group: &str, layer: &str) -> bool {
    let mut from = 0usize;
    while let Some(rel) = group[from..].find(layer) {
        let i = from + rel;
        from = i + layer.len();
        let before_ok = group[..i]
            .chars()
            .next_back()
            .is_some_and(|c| c == '{' || c == ',' || c.is_whitespace());
        let after_ok = !group[from..].starts_with(|c: char| c.is_ascii_alphanumeric() || c == '_');
        if before_ok && after_ok {
            return true;
        }
    }
    false
}

/// 给一份 `(名字, 生产段)` 表与一组被禁层，数出所有违规边。
///
/// 真判据与活体夹具**都只经由这一个函数**，中间没有第二份实现 ——
/// 夹具若另写一份扫描，它证明的是那一份、不是护栏。
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

/// ★ **反向零容忍**：`control/` 不许引用 `observe/`。
///
/// ⚠ 这一条今天是 **`[] == []`**（盘上零违规）⇒ 它单独看是空真的，
/// 真正证明它会咬人的是下面那条活体夹具。两条一起才算数。
#[test]
fn control_layer_must_not_reference_observe() {
    let files = layer_sources("control");
    assert_collection_is_complete("control", &files);
    let bad = violating_edges(&files, &["observe"]);
    assert!(
        bad.is_empty(),
        "control/ 引用了 observe/（反向不许）：\n  {}\n\
             **先别急着加例外** —— daemon 侧摸底时那条反向边的正解是\
             「被引的那个函数根本不属于观测面」，搬走之后边就没了。\n\
             先问：被引用的那个东西，是不是只是个放错地方的通用工具？\n\
             ⚠ 另一条常见正解见 `src/doc/ARCHITECTURE.md` 2.2：**只喂控制决策的只读查询一律归 \
             `control/`** —— 按「读/写」分层会凭空造出这种反向边。",
        bad.join("\n  ")
    );
}

/// ★ **正向要显式列举且条数钉死**：`observe/` 只许用登记过的那几个 control 符号。
///
/// 🔴 这一条**不是空真**：盘上今天真有 3 条这样的边（`K-R71` 归位时显形的那一处），
/// 登记表非空 ⇒ `assert_eq!` 两边都有内容。
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
        .map(|(s, _)| s.to_string())
        .collect();
    want.sort();
    // 登记表空了 ⇒ 下面那个等号会变成「空 == 空」，恒绿。
    assert!(
        !ALLOWED_OBSERVE_TO_CONTROL.is_empty(),
        "登记表空了 —— 那条等号断言会退化成「空 == 空」，恒绿"
    );
    // **登记项必须钉到符号级**：模块级登记（`crate::backend::control::local_backend`）
    // 等于把整个模块的接口面都放开，而条数看不出区别。
    for (e, why) in ALLOWED_OBSERVE_TO_CONTROL {
        let tail = e
            .strip_prefix("crate::backend::control::")
            .unwrap_or_else(|| panic!("登记项必须以 `crate::backend::control::` 开头：{e}"));
        assert!(
            tail.contains("::"),
            "登记项 `{e}` 只钉到**模块级** —— 那等于把整个模块的接口面都放开，\
                 而条数看不出区别。必须钉到符号：`crate::backend::control::<模块>::<符号>`。"
        );
        assert!(
            why.trim().chars().count() >= 20,
            "登记项 `{e}` 没写清「为什么这件事非得由读面发起、控制面能不能自己做」"
        );
    }
    assert_eq!(
        found, want,
        "observe → control 的接口面与登记表对不上。\n\
             **多出来的**：加进 `ALLOWED_OBSERVE_TO_CONTROL` 之前先回答\
             「为什么这件事非得由读面发起、控制面能不能自己做」。\n\
             **少了的**：那条边没了就把登记摘掉 —— 登记表腐烂比没有登记更糟。"
    );
}

/// ★ 反向自检 ①：**扫描器**真的会咬人（喂字符串，不碰真文件）。
///
/// ⚠ 它证明的是 [`refs_to_layer`] 这一个函数有牙，**不是判据有牙** ——
/// 判据还有「走目录 → 剥生产段 → 采集面自检 → 汇总 → 断言」四段，喂字符串一段都盖不到。
/// 那一半归下面那条活体夹具。
#[test]
fn the_backend_layer_scan_actually_bites() {
    assert_eq!(
        refs_to_layer(
            "let x = crate::backend::observe::local_query::run_query();",
            "observe"
        ),
        vec!["crate::backend::observe::local_query::run_query"]
    );
    // 同一个符号出现多次只记一次。
    assert_eq!(
        refs_to_layer(
            "crate::backend::control::a::b; crate::backend::control::a::b;",
            "control"
        ),
        vec!["crate::backend::control::a::b"]
    );
    // 层内某个文件看兄弟层。
    assert_eq!(
        refs_to_layer("super::super::control::local_backend::x();", "control"),
        vec!["crate::backend::control::local_backend::x"]
    );
    // 🔴 层自己的 `mod.rs` 看兄弟层 —— **daemon 那份样板认不出这一种**。
    assert_eq!(
        refs_to_layer(
            "pub fn f() { super::control::daemon_route::y(); }",
            "control"
        ),
        vec!["crate::backend::control::daemon_route::y"]
    );
    // 模块级引入的四种写法都要认。
    for form in [
        "use crate::backend::observe;",
        "use crate::backend::observe as ob;",
        "use super::super::observe;",
        "use super::observe;",
    ] {
        assert!(
            refs_to_layer(form, "observe")
                .iter()
                .any(|h| h.contains("模块级引入")),
            "`{form}` 没被判成模块级引入 —— 引进来之后用法都是裸 `observe::…`，扫不到"
        );
    }
    // 成组导入：层名在花括号里。
    for form in [
        "use crate::backend::{observe, control};",
        "use super::{observe, control};",
    ] {
        assert!(
            refs_to_layer(form, "observe")
                .iter()
                .any(|h| h.contains("成组导入")),
            "`{form}` 的层名藏在组里没被抓到"
        );
    }
    // 反向：**只是前缀相同**的名字不许误命中 —— 误伤会训练人绕过判据。
    assert!(refs_to_layer("use crate::backend::observe_helpers;", "observe").is_empty());
    assert!(refs_to_layer("use crate::backend::{common::observe_x};", "observe").is_empty());
    assert!(refs_to_layer("crate::backend::control::gate::x();", "observe").is_empty());
}

/// ★★ 反向自检 ②：**判据本身**在一棵真树上会咬人。
///
/// [`control_layer_must_not_reference_observe`] 今天是 `[] == []`（盘上零违规）——
/// **闸死了照样绿**。这里造真目录、真 `.rs`，让 [`layer_sources_at`] ·
/// [`assert_collection_is_complete_at`] · [`violating_edges`] **原封不动**跑一遍。
///
/// 夹具的目录名 / 文件名一律**中性**，下面每一条断言都只认**符号**（来自文件内容），
/// 不认路径 —— 断言取自夹具的名字会靠路径恒真。
#[test]
fn the_backend_direction_judgments_bite_on_a_live_tree() {
    // 每棵树放**两个** `.rs`：一个违规、一个干净。两个的理由有两条 ——
    // ① 采集面自检的地板是 `tree >= 2`；② 顺带证明遍历真的走到了第二个文件。
    let clean = "pub fn ok() -> usize { 0 }\n";

    // 探针一：control 形状的文件引 observe（反向那条边）。
    let t1 = write_probe_tree("a", "use crate::backend::observe;\npub fn x() {}\n", clean);
    let f1 = layer_sources_at(&t1, "x");
    assert_collection_is_complete_at(&t1, "x", &f1);
    let hit1 = violating_edges(&f1, &["observe"]);
    assert!(
        hit1.iter().any(|h| h.contains("模块级引入")),
        "反向零容忍那条对一条真的 control→observe 边没出声 —— 它此刻是空转的。实得：{hit1:?}"
    );
    assert!(
        violating_edges(&f1, &["control"]).is_empty(),
        "扫 control 时认领了一条 observe 的边 —— 两个方向的信号串了"
    );

    // 探针二：observe 形状的文件引一个**没登记**的 control 符号。
    let t2 = write_probe_tree(
        "b",
        "pub fn y() { super::super::control::daemon_route::look(); }\n",
        clean,
    );
    let f2 = layer_sources_at(&t2, "x");
    assert_collection_is_complete_at(&t2, "x", &f2);
    let mut found: Vec<String> = Vec::new();
    for (_, code) in &f2 {
        for sym in refs_to_layer(code, "control") {
            found.push(sym);
        }
    }
    assert!(
        found
            .iter()
            .any(|s| s == "crate::backend::control::daemon_route::look"),
        "正向登记那条采不到一条真的 observe→control 边 —— 它此刻在数一个空集。实得：{found:?}"
    );
    assert!(
        !ALLOWED_OBSERVE_TO_CONTROL
            .iter()
            .any(|(e, _)| found.iter().any(|s| s == e)),
        "夹具那条边居然在登记表里 —— 那这个探针证明不了「没登记的会被逮到」"
    );

    // 采集面自检本身也要有牙：树上有 2 个 `.rs`，采集表里塞回 1 个 ⇒ 必须红。
    // 没有这一格，采集面自检就是本护栏里唯一没人验过的那段。
    let shrunk = vec![f1[0].clone()];
    let r = std::panic::catch_unwind(|| assert_collection_is_complete_at(&t1, "x", &shrunk));
    assert!(
        r.is_err(),
        "采集面自检对「采集漏了一个文件」没出声 —— 那意味着上面两条判据可以被\
             「悄悄少扫几个文件」整个绕开"
    );

    for t in [t1, t2] {
        let _ = std::fs::remove_dir_all(&t);
    }
}

/// 给活体夹具造一棵**真**小树。
///
/// 🔴 **落系统临时目录，不落 `src/bridge/src/`** —— 落进源码树会让全树自检假红。
/// `tag` 只把两棵树的目录名岔开（配 pid 防并行撞车），**一律取中性名**，
/// 且不许出现在任何断言里。
fn write_probe_tree(tag: &str, dirty: &str, clean: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("ccm-blg-{}-{}", tag, std::process::id()));
    // 先清一次：上一趟留下的文件会让「树上有几个 .rs」这个分母漂。
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("造夹具目录");
    std::fs::write(root.join("one.rs"), dirty).expect("写夹具文件");
    std::fs::write(root.join("two.rs"), clean).expect("写夹具文件");
    root
}
