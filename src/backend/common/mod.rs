//! `common/` —— observe 与 control 两边都要、又不含平台原语的纯工具（谁也不属于的东西：塞进 observe 会让 control 反向依赖它，塞进 platform 更错）。
//!
//! 进 `common/` 要同时满足三条，不满足就留在它自己的模块里（「反正大家都可能用」不是理由）：
//! ① ≥2 个上层用（按层数，不是按文件数）
//! ② 平台无关（不含平台 cfg、不依赖某个 OS 的文件布局或 ABI —— 那是 `platform/` 的事）；做 I/O 不算违反（`std::fs` 在哪都一样）
//! ③ 无域知识（不认识 `WatchEvent` / `ResumeSpec` 这类东西）

/// 只往后追加的文件（会话记录）的增量读：读到哪了的账（[`append_read::Tailed`]）· 按块读（[`append_read::Blocks`]）。① observe（全文搜索的常驻索引 · watcher · 子运行 · 骨架）
/// 与 history（历史清单的缓存）都用 · ② 读盘经 `platform::fs::Ro` · ③ 只认「一份只往后长的文件读到了哪」，不认里面是什么（原 `observe/fs.rs`）。
pub(crate) mod append_read;
// 一组一条那几条命令共用的票 · 进度 · 撤 · 同名怎么答 · 结局那一行（同台三条与两台之间的复制各引各的）。
/// 报到序号。① control（起会话之前记下）与 observe（见到会话报到时加一）都用 · ② 一个进程内的计数 · ③ 只认「第几条报到」。
pub(crate) mod arrival_seq;
pub(crate) mod batch_book;
/// 成品格的两种标记类型（写好的字 · 语气）。① observe（会话事实 · 过程行）· agents（通用记录）· stream（会话帧）都用 · ② 纯数据 ·
/// ③ 只认「这一格是写好的字 / 语气」。
pub(crate) mod cells;
pub(crate) mod contract;
/// 内容摘要（SHA-256 流式那一形 ＋ 十六进制长度；另有只答相同 / 不同的 FNV 那一形，资产与账号库的 MCP 登录都用）。① 文件管理那一块（读 · 写面比对 · 提交核暂存件）与原生那一块（传输台上传边传边算）都用 ·
/// ② 纯算术（`ring`）· ③ 只认「一串字节的摘要」。
pub(crate) mod digest;
/// 后端够用户文件的那一扇门（这台自己的文件管理面，`files-*` 那几问的进程内分派）。① 账号库 · 资产 · 足迹 · 控制面 · 中转都经它读写用户文件 ·
/// ② 只收发线上类型，平台差异在文件管理那一面 · ③ 只认「一份文件的读 / 写 / 列 / 删」，不认那是谁的配置。
pub(crate) mod door;
/// 读路径的越界围栏（`canonicalize` ＋ 前缀校验，挡符号链接逃逸 / `../` 穿越）。① observe（按路径读会话 · 全文搜索 · 骨架）与 history（会话清单 · 藏起来的目录）都过它 ·
/// ② 解链接经 `std::fs` · ③ 只认「一个根 ＋ 一个候选路径」，根是哪一个由调用方给（原 `observe/fence.rs`）。
pub(crate) mod fence;
/// 一份 JSON 原文里就地设一格 / 往数组加一项，别的字节不动（＋ 行号 ＋ diff）。① 足迹（「待办」要贴的改法）与适配层（settings 里就地补中转地址那一格）都用 ·
/// ② 纯文本 ＋ `serde_json` · ③ 只认「JSON 原文 ＋ 一条键路径」，不认那是谁的设置。
pub(crate) mod json_patch;
/// 只听回环的那一个地址。① stream（常驻监听口）· relay（中转的宿主）· control（本机探口）都绑 / 连它 · ② `std::net` 的常量 · ③ 只认「回环」。
pub mod net;
/// 后端自家目录的几个名字与权限位（只是常量；建目录经 `platform::fs::Own::ensure_dir`）。① 原生那一块与文件管理那一块（暂存区）都认这几个名字 ·
/// ② 只是常量 · ③ 一个盘都不碰。
pub(crate) mod own_dir;
/// 后端自有状态文件的读三态与编码。① 写自家状态的各份（control · accounts · assets · history · relay · dial）读都经它 ·
/// ② 读经 `platform::fs::Ro`，写不在这里（经 `platform::fs::Own::write`）· ③ 只认「一份文件整份读、整份换」。
pub(crate) mod own_state;
/// 一串互不相干的活分给几条线程、按原序交回（线程数只在这里算）· 常驻线程池。① observe（全文搜索 · 记录预读）与 history（历史清单按记录目录扫）都用 ·
/// ② `std::thread` · ③ 只认「一串活」，不认活是什么（原 `observe/par.rs`）。
pub(crate) mod par;
/// **路径原始字节的线上两种形**（字符串 / `{"b16": …}`，原住 `files/raw.rs`）。
///
/// 它满足门槛的方式：①（≥2 层）文件管理那一块（整族入参 / 回送）与原生那一块（`dial/terminal.rs`：文件窗口「在此打开终端」
/// 交来的当前目录 · 传输台的本机落点）都读这一形 —— 两块之间零互相依赖，共用的只许在这一层。②（平台无关）只做 JSON ⇄ 字节，
/// 以及字节还原成 `PathBuf`（`OsStr::from_encoded_bytes_unchecked`，标准库在哪都一样）。
/// ③（无域知识）它只认「一串字节怎么上线」，不认那是哪个文件、拿去干什么。
pub(crate) mod path_wire;
/// 一次失败给人看的那一句 ＋ 下层原话（原话进复制详情，不上句子）。① control（传输台）与 dial（SFTP）都回它 · ② 纯数据 · ③ 只认「一句 ＋ 原话」。
/// 根底下路径的两道解析（词法 ＋ 父目录解链接）。① 文件管理那一块（写面 · 提交 · 解压 · 分块上传）与原生那一块（传输台的本机落点）都过它 ·
/// ② 读盘经 `platform::fs::Ro`（解链接）· ③ 只认「根 ＋ 一段相对路径」，不认落点是谁的数据。
pub(crate) mod root_path;
pub(crate) mod said;
/// 「此刻显示这个会话的终端」那一形（`session-terminals` 的回包 · `terminal-processes` 的入参）。① observe（读出来）与 dial（拿去对这台的连接表）
/// 都用，界面原样转交 · ② 纯数据 · ③ 只认「一个终端 ＋ 它那条 ssh 连接的四元组」。
pub(crate) mod seen_terminal;
/// 「这台机器上现在有哪些 tmux 会话」那一张快照。① control（`gate::list_sessions` 判活、`ccm` 铸名避让）与 observe（`watcher` 焐热）都用 ——
/// 住 `control/` 的话 watcher 要走一条 `observe → control` 的回边，住 `observe/` 的话 Gate 引用不到 · ② 起的是跨平台的 `tmux` · ③ 只认「会话名 + `@ccm_sid`」。
pub(crate) mod session_snapshot;
/// 暂存区上传件的键长与后缀。① 原生那一块（传输台造键、往暂存区写）与文件管理那一块（提交、孤儿扫）都认 · ② 只是常量 · ③ 只认「一个键 ＋ 一个后缀」。
pub(crate) mod staging;
/// 公历换算与 ISO8601 时刻。① control（额度时刻排字 · 账号库操作戳）、observe（搜索与历史清单的时刻）、agents（子运行时刻 · 额度窗口重置）都用 ·
/// ② 纯算术 · ③ 只认日历。
pub(crate) mod time;
/// 「tmux 的打印通道必须是 UTF-8」这一个口径的家。① 只在「`-u` 与 `LC_ALL=C.UTF-8` 是一个口径的两种表示」这个读法下成立
/// （同一个开关 · `-u` 压在 env 之上 · 两者在调用点上不可互换，逐条在该模块头注里）。
pub(crate) mod tmux_utf8;
/// 带键集合的「没变 / 只给变了的 / 整份」（每种问法一份备忘）。① 会话列表（history）与目录列表（files）都答这一形 · ② 序列化 ＋ 摘要 ·
/// ③ 只认「排好的一串键 ＋ 成品」，不认那是会话还是文件。
pub(crate) mod views;
