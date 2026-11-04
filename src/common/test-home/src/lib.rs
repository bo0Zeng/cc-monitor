//! **测试档的家一开始就是沙箱**（10-11 那一次：直接敲 `cargo test` 没换 `$HOME`，后端判据经 `Ctx::here()` 把真家里的
//! `~/.cc-monitor/rotation.json` 换了形，装着的旧后端读不懂、轮换当场停了）。
//!
//! 每个有判据的 Rust crate（后端 · monitor · 文件窗口 · `src/common/*` · `src/comms/*`）都把本 crate 当 dev 依赖，并在 crate 根上
//! 留一行 `#[cfg(test)] #[used] static TEST_HOME: fn() = test_home::linked;`（让链接器一定把启动段那一节带进测试二进制）。
//! 于是直接敲 `cargo test`、IDE 里点、quick-check 都一样：测试二进制起来时（`main` 之前，单线程），`HOME` 若是本账号在账号数据库里
//! 登记的那个真家（或没设），就把 `HOME` · `USERPROFILE` · `XDG_*` 指到这个进程自己的临时目录。给了别的家照用。
//! 外面那个会话带进来的状态（`CLAUDE_CONFIG_DIR` 指着真账号目录 · `TMUX` / `TMUX_PANE` 指着默认 tmux ·
//! `CCM_*` / `ANTHROPIC_*` / `CODEX_*` 一族指着真数据目录、真端口、真上游）按仓里那一张会话变量表摘（见 [`leaked`] · [`strays`]）：
//! 家是真家（或没设）⇒ 表里的一律摘；给了别的家 ⇒ 表里的值是一条指向那个家以外的绝对路径就摘（10-28 那一次：只换了 `HOME`、
//! 没摘 `CLAUDE_CONFIG_DIR`，判据照着真账号的记录树读写），别的照用（判据 `env_clear()` 后起的子测试进程，父判据交给它的
//! `CCM_HOST` 之类是输入）。摘掉之后记录树 · 账号库 · 数据目录都退回 `HOME` 底下。
//! 判据起的子测试进程继承了沙箱标记的，一样都不摘。
//!
//! 判据要真家的只能自己显式去拿账号数据库那一格；本 crate 不给。
//!
//! 临时目录也一样：每个测试进程起来时在外面给的临时目录下建自己的沙箱根，`TMPDIR`（Windows 上 `TMP` · `TEMP`）指进去，
//! 进程退出时（判据红了也一样）整个删掉 —— 判据各自建的临时目录、它起的子进程留下的套接字目录都随它走，不在系统临时目录里攒着。

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

/// 沙箱根已经建好的标记（值 ＝ 沙箱根）：判据起的子进程继承它 ⇒ 子进程里不再建、不再换家。
pub const SANDBOX_MARK: &str = "RUST_TEST_SANDBOX_ROOT";

/// 指向临时目录的那几个变量（`std::env::temp_dir` 与各家临时目录库都照它们）。
const TEMP_VARS: &[&str] = &["TMPDIR", "TMP", "TEMP"];

/// 沙箱根是本进程建的 ⇒ 退出时删。继承来的（父判据建的）归父进程删。
static OWNED: AtomicBool = AtomicBool::new(false);

/// 会话继承来的环境变量表（仓里 `tests/scripts/session-vars`，跑测试的脚本也读这一张）：一行一个名字，`*` 结尾按前缀认，
/// `!` 起头是族里照留的判据输入。
const SESSION_VARS: &str = include_str!("../../../../tests/scripts/session-vars");

/// 这个变量是外面那个会话漏进来的状态 ⇒ 测试进程起来时摘掉。清单只在 [`SESSION_VARS`] 那一张表。
pub fn leaked(name: &str) -> bool {
    let hit = |p: &str| match p.strip_suffix('*') {
        Some(prefix) => name.starts_with(prefix),
        None => name == p,
    };
    let rules = SESSION_VARS
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'));
    let (mut take, mut keep) = (false, false);
    for r in rules {
        match r.strip_prefix('!') {
            Some(k) => keep |= hit(k),
            None => take |= hit(r),
        }
    }
    take && !keep
}

/// 各 crate 根上那一行 `static` 指着它：让链接器把本 crate 的目标文件（连同启动段）带进测试二进制。什么都不做。
pub fn linked() {}

/// 这个测试进程的沙箱根（标记里那一个；没有 ⇒ 按进程号在临时目录下算出的那一个，还没建）。临时目录就是它，换了家 ⇒ 家是它下面的 `home`。
/// 名字短：判据在它下面绑 Unix 套接字，整条路径要压在 108 字节里。
pub fn sandbox_root() -> PathBuf {
    std::env::var_os(SANDBOX_MARK)
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::temp_dir().join(format!("ccm-{}", std::process::id())))
}

