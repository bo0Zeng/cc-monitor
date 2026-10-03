use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

fn repo_root() -> PathBuf {
    // 住址唯一源：`crate::guard_support`（头注写着 24 份副本怎么一起漂的）。
    crate::guard_support::repo_root()
}

/// 从 `Cargo.lock` 抠出 `包名 → 版本集合`。
///
/// ⚠ 只认 TOML 里 `name = "..."` 紧跟 `version = "..."` 这个形状 ——
/// 抠不到东西时下面的对拍会两边都空、静默变绿，所以调用方必须做计数自检。
fn parse_lock(rel: &str) -> BTreeMap<String, BTreeSet<String>> {
    let body = std::fs::read_to_string(repo_root().join(rel))
        .unwrap_or_else(|e| panic!("{rel} 读不到：{e} —— 路径变了就把这条一起改"));
    let mut out: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let mut name: Option<String> = None;
    for line in body.lines() {
        if let Some(v) = line.strip_prefix("name = ") {
            name = v.trim().trim_matches('"').to_string().into();
        } else if let Some(v) = line.strip_prefix("version = ") {
            if let Some(n) = name.take() {
                out.entry(n)
                    .or_default()
                    .insert(v.trim().trim_matches('"').to_string());
            }
        }
    }
    out
}

/// backend lock 里 `name version` 那一格的**依赖者**（`(包名, 版本)`）。依赖串写成 `"name"`
/// （全 lock 只有一版时）或 `"name version"`（多版并存时）两种形状 —— 两种都认。
fn dependents_in_lock(rel: &str, name: &str, version: &str) -> BTreeSet<(String, String)> {
    let body = std::fs::read_to_string(repo_root().join(rel)).expect("读 lock");
    let versions = parse_lock(rel).get(name).cloned().unwrap_or_default();
    let field = |block: &str, key: &str| {
        block
            .lines()
            .find_map(|l| l.strip_prefix(key))
            .map(|v| v.trim().trim_matches('"').to_string())
            .unwrap_or_default()
    };
    // 按行切块（一行恰好是 `[[package]]` 就起一块）—— 整行相等，不在语料上做子串切分（`needle_anchor_registry`）。
    let mut blocks: Vec<String> = Vec::new();
    for row in body.lines() {
        if row.trim() == "[[package]]" {
            blocks.push(String::new());
        } else if let Some(b) = blocks.last_mut() {
            b.push_str(row);
            b.push('\n');
        }
    }
    let mut out = BTreeSet::new();
    for block in &blocks {
        let hits = block.lines().any(|l| {
            let dep = l.trim().trim_end_matches(',').trim_matches('"');
            dep == format!("{name} {version}") || (dep == name && versions.len() == 1)
        });
        if hits {
            out.insert((field(block, "name = "), field(block, "version = ")));
        }
    }
    out
}

/// ★ 正题：**真冲突集必须为空**。
///
/// **「真」多了一道条件**：backend 那一版的**依赖者（包名 ＋ 版本）全都不在 monitor lock 里** ⇒ 不算。
/// 起因：monitor 删了 `russh` / `russh-sftp`（界面进程零 SSH）之后，只有 SSH 那棵树要的几个新版密码学包
/// （`sha2 0.11` · `digest 0.11` · `rand 0.10` …）只剩 backend 一侧有 —— 而它们的依赖者（`russh` · `ssh-key` ·
/// 新版 `sha2` …）**monitor 那一侧一份都不编**。本条要防的是「同一份源码（两侧都编的那个包、同一版）在两边拿到
/// 不同版本的依赖」，那一形要求依赖者那一格（名 ＋ 版）**两侧都有**；只在 backend 的依赖者没有「同一份源码」这回事。
/// 正控：`the_backend_only_exemption_really_needs_backend_only_dependents`。
fn only_backend_only_dependents(
    m: &BTreeMap<String, BTreeSet<String>>,
    name: &str,
    version: &str,
) -> bool {
    let who = dependents_in_lock("src/backend/Cargo.lock", name, version);
    // monitor 那一侧**依赖 `name`（任何一版）**的那些格：同一格（名 ＋ 版）两侧都依赖它、版本却不同，才是冲突。
    // 依赖者两侧都有、但 monitor 那一格压根不依赖 `name` ⇒ 那是 backend 那边多开了一个可选 feature
    // （`getrandom 0.4.2` 的 `rand_core` 就是这一形），不是「同一份源码拿到两个版本」。
    let monitor_side: BTreeSet<(String, String)> = m
        .get(name)
        .into_iter()
        .flatten()
        .flat_map(|mv| dependents_in_lock("src/frontend/shell/Cargo.lock", name, mv))
        .collect();
    !who.is_empty() && who.iter().all(|pair| !monitor_side.contains(pair))
}

