//! `common/views.rs` 的判据：每一问都照答换到手上（`Hand::apply`）、再与这一问此刻那一窗逐项相等；再看答的形（没变 · 只这几项 · 整份）。
//! 金样 `sync-patch.golden.jsonl` 由这里真答出来写（Rust 与手机那两份 `apply` 照它逐条）。

use super::*;
use cmd_wire::sync::Hand;
use serde_json::{json, Value};

/// 一种测试用的集合：键 ＋ 成品；`cut` ＝ 这一问截到几项（截断挤进来的那一形）。
#[derive(Clone)]
struct Src {
    ask: String,
    info: String,
    rows: Vec<(String, Value)>,
    cut: Option<usize>,
}

impl ViewSource for Src {
    type Head = String;
    type Item = Value;
    fn ask_key(&self) -> String {
        self.ask.clone()
    }
    fn now(&self) -> (String, Vec<(String, Value)>) {
        let rows = self
            .rows
            .iter()
            .take(self.cut.unwrap_or(usize::MAX))
            .cloned()
            .collect();
        (self.info.clone(), rows)
    }
}

fn src(keys: &[&str]) -> Src {
    Src {
        ask: "a".into(),
        info: "i".into(),
        rows: keys
            .iter()
            .map(|k| (k.to_string(), json!({ "name": k })))
            .collect(),
        cut: None,
    }
}

fn set(s: &mut Src, key: &str, v: Value) {
    s.rows
        .iter_mut()
        .find(|(k, _)| k == key)
        .expect("有这一项")
        .1 = v;
}

type H = Hand<String, Value>;

/// 这一问此刻那一窗（直接从集合取，不经备忘）。
fn truth(s: &Src, range: Option<Range>) -> H {
    let (info, all) = s.now();
    let total = all.len() as u32;
    let win: Vec<(String, Value)> = match range {
        Some(r) => all
            .into_iter()
            .skip(r.from as usize)
            .take(r.limit as usize)
            .collect(),
        None => all,
    };
    Hand {
        held: None,
        order: win.iter().map(|(k, _)| k.clone()).collect(),
        items: win.into_iter().collect(),
        head: Some(Head { total, info }),
    }
}

/// 问一次、照答换到手上、手上那份必须就是此刻那一窗（记号不比）。
fn ask(v: &Views, s: &Src, hand: &mut H, range: Option<Range>) -> Patch<String, Value> {
    let p = v.answer(s, hand.held.as_ref(), range);
    hand.apply(p.clone());
    let mut want = truth(s, range);
    want.held = hand.held.clone();
    assert_eq!(*hand, want, "照答换完与此刻那一窗不同；答：{p:?}");
    p
}

fn keys(p: &Patch<String, Value>) -> Vec<&str> {
    p.up.keys().map(String::as_str).collect()
}

fn win(from: u32, limit: u32) -> Option<Range> {
    Some(Range { from, limit })
}

#[test]
fn the_first_ask_is_whole_and_an_unchanged_one_is_same() {
    let v = Views::new(4);
    let s = src(&["a", "b", "c"]);
    let mut hand = H::default();
    let p = ask(&v, &s, &mut hand, None);
    assert!(p.whole && !p.same);
    let p = ask(&v, &s, &mut hand, None);
    assert!(p.same && !p.whole, "{p:?}");
    assert!(
        p.up.is_empty() && p.order.is_none() && p.head.is_none(),
        "{p:?}"
    );
}

#[test]
fn one_changed_item_comes_alone_without_order_or_head() {
    let v = Views::new(4);
    let mut s = src(&["a", "b", "c"]);
    let mut hand = H::default();
    ask(&v, &s, &mut hand, None);
    set(&mut s, "b", json!({ "name": "b", "n": 2 }));
    let p = ask(&v, &s, &mut hand, None);
    assert!(!p.whole && !p.same);
    assert_eq!(keys(&p), ["b"]);
    assert!(p.order.is_none() && p.head.is_none(), "{p:?}");
}

