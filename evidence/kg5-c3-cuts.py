#!/usr/bin/env python3
"""K-G5 `C3` 的死值验量具（09-01）—— 本拍五刀的**可复跑凭据**。

为什么它在版本控制里：上一拍的交回报告里写了 P1-P5 五刀，而盘上找不到可复跑的东西，
审计员复不了那五刀。`brief` 第 12 条那一句：「写『可重跑』就要给量具的住址，给不出别写」。

── 被测对象指向哪棵树（`brief` `5k`）─────────────────────────────────────────
本量具**只测它自己所在的那棵工作树**：树根由 `git rev-parse --show-toplevel` 在
**本文件所在目录**里现算，不接受任何外部路径参数 ⇒ 它被复制到别处之后也不会还指着原来那棵树。
每次运行先把「树根 · HEAD · 被切文件的 md5 · 未提交改动处数」印出来 —— 读数与这四样是一套，
别拆开引用（同一住址下先后住过两份被测对象不同的量具 = 一次静默的假读数）。
落地那一刻：工作树 `.claude/worktrees/k-g5`、分支 `track/k-g5`、基线尖 `a68fd25`，
这棵树**未铺** `src-tauri/embedded-daemons/`（cargo 1303 · daemon 490 · npm 1480 是这种配置下的数）。

── 为什么它绕开 `scripts/gate.sh` 的 `run_gate` 包装 ────────────────────────
`run_gate` 在 `rc != 0` 时把输出丢掉，而死值验要的正是**失败那一趟**的判定行与报错模板。
所以 `run` 子命令**同镜像、同挂载、同 CARGO_TARGET_DIR**，在容器里直接跑 `cargo test`。
宿主上一条测试都不跑。（这已经是本仓第四次有人被那个包装逼得绕开 ⇒ 归 `K-R11` 的材料。）

⚠ 为什么是 `.py` 不是 `.sh`：`shell_lint_registry::every_shell_script_is_either_linted_or_registered_as_exempt`
要求仓里每个 `.sh` 要么进 CI 的 shellcheck 表达式、要么在它的 `EXEMPT` 里登记 ——
两条路都要改本拍写区之外的文件（`ci.yml` / `shell_lint_registry.rs`）。
第一版真写成了 `.sh`，被那条判据当场逮住（cargo 门 `1206 passed; 2 failed`）。**它逮得对。**

── 用法 ────────────────────────────────────────────────────────────────────
  kg5-c3-cuts.py readings     本拍五刀的真实读数（写死在这里，供下一轮对账）
  kg5-c3-cuts.py anchors      逐刀核锚点在当前文件里恰好命中几次（只读；`brief` 第 7 条）
  kg5-c3-cuts.py selftest     把每一刀落到临时目录的一份副本上，验它今天还贴得上（不碰工作树）
  kg5-c3-cuts.py cut M3 M4    把一刀（或叠着的几刀）真的落进工作树（要求那个文件干净）
  kg5-c3-cuts.py run          在沙箱里跑 daemon 测试，印判定行 + 带 `C3-` 前缀的读数
  kg5-c3-cuts.py restore      `git checkout` 还原被切的那个文件并核 md5

一刀的标准跑法：`anchors` -> `cut M1` -> `run` -> `restore`。
⚠ `cut` 会**改工作树里的源码**；`restore` 之后必须看到 md5 回到基线、未提交改动回到 0 处。
"""

import hashlib
import pathlib
import subprocess
import sys
import tempfile

TARGET_REL = "remote-daemon-proto/src/single_stream_guard.rs"
IMAGE = "ccmon-devbox:latest"
TAG = "k-g5-c3"
PROJ = "/home/zbl/文档/claudecode-frontend"

HERE = pathlib.Path(__file__).resolve().parent


def git(*args, root=None):
    return subprocess.run(
        ["git", "-C", str(root or HERE), *args], capture_output=True, text=True
    ).stdout.strip()


ROOT = pathlib.Path(git("rev-parse", "--show-toplevel"))
TARGET = ROOT / TARGET_REL


def banner():
    md5 = hashlib.md5(TARGET.read_bytes()).hexdigest()
    dirty = len([ln for ln in git("status", "--porcelain", root=ROOT).splitlines() if ln])
    print(f"· 树根   {ROOT}")
    print(f"· HEAD   {git('rev-parse', 'HEAD', root=ROOT)}  分支 {git('rev-parse', '--abbrev-ref', 'HEAD', root=ROOT)}")
    print(f"· 被切件 {TARGET_REL}  md5 {md5}")
    print(f"· git    {dirty} 处未提交改动")


