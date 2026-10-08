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
//! 把它那几张表里的字段名抠出来，与 `find.rs` 声明的 [`crate::find::FIND_FIELDS`]
//! ／ [`crate::find::STATUS_FIELDS`] 对拍成**集合相等**。
//!
//! # ⚠ 三、它**买不到**什么（这是本摞最要紧的一条边界）
//!
//! - **它不是后端。** 它的索引是「把那棵树 `read_dir` 走一遍、存一摞字节」，
//!   与 `src/backend/files/index.rs` 那份（连续 blob ＋ u32 界桩 ＋ overlay）**不是同一份实现**。
//!   ⇒ 本摞判据买到的是**客户端侧那条链**（发命令 · 解析回参 · 画到帧上）真的通，
//!   **不是**后端那份真索引的正确性 —— 后者的判据住 `tests/backend/files/`。
//! - **它挂在通道宿主的 `Backends` 那一格上**（[`wire_up`]：真回环口、
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
//! ⇒ **这棵树刻意不与那张分布表对拍** —— 那一条是
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
    /// 这棵树里路径含 `needle` 的那些条目 —— **预期集**（[`remote_form`] 那一形：窗口看见的就是它）。
    ///
    /// 纯路径过滤，不碰磁盘、不碰那台合成后端。
    pub fn expected(&self, needle: &str) -> std::collections::BTreeSet<String> {
        self.entries
            .iter()
            .filter(|p| p.contains(needle))
            .map(|p| remote_form(p))
            .collect()
    }

    /// 根交给窗口的那一形（窗口的 `cwd`）。
    pub fn remote_root(&self) -> String {
        remote_form(&self.root.to_string_lossy())
    }
}

/// 本机夹具路径 → 当「远端」交给窗口的那一形：分隔符一律 `/`。
///
/// 要求：「只有远端，没有本机」：远端路径恒用 `/`，窗口切路径只认 `/`（`source::parent_dir` 头注）。
/// 本摞拿本机临时目录演那台远端 —— Linux 上恒等；Windows 上 `C:\…\x` 换成 `C:/…/x`（Windows 的文件 API 两种都认，
/// 合成后端拿它回盘上照样打得开）。不换 ⇒ 窗口把整条 `C:\…\x` 当成一个名字（Windows runner 上那几条红）。
pub fn remote_form(local: &str) -> String {
    remote_form_with(local, std::path::MAIN_SEPARATOR)
}

/// 同 [`remote_form`]，本机分隔符由调用方交 —— 判据在 Linux 上注入 `\` 演 Windows 那一形。
pub fn remote_form_with(local: &str, sep: char) -> String {
    local.replace(sep, "/")
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
/// （「深路径段短，末尾常有一个长文件名」）。
fn clamped_relatives(n: usize, seed: u64) -> std::collections::BTreeSet<Vec<String>> {
    let mut out = std::collections::BTreeSet::new();
    for p in crate::corpus::synth_paths(n, seed) {
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
    /// 读不进去的那几个目录（后端交前 20 个的线上形；夹具直接给）。
    pub unreadable_paths: Vec<String>,
    pub truncated: bool,
    pub browse_watches: u64,
    pub browse_watch_cap: u64,
    /// 后端声明的冷启动首建估计。
    pub cold_first_build_secs: u64,
    /// 没走进去的挂载点个数。
    pub skipped_mounts: u64,
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
            unreadable_paths: Vec::new(),
            truncated: false,
            browse_watches: 0,
            browse_watch_cap: 64,
            // 🔴 同上：**刻意不是 10**（后端今天声明的那个数）。
            cold_first_build_secs: 4747,
            skipped_mounts: 0,
        }
    }
}

/// 这台合成后端收到过的每一行请求（按顺序）。
pub type WireLog = std::sync::Arc<std::sync::Mutex<Vec<serde_json::Value>>>;

