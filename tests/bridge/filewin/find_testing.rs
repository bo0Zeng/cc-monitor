//! 量具：**一台合成后端** ＋ **一棵合成的真目录树**。
//!
//! # 🔴 一、它为什么必须存在（而不是「起一个真后端」）
//!
//! `src/backend` **不是 `monitor` 的依赖**，也不是同一个 workspace 的成员
//! （它自己那份 `Cargo.toml` 的头注逐字：「Standalone crate, intentionally NOT part
//! of a workspace」）⇒ `cargo test -p monitor --lib` 这一趟里，**后端那份代码
//! 既没被链进来、也没有一个保证存在的二进制**。
//!
//! 真后端那条路存在，但它住 `tests/e2e/`（脚本 ＋ 真 exec），而那些判据**不在
//! `--lib` 那一格的执行链上**（门禁 `cargo` 那一格跑的就是 `--lib`）。
//! ⇒ 要一条**能红的、进得了门禁的**端到端判据，只有这一条路：
//! 在进程里立一台按**冻结契约**答话的后端。
//!
//! # 🔴 二、它答话的依据是那份**冻结的线上契约**，不是我随手编的形状
//!
//! 每一条应答的字段名与语义逐条对着 `src/doc/IPC-PROTOCOL.md §10` 写
//! （那份文档的读者在仓外）。而「我照它写了」这件事**不靠自律**：
//! [`super::tests::the_reply_shapes_match_the_frozen_wire_contract`] 现读那份文档、
//! 把它那几张表里的字段名抠出来，与 `find.rs` 声明的 [`crate::filewin::find::FIND_FIELDS`]
//! ／ [`crate::filewin::find::STATUS_FIELDS`] 对拍成**集合相等**。
//!
//! # ⚠ 三、它**买不到**什么（这是本摞最要紧的一条边界）
//!
//! - **它不是后端。** 它的索引是「把那棵树 `read_dir` 走一遍、存一摞字节」，
//!   与 `src/backend/files/index.rs` 那份（连续 blob ＋ u32 界桩 ＋ overlay）**不是同一份实现**。
//!   ⇒ 本摞判据买到的是**客户端侧那条链**（发命令 · 解析回参 · 画到帧上）真的通，
//!   **不是**后端那份真索引的正确性 —— 后者的判据住 `tests/backend/files/`。
//! - 〔F2 · 2026-09-24〕**它挂在通道宿主的 `Backends` 那一格上**（[`wire_up`]：真回环口、
//!   真钥匙、真 `dial`，窗口手里拿的就是生产那个 `chan::client::Client`）。
//!   ⇒ 「窗口 → 通道 → 路由器」在射程里；**宿主往 `inbound_client` 转交那一跳不在**
//!   （这台合成后端就坐在那一跳的位置上）。那一跳的能力协商（后端没声明的命令一个字节都不发）
//!   由本台后端**照样演**：没在 [`FakeBackend::offered`] 里的命令答 `Peer{Unsupported}`，
//!   而且**不进线上记录**（没发出去）。
//! - **时延买不到。** 这台后端在同一个进程里、走回环。
//!
//! # 四、那棵树：**采结构不采内容**
//!
//! 路径由 [`super::super::corpus::synth_paths`]（生产件，`24e` 就在的那一份）生成，
//! **一个真路径都没有** —— 真路径带个人信息，而判据要的是结构。
//!
//! ⚠ **为了能落到真文件系统上，段长与深度被夹紧了**（[`SEG_CAP`] / [`DEPTH_CAP`]）：
//! `corpus` 那份采样会产出单段上千字符、深度几十级的路径，而
//! Linux 的单段上限是 255 字节、Windows 的整条路径上限是 260 字符。
//! ⇒ **这棵树刻意不与 `真相源/99 §四` 那张分布表对拍** —— 那一条是
//! `corpus_tests` 的活，本模块要的只是「一棵形状不平凡、内容全合成的真树」。
//! 夹紧这件事有判据看着：[`super::tests::the_synthetic_tree_is_not_a_trivial_shape`]。

/// 单段最多这么多字符。Linux 的 `NAME_MAX` 是 255；留大半余量，
/// 顺带让整条路径在 Windows 的 260 上限里也放得下。
///
/// 🔴 **它是真承重的**（第二轮死值验的刀 20 逼出来的 —— 逐条见
/// [`clamped_relatives`] 的头注）：`corpus` 那条长文件名动辄上百字符，
/// 而本常量与 [`DEPTH_CAP`] 一起把「相对那一段的长度」钉在
/// `the_synthetic_tree_is_not_a_trivial_shape` 判的那个预算里。
pub const SEG_CAP: usize = 24;

