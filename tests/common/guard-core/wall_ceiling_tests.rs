//! **按墙钟判的上限只有一个数**：测试里「等多久算挂了」一律是 [`crate::wall::CEILING`]（300 秒）。
//!
//! 起因（10-11）：`scale6-memory-ledger.vitest.ts` 在负载 95（24 核）下撞上 vitest 默认的 5 秒、单跑就绿；
//! 截图台架在负载 20+ 下也随机漏了 8 张。按墙钟定的期限一紧，机器一忙判据就红，红了没人分得清是退步还是忙。
//! 规矩：判「够不够快」的改计步数 / 计调用次数 / 注入钟，真要看速度的进手跑的速度台（`tests/benches/`）；
//! 只为兜底「别挂死」的期限，一律用这一个数。量纲是「机器忙」，见 `wall` 模块头注。
//!
//! | # | 判什么 |
//! |---|---|
//! | W1 | 同一个数到处一样：Rust `wall::CEILING_SECS` · TS `tests/test-support/wall-ceiling.ts` 的 `WALL_CEILING_MS` · Kotlin 测试源里每一处 `WALL_CEILING_MS` · e2e `sandbox-env.sh` 的 `WALL_CEILING_S` · 截图台架 `tests/shots/run.mjs` 的 `WALL_CEILING_MS`；vitest 的 `testTimeout` / `hookTimeout` 就是它 |
//! | W2 | 门禁跑的测试源里，下面这几形的上限只许是那个数、也不许睡一会儿再断言（[`FORMS`]），例外逐条登记理由（[`EXEMPT`]，死行也红） |
//! | W3 | 用到 `WALL_CEILING_S` 的 e2e 脚本都 source 了 `sandbox-env.sh`（不然变量是空的，`timeout ""` 当场报错） |
//! | W4 | 每一形的认法在正反两个样本上判对（文件末尾） |
//!
//! 人群（门禁真跑的那些）：`tests/` 下 `*.vitest.ts` · `*.test.ts` · `vitest.config.ts`；
//! `tests/` 下 `.rs` ＋ `src/` 下 `.rs` 的测试段（`test_source`）；手机端 `src/test` · `src/androidTest` · `testing/` 的 `.kt`；
//! `tests/e2e/` 下 `.sh`。不在人群里：`tests/evidence/`（量具与读数）· `tests/benches/`（速度台，手跑）·
//! `src/vendor/`（别人的代码）· 门禁不跑的 e2e 脚本 [`SH_NOT_GATED`] · 手跑的真机台架（`test_tiers` 的 `MANUAL_RIGS`）。
//!
//! 只抓**上限**：下限（`took >= 600ms`）与静窗（「这 400 毫秒里不许有事件」）机器忙只会让它更长 / 更弱，不会误红，不管。
//! 一秒以下的等待当轮询片或静窗（逐条看过：`recv_timeout(400ms)` 后面都是 `.is_err()` / `Timeout =>`）。
//!
//! 「睡一会儿再断言」：不在轮询圈里的 `sleep(…)`、同一块往后五句之内有断言（rs-sleep）·
//! e2e 里前台的 `sleep N` 不在轮询圈里（sh-sleep）· 拿圈数 × 睡眠当期限的 Rust 轮询圈（rs-poll，换成 `wall::rounds()`）。
//! 等一件事 ⇒ 等它成立；造时间差 ⇒ 直接写 mtime / 把钟拨给被测函数；看「没发生」⇒ 先等一个说明「定了」的正面迹象
//! （收尾记号、同一条输入口打进去的记号、终局应答），等不到这种迹象的静窗才登记进例外表。
//!
//! 买不到：
//! - Rust 里睡眠藏在别的函数里（`settle()` 里睡、调用处断言）、隔了五句以上才断言的，认不出；
//!   TS 不在这里：测试源里 `setTimeout` / `setInterval` 只许 0，由 eslint 拦（`eslint.config.js`，基线 0 由 `eslint-baseline` 数着）；
//! - 期限藏进函数参数、跨文件常量、`format!` 拼出来的，认不出（同文件里 `let` / `const` 绑的认得出）；
//! - 被测系统自己的期限（喂给被测代码的 `deadline: …`、`arrive_within_ms`、`connectTimeoutMs`）不是判据的期限，不在这几形里
//!   （喂给 `set_read_timeout` / `Instant::now() + …` 的算，那几处要么换成兜底的数、要么登记成「被测期限」）；
//! - e2e 里带秒数参数的自家等待函数（`wait_line … 12`）只认得出函数体里那一圈，调用处的数认不出 —— 今天几份都改成了不收秒数；
//! - 截图台架（`tests/shots/`）不在门禁里、不扫；只核 `run.mjs` 等页面起好的那个数是同一个（W1）。

use regex::Regex;
use std::collections::BTreeMap;
use std::path::Path;

/// 固定的正则只编一次（几千份文件 × 十几个式子，每次现编要几十秒）。
macro_rules! re {
    ($pat:expr) => {{
        static RE: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
        RE.get_or_init(|| Regex::new($pat).unwrap())
    }};
}

/// `tests/e2e/` 下门禁不跑的脚本（`e2e_gate_registry` 那张表登记过为什么）：不在人群里。
const SH_NOT_GATED: &[(&str, &str)] = &[
    (
        "tests/e2e/f40-suite.sh",
        "要 Xvfb 上真开着的窗口，手跑（`tests/e2e/README.md`）",
    ),
    ("tests/e2e/graylight-suite.sh", "同上：真窗口，手跑"),
    (
        "tests/e2e/local-backend-container/",
        "容器台架（要 docker 与一份装好的包），手跑",
    ),
];

/// 认得出的几形。`(名, 语言, 说明)`。
const FORMS: &[(&str, &str, &str)] = &[
    (
        "ts-timeout",
        "ts",
        "`timeout:` / `testTimeout:` / `hookTimeout:` 给了别的数",
    ),
    (
        "ts-trailing",
        "ts",
        "`it(…, () => {…}, 60_000)` 末尾那个期限参数",
    ),
    (
        "ts-waitfor",
        "ts",
        "`vi.waitFor` / `vi.waitUntil` 没给上限（默认 1 秒）或给了别的数",
    ),
    (
        "ts-upper",
        "ts",
        "量出来的耗时（`performance.now()` / `Date.now()` 之差）拿去比上界",
    ),
    (
        "rs-wait",
        "rs",
        "`recv_timeout` / `wait_timeout` / `Instant::now() + …` / `elapsed() < …` 的期限 ≥ 1 秒",
    ),
    (
        "rs-upper",
        "rs",
        "`assert!` 里量出来的耗时（`elapsed()` 或绑了它的名字）比上界",
    ),
    ("kt-test-timeout", "kt", "`@Test(timeout = …)`"),
    (
        "kt-withtimeout",
        "kt",
        "`runBlocking` 里的 `withTimeout(…)`（`runTest` 里的是虚拟时间，不算）",
    ),
    ("kt-waituntil", "kt", "Compose 的 `waitUntil`（默认 1 秒）"),
    (
        "kt-deadline",
        "kt",
        "`System.currentTimeMillis() + …` 自己搭的期限",
    ),
    ("sh-timeout", "sh", "`timeout <数>`"),
    (
        "sh-poll",
        "sh",
        "`for i in $(seq …)` / `for ((i=0; i<N; …))` ＋ `sleep`：拿圈数 × 睡眠当期限",
    ),
    (
        "rs-poll",
        "rs",
        "`for _ in 0..N { … break … sleep(…) }` / `(0..N).any(|_| … sleep(…))`：拿圈数 × 睡眠当期限（换成 `guard_core::wall::rounds()`）",
    ),
    (
        "rs-sleep",
        "rs",
        "不在轮询圈里的 `sleep(…)`，同一块里往后五句之内有断言：睡一会儿再断言（等事件 ⇒ 等条件；造时间差 ⇒ 注入钟 / 直接写 mtime）",
    ),
    (
        "sh-sleep",
        "sh",
        "前台的 `sleep N` 不在轮询圈里：睡一会儿再看（等事件 ⇒ `wall_until`）",
    ),
    (
        "sh-upper",
        "sh",
        "量出来的耗时（`date` / `EPOCHREALTIME` 之差）拿去比上界",
    ),
];

