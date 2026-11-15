//! 带去远端的环境：调度不替作业设任何变量，只带项目配置 `env` 列的、调用方此刻设着的（空值照带，空 ＝ 不要）。

use crate::config::Project;
use crate::plan::Vars;
use std::collections::BTreeMap;

/// 指向一个包装程序的变量：那台没有就摘掉。
const WRAPPERS: &[&str] = &[
    "RUSTC_WRAPPER",
    "CARGO_BUILD_RUSTC_WRAPPER",
    "RUSTC_WORKSPACE_WRAPPER",
];
/// 指向一个程序、不是包装的变量：那台没有只说一句。
const PROGRAMS: &[&str] = &["RUSTC", "CC", "CXX", "AR", "LINKER"];

/// 项目配置 `env` 列的、调用方此刻设着的。
pub fn carried(project: &Project, vars: &Vars) -> BTreeMap<String, String> {
    project
        .list("env")
        .into_iter()
        .filter_map(|k| vars.get(&k).map(|v| (k, v.clone())))
        .collect()
}

/// 项目配置盖过调用方与那台 cargo 配置的：`sccache = 0` ⇒ `RUSTC_WRAPPER` 设成空（cargo 认空值为「不用包装」，
/// 盖过 `~/.cargo/config.toml` 的 rustc-wrapper）。不用 sccache 是正确性问题的项目（编进 `env!` 的路径进共享缓存会写坏别的树）写它。
/// 远端那一支与本机那一支都照这一份。
pub fn forced(project: &Project) -> BTreeMap<String, String> {
    let mut m = BTreeMap::new();
    if project.get("sccache").trim() == "0" {
        m.insert("RUSTC_WRAPPER".to_string(), String::new());
    }
    m
}

fn program_name(v: &str) -> &str {
    let first = v.split_whitespace().next().unwrap_or("");
    first.rsplit('/').next().unwrap_or(first)
}

/// 对那台 hello 报的程序表核一遍：包装类没有的摘掉，别的照实说一句。返回说的那几句。
pub fn check_programs(
    env: &mut BTreeMap<String, String>,
    programs: &[String],
    host: &str,
) -> Vec<String> {
    let mut notes = Vec::new();
    let has = |p: &str| programs.iter().any(|x| x == p);
    for k in WRAPPERS {
        if let Some(v) = env.get(*k).cloned() {
            if v.is_empty() {
                continue;
            }
            let p = program_name(&v);
            if !has(p) {
                env.remove(*k);
                notes.push(format!("{host} 上没有 {p}：{k} 不带过去"));
            }
        }
    }
    for k in PROGRAMS {
        if let Some(v) = env.get(*k) {
            let p = program_name(v);
            if !v.is_empty() && !has(p) {
                notes.push(format!("{host} 没报有 {p}（{k}={v} 照带）"));
            }
        }
    }
    notes
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{fixtures, Config};

    #[test]
    fn wrapper_missing_on_host_is_dropped_and_said() {
        let c = Config::load(&fixtures()).unwrap();
        let p = &c.projects[0]; // env = CARGO_INCREMENTAL RUSTC_WRAPPER CC
        let mut v = Vars::new();
        v.insert("RUSTC_WRAPPER".into(), "sccache".into());
        v.insert("CC".into(), "/usr/bin/clang".into());
        v.insert("CARGO_INCREMENTAL".into(), "".into());
        v.insert("SECRET_TOKEN".into(), "x".into());
        let mut env = carried(p, &v);
        assert_eq!(env.len(), 3, "只带 env 列的；空值照带");
        assert_eq!(env.get("CARGO_INCREMENTAL").map(|s| s.as_str()), Some(""));
        let notes = check_programs(&mut env, &["cc".to_string()], "alpha");
        assert!(!env.contains_key("RUSTC_WRAPPER"));
        assert!(env.contains_key("CC"), "不是包装的照带");
        assert_eq!(notes.len(), 2, "{notes:?}");
        assert!(notes[0].contains("sccache"));
        // 那台有 ⇒ 照带、不说
        let mut env = carried(p, &v);
        let notes = check_programs(
            &mut env,
            &["sccache".to_string(), "clang".to_string()],
            "alpha",
        );
        assert!(env.contains_key("RUSTC_WRAPPER") && notes.is_empty());
    }

    #[test]
    fn nothing_added_when_caller_set_nothing() {
        let c = Config::load(&fixtures()).unwrap();
        assert!(carried(&c.projects[0], &Vars::new()).is_empty());
        // 空值的包装 ＝ 不要包装：照带、不核
        let mut v = Vars::new();
        v.insert("RUSTC_WRAPPER".into(), "".into());
        let mut env = carried(&c.projects[0], &v);
        assert!(check_programs(&mut env, &[], "alpha").is_empty());
        assert_eq!(env.get("RUSTC_WRAPPER").map(|s| s.as_str()), Some(""));
    }
}