/// 一台按 `src/doc/IPC-PROTOCOL.md §10` 答话的合成后端。
pub struct FakeBackend {
    /// 它手上那份索引：根 ＋ 每条（路径, 是不是目录）（`None` = **还没建过** ⇒ `index_missing: true`）。
    index: Option<(String, Vec<(Vec<u8>, bool)>)>,
    /// 它当自己家目录的那一处（`files-find` 不给 `under` 时的范围、`files-index-rebuild` 不给 `path` 时的根）。
    home: Option<String>,
    /// 给了 ⇒ 一屏最多回这么多条（不管窗口要多少）—— 判据要一屏小到最后一行露在帧上。
    page_cap: Option<usize>,
    declared: Declared,
    /// 它声明自己认得哪几条命令（`hello.commands`）。
    pub offered: Vec<String>,
    pub log: WireLog,
    /// 送进来的块（`(key, seq)` → 内容），`files-commit-text` 按块号读回拼起来。
    chunks: std::collections::BTreeMap<(String, u64), String>,
    /// 最近一次提交成功拼出来的那一份（`(root/rel, 全文)`）—— 判据拿它与原文比。
    pub committed: std::sync::Arc<std::sync::Mutex<Option<(String, String)>>>,
    /// 第几块（块号）起按「盘满」那一档拒（演「送到一半断了」）。
    pub refuse_stage_at: Option<u64>,
    /// 合成后端的「盘」：完整路径 → 此刻那份文本（没登记的路径 = `text of <path>`）。
    /// 读交出这一份的摘要，存盘（`files-write-text` / `files-commit-text`）按 CAS 比它 —— 判据改它就是「别人在这期间写了」。
    pub disk: std::sync::Arc<std::sync::Mutex<std::collections::HashMap<String, String>>>,
    /// 给了 ⇒ `files-index-rebuild` 的**应答扣住**，等用例放行才回。
    /// 索引照常当场建好（与后端「走完才回」的时序不同，但窗口只看应答什么时候到）——
    /// 判据要在「重走还在飞」那一刻跑一帧，看首建那一行在不在。
    hold_rebuild: Option<std::sync::Arc<tokio::sync::Notify>>,
    /// `files-grep` 回的那一份成品（`None` ⇒ 按「这台后端拒」那一档：`unreadable`）。
    pub grep_reply: Option<serde_json::Value>,
    /// 给了 ⇒ `files-grep` 的应答扣住，等用例放行才回（「停」那条判据要在它还在飞那一刻撤）。
    hold_grep: Option<std::sync::Arc<tokio::sync::Notify>>,
    /// `files-delete` 一趟一趟回的「还剩几条」（取一个用一个；用完 ⇒ 不带这一格，即删完了）。
    pub delete_left: std::sync::Arc<std::sync::Mutex<std::collections::VecDeque<u64>>>,
    /// 路径换名（截图那几张）：`(窗口看到的前缀, 盘上真的前缀)` —— 进来的参数把前者换成后者再答，答出去的再换回来，
    /// 窗口里的面包屑就是一台真机器那样的 `/home/user/…`，看不到沙箱路径。
    pub alias: Option<(String, String)>,
}

impl FakeBackend {
    pub fn new(offered: &[&str], declared: Declared) -> Self {
        Self {
            index: None,
            home: None,
            page_cap: None,
            declared,
            offered: offered.iter().map(|s| s.to_string()).collect(),
            log: std::sync::Arc::new(std::sync::Mutex::new(Vec::new())),
            chunks: Default::default(),
            committed: Default::default(),
            refuse_stage_at: None,
            disk: Default::default(),
            hold_rebuild: None,
            grep_reply: None,
            hold_grep: None,
            delete_left: Default::default(),
            alias: None,
        }
    }

    /// 把 `files-grep` 的应答扣到 `gate` 被 `notify_one` 为止。
    pub fn holding_grep(mut self, gate: std::sync::Arc<tokio::sync::Notify>) -> Self {
        self.hold_grep = Some(gate);
        self
    }

    /// 合成盘上此刻那一份。
    fn on_disk(&self, path: &str) -> String {
        self.disk
            .lock()
            .unwrap()
            .get(path)
            .cloned()
            .unwrap_or_else(|| format!("text of {path}"))
    }

    /// CAS 那一关（同后端 `overwrite_text_expecting` 的形状）：`expect.sha256` 必须等于盘上那份的摘要。
    fn cas(
        &self,
        path: &str,
        args: &serde_json::Value,
    ) -> Result<
        (),
        (
            bool,
            Option<String>,
            Option<String>,
            Option<serde_json::Value>,
        ),
    > {
        let want = args["expect"]["sha256"].as_str();
        match want {
            None => Err((
                false,
                Some("bad_args".into()),
                Some("少了 `expect`".into()),
                None,
            )),
            Some(w) if w != fake_sha256(&self.on_disk(path)) => Err((
                false,
                Some("stale".into()),
                Some(format!("refuse write: {path} 在你打开之后被改过了")),
                None,
            )),
            Some(_) => Ok(()),
        }
    }

    /// 把 `files-index-rebuild` 的应答扣到 `gate` 被 `notify_one` 为止（见 [`FakeBackend::hold_rebuild`]）。
    pub fn holding_rebuild(mut self, gate: std::sync::Arc<tokio::sync::Notify>) -> Self {
        self.hold_rebuild = Some(gate);
        self
    }

    /// 让它开局就**已经**有一份索引（`files-index-rebuild` 那条阴性对照要它）。
    pub fn preindexed(mut self, root: &std::path::Path) -> Self {
        self.index = Some((remote_form(&root.to_string_lossy()), walk(root)));
        self
    }

    /// 一屏最多回 `n` 条。
    pub fn paging_by(mut self, n: usize) -> Self {
        self.page_cap = Some(n);
        self
    }

    /// 给它一个家目录（窗口不开「只搜当前目录」时，搜的与重走的都是这一处）。
    /// 见 [`Self::alias`]。
    pub fn aliased(mut self, shown: &str, real: &std::path::Path) -> Self {
        self.alias = Some((shown.to_string(), real.to_string_lossy().to_string()));
        self
    }