/// 例外：`(文件, 形, 那一行里的一段原文, 理由)`。每一行都得真的命中（死行 ⇒ 红）。
const EXEMPT: &[(&str, &str, &str, &str)] = &[
    (
        "tests/e2e/weak-net/build-image.sh",
        "sh-timeout",
        "timeout 2 bash -c",
        "建镜像前探本机代理口通不通（不通就不带代理建）：环境探测，不是判据",
    ),
    (
        "tests/e2e/tmux-target-acceptance.sh",
        "sh-timeout",
        "timeout 4 script -qec \"$TMUX_BIN -L $SOCK attach -t '=cc-p1:'\"",
        "接上之后 attach 不会自己退，4 秒是收它的采样窗；判的是「窗里没出现 can't find」，忙机只会让窗里更空、不会误红",
    ),
    (
        "tests/frontend/filewin/xvfb_rig.rs",
        "rs-poll",
        "let gone = (0..20).any(|_| {",
        "收尾的宽限：请子进程自己退、两秒不退就强杀 —— 不是判据，等多久不影响红绿；换成兜底的数反倒让每次收尾可能挂五分钟",
    ),
    (
        "tests/frontend/filewin/rows_tests.rs",
        "rs-poll",
        "for _ in 0..60 {",
        "实景那一趟里「真双击认不认得出那一行」那一维判不了、只印读数登记：`row_clicks` 可能一直是 0，只给它看一眼的窗；不判红绿",
    ),
    (
        "tests/frontend/shell/local_backend_tests.rs",
        "rs-sleep",
        "std::thread::sleep(std::time::Duration::from_millis(500));",
        "静窗：判「关掉 stdin 写端之后后端不许死」——没有一个说明「它已经读到 EOF、要死早死了」的正面迹象可等；\
         机器忙只会让这半秒更弱（漏报），不会误红",
    ),
    (
        "tests/e2e/inbound-backend-frames.sh",
        "sh-sleep",
        "sleep 0.4",
        "静窗：同上（关写端之后后端不许死），e2e 那一份",
    ),
    (
        "tests/frontend/shell/dial_host_tests.rs",
        "rs-wait",
        "tokio::time::Instant::now() + total",
        "被测期限：交给生产 `Bounded` 的那个绝对时刻（1 秒），判的是到点之前 / 之后的先后，判据自己等的上限另用兜底的数",
    ),
];

pub(super) const RS_CEILING: &str = "CEILING";
/// TS 那边认两个名字：数本身，与带着它的 `vi.waitFor` 选项（同一份文件里定义）。
const TS_CEILING: &[&str] = &["WALL_CEILING_MS", "UNTIL_CEILING"];
fn ts_ceiling(s: &str) -> bool {
    TS_CEILING.iter().any(|c| s.contains(c))
}
const KT_CEILING: &str = "WALL_CEILING_MS";
const SH_CEILING: &str = "WALL_CEILING_S";
const TS_CEILING_FILE: &str = "tests/test-support/wall-ceiling.ts";
const VITEST_CONFIG: &str = "vitest.config.ts";
const SH_ENV: &str = "tests/e2e/sandbox-env.sh";
const SHOTS_RUNNER: &str = "tests/shots/run.mjs";

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Hit {
    pub form: &'static str,
    pub line: usize,
    pub text: String,
}

// ───────────────────────────── 人群 ─────────────────────────────

fn files() -> Vec<String> {
    let root = super::no_outside_refs::repo();
    let out = std::process::Command::new("git")
        .args([
            "ls-files",
            "-z",
            "--cached",
            "--others",
            "--exclude-standard",
        ])
        .current_dir(&root)
        .output()
        .expect("跑不动 `git ls-files`");
    assert!(out.status.success(), "`git ls-files` 非零退出");
    let mut v: Vec<String> = String::from_utf8_lossy(&out.stdout)
        .split('\0')
        .filter(|r| !r.is_empty() && root.join(r).is_file())
        .map(str::to_string)
        .collect();
    v.sort();
    v.dedup();
    v
}

/// 本文件：认法自检的样本写在字符串里，不扫自己。
const SELF: &str = "tests/common/guard-core/wall_ceiling_tests.rs";

fn lang_of(rel: &str) -> Option<&'static str> {
    if rel == SELF {
        return None;
    }
    let out_of_gate = ["tests/evidence/", "tests/benches/", "src/vendor/"];
    if out_of_gate.iter().any(|p| rel.starts_with(p)) {
        return None;
    }
    if rel == VITEST_CONFIG
        || (rel.starts_with("tests/") && (rel.ends_with(".vitest.ts") || rel.ends_with(".test.ts")))
    {
        return Some("ts");
    }
    if rel.ends_with(".rs") && (rel.starts_with("tests/") || rel.starts_with("src/")) {
        return Some("rs");
    }
    if rel.starts_with("src/mobile/")
        && rel.ends_with(".kt")
        && (rel.contains("/src/test/")
            || rel.contains("/src/androidTest/")
            || rel.starts_with("src/mobile/testing/"))
    {
        return Some("kt");
    }
    if rel.starts_with("tests/e2e/")
        && rel.ends_with(".sh")
        && !SH_NOT_GATED.iter().any(|(p, _)| rel.starts_with(p))
        // 手跑的真机台架（`test_tiers` 那张表登记过为什么不进执行链）也不在人群里。
        && !super::test_tiers::MANUAL_RIGS.iter().any(|(p, _)| rel == *p)
    {
        return Some("sh");
    }
    None
}

fn read(rel: &str) -> String {
    let p = super::no_outside_refs::repo().join(rel);
    std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("读不到 {rel}：{e}"))
}

/// 一份文件里要扫的那段正文（注释剥掉；Rust · TS · Kotlin 行号不变）。Rust 的 `src/` 只取测试段。
fn body(rel: &str, lang: &str, raw: &str) -> String {
    match lang {
        "rs" => {
            let t = if rel.starts_with("src/") {
                crate::test_source(raw)
            } else {
                raw.to_string()
            };
            crate::strip_trailing_comments(&crate::strip_comment_lines(&t))
        }
        "ts" | "kt" => {
            crate::strip_trailing_comments(&crate::strip_comment_lines(&strip_c_blocks(raw)))
        }
        // 整行 `#` 注释删掉（行号往后会错位几行；例外表按原文认，不按行号）。
        "sh" => crate::strip_hash_comment_lines(raw),
        _ => unreachable!(),
    }
}

