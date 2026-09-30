//! 〔SX1 · J1〕`history-search` 的应答 == 起步树 `e7d4872d` 现扫实现冻结下来的金样，逐问逐行逐字节（题面「结果与现扫逐条相等（两向）」）。
//! 异源：金样在 SX1 子步 1（生产代码未改）由旧现扫跑出入库；改语料 / 查询 ⇒ 必须回到一份现扫实现上重跑，不许拿新实现去「更新」它。
//! 语料全合成，逐格覆盖旧路逐行那段的分支（BOM · CRLF · 坏行 · 标题先后 · 没写完的末行 · 读不动 · 超 30 条 · 深度 1 / 3 …）。

use super::*;
use serde_json::json;

/// 一个时间点（毫秒）；mtime 都从它往后排、两两不同。
const T0: i64 = 1_700_000_000_000;

fn at(ms: i64) -> std::time::SystemTime {
    std::time::UNIX_EPOCH + std::time::Duration::from_millis(ms as u64)
}

fn put(home: &Path, rel: &str, bytes: &[u8], mtime_ms: i64) {
    let p = home.join("projects").join(rel);
    std::fs::create_dir_all(p.parent().expect("有父目录")).expect("建目录");
    std::fs::write(&p, bytes).expect("写夹具");
    let f = std::fs::File::options()
        .write(true)
        .open(&p)
        .expect("打开夹具改 mtime");
    f.set_times(
        std::fs::FileTimes::new()
            .set_accessed(at(mtime_ms))
            .set_modified(at(mtime_ms)),
    )
    .expect("set_times");
}

fn user(uuid: &str, ts: &str, text: &str) -> String {
    json!({"type":"user","uuid":uuid,"timestamp":ts,"message":{"role":"user","content":text}})
        .to_string()
}

fn assistant(uuid: &str, ts: &str, blocks: Value) -> String {
    json!({"type":"assistant","uuid":uuid,"timestamp":ts,"message":{"role":"assistant","content":blocks}}).to_string()
}

