//! **每台此刻有没有在走的活**：一组（从起到收场）· 一趟分块传（从起到结局）各握一个 [`Busy`]；
//! 那台的数从 0 变正 · 回到 0 那一下交 `busy` 那一件（`true` / `false`），保活服务照它判（切到后台活不断）。

use crate::resident::OnEvent;
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

fn counts() -> &'static Mutex<HashMap<i64, usize>> {
    static C: OnceLock<Mutex<HashMap<i64, usize>>> = OnceLock::new();
    C.get_or_init(Default::default)
}

/// 握着 ＝ 这台有一样活在走；丢下 ⇒ 少一样。
pub struct Busy {
    h: i64,
    on_event: OnEvent,
}

impl Busy {
    pub fn hold(h: i64, on_event: OnEvent) -> Self {
        let mut c = counts().lock().unwrap_or_else(|e| e.into_inner());
        let n = c.entry(h).or_default();
        *n += 1;
        // 在锁里交：前后两下（变正 · 回 0）不会交反。
        if *n == 1 {
            on_event(h, "busy", "true".into());
        }
        Busy { h, on_event }
    }
}

impl Drop for Busy {
    fn drop(&mut self) {
        let mut c = counts().lock().unwrap_or_else(|e| e.into_inner());
        let Some(n) = c.get_mut(&self.h) else { return };
        *n = n.saturating_sub(1);
        if *n == 0 {
            c.remove(&self.h);
            (self.on_event)(self.h, "busy", "false".into());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    /// 🔴 一台两样活：头一样起交 `true`，两样都完才交 `false`；中间不交。别的台各算各的。
    #[test]
    fn busy_flips_only_on_the_first_and_the_last() {
        let seen: Arc<Mutex<Vec<(i64, String)>>> = Arc::default();
        let s2 = seen.clone();
        let on: OnEvent = Arc::new(move |h, kind, body| {
            assert_eq!(kind, "busy");
            s2.lock().unwrap().push((h, body));
        });
        let a = Busy::hold(7701, on.clone());
        let b = Busy::hold(7701, on.clone());
        let other = Busy::hold(7702, on.clone());
        drop(a);
        assert_eq!(
            seen.lock().unwrap().clone(),
            [(7701, "true".into()), (7702, "true".into())],
            "还有一样在走 ⇒ 不交 false"
        );
        drop(b);
        drop(other);
        assert_eq!(
            seen.lock().unwrap()[2..].to_vec(),
            [(7701, "false".to_string()), (7702, "false".to_string())]
        );
    }
}
