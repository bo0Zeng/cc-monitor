//! 扫源码守规矩的判据（规则）的唯一入口：规则只交「看哪片 · 找什么 · 登记了什么」，遍历与判法都在这里。
//!
//! - [`repo`]：仓里的文件清单问 git 一次（含还没 `git add` 的新文件），每份文件第一次被看时读、之后复用；生产段第一次用到时剥、之后复用，剥完自带「剥干净没有」的反向自检。
//! - 取人群（[`Repo::under`] · [`Repo::crate_tree`]）默认摘掉本目录，判据读不到自己；判别判据的规则显式写 [`Repo::with_rules`]。人群为空当场红。
//! - [`equal`] / [`none`]：找到的集合与登记表两向相等、零命中带正控，由这里判、这里印两边各多什么。
//! - [`rule`]：每条规则跑完把耗时印进 `--nocapture`（快照建了多久另印一行）。
//!
//! 规则不许自己读盘：本 crate 的 `clippy.toml` 禁了读盘那几个口，本模块里只有 [`disk`] 那一块放行。
//! 生成物当事实来源的规则读的是盘上那一份：只跑本 crate 时它可能是旧的，要先跑后端测试才新。

#![deny(clippy::disallowed_methods)]

use std::collections::BTreeSet;
use std::fmt::Debug;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::Instant;

#[path = "apikey_rules_one_home.rs"]
mod apikey_rules_one_home;
#[path = "bind_guard.rs"]
mod bind_guard;
#[path = "cc_bus_one_home.rs"]
mod cc_bus_one_home;
#[path = "creds_store_exits.rs"]
mod creds_store_exits;
#[path = "filewin_panic_points.rs"]
mod filewin_panic_points;
#[path = "raw_said_guard.rs"]
mod raw_said_guard;
#[path = "search_kou_jing_guard.rs"]
mod search_kou_jing_guard;
#[path = "strip_clean.rs"]
mod strip_clean;
#[path = "structural_scan.rs"]
mod structural_scan;

#[path = "mod_tests.rs"]
mod tests;

/// 规则住的目录（仓根相对）：取人群时默认摘掉。
pub const RULES_DIR: &str = "tests/common/guard-core/rules/";

/// 读盘只在这一块。
mod disk {
    #![allow(clippy::disallowed_methods)]

    use std::path::Path;

    /// 仓里的文件清单：git 跟踪着的 ＋ 还没 `git add`、也没被忽略的；工作树里已删的不算。仓根相对、正斜杠。
    pub(super) fn listing(root: &Path) -> Vec<String> {
        let out = std::process::Command::new("git")
            .args([
                "ls-files",
                "-z",
                "--cached",
                "--others",
                "--exclude-standard",
            ])
            .current_dir(root)
            .output()
            .unwrap_or_else(|e| panic!("起不来 `git ls-files`（{e}）—— 判不了"));
        assert!(
            out.status.success(),
            "`git ls-files` 非零退出：{}",
            String::from_utf8_lossy(&out.stderr)
        );
        let mut v: Vec<String> = String::from_utf8_lossy(&out.stdout)
            .split('\0')
            .filter(|rel| !rel.is_empty() && root.join(rel).is_file())
            .map(str::to_string)
            .collect();
        v.sort();
        v.dedup();
        v
    }

    /// 一份文件的原文；不是 UTF-8 ⇒ `None`（不是语料）。读不动 ⇒ panic。
    pub(super) fn text(path: &Path) -> Option<String> {
        let bytes = std::fs::read(path).unwrap_or_else(|e| panic!("读 {path:?} 失败: {e}"));
        String::from_utf8(bytes).ok()
    }
}

/// 仓里的一份文件。
pub struct File {
    /// 仓根相对、正斜杠。
    pub rel: String,
    abs: PathBuf,
    text: OnceLock<Option<String>>,
    prod: OnceLock<String>,
}

impl File {
    fn new(root: &Path, rel: String) -> File {
        File {
            abs: root.join(&rel),
            rel,
            text: OnceLock::new(),
            prod: OnceLock::new(),
        }
    }

    fn text_opt(&self) -> Option<&str> {
        self.text.get_or_init(|| disk::text(&self.abs)).as_deref()
    }

    /// 原文。不是文本的文件 panic（取人群时已经筛掉）。
    pub fn text(&self) -> &str {
        self.text_opt()
            .unwrap_or_else(|| panic!("{} 不是 UTF-8 文本", self.rel))
    }

    /// 生产段（`.rs`）：剥掉注释与测试段。住 `tests/` 下的整份是测试段 ⇒ 空串。
    /// 第一次剥完当场自检：剩下的里面不许还有测试属性、不许还有剥法认不出的测试模块。
    pub fn prod(&self) -> &str {
        assert!(
            self.rel.ends_with(".rs"),
            "{} 不是 `.rs`，没有生产段这一说",
            self.rel
        );
        self.prod.get_or_init(|| {
            if self.rel.starts_with("tests/") {
                return String::new();
            }
            let text = self.text();
            let prod = crate::production_code(text);
            crate::assert_no_test_code(&self.rel, &prod);
            crate::assert_no_unstripped_test_module(&self.rel, &prod);
            crate::assert_test_module_ranges_are_brace_balanced(&self.rel, text);
            prod
        })
    }
}

