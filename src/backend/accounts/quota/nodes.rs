//! ★ 轮换的**节点**：事实或设置变了的那一刻（`轮换-v5-算法.md` §4 外来的那几种 ＋ 设置写入）。全进程只有这一条通道：
//! 判的缓存与推送都只订它，不再各订各的铃。节点只说「什么变了 · 哪个会话」，不带结论；收到的一方自己重跑 / 重问。
//!
//! 已知时刻那几种（重置 · 分时段边界 · 启用延迟到期 · 策略交出的「次序最早可能在什么时刻变」）不走这里：判的那一侧记进下一个节点时刻，
//! 发之前比一下（不起定时器）。
//!
//! 来源各一处：
//! - 回包头读数让显示变了 · 官方客户端报的用量（[`super::ledger`]，[`Source::Reading`]）；
//! - 回 429 / 被拒（中转的重发那一路，[`Source::Refused`]，带会话）；
//! - 续期失败（换号备令牌那一下，[`Source::Login`]）；
//! - 文件事件：`quota.json` 被别的进程写（[`Source::QuotaFile`]）· 账号库清单或某个号的登录文件变了（[`Source::Logins`]）·
//!   `rotation.json` 某个会话那一条变了（[`Source::Book`]，带会话）· 策略目录变了（[`Source::Strategies`]）。
//!
//! 文件事件各用自己的盯，不经会话文件那一圈（那一圈卡住时节点照到）。

/// 这一件从哪来。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Source {
    /// 额度账上某个号的显示变了（回包头 · 官方客户端报的用量）。
    Reading,
    /// 这一发被拒（429 等），当场在同一发里重判。
    Refused,
    /// 续期失败：那个号的登录这会儿拿不到。
    Login,
    /// `quota.json` 被写了（多半是别的进程）。
    QuotaFile,
    /// 账号库清单或某个号的登录文件变了。
    Logins,
    /// `rotation.json` 里某个会话那一条变了（本进程写的或别的进程写的）。
    Book,
    /// 策略目录 · 配置表 · 本机默认变了。
    Strategies,
}

impl Source {
    /// 事实那几种（推送那一侧据此推 `changed {quota}`）。
    pub(crate) fn is_fact(self) -> bool {
        matches!(
            self,
            Source::Reading | Source::Refused | Source::Login | Source::QuotaFile | Source::Logins
        )
    }
}

/// 一件节点。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Node {
    pub(crate) source: Source,
    /// 只关这一个会话（`Book` · `Refused`）；关所有会话 ⇒ `None`。
    pub(crate) sid: Option<String>,
}

/// 通道的发送端（判据各用自己的一条，生产只有 [`bus`] 那一条）。
pub(crate) type Nodes = tokio::sync::broadcast::Sender<Node>;

/// 进程里唯一那条节点通道。可丢：落后的订阅者丢几件，下一件或重问补上。
pub(crate) fn bus() -> &'static Nodes {
    static TX: std::sync::OnceLock<Nodes> = std::sync::OnceLock::new();
    TX.get_or_init(|| tokio::sync::broadcast::channel::<Node>(256).0)
}

/// 发一件（没人订 ⇒ 丢掉，不是错）。
pub(crate) fn emit(tx: &Nodes, source: Source, sid: Option<&str>) {
    let _ = tx.send(Node {
        source,
        sid: sid.map(str::to_string),
    });
}

/// 账号库那一处的盯：清单所在的目录（只认清单那个文件名）＋ 每个号的配置目录（只认登录文件）。
/// 清单一变就照新的清单重盯（号可能加了 / 删了）。返回的那一份活着就一直盯。
pub(crate) struct LoginWatch {
    held: std::sync::Arc<std::sync::Mutex<Option<crate::platform::watch_file::Watching>>>,
}

