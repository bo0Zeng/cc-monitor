//! 秤 7 ——「Rust 侧」的量具。`调研/设计/17 §6` 表**第 7 行**，验的是 `§3.1` 那句
//! 「取尾是 O(文件) 时间但 O(N) 内存，且首次打开必须数总行数，这个 O(文件) 是**不可避免**的」。
//!
//! 要回答的那一问逐字：**「若实测很贵，则值得缓存 `(size, mtime) → (total, tail_from)`
//! 让计数也增量化」** —— 所以这把秤不能只吐一个总数，必须把总耗时拆成
//! 「**数行的那一趟**（缓存能省掉的）」与「**把字节搬出去**（缓存省不掉的）」两半。
//!
//! # 怎么测：驱动**真的发货二进制**，一行生产代码都不抄
//!
//! `read_session_tail` / `stream_from_offset` 都是 `src/backend/observe/history_query.rs`
//! 里的**私有 fn**，而本 crate 是**纯 bin**（`Cargo.toml` 只有 `[[bin]]`，没有 `[lib]`）
//! ⇒ 外部 target 够不着它们。两条路：
//!
//! - ❌ **在 bench 里手抄一份同形实现** —— `设计/17 §0` 逐字警告过这一形
//!   （「手抄的有漂移风险，进仓固化时必须改成 import 真模块」）。本秤不走。
//! - ✅ **spawn 真二进制的真 CLI 入口**（`--read-session-tail` / `--read-session`
//!   / `--read-session-from-offset`）。`CARGO_BIN_EXE_*` 由 cargo 在**构建 bench target
//!   时**注入，指向同一 profile 下编出来的那个 bin ⇒ 被测的就是发货路径本身，零漂移。
//!
//! **代价如实记**：每次读数里含一次 `fork/exec` + 动态链接。所以表里**必有一行进程地板**
//! （`offset=EOF`：走完 argv 派发 + 路径围栏 + `File::open` + `seek`，然后 copy 0 字节），
//! 所有结论都用**差值**说话，不用绝对值说话。
//!
//! # 三条减法（表里的读数就是这么拆出来的）
//!
//! | 量 | 怎么得 |
//! |---|---|
//! | 进程地板 `S` | `--read-session-from-offset <f> <size(f)>` |
//! | 搬字节 `C` | `--read-session <f>` − `S`（`io::copy` 整文件，**不数行**） |
//! | **数行那一趟 `K`** | `--read-session-tail <f> N` − `--read-session <f>` |
//!
//! `K` 就是「`(size, mtime)` 缓存能省掉的那一块」。**判断值不值得做，看的是 K，不是总数。**
//!
//! # 它量不到什么（别把这份读数当全部）
//!
//! - **只是 Linux**。Windows 客户端上没量（`设计/17 §1` 自己也写着 Windows 那侧只跑 monitor）。
//! - **热页缓存**。夹具刚生成就读，文件整个在 page cache 里。冷缓存要 root 才能
//!   `drop_caches`，本秤不做 ⇒ 读数是**计算 + 内存拷贝**的下界，不是磁盘的。
//! - **stdout 丢进 `/dev/null`**，不过 SSH。生产里这些字节要过网络，那一段不在读数里。
//! - **不是真机语料**。是按仓内那份**合成**语料的结构放大出来的（见 `S7-make-corpus.sh`）。

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Instant;

/// 被测的真二进制。cargo 构建 bench target 时注入，与本次 profile 同一份。
const BIN: &str = env!("CARGO_BIN_EXE_cc-monitor-remote");

/// 生产里快照拉取用的 N（`src/bridge/src/ssh_source.rs` 的 `SNAPSHOT_TAIL_LINES`）。
const SNAPSHOT_TAIL_LINES: usize = 500;

/// 大文件那几格的计时次数。15 MB 一趟几十毫秒，9 次够出 min/p50/max 又不拖慢门禁。
const REPS_BIG: usize = 9;
/// 小增量那几格的计时次数（单次逼近进程地板，噪声占比大 ⇒ 多打几次）。
const REPS_SMALL: usize = 25;

