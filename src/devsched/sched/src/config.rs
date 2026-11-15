//! 主机表与项目配置（各键见 CONFIG.md）。认不得的键照留、不报错。

use std::collections::BTreeMap;
use std::io;
use std::path::{Path, PathBuf};

/// 去法。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Way {
    Local,
    /// `ssh:<ssh 名>`
    Ssh(String),
}

/// 主机表的一行。
#[derive(Debug, Clone, PartialEq)]
pub struct Host {
    pub name: String,
    pub slots: u32,
    pub way: Way,
    /// 选项照写的先后；没有 `=` 的一项值为空
    pub opts: Vec<(String, String)>,
}

impl Host {
    pub fn is_local(&self) -> bool {
        self.way == Way::Local
    }

    /// 远端那台的登记根（`base=`，绝对路径，去掉末尾的 `/`）；没写或不是绝对路径 ⇒ 说一句为什么。
    pub fn base(&self) -> Result<&str, String> {
        self.opt("base")
            .filter(|b| b.starts_with('/'))
            .map(|b| b.trim_end_matches('/'))
            .ok_or_else(|| format!("{} 没写 base=（绝对路径）", self.name))
    }

    pub fn opt(&self, k: &str) -> Option<&str> {
        self.opts
            .iter()
            .find(|(x, _)| x == k)
            .map(|(_, v)| v.as_str())
    }

    /// `放行/停` 一对；只写一个数 ⇒ 两个都是它。
    pub fn pair(&self, k: &str) -> Option<(f64, f64)> {
        let v = self.opt(k)?;
        let (a, b) = v.split_once('/').unwrap_or((v, v));
        Some((a.trim().parse().ok()?, b.trim().parse().ok()?))
    }

    pub fn num(&self, k: &str) -> Option<i64> {
        self.opt(k)?.trim().parse().ok()
    }

    /// 选项原样拼回一串（给 `Trust::from_host_opts`）。
    pub fn opts_string(&self) -> String {
        self.opts
            .iter()
            .map(|(k, v)| {
                if v.is_empty() && !k.contains('=') {
                    k.clone()
                } else {
                    format!("{k}={v}")
                }
            })
            .collect::<Vec<_>>()
            .join(" ")
    }
}

/// 主机表；行序 ＝ 挑的先后。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct HostTable {
    pub hosts: Vec<Host>,
    /// 认不出的行（位数不是数、去法不认得），照实记下
    pub skipped: Vec<String>,
}

impl HostTable {
    pub fn parse(text: &str) -> HostTable {
        let mut t = HostTable::default();
        for line in text.lines() {
            let l = line.trim();
            if l.is_empty() || l.starts_with('#') {
                continue;
            }
            let mut it = l.split_whitespace();
            let (Some(name), Some(slots), Some(way)) = (it.next(), it.next(), it.next()) else {
                t.skipped.push(line.to_string());
                continue;
            };
            let Ok(slots) = slots.parse::<u32>() else {
                t.skipped.push(line.to_string());
                continue;
            };
            let way = if way == "local" {
                Way::Local
            } else if let Some(d) = way.strip_prefix("ssh:").filter(|d| !d.is_empty()) {
                Way::Ssh(d.to_string())
            } else {
                t.skipped.push(line.to_string());
                continue;
            };
            let opts = it
                .map(|o| match o.split_once('=') {
                    Some((k, v)) => (k.to_string(), v.to_string()),
                    None => (o.to_string(), String::new()),
                })
                .collect();
            t.hosts.push(Host {
                name: name.to_string(),
                slots,
                way,
                opts,
            });
        }
        t
    }

    pub fn get(&self, name: &str) -> Option<&Host> {
        self.hosts.iter().find(|h| h.name == name)
    }

    pub fn local(&self) -> Option<&Host> {
        self.hosts.iter().find(|h| h.is_local())
    }
}

/// 一份项目配置。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Project {
    /// 文件名去掉 `.conf`
    pub name: String,
    pub keys: BTreeMap<String, String>,
}

impl Project {
    pub fn parse(name: &str, text: &str) -> Project {
        let mut keys = BTreeMap::new();
        for line in text.lines() {
            let l = line.trim_start();
            let Some((k, rest)) = l.split_once('=') else {
                continue;
            };
            let k = k.trim_end();
            let ok = k
                .chars()
                .next()
                .is_some_and(|c| c.is_ascii_lowercase() || c == '_')
                && k.chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_');
            if !ok {
                continue;
            }
            // 等号后面至多吃一个空白（见 CONFIG.md 项目配置），末尾空白去掉
            let v = rest.strip_prefix([' ', '\t']).unwrap_or(rest).trim_end();
            keys.insert(k.to_string(), v.to_string());
        }
        Project {
            name: name.to_string(),
            keys,
        }
    }

    /// 没写或空 ⇒ ""。
    pub fn get(&self, k: &str) -> &str {
        self.keys.get(k).map(|s| s.as_str()).unwrap_or("")
    }

    /// 一串词按空白拆（原样，不做路径展开）。
    pub fn list(&self, k: &str) -> Vec<String> {
        self.get(k)
            .split_whitespace()
            .map(|s| s.to_string())
            .collect()
    }

    /// `need_<能力>` 的那几个（能力名 → 正则原文），空值不算。
    pub fn needs(&self) -> Vec<(String, String)> {
        self.keys
            .iter()
            .filter_map(|(k, v)| {
                let cap = k.strip_prefix("need_")?;
                (cap != "own" && !v.is_empty()).then(|| (cap.to_string(), v.clone()))
            })
            .collect()
    }
}