/// 最多这么多级。`corpus` 的深度中位是 10、p90 是 13，夹到 6 是为了
/// 「`临时目录前缀 + 6×24` 仍然远小于 260」。
pub const DEPTH_CAP: usize = 6;

/// 一棵合成树的**全部条目**（绝对路径），以及它落在盘上的根。
pub struct SynthTree {
    pub root: std::path::PathBuf,
    /// 🔴 **这棵树该有的每一条**（目录 ＋ 文件，根自己不算）。
    ///
    /// 它由**路径算术**算出来（[`closure_of`]），**不是**走一趟磁盘数出来的
    /// —— 那样两侧就同源了，而这一摞的相等断言正是拿它当另一侧。
    pub entries: std::collections::BTreeSet<String>,
}

impl Drop for SynthTree {
    fn drop(&mut self) {
        // 收尾放 `Drop` 里 —— **panic 时它照样跑**，写在用例末尾的不会。
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

impl SynthTree {
    /// 这棵树里路径含 `needle` 的那些条目 —— **预期集**。
    ///
    /// 纯路径过滤，不碰磁盘、不碰那台合成后端。
    pub fn expected(&self, needle: &str) -> std::collections::BTreeSet<String> {
        self.entries
            .iter()
            .filter(|p| p.contains(needle))
            .cloned()
            .collect()
    }
}

/// 把 `corpus` 那几条路径夹紧成「落得到盘上」的相对路径。
///
/// # 🔴 **取的是「最后 `DEPTH_CAP` 段」，不是「前 `DEPTH_CAP` 段」—— 这一条是死值验逼出来的**
///
/// 〔2026-09-21，`24f` 第四刀第二轮死值验的**刀 20**〕上一版取的是**前** `DEPTH_CAP` 段，
/// 而那一刀（把 [`SEG_CAP`] 从 24 放宽到 **200**）**没有红**。逮到的是真东西：
///
/// `corpus::synth_paths` 刻意把长度余量堆到**1～2 段**上，而**头一段必定是最末那一段**
/// （它自己的注释逐字：「最后一段（文件名）优先」）—— 也就是说那条又长又真实的
/// 「长文件名」**长在路径末尾**。取前 6 段就把它**整段丢掉**了
/// ⇒ 留下的全是 2～9 字符的短段 ⇒ **`SEG_CAP` 事实上从不生效**，
/// 而它的头注却写着「它挡住了上千字符的单段」。那句话是假的，
/// 而且假在「一条看起来是门的东西其实是装饰」这个方向上（本仓最贵那族病的形状）。
///
/// ⇒ 改成取**最后** `DEPTH_CAP` 段：那个长文件名留下来了，
/// [`SEG_CAP`] 从此**真的**在夹它，而刀 20 会红。
/// ⚠ 顺带一个好处：这棵树的形状从此更像真数据
/// （`真相源/99 §4.1` 逐字「深路径段短，末尾常有一个长文件名」）。
fn clamped_relatives(n: usize, seed: u64) -> std::collections::BTreeSet<Vec<String>> {
    let mut out = std::collections::BTreeSet::new();
    for p in crate::filewin::corpus::synth_paths(n, seed) {
        let all: Vec<&str> = p.split('/').filter(|s| !s.is_empty()).collect();
        let from = all.len().saturating_sub(DEPTH_CAP);
        let segs: Vec<String> = all[from..]
            .iter()
            .map(|s| s.chars().take(SEG_CAP).collect::<String>())
            .filter(|s| !s.is_empty())
            .collect();
        if !segs.is_empty() {
            out.insert(segs);
        }
    }
    out
}

/// 一摞相对路径 → **它们的前缀闭包**（每一级目录都算一条条目）。
///
/// 回的是 `(相对段列表, 是不是目录)`：某一条是另一条的**真前缀** ⇒ 它必须是目录。
fn closure_of(
    rels: &std::collections::BTreeSet<Vec<String>>,
) -> std::collections::BTreeMap<Vec<String>, bool> {
    let mut all: std::collections::BTreeMap<Vec<String>, bool> = std::collections::BTreeMap::new();
    for segs in rels {
        for k in 1..=segs.len() {
            let prefix = segs[..k].to_vec();
            let is_dir = k < segs.len();
            let e = all.entry(prefix).or_insert(is_dir);
            // 只要有**任何一条**路径把它当中间目录，它就是目录。
            *e = *e || is_dir;
        }
    }
    all
}

/// 在临时目录下**真的**造出一棵合成树。
///
/// `tag` 只用来让并行跑的用例各有自己的根（本仓 `cargo test` 默认并行）。
pub fn plant(tag: &str, n: usize, seed: u64) -> std::io::Result<SynthTree> {
    let root = std::env::temp_dir().join(format!(
        "ccm-filewin-find-{tag}-{}-{}",
        std::process::id(),
        seed
    ));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root)?;
    let rels = clamped_relatives(n, seed);
    let closure = closure_of(&rels);
    let mut entries = std::collections::BTreeSet::new();
    for (segs, is_dir) in &closure {
        let mut p = root.clone();
        for s in segs {
            p.push(s);
        }
        if *is_dir {
            std::fs::create_dir_all(&p)?;
        } else {
            if let Some(parent) = p.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(&p, b"")?;
        }
        entries.insert(p.to_string_lossy().to_string());
    }
    Ok(SynthTree { root, entries })
}

// ═══════════════════════════════════════════════════════════════════
// 那台合成后端
// ═══════════════════════════════════════════════════════════════════

/// 一趟 `files-index-status` 该报什么 —— **这几个数由用例给**，
/// 因为「界面显示的是后端报的那个数」这件事只有喂两组不同的数才判得出来。
#[derive(Clone, Debug)]
pub struct Declared {
    pub rewalk_interval_secs: u64,
    pub age_secs: u64,
    /// `None` = 按 `age_secs > rewalk_interval_secs` 自己算（同后端 `index::is_stale`）；
    /// `Some(b)` = 用例钉死（判据要单独喂 `stale` 那一侧）。
    pub stale: Option<bool>,
    pub unreadable_dirs: u64,
    pub truncated: bool,
    pub browse_watches: u64,
    pub browse_watch_cap: u64,
}

impl Default for Declared {
    fn default() -> Self {
        Self {
            // 🔴 **刻意不是 300。** 后端今天声明的是 300，而这一摞要证明界面画的是
            //    「后端报的那个数」—— 用 300 当夹具就分不出「显示了」与「写死了」。
            rewalk_interval_secs: 4242,
            age_secs: 7,
            stale: None,
            unreadable_dirs: 0,
            truncated: false,
            browse_watches: 0,
            browse_watch_cap: 64,
        }
    }
}

/// 这台合成后端收到过的每一行请求（按顺序）。
pub type WireLog = std::sync::Arc<std::sync::Mutex<Vec<serde_json::Value>>>;

/// 一台按 `src/doc/IPC-PROTOCOL.md §10` 答话的合成后端。
pub struct FakeBackend {
    /// 它手上那份索引（`None` = **还没建过** ⇒ `index_missing: true`）。
    index: Option<Vec<Vec<u8>>>,
    declared: Declared,
    /// 它声明自己认得哪几条命令（`hello.commands`）。
    pub offered: Vec<String>,
    pub log: WireLog,
}

impl FakeBackend {
    pub fn new(offered: &[&str], declared: Declared) -> Self {
        Self {
            index: None,
            declared,
            offered: offered.iter().map(|s| s.to_string()).collect(),
            log: std::sync::Arc::new(std::sync::Mutex::new(Vec::new())),
        }
    }

