//! 〔RM1c · 第四波〕`control/panorama.rs` 的判据。
//!
//! 走全流程的那几条用一个**真进程**当插件（`/bin/sh` 写的小脚本，说小程序那套方言），
//! 喂进来的字节都是它现打的 —— 找它 / 问它 / 起它 / 拿码 / 翻语义五跳全是真的。
//! 小程序本体（真引擎）的判据住它自己那棵树（`tests/panorama-engine/cli_tests.rs`）。

use super::*;
use crate::plugin::invoke::Done;

/// 〔RM1f〕`answer_with` 变成了 async（`Run::Async` 那一档）：判据在一个现起的 runtime 上等它。
fn answer_now(fixed: &[PathBuf], store: &Path, args: &Value) -> Result<Value, CmdErr> {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(answer_with(fixed, store, args))
}

/// 私有临时目录（进程号 ＋ 标签 ＋ 序号）。
fn scratch(tag: &str) -> PathBuf {
    use std::sync::atomic::{AtomicUsize, Ordering};
    static N: AtomicUsize = AtomicUsize::new(0);
    let p = std::env::temp_dir().join(format!(
        "ccm-be-panorama-{tag}-{}-{}",
        std::process::id(),
        N.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = std::fs::remove_dir_all(&p);
    std::fs::create_dir_all(&p).unwrap();
    p
}

/// 在 `dir` 里放一个名叫 [`PLUGIN_NAME`] 的假小程序：`--probe` 报 `caps`，
/// 其余调用照 `body`（一段 sh，`$@` 是 argv）。
#[cfg(unix)]
fn fake_program(dir: &Path, first_line: &str, caps: &str, body: &str) -> PathBuf {
    fake_program_shaped(dir, first_line, caps, &format!("shape={SHAPE}"), body)
}

/// 同 [`fake_program`]，`--probe` 的形状那一行由调用方给（空串 = 老一代，没有这一行）。
#[cfg(unix)]
fn fake_program_shaped(
    dir: &Path,
    first_line: &str,
    caps: &str,
    shape_line: &str,
    body: &str,
) -> PathBuf {
    use std::os::unix::fs::PermissionsExt;
    let f = dir.join(PLUGIN_NAME);
    std::fs::write(
        &f,
        format!(
            "#!/bin/sh\nif [ \"$1\" = \"--probe\" ]; then printf '{first_line}\\nversion=t\\ncapabilities={caps}\\n{shape_line}\\n'; exit 0; fi\n{body}\n"
        ),
    )
    .unwrap();
    std::fs::set_permissions(&f, std::fs::Permissions::from_mode(0o755)).unwrap();
    f
}

#[test]
fn the_candidates_are_beside_the_backend_then_the_deploy_home_and_never_path() {
    let exe = Path::new("/opt/b");
    let home = Path::new("/h");
    assert_eq!(
        fixed_candidates(Some(exe), Some(home)),
        vec![
            PathBuf::from("/opt/b/cc-monitor-panorama"),
            PathBuf::from("/h/.cc-monitor/bin/cc-monitor-panorama"),
        ]
    );
    // 后端本来就住在部署落点 ⇒ 同一条路径不列两遍。
    assert_eq!(
        fixed_candidates(Some(Path::new("/h/.cc-monitor/bin")), Some(home)),
        vec![PathBuf::from("/h/.cc-monitor/bin/cc-monitor-panorama")]
    );
    assert!(fixed_candidates(None, None).is_empty());
    assert_eq!(
        store_dir(home),
        PathBuf::from("/h/.cc-monitor/panorama"),
        "索引落后端自己的数据目录"
    );
}

/// ★ 本表 == 小程序那张 op 表（两向集合相等，**异源**：运行时读 `src/panorama-engine/main.rs`）；
/// 用建索引那一档期限的 op == 小程序里标成独占写（`Need::Build`）的那几个。
///
/// ⚠ 跨树运行时读、不走 `include_str!`：同 `panorama_locus_guard` 的取舍（编译期读会给
/// `cross_half_edge_registry` 添一条跨半边）；代价是「文件挪了只有本条红」，补偿是 `expect` ＋ 字节地板。
#[test]
fn the_op_table_matches_the_program_one() {
    let p = crate::guard_support::repo_root().join("src/panorama-engine/main.rs");
    let src = std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("读不到 {p:?}：{e}"));
    assert!(
        src.len() > 5_000,
        "{p:?} 只有 {} 字节 —— 没读到真文件",
        src.len()
    );
    let prod = guard_core::production_code(&src);
    let at = prod
        .find("pub const OPS: &[(&str, Need)] = &[")
        .expect("小程序的 op 表改了写法 —— 本条跟着改");
    let body = &prod[at..at + prod[at..].find("];").expect("op 表没收尾")];
    let mut theirs: Vec<(String, bool)> = Vec::new();
    for line in body.lines() {
        let t = line.trim();
        let Some(rest) = t.strip_prefix("(\"") else {
            continue;
        };
        let q = rest.find('"').expect("op 名没闭合");
        theirs.push((rest[..q].to_string(), rest[q..].contains("Need::Build")));
    }
    theirs.sort();
    let mut ours: Vec<(String, bool)> = OPS
        .iter()
        .map(|(n, d)| (n.to_string(), *d == BUILD_DEADLINE_SECS))
        .collect();
    ours.sort();
    assert!(
        theirs.len() >= 10,
        "只抽到 {} 个 op —— 抽取坏了",
        theirs.len()
    );
    assert_eq!(
        ours, theirs,
        "后端认得的全景 op（与期限档）和小程序那张 op 表对不上 —— 两边同拍改"
    );
}

#[cfg(unix)]
#[test]
fn a_real_process_walks_find_probe_run_and_back() {
    let dir = scratch("walk");
    let store = dir.join("store");
    let log = dir.join("argv.txt");
    // 它把收到的 argv 一行一个记下来（验「argv 直传、不过 shell」：带空格的路径是**一个**参数），
    // 应答里带回参数个数。
    let bin = fake_program(
        &dir,
        "name=cc-monitor-panorama",
        "status,overview",
        &format!(
            r#"printf '%s\n' "$@" > '{}'; printf '{{"ok":true,"data":{{"n":%s}}}}\n' "$#""#,
            log.display()
        ),
    );
    let got = answer_now(
        &[bin],
        &store,
        &json!({"op": "overview", "repo": "/r e/p", "args": {"budget": 5}}),
    )
    .unwrap();
    assert_eq!(got, json!({"result": {"n": 7}}));
    let argv: Vec<String> = std::fs::read_to_string(&log)
        .unwrap()
        .lines()
        .map(str::to_string)
        .collect();
    assert_eq!(
        argv,
        vec![
            "overview".to_string(),
            "--store".to_string(),
            store.display().to_string(),
            "--repo".to_string(),
            "/r e/p".to_string(),
            "--args".to_string(),
            "{\"budget\":5}".to_string(),
        ]
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[cfg(unix)]
#[test]
fn every_rejection_lands_on_its_own_code() {
    let dir = scratch("codes");
    let store = dir.join("store");
    let body = r#"case "$1" in
  status) printf '{"ok":false,"code":"bad_args","message":"缺仓"}\n'; echo 缺仓 >&2; exit 2;;
  overview) printf '{"ok":false,"code":"failed","message":"仓打不开"}\n'; exit 3;;
  node) echo boom >&2; exit 7;;
  search) echo 'not json'; exit 0;;
