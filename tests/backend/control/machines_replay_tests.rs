//! 界面测试的机器表替身（`tests/frontend/ui/machines-fake.ts`）交的每一份回包都在这里现打：
//! 金样每条的输入（命令 · 盘上那一份 · 补丁）原样留着，回包 / 拒绝体由真的 `answer_read_in` · `answer_try_in` ·
//! `answer_set_in` 在临时家目录里现打（盘照输入摆）。只比不写，对过后 bless 拷回。

use crate::control::machines::{
    answer_read_in, answer_set_in, answer_try_in, path_in, MachinesEdits,
};
use crate::wire::typed::Codes as _;
use serde_json::{json, Value};

/// 替身只照这份回。
const REPLAYED: &str = "tests/__fixtures__/machines-replies.golden.json";
/// 替身遇到金样里没有的输入记在这里（本树、不进仓）；这条判据读一次就收走（收进现打的那份）。
const MISSING: &str = ".build/golden-missing/machines-replies.jsonl";

/// 一组输入的临时家目录：盘照 `disk` 摆（`null` ⇒ 没有那份；数组 ⇒ 那几台；`{text}` ⇒ 原文照写）。
fn home_with(i: usize, disk: &Value) -> std::path::PathBuf {
    let home = std::env::temp_dir().join(format!("ccm-machines-replay-{}-{i}", std::process::id()));
    let _ = std::fs::remove_dir_all(&home);
    std::fs::create_dir_all(home.join(".cc-monitor")).unwrap();
    let body = match disk {
        Value::Null => None,
        Value::Array(_) => Some(serde_json::to_string(&json!({ "hosts": disk })).unwrap()),
        Value::Object(o) if o.len() == 1 && o.get("text").is_some_and(Value::is_string) => {
            Some(o["text"].as_str().unwrap().to_string())
        }
        other => panic!("{REPLAYED}：disk 只认 null · 数组 · {{text}}，这一条是 {other}"),
    };
    if let Some(b) = body {
        std::fs::write(path_in(&home), b).unwrap();
    }
    home
}

/// 一组输入跑真的那一条：成 ⇒ `{reply}`；拒 ⇒ `{refused: {code, message, data?}}`（线上拒绝体那一形）。
fn answer_for(i: usize, case: &Value) -> Value {
    let cmd = case["cmd"]
        .as_str()
        .unwrap_or_else(|| panic!("{REPLAYED}：第 {i} 条没有 cmd"));
    let home = home_with(i, &case["disk"]);
    let edits = || -> MachinesEdits {
        serde_json::from_value(json!({ "edits": case["edits"] })).unwrap_or_else(|e| {
            panic!("{REPLAYED}：第 {i} 条的 edits 不是线上的 MachineEdit（{e}）")
        })
    };
    let out = match cmd {
        "machines-read" => json!({ "reply": answer_read_in(Some(&home)) }),
        "machines-try" => json!({ "reply": answer_try_in(Some(&home), &edits()) }),
        "machines-set" => {
            let own = crate::platform::Own::issue_at(&home);
            match answer_set_in(Some(&home), &own, &edits()) {
                Ok(r) => json!({ "reply": r }),
                Err(e) => {
                    let f = e.into_fail();
                    let mut v = json!({ "code": f.code, "message": f.message });
                    if let Some(d) = f.data {
                        v["data"] = d;
                    }
                    json!({ "refused": v })
                }
            }
        }
        other => panic!("{REPLAYED}：第 {i} 条的 cmd `{other}` 不是机器表那三条"),
    };
    let _ = std::fs::remove_dir_all(&home);
    // 回包里的路径（读不懂那一句里也有）照家目录写：临时目录换成 `~`，金样不随机器变。
    let text = serde_json::to_string(&out)
        .unwrap()
        .replace(&home.display().to_string(), "~");
    let mut v: Value = serde_json::from_str(&text).unwrap();
    steady_detail(&mut v);
    v
}

/// 复制详情里随这一刻 · 这一版变的那两行（时刻 · 机器与后端版本）摘掉：金样不随时刻与版本变，别的行原样留。
fn steady_detail(v: &mut Value) {
    match v {
        Value::Object(o) => {
            for (k, x) in o.iter_mut() {
                match x {
                    Value::String(s) if k == "detail" => {
                        *s = s
                            .lines()
                            .filter(|l| !l.starts_with("时刻：") && !l.starts_with("机器："))
                            .collect::<Vec<_>>()
                            .join("\n");
                    }
                    _ => steady_detail(x),
                }
            }
        }
        Value::Array(a) => a.iter_mut().for_each(steady_detail),
        _ => {}
    }
}

/// 一条的输入那一半（比的就是它）。
fn input_of(c: &Value) -> Value {
    json!({ "cmd": c["cmd"], "disk": c["disk"], "edits": c.get("edits").cloned().unwrap_or(Value::Null) })
}

/// ★ 机器表替身回的每一份回包都是这里现打的；替身记下的没录的输入（[`MISSING`]）一并收进现打的那份 ⇒ 红，对过后 bless 拷回。
#[test]
fn the_machines_fake_replays_replies_made_here() {
    let root = guard_core::addr::repo_root();
    let text = std::fs::read_to_string(root.join(REPLAYED))
        .unwrap_or_else(|e| panic!("读不到 {REPLAYED}：{e}"));
    let golden: Value = serde_json::from_str(&text).expect("金样不是 JSON");
    let mut inputs: Vec<Value> = golden["cases"]
        .as_array()
        .expect("金样没有 cases")
        .iter()
        .map(input_of)
        .collect();
    if let Ok(missing) = std::fs::read_to_string(root.join(MISSING)) {
        let _ = std::fs::remove_file(root.join(MISSING));
        for line in missing.lines().filter(|l| !l.trim().is_empty()) {
            let c = input_of(&serde_json::from_str::<Value>(line).expect("没录的那一行不是 JSON"));
            if !inputs.contains(&c) {
                inputs.push(c);
            }
        }
    }
    assert!(
        !inputs.is_empty(),
        "{REPLAYED} 一条都没有（空集上的回放恒真）"
    );
    let cases: Vec<Value> = inputs
        .iter()
        .enumerate()
        .map(|(i, c)| {
            let mut out = c.clone();
            if c["edits"].is_null() {
                out.as_object_mut().unwrap().remove("edits");
            }
            for (k, v) in answer_for(i, c).as_object().unwrap() {
                out[k] = v.clone();
            }
            out
        })
        .collect();
    let want = json!({
        "读法": [
            "界面测试的机器表替身（tests/frontend/ui/machines-fake.ts）只照这份回：按输入（cmd · disk 盘上那一份 · edits 补丁）找到那一条，交它的 reply，或照 refused 拒。",
            "disk：null ⇒ 没有那份；数组 ⇒ 那几台（写成 {hosts}）；{text} ⇒ 原文照写。",
            "reply / refused 由后端判据 tests/backend/control/machines_replay_tests.rs 跑真的 answer_read_in · answer_try_in · answer_set_in 现打（临时家目录，路径写成 ~；复制详情摘掉时刻与机器那两行）；只比不写，对过后 bless 拷回。",
            "替身遇到没录的输入记进本树 .build/golden-missing/machines-replies.jsonl，那条判据一并收进现打的那份。"
        ],
        "cases": cases,
    });
    crate::golden::assert_golden(
        REPLAYED,
        &(serde_json::to_string_pretty(&want).unwrap() + "\n"),
        "机器表替身的回放金样变了：界面测试照它画（remote-config · settings/remote-section · settings/data-page）",
    );
}
