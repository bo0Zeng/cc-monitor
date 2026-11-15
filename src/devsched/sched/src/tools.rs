//! 准备那台：照项目的工具清单（项目配置 `tools = <树里的相对路径>`）把工具装进那台的 `<根>/toolchains/<工具名>/`。
//! 下载 · 核 sha256 · 解包 · 换前缀都在本机（下到的东西本机只解不跑）；那台只收 rsync 送过去的文件。

use std::io::Write;
use std::path::{Path, PathBuf};

use crate::config::Host;
use crate::sync::Runner;

/// 根下另有用处的名字（Rust 工具链 · 手机 SDK），清单里不许用。
pub const RESERVED: &[&str] = devsched_wire::TOOLCHAINS_NOT_TOOLS;

/// 一份档。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Archive {
    pub url: String,
    pub sha256: String,
    /// 解包时去掉前几层目录（tar 档用）
    pub strip: u32,
}

/// 一样工具：同名的几行，装进同一个 `toolchains/<name>/`。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tool {
    pub name: String,
    pub archives: Vec<Archive>,
}

/// 档的种类，照 url 后缀认。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Conda,
    Tar,
}

/// `.conda` ⇒ conda 包；`.tar.gz` · `.tgz` · `.tar.xz` · `.tar.bz2` ⇒ tar 档；别的不认。
pub fn kind(url: &str) -> Option<Kind> {
    if url.ends_with(".conda") {
        Some(Kind::Conda)
    } else if [".tar.gz", ".tgz", ".tar.xz", ".tar.bz2"]
        .iter()
        .any(|s| url.ends_with(s))
    {
        Some(Kind::Tar)
    } else {
        None
    }
}

/// 清单：一行一份档 `<工具名> <url> <sha256> [strip=N]`，`#` 起头是注释；同名的几行归一样工具（照头一次出现的先后）。
pub fn parse(text: &str) -> Result<Vec<Tool>, String> {
    let mut v: Vec<Tool> = Vec::new();
    for (n, line) in text.lines().enumerate() {
        let l = line.trim();
        if l.is_empty() || l.starts_with('#') {
            continue;
        }
        let at = |why: &str| format!("清单第 {} 行：{why}", n + 1);
        let w: Vec<&str> = l.split_whitespace().collect();
        let [name, url, sha, rest @ ..] = w.as_slice() else {
            return Err(at("要「工具名 url sha256 [strip=N]」"));
        };
        if name.is_empty()
            || !name
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || "._-".contains(c))
            || name.starts_with('.')
        {
            return Err(at(&format!("工具名 {name} 只许小写字母 · 数字 · . _ -")));
        }
        if RESERVED.contains(name) {
            return Err(at(&format!("工具名 {name} 根下另有用处")));
        }
        if sha.len() != 64
            || !sha
                .chars()
                .all(|c| c.is_ascii_digit() || ('a'..='f').contains(&c))
        {
            return Err(at("sha256 要 64 位小写十六进制"));
        }
        if kind(url).is_none() {
            return Err(at(&format!(
                "认不得的档 {url}（.conda · .tar.gz · .tgz · .tar.xz · .tar.bz2）"
            )));
        }
        let mut strip = 0;
        for r in rest {
            match r.strip_prefix("strip=").map(str::parse::<u32>) {
                Some(Ok(s)) => strip = s,
                _ => return Err(at(&format!("认不得 {r}（只认 strip=N）"))),
            }
        }
        let a = Archive {
            url: url.to_string(),
            sha256: sha.to_string(),
            strip,
        };
        match v.iter_mut().find(|t| t.name == *name) {
            Some(t) => t.archives.push(a),
            None => v.push(Tool {
                name: name.to_string(),
                archives: vec![a],
            }),
        }
    }
    Ok(v)
}

