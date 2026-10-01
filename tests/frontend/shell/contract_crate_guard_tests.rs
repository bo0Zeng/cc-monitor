//! 要求：「共享 crate 只放两边必须对上的契约（路径 · 端口 · 文件格式 · 文案表 · 令牌形状 · 字节表键），不放判定；判定只在后端」＋ THIN 第 5 件「monitor 生产段只许依赖契约类 crate」
//! ＋「`contract_crate_guard_tests.rs` ③ 由『按符号』改成 crate 级」。
//!
//! 三道：
//! ① `src/common/` 下每个 crate 恰好登记一类、逐个写理由（两向：目录 == [`CRATES`]）；
//! ② monitor 生产段（`[dependencies]` · `[build-dependencies]` · `[target.*.dependencies]`）点名的每个 path 依赖、连同它们自己生产段的
//!    path 依赖闭包，只许住 `src/common/` 且是契约类；判定类出现即红，**没有例外**（`deploy-core` 拆成契约 `deploy-contract` ＋ 后端判定之后，按符号那道豁免退役）；
//! ③ crate 级：部署判定（[`DEPLOY_DECISIONS`]）只有一个家 —— 后端 `control/deploy_plan.rs` 恰一处定义；契约 crate `deploy-contract` 与
//!    monitor 生产源码里用着部署契约的每一份（壳 `src/**` ＋ 它经 `#[path]` 收进来的通信层文件）零处定义。
//! ④「前端宿主原语」类（`host-core`）只许两个前端链：后端生产段闭包里出现即红。
//! ⑤〔主会话 09-29 拍板 Q1〕「前端包」（[`FRONTEND_PACKAGES`]：文件窗口）不住 `src/common/`：monitor 链它**只为**那个 `[[bin]]`
//!    （生产源码里提到它的恰好是 `filewin/win_main.rs` 一份），它自己的闭包照 ② 判；monitor 的源码人群声明
//!    （`[package.metadata.guard] population`）== monitor 生产闭包 − 后端生产闭包（两向）。
//!
//! 买不到：契约类 crate **里面**长出一个名字不在 [`DEPLOY_DECISIONS`] 里的判定（这条只按 crate 分类 ＋ 点名的判定符号，不读函数体；
//! 那是 `judgment-single-home.vitest.ts` 的 CORE_ITEMS）。

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Class {
    /// 两侧必须对上的形状：monitor 生产段可以依赖。
    Contract,
    /// 裁决：只许后端用；monitor 生产段依赖即红。
    Decision,
    /// 判据原语：两侧只在 dev 侧。
    TestInfra,
    /// 〔主会话 09-29 拍板 Q2〕前端宿主原语：两个前端（monitor · 文件窗口）都要、只该有一份的宿主那几件；后端不许链。
    HostPrimitive,
}

/// `src/common/` 每个 crate 的类与理由（表「类」一列的判据版）。
const CRATES: &[(&str, Class, &str)] = &[
    ("acct-core", Class::Decision, "账号能不能用 · 走哪一支（`auth_ready` · `apikey_routed_subset`）是裁决；界面那份 `AUTH_KINDS` 由测试档的生成器现生成（monitor 只在 dev 侧链它）"),
    ("chan-core", Class::Contract, "通道（通信层面 A）的线上词汇与帧格式，两端必须对上；外加搬字节的客户端 / 路由器 / 拨号（零业务判断，`05` C1–C5）"),
    ("copy-core", Class::Contract, "文案表：两侧按同一个键取同一句（`copy_text` / `copy_static`），不裁决"),
    ("creds-core", Class::Contract, "文件格式 · 路径：凭据落盘格式与文件名、数据目录（monitor 只算路径；写半边在 `harden` 后面，monitor 不开）"),
    ("deploy-contract", Class::Contract, "字节表键 · 文件格式 · 答话形状 · 路径：表 A 的键与行、拒绝的形状与话、身份戳格式、计划答话、落点路径，两侧对上同一份（判定那一半住后端 `control/deploy_plan.rs`）"),
    ("filewin-contract", Class::Contract, "monitor 与文件窗口进程两边对上的形状：开窗种子 · 就绪那一行 · 「在此打开终端」那一问的名字与参数，不裁决"),
    ("guard-core", Class::TestInfra, "源码扫描判据的原语，两侧都只在 dev 侧"),
    ("host-core", Class::HostPrimitive, "两个前端（monitor 主界面 · 文件窗口进程）共用的宿主那几件：自有状态文件的原子写 · 窗口夹进工作区；不裁决业务，后端不链"),
    ("relay-route-core", Class::Contract, "端口 · 路径 · 路由语法：中转与常驻监听口的门牌、后端落点，两侧拼 / 拆同一份"),
    ("shell-quote-core", Class::Contract, "令牌形状：POSIX 单引号 quote 与标识符放行形状（session id · 启动令牌 · cc-bus id · 路径），两侧拼进 shell 前对上同一份"),
    ("upstream-url-core", Class::Decision, "上游 URL 能不能用是裁决；界面读的是生成器现生成的式子（monitor 只在 dev 侧链它）"),
];

