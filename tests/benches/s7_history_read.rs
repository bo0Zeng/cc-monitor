//! 秤 7 ——「Rust 侧」的量具。表**第 7 行**，验的是 `§3.1` 那句
//! 「取尾是 O(文件) 时间但 O(N) 内存，且首次打开必须数总行数，这个 O(文件) 是**不可避免**的」。
//!
//! 要回答的那一问逐字：**「若实测很贵，则值得缓存 `(size, mtime) → (total, tail_from)`
//! 让计数也增量化」** —— 所以这把秤不能只吐一个总数，必须把总耗时拆成
//! 「**数行的那一趟**（缓存能省掉的）」与「**把字节搬出去**（缓存省不掉的）」两半。
//!
//! # 怎么测：驱动**真的发货二进制**，一行生产代码都不抄
//!
//! 取尾那一趟（`query::tail::tail_plan`，只数行、不搬正文）是后端的私有 fn ⇒ 外部 target 够不着。
//! 本秤 **spawn 真二进制的真 CLI 入口** `--history-tail`（帧命令 `history-tail` 派生的那一面，入参经
//! `--args-b64`）。`CARGO_BIN_EXE_*` 由 cargo 在**构建 bench target 时**注入，指向同一 profile 下编出来的那个 bin
//! ⇒ 被测的就是发货路径本身，零漂移。
//! （从前驱动的是一次性 `--read-session-tail` / `--read-session` / `--read-session-from-offset`；那几条手写子命令
//! 第 7 批第二题删了，正文按字节区间另走 `history-read` 分页，「搬字节」那一格随之没有了单独的 CLI 入口。）
//!
//! **代价如实记**：每次读数里含一次 `fork/exec` + 动态链接。所以表里**必有一行进程地板**
//! （同一入口问一份只有一行的会话：走完 argv 派发 + 解入参 + 路径围栏 + `File::open` + 数一行），
//! 所有结论都用**差值**说话，不用绝对值说话。
//!
//! # 一条减法（表里的读数就是这么拆出来的）
//!
//! | 量 | 怎么得 |
//! |---|---|
//! | 进程地板 `S` | `--history-tail`（一行的会话） |
//! | **数行那一趟 `K`** | `--history-tail`（那一份大会话）− `S` |
//!
//! `K` 就是「`(size, mtime)` 缓存能省掉的那一块」。**判断值不值得做，看的是 K，不是总数。**
//!
//! # 两档：`cargo bench` 出读数，`cargo test` 只证明「跑得起来」
//!
//! bench 源码的家是 `tests/benches/`（「度量能力」那一格）。
//!
//! - **`cargo bench --bench s7_history_read`**（cargo 给目标传 `--bench`）⇒ 下面那张读数表，照旧。
//!   🔴 墙钟**不当判据**：这一档从不进门禁。
//! - **`cargo test`**（`Cargo.toml` 里本目标标了 `test = true`，cargo 不传 `--bench`）⇒ [`smoke`]：
//!   自造一份小语料、取尾与地板两格各经本秤自己的 [`measure`] / [`measure_piped`] 跑一次，
//!   断言**那张图与从语料独立算出的期望相等**（异源：一边是真二进制的输出，
//!   一边是本文件按行切原文算出来的）；一个耗时都不看。
//!   ⇒ 门禁 `backend` 那一格的 `cargo test` 顺带跑它：秤坏了（被测 CLI 改了参数 / 输出形状、
//!   `measure` 读不到字节、反空真自检不再拒空输入）当场红，而不是等下一次有人想量的时候才发现。
//!
//! # 它量不到什么（别把这份读数当全部）
//!
//! - **只是 Linux**。Windows 客户端上没量（自己也写着 Windows 那侧只跑 monitor）。
//! - **热页缓存**。夹具刚生成就读，文件整个在 page cache 里。冷缓存要 root 才能
//!   `drop_caches`，本秤不做 ⇒ 读数是**计算 + 内存拷贝**的下界，不是磁盘的。
//! - **stdout 丢进 `/dev/null`**，不过 SSH。生产里应答要过网络，那一段不在读数里。
//! - **不是真机语料**。是按仓内那份**合成**语料的结构放大出来的（见 `S7-make-corpus.sh`）。

#![allow(
    clippy::disallowed_methods,
    reason = "量具：掐表量读历史的耗时，不进后端产品"
)]
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Instant;