/// 照 conda 的办法换前缀：文本照换；二进制在占位前缀所在的那个 C 字符串里换、末尾补 NUL 保长度（新前缀比占位长 ⇒ 错）。
pub fn relocate(
    data: &[u8],
    placeholder: &[u8],
    prefix: &[u8],
    binary: bool,
) -> Result<Vec<u8>, String> {
    if placeholder.is_empty() {
        return Ok(data.to_vec());
    }
    if binary && prefix.len() > placeholder.len() {
        return Err(format!(
            "新前缀（{} 字节）比构建时的占位前缀（{} 字节）长，二进制里换不了",
            prefix.len(),
            placeholder.len()
        ));
    }
    let find = |from: usize| {
        data[from..]
            .windows(placeholder.len())
            .position(|w| w == placeholder)
            .map(|p| p + from)
    };
    let replace_all = |seg: &[u8]| {
        let mut o = Vec::with_capacity(seg.len());
        let mut i = 0;
        while i < seg.len() {
            if seg[i..].starts_with(placeholder) {
                o.extend_from_slice(prefix);
                i += placeholder.len();
            } else {
                o.push(seg[i]);
                i += 1;
            }
        }
        o
    };
    if !binary {
        return Ok(replace_all(data));
    }
    let mut out = Vec::with_capacity(data.len());
    let mut i = 0;
    while let Some(p) = find(i) {
        out.extend_from_slice(&data[i..p]);
        let end = data[p..]
            .iter()
            .position(|b| *b == 0)
            .map_or(data.len(), |e| p + e);
        let mut seg = replace_all(&data[p..end]);
        seg.resize(end - p, 0);
        out.extend_from_slice(&seg);
        i = end;
    }
    out.extend_from_slice(&data[i..]);
    Ok(out)
}

/// `info/has_prefix` 一行：`<占位前缀> <text|binary> <路径>`；只写路径的老格式 ⇒ 老占位、文本。
fn has_prefix_entries(text: &str) -> Vec<(String, bool, String)> {
    text.lines()
        .filter_map(|l| {
            let w: Vec<&str> = l.split_whitespace().collect();
            match w.as_slice() {
                [ph, mode, path] => Some((ph.to_string(), *mode == "binary", path.to_string())),
                [path] => Some((
                    "/opt/anaconda1anaconda2anaconda3".into(),
                    false,
                    path.to_string(),
                )),
                _ => None,
            }
        })
        .collect()
}

fn sh(cmd: &str, args: &[&std::ffi::OsStr]) -> Result<(), String> {
    let o = std::process::Command::new("sh")
        .arg("-c")
        .arg(cmd)
        .arg("sh")
        .args(args)
        .output()
        .map_err(|e| format!("起不了 sh：{e}"))?;
    if o.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&o.stderr).trim().to_string())
    }
}

fn sha256_of(p: &Path) -> Result<String, String> {
    let o = std::process::Command::new("sha256sum")
        .arg(p)
        .output()
        .map_err(|e| format!("起不了 sha256sum：{e}"))?;
    String::from_utf8_lossy(&o.stdout)
        .split_whitespace()
        .next()
        .map(str::to_string)
        .ok_or_else(|| "sha256sum 没给出数".into())
}

/// 下到 `<dl>/<sha256>`（在就不再下），核对 sha256。
fn fetch(dl: &Path, a: &Archive) -> Result<PathBuf, String> {
    let dest = dl.join(&a.sha256);
    if dest.is_file() && sha256_of(&dest)? == a.sha256 {
        return Ok(dest);
    }
    std::fs::create_dir_all(dl).map_err(|e| format!("建不了 {}：{e}", dl.display()))?;
    let part = dl.join(format!("{}.part", a.sha256));
    sh(
        "curl -fsSL --retry 2 --max-time 1800 -o \"$1\" \"$2\"",
        &[part.as_os_str(), std::ffi::OsStr::new(&a.url)],
    )
    .map_err(|e| format!("下不来 {}：{e}", a.url))?;
    let got = sha256_of(&part)?;
    if got != a.sha256 {
        let _ = std::fs::remove_file(&part);
        return Err(format!(
            "{} 的 sha256 对不上：清单写 {}，下到的是 {got}",
            a.url, a.sha256
        ));
    }
    std::fs::rename(&part, &dest).map_err(|e| e.to_string())?;
    Ok(dest)
}