# 每一刀：id -> (说明, 锚点原文, 换上去的东西, 锚点该命中几次)
CUTS = {}

CUTS["M1"] = (
    "逼判据自己印出语料的真值：文件数 / 字节(c.len()) / 字符(chars().count())",
    '''        assert!(
            bytes >= 150_000,
            "全 crate 语料只有 {bytes} 字节（下限 150_000）—— 剥过头了，本条此刻在空转"
        );
        // 反空真②：`scan_tree!` 的自摘那一刀**真的落下了**。''',
    '''        assert!(
            bytes >= 9_000_000,
            "C3-M1 读数：文件 {} · 字节(c.len()) {bytes} · 字符(chars().count()) {} · 字符+文件数 {}",
            corpus.len(),
            corpus.iter().map(|(_, c)| c.chars().count()).sum::<usize>(),
            corpus.iter().map(|(_, c)| c.chars().count()).sum::<usize>() + corpus.len()
        );
        // 反空真②：`scan_tree!` 的自摘那一刀**真的落下了**。''',
    1,
)

CUTS["M2"] = (
    "印出本模块自己的生产段：几字节 · 内容 · 锚点各几处（`production_code` 剥完之后）",
    '''        let corpus = crate_sources();
        // 反空真①：语料塌了 ⇒ 下面是一句 0 == 0 的空真。''',
    '''        let self_prod = source_of("single_stream_guard.rs");
        let per_pin: Vec<(&str, usize)> = PINS
            .iter()
            .map(|(_, needle, _, _, _)| (*needle, self_prod.matches(needle).count()))
            .collect();
        assert!(
            self_prod.len() > 9_000_000,
            "C3-M2 读数：本模块生产段 {} 字节 · 内容 {:?} · `{}` {} 处 · PINS 六锚点 {:?} · 原文件 {} 字节",
            self_prod.len(),
            self_prod,
            BARE_BIRTH,
            self_prod.matches(BARE_BIRTH).count(),
            per_pin,
            include_str!("single_stream_guard.rs").len()
        );
        let corpus = crate_sources();
        // 反空真①：语料塌了 ⇒ 下面是一句 0 == 0 的空真。''',
    1,
)

CUTS["M3"] = (
    "模拟「自摘那一刀没落下」：把本模块自己塞回语料 ⇒ 两条反空真② 该一起红",
    '''        guard_core::scan_tree!(&root, &["rs"])
            .into_iter()
            .map(|(path, src)| {
                let rel = path
                    .strip_prefix(&root)
                    .unwrap_or(&path)
                    .to_string_lossy()
                    .replace('\\\\', "/");
                (rel, production_code(&src))
            })
            .collect()
    }''',
    '''        let mut v: Vec<(String, String)> = guard_core::scan_tree!(&root, &["rs"])
            .into_iter()
            .map(|(path, src)| {
                let rel = path
                    .strip_prefix(&root)
                    .unwrap_or(&path)
                    .to_string_lossy()
                    .replace('\\\\', "/");
                (rel, production_code(&src))
            })
            .collect();
        // C3-M3：模拟「自摘那一刀没落下」——本模块自己进语料。
        v.push((
            "single_stream_guard.rs".to_string(),
            source_of("single_stream_guard.rs"),
        ));
        v
    }''',
    1,
)

CUTS["M4"] = (
    "叠在 M3 上：把 `no_qualified…` 的反空真② 退掉，看本条还有没有别的东西认得出摘除坏了",
    '''        assert!(
            !corpus
                .iter()
                .any(|(rel, _)| rel.ends_with("single_stream_guard.rs")),
            "本护栏自己进了语料 —— `scan_tree!` 的自摘那一刀没落下。\\
             ★ 本格认的是**摘除坏了这件事本身**，不是「它会拿自己的样本把自己打红」：\\
             本模块整份内容都住在 `#[cfg(test)] mod tests` 里，`production_code` 把它整块剥掉，\\
             生产段今天只有 15 字节（09-01 现打），里面 `{BARE_BIRTH}` 0 处 —— \\
             就算它进了语料，也一口都喂不进去。"
        );''',
    '''        // C3-M4：把反空真② 退掉（摘除仍然坏着），看本条还有没有别的东西认得出来。''',
    1,
)

