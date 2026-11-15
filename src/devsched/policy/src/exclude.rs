//! 排除清单：项目配置 `exclude` 那一串，加上删不掉的内置一组。

use std::path::{Component, Path};

/// 删不掉的内置一组：私人目录与凭据类。项目配置只能往上加。
const BUILTIN: &[&str] = &[
    ".planned-build/",
    ".claude/",
    ".cc-monitor/",
    ".ssh/",
    ".env*",
    "*.key",
    "*.pem",
    "*.p12",
    // ssh 私钥（连 `.pub` 等后缀）；不写 `id_*`：那会连 `src/id_map.rs` 这种源文件一起排掉。
    "id_rsa*",
    "id_dsa*",
    "id_ecdsa*",
    "id_ed25519*",
    "credentials*",
    "*.secret",
];

/// 一项模式：`/` 起头或中间带 `/` ⇒ 从树根逐段比；否则只比最后一段、哪一层都认。
#[derive(Debug, Clone)]
struct Pattern {
    anchored: bool,
    dir_only: bool,
    segs: Vec<Vec<char>>,
}

impl Pattern {
    fn new(raw: &str) -> Pattern {
        let (anchored_prefix, s) = match raw.strip_prefix('/') {
            Some(rest) => (true, rest),
            None => (false, raw),
        };
        let (dir_only, s) = match s.strip_suffix('/') {
            Some(rest) => (true, rest),
            None => (false, s),
        };
        // 多写的斜杠照少写算。一段都不剩（`/` · `//`），或有永远比不中的段（`.` · `..` · 带 `**`）
        // ⇒ 段清空 ＝ 排掉整棵：写错宁可什么都不送。
        let mut segs: Vec<Vec<char>> = s
            .split('/')
            .filter(|g| !g.is_empty())
            .map(|g| g.chars().collect())
            .collect();
        let unmatchable = |g: &Vec<char>| {
            g[..] == ['.'] || g[..] == ['.', '.'] || g.windows(2).any(|w| w == ['*', '*'])
        };
        if segs.iter().any(unmatchable) {
            segs.clear();
        }
        Pattern {
            anchored: anchored_prefix || segs.len() > 1,
            dir_only,
            segs,
        }
    }

    /// 这一条路径（逐段）本身中不中；祖先由调用方逐层问。
    fn hits(&self, comps: &[Vec<char>], is_dir: bool) -> bool {
        if self.segs.is_empty() {
            return true;
        }
        if self.dir_only && !is_dir {
            return false;
        }
        if self.anchored {
            comps.len() == self.segs.len() && self.segs.iter().zip(comps).all(|(p, c)| glob(p, c))
        } else {
            comps.last().is_some_and(|c| glob(&self.segs[0], c))
        }
    }
}

/// 一段之内的通配：`*` 任意多个字符，`?` 恰一个；别的照字面。
fn glob(p: &[char], s: &[char]) -> bool {
    let (mut pi, mut si) = (0, 0);
    let mut back: Option<(usize, usize)> = None;
    while si < s.len() {
        if pi < p.len() && (p[pi] == '?' || (p[pi] != '*' && p[pi] == s[si])) {
            pi += 1;
            si += 1;
        } else if pi < p.len() && p[pi] == '*' {
            back = Some((pi, si));
            pi += 1;
        } else if let Some((bp, bs)) = back {
            pi = bp + 1;
            si = bs + 1;
            back = Some((bp, bs + 1));
        } else {
            return false;
        }
    }
    p[pi..].iter().all(|&c| c == '*')
}

/// 排除清单：内置一组加上项目配置那一串。
#[derive(Debug, Clone)]
pub struct ExcludeList {
    patterns: Vec<Pattern>,
}

impl ExcludeList {
    /// 照项目配置 `exclude` 那一串建清单：按空白拆，每项原样当模式（不做路径展开）。
    pub fn parse(spec: &str) -> ExcludeList {
        let patterns = BUILTIN
            .iter()
            .copied()
            .chain(spec.split_whitespace())
            .map(Pattern::new)
            .collect();
        ExcludeList { patterns }
    }

