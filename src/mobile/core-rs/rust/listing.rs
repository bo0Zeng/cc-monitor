//! **会话列表副本在手机上**：一台一份 `history-list` 不带筛选那一问手上那一份（[`cmd_wire::sync::Hand`]，换法是两端同一份
//! `Hand::apply`），照那台的答换、存进 App 缓存目录里那一台的一份文件（目录由 Kotlin 装核心时交）。Kotlin 只交答、拿整份。
//! 这是缓存：文件丢了 · 读不动 ⇒ 当没问过（下次不带记号问，那台答整份）。

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;

use cmd_wire::listing::{HistoryHead, HistoryRow};
use cmd_wire::sync::{Hand, Patch};

/// 手上那一份。
pub type ListHand = Hand<HistoryHead, HistoryRow>;
/// 那台的一答 · 交 Kotlin 的整份（`whole` 那一形，记号是手上那一份的）。
pub type ListPatch = Patch<HistoryHead, HistoryRow>;

pub struct Lists {
    dir: PathBuf,
    /// 读过盘的那几台（进程内一份；换了就整份写回）。
    held: Mutex<HashMap<String, ListHand>>,
}

impl Lists {
    pub fn new(dir: PathBuf) -> Lists {
        Lists {
            dir,
            held: Mutex::new(HashMap::new()),
        }
    }

    /// 这台那一份文件：名字是机器 id 的十六进制（id 里的字跑不出这个目录）。
    fn file(&self, host: &str) -> PathBuf {
        let name: String = host.bytes().map(|b| format!("{b:02x}")).collect();
        self.dir.join(format!("{name}.json"))
    }

    /// 进程内那一份；没读过 ⇒ 照盘上读（没有 · 读不动 ⇒ 空的那一份）。
    fn with<R>(&self, host: &str, f: impl FnOnce(&mut ListHand) -> R) -> R {
        let mut held = self.held.lock().unwrap_or_else(|e| e.into_inner());
        let hand = held.entry(host.to_string()).or_insert_with(|| {
            std::fs::read(self.file(host))
                .ok()
                .and_then(|b| serde_json::from_slice(&b).ok())
                .unwrap_or_default()
        });
        f(hand)
    }

    /// 整份写回：先写旁边一份再换名（写到一半断电不留半份）。写不成就算了（缓存，下次那台再答整份）。
    fn store(&self, host: &str, hand: &ListHand) {
        let path = self.file(host);
        let tmp = path.with_extension("tmp");
        let Ok(bytes) = serde_json::to_vec(hand) else {
            return;
        };
        let _ = std::fs::create_dir_all(&self.dir)
            .and_then(|()| std::fs::write(&tmp, bytes))
            .and_then(|()| std::fs::rename(&tmp, &path));
    }

    /// 这台手上那一份摊成整份；没问过 ⇒ `None`（不带记号问）。
    pub fn held(&self, host: &str) -> Option<ListPatch> {
        self.with(host, |h| whole(h))
    }

    /// 那台的一答（`history-list` 回包的 `data`）换上、存盘，回换好的整份。答读不动 ⇒ `None`，手上那一份不动。
    pub fn keep(&self, host: &str, reply: &serde_json::Value) -> Option<ListPatch> {
        let patch: ListPatch = serde_json::from_value(reply.clone()).ok()?;
        self.with(host, |h| {
            h.apply(patch);
            self.store(host, h);
            whole(h)
        })
    }

    /// 删了一台：手上那一份与它的文件一并丢掉。
    pub fn forget(&self, host: &str) {
        self.held
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(host);
        let _ = std::fs::remove_file(self.file(host));
    }
}

