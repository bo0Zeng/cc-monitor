//! **后端的「现在」只有一处**：各处取当前墙钟一律调 `common::time::now`（及它的几个读法 `now_secs` · `now_ms`），
//! 别处不许直接问系统钟。只有一处，截图台架才能把「现在」给死（台架起每台后端时交一个时刻，页面在同一刻拨 `Date`）。
//!
//! | # | 判什么 |
//! |---|---|
//! | C1 | （已换成 clippy：`src/backend/clippy.toml` 的 `disallowed-methods` ＋ `Cargo.toml` 里定成 deny；别处直接问系统钟 clippy 红，起别名也认得出） |
//! | C2 | 拨钟那一段只在特性 `shots` 下编：`Cargo.toml` 的 `shots = []` 不在 `default` 里；`common/time.rs` 里 `mod shots_clock` 紧跟在 `#[cfg(feature = "shots")]` 下；台架那个环境变量名只在那一段里出现，生产源码别处不提 |
//! | C3 | 台架真在编它、真在交时刻：`tests/shots/real/pool.mjs` 按 `--features shots` 编后端、给每台后端设那个环境变量 |
//! | C4 | 默认构建（本测试这一档）里特性没开；`now()` 就是系统钟 |
//! | C6 | 「此刻 unix 毫秒 / 秒」只有 `common/time.rs` 的两个读口（`now_ms` · `now_secs`，值域 `u64`）：别处不另定同名读口、不就地把 `now()` 换算成毫秒 / 秒；还没收的照 [`STILL`] 逐份记着 |
//!
//! C2 · C3 读的是源码文本。
//! 发版 / CI / 门禁里不许点名特性 `shots`，由壳那一侧的 `shots_feature_guard_tests::s3` 一并钉着（它扫的是全仓的工作流与门禁脚本）。

use std::path::Path;

const FEATURE: &str = "shots";
const CLOCK_FILE: &str = "common/time.rs";
const MODULE: &str = "shots_clock";
/// 台架交时刻的那个环境变量（unix 毫秒）。
const ENV: &str = "CCM_SHOTS_NOW_MS";

fn read(p: &Path) -> String {
    std::fs::read_to_string(p).unwrap_or_else(|e| panic!("读不到 {}：{e}", p.display()))
}

fn production_files() -> Vec<(String, String)> {
    let root = guard_core::addr::backend_src_root();
    guard_core::scan_tree_excluding(&root, &["rs"], &[])
        .into_iter()
        .map(|(p, src)| {
            let rel = p
                .strip_prefix(&root)
                .unwrap_or(&p)
                .to_string_lossy()
                .replace('\\', "/");
            (rel, guard_core::production_code(&src))
        })
        .collect()
}

/// 一份清单 `[features]` 里那个特性的值与 `default` 的值（没写 ⇒ `None`）。
fn feature_lines(manifest: &str) -> (Option<String>, Option<String>) {
    let mut in_features = false;
    let mut feat = None;
    let mut default = None;
    for l in manifest.lines() {
        let t = l.trim();
        if t.starts_with('[') {
            in_features = t == "[features]";
            continue;
        }
        if !in_features {
            continue;
        }
        if let Some((k, v)) = t.split_once('=') {
            match k.trim() {
                k if k == FEATURE => feat = Some(v.trim().to_string()),
                "default" => default = Some(v.trim().to_string()),
                _ => {}
            }
        }
    }
    (feat, default)
}