/// 一个配置目录：`hosts` ＋ `projects/*.conf`。
#[derive(Debug, Clone)]
pub struct Config {
    pub dir: PathBuf,
    pub hosts: HostTable,
    /// 按文件名排
    pub projects: Vec<Project>,
}

impl Config {
    pub fn load(dir: &Path) -> io::Result<Config> {
        let hosts = HostTable::parse(&std::fs::read_to_string(dir.join("hosts")).map_err(|e| {
            io::Error::new(
                e.kind(),
                format!("读不了主机表 {}：{e}", dir.join("hosts").display()),
            )
        })?);
        let mut projects = Vec::new();
        if let Ok(rd) = std::fs::read_dir(dir.join("projects")) {
            let mut files: Vec<PathBuf> = rd
                .filter_map(|e| e.ok().map(|e| e.path()))
                .filter(|p| p.extension().is_some_and(|x| x == "conf"))
                .collect();
            files.sort();
            for f in files {
                let name = f
                    .file_stem()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_string();
                if let Ok(text) = std::fs::read_to_string(&f) {
                    projects.push(Project::parse(&name, &text));
                }
            }
        }
        Ok(Config {
            dir: dir.to_path_buf(),
            hosts,
            projects,
        })
    }

    /// 重读主机表（每一圈）；读不了就留着上一份。
    pub fn reload_hosts(&mut self) {
        if let Ok(text) = std::fs::read_to_string(self.dir.join("hosts")) {
            self.hosts = HostTable::parse(&text);
        }
    }
}

/// 配置目录：`--config` ＞ `DEVSCHED_CONFIG` ＞ `~/.config/devsched/`。
pub fn config_dir(flag: Option<&Path>, vars: &BTreeMap<String, String>) -> Option<PathBuf> {
    if let Some(p) = flag {
        return Some(p.to_path_buf());
    }
    if let Some(p) = vars.get("DEVSCHED_CONFIG").filter(|p| !p.is_empty()) {
        return Some(PathBuf::from(p));
    }
    vars.get("HOME")
        .filter(|h| !h.is_empty())
        .map(|h| Path::new(h).join(".config/devsched"))
}

#[cfg(test)]
pub(crate) fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixture_hosts_parse_like_the_bash_table() {
        let c = Config::load(&fixtures()).unwrap();
        let names: Vec<&str> = c.hosts.hosts.iter().map(|h| h.name.as_str()).collect();
        assert_eq!(names, ["alpha", "beta", "gamma", "delta", "local"]);
        assert_eq!(c.hosts.skipped.len(), 1, "位数不是数的那一行认不出");
        let delta = c.hosts.get("delta").unwrap();
        assert_eq!(delta.slots, 3);
        assert_eq!(delta.way, Way::Ssh("delta".into()));
        assert_eq!(delta.pair("mem"), Some((6.0, 2.0)));
        assert_eq!(delta.opt("futurekey"), Some("xyz"), "认不得的键照留");
        assert_eq!(delta.opt("caps"), Some("gui,node,tmux,tmuxnew"));
        assert!(delta.opts_string().contains("trust=own"));
        let alpha = c.hosts.get("alpha").unwrap();
        assert_eq!(alpha.opt("slurm"), Some("-p,cpu,-c,16,--mem=32G"));
        assert_eq!(alpha.pair("psi"), Some((25.0, 60.0)));
        assert!(c.hosts.local().unwrap().is_local());
        assert_eq!(c.hosts.local().unwrap().num("fallback"), Some(15));
    }

    #[test]
    fn fixture_projects_keep_values_verbatim() {
        let c = Config::load(&fixtures()).unwrap();
        let names: Vec<&str> = c.projects.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(names, ["alpha", "beta"]);
        let a = &c.projects[0];
        assert!(
            a.list("exclude").contains(&".env*".to_string()),
            "`.env*` 原样，不展开"
        );
        assert_eq!(a.get("seed"), "");
        assert_eq!(a.get("marker"), "tests/scripts/quick-check.sh");
        let needs: Vec<String> = a.needs().into_iter().map(|(k, _)| k).collect();
        assert_eq!(
            needs,
            ["cargo", "gui", "node", "tmuxnew"],
            "need_own 不是一项能力"
        );
        assert!(
            regex::Regex::new(a.get("need_cargo"))
                .unwrap()
                .is_match("/ cargo test -p x "),
            "cargo 的趟要那台报 cargo"
        );
        assert_eq!(a.get("need_own"), "bless|export_bindings");
        let b = &c.projects[1];
        assert_eq!(b.get("trailing"), "  spaced value");
        assert_eq!(b.get("remote_if"), "");
        assert_eq!(b.list("siblings"), ["/srv/beta-sib"]);
    }

    #[test]
    fn config_dir_precedence() {
        let mut v = BTreeMap::new();
        v.insert("HOME".to_string(), "/h".to_string());
        assert_eq!(
            config_dir(None, &v),
            Some(PathBuf::from("/h/.config/devsched"))
        );
        v.insert("DEVSCHED_CONFIG".to_string(), "/c".to_string());
        assert_eq!(config_dir(None, &v), Some(PathBuf::from("/c")));
        assert_eq!(
            config_dir(Some(Path::new("/f")), &v),
            Some(PathBuf::from("/f"))
        );
    }
}
