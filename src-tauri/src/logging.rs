//! GUI app 诊断日志基础设施（v2.0.0 落地 issue #4）。
//!
//! ## 解决什么问题
//!
//! `windows_subsystem = "windows"` 的 GUI app **没有 stderr 控制台** —— 所有
//! `tracing::warn!` / `tracing::error!` 用户和开发者都看不到。这造成了 v1.7.0-1.7.7
//! "cc 集成装上没用" 7 个版本带病发布的真凶（PS 5.1 Out-File 写 BOM → serde_json
//! 解析失败 → 一直 `tracing::warn!("bind: parse ... failed")` 但没人看到）。
//!
//! ## 三层方案
//!
//! 1. **滚动 log 文件**：写到 `~/.claude/claudecode-frontend/logs/monitor.YYYY-MM-DD.log`，
//!    按天滚动，保留最近 N 天（默认 3）。non_blocking writer 不阻塞业务线程。
//! 2. **EnvFilter reload**：用 `tracing_subscriber::reload::Layer<EnvFilter>` 让
//!    "改日志级别" 这种运行时操作不重启就生效。
//! 3. **ErrorEmitter Layer**：自定义 Layer 拦截 `Level::ERROR`，emit `monitor-error`
//!    事件给前端 → 弹红色 toast，关键错误用户**一定能看到**。
//!
//! ## 解耦边界
//!
//! - 本模块完全不知道 Tauri State / IPC 的存在。`init()` 返回 `Arc<LoggingState>`，
//!   由 `lib.rs` 自己 `app.manage()`。
//! - ErrorEmitter Layer 通过 closure 注入 emit 行为（`install_error_emitter`），
//!   避免对 `tauri::Runtime` generic 的依赖泄漏。
//! - DiagnosticsConfig 字段独立 R/W `config.json` 的 `diagnostics` 子对象，不污染
//!   现有 `config::load_config / save_config` 接口。
//!
//! ## 启动时序
//!
//! ```text
//! lib.rs::run()
//!   ├─ logging::init(monitor_data_dir) → Arc<LoggingState>
//!   │   ├─ 读 config.json 的 diagnostics 字段（缺省走 default）
//!   │   ├─ tracing_subscriber::registry()
//!   │   │     .with(reload<EnvFilter>)
//!   │   │     .with(stdout fmt::layer)
//!   │   │     .with(file fmt::layer + non_blocking)   ← log_enabled=true 才装
//!   │   │     .with(ErrorEmitterLayer)                ← 始终装；用 atomic enabled 切开关
//!   │   │     .init()
//!   │   └─ WorkerGuard 存到 state 字段（必须保持存活到进程退出，drop 时 flush）
//!   ├─ tauri::Builder::default().setup(|app| {
//!   │     state.install_error_emitter(app.handle().clone());  // 注入 emit closure
//!   │     app.manage(state.clone());
//!   │  })
//!   └─ IPC: get/set_diagnostics_config / open_log_file/dir / get_log_file_info
//! ```
//!
//! ## 不变量
//! - log 目录创建失败不能阻塞 monitor 启动 → fallback 到 stdout-only
//! - tracing 全局 dispatcher 一旦 init 不能再换 → 必须在 `tauri::Builder` 之前调用
//! - WorkerGuard drop 才会 flush 缓冲 → 必须挂在 state 上（与 app 同生命周期）

use parking_lot::{Mutex, RwLock};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Emitter, Runtime};
use tracing::{Event, Level, Subscriber};
use tracing_appender::non_blocking::WorkerGuard;
use tracing_appender::rolling::{Builder as RollingBuilder, Rotation};
use tracing_subscriber::field::Visit;
use tracing_subscriber::filter::EnvFilter;
use tracing_subscriber::fmt;
use tracing_subscriber::layer::{Context as LayerContext, Layer, SubscriberExt};
use tracing_subscriber::registry::LookupSpan;
use tracing_subscriber::reload;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::Registry;

