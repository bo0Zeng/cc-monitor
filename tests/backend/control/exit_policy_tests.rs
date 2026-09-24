//! 〔B2 · 条 66〕`control/exit_policy.rs` 的判据 —— `设计/01 §3.3b` 的 `E1` / `E2` 后端那一半 ＋ 三态行为。
//!
//! # 买到的
//!
//! - 三态真的分得开（本机临时目录上真读真写，不是源码扫描）：没文件 ⇒ `Absent`；
//!   坏 JSON / 缺格 / 类型错 ⇒ `Unreadable` 且带原因；写过 ⇒ `Chosen`。
//! - **现读**（`E2` 的行为半）：同一进程里读一次、**绕过写口**在盘上改掉、再读 —— 必须读到新值。
//!   加一层 memo（哪怕只记一次）这里当场红。
//! - 写是原子的、只有一个写口、不留临时文件（`E1` 的后端半）。
//! - `E1` 的全仓半：文件名字面量在 `src/` 全部生产代码里**恰好一个家**（两向集合相等 ＋ 正控）。
//! - `E2` 的结构半：本模块生产段的缓存构件数 == 登记表里的 0（相等，不是「扫不到就绿」）；
//!   决策那一处（`main.rs` 流结束那一臂）是**唯一**一处引用本模块的地方、而且在那一臂里面。
//!
//! # 买不到的
//!
//! - 🔴 **真远端**：远端后端今天只走 stdio（SSH exec），它**没有**「最后一个客户走了」那一臂 ——
//!   远端那一行的值写得进去、读得回来，但**没有任何进程按它动手**（它本来就随 SSH 断而死）。
//! - 🔴 **真 Windows**：`USERPROFILE` 那一支只在编译面上存在，本族一格没在 Windows 上跑过。
//! - 常驻那条载体上「流断了 ⇒ 真的退出」没有 e2e（要真起一个脱离的后端再断流）；
//!   本族钉的是那一臂**在不在、读的是不是现值**，不是进程真退了。

use super::*;

