//! 各条源码扫描型守卫共用的「只留生产段」剥法。整个模块只在 `cfg(test)` 下存在。
//! 实现在 [`guard_core`]（`src/common/guard-core`，剥法的坑与无花括号体 mod 声明那条都写在它的模块头注里），本模块只是再导出 +
//! 存放后端专属的那几条住址；`crate::guard_support::production_code` 等路径原样可用。它是量具，不是判据。

pub(crate) use guard_core::{assert_no_test_code, production_code, production_source};

/// 本 crate 源码树根的唯一住址。搬树时只改这一行：几十个扫描型守卫各写一份的话，源码树一搬它们会扫一个不存在的目录、
/// 扫了个空集 ⇒ 恒绿。前端侧同形（`tests/test-support/repo-root.ts` 的 `srcDirOf`）。
/// 抽住址时把定义处排除在替换人群之外（新住址的定义本身就是旧形状的最后一个实例）。
/// 不覆盖 `.join("Cargo.toml")` 那几处：它们跟着 manifest 走，不跟着源码树走。
pub(crate) fn src_root() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).to_path_buf()
}

/// 后端测试树根的唯一住址 —— [`src_root`] 的配对。测试住 `tests/backend/`：凡是「全体后端代码」的人群用 [`code_roots`]，
/// 只用 [`src_root`] 会安静地少一块人群（扫不全与扫得对在断言上长得一样，除非判据带着「采集到的 ＋ 跳过的 ＝ 树上全部」的总量对账）。
pub(crate) fn tests_root() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/backend")
}

/// 一份文件的生产段 —— 按它住在哪棵树分流：
/// · 住 `src/backend` ⇒ `production_code(src)`（文件内可能还有 `#[cfg(test)] mod x;` 的分号桩）；
/// · 住 `tests/backend` ⇒ 空串（整份是测试段，外面没有 `#[cfg(test)]` 包着；直接调 [`production_code`] 会原样返回整份，把测试夹具当成生产代码）。
/// 那些文件仍要收进人群（贡献 0 字节生产代码）：[`no_timer_guard`] 那条「采集到的 ＋ 跳过的 ＝ 树上全部」按文件数对账。
/// 判定按 [`tests_root`] 的规范化绝对路径前缀比，不按文件名后缀（名字里带 `_tests` 是约定，住址才是事实）。
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

/// 逐分量消掉 `.` 与 `..`，不碰文件系统：本文件住 `src/backend/`，在 `readonly_guard` 的生产段人群里，`canonicalize` 不在它的只读动词表上；
/// 两个路径都由 `env!("CARGO_MANIFEST_DIR")` 拼出来，里面的 `..` 是字面的，词法消解就够。
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

/// 仓库根的唯一住址（与 [`src_root`] / [`tests_root`] 同一条纪律：挪 manifest 只改这里）。
pub(crate) fn repo_root() -> std::path::PathBuf {
    src_root()
        .parent()
        .and_then(|p| p.parent())
        .expect("src/backend 的上两级 = 仓根")
        .to_path_buf()
}

/// 后端 crate 的根源码面：`lib.rs`（模块声明、身份 `BUILD_ID` ＋ 戳、`SUBCOMMANDS` / `CAPABILITIES` / `EMITS` 这一族）＋ `main.rs`（分派），两份拼起来的全集。
/// 住址只有这一处：各条扫描守卫手写 `include_str!` 的话，下一次搬家就会一起红、或被改成「找不到就算了」静默恒绿。
/// 要「只看分派」的判据别用它 —— 那种判据要 `main.rs` 单独一份。
pub fn backend_root_source() -> String {
    format!("{}\n{}", include_str!("lib.rs"), include_str!("main.rs"),)
}

/// 入方向那个目录（`stream/inbound/`，含命令表各族）下的每一份源文件：`(相对 [`src_root`] 的路径, 生产段)`，按路径排。
/// 只看其中一份会安静地少一块人群。
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

/// 一条命令应答的 typed 结构体（serde 序列化；处理器经 `stream::inbound::spec::wire` 把它变成应答 `data`）。
/// 判据（`every_command_declares_exactly_the_fields_it_puts_out`）从它真序列化的样本拿键，与登记的出参两向对拍：
/// `samples` 每一支（枚举的每个变体 · 只在某一支才有的格）给一个，可缺的格都填上 —— 漏了哪一支，那一支的格登了就红在「登了没出」。
pub(crate) trait Shaped: serde::Serialize + Sized {
    fn samples() -> Vec<Self>;
}

/// [`Shaped`] 的样本过真序列化器（判据那一侧读）。
pub(crate) fn sampled<T: Shaped>() -> serde_json::Value {
    serde_json::Value::Array(
        T::samples()
            .iter()
            .map(|s| serde_json::to_value(s).expect("样本序列化不出"))
            .collect(),
    )
}
