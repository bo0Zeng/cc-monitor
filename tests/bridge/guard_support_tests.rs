use super::*;

/// 🔴 反空真自检（`设计/16 §5.2`）—— 没有这条，上面每个住址都可能安静地指向
/// 一个不存在的目录，而所有靠它取人群的测试会**扫空集然后通过**。
///
/// ⚠ 判据**点名具体文件**，不数条目数。两个理由：
/// ① 数条目数是弱判据（一个装了别的东西的目录也能过）；
/// ② `scanning_guard_registry` 那条元判据禁止测试段里**裸遍历目录**
///    （理由：判据在自己那份语料里找到自己 ⇒ 恒绿），而那张存量清单**只许变短**
///    ⇒ 新增的判据不该去要豁免，应当换成不遍历的写法。
#[test]
fn every_address_points_at_something_we_can_name() {
    for (name, dir, probes) in [
        ("crate_root", crate_root(), &["Cargo.toml", "build.rs"][..]),
        (
            "crate_src_root",
            crate_src_root(),
            &["lib.rs", "main.rs"][..],
        ),
        (
            "repo_root",
            repo_root(),
            &["package.json", "tsconfig.json", "vite.config.ts"][..],
        ),
        (
            "repo_src_root",
            repo_src_root(),
            &["main.ts", "backend", "bridge"][..],
        ),
        ("tests_root", tests_root(), &["backend", "e2e"][..]),
        (
            "backend_src_root",
            backend_src_root(),
            &["main.rs", "Cargo.toml"][..],
        ),
    ] {
        assert!(dir.is_dir(), "{name}() = {dir:?}，不是目录");
        for probe in probes {
            assert!(
                dir.join(probe).exists(),
                "{name}() = {dir:?} 下没有 {probe} —— 这个住址指错了地方"
            );
        }
    }
    // 两个 `src` 必须是不同的地方 —— 这是本文件最容易被误用的一格。
    assert_ne!(
        crate_src_root(),
        repo_src_root(),
        "crate_src_root() 与 repo_src_root() 撞了 —— 说明某一级爬错"
    );
    assert!(
        crate_root().starts_with(repo_src_root()),
        "本 crate 应当住在 <repo>/src/ 下面"
    );
}
