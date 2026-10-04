//! 各条源码扫描型守卫共用的「只留生产段」剥法。**整个模块只在 `cfg(test)` 下存在。**
//!
//! # 剥法本体已搬进共享 crate（U8a-2a）
//!
//! 实现在 [`guard_core`]（`src/common/guard-core`），本模块只是**再导出** +
//! 存放后端专属的那两条语义钉。搬家的理由：monitor 侧够不着后端的 `cfg(test)`
//! 模块，于是它的守卫各自写了便宜近似（`src.split("\n#[cfg(test)]").next()`）——
//! 那个近似在 `stream_source/` 这种「第一个测试模块在 804 行、要扫的代码在 1771 行」的文件上
//! **把扫描面砍掉三分之二**。剥法的来龙去脉（两个互相掩盖的坑、无花括号体 mod 声明那条）
//! 全部留在 `guard-core` 的模块头注里，别在这里再写一份。
//!
//! 调用点一行不用改：下面三条 `pub(crate) use` 让 `crate::guard_support::production_code`
//! 等路径原样可用。
//!
//! # ⚠ 它服务哪条要求：**没有，它是量具不是判据**
//!
//! 同 `guard_core` 那份头注的同名一节 —— 不在这里抄第二份。依据住。

pub(crate) use guard_core::{assert_no_test_code, production_code, production_source};

/// 本 crate **源码树根**的唯一住址。
///
/// # 为什么是一个函数而不是 45 处各写一行
///
/// 这条形状此前在 33 个文件里出现 **45 次**（`Path::new(env!("CARGO_MANIFEST_DIR"))` 接
/// `.join("src")`；这里刻意**不写成完整的可替换形**，理由见下面那条⚠）。
/// 那在「源码树就住在 `Cargo.toml` 旁边」时成立 —— 而仓库重组要把这棵树搬到
/// `<repo>/src/backend/`，`Cargo.toml` 留在原处 ⇒ 那 45 处**同时失效**。
///
/// 🔴 **失效的形态比「报错」坏得多**：`CARGO_MANIFEST_DIR/src` 搬走后是个**不存在的目录**，
/// 而扫描型守卫拿不到文件时多半**扫了个空集 ⇒ 恒绿**，不是红。按本仓自己的说法
/// 「恒绿看起来和真绿一模一样」—— 33 个守卫会一起变成装饰品，而门禁全绿。
/// ⇒ 一个东西一个住址，搬树时只改这一行。前端侧同形同理（`tests/test-support/repo-root.ts`
/// 的 `srcDirOf`，那边是 14 个消费者）。
///
/// ⚠⚠ **建这个住址的那一轮，机械替换把本函数的函数体也换成了对自己的调用** ——
/// 无限递归，`cargo test` 以**栈溢出**（SIGABRT）现形，而不是断言失败。同一趟还把上面
/// 那句「此前的形状长什么样」一并替换了，于是那句话变成「旧形状 ＝ 新形状」的废话。
/// ⇒ 这是本仓「**尺子量到了自己**」那一族的又一例，而且是最容易中的一种：抽住址时，
/// **新住址的定义本身就是旧形状的最后一个实例**，任何按形状扫全树的改法都会吃掉它。
/// 教训落成纪律：**抽住址时把定义处排除在替换人群之外**；引用旧形状的文字也不写成
/// 能被同一条规则命中的完整形。
///
/// ⚠ 本函数**不覆盖** `.join("Cargo.toml")` 那 3 处 —— 它们跟着 **manifest** 走，
/// 不跟着源码树走，搬树时本来就不该动。两件事别混成一件。
pub(crate) fn src_root() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).to_path_buf()
}

/// 后端**测试树**根的唯一住址 —— [`src_root`] 的配对。
///
/// # 为什么必须有这一个
///
/// 测试整体搬出 `src/` 之后，凡是「**人群 ＝ 源码树**」的判据都会**安静地少一块人群**：
/// 它们仍然扫得到东西（所以不红），只是扫不到搬走的那 19 个文件。
/// 🔴 「扫不全」与「扫得对」在断言上长得一模一样 —— 除非那条判据自己带着
/// 「采集到的 ＋ 跳过的 ＝ 树上全部」这类**总量对账**。本轮正是那几条对账把它逮住的
/// （`no_timer_guard` 的数量相等 · `plugin_walk_fixture` 的集合相等 · `listen` 的住址存在性）。
/// ⇒ 凡是「全体后端代码」的人群，用 [`code_roots`]，不要只用 [`src_root`]。
pub(crate) fn tests_root() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/backend")
}