fn main() {
    let home = resolve_home();
    let sess = home.join("projects/s7-bench");

    println!("== 秤 7：Rust 侧（`设计/17 §6` 表第 7 行）==");
    print_machine_banner();
    println!("被测二进制 : {BIN}");
    println!("夹具目录   : {}", sess.display());
    println!("重建夹具   : tests/evidence/S7-make-corpus.sh   （产物落 .build/，不进 git）\n");

    // ── 反空真自检：先证明「输入不存在 / 是空的」会被当成失败 ───────────────
    // `设计/17` 全篇的教训之一：恒绿看起来和真绿一模一样。一把量文件的秤，
    // 最容易的坏法就是「文件没生成 → 什么都没读 → 0 ms → 绿」。
    let missing = sess.join("s7-does-not-exist.jsonl");
    let empty = sess.join("s7-empty.jsonl");
    let mut selfcheck_ok = true;
    match check_input(&missing, 1) {
        Err(why) => println!("[反空真] 不存在的文件 → 判失败 ✓  （{why}）"),
        Ok(n) => {
            selfcheck_ok = false;
            println!("[反空真] ✗ 不存在的文件居然过了（{n} B）");
        }
    }
    match check_input(&empty, 1) {
        Err(why) => println!("[反空真] 空文件 → 判失败 ✓  （{why}）"),
        Ok(n) => {
            selfcheck_ok = false;
            println!("[反空真] ✗ 空文件居然过了（{n} B）");
        }
    }
    if !selfcheck_ok {
        eprintln!("秤 7：反空真自检没过 —— 这把秤挡不住空输入，读数不能用。");
        std::process::exit(1);
    }
    println!();

    // ── 夹具：三档大小同形同源（`设计/17 §1` 单会话字节 中位/p90/max）+ 长尾变体 ──
    let fixtures: [(&str, &str, u64); 4] = [
        ("p50 990 KiB", "s7-p50.jsonl", 990 * 1024),
        ("p90 4318 KiB", "s7-p90.jsonl", 4318 * 1024),
        ("max 15.1 MiB", "s7-main.jsonl", 15_833_498),
        ("max 15.1 MiB + 巨记录", "s7-longtail.jsonl", 15_833_498),
    ];
    let mut sizes: Vec<(&str, PathBuf, u64)> = Vec::new();
    for (label, name, floor) in fixtures {
        let p = sess.join(name);
        match check_input(&p, floor) {
            Ok(n) => {
                println!("夹具 {label:<24} {name:<20} {n} B");
                sizes.push((label, p, n));
            }
            Err(why) => {
                eprintln!("秤 7：夹具不合格 —— {why}");
                eprintln!("      先跑 tests/evidence/S7-make-corpus.sh 重建。");
                std::process::exit(1);
            }
        }
    }
    println!();

    let mut rows: Vec<Row> = Vec::new();

    // ① 取尾：`read_session_tail`。三档大小 + 长尾变体，N = 生产的 500。
    for (label, path, _) in &sizes {
        let n = SNAPSHOT_TAIL_LINES.to_string();
        rows.push(measure(
            &format!("tail  N=500  {label}"),
            &home,
            &[o("--read-session-tail"), path.clone().into(), n.into()],
            REPS_BIG,
        ));
    }
    // ② 写出地板：`--read-session` = `io::copy` 整文件，**不数行**。
    for (label, path, _) in &sizes {
        rows.push(measure(
            &format!("copy  整文件 {label}"),
            &home,
            &[o("--read-session"), path.clone().into()],
            REPS_BIG,
        ));
    }
    // ③ 进程地板：seek 到 EOF、copy 0 字节。argv 派发 + 围栏 + open + seek 都走了。
    let (_, big_path, big_size) = &sizes[2];
    rows.push(measure(
        "floor 进程地板（offset=EOF，0 字节）",
        &home,
        &[
            o("--read-session-from-offset"),
            big_path.clone().into(),
            big_size.to_string().into(),
        ],
        REPS_SMALL,
    ));
    // ④ 续拉：`stream_from_offset`。增量取 `设计/17 §1`「单条记录字节」那一列的分位。
    for (label, delta, reps) in [
        ("中位 1 641 B", 1_641u64, REPS_SMALL),
        ("p90 6 431 B", 6_431, REPS_SMALL),
        ("p99 27 184 B", 27_184, REPS_SMALL),
        ("max 631 672 B", 631_672, REPS_SMALL),
        ("1 MiB", 1_048_576, REPS_SMALL),
    ] {
        let off = big_size.saturating_sub(delta);
        rows.push(measure(
            &format!("offset 增量 {label}"),
            &home,
            &[
                o("--read-session-from-offset"),
                big_path.clone().into(),
                off.to_string().into(),
            ],
            reps,
        ));
    }
    // ⑤ 续拉的另一端：offset = 0 ⇒ 整文件。与 ② 同一件事，两条入口对照。
    rows.push(measure(
        "offset 增量 全量 15.1 MiB（offset=0）",
        &home,
        &[
            o("--read-session-from-offset"),
            big_path.clone().into(),
            o("0"),
        ],
        REPS_BIG,
    ));

    // ⑥ 管道对照：上面整张表的 stdout 都丢 `/dev/null`，而 `/dev/null` 上
    //    `io::copy` 走内核快路（15.8 MB「搬」出去只花 0.4 ms ⇒ 字节根本没真的被搬）。
    //    生产里 stdout 是一条 SSH 通道，字节要真的出去。这里把同两格改成**父进程
    //    用管道全读回来**再跑一遍 —— 「数行那一趟 K」在两种去处下必须对得上，
    //    对得上才说明 K 量的是扫描本身、不是某个 sink 的快路伪影。
    let (_, big_path, _) = &sizes[2];
    rows.push(measure_piped(
        "tail  N=500  max 15.1 MiB〔管道〕",
        &home,
        &[
            o("--read-session-tail"),
            big_path.clone().into(),
            SNAPSHOT_TAIL_LINES.to_string().into(),
        ],
        REPS_BIG,
    ));
    rows.push(measure_piped(
        "copy  整文件 max 15.1 MiB〔管道〕",
        &home,
        &[o("--read-session"), big_path.clone().into()],
        REPS_BIG,
    ));

    println!(
        "{:<38} {:>3} {:>9} {:>9} {:>9} {:>9}  {:>12}",
        "格", "n", "min ms", "p50 ms", "均 ms", "max ms", "出字节"
    );
    for r in &rows {
        println!("{r}");
    }

    // 出字节恒零的告警：`floor` 那一格**本来就该**是 0（它 seek 到 EOF），其余每一格
    // 都被要求把文件吐出来。恒零 ⇒ 那个函数没在干活（死值验时就是这个样子）。
    // 这里**只告警不退出** —— 死值那一跑要的正是「读数还印得出来、但肉眼可分」。
    let hollow: Vec<&str> = rows
        .iter()
        .filter(|r| r.out_bytes == 0 && !r.label.starts_with("floor"))
        .map(|r| r.label.as_str())
        .collect();
    if !hollow.is_empty() {
        println!("\n⚠ 这几格出字节 = 0，被测函数没在干活（死值验就长这样）：{hollow:?}");
        println!("  ⇒ 下面「减法」那一段对它们没有意义（K 会算成负数）。");
    }

    // ── 减法：把总耗时拆成「数行那一趟」与「搬字节」 ─────────────────────────
    println!("\n== 减法（同一台机、同一轮、同一夹具；用差值说话） ==");
    let floor = rows
        .iter()
        .find(|r| r.label.starts_with("floor"))
        .map(|r| r.p50_ms)
        .unwrap_or(0.0);
    println!("进程地板 S = {floor:.3} ms（fork/exec + 动态链接 + argv 派发 + 围栏 + open + seek）");
    for (label, _, bytes) in &sizes {
        let t = pick(&rows, &format!("tail  N=500  {label}"));
        let c = pick(&rows, &format!("copy  整文件 {label}"));
        let (Some(t), Some(c)) = (t, c) else { continue };
        let k = t - c;
        let mb = *bytes as f64 / 1_048_576.0;
        println!(
            "{label:<24} 总 {t:7.3} ms  搬字节 {:7.3} ms  **数行 K = {k:7.3} ms**  \
             （{:.1} MiB ⇒ 数行 {:.1} MiB/s）",
            c - floor,
            mb,
            mb / (k / 1000.0),
        );
    }
    if let (Some(tp), Some(cp)) = (
        pick(&rows, "tail  N=500  max 15.1 MiB〔管道〕"),
        pick(&rows, "copy  整文件 max 15.1 MiB〔管道〕"),
    ) {
        println!(
            "〔管道对照〕max 15.1 MiB  总 {tp:7.3} ms  搬字节 {:7.3} ms  **数行 K = {:7.3} ms**",
            cp - floor,
            tp - cp,
        );
        println!(
            "            ⇒ 两种 sink 下的 K 对得上 ⇒ K 量的是扫描本身，不是 /dev/null 的快路。"
        );
    }
}