/// 放行那一格的**正控**：依赖者两侧都有的那种（`tokio` 依赖的任何一版包都是）不许被放行。
#[test]
fn the_backend_only_exemption_really_needs_backend_only_dependents() {
    let m = parse_lock("src/frontend/shell/Cargo.lock");
    let d = parse_lock("src/backend/Cargo.lock");
    // `mio` 的依赖者里有 `tokio`（两侧都编）⇒ 无论版本怎样都不许被放行。
    let v = d["mio"]
        .iter()
        .next()
        .expect("backend lock 里有 mio")
        .clone();
    assert!(
        !only_backend_only_dependents(&m, "mio", &v),
        "依赖者两侧都有的包被当成了「只有 backend 要」—— 放行口子开大了"
    );
    // 反向：`russh` 那棵树里的新版 `sha2` 今天确实只被 backend 独有的包依赖（本拍的放行对象，现打）。
    if let Some(v) = d
        .get("sha2")
        .and_then(|vs| vs.iter().find(|v| !m["sha2"].contains(*v)))
    {
        assert!(only_backend_only_dependents(&m, "sha2", v));
    }
}

///
/// 「真冲突」= 同一个包，**backend 解析出的版本集不是 monitor 的子集**。
/// 超集（monitor 多持有几版）不算 —— 那不是「同一份源码编两次」。
#[test]
fn the_two_lockfiles_have_no_real_version_conflict() {
    let m = parse_lock("src/frontend/shell/Cargo.lock");
    let d = parse_lock("src/backend/Cargo.lock");
    let common: Vec<&String> = m.keys().filter(|k| d.contains_key(*k)).collect();
    // 抽取器自检：解析坏了的时候 `common` 会是空的，下面那条就成了一句废话。
    assert!(
        common.len() >= 50,
        "两份 lock 只解析出 {} 个共有包（08-06 实测 68）—— 解析器坏了，\
             下面那条会零命中地绿",
        common.len()
    );

    let mut conflicts = Vec::new();
    for k in common {
        let (mv, dv) = (&m[k], &d[k]);
        let real = dv
            .iter()
            .filter(|v| !mv.contains(*v))
            .any(|v| !only_backend_only_dependents(&m, k, v));
        if real {
            conflicts.push(format!(
                "  {k}: monitor {:?} / backend {:?}",
                mv.iter().collect::<Vec<_>>(),
                dv.iter().collect::<Vec<_>>()
            ));
        }
    }
    assert!(
        conflicts.is_empty(),
        "两份 lockfile 有**真冲突**（backend 解析出的版本不在 monitor 的集合里）：\n{}\n\n\
             ★ 后果不是「多编一遍」：`ci.yml` 的 backend job 有 \
             `defaults.run.working-directory: src/backend` ⇒ 那条**跨 target Windows check** \n\
             走的是 **backend 的 lock**，而 monitor 真编走**它自己的**。\n\
             而 `branch-core` / `usage-core` 既是后端生产依赖、又是 monitor workspace member \n\
             ⇒ **同一份源码分别编进两个版本**，那条 check 证明的依赖树与真编的不是同一棵。\n\
             ⚠ 它是 `ci.yml` 自称的「平台线**唯一真判据**」。\n\n\
             修法：在 `src/frontend/shell/` 下 `cargo update -p <包>` 把 monitor 抬到后端那一版\n\
             （两侧都验一遍编译），**不是**把判据放宽成「版本集合相同」——\n\
             那会把 11 个 monitor 超集也判红，是一条错的判据。",
        conflicts.join("\n")
    );
}