/// 一份文件的**生产段** —— 按它住在哪棵树分流。
///
/// # 🔴 为什么不能直接调 [`production_code`]〔步 7c 剖分 2026-09-19〕
///
/// [`production_code`] 剥的是**文件内部**的 `#[cfg(test)]` 段 —— 那是「生产与测试同住一份
/// 文件」时代的分界线。步 7c 把 `src/backend` 的测试段整批搬进了 `<repo>/tests/backend/`：
/// 那些文件**整份都是测试代码，而且外面没有 `#[cfg(test)]` 包着** ⇒ 对它们调
/// [`production_code`] 会**原样返回整份**，于是测试代码被当成生产代码。
///
/// 现打后果（本轮真撞上，不是假想）：`no_timer_guard` 报
/// 「生产代码 `../../tests/backend/control/capture_pane_tests.rs` 里有 `sleep(` 调用」——
/// 那是**判据自己的夹具**在睡觉，而后端的零定时器铁律说的是生产代码。
/// 同一形让 `every_duration_use_is_registered_as_non_timer` 从 4 处涨到 39 处。
///
/// ⇒ 分界线从「文件内的属性」换成「文件住哪棵树」：
/// · 住 `src/backend` ⇒ `production_code(src)`（文件内可能还有 `#[cfg(test)] mod x;` 的分号桩）；
/// · 住 `tests/backend` ⇒ **空串**（整份是测试段）。
///
/// ⚠ **仍然要把那些文件收进人群**（只是贡献 0 字节生产代码）：
/// [`no_timer_guard`] 那条「采集到的 ＋ 跳过的 ＝ 树上全部」是按**文件数**对账的，
/// 把它们整个剔出去会让那条对账红，而那条对账正是 [`tests_root`] 头注说的那道网。
/// 「全空了」这一形由各判据自己的**真代码总量下限**接着。
///
/// ⚠ 判定按 [`tests_root`] 的**规范化绝对路径**前缀比，不按文件名后缀 ——
/// 名字里带 `_tests` 是约定，住址才是事实（纪律 2：
/// 靠位置的判断要明写成一条读得出来的规则，别靠命名巧合）。
pub(crate) fn production_side_of(path: &std::path::Path, src: &str) -> String {
    let at = lexically_normalized(path);
    if test_roots()
        .iter()
        .any(|t| at.starts_with(lexically_normalized(t)))
    {
        return String::new();
    }
    production_code(src)
}

/// 逐分量消掉 `.` 与 `..`，**不碰文件系统**。
///
/// 🔴 为什么不用 `std::fs::canonicalize`〔步 7c 现打撞上〕：
/// 本文件住 `src/backend/`，也就是 `readonly_guard` 的**生产段人群**里。
/// 那条只读白名单按「动词」认文件系统调用，而 `canonicalize` 不在 `READ_ONLY` 里
/// ⇒ 加一行 `fs::canonicalize` 当场把 `every_fs_call_in_backend_production_is_read_only` 打红。
/// 它确实是只读调用，但「往白名单加一个动词」是**放宽那条红线**，
/// 而这里根本不需要碰盘：两个路径都由 `env!("CARGO_MANIFEST_DIR")` 拼出来，
/// 里面的 `..` 是**字面**的，词法消解就够，而且不依赖目录存不存在。
fn lexically_normalized(p: &std::path::Path) -> std::path::PathBuf {
    let mut out = std::path::PathBuf::new();
    for c in p.components() {
        match c {
            std::path::Component::ParentDir => {
                out.pop();
            }
            std::path::Component::CurDir => {}
            other => out.push(other.as_os_str()),
        }
    }
    out
}

/// **全体后端代码**的两棵树：生产（`src/backend`）＋ 测试（`tests/backend`）。
pub(crate) fn code_roots() -> [std::path::PathBuf; 3] {
    [src_root(), tests_root(), comms_tests_root()]
}

/// 中转 crate（`comms-outward`，住 `src/comms/outward/`）的单测住 `tests/comms/outward/`：它们不编进本 crate，
/// 但中转是本 crate 生产闭包的一部分（人群声明见 `Cargo.toml` 的 `[package.metadata.guard]`），按人群扫的判据连它的测试树一起看。
pub(crate) fn comms_tests_root() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/comms/outward")
}

/// 中转 crate 的根（`lib.rs` 所在，`src/comms/outward/`）。扫「中转那一层」用它。
pub(crate) fn relay_root() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../comms/outward")
}

/// 中转的宿主（本 crate 的 `relay/` 模块：绑口 · 钥匙 · 起中转）。
pub(crate) fn relay_host_root() -> std::path::PathBuf {
    src_root().join("relay")
}

/// 本 crate 的全部测试树（[`tests_root`] ＋ [`comms_tests_root`]），两棵互不包含。
pub(crate) fn test_roots() -> [std::path::PathBuf; 2] {
    [tests_root(), comms_tests_root()]
}

