//! **测试档的家一开始就是沙箱**（10-11 那一次：直接敲 `cargo test` 没换 `$HOME`，后端判据经 `Ctx::here()` 把真家里的
//! `~/.cc-monitor/rotation.json` 换了形，装着的旧后端读不懂、轮换当场停了）。
//!
//! 每个有判据的 Rust crate（后端 · monitor · 文件窗口 · `src/common/*` · `src/comms/*`）都把本 crate 当 dev 依赖，并在 crate 根上
//! 留一行 `#[cfg(test)] #[used] static TEST_HOME: fn() = test_home::linked;`（让链接器一定把启动段那一节带进测试二进制）。
//! 于是直接敲 `cargo test`、IDE 里点、quick-check 都一样：测试二进制起来时（`main` 之前，单线程），`HOME` 若是本账号在账号数据库里
//! 登记的那个真家（或没设），就把 `HOME` · `USERPROFILE` · `XDG_*` 指到这个进程自己的临时目录；`CCM_DATA_DIR` 指在真家里 ⇒ 摘掉。
//! 判据自己给的家（例如起子进程时 `env_clear()` 再设的 `HOME`）照用。
//! 换家的同时，外面那个会话带进来的状态（`CLAUDE_CONFIG_DIR` 指着真账号目录 · `TMUX` / `TMUX_PANE` 指着默认 tmux ·
//! `CCM_*` / `ANTHROPIC_*` 一族指着真数据目录、真端口、真上游）一并摘掉：清单只在 [`leaked`] 一处。自己给了家的（台架 ·
//! 判据 `env_clear()` 后起的子进程）环境是给的那一方配的，照用（父判据给子进程设的 `CCM_RELAY_PORT` 之类不摘）。
//!
//! 判据要真家的只能自己显式去拿账号数据库那一格；本 crate 不给。

use std::path::{Path, PathBuf};

/// 已经换过家的标记（值 ＝ 沙箱根）：判据起的子进程继承它 ⇒ 子进程里不再换。
pub const SANDBOX_MARK: &str = "RUST_TEST_SANDBOX_HOME";

/// 这个变量是外面那个会话漏进来的状态 ⇒ 测试进程起来时摘掉。**唯一一份清单。**
/// 摘：`CLAUDE_CONFIG_DIR` · `CLAUDE_CODE_*` · `TMUX` · `TMUX_PANE` · `CCM_*` · `ANTHROPIC_*`。
/// 留：台架 / 人交给判据的输入（`CCM_E2E_*` · `CCM_SHOTS_*` · `CCM_FILEWIN_*` · `CCM_REGEN_*` · `CCM_BLESS*` · `CCM_PWSH` ·
/// `CCM_CJK_FONT`），它们只有判据自己读，不指向任何真东西；外加 `CCM_SANDBOX`（「这是一次沙箱跑」的标记，只会让后端更不碰真家）。
pub fn leaked(name: &str) -> bool {
    const EXACT: &[&str] = &["CLAUDE_CONFIG_DIR", "TMUX", "TMUX_PANE"];
    const FAMILY: &[&str] = &["CCM_", "ANTHROPIC_", "CLAUDE_CODE_"];
    const INPUTS: &[&str] = &[
        "CCM_E2E_",
        "CCM_SHOTS_",
        "CCM_FILEWIN_",
        "CCM_REGEN_",
        "CCM_BLESS",
        "CCM_PWSH",
        "CCM_CJK_FONT",
        "CCM_SANDBOX",
    ];
    (EXACT.contains(&name) || FAMILY.iter().any(|p| name.starts_with(p)))
        && !INPUTS.iter().any(|p| name.starts_with(p))
}

/// 后端的数据目录变量（与 `creds_core::store::DATA_DIR_ENV` 同名；本 crate 不依赖它，免得测试档多一层依赖）。
const DATA_DIR_ENV: &str = "CCM_DATA_DIR";

/// 各 crate 根上那一行 `static` 指着它：让链接器把本 crate 的目标文件（连同启动段）带进测试二进制。什么都不做。
pub fn linked() {}

/// 这个测试进程的沙箱根（换过家 ⇒ 标记里那一个；没换 ⇒ 按进程号算出的那一个，还没建）。
pub fn sandbox_root() -> PathBuf {
    std::env::var_os(SANDBOX_MARK)
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            std::env::temp_dir().join(format!("ccm-test-home-{}", std::process::id()))
        })
}

/// 本账号在账号数据库里登记的家目录（`getpwuid_r`，不看 `$HOME`）；查不到 · 非 unix ⇒ `None`。
#[cfg(unix)]
pub fn account_home() -> Option<PathBuf> {
    use std::os::unix::ffi::OsStrExt as _;
    let mut buf = vec![0 as libc::c_char; 16 * 1024];
    // SAFETY: `pwd` 由 `getpwuid_r` 填；`buf` 活到读完 `pw_dir`；返回的 `out` 只在非空时解引用。
    unsafe {
        let mut pwd: libc::passwd = std::mem::zeroed();
        let mut out: *mut libc::passwd = std::ptr::null_mut();
        let rc = libc::getpwuid_r(
            libc::getuid(),
            &mut pwd,
            buf.as_mut_ptr(),
            buf.len(),
            &mut out,
        );
        if rc != 0 || out.is_null() || pwd.pw_dir.is_null() {
            return None;
        }
        let dir = std::ffi::CStr::from_ptr(pwd.pw_dir);
        let p = PathBuf::from(std::ffi::OsStr::from_bytes(dir.to_bytes()));
        (!p.as_os_str().is_empty()).then_some(p)
    }
}

#[cfg(not(unix))]
pub fn account_home() -> Option<PathBuf> {
    None
}

/// 要不要换：`HOME` 是真家（或没设）⇒ 换；判据（或它起的子进程）自己给了别的家 ⇒ 用它给的。查不到真家 ⇒ 一律换。
pub fn must_move(home: Option<&Path>, account_home: Option<&Path>) -> bool {
    match (home, account_home) {
        (None, _) | (_, None) => true,
        (Some(h), Some(a)) => h == a,
    }
}

#[ctor::ctor]
fn sandbox_home() {
    let real = account_home();
    if let (Some(d), Some(r)) = (std::env::var_os(DATA_DIR_ENV), &real) {
        if Path::new(&d).starts_with(r) {
            std::env::remove_var(DATA_DIR_ENV);
        }
    }
    if std::env::var_os(SANDBOX_MARK).is_some() {
        return;
    }
    let home = std::env::var_os("HOME").map(PathBuf::from);
    if !must_move(home.as_deref(), real.as_deref()) {
        return;
    }
    // 摘外面会话的状态：`HOME` 没设，或就是真家（查不到真家的平台上只认「没设」：给了家 ⇒ 是谁配好的环境）。
    if home.is_none() || home.as_deref() == real.as_deref() {
        let gone: Vec<std::ffi::OsString> = std::env::vars_os()
            .map(|(k, _)| k)
            .filter(|k| k.to_str().is_some_and(leaked))
            .collect();
        for k in gone {
            std::env::remove_var(k);
        }
    }
    let root = sandbox_root();
    let home = root.join("home");
    // 建不出来也照样换（判据宁可在沙箱里红，也不回到真家）。
    if let Err(e) = std::fs::create_dir_all(&home) {
        eprintln!("test-home: 建不出 {}：{e}", home.display());
    }
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