    /// 让它开局就**已经**有一份索引（`files-index-rebuild` 那条阴性对照要它）。
    pub fn preindexed(mut self, root: &std::path::Path) -> Self {
        self.index = Some(walk(root));
        self
    }

    /// 答一条请求。回的是 `(ok, code, message, data)` —— 与
    /// `InboundClient::route_reply` 的入参同形。
    fn answer(
        &mut self,
        cmd: &str,
        args: &serde_json::Value,
    ) -> (
        bool,
        Option<String>,
        Option<String>,
        Option<serde_json::Value>,
    ) {
        match cmd {
            "files-index-status" => (true, None, None, Some(self.status_json())),
            "files-index-rebuild" => {
                let Some(root) = args.get("path").and_then(|v| v.as_str()) else {
                    return (
                        false,
                        Some("bad_path".into()),
                        Some("少了 `path`".into()),
                        None,
                    );
                };
                // 🔴 照后端那一层的语义：**根读不进去是「拒」，不是走出一棵空树**
                //    （`src/doc/IPC-PROTOCOL.md §10` 逐字，常驻那一份一个字节不动）。
                if crate::filewin::source::list_local(std::path::Path::new(root)).is_err() {
                    return (
                        false,
                        Some("unreadable".into()),
                        Some("这个根打不开".into()),
                        None,
                    );
                }
                let paths = walk(std::path::Path::new(root));
                let resident: usize =
                    paths.iter().map(|p| p.len()).sum::<usize>() + 4 * paths.len();
                let entries = paths.len();
                self.index = Some(paths);
                (
                    true,
                    None,
                    None,
                    Some(serde_json::json!({
                        "path": root,
                        "entries": entries,
                        "resident_bytes": resident,
                        "unreadable_dirs": self.declared.unreadable_dirs,
                        "truncated": self.declared.truncated,
                    })),
                )
            }
            "files-browse" => {
                let n = args
                    .get("dirs")
                    .and_then(|v| v.as_array())
                    .map(|a| a.len())
                    .unwrap_or(0);
                self.declared.browse_watches = n as u64;
                (
                    true,
                    None,
                    None,
                    Some(serde_json::json!({
                        "added": n,
                        "removed": 0,
                        "rejected": 0,
                        "browse_watch_cap": self.declared.browse_watch_cap,
                    })),
                )
            }
            "files-find" => {
                let Some(needle) = args.get("needle").and_then(|v| v.as_str()) else {
                    return (
                        false,
                        Some("bad_args".into()),
                        Some("少了 `needle`".into()),
                        None,
                    );
                };
                let Some(idx) = self.index.as_ref() else {
                    // **索引还没建过** —— 几个数全 0，而那不是「没搜到」。
                    return (
                        true,
                        None,
                        None,
                        Some(serde_json::json!({
                            "hits": Vec::<String>::new(),
                            "total_hits": 0,
                            "truncated": false,
                            "scanned": 0,
                            "index_age_secs": 0,
                            "index_missing": true,
                        })),
                    );
                };
                let n = needle.as_bytes();
                let hits: Vec<&Vec<u8>> = idx.iter().filter(|p| contains(p, n)).collect();
                let wire: Vec<serde_json::Value> = hits.iter().map(|p| to_json(p)).collect();
                (
                    true,
                    None,
                    None,
                    Some(serde_json::json!({
                        "hits": wire,
                        "total_hits": hits.len(),
                        "truncated": false,
                        "scanned": idx.len(),
                        "index_age_secs": self.declared.age_secs,
                        "index_missing": false,
                    })),
                )
            }
            // 〔F2〕列目录：走 `list_local`（理由同 [`walk`] 头注：扫描型判据不许裸遍历），
            //   按线上契约把每一条摊成 `{path, kind, size}`。
            "files-ls" => {
                let Some(dir) = args.get("path").and_then(|v| v.as_str()) else {
                    return (
                        false,
                        Some("bad_path".into()),
                        Some("少了 `path`".into()),
                        None,
                    );
                };
                match crate::filewin::source::list_local(std::path::Path::new(dir)) {
                    Ok(rows) => {
                        let entries: Vec<serde_json::Value> = rows
                            .iter()
                            .map(|r| {
                                serde_json::json!({
                                    "path": r.path,
                                    "kind": if r.is_dir { "dir" } else { "file" },
                                    "size": r.size,
                                })
                            })
                            .collect();
                        (
                            true,
                            None,
                            None,
                            Some(serde_json::json!({ "entries": entries, "truncated": false })),
                        )
                    }
                    Err(e) => (false, Some("unreadable".into()), Some(e), None),
                }
            }
            // 〔F2〕那条路径上有没有东西：列它的上一级，找那个名字。
            "files-stat" => {
                let Some(p) = args.get("path").and_then(|v| v.as_str()) else {
                    return (
                        false,
                        Some("bad_path".into()),
                        Some("少了 `path`".into()),
                        None,
                    );
                };
                let parent = crate::filewin::source::parent_dir(p);
                let name = crate::filewin::source::remote_basename(p);
                let hit = crate::filewin::source::list_local(std::path::Path::new(&parent))
                    .map(|rows| rows.iter().any(|r| r.name == name))
                    .unwrap_or(false);
                if hit {
                    (
                        true,
                        None,
                        None,
                        Some(serde_json::json!({ "path": p, "kind": "file" })),
                    )
                } else {
                    (
                        false,
                        Some("unreadable".into()),
                        Some("这个路径读不到".into()),
                        None,
                    )
                }
            }
            // 〔F7a · 第三波 09-24〕读一份文本：按路径里的字眼演后端那几形
            //   （`binary` ⇒ `not_text` · `huge` ⇒ `too_large` · `gone` ⇒ `unreadable`），其余交回
            //   `text of <path>` —— 判据要的是「窗口发了什么、怎么落那几形」，不是真读盘。
            "files-read-text" => {
                let Some(p) = args.get("path").and_then(|v| v.as_str()) else {
                    return (
                        false,
                        Some("bad_path".into()),
                        Some("少了 `path`".into()),
                        None,
                    );
                };
                for (word, code) in [
                    ("binary", "not_text"),
                    ("huge", "too_large"),
                    ("gone", "unreadable"),
                ] {
                    if p.contains(word) {
                        return (false, Some(code.into()), Some(format!("{code}：{p}")), None);
                    }
                }
                let text = format!("text of {p}");
                let n = text.len();
                (
                    true,
                    None,
                    None,
                    Some(serde_json::json!({ "path": p, "text": text, "bytes": n })),
                )
            }
            // 〔F7a · 第三波 09-24〕复制：只记下来、不落盘；`root` 里带 `refuse` ⇒ 按围栏那一档拒，
            //   否则回一个定值字节数（判据要的是「那个数原样带回来」）。
            "files-copy" => {
                let root = args.get("root").and_then(|v| v.as_str()).unwrap_or("");
                if root.contains("refuse") {
                    return (
                        false,
                        Some("refused".into()),
                        Some("refuse write: 围栏".into()),
                        None,
                    );
                }
                (
                    true,
                    None,
                    None,
                    Some(serde_json::json!({ "path": root, "bytes": 42 })),
                )
            }
            // 〔F2〕写面五条：**只记下来、不落盘**（判据要的是「窗口发了哪一条、参数长什么样」），
            //   `root` 里带 `refuse` 的一律按后端围栏那一档拒（`refused`）。
            "files-mkdir" | "files-delete" | "files-rename" | "files-chmod"
            | "files-write-text" => {
                let root = args.get("root").and_then(|v| v.as_str()).unwrap_or("");
                if root.contains("refuse") {
                    return (
                        false,
                        Some("refused".into()),
                        Some("refuse write: 围栏".into()),
                        None,
                    );
                }
                (true, None, None, Some(serde_json::json!({ "path": root })))
            }
            // 〔F7b〕新建空文件：同写面五条「只记下来、不落盘」；`root` 带 `refuse` ⇒ 围栏那一档，
            //   `rel` 带 `exists` ⇒ 「目标已经在了」那一档（后端 `O_EXCL` 失败走的是 `io_failed`）。
            "files-create" => {
                let root = args.get("root").and_then(|v| v.as_str()).unwrap_or("");
                let rel = args.get("rel").and_then(|v| v.as_str()).unwrap_or("");
                if root.contains("refuse") {
                    return (
                        false,
                        Some("refused".into()),
                        Some("refuse write: 围栏".into()),
                        None,
                    );
                }
                if rel.contains("exists") {
                    return (
                        false,
                        Some("io_failed".into()),
                        Some("目标已经在了（不覆盖）".into()),
                        None,
                    );
                }
                (
                    true,
                    None,
                    None,
                    Some(serde_json::json!({ "path": format!("{root}/{rel}"), "bytes": 0 })),
                )
            }
            other => (
                false,
                Some("unknown_command".into()),
                Some(format!("`{other}` 不认")),
                None,
            ),
        }
    }

