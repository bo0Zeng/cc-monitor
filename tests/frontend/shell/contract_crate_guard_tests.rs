//! 共用包（`src/common/`）分四类，类名照它许谁链、许放什么（规矩 R3：判定只在核心算一次）：
//! - **契约**：两个核心与各出口都可链；只放线上类型与它的纯编解码 · 常量 · 按键取文案表的那一句 · 机制原语（只回系统原话与结构化结局）。
//!   copy-core 整包留契约：它带的格式化（大小 · 时长 · 个数 · 截字）出口也照它写，出口拿它格式化后端的数由出口零判定扫描管。
//! - **核心库**：只两个核心链（每台的后端 —— 桌面这一侧的拨号端核心也在本机后端里 · 手机嵌的核心 `core-rs`）；判定 · 汇总 · 写句子 · 格式化住这里。
//! - **判据原语**：只在 dev 侧。
//! - **前端宿主原语**：两个前端（壳 · 文件窗口）链、后端不链。
//!
//! 几道：
//! ① `src/common/` 下每个 crate 恰好登记一类、逐个写理由（两向：目录 == [`CRATES`]）；
//! ② 每个出口（[`OUTLETS`]：monitor · 文件窗口）生产段（`[dependencies]` · `[build-dependencies]` · `[target.*.dependencies]`）的 path 依赖闭包
//!    只许住 `src/common/` 且是契约或前端宿主原语；通信层的包只许 [`CLIENT_SIDE_COMMS`] 登过的那几个（它们本身不判类，它们的第一方依赖照样顺着闭包在这里判）。
//!    出口链着核心库的那几对照实登在 [`OUTLET_DEBT`]（写明由哪块删），与量到的那一组两向相等：新链一个要红，删了链接不删行也要红；
//! ③ crate 级：部署判定（[`DEPLOY_DECISIONS`]）只有一个家 —— 后端 `deploy` crate（`place_verdict` 在 `control/deploy_plan.rs`）恰一处定义；契约 crate `deploy-contract` 与
//!    monitor 生产源码里用着部署契约的每一份（壳 `src/**` ＋ 通信层面 A 那个 crate）零处定义。
//! ④「前端宿主原语」类（`host-core` · `win-manifest`）只许两个前端链：后端生产段闭包里出现即红。
//! ⑤「前端包」（[`FRONTEND_PACKAGES`]：文件窗口）不住 `src/common/`、自己出一个程序（它的 `[[bin]]` 住它自己名下，size2 10-11 从 monitor 包搬来）：
//!    monitor 生产段不链它、生产源码一处都不提它；它自己的闭包照 ② 判（「前端那一侧的生产闭包」= monitor ＋ 各前端包）；
//!    monitor 的源码人群声明（`[package.metadata.guard] population`）== 前端那一侧的生产闭包 − 后端生产闭包 ＋ 两头都链、归这一群的通信层客户端包
//!    （[`CLIENT_SIDE_COMMS`]；后端那一群不写它们）（两向）。
//! ⑥ 类之间的依赖方向（编译器挡不住）：每个共用包生产段的第一方依赖都住 `src/common/`，类落在 [`may_depend`] 许的范围里
//!    （契约 → 契约；核心库 → 契约 · 核心库；前端宿主原语 → 契约 · 前端宿主原语；判据原语不管）。
//!
//! 买不到：契约类 crate **里面**长出一个名字不在 [`DEPLOY_DECISIONS`] 里的判定（这条只按 crate 分类 ＋ 点名的判定符号，不读函数体；
//! 那是 `judgment-single-home.vitest.ts` 的 CORE_ITEMS）。

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Class {
    /// 两侧必须对上的形状与机制原语：两个核心与各出口都可链。
    Contract,
    /// 判定 · 汇总 · 写句子 · 格式化：只许两个核心链（后端 · 手机嵌的核心）；出口链着的照实登 [`OUTLET_DEBT`]。
    Core,
    /// 判据原语：两侧只在 dev 侧。
    TestInfra,
    /// 前端宿主原语：两个前端（monitor · 文件窗口）都要、只该有一份的宿主那几件；后端不许链。
    HostPrimitive,
}

