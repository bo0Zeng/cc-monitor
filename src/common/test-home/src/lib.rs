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
//! `TMUX_TMPDIR` 指到沙箱根底下先建好的 `tx`（外面给了也换），`TMUX` / `TMUX_PANE` 不论家是哪个都摘：判据起的孙进程里的裸 tmux
//! 连不到缺省那台 server；建不出 ⇒ 测试进程不跑。子测试进程继承了沙箱标记、`TMUX_TMPDIR` 却缺了 ⇒ 补成同一个沙箱根底下的 `tx`。
//! ⚠ 只在测试进程活着时成立：进程退出删掉沙箱根之后，留下的孤儿进程再裸调 tmux，目录不在了，tmux 退回 `/tmp`、连上缺省那台。
//!
//! 判据要真家的只能自己显式去拿账号数据库那一格；本 crate 不给。
//!
//! 临时目录也一样：每个测试进程起来时在外面给的临时目录下建自己的沙箱根，`TMPDIR`（Windows 上 `TMP` · `TEMP`）指进去，
//! 进程退出时（判据红了也一样）整个删掉 —— 判据各自建的临时目录、它起的子进程留下的套接字目录都随它走，不在系统临时目录里攒着。
//!
//! 另外收着几样判据共用的小助手，每样只这一份实现：临时目录（[`scratch`] · [`list`]）、马上 exec 的假程序（[`write_exe`]）、目录改权限位（[`set_dir_mode`]）、
//! 撑开 fork 窗口（[`forking`]）、「别人持锁」（[`held_elsewhere`] · [`let_go`]）。

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

/// 沙箱根已经建好的标记（值 ＝ 沙箱根）：判据起的子进程继承它 ⇒ 子进程里不再建、不再换家。
pub const SANDBOX_MARK: &str = "RUST_TEST_SANDBOX_ROOT";

/// 指向临时目录的那几个变量（`std::env::temp_dir` 与各家临时目录库都照它们）。
const TEMP_VARS: &[&str] = &["TMPDIR", "TMP", "TEMP"];

/// 沙箱根底下给裸 tmux 的 socket 目录（`TMUX_TMPDIR`）。名字短：tmux 在它下面再套 `tmux-<uid>/default`，整条要压在 108 字节里。
const TMUX_DIR: &str = "tx";

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

/// 判据要写一份马上 exec 的假程序：都经这里。本进程只写旁稿 `<path>.src`，由 `cp` 子进程写到 `path`，删旁稿，chmod 755。
/// 本进程里从没有过 `path` 的写 fd：在本进程里直接写，旁边别的判据线程正 fork 时，子进程在 fork 到 exec 之间也攥着那份写 fd
/// （`O_CLOEXEC` 到 exec 才生效），这时 exec 它 ＝ ETXTBSY。写旁稿再改名没用：改名后还是同一个 inode，漏出去的写端照样指着它。
/// 写不成 ⇒ panic。
#[cfg(unix)]
pub fn write_exe(path: &Path, body: &str) {
    use std::os::unix::fs::PermissionsExt as _;
    let mut side = path.as_os_str().to_owned();
    side.push(".src");
    let side = PathBuf::from(side);
    std::fs::write(&side, body)
        .unwrap_or_else(|e| panic!("test-home: 写不出 {}：{e}", side.display()));
    let st = std::process::Command::new("cp")
        .arg(&side)
        .arg(path)
        .status()
        .unwrap_or_else(|e| panic!("test-home: 起不来 cp：{e}"));
    assert!(
        st.success(),
        "test-home: cp 没写成 {}：{st}",
        path.display()
    );
    std::fs::remove_file(&side)
        .unwrap_or_else(|e| panic!("test-home: 删不掉 {}：{e}", side.display()));
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755))
        .unwrap_or_else(|e| panic!("test-home: 改不了 {} 的权限：{e}", path.display()));
}

/// 判据给目录改权限位（只读 · 进不去 · 改回来）：都经这里。目录的 x 位是「进得去」，不是「能执行」；
/// 给文件设可执行位只在 [`write_exe`] 一处（现写现 exec 不撞 ETXTBSY）⇒ `dir` 不是目录（连指向目录的链接也不算）⇒ panic。
/// 改不成 ⇒ panic。
#[cfg(unix)]
pub fn set_dir_mode(dir: &Path, mode: u32) {
    use std::os::unix::fs::PermissionsExt as _;
    let is_dir = std::fs::symlink_metadata(dir).is_ok_and(|m| m.is_dir());
    assert!(
        is_dir,
        "test-home: {} 不是目录 —— 给文件设权限位不经这里（可执行的经 write_exe）",
        dir.display()
    );
    std::fs::set_permissions(dir, std::fs::Permissions::from_mode(mode))
        .unwrap_or_else(|e| panic!("test-home: 改不了 {} 的权限：{e}", dir.display()));
}

/// 旁边不停 fork：[`forking`] 起的那几条线程，drop 时停下、收齐；哪一条起不来 `/bin/true`（窗口没撑开）⇒ drop 时红。
#[cfg(unix)]
pub struct Forking {
    stop: std::sync::Arc<AtomicBool>,
    threads: Vec<std::thread::JoinHandle<()>>,
}