const LOG_DIR_NAME: &str = "logs";
const LOG_FILE_PREFIX: &str = "monitor";
const LOG_FILE_SUFFIX: &str = "log";
const ERROR_EVENT: &str = "monitor-error";

// ===== DiagnosticsConfig =====

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DiagnosticsConfig {
    /// 是否写 log 文件。toggle 后需要重启 monitor 才能生效（layer 已注册不可摘）。
    #[serde(default = "default_log_enabled")]
    pub log_enabled: bool,
    /// "trace" / "debug" / "info" / "warn" / "error" / "off"。set 后立即 reload 生效。
    #[serde(default = "default_log_level")]
    pub log_level: String,
    /// ERROR 级别是否 emit 给前端弹 toast。set 后立即生效。
    #[serde(default = "default_error_toast")]
    pub error_toast: bool,
    /// 保留最近多少天的 log 文件。需要重启生效（rolling builder 启动时定型）。
    #[serde(default = "default_max_files")]
    pub max_files: u32,
}

fn default_log_enabled() -> bool {
    true
}
fn default_log_level() -> String {
    "info".to_string()
}
fn default_error_toast() -> bool {
    true
}
fn default_max_files() -> u32 {
    3
}

impl Default for DiagnosticsConfig {
    fn default() -> Self {
        Self {
            log_enabled: default_log_enabled(),
            log_level: default_log_level(),
            error_toast: default_error_toast(),
            max_files: default_max_files(),
        }
    }
}

/// `set_diagnostics_config` 返回值：告诉前端是否需要弹"请重启"提示。
#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RestartHint {
    /// 全部立即生效，无需重启
    None,
    /// log_enabled 或 max_files 改了，新设置已写入 config.json，下次启动 monitor 生效
    NeedsRestart,
}

// ===== LoggingState =====

/// reload Handle 的类型别名。`reload::Handle<EnvFilter, Registry>` 在不同 layer
/// 组合下类型链很长，alias 简化。
type FilterReloadHandle = reload::Handle<EnvFilter, Registry>;

/// 由 setup() 注入的 emit closure 类型 —— 把 `AppHandle<R>` 的 generic 限制隔在
/// closure 里面，对外暴露统一的 `Fn(&MonitorErrorPayload)` trait object。
type ErrorEmitFn = Box<dyn Fn(&MonitorErrorPayload) + Send + Sync>;

pub struct LoggingState {
    cfg: RwLock<DiagnosticsConfig>,
    reload_handle: FilterReloadHandle,
    log_dir: PathBuf,
    monitor_data_dir: PathBuf,
    /// emit 给前端 toast 的 closure。`install_error_emitter` 之前是 None
    /// （setup 期间的 ERROR 只写 log 不发 toast；不致命，setup 失败的 ERROR 用户
    /// 也看不见 GUI 框）。
    error_emit_fn: Arc<RwLock<Option<ErrorEmitFn>>>,
    /// error_toast toggle 用 atomic 实现"运行时切换不重装 layer"
    error_emit_enabled: Arc<AtomicBool>,
    /// non_blocking writer 的 WorkerGuard。drop 时 worker 线程 flush + 退出。
    /// 必须跟 monitor 进程同生命周期 → 挂在 app.manage 的 state 上。
    _guard: Mutex<Option<WorkerGuard>>,
}

impl LoggingState {
    pub fn config(&self) -> DiagnosticsConfig {
        self.cfg.read().clone()
    }

    pub fn log_dir(&self) -> PathBuf {
        self.log_dir.clone()
    }

    /// 找 log_dir 下 mtime 最新的 .log 文件 —— rolling::daily 按日期命名
    /// `monitor.YYYY-MM-DD.log`，最新那个就是"今天的"。用户可能跨天运行
    /// monitor，每次查都拿到当下正确的文件。
    pub fn current_log_file(&self) -> Option<PathBuf> {
        find_latest_log_file(&self.log_dir)
    }