/// **仓库根**的唯一住址。
///
/// 此前有 6 处各自写 `Path::new(env!("CARGO_MANIFEST_DIR")).parent()` —— 那在
/// `Cargo.toml` 住仓根下一层时成立。manifest 一挪，6 处偏移**各自都要改**，
/// 而它们分散在 4 个文件里、每处的 `.parent()` 层数还不一样。
/// ⇒ 与 [`src_root`] / [`tests_root`] 同一条纪律：**一个东西一个住址**，挪 manifest 只改这里。
pub(crate) fn repo_root() -> std::path::PathBuf {
    src_root()
        .parent()
        .and_then(|p| p.parent())
        .expect("src/backend 的上两级 = 仓根")
        .to_path_buf()
}

/// 界面那一侧的一个毫秒常量（`const NAME = 10_000;` 那一行，恰好一处），现从源码取。
/// 后端的总期限要短于界面等那条命令的时间，判据两边都现取，不手抄数。
pub(crate) fn ui_const_ms(rel: &str, name: &str) -> u64 {
    let text = std::fs::read_to_string(repo_root().join(rel))
        .unwrap_or_else(|e| panic!("读不到 {rel}：{e}"));
    let head = format!("const {name} = ");
    let hits: Vec<&str> = text
        .lines()
        .filter_map(|l| l.trim_start().strip_prefix(&head))
        .collect();
    assert_eq!(hits.len(), 1, "{rel} 里 `{head}` 应当恰好一处：{hits:?}");
    hits[0]
        .trim_end_matches(';')
        .replace('_', "")
        .trim()
        .parse()
        .unwrap_or_else(|e| panic!("{rel} 的 {name} 不是整数毫秒：{:?}（{e}）", hits[0]))
}

/// 🔴 后端 crate 的**根源码面** —— 它从一份变成了两份。
///
/// 前置 1 把模块声明、身份（`BUILD_ID` ＋ 戳）、`SUBCOMMANDS` /
/// `CAPABILITIES` / `EMITS` 这一族搬进了 `lib.rs`，**分派留在 `main.rs`**。
///
/// ⚠ **这个函数存在的理由是「别有第二份手抄」**：搬家当天有 **8** 条源码扫描型守卫
/// 同时红，全部是因为各自手写着 `include_str!(".../main.rs")`。它们没有瞎报 ——
/// 每一条都逐字说清了「我要的那个东西不在扫描面里了」。但**八份住址意味着下一次
/// 搬家还会红八次**，而那八次里只要有一条被人顺手改成「找不到就算了」，
/// 它就会从此静默恒绿。⇒ 住址收成这一处。
///
/// ⚠ **要「只看分派」的判据别用它** —— 那种判据要的是 `main.rs` 单独一份
/// （比如「这条臂在不在 `match` 里」）。本函数给的是**两份拼起来的全集**。
pub fn backend_root_source() -> String {
    format!("{}\n{}", include_str!("lib.rs"), include_str!("main.rs"),)
}

/// 入方向那个目录（`stream/inbound/`，含命令表各族）下的**每一份**源文件：`(相对 [`src_root`] 的路径, 生产段)`，按路径排。
///
/// 入方向从一份文件拆成一个目录之后，人群曾经是「`inbound.rs` 那一份」的判据，人群现在是这一整个目录 ——
/// 只看其中一份会安静地少一块人群（不红，只是扫不全）。
pub(crate) fn inbound_sources() -> Vec<(String, String)> {
    let root = src_root();
    let mut out: Vec<(String, String)> =
        guard_core::scan_tree_excluding(&root.join("stream/inbound"), &["rs"], &[])
            .into_iter()
            .map(|(path, src)| {
                let rel = path
                    .strip_prefix(&root)
                    .unwrap_or(&path)
                    .to_string_lossy()
                    .replace('\\', "/");
                (rel, production_code(&src))
            })
            .collect();
    out.sort();
    assert!(
        out.iter().any(|(rel, _)| rel == "stream/inbound/mod.rs"),
        "入方向目录里没扫到 `mod.rs` —— 住址坏了，用它的判据会在空人群上恒绿"
    );
    out
}

/// 命令表各族（`stream/inbound/registry/*.rs`）：形状同 [`inbound_sources`]，一族一份。
///
/// 按 `CommandSpec {` 切块的判据要**逐份切**，别把几份拼起来再切：拼起来的话，
/// 一份的文件头（`use` · 常量）会粘到上一份的最后一块上，被算成那条命令的一部分。
pub(crate) fn registry_sources() -> Vec<(String, String)> {
    let out: Vec<(String, String)> = inbound_sources()
        .into_iter()
        .filter(|(rel, _)| rel.starts_with("stream/inbound/registry/"))
        .collect();
    assert!(
        out.len() >= 2,
        "只扫到 {} 份命令表族文件 —— 住址坏了，用它的判据会在空人群上恒绿",
        out.len()
    );
    out
}

#[cfg(test)]
#[path = "../../tests/backend/guard_support_tests.rs"]
mod tests;