esac"#;
    let bin = fake_program(
        &dir,
        "name=cc-monitor-panorama",
        "status,overview,node,search",
        body,
    );
    let ask = |args: Value| answer_now(std::slice::from_ref(&bin), &store, &args);
    let code_of = |args: Value| ask(args).unwrap_err();

    // 词表之外的 op：**不起进程**就拒（候选空也是 bad_args，不是 not_installed）。
    assert_eq!(
        answer_now(&[], &store, &json!({"op": "vacuum"}))
            .unwrap_err()
            .0,
        "bad_args"
    );
    assert_eq!(code_of(json!({})).0, "bad_args");
    assert_eq!(code_of(json!({"op": "status", "repo": 3})).0, "bad_args");
    assert_eq!(code_of(json!({"op": "status", "args": [1]})).0, "bad_args");
    // 小程序自己说的两类：原话带回。
    assert_eq!(
        code_of(json!({"op": "status"})),
        ("bad_args", "缺仓".to_string())
    );
    assert_eq!(
        code_of(json!({"op": "overview"})),
        ("failed", "仓打不开".to_string())
    );
    // 别的码：failed ＋ 码 ＋ 诊断。
    let (c, m) = code_of(json!({"op": "node", "args": {"symbol": "x"}}));
    assert_eq!(c, "failed");
    assert!(m.contains("码 7") && m.contains("boom"), "{m}");
    // 码 0 但那一行不是 ok 应答：不许当成功。
    assert_eq!(
        code_of(json!({"op": "search", "args": {"query": "a"}})).0,
        "failed"
    );
    // 装的这份不会这个 op：说出缺的那一个。
    let (c, m) = code_of(json!({"op": "diagram", "args": {"kind": "arch", "request": {}}}));
    assert_eq!(c, "unsupported");
    assert!(m.contains("「diagram」"), "缺能力要点名缺的那个：{m}");
    // 找不到：说清查过哪儿。
    let (c, m) = answer_now(&[dir.join("nowhere")], &store, &json!({"op": "status"})).unwrap_err();
    assert_eq!(c, "not_installed");
    assert!(m.contains("查过") && m.contains("nowhere"), "{m}");
    // 不兜 `PATH`（同名的无关程序不该有机会被当成它）：那句话里 PATH 那一格是 0 个目录。
    assert!(m.contains("PATH 上的 0 个目录"), "{m}");
    let _ = std::fs::remove_dir_all(&dir);
}

