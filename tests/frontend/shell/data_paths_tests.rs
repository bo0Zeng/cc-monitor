//! # （数据位置：逐个文件、目录不算大小、说明不说内部词）＋ `INVARIANTS §2.1`（真相 / 缓存两类）
//!
//! 核原文：红线格逐字「③ 不递归算目录大小 ⇒ 目录行只显示「已创建」，是刻意的」——
//! `probe_dir_never_returns_size` 判这一句；同节「今天 / 改成」表逐字「去掉 `sid` / `HWND`」——
//! `no_entry_description_speaks_our_internal_words` 判这一句；每项的类与 `INVARIANTS §2.1` 那张表两向相等。
//! ⚠ 那条判据原先自称出自「差项 4」，里没有那一节（已改指 `§6.2`；生产侧同一处注释在写区外，未改）。〔JA1 点址 2026-09-24〕

use super::*;
use std::fs;
use std::sync::atomic::{AtomicU64, Ordering};

struct TestDir(PathBuf);
impl TestDir {
    fn new(tag: &str) -> Self {
        static N: AtomicU64 = AtomicU64::new(0);
        let n = N.fetch_add(1, Ordering::Relaxed);
        let p = std::env::temp_dir().join(format!(
            "ccm-data-paths-test-{}-{tag}-{n}",
            std::process::id(),
        ));
        let _ = fs::remove_dir_all(&p);
        fs::create_dir_all(&p).unwrap();
        TestDir(p)
    }
    fn path(&self) -> &Path {
        &self.0
    }
}
impl Drop for TestDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn probe_file_returns_exists_and_size_when_present() {
    let d = TestDir::new("probe-file");
    let p = d.path().join("x.json");
    fs::write(&p, "hello").unwrap();
    let info = probe_file(p.clone(), "x.json", "test", DataClass::Truth);
    assert!(info.exists);
    assert_eq!(info.size_bytes, Some(5));
    assert_eq!(info.kind, "file");
}

#[test]
fn probe_file_returns_not_exists_when_absent() {
    let d = TestDir::new("probe-file-absent");
    let info = probe_file(
        d.path().join("absent.json"),
        "absent.json",
        "test",
        DataClass::Cache,
    );
    assert!(!info.exists);
    assert!(info.size_bytes.is_none());
}

#[test]
fn probe_dir_never_returns_size() {
    let d = TestDir::new("probe-dir");
    fs::write(d.path().join("a"), "abc").unwrap();
    let info = probe_dir(d.path().to_path_buf(), "x/", "test", DataClass::Cache);
    assert!(info.exists);
    assert!(info.size_bytes.is_none());
    assert_eq!(info.kind, "dir");
}

// 这里原来有「目录里认得出 `.ccm-backup-`」那一条 —— 那一格随 `$PROFILE` 备份搬到界面问本机后端
//   （`tests/frontend/ui/settings/profile-backups.vitest.ts`）。

// ── 〔真相 / 缓存列〕────────────────────────

/// 把 `INVARIANTS.md §2.1` 那张散文表读成 `(名字, 类)`。**异源**：那张表是人写的散文，
/// 枚举是代码 —— 两边各自漂了才会不等。「混（良性）」按真相记（理由见 `DataClass` 头注）。
fn invariants_classes(md: &str) -> Vec<(String, DataClass)> {
    let start = md
        .find("### 2.1 真相 vs 缓存")
        .expect("INVARIANTS.md 里找不到 §2.1 —— 标题改了就来改这条");
    let sec = &md[start..];
    let end = sec[4..].find("\n## ").map_or(sec.len(), |i| i + 4);
    let mut out = Vec::new();
    for line in sec[..end].lines() {
        let cols: Vec<&str> = line.split('|').map(str::trim).collect();
        if cols.len() < 4 || !cols[1].starts_with('`') {
            continue;
        }
        let class = if cols[2].contains("真相") || cols[2].contains("混") {
            DataClass::Truth
        } else if cols[2].contains("缓存") {
            DataClass::Cache
        } else {
            panic!("§2.1 那一行的类认不出来：{line}")
        };
        for name in cols[1].split('`').skip(1).step_by(2) {
            out.push((name.to_string(), class));
        }
    }
    out
}

