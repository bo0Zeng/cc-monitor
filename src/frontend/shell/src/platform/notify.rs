//! 系统通知的平台那一半：壳里发系统通知只有这一个家（界面经 `desktop_notify::notify_desktop`，壳自己经 [`show`]）。
//!
//! Linux：经会话总线直接说 freedesktop 通知协议，**全进程一条长连接**（第一次发时连上），一条线程收这条连接上的
//! `NotificationClosed` / `ActivationToken` / `ActionInvoked`，按通知 id 分发（点通知本身 ⇒ 拿桌面随点击发来的激活令牌把主窗口
//! 拉到前面 —— Wayland 上没有令牌，桌面不让抢前台）。为什么要长连接：
//! GNOME 见发信人的总线名没了、而它又认得出是哪个有窗口的程序，就把那条通知连同来源收掉（L2 · 10-08 真窗口：
//! notification 插件每条新开一条连接、发完就丢 ⇒ `Notify` 之后十几毫秒 `NotificationClosed`）；每条一条连接守到
//! 关掉也不行 —— 没点掉的通知会在消息栏里留好几天，连接与线程越攒越多。
//! 带 `desktop-entry` 提示（安装包装的是 `cc-monitor.desktop`），桌面据它认出是谁。
//! 别处：照旧经 notification 插件。

/// 发一条系统通知。发不出去 ⇒ 一句原话（调用方决定说不说）。
pub fn show(app: &tauri::AppHandle, title: &str, body: &str) -> Result<(), String> {
    #[cfg(target_os = "linux")]
    {
        linux::POOL.send(app, title, body).map(|_| ())
    }
    #[cfg(not(target_os = "linux"))]
    {
        use tauri_plugin_notification::NotificationExt;
        app.notification()
            .builder()
            .title(title)
            .body(body)
            .show()
            .map_err(|e| e.to_string())
    }
}

#[cfg(target_os = "linux")]
pub(crate) mod linux {
    use std::collections::HashSet;
    use std::sync::{Arc, Mutex};

    /// 安装包里那份 .desktop 的名字（不带后缀）：通知的应用名 · 图标名 · `desktop-entry` 提示。
    pub(crate) const DESKTOP_ENTRY: &str = "cc-monitor";
    /// 点通知本身那一下的动作名（freedesktop 约定）。
    pub(crate) const DEFAULT_ACTION: &str = "default";

