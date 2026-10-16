//! `config.json` 唯一写口 `config::patch_config_at` 的判据。
//!
//! 守的要求（住址）：
//! -：「两者 …… 各自只写自己那个键」；`§C.3`：「**只动自己那个键** …… 不整段覆盖」。
//! - 红线 ④：「**读不懂的 `config.json` 不写** …… 盘上一个字节不动（不退回『当成空对象覆盖』，
//!   那会把用户其余配置一起抹掉）」。
//!
//! J2（并发不丢）打真文件、真线程；J3（补丁语义）期望全是手写 JSON 字面量，不经被测代码生成。
//! 前端那一半（每个写者只交自己的路径、两个 realm 同写）在 `tests/frontend/ui/config-lost-update.vitest.ts`。
use super::*;
use serde_json::json;
use std::sync::{Arc, Barrier};

fn tmpdir(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("ccm-cfg1-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn set(path: &[&str], value: Value) -> ConfigEdit {
    ConfigEdit::Set {
        path: path.iter().map(|s| s.to_string()).collect(),
        value,
    }
}

fn remove(path: &[&str]) -> ConfigEdit {
    ConfigEdit::Remove {
        path: path.iter().map(|s| s.to_string()).collect(),
    }
}

/// 写口用的临时件名（`config.json.<pid>.tmp`）。
fn tmp_of(file: &Path) -> PathBuf {
    file.with_extension(format!("json.{}.tmp", std::process::id()))
}

fn read(p: &Path) -> Value {
    serde_json::from_str(&std::fs::read_to_string(p).unwrap()).unwrap()
}

/// J2：N 线程 × M 轮，每个线程每轮写**自己的**顶层键和同一个段里**自己的**子键，屏障同时放。
/// 终态键集 == 期望键集（两向）。期望由线程号直接生成，不经被测代码。
///
/// 死值验：把 `patch_config_at` 里那一行 `WRITE_LOCK.lock()` 去掉 ⇒ 本条红（读-改-写交错丢键，
/// 或同一个带 pid 的临时件被另一个线程先 rename 走 ⇒ `Io`）。
#[test]
fn concurrent_patches_to_different_keys_all_survive() {
    const THREADS: usize = 12;
    const ROUNDS: usize = 20;
    let dir = tmpdir("race");
    let file = dir.join("config.json");
    std::fs::write(&file, r#"{"userWrote":"手写的那一格"}"#).unwrap();
    let file = Arc::new(file);
    let barrier = Arc::new(Barrier::new(THREADS));
    let handles: Vec<_> = (0..THREADS)
        .map(|t| {
            let file = Arc::clone(&file);
            let barrier = Arc::clone(&barrier);
            std::thread::spawn(move || {
                for r in 0..ROUNDS {
                    barrier.wait();
                    let top = format!("k{t}");
                    let sub = format!("s{t}");
                    patch_config_at(
                        &file,
                        &[
                            set(&[top.as_str()], json!(r)),
                            set(&["seg", sub.as_str()], json!(r)),
                        ],
                    )
                    .unwrap();
                }
            })
        })
        .collect();
    for h in handles {
        h.join().unwrap();
    }
    let mut want = serde_json::Map::new();
    want.insert("userWrote".into(), json!("手写的那一格"));
    let mut seg = serde_json::Map::new();
    for t in 0..THREADS {
        want.insert(format!("k{t}"), json!(ROUNDS - 1));
        seg.insert(format!("s{t}"), json!(ROUNDS - 1));
    }
    want.insert("seg".into(), Value::Object(seg));
    assert_eq!(read(&file), Value::Object(want));
    // 临时件没留下（名字由 pid 定，按名字查 —— 不遍历目录，见 `scanning_guard_registry`）
    assert!(!tmp_of(&file).exists(), "临时件留下了");
    let _ = std::fs::remove_dir_all(&dir);
}

/// J3：补丁语义逐格对手写期望。
#[test]
fn set_and_remove_touch_only_their_path() {
    let dir = tmpdir("sem");
    let file = dir.join("config.json");
    std::fs::write(
        &file,
        r#"{"tabBar":{"order":["a"],"pinned":[1]},"accounts":"不是对象","keep":true}"#,
    )
    .unwrap();
    patch_config_at(
        &file,
        &[
            // 段内一个子键：兄弟键不动
            set(&["tabBar", "order"], json!(["b", "a"])),
            // 中间段不是对象 ⇒ 换成对象
            set(&["accounts", "modelByAccount", "a1"], json!("opus")),
            // 整键替换，值里的 null 原样存（快捷键解绑）
            set(&["keybindings"], json!({"x": null, "y": "Ctrl+Y"})),
            // 删一个不存在的路径：无事，也不建出空段
            remove(&["nope", "deeper"]),
            remove(&["alsoNope"]),
        ],
    )
    .unwrap();
    assert_eq!(
        read(&file),
        json!({
            "tabBar": {"order": ["b", "a"], "pinned": [1]},
            "accounts": {"modelByAccount": {"a1": "opus"}},
            "keybindings": {"x": null, "y": "Ctrl+Y"},
            "keep": true
        })
    );
    patch_config_at(
        &file,
        &[
            remove(&["accounts", "modelByAccount", "a1"]),
            remove(&["keep"]),
        ],
    )
    .unwrap();
    assert_eq!(
        read(&file),
        json!({
            "tabBar": {"order": ["b", "a"], "pinned": [1]},
            "accounts": {"modelByAccount": {}},
            "keybindings": {"x": null, "y": "Ctrl+Y"}
        })
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// J3：文件不在 ⇒ 从空对象起（连目录一起建）。
#[test]
fn a_missing_file_starts_from_an_empty_object() {
    let dir = tmpdir("missing");
    let file = dir.join("sub").join("config.json");
    patch_config_at(&file, &[set(&["theme"], json!({"bg": "#000"}))]).unwrap();
    assert_eq!(read(&file), json!({"theme": {"bg": "#000"}}));
    let _ = std::fs::remove_dir_all(&dir);
}

/// 数据目录与 `config.json` 只给本人：缺的那层目录建成 700、写出来的文件 600（旧的 664 那份写一次也收成 600）。
#[cfg(unix)]
#[test]
fn the_config_file_and_its_new_directory_are_only_for_the_owner() {
    use std::os::unix::fs::PermissionsExt as _;
    let dir = tmpdir("private");
    let file = dir.join("data").join("config.json");
    patch_config_at(&file, &[set(&["a"], json!(1))]).unwrap();
    let mode = |p: &Path| std::fs::metadata(p).unwrap().permissions().mode() & 0o777;
    assert_eq!((mode(&dir.join("data")), mode(&file)), (0o700, 0o600));
    std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o664)).unwrap();
    patch_config_at(&file, &[set(&["a"], json!(2))]).unwrap();
    assert_eq!(mode(&file), 0o600);
    let _ = std::fs::remove_dir_all(&dir);
}

/// J3：空路径（= 整份替换换了个名字）⇒ 整批拒，同批里成形的那条也不落，盘上字节不变。
#[test]
fn an_empty_path_refuses_the_whole_batch() {
    let dir = tmpdir("empty");
    let file = dir.join("config.json");
    let original = r#"{"keep":1}"#;
    std::fs::write(&file, original).unwrap();
    let err = patch_config_at(&file, &[set(&["a"], json!(1)), set(&[], json!({}))])
        .expect_err("空路径被接受了");
    assert!(matches!(err, ConfigWriteError::BadEdit(_)), "{err:?}");
    assert_eq!(std::fs::read_to_string(&file).unwrap(), original);
    let _ = std::fs::remove_dir_all(&dir);
}

/// J3：红线 ④ —— 读不懂 / 根不是对象 ⇒ `Unreadable`、盘上字节不变、没有临时件。
#[test]
fn an_unreadable_file_is_left_alone() {
    for (tag, original) in [("bad", r#"{"a":1 "b":2}"#), ("arr", "[1,2]")] {
        let dir = tmpdir(tag);
        let file = dir.join("config.json");
        std::fs::write(&file, original).unwrap();
        let err = patch_config_at(&file, &[set(&["a"], json!(9))]).expect_err("读不懂的被写了");
        assert!(
            matches!(err, ConfigWriteError::Unreadable { .. }),
            "{err:?}"
        );
        let msg = err.to_string();
        assert!(
            msg.contains("config.json")
                && copy_core::copy_matches("rsConfig.write.unreadable", &msg),
            "{msg}"
        );
        assert_eq!(std::fs::read_to_string(&file).unwrap(), original);
        assert!(
            !tmp_of(&file).exists(),
            "留下了临时件 —— 「不写」要在写之前就停"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}

/// 线上形状：前端交来的 JSON（`src/frontend/ui/generated/ConfigEdit.ts` 的形状）能按原样解出来。手写字面量。
#[test]
fn the_wire_shape_deserializes() {
    let edits: Vec<ConfigEdit> = serde_json::from_value(json!([
        {"op": "set", "path": ["tabBar", "order"], "value": ["a"]},
        {"op": "remove", "path": ["claudeDir"]}
    ]))
    .unwrap();
    assert!(
        matches!(&edits[0], ConfigEdit::Set { path, value } if path == &["tabBar", "order"] && value == &json!(["a"]))
    );
    assert!(matches!(&edits[1], ConfigEdit::Remove { path } if path == &["claudeDir"]));
    // 认不出的 op 不许被当成别的
    assert!(serde_json::from_value::<ConfigEdit>(json!({"op": "replace", "path": ["a"]})).is_err());
}

/// 金样 `tests/__fixtures__/config-patch.golden.json`（手写）逐条跑真写口：`after` 逐值相等；
/// `refused` ⇒ 错误种类对得上、盘上字节不变。前端测试用的假盘跑同一份（`tests/frontend/ui/config-patch-fake.vitest.ts`）。
#[test]
fn the_golden_cases_hold() {
    let golden: Value =
        serde_json::from_str(include_str!("../../__fixtures__/config-patch.golden.json")).unwrap();
    let cases = golden["cases"].as_array().unwrap();
    assert_eq!(
        cases.len(),
        9, // 按键认元素那几条随机器表搬进本机后端（`control/machines.rs`，判据在 `machines_tests.rs`）
        "金样条数变了 —— 两边（这里与 vitest 那边）一起改"
    );
    for (i, c) in cases.iter().enumerate() {
        let name = c["name"].as_str().unwrap();
        let dir = tmpdir(&format!("golden{i}"));
        let file = dir.join("config.json");
        let before = c["before"].as_str().unwrap();
        std::fs::write(&file, before).unwrap();
        let edits: Vec<ConfigEdit> = serde_json::from_value(c["edits"].clone()).unwrap();
        let got = patch_config_at(&file, &edits);
        match c.get("refused").and_then(Value::as_str) {
            Some(kind) => {
                let err = got.expect_err(name);
                let got_kind = match err {
                    ConfigWriteError::BadEdit(_) => "bad_edit",
                    ConfigWriteError::Unreadable { .. } => "unreadable",
                    ConfigWriteError::Io(_) => "io",
                };
                assert_eq!(got_kind, kind, "{name}");
                assert_eq!(
                    std::fs::read_to_string(&file).unwrap(),
                    before,
                    "{name}：拒了却动了盘"
                );
            }
            None => {
                got.unwrap_or_else(|e| panic!("{name}: {e}"));
                assert_eq!(read(&file), c["after"], "{name}");
            }
        }
        let _ = std::fs::remove_dir_all(&dir);
    }
}

// ═══ 两个 monitor 进程同写：跨进程锁 ═══════════════════════════════════════
//
// 要求：「CFG1 把 config.json 收成单一写口 `config.rs::patch_config_at`（进程内锁、锁内现读、按路径补丁），
// 但**两个 monitor 进程同写**没有跨进程锁 —— 用你那一族同一套 `flock`（Windows 对应）把它也包上」；「各自只写自己那个键」。
// ⚠ 用两个线程各开一次描述量（`flock` 锁在打开文件描述上，与两个进程同一种互斥）；限期只在判据里。

/// 🔴 C-L1：别人（另一个进程的样子：本线程直接拿目录锁）拿着锁时，`patch_config_at` 限期内不落盘；
/// 那人在锁里写下自己的键再放锁 ⇒ 这一趟现读到它、两键都在（刀：`patch_config_at` 不拿跨进程锁 ⇒ 先写完，随后被那人整份盖掉）。
#[test]
fn hx2_a_patch_waits_for_another_process_holding_the_config_dir() {
    let dir = tmpdir("hx2-xproc");
    let file = dir.join("config.json");
    std::fs::write(&file, r#"{"userWrote":"手写的那一格"}"#).unwrap();
    let held = crate::platform::fs::hold_dir_lock(&dir).expect("拿不到锁");
    let (tx, rx) = std::sync::mpsc::channel();
    let f2 = file.clone();
    let racer = std::thread::spawn(move || {
        let r = patch_config_at(&f2, &[set(&["mine"], json!(1))]);
        tx.send(r.is_ok()).unwrap();
    });
    assert!(
        rx.recv_timeout(std::time::Duration::from_millis(400))
            .is_err(),
        "另一个进程还拿着目录锁，这一趟就写完了"
    );
    std::fs::write(&file, r#"{"userWrote":"手写的那一格","other":2}"#).unwrap();
    drop(held);
    assert!(rx
        .recv_timeout(guard_core::wall::CEILING)
        .expect("放了锁还没写完"));
    racer.join().unwrap();
    assert_eq!(
        read(&file),
        json!({"userWrote": "手写的那一格", "other": 2, "mine": 1}),
        "有一方的键丢了"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// 🔴 C-L2：生产段 `patch_config_at` 拿跨进程锁**恰好一处**，且排在现读之前（锁住之后才读，才叫读—改—写串行）。
#[test]
fn hx2_the_config_writer_takes_the_cross_process_lock_before_it_reads() {
    let prod =
        guard_core::production_code(include_str!("../../../src/frontend/shell/src/config.rs"));
    let at = guard_core::find_pinned(&prod, "pub(crate) fn patch_config_at(").expect("写口不在了");
    let body = &prod[at..];
    let body = &body[..body.find("\nfn ").unwrap_or(body.len())];
    let lock = guard_core::find_pinned(body, "crate::platform::fs::hold_dir_lock(dir)")
        .expect("写口里拿跨进程锁不是恰好一处");
    let read_at = guard_core::find_pinned(body, "std::fs::read_to_string(path)")
        .expect("写口里现读不是恰好一处");
    assert!(
        lock < read_at,
        "跨进程锁排在现读之后 —— 读到的可能是别人写到一半之前的那一份"
    );
}

/// 🔴 临时件先落盘再换名：`sync_all` 恰好一处、排在换名之前（反过来 ⇒ 断电后换上去的可能是一份空文件，用户配置全丢）。
#[test]
fn the_config_writer_syncs_the_temp_file_before_it_renames() {
    let prod =
        guard_core::production_code(include_str!("../../../src/frontend/shell/src/config.rs"));
    let at = guard_core::find_pinned(&prod, "pub(crate) fn patch_config_at(").expect("写口不在了");
    let body = &prod[at..];
    let body = &body[..body.find("\nfn ").unwrap_or(body.len())];
    let sync = guard_core::find_pinned(body, ".sync_all()").expect("写口里落盘不是恰好一处");
    let swap = guard_core::find_pinned(body, "std::fs::rename(&tmp, path)")
        .expect("写口里换名不是恰好一处");
    assert!(
        sync < swap,
        "落盘排在换名之后 —— 换上去的那一份可能还没写到盘上"
    );
}

/// 路径超过 260 字符照样写得上（Windows 上换名要带长路径前缀；标准库自己补）。
#[test]
fn a_config_path_longer_than_260_characters_is_written() {
    let base = tmpdir("long");
    let mut dir = base.clone();
    while dir.as_os_str().len() < 300 {
        dir.push("abcdefghijklmnopqrstuvwxyz0123456789");
    }
    let file = dir.join("config.json");
    patch_config_at(&file, &[set(&["a"], json!(1))]).unwrap();
    patch_config_at(&file, &[set(&["a"], json!(2))]).unwrap();
    assert_eq!(read(&file), json!({"a": 2}));
    let _ = std::fs::remove_dir_all(&base);
}

/// 🔴 C-L3：monitor 那一份锁与后端第四层那一份是**同一种锁**（两份各一，读后端源码对拍，异源）：
/// unix 都锁**目录**（monitor：std `File::lock`；后端：`libc::flock(LOCK_EX)`）；Windows 互斥量名字的格式串逐字相等、摘要调同一个函数。
#[test]
fn hx2_the_monitor_and_backend_dir_locks_are_the_same_kind_of_lock() {
    let mine = guard_core::production_code(include_str!(
        "../../../src/frontend/shell/src/platform/fs.rs"
    ));
    let theirs = guard_core::production_code(include_str!("../../../src/backend/platform/lock.rs"));
    for needle in [
        "format!(\"{namespace}\\\\ccm-own-state-{h:016x}\")",
        "cmd_wire::fnv::fnv1a64(",
        "for ns in [\"Global\", \"Local\"] {",
    ] {
        assert_eq!(
            mine.matches(needle).count(),
            1,
            "monitor 那一份没有恰好一处 `{needle}`"
        );
        assert_eq!(
            theirs.matches(needle).count(),
            1,
            "后端那一份没有恰好一处 `{needle}`"
        );
    }
    assert_eq!(
        mine.matches("std::fs::File::open(dir)").count(),
        1,
        "monitor 那一份不是锁目录"
    );
    assert_eq!(
        mine.matches("f.lock()").count(),
        1,
        "monitor 那一份不是排他锁"
    );
    // 后端那一份读盘只经 `platform::fs::Ro`（后端除 `platform/fs/` 外不许点名 `std::fs`）⇒ 开目录那一下是 `Ro::open(dir)`，同一个只读开法。
    assert_eq!(
        theirs.matches("crate::platform::fs::Ro::open(dir)").count(),
        1,
        "后端那一份不是锁目录"
    );
    assert_eq!(
        theirs.matches("libc::LOCK_EX").count(),
        1,
        "后端那一份不是排他锁"
    );
}

/// 盘上那一步没成（这里：数据目录的位置被一个普通文件占着）⇒ 句子是「写入失败 · 那个目录 · 原因词」，系统原话进复制详情、不上句子。
#[test]
fn a_disk_failure_says_the_reason_word_and_keeps_the_os_text_in_the_detail() {
    let base = std::env::temp_dir().join(format!("ccm-cfg-io-{}-{}", std::process::id(), line!()));
    let _ = std::fs::remove_dir_all(&base);
    std::fs::create_dir_all(&base).unwrap();
    let blocker = base.join("occupied");
    std::fs::write(&blocker, b"x").unwrap();
    let path = blocker.join("sub").join("config.json");
    let edit = ConfigEdit::Set {
        path: vec!["theme".to_string()],
        value: serde_json::json!("dark"),
    };
    let Err(ConfigWriteError::Io(s)) = patch_config_at(&path, &[edit]) else {
        panic!("父路径是个普通文件，写口居然没走盘上失败那一形");
    };
    assert!(s.said.contains("occupied"), "没说清是哪个目录：{s}");
    let raw = s.raw();
    assert!(
        !raw.is_empty() && !s.said.contains(&raw),
        "系统原话没进详情 / 还在句子里：{s} · {}",
        s.detail
    );
    let _ = std::fs::remove_dir_all(&base);
}
