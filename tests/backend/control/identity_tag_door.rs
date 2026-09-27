//! [`crate::control::identity_tag`] 起 tmux 的那个口的**测试构建那一份**（`设计/16 §3.1`：体住 `tests/`，生产树里只留桩）。
//!
//! 〔RESYNC · `INVARIANTS §48.3`〕进程内会走到 `identity_tag::tag` 的测试：假 tmux 只从这里注入。
//! 注入是**线程级**的（每条测试一条线程），出作用域自动摘掉；本线程没注入 ⇒ [`tmux`] 炸。

thread_local! {
    static FAKE_TMUX: std::cell::RefCell<Option<std::path::PathBuf>> =
        const { std::cell::RefCell::new(None) };
}

/// 注入凭证：活着时本线程的 `tag` 起的是那个假 tmux；丢掉即摘。
pub(crate) struct Isolated(());

impl Drop for Isolated {
    fn drop(&mut self) {
        FAKE_TMUX.with(|f| *f.borrow_mut() = None);
    }
}

/// 本线程的 `tag` 改起 `script`（经 `/bin/sh`）。
pub(crate) fn isolate_with(script: &std::path::Path) -> Isolated {
    FAKE_TMUX.with(|f| *f.borrow_mut() = Some(script.to_path_buf()));
    Isolated(())
}

/// 不关心打标的测试用：一个一声不吭、退出码 1 的假 tmux（探测回空 ⇒ `NoSuchPane`，从不写）。
pub(crate) fn isolate() -> Isolated {
    let p = std::env::temp_dir().join(format!("ccm-resync-mute-tmux-{}", std::process::id()));
    if !p.exists() {
        // 同进程并发写同一份内容，谁赢都一样；经 `/bin/sh` 读，不 exec 它（理由见 `identity_tag_tests::fake_cmd`）。
        std::fs::write(&p, "#!/bin/sh\nexit 1\n").expect("写假 tmux");
    }
    isolate_with(&p)
}

/// 生产段 `door::tmux()` 的测试构建那一份。
pub(crate) fn tmux() -> std::process::Command {
    FAKE_TMUX.with(|f| match f.borrow().as_ref() {
        Some(p) => {
            let mut c = std::process::Command::new("/bin/sh");
            c.arg(p);
            c
        }
        None => panic!(
            "进程内测试走到了 `identity_tag::tag` 却没注入假 tmux —— 先 \
             `let _iso = crate::control::identity_tag::door::isolate();`（INVARIANTS §48.3：\
             不隔离就是往跑测试那个终端所在的真 tmux 上打标）"
        ),
    })
}