/// `/* … */` 块注释换成同样多的空行（行号不变）。TS / Kotlin 用。
fn strip_c_blocks(src: &str) -> String {
    let mut out = String::with_capacity(src.len());
    let mut rest = src;
    while let Some(a) = rest.find("/*") {
        out.push_str(&rest[..a]);
        let Some(b) = rest[a..].find("*/") else {
            rest = "";
            break;
        };
        out.extend(rest[a..a + b + 2].chars().filter(|c| *c == '\n'));
        rest = &rest[a + b + 2..];
    }
    out.push_str(rest);
    out
}

// ───────────────────────────── 认法 ─────────────────────────────

fn line_at(src: &str, byte: usize) -> usize {
    src[..byte].matches('\n').count() + 1
}

fn line_text(src: &str, byte: usize) -> String {
    let a = src[..byte].rfind('\n').map_or(0, |i| i + 1);
    let b = src[byte..].find('\n').map_or(src.len(), |i| byte + i);
    src[a..b].trim().to_string()
}

/// 从 `open`（指着 `(`）起到配平的 `)` 为止那一段；不配平就取到文件尾。
fn paren_span(src: &str, open: usize) -> &str {
    let mut depth = 0i32;
    for (i, c) in src[open..].char_indices() {
        match c {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    return &src[open..open + i + 1];
                }
            }
            _ => {}
        }
    }
    &src[open..]
}

fn num(s: &str) -> f64 {
    s.replace('_', "").parse().unwrap_or(0.0)
}

/// 一段 Rust 表达式里的 `Duration` 字面量（毫秒）。认不出 ⇒ None。
fn rs_literal_ms(expr: &str) -> Option<f64> {
    let re = re!(r"from_(secs_f64|secs_f32|secs|millis|micros|nanos)\(\s*([0-9][0-9_.]*)\s*\)");
    re.captures(expr).map(|c| {
        let n = num(&c[2]);
        match &c[1] {
            "millis" => n,
            "micros" => n / 1000.0,
            "nanos" => n / 1e6,
            _ => n * 1000.0,
        }
    })
}

/// 同一份文件里 `let` / `const` / `static` 绑了 `Duration` 字面量的名字（毫秒）。
fn rs_bound(src: &str) -> BTreeMap<String, f64> {
    let re = re!(
        r"(?:let|const|static)\s+(?:mut\s+)?([A-Za-z_][A-Za-z0-9_]*)\s*(?::[^=;]*)?=\s*([^;]*)"
    );
    let mut m = BTreeMap::new();
    for c in re.captures_iter(src) {
        if &c[1] == "_" {
            continue;
        }
        if let Some(ms) = rs_literal_ms(&c[2]) {
            if !c[2].contains("Instant") {
                m.insert(c[1].to_string(), ms);
            }
        }
    }
    m
}

/// 期限表达式（一个调用的实参 / `+` 号右边）折成毫秒：字面量 · 绑过字面量的名字 · 它们乘一个整数。
fn rs_deadline_ms(expr: &str, bound: &BTreeMap<String, f64>) -> Option<f64> {
    let base = rs_literal_ms(expr).or_else(|| {
        let id = re!(r"[A-Za-z_][A-Za-z0-9_]*");
        let found = id
            .find_iter(expr)
            .find_map(|m| bound.get(m.as_str()).copied());
        found
    })?;
    let mul = re!(r"\*\s*([0-9][0-9_]*)");
    Some(mul.captures(expr).map_or(base, |c| base * num(&c[1])))
}

/// `(a, b, …)` 那一段里的第一个实参（顶层逗号之前）。
fn first_arg(span: &str) -> &str {
    let mut depth = 0i32;
    for (i, c) in span.char_indices() {
        match c {
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => depth -= 1,
            ',' if depth == 1 => return &span[..i],
            _ => {}
        }
    }
    span
}

/// `at` 所在那条测试走虚拟时间：`#[tokio::test(start_paused = true)]`，或测试体里调了 `tokio::time::pause()`。
/// 往回找最近的一个测试属性，往后看到下一个测试属性为止。
fn paused_tokio(src: &str, at: usize) -> bool {
    let head = &src[..at];
    let t = head.rfind("#[test]");
    let Some(k) = head.rfind("#[tokio::test") else {
        return false;
    };
    if t.is_some_and(|t| t > k) {
        return false;
    }
    let end = src[at..]
        .find("#[test]")
        .into_iter()
        .chain(src[at..].find("#[tokio::test"))
        .min()
        .map_or(src.len(), |e| at + e);
    src[k..]
        .lines()
        .next()
        .is_some_and(|l| l.contains("start_paused"))
        || src[k..end].contains("time::pause()")
}

/// 从 `at` 起到语句末（`;`）那一段里有 `.is_err()`：等的是「这段时间里不许发生」（静窗），机器忙只会让它更弱。
fn quiet_window(src: &str, at: usize) -> bool {
    let end = src[at..].find(';').map_or(src.len(), |e| at + e);
    src[at..end].contains(".is_err()")
}

pub(super) fn rs_hits(src: &str) -> Vec<Hit> {
    let mut v = Vec::new();
    let bound = rs_bound(src);
    // rs-wait ①：等待调用的实参。
    let call = re!(
        r"\b(recv_timeout|wait_timeout|wait_timeout_ms|wait_timeout_while|time::timeout|set_read_timeout|set_write_timeout)\s*\("
    );
    for m in call.find_iter(src) {
        let arg = paren_span(src, m.end() - 1);
        // `time::timeout(d, fut)` 只看第一个实参（后面那个 future 里的数不是它的期限）。
        let arg = if m.as_str().starts_with("time::") {
            first_arg(arg)
        } else {
            arg
        };
        if paused_tokio(src, m.start()) || quiet_window(src, m.start()) {
            continue; // 虚拟时间 · 静窗
        }
        if !arg.contains(RS_CEILING) && rs_deadline_ms(arg, &bound).is_some_and(|ms| ms >= 1000.0) {
            v.push(Hit {
                form: "rs-wait",
                line: line_at(src, m.start()),
                text: line_text(src, m.start()),
            });
        }
    }
    // rs-wait ②：自己搭的期限 `Instant::now() + …` 与 `while t.elapsed() < …`。
    let plus = re!(r"Instant::now\(\)\s*\+\s*([^;{]*)");
    for c in plus.captures_iter(src) {
        let at = c.get(0).unwrap().start();
        if paused_tokio(src, at) {
            continue;
        }
        if !c[1].contains(RS_CEILING)
            && rs_deadline_ms(&c[1], &bound).is_some_and(|ms| ms >= 1000.0)
        {
            v.push(Hit {
                form: "rs-wait",
                line: line_at(src, at),
                text: line_text(src, at),
            });
        }
    }
    let lt = re!(r"\.elapsed\(\)(\.as_(secs|millis)\(\))?\s*<\s*([^{;]*)");
    for c in lt.captures_iter(src) {
        let at = c.get(0).unwrap().start();
        let head = &src[src[..at].rfind('\n').map_or(0, |i| i + 1)..at];
        if !head.contains("while") || paused_tokio(src, at) {
            continue; // 断言里的那一形归 rs-upper
        }
        let rhs = &c[3];
        let ms = match c.get(2).map(|m| m.as_str()) {
            Some("secs") => rhs
                .trim()
                .split(|ch: char| !ch.is_ascii_digit() && ch != '_')
                .next()
                .map(|n| num(n) * 1000.0),
            Some("millis") => rhs
                .trim()
                .split(|ch: char| !ch.is_ascii_digit() && ch != '_')
                .next()
                .map(num),
            _ => rs_deadline_ms(rhs, &bound),
        };
        if !rhs.contains(RS_CEILING) && ms.is_some_and(|ms| ms >= 1000.0) {
            v.push(Hit {
                form: "rs-wait",
                line: line_at(src, at),
                text: line_text(src, at),
            });
        }
    }
    // rs-upper：断言里拿量出来的耗时比上界。
    let took_re = re!(
        r"let\s+(?:mut\s+)?([a-z_][a-z0-9_]*)\s*(?::[^=;]*)?=\s*[^;]*(?:\.elapsed\(\)|duration_since\()"
    );
    let took: Vec<String> = took_re
        .captures_iter(src)
        .map(|c| c[1].to_string())
        .collect();
    let mut subj = vec![r"\.elapsed\(\)".to_string()];
    subj.extend(took.iter().map(|t| format!(r"\b{t}\b")));
    let upper = Regex::new(&format!(
        r"(?:{})[\w.()]*\s*<=?\s*([^,&|]*)",
        subj.join("|")
    ))
    .unwrap();
    let assert = re!(r"\b(?:debug_)?assert!\s*\(");
    for m in assert.find_iter(src) {
        let span = paren_span(src, m.end() - 1);
        if !span.ends_with(')') || span.len() > 4000 {
            continue; // 没配平（括号在字符串里）⇒ 这一段认不准，不猜
        }
        for c in upper.captures_iter(span) {
            let rhs = &c[1];
            let whole = c.get(0).unwrap();
            // 名字是路径的一段（`std::time::…`）不是那个绑定。
            let path_seg = span[..whole.start()].ends_with("::")
                || span[whole.start()..]
                    .split(|ch: char| !(ch.is_alphanumeric() || ch == '_'))
                    .next()
                    .is_some_and(|w| span[whole.start() + w.len()..].starts_with("::"));
            if path_seg || rhs.trim_start().starts_with('<') || rhs.contains(RS_CEILING) {
                continue;
            }
            let at = m.start() + c.get(0).unwrap().start();
            if paused_tokio(src, at) {
                continue;
            }
            v.push(Hit {
                form: "rs-upper",
                line: line_at(src, at),
                text: line_text(src, at),
            });
        }
    }
    v.sort_by_key(|h| h.line);
    v.dedup();
    v
}