/// `src/common/` 每个 crate 的类与理由（表「类」一列的判据版）。
const CRATES: &[(&str, Class, &str)] = &[
    ("acct-core", Class::Core, "账号能不能用 · 走哪一支（`auth_ready` · `apikey_routed_subset`）在这里判；后端链，界面那份 `AUTH_KINDS` 由测试档的生成器现生成（monitor 只在 dev 侧链它）"),
    ("cmd-wire", Class::Contract, "线上类型与它的纯编解码：机器名（`Machine`）· typed 命令的形状指纹表（后端 hello 交出去、客户端比）· 文件管理那一族的入参与回包 · 会话族手机问的那几条的回包 · b64 · fnv"),
    ("face-core", Class::Core, "照线上类型与结局 · 原因取文案表写成给人看的那一张（↗ 浮层 · 会话簿 · 活卡板的折法）：写句子 · 汇总在这里；后端与手机嵌的核心链，壳链着的照实登欠账"),
    ("chunk-core", Class::Core, "分块传那一圈带状态的循环：块多大 · 块号连着 · 整份摘要怎么算 · 读到哪为止；手机嵌的核心链，壳（附件 `attach_stage`）与文件窗口链着的照实登欠账"),
    ("batch-core", Class::Core, "一组一条命令：带票发 · 同名问一次 · 断了同票再问 · 重试与撤销计划（`retry_plan` · `undo_plan`）· 标题 · 结局句，判与写字都在这里；后端与手机嵌的核心链，文件窗口链着的照实登欠账"),
    ("pick-core", Class::Core, "选中几项 ⇒ 给哪几项、灰着说什么（`blocked` · `refused_here` · `why_not_editable` 回的都是句子）；手机嵌的核心链，文件窗口链着的照实登欠账"),
    ("copy-core", Class::Contract, "文案表：按同一个键取同一句（`copy_text` / `copy_static`），连带出口照它写的格式化（大小 · 时长 · 个数 · 截字）与通道那几句（said · reason · discard · detail）；各家照同一份取，判好的结局由调用方交进来"),
    ("creds-core", Class::Core, "凭据落盘格式与文件名 · 写半边（`harden`）· 家目录与数据目录那一族；后端链，壳与文件窗口链着的照实登欠账（家与数据目录挪走之后清）"),
    ("deploy-contract", Class::Contract, "部署那一族两侧对上的：表 A 的键与行 · 拒绝的形状与话 · 身份戳格式 · 计划答话；包里另有 `build_relation` · `build_order` · `key_from_uname` · 读身份戳那几样判定（壳 `local_backend_host` 还调 `build_relation`），挪进后端 `deploy` 之前照实记在这里"),
    ("filewin-contract", Class::Contract, "monitor 与文件窗口进程两边对上的形状：开窗种子（连同主窗所在显示器的工作区 `WorkArea`）· 就绪那一行 · 「在此打开终端」那一问的名字与参数 · 主题令牌名单"),
    ("test-home", Class::TestInfra, "测试档的家一开始就是沙箱：各 crate 只在 dev 侧引它，不判业务"),
    ("guard-core", Class::TestInfra, "源码扫描判据的原语，两侧都只在 dev 侧"),
    ("host-core", Class::HostPrimitive, "两个前端（monitor 主界面 · 文件窗口进程）共用的宿主那几件：自有状态文件的原子写 · 带上限整读一份文件 · 本机时区 · 编给的系统与架构 · 窗口夹进工作区 · 问一点所在那块屏；后端不链"),
    ("own-chan", Class::Contract, "机制原语：只有本人连得上的那一种系统通道（套接字住只给本人的目录 · 收连接核对端 uid · 目录独占锁），后端在听、monitor 去连；只回系统结局"),
    ("pidwatch-core", Class::Contract, "机制原语：等一个进程退出（Linux pidfd · Windows 进程句柄，内核叫醒），后端与 monitor 同一份；「进程退了没有」那三条判法是平台事实、本包判，兜底只记一条错；「还是不是那个进程」由调用方判"),
    ("platform-core", Class::Contract, "机制原语：目录的跨进程锁 · 只给本人的目录 · 自家状态文件的原子写 · `/proc/<pid>/stat` 怎么切 · Windows 进程表 · CREATE_NO_WINDOW，后端与 monitor（经 host-core 也给文件窗口）同一份；只回系统原话与结构化结局"),
    ("relay-route-core", Class::Contract, "门牌与家的布局：中转口与常驻后端套接字的门牌 · 路由语法 · 家目录底下各样东西住哪 · 后端落点 · 落盘声明表，两侧拼 / 拆同一份"),
    ("cap-core", Class::Contract, "机制原语：一道读上限是多少字节 · 超了怎么办（`Cap` · `Over`），读到流尽 · 读一个窗口 · 读一份本机文件只经这一份；只回读到的字节与「超了」，超了怎么办由调用方照 `over` 办"),
    ("shell-quote-core", Class::Contract, "机制原语：POSIX 单引号 quote 与标识符放行形状（session id · 启动令牌 · cc-bus id · 路径），两侧拼进 shell 前对上同一份"),
    ("win-path-core", Class::Contract, "机制原语：交给 Win32 文件 API 的那一形（UTF-16 ＋ 长路径前缀），后端 · monitor · 文件窗口 · creds-core 拼同一份"),
    ("md-core", Class::Core, "助手正文怎么解析成块树 · 代码高亮成哪一类 · 长正文在哪分段在这里判：后端链，两端只照 cmd-wire 的 `Doc` 排版"),
    ("win-manifest", Class::HostPrimitive, "两个前端包在 Windows 上带的进程清单（通用控件 v6）：清单本身 · 构建脚本里的编法 · 测试程序读回，只一份；后端不链"),
    ("plain-path-core", Class::Core, "一段相对路径是不是根底下的普通段（绝对 · `..` · `.` 怎么判）只在这里判：后端写面 · 远端写围栏（拨号核心）· 解压条目都调这一份；出口不链"),
    ("upstream-url-core", Class::Core, "上游 URL 能不能用在这里判；后端 · 中转 · 出网口链，界面不判（只画后端拒的那一句），monitor 只在 dev 侧为金样判据链它"),
];