/// ★★ 枚举里每一项的类 == `INVARIANTS §2.1` 那张表（两向集合相等，异源）。
///
/// 反空真：表至少抽出 3 条真相、3 条缓存（少于这个数说明抽取器坏了，下面的相等在小人群上成立）。
#[test]
fn every_entry_carries_the_class_the_invariants_table_gives_it() {
    let md = include_str!("../../../src/doc/INVARIANTS.md");
    let mut want = invariants_classes(md);
    want.sort_by(|a, b| a.0.cmp(&b.0));
    let truths = want.iter().filter(|(_, c)| *c == DataClass::Truth).count();
    let caches = want.iter().filter(|(_, c)| *c == DataClass::Cache).count();
    assert!(
        truths >= 3 && caches >= 3,
        "从 §2.1 只抽出 {truths} 条真相 / {caches} 条缓存 —— 抽取器坏了"
    );
    let d = TestDir::new("classes");
    // 后端住在同一个家里的那几样也在这张表里（家目录与数据目录在测试里是同一个临时目录，只比名字与类）。
    let mut got: Vec<(String, DataClass)> = monitor_entries(d.path())
        .into_iter()
        .chain(backend_entries(d.path(), d.path()))
        .map(|e| (e.label, e.class))
        .collect();
    got.sort_by(|a, b| a.0.cmp(&b.0));
    assert_eq!(
        got, want,
        "data dir 枚举里的类与 `INVARIANTS §2.1` 那张表对不上。\n\
         ⇒ 新加的文件要**同拍**两处：`data_paths.rs::monitor_entries` / `backend_entries` 选类，§2.1 那张表加一行。"
    );
}

/// ★ 线上形状：`class` 是**必有**的一格，值只有 `truth` / `cache` 两种（TS 那侧按这两个值分档）。
#[test]
fn the_class_goes_over_the_wire_as_two_lowercase_words() {
    let d = TestDir::new("wire");
    let mut seen = std::collections::BTreeSet::new();
    for e in monitor_entries(d.path()) {
        let v = serde_json::to_value(&e).unwrap();
        seen.insert(v["class"].as_str().expect("class 这一格缺席了").to_string());
    }
    assert_eq!(
        seen.into_iter().collect::<Vec<_>>(),
        vec!["cache".to_string(), "truth".to_string()]
    );
}

/// ★ 「今天 / 改成」表那一行：条目说明里不许再有 R1 那两个词（`sid` / `HWND`）。带正控。
/// 原写「差项 4」—— 里已没有那一节，改指今天写着这件事的那一节。
#[test]
fn no_entry_description_speaks_our_internal_words() {
    let hits = |s: &str| {
        let low = s.to_lowercase();
        low.split(|c: char| !c.is_ascii_alphanumeric())
            .any(|w| w == "sid" || w == "hwnd")
    };
    assert!(
        hits("cc 集成的 sid → 终端 HWND 持久绑定"),
        "正控没逮到 —— 尺子瞎了"
    );
    let d = TestDir::new("r1");
    let bad: Vec<String> = monitor_entries(d.path())
        .into_iter()
        .filter(|e| hits(&e.description))
        .map(|e| format!("{}：{}", e.label, e.description))
        .collect();
    assert_eq!(bad, Vec::<String>::new(), "条目说明里又出现了 sid / HWND");
}