    fn status_json(&self) -> serde_json::Value {
        let d = &self.declared;
        let (missing, entries, resident) = match self.index.as_ref() {
            None => (true, 0usize, 0usize),
            Some(idx) => (
                false,
                idx.len(),
                idx.iter().map(|p| p.len()).sum::<usize>() + 4 * idx.len(),
            ),
        };
        let stale = d
            .stale
            .unwrap_or(!missing && d.age_secs > d.rewalk_interval_secs);
        serde_json::json!({
            "index_missing": missing,
            "entries": entries,
            "resident_bytes": resident,
            "unreadable_dirs": if missing { 0 } else { d.unreadable_dirs },
            "truncated": if missing { false } else { d.truncated },
            "age_secs": if missing { 0 } else { d.age_secs },
            "rewalk_interval_secs": d.rewalk_interval_secs,
            "stale": stale,
            "browse_watches": d.browse_watches,
            "browse_watch_cap": d.browse_watch_cap,
        })
    }
}

/// 字节级子串（后端那一侧逐字是「匹配是字节级的，不做归一化」）。
fn contains(hay: &[u8], needle: &[u8]) -> bool {
    if needle.is_empty() {
        return true;
    }
    hay.windows(needle.len()).any(|w| w == needle)
}

/// 路径字节 → 线上那两种形（同 `src/backend/files/raw.rs::to_json` 的契约）。
fn to_json(bytes: &[u8]) -> serde_json::Value {
    match std::str::from_utf8(bytes) {
        Ok(s) => serde_json::Value::String(s.to_string()),
        Err(_) => {
            const D: &[u8; 16] = b"0123456789abcdef";
            let mut h = String::with_capacity(bytes.len() * 2);
            for b in bytes {
                h.push(D[(b >> 4) as usize] as char);
                h.push(D[(b & 0x0f) as usize] as char);
            }
            serde_json::json!({ "b16": h })
        }
    }
}

/// 走一趟磁盘（**显式栈，不递归** —— 同后端 `index::build` 的理由：
/// 目录深度是用户数据说了算的）。
///
/// # 🔴 为什么它列目录走 [`crate::filewin::source::list_local`]，而不是裸 `read_dir`
///
/// 两条独立的理由，都不是风格：
///
/// 1. **`scanning_guard_registry` 不许测试段裸遍历目录**（现打：第一版就是裸的，
///    被 `no_new_guard_walks_the_tree_without_excluding_itself` 当场逮住）。
///    那条判据的存量清单逐字「**只许变短**，不许往里加」⇒ 只能换写法。
/// 2. 换出来的写法**更好**，不是将就：`list_local` 是**生产件**（`24e` 第一刀就在，
///    自己有判据），于是这台合成后端的「列一个目录」与窗口自己的「列一个目录」
///    是**同一份实现** —— 两处漂开这件事在这一格结构上不存在。
///
/// ⚠ **它买不到「原始字节」那一档**：`list_local` 的 `path` 是
/// `to_string_lossy` 过的（同 [`crate::filewin::source::Row::lossy_name`]）
/// ⇒ 这棵合成树刻意**全 ASCII**，非 UTF-8 那一形由
/// `super::tests::a_non_utf8_hit_keeps_its_bytes_and_says_it_is_lossy`
/// 单独喂一份十六进制夹具去判，不靠这棵树。
fn walk(root: &std::path::Path) -> Vec<Vec<u8>> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(rows) = crate::filewin::source::list_local(&dir) else {
            continue;
        };
        for r in rows {
            out.push(r.path.as_bytes().to_vec());
            if r.is_dir {
                stack.push(std::path::PathBuf::from(&r.path));
            }
        }
    }
    out
}