/// ② 出口：(名字, 它的清单（仓根相对）)。每个出口自己出一个程序，各自的生产闭包分开量。
const OUTLETS: &[(&str, &str)] = &[
    ("monitor", "src/frontend/shell/Cargo.toml"),
    ("文件窗口", "src/frontend/filewin/Cargo.toml"),
];

/// ② 出口链着核心库的欠账 —— (出口, 核心库, 由哪块删)。与量到的那一组两向相等：新链一个要红，删了链接不删这一行也要红。
/// 表空了，整张连同 ② 里认它的那几行一起删。
const OUTLET_DEBT: &[(&str, &str, &str)] = &[
    (
        "monitor",
        "face-core",
        "W33（↗ 那一句由本机后端写）· W37（会话簿 · 活卡板照本机后端推的成品）两块都做完才删",
    ),
    (
        "monitor",
        "chunk-core",
        "W79（附件草稿整圈随上传路进核心，壳只交暂存文件的路径）",
    ),
    (
        "monitor",
        "creds-core",
        "W03（家目录 · 数据目录挪进 relay-route-core）",
    ),
    (
        "文件窗口",
        "batch-core",
        "G31 在 A24 下立的待排施工单（一组动作由后端跑、`files_batch` 帧带写好的成品）",
    ),
    (
        "文件窗口",
        "chunk-core",
        "G31 在 A24 下立的待排施工单（上传 · 编辑存大文本 · 预览拉取走后端那条传输路）",
    ),
    (
        "文件窗口",
        "pick-core",
        "W14（选中变了问本机后端要菜单成品）",
    ),
    (
        "文件窗口",
        "creds-core",
        "W03（家目录 · 数据目录挪进 relay-route-core）",
    ),
];

/// ⑤ 两头都链、归 monitor 那一群的通信层包 —— (包目录（仓根相对）, 理由)。两头都链的包只归一群、只登一份：
/// 这几包是通信层**客户端那一侧**，归 monitor；后端的源码人群（它 manifest 的 `population`，不顺着依赖扫）不写它们。
const CLIENT_SIDE_COMMS: &[(&str, &str)] = &[
    (
        "src/comms/inward",
        "通信层面 A（前端 ↔ 后端）的客户端零件：帧 · 路由器 · 问后端的分流；后端根包与 control · deploy · dial · history 的生产段也直接链它（有界异步读 `line::read_capped_*` · 通道那几句）；两头都链，只归 monitor 一群",
    ),
    (
        "src/comms/resident-client",
        "连常驻后端的客户端那一侧（握手 · 重连退避 · 期限，有定时器）：桌面 · 手机的真连接走它；后端根包 · control · dial 生产段也直接链（`control/feeds.rs` 给每台跑 `feed` · `dial/probe.rs` 测试连接那个功能 · `stream/inbound` 认 `WATCH_CMD`）；两头都链，只归 monitor 一群",
    ),
];

/// ⑥ 一个共用包（类 `from`）的生产段许不许链类 `to` 的共用包。判据原语不管（只在 dev 侧）。
fn may_depend(from: Class, to: Class) -> bool {
    match from {
        Class::Contract => to == Class::Contract,
        Class::Core => matches!(to, Class::Contract | Class::Core),
        Class::HostPrimitive => matches!(to, Class::Contract | Class::HostPrimitive),
        Class::TestInfra => true,
    }
}

/// ⑤前端包：不住 `src/common/`、自己出一个程序的包 —— (包目录（仓根相对）, 那个 bin 的 crate 根, 理由)。
const FRONTEND_PACKAGES: &[(&str, &str, &str)] = &[(
    "src/frontend/filewin",
    "src/frontend/filewin/src/main.rs",
    "文件窗口（又一个前端）：它自己的 `[[bin]] cc-monitor-filewin` 一行转调库面；只经 monitor 内嵌那份出货（不进安装包，入包量具 ⑭a）；monitor 一行都不引它",
)];