    /// 通知总线的那一点点能力：发一条（回通知 id）· 收一个信号（阻塞，连接断了 / 收掉了回 `None`）· 收掉这条连接。判据换成假的。
    pub(crate) trait Bus: Send + Sync + 'static {
        fn notify(&self, title: &str, body: &str) -> Result<u32, NotifyFail>;
        fn next_signal(&self) -> Option<Signal>;
        /// 收掉这条连接：之后 [`Bus::next_signal`] 回 `None`（收信那条线程随之退出）。
        fn close(&self);
    }

    /// 一条没发出去：原话 ＋ 连接是不是已经断了（断了才换连接；通知服务只是回了一个错 ⇒ 这条连接照旧用）。
    #[derive(Debug)]
    pub(crate) struct NotifyFail {
        pub(crate) said: String,
        pub(crate) dead: bool,
    }

    /// 这条连接上收到的、与通知有关的信号。
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub(crate) enum Signal {
        Closed(u32),
        /// 点击那一下桌面先发来的激活令牌（xdg-activation；紧跟着是 `Action`）。
        Token(u32, String),
        Action(u32, String),
    }

    /// 点了某一条通知的某个动作之后做什么（`id` · 动作名 · 这一下的激活令牌）。生产：点通知本身 ⇒ 主窗口拉前。
    pub(crate) type OnAction = Arc<dyn Fn(u32, &str, Option<&str>) + Send + Sync>;

    /// 全进程那一条连接 ＋ 我们发出去、还没关掉的那几条 id。
    pub(crate) struct Pool<B: Bus> {
        open: fn() -> Result<B, String>,
        bus: Mutex<Option<Arc<B>>>,
        live: Mutex<Option<Arc<Mutex<HashSet<u32>>>>>,
    }

    impl<B: Bus> Pool<B> {
        pub(crate) const fn new(open: fn() -> Result<B, String>) -> Self {
            Pool {
                open,
                bus: Mutex::new(None),
                live: Mutex::new(None),
            }
        }

        fn live_set(&self) -> Arc<Mutex<HashSet<u32>>> {
            self.live
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .get_or_insert_with(Default::default)
                .clone()
        }

        /// 那条连接（没有就连上，并起那一条收信号的线程，按 id 分发）。
        fn bus(&self, on_action: OnAction) -> Result<Arc<B>, String> {
            let mut slot = self.bus.lock().unwrap_or_else(|e| e.into_inner());
            if let Some(b) = slot.as_ref() {
                return Ok(b.clone());
            }
            let b = Arc::new((self.open)()?);
            let (reader, live) = (b.clone(), self.live_set());
            std::thread::spawn(move || {
                let mut tokens: std::collections::HashMap<u32, String> = Default::default();
                while let Some(sig) = reader.next_signal() {
                    let ours =
                        |id: &u32| live.lock().unwrap_or_else(|e| e.into_inner()).contains(id);
                    match sig {
                        Signal::Closed(id) => {
                            live.lock().unwrap_or_else(|e| e.into_inner()).remove(&id);
                            tokens.remove(&id);
                        }
                        Signal::Token(id, token) => {
                            if ours(&id) {
                                tokens.insert(id, token);
                            }
                        }
                        Signal::Action(id, action) => {
                            if ours(&id) {
                                on_action(id, &action, tokens.remove(&id).as_deref());
                            }
                        }
                    }
                }
            });
            *slot = Some(b.clone());
            Ok(b)
        }

        /// 发一条；连接断了 ⇒ 收掉它（旧那条收信线程随之退出），下一条重连；通知服务只是回了一个错 ⇒ 照旧用这条。
        pub(crate) fn send_with(
            &self,
            on_action: OnAction,
            title: &str,
            body: &str,
        ) -> Result<u32, String> {
            let b = self.bus(on_action)?;
            match b.notify(title, body) {
                Ok(id) => {
                    self.live_set()
                        .lock()
                        .unwrap_or_else(|e| e.into_inner())
                        .insert(id);
                    Ok(id)
                }
                Err(f) => {
                    if f.dead {
                        let old = self.bus.lock().unwrap_or_else(|e| e.into_inner()).take();
                        if let Some(old) = old {
                            old.close();
                        }
                    }
                    Err(f.said)
                }
            }
        }

        /// 还没关掉的那几条（判据看）。
        #[cfg(test)]
        pub(crate) fn live(&self) -> Vec<u32> {
            let mut v: Vec<u32> = self.live_set().lock().unwrap().iter().copied().collect();
            v.sort_unstable();
            v
        }
    }

    impl Pool<Session> {
        pub(crate) fn send(
            &self,
            app: &tauri::AppHandle,
            title: &str,
            body: &str,
        ) -> Result<u32, String> {
            let app = app.clone();
            let on_action: OnAction = Arc::new(move |_id, action, token| {
                if action == DEFAULT_ACTION {
                    crate::platform::window::raise_main(&app, token.map(str::to_string), true);
                }
            });
            self.send_with(on_action, title, body)
        }
    }

    /// 生产那一条：本会话总线。
    pub(crate) static POOL: Pool<Session> = Pool::new(Session::open);

    /// 会话总线上的通知服务。
    pub(crate) struct Session {
        conn: zbus::blocking::Connection,
        signals: Mutex<zbus::blocking::MessageIterator>,
    }

    const DEST: &str = "org.freedesktop.Notifications";
    const PATH: &str = "/org/freedesktop/Notifications";

    impl Session {
        fn open() -> Result<Self, String> {
            let conn = zbus::blocking::Connection::session().map_err(|e| e.to_string())?;
            let rule = zbus::MatchRule::builder()
                .msg_type(zbus::message::Type::Signal)
                .interface(DEST)
                .and_then(|r| r.path(PATH))
                .map_err(|e| e.to_string())?
                .build();
            let signals = zbus::blocking::MessageIterator::for_match_rule(rule, &conn, Some(64))
                .map_err(|e| e.to_string())?;
            Ok(Session {
                conn,
                signals: Mutex::new(signals),
            })
        }
    }

    /// 这一种错说明连接还活着吗：通知服务回了一个 D-Bus 错 / 回的东西读不懂 ⇒ 活着；别的（读写断了 · 握手坏了…）⇒ 断了。
    fn conn_dead(e: &zbus::Error) -> bool {
        !matches!(
            e,
            zbus::Error::MethodError(..) | zbus::Error::FDO(_) | zbus::Error::Variant(_)
        )
    }

    impl Bus for Session {
        fn notify(&self, title: &str, body: &str) -> Result<u32, NotifyFail> {
            let fail = |e: zbus::Error| NotifyFail {
                dead: conn_dead(&e),
                said: e.to_string(),
            };
            use std::collections::HashMap;
            use zbus::zvariant::Value;
            let hints: HashMap<&str, Value> =
                HashMap::from([("desktop-entry", Value::from(DESKTOP_ENTRY))]);
            let actions: Vec<&str> = vec![DEFAULT_ACTION, ""];
            let reply = self
                .conn
                .call_method(
                    Some(DEST),
                    PATH,
                    Some(DEST),
                    "Notify",
                    &(
                        DESKTOP_ENTRY,
                        0u32,
                        DESKTOP_ENTRY,
                        title,
                        body,
                        actions,
                        hints,
                        -1i32,
                    ),
                )
                .map_err(fail)?;
            reply.body().deserialize::<u32>().map_err(fail)
        }

        fn close(&self) {
            if let Err(e) = self.conn.clone().close() {
                tracing::debug!("收掉通知那条总线连接没成（{e}）—— 它多半已经断了");
            }
        }

        fn next_signal(&self) -> Option<Signal> {
            let mut it = self.signals.lock().unwrap_or_else(|e| e.into_inner());
            loop {
                let msg = it.next()?.ok()?;
                let header = msg.header();
                match header.member().map(|m| m.as_str()) {
                    Some("NotificationClosed") => {
                        if let Ok((id, _reason)) = msg.body().deserialize::<(u32, u32)>() {
                            return Some(Signal::Closed(id));
                        }
                    }
                    Some("ActivationToken") => {
                        if let Ok((id, token)) = msg.body().deserialize::<(u32, String)>() {
                            return Some(Signal::Token(id, token));
                        }
                    }
                    Some("ActionInvoked") => {
                        if let Ok((id, action)) = msg.body().deserialize::<(u32, String)>() {
                            return Some(Signal::Action(id, action));
                        }
                    }
                    _ => {}
                }
            }
        }
    }
}

#[cfg(all(test, target_os = "linux"))]
#[path = "../../../../../tests/frontend/shell/platform/notify_tests.rs"]
mod tests;