/// 手上那一份 ⇒ 整份那一答（先后里只留手上有的那几项）；还没有记号（没答过）⇒ `None`。
fn whole(h: &ListHand) -> Option<ListPatch> {
    let held = h.held.clone()?;
    Some(Patch {
        held,
        whole: true,
        same: false,
        order: Some(
            h.order
                .iter()
                .filter(|k| h.items.contains_key(*k))
                .cloned()
                .collect(),
        ),
        up: h.items.clone(),
        head: h.head.clone(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Value};

    fn golden() -> Value {
        serde_json::from_str(include_str!(
            "../../../../tests/__fixtures__/history-list.golden.json"
        ))
        .unwrap()
    }

    /// 一个只给这一条判据的目录（沙箱：系统临时目录下按进程号与名字分开）。
    fn lists(name: &str) -> (Lists, PathBuf) {
        let dir = std::env::temp_dir().join(format!("mobcore-lists-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        (Lists::new(dir.clone()), dir)
    }

    fn order(p: &ListPatch) -> Vec<String> {
        p.order.clone().unwrap_or_default()
    }

    #[test]
    fn a_host_never_asked_has_nothing_in_hand() {
        let (l, _) = lists("none");
        assert!(l.held("h").is_none());
    }

    #[test]
    fn the_first_whole_answer_is_kept_and_reads_back_after_a_restart() {
        let (l, dir) = lists("first");
        let g = golden();
        let got = l.keep("h", &g).expect("那台的整份答要换得上");
        assert!(got.whole && !got.same);
        assert_eq!(got.held.0, g["held"].as_str().unwrap());
        let want: Vec<String> = serde_json::from_value(g["order"].clone()).unwrap();
        assert_eq!(order(&got), want);
        assert_eq!(got.up.len(), want.len());
        // 进程重来：照盘上那一份读回，同一份。
        let again = Lists::new(dir);
        let back = again.held("h").expect("盘上那一份读得回");
        assert_eq!(back.held, got.held);
        assert_eq!(order(&back), want);
        assert_eq!(back.head.map(|h| h.total), got.head.map(|h| h.total));
    }

    #[test]
    fn an_unchanged_answer_only_moves_the_mark() {
        let (l, _) = lists("same");
        let g = golden();
        let first = l.keep("h", &g).unwrap();
        let same = json!({"held": "h2", "whole": false, "same": true, "order": null, "up": {}, "head": null});
        let got = l.keep("h", &same).unwrap();
        assert_eq!(got.held.0, "h2");
        assert_eq!(order(&got), order(&first));
        assert_eq!(l.held("h").unwrap().held.0, "h2");
    }

    #[test]
    fn a_partial_answer_drops_the_rows_that_left_the_window() {
        let (l, _) = lists("part");
        let first = l.keep("h", &golden()).unwrap();
        let keep = order(&first)[..2].to_vec();
        let part = json!({"held": "h3", "whole": false, "same": false, "order": keep, "up": {}, "head": null});
        let got = l.keep("h", &part).unwrap();
        assert_eq!(order(&got), keep);
        assert_eq!(got.up.len(), 2);
        assert!(got.head.is_some(), "没带头 ⇒ 头照旧");
    }

    #[test]
    fn an_unreadable_answer_leaves_the_hand_alone() {
        let (l, _) = lists("bad");
        let first = l.keep("h", &golden()).unwrap();
        assert!(l.keep("h", &json!({"rows": []})).is_none());
        let back = l.held("h").unwrap();
        assert_eq!(back.held, first.held);
        assert_eq!(order(&back), order(&first));
    }

    #[test]
    fn hosts_are_kept_apart_and_forgetting_one_drops_its_file() {
        let (l, dir) = lists("forget");
        l.keep("a", &golden()).unwrap();
        l.keep("b/../c", &golden()).unwrap();
        assert_eq!(
            std::fs::read_dir(&dir).unwrap().count(),
            2,
            "一台一份文件，名字不随机器 id 里的字跑出目录"
        );
        l.forget("a");
        assert!(l.held("a").is_none());
        assert!(Lists::new(dir.clone()).held("a").is_none());
        assert!(l.held("b/../c").is_some());
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 1);
    }
}