/// 部署判定（原 `deploy-core` 的判定那一半）：各自只许在后端那一个家恰一处定义 —— (名字, 家)。
/// 判定与计划住 `deploy` crate（桌面后端与手机 mobcore 同一份）；`place_verdict` 要这份字节自己的 `BUILD_ID`，留在后端的帧面那一层。
const DEPLOY_DECISIONS: &[(&str, &str)] = &[
    ("judge", DEPLOY_HOME),
    ("promised", DEPLOY_HOME),
    ("identity_decision", DEPLOY_HOME),
    ("interpret_target_probe", DEPLOY_HOME),
    ("place_verdict", "src/backend/control/deploy_plan.rs"),
];

/// 部署判定的家（仓内相对路径）。
const DEPLOY_HOME: &str = "src/backend/deploy/lib.rs";

fn root() -> PathBuf {
    guard_core::addr::repo_root()
}

fn class_of(name: &str) -> Option<Class> {
    CRATES
        .iter()
        .find(|(n, _, _)| *n == name)
        .map(|(_, c, _)| *c)
}

/// 生产段：`[dependencies]` · `[build-dependencies]` · `[target.<cfg>.dependencies]` · `[target.<cfg>.build-dependencies]`。
fn is_production_section(header: &str) -> bool {
    let h = header
        .trim()
        .trim_start_matches('[')
        .trim_end_matches(']')
        .trim();
    let last = h.rsplit('.').next().unwrap_or(h);
    (h == "dependencies" || h == "build-dependencies" || h.starts_with("target."))
        && (last == "dependencies" || last == "build-dependencies")
}

/// 一份 manifest 生产段里的 path 依赖 → (依赖名, 解析后的目录)。
fn production_path_deps(manifest: &Path) -> Vec<(String, PathBuf)> {
    let raw = std::fs::read_to_string(manifest)
        .unwrap_or_else(|e| panic!("读 {}: {e}", manifest.display()));
    let text = guard_core::strip_hash_comment_lines(&raw);
    let dir = manifest.parent().expect("manifest 有父目录");
    let mut section = String::new();
    let mut out = Vec::new();
    for line in text.lines() {
        let t = line.trim();
        if t.starts_with('[') {
            section = t.to_string();
            continue;
        }
        if !is_production_section(&section) {
            continue;
        }
        for p in guard_core::inline_table_paths(line) {
            let name = t.split('=').next().unwrap_or_default().trim().to_string();
            out.push((name, normalize(&dir.join(p))));
        }
    }
    out
}

fn normalize(p: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for c in p.components() {
        match c {
            std::path::Component::ParentDir => {
                out.pop();
            }
            std::path::Component::CurDir => {}
            other => out.push(other),
        }
    }
    out
}

fn common_dir() -> PathBuf {
    root().join("src/common")
}

/// 从几份清单出发的生产段 path 依赖闭包（闭包的遍历只这一份）：(谁带进来的, 依赖名, 目录)。
fn production_closure(starts: Vec<(String, PathBuf)>) -> Vec<(String, String, PathBuf)> {
    let mut out = Vec::new();
    let mut seen: BTreeSet<PathBuf> = starts
        .iter()
        .filter_map(|(_, m)| m.parent().map(Path::to_path_buf))
        .collect();
    let mut todo = starts;
    while let Some((who, manifest)) = todo.pop() {
        for (name, dir) in production_path_deps(&manifest) {
            out.push((who.clone(), name.clone(), dir.clone()));
            if seen.insert(dir.clone()) {
                todo.push((name, dir.join("Cargo.toml")));
            }
        }
    }
    out
}

/// monitor 生产段依赖闭包里每个 path 依赖：(谁带进来的, 依赖名, 目录)。
fn monitor_production_closure() -> Vec<(String, String, PathBuf)> {
    let mut starts = vec![(
        "monitor".to_string(),
        root().join("src/frontend/shell/Cargo.toml"),
    )];
    let mut out = Vec::new();
    // 前端那一侧的程序不止 monitor 一个：各前端包自己出程序（monitor 不链它们），它们与自己的生产闭包一起算进来。
    for (at, _, _) in FRONTEND_PACKAGES {
        let dir = root().join(at);
        out.push(("前端".to_string(), (*at).to_string(), dir.clone()));
        starts.push(((*at).to_string(), dir.join("Cargo.toml")));
    }
    out.extend(production_closure(starts));
    out
}