/// 仓的一份快照。
pub struct Repo {
    files: Vec<File>,
}

static REPO: OnceLock<Repo> = OnceLock::new();

/// 本进程那一份快照（第一次用到时问 git，印一行建了多久）。
pub fn repo() -> &'static Repo {
    REPO.get_or_init(|| {
        let t = Instant::now();
        let r = Repo::at(&crate::addr::repo_root());
        eprintln!(
            "快照：{} 份文件 · {} ms",
            r.files.len(),
            t.elapsed().as_millis()
        );
        r
    })
}

/// 落在 `dir`（仓根相对，不带尾斜杠也行）这棵树里。
fn is_under(rel: &str, dir: &str) -> bool {
    let dir = dir.trim_end_matches('/');
    dir.is_empty() || rel.strip_prefix(dir).is_some_and(|r| r.starts_with('/'))
}

fn ext_ok(rel: &str, exts: &[&str]) -> bool {
    exts.is_empty()
        || Path::new(rel)
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| exts.contains(&e))
}

impl Repo {
    fn at(root: &Path) -> Repo {
        Repo::from_listing(root, disk::listing(root))
    }

    fn from_listing(root: &Path, rels: Vec<String>) -> Repo {
        Repo {
            files: rels.into_iter().map(|r| File::new(root, r)).collect(),
        }
    }

    /// 合成的快照（入口自己的判据用）：文本现给，不读盘。
    fn synthetic(files: &[(&str, &str)]) -> Repo {
        let mut files: Vec<File> = files
            .iter()
            .map(|(rel, text)| {
                let f = File::new(Path::new("/合成"), rel.to_string());
                f.text.get_or_init(|| Some(text.to_string()));
                f
            })
            .collect();
        files.sort_by(|a, b| a.rel.cmp(&b.rel));
        Repo { files }
    }

    /// 点名的一份文件；不在仓里 ⇒ 红（住址坏了，用它的规则判不了）。
    pub fn file(&self, rel: &str) -> &File {
        self.find(rel)
            .unwrap_or_else(|| panic!("仓里没有 `{rel}` —— 住址坏了"))
    }

    /// 仓里有没有这份文件。
    pub fn has(&self, rel: &str) -> bool {
        self.find(rel).is_some()
    }

    fn find(&self, rel: &str) -> Option<&File> {
        self.files
            .binary_search_by(|f| f.rel.as_str().cmp(rel))
            .ok()
            .map(|i| &self.files[i])
    }

    fn has_dir(&self, dir: &str) -> bool {
        self.files.iter().any(|f| is_under(&f.rel, dir))
    }

    /// `dirs` 这几棵树里后缀在 `exts` 里（空 = 不筛）的文本文件；本目录摘掉。人群为空 ⇒ 红。
    pub fn under(&self, dirs: &[&str], exts: &[&str]) -> Vec<&File> {
        self.pick(dirs, exts, false)
    }