    /// `rel`（相对树根）排不排掉：它自己或它的哪一层上级目录中了一项，就排掉。
    /// 带 `..` · 绝对路径这类不是树里相对路径的、名字里带控制字符（换行之类）的，一律当排掉。
    /// 问树根本身（空路径）只有排掉整棵的那种项中。
    pub fn is_excluded(&self, rel: &Path, is_dir: bool) -> bool {
        let mut comps: Vec<Vec<char>> = Vec::new();
        for c in rel.components() {
            match c {
                Component::Normal(s) => {
                    let name: Vec<char> = s.to_string_lossy().chars().collect();
                    if name.iter().any(|c| c.is_control()) {
                        return true;
                    }
                    comps.push(name);
                }
                Component::CurDir => {}
                _ => return true,
            }
        }
        if comps.is_empty() {
            return self.patterns.iter().any(|p| p.segs.is_empty());
        }
        (1..=comps.len()).any(|n| {
            let dir = n < comps.len() || is_dir;
            self.patterns.iter().any(|p| p.hits(&comps[..n], dir))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ex(spec: &str, rel: &str, is_dir: bool) -> bool {
        ExcludeList::parse(spec).is_excluded(Path::new(rel), is_dir)
    }

    #[test]
    fn slash_prefix_anchors_to_root_only() {
        assert!(ex("/accounts", "accounts", true));
        assert!(ex("/accounts", "accounts/a.json", false));
        assert!(!ex("/accounts", "src/accounts", true));
        assert!(!ex("/accounts", "src/accounts/a.json", false));
    }

    #[test]
    fn unanchored_matches_at_any_depth() {
        assert!(ex("node_modules", "node_modules", true));
        assert!(ex("node_modules", "web/node_modules", true));
        assert!(ex("node_modules", "web/node_modules/x/y.js", false));
        assert!(!ex("node_modules", "web/node_modules2", true));
    }

    #[test]
    fn slash_suffix_matches_dirs_only() {
        assert!(ex("build/", "a/build", true));
        assert!(ex("build/", "a/build/out.o", false));
        assert!(!ex("build/", "a/build", false));
        assert!(!ex("build/", "build", false));
    }

    #[test]
    fn star_and_question_stay_within_one_segment() {
        assert!(ex("/a*b", "axyb", false));
        assert!(!ex("/a*b", "ax/yb", false));
        assert!(ex("/src/*.rs", "src/x.rs", false));
        assert!(!ex("/src/*.rs", "src/sub/x.rs", false));
        assert!(ex("f?o", "d/fxo", false));
        assert!(!ex("f?o", "d/fo", false));
        assert!(!ex("f?o", "d/fxxo", false));
    }

    #[test]
    fn middle_slash_anchors_to_root() {
        assert!(ex("api/secret.txt", "api/secret.txt", false));
        assert!(!ex("api/secret.txt", "x/api/secret.txt", false));
    }

    #[test]
    fn project_listed_dir_excluded_whole() {
        let list = ExcludeList::parse(".git target");
        for p in [".git", "sub/.git", "target"] {
            assert!(list.is_excluded(Path::new(p), true), "{p}");
        }
        assert!(list.is_excluded(Path::new(".git/objects/ab/cd"), false));
        assert!(list.is_excluded(Path::new("target/debug/x"), false));
        assert!(!list.is_excluded(Path::new("src/target.rs"), false));
    }

    #[test]
    fn builtins_always_present_and_cannot_be_removed() {
        // 空串、只写项目自己的、写成「取反」样子的，内置一组都还在。
        for spec in ["", "target", "!.env* !.planned-build/ !.ssh/"] {
            let list = ExcludeList::parse(spec);
            for (p, d) in [
                (".planned-build", true),
                ("x/.claude", true),
                (".cc-monitor/a", false),
                ("home/.ssh/config", false),
                (".env", false),
                ("api/.env.local", false),
                ("k/server.key", false),
                ("c.pem", false),
                ("c.p12", false),
                ("id_ed25519", false),
                ("credentials.json", false),
                ("a/b.secret", false),
            ] {
                assert!(list.is_excluded(Path::new(p), d), "spec={spec:?} path={p}");
            }
            assert!(
                !list.is_excluded(Path::new("src/a.rs"), false),
                "spec={spec:?}"
            );
        }
    }

    #[test]
    fn builtin_ssh_keys_only_not_any_id_prefix() {
        // 内置只认 ssh 私钥那几种名字（连 `.pub` 等后缀）；`id_` 起头的源文件照送。
        for p in [
            "id_rsa",
            "id_rsa.pub",
            "id_dsa",
            "home/id_ecdsa",
            "id_ed25519",
            "id_ed25519_sk",
        ] {
            assert!(ex("", p, false), "{p}");
        }
        for p in ["src/id_map.rs", "id_card.txt", "lib/id_gen"] {
            assert!(!ex("", p, false), "{p}");
        }
    }

    #[test]
    fn builtin_dirs_are_dir_only() {
        // `.claude/` 是目录项：叫这个名字的文件不算。
        assert!(!ex("", "docs/.claude", false));
    }

    #[test]
    fn empty_segments_collapse_and_bare_slash_excludes_all() {
        // 多写的斜杠照少写算；只剩斜杠的一项排掉整棵（写错宁可什么都不送）。
        assert!(ex("/api//x.txt", "api/x.txt", false));
        assert!(ex("a//b/", "a/b", true));
        assert!(ex("/", "src/a.rs", false));
        assert!(ex("//", "a", true));
    }

    #[test]
    fn uninterpretable_patterns_exclude_all() {
        // 带 `.` · `..` 段或 `**` 的项照这里的写法永远比不中，写错宁可什么都不送。
        for spec in ["./secrets/", "src/../keys", "**/secret.txt", "a/**"] {
            assert!(ex(spec, "src/a.rs", false), "{spec}");
        }
    }

    #[test]
    fn root_itself_excluded_by_bare_slash() {
        assert!(ex("/", "", true));
        assert!(ex("/", ".", true));
        assert!(!ex("", "", true));
    }

    #[test]
    fn names_with_control_chars_are_excluded() {
        // 带换行的名字按行读清单时会被拆成两项。
        assert!(ex("", "ok\n.env", false));
        assert!(ex("", "d\tx/a.rs", false));
        assert!(ex("", "a\u{7f}", false));
    }

    #[test]
    fn non_plain_paths_are_excluded() {
        // 公开口子收到 `..` · 绝对路径这类不是树里相对路径的，一律当排掉。
        assert!(ex("/accounts", "x/../accounts", true));
        assert!(ex("", "../src/a.rs", false));
        assert!(ex("", "/src/a.rs", false));
        assert!(!ex("", "./src/a.rs", false));
    }

    #[test]
    fn spec_items_split_on_any_whitespace() {
        let list = ExcludeList::parse("  a\tb\n\nc  ");
        for p in ["a", "b", "c"] {
            assert!(list.is_excluded(Path::new(p), false), "{p}");
        }
        assert!(!list.is_excluded(Path::new("d"), false));
    }
}
