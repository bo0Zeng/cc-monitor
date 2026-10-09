//! `common/` —— observe 与 control 两边都要、又不含平台原语的纯工具（谁也不属于的东西：塞进 observe 会让 control 反向依赖它，塞进 platform 更错）。
//!
//! 进 `common/` 要同时满足三条，不满足就留在它自己的模块里（「反正大家都可能用」不是理由）：
//! ① ≥2 个上层用（按层数，不是按文件数）
//! ② 平台无关（不含平台 cfg、不依赖某个 OS 的文件布局或 ABI —— 那是 `platform/` 的事）；做 I/O 不算违反（`std::fs` 在哪都一样）
//! ③ 无域知识（不认识 `WatchEvent` / `ResumeSpec` 这类东西）

pub(crate) mod contract;
pub(crate) mod fs;
/// 只听回环的那一个地址。① stream（常驻监听口）· relay（中转的宿主）· control（本机探口）都绑 / 连它 · ② `std::net` 的常量 · ③ 只认「回环」。
pub mod net;
/// 后端建自家目录的那一个函数。① 原生那一块（退出行为 · 资产目录 · skill 装记录 · 中转钥匙 · 常驻登记）与文件管理那一块（暂存区，
/// `control/files_commit.rs`）都建这一层，两块之间零互相依赖（`files/module_boundary_guard.rs`）· ② unix 权限位那一句是 `std` 的扩展 trait ·
/// ③ 只认「建一层目录、只给本人」。它写盘（建目录）：`readonly_guard` 第四层登记它（`common/own_dir.rs`）。
pub(crate) mod own_dir;
/// 后端自有状态文件的读三态与原子写（旁名唯一 · 0600 · `sync_all` · 失败只删自己的旁名）。① 第四层各份分住 control · accounts · assets ·
/// history · relay · dial，都经它读写 · ② 只用 `std::fs` 与 `creds_core` 的「出生即只给本人」· ③ 只认「一份文件整份读、整份换」。它写盘：`readonly_guard` 第四层登记它。
pub(crate) mod own_state;
/// **路径原始字节的线上两种形**（字符串 / `{"b16": …}`，原住 `files/raw.rs`）。
///
/// 它满足门槛的方式：①（≥2 层）文件管理那一块（整族入参 / 回送）与原生那一块（`dial/terminal.rs`：文件窗口「在此打开终端」
/// 交来的当前目录）都读这一形 —— 两块之间零互相依赖，共用的只许在这一层。②（平台无关）只做 JSON ⇄ 字节。
/// ③（无域知识）它只认「一串字节怎么上线」，不认那是哪个文件、拿去干什么。
pub(crate) mod path_wire;
/// 一次失败给人看的那一句 ＋ 下层原话（原话进复制详情，不上句子）。① control（传输台）与 dial（SFTP）都回它 · ② 纯数据 · ③ 只认「一句 ＋ 原话」。
pub(crate) mod said;
/// 「这台机器上现在有哪些 tmux 会话」那一张快照。① control（`gate::list_sessions` 判活、`ccm` 铸名避让）与 observe（`watcher` 焐热）都用 ——
/// 住 `control/` 的话 watcher 要走一条 `observe → control` 的回边，住 `observe/` 的话 Gate 引用不到 · ② 起的是跨平台的 `tmux` · ③ 只认「会话名 + `@ccm_sid`」。
pub(crate) mod session_snapshot;
/// 公历换算与 ISO8601 时刻。① control（额度时刻排字 · 账号库操作戳）、observe（搜索与历史清单的时刻）、agents（子运行时刻 · 额度窗口重置）都用 ·
/// ② 纯算术 · ③ 只认日历。
pub(crate) mod time;
/// 「tmux 的打印通道必须是 UTF-8」这一个口径的家。① 只在「`-u` 与 `LC_ALL=C.UTF-8` 是一个口径的两种表示」这个读法下成立
/// （同一个开关 · `-u` 压在 env 之上 · 两者在调用点上不可互换，逐条在该模块头注里）。
pub(crate) mod tmux_utf8;