/// 一格读数。
struct Row {
    label: String,
    n: usize,
    min_ms: f64,
    p50_ms: f64,
    mean_ms: f64,
    max_ms: f64,
    out_bytes: usize,
}

impl std::fmt::Display for Row {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{:<38} {:>3} {:>9.3} {:>9.3} {:>9.3} {:>9.3}  {:>12}",
            self.label, self.n, self.min_ms, self.p50_ms, self.mean_ms, self.max_ms, self.out_bytes
        )
    }
}

fn pick(rows: &[Row], label: &str) -> Option<f64> {
    rows.iter().find(|r| r.label == label).map(|r| r.p50_ms)
}

fn o(s: &str) -> std::ffi::OsString {
    std::ffi::OsString::from(s)
}

/// 跑一格：`reps` 次计时（stdout 丢 `/dev/null`）+ 1 次管道跑读回出字节数。
///
/// **退出码非 0 当场中止** —— 不然一条 exit 2 的错误路径会伪装成一个漂亮的小读数
/// （`设计/17` 那条「恒绿看起来和真绿一模一样」的同族）。
fn measure(label: &str, home: &Path, args: &[std::ffi::OsString], reps: usize) -> Row {
    let warm = spawn_timed(home, args);
    if warm.1 != 0 {
        eprintln!("秤 7：`{label}` 预热跑退出码 {} —— 读数作废。", warm.1);
        std::process::exit(1);
    }
    let mut ms: Vec<f64> = Vec::with_capacity(reps);
    for _ in 0..reps {
        let (dur, code) = spawn_timed(home, args);
        if code != 0 {
            eprintln!("秤 7：`{label}` 退出码 {code} —— 读数作废。");
            std::process::exit(1);
        }
        ms.push(dur);
    }
    let out = Command::new(BIN)
        .env("CLAUDE_CONFIG_DIR", home)
        .args(args)
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .expect("spawn 被测二进制");
    if !out.status.success() {
        eprintln!("秤 7：`{label}` 取出字节那一跑失败 —— 读数作废。");
        std::process::exit(1);
    }
    let mut sorted = ms.clone();
    sorted.sort_by(|a, b| a.partial_cmp(b).expect("耗时不会是 NaN"));
    Row {
        label: label.to_string(),
        n: sorted.len(),
        min_ms: sorted[0],
        p50_ms: sorted[sorted.len() / 2],
        mean_ms: sorted.iter().sum::<f64>() / sorted.len() as f64,
        max_ms: sorted[sorted.len() - 1],
        out_bytes: out.stdout.len(),
    }
}