/// 被测的真二进制。cargo 构建 bench target 时注入，与本次 profile 同一份。
// 🔴 bin 名 `cc-monitor-remote → cc-monitor-backend`。
// ⚠ **这一处是改名那一刀的射程漏洞的活样本**：那一路把整个 `tests/evidence/` 排掉了
//   （理由是「改读数等于把历史改成假话」—— 对读数成立），
//   **但本文件是 `[[bench]]`，它真的在编译** ⇒ 排除规则「按目录」太粗。
//   它没有静默：`winchk-backend`（跨 target `--all-targets`）当场红。
const BIN: &str = env!("CARGO_BIN_EXE_cc-monitor-backend");

/// 生产里快照拉取用的 N（`src/frontend/shell/src/stream_source/snapshot.rs` 的 `SNAPSHOT_TAIL_LINES` ＝ 壳的 `REPLAY_TAIL_KEEP`）。
const SNAPSHOT_TAIL_LINES: usize = 600;

/// 大文件那几格的计时次数。15 MB 一趟几十毫秒，9 次够出 min/p50/max 又不拖慢门禁。
const REPS_BIG: usize = 9;
/// 小增量那几格的计时次数（单次逼近进程地板，噪声占比大 ⇒ 多打几次）。
const REPS_SMALL: usize = 25;

fn main() {
    // `cargo bench` 给 bench 目标传 `--bench`；`cargo test` 不传（它可能传过滤串，一概不认）。
    if !std::env::args().skip(1).any(|a| a == "--bench") {
        smoke();
        return;
    }
    let home = resolve_home();
    let sess = home.join("projects/s7-bench");

    println!("== 秤 7：Rust 侧（表第 7 行）==");
    print_machine_banner();
    println!("被测二进制 : {BIN}");
    println!("夹具目录   : {}", sess.display());
    println!("重建夹具   : tests/evidence/S7-make-corpus.sh   （产物落 .build/，不进 git）\n");

    // ── 反空真自检：先证明「输入不存在 / 是空的」会被当成失败 ───────────────
    // 全篇的教训之一：恒绿看起来和真绿一模一样。一把量文件的秤，
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

    // ── 夹具：三档大小同形同源（单会话字节 中位/p90/max）+ 长尾变体 ──
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

    // 进程地板那一格问的那份：只有一行的会话（夹具目录里现写，不进 git）。
    let one = sess.join("s7-one-line.jsonl");
    std::fs::write(&one, b"{\"type\":\"system\"}\n").expect("写一行的会话");

    let mut rows: Vec<Row> = Vec::new();

    // ① 取尾：`history-tail`（只数行）。三档大小 + 长尾变体，N = 生产的那个数。
    for (label, path, _) in &sizes {
        rows.push(measure(
            &format!("tail  N={SNAPSHOT_TAIL_LINES}  {label}"),
            &home,
            &tail_args(path, SNAPSHOT_TAIL_LINES),
            REPS_BIG,
        ));
    }
    // ② 进程地板：同一入口问一行的会话。argv 派发 + 解入参 + 围栏 + open + 数一行都走了。
    rows.push(measure(
        "floor 进程地板（一行的会话）",
        &home,
        &tail_args(&one, SNAPSHOT_TAIL_LINES),
        REPS_SMALL,
    ));
    // ③ 管道对照：上面整张表的 stdout 都丢 `/dev/null`；这里把最大那一格改成**父进程
    //    用管道全读回来**再跑一遍 —— 「数行那一趟 K」在两种去处下必须对得上，
    //    对得上才说明 K 量的是扫描本身、不是某个 sink 的伪影。
    let (_, big_path, _) = &sizes[2];
    rows.push(measure_piped(
        &format!("tail  N={SNAPSHOT_TAIL_LINES}  max 15.1 MiB〔管道〕"),
        &home,
        &tail_args(big_path, SNAPSHOT_TAIL_LINES),
        REPS_BIG,
    ));

    println!(
        "{:<38} {:>3} {:>9} {:>9} {:>9} {:>9}  {:>12}",
        "格", "n", "min ms", "p50 ms", "均 ms", "max ms", "出字节"
    );
    for r in &rows {
        println!("{r}");
    }

    // 出字节恒零的告警：每一格都该回一行图。恒零 ⇒ 那个入口没在干活（死值验时就是这个样子）。
    // 这里**只告警不退出** —— 死值那一跑要的正是「读数还印得出来、但肉眼可分」。
    let hollow: Vec<&str> = rows
        .iter()
        .filter(|r| r.out_bytes == 0)
        .map(|r| r.label.as_str())
        .collect();
    if !hollow.is_empty() {
        println!("\n⚠ 这几格出字节 = 0，被测入口没在干活（死值验就长这样）：{hollow:?}");
        println!("  ⇒ 下面「减法」那一段对它们没有意义。");
    }

    // ── 减法：把总耗时里「数行那一趟」拆出来 ─────────────────────────────────
    println!("\n== 减法（同一台机、同一轮、同一夹具；用差值说话） ==");
    let floor = rows
        .iter()
        .find(|r| r.label.starts_with("floor"))
        .map(|r| r.p50_ms)
        .unwrap_or(0.0);
    println!("进程地板 S = {floor:.3} ms（fork/exec + 动态链接 + argv 派发 + 解入参 + 围栏 + open + 数一行）");
    for (label, _, bytes) in &sizes {
        let Some(t) = pick(&rows, &format!("tail  N={SNAPSHOT_TAIL_LINES}  {label}")) else {
            continue;
        };
        let k = t - floor;
        let mb = *bytes as f64 / 1_048_576.0;
        println!(
            "{label:<24} 总 {t:7.3} ms  **数行 K = {k:7.3} ms**  （{mb:.1} MiB ⇒ 数行 {:.1} MiB/s）",
            mb / (k / 1000.0),
        );
    }
    if let Some(tp) = pick(
        &rows,
        &format!("tail  N={SNAPSHOT_TAIL_LINES}  max 15.1 MiB〔管道〕"),
    ) {
        println!(
            "〔管道对照〕max 15.1 MiB  总 {tp:7.3} ms  **数行 K = {:7.3} ms**",
            tp - floor,
        );
        println!("            ⇒ 两种 sink 下的 K 对得上 ⇒ K 量的是扫描本身。");
    }
}