/// monitor 生产源码：壳 `src/**/*.rs` ＋ 通信层面 A 那个 crate（`src/comms/inward/`）＋ 其中生产段经 `#[path = "…"]` 收进来、不在 `tests/` 下的文件。→ (仓内相对路径, 生产段正文)。
fn monitor_production_sources() -> Vec<(String, String)> {
    let root = root();
    let mut files: BTreeMap<PathBuf, String> = [
        root.join("src/frontend/shell/src"),
        root.join("src/comms/inward"),
    ]
    .iter()
    .flat_map(|dir| guard_core::scan_tree_excluding(dir, &["rs"], &[]))
    .map(|(p, raw)| (normalize(&p), guard_core::production_code(&raw)))
    .collect();
    let mut todo: Vec<PathBuf> = files.keys().cloned().collect();
    while let Some(p) = todo.pop() {
        let body = files[&p].clone();
        for rest in body.split("#[path = \"").skip(1) {
            let rel = rest.split('"').next().unwrap_or_default();
            let target = normalize(&p.parent().expect("文件有父目录").join(rel));
            if target.starts_with(root.join("tests")) || files.contains_key(&target) {
                continue;
            }
            let raw = std::fs::read_to_string(&target)
                .unwrap_or_else(|e| panic!("`#[path]` 指向 {} 读不出：{e}", target.display()));
            files.insert(target.clone(), guard_core::production_code(&raw));
            todo.push(target);
        }
    }
    files
        .into_iter()
        .map(|(p, body)| {
            let rel = p
                .strip_prefix(&root)
                .unwrap_or(&p)
                .to_string_lossy()
                .replace('\\', "/");
            (rel, body)
        })
        .collect()
}

/// 一段生产正文里 `fn <名>` 的定义处数（名字两侧不连着标识符字符）。
fn fn_defs(body: &str, name: &str) -> usize {
    let needle = format!("fn {name}");
    body.match_indices(&needle)
        .filter(|(at, _)| {
            let before_ok = *at == 0 || {
                let b = body.as_bytes()[at - 1];
                !b.is_ascii_alphanumeric() && b != b'_'
            };
            let after = body[at + needle.len()..].chars().next();
            before_ok && !after.is_some_and(|c| c.is_alphanumeric() || c == '_')
        })
        .count()
}

#[test]
fn every_common_crate_has_exactly_one_class_with_a_reason() {
    // 遍历口径只有一份（`guard_core`）：`src/common/<crate>/Cargo.toml` 那一层。
    let common = common_dir();
    let on_disk: BTreeSet<String> = guard_core::scan_tree_excluding(&common, &["toml"], &[])
        .into_iter()
        .filter_map(|(p, _)| {
            let rel = p
                .strip_prefix(&common)
                .ok()?
                .to_string_lossy()
                .replace('\\', "/");
            rel.strip_suffix("/Cargo.toml")
                .filter(|c| !c.contains('/'))
                .map(str::to_string)
        })
        .collect();
    let listed: BTreeSet<String> = CRATES.iter().map(|(n, _, _)| n.to_string()).collect();
    assert_eq!(listed.len(), CRATES.len(), "登记表里有重名");
    assert_eq!(
        on_disk, listed,
        "`src/common/` 与登记表两向不等 —— 新共享 crate 先想清楚它是契约还是判定（判定的家在后端），再登记"
    );
}

/// ② 一个出口的生产闭包里：不许出现的那几处（非核心库的越界）与链着的核心库（出口, 包）。
fn outlet_links(
    outlet: &str,
    closure: &[(String, String, PathBuf)],
) -> (Vec<String>, BTreeSet<(String, String)>) {
    let common = common_dir();
    let client_side: BTreeSet<PathBuf> = CLIENT_SIDE_COMMS
        .iter()
        .map(|(dir, _)| normalize(&root().join(dir)))
        .collect();
    let mut bad = Vec::new();
    let mut core = BTreeSet::new();
    for (who, name, dir) in closure {
        let Ok(rel) = dir.strip_prefix(&common) else {
            // 通信层：只放行登过的客户端那一侧的包；它们本身不判类，它们的第一方依赖照样顺着闭包在这里判。
            if client_side.contains(dir) {
                continue;
            }
            bad.push(format!(
                "{outlet}：{who} → {name}：path 依赖不在 `src/common/` 下、也不是登过的通信层客户端包（{}）",
                dir.display()
            ));
            continue;
        };
        let crate_dir = rel.to_string_lossy().replace('\\', "/");
        match class_of(&crate_dir) {
            Some(Class::Contract | Class::HostPrimitive) => {}
            Some(Class::Core) => {
                core.insert((outlet.to_string(), crate_dir));
            }
            Some(c) => bad.push(format!(
                "{outlet}：{who} → {crate_dir}：{c:?} 类，出口生产段不许链它"
            )),
            None => bad.push(format!("{outlet}：{who} → {crate_dir}：没登记类")),
        }
    }
    (bad, core)
}

