//! 〔CFG1 · 4D〕`config.json` 唯一写口 `config::patch_config_at` 的判据。
//!
//! 守的要求（住址）：
//! - `设计/30 §4`：「两者 …… 各自只写自己那个键」；`§C.3`：「**只动自己那个键** …… 不整段覆盖」。
//! - `设计/70 §287` 红线 ④：「**读不懂的 `config.json` 不写** …… 盘上一个字节不动（不退回『当成空对象覆盖』，
//!   那会把用户其余配置一起抹掉）」。
//!
//! J2（并发不丢）打真文件、真线程；J3（补丁语义）期望全是手写 JSON 字面量，不经被测代码生成。
//! 前端那一半（每个写者只交自己的路径、两个 realm 同写）在 `tests/config-lost-update.vitest.ts`。
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

/// J3：`70 §287` 红线 ④ —— 读不懂 / 根不是对象 ⇒ `Unreadable`、盘上字节不变、没有临时件。
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
            msg.contains("config.json") && msg.contains("没有存"),
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

/// 线上形状：前端交来的 JSON（`src/generated/ConfigEdit.ts` 的形状）能按原样解出来。手写字面量。
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
/// `refused` ⇒ 错误种类对得上、盘上字节不变。前端测试用的假盘跑同一份（`tests/config-patch-fake.vitest.ts`）。
#[test]
fn the_golden_cases_hold() {
    let golden: Value =
        serde_json::from_str(include_str!("../__fixtures__/config-patch.golden.json")).unwrap();
    let cases = golden["cases"].as_array().unwrap();
    assert_eq!(
        cases.len(),
        8,
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