    /// 由 lib.rs::setup() 调用：把 AppHandle wrap 成 emit closure 存到 state，
    /// ErrorEmitterLayer 拦到 ERROR 时调用它。
    ///
    /// 用 closure 隔绝 `AppHandle<R>` 的 generic R，对外接口统一。
    pub fn install_error_emitter<R: Runtime>(&self, handle: AppHandle<R>) {
        let h = handle;
        *self.error_emit_fn.write() = Some(Box::new(move |p: &MonitorErrorPayload| {
            let _ = h.emit(ERROR_EVENT, p);
        }));
    }

    /// 更新配置：reload level + 切 error_toast 开关 + 持久化到 config.json。
    /// log_enabled / max_files 改变需要重启（layer 启动时定型）→ 返回 RestartHint。
    pub fn update_config(&self, new_cfg: DiagnosticsConfig) -> Result<RestartHint, String> {
        let old = self.cfg.read().clone();
        let mut hint = RestartHint::None;

        // 1. log_level 改变 → 即时 reload
        if new_cfg.log_level != old.log_level {
            let filter = build_env_filter(&new_cfg.log_level)
                .ok_or_else(|| format!("invalid log level: {:?}", new_cfg.log_level))?;
            self.reload_handle
                .modify(|f| *f = filter)
                .map_err(|e| format!("reload filter failed: {e}"))?;
            tracing::info!(
                "log level changed: {} → {}",
                old.log_level,
                new_cfg.log_level
            );
        }

        // 2. error_toast 改变 → 切 atomic 开关
        if new_cfg.error_toast != old.error_toast {
            self.error_emit_enabled
                .store(new_cfg.error_toast, Ordering::Relaxed);
            tracing::info!("error_toast: {} → {}", old.error_toast, new_cfg.error_toast);
        }

        // 3. log_enabled / max_files 改变 → 提示重启
        if new_cfg.log_enabled != old.log_enabled || new_cfg.max_files != old.max_files {
            hint = RestartHint::NeedsRestart;
        }

        // 4. 写回 config.json
        write_diagnostics_to_config(&self.monitor_data_dir, &new_cfg)
            .map_err(|e| format!("save diagnostics config failed: {e}"))?;
        *self.cfg.write() = new_cfg;
        Ok(hint)
    }
}

// ===== MessageVisitor + ErrorEmitterLayer =====

/// 提取 tracing event 的 message 字段（其他字段忽略）。tracing 用 visitor pattern
/// 是因为 event field 是泛型 + lazy formatted，必须 visit 才能取值。
#[derive(Default)]
struct MessageVisitor {
    message: String,
}

impl Visit for MessageVisitor {
    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
        if field.name() == "message" {
            self.message = format!("{value:?}");
        }
    }
    fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
        if field.name() == "message" {
            self.message = value.to_string();
        }
    }
}

/// emit 给前端的 ERROR 事件 payload。
#[derive(Serialize, Clone)]
pub struct MonitorErrorPayload {
    pub level: &'static str,
    /// 触发 ERROR 的 tracing target（如 `monitor_lib::bind` / `monitor_lib::config`）。
    /// 前端 toast 用作 headline，让用户看到错误来源模块。
    pub target: String,
    pub message: String,
    pub timestamp: i64,
}

struct ErrorEmitterLayer {
    emit_fn: Arc<RwLock<Option<ErrorEmitFn>>>,
    enabled: Arc<AtomicBool>,
    /// 简单限频：60s 窗口内最多 20 条；超出丢弃（避免错误风暴 toast 满屏）。
    /// "多一点无所谓"（issue 决策）：20 比较宽，但大于 20 的密度本来就 UI 没法看
    recent: Mutex<VecDeque<Instant>>,
}

const RATE_WINDOW: Duration = Duration::from_secs(60);
const RATE_MAX: usize = 20;

impl ErrorEmitterLayer {
    fn new(emit_fn: Arc<RwLock<Option<ErrorEmitFn>>>, enabled: Arc<AtomicBool>) -> Self {
        Self {
            emit_fn,
            enabled,
            recent: Mutex::new(VecDeque::new()),
        }
    }
}