/// ② 每个出口的生产闭包只许契约 · 前端宿主原语 · 登过的通信层客户端包；链着的核心库与 [`OUTLET_DEBT`] 两向相等。
#[test]
fn outlets_link_only_contract_crates_and_their_core_links_are_the_debt_table() {
    let mut bad = Vec::new();
    let mut linked: BTreeSet<(String, String)> = BTreeSet::new();
    for (outlet, manifest) in OUTLETS {
        let closure = production_closure(vec![(outlet.to_string(), root().join(manifest))]);
        assert!(
            closure.iter().any(|(_, n, _)| n == "copy-core"),
            "{outlet} 生产段一个 path 依赖都没量到（`copy-core` 明明在）—— 量法瞎了"
        );
        let (b, c) = outlet_links(outlet, &closure);
        bad.extend(b);
        linked.extend(c);
    }
    assert!(
        bad.is_empty(),
        "出口生产段链了不许链的包：\n{}",
        bad.join("\n")
    );
    let debt: BTreeSet<(String, String)> = OUTLET_DEBT
        .iter()
        .map(|(o, c, _)| (o.to_string(), c.to_string()))
        .collect();
    assert_eq!(debt.len(), OUTLET_DEBT.len(), "`OUTLET_DEBT` 里有重行");
    let new: Vec<_> = linked.difference(&debt).collect();
    let stale: Vec<_> = debt.difference(&linked).collect();
    assert!(
        new.is_empty() && stale.is_empty(),
        "出口链核心库与 `OUTLET_DEBT` 两向不等 —— 新链上的（判定 · 写句子归核心，出口只收成品）：{new:?}；链接删了、行还在（删掉那一行）：{stale:?}"
    );
    // 正控：一个出口闭包里的核心库认得出，登过的通信层客户端包放行、没登的通信层包红。
    let probe = vec![
        (
            "p".to_string(),
            "face-core".to_string(),
            common_dir().join("face-core"),
        ),
        (
            "p".to_string(),
            "comms-inward".to_string(),
            root().join("src/comms/inward"),
        ),
        (
            "p".to_string(),
            "ssh-dial".to_string(),
            root().join("src/comms/ssh-dial"),
        ),
    ];
    let (b, c) = outlet_links("p", &probe);
    assert_eq!(c.len(), 1, "量法认不出出口链着的核心库");
    assert_eq!(
        b.len(),
        1,
        "量法放过了没登的通信层包，或拦了登过的那个：{b:?}"
    );
}

/// ⑥ 每个共用包生产段的第一方依赖都住 `src/common/`，类落在 [`may_depend`] 许的范围里。
#[test]
fn common_crates_depend_only_in_the_allowed_class_direction() {
    let common = common_dir();
    let mut bad = Vec::new();
    let mut seen_edges = 0;
    for (name, class, _) in CRATES {
        if *class == Class::TestInfra {
            continue;
        }
        for (dep, dir) in production_path_deps(&common.join(name).join("Cargo.toml")) {
            seen_edges += 1;
            let Ok(rel) = dir.strip_prefix(&common) else {
                bad.push(format!(
                    "{name} → {dep}：第一方依赖不住 `src/common/`（{}）",
                    dir.display()
                ));
                continue;
            };
            let to = rel.to_string_lossy().replace('\\', "/");
            match class_of(&to) {
                Some(t) if may_depend(*class, t) => {}
                Some(t) => bad.push(format!(
                    "{name}（{class:?}）→ {to}（{t:?}）：类之间不许这样依赖"
                )),
                None => bad.push(format!("{name} → {to}：没登记类")),
            }
        }
    }
    assert!(seen_edges > 0, "共用包之间一条依赖都没量到 —— 量法瞎了");
    // 正控：契约链核心库 · 契约链前端宿主原语都不许。
    assert!(!may_depend(Class::Contract, Class::Core));
    assert!(!may_depend(Class::Contract, Class::HostPrimitive));
    assert!(
        bad.is_empty(),
        "共用包的类间依赖方向不对：\n{}",
        bad.join("\n")
    );
}

/// ④ 后端生产段（`src/backend/Cargo.toml` 的生产段 path 依赖闭包）不链前端宿主原语。
#[test]
fn the_backend_links_no_host_primitive_crate() {
    let common = common_dir();
    let linked: Vec<(String, Option<Class>)> = backend_closure()
        .into_iter()
        .filter_map(|(_, name, dir)| {
            let rel = dir.strip_prefix(&common).ok()?;
            Some((name, class_of(&rel.to_string_lossy().replace('\\', "/"))))
        })
        .collect();
    assert!(
        linked.iter().any(|(n, _)| n == "copy-core"),
        "后端生产段一个共享 crate 都没量到（`copy-core` 明明在）—— 量法瞎了"
    );
    let bad: Vec<&String> = linked
        .iter()
        .filter(|(_, c)| *c == Some(Class::HostPrimitive))
        .map(|(n, _)| n)
        .collect();
    assert!(
        bad.is_empty(),
        "后端生产段链了前端宿主原语 {bad:?} —— 那一类只许两个前端用（窗口几何 · 前端自有状态），后端有自己的 `platform/`"
    );
}