/// 一台**挂在通道宿主上**的合成后端（〔F2 · 2026-09-24〕从前挂在进程级登记表上）。
///
/// 回的那个 `WireLog` 是**它收到过的每一行请求**（按顺序）——
/// `§3.5.2a` 那条判据数的就是它里面的 `files-index-rebuild`。
pub struct Wired {
    pub origin: String,
    pub log: WireLog,
    /// 窗口手里那条线 —— 生产那个 `chan::client::Client`，拨的是真回环口。
    pub line: crate::filewin::source::Line,
}

/// 把 [`FakeBackend`] 挂成通道宿主的 `Backends`。
struct Hosted {
    be: std::sync::Mutex<FakeBackend>,
    log: WireLog,
}

impl crate::chan::router::Backends for Hosted {
    fn call(
        &self,
        _origin: crate::chan::wire::Origin,
        op: crate::chan::wire::Op,
        payload: crate::chan::wire::Body,
        _left: std::time::Duration,
        _cancel: crate::chan::wire::CancelToken,
    ) -> futures::future::BoxFuture<
        'static,
        Result<crate::chan::wire::Body, crate::chan::wire::CallError>,
    > {
        use crate::chan::wire::{Body, CallError, PeerFault};
        let args: serde_json::Value =
            serde_json::from_slice(&payload.0).unwrap_or(serde_json::Value::Null);
        let mut be = self.be.lock().unwrap();
        // 能力协商（同 `InboundClient::call` 第一行）：没声明的命令**一个字节都不发** ⇒ 不进记录。
        if !be.offered.iter().any(|c| c == &op.0) {
            return Box::pin(async {
                Err(CallError::Peer {
                    why: PeerFault::Unsupported,
                })
            });
        }
        self.log
            .lock()
            .unwrap()
            .push(serde_json::json!({ "cmd": op.0, "args": args }));
        let (ok, code, message, data) = be.answer(&op.0, &args);
        let r = if ok {
            Ok(Body(
                serde_json::to_vec(&data.unwrap_or(serde_json::Value::Null)).unwrap_or_default(),
            ))
        } else {
            // 与宿主那一侧 `backend_route::layer_call_error` 拼的 body 同形。
            Err(CallError::Peer {
                why: PeerFault::Refused {
                    body: Body(
                        serde_json::to_vec(
                            &serde_json::json!({ "code": code, "message": message }),
                        )
                        .unwrap_or_default(),
                    ),
                },
            })
        };
        Box::pin(async move { r })
    }

