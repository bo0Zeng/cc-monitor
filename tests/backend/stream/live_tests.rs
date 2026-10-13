//! `live.rs`：活卡成品（对账 · 墓碑 · 子运行 · 上界 · 截头 · 名单 · 发件箱满了合成整卡）。
//! 流怎么折、归哪个运行住 `runs_guard.rs` / `run_route_tests.rs`；这里用假适配层的流形（`agents::fake::runs::STREAM`）。

use super::*;
use crate::agents::StreamFamily;
use crate::relay::{TapBody, TapEvent};
use crate::stream::wire::LiveCard;

fn live() -> Live {
    Live::new(
        Arc::new(RunBook::default()),
        vec![StreamFamily {
            face: crate::agents::fake::runs::STREAM,
            owns: true,
        }],
    )
}

/// 一段流：会话 `sid` · 段号 `resp` · 自报运行 `owner`（空 ＝ 主运行）。逐件喂，自己数号。
struct Seg {
    sid: &'static str,
    owner: &'static str,
    resp: u64,
    n: u64,
}

impl Seg {
    fn new(sid: &'static str, resp: u64) -> Self {
        Self { sid, owner: "", resp, n: 0 }
    }
    fn run(sid: &'static str, owner: &'static str, resp: u64) -> Self {
        Self { sid, owner, resp, n: 0 }
    }
    fn put(&mut self, l: &mut Live, d: &str) {
        l.tap(TapEvent {
            stream: self.sid.into(),
            owner: self.owner.into(),
            resp: self.resp,
            n: self.n,
            body: TapBody::Data(d.into()),
        });
        self.n += 1;
    }
    fn open(&mut self, l: &mut Live, rid: &str) {
        self.put(l, &format!(r#"{{"ev":"open","rid":"{rid}"}}"#));
        self.put(l, r#"{"ev":"part","at":0,"is":"words"}"#);
    }
    fn say(&mut self, l: &mut Live, s: &str) {
        self.put(l, &format!(r#"{{"ev":"chunk","at":0,"txt":"{s}"}}"#));
    }
    fn shut(&mut self, l: &mut Live) {
        self.put(l, r#"{"ev":"shut"}"#);
    }
    fn end(&mut self, l: &mut Live, broken: bool) {
        l.tap(TapEvent {
            stream: self.sid.into(),
            owner: self.owner.into(),
            resp: self.resp,
            n: self.n,
            body: TapBody::End { broken },
        });
        self.n += 1;
    }
}

fn drain(l: &mut Live, id: u64) -> Vec<LiveCard> {
    let mut v = Vec::new();
    while let Some(f) = l.pull(id) {
        match f {
            Frame::LiveCard(x) => v.push(x),
            other => panic!("活卡那一路出了别的帧：{other:?}"),
        }
    }
    v
}

/// 客户端那一侧：只照帧画（`n` 不接 ⇒ 判据当场红：核心该保证同一连接上永远接得上）。
#[derive(Default, Debug)]
struct Screen {
    cards: BTreeMap<u64, (String, Option<String>, LivePhase, bool, Vec<LiveBlock>)>,
    next: HashMap<u64, u64>,
}

impl Screen {
    fn eat(&mut self, fs: &[LiveCard]) {
        for f in fs {
            match &f.op {
                LiveOp::Open { rid: _, phase, clipped, blocks } => {
                    assert_eq!(f.n, 0, "整卡的 n 不是 0：{f:?}");
                    self.cards.insert(f.card, (f.sid.clone(), f.run.clone(), *phase, *clipped, blocks.clone()));
                }
                op => {
                    assert_eq!(self.next.get(&f.card).copied(), Some(f.n), "n 没接上：{f:?}");
                    let c = self.cards.get_mut(&f.card).expect("没开过的卡来了增量");
                    match op {
                        LiveOp::Block { i, block, tool } => {
                            let i = *i as usize;
                            while c.4.len() <= i {
                                c.4.push(LiveBlock { kind: BlockKind::Other, text: None, tool: None });
                            }
                            c.4[i] = LiveBlock {
                                kind: *block,
                                text: (*block == BlockKind::Text).then(String::new),
                                tool: tool.clone(),
                            };
                        }
                        LiveOp::Text { i, s } => c.4[*i as usize].text.as_mut().unwrap().push_str(s),
                        LiveOp::Awaiting => c.2 = LivePhase::Awaiting,
                        LiveOp::Close { .. } => {
                            self.cards.remove(&f.card);
                            self.next.remove(&f.card);
                            continue;
                        }
                        LiveOp::Open { .. } => unreachable!(),
                    }
                }
            }
            self.next.insert(f.card, f.n + 1);
        }
    }
    fn text(&self, card: u64) -> Option<String> {
        self.cards.get(&card).map(|c| c.4.iter().filter_map(|b| b.text.clone()).collect())
    }
}

fn closes(fs: &[LiveCard]) -> Vec<(String, u64, LiveWhy)> {
    fs.iter()
        .filter_map(|f| match f.op {
            LiveOp::Close { why } => Some((f.sid.clone(), f.card, why)),
            _ => None,
        })
        .collect()
}

/// B1：两条会话同时在说、名单只报 A ⇒ 这条连接收到的全是 A 的。
#[test]
fn a_connection_only_gets_the_sessions_on_its_watch_list() {
    let mut l = live();
    let (id, _) = l.join();
    l.watch(id, &["A".into()]);
    let (mut a, mut b) = (Seg::new("A", 1), Seg::new("B", 2));
    a.open(&mut l, "ra");
    b.open(&mut l, "rb");
    for _ in 0..5 {
        a.say(&mut l, "x");
        b.say(&mut l, "y");
    }
    let fs = drain(&mut l, id);
    assert!(!fs.is_empty(), "A 的一帧都没有");
    assert!(fs.iter().all(|f| f.sid == "A"), "名单外的会话上了这条流：{fs:?}");
    let mut s = Screen::default();
    s.eat(&fs);
    assert_eq!(s.text(1).as_deref(), Some("xxxxx"));
}

/// B1：没报过名单 ＝ 全看（同 `line`）。
#[test]
fn a_connection_that_never_reported_a_list_gets_every_session() {
    let mut l = live();
    let (id, _) = l.join();
    let (mut a, mut b) = (Seg::new("A", 1), Seg::new("B", 2));
    a.open(&mut l, "ra");
    b.open(&mut l, "rb");
    let sids: HashSet<String> = drain(&mut l, id).into_iter().map(|f| f.sid).collect();
    assert_eq!(sids, ["A", "B"].into_iter().map(String::from).collect());
}

/// B1：一个正在说的会话刚进名单 ⇒ 先拿到它此刻的整张卡（已到的正文都在），之后接着长；出名单 ⇒ 撤卡。
#[test]
fn joining_the_list_mid_reply_hands_over_the_whole_card_and_leaving_closes_it() {
    let mut l = live();
    let (id, _) = l.join();
    l.watch(id, &["A".into()]);
    let mut b = Seg::new("B", 2);
    b.open(&mut l, "rb");
    b.say(&mut l, "hel");
    b.say(&mut l, "lo");
    assert!(drain(&mut l, id).is_empty());
    l.watch(id, &["A".into(), "B".into()]);
    b.say(&mut l, "!");
    let fs = drain(&mut l, id);
    assert!(matches!(fs[0].op, LiveOp::Open { .. }), "刚进名单头一帧不是整卡：{fs:?}");
    let mut s = Screen::default();
    s.eat(&fs);
    assert_eq!(s.text(2).as_deref(), Some("hello!"));
    l.watch(id, &["A".into()]);
    let fs = drain(&mut l, id);
    assert_eq!(closes(&fs), vec![("B".into(), 2, LiveWhy::Unwatched)]);
}

/// 后接上的连接：此刻每张卡先拿一张整卡。
#[test]
fn a_late_connection_gets_every_card_whole() {
    let mut l = live();
    let mut a = Seg::new("A", 1);
    a.open(&mut l, "ra");
    a.say(&mut l, "abc");
    let (id, _) = l.join();
    let mut s = Screen::default();
    s.eat(&drain(&mut l, id));
    assert_eq!(s.text(1).as_deref(), Some("abc"));
}

/// 记录到了 ⇒ 同 `rid` 的卡撤（整轮覆盖）；说完之后、记录之前停在「等写入记录」。
#[test]
fn the_record_closes_its_card_and_a_finished_card_waits_for_it() {
    let mut l = live();
    let (id, _) = l.join();
    let mut a = Seg::new("A", 1);
    a.open(&mut l, "ra");
    a.say(&mut l, "hi");
    a.shut(&mut l);
    a.end(&mut l, false);
    let mut s = Screen::default();
    s.eat(&drain(&mut l, id));
    assert_eq!(s.cards[&1].2, LivePhase::Awaiting);
    l.record("A", "ra", Some(None));
    let fs = drain(&mut l, id);
    assert_eq!(closes(&fs), vec![("A".into(), 1, LiveWhy::Record)]);
}

/// 记录比流先到 ⇒ 那张卡不出现。
#[test]
fn a_record_that_came_first_keeps_its_card_from_ever_showing() {
    let mut l = live();
    let (id, _) = l.join();
    l.record("A", "ra", Some(None));
    let mut a = Seg::new("A", 1);
    a.open(&mut l, "ra");
    a.say(&mut l, "late");
    assert!(drain(&mut l, id).is_empty());
}

/// 同一运行里已说完而 `rid` 不同的卡：这个运行来了别的记录 ⇒ 撤（它的记录不会再来了）。正在说的不撤。
#[test]
fn a_finished_card_of_the_same_run_goes_when_another_record_of_that_run_lands() {
    let mut l = live();
    let (id, _) = l.join();
    let (mut a, mut b) = (Seg::new("A", 1), Seg::new("A", 2));
    a.open(&mut l, "r1");
    a.shut(&mut l);
    b.open(&mut l, "r2");
    drain(&mut l, id);
    l.record("A", "r0", Some(None));
    let fs = drain(&mut l, id);
    assert_eq!(closes(&fs), vec![("A".into(), 1, LiveWhy::Record)]);
}

/// 子运行的卡带 `run`（主 tab 不画），也照样等它自己的记录。
#[test]
fn a_sub_run_card_carries_its_run_and_waits_for_its_own_record() {
    let mut l = live();
    let (id, _) = l.join();
    let mut r = Seg::run("A", "a1", 3);
    r.open(&mut l, "rr");
    r.shut(&mut l);
    let fs = drain(&mut l, id);
    assert!(!fs.is_empty());
    assert!(fs.iter().all(|f| f.run.as_deref() == Some("a1")), "{fs:?}");
    l.record("A", "rr", None);
    assert_eq!(closes(&drain(&mut l, id)), vec![("A".into(), 3, LiveWhy::Record)]);
}

/// 断在半路 ⇒ 撤；报错收尾 ⇒ 撤；说完以后再断（内容是全的）⇒ 照等记录。
#[test]
fn a_broken_or_failed_reply_is_closed_but_one_broken_after_it_finished_is_kept() {
    let mut l = live();
    let (id, _) = l.join();
    let (mut a, mut b, mut c) = (Seg::new("A", 1), Seg::new("A", 2), Seg::new("B", 3));
    a.open(&mut l, "r1");
    b.open(&mut l, "r2");
    c.open(&mut l, "r3");
    drain(&mut l, id);
    a.end(&mut l, true);
    b.put(&mut l, r#"{"ev":"fail"}"#);
    c.shut(&mut l);
    c.end(&mut l, true);
    let got = closes(&drain(&mut l, id));
    assert_eq!(
        got,
        vec![("A".into(), 1, LiveWhy::Broken), ("A".into(), 2, LiveWhy::Error)],
        "断的 · 报错的该撤，说完以后才断的那张不该撤"
    );
}

/// 每个运行最多两张：第三张来了挤掉最老的（先挤说完的）。
#[test]
fn a_run_keeps_at_most_two_cards() {
    let mut l = live();
    let (id, _) = l.join();
    let mut fs = Vec::new();
    for k in 1..=3 {
        Seg::new("A", k).open(&mut l, &format!("r{k}"));
        fs.extend(drain(&mut l, id));
    }
    assert_eq!(closes(&fs), vec![("A".into(), 1, LiveWhy::Evicted)]);
    assert_eq!(l.cards.len(), LIVE_PER_RUN);
}

/// 整台最多 16 张：只剩正在说的主运行时，新来的子运行不收、主运行照收。
#[test]
fn the_machine_keeps_at_most_sixteen_cards_and_never_drops_a_speaking_main_run_for_a_sub_run() {
    let mut l = live();
    for k in 0..LIVE_CARDS_KEEP as u64 {
        Seg::new(["s0", "s1", "s2", "s3", "s4", "s5", "s6", "s7", "s8", "s9", "sa", "sb", "sc", "sd", "se", "sf"][k as usize], k).open(&mut l, "r");
    }
    Seg::run("x", "a1", 100).open(&mut l, "rr");
    assert!(!l.cards.contains_key(&100), "满了还收了子运行");
    Seg::new("y", 101).open(&mut l, "r");
    assert!(l.cards.contains_key(&101), "主运行没收");
}

/// 正文留尾：超过留尾 ＋ 余量才截一次，截回正好留尾，整张重发、标「截过头」。
#[test]
fn a_long_reply_keeps_only_its_tail_and_is_resent_whole_once_clipped() {
    let mut l = live();
    let (id, _) = l.join();
    let mut a = Seg::new("A", 1);
    a.open(&mut l, "r");
    let mut fs = drain(&mut l, id);
    let chunk = "0123456789".repeat(100); // 1000 字
    for _ in 0..((LIVE_TEXT_KEEP + LIVE_TEXT_SLACK) / 1000 + 1) {
        a.say(&mut l, &chunk);
        fs.extend(drain(&mut l, id));
    }
    let opens = fs.iter().filter(|f| matches!(f.op, LiveOp::Open { .. })).count();
    assert_eq!(opens, 2, "截头那一次没有整张重发（或重发了不止一次）");
    let mut s = Screen::default();
    s.eat(&fs);
    assert_eq!(s.text(1).unwrap().chars().count(), LIVE_TEXT_KEEP);
    assert!(s.cards[&1].3, "没标截过头");
}

/// 发件箱满了：不丢件，那张卡合成一张整卡 —— 客户端照画，得到的正文与核心一字不差、`n` 处处接得上。
#[test]
fn a_full_outbox_folds_a_card_into_one_whole_card_instead_of_losing_pieces() {
    let mut l = live();
    let (id, _) = l.join();
    let mut a = Seg::new("A", 1);
    a.open(&mut l, "r");
    let mut s = Screen::default();
    s.eat(&drain(&mut l, id)); // 先拿到整卡：之后来的都是增量，才会把发件箱灌满
    let mut want = String::new();
    for k in 0..(LIVE_OUTBOX_CAP * 3) {
        let piece = format!("{k},");
        a.say(&mut l, &piece);
        want.push_str(&piece);
    }
    let fs = drain(&mut l, id);
    assert!(fs.len() <= LIVE_OUTBOX_CAP, "发件箱没封顶：{} 件", fs.len());
    s.eat(&fs);
    assert_eq!(s.text(1).as_deref(), Some(want.as_str()));
}

/// 连接走了 ⇒ 它那一份（名单 · 发件箱）摘掉，不再攒。
#[test]
fn a_gone_connection_is_dropped_with_its_outbox() {
    let mut l = live();
    let (id, _) = l.join();
    l.leave(id);
    Seg::new("A", 1).open(&mut l, "r");
    assert!(l.conns.is_empty(), "走了的连接没摘掉");
    assert!(l.pull(id).is_none());
}

/// 会话走了 ⇒ 它的卡全撤。
#[test]
fn a_session_that_goes_takes_its_cards_with_it() {
    let mut l = live();
    let (id, _) = l.join();
    Seg::new("A", 1).open(&mut l, "r");
    drain(&mut l, id);
    l.gone("A");
    assert_eq!(closes(&drain(&mut l, id)), vec![("A".into(), 1, LiveWhy::Gone)]);
}

/// 读数（不是判据）：两条会话各流同样一段、名单只报 A，这条连接收到的逐字流字节（改前用 `tap` 帧量过：408 帧 · 34148 字节）。
/// 跑法：`cargo test --lib -- --ignored --nocapture stream::live::tests::live_bytes_reading`
#[test]
#[ignore]
fn live_bytes_reading() {
    let mut l = live();
    let (id, _) = l.join();
    l.watch(id, &["A".into()]);
    let mut bytes = 0usize;
    let mut frames = 0usize;
    for (resp, sid) in [(1u64, "A"), (2u64, "B")] {
        let mut s = Seg::new(sid, resp);
        s.open(&mut l, &format!("r{resp}"));
        for k in 0..200 {
            s.say(&mut l, &format!("word{k} "));
            for f in drain(&mut l, id) {
                bytes += crate::stream::wire::to_line(&Frame::LiveCard(f)).unwrap().len() + 1;
                frames += 1;
            }
        }
        s.shut(&mut l);
        s.end(&mut l, false);
    }
    for f in drain(&mut l, id) {
        bytes += crate::stream::wire::to_line(&Frame::LiveCard(f)).unwrap().len() + 1;
        frames += 1;
    }
    println!("READING after: frames={frames} bytes={bytes}");
}