impl<S> Layer<S> for ErrorEmitterLayer
where
    S: Subscriber + for<'a> LookupSpan<'a>,
{
    fn on_event(&self, event: &Event<'_>, _ctx: LayerContext<'_, S>) {
        if *event.metadata().level() != Level::ERROR {
            return;
        }
        if !self.enabled.load(Ordering::Relaxed) {
            return;
        }
        let guard = self.emit_fn.read();
        let Some(emit) = guard.as_ref() else {
            return; // setup 前还没注入；ERROR 只写 log 文件
        };

        // 限频
        let now = Instant::now();
        {
            let mut q = self.recent.lock();
            while q
                .front()
                .is_some_and(|t| now.duration_since(*t) > RATE_WINDOW)
            {
                q.pop_front();
            }
            if q.len() >= RATE_MAX {
                return;
            }
            q.push_back(now);
        }

        let mut v = MessageVisitor::default();
        event.record(&mut v);

        let payload = MonitorErrorPayload {
            level: "error",
            target: event.metadata().target().to_string(),
            message: v.message,
            timestamp: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_millis() as i64)
                .unwrap_or(0),
        };
        emit(&payload);
    }
}

// ===== init =====

/// 初始化 tracing subscriber。**必须在 `tauri::Builder` 之前调用**
/// （tracing 全局 dispatcher 一旦 init 不能再换）。
///
/// 返回的 `Arc<LoggingState>` 含 `WorkerGuard`，调用方应通过 `app.manage(state)`
/// 持有到 monitor 进程退出（drop 时 worker 线程 flush）。
pub fn init(monitor_data_dir: &Path) -> Arc<LoggingState> {
    let cfg = read_diagnostics_from_config(monitor_data_dir);
    let log_dir = monitor_data_dir.join(LOG_DIR_NAME);

    // 1. EnvFilter + reload handle
    let initial_filter = build_env_filter(&cfg.log_level).unwrap_or_else(|| {
        eprintln!(
            "invalid initial log_level {:?}, falling back to info",
            cfg.log_level
        );
        EnvFilter::new("info")
    });
    let (filter_layer, reload_handle) = reload::Layer::new(initial_filter);

    // 2. stdout layer（dev shell 可见；prod windows_subsystem=windows 无 console 也无害）
    let stdout_layer = fmt::layer().with_target(false);

    // 3. file layer + WorkerGuard（log_enabled=true 才装；失败 fallback 到 None）
    //
    // **必须内联**而非抽函数：`fmt::layer()` 返回的 Layer<S, ...> 的 S 必须由
    // `.with()` chain 的 Subscriber 类型回流推导（Layered<reload, Layered<stdout,
    // Registry>>），抽到函数里 S 会被推成 Registry 导致 with() trait bound 不满足。
    let (file_layer, guard) = if cfg.log_enabled {
        match build_rolling_appender(&log_dir, cfg.max_files as usize) {
            Some(appender) => {
                let (nb, g) = tracing_appender::non_blocking(appender);
                let layer = fmt::layer()
                    .with_ansi(false)
                    .with_target(true)
                    .with_writer(nb);
                (Some(layer), Some(g))
            }
            None => (None, None),
        }
    } else {
        (None, None)
    };

    // 4. error emitter layer（始终装；emit_fn=None 时 noop，enabled=false 时 skip）
    let emit_fn_slot: Arc<RwLock<Option<ErrorEmitFn>>> = Arc::new(RwLock::new(None));
    let enabled_flag = Arc::new(AtomicBool::new(cfg.error_toast));
    let error_layer = ErrorEmitterLayer::new(emit_fn_slot.clone(), enabled_flag.clone());

    // 5. 组装 + init
    let init_result = tracing_subscriber::registry()
        .with(filter_layer)
        .with(stdout_layer)
        .with(file_layer)
        .with(error_layer)
        .try_init();
    if let Err(e) = init_result {
        // 测试场景可能已有 subscriber；非测试场景到这一步算严重 bug 但不致命
        eprintln!("tracing init failed (subscriber already set?): {e}");
    }

    Arc::new(LoggingState {
        cfg: RwLock::new(cfg),
        reload_handle,
        log_dir,
        monitor_data_dir: monitor_data_dir.to_path_buf(),
        error_emit_fn: emit_fn_slot,
        error_emit_enabled: enabled_flag,
        _guard: Mutex::new(guard),
    })
}