/// 截到两项：前面一项没了 ⇒ 第三项被挤进来，它没变过也进 `up`；先后变了 ⇒ 带 `order`。
#[test]
fn an_item_pushed_in_by_the_cut_comes_up() {
    let v = Views::new(4);
    let mut s = src(&["a", "b", "c"]);
    s.cut = Some(2);
    let mut hand = H::default();
    ask(&v, &s, &mut hand, None);
    s.rows.remove(0);
    let p = ask(&v, &s, &mut hand, None);
    assert_eq!(keys(&p), ["c"]);
    assert_eq!(
        p.order.as_deref(),
        Some(&["b".to_string(), "c".to_string()][..])
    );
}

/// 出去又回来（筛掉再补上的父项也是这一形）：内容一字没变也进 `up`。
#[test]
fn an_item_that_left_and_came_back_comes_up() {
    let v = Views::new(4);
    let mut s = src(&["a", "b", "c"]);
    let mut hand = H::default();
    ask(&v, &s, &mut hand, None);
    let b = s.rows.remove(1);
    let p = ask(&v, &s, &mut hand, None);
    assert!(p.up.is_empty() && p.order.is_some(), "{p:?}");
    s.rows.insert(1, b);
    let p = ask(&v, &s, &mut hand, None);
    assert_eq!(keys(&p), ["b"]);
}

/// 先后变了、各项没变 ⇒ 只带 `order`；先后没变就不带（上面那几条也核着）。
#[test]
fn a_reorder_carries_only_the_order() {
    let v = Views::new(4);
    let mut s = src(&["a", "b", "c"]);
    let mut hand = H::default();
    ask(&v, &s, &mut hand, None);
    s.rows.swap(0, 2);
    let p = ask(&v, &s, &mut hand, None);
    assert!(
        p.up.is_empty() && p.head.is_none() && p.order.is_some(),
        "{p:?}"
    );
}

/// 另一份 `Views`（那台重启过）的记号 · 另一种问法的记号 · 认不出的记号 ⇒ 整份。
#[test]
fn a_held_from_another_process_or_another_ask_is_whole() {
    let v = Views::new(4);
    let s = src(&["a", "b"]);
    let mut hand = H::default();
    ask(&v, &s, &mut hand, None);
    let other = Views::new(4);
    let p = other.answer(&s, hand.held.as_ref(), None);
    assert!(p.whole, "别的进程的记号：{p:?}");
    let mut t = s.clone();
    t.ask = "b".into();
    let p = v.answer(&t, hand.held.as_ref(), None);
    assert!(p.whole, "别的问法的记号：{p:?}");
    for junk in ["", "x", "1.2", "1.2.3.4"] {
        assert!(
            v.answer(&s, Some(&Held(junk.into())), None).whole,
            "{junk:?}"
        );
    }
    assert!(
        v.answer(&s, hand.held.as_ref(), None).same,
        "正控：自己的记号照样认"
    );
}

/// 问法多过 `cap` 份 ⇒ 最久没问的那份被挤掉、再问是整份；刚问过的那份还在。
#[test]
fn a_memo_pushed_out_of_the_lru_is_whole() {
    let v = Views::new(2);
    let mut hands: Vec<(Src, H)> = ["p", "q", "r"]
        .iter()
        .map(|a| {
            let mut s = src(&["a"]);
            s.ask = a.to_string();
            (s, H::default())
        })
        .collect();
    let (s, h) = &mut hands[0];
    ask(&v, s, h, None);
    let (s, h) = &mut hands[1];
    ask(&v, s, h, None);
    let (s, h) = &mut hands[0];
    assert!(ask(&v, s, h, None).same, "p 刚问过");
    let (s, h) = &mut hands[2];
    ask(&v, s, h, None);
    let (s, h) = &mut hands[1];
    assert!(ask(&v, s, h, None).whole, "q 最久没问，被 r 挤掉");
}