/// `--history-tail` 那一问的 argv：`{path, n}` 经 `--args-b64`（不碰 stdin）。
fn tail_args(path: &Path, n: usize) -> Vec<std::ffi::OsString> {
    let args = serde_json::json!({ "path": path.to_string_lossy(), "n": n }).to_string();
    vec![
        o("--history-tail"),
        o("--args-b64"),
        cc_monitor_backend::wire::b64_encode(args.as_bytes()).into(),
    ]
}

/// 冒烟档（`cargo test` 跑它）：证明这把秤**跑得起来、量的是它自称的那件事**，不出任何耗时。
///
/// 语料：仓内那份已经合成过的 `tests/__fixtures__/scale2-height-records.jsonl`（
/// 结构照真的、内容一律合成），原样放进一个一次性 agent home（后端的路径围栏要它落在
/// `<home>/projects/` 下）。期望值**全部由本函数按行切原文算**，不问被测二进制：
///
/// | 被测入口 | 期望 |
/// |---|---|
/// | `--history-tail {path, n: N}` | `{total: T, tail_from: T−N, split_at: 前 T−N 行的字节和, end: 文件长, file: 第一行的摘要}` |
/// | 同上，一行的会话（进程地板那一格） | `{total: 1, tail_from: 0, split_at: 0, end: 那一行的长, file: 那一行的摘要}` |
///
/// 每一格都先经本秤自己的 [`measure`]（`reps = 1`）拿出字节数与期望那一行比，取尾那一格再经 [`measure_piped`]
/// 比一次，并直接比一次**解出来的那张图** —— 读数表里的「出字节」那一列就是这么来的。
fn smoke() {
    const N: usize = 10;
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/__fixtures__/scale2-height-records.jsonl");
    let corpus = std::fs::read(&fixture).expect("读仓内合成语料 scale2-height-records.jsonl");
    // 行（含换行符）。本语料每一行都完整、非空 —— 这是期望式成立的前提，先钉住它。
    let lines: Vec<&[u8]> = corpus.split_inclusive(|b| *b == b'\n').collect();
    assert!(
        lines.len() > N && lines.iter().all(|l| l.ends_with(b"\n") && l.len() > 1),
        "秤 7 冒烟：语料要多于 {N} 行、每行完整且非空（现 {} 行）—— 期望式的前提不成立",
        lines.len()
    );
    let total = lines.len();
    let tail_from = total - N;
    let split_at: usize = lines[..tail_from].iter().map(|l| l.len()).sum();
    // 文件身份：第一行正文（去掉 `\n`）的 FNV-1a 64 摘要，16 位十六进制。按定义在这里现算，不调后端那一份。
    let fnv = |body: &[u8]| {
        format!(
            "{:016x}",
            body.iter().fold(0xcbf2_9ce4_8422_2325_u64, |h, &b| {
                (h ^ u64::from(b)).wrapping_mul(0x0000_0100_0000_01b3)
            })
        )
    };
    let file = fnv(&lines[0][..lines[0].len() - 1]);

    let home = std::env::temp_dir().join(format!("s7-smoke-{}", std::process::id()));
    let sess = home.join("projects/s7-bench");
    std::fs::create_dir_all(&sess).expect("建一次性 agent home");
    let f = sess.join("s7-smoke.jsonl");
    std::fs::write(&f, &corpus).expect("放语料");
    let empty = sess.join("s7-empty.jsonl");
    std::fs::write(&empty, b"").expect("放空文件");
    let one_line: &[u8] = b"{\"type\":\"system\"}\n";
    let one = sess.join("s7-one-line.jsonl");
    std::fs::write(&one, one_line).expect("放一行的会话");

    // 反空真：秤自己的输入检查真的会拒「不存在」与「空」。
    assert!(check_input(&sess.join("s7-does-not-exist.jsonl"), 1).is_err());
    assert!(check_input(&empty, 1).is_err());
    assert_eq!(check_input(&f, 1), Ok(corpus.len() as u64));

    let cases: [(&str, Vec<std::ffi::OsString>, serde_json::Value); 2] = [
        (
            "tail",
            tail_args(&f, N),
            serde_json::json!({"total": total, "tail_from": tail_from, "split_at": split_at, "end": corpus.len(), "file": file}),
        ),
        (
            "floor",
            tail_args(&one, N),
            serde_json::json!({"total": 1, "tail_from": 0, "split_at": 0, "end": one_line.len(), "file": fnv(&one_line[..one_line.len() - 1])}),
        ),
    ];
    for (label, args, want) in &cases {
        let out = Command::new(BIN)
            .env("CLAUDE_CONFIG_DIR", &home)
            .arg("--") // 调后端一律带打头的 `--`（分流不看 argv0）
            .args(args)
            .stdin(Stdio::null())
            .output()
            .expect("spawn 被测二进制");
        assert!(
            out.status.success(),
            "秤 7 冒烟：`{label}` 退出码 {:?}：{}",
            out.status.code(),
            String::from_utf8_lossy(&out.stderr)
        );
        let got: serde_json::Value =
            serde_json::from_slice(&out.stdout).expect("秤 7 冒烟：应答不是一行 JSON");
        assert_eq!(
            &got, want,
            "秤 7 冒烟：`{label}` 那张图与从语料独立算出的期望不等"
        );
        let row = measure(label, &home, args, 1);
        assert_eq!(
            row.out_bytes,
            out.stdout.len(),
            "秤 7 冒烟：`{label}` 那一格经 measure() 取到 {} 字节，直接跑是 {}",
            row.out_bytes,
            out.stdout.len()
        );
        assert!(row.out_bytes > 0, "秤 7 冒烟：`{label}` 一个字节都没出");
    }
    let piped = measure_piped("tail〔管道〕", &home, &cases[0].1, 1);
    let direct = measure("tail", &home, &cases[0].1, 1);
    assert_eq!(
        piped.out_bytes, direct.out_bytes,
        "秤 7 冒烟：管道那一支取到的字节数不对"
    );

    std::fs::remove_dir_all(&home).expect("收一次性 agent home");
    println!(
        "秤 7 冒烟：2 格 × 真二进制 {total} 行语料，那张图与期望相等（N={N}，tail_from={tail_from}）—— 不计时"
    );
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
/// （那条「恒绿看起来和真绿一模一样」的同族）。
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
        .arg("--")
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
/// 这一支让应答真的出进程、真的被消费，用来给「数行 K」做**换个 sink 也一样**的对照。
fn measure_piped(label: &str, home: &Path, args: &[std::ffi::OsString], reps: usize) -> Row {
    let mut ms: Vec<f64> = Vec::with_capacity(reps);
    let mut bytes = 0usize;
    for i in 0..=reps {
        let t0 = Instant::now();
        let out = Command::new(BIN)
            .env("CLAUDE_CONFIG_DIR", home)
            .arg("--")
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
        .arg("--")
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
    // 本文件住 `<repo>/tests/benches/`，`CARGO_MANIFEST_DIR` 是 `<repo>/src/backend`。
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../.build/s7/home")
        .to_path_buf()
}

/// 🔴 **毫秒不可跨机比较。** 那组数既没记机器也没记语料，
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