/// 这台的账号库要盯的那几个目录：清单的目录 ＋ 每个号的配置目录。家推不出 / 没建账号库 ⇒ 只有清单的目录。
fn logins_dirs(home: &std::path::Path) -> (std::path::PathBuf, Vec<std::path::PathBuf>) {
    let manifest_dir = home.join(relay_route_core::ACCOUNTS_DIR_REL);
    let lib = crate::faces::rotation_face::library_at(home);
    let dirs = lib.accounts.iter().filter_map(|a| a.dir.clone()).collect();
    (manifest_dir, dirs)
}

fn logins_arm(
    home: std::path::PathBuf,
    tx: Nodes,
    held: std::sync::Arc<std::sync::Mutex<Option<crate::platform::watch_file::Watching>>>,
) -> Result<(), String> {
    let (manifest_dir, dirs) = logins_dirs(&home);
    let login_name = crate::agents::login_of(crate::agents::credentials_file_agent())
        .map(|f| f.creds_file.to_string());
    let mut watched: Vec<(std::path::PathBuf, bool)> = vec![(manifest_dir.clone(), false)];
    watched.extend(dirs.into_iter().filter(|d| d.is_dir()).map(|d| (d, false)));
    let md = manifest_dir.clone();
    let ln = login_name.clone();
    let concerns = move |p: &std::path::Path| {
        let name = p.file_name().and_then(|n| n.to_str());
        (p.parent() == Some(md.as_path()) && name == Some(relay_route_core::ACCOUNTS_MANIFEST_NAME))
            || (ln.is_some() && name == ln.as_deref())
    };
    let (h2, tx2, home2, md2) = (held.clone(), tx.clone(), home.clone(), manifest_dir);
    let w =
        crate::platform::watch_file::watch(&watched, concerns, "rotation-accounts", move |hit| {
            emit(&tx2, Source::Logins, None);
            let manifest_moved = hit.iter().any(|p| {
                p.parent() == Some(md2.as_path())
                    && p.file_name().and_then(|n| n.to_str())
                        == Some(relay_route_core::ACCOUNTS_MANIFEST_NAME)
            });
            if manifest_moved {
                // 号可能加了 / 删了：照新的清单重盯（在别的线程上换，免得在自己的回调里放掉自己）。
                let (h3, tx3, home3) = (h2.clone(), tx2.clone(), home2.clone());
                std::thread::spawn(move || {
                    if let Err(e) = logins_arm(home3, tx3, h3) {
                        tracing::warn!("[rotation] 账号库重盯不上：{e}");
                    }
                });
            }
        })?;
    *held.lock().unwrap_or_else(|e| e.into_inner()) = Some(w);
    Ok(())
}

impl LoginWatch {
    /// 盯 `home` 下的账号库，节点发到 `tx`。
    pub(crate) fn start(home: &std::path::Path, tx: &Nodes) -> Result<Self, String> {
        let held = std::sync::Arc::new(std::sync::Mutex::new(None));
        logins_arm(home.to_path_buf(), tx.clone(), held.clone())?;
        Ok(Self { held })
    }

    /// 此刻在盯的那一份还在（重盯失败过 ⇒ 假）。
    pub(crate) fn armed(&self) -> bool {
        self.held
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .is_some()
    }
}

/// 常驻 / 流那一路起来时调一次：盯这台的账号库，进程活着就一直盯。盯不上只出声。
pub(crate) fn watch_logins_here() {
    static HELD: std::sync::Mutex<Option<LoginWatch>> = std::sync::Mutex::new(None);
    let mut g = HELD.lock().unwrap_or_else(|e| e.into_inner());
    if g.is_some() {
        return;
    }
    let get = |k: &str| std::env::var(k).ok();
    let Some(home) = crate::platform::paths::home_dir_from(&|k| get(k).map(Into::into)) else {
        return;
    };
    match LoginWatch::start(&home, bus()) {
        Ok(w) if w.armed() => *g = Some(w),
        Ok(_) => {}
        Err(e) => tracing::warn!("[rotation] 盯不上账号库：{e}（登录变了要等下一发才看得见）"),
    }
}

#[cfg(test)]
#[path = "../../../../tests/backend/accounts/quota/nodes_tests.rs"]
mod tests;