/// `close` 指着的 `)` 是不是一条用例 / 钩子调用（`it(` · `test.each(…)(` · `beforeAll(` …）的收尾：
/// 往回配平到它的 `(`，看前面的调用名。`setTimeout(() => {…}, 0)` 这种不算。
fn closes_a_test_call(src: &str, close: usize) -> bool {
    let b = src.as_bytes();
    let mut depth = 0i32;
    let mut i = close as isize;
    while i >= 0 {
        match b[i as usize] {
            b')' => depth += 1,
            b'(' => {
                depth -= 1;
                if depth == 0 {
                    break;
                }
            }
            _ => {}
        }
        i -= 1;
    }
    if i <= 0 {
        return false;
    }
    let head = src[..i as usize].trim_end();
    if head.ends_with(')') {
        return true; // `it.each(rows)(…)` · `it.skipIf(x)(…)`
    }
    let name: String = head
        .chars()
        .rev()
        .take_while(|c| c.is_alphanumeric() || *c == '_' || *c == '.')
        .collect::<String>()
        .chars()
        .rev()
        .collect();
    let last = name.rsplit('.').next().unwrap_or("");
    let first = name.split('.').next().unwrap_or("");
    const CALLS: &[&str] = &[
        "it",
        "test",
        "describe",
        "suite",
        "bench",
        "beforeAll",
        "beforeEach",
        "afterAll",
        "afterEach",
    ];
    CALLS.contains(&first) || CALLS.contains(&last)
}

pub(super) fn ts_hits(src: &str) -> Vec<Hit> {
    let mut v = Vec::new();
    let mut push = |form: &'static str, at: usize| {
        v.push(Hit {
            form,
            line: line_at(src, at),
            text: line_text(src, at),
        })
    };
    let opt = re!(r"\b(timeout|testTimeout|hookTimeout)\s*:\s*([^,}\n]+)");
    for c in opt.captures_iter(src) {
        if !ts_ceiling(&c[2]) {
            push("ts-timeout", c.get(0).unwrap().start());
        }
    }
    let trailing = re!(r"(?m)^\s*\}\s*,\s*([0-9][0-9_]*|[A-Za-z_][A-Za-z0-9_]*)\s*,?\s*\)");
    for c in trailing.captures_iter(src) {
        let whole = c.get(0).unwrap();
        if !ts_ceiling(&c[1]) && closes_a_test_call(src, whole.end() - 1) {
            push("ts-trailing", whole.start());
        }
    }
    let waitfor = re!(r"\bvi\.(waitFor|waitUntil)\s*\(");
    for m in waitfor.find_iter(src) {
        if !ts_ceiling(paren_span(src, m.end() - 1)) {
            push("ts-waitfor", m.start());
        }
    }
    let took_re =
        re!(r"(?:const|let)\s+(\w+)\s*=\s*[^;\n]*(?:performance\.now\(\)|Date\.now\(\))\s*-\s*\w");
    for c in took_re.captures_iter(src) {
        let id = regex::escape(&c[1]);
        let cmp = Regex::new(&format!(
            r"\b{id}\s*<=?\s*([^,;)]+)|expect\(\s*{id}\b[^;]*?\.toBeLessThan(?:OrEqual)?\(([^)]*)\)"
        ))
        .unwrap();
        for k in cmp.captures_iter(src) {
            let rhs = k.get(1).or(k.get(2)).map_or("", |m| m.as_str());
            if !ts_ceiling(rhs) {
                push("ts-upper", k.get(0).unwrap().start());
            }
        }
    }
    v.sort_by_key(|h| h.line);
    v.dedup();
    v
}

pub(super) fn kt_hits(src: &str) -> Vec<Hit> {
    let mut v = Vec::new();
    let mut push = |form: &'static str, at: usize| {
        v.push(Hit {
            form,
            line: line_at(src, at),
            text: line_text(src, at),
        })
    };
    let test = re!(r"@Test\s*\(\s*timeout\s*=\s*([^)]*)\)");
    for c in test.captures_iter(src) {
        if !c[1].contains(KT_CEILING) {
            push("kt-test-timeout", c.get(0).unwrap().start());
        }
    }
    let wt = re!(r"\bwithTimeout(?:OrNull)?\s*\(");
    let scope = re!(r"\b(runTest|runBlocking)\b");
    for m in wt.find_iter(src) {
        let nearest = scope
            .find_iter(&src[..m.start()])
            .last()
            .map(|s| s.as_str());
        if nearest == Some("runBlocking") && !paren_span(src, m.end() - 1).contains(KT_CEILING) {
            push("kt-withtimeout", m.start());
        }
    }
    let wu = re!(r"\.waitUntil\b\s*(\([^)]*\))?");
    for c in wu.captures_iter(src) {
        if !c.get(1).is_some_and(|a| a.as_str().contains(KT_CEILING)) {
            push("kt-waituntil", c.get(0).unwrap().start());
        }
    }
    let dl = re!(r"System\.(?:currentTimeMillis|nanoTime)\(\)\s*\+\s*([^\n]*)");
    for c in dl.captures_iter(src) {
        if !c[1].contains(KT_CEILING) {
            push("kt-deadline", c.get(0).unwrap().start());
        }
    }
    v.sort_by_key(|h| h.line);
    v.dedup();
    v
}