/// 本机解 `.conda` 要的程序（zip 套 tar.zst）。
pub const CONDA_NEEDS: &[&str] = &["unzip", "zstd"];

/// 这几样里本机 PATH 上找不到的。
pub fn missing_programs<'a>(names: &[&'a str]) -> Vec<&'a str> {
    let path = std::env::var_os("PATH").unwrap_or_default();
    names
        .iter()
        .copied()
        .filter(|n| !std::env::split_paths(&path).any(|d| d.join(n).is_file()))
        .collect()
}

/// 解一份档进 `into`；conda 包照 has_prefix 把占位前缀换成 `prefix`。只解不跑。
fn extract(f: &Path, a: &Archive, into: &Path, prefix: &str) -> Result<(), String> {
    match kind(&a.url) {
        Some(Kind::Tar) => sh(
            "tar -x -f \"$1\" -C \"$2\" --strip-components=\"$3\"",
            &[
                f.as_os_str(),
                into.as_os_str(),
                std::ffi::OsStr::new(&a.strip.to_string()),
            ],
        ),
        Some(Kind::Conda) => {
            let lack = missing_programs(CONDA_NEEDS);
            if !lack.is_empty() {
                return Err(format!(
                    "解 .conda 要本机有 {}：缺 {}",
                    CONDA_NEEDS.join(" · "),
                    lack.join(" · ")
                ));
            }
            let work = into.with_extension(format!("conda.{}", &a.sha256[..8]));
            let _ = std::fs::remove_dir_all(&work);
            std::fs::create_dir_all(work.join("info")).map_err(|e| e.to_string())?;
            sh(
                "unzip -oq \"$1\" -d \"$2\" && for p in \"$2\"/pkg-*.tar.zst; do zstd -qdc \"$p\" | tar -x -C \"$3\" || exit 1; done && \
                 for p in \"$2\"/info-*.tar.zst; do zstd -qdc \"$p\" | tar -x -C \"$2/info\" || exit 1; done",
                &[f.as_os_str(), work.as_os_str(), into.as_os_str()],
            )?;
            let hp = std::fs::read_to_string(work.join("info/info/has_prefix")).unwrap_or_default();
            for (ph, binary, rel) in has_prefix_entries(&hp) {
                let p = into.join(&rel);
                let Ok(data) = std::fs::read(&p) else {
                    continue;
                };
                let new = relocate(&data, ph.as_bytes(), prefix.as_bytes(), binary)
                    .map_err(|e| format!("{rel}：{e}"))?;
                std::fs::write(&p, new).map_err(|e| format!("写不了 {rel}：{e}"))?;
            }
            let _ = std::fs::remove_dir_all(&work);
            Ok(())
        }
        None => Err(format!("认不得的档 {}", a.url)),
    }
}

/// 本机暂存一样工具（解好、换好前缀）；记号没变 ⇒ 不重做，回 false。
fn stage(tool: &Tool, prefix: &str, dl: &Path, dir: &Path) -> Result<bool, String> {
    let lines: String = tool
        .archives
        .iter()
        .map(|a| format!("{} {} {}\n", a.url, a.sha256, a.strip))
        .collect();
    let stamp = devsched_wire::digest(format!("{prefix}\n{lines}").as_bytes());
    if std::fs::read_to_string(dir.join(STAMP)).ok().as_deref() == Some(stamp.as_str()) {
        return Ok(false);
    }
    let tmp = dir.with_extension("tmp");
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).map_err(|e| e.to_string())?;
    for a in &tool.archives {
        let f = fetch(dl, a)?;
        extract(&f, a, &tmp, prefix)?;
    }
    std::fs::write(tmp.join(STAMP), &stamp).map_err(|e| e.to_string())?;
    let _ = std::fs::remove_dir_all(dir);
    std::fs::rename(&tmp, dir).map_err(|e| e.to_string())?;
    Ok(true)
}

/// 暂存目录里那份记号（清单那几行与前缀的摘要），随目录一起送过去。
pub const STAMP: &str = ".devsched-tool";