/// ⑤前端包：不住 `src/common/`、monitor 生产段只为某个 `[[bin]]` 链它的包 —— (包目录（仓根相对）, 那个 bin 的 crate 根, 理由)。
const FRONTEND_PACKAGES: &[(&str, &str, &str)] = &[(
    "src/frontend/filewin",
    "src/frontend/shell/src/filewin/win_main.rs",
    "文件窗口（又一个前端）：包里第二个 `[[bin]] cc-monitor-filewin` 一行转调它，打包路线不变（K-R124 ⑭）；monitor 库面一行都不引它",
)];

/// 部署判定（原 `deploy-core` 的判定那一半）：只许在后端 [`DEPLOY_HOME`] 恰一处定义。
const DEPLOY_DECISIONS: &[&str] = &[
    "judge",
    "promised",
    "is_newer",
    "identity_decision",
    "landing_verdict",
    "legacy_verdict",
    "is_ours",
    "interpret_target_probe",
    "place_verdict",
];

/// 部署判定的唯一住址（仓内相对路径）。
const DEPLOY_HOME: &str = "src/backend/control/deploy_plan.rs";

fn root() -> PathBuf {
    crate::guard_support::repo_root()
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

/// monitor 生产段依赖闭包里每个 path 依赖：(谁带进来的, 依赖名, 目录)。
fn monitor_production_closure() -> Vec<(String, String, PathBuf)> {
    let mut out = Vec::new();
    let mut seen = BTreeSet::new();
    let mut todo = vec![(
        "monitor".to_string(),
        root().join("src/frontend/shell/Cargo.toml"),
    )];
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

/// monitor 生产源码：壳 `src/**/*.rs` ＋ 其中生产段经 `#[path = "…"]` 收进来、不在 `tests/` 下的文件。→ (仓内相对路径, 生产段正文)。
fn monitor_production_sources() -> Vec<(String, String)> {
    let root = root();
    let mut files: BTreeMap<PathBuf, String> =
        guard_core::scan_tree_excluding(&root.join("src/frontend/shell/src"), &["rs"], &[])
            .into_iter()
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
    for (n, _, why) in CRATES {
        assert!(why.chars().count() >= 12, "`{n}` 的理由写得太短：{why:?}");
    }
}

#[test]
fn the_monitor_production_segment_links_only_contract_crates() {
    let closure = monitor_production_closure();
    assert!(
        closure.iter().any(|(_, n, _)| n == "copy-core"),
        "monitor 生产段一个 path 依赖都没量到（`copy-core` 明明在）—— 量法瞎了"
    );
    let common = common_dir();
    let mut bad = Vec::new();
    for (who, name, dir) in &closure {
        let Ok(rel) = dir.strip_prefix(&common) else {
            // ⑤ 前端包：它自己的生产依赖也在这张闭包里，照同一条规矩判。
            if FRONTEND_PACKAGES
                .iter()
                .any(|(at, _, _)| *dir == root().join(at))
            {
                continue;
            }
            bad.push(format!(
                "{who} → {name}：path 依赖不在 `src/common/` 下（{}）",
                dir.display()
            ));
            continue;
        };
        let crate_dir = rel.to_string_lossy().replace('\\', "/");
        match class_of(&crate_dir) {
            Some(Class::Contract | Class::HostPrimitive) => {}
            Some(c) => bad.push(format!(
                "{who} → {crate_dir}：{c:?} 类，monitor 生产段不许链它（判定的家在后端）"
            )),
            None => bad.push(format!("{who} → {crate_dir}：没登记类")),
        }
    }
    assert!(
        bad.is_empty(),
        "monitor 生产段依赖了非契约类 crate：\n{}",
        bad.join("\n")
    );
}

/// ④ 后端生产段（`src/backend/Cargo.toml` 的生产段 path 依赖闭包）不链前端宿主原语。
#[test]
fn the_backend_links_no_host_primitive_crate() {
    let mut todo = vec![root().join("src/backend/Cargo.toml")];
    let mut seen = BTreeSet::new();
    let common = common_dir();
    let mut linked = Vec::new();
    while let Some(manifest) = todo.pop() {
        for (name, dir) in production_path_deps(&manifest) {
            if let Ok(rel) = dir.strip_prefix(&common) {
                linked.push((
                    name.clone(),
                    class_of(&rel.to_string_lossy().replace('\\', "/")),
                ));
            }
            if seen.insert(dir.clone()) {
                todo.push(dir.join("Cargo.toml"));
            }
        }
    }
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
    let home = guard_core::production_code(
        &std::fs::read_to_string(root.join(DEPLOY_HOME))
            .unwrap_or_else(|e| panic!("读 {DEPLOY_HOME}：{e}")),
    );
    for name in DEPLOY_DECISIONS {
        let n = fn_defs(&home, name);
        if n != 1 {
            bad.push(format!("`{name}` 在 {DEPLOY_HOME} 定义 {n} 处（该恰一处）"));
        }
    }
    let sources = monitor_production_sources();
    assert!(
        sources
            .iter()
            .any(|(r, _)| r == "src/comms/inward/backend_route.rs"),
        "经 `#[path]` 收进来的通信层文件没进扫描面 —— 量法瞎了"
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
        users.iter().any(|(r, _)| r.ends_with("local_backend.rs")),
        "monitor 里用部署契约的人群没扫到 `local_backend.rs` —— 量法瞎了"
    );
    for (rel, body) in contract.iter().chain(users) {
        for name in DEPLOY_DECISIONS {
            if fn_defs(body, name) > 0 {
                bad.push(format!(
                    "{rel}：定义了部署判定 `{name}`（判定只在后端 {DEPLOY_HOME}）"
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
    let mut todo = vec![root().join("src/backend/Cargo.toml")];
    let mut seen = BTreeSet::new();
    while let Some(manifest) = todo.pop() {
        for (_, dir) in production_path_deps(&manifest) {
            if seen.insert(dir.clone()) {
                todo.push(dir.join("Cargo.toml"));
            }
        }
    }
    seen
}

/// ⑤ 前端包只为它那个 `[[bin]]` 链：monitor 生产源码（壳 `src/` ＋ `#[path]` 收进来的通信层文件，不含人群声明里的兄弟包）里
/// 提到它 crate 名的文件 == 那个 bin 的 crate 根，恰好一份。
#[test]
fn a_frontend_package_is_linked_only_for_its_bin() {
    let sources = monitor_production_sources();
    for (at, bin_root, _) in FRONTEND_PACKAGES {
        let manifest = std::fs::read_to_string(root().join(at).join("Cargo.toml"))
            .unwrap_or_else(|e| panic!("读 {at}/Cargo.toml：{e}"));
        let name = manifest
            .lines()
            .find_map(|l| l.trim().strip_prefix("name = \""))
            .and_then(|r| r.split('"').next())
            .unwrap_or_else(|| panic!("{at}/Cargo.toml 没有包名"))
            .replace('-', "_");
        let mentioned: BTreeSet<&str> = sources
            .iter()
            .filter(|(rel, _)| {
                rel.starts_with("src/frontend/shell/") || rel.starts_with("src/comms/")
            })
            .filter(|(_, body)| guard_core::contains_word(body, &name))
            .map(|(rel, _)| rel.as_str())
            .collect();
        assert_eq!(
            mentioned,
            BTreeSet::from([*bin_root]),
            "monitor 生产源码里提到 `{name}` 的文件变了 —— 它只许由那个 `[[bin]]` 转调（库面引它 = 两个前端又缠回一个 crate）"
        );
    }
}

/// ⑤ monitor 的源码人群声明（`guard_core::population_trees`：判据按它收兄弟源码树）== monitor 生产闭包 − 后端生产闭包。
/// 少写一包 ⇒ 按人群扫的判据静默少扫搬进去的那几份；多写一包（后端也链的契约）⇒ 人群被别家的代码撑大。
#[test]
fn the_monitor_population_is_exactly_its_frontend_only_closure() {
    let declared: BTreeSet<PathBuf> =
        guard_core::population_trees(&root().join("src/frontend/shell/src"))
            .into_iter()
            .map(|(_, src)| normalize(src.parent().expect("人群树是 <包>/src")))
            .collect();
    let backend = backend_production_closure();
    assert!(
        backend.iter().any(|d| d.ends_with("copy-core")),
        "后端生产闭包一个共享 crate 都没量到（`copy-core` 明明在）—— 量法瞎了"
    );
    let frontend_only: BTreeSet<PathBuf> = monitor_production_closure()
        .into_iter()
        .map(|(_, _, dir)| dir)
        .filter(|d| !backend.contains(d))
        .collect();
    assert!(
        !frontend_only.is_empty(),
        "monitor 生产闭包减掉后端那一份之后是空集 —— 量法瞎了"
    );
    assert_eq!(
        declared, frontend_only,
        "monitor 的源码人群声明（`src/frontend/shell/Cargo.toml` 的 `[package.metadata.guard] population`）与「只有前端链的包」两向不等"
    );
}
