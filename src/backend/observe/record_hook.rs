//! 问过一条会话的记录在不在之后交给谁 —— 进程里装的那一个函数（生产：watcher 起来时装它交会话账本的那一口；没装 ⇒ 不交）。
//! 隔这一层：问记录的那几条命令不经 watcher（它够得着 tmux），只经这一口。

/// 交一条会话的记录在不在（`sid` · `present`）。
pub(crate) type Seen = fn(&str, bool);

static HOOK: std::sync::OnceLock<Seen> = std::sync::OnceLock::new();

/// 装上（进程里第一次装的那一个算数）。
pub(crate) fn install(f: Seen) {
    HOOK.get_or_init(|| f);
}

/// 交；没装 ⇒ 不做。
pub(crate) fn seen(sid: &str, present: bool) {
    if let Some(f) = HOOK.get() {
        f(sid, present);
    }
}