    fn subscribe(
        &self,
        _origin: crate::chan::wire::Origin,
        _kind: crate::chan::wire::Kind,
        _from: Option<crate::chan::wire::Cursor>,
    ) -> futures::stream::BoxStream<'static, crate::chan::wire::Item> {
        Box::pin(futures::stream::iter([crate::chan::wire::Item::Closed {
            by: crate::chan::wire::By::Ours(crate::chan::wire::OursFault::Misuse),
        }]))
    }
}

/// 把一台合成后端挂到一个真通道口上，拨通，交回窗口要的那条线。**要在 tokio 运行时里调。**
pub async fn wire_up(origin: &str, be: FakeBackend) -> Wired {
    let log = be.log.clone();
    let hosted = std::sync::Arc::new(Hosted {
        be: std::sync::Mutex::new(be),
        log: log.clone(),
    });
    let h = crate::chan::host::start_with(
        hosted,
        crate::chan::host::mint_key(),
        1 << 22,
        std::time::Duration::from_secs(5),
    )
    .await
    .expect("回环口绑得上");
    let line = crate::chan::dial::dial(
        &h,
        crate::chan::wire::Budget {
            until: std::time::Instant::now() + std::time::Duration::from_secs(5),
            cancel: crate::chan::wire::CancelToken::new(),
        },
    )
    .await
    .expect("拨得通、过得了认证");
    Wired {
        origin: origin.to_string(),
        log,
        line,
    }
}