CUTS["M5"] = (
    "印出诚实边界第 1 条那两个子串在语料里的真实分布：`channel(` 与 `use tokio::sync::mpsc`",
    '''        assert!(
            bytes >= 150_000,
            "全 crate 语料只有 {bytes} 字节（下限 150_000）—— 剥过头了，本条此刻在空转"
        );
        // 反空真②：`scan_tree!` 的自摘那一刀**真的落下了**。''',
    '''        let d_ch: Vec<(&str, usize)> = corpus
            .iter()
            .map(|(r, c)| (r.as_str(), c.matches("channel(").count()))
            .filter(|(_, n)| *n > 0)
            .collect();
        let d_use: Vec<(&str, usize)> = corpus
            .iter()
            .map(|(r, c)| (r.as_str(), c.matches("use tokio::sync::mpsc").count()))
            .filter(|(_, n)| *n > 0)
            .collect();
        assert!(
            bytes >= 9_000_000,
            "C3-M5 读数：`channel(` 合计 {} 分布 {:?} · `use tokio::sync::mpsc` 合计 {} 分布 {:?} · `use tokio::sync::mpsc::` 合计 {}",
            d_ch.iter().map(|(_, n)| n).sum::<usize>(),
            d_ch,
            d_use.iter().map(|(_, n)| n).sum::<usize>(),
            d_use,
            corpus.iter().map(|(_, c)| c.matches("use tokio::sync::mpsc::").count()).sum::<usize>()
        );
        // 反空真②：`scan_tree!` 的自摘那一刀**真的落下了**。''',
    1,
)

CUTS["M6"] = (
    "叠在 M3 上：把 `none_of_these_anchors…` 的反空真② 退掉（M4 的同族一刀，换另一条判据）",
    '''        assert!(
            !corpus
                .iter()
                .any(|(rel, _)| rel.ends_with("single_stream_guard.rs")),
            "本护栏自己进了语料 —— `scan_tree!` 的自摘那一刀没落下。\\
             ★ 本格认的是**摘除坏了这件事本身**，不是「`PINS` 里那六个锚点会多数出来」：\\
             本模块的生产段今天只有 15 字节、六个锚点各 0 处（09-01 现打），\\
             进了语料也一处都不会多算 —— 会出声的只有本格。"
        );''',
    '''        // C3-M6：把本格退掉（摘除仍然坏着），看本条还有没有别的东西认得出来。''',
    1,
)

READINGS = """
本拍五刀的真实读数（量于基线尖 a68fd25，沙箱 ccmon-devbox:latest，--network host，
CARGO_TARGET_DIR=.claude/pm-targets/k-g5-c3，这棵树未铺 embedded-daemons）：

  基线（未切）                 490 passed; 0 failed; 1 ignored
  M1  锚点 1 处 · 判定行 489/1  文件 72 · 字节 255999 · 字符 248516 · 字符+文件数 248588
  M2  锚点 1 处 · 判定行 489/1  本模块生产段 15 字节，内容就是一行 `#![cfg(test)]`（前后各一个空行）；
                               `::channel(` 0 处；PINS 六锚点在里面各 0 处；原文件 38174 字节
  M3  锚点 1 处 · 判定行 488/2  两条反空真② 一起红 —— 摘除坏了，两条判据都出声
  M4  锚点 1 处 · 判定行 489/1  M3 之上退掉 `no_qualified…` 的反空真② ⇒ 本条**绿着走过去**，
                               红的只剩 `none_of_these_anchors…`
                               ⇒ 那一格是本条判据里**唯一**认得出「摘除坏了」的东西
  M5  锚点 1 处 · 判定行 489/1  `channel(` 合计 0（分布空）；`use tokio::sync::mpsc` 合计 2
                               = inbound.rs 1 + observe/watcher.rs 1；`use tokio::sync::mpsc::` 合计 0
  M6  锚点 1 处 · 判定行 489/1  M3 之上退掉 `none_of_these_anchors…` 的反空真② ⇒ 本条也**绿着走过去**
                               （量于 6c61a2f，那一刀之后红的只剩 `no_qualified…`）
                               ⇒ 两条判据各自的那一格，各自都是**唯一**认得出「摘除坏了」的东西

分母怎么数的：判定行取 `cargo test` 自己那行 `test result:`（daemon 一个二进制、一行）；
「几处」一律 `str::matches(needle).count()`，语料 = `crate_sources()`（全 crate 生产段，摘掉本模块自己）。
★ 字节 = `c.len()`（UTF-8 字节），字符 = `chars().count()` —— 混用一次就读出一次假漂移：
  头注里原先那个「248 588」正是这么来的，= 字符 248516 + 文件数 72（每份文件一个尾行伪影）。
"""