#[test]
fn deploy_decisions_live_only_in_the_backend() {
    let root = root();
    let mut bad = Vec::new();
    for (name, home) in DEPLOY_DECISIONS {
        let body = guard_core::production_code(
            &std::fs::read_to_string(root.join(home)).unwrap_or_else(|e| panic!("读 {home}：{e}")),
        );
        let n = fn_defs(&body, name);
        if n != 1 {
            bad.push(format!("`{name}` 在 {home} 定义 {n} 处（该恰一处）"));
        }
    }
    let sources = monitor_production_sources();
    assert!(
        sources
            .iter()
            .any(|(r, _)| r == "src/comms/inward/backend_route.rs"),
        "monitor 链的通信层 crate 没进扫描面 —— 量法瞎了"
    );
    // 人群：契约 crate 自己 ＋ monitor 生产源码里用着部署契约的那几份（判定名很泛 —— `judge` 在文件窗口 · 凭据权限里另有同名的别的判定，
    //   按「用不用部署契约」取人群，不按名字猜）。
    let contract: Vec<(String, String)> =
        guard_core::scan_tree_excluding(&common_dir().join("deploy-contract"), &["rs"], &[])
            .into_iter()
            .map(|(p, raw)| {
                (
                    p.to_string_lossy().replace('\\', "/"),
                    guard_core::production_code(&raw),
                )
            })
            .collect();
    assert!(
        contract.iter().any(|(_, b)| fn_defs(b, "key_of") == 1),
        "契约 crate 里没扫到 `key_of` —— 量法瞎了"
    );
    let users: Vec<&(String, String)> = sources
        .iter()
        .filter(|(_, b)| b.contains("deploy_contract::"))
        .collect();
    assert!(
        users
            .iter()
            .any(|(r, _)| r == "src/frontend/shell/src/byte_table.rs"),
        "monitor 里用部署契约的人群没扫到 `src/frontend/shell/src/byte_table.rs` —— 量法瞎了"
    );
    for (rel, body) in contract.iter().chain(users) {
        for (name, home) in DEPLOY_DECISIONS {
            if fn_defs(body, name) > 0 {
                bad.push(format!(
                    "{rel}：定义了部署判定 `{name}`（判定只在后端 {home}）"
                ));
            }
        }
    }
    // 正控：量法认得出一个定义、认不出前后连着字的名字。
    assert_eq!(fn_defs("pub fn judge(x: u8) {}", "judge"), 1);
    assert_eq!(
        fn_defs("fn judge_path_ccm() {} fn misjudge() {}", "judge"),
        0
    );
    assert!(bad.is_empty(), "{}", bad.join("\n"));
}

/// 后端生产段的 path 依赖闭包（包目录）。
fn backend_production_closure() -> BTreeSet<PathBuf> {
    backend_closure()
        .into_iter()
        .map(|(_, _, dir)| dir)
        .collect()
}

/// 后端生产段的 path 依赖闭包：(谁带进来的, 依赖名, 目录)。
fn backend_closure() -> Vec<(String, String, PathBuf)> {
    production_closure(vec![(
        "backend".to_string(),
        root().join("src/backend/Cargo.toml"),
    )])
}