    /// 连本目录一起看（判别判据的规则用）。
    pub fn with_rules(&self) -> WithRules<'_> {
        WithRules(self)
    }

    fn pick(&self, dirs: &[&str], exts: &[&str], rules: bool) -> Vec<&File> {
        let out: Vec<&File> = self
            .files
            .iter()
            .filter(|f| dirs.iter().any(|d| is_under(&f.rel, d)))
            .filter(|f| rules || !is_under(&f.rel, RULES_DIR))
            .filter(|f| ext_ok(&f.rel, exts))
            .filter(|f| f.text_opt().is_some())
            .collect();
        assert!(
            !out.is_empty(),
            "{dirs:?} 下没有后缀 {exts:?} 的文件 —— 住址坏了，用它的规则此刻在空人群上恒绿"
        );
        out
    }

    /// 一个包的源码人群：它的源码树（有 `src/` 取 `src/`，否则取包目录），
    /// ＋ manifest `[package.metadata.guard] population` 明写的兄弟包，＋ 由 `#[path]` 挂进来、住在根外的生产文件
    /// （挂的是 `mod.rs` 就连那一层目录）。指向 `tests` 段的挂载不跟。口径与 [`crate::scan_tree_excluding`] 那一趟遍历相同。
    pub fn crate_tree(&self, pkg: &str, exts: &[&str]) -> Vec<&File> {
        let pkg = pkg.trim_end_matches('/');
        let src_of = |dir: &str| -> String {
            let src = format!("{dir}/src");
            if self.has_dir(&src) {
                return src;
            }
            self.file(&format!("{dir}/lib.rs"));
            dir.to_string()
        };
        let main = src_of(pkg);
        let mut roots = vec![main.clone()];
        let manifest = self.file(&format!("{pkg}/Cargo.toml")).text();
        for rel in crate::population_decl(manifest, Path::new(pkg)) {
            let dir = crate::lexically_normalized(&Path::new(pkg).join(&rel))
                .to_string_lossy()
                .replace('\\', "/");
            self.file(&format!("{dir}/Cargo.toml"));
            roots.push(src_of(&dir));
        }
        let rs = exts.is_empty() || exts.contains(&"rs");
        let mut seen: BTreeSet<&str> = BTreeSet::new();
        let mut out: Vec<&File> = Vec::new();
        for f in self.pick(
            &roots.iter().map(String::as_str).collect::<Vec<_>>(),
            exts,
            false,
        ) {
            if seen.insert(&f.rel) {
                out.push(f);
            }
        }
        let mut i = 0;
        while rs && i < out.len() {
            let host = out[i];
            i += 1;
            if !host.rel.ends_with(".rs") {
                continue;
            }
            let dir = Path::new(&host.rel).parent().unwrap_or(Path::new(""));
            for target in host.text().lines().filter_map(crate::path_attr) {
                if crate::mounts_tests(target) {
                    continue;
                }
                let t = crate::lexically_normalized(&dir.join(target))
                    .to_string_lossy()
                    .replace('\\', "/");
                if is_under(&t, &main)
                    || self.find(&t).is_none()
                    || !seen.insert(self.file(&t).rel.as_str())
                {
                    continue;
                }
                let found: Vec<&File> = if t.ends_with("/mod.rs") {
                    let tdir = &t[..t.len() - "/mod.rs".len()];
                    self.files
                        .iter()
                        .filter(|f| is_under(&f.rel, tdir) && !is_under(&f.rel, RULES_DIR))
                        .filter(|f| ext_ok(&f.rel, exts) && f.text_opt().is_some())
                        .collect()
                } else {
                    vec![self.file(&t)]
                };
                for f in found {
                    if f.rel == t || seen.insert(&f.rel) {
                        out.push(f);
                    }
                }
            }
        }
        out.sort_by(|a, b| a.rel.cmp(&b.rel));
        out.dedup_by(|a, b| a.rel == b.rel);
        out
    }
}

/// [`Repo::with_rules`] 的产物：取人群时不摘本目录。
pub struct WithRules<'a>(&'a Repo);

impl<'a> WithRules<'a> {
    pub fn under(&self, dirs: &[&str], exts: &[&str]) -> Vec<&'a File> {
        self.0.pick(dirs, exts, true)
    }
}

/// 找到的与登记的两向相等。登记表不许是空的（零命中的规则用 [`none`]，它要正控）。
#[track_caller]
pub fn equal<T: Ord + Debug>(
    what: &str,
    found: impl IntoIterator<Item = T>,
    registered: impl IntoIterator<Item = T>,
) {
    let found: BTreeSet<T> = found.into_iter().collect();
    let registered: BTreeSet<T> = registered.into_iter().collect();
    assert!(
        !registered.is_empty(),
        "{what}：登记表是空的 —— 零命中的规则用 `none`（带正控），不用空表比"
    );
    let extra: Vec<&T> = found.difference(&registered).collect();
    let gone: Vec<&T> = registered.difference(&found).collect();
    assert!(
        extra.is_empty() && gone.is_empty(),
        "{what}：找到的与登记的对不上。\n  找到、没登记的：{extra:#?}\n  登记了、没找到的：{gone:#?}"
    );
}

/// 零命中。`control` 是同一个认法在合成样本上认出的处数：0 ⇒ 认法坏了，零命中不算数。
#[track_caller]
pub fn none<T: Debug>(what: &str, found: impl IntoIterator<Item = T>, control: usize) {
    assert!(
        control > 0,
        "{what}：正控在合成样本上一处都没认出来 —— 认法坏了，零命中不携带信息"
    );
    let found: Vec<T> = found.into_iter().collect();
    assert!(found.is_empty(), "{what}：{found:#?}");
}

/// 一条规则：拿到快照之后起表，跑完（红了也印）把耗时印一行（测试名取线程名；单线程跑时取调用处）。
#[track_caller]
pub fn rule(body: impl FnOnce(&Repo)) {
    struct Clock(Instant, String);
    impl Drop for Clock {
        fn drop(&mut self) {
            eprintln!("规则耗时：{} · {} ms", self.1, self.0.elapsed().as_millis());
        }
    }
    let at = std::panic::Location::caller();
    let name = match std::thread::current().name() {
        Some(n) if n != "main" => n.to_string(),
        _ => format!("{}:{}", at.file(), at.line()),
    };
    let r = repo();
    let _clock = Clock(Instant::now(), name);
    body(r);
}