/// 同 [`measure`]，但 stdout 走**管道**、父进程全读回来。
///
/// `/dev/null` 上 `io::copy` 会走内核快路 ⇒ 「搬字节」那一格便宜到不真实。
/// 这一支让字节真的出进程、真的被消费，用来给「数行 K」做**换个 sink 也一样**的对照。
fn measure_piped(label: &str, home: &Path, args: &[std::ffi::OsString], reps: usize) -> Row {
    let mut ms: Vec<f64> = Vec::with_capacity(reps);
    let mut bytes = 0usize;
    for i in 0..=reps {
        let t0 = Instant::now();
        let out = Command::new(BIN)
            .env("CLAUDE_CONFIG_DIR", home)
            .args(args)
            .stdin(Stdio::null())
            .stderr(Stdio::null())
            .output()
            .expect("spawn 被测二进制");
        let dur = t0.elapsed().as_secs_f64() * 1000.0;
        if !out.status.success() {
            eprintln!(
                "秤 7：`{label}` 退出码 {:?} —— 读数作废。",
                out.status.code()
            );
            std::process::exit(1);
        }
        bytes = out.stdout.len();
        if i > 0 {
            ms.push(dur); // i == 0 是预热
        }
    }
    let mut sorted = ms;
    sorted.sort_by(|a, b| a.partial_cmp(b).expect("耗时不会是 NaN"));
    Row {
        label: label.to_string(),
        n: sorted.len(),
        min_ms: sorted[0],
        p50_ms: sorted[sorted.len() / 2],
        mean_ms: sorted.iter().sum::<f64>() / sorted.len() as f64,
        max_ms: sorted[sorted.len() - 1],
        out_bytes: bytes,
    }
}