fn temp_dir(tag: &str) -> PathBuf {
    let p = std::env::temp_dir().join(format!("ccm-exitpol-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&p);
    std::fs::create_dir_all(&p).expect("建临时目录");
    p
}

#[test]
fn three_states_are_told_apart() {
    let d = temp_dir("states");
    let f = d.join(DIR_NAME).join(FILE_NAME);
    assert_eq!(read_at(&f), Read::Absent, "没文件应当是「没人选过」");
    assert!(!Read::Absent.kill_on_exit(), "没人选过 ⇒ 缺省不结束");

    std::fs::create_dir_all(f.parent().unwrap()).unwrap();
    for (body, why) in [
        ("{not json", "不是 JSON"),
        ("{}", "缺那一格"),
        ("{\"killOnExit\":\"yes\"}", "类型不对"),
    ] {
        std::fs::write(&f, body).unwrap();
        match read_at(&f) {
            Read::Unreadable(reason) => assert!(
                !reason.trim().is_empty(),
                "「{why}」读成了 Unreadable，但原因是空的 —— 读不出来要说出为什么"
            ),
            other => panic!("「{why}」应当是读不出来，实得 {other:?}"),
        }
        assert!(!read_at(&f).kill_on_exit(), "读不出来 ⇒ 按缺省（不结束）办");
        assert_eq!(read_at(&f).state(), "unreadable");
    }
    assert_eq!(Read::Absent.state(), "absent");
    assert_eq!(Read::Chosen(true).state(), "chosen");
    let _ = std::fs::remove_dir_all(&d);
}

/// ★★ `E2` 的行为半：**现读**。
///
/// 读一次 → **绕过本模块的写口**直接在盘上改（模拟「另一台 monitor 刚改过它」）→ 再读。
/// 必须读到新值。任何一种记忆（`OnceLock` / 读一次存下 / 按 mtime 缓存而 mtime 粒度不够）都会在这里红。
#[test]
fn every_read_goes_to_disk() {
    let d = temp_dir("fresh");
    let f = d.join(DIR_NAME).join(FILE_NAME);
    write_at(&f, false).expect("写 false");
    assert_eq!(read_at(&f), Read::Chosen(false));
    std::fs::write(&f, "{\"killOnExit\":true}").unwrap();
    assert_eq!(
        read_at(&f),
        Read::Chosen(true),
        "盘上已经是 true，读到的还是旧值 —— 读取路径上有记忆（E2 要防的正是这个：远端改了不生效，而且不报错）"
    );
    std::fs::remove_file(&f).unwrap();
    assert_eq!(read_at(&f), Read::Absent, "文件没了还读得出值 —— 有记忆");
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn the_writer_is_atomic_creates_its_own_dir_and_leaves_nothing_behind() {
    let d = temp_dir("write");
    let f = d.join(DIR_NAME).join(FILE_NAME);
    assert!(!f.parent().unwrap().exists(), "前提：家目录下还没有那一层");
    write_at(&f, true).expect("第一次写（顺带建那一层目录）");
    assert_eq!(read_at(&f), Read::Chosen(true));
    write_at(&f, false).expect("第二次写（盖掉既有的）");
    assert_eq!(read_at(&f), Read::Chosen(false));
    let tmp = f
        .parent()
        .unwrap()
        .join(format!("{FILE_NAME}.{}.tmp", std::process::id()));
    assert!(
        !tmp.exists(),
        "写完之后临时文件还在（{}）—— 没挪过去，或者挪了个副本",
        tmp.display()
    );
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn set_rejects_a_non_boolean_and_writes_nothing() {
    for bad in [
        serde_json::json!({}),
        serde_json::json!({ "killOnExit": "true" }),
        serde_json::json!({ "killOnExit": 1 }),
    ] {
        let err = answer_set(&bad).expect_err("非布尔应当被拒");
        assert_eq!(err.0, "bad_args", "{bad} 应当回 bad_args");
    }
}

#[test]
fn the_wire_shape_carries_the_three_states_and_the_shell() {
    let v = wire(&Read::Unreadable("坏了".into()), None);
    assert_eq!(v["state"], "unreadable");
    assert_eq!(v["reason"], "坏了");
    assert_eq!(v["killOnExit"], DEFAULT_KILL_ON_EXIT);
    assert_eq!(v["shell"], SHELL);
    let v = wire(&Read::Chosen(true), None);
    assert_eq!(v["state"], "chosen");
    assert_eq!(v["killOnExit"], true);
    assert!(v["reason"].is_null(), "选过的那一态不该带原因");
    let v = wire(&Read::Absent, None);
    assert_eq!(v["state"], "absent");
    assert!(v["reason"].is_null());
}

// ═══════════════════════════ E1：写者全仓只有一处 ═══════════════════════════

/// 本模块生产段里的写原语 —— **恰好**这几处（按词找，钉住「只有一个写口」）。
#[test]
fn the_module_has_exactly_one_commit_point() {
    let prod = crate::guard_support::production_code(include_str!(
        "../../../src/backend/control/exit_policy.rs"
    ));
    for (needle, why) in [
        (
            "fs::rename(",
            "那一下原子挪进去 —— 真正落进 `backend.json` 的**唯一**一下",
        ),
        (
            ".create_new(true)",
            "临时文件 `O_EXCL` 新建 —— 不跟随、不覆盖别人的临时文件",
        ),
    ] {
        guard_core::find_pinned(&prod, needle).unwrap_or_else(|e| {
            panic!("`exit_policy.rs` 里 `{needle}` 不是恰好一处（{why}）：{e}")
        });
    }
}

/// 一份生产源码去掉注释之后的样子（按扩展名分流，剥法全走共享原语，不另写一份）：
/// `.rs` 走 `production_code`（剥测试段 ＋ 注释）；`.ts` 走 `strip_comment_lines`（`//` 与块注释）；
/// `.sh` 走 `strip_hash_comment_lines`。
fn code_of(path: &std::path::Path, src: &str) -> String {
    match path.extension().and_then(|e| e.to_str()) {
        Some("rs") => crate::guard_support::production_code(src),
        Some("ts") => guard_core::strip_comment_lines(src),
        _ => guard_core::strip_hash_comment_lines(src),
    }
}

/// ★★ `E1` 的全仓半：**`backend.json` 这个名字在 `src/` 全部生产代码里只有一个家**，就是本模块。
///
/// ⇒ monitor（`src/bridge/`）与前端（`src/**/*.ts`）零命中 —— 它们连这个文件叫什么都说不出来，
/// 更写不了它。**两向集合相等**：多一个家 = 第二个写者（或读者）冒出来了；少了本模块 = 正控失败
/// （判据在一个空集上绿）。
///
/// ⚠ 漏判面（如实登记）：把名字拆开拼（`format!("{}.json", "backend")`）骗得过它；
/// 仓外的脚本 / 用户手工编辑不在人群里（那也不是「我们的写者」）。
#[test]
fn the_file_name_has_exactly_one_home_in_all_production_code() {
    let src = crate::guard_support::repo_root().join("src");
    let needle = format!("{}.{}", "backend", "json");
    let mut scanned = 0usize;
    let mut homes: std::collections::BTreeSet<String> = Default::default();
    for (path, body) in guard_core::scan_tree_excluding(&src, &["rs", "ts", "sh"], &[]) {
        scanned += 1;
        if code_of(&path, &body).contains(needle.as_str()) {
            let rel = path
                .strip_prefix(&src)
                .unwrap_or(&path)
                .to_string_lossy()
                .replace('\\', "/");
            homes.insert(rel);
        }
    }
    assert!(
        scanned > 500,
        "只扫到 {scanned} 份源码 —— 遍历坏了，零命中在空人群上恒绿"
    );
    let want: std::collections::BTreeSet<String> =
        ["backend/control/exit_policy.rs".to_string()].into();
    assert_eq!(
        homes,
        want,
        "\n`{needle}` 在生产代码里的家与期望对不上。\n  多出来的（🔴 第二个写者 / 读者）：{:?}\n  \
         少了的（本模块自己都没命中 ⇒ 判据在空转）：{:?}\n\
         `设计/01 §3.3b ③`：只有后端写，前端改它走一条后端命令，**前端从不碰那个文件**。",
        homes.difference(&want).collect::<Vec<_>>(),
        want.difference(&homes).collect::<Vec<_>>()
    );
}

// ═══════════════════════════ E2：决策处现读 ═══════════════════════════

/// 会让「读」变成「记」的构件。`static` 带空格：`'static` 生命周期不算。
const CACHE_CONSTRUCTS: &[&str] = &[
    "static ",
    "OnceLock",
    "OnceCell",
    "LazyLock",
    "Lazy<",
    "lazy_static",
    "thread_local",
    "Mutex",
    "RwLock",
    "Atomic",
];

fn cache_points(prod: &str) -> Vec<&'static str> {
    CACHE_CONSTRUCTS
        .iter()
        .filter(|c| {
            // `static ` 要排掉生命周期写法（`&'static str` 里也有 `static ` 这一串）。
            prod.match_indices(**c)
                .any(|(k, _)| !(**c == "static " && prod[..k].ends_with('\'')))
        })
        .copied()
        .collect()
}

/// 读取路径上**登记在案**的缓存点。今天是 **0**，而这张表就是那个 0 的住址（`E2` 的反空真自检：
/// 绿必须来自一次**相等断言**，不是「扫不到缓存就绿」）。
const CACHE_POINTS: &[(&str, &str)] = &[];

/// ★★ `E2` 的结构半：本模块生产段的缓存构件 == 登记表（0）。**带正控**：
/// 扫描器对一份真的带缓存的样本必须报出来，否则下面那条相等是在一台瞎了的尺子上成立的。
#[test]
fn the_decision_path_has_exactly_the_registered_cache_points() {
    let sample =
        "fn f() -> bool { static V: OnceLock<bool> = OnceLock::new(); *V.get_or_init(|| true) }";
    assert_eq!(
        cache_points(sample),
        vec!["static ", "OnceLock"],
        "正控失败：一份明摆着带 `static` ＋ `OnceLock` 的样本，扫描器没看全 —— 尺子瞎了"
    );
    assert!(
        cache_points("fn g() -> &'static str { \"x\" }").is_empty(),
        "负控失败：生命周期 `'static` 被当成了一个缓存点 —— 那会逼人去「绕」这条判据"
    );
    let prod = crate::guard_support::production_code(include_str!(
        "../../../src/backend/control/exit_policy.rs"
    ));
    assert!(
        prod.contains("fn read_at("),
        "生产段里找不到 `read_at` —— 剥法或住址坏了，下面那条相等在空串上成立"
    );
    let found = cache_points(&prod);
    assert_eq!(
        found.len(),
        CACHE_POINTS.len(),
        "`exit_policy.rs` 生产段的缓存构件 {found:?} 与登记表（{} 条）对不上。\n\
         `设计/01 §3.3b ④`：**不缓存、不在启动时读一次存内存** —— 用户可能刚从另一台 monitor 改过它。\n\
         缓存回来 ⇒ 远端改了不生效，**而且不报错**（E2 防的就是这一形）。",
        CACHE_POINTS.len()
    );
}

/// ★★ `E2` 的另一半：**决策那一处**在它该在的地方，而且是**唯一**一处。
///
/// `main.rs` 里对本模块的引用恰好一处、就在「流结束」那一臂的体内 ⇒
/// 「启动时读一次、存进一个变量、流结束时用那个变量」这一形（启动时快照）过不了：
/// 那需要第二处引用，或者引用不在那一臂里。
#[test]
fn the_only_decision_site_is_the_stream_end_arm() {
    let main = crate::guard_support::production_code(include_str!("../../../src/backend/main.rs"));
    let needle = "exit_policy::";
    assert_eq!(
        main.matches(needle).count(),
        1,
        "`main.rs` 生产段里 `{needle}` 不是恰好一处 —— 多一处多半是启动时快照（E2 点名的那一形）"
    );
    let arm = main
        .find("done_rx.recv()")
        .expect("找不到「流结束」那一臂（`done_rx.recv()`）—— 接受循环的形状变了，先修锚点");
    let open = arm + main[arm..].find('{').expect("那一臂之后没有块");
    let mut depth = 0i32;
    let mut end = main.len();
    for (i, b) in main.as_bytes()[open..].iter().enumerate() {
        match b {
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    end = open + i + 1;
                    break;
                }
            }
            _ => {}
        }
    }
    let body = &main[open..end];
    assert!(
        body.len() > 40 && body.len() < 4000,
        "切出来的那一臂 {} 字节 —— 配平切错了",
        body.len()
    );
    guard_core::find_pinned(body, "exit_policy::last_client_left()").unwrap_or_else(|e| {
        panic!("「流结束」那一臂里没有恰好一处现读决策（{e}）。\n臂体：{body}")
    });
}
