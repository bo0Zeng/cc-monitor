//! **每个 shell 脚本要么进 shellcheck，要么登记为什么不进**〔audit-0805 08-08，Phase G 第 71 件〕。
//!
//! # 洞：人群是手写分组，地板只会数数
//!
//! `ci.yml` 的 shellcheck 步骤用一条手写的 `FILES=$(printf …)` 取人群
//!（`e2e/*.sh` · `shared/cc-bus/scripts/*` · `shared/ccm` · `scripts/*.sh` + vendored 四个），
//! 后面跟一条**计数地板**（`[ "$N" -ge <数> ]`）。⚠ 那个数**刻意不抄在这里** ——
//! 它的家是 `ci.yml`，而下面第二条判据每次都去读它；抄一份在注释里，
//! 下次棘紧时这里就成了本区一直在治的那种过期散文。两件事它都挡不住：
//!
//! - **新脚本落在任何一个分组之外 ⇒ 静默不被 lint**，而计数一个都不少 ⇒ 地板照过。
//!   08-08 实测：全仓 46 个 shell 脚本，那条表达式覆盖 44 —— 漏的是
//!   `e2e/fake-claude`（e2e 的 claude shim，那一组的 glob 是 `e2e/*.sh`，它没有后缀）
//!   与 `shared/ccm-aliases.sh`。
//! - **地板会落后**：它自己的注释逐字承认「这已经是同一条地板**第三次**落后
//!   （37→39→41 每次都是事后补）」。⇒ 本模块把地板钉成**等于**今天真实覆盖数，
//!   而不是「≥」：落后这件事从此当场红。
//!
//! # 「刻意不含」不能只是散文（E12）
//!
//! `shared/ccm-aliases.sh` 的排除是**有理由的、先核过的**：它是供 `source` 的片段、
//! 没有 shebang（SC2148 是它的构造性属性），而它会被写进用户 shell profile、
//! 还在 UI 面板里展示供手动复制 —— 为过 lint 往里塞 `# shellcheck shell=bash`
//! 等于往用户配置和界面文案里掺 lint 噪音。理由成立，**但它只写在 `ci.yml` 的注释里**。
//! 本模块把它登记成一条**豁免**：默认拒绝，豁免要写理由，且豁免行不许变成死行。
//!
//! ⚠ 如实记一笔量到的事：`shellcheck -s bash shared/ccm-aliases.sh` 今天**零 error**
//! —— 也就是说那条豁免是**可以撤销**的（代价是上面说的用户可见噪音）。
//! 写在这里是为了让下一个人不必重量一次，**不是**在建议撤销。

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    /// **刻意不进 shellcheck 的脚本**（路径, 为什么）。
    ///
    /// 默认拒绝：不在这里、又不被 CI 那条表达式覆盖的脚本，正题判据会点名。
    const EXEMPT: &[(&str, &str)] = &[(
        "shared/ccm-aliases.sh",
        "供 source 的片段、无 shebang（SC2148 是构造性属性）；它会被写进用户 shell profile \
         并在 UI 面板里展示供手动复制 ⇒ 塞 `# shellcheck shell=bash` 等于往用户配置与界面文案里掺 lint 噪音",
    )];

    fn repo_root() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("仓根")
            .to_path_buf()
    }

    /// 从 `e2e-smoke` job 里抠出那条 `FILES=$(printf …)` 的**各个 pattern**。
    ///
    /// ⚠ 用 `ci_yaml::job_block`（E3：`ci.yml` 的读取与切块只有一个家），它**剔注释** ——
    /// 这里非剔不可：同一段注释里逐字写着 `scripts/*.sh`、`src-tauri/vendor/.../scripts/**`
    /// 这些 pattern，整份 `contains` 会把注释里的写法当成真的在扫。
    fn shellcheck_patterns() -> Vec<String> {
        let block = crate::shared_crate_registry::ci_yaml::job_block("e2e-smoke");
        let mut out = Vec::new();
        let mut in_files = false;
        for line in block.lines() {
            let t = line.trim();
            if let Some(rest) = t.strip_prefix("FILES=$(printf '%s\\n'") {
                in_files = true;
                push_tokens(rest, &mut out);
                if !t.ends_with('\\') {
                    break;
                }
                continue;
            }
            if in_files {
                push_tokens(t, &mut out);
                if !t.ends_with('\\') {
                    break;
                }
            }
        }
        out
    }

    fn push_tokens(s: &str, out: &mut Vec<String>) {
        for tok in s.split_whitespace() {
            let tok = tok.trim_end_matches(')').trim_end_matches('\\');
            if !tok.is_empty() {
                out.push(tok.to_string());
            }
        }
    }

    /// bash 在**不开 globstar** 时的匹配语义：`*` 不跨 `/`。
    ///
    /// ⚠ 这一点不是细节 —— `ci.yml` 那段注释逐字记着：写成 `.../scripts/**` 时
    /// `**` 等价于 `*`，会把目录喂给 shellcheck 而恒红。判据要和它**同一套语义**，
    /// 否则我这边算出的「覆盖」和 CI 真扫的不是一回事。
    fn matches(pattern: &str, path: &str) -> bool {
        let (ps, xs): (Vec<&str>, Vec<&str>) =
            (pattern.split('/').collect(), path.split('/').collect());
        if ps.len() != xs.len() {
            return false;
        }
        ps.iter()
            .zip(xs.iter())
            .all(|(p, x)| match p.split_once('*') {
                None => *p == *x,
                Some((pre, suf)) => {
                    x.len() >= pre.len() + suf.len() && x.starts_with(pre) && x.ends_with(suf)
                }
            })
    }

    fn covered(patterns: &[String], path: &str) -> bool {
        patterns.iter().any(|p| matches(p, path))
    }

    /// CI 那条计数地板写的数（`[ "$N" -ge <数> ]` 那一行）。
    fn floor_in_ci() -> usize {
        let block = crate::shared_crate_registry::ci_yaml::job_block("e2e-smoke");
        let line = block
            .lines()
            .find(|l| l.contains("-ge") && l.contains("覆盖面缩水"))
            .unwrap_or_else(|| {
                panic!("`e2e-smoke` 里找不到那条计数地板（含 `-ge` 与「覆盖面缩水」）—— 形状变了，本模块两条判据都会零命中地绿")
            });
        let after = line.split("-ge").nth(1).expect("地板行没有 -ge 右侧");
        after
            .split_whitespace()
            .next()
            .and_then(|s| s.trim_matches(|c: char| !c.is_ascii_digit()).parse().ok())
            .unwrap_or_else(|| panic!("地板行解析不出数字：{line}"))
    }

    /// ★ 正题：**每个 shell 脚本要么被 shellcheck 扫到，要么登记豁免**。
    #[test]
    fn every_shell_script_is_either_linted_or_registered_as_exempt() {
        let scripts = guard_core::shell_scripts(&repo_root());
        let patterns = shellcheck_patterns();
        // 抽取器自检：任何一头空了，下面那条对拍都会零命中地绿。
        assert!(
            scripts.len() >= 40,
            "全仓只扫到 {} 个 shell 脚本（08-08 实测 46）—— 遍历口径坏了，本条会零命中地绿",
            scripts.len()
        );
        assert!(
            patterns.len() >= 5,
            "从 `e2e-smoke` 只抠到 {} 个 pattern（08-08 实测 8）—— `FILES=$(printf …)` 的写法变了，\
             本条会把**所有**脚本判成「没被扫」，或者（更坏）把地板那条判成 0。抽取器先修",
            patterns.len()
        );

        let unlinted: Vec<&String> = scripts
            .iter()
            .filter(|s| !covered(&patterns, s))
            .filter(|s| !EXEMPT.iter().any(|(p, _)| *p == s.as_str()))
            .collect();
        assert!(
            unlinted.is_empty(),
            "这些 shell 脚本既不在 CI 的 shellcheck 表达式里、也没登记豁免：\n{}\n\n\
             ★ 它们**一次都没被 lint 过**，而 CI 那条计数地板一个都不少 ⇒ 照旧绿\n\
             （地板只会数它扫到的那些，数不出它没扫的那些）。\n\
             两条路：① 补进 `.github/workflows/ci.yml` 的 `FILES=$(printf …)`（记得同步地板数），\n\
             或 ② 在本文件的 `EXEMPT` 里登记，**并写清为什么** —— \n\
             「刻意不含」写在 `ci.yml` 的注释里不算，那正是本区在治的病（E12）。",
            unlinted
                .iter()
                .map(|s| format!("  {s}"))
                .collect::<Vec<_>>()
                .join("\n")
        );
    }

    /// ★ **地板必须等于今天真实覆盖数**，不是「≥」。
    ///
    /// 那条地板自己的注释逐字承认落后过三次（37→39→41 每次事后补），
    /// 而落后期间「可以少扫几个文件而照样绿」。「≥」这个形状是落后的**成因**：
    /// 加脚本时它不响，于是没人回来棘。改成等号之后，加一个脚本就必须在同一次改动里
    /// 把数写对 —— 诊断会直接告诉他该写几。
    #[test]
    fn the_coverage_floor_equals_what_is_actually_covered_today() {
        let scripts = guard_core::shell_scripts(&repo_root());
        let patterns = shellcheck_patterns();
        let n = scripts.iter().filter(|s| covered(&patterns, s)).count();
        assert!(
            n >= 40,
            "算出的覆盖数只有 {n}（08-08 实测 45）—— 匹配语义坏了，本条会去比一个假数"
        );
        let floor = floor_in_ci();
        assert_eq!(
            floor, n,
            "CI 的 shellcheck 计数地板写着 {floor}，而那条表达式今天真实覆盖 {n} 个文件。\n\
             ⇒ 把 `ci.yml` 里那条 `[ \"$N\" -ge {floor} ]` 改成 {n}，并按它自己的规矩\n\
             「棘的时候把**实测构成**一起写下，别只改数字」。\n\
             ⚠ 数字比实际小 = 那段时间可以少扫几个文件而门禁照样绿（它自己记着这事发生过三次）；\n\
             数字比实际大 = CI 会红在一条与真实原因无关的诊断上。"
        );
    }

    /// **豁免行不许变成死行，也不许是废话**（反向锚点）。
    #[test]
    fn every_exemption_still_points_at_a_real_unlinted_script() {
        let scripts = guard_core::shell_scripts(&repo_root());
        let patterns = shellcheck_patterns();
        for (path, why) in EXEMPT {
            assert!(
                scripts.contains(&path.to_string()),
                "`EXEMPT` 里登记着 `{path}`，而全仓扫不到这个 shell 脚本 —— \
                 它被删了或改名了 ⇒ 删掉这一行，别让豁免表长成一张没人看的旧账"
            );
            assert!(
                !covered(patterns.as_slice(), path),
                "`{path}` 登记着豁免，可 CI 那条表达式**已经在扫它了** ⇒ 删掉这条豁免。\n\
                 留着的害处是具体的：下一个人会以为这个文件没被 lint，\
                 从而不敢改它 / 或者以为「反正没人扫」而放松它"
            );
            assert!(
                why.len() >= 20,
                "`{path}` 的豁免理由只有 {} 个字节 —— 太短的理由等于没有理由。\
                 写清楚「为什么这个文件不该被 lint」，不是「暂时不弄」",
                why.len()
            );
        }
    }
}