impl Wired {
    /// 线上出现过几条 `cmd == name` 的请求。
    pub fn count(&self, name: &str) -> usize {
        self.log
            .lock()
            .unwrap()
            .iter()
            .filter(|r| r["cmd"].as_str() == Some(name))
            .count()
    }

    /// 线上出现过的命令名，按顺序。
    pub fn cmds(&self) -> Vec<String> {
        self.log
            .lock()
            .unwrap()
            .iter()
            .filter_map(|r| r["cmd"].as_str().map(|s| s.to_string()))
            .collect()
    }
}

// ═══════════════════════════════════════════════════════════════════
// 驱动窗口：打字 → 等答案 → 跑一帧 → 把文字读回来
// ═══════════════════════════════════════════════════════════════════

/// 一台不联网的窗口（远端那一侧，但**永不 `reload`**）。
///
/// 🔴 **每条用例一个自己的 label** —— 不是洁癖：入方向通道登记表是一张
/// **进程内全局**表（`local_origin_test_lock` 的头注逐字讲过两条用例互相
/// 看见对方通道那一形）⇒ 各用各的键就不用抢那把锁。
/// ⚠〔2026-09-23〕从前这句话写的是「用 `Source::Remote` 而不是 `Source::Local`，
/// 后者的 origin 是 `inbound_client::LOCAL_ORIGIN`（一个全局键）」——
/// 本机侧退役之后**没有别的选择了**，但那条纪律本身照旧成立。
///
/// ⚠ 用 [`crate::filewin::shell::FileWindow::seeded`] 而不是 `new`：后者会
/// `reload()`，而远端那一支会去拨真 SFTP —— 本仓红线不许起真连接。
pub fn window_on(wired: &Wired, cwd: &str) -> crate::filewin::shell::FileWindow {
    let origin = wired.origin.as_str();
    let cfg = crate::ssh_source::RemoteConfig {
        host: "example.invalid".into(),
        label: origin.into(),
        port: 22,
        user: "nobody".into(),
        key_path: None,
        backend_path: "/nonexistent".into(),
        host_key_fingerprint: None,
        addresses: Vec::new(),
        jump: None,
    };
    let mut w = crate::filewin::shell::FileWindow::seeded(
        crate::filewin::source::Source::remote(cfg),
        cwd.to_string(),
        tokio::runtime::Handle::try_current().ok(),
        Vec::<crate::filewin::source::Row>::new(),
    );
    w.attach_line(wired.line.clone());
    w
}