/// 设计/97 §8 · §6.5 · 99 §2.1 ㉝①：「`--probe` 加形状代号，后端判旧回 `unsupported`，放字节那条自动接上」。
/// 能力都在、形状代号对不上（或老一代压根没这一行）⇒ `unsupported`（`PUSH_ON` 里有它 ⇒ monitor 放字节），且不起那个 op。
#[cfg(unix)]
#[test]
fn an_old_generation_with_every_op_is_still_unsupported() {
    for (tag, shape_line) in [("stale", "shape=0000000000000000"), ("none", "")] {
        let dir = scratch(tag);
        let ran = dir.join("ran");
        let bin = fake_program_shaped(
            &dir,
            "name=cc-monitor-panorama",
            "status",
            shape_line,
            &format!(
                "touch '{}'; printf '{{\"ok\":true,\"data\":1}}\\n'",
                ran.display()
            ),
        );
        let (c, _) = answer_now(&[bin], &dir.join("s"), &json!({"op": "status"})).unwrap_err();
        assert_eq!(c, "unsupported", "{tag}：旧一代没被判旧");
        assert!(!ran.exists(), "{tag}：判了旧还是把 op 起了");
        let _ = std::fs::remove_dir_all(&dir);
    }
}

#[cfg(unix)]
#[test]
fn a_same_named_stranger_is_not_taken_for_the_program() {
    let dir = scratch("stranger");
    let bin = fake_program(&dir, "name=something-else", "status", "exit 0");
    let (c, m) = answer_now(&[bin], &dir.join("s"), &json!({"op": "status"})).unwrap_err();
    assert_eq!(c, "not_installed");
    assert!(m.contains("something-else"), "{m}");
    let _ = std::fs::remove_dir_all(&dir);
}

/// 码 → 语义那张表里**拿不到真进程**的两格：期限命令超时 · 结果过大（合成 `Done`）。
#[test]
fn a_timeout_and_an_oversized_answer_are_said_as_such() {
    let done = |code: Option<i32>, stdout: Vec<u8>| Done {
        code,
        stdout,
        stderr: Vec::new(),
    };
    let (c, m) = classify(
        "index",
        BUILD_DEADLINE_SECS,
        done(Some(crate::plugin::invoke::TIMED_OUT_CODE), Vec::new()),
    )
    .unwrap_err();
    assert_eq!(c, "timed_out");
    assert!(
        m.contains(&BUILD_DEADLINE_SECS.to_string()),
        "说清是哪一档期限：{m}"
    );
    let big = vec![b'x'; crate::faces::read_face::LINES_CAP_BYTES + 1];
    assert_eq!(
        classify("overview", 1, done(Some(0), big)).unwrap_err().0,
        "too_large"
    );
    assert_eq!(
        classify("status", 1, done(None, Vec::new())).unwrap_err(),
        (
            "failed",
            "代码全景「status」没做成（被中途终止）：".to_string()
        )
    );
    // 成功那一格：`data` 原样装进 `result`（`null` 也是一个答案：「没有这个符号」）。
    assert_eq!(
        classify(
            "node",
            1,
            done(Some(0), b"{\"ok\":true,\"data\":null}\n".to_vec())
        )
        .unwrap(),
        json!({"result": null})
    );
}

/// 设计/97 §8（主会话 09-28 裁 FIX4）「全景小程序卸口：给（受管工具都应可卸，照 SU1 装卸账）」：
/// 不在 ⇒ `removed:false`；是全景小程序（`--probe` 首行认得）⇒ 删掉那一份、索引不动；认不出 ⇒ `not_ours`、一个字节不动。
#[cfg(unix)]
#[tokio::test]
async fn uninstall_removes_only_the_placed_program_and_only_when_it_is_ours() {
    use std::os::unix::fs::PermissionsExt;
    let home = std::env::temp_dir().join(format!("pano-uninstall-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&home);
    let bin = home.join(".cc-monitor/bin");
    std::fs::create_dir_all(&bin).unwrap();
    let index = home.join(".cc-monitor/panorama");
    std::fs::create_dir_all(&index).unwrap();
    let prog = bin.join(program_file_name());
    let door = crate::stream::inbound::LocalFiles;

    let v = uninstall_at(door, home.clone())
        .await
        .expect("不在也不是错");
    assert_eq!(v["removed"], false);

    let write = |body: &str| {
        std::fs::write(&prog, body).unwrap();
        std::fs::set_permissions(&prog, std::fs::Permissions::from_mode(0o755)).unwrap();
    };
    write("#!/bin/sh\necho 'name=someone-else'\n");
    let e = uninstall_at(door, home.clone())
        .await
        .expect_err("认不出的不许删");
    assert_eq!(e.0, "not_ours");
    assert!(prog.exists(), "认不出却动了它");

    write("#!/bin/sh\necho 'name=cc-monitor-panorama'\necho 'version=9'\n");
    let v = uninstall_at(door, home.clone()).await.expect("是它就删");
    assert_eq!(v["removed"], true);
    assert!(!prog.exists(), "说删了却还在");
    assert!(index.exists(), "索引不是装时写的，不许删");
    let _ = std::fs::remove_dir_all(&home);
}