/// 建语料，回家目录（`<tmp>/…/`，其下 `projects/`）。
pub(super) fn build_corpus(tag: &str) -> std::path::PathBuf {
    let home = std::env::temp_dir().join(format!("ccm-sx1-{tag}-{}", std::process::id()));
    std::fs::remove_dir_all(&home).ok();

    // a1：分支最全的那一份。文件头 BOM；cwd 迟到；三种标题先后；CRLF；坏 JSON；非对象；没 uuid；末行完整但缺 `\n`。
    let mut a1 = String::from("\u{feff}");
    a1.push_str(&json!({"type":"summary","summary":"structural only"}).to_string());
    a1.push('\n');
    a1.push_str(&user(
        "a1-u1",
        "2026-01-01T00:00:01.5Z",
        "<system-reminder>注入噪声 docker</system-reminder>Deploy the Docker stack",
    ));
    a1.push('\n');
    a1.push_str(&json!({"type":"ai-title","aiTitle":"标题一"}).to_string());
    a1.push('\n');
    a1.push_str(
        &json!({"type":"assistant","uuid":"a1-a1","timestamp":"2026-01-01T00:00:02Z","cwd":"/work/alpha",
            "message":{"role":"assistant","content":[
                {"type":"text","text":"Running docker compose up"},
                {"type":"tool_use","name":"Bash","input":{"command":"docker compose up -d"}},
                {"type":"thinking","thinking":"think about kubernetes"}]}})
        .to_string(),
    );
    a1.push('\n');
    a1.push_str(
        &json!({"type":"user","uuid":"a1-u2","timestamp":"2026-01-01T00:00:03Z",
            "message":{"role":"user","content":[{"type":"tool_result","content":"container started: kubernetes-free"}]}})
        .to_string(),
    );
    a1.push_str("\r\n");
    a1.push_str("\n   \t \n");
    a1.push_str("{\"type\":\"user\", broken\n");
    a1.push_str("[1,2,3]\n\"just a string\"\n");
    a1.push_str(&json!({"type":"custom-title","customTitle":"自定义标题"}).to_string());
    a1.push('\n');
    a1.push_str(&user(
        "a1-u3",
        "2026-03-01T00:00:00Z",
        "设计 文档 第二节 Docker",
    ));
    a1.push('\n');
    a1.push_str(&json!({"type":"ai-title","aiTitle":"最后的标题"}).to_string());
    a1.push('\n');
    a1.push_str(
        &json!({"type":"user","message":{"role":"user","content":"no uuid docker here"}})
            .to_string(),
    );
    a1.push('\n');
    a1.push_str(&assistant(
        "a1-a9",
        "2026-03-02T00:00:00Z",
        json!([{"type":"text","text":"tail line mentions Docker again"}]),
    ));
    put(&home, "p-alpha/a1.jsonl", a1.as_bytes(), T0 + 9_000);

    // b1：单会话超 `PER_SESSION_CAP`；第一行 cwd 是空串（不算），第二行才有。
    let mut b1 = String::new();
    for i in 0..(crate::observe::search_rules::PER_SESSION_CAP + 5) {
        let mut v = json!({"type":"user","uuid":format!("b1-u{i}"),
            "timestamp":format!("2026-02-{:02}T00:00:00Z", 1 + i % 28),
            "message":{"role":"user","content":format!("docker 第 {i} 条")}});
        if i == 0 {
            v["cwd"] = json!("");
        } else if i == 1 {
            v["cwd"] = json!("/work/beta");
        }
        b1.push_str(&v.to_string());
        b1.push('\n');
    }
    put(&home, "p-beta/b1.jsonl", b1.as_bytes(), T0 + 8_000);

    // b2：只有工具里有 kubernetes；首条 user 正文很长（摘要截断）；没有标题 ⇒ 标题取摘要。
    let long = "这是一段很长的开场白 ".repeat(30);
    let mut b2 = String::new();
    b2.push_str(&user("b2-u1", "2026-01-05T00:00:00Z", &long));
    b2.push('\n');
    b2.push_str(&assistant(
        "b2-a1",
        "2026-01-05T00:00:01Z",
        json!([{"type":"text","text":"plain answer"},
               {"type":"tool_use","name":"Bash","input":{"command":"kubectl get pods # kubernetes"}}]),
    ));
    b2.push('\n');
    put(&home, "p-beta/b2.jsonl", b2.as_bytes(), T0 + 7_000);

    // g1：中间一行不是合法 UTF-8 ⇒ 整份读不动。
    let mut g1 = user("g1-u1", "2026-01-01T00:00:00Z", "docker in g1").into_bytes();
    g1.extend_from_slice(b"\n\xff\xfe docker\n");
    put(&home, "p-gamma/g1.jsonl", &g1, T0 + 6_000);

    // g2：只有 assistant（摘要空 ⇒ 标题取 sid 前缀）＋ 撕裂的半行（坏 JSON，缺 `\n`）。
    let mut g2 = assistant(
        "g2-a1",
        "2026-01-02T00:00:00Z",
        json!([{"type":"text","text":"Docker only here"}]),
    );
    g2.push('\n');
    g2.push_str(r#"{"type":"user","uuid":"g2-u2","message":{"role":"user","content":"docker tor"#);
    put(&home, "p-gamma/g2.jsonl", g2.as_bytes(), T0 + 5_000);

    // g3：前面都好，末行撕在一个多字节字符中间（`中` 只写了两个字节）⇒ 整份不是合法 UTF-8 ⇒ 读不动。
    let mut g3 = user("g3-u1", "2026-01-03T00:00:00Z", "docker in g3").into_bytes();
    g3.push(b'\n');
    g3.extend_from_slice(br#"{"type":"user","message":{"content":""#);
    g3.extend_from_slice(&"中".as_bytes()[..2]);
    put(&home, "p-gamma/g3.jsonl", &g3, T0 + 4_000);

    // 深度 1：`projects/` 下直接一份（在人群里）。
    let mut top = user("top-u1", "2026-01-04T00:00:00Z", "top level docker");
    top.push('\n');
    put(&home, "top.jsonl", top.as_bytes(), T0 + 3_000);

    // 不在人群里：非 `.jsonl` · 深度 3。
    put(&home, "p-alpha/notes.txt", b"docker docker", T0 + 2_900);
    let mut deep = user("deep-u1", "2026-01-04T00:00:00Z", "deep docker");
    deep.push('\n');
    put(
        &home,
        "p-alpha/a1/subagents/deep.jsonl",
        deep.as_bytes(),
        T0 + 2_800,
    );

    // 空文件；行首 BOM ＋ 全 CRLF。
    put(&home, "p-delta/empty.jsonl", b"", T0 + 2_000);
    let mut crlf = user("c-u1", "2026-01-06T00:00:00Z", "first Docker line");
    crlf.push_str("\r\n\u{feff}");
    crlf.push_str(&user("c-u2", "2026-01-06T00:00:01Z", "second docker line"));
    crlf.push_str("\r\n");
    put(&home, "p-delta/crlf.jsonl", crlf.as_bytes(), T0 + 1_000);

    home
}

/// 查询清单：`(名字, 查询, --search 之后那一截 argv)`。
pub(super) fn cases() -> Vec<(&'static str, &'static str, Vec<String>)> {
    let a = |xs: &[&str]| xs.iter().map(|s| s.to_string()).collect::<Vec<_>>();
    let after = crate::observe::search_query::parse_iso8601_ms("2026-02-15T00:00:00Z")
        .expect("解得开")
        .to_string();
    vec![
        ("docker", "docker", a(&[])),
        ("upper", "DOCKER", a(&[])),
        ("padded", "   docker  ", a(&[])),
        ("budget5", "docker", a(&["--limit", "5"])),
        ("user", "docker", a(&["--scope", "user"])),
        ("assistant", "docker", a(&["--scope", "assistant"])),
        ("after", "docker", vec!["--after-ms".into(), after.clone()]),
        ("kube", "kubernetes", a(&[])),
        ("kube-tools", "kubernetes", a(&["--include-tools"])),
        ("cjk", "设计", a(&[])),
        ("noise", "注入噪声", a(&[])),
        ("empty", "", a(&[])),
        ("nohit", "zzz-no-hit", a(&[])),
        (
            "mixed",
            "docker",
            vec![
                "--include-tools".into(),
                "--limit".into(),
                "40".into(),
                "--scope".into(),
                "user".into(),
                "--after-ms".into(),
                "1".into(),
            ],
        ),
    ]
}

/// 在 `home` 上把 [`cases`] 全跑一遍，渲染成金样那一形（每问一行 JSON；路径前缀归一成 `<HOME>`）。
pub(super) fn render(home: &Path) -> String {
    let canon = home.canonicalize().expect("家目录规范化");
    let prefix = canon.to_string_lossy().into_owned();
    let mut out = String::new();
    for (name, q, rest) in cases() {
        let opts = parse_opts(&rest);
        let mut buf: Vec<u8> = Vec::new();
        let unreadable = search_counting(home, q, &opts, &mut buf).expect("search ok");
        let lines: Vec<String> = String::from_utf8(buf)
            .expect("输出是 UTF-8")
            .lines()
            .map(|l| l.replace(&prefix, "<HOME>"))
            .collect();
        out.push_str(
            &json!({"case": name, "query": q, "args": rest, "unreadable": unreadable, "lines": lines})
                .to_string(),
        );
        out.push('\n');
    }
    out
}

/// ★ J1：逐问、逐行、逐字节 == 起步树现扫实现冻结下来的金样（行序列相等 ⇒ 两向）。
#[test]
fn sx1_the_answer_equals_the_frozen_rescan_answer_line_for_line() {
    let home = build_corpus("golden");
    let got = render(&home);
    std::fs::remove_dir_all(&home).ok();
    let want = include_str!("../../__fixtures__/history-search-rescan.golden.jsonl");
    let got_rows: Vec<&str> = got.lines().collect();
    let want_rows: Vec<&str> = want.lines().collect();
    assert_eq!(
        got_rows.len(),
        want_rows.len(),
        "问的条数变了 —— 语料 / 查询清单一改，金样就得回到一份现扫实现上重新跑出来"
    );
    // 反空真：金样里真有命中行、也真有「读不动」的那两份 —— 否则下面的逐条相等可以在两侧都空时绿。
    let parsed: Vec<Value> = want_rows
        .iter()
        .map(|w| serde_json::from_str(w).expect("金样是 JSON"))
        .collect();
    let hit_rows: usize = parsed
        .iter()
        .map(|w| w["lines"].as_array().map_or(0, Vec::len))
        .sum();
    assert!(
        hit_rows > 0 && parsed.iter().any(|w| w["unreadable"] == 2),
        "金样里没有命中行 / 没有读不动的那两份 —— 语料没建起来，本条空转"
    );
    for (g, w) in got_rows.iter().zip(want_rows.iter()) {
        let gv: Value = serde_json::from_str(g).expect("渲染是 JSON");
        let wv: Value = serde_json::from_str(w).expect("金样是 JSON");
        let name = wv["case"].as_str().unwrap_or("?");
        let gl: Vec<&str> = gv["lines"]
            .as_array()
            .map_or_else(Vec::new, |a| a.iter().filter_map(Value::as_str).collect());
        let wl: Vec<&str> = wv["lines"]
            .as_array()
            .map_or_else(Vec::new, |a| a.iter().filter_map(Value::as_str).collect());
        let only_got: Vec<&&str> = gl.iter().filter(|l| !wl.contains(l)).collect();
        let only_want: Vec<&&str> = wl.iter().filter(|l| !gl.contains(l)).collect();
        assert_eq!(
            g, w,
            "问 `{name}` 的应答与现扫金样不等。\n只在这一侧：{only_got:#?}\n只在金样里：{only_want:#?}"
        );
    }
}