#[test]
fn c2_the_clock_dial_only_compiles_under_the_feature() {
    // 拨钟那一段住 `backend-common`：特性在那个包里定（`shots = []`，不开任何依赖的特性）；主 crate 只把同名特性转给它。
    let root = guard_core::addr::backend_src_root();
    let (feat, home_default) = feature_lines(&read(&root.join("common/Cargo.toml")));
    assert_eq!(
        feat.as_deref(),
        Some("[]"),
        "common/Cargo.toml 的 [features] 里有 `{FEATURE} = []`（它不开任何依赖的特性）"
    );
    let (forward, main_default) = feature_lines(&read(&root.join("Cargo.toml")));
    assert_eq!(
        forward,
        Some(format!("[\"backend-common/{FEATURE}\"]")),
        "主 crate 的 [features] 里 `{FEATURE}` 只转给 backend-common 那一个"
    );
    for d in [home_default, main_default].into_iter().flatten() {
        assert!(
            !d.contains(FEATURE),
            "`default` 里不许有 `{FEATURE}`（连转给下层的那一形）：{d}"
        );
    }

    let src = read(&guard_core::addr::backend_src_root().join(CLOCK_FILE));
    let prod = guard_core::production_code(&src);
    let lines: Vec<&str> = prod.lines().collect();
    let decl: Vec<usize> = lines
        .iter()
        .enumerate()
        .filter(|(_, l)| l.trim() == format!("mod {MODULE} {{"))
        .map(|(i, _)| i)
        .collect();
    assert_eq!(decl.len(), 1, "{CLOCK_FILE} 里 `mod {MODULE}` 恰好一个");
    assert_eq!(
        lines[decl[0] - 1].trim(),
        format!("#[cfg(feature = \"{FEATURE}\")]"),
        "`mod {MODULE}` 紧跟在特性闸下面"
    );
    // 那个模块的体（到与它同缩进的 `}` 为止）。
    let indent = lines[decl[0]].len() - lines[decl[0]].trim_start().len();
    let end = (decl[0] + 1..lines.len())
        .find(|&i| lines[i].trim() == "}" && lines[i].len() - lines[i].trim_start().len() == indent)
        .expect("`mod shots_clock` 有收尾");
    let body = lines[decl[0]..=end].join("\n");
    assert!(
        body.contains(ENV),
        "环境变量 `{ENV}` 在 `mod {MODULE}` 里读"
    );
    let rest = format!(
        "{}\n{}",
        lines[..decl[0]].join("\n"),
        lines[end + 1..].join("\n")
    );
    assert!(!rest.contains(ENV), "{CLOCK_FILE} 里特性闸外读了 `{ENV}`");
    // 引用那一行（`shots_clock::now()`）也在特性闸下。
    let uses: Vec<usize> = lines
        .iter()
        .enumerate()
        .filter(|(i, l)| (*i < decl[0] || *i > end) && l.contains(&format!("{MODULE}::")))
        .map(|(i, _)| i)
        .collect();
    assert_eq!(
        uses.len(),
        1,
        "`{MODULE}::` 在模块外恰好用一次（`now()` 里）"
    );
    assert_eq!(
        lines[uses[0] - 1].trim(),
        format!("#[cfg(feature = \"{FEATURE}\")]"),
        "`{MODULE}::now()` 那一句紧跟在特性闸下面"
    );

    // 生产源码别处不提它。
    let mut hits = Vec::new();
    for (rel, prod) in production_files() {
        if rel != CLOCK_FILE && (prod.contains(ENV) || guard_core::contains_word(&prod, MODULE)) {
            hits.push(rel);
        }
    }
    assert!(hits.is_empty(), "生产源码别处提了台架的拨钟：{hits:?}");
}

#[test]
fn c3_the_rig_builds_it_and_hands_a_moment() {
    let pool = read(&guard_core::addr::repo_root().join("tests/shots/real/pool.mjs"));
    let build = pool
        .lines()
        .find(|l| l.contains("cwd: path.join(repo, \"src/backend\")"))
        .expect("pool.mjs 里有编后端那一行");
    assert!(
        build.contains(&format!("\"--features\", \"{FEATURE}\"")),
        "台架按特性 `{FEATURE}` 编后端：{}",
        build.trim()
    );
    assert!(
        pool.contains(&format!("{ENV}:")),
        "台架给每台后端设 `{ENV}`"
    );
}

#[test]
fn c4_default_build_reads_the_system_clock() {
    assert!(!cfg!(feature = "shots"), "默认构建里特性 `shots` 没开");
    let sys = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64;
    let ours: u64 = crate::common::time::now_ms();
    assert!(
        ours.abs_diff(sys) < guard_core::wall::CEILING.as_millis() as u64,
        "默认构建里 `now_ms()` 就是系统钟：{ours} vs {sys}"
    );
}