/// ★ 〔「一台机器一个家」〕后端那几样的**路径**就是后端落盘用的那一份：
/// 每一行 == 家目录 ＋ 契约常量（`relay_route_core`，后端各写者引的就是它）/ 数据目录 ＋ 凭据文件名（`creds_core::store`）/
/// 宿主交给后端的错误输出所在的目录。期望逐条手写常量名，不从被测函数派生。
#[test]
fn backend_rows_point_where_the_backend_itself_writes() {
    use relay_route_core as rr;
    let home = TestDir::new("backend-home");
    let data = TestDir::new("backend-data");
    let got: Vec<(String, PathBuf, String)> = backend_entries(home.path(), data.path())
        .into_iter()
        .map(|e| (e.label, PathBuf::from(e.path), e.kind))
        .collect();
    let h = home.path();
    let row = |l: &str, p: PathBuf, k: &str| (l.to_string(), p, k.to_string());
    let want = vec![
        row("bin/", h.join(".cc-monitor/bin"), "dir"),
        row("staging/", h.join(rr::STAGING_DIR_REL), "dir"),
        row("relay-key", h.join(rr::KEY_FILE_REL), "file"),
        row("relay-pass-key", h.join(rr::PASS_KEY_FILE_REL), "file"),
        row("run/", h.join(rr::LISTEN_DIR_REL), "dir"),
        row("backend.json", h.join(rr::BACKEND_POLICY_REL), "file"),
        row("profiles.toml", h.join(rr::PROFILES_REL), "file"),
        row(
            "profiles-migrated.json",
            h.join(rr::PROFILES_MIGRATED_REL),
            "file",
        ),
        row(
            "profiles-written.json",
            h.join(rr::PROFILES_WRITTEN_REL),
            "file",
        ),
        row("aliases.sh", h.join(rr::POSIX_ALIASES_REL), "file"),
        row("aliases.ps1", h.join(rr::PS_ALIASES_REL), "file"),
        row("skill-installs.json", h.join(rr::SKILL_LEDGER_REL), "file"),
        row("chores.json", h.join(rr::CHORES_REL), "file"),
        row("last-seen.json", h.join(rr::LAST_SEEN_REL), "file"),
        row("assets-catalog.json", h.join(rr::ASSET_CATALOG_REL), "file"),
        row("quota.json", h.join(rr::QUOTA_LEDGER_REL), "file"),
        row("rotation.json", h.join(rr::ROTATION_REL), "file"),
        row(
            "launch-accounts.json",
            h.join(rr::LAUNCH_ACCOUNTS_REL),
            "file",
        ),
        row("launch-pending/", h.join(rr::LAUNCH_NOTES_DIR_REL), "dir"),
        row("known_hosts", h.join(rr::KNOWN_HOSTS_REL), "file"),
        row("accounts/", h.join(rr::ACCOUNTS_DIR_REL), "dir"),
        row("accounts-mcp.json", h.join(rr::ACCOUNTS_MCP_REL), "file"),
        row(
            "apikey-credentials.json",
            creds_core::store::credentials_path(data.path()),
            "file",
        ),
        row(
            "logs/backend/",
            crate::logging::backend_stderr_log_path(data.path())
                .parent()
                .unwrap()
                .to_path_buf(),
            "dir",
        ),
        row("backups/", h.join(rr::EXT_BACKUPS_DIR_REL), "dir"),
    ];
    assert_eq!(got, want);
}

/// ★★ 〔「家里的都进唯一枚举，判据两向」〕契约 crate 里 `~/.cc-monitor/` 下的每一个相对路径常量
/// == 数据位置页后端那几行覆盖的路径（后端落点由它所在的 `bin/` 那一行覆盖）。**异源**：一侧是 `relay-route-core`
/// 源码里现抽的常量值，一侧是被测枚举。契约里新长一个家里的路径却没进这一页 ⇒ 红；这一页列了契约里没有的 ⇒ 红。
#[test]
fn every_home_path_in_the_contract_has_a_row() {
    let src = guard_core::production_code(include_str!(
        "../../../src/common/relay-route-core/src/lib.rs"
    ));
    // 账号库那一段的字面量住一个宏里（足迹表要 `concat!` 它）⇒ `X!()` 形的常量按宏体里那个字面量算。
    let macro_lit = |name: &str| -> Option<String> {
        let body = &src[src.find(&format!("macro_rules! {name} {{"))?..];
        let q = body.find('"')? + 1;
        Some(body[q..q + body[q..].find('"')?].to_string())
    };
    let mut declared: std::collections::BTreeSet<String> = Default::default();
    for line in src.lines() {
        let l = line.trim();
        if let Some(rest) = l.strip_prefix("pub const ") {
            if let Some((_, v)) = rest.split_once("&str = ") {
                let v = v.trim_end_matches(';');
                let lit = match v.strip_prefix('"') {
                    Some(q) => Some(q.trim_end_matches('"').to_string()),
                    None => v.strip_suffix("!()").and_then(macro_lit),
                };
                if let Some(v) = lit.filter(|v| v.starts_with(".cc-monitor/")) {
                    declared.insert(v);
                }
            }
        }
    }
    assert!(
        declared.len() >= 10,
        "只抽到 {} 个常量 —— 抽取器坏了：{declared:?}",
        declared.len()
    );
    let home = TestDir::new("contract-home");
    let data = TestDir::new("contract-data");
    let covered: std::collections::BTreeSet<String> =
        backend_entries(home.path(), data.path())
            .into_iter()
            .filter_map(|e| {
                Path::new(&e.path)
                    .strip_prefix(home.path())
                    .ok()
                    .map(|p| p.to_string_lossy().replace('\\', "/"))
            })
            .collect();
    let mut want = declared.clone();
    // 后端落点（`bin/ccm`）由它所在目录那一行覆盖
    want.remove(relay_route_core::BACKEND_LANDING_REL);
    want.insert(".cc-monitor/bin".to_string());
    assert_eq!(covered, want);
}
