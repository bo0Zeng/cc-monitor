//! `session-interrupts`：三族事实 ⇒ 按族的清单（空族不出）；任务只认 `in_progress`；sid 缺 ⇒ `bad_args`；读不到按没有答。

use super::*;
use serde_json::json;
use std::path::PathBuf;

fn scratch(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("interrupts-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn src<'a>(
    busy: &'a dyn Fn(&str) -> bool,
    agents: &'a dyn Fn(&str) -> Vec<String>,
    tasks: &'a dyn Fn(&str) -> Vec<String>,
) -> Sources<'a> {
    Sources {
        busy,
        agents,
        tasks,
    }
}

#[test]
fn families_come_out_in_order_and_empty_ones_stay_out() {
    let busy = |s: &str| s == "a";
    let agents = |s: &str| {
        if s == "a" {
            vec!["Explore".to_string()]
        } else {
            Vec::new()
        }
    };
    let none = |_: &str| Vec::new();
    let got = interrupts_of("a", &src(&busy, &agents, &none));
    assert_eq!(
        serde_json::to_value(&got).unwrap(),
        json!({"families": [{"family": "turn", "names": []}, {"family": "agent", "names": ["Explore"]}]})
    );
    // 什么都没有 ⇒ 空表（界面据此直接做、不问）。
    let idle = interrupts_of("b", &src(&busy, &agents, &none));
    assert_eq!(
        serde_json::to_value(&idle).unwrap(),
        json!({"families": []})
    );
}

#[test]
fn tasks_count_only_when_in_progress_and_a_missing_dir_is_none() {
    let home = scratch("tasks");
    let dir = home.join("tasks").join("s1");
    std::fs::create_dir_all(&dir).unwrap();
    let task = |id: &str, subject: &str, status: &str| {
        json!({"id": id, "subject": subject, "status": status}).to_string()
    };
    std::fs::write(dir.join("1.json"), task("1", "写判据", "in_progress")).unwrap();
    std::fs::write(dir.join("2.json"), task("2", "改文案", "pending")).unwrap();
    std::fs::write(dir.join("3.json"), task("3", "合并", "completed")).unwrap();
    assert_eq!(tasks_in_progress(&home, "s1"), vec!["写判据".to_string()]);
    assert_eq!(tasks_in_progress(&home, "nope"), Vec::<String>::new());
    let _ = std::fs::remove_dir_all(&home);
}

#[test]
fn the_frame_face_answers_a_shape_and_refuses_a_missing_sid() {
    let home = scratch("face");
    // 夹具家目录里什么都没有：没有 pidfile、没有任务 ⇒ 空清单（子运行那一族读本进程的活簿表，这里没有连接）。
    let v = answer_at(&home, &json!({"sid": "zz-none"})).unwrap();
    assert_eq!(v, json!({"families": []}));
    let e = answer_at(&home, &json!({})).unwrap_err();
    assert_eq!(e.0, "bad_args");
    assert_eq!(
        answer_at(&home, &json!({"sid": ""})).unwrap_err().0,
        "bad_args"
    );
    let _ = std::fs::remove_dir_all(&home);
}

#[test]
fn a_running_child_in_any_live_book_is_named() {
    // 活簿表是进程级的：新建一本、记一个在跑的子运行 ⇒ 查得到它的显示名；簿走了 ⇒ 查不到。
    let book = crate::observe::runs::RunBook::shared();
    let sid = "interrupts-test-sid";
    assert!(crate::observe::runs::running_names(sid).is_empty());
    crate::observe::runs::testing::seed_running(&book, sid, "r1", Some("Explore"));
    assert_eq!(
        crate::observe::runs::running_names(sid),
        vec!["Explore".to_string()]
    );
    drop(book);
    // 同进程里别的判据并发查活簿表时会短暂升级这本 ⇒ 有界地等它真走。
    let gone = (0..200).any(|_| {
        if crate::observe::runs::running_names(sid).is_empty() {
            return true;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
        false
    });
    assert!(gone, "簿已经没了，查到的还是它");
}

impl crate::guard_support::Shaped for SessionInterrupts {
    fn samples() -> Vec<Self> {
        vec![SessionInterrupts {
            families: vec![InterruptFamily {
                family: InterruptKind::Agent,
                names: vec!["review".into()],
            }],
        }]
    }
}