/// 跑一帧**生产那个** [`crate::filewin::shell::FileWindow::frame_body`]，
/// 把这一帧真的交给文字排版的每一段文字读回来。
///
/// 量具本体（从 `FullOutput::shapes` 里抠 galley）复用
/// [`crate::filewin::copy::testing::text_in_frame`] —— 不抄第二份。
pub fn frame_text(
    ctx: &egui::Context,
    w: &mut crate::filewin::shell::FileWindow,
    events: Vec<egui::Event>,
) -> Vec<String> {
    let input = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(1280.0, 800.0),
        )),
        events,
        ..Default::default()
    };
    let out = ctx.run_ui(input, |ui| w.frame_body(ui));
    let painted = crate::filewin::copy::testing::text_in_frame(&out);
    out.drop_without_applying_deltas();
    painted.into_iter().map(|(t, _)| t).collect()
}

/// 在搜索框里打一段字（合成事件：先点一下让它拿到焦点，再送文字）。
///
/// ⚠ 焦点要靠**真点一下**拿到 —— egui 的 `Event::Text` 只送给有焦点的控件。
/// 两帧：第一帧点下去，第二帧才送字（命中测试按上一帧的 widget 表做，
/// 同 `super::super::rows` 的 `paint_one_row` 头注那条现打）。
pub fn type_into_search(
    ctx: &egui::Context,
    w: &mut crate::filewin::shell::FileWindow,
    text: &str,
) {
    // 第一帧：找到那个输入框（它左边紧挨着「搜索」那两个字）。
    let input = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(1280.0, 800.0),
        )),
        ..Default::default()
    };
    let out = ctx.run_ui(input, |ui| w.frame_body(ui));
    let painted = crate::filewin::copy::testing::text_in_frame(&out);
    out.drop_without_applying_deltas();
    let label = crate::filewin::copy::testing::rects_of(&painted, "搜索");
    let anchor = label
        .first()
        .copied()
        .expect("这一帧上没有「搜索」那两个字 —— 搜索那一行被谁摘了");
    // 输入框在标签右边；点它的左段（离标签一点点，别点到「重建索引」那颗按钮上）。
    let pos = egui::pos2(anchor.right() + 20.0, anchor.center().y);
    let events = crate::filewin::rows::testing::click_at(pos);
    let _ = frame_text(ctx, w, events);
    // 第二帧：送字。
    let _ = frame_text(ctx, w, vec![egui::Event::Text(text.to_string())]);
    // 🔴 **量具自检**：字真的落进那个框了吗。
    //    没落进去（框没拿到焦点 / 那一行被谁摘了）时，后面每一条断言都会以
    //    「搜出来是空的」的形式红 —— 而那与「搜索坏了」在输出上一模一样。
    //    ⇒ 在这里先分开。
    assert!(
        w.query().contains(text),
        "合成事件没落进搜索框（框里现在是 {:?}，该含 {text:?}）——\n\
         那一帧上这个框没拿到焦点，**下面每一条读数都不许用**。",
        w.query()
    );
}

/// 等那块板子落下**至少**一份新答案。**带上限，绝不挂死。**
///
/// 照 `inbound_client_tests::next_line` 那条纪律办：一条判据该干净地红，
/// 不该让 `cargo test` 悬在那儿（那在 CI 上表现为 job 超时，不是失败列表）。
pub async fn settle(board: &crate::filewin::find::SearchBoard, before: u64, who: &str) {
    for _ in 0..600 {
        if board.rounds() > before {
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    }
    panic!("{who}：等了 3 秒那块板子还是没有新答案（rounds 仍是 {before}）");
}