/// 判据要撑开「fork 到 exec 之间，子进程攥着本进程开着的 fd」那个窗口：`n` 条线程循环起 `/bin/true` 并收尸，直到返回的那一个
/// drop。不看墙钟：判据在它活着时把要判的那一段循环若干次，撞上一次当场红、不重试。
#[cfg(unix)]
pub fn forking(n: usize) -> Forking {
    let stop = std::sync::Arc::new(AtomicBool::new(false));
    let threads = (0..n)
        .map(|_| {
            let stop = stop.clone();
            std::thread::spawn(move || {
                while !stop.load(Ordering::Relaxed) {
                    std::process::Command::new("/bin/true")
                        .status()
                        .unwrap_or_else(|e| {
                            panic!("test-home: 旁边 fork 的 /bin/true 起不来：{e}")
                        });
                }
            })
        })
        .collect();
    Forking { stop, threads }
}

#[cfg(unix)]
impl Drop for Forking {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        let broke = self
            .threads
            .drain(..)
            .map(std::thread::JoinHandle::join)
            .filter(Result::is_err)
            .count();
        if broke > 0 && !std::thread::panicking() {
            panic!("test-home: 旁边 fork 的 {broke} 条线程半路停了 —— 窗口没撑开，这一趟判不了");
        }
    }
}

/// 「别人」持着的一把排他文件锁：[`held_elsewhere`] 拿，[`let_go`] 放。unix 上是持锁的那个子进程。
#[cfg(unix)]
pub struct Held(std::process::Child);

/// 「别人」持着的一把排他文件锁：[`held_elsewhere`] 拿，[`let_go`] 放。没有 fork 的那几家是本进程另开的那份句柄。
#[cfg(not(unix))]
pub struct Held(std::fs::File);

/// 判据要「别人持着 `dir` 里 `name` 那份锁文件的排他锁」：都从这里拿。unix 上是一个真子进程开着它、拿上 `flock`（等着拿），
/// 说了 ok 才回来。不在本进程里开那份锁文件：本进程开着它时旁边别的判据线程 fork 出的子进程在 fork 到 exec 之间也攥着
/// （flock 跟着打开的那份文件），本进程放了、锁还在。命令用 `&&` 串：没拿上锁（没有 `flock` 这条命令也算）⇒ 不说 ok，当场红。
#[cfg(unix)]
pub fn held_elsewhere(dir: &Path, name: &str) -> Held {
    use std::io::BufRead as _;
    let mut c = std::process::Command::new("/bin/sh")
        .arg("-c")
        .arg(format!(
            "exec 9>>'{name}' && flock -x 9 && echo ok && read _"
        ))
        .current_dir(dir)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn()
        .unwrap_or_else(|e| panic!("test-home: 起不来持锁的 sh：{e}"));
    let mut said = String::new();
    std::io::BufReader::new(c.stdout.take().expect("test-home: 接了 stdout"))
        .read_line(&mut said)
        .unwrap_or_else(|e| panic!("test-home: 读不到持锁的 sh 说的：{e}"));
    assert_eq!(said, "ok\n", "test-home: 别人没拿上 {name} 的锁");
    Held(c)
}

/// 没有 fork 的那几家：句柄各管各的锁，本进程另开一份持着就是「别人」。
#[cfg(not(unix))]
pub fn held_elsewhere(dir: &Path, name: &str) -> Held {
    let f = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(dir.join(name))
        .unwrap_or_else(|e| panic!("test-home: 开不了锁文件 {name}：{e}"));
    f.lock()
        .unwrap_or_else(|e| panic!("test-home: 别人没拿上 {name} 的锁：{e}"));
    Held(f)
}

/// 放掉 [`held_elsewhere`] 拿的锁：unix 上关子进程的 stdin、收尸；别处关掉那份句柄。
pub fn let_go(h: Held) {
    #[cfg(unix)]
    {
        let mut c = h.0;
        drop(c.stdin.take());
        c.wait()
            .unwrap_or_else(|e| panic!("test-home: 收不了持锁的 sh：{e}"));
    }
    #[cfg(not(unix))]
    drop(h.0);
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

/// 裸 tmux（判据起的孙进程里 `sh -c 'tmux …'` 那几条不经子进程闸）连 `$TMUX_TMPDIR/tmux-<uid>/default`：指进沙箱根底下的
/// [`TMUX_DIR`]，整棵进程树继承。目录不在 tmux 就退回 `/tmp`、连上缺省那台 ⇒ 建不出就不跑。
fn point_bare_tmux_into(root: &Path) {
    let tmux = root.join(TMUX_DIR);
    if let Err(e) = std::fs::create_dir_all(&tmux) {
        eprintln!(
            "test-home: 建不出 {}：{e} —— 裸 tmux 会落到缺省 server，不跑",
            tmux.display()
        );
        std::process::exit(101);
    }
    std::env::set_var("TMUX_TMPDIR", &tmux);
}

#[ctor::ctor]
fn sandbox_home() {
    if let Some(root) = std::env::var_os(SANDBOX_MARK) {
        // 子测试进程：父判据交来的照用；`TMUX_TMPDIR` 缺了（父判据 `env_clear()` 过）补成同一个沙箱根底下的那一个。
        if std::env::var_os("TMUX_TMPDIR").is_none() {
            point_bare_tmux_into(Path::new(&root));
        }
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
    point_bare_tmux_into(&root);
    // `TMUX` 压过 `TMUX_TMPDIR`：不论家是哪个（家是那条路径的前缀时 `strays` 会留下它），一律摘。
    std::env::remove_var("TMUX");
    std::env::remove_var("TMUX_PANE");
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