/// 判据要一个自己的临时目录：都从这里拿。在 [`sandbox_root`] 底下，名字是 `tag` ＋ 本进程里第几次拿（沙箱根是父判据传下来的
/// ⇒ 再带上进程号）；先清再建，回来的是空目录。每拿一次是一个新的：并行的判据、同一条判据拿两回、继承了沙箱根的子测试进程
/// 都撞不到一起。名字短：判据在它下面绑 Unix 套接字，整条路径要压在 108 字节里，`tag` 也写短。
pub fn scratch(tag: &str) -> PathBuf {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let n = NEXT.fetch_add(1, Ordering::Relaxed);
    let name = if OWNED.load(Ordering::Relaxed) {
        format!("{tag}-{n}")
    } else {
        format!("{tag}-{}-{n}", std::process::id())
    };
    let d = sandbox_root().join(name);
    let _ = std::fs::remove_dir_all(&d);
    if let Err(e) = std::fs::create_dir_all(&d) {
        panic!("test-home: 建不出 {}：{e}", d.display());
    }
    d
}

/// 判据要看自己铺的临时目录里有什么：都从这里列。回目录里的名字（文件与子目录都算），排好序；列不出 ⇒ panic。
pub fn list(dir: &Path) -> Vec<String> {
    let rd = std::fs::read_dir(dir)
        .unwrap_or_else(|e| panic!("test-home: 列不出 {}：{e}", dir.display()));
    let mut names: Vec<String> = rd
        .map(|e| {
            e.unwrap_or_else(|e| panic!("test-home: 列 {} 时出错：{e}", dir.display()))
                .file_name()
                .to_string_lossy()
                .into_owned()
        })
        .collect();
    names.sort();
    names
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
/// 比的是规范化之后的路径（符号链接 · `..` 写法的真家也是真家）；哪一边规范化不了 ⇒ 照字面比。
pub fn must_move(home: Option<&Path>, account_home: Option<&Path>) -> bool {
    match (home, account_home) {
        (None, _) | (_, None) => true,
        (Some(h), Some(a)) => same_dir(h, a),
    }
}

/// 两条路径指的是不是同一个目录：字面相同，或两边都规范化得了、规范化之后相同。
fn same_dir(a: &Path, b: &Path) -> bool {
    a == b
        || matches!(
            (std::fs::canonicalize(a), std::fs::canonicalize(b)),
            (Ok(x), Ok(y)) if x == y
        )
}

/// 这一格（名字 ＝ 值）要不要摘：不在表里 ⇒ 不摘；`HOME` 没设或就是真家（规范化之后比，同 [`must_move`]）⇒ 摘；给了别的家 ⇒ 值是一条那个家以外的绝对路径才摘。
pub fn strays(
    name: &str,
    value: &std::ffi::OsStr,
    home: Option<&Path>,
    account_home: Option<&Path>,
) -> bool {
    if !leaked(name) {
        return false;
    }
    match home {
        Some(h) if !account_home.is_some_and(|a| same_dir(h, a)) => {
            let p = Path::new(value);
            p.is_absolute() && !p.starts_with(h)
        }
        _ => true,
    }
}

#[ctor::ctor]
fn sandbox_home() {
    if std::env::var_os(SANDBOX_MARK).is_some() {
        return;
    }
    let root = sandbox_root();
    // 建不出来也照样指过去（判据宁可在沙箱里红，也不往外面的临时目录里攒）。
    if let Err(e) = std::fs::create_dir_all(&root) {
        eprintln!("test-home: 建不出 {}：{e}", root.display());
    }
    OWNED.store(true, Ordering::Relaxed);
    std::env::set_var(SANDBOX_MARK, &root);
    for k in TEMP_VARS {
        std::env::set_var(k, &root);
    }
    let home = std::env::var_os("HOME").map(PathBuf::from);
    let real = account_home();
    let gone: Vec<std::ffi::OsString> = std::env::vars_os()
        .filter(|(k, v)| {
            k.to_str()
                .is_some_and(|k| strays(k, v, home.as_deref(), real.as_deref()))
        })
        .map(|(k, _)| k)
        .collect();
    for k in gone {
        std::env::remove_var(k);
    }
    if !must_move(home.as_deref(), real.as_deref()) {
        return;
    }
    let home = root.join("home");
    if let Err(e) = std::fs::create_dir_all(&home) {
        eprintln!("test-home: 建不出 {}：{e}", home.display());
    }
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

/// 进程退出（`main` 返回 · `process::exit`，判据红了走的也是它）时删掉本进程建的沙箱根；被信号杀掉的留下。
#[ctor::dtor]
fn drop_sandbox() {
    if OWNED.load(Ordering::Relaxed) {
        let root = sandbox_root();
        if let Err(e) = std::fs::remove_dir_all(&root) {
            eprintln!("test-home: 删不掉 {}：{e}", root.display());
        }
    }
}