/// ⑤ 前端包自己出程序、monitor 不链它：monitor 的清单生产段不点名它，monitor 生产源码（壳 `src/` ＋ 通信层面 A 那个 crate）一处都不提它的 crate 名；
/// 它那个 `[[bin]]` 的 crate 根真的转调它自己的库面。
#[test]
fn a_frontend_package_ships_its_own_bin_and_the_monitor_does_not_link_it() {
    let sources = monitor_production_sources();
    let monitor_deps = production_path_deps(&root().join("src/frontend/shell/Cargo.toml"));
    for (at, bin_root, _) in FRONTEND_PACKAGES {
        let manifest = std::fs::read_to_string(root().join(at).join("Cargo.toml"))
            .unwrap_or_else(|e| panic!("读 {at}/Cargo.toml：{e}"));
        let name = manifest
            .lines()
            .find_map(|l| l.trim().strip_prefix("name = \""))
            .and_then(|r| r.split('"').next())
            .unwrap_or_else(|| panic!("{at}/Cargo.toml 没有包名"))
            .replace('-', "_");
        let linked: Vec<&String> = monitor_deps
            .iter()
            .filter(|(_, dir)| *dir == root().join(at))
            .map(|(n, _)| n)
            .collect();
        assert!(
            linked.is_empty(),
            "monitor 的生产段又链了前端包 `{at}`（{linked:?}）—— 它自己出程序，monitor 不链它"
        );
        let mentioned: BTreeSet<&str> = sources
            .iter()
            .filter(|(rel, _)| {
                // 按整段路径名比（`src/comms-x/` 不算 `src/comms/`）。
                let parts: Vec<&str> = rel.split('/').collect();
                matches!(
                    parts.as_slice(),
                    ["src", "frontend", "shell", _, ..] | ["src", "comms", _, ..]
                )
            })
            .filter(|(_, body)| guard_core::contains_word(body, &name))
            .map(|(rel, _)| rel.as_str())
            .collect();
        assert!(
            mentioned.is_empty(),
            "monitor 生产源码里提到了 `{name}`：{mentioned:?} —— 两个前端又缠回一个 crate"
        );
        let entry = std::fs::read_to_string(root().join(bin_root))
            .unwrap_or_else(|e| panic!("读 {bin_root}：{e}"));
        assert!(
            guard_core::contains_word(&guard_core::production_code(&entry), &name),
            "`{bin_root}` 不转调 `{name}` 的库面 —— 登记的 bin 根不是那个程序的入口"
        );
    }
}

/// ⑤ monitor 的源码人群声明（`guard_core::population_trees`：判据按它收兄弟源码树）== 前端那一侧的生产闭包（monitor ＋ 各前端包）− 后端生产闭包
/// ＋ [`CLIENT_SIDE_COMMS`]（两头都链、归这一群的通信层客户端包）。
/// 少写一包 ⇒ 按人群扫的判据静默少扫搬进去的那几份；多写一包（后端也链的契约）⇒ 人群被别家的代码撑大。
/// 两头都链的包只归一群：登记的那几包两头真都链（不是了就摘：那时它按「只有前端链」本来就在）、后端那一群的声明里没有它们。
#[test]
fn the_monitor_population_is_exactly_its_frontend_only_closure() {
    let backend_declared: BTreeSet<PathBuf> = {
        let manifest = root().join("src/backend/Cargo.toml");
        let text = std::fs::read_to_string(&manifest).expect("读后端 manifest");
        guard_core::population_decl(&text, &manifest)
            .iter()
            .map(|rel| normalize(&root().join("src/backend").join(rel)))
            .collect()
    };
    let shared_client: BTreeSet<PathBuf> = CLIENT_SIDE_COMMS
        .iter()
        .map(|(dir, _)| normalize(&root().join(dir)))
        .collect();
    let monitor_links: BTreeSet<PathBuf> = monitor_production_closure()
        .into_iter()
        .map(|(_, _, dir)| dir)
        .collect();
    let backend_links = backend_production_closure();
    for d in &shared_client {
        assert!(
            monitor_links.contains(d) && backend_links.contains(d),
            "`CLIENT_SIDE_COMMS` 登的 {d:?} 不再两头都链 —— 摘掉那一行（只有前端链的包按差集本来就在人群里）"
        );
        assert!(
            !backend_declared.contains(d),
            "{d:?} 既归 monitor 那一群、又写进了后端那一群（`src/backend/Cargo.toml` 的 population）—— 两头都链的包只归一群"
        );
    }
    let declared: BTreeSet<PathBuf> =
        guard_core::population_trees(&root().join("src/frontend/shell/src"))
            .into_iter()
            // 人群树是 `<包>/src`，或 crate 根与清单同层的包目录本身（通信层那两个）。
            .map(|(_, src)| {
                normalize(if src.ends_with("src") {
                    src.parent().expect("人群树是 <包>/src")
                } else {
                    &src
                })
            })
            .collect();
    let backend = backend_production_closure();
    assert!(
        backend.iter().any(|d| d.ends_with("copy-core")),
        "后端生产闭包一个共享 crate 都没量到（`copy-core` 明明在）—— 量法瞎了"
    );
    let frontend_only: BTreeSet<PathBuf> = monitor_links
        .iter()
        .filter(|d| !backend.contains(*d))
        .cloned()
        .collect();
    assert!(
        !frontend_only.is_empty(),
        "monitor 生产闭包减掉后端那一份之后是空集 —— 量法瞎了"
    );
    let want: BTreeSet<PathBuf> = frontend_only.union(&shared_client).cloned().collect();
    assert_eq!(
        declared, want,
        "monitor 的源码人群声明（`src/frontend/shell/Cargo.toml` 的 `[package.metadata.guard] population`）与「只有前端链的包 ＋ 归这一群的通信层客户端包」两向不等"
    );
}