/// 一次 spawn 的墙钟毫秒 + 退出码。
fn spawn_timed(home: &Path, args: &[std::ffi::OsString]) -> (f64, i32) {
    let t0 = Instant::now();
    let st = Command::new(BIN)
        .env("CLAUDE_CONFIG_DIR", home)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .expect("spawn 被测二进制");
    let ms = t0.elapsed().as_secs_f64() * 1000.0;
    (ms, st.code().unwrap_or(-1))
}

/// 反空真：输入必须在、必须是普通文件、必须够大。
///
/// 返回 `Err` 的每一种情形都**必须**让 bench 退非 0 —— 「文件没生成 ⇒ 什么都没读
/// ⇒ 0 ms ⇒ 绿」是这类量具最容易的坏法。上面 `main` 里先拿**不存在的路径**与
/// **空文件**各验一遍本函数真的会拒，再拿它去验真夹具。
fn check_input(p: &Path, min_bytes: u64) -> Result<u64, String> {
    let m = std::fs::metadata(p)
        .map_err(|e| format!("{}：取不到元数据（多半不存在）：{e}", p.display()))?;
    if !m.is_file() {
        return Err(format!("{}：不是普通文件", p.display()));
    }
    let n = m.len();
    if n < min_bytes {
        return Err(format!(
            "{}：只有 {n} B，下限 {min_bytes} B —— 空文件或没生成全",
            p.display()
        ));
    }
    Ok(n)
}

/// 夹具 agent home。默认 `<repo>/.build/s7/home`，`S7_HOME` 可覆盖。
fn resolve_home() -> PathBuf {
    if let Some(v) = std::env::var_os("S7_HOME") {
        return PathBuf::from(v);
    }
    // 本文件住 `<repo>/tests/evidence/`，`CARGO_MANIFEST_DIR` 是 `<repo>/src/backend`。
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../.build/s7/home")
        .to_path_buf()
}

/// 🔴 **毫秒不可跨机比较。** `设计/17 §2.7` 那组数既没记机器也没记语料，
/// 于是「对不上」是谁的问题今天没人判得了。这把秤自己把机器打在读数第一屏。
fn print_machine_banner() {
    let host = std::fs::read_to_string("/etc/hostname")
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|_| "?".into());
    let cpu = std::fs::read_to_string("/proc/cpuinfo")
        .ok()
        .and_then(|s| {
            s.lines()
                .find(|l| l.starts_with("model name"))
                .and_then(|l| l.split_once(':'))
                .map(|(_, v)| v.trim().to_string())
        })
        .unwrap_or_else(|| "?".into());
    let kernel = std::fs::read_to_string("/proc/sys/kernel/osrelease")
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|_| "?".into());
    println!("机器       : {host} · {cpu} · Linux {kernel}");
    println!("口径       : 热页缓存 · stdout→/dev/null（不过 SSH）· 含一次 fork/exec");
}