/// `devsched --prepare <台>`：每样工具在本机暂存好，`rsync -a -c --delete` 送进那台的 `<根>/toolchains/<名>/`。
/// 全装上 ⇒ 0；有一样没成 · 同一台另有一个在跑 ⇒ 1（照实说哪一样、为什么）。装完清掉调度端记的那台能力，下一趟重新 hello 收。
pub fn prepare(
    host: &Host,
    tools: &[Tool],
    state_dir: &Path,
    runner: &dyn Runner,
    out: &mut dyn Write,
) -> i32 {
    let crate::config::Way::Ssh(dest) = &host.way else {
        let _ = writeln!(out, "[devsched] {} 是本机那一行：不准备", host.name);
        return 2;
    };
    let base = match host.base() {
        Ok(b) => b,
        Err(e) => {
            let _ = writeln!(out, "[devsched] {e}");
            return 2;
        }
    };
    let root = state_dir.join("tools");
    let _ = std::fs::create_dir_all(root.join(&host.name));
    let _lock = match crate::queue::State::new(state_dir.to_path_buf()).try_prepare(&host.name) {
        Ok(Some(l)) => l,
        Ok(None) => {
            let _ = writeln!(out, "[devsched] {} 另有一个 --prepare 在跑", host.name);
            return 1;
        }
        Err(e) => {
            let _ = writeln!(out, "[devsched] 开不了准备那台的锁：{e}");
            return 1;
        }
    };
    let mut bad = false;
    for t in tools {
        let prefix = format!("{base}/toolchains/{}", t.name);
        let dir = root.join(&host.name).join(&t.name);
        let fresh = match stage(t, &prefix, &root.join("dl"), &dir) {
            Ok(f) => f,
            Err(e) => {
                let _ = writeln!(out, "[devsched] {}：没装（{e}）", t.name);
                bad = true;
                continue;
            }
        };
        let argv: Vec<String> = vec![
            "rsync".into(),
            "-a".into(),
            "-c".into(),
            "--delete".into(),
            crate::sync::rsync_path_mkdir(&prefix),
            format!("--timeout={}", crate::sync::RSYNC_IO_TIMEOUT),
            "-e".into(),
            runner.ssh(),
            format!("{}/", dir.display()),
            format!("{dest}:{prefix}/"),
        ];
        match runner.run_within(&argv, crate::sync::RSYNC_WALL) {
            Ok(0) => {
                let _ = writeln!(
                    out,
                    "[devsched] {}：{} · 在 {}:{prefix}",
                    t.name,
                    if fresh {
                        "新解好送过去"
                    } else {
                        "没变"
                    },
                    host.name
                );
            }
            Ok(c) => {
                let _ = writeln!(out, "[devsched] {}：送不过去（rsync 退 {c}）", t.name);
                bad = true;
            }
            Err(e) => {
                let _ = writeln!(out, "[devsched] {}：送不过去（{e}）", t.name);
                bad = true;
            }
        }
    }
    let _ = std::fs::remove_file(state_dir.join("caps").join(&host.name));
    i32::from(bad)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::HostTable;
    use crate::sync::RealRunner;
    use std::process::Command;

    const SHA: &str = "da53da9cb71866b8ff4a5ba6f062f2ef9f6b5a8c5dcc8ae6acb634d54c4d61d7";

    #[test]
    fn manifest_groups_by_name_and_rejects_bad_lines() {
        let t = parse(&format!(
            "# 注释\n\ntmux https://x/tmux-3.5a.conda {SHA}\nnode https://x/node.tar.xz {SHA} strip=1\ntmux https://x/libevent.conda {SHA}\n"
        ))
        .unwrap();
        assert_eq!(t.len(), 2);
        assert_eq!(t[0].name, "tmux");
        assert_eq!(t[0].archives.len(), 2);
        assert_eq!(t[1].archives[0].strip, 1);
        assert_eq!(kind("https://x/a.conda"), Some(Kind::Conda));
        assert_eq!(kind("https://x/a.tar.gz"), Some(Kind::Tar));
        assert_eq!(kind("https://x/a.tgz"), Some(Kind::Tar));
        assert_eq!(kind("https://x/a.tar.xz"), Some(Kind::Tar));
        assert_eq!(kind("https://x/a.tar.bz2"), Some(Kind::Tar));
        assert_eq!(kind("https://x/a.zip"), None);
        for bad in [
            "tmux https://x/a.conda abc",
            &format!("cargo https://x/a.conda {SHA}"),
            &format!("tmux https://x/a.zip {SHA}"),
            &format!("tmux https://x/a.conda {SHA} strip=x"),
            &format!("tmux https://x/a.conda {SHA} odd=1"),
            &format!("../x https://x/a.conda {SHA}"),
            "tmux https://x/a.conda",
        ] {
            assert!(parse(bad).is_err(), "该拒：{bad}");
        }
    }

    #[test]
    fn relocate_binary_keeps_length_and_text_just_replaces() {
        let ph = b"/build/_h_env_placehold_placehold";
        let mut data = b"head\0".to_vec();
        data.extend_from_slice(ph);
        data.extend_from_slice(b"/share/terminfo\0tail");
        let out = relocate(&data, ph, b"/r/t", true).unwrap();
        assert_eq!(out.len(), data.len());
        let s = String::from_utf8_lossy(&out);
        assert!(s.starts_with("head\0/r/t/share/terminfo\0"), "{s:?}");
        assert!(s.ends_with("\0tail"));
        assert!(relocate(&data, ph, &vec![b'x'; ph.len() + 1], true).is_err());
        let txt = relocate(
            b"prefix=/build/_h_env_placehold_placehold/lib\n",
            ph,
            b"/r/t",
            false,
        )
        .unwrap();
        assert_eq!(txt, b"prefix=/r/t/lib\n");
    }

    /// 夹具：一个 `.conda`（zip：pkg-*.tar.zst ＋ info-*.tar.zst）。bin/tmux 是现编的小 C 程序，
    /// 读编进去的「占位前缀/share/v」印出来；info/has_prefix 点它（binary）。
    fn conda_fixture(dir: &Path, placeholder: &str) -> PathBuf {
        let pkg = dir.join("pkg");
        std::fs::create_dir_all(pkg.join("bin")).unwrap();
        std::fs::create_dir_all(pkg.join("share")).unwrap();
        std::fs::write(pkg.join("share/v"), "tmux 3.5a\n").unwrap();
        let c = dir.join("t.c");
        std::fs::write(
            &c,
            format!(
                "#include <stdio.h>\nstatic const char p[] = \"{placeholder}/share/v\";\n\
                 int main(void){{FILE*f=fopen(p,\"r\");if(!f)return 3;char b[64];size_t n=fread(b,1,63,f);b[n]=0;fputs(b,stdout);return 0;}}\n"
            ),
        )
        .unwrap();
        assert!(Command::new("cc")
            .arg("-o")
            .arg(pkg.join("bin/tmux"))
            .arg(&c)
            .status()
            .unwrap()
            .success());
        let info = dir.join("infod/info");
        std::fs::create_dir_all(&info).unwrap();
        std::fs::write(
            info.join("has_prefix"),
            format!("{placeholder} binary bin/tmux\n"),
        )
        .unwrap();
        let z = dir.join("z");
        std::fs::create_dir_all(&z).unwrap();
        let sh = |cmd: &str| {
            assert!(
                Command::new("sh")
                    .arg("-c")
                    .arg(cmd)
                    .status()
                    .unwrap()
                    .success(),
                "{cmd}"
            );
        };
        sh(&format!(
            "cd '{}' && tar -cf - bin share | zstd -q -o '{}/pkg-t-1.tar.zst'",
            pkg.display(),
            z.display()
        ));
        sh(&format!(
            "cd '{}' && tar -cf - info | zstd -q -o '{}/info-t-1.tar.zst'",
            dir.join("infod").display(),
            z.display()
        ));
        let out = dir.join("t-1.conda");
        sh(&format!(
            "cd '{}' && zip -q '{}' pkg-t-1.tar.zst info-t-1.tar.zst",
            z.display(),
            out.display()
        ));
        out
    }

    fn sha_of(p: &Path) -> String {
        let o = Command::new("sha256sum").arg(p).output().unwrap();
        String::from_utf8_lossy(&o.stdout)
            .split_whitespace()
            .next()
            .unwrap()
            .to_string()
    }

    struct LocalSsh {
        real: RealRunner,
        ssh: String,
        rsyncs: std::sync::Mutex<u32>,
    }
    impl Runner for LocalSsh {
        fn run(&self, argv: &[String]) -> std::io::Result<i32> {
            if argv.first().map(String::as_str) == Some("rsync") {
                *self.rsyncs.lock().unwrap() += 1;
            }
            self.real.run(argv)
        }
        fn ssh(&self) -> String {
            self.ssh.clone()
        }
    }

    fn fake_ssh(dir: &Path) -> String {
        let p = dir.join("ssh");
        crate::testkit::write_exe(
            &p,
            "#!/bin/sh\nwhile [ $# -gt 0 ]; do case \"$1\" in -o|-p|-l|-i|-F) shift 2 ;; --) shift; break ;; -*) shift ;; *) break ;; esac; done\n\
             shift\nexec sh -c \"$*\"\n",
        );
        p.display().to_string()
    }

    #[test]
    fn prepare_installs_what_this_machine_can_unpack_and_says_why_not_for_the_rest() {
        // 连跑几趟 prepare，每趟拿放一次「同一台只一个 prepare」的 flock：在单跑它的子进程里判
        // （同一进程里别的测试 fork 的子进程会攥着放了的锁，下一趟就成了「另有一个在跑」）
        if !crate::testkit::alone(
            "tools::tests::prepare_installs_what_this_machine_can_unpack_and_says_why_not_for_the_rest",
        ) {
            return;
        }
        let t = crate::testkit::TempDir::new("tools");
        // 解 .conda 要本机的 unzip · zstd：这台没有 ⇒ 判另一半（照实说缺、那一样不装、退 1），装上的那一半在有的机器上判
        // 这台有没有：测试自己起一下探（不借被测的 missing_programs）
        let runs = |p: &str, a: &str| {
            Command::new(p)
                .arg(a)
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status()
                .is_ok()
        };
        let lack: Vec<&str> = [("unzip", "-v"), ("zstd", "--version")]
            .into_iter()
            .filter(|(p, a)| !runs(p, a))
            .map(|(p, _)| p)
            .collect();
        if !lack.is_empty() {
            return conda_without_its_programs_is_refused(&t, &lack);
        }
        assert!(
            runs("zip", "-v") && runs("cc", "--version"),
            "判不了：这台有 unzip · zstd，却没有造夹具要的 zip · cc"
        );
        // 占位前缀照 conda 的样子取长（真包里是 255 字节上下），临时根的路径长短随进程号变
        let placeholder = &format!("/build/feedstock/_h_env{}", "_placehold".repeat(20));
        let pkg = conda_fixture(&t.path().join("fx"), placeholder);
        let tar_src = t.path().join("tarsrc/node-v1/bin");
        std::fs::create_dir_all(&tar_src).unwrap();
        std::fs::write(tar_src.join("node"), "#!/bin/sh\necho v1\n").unwrap();
        let tgz = t.path().join("node-v1.tar.gz");
        assert!(Command::new("tar")
            .arg("-czf")
            .arg(&tgz)
            .arg("-C")
            .arg(t.path().join("tarsrc"))
            .arg("node-v1")
            .status()
            .unwrap()
            .success());
        let base = t.path().join("remote/root");
        let hosts = HostTable::parse(&format!("a 1 ssh:a-dest base={}\n", base.display()));
        let host = hosts.get("a").unwrap();
        let state = t.path().join("state");
        let manifest = format!(
            "tmux file://{} {}\nnode file://{} {} strip=1\n",
            pkg.display(),
            sha_of(&pkg),
            tgz.display(),
            sha_of(&tgz)
        );
        let tools = parse(&manifest).unwrap();
        let r = LocalSsh {
            real: RealRunner::default(),
            ssh: fake_ssh(t.path()),
            rsyncs: Default::default(),
        };
        let mut out = Vec::new();
        assert_eq!(
            prepare(host, &tools, &state, &r, &mut out),
            0,
            "{}",
            String::from_utf8_lossy(&out)
        );
        // 在一个跟构建前缀不同的根里跑得起来，读得到换过前缀的那份文件
        let tm = base.join("toolchains/tmux/bin/tmux");
        let o = Command::new(&tm).output().unwrap();
        assert!(o.status.success(), "{o:?}");
        assert_eq!(String::from_utf8_lossy(&o.stdout), "tmux 3.5a\n");
        assert!(base.join("toolchains/node/bin/node").is_file());
        // 再跑一次：没变就不重解，rsync 照常（按内容比，不送东西）
        let n = *r.rsyncs.lock().unwrap();
        let mut out = Vec::new();
        assert_eq!(
            prepare(host, &tools, &state, &r, &mut out),
            0,
            "{}",
            String::from_utf8_lossy(&out)
        );
        assert!(
            String::from_utf8_lossy(&out).contains("没变"),
            "{}",
            String::from_utf8_lossy(&out)
        );
        assert_eq!(*r.rsyncs.lock().unwrap(), n + 2);
        // sha256 对不上的那一样不装、照实报、退 1；别的照装
        let bad = format!(
            "tmux file://{} {}\nother file://{} {}\n",
            pkg.display(),
            sha_of(&pkg),
            tgz.display(),
            "0".repeat(64)
        );
        let mut out = Vec::new();
        assert_eq!(
            prepare(host, &parse(&bad).unwrap(), &state, &r, &mut out),
            1
        );
        let s = String::from_utf8_lossy(&out);
        assert!(s.contains("other") && s.contains("sha256"), "{s}");
        assert!(!base.join("toolchains/other").exists());
    }

    #[test]
    fn missing_programs_names_only_what_is_not_on_path() {
        assert_eq!(
            missing_programs(&["sh", "devsched-surely-not-a-program"]),
            ["devsched-surely-not-a-program"]
        );
    }

    /// 本机缺解 .conda 的程序：那一样照实说缺哪个、不装、退 1；同一份清单里的 tar 档照装。
    fn conda_without_its_programs_is_refused(t: &crate::testkit::TempDir, lack: &[&str]) {
        let pkg = t.path().join("t-1.conda");
        std::fs::write(&pkg, b"not unpacked here").unwrap();
        let tar_src = t.path().join("tarsrc/node-v1/bin");
        std::fs::create_dir_all(&tar_src).unwrap();
        std::fs::write(tar_src.join("node"), "#!/bin/sh\necho v1\n").unwrap();
        let tgz = t.path().join("node-v1.tar.gz");
        assert!(Command::new("tar")
            .arg("-czf")
            .arg(&tgz)
            .arg("-C")
            .arg(t.path().join("tarsrc"))
            .arg("node-v1")
            .status()
            .unwrap()
            .success());
        let base = t.path().join("remote/root");
        let hosts = HostTable::parse(&format!("a 1 ssh:a-dest base={}\n", base.display()));
        let host = hosts.get("a").unwrap();
        let manifest = format!(
            "tmux file://{} {}\nnode file://{} {} strip=1\n",
            pkg.display(),
            sha_of(&pkg),
            tgz.display(),
            sha_of(&tgz)
        );
        let r = LocalSsh {
            real: RealRunner::default(),
            ssh: fake_ssh(t.path()),
            rsyncs: Default::default(),
        };
        let mut out = Vec::new();
        assert_eq!(
            prepare(
                host,
                &parse(&manifest).unwrap(),
                &t.path().join("state"),
                &r,
                &mut out
            ),
            1
        );
        let s = String::from_utf8_lossy(&out);
        assert!(
            s.contains("tmux") && s.contains("缺") && lack.iter().all(|p| s.contains(p)),
            "{s}"
        );
        assert!(!base.join("toolchains/tmux").exists(), "缺程序的那一样不装");
        assert!(base.join("toolchains/node/bin/node").is_file(), "别的照装");
    }
}