/// ★ 上面那套论证的**前提**：跨 target check 确实跑在 `src/backend` 下。
///
/// 这个 `working-directory` 一改，「两份 lock 编的不是同一棵树」这句话就不成立了 ——
/// 那时该回来重判整条，而不是留着一条论证已经落空的判据。
///
/// ⚠⚠ **08-07 订正：本条原本钉的是两个「字符串各自存在」，不是它们的关系。**
/// 变异实测两刀：① 把跨 target check 整步搬进 `rust` job（它改走 monitor 的 lock）；
/// ② 把 backend job 的 `working-directory` 改成 `src/frontend/shell`、同时把那一行原样挪到别的 job。
/// 两刀都让本模块整套论证**反过来**，而本条**一声不吭**（三条全绿）。
///
/// ★ 两刀确实各有别的判据红了 —— 但读它们的诊断：说的是
/// 「切出来的块里没有 `working-directory: src/backend` —— **切错 job 了，本条会零命中地绿**」，
/// 那是当时另一条判据（钉 backend job 四步的那条，后来随四步改调门禁删了）的**抽取器自检**在说话。
/// 照它去修，人会去查切块逻辑，而真实事件是 **backend job 换了工作目录**。
/// ⇒ **「有别的判据接住」不等于「有人把这件事讲对了」** —— 本条才是该讲这句话的那条。
///
/// 改法：钉**关系** —— 两件事必须落在**同一个 job 块**里。切块用
/// `shared_crate_registry::ci_yaml`（E3：`ci.yml` 的读取与切块只有一个家）。
#[test]
fn the_cross_target_check_still_runs_under_the_backend_lock() {
    use crate::shared_crate_registry::ci_yaml;
    const CHECK: &str = "cargo check --all-targets --target x86_64-pc-windows-msvc";
    // ⚠ **行锚定**，不能用 `contains` 裸匹配：`src/backend` 是
    // `src/backend-X` 的**前缀** —— 变异实测过，裸 `contains` 照样绿。
    const WD: &str = "working-directory: src/backend";

    let block = ci_yaml::job_block("backend");
    // 抽取器自检：切不出块时下面两条会零命中地绿。
    assert!(
        block.lines().count() >= 10,
        "从 `ci.yml` 切 `backend:` job 只得到 {} 行 —— job 名或缩进变了，本条会零命中地绿",
        block.lines().count()
    );
    assert!(
        block.lines().any(|l| l.trim() == WD),
        "`backend:` job 里没有 `{WD}` 了 —— 它可能被改了值、也可能被挪到了别的 job。\n\
             ⚠ **别只看「这行字在不在 ci.yml 里」** —— 它在别处照样在，而本模块要的是\n\
             「**跨 target check 所在的那个 job** 跑在后端的 lock 下」。\n\
             这个前提一没，`the_two_lockfiles_have_no_real_version_conflict` 整套论证就落空，\n\
             该回来重判整条，而不是留着一条论证已经落空的判据。"
    );
    assert!(
        block.contains(CHECK),
        "跨 target Windows check 不在 `backend:` job 里了（它是 `ci.yml` 自称的\n\
             「平台线唯一真判据」）。要么它被删了（那是个更大的问题），\n\
             要么它被搬进了别的 job —— 而别的 job 的 `working-directory` 不是\n\
             `src/backend` ⇒ 它改走 **monitor 的 lock**，本模块整套论证反过来。\n\
             ⚠ 08-07 变异实测：这一刀之前本条是绿的。"
    );
}

/// **超集不算冲突** —— 这条防的是「把判据放宽/收紧成错的形状」。
#[test]
fn a_monitor_superset_is_not_a_conflict() {
    let m = parse_lock("src/frontend/shell/Cargo.lock");
    let d = parse_lock("src/backend/Cargo.lock");
    let supersets: Vec<&String> = m
        .keys()
        .filter(|k| d.contains_key(*k))
        .filter(|k| m[*k] != d[*k] && d[*k].is_subset(&m[*k]))
        .collect();
    assert!(
        !supersets.is_empty(),
        "一个 monitor 超集都没有了（08-06 实测 11 个，含 8 个 `windows_*`）。\
             ⚠ 若真是对齐掉了那很好；但更可能是**解析器坏了**或**有人把判据改成了\
             「版本集合必须相同」** —— 后者会把这 11 个也判红，是报告 I-6 那条\
             「10 个不一致」误导性写法的翻版。"
    );
}