/// 只构造 RollingFileAppender（不构造 fmt::Layer —— 见 init() 内联注释）。
/// 失败返回 None，init() 那边 fallback 到 stdout-only。
fn build_rolling_appender(
    log_dir: &Path,
    max_files: usize,
) -> Option<tracing_appender::rolling::RollingFileAppender> {
    if let Err(e) = std::fs::create_dir_all(log_dir) {
        eprintln!("create log dir {} failed: {e}", log_dir.display());
        return None;
    }
    RollingBuilder::new()
        .rotation(Rotation::DAILY)
        .filename_prefix(LOG_FILE_PREFIX)
        .filename_suffix(LOG_FILE_SUFFIX)
        .max_log_files(max_files)
        .build(log_dir)
        .map_err(|e| {
            eprintln!("build rolling log appender failed: {e}");
        })
        .ok()
}

/// 构造合适的 EnvFilter。除用户级别外，强制压低噪声 crate（tao/wry 是 Tauri 内部
/// GUI 框架，DEBUG 输出量极大但跟业务无关）。
///
/// 返回 None 表示 level 字符串非法。
fn build_env_filter(level: &str) -> Option<EnvFilter> {
    let combined = format!("{level},tao=warn,wry=warn,tracing=off");
    EnvFilter::try_new(combined).ok()
}

/// 列 log_dir 下 mtime 最新的 .log 文件。无 → None。
fn find_latest_log_file(log_dir: &Path) -> Option<PathBuf> {
    let entries = std::fs::read_dir(log_dir).ok()?;
    let mut latest: Option<(PathBuf, SystemTime)> = None;
    for e in entries.flatten() {
        let p = e.path();
        if !p.is_file() {
            continue;
        }
        // rolling::daily 写的文件名形如 monitor.YYYY-MM-DD.log，扩展名是 "log"
        if p.extension().is_some_and(|x| x == LOG_FILE_SUFFIX) {
            let mtime = e
                .metadata()
                .ok()
                .and_then(|m| m.modified().ok())
                .unwrap_or(SystemTime::UNIX_EPOCH);
            match &latest {
                Some((_, t)) if *t >= mtime => {}
                _ => latest = Some((p, mtime)),
            }
        }
    }
    latest.map(|(p, _)| p)
}

// ===== diagnostics 字段 R/W（独立读写 config.json 的子对象） =====

fn read_diagnostics_from_config(monitor_data_dir: &Path) -> DiagnosticsConfig {
    let cfg_path = monitor_data_dir.join("config.json");
    let Ok(raw) = std::fs::read_to_string(&cfg_path) else {
        return DiagnosticsConfig::default();
    };
    let Ok(v) = serde_json::from_str::<serde_json::Value>(&raw) else {
        return DiagnosticsConfig::default();
    };
    v.get("diagnostics")
        .cloned()
        .and_then(|d| serde_json::from_value::<DiagnosticsConfig>(d).ok())
        .unwrap_or_default()
}

fn write_diagnostics_to_config(
    monitor_data_dir: &Path,
    cfg: &DiagnosticsConfig,
) -> Result<(), String> {
    let cfg_path = monitor_data_dir.join("config.json");
    let mut v: serde_json::Value = if cfg_path.exists() {
        let raw = std::fs::read_to_string(&cfg_path)
            .map_err(|e| format!("read {}: {e}", cfg_path.display()))?;
        serde_json::from_str(&raw).unwrap_or_else(|_| serde_json::json!({}))
    } else {
        std::fs::create_dir_all(monitor_data_dir)
            .map_err(|e| format!("mkdir {}: {e}", monitor_data_dir.display()))?;
        serde_json::json!({})
    };
    let obj = v.as_object_mut().ok_or("config.json root not an object")?;
    obj.insert(
        "diagnostics".to_string(),
        serde_json::to_value(cfg).map_err(|e| e.to_string())?,
    );

    let pretty = serde_json::to_string_pretty(&v).map_err(|e| e.to_string())?;
    let tmp = cfg_path.with_extension("json.tmp");
    std::fs::write(&tmp, pretty).map_err(|e| format!("write tmp: {e}"))?;
    atomic_replace(&tmp, &cfg_path).map_err(|e| format!("replace: {e}"))?;
    Ok(())
}