pub(super) fn sh_hits(src: &str) -> Vec<Hit> {
    let mut v = Vec::new();
    let to = re!(
        r#"(?m)(?:(?:^|[\s;(|&!`])timeout|"\$TIMEOUT")\s+(?:-[a-zA-Z-]+(?:[ =]\S+)?\s+)*([0-9][0-9.]*[smh]?)\b"#
    );
    for c in to.captures_iter(src) {
        let at = c.get(1).unwrap().start();
        v.push(Hit {
            form: "sh-timeout",
            line: line_at(src, at),
            text: line_text(src, at),
        });
    }
    let lines: Vec<&str> = src.lines().collect();
    let seq = re!(
        r"\bfor\s+\w+\s+in\s+\$\(seq\b|\bfor\s*\(\(|\bfor\s+\w+\s+in\s+[0-9]+(?:\s+[0-9]+){2,}\s*;"
    );
    for (i, l) in lines.iter().enumerate() {
        if !seq.is_match(l) {
            continue;
        }
        let window = lines[i..(i + 4).min(lines.len())].join("\n");
        if window.contains("sleep") && !window.contains(SH_CEILING) {
            v.push(Hit {
                form: "sh-poll",
                line: i + 1,
                text: l.trim().to_string(),
            });
        }
    }
    // sh-upper：`X=$(( $(date …) - … ))` 一类量出来的耗时，后面拿 `-lt` / `-le` 比。
    let took =
        re!(r"(?m)\b([A-Za-z_][A-Za-z0-9_]*)=\$\(\([^\n]*(?:date \+%s|EPOCHREALTIME|c2ms)[^\n]*-");
    for c in took.captures_iter(src) {
        let id = regex::escape(&c[1]);
        let cmp = Regex::new(&format!(r#"\$\{{?{id}\}}?"?\s+-(?:lt|le)\b"#)).unwrap();
        for m in cmp.find_iter(src) {
            v.push(Hit {
                form: "sh-upper",
                line: line_at(src, m.start()),
                text: line_text(src, m.start()),
            });
        }
    }
    v.sort_by_key(|h| h.line);
    v.dedup();
    v
}

/// 从 `open`（指着 `{`）起到配平的 `}` 为止那一段；不配平就取到文件尾。
fn brace_span(src: &str, open: usize) -> &str {
    let mut depth = 0i32;
    for (i, c) in src[open..].char_indices() {
        match c {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return &src[open..open + i + 1];
                }
            }
            _ => {}
        }
    }
    &src[open..]
}

/// rs-poll：`for _ in 0..N { … break … sleep(…) }` / `(0..N).any(|_| { … sleep(…) … })`：拿圈数 × 睡眠当期限
/// （等的是圈里那个条件；圈数改成 `guard_core::wall::rounds()`，到兜底的那个数才停）。
/// 圈里没有 `break` / `return` 的不是在等什么（固定做几遍），不算。虚拟时间里的不算。
pub(super) fn rs_poll_hits(src: &str) -> Vec<Hit> {
    let masked = crate::mask_all_literals(src).unwrap_or_else(|| src.to_string());
    let sleep = re!(r"\bsleep\s*\(");
    let mut v = Vec::new();
    let mut push = |at: usize| {
        v.push(Hit {
            form: "rs-poll",
            line: line_at(src, at),
            text: line_text(src, at),
        })
    };
    let for_n = re!(r"\bfor\s+_\w*\s+in\s+0\s*\.\.=?\s*[0-9][0-9_]*\s*\{");
    for m in for_n.find_iter(&masked) {
        let body = brace_span(&masked, m.end() - 1);
        if sleep.is_match(body)
            && re!(r"\bbreak\b|\breturn\b").is_match(body)
            && !paused_tokio(src, m.start())
        {
            push(m.start());
        }
    }
    let any_n =
        re!(r"\(\s*0\s*\.\.=?\s*[0-9][0-9_]*\s*\)\s*\.\s*(?:any|find|find_map|position|all)\s*\(");
    for m in any_n.find_iter(&masked) {
        let body = paren_span(&masked, m.end() - 1);
        if sleep.is_match(body) && !paused_tokio(src, m.start()) {
            push(m.start());
        }
    }
    v
}

// ───────────────────── 睡一会儿再断言（10-11 tools2 补） ─────────────────────

/// `at` 往外一层层找包着它的 `{`（在抹掉串与注释的文本上数括号），返回每一层 `{` 前面那段头
/// （从上一个 `;` / `{` / `}` 到这个 `{`），由里往外；碰到函数 / 闭包 / async 块那一层为止（含它）。
fn rs_enclosing_heads(masked: &str, at: usize) -> Vec<String> {
    let b = masked.as_bytes();
    let mut heads = Vec::new();
    let mut i = at;
    loop {
        let mut depth = 0i32;
        let mut open = None;
        while i > 0 {
            i -= 1;
            match b[i] {
                b'}' => depth += 1,
                b'{' => {
                    if depth == 0 {
                        open = Some(i);
                        break;
                    }
                    depth -= 1;
                }
                _ => {}
            }
        }
        let Some(o) = open else { return heads };
        let from = masked[..o].rfind([';', '{', '}']).map_or(0, |k| k + 1);
        let head = masked[from..o].to_string();
        let stop = re!(r"\bfn\b|\||\basync\b").is_match(&head);
        heads.push(head);
        if stop {
            return heads;
        }
        i = o;
    }
}

/// `from` 起（一条语句的末尾之后）同一块里往后至多 `n` 句（块收尾就停）。
fn rs_next_statements(masked: &str, from: usize, n: usize) -> &str {
    let mut depth = 0i32;
    let mut semis = 0;
    for (k, c) in masked[from..].char_indices() {
        match c {
            '{' | '(' | '[' => depth += 1,
            '}' | ')' | ']' => {
                depth -= 1;
                if depth < 0 {
                    return &masked[from..from + k];
                }
            }
            ';' if depth == 0 => {
                semis += 1;
                if semis >= n {
                    return &masked[from..from + k];
                }
            }
            _ => {}
        }
    }
    &masked[from..]
}

/// rs-sleep：不在轮询圈里的一次 `sleep(…)`（`thread::sleep` · `tokio::time::sleep` · 绑成 `sleep` 的闭包），
/// 同一块里往后五句之内有断言 ⇒ 「睡一会儿再断言」。虚拟时间（`start_paused` / `time::pause()`）里的不算。
pub(super) fn rs_sleep_hits(src: &str) -> Vec<Hit> {
    let masked = crate::mask_all_literals(src).unwrap_or_else(|| src.to_string());
    let call = re!(r"\bsleep\s*\(");
    let poll = re!(r"\b(loop|while|for)\b");
    let assert = re!(r"\b(?:debug_|prop_)?assert(?:_eq|_ne|_matches)?!|\bpanic!");
    let mut v = Vec::new();
    for m in call.find_iter(&masked) {
        let ls = masked[..m.start()].rfind('\n').map_or(0, |k| k + 1);
        let before = &masked[ls..m.start()];
        // 定义处（`fn sleep(` · `let sleep = |ms| …sleep(…)` 那一行）不是一次调用。
        if before.trim_end().ends_with("fn") || re!(r"=\s*(?:move\s*)?\|").is_match(before) {
            continue;
        }
        if paused_tokio(src, m.start()) {
            continue;
        }
        if rs_enclosing_heads(&masked, m.start())
            .iter()
            .any(|h| poll.is_match(h))
        {
            continue; // 轮询圈里的一片：等的是圈的条件，上限归 rs-wait 那几形
        }
        let span = paren_span(&masked, m.end() - 1);
        let after = m.end() - 1 + span.len();
        let end = masked[after..]
            .find(';')
            .map_or(masked.len(), |e| after + e + 1);
        if assert.is_match(rs_next_statements(&masked, end, 5)) {
            v.push(Hit {
                form: "rs-sleep",
                line: line_at(src, m.start()),
                text: line_text(src, m.start()),
            });
        }
    }
    v
}