/// 只要一窗 ⇒ 只回这一窗的项与键，`head` 带整份的总数。
#[test]
fn a_window_answers_only_that_window_with_the_total() {
    let v = Views::new(4);
    let s = src(&["a", "b", "c", "d", "e"]);
    let mut hand = H::default();
    let p = ask(&v, &s, &mut hand, win(1, 2));
    assert_eq!(keys(&p), ["b", "c"]);
    assert_eq!(
        p.order.as_deref(),
        Some(&["b".to_string(), "c".to_string()][..])
    );
    assert_eq!(p.head.as_ref().map(|h| h.total), Some(5));
}

/// 窗外变了 ⇒ 这一窗 `same`；窗外多了一项（总数变了）⇒ 只带 `head`。
#[test]
fn a_change_outside_the_window_is_same_unless_the_total_moved() {
    let v = Views::new(4);
    let mut s = src(&["a", "b", "c", "d", "e"]);
    let mut hand = H::default();
    ask(&v, &s, &mut hand, win(0, 2));
    set(&mut s, "d", json!({ "name": "d", "n": 1 }));
    assert!(ask(&v, &s, &mut hand, win(0, 2)).same);
    s.rows.push(("f".into(), json!({ "name": "f" })));
    let p = ask(&v, &s, &mut hand, win(0, 2));
    assert!(p.up.is_empty() && p.order.is_none(), "{p:?}");
    assert_eq!(p.head.as_ref().map(|h| h.total), Some(6));
}

/// 窗口挪了 ⇒ 新窗里手上没有的进 `up`，两窗重叠的那几项不再给。
#[test]
fn a_moved_window_sends_only_what_the_hand_lacks() {
    let v = Views::new(4);
    let s = src(&["a", "b", "c", "d", "e"]);
    let mut hand = H::default();
    ask(&v, &s, &mut hand, win(0, 3));
    let p = ask(&v, &s, &mut hand, win(2, 3));
    assert!(!p.whole);
    assert_eq!(keys(&p), ["d", "e"]);
}

/// 两个客户端各看各的窗、交替问、集合一路在变：每一问照答换完都等于此刻那一窗（`ask` 里核），
/// 只有头一次是整份，紧接着再问同一窗答 `same`。
#[test]
fn two_hands_on_one_memo_stay_exact() {
    let v = Views::new(4);
    let mut s = src(&["a", "b", "c", "d", "e", "f", "g", "h"]);
    let mut hands = [H::default(), H::default()];
    let mut seed: u64 = 7;
    let mut roll = |n: u64| {
        seed = seed
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        (seed >> 33) % n
    };
    let (mut sames, mut wholes) = (0, 0);
    for step in 0..400 {
        match roll(5) {
            0 => {
                let i = roll(s.rows.len() as u64) as usize;
                s.rows[i].1 = json!({ "name": s.rows[i].0, "n": step });
            }
            1 if s.rows.len() > 2 => {
                s.rows.remove(roll(s.rows.len() as u64) as usize);
            }
            2 => {
                let at = roll(s.rows.len() as u64 + 1) as usize;
                s.rows
                    .insert(at, (format!("k{step}"), json!({ "name": step })));
            }
            3 => {
                let (i, j) = (
                    roll(s.rows.len() as u64) as usize,
                    roll(s.rows.len() as u64) as usize,
                );
                s.rows.swap(i, j);
            }
            _ => {}
        }
        let who = roll(2) as usize;
        let range = win(roll(4) as u32, 1 + roll(4) as u32);
        if ask(&v, &s, &mut hands[who], range).whole {
            wholes += 1;
        }
        if ask(&v, &s, &mut hands[who], range).same {
            sames += 1;
        }
    }
    assert_eq!(wholes, 2, "记号一直作数 ⇒ 只有两只手头一次是整份");
    assert_eq!(sames, 400, "同一只手紧接着再问同一窗 ⇒ 每次都是 same");
}