/// C5 ★ **产品代码不认台架**（10-26 定：截图要的各态由台架的世界真长成那样，不由后端读台架交的结论）：
/// 生产源码里特性闸 `shots` 只在拨钟那一份（[`CLOCK_FILE`]）；台架的环境变量（`CCM_SHOTS_` 打头）只有拨钟那一个。
/// 正控：同一识别器在合成语料里认得出一处多出来的特性闸、一个多出来的台架环境变量。
#[test]
fn c5_production_code_knows_no_rig_but_the_clock() {
    fn gated(prod: &str) -> bool {
        prod.contains(&format!("feature = \"{FEATURE}\""))
    }
    fn rig_envs(prod: &str) -> Vec<String> {
        let mut out = Vec::new();
        let mut rest = prod;
        while let Some(i) = rest.find("CCM_SHOTS_") {
            let tail = &rest[i..];
            let end = tail
                .find(|c: char| !(c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_'))
                .unwrap_or(tail.len());
            out.push(tail[..end].to_string());
            rest = &tail[end..];
        }
        out
    }
    assert!(gated("#[cfg(feature = \"shots\")]\nmod x {}"));
    assert_eq!(
        rig_envs("std::env::var(\"CCM_SHOTS_ROUNDS\")"),
        ["CCM_SHOTS_ROUNDS"]
    );

    let mut gates = Vec::new();
    let mut envs = Vec::new();
    for (rel, prod) in production_files() {
        if rel != CLOCK_FILE && gated(&prod) {
            gates.push(rel.clone());
        }
        for e in rig_envs(&prod) {
            if e != ENV {
                envs.push(format!("{rel}: {e}"));
            }
        }
    }
    assert!(
        gates.is_empty(),
        "生产源码在拨钟之外认特性 `{FEATURE}`：{gates:?}"
    );
    assert!(
        envs.is_empty(),
        "生产源码读了拨钟之外的台架环境变量：{envs:?}"
    );
}

/// 别处不许另定的「此刻」读口名。
const READER_NAMES: [&str; 4] = ["now_ms", "now_secs", "now_unix", "unix_now"];

/// 还没收的那几份（相对 `src/backend`）：各自另定了「此刻」读口，或就地把 `now()` 换算成毫秒 / 秒。只许变短，收完一份划掉一行。
const STILL: &[&str] = &[
    "accounts/manage/exec.rs",
    "accounts/manage/mcp_share_exec.rs",
    "accounts/quota/mod.rs",
    "assets/aliases/page.rs",
    "assets/asset_catalog.rs",
    "assets/cc_bus_install.rs",
    "assets/ext.rs",
    "control/asks/hook.rs",
    "control/asks/mod.rs",
    "control/ccm/mod.rs",
    "control/deploy_plan.rs",
    "control/terminals.rs",
    "faces/history_face.rs",
    "faces/plan_face.rs",
    "faces/read_face.rs",
    "files/commit.rs",
    "files/write.rs",
    "footprint/last_seen.rs",
];

/// 一份生产源码里 C6 认得出的命中：另定的读口（[`READER_NAMES`]）· 就地换算（`now()` 之后 3 行内接 `UNIX_EPOCH` 再接 `.as_millis()` / `.as_secs()`）。
/// 做盐的 `.as_nanos()` · `.subsec_nanos()` 不算；别人的 `.now()`（注入的钟）不算。
fn own_clock_hits(prod: &str) -> Vec<String> {
    let mut out = Vec::new();
    for name in READER_NAMES {
        if prod.contains(&format!("fn {name}(")) {
            out.push(format!("fn {name}"));
        }
    }
    let lines: Vec<&str> = prod.lines().collect();
    for (i, l) in lines.iter().enumerate() {
        let Some(at) = l.find("now()").filter(|&at| {
            let before = &l[..at];
            before.ends_with("time::")
                || !before
                    .chars()
                    .next_back()
                    .is_some_and(|c| c == '.' || c == '_' || c == ':' || c.is_alphanumeric())
        }) else {
            continue;
        };
        let window = std::iter::once(&l[at..])
            .chain(lines[i + 1..lines.len().min(i + 4)].iter().copied())
            .collect::<Vec<_>>()
            .join("\n");
        let Some(epoch) = window.find("UNIX_EPOCH") else {
            continue;
        };
        let tail = &window[epoch..];
        if tail.contains(".as_millis()") || tail.contains(".as_secs()") {
            out.push(format!("第 {} 行就地换算", i + 1));
        }
    }
    out
}

/// C6 ★ **「此刻」只有两个读口**：`common/time.rs` 的 `now_ms` · `now_secs`（`u64`）。别处另定同名读口、或就地把 `now()` 换算成毫秒 / 秒的，
/// 按文件归，和 [`STILL`] 两头比：多出来的红（新写了一份），`STILL` 里有、源码里已经没有的也红（收了忘了划掉）。
/// 正控：合成语料里认得出一份私有 `fn now_ms`、一处内联换算；做盐的纳秒 · 注入的钟 `.now()` 不认。
#[test]
fn c6_one_pair_of_now_readers() {
    assert_eq!(
        own_clock_hits("fn now_ms() -> u64 {\n    0\n}"),
        ["fn now_ms"],
        "认得出私有读口"
    );
    assert_eq!(
        own_clock_hits(
            "let t = crate::common::time::now()\n    .duration_since(std::time::UNIX_EPOCH)\n    .map_or(0, |d| d.as_secs());"
        )
        .len(),
        1,
        "认得出内联换算"
    );
    assert!(
        own_clock_hits(
            "let salt = crate::common::time::now()\n    .duration_since(UNIX_EPOCH)\n    .map(|d| d.as_nanos());"
        )
        .is_empty(),
        "做盐的纳秒不算"
    );
    assert!(
        own_clock_hits(
            "env.now()\n    .duration_since(std::time::UNIX_EPOCH)\n    .map(|d| d.as_secs())"
        )
        .is_empty(),
        "注入的钟 `.now()` 不算"
    );

    let mut found = std::collections::BTreeMap::new();
    for (rel, prod) in production_files() {
        if rel == CLOCK_FILE {
            continue;
        }
        let hits = own_clock_hits(&prod);
        if !hits.is_empty() {
            found.insert(rel, hits);
        }
    }
    let still: std::collections::BTreeSet<&str> = STILL.iter().copied().collect();
    let extra: Vec<String> = found
        .iter()
        .filter(|(rel, _)| !still.contains(rel.as_str()))
        .map(|(rel, hits)| format!("{rel}: {}", hits.join(" · ")))
        .collect();
    let gone: Vec<&str> = still
        .iter()
        .copied()
        .filter(|rel| !found.contains_key(*rel))
        .collect();
    assert!(
        extra.is_empty(),
        "「此刻」的读口在 `{CLOCK_FILE}` 之外又写了一份（改调 `common::time::now_ms` / `now_secs`）：{extra:#?}"
    );
    assert!(gone.is_empty(), "`STILL` 里这几份已经收了，划掉：{gone:?}");
}
