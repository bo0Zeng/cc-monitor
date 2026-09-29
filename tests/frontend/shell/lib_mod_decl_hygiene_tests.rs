/// ★ **`mod` 声明区不许出现连续重复的 `#[cfg(test)]`**〔devbench F10 / audit-0805 V7-4〕。
///
/// # 它逮的是痕迹，不是病本身（这一点要说清）
///
/// 本文件 `:61-66` 那条警告自己写过真风险：「插在这里、不要插在上面那条注释与
/// `#[cfg(test)]` 之间…U1a 初版就插错了位置，把属性与 `mod structural_scan;` 的配对拆开
/// —— `structural_scan` 当场变成无条件编译……而 CI 的 `cargo build` 没有 `-D warnings`
/// ⇒ **不会红**」。
///
/// 08-10 实测：那个失效模式**又发生过两次** —— `lockfile_conflict_guard` 上方叠了 2 个
/// `#[cfg(test)]`、`parity_ledger` 上方叠了 3 个（`git blame` 显示来自 4 个不同 commit）。
/// Rust 对重复 cfg **静默 AND**（无 error 无 warning），所以那两处**行为上无害** ——
/// 但它们是「属性被插错位置」留下的**痕迹**。
///
/// ⚠ **真风险（某个 mod 该带 cfg 却没带）钉不了**：本仓「哪些 mod 该带 `cfg(test)`」
/// 没有统一规则 —— 24 个判据模块里 8 个带、16 个不带，因为多数是**整个文件内部**
/// `cfg(test)`（F02 时逐个核过：那些文件首个非注释 item 之前顶层 item 数全为 0）。
/// ⇒ 本条只逮痕迹。**零误报是它的优点，别读成「配对都对了」。**
#[test]
fn no_duplicated_cfg_test_attributes_in_the_mod_block() {
    let src = include_str!("../../../src/frontend/shell/src/lib.rs");
    let dup = format!("#[cfg({0})]\n#[cfg({0})]", "test");
    // 抽取器自检：本文件必须真的有 `mod` 声明与 cfg 属性，否则整条空转。
    assert!(
        src.contains("mod structural_scan;") && src.contains(&format!("#[cfg({})]", "test")),
        "抽不到 `mod` 声明区或 cfg 属性 —— 抽取器坏了，本条会零命中地绿"
    );
    assert!(
        !src.contains(&dup),
        "`lib.rs` 里出现了**连续重复的 `#[cfg(test)]`**。\n\
             \n\
             Rust 对它静默 AND，所以编译器不会说话 —— 但它意味着有人**在属性与它配对的\n\
             `mod` 之间插了新东西**。那个动作的另一半后果是真的：\n\
             属性一旦与原本的 `mod` 拆开，那个 mod 就变成**无条件编译**，\n\
             而 CI 的 `cargo build` 没有 `-D warnings` ⇒ 不会红（见本文件 :61-66 那条警告）。\n\
             \n\
             ⇒ 修法：删掉多余的那个，并确认每个 `#[cfg(test)]` 下面紧跟的是它该管的那个 `mod`。\n\
             ⚠ 本条只逮这个痕迹，**逮不到「某个 mod 该带 cfg 却没带」** —— 那个没有统一规则可钉。"
    );
}