// config.rs::atomic_replace 是 pub(crate) sibling 函数；这里复制一份避免 cross-module
// 依赖（logging 不该 import config，否则模块图不干净）
#[cfg(windows)]
fn atomic_replace(src: &Path, dst: &Path) -> std::io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows::core::PCWSTR;
    use windows::Win32::Storage::FileSystem::{MoveFileExW, MOVEFILE_REPLACE_EXISTING};

    let to_wide = |p: &Path| -> Vec<u16> {
        p.as_os_str()
            .encode_wide()
            .chain(std::iter::once(0))
            .collect()
    };
    let src_w = to_wide(src);
    let dst_w = to_wide(dst);
    unsafe {
        MoveFileExW(
            PCWSTR(src_w.as_ptr()),
            PCWSTR(dst_w.as_ptr()),
            MOVEFILE_REPLACE_EXISTING,
        )
        .map_err(|e| std::io::Error::other(e.message().to_string()))
    }
}

#[cfg(not(windows))]
fn atomic_replace(src: &Path, dst: &Path) -> std::io::Result<()> {
    std::fs::rename(src, dst)
}

// ===== 文件信息（IPC get_log_file_info 用） =====

#[derive(Debug, Clone, Serialize)]
pub struct LogFileInfo {
    pub dir: String,
    pub current_file: Option<String>,
    pub current_size_bytes: u64,
    pub all_files: Vec<LogFileEntry>,
}

#[derive(Debug, Clone, Serialize)]
pub struct LogFileEntry {
    pub path: String,
    pub size_bytes: u64,
    pub modified_ms: i64,
}

impl LoggingState {
    pub fn log_file_info(&self) -> LogFileInfo {
        let mut entries = Vec::new();
        if let Ok(rd) = std::fs::read_dir(&self.log_dir) {
            for e in rd.flatten() {
                let p = e.path();
                if !p.is_file() {
                    continue;
                }
                if p.extension().is_some_and(|x| x == LOG_FILE_SUFFIX) {
                    let meta = e.metadata().ok();
                    let size = meta.as_ref().map(|m| m.len()).unwrap_or(0);
                    let mtime = meta
                        .and_then(|m| m.modified().ok())
                        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                        .map(|d| d.as_millis() as i64)
                        .unwrap_or(0);
                    entries.push(LogFileEntry {
                        path: p.to_string_lossy().into_owned(),
                        size_bytes: size,
                        modified_ms: mtime,
                    });
                }
            }
        }
        entries.sort_by(|a, b| b.modified_ms.cmp(&a.modified_ms));
        let current_file = entries.first().map(|e| e.path.clone());
        let current_size_bytes = entries.first().map(|e| e.size_bytes).unwrap_or(0);
        LogFileInfo {
            dir: self.log_dir.to_string_lossy().into_owned(),
            current_file,
            current_size_bytes,
            all_files: entries,
        }
    }
}

