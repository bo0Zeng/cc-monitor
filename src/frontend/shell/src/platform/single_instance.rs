//! 只开一个 cc-monitor：第二次启动把参数（和桌面给的**激活令牌**）交给第一个，自己退出；第一个据此把主窗口拉回来。
//!
//! - Windows / macOS：照旧经 `tauri-plugin-single-instance`（它交参数与工作目录；那两个平台拉前不要令牌）。
//! - Linux：自己在会话总线上占名字 `com.ccmonitor.app.SingleInstance`（第一个 tauri 插件，同官方那一个的时机）。为什么不用插件：
//!   它在 Linux 上只交参数与工作目录，不交 `XDG_ACTIVATION_TOKEN` —— GNOME（Wayland）不让没有令牌的程序抢前台，
//!   第一个只能弹「“cc-monitor”已就绪」、点了才出来（L2 · 10-08 真窗口）。启动器（dock · 应用列表 · 文件管理器）起
//!   第二个时把令牌放在环境里，这里原样交给第一个，第一个照点通知那一套（GTK `set_startup_id` ＋ `present`）拉前。
//!
//! 判定（`--background` 不抢焦点 · 有没有令牌怎么拉）在调用方；这里只管「交过去 / 收过来」。

/// 第二次启动交来的东西。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Second {
    /// 第二个实例的命令行参数（含程序名）。
    pub args: Vec<String>,
    /// 启动它的那一方给的激活令牌（Wayland `XDG_ACTIVATION_TOKEN` · X11 `DESKTOP_STARTUP_ID`）；没有 ⇒ `None`。
    pub token: Option<String>,
}

/// 单实例那个插件：必须是第一个注册的插件（第二个实例在别的插件起来之前就退）。`on_second` 在第一个实例里跑。
pub fn plugin(on_second: fn(&tauri::AppHandle, Second)) -> tauri::plugin::TauriPlugin<tauri::Wry> {
    #[cfg(target_os = "linux")]
    {
        linux::plugin(on_second)
    }
    #[cfg(not(target_os = "linux"))]
    {
        tauri_plugin_single_instance::init(move |app, args, _cwd| {
            on_second(app, Second { args, token: None })
        })
    }
}

#[cfg(target_os = "linux")]
pub(crate) mod linux {
    use super::Second;

    /// 会话总线上的名字与对象路径。
    pub(crate) const NAME: &str = "com.ccmonitor.app.SingleInstance";
    const PATH: &str = "/com/ccmonitor/app/SingleInstance";

    /// 占名字的结局。
    pub(crate) enum Claim {
        /// 我是第一个；连接要一直留着（名字跟着它）。
        First(zbus::blocking::Connection),
        /// 已经有一个了，参数与令牌交过去了（交没交成都算：第二个照样退）。
        Handed,
    }

    /// 两个启动变量 ⇒ 令牌：Wayland 的优先；空串当没有。
    pub(crate) fn launch_token(xdg: Option<String>, startup_id: Option<String>) -> Option<String> {
        xdg.filter(|t| !t.is_empty())
            .or_else(|| startup_id.filter(|t| !t.is_empty()))
    }

    struct Iface {
        deliver: Box<dyn Fn(Second) + Send + Sync>,
    }

    #[zbus::interface(name = "com.ccmonitor.app.SingleInstance")]
    impl Iface {
        /// 第二个实例交来参数与令牌（空串 = 没有）。
        fn activate(&self, args: Vec<String>, token: String) {
            (self.deliver)(Second {
                args,
                token: Some(token).filter(|t| !t.is_empty()),
            });
        }
    }

    /// 占名字：占到了 ⇒ 挂上接收那一头（`deliver` 收后来者交来的）；已被占 ⇒ 把 `args` 与 `token` 交给占着的那个。
    /// `open` 给一条到会话总线的连接构造器（判据给私有总线）。连不上总线 ⇒ `Err(原话)`（调用方当没有单实例照常起）。
    pub(crate) fn claim(
        open: &dyn Fn() -> zbus::Result<zbus::blocking::connection::Builder<'static>>,
        deliver: Box<dyn Fn(Second) + Send + Sync>,
        args: &[String],
        token: Option<String>,
    ) -> Result<Claim, String> {
        let built = open()
            .and_then(|b| b.name(NAME))
            .and_then(|b| b.serve_at(PATH, Iface { deliver }))
            .map(|b| {
                b.replace_existing_names(false)
                    .allow_name_replacements(false)
            })
            .and_then(|b| b.build());
        match built {
            Ok(conn) => Ok(Claim::First(conn)),
            Err(zbus::Error::NameTaken) => {
                let handed = open().and_then(|b| b.build()).and_then(|c| {
                    c.call_method(
                        Some(NAME),
                        PATH,
                        Some(NAME),
                        "Activate",
                        &(args, token.unwrap_or_default()),
                    )
                    .map(|_| ())
                });
                if let Err(e) = handed {
                    tracing::info!("单实例：交给第一个没交成：{e}");
                }
                Ok(Claim::Handed)
            }
            Err(e) => Err(e.to_string()),
        }
    }

    /// 占住的那条连接（名字跟着它活）。
    struct Held(#[allow(dead_code)] zbus::blocking::Connection);

    pub(crate) fn plugin(
        on_second: fn(&tauri::AppHandle, Second),
    ) -> tauri::plugin::TauriPlugin<tauri::Wry> {
        // 令牌现在就读：GTK 打开显示那一下会读走并清掉这两个变量（窗口还没建、插件 setup 之前就已经开了显示）。
        let token = launch_token(
            std::env::var("XDG_ACTIVATION_TOKEN").ok(),
            std::env::var("DESKTOP_STARTUP_ID").ok(),
        );
        tauri::plugin::Builder::new("ccm-single-instance")
            .setup(move |app, _api| {
                use tauri::Manager;
                let handle = app.clone();
                let deliver = Box::new(move |s: Second| {
                    let h = handle.clone();
                    if let Err(e) = handle.run_on_main_thread(move || on_second(&h, s)) {
                        tracing::info!("单实例：派不到主线程：{e}");
                    }
                });
                let args: Vec<String> = std::env::args().collect();
                let open = zbus::blocking::connection::Builder::session;
                match claim(&open, deliver, &args, token.clone()) {
                    Ok(Claim::First(conn)) => {
                        app.manage(Held(conn));
                    }
                    Ok(Claim::Handed) => {
                        tracing::info!("单实例：已经有一个 cc-monitor 在跑，交过去了，这个退出");
                        app.cleanup_before_exit();
                        std::process::exit(0);
                    }
                    Err(e) => tracing::warn!("单实例：会话总线连不上，照常起：{e}"),
                }
                Ok(())
            })
            .build()
    }
}

#[cfg(all(test, target_os = "linux"))]
#[path = "../../../../../tests/frontend/shell/platform/single_instance_tests.rs"]
mod tests;