// ── 金样 ──────────────────────────────────────────────────────────────

const GOLDEN: &str = "tests/__fixtures__/sync-patch.golden.jsonl";

/// 记号按头一次出现的先后换成 `h1` `h2` …
fn canon(h: &Option<Held>, names: &mut BTreeMap<String, String>) -> Value {
    let Some(h) = h else { return Value::Null };
    let n = names.len() + 1;
    Value::String(
        names
            .entry(h.0.clone())
            .or_insert_with(|| format!("h{n}"))
            .clone(),
    )
}

/// 一段真答出来的过程（整份 · 没变 · 改一项 · 挤进来 · 换先后 · 窗口 · 只变总数 · 窗口挪了 · 记号作废），每一问一行：
/// 问之前手上那份 · 那一答 · 此刻那一窗。记号换成 `h1` `h2` …（随进程变的那一段不进金样）。
/// 形状是有意改的 ⇒ `REGOLD_SYNC_PATCH=1` 重打，Rust 与手机两份 `apply` 一起对。
#[test]
fn the_golden_is_what_the_views_really_answer() {
    let mut lines = Vec::new();
    let mut names: BTreeMap<String, String> = BTreeMap::new();
    let v = Views::new(4);
    let mut s = src(&["a", "b", "c", "d", "e"]);
    s.cut = Some(3);
    let mut hand = H::default();
    let mut step = |case: &str,
                    s: &Src,
                    hand: &mut H,
                    range: Option<Range>,
                    names: &mut BTreeMap<String, String>| {
        let before = hand.clone();
        let p = ask(&v, s, hand, range);
        let mut b = serde_json::to_value(&before).unwrap();
        b["held"] = canon(&before.held, names);
        let mut pv = serde_json::to_value(&p).unwrap();
        pv["held"] = canon(&Some(p.held.clone()), names);
        let mut w = serde_json::to_value(&*hand).unwrap();
        w["held"] = canon(&hand.held, names);
        lines.push(json!({ "case": case, "hand": b, "patch": pv, "want": w }).to_string());
    };
    step("first ask: whole", &s, &mut hand, None, &mut names);
    step("unchanged: same", &s, &mut hand, None, &mut names);
    set(&mut s, "b", json!({ "name": "b", "n": 2 }));
    step("one item changed", &s, &mut hand, None, &mut names);
    s.rows.remove(0);
    step(
        "first left, the cut pushes one in",
        &s,
        &mut hand,
        None,
        &mut names,
    );
    s.rows.swap(0, 1);
    step("reorder only", &s, &mut hand, None, &mut names);
    step(
        "window: whole for a new hand",
        &s,
        &mut H::default(),
        win(0, 2),
        &mut names,
    );
    s.cut = None;
    let mut w = H::default();
    step("window: first", &s, &mut w, win(0, 2), &mut names);
    s.rows.push(("f".into(), json!({ "name": "f" })));
    step(
        "window: only the total moved",
        &s,
        &mut w,
        win(0, 2),
        &mut names,
    );
    step("window moved", &s, &mut w, win(1, 3), &mut names);
    w.held = Some(Held("stale".into()));
    step("held not ours: whole", &s, &mut w, win(1, 3), &mut names);
    let produced = lines.join("\n") + "\n";
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(GOLDEN);
    if std::env::var_os("REGOLD_SYNC_PATCH").is_some() {
        std::fs::write(&path, &produced).expect("写金样");
    }
    let on_disk = std::fs::read_to_string(&path).unwrap_or_default();
    assert!(
        on_disk == produced,
        "带键集合的金样与真答的不一致（有意改的 ⇒ `REGOLD_SYNC_PATCH=1` 重打）：\n{produced}"
    );
}