// ===== 单元测试 =====

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diagnostics_default_is_user_friendly() {
        let d = DiagnosticsConfig::default();
        assert!(d.log_enabled, "log 默认开（issue #4 要求）");
        assert!(
            d.error_toast,
            "error toast 默认开（用户看不见 ERROR 是 v1.7 事故根因）"
        );
        assert_eq!(d.log_level, "info");
        assert_eq!(d.max_files, 3);
    }

    #[test]
    fn diagnostics_legacy_config_missing_field_uses_defaults() {
        // 老用户的 config.json 没有 diagnostics 字段；新版本读应回退到 default
        let raw = r#"{"claudeDir":"C:\\Users\\foo\\.claude","theme":{}}"#;
        let v: serde_json::Value = serde_json::from_str(raw).unwrap();
        let d = v
            .get("diagnostics")
            .cloned()
            .and_then(|d| serde_json::from_value::<DiagnosticsConfig>(d).ok())
            .unwrap_or_default();
        assert_eq!(d, DiagnosticsConfig::default());
    }

    #[test]
    fn diagnostics_partial_field_serde_default_other() {
        // 只有 log_level 一个字段的 config 也能 deserialize（其他字段拿 default）
        let raw = r#"{"log_level":"debug"}"#;
        let d: DiagnosticsConfig = serde_json::from_str(raw).unwrap();
        assert_eq!(d.log_level, "debug");
        assert!(d.log_enabled);
        assert!(d.error_toast);
        assert_eq!(d.max_files, 3);
    }

    #[test]
    fn build_env_filter_accepts_valid_levels() {
        for lv in ["trace", "debug", "info", "warn", "error", "off"] {
            assert!(build_env_filter(lv).is_some(), "level {lv} should parse");
        }
    }

    #[test]
    fn build_env_filter_rejects_garbage() {
        assert!(build_env_filter("nonsense=42=42=").is_none());
    }

    #[test]
    fn write_then_read_diagnostics_roundtrip() {
        let tmp = std::env::temp_dir().join(format!("ccm-log-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();

        let cfg = DiagnosticsConfig {
            log_enabled: true,
            log_level: "warn".to_string(),
            error_toast: false,
            max_files: 7,
        };
        write_diagnostics_to_config(&tmp, &cfg).unwrap();

        let back = read_diagnostics_from_config(&tmp);
        assert_eq!(back, cfg);

        // 不破坏 config.json 已有字段（手写 theme/claudeDir 后再写 diagnostics）
        let cfg_path = tmp.join("config.json");
        let raw = std::fs::read_to_string(&cfg_path).unwrap();
        let v: serde_json::Value = serde_json::from_str(&raw).unwrap();
        assert!(v.get("diagnostics").is_some());

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn write_diagnostics_preserves_other_fields() {
        let tmp = std::env::temp_dir().join(format!("ccm-log-test2-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();

        // 先写 config.json 含 theme + claudeDir（模拟老用户）。
        // 用 r##"..."## 加一层 `#` 是因为 JSON 内含 "#000000"，普通 r#"..."# 会被
        // "# 提前终止（raw string 的结束分隔符是 `"` 后跟匹配数量的 `#`）
        let original = r##"{"claudeDir":"C:\\foo","theme":{"bg":"#000000"}}"##;
        std::fs::write(tmp.join("config.json"), original).unwrap();

        // 再写 diagnostics
        write_diagnostics_to_config(&tmp, &DiagnosticsConfig::default()).unwrap();

        // 老字段必须还在
        let raw = std::fs::read_to_string(tmp.join("config.json")).unwrap();
        let v: serde_json::Value = serde_json::from_str(&raw).unwrap();
        assert_eq!(v.get("claudeDir").and_then(|x| x.as_str()), Some("C:\\foo"));
        assert!(v.get("theme").is_some());
        assert!(v.get("diagnostics").is_some());

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn find_latest_log_picks_newest_mtime() {
        let tmp = std::env::temp_dir().join(format!("ccm-log-test3-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();

        std::fs::write(tmp.join("monitor.2026-01-01.log"), "old").unwrap();
        std::thread::sleep(std::time::Duration::from_millis(20));
        std::fs::write(tmp.join("monitor.2026-05-25.log"), "new").unwrap();
        // 非 log 文件不应被选中
        std::fs::write(tmp.join("notes.txt"), "noise").unwrap();

        let latest = find_latest_log_file(&tmp).unwrap();
        assert!(latest.to_string_lossy().contains("monitor.2026-05-25"));

        let _ = std::fs::remove_dir_all(&tmp);
    }
}
