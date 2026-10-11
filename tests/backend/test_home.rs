//! 测试档的家从一开始就是沙箱：测试二进制的第一条判据跑之前，`HOME` · `USERPROFILE` · `XDG_*` 指到这个进程自己的临时目录、
//! `CCM_DATA_DIR` 摘掉 —— 直接敲 `cargo test`、IDE 里点、quick-check 都一样，判据经 `Ctx::here()` · `path_now()` 碰不到真家
//! （10-11 那一次：直接敲 `cargo test`，判据把真家里的 `rotation.json` 换了形）。整个模块只在测试档（`lib.rs` 里 `#[cfg(test)]`）。

/// 测试档的家（这个进程自己的临时目录，测试二进制起来时建好）。
pub(crate) fn sandbox_root() -> std::path::PathBuf {
    std::env::var_os(SANDBOX_MARK)
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| {
            std::env::temp_dir().join(format!("ccm-test-home-{}", std::process::id()))
        })
}

/// 已经换过家的标记（值 ＝ 沙箱根）：判据自己起的子进程继承它 ⇒ 子进程里不再换。
pub(crate) const SANDBOX_MARK: &str = "RUST_TEST_SANDBOX_HOME";

/// 要不要换：`HOME` 是本账号在账号数据库里登记的那个真家（或没设）⇒ 换；判据（或它起的子进程）自己给了别的家 ⇒ 用它给的。
/// 账号数据库查不到（非 unix）⇒ 一律换。
fn must_move(home: Option<&std::path::Path>, account_home: Option<&std::path::Path>) -> bool {
    match (home, account_home) {
        (None, _) | (_, None) => true,
        (Some(h), Some(a)) => h == a,
    }
}

/// 测试二进制的第一条判据跑之前（`main` 之前，单线程）：`HOME` 还是这台用户的真家 ⇒ `HOME` · `USERPROFILE` · `XDG_*` 指到
/// [`sandbox_root`] 下；`CCM_DATA_DIR` 指在真家里 ⇒ 摘掉。判据经 `Ctx::here()` · `rotation::path_now()` · `platform::paths::home_dir()`
/// 走到的家都不是真家，判出来的也不随这台机器上有什么而变。判据要真家的只能自己显式拿账号数据库那一格（`platform::paths::account_home`）。
#[ctor::ctor]
fn sandbox_home() {
    let real = crate::platform::paths::account_home();
    if let (Some(d), Some(r)) = (std::env::var_os(creds_core::store::DATA_DIR_ENV), &real) {
        if std::path::Path::new(&d).starts_with(r) {
            std::env::remove_var(creds_core::store::DATA_DIR_ENV);
        }
    }
    if std::env::var_os(SANDBOX_MARK).is_some() {
        return;
    }
    let home = std::env::var_os("HOME").map(std::path::PathBuf::from);
    if !must_move(home.as_deref(), real.as_deref()) {
        return;
    }
    let root = sandbox_root();
    let home = root.join("home");
    let _ = std::fs::create_dir_all(&home);
    std::env::set_var(SANDBOX_MARK, &root);
    std::env::set_var("HOME", &home);
    std::env::set_var("USERPROFILE", &home);
    for (k, sub) in [
        ("XDG_CONFIG_HOME", ".config"),
        ("XDG_DATA_HOME", ".local/share"),
        ("XDG_CACHE_HOME", ".cache"),
        ("XDG_STATE_HOME", ".local/state"),
    ] {
        std::env::set_var(k, home.join(sub));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 只在 `HOME` 是真家（或没设）时换；判据自己给的家照用。
    #[test]
    fn only_the_real_home_is_moved() {
        let real = std::path::Path::new("/home/someone");
        assert!(must_move(Some(real), Some(real)));
        assert!(must_move(None, Some(real)));
        assert!(!must_move(
            Some(std::path::Path::new("/tmp/x/home")),
            Some(real)
        ));
        assert!(must_move(Some(real), None));
    }
}
