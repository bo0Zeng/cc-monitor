//! **两份 lockfile 的真冲突必须为空**〔audit-0805 F16 下半，报告 I-6〕。
//!
//! # 报告说「10 个不一致」，核实之后是「2 个真冲突 + 11 个 monitor 超集」
//!
//! 08-06 实测：共有包 **68** · 版本集合不同 **13** · **真冲突 2**
//!（`serde_json 1.0.149/1.0.150` · `memchr 2.8.0/2.8.1`）· monitor 超集 **11**。
//!
//! 报告列的 8 个 `windows_*` **不构成冲突** —— monitor 侧**同时持有**那一版，
//! daemon 的解析是 monitor 的**子集**。⇒ **判据不能写成「版本集合相同」**：
//! 那会把 11 个超集也判红，是**一条错的判据**（而且它会逼人去做一件没必要的对齐）。
//!
//! # 那 2 条为什么要紧
//!
//! `ci.yml` 的 daemon job 有 `defaults.run.working-directory: remote-daemon-proto`
//! ⇒ 那条**跨 target Windows check** 走的是 **daemon 的 lock**；
//! 而 monitor 真编在 `src-tauri` 下 ⇒ 走**它自己的 lock**。
//!
//! 而 `branch-core` / `usage-core` 都写 `serde_json = "1"`，两者**既是 daemon 的生产依赖、
//! 又是 monitor 的 workspace member** ⇒ **同一份源码分别编进两个版本**。
//!
//! ⚠ 别读成小事：那条跨 target check 是 `ci.yml` 自称的「平台线**唯一真判据**」，
//! 而它证明的依赖树**与 monitor 真编的不是同一棵**。
//!
//! # 顺序：先立判据（红），再对齐（绿）
//!
//! 功能件 §4 把「得先动 lockfile」写成了不做的理由。其实**先立判据才是对的顺序** ——
//! 判据当时就是红的，那个红本身就是 **E11「先红后信」** 要的证据。
//!
//! ⚠ **08-07 订正时态**：上一句原写「判据**此刻**就该是红的」。那是建判据当天的现场，
//! 而那 2 条随后就对齐了（今天两侧都是 `serde_json 1.0.150` / `memchr 2.8.1`）⇒
//! 本模块现在是**绿的**，它守的是「别再漂回去」。
//! 留着原句会让人以为仓里还欠着一次对齐 —— 这正是 E12 那一族（散文记的是修之前）。

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, BTreeSet};
    use std::path::{Path, PathBuf};

    fn repo_root() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("仓根")
            .to_path_buf()
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

    /// ★ 正题：**真冲突集必须为空**。
    ///
    /// 「真冲突」= 同一个包，**daemon 解析出的版本集不是 monitor 的子集**。
    /// 超集（monitor 多持有几版）不算 —— 那不是「同一份源码编两次」。
    #[test]
    fn the_two_lockfiles_have_no_real_version_conflict() {
        let m = parse_lock("src-tauri/Cargo.lock");
        let d = parse_lock("remote-daemon-proto/Cargo.lock");
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
            if !dv.is_subset(mv) {
                conflicts.push(format!(
                    "  {k}: monitor {:?} / daemon {:?}",
                    mv.iter().collect::<Vec<_>>(),
                    dv.iter().collect::<Vec<_>>()
                ));
            }
        }
        assert!(
            conflicts.is_empty(),
            "两份 lockfile 有**真冲突**（daemon 解析出的版本不在 monitor 的集合里）：\n{}\n\n\
             ★ 后果不是「多编一遍」：`ci.yml` 的 daemon job 有 \
             `defaults.run.working-directory: remote-daemon-proto` ⇒ 那条**跨 target Windows check** \n\
             走的是 **daemon 的 lock**，而 monitor 真编走**它自己的**。\n\
             而 `branch-core` / `usage-core` 既是 daemon 生产依赖、又是 monitor workspace member \n\
             ⇒ **同一份源码分别编进两个版本**，那条 check 证明的依赖树与真编的不是同一棵。\n\
             ⚠ 它是 `ci.yml` 自称的「平台线**唯一真判据**」。\n\n\
             修法：在 `src-tauri/` 下 `cargo update -p <包>` 把 monitor 抬到 daemon 那一版\n\
             （两侧都验一遍编译），**不是**把判据放宽成「版本集合相同」——\n\
             那会把 11 个 monitor 超集也判红，是一条错的判据。",
            conflicts.join("\n")
        );
    }

    /// ★ 上面那套论证的**前提**：跨 target check 确实跑在 `remote-daemon-proto` 下。
    ///
    /// 这个 `working-directory` 一改，「两份 lock 编的不是同一棵树」这句话就不成立了 ——
    /// 那时该回来重判整条，而不是留着一条论证已经落空的判据。
    ///
    /// ⚠⚠ **08-07 订正：本条原本钉的是两个「字符串各自存在」，不是它们的关系。**
    /// 变异实测两刀：① 把跨 target check 整步搬进 `rust` job（它改走 monitor 的 lock）；
    /// ② 把 daemon job 的 `working-directory` 改成 `src-tauri`、同时把那一行原样挪到别的 job。
    /// 两刀都让本模块整套论证**反过来**，而本条**一声不吭**（三条全绿）。
    ///
    /// ★ 两刀确实各有别的判据红了 —— 但读它们的诊断：说的是
    /// 「切出来的块里没有 `working-directory: remote-daemon-proto` —— **切错 job 了，本条会零命中地绿**」，
    /// 那是 `ci_actually_runs_the_daemon_four_steps` 的**抽取器自检**在说话。
    /// 照它去修，人会去查切块逻辑，而真实事件是 **daemon job 换了工作目录**。
    /// ⇒ **「有别的判据接住」不等于「有人把这件事讲对了」** —— 本条才是该讲这句话的那条。
    ///
    /// 改法：钉**关系** —— 两件事必须落在**同一个 job 块**里。切块用
    /// `shared_crate_registry::ci_yaml`（E3：`ci.yml` 的读取与切块只有一个家）。
    #[test]
    fn the_cross_target_check_still_runs_under_the_daemon_lock() {
        use crate::shared_crate_registry::ci_yaml;
        const CHECK: &str = "cargo check --all-targets --target x86_64-pc-windows-msvc";
        // ⚠ **行锚定**，不能用 `contains` 裸匹配：`remote-daemon-proto` 是
        // `remote-daemon-proto-X` 的**前缀** —— 变异实测过，裸 `contains` 照样绿。
        const WD: &str = "working-directory: remote-daemon-proto";

        let block = ci_yaml::job_block("daemon");
        // 抽取器自检：切不出块时下面两条会零命中地绿。
        assert!(
            block.lines().count() >= 10,
            "从 `ci.yml` 切 `daemon:` job 只得到 {} 行 —— job 名或缩进变了，本条会零命中地绿",
            block.lines().count()
        );
        assert!(
            block.lines().any(|l| l.trim() == WD),
            "`daemon:` job 里没有 `{WD}` 了 —— 它可能被改了值、也可能被挪到了别的 job。\n\
             ⚠ **别只看「这行字在不在 ci.yml 里」** —— 它在别处照样在，而本模块要的是\n\
             「**跨 target check 所在的那个 job** 跑在 daemon 的 lock 下」。\n\
             这个前提一没，`the_two_lockfiles_have_no_real_version_conflict` 整套论证就落空，\n\
             该回来重判整条，而不是留着一条论证已经落空的判据。"
        );
        assert!(
            block.contains(CHECK),
            "跨 target Windows check 不在 `daemon:` job 里了（它是 `ci.yml` 自称的\n\
             「平台线唯一真判据」）。要么它被删了（那是个更大的问题），\n\
             要么它被搬进了别的 job —— 而别的 job 的 `working-directory` 不是\n\
             `remote-daemon-proto` ⇒ 它改走 **monitor 的 lock**，本模块整套论证反过来。\n\
             ⚠ 08-07 变异实测：这一刀之前本条是绿的。"
        );
    }

    /// **超集不算冲突** —— 这条防的是「把判据放宽/收紧成错的形状」。
    #[test]
    fn a_monitor_superset_is_not_a_conflict() {
        let m = parse_lock("src-tauri/Cargo.lock");
        let d = parse_lock("remote-daemon-proto/Cargo.lock");
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
}
