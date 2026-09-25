//! 〔NT2 · S1〕`stderr_log` 的判据。
//!
//! # 守的要求（住址，纪律 19）
//!
//! - `设计/15 §4.7 S1`（逐字）：「**本机 · 脱离常驻载体**（Linux 缺省；全部 SSH 与中转都在它里面）| null（`StderrSink::Null`）| **仍开**」·
//!   「受害的都是事件源的失效告知 …… 说出来了，没人听」。
//! - 主会话 4C 第二批裁（`4c2-lanes.md` 开头，逐字）：「诊断没有读者（S1）：脱离载体的常驻后端 stderr 落本机日志文件（有上限、滚动），
//!   设置页『日志』里看得到」。
//! - 设计与现打：`调研/第四波记录/NT2.md §0.2 · §2`。
//!
//! # 判据
//!
//! - L1 滚动（真文件）：累计超上限 ⇒ 盘上恰好两份、当前那份 ≤ 上限、旧那份逐字节是上一段；起来时已有当前那份 ⇒ 它变成旧的；
//!   写不进去（目录不在）⇒ 字节记丢、不 panic。期望由写进去的字节现算（异源：不看实现怎么切）。
//! - L2 生产接线（真子进程 ＋ 真 fd 2）：子进程走 `install_from_env`（`main.rs` 真调的那一个），之后它 `eprintln!` 的、`tracing` 写的、
//!   **它起的子进程**写 stderr 的，全落进那份文件；它自己的 stderr 管子上**一个字节都没有**（fd 真被换走了）。没交路径 ⇒ 零文件（另一向）。
//!
//! # 买不到
//!
//! - 真脱离载体（`spawn_detached` × 真 monitor）端到端：那一格在 monitor 那侧判据（交不交这一格）＋ 读数脚本。
//! - 真 Windows：非 unix 臂回 `Failed`（本机没跨 target 跑）。

use super::*;