/// 一行里 `at` 之前单引号 / 双引号各出现几次是奇数 ⇒ `at` 在引号里。
fn sh_in_quotes(line: &str, at: usize) -> bool {
    let head = &line[..at];
    head.matches('\'').count() % 2 == 1 || head.matches('"').count() % 2 == 1
}

/// 脚本里每一圈 `do … done` 的行区间（含两头），与那一圈是不是轮询（体里有 `break` / `return`，或是 `until` / `while` 圈）。
fn sh_loops(lines: &[&str]) -> Vec<(usize, usize, bool)> {
    let word = re!(r"(?:^|[\s;&|])(do|done|until|while)(?:$|[\s;&|])");
    let mut stack: Vec<(usize, bool)> = Vec::new();
    let mut out = Vec::new();
    let mut cond = false;
    for (i, l) in lines.iter().enumerate() {
        let mut k = 0;
        while let Some(c) = word.captures_at(l, k) {
            let w = c.get(1).unwrap();
            k = w.end();
            if sh_in_quotes(l, w.start()) {
                continue;
            }
            match w.as_str() {
                "until" | "while" => cond = true,
                "do" => {
                    stack.push((i, cond));
                    cond = false;
                }
                _ => {
                    if let Some((a, c)) = stack.pop() {
                        let body = lines[a..=i].join("\n");
                        let poll = c || re!(r"\bbreak\b|\breturn\b").is_match(&body);
                        out.push((a, i, poll));
                    }
                }
            }
        }
    }
    out
}