def do_anchors():
    src = TARGET.read_text(encoding="utf-8")
    bad = 0
    for cid, (why, old, _new, want) in CUTS.items():
        got = src.count(old)
        if got != want:
            bad = 1
        print(f"  {'ok ' if got == want else '馊了'} {cid}  锚点命中 {got} 处（该 {want} 处） —— {why}")
    print()
    print("锚点全部对上" if not bad else "★ 有锚点对不上 ⇒ 文件漂了，先把这几刀重新定准，别拿旧读数对账")
    return bad


def do_selftest():
    src = TARGET.read_text(encoding="utf-8")
    out = pathlib.Path(tempfile.mkdtemp(prefix="kg5-c3-"))
    bad = 0
    for cid, (why, old, new, want) in CUTS.items():
        got = src.count(old)
        if got != want:
            print(f"  馊了 {cid}  锚点 {got} 处（该 {want} 处）—— 贴不上")
            bad = 1
            continue
        mutated = src.replace(old, new, 1)
        (out / f"{cid}.rs").write_text(mutated, encoding="utf-8")
        print(f"  ok  {cid}  贴得上（{len(mutated) - len(src):+d} 字符） —— {why}")
    print()
    print("每一刀今天都贴得上（贴在副本上，工作树没被碰）" if not bad else "★ 有刀贴不上")
    print(f"· 副本在 {out}（用完自己删）")
    return bad


def do_cut(ids):
    if git("status", "--porcelain", "--", TARGET_REL, root=ROOT):
        print(f"★ {TARGET_REL} 有未提交改动 —— 先 restore，别把上一刀带进这一刀")
        return 3
    src = TARGET.read_text(encoding="utf-8")
    for cid in ids:
        if cid not in CUTS:
            print("没有这一刀：", cid)
            return 3
        _why, old, new, want = CUTS[cid]
        got = src.count(old)
        print(f"· {cid} 锚点命中 {got} 处（该 {want} 处）")
        if got != want:
            print("★ 锚点数对不上 —— 一刀都不落，先重新定准")
            return 3
        src = src.replace(old, new, 1)
    TARGET.write_text(src, encoding="utf-8")
    print("· 变异已落地：" + " + ".join(ids))
    print(f"· 现在跑：{sys.argv[0]} run   跑完务必：{sys.argv[0]} restore")
    return 0


def do_run():
    print(f"· 沙箱 {IMAGE} · --network host · CARGO_TARGET_DIR={PROJ}/.claude/pm-targets/{TAG}")
    print()
    proc = subprocess.run(
        [
            "docker", "run", "--rm", "--network", "host",
            "-v", f"{PROJ}:{PROJ}",
            "-v", "ccmon-cargo-registry:/opt/rust/cargo/registry",
            "-e", f"CARGO_TARGET_DIR={PROJ}/.claude/pm-targets/{TAG}",
            "-e", "HOME=/home/zbl",
            "-w", str(ROOT),
            IMAGE,
            "bash", "-o", "pipefail", "-c", "cd remote-daemon-proto && cargo test 2>&1",
        ],
        capture_output=True,
        text=True,
    )
    keep = ("C3-M", "test result:", "single_stream_guard::tests::", "本护栏自己进了语料", "error[", "error:")
    for line in proc.stdout.splitlines():
        if any(k in line for k in keep):
            print(line)
    return 0


def do_restore():
    subprocess.run(["git", "-C", str(ROOT), "checkout", "--", TARGET_REL], check=False)
    banner()
    print("· 还原完成 —— 上面那行 md5 要与切之前印的那一个逐字相同，未提交改动要回到切之前那个数")
    return 0


def main(argv):
    mode = argv[1] if len(argv) > 1 else ""
    if mode not in {"readings", "anchors", "selftest", "cut", "run", "restore"}:
        print(__doc__)
        return 2
    banner()
    print()
    if mode == "readings":
        print(READINGS)
        return 0
    if mode == "anchors":
        return do_anchors()
    if mode == "selftest":
        return do_selftest()
    if mode == "cut":
        return do_cut(argv[2:]) if len(argv) > 2 else 2
    if mode == "run":
        return do_run()
    return do_restore()


if __name__ == "__main__":
    sys.exit(main(sys.argv))