fn tmp_dir(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("nt2-stderr-log-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).expect("建临时目录");
    d
}

/// 目录里的 `.log` 文件名（经共用遍历原语，不裸 `read_dir`）。
fn log_names(d: &Path) -> Vec<String> {
    let mut names: Vec<String> = guard_core::scan_tree_excluding(d, &["log"], &[])
        .into_iter()
        .map(|(p, _)| p.file_name().unwrap().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

/// 去掉滚动说明那一行（有就去，没有原样）。
fn body_of(bytes: Vec<u8>) -> (bool, Vec<u8>) {
    if bytes.starts_with(b"[stderr-log] ") {
        let nl = bytes
            .iter()
            .position(|b| *b == b'\n')
            .expect("说明行没有换行")
            + 1;
        (true, bytes[nl..].to_vec())
    } else {
        (false, bytes)
    }
}

/// L1 ★ 滚动：上限 200、逐块写 9 块 × 30 字节（每块内容不同）⇒ 盘上恰好两份（`.log` 里）、每份 ≤ 上限；
/// 旧 ＋ 当前（去掉说明行）拼起来是写进去的字节流的**尾巴**、当前那份以最后一块收尾；当前那份第一行说清旧的挪去了哪、再早的丢了。
/// 期望不照实现的切法算（异源）：只判「没丢尾、没乱序、有说明、没超限」这几条性质。
#[test]
fn it_rolls_at_the_cap_and_keeps_exactly_one_old_file() {
    let d = tmp_dir("roll");
    let cur = d.join("stderr.log");
    let cap = 200u64;
    let mut r = Roller::start(cur.clone(), cap);
    let chunks: Vec<Vec<u8>> = (0..9u8).map(|i| vec![b'a' + i; 30]).collect();
    for c in &chunks {
        r.write(c);
    }
    assert_eq!(r.dropped(), 0, "一个字节都不该丢");
    assert_eq!(
        log_names(&d),
        vec!["stderr.log".to_string(), "stderr.old.log".to_string()],
        "盘上不是恰好当前 ＋ 旧的两份"
    );
    let cur_raw = std::fs::read(&cur).unwrap();
    let old_raw = std::fs::read(old_path_of(&cur)).unwrap();
    assert!(
        cur_raw.len() as u64 <= cap && old_raw.len() as u64 <= cap,
        "有一份超了上限"
    );
    let (noted, cur_body) = body_of(cur_raw.clone());
    assert!(noted, "滚过之后当前那份第一行没有说明");
    let said = String::from_utf8_lossy(&cur_raw).into_owned();
    assert!(
        said.contains(&old_path_of(&cur).display().to_string()) && said.contains("丢了"),
        "说明行没说清旧的挪去了哪、再早的丢了：{said:?}"
    );
    let (_, old_body) = body_of(old_raw);
    assert!(!old_body.is_empty(), "旧的那份是空的");
    let all: Vec<u8> = chunks.concat();
    let tail = [old_body, cur_body.clone()].concat();
    assert!(
        all.ends_with(&tail),
        "旧 ＋ 当前不是写进去那一串的尾巴（丢了中间 / 乱了序）"
    );
    assert!(cur_body.ends_with(&chunks[8]), "当前那份没以最后一块收尾");
    let _ = std::fs::remove_dir_all(&d);
}

/// L1 ★ 起来时已有当前那份（上一次常驻留下的）⇒ 它变成旧的、当前那份从说明行开始；原先的旧那份被盖掉（盘上仍是两份）。
/// 另一向：起来时没有上一份 ⇒ 没什么可说，当前那份从第一个字节就是这一次的。
#[test]
fn a_restart_moves_the_last_run_aside() {
    let d = tmp_dir("restart");
    let cur = d.join("stderr.log");
    std::fs::write(old_path_of(&cur), b"older").unwrap();
    std::fs::write(&cur, b"last run").unwrap();
    let mut r = Roller::start(cur.clone(), 1000);
    r.write(b"this run");
    let (noted, body) = body_of(std::fs::read(&cur).unwrap());
    assert!(noted, "挪了上一次那份却没说");
    assert_eq!(body, b"this run");
    assert_eq!(std::fs::read(old_path_of(&cur)).unwrap(), b"last run");
    assert_eq!(log_names(&d).len(), 2);
    let d2 = tmp_dir("fresh");
    let cur2 = d2.join("stderr.log");
    let mut r2 = Roller::start(cur2.clone(), 1000);
    r2.write(b"first");
    assert_eq!(std::fs::read(&cur2).unwrap(), b"first");
    assert_eq!(log_names(&d2), vec!["stderr.log".to_string()]);
    let _ = std::fs::remove_dir_all(&d);
    let _ = std::fs::remove_dir_all(&d2);
}

/// L1 另一向：写不进去（目录不在）⇒ 字节记丢、不 panic、`pump` 照样把源读到底。
#[test]
fn an_unwritable_target_drops_bytes_but_keeps_draining() {
    let d = tmp_dir("gone");
    let cur = d.join("no-such-dir").join("stderr.log");
    let src = std::io::Cursor::new(vec![b'x'; 50_000]);
    let r = pump(src, Roller::start(cur.clone(), 1 << 20));
    assert_eq!(
        r.dropped(),
        50_000,
        "读到的字节没有全部记丢 —— 源没被读到底，或者假装写进去了"
    );
    assert!(!cur.exists());
    let _ = std::fs::remove_dir_all(&d);
}

/// 没交路径 / 交了空白 ⇒ `NotAsked`，一个 fd 都不碰（本进程 fd 2 原样 —— 否则测试进程自己的输出就没了）。
#[test]
fn nothing_is_installed_without_a_path() {
    for v in [None, Some(""), Some("   ")] {
        let got = install_from_env(&|k| {
            assert_eq!(k, ENV);
            v.map(str::to_string)
        });
        assert_eq!(got, Installed::NotAsked, "交了 {v:?} 却装了");
    }
}

// ─────────────────────────────────────────────────────────────────────────────
//  L2：生产接线在真子进程里 —— fd 2 真被换走、三种写 stderr 的都落盘
// ─────────────────────────────────────────────────────────────────────────────

const CHILD_MARK: &str = "CCM_NT2_STDERR_CHILD";
const CHILD_TEST_NAME: &str = "stderr_log::tests::stderr_log_child_entry_point";
const DONE: &str = "nt2-child-done";

/// 子进程入口（不是判据 ⇒ `#[ignore]`）：装上（与 `main.rs` 同一个写口）→ 三种方式写 stderr → 等它们落盘 → 退出。
#[test]
#[ignore = "子进程入口：只在被父判据用 CCM_NT2_STDERR_CHILD 拉起时才跑"]
fn stderr_log_child_entry_point() {
    if std::env::var(CHILD_MARK).is_err() {
        return;
    }
    let got = install_from_env(&|k| std::env::var(k).ok());
    let Installed::Logging(path) = got else {
        panic!("子进程没装上：{got:?}");
    };
    eprintln!("nt2-eprintln-line");
    // 起一个继承 stderr 的子进程写 stderr（fd 那一层接住的才接得住它）。
    let st = std::process::Command::new("sh")
        .args(["-c", "echo nt2-grandchild-line >&2"])
        .status()
        .expect("起 sh");
    assert!(st.success());
    eprintln!("{DONE}");
    // 等读线程把这几行落盘（测试段，不在零定时器的人群里）。
    for _ in 0..200 {
        if std::fs::read_to_string(&path).is_ok_and(|s| s.contains(DONE)) {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(25));
    }
    panic!("5 秒内那几行没落进 {}", path.display());
}

fn run_child(env_path: Option<&Path>) -> (std::process::ExitStatus, String) {
    let exe = std::env::current_exe().expect("测试二进制自己的路径");
    let mut cmd = std::process::Command::new(exe);
    cmd.args([
        CHILD_TEST_NAME,
        "--exact",
        "--ignored",
        "--nocapture",
        "--test-threads=1",
    ])
    .env(CHILD_MARK, "1")
    .env_remove(ENV)
    .stdin(std::process::Stdio::null())
    .stdout(std::process::Stdio::null())
    .stderr(std::process::Stdio::piped());
    if let Some(p) = env_path {
        cmd.env(ENV, p);
    }
    let out = cmd.output().expect("起子进程");
    (
        out.status,
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

/// L2 ★ 生产接线：三种写 stderr 的（`eprintln!` · 继承 stderr 的孙进程 · 标记行）全落进文件，
/// 而子进程自己的 stderr 管子上**这几行一行都没有**（fd 2 真被换走了，不是「写了两份」）。
#[test]
fn the_production_wiring_lands_every_stderr_writer_in_the_file() {
    let d = tmp_dir("child");
    let cur = d.join("stderr.log");
    let (st, pipe) = run_child(Some(&cur));
    assert!(
        st.success(),
        "子进程没正常收工：{st:?}\n它的 stderr：{pipe}"
    );
    let file = std::fs::read_to_string(&cur).expect("那份文件没建出来");
    for line in ["nt2-eprintln-line", "nt2-grandchild-line", DONE] {
        assert!(file.contains(line), "`{line}` 没落进文件：{file:?}");
        assert!(
            !pipe.contains(line),
            "`{line}` 还出现在原来那根 stderr 上 —— fd 2 没被换走"
        );
    }
    let _ = std::fs::remove_dir_all(&d);
}

/// L2 另一向：没交路径 ⇒ 子进程入口当场报「没装上」（它的 panic 话出现在原来那根 stderr 上 —— 那根管子是活的），且零文件。
#[test]
fn without_the_env_the_child_installs_nothing_and_writes_no_file() {
    let d = tmp_dir("child-none");
    let (st, pipe) = run_child(None);
    assert!(!st.success(), "没交路径子进程却装上了");
    assert!(
        pipe.contains("子进程没装上：NotAsked"),
        "原来那根 stderr 上没有那句 panic 话（非空对照失败）：{pipe}"
    );
    assert!(log_names(&d).is_empty(), "没交路径却写了文件");
    let _ = std::fs::remove_dir_all(&d);
}