    pub fn homed(mut self, home: &std::path::Path) -> Self {
        self.home = Some(remote_form(&home.to_string_lossy()));
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
            // 按内容搜：回用例给的那一份成品（窗口侧只判收、画、跳；走树的正确性判在 `tests/backend/files/grep_tests.rs`）。
            "files-grep" => match &self.grep_reply {
                Some(v) => (true, None, None, Some(v.clone())),
                None => (
                    false,
                    Some("unreadable".into()),
                    Some("要搜的这个目录读不到".into()),
                    None,
                ),
            },
            "files-index-status" => (true, None, None, Some(self.status_json())),
            // 主目录：`homed` 给过 ⇒ 答它（「复制到另一台」那一块选择器缺省停在那台的主目录）；没给 ⇒ 读不到。
            "files-home" => match &self.home {
                Some(h) => (true, None, None, Some(serde_json::json!({ "path": h }))),
                None => (
                    false,
                    Some("unreadable".into()),
                    Some("主目录读不到".into()),
                    None,
                ),
            },
            "files-index-rebuild" => {
                // 不给 `path` ⇒ 家目录（同后端）。
                let root = match args.get("path") {
                    None => self.home.clone(),
                    Some(v) => v.as_str().map(str::to_string),
                };
                let Some(root) = root else {
                    return (
                        false,
                        Some("bad_path".into()),
                        Some("这个根不认".into()),
                        None,
                    );
                };
                let root = root.as_str();
                // 🔴 照后端那一层的语义：**根读不进去是「拒」，不是走出一棵空树**
                //    （`src/doc/IPC-PROTOCOL.md §10` 逐字，常驻那一份一个字节不动）。
                if crate::source::list_local(std::path::Path::new(root)).is_err() {
                    return (
                        false,
                        Some("unreadable".into()),
                        Some("这个根打不开".into()),
                        None,
                    );
                }
                let paths = walk(std::path::Path::new(root));
                let resident: usize =
                    paths.iter().map(|(p, _)| p.len()).sum::<usize>() + 5 * paths.len();
                let entries = paths.len();
                self.index = Some((root.to_string(), paths));
                (
                    true,
                    None,
                    None,
                    Some(serde_json::json!({
                        "path": root,
                        "entries": entries,
                        "resident_bytes": resident,
                        "unreadable_dirs": self.declared.unreadable_dirs,
                        "unreadable_paths": self.declared.unreadable_paths,
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
            // 文件名搜索：**这台合成后端只认字面子串**（对全路径），语法不在它这里 ——
            //   窗口侧只判「发原样的词与号 · 画 · 翻页 · 照后端说的重走」，语法的正确性判在后端那棵树。
            "files-find" => {
                let Some(q) = args.get("query").and_then(|v| v.as_str()) else {
                    return (
                        false,
                        Some("bad_args".into()),
                        Some("少了 `query`".into()),
                        None,
                    );
                };
                // 「整台机器」⇒ 范围是根（`/`）；否则照 `under`。
                let machine = args.get("scope").and_then(|v| v.as_str()) == Some("machine");
                let under = if machine {
                    Some(serde_json::Value::String("/".into()))
                } else {
                    args.get("under").cloned().filter(|v| !v.is_null())
                };
                let offset = args.get("offset").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
                let limit = args
                    .get("limit")
                    .and_then(|v| v.as_u64())
                    .filter(|n| *n > 0)
                    .unwrap_or(1000) as usize;
                let limit = self.page_cap.map_or(limit, |c| c.min(limit));
                let seq = args.get("seq").cloned().unwrap_or(serde_json::Value::Null);
                let home = self.home.clone().map(serde_json::Value::String);
                let fresh_root = under.clone().or(home).unwrap_or(serde_json::Value::Null);
                let Some((root, idx)) = self.index.as_ref() else {
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
                            "stale": false,
                            "index_root": null,
                            "out_of_index": false,
                            "cover_root": fresh_root,
                            "seq": seq,
                            "offset": offset,
                        })),
                    );
                };
                // 范围只认字符串形（字节形一律当「不在索引里」）。
                let scope = under.as_ref().map(|v| v.as_str().map(str::to_string));
                let covered = match &scope {
                    None => true,
                    Some(Some(u)) => u == root || u.starts_with(&format!("{root}/")),
                    Some(None) => false,
                };
                let inside = |p: &[u8]| match &scope {
                    Some(Some(u)) => p.starts_with(format!("{u}/").as_bytes()),
                    Some(None) => false,
                    None => true,
                };
                // 起点：`under` 或家目录；位置 ＝ 父目录相对它那一段。
                let start: Option<String> = under
                    .as_ref()
                    .and_then(|v| v.as_str().map(str::to_string))
                    .or_else(|| self.home.clone());
                let location = |p: &[u8]| -> Vec<u8> {
                    let parent = &p[..p.iter().rposition(|&b| b == b'/').unwrap_or(0)];
                    match &start {
                        Some(st) => {
                            let st = st.trim_end_matches('/').as_bytes();
                            if parent == st {
                                Vec::new()
                            } else if parent.starts_with(st) && parent.get(st.len()) == Some(&b'/')
                            {
                                parent[st.len() + 1..].to_vec()
                            } else {
                                parent.to_vec()
                            }
                        }
                        None => parent.to_vec(),
                    }
                };
                let name = |p: &[u8]| {
                    p[p.iter().rposition(|&b| b == b'/').map_or(0, |i| i + 1)..].to_vec()
                };
                let mut all: Vec<&(Vec<u8>, bool)> = idx
                    .iter()
                    .filter(|(p, _)| inside(p) && contains(p, q.as_bytes()))
                    .collect();
                // 排序：合成后端只认「名称」「位置」两列（其余照索引序），倒序就倒过来。
                match args.get("sort").and_then(|v| v.as_str()) {
                    Some("name") => {
                        all.sort_by(|a, b| name(&a.0).cmp(&name(&b.0)).then(a.0.cmp(&b.0)))
                    }
                    Some("location") => all.sort_by(|a, b| {
                        (location(&a.0), name(&a.0))
                            .cmp(&(location(&b.0), name(&b.0)))
                            .then(a.0.cmp(&b.0))
                    }),
                    _ => {}
                }
                if args.get("desc").and_then(|v| v.as_bool()) == Some(true) {
                    all.reverse();
                }
                let page: Vec<serde_json::Value> = all
                    .iter()
                    .skip(offset)
                    .take(limit)
                    .map(|(p, d)| {
                        // 盘上那两格照夹具读（合成树是现造的临时目录）；名字里对上搜索词的那几段。
                        let md = std::fs::metadata(String::from_utf8_lossy(p).as_ref()).ok();
                        let n = name(p);
                        let marks: Vec<[usize; 2]> = (!q.is_empty())
                            .then(|| {
                                n.windows(q.len())
                                    .enumerate()
                                    .filter(|(_, w)| w.eq_ignore_ascii_case(q.as_bytes()))
                                    .map(|(i, _)| [i, i + q.len()])
                                    .take(1)
                                    .collect()
                            })
                            .unwrap_or_default();
                        serde_json::json!({
                            "path": to_json(p),
                            "kind": if *d { "dir" } else { "file" },
                            "location": to_json(&location(p)),
                            "size": md.as_ref().filter(|m| !m.is_dir()).map(|m| m.len()),
                            "mtime_secs": md.as_ref().and_then(|m| m.modified().ok()).and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|t| t.as_secs()),
                            // 真后端按那台本地钟写好的短写法（合成后端写一个固定的字，窗口只照抄）。
                            "mtime_text": md.as_ref().map(|_| "10-02"),
                            "marks": marks,
                        })
                    })
                    .collect();
                let d = &self.declared;
                (
                    true,
                    None,
                    None,
                    Some(serde_json::json!({
                        "truncated": all.len() > offset + page.len(),
                        "hits": page,
                        "total_hits": all.len(),
                        "scanned": idx.len(),
                        "index_age_secs": d.age_secs,
                        "index_missing": false,
                        "stale": d.stale.unwrap_or(d.age_secs > d.rewalk_interval_secs),
                        "index_root": root,
                        "out_of_index": !covered,
                        "cover_root": if covered { serde_json::Value::String(root.clone()) } else { fresh_root },
                        "seq": seq,
                        "offset": offset,
                        "start": start,
                        "sort": args.get("sort").cloned().unwrap_or("relevance".into()),
                        "desc": args.get("desc").cloned().unwrap_or(false.into()),
                    })),
                )
            }
            // 列目录：走 `list_local`（理由同 [`walk`] 头注：扫描型判据不许裸遍历），
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
                match crate::source::list_local(std::path::Path::new(dir)) {
                    Ok(rows) => {
                        let entries: Vec<serde_json::Value> = rows
                            .iter()
                            .map(|r| {
                                serde_json::json!({
                                    "path": remote_form(&r.path),
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
                    // 不在了 ⇒ `not_found`（同真后端按 `io::ErrorKind` 分的那几个码）。
                    Err(e) if !std::path::Path::new(dir).exists() => {
                        (false, Some("not_found".into()), Some(e), None)
                    }
                    Err(e) => (false, Some("unreadable".into()), Some(e), None),
                }
            }
            // 那条路径上有没有东西：列它的上一级，找那个名字。
            "files-stat" => {
                let Some(p) = args.get("path").and_then(|v| v.as_str()) else {
                    return (
                        false,
                        Some("bad_path".into()),
                        Some("少了 `path`".into()),
                        None,
                    );
                };
                let parent = crate::source::parent_dir(p);
                let name = crate::source::remote_basename(p);
                let hit = crate::source::list_local(std::path::Path::new(&parent))
                    .map(|rows| rows.iter().any(|r| r.name == name))
                    .unwrap_or(false);
                if hit {
                    // 照后端那一格：unix 上送 `mode`（盘上那个文件真的低 12 位），非 unix 缺席。
                    let mut v = serde_json::json!({ "path": p, "kind": "file" });
                    #[cfg(unix)]
                    {
                        use std::os::unix::fs::PermissionsExt as _;
                        if let Ok(md) = std::fs::metadata(p) {
                            v["mode"] = serde_json::json!(md.permissions().mode() & 0o7777);
                        }
                    }
                    (true, None, None, Some(v))
                } else {
                    (
                        false,
                        Some("unreadable".into()),
                        Some("这个路径读不到".into()),
                        None,
                    )
                }
            }
            // 读一份文本：按路径里的字眼演后端那几形
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
                let text = self.on_disk(p);
                let n = text.len();
                (
                    true,
                    None,
                    None,
                    Some(
                        serde_json::json!({ "path": p, "text": text, "bytes": n, "sha256": fake_sha256(&text) }),
                    ),
                )
            }
            // 解压：只记下来、不落盘；`rel` 里带 `taken` 且没带 `fresh` ⇒ 按「落点已在」答 `exists`；
            //   带 `bad` ⇒ 按「不认这种包」拒；否则回一组定值（落点带不带 ` (2)` 看 `fresh`）。
            "files-extract" => {
                let rel = args.get("rel").and_then(|v| v.as_str()).unwrap_or("");
                let fresh = args.get("fresh").and_then(|v| v.as_bool()).unwrap_or(false);
                if rel.contains("bad") {
                    return (
                        false,
                        Some("unsupported".into()),
                        Some("不认这种包".into()),
                        None,
                    );
                }
                if rel.contains("taken") && !fresh {
                    return (
                        false,
                        Some("exists".into()),
                        Some("/srv/taken 已经在了".into()),
                        None,
                    );
                }
                let path = if fresh { "/srv/taken (2)" } else { "/srv/pkg" };
                (
                    true,
                    None,
                    None,
                    Some(serde_json::json!({
                        "path": path, "files": 4, "dirs": 2, "links": 1, "bytes": 3072,
                    })),
                )
            }
            // 算大小：只记下来、回一组定值；`path` 里带 `refuse` ⇒ 按「读不到」拒。
            "files-size" => {
                let path = args.get("path").and_then(|v| v.as_str()).unwrap_or("");
                if path.contains("refuse") {
                    return (
                        false,
                        Some("unreadable".into()),
                        Some("这个路径读不到：PermissionDenied".into()),
                        None,
                    );
                }
                (
                    true,
                    None,
                    None,
                    Some(serde_json::json!({
                        "path": path, "bytes": 2048, "files": 3, "dirs": 2, "links": 1,
                        "other": 0, "skipped_mounts": 0, "unreadable_dirs": 0,
                    })),
                )
            }
            // 复制：只记下来、不落盘；`root` 里带 `refuse` ⇒ 按围栏那一档拒，
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
                // 应答照新后端：`files` / `dirs` 恒在；带 `recursive: true` ⇒ 按「一棵」回一组定值。
                let tree = args.get("recursive").and_then(|v| v.as_bool()) == Some(true);
                let (files, dirs) = if tree { (3, 2) } else { (1, 0) };
                (
                    true,
                    None,
                    None,
                    Some(
                        serde_json::json!({ "path": root, "bytes": 42, "files": files, "dirs": dirs }),
                    ),
                )
            }
            // 写面五条：**只记下来、不落盘**（判据要的是「窗口发了哪一条、参数长什么样」），
            //   `root` 里带 `refuse` 的一律按后端围栏那一档拒（`refused`）。
            // 存盘那一条：先过 CAS（盘上那份 == 打开时那份），过了才记进合成盘、交新摘要。
            "files-write-text" => {
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
                let at = format!("{root}/{rel}");
                if let Err(e) = self.cas(&at, args) {
                    return e;
                }
                let content = args["content"].as_str().unwrap_or("").to_string();
                let sha = fake_sha256(&content);
                let n = content.len();
                self.disk.lock().unwrap().insert(at.clone(), content);
                (
                    true,
                    None,
                    None,
                    Some(serde_json::json!({ "path": at, "bytes": n, "sha256": sha })),
                )
            }
            "files-mkdir" | "files-delete" | "files-rename" | "files-chmod" => {
                let root = args.get("root").and_then(|v| v.as_str()).unwrap_or("");
                if root.contains("refuse") {
                    return (
                        false,
                        Some("refused".into()),
                        Some("refuse write: 围栏".into()),
                        None,
                    );
                }
                // 新名字带 `exists` ⇒ 「落点名被占了」那一档（后端 `exists`，就地改名 / 新建据它说「x 已存在」）。
                let to = args
                    .get(if cmd == "files-rename" { "to" } else { "rel" })
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                // 盘上真有那一项（截图那几张的合成目录）也算被占了。
                let taken = !to.is_empty() && std::path::Path::new(root).join(to).exists();
                if (cmd == "files-rename" || cmd == "files-mkdir")
                    && (to.contains("exists") || taken)
                {
                    return (
                        false,
                        Some("exists".into()),
                        Some("目标已经在了（不覆盖）".into()),
                        None,
                    );
                }
                let mut reply = serde_json::json!({ "path": root });
                // 改权限：回改之前的位（撤销据它；合成值恒 0o644）。
                if cmd == "files-chmod" {
                    reply["mode"] = args.get("mode").cloned().unwrap_or_default();
                    reply["before"] = serde_json::json!(0o644);
                }
                if cmd == "files-delete" {
                    if let Some(n) = self.delete_left.lock().unwrap().pop_front() {
                        reply["remaining"] = serde_json::json!(n);
                    }
                }
                (true, None, None, Some(reply))
            }
            // 新建空文件：同写面五条「只记下来、不落盘」；`root` 带 `refuse` ⇒ 围栏那一档，
            //   `rel` 带 `exists` ⇒ 「目标已经在了」那一档（后端 `O_EXCL` 失败走的是 `exists`）。
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
                        Some("exists".into()),
                        Some("目标已经在了（不覆盖）".into()),
                        None,
                    );
                }
                // 带 `single` 而 `rel` 不是一段名字 ⇒ 后端 `bad_name` 那一档（这里只摆那一形的应答，规则住后端）。
                let single = args.get("single").and_then(|v| v.as_bool()) == Some(true);
                if single && (rel.is_empty() || rel.contains('/') || rel == "." || rel == "..") {
                    return (
                        false,
                        Some("bad_name".into()),
                        Some(format!("名称不能是路径：{rel}")),
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
            // 存盘的两步：按后端 `control/files_commit.rs` 的契约演 —— 同一块只收一次；
            //   提交按块号 `0..chunks` 读回拼起来、总长必须恰好等于 `bytes`，否则拒；不论成败删掉这一键的块。
            "files-stage-chunk" => {
                let key = args["key"].as_str().unwrap_or("").to_string();
                let Some(seq) = args["seq"].as_u64() else {
                    return (
                        false,
                        Some("bad_args".into()),
                        Some("少了 `seq`".into()),
                        None,
                    );
                };
                let content = args["content"].as_str().unwrap_or("").to_string();
                if content.is_empty() {
                    return (false, Some("bad_args".into()), Some("空块".into()), None);
                }
                if self.refuse_stage_at.is_some_and(|at| seq >= at) {
                    return (false, Some("io_failed".into()), Some("盘满了".into()), None);
                }
                if self.chunks.contains_key(&(key.clone(), seq)) {
                    return (
                        false,
                        Some("io_failed".into()),
                        Some("这一块已经在了".into()),
                        None,
                    );
                }
                let n = content.len();
                self.chunks.insert((key, seq), content);
                (true, None, None, Some(serde_json::json!({ "bytes": n })))
            }
            "files-commit-text" => {
                let key = args["key"].as_str().unwrap_or("").to_string();
                let (Some(chunks), Some(bytes)) = (args["chunks"].as_u64(), args["bytes"].as_u64())
                else {
                    return (
                        false,
                        Some("bad_args".into()),
                        Some("少了块数或字节数".into()),
                        None,
                    );
                };
                let mut whole = String::new();
                let mut missing = false;
                for seq in 0..chunks {
                    match self.chunks.get(&(key.clone(), seq)) {
                        Some(c) => whole.push_str(c),
                        None => missing = true,
                    }
                }
                self.chunks.retain(|(k, _), _| k != &key);
                if missing || whole.len() as u64 != bytes {
                    return (
                        false,
                        Some("io_failed".into()),
                        Some("块对不上".into()),
                        None,
                    );
                }
                let root = args["root"].as_str().unwrap_or("");
                let rel = args["rel"].as_str().unwrap_or("");
                if root.contains("refuse") {
                    return (
                        false,
                        Some("refused".into()),
                        Some("refuse write: 围栏".into()),
                        None,
                    );
                }
                let at = format!("{root}/{rel}");
                if let Err(e) = self.cas(&at, args) {
                    return e;
                }
                let sha = fake_sha256(&whole);
                self.disk.lock().unwrap().insert(at.clone(), whole.clone());
                *self.committed.lock().unwrap() = Some((at.clone(), whole));
                (
                    true,
                    None,
                    None,
                    Some(serde_json::json!({ "path": at, "bytes": bytes, "sha256": sha })),
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
            Some((_, idx)) => (
                false,
                idx.len(),
                idx.iter().map(|(p, _)| p.len()).sum::<usize>() + 5 * idx.len(),
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
            "unreadable_paths": if missing { Vec::new() } else { d.unreadable_paths.clone() },
            "truncated": if missing { false } else { d.truncated },
            "age_secs": if missing { 0 } else { d.age_secs },
            "rewalk_interval_secs": d.rewalk_interval_secs,
            "stale": stale,
            "browse_watches": d.browse_watches,
            "browse_watch_cap": d.browse_watch_cap,
            "cold_first_build_secs": d.cold_first_build_secs,
            "skipped_mounts": if missing { 0 } else { d.skipped_mounts },
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

/// 路径字节 → 线上那两种形（同 `src/backend/common/path_wire.rs::to_json` 的契约）。
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
/// # 🔴 为什么它列目录走 [`crate::source::list_local`]，而不是裸 `read_dir`
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
/// `to_string_lossy` 过的（同 [`crate::source::Row::lossy_name`]）
/// ⇒ 这棵合成树刻意**全 ASCII**，非 UTF-8 那一形由
/// `super::tests::a_non_utf8_hit_keeps_its_bytes_and_says_it_is_lossy`
/// 单独喂一份十六进制夹具去判，不靠这棵树。
fn walk(root: &std::path::Path) -> Vec<(Vec<u8>, bool)> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(rows) = crate::source::list_local(&dir) else {
            continue;
        };
        for r in rows {
            out.push((remote_form(&r.path).into_bytes(), r.is_dir));
            if r.is_dir {
                stack.push(std::path::PathBuf::from(&r.path));
            }
        }
    }
    out
}

/// 一台**挂在通道宿主上**的合成后端（从前挂在进程级登记表上）。
///
/// 回的那个 `WireLog` 是**它收到过的每一行请求**（按顺序）——
/// `§3.5.2a` 那条判据数的就是它里面的 `files-index-rebuild`。
pub struct Wired {
    pub origin: String,
    pub log: WireLog,
    /// 窗口手里那条线 —— 生产那个 `chan::client::Client`，拨的是真回环口。
    pub line: crate::source::Line,
}

/// 把 [`FakeBackend`] 挂成通道宿主的 `Backends`。
struct Hosted {
    be: std::sync::Mutex<FakeBackend>,
    log: WireLog,
}

impl comms_inward::chan::router::Backends for Hosted {
    fn call(
        &self,
        _origin: comms_inward::chan::wire::Origin,
        op: comms_inward::chan::wire::Op,
        payload: comms_inward::chan::wire::Body,
        _left: std::time::Duration,
        _cancel: comms_inward::chan::wire::CancelToken,
    ) -> futures::future::BoxFuture<
        'static,
        Result<comms_inward::chan::wire::Body, comms_inward::chan::wire::CallError>,
    > {
        use comms_inward::chan::wire::{Body, CallError, PeerFault};
        let args: serde_json::Value =
            serde_json::from_slice(&payload.0).unwrap_or(serde_json::Value::Null);
        let mut be = self.be.lock().unwrap();
        // 路径换名：参数里窗口那一形 → 盘上那一形（答出去的在下面换回来）。
        let swap = |v: &serde_json::Value, from: &str, to: &str| -> serde_json::Value {
            serde_json::from_str(
                &serde_json::to_string(v)
                    .unwrap_or_default()
                    .replace(from, to),
            )
            .unwrap_or(serde_json::Value::Null)
        };
        let alias = be.alias.clone();
        let args = match &alias {
            Some((shown, real)) => swap(&args, shown, real),
            None => args,
        };
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
        let (message, data) = match &alias {
            Some((shown, real)) => (
                message.map(|m| m.replace(real.as_str(), shown)),
                data.map(|d| swap(&d, real, shown)),
            ),
            None => (message, data),
        };
        let hold = if op.0 == "files-index-rebuild" {
            be.hold_rebuild.clone()
        } else if op.0 == "files-grep" {
            be.hold_grep.clone()
        } else {
            None
        };
        drop(be);
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
        Box::pin(async move {
            if let Some(g) = hold {
                g.notified().await;
            }
            r
        })
    }

    fn subscribe(
        &self,
        _origin: comms_inward::chan::wire::Origin,
        _kind: comms_inward::chan::wire::Kind,
        _from: Option<comms_inward::chan::wire::Cursor>,
    ) -> futures::stream::BoxStream<'static, comms_inward::chan::wire::Item> {
        Box::pin(futures::stream::iter([
            comms_inward::chan::wire::Item::Closed {
                by: comms_inward::chan::wire::By::Ours(comms_inward::chan::wire::OursFault::Misuse),
            },
        ]))
    }
}

/// 把一台合成后端挂到一个真通道口上，拨通，交回窗口要的那条线。**要在 tokio 运行时里调。**
pub async fn wire_up(origin: &str, be: FakeBackend) -> Wired {
    let log = be.log.clone();
    let hosted = std::sync::Arc::new(Hosted {
        be: std::sync::Mutex::new(be),
        log: log.clone(),
    });
    let h = crate::find::testing::start_host(
        hosted,
        crate::find::testing::test_key(),
        1 << 22,
        std::time::Duration::from_secs(5),
    )
    .await
    .expect("回环口绑得上");
    let line = comms_inward::chan::dial::dial(
        &h,
        comms_inward::chan::wire::Budget {
            until: std::time::Instant::now() + std::time::Duration::from_secs(5),
            cancel: comms_inward::chan::wire::CancelToken::new(),
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
/// ⚠从前这句话写的是「用 `Source::Remote` 而不是 `Source::Local`，
/// 后者的 origin 是 `inbound_client::LOCAL_ORIGIN`（一个全局键）」——
/// 本机侧退役之后**没有别的选择了**，但那条纪律本身照旧成立。
///
/// ⚠ 用 [`crate::shell::FileWindow::seeded`] 而不是 `new`：后者会
/// `reload()`，而远端那一支会去拨真 SFTP —— 本仓红线不许起真连接。
pub fn window_on(wired: &Wired, cwd: &str) -> crate::shell::FileWindow {
    let origin = wired.origin.as_str();
    let cfg = String::from(origin);
    let mut w = crate::shell::FileWindow::seeded(
        crate::source::Source::remote(cfg),
        cwd.to_string(),
        tokio::runtime::Handle::try_current().ok(),
        Vec::<crate::source::Row>::new(),
    );
    w.attach_line(wired.line.clone());
    w
}

/// 跑一帧**生产那个** [`crate::shell::FileWindow::frame_body`]，
/// 把这一帧真的交给文字排版的每一段文字读回来。
///
/// 量具本体（从 `FullOutput::shapes` 里抠 galley）复用
/// [`crate::copy::testing::text_in_frame`] —— 不抄第二份。
pub fn frame_text(
    ctx: &egui::Context,
    w: &mut crate::shell::FileWindow,
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
    let painted = crate::copy::testing::text_in_frame(&out);
    out.drop_without_applying_deltas();
    painted.into_iter().map(|(t, _)| t).collect()
}

/// 在搜索框里打一段字（合成事件：先把焦点交给它，再送文字）。
///
/// ⚠ egui 的 `Event::Text` 只送给有焦点的控件 ⇒ 先给焦点（与 Ctrl+F 同一个口），下一帧才送字。
pub fn type_into_search(ctx: &egui::Context, w: &mut crate::shell::FileWindow, text: &str) {
    // 搜索框住工具条上那一格（`toolbar_ui`）：画一帧带框的，把焦点交给它（同 Ctrl+F 那一下），再送字。
    let _ = crate::chrome::testing::frame(ctx, w, Vec::new());
    ctx.memory_mut(|m| m.request_focus(egui::Id::new(crate::shell::SEARCH_BOX_ID)));
    let _ = crate::chrome::testing::frame(ctx, w, Vec::new());
    let _ = crate::chrome::testing::frame(ctx, w, vec![egui::Event::Text(text.to_string())]);
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
pub async fn settle(board: &crate::find::SearchBoard, before: u64, who: &str) {
    for _ in 0..600 {
        if board.rounds() > before {
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    }
    panic!("{who}：等了 3 秒那块板子还是没有新答案（rounds 仍是 {before}）");
}

/// 合成后端的「摘要」：64 位小写十六进制、内容不同就不同（判据只要这两条；窗口把它当不透明令牌）。
/// 不是 SHA-256 —— 窗口从不自己算，算法对不对由后端那一侧的判据对拍 `sha2`。
pub fn fake_sha256(text: &str) -> String {
    use std::hash::{Hash, Hasher};
    (0..4u8)
        .map(|salt| {
            let mut h = std::collections::hash_map::DefaultHasher::new();
            (salt, text).hash(&mut h);
            format!("{:016x}", h.finish())
        })
        .collect()
}

/// 判据用的一把钥匙（形状同生产：两枚 v4 UUID 拼成的 64 位十六进制）。
pub fn test_key() -> comms_inward::chan::wire::Key {
    comms_inward::chan::wire::Key(format!(
        "{}{}",
        uuid::Uuid::new_v4().simple(),
        uuid::Uuid::new_v4().simple()
    ))
}

/// 判据用的通道口：绑回环 `127.0.0.1:0`，每条接进来的连接交给路由器，回交接件。
/// 生产里绑口的是 monitor 那一侧（壳里 `chan/host.rs::start_with`）；窗口这一侧的判据要自己起一个挂着合成句柄的口。
pub async fn start_host(
    backends: std::sync::Arc<dyn comms_inward::chan::router::Backends>,
    key: comms_inward::chan::wire::Key,
    frame: usize,
    hello_within: std::time::Duration,
) -> std::io::Result<comms_inward::chan::handoff::Handoff> {
    use comms_inward::chan::router;
    let listener = tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0)).await?;
    let addr = listener.local_addr()?;
    let terms = router::Terms {
        key: key.clone(),
        frame,
        hello_within,
    };
    tokio::spawn(async move {
        while let Ok((stream, _)) = listener.accept().await {
            let terms = terms.clone();
            let backends = std::sync::Arc::clone(&backends);
            tokio::spawn(async move {
                let _ = router::serve(stream, terms, backends).await;
            });
        }
    });
    Ok(comms_inward::chan::handoff::Handoff { addr, key, frame })
}