/// sh-sleep：前台的 `sleep N`（不在引号里、不是 `… &` 放后台、不是 `<(sleep …)` 那根管子），不在轮询圈里 ⇒
/// 「睡一会儿再看」。e2e 脚本里 sleep 之后就是去看结果（下一条命令、下一个用例），不再分后面是不是断言。
pub(super) fn sh_sleep_hits(src: &str) -> Vec<Hit> {
    let lines: Vec<&str> = src.lines().collect();
    let loops = sh_loops(&lines);
    let cmd = re!(r#"(?:^|[;&|]|\bthen|\bdo|\belse|\{)\s*sleep\s+("?\$\{?\w+\}?"?|[0-9][0-9.]*)"#);
    let heredoc = re!(r#"<<-?\s*['"]?([A-Za-z_][A-Za-z0-9_]*)['"]?"#);
    let mut v = Vec::new();
    let mut doc_end: Option<String> = None;
    for (i, l) in lines.iter().enumerate() {
        // here-doc 的正文是写进文件 / 喂给命令的字，不是这份脚本自己跑的命令。
        if let Some(end) = &doc_end {
            if l.trim() == end {
                doc_end = None;
            }
            continue;
        }
        if let Some(c) = heredoc.captures(l) {
            if !sh_in_quotes(l, c.get(0).unwrap().start()) {
                doc_end = Some(c[1].to_string());
            }
        }
        for c in cmd.captures_iter(l) {
            let at = c.get(0).unwrap().start();
            let end = c.get(0).unwrap().end();
            let pre = &l[..at];
            if sh_in_quotes(l, at)
                || pre.ends_with("<(")
                || pre.ends_with("$(")
                || re!(r"^\s*&(?:[^&]|$)").is_match(&l[end..])
            {
                continue;
            }
            let inner = loops
                .iter()
                .filter(|(a, b, _)| *a <= i && i <= *b)
                .min_by_key(|(a, b, _)| b - a);
            if inner.is_some_and(|(_, _, poll)| *poll) {
                continue;
            }
            v.push(Hit {
                form: "sh-sleep",
                line: i + 1,
                text: l.trim().to_string(),
            });
        }
    }
    v
}

fn hits_for(lang: &str, src: &str) -> Vec<Hit> {
    match lang {
        "rs" => [rs_hits(src), rs_poll_hits(src), rs_sleep_hits(src)].concat(),
        "ts" => ts_hits(src),
        "kt" => kt_hits(src),
        "sh" => [sh_hits(src), sh_sleep_hits(src)].concat(),
        _ => unreachable!(),
    }
}

// ───────────────────────────── 判据 ─────────────────────────────

fn all_hits() -> (usize, Vec<(String, Hit)>) {
    let mut n = 0;
    let mut v = Vec::new();
    for rel in files() {
        let Some(lang) = lang_of(&rel) else { continue };
        n += 1;
        let raw = read(&rel);
        for h in hits_for(lang, &body(&rel, lang, &raw)) {
            v.push((rel.clone(), h));
        }
    }
    (n, v)
}

#[test]
fn w2_no_wall_clock_bound_in_the_gated_tests_but_the_ceiling() {
    let (n, hits) = all_hits();
    assert!(n > 1000, "人群不空（读到 {n} 份测试源）");
    let mut used = vec![false; EXEMPT.len()];
    let mut bad = Vec::new();
    for (rel, h) in &hits {
        let ex = EXEMPT.iter().position(|(f, form, needle, _)| {
            f == rel && *form == h.form && h.text.contains(needle)
        });
        match ex {
            Some(i) => used[i] = true,
            None => bad.push(format!("{rel}:{} [{}] {}", h.line, h.form, h.text)),
        }
    }
    let dead: Vec<_> = EXEMPT
        .iter()
        .zip(&used)
        .filter(|(_, u)| !**u)
        .map(|(e, _)| format!("{} [{}] {}", e.0, e.1, e.2))
        .collect();
    assert!(
        dead.is_empty(),
        "例外表里这几行一处都没命中（死行，删掉）：\n  {}",
        dead.join("\n  ")
    );
    let forms: BTreeMap<&str, &str> = FORMS.iter().map(|(f, _, why)| (*f, *why)).collect();
    assert!(
        bad.is_empty(),
        "{} 处按墙钟判的上限不是那个兜底的数（{} 秒，`guard_core::wall::CEILING` / TS·Kotlin `WALL_CEILING_MS` / e2e `$WALL_CEILING_S`）。\n\
         判快慢的改计步数 / 计调用次数 / 注入钟，真看速度的搬进 `tests/benches/`；只是兜底的换成那个数。\n\
         几形的意思：\n  {}\n命中：\n  {}",
        bad.len(),
        crate::wall::CEILING_SECS,
        forms.iter().map(|(f, w)| format!("{f}：{w}")).collect::<Vec<_>>().join("\n  "),
        bad.join("\n  ")
    );
}

#[test]
fn w1_the_ceiling_is_one_number_everywhere() {
    let secs = crate::wall::CEILING_SECS;
    assert_eq!(crate::wall::CEILING, std::time::Duration::from_secs(secs));
    let ms_def = re!(r"WALL_CEILING_MS\s*(?::\s*\w+\s*)?=\s*([0-9][0-9_]*)L?\b");
    let ts = read(TS_CEILING_FILE);
    let got: Vec<f64> = ms_def.captures_iter(&ts).map(|c| num(&c[1])).collect();
    assert_eq!(
        got,
        vec![(secs * 1000) as f64],
        "{TS_CEILING_FILE} 里 `WALL_CEILING_MS` 恰好定义一次、等于 {secs} 秒"
    );
    let cfg = read(VITEST_CONFIG);
    for key in ["testTimeout", "hookTimeout"] {
        assert!(
            Regex::new(&format!(r"\b{key}\s*:\s*WALL_CEILING_MS\b")).unwrap().is_match(&cfg),
            "{VITEST_CONFIG} 的 `{key}` 不是 `WALL_CEILING_MS`（vitest 默认 5 秒 / 10 秒，忙机上会撞）"
        );
    }
    let mut kt = Vec::new();
    for rel in files() {
        // 截图台架不在门禁里，但它等页面起好用的也是这一个数（发版前与合并时手跑）。
        if lang_of(&rel) == Some("kt") || rel == SHOTS_RUNNER {
            for c in ms_def.captures_iter(&read(&rel)) {
                kt.push((rel.clone(), num(&c[1])));
            }
        }
    }
    assert!(
        !kt.is_empty(),
        "Kotlin 测试源里一处 `WALL_CEILING_MS` 都没定义 —— 下面那条恒真"
    );
    let wrong: Vec<_> = kt
        .iter()
        .filter(|(_, v)| *v != (secs * 1000) as f64)
        .collect();
    assert!(
        wrong.is_empty(),
        "Kotlin 里这几处 `WALL_CEILING_MS` 不是 {secs} 秒：{wrong:?}"
    );
    let sh = read(SH_ENV);
    let sh_def = re!(r"(?m)^\s*(?:export\s+)?WALL_CEILING_S=([0-9]+)\s*$");
    let got: Vec<f64> = sh_def.captures_iter(&sh).map(|c| num(&c[1])).collect();
    assert_eq!(
        got,
        vec![secs as f64],
        "{SH_ENV} 里 `WALL_CEILING_S` 恰好定义一次、等于 {secs}"
    );
}

#[test]
fn w3_every_e2e_script_that_reads_the_ceiling_sources_the_sandbox_env() {
    let mut users = 0;
    let mut bad = Vec::new();
    for rel in files() {
        if lang_of(&rel) != Some("sh") || rel == SH_ENV {
            continue;
        }
        let src = read(&rel);
        if src.contains(SH_CEILING) {
            users += 1;
            if !src.contains("sandbox-env.sh") {
                bad.push(rel);
            }
        }
    }
    assert!(
        users > 0,
        "没有一份 e2e 脚本用到 `{SH_CEILING}` —— 本条恒真"
    );
    assert!(
        bad.is_empty(),
        "这几份用了 `{SH_CEILING}` 却没 source `sandbox-env.sh`：{bad:?}"
    );
}

// ───────────────────────────── 认法自检 ─────────────────────────────

fn forms_of(h: &[Hit]) -> Vec<&'static str> {
    h.iter().map(|h| h.form).collect()
}

#[test]
fn w4_rust_forms_are_told_apart() {
    let red = "fn a() {\n\
        let _ = rx.recv_timeout(Duration::from_secs(5)).unwrap();\n\
        let d = std::time::Instant::now() + Duration::from_millis(2_000);\n\
        let w = Duration::from_millis(200);\n\
        let _ = rx.recv_timeout(w * 20);\n\
        while t0.elapsed().as_secs() < 20 {}\n\
        let took = t0.elapsed();\n\
        assert!(took >= Duration::from_millis(600) && took < Duration::from_secs(3), \"x\");\n\
        assert!(t1.elapsed() < Duration::from_secs(1));\n\
        let f = tokio::time::timeout(std::time::Duration::from_secs(5), async { sleep(Duration::from_millis(10)) });\n\
        c.set_read_timeout(Some(Duration::from_secs(10)));\n\
    }\n";
    assert_eq!(
        forms_of(&rs_hits(red)),
        vec![
            "rs-wait", "rs-wait", "rs-wait", "rs-wait", "rs-upper", "rs-upper", "rs-wait",
            "rs-wait"
        ]
    );
    let paused = "#[tokio::test(start_paused = true)]\nasync fn p() { let _ = tokio::time::timeout(Duration::from_secs(5), x).await; }\n\
        #[test]\nfn q() { let _ = rx.recv_timeout(Duration::from_secs(5)); }\n";
    assert_eq!(
        forms_of(&rs_hits(paused)),
        vec!["rs-wait"],
        "虚拟时间那条不算、真钟那条算"
    );
    let green = "fn a() {\n\
        let _ = rx.recv_timeout(guard_core::wall::CEILING).unwrap();\n\
        assert!(rx.recv_timeout(Duration::from_millis(400)).is_err());\n\
        let d = std::time::Instant::now() + guard_core::wall::CEILING;\n\
        let took = t0.elapsed();\n\
        assert!(took >= Duration::from_millis(600));\n\
        assert!(took < guard_core::wall::CEILING, \"x\");\n\
        assert_eq!(next_backoff(Duration::from_secs(2)), Duration::from_secs(4));\n\
    }\n";
    assert_eq!(rs_hits(green), vec![]);
}

#[test]
fn w4_ts_forms_are_told_apart() {
    let red = "it(\"a\", { timeout: 30_000 }, async () => {});\n\
        it(\"b\", () => {\n\
        }, 60_000);\n\
        await vi.waitFor(() => expect(x).toBe(1));\n\
        const dt = Date.now() - t0;\n\
        ok(dt < 3000, \"slow\");\n";
    assert_eq!(
        forms_of(&ts_hits(red)),
        vec!["ts-timeout", "ts-trailing", "ts-waitfor", "ts-upper"]
    );
    let green = "it(\"a\", { timeout: WALL_CEILING_MS }, async () => {});\n\
        await vi.waitFor(() => expect(x).toBe(1), { timeout: WALL_CEILING_MS });\n\
        for (let i = 0; i < 4; i++) await new Promise((r) => setTimeout(r, 0));\n\
        const ms = performance.now() - t0;\n\
        console.log(ms);\n\
        }, [a, b]);\n\
        setTimeout(() => {\n\
        }, 0);\n\
        it.each(rows)(\"x\", () => {\n\
        });\n";
    assert_eq!(ts_hits(green), vec![]);
    assert_eq!(
        forms_of(&ts_hits("it.skipIf(!x)(\"a\", () => {\n}, 5_000);\n")),
        vec!["ts-trailing"]
    );
}

#[test]
fn w4_kotlin_forms_are_told_apart() {
    let red = "@Test(timeout = 15_000L)\nfun a() = runBlocking { withTimeout(5_000L) { x() } }\n\
        composeRule.waitUntil(timeoutMillis = 15_000) { y }\ncomposeRule.waitUntil { y }\n\
        val deadline = System.currentTimeMillis() + 5000\n";
    assert_eq!(
        forms_of(&kt_hits(red)),
        vec![
            "kt-test-timeout",
            "kt-withtimeout",
            "kt-waituntil",
            "kt-waituntil",
            "kt-deadline"
        ]
    );
    let green = "@Test(timeout = WALL_CEILING_MS)\nfun a() = runBlocking { withTimeout(WALL_CEILING_MS) { x() } }\n\
        fun b() = runTest { withTimeout(1_000) { y() } }\n\
        composeRule.waitUntil(timeoutMillis = WALL_CEILING_MS) { y }\n";
    assert_eq!(kt_hits(green), vec![]);
}

#[test]
fn w4_shell_forms_are_told_apart() {
    let red = "timeout 30 \"$X\" a\nout=$(timeout 4 script -q)\nfor i in $(seq 1 200); do grep -q x f && break; sleep 0.05; done\n\
        \"$TIMEOUT\" 6 \"$D\"\nfor ((i=0; i<to*2; i++)); do sleep 0.5; done\n\
        _ms=$(( ($(date +%s%N) - _t0) / 1000000 ))\n[ \"$_ms\" -lt 5000 ] && echo y\n";
    assert_eq!(
        forms_of(&sh_hits(red)),
        vec![
            "sh-timeout",
            "sh-timeout",
            "sh-poll",
            "sh-timeout",
            "sh-poll",
            "sh-upper"
        ]
    );
    let green = "timeout \"$WALL_CEILING_S\" \"$X\" a\nfor i in $(seq 30); do echo $i; done\n# timeout 5 x\n\
        for ((i=0; i<WALL_CEILING_S*2; i++)); do sleep 0.5; done\nwall_until '[ -f x ]'\n";
    assert_eq!(sh_hits(&body("tests/e2e/x.sh", "sh", green)), vec![]);
}

#[test]
fn w4_sleep_then_assert_is_told_apart() {
    let red = "fn a() {\n\
        std::thread::sleep(Duration::from_millis(50));\n\
        assert!(done.load(SeqCst));\n\
    }\n\
    #[tokio::test]\nasync fn b() {\n\
        tokio::time::sleep(Duration::from_millis(5)).await;\n\
        let n = rx.len();\n\
        assert_eq!(n, 0, \"{}\", \"x\");\n\
    }\n\
    fn c() {\n\
        let sleep = |ms: u64| std::thread::sleep(Duration::from_millis(ms));\n\
        sleep(300);\n\
        assert_ne!(a, b);\n\
    }\n";
    assert_eq!(
        forms_of(&rs_sleep_hits(red)),
        vec!["rs-sleep", "rs-sleep", "rs-sleep"]
    );
    let green = "fn a() {\n\
        loop {\n\
            if ok() { break; }\n\
            std::thread::sleep(Duration::from_millis(20));\n\
        }\n\
        assert!(ok());\n\
        while t0.elapsed() < CEILING { match x() { Some(_) => std::thread::sleep(Duration::from_millis(5)), None => break } }\n\
        assert!(y);\n\
        let h = std::thread::spawn(move || { std::thread::sleep(Duration::from_millis(50)); tx.send(1).unwrap(); });\n\
        assert!(z);\n\
        std::thread::sleep(Duration::from_millis(50));\n\
        h.join().unwrap();\n\
    }\n\
    #[tokio::test(start_paused = true)]\nasync fn p() { tokio::time::sleep(Duration::from_secs(5)).await; assert!(fired()); }\n\
    fn d() { let s = \"sleep(1); assert!(x)\"; }\n";
    assert_eq!(rs_sleep_hits(green), vec![]);
}

#[test]
fn w4_rust_poll_is_told_apart() {
    let red = "fn a() {\n\
        for _ in 0..600 {\n\
            if done() { break; }\n\
            std::thread::sleep(Duration::from_millis(5));\n\
        }\n\
        let gone = (0..200).any(|_| { if x() { return true; } std::thread::sleep(Duration::from_millis(5)); false });\n\
    }\n";
    assert_eq!(forms_of(&rs_poll_hits(red)), vec!["rs-poll", "rs-poll"]);
    let green = "fn a() {\n\
        for _ in guard_core::wall::rounds() {\n\
            if done() { break; }\n\
            std::thread::sleep(Duration::from_millis(5));\n\
        }\n\
        for _ in 0..30 { press(); std::thread::sleep(Duration::from_millis(167)); }\n\
        for _ in 0..3 { if x() { break; } }\n\
    }\n\
    #[tokio::test(start_paused = true)]\nasync fn p() { for _ in 0..600 { if y() { break; } tokio::time::sleep(Duration::from_millis(5)).await; } }\n";
    assert_eq!(rs_poll_hits(green), vec![]);
}

#[test]
fn w4_shell_sleep_is_told_apart() {
    let red =
        "fire; sleep 1\n[ -f x ] || bad\nreset() { T kill-server; sleep 0.3; }\nsleep \"$GAP\"\n\
        for c in a b; do run \"$c\"; sleep 0.5; check \"$c\"; done\nif x; then sleep 2; fi\n";
    assert_eq!(
        forms_of(&sh_sleep_hits(red)),
        vec!["sh-sleep", "sh-sleep", "sh-sleep", "sh-sleep", "sh-sleep"]
    );
    let green = "tmux new-session -d -s a 'sleep 300'\nsleep 60 & pid=$!\nd --x < <(sleep 30) >o\n\
        for _i in $(seq 1 WALL_CEILING_S); do [ -s f ] && break; sleep 0.3; done\n\
        until [ -f x ]; do\n  sleep 0.05\ndone\nexec sleep 30\nprintf 'exec sleep 300\\n' > f\n\
        while :; do\n  grep -q y f && return 0\n  sleep 0.1\ndone\n\
        cat > f << 'EOF'\nsleep 300\nEOF\n";
    assert_eq!(sh_sleep_hits(green), vec![]);
    assert_eq!(
        forms_of(&sh_hits(
            "for _i in 1 2 3 4 5; do [ -s f ] && break; sleep 0.3; done\n"
        )),
        vec!["sh-poll"],
        "拿字面的圈数当期限也是那一形"
    );
}

#[test]
fn w4_the_population_is_what_the_gate_runs() {
    assert_eq!(lang_of("tests/frontend/ui/a.vitest.ts"), Some("ts"));
    assert_eq!(lang_of("tests/frontend/ui/cards/diff.test.ts"), Some("ts"));
    assert_eq!(lang_of("vitest.config.ts"), Some("ts"));
    assert_eq!(lang_of("tests/backend/x_tests.rs"), Some("rs"));
    assert_eq!(lang_of("src/backend/main.rs"), Some("rs"));
    assert_eq!(
        lang_of("src/mobile/app/src/androidTest/kotlin/A.kt"),
        Some("kt")
    );
    assert_eq!(lang_of("src/mobile/app/src/main/kotlin/A.kt"), None);
    assert_eq!(lang_of("tests/e2e/cc-spawn-uplift.sh"), Some("sh"));
    assert_eq!(lang_of("tests/e2e/f40-suite.sh"), None);
    for (p, _) in SH_NOT_GATED {
        assert!(
            super::no_outside_refs::repo().join(p).exists(),
            "不跑表里的 {p} 还在"
        );
    }
    assert_eq!(lang_of("tests/evidence/RT1-relay-bench.rs"), None);
    assert_eq!(lang_of("tests/benches/s7_history_read.rs"), None);
    // 要台架的设备测（没给台架参数就整份跳过）照样在人群里：设备测入口不再按类过滤。
    const RIG_DEVICE_TEST: &str =
        "src/mobile/core-ssh/src/androidTest/kotlin/com/ccmonitor/mobile/core/ssh/RustTransportTest.kt";
    assert_eq!(lang_of(RIG_DEVICE_TEST), Some("kt"));
    assert!(
        Path::new(&super::no_outside_refs::repo().join(RIG_DEVICE_TEST)).is_file(),
        "要台架的那份设备测还在"
    );
}
