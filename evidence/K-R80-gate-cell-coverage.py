#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""K-R80 `KR80D3`：门禁**每一格的分母各盖到哪几棵树**的登记 ＋ 它的机检。

## 它治的是什么

`K-R80` 的题面是：`fmt` 那一格的分母里逐字写着「`remote-daemon-proto` 是另一个
workspace，本行盖不到」——**一格「我盖不到那儿」的诚实注释，被当成了处置**。
盲区自己写在注释里，而没有任何东西在数它们。
⇒ 本尺子把「哪一格盖到哪几棵树」变成一份**逐格点名、机器对得上**的登记。

## ⚠ 它买得到什么、买不到什么（写死，别读宽）

**买得到**（下面 6 条 `C1`–`C6`，任一条不满足就 `exit 1`）：
  · `C1` `scripts/gate.sh` 里真有的判定格 ↔ 本表的键，**两侧互为子集**（多一格少一格都红）
  · `C2` 每条登记的**逐字锚点**在 `gate.sh` 里 `count() == 1`（登记指得到真东西，且不含糊）
  · `C3` 每一格对**每一棵树**都有一个裁词，取值只许是 `全 / 部 / 无`（**没有空白格**
        —— 这一条就是「**逐格点名它盖不到的那些**」的机器口径）
  · `C4` 树的全集 == `git ls-files` 现打的顶层目录（＋ vendor 那一棵单拆 ＋「仓根文件」一格）
        ⇒ 新增一棵顶层树而没人回来登记 ⇒ 红
  · `C5` `gate.sh` 末尾那行 `GATE: OK —— <n> 格全绿` 里的 `n` == 现打的判定格数
        ⇒ 加了一格而那行点名没跟 ⇒ 红（那一行 09-10 起就馊过一次：加了 `fmt`/`winchk`
          而它还写着「三道门 + 生成物漂移 + pb check + 四套 ccm e2e」= 9 格，盘上已是 11 格）
  · `C6` 每一个裁词都带一句**非空**的理由（`无` 也要写为什么盖不到）

**买不到**（同样是判据，只是方向相反 —— 别把绿读成这个）：
  · 它**不判裁词对不对**。`fmt` 那一格到底盖没盖住 `src-tauri`，机器在这里问不出来；
    它只保证**每一格都被表过态、锚点指得到真东西、格数对得上**。
    ⇒ 一条**写错的**裁词能骗过本尺子。**这是登记的机检，不是覆盖率的判据。**
  · 它**不补任何盲区**（`K-R80 §0d` 逐字：数出来归数出来，补是另一件）。

## 跑法

    python3 evidence/K-R80-gate-cell-coverage.py [<gate.sh 路径>]

不给参数就用仓根的 `scripts/gate.sh`（仓根 = 本文件的上一级）。
给参数是为了**对着变异过的副本跑**（死值验），不必去动真文件。
"""

import os
import re
import subprocess
import sys
from pathlib import Path

# `K_R80_ROOT` 只为**死值验**存在：把本文件拷进 scratchpad 变异之后，仓根仍要指回真工作树。
# ⚠ 它不是配置项，日常跑一律不带。
ROOT = Path(os.environ.get("K_R80_ROOT") or Path(__file__).resolve().parent.parent)
GATE = Path(sys.argv[1]).resolve() if len(sys.argv) > 1 else ROOT / "scripts" / "gate.sh"

# ── 树的全集 ────────────────────────────────────────────────────────────────
# 分母不是拍脑袋定的：`C4` 拿 `git ls-files` 现打的**顶层目录**跟它对拍。
# 两处刻意与顶层目录不一样，各有理由，都写在这里：
#   · `src-tauri/vendor/code-picture-core/` 从 `src-tauri/` 里**单拆出来** ——
#     `C7` 逐字「vendor `code-picture-core` **不动**」，它被 `cargo` 那一格显式 `--exclude`，
#     与 `src-tauri/` 的其余部分**受不同的门管**，混成一棵就点不出这一格盲区。
#   · `<仓根文件>` 是一格，装 `package.json` / `vite.config.ts` / `README.md` 那些
#     不属于任何顶层目录的文件（`git ls-files` 里 `NF==1` 的那些）。
VENDOR = "src-tauri/vendor/code-picture-core/"
ROOTFILES = "<仓根文件>"
TREES = [
    "src/",
    "src-tauri/",            # 不含 vendor 那一棵
    VENDOR,
    "remote-daemon-proto/",
    "e2e/",
    "scripts/",
    "shared/",
    "hooks/",
    "doc/",
    ".github/",
    "evidence/",
    ROOTFILES,
]

FULL, PART, NONE = "全", "部", "无"

# ── 登记 ────────────────────────────────────────────────────────────────────
# 每一格：`anchor` = 在 `gate.sh` 里逐字唯一的一串（`C2` 数它 == 1）；
#         `cover` = **对每一棵树**的裁词 + 一句理由（`C3`/`C6` 要求一格不空）。
#
# ⚠ 「部」的注里凡是带数字的，都注明是**哪把尺子现打的**；本文件末尾那段
#   `cross_tree_reads()` 会把那把尺子再跑一遍印出来 —— 它**不是判据**，是让这些数可复算。
def blind(reason):
    return (NONE, reason)


OUT_OF_TREE = "本格的命令根本不进这棵树"


def all_blind(reason=OUT_OF_TREE):
    return {t: blind(reason) for t in TREES}


REGISTRY = {}


def cell(name, anchor, cwd, cmd, **verdicts):
    c = all_blind()
    c.update(verdicts)
    REGISTRY[name] = {"anchor": anchor, "cwd": cwd, "cmd": cmd, "cover": c}


cell(
    "fmt",
    anchor="run_gate fmt '不是数出来的数",
    cwd="src-tauri/",
    cmd="cargo fmt --all --check",
    **{
        "src-tauri/": (PART, "只有那 8 个 workspace 成员从 crate 根顺 mod 走得到的 `.rs`；"
                             "`tauri.conf.json` / `Cargo.toml` 那些非 Rust 文件一概不在"),
        VENDOR: blind("`[workspace] exclude` 把它排掉了 —— 而 `C7` 逐字「vendor 不动」，"
                      "把它拉进出货门禁就是一道我们满足不了的闸"),
        "remote-daemon-proto/": blind("另一个 workspace —— **这就是 `K-R80` 的题面**，"
                                      "今天由 `fmt-daemon` 那一格盖"),
    },
)

cell(
    "fmt-daemon",
    anchor="run_gate fmt-daemon '不是数出来的数",
    cwd="remote-daemon-proto/",
    cmd="cargo fmt --check（刻意不加 --all）",
    **{
        "remote-daemon-proto/": (PART, "唯一成员 `cc-monitor-remote`，射程 = 从 `src/main.rs` "
                                       "顺 `mod` 走得到的那些 `.rs`；走不到的文件本格看不见"),
        "src-tauri/": blind("刻意**不加** `--all`：加了 rustfmt 实收 12 个 crate 根、11 个在这棵树外"),
        VENDOR: blind("同上 —— 加 `--all` 会把这棵我们无权修的树拉进来"),
    },
)

cell(
    "winchk",
    anchor="run_gate winchk '不是数出来的数",
    cwd="src-tauri/",
    cmd="cargo check --locked -p monitor --target x86_64-pc-windows-gnu",
    **{
        "src-tauri/": (PART, "`-p monitor` 一个包的**生产段编得过**；7 个共享 crate 作为依赖被编。"
                             "⚠ `#[cfg(test)]` 不进 `check`，「行为对」更买不到"),
        VENDOR: (PART, "作为 `monitor` 的依赖被编。⚠ 这一条**本件没现打**，"
                       "是从依赖关系推的 —— 按「未验」读"),
        "remote-daemon-proto/": blind("本格逐字写着它盖不到：那 17 处 `cfg(windows)` 没人跨编"),
    },
)

cell(
    "cargo",
    anchor="run_gate_sum cargo 8 bash -c",
    cwd="src-tauri/",
    cmd="cargo test --workspace --exclude code-picture-core --lib",
    **{
        "src-tauri/": (FULL, "8 个成员的 `--lib` 判据，合计求和 + 包数相等断言"),
        VENDOR: blind("显式 `--exclude code-picture-core`（`C7`：vendor 不动）"),
        "remote-daemon-proto/": (PART, "**经扫描型守卫读进去**：现打 18 个读点"
                                       "（尺子见本文件 `cross_tree_reads()`）"),
        "scripts/": (PART, "同上，现打 6 个读点（`shared_crate_registry` 读 `gate.sh` 那条最有名）"),
        "doc/": (PART, "同上，现打 9 个读点"),
        "shared/": (PART, "同上，现打 5 个读点"),
        ".github/": (PART, "同上，现打 3 个读点（`capability_registry` 读 `ci.yml`）"),
        "e2e/": (PART, "同上，现打 4 个读点"),
        "src/": blind("前端那棵树归 `npm` 与 `generated` 两格；Rust 判据里没有读点"),
    },
)

cell(
    "generated",
    anchor="git diff --quiet --exit-code -- src/generated/",
    cwd="仓根",
    cmd="git diff --quiet --exit-code -- src/generated/",
    **{
        "src/": (PART, "**只有 `src/generated/`**，而且只判**已跟踪文件的 diff** ——"
                       "全新的生成物是 untracked，本格看不见（那一格归 `generated-boundary-guard`）"),
    },
)

cell(
    "daemon",
    anchor="run_gate daemon '单包 remote-daemon-proto",
    cwd="remote-daemon-proto/",
    cmd="cargo test",
    **{
        "remote-daemon-proto/": (FULL, "单包全量 `cargo test`（含 `#[cfg(test)]` 那一族守卫）"),
        "src-tauri/": (PART, "跨轨对拍：现打 4 个读点（`control/gate.rs` 读 Gate 2 黄金夹具、"
                             "`control/launch.rs` 读 monitor 的 `tmux.rs`）"),
        ".github/": (PART, "现打 1 个读点"),
        "e2e/": (PART, "现打 2 个读点"),
        "doc/": (PART, "现打 5 个读点（协议文档对拍）"),
    },
)

cell(
    "npm",
    anchor="run_gate npm '17 个套件",
    cwd="仓根",
    cmd="npm test（16 个 tsx 套件 + vitest run）",
    **{
        "src/": (FULL, "16 个 tsx 套件全在 `src/` 下 + `vitest.config.ts` 的 "
                       "`include: [\"src/**/*.vitest.ts\"]`。⚠ **「跑了 0 个也照绿」那一格 15/17 守不住**"
                       "（本格分母那句话逐字写着）"),
        "src-tauri/": (PART, "现打 2 个读点"),
        "remote-daemon-proto/": (PART, "现打 1 个读点"),
        "scripts/": (PART, "现打 2 个读点"),
        "e2e/": (PART, "现打 1 个读点"),
        "shared/": (PART, "现打 1 个读点"),
    },
)

E2E_NOTE = ("四套 `ccm` e2e 之一。`e2e/` 下的套件今天远不止四套 —— "
            "`ccm-acceptance` / `ccm-pretrust` / `cc-spawn-uplift` 等**都不在这道门里**"
            "（那笔账逐字记在本文件头注引的 `gate.sh` 那一段：一次真行为变更的 71 条红里"
            "「这道门看得见 9 条、看不见 62 条」）")
for suite, anchor in [
    ("ccm e2e/ccm-print-parity", "run_e2e ccm-print-parity 12"),
    ("ccm e2e/ccm-rbind-title", "run_e2e ccm-rbind-title  8"),
    ("ccm e2e/ccm-cli", "run_e2e ccm-cli               46"),
    ("ccm e2e/ccm-contract-parity", "run_e2e ccm-contract-parity   45"),
]:
    cell(
        suite,
        anchor=anchor,
        cwd="仓根",
        cmd="bash e2e/assert-pass-floor.sh <套件> <地板> exact",
        **{
            "e2e/": (PART, E2E_NOTE),
            "remote-daemon-proto/": (PART, "**被测对象是它编出来的二进制** "
                                           "`$CARGO_TARGET_DIR/debug/cc-monitor-remote`"
                                           "（`K-R48` 第二拍起）——买的是行为，不是它的源码"),
        },
    )

cell(
    "pb check",
    anchor='python3 "$HOME/.claude-accts/z/skills/planned-build/bin/pb.py" check',
    cwd="仓根（查的目录在仓外）",
    cmd="pb.py check ../.claude/planned-build/$PB_WS",
    # 12 棵树全「无」——刻意的，见下
    **{t: blind("本格的分母**根本不在本仓**：它查 `../.claude/planned-build/$PB_WS` 那个计划仓") for t in TREES},
)


# ── 现打：`git ls-files` ────────────────────────────────────────────────────
# 🔴 **必须走 `-z`。** 第一版用的是裸 `ls-files` + 按换行切，而本仓有中文文件名 ——
#   git 默认会把它们**加引号并转义**（`"doc/\350\256\241…"`），于是顶层目录被切成 `"doc/`
#   与 `"evidence/`，`C4` 当场报「盘上多出两棵树」。**那是尺子的病，不是盘上的事实**
#   （本仓最高频那族：量具的作用域对不上事实）。`-z` 出的是原始字节、NUL 分隔，不转义。
def ls_files():
    out = subprocess.run(["git", "-C", str(ROOT), "ls-files", "-z"],
                         capture_output=True, text=True, encoding="utf-8", errors="surrogateescape")
    return [p for p in out.stdout.split("\0") if p]


# ── 现打：跨树读点矩阵（不是判据，是让上面那些「部」里的数可复算）──────────────
def cross_tree_reads():
    """粗尺：**同一行**里既出现文件读调用、又出现别的树的住址 ⇒ 记一个读点。

    ⚠ 它的失效面写清楚，别把它当权威：
      · 跨行写法（住址在上一行的常量里）**数不到**；
      · 注释里提到别的树而并没有读它 ⇒ **数多了**；
      · 只认这几种读法：`read_to_string` / `include_str!` / `readFileSync` /
        `read_dir` / `readdirSync`。
    ⇒ 它给的是**量级**，不是精确数。上面登记里那些「现打 N 个读点」就是这把尺子的读数。
    """
    read_call = re.compile(r"read_to_string|include_str!|readFileSync|read_dir|readdirSync")
    markers = {
        "scripts/": "scripts/", ".github/": ".github/", "e2e/": "e2e/", "doc/": "doc/",
        "remote-daemon-proto": "remote-daemon-proto/", "src-tauri": "src-tauri/",
        "shared/": "shared/", "hooks/": "hooks/",
    }
    matrix = {}
    for f in ls_files():
        if not f or f.rsplit(".", 1)[-1] not in ("rs", "ts", "mts", "js", "mjs"):
            continue
        if f.startswith("src-tauri/vendor/"):
            owner = VENDOR
        elif f.startswith("src-tauri/"):
            owner = "src-tauri/"
        elif f.startswith("remote-daemon-proto/"):
            owner = "remote-daemon-proto/"
        elif "/" in f:
            owner = f.split("/")[0] + "/"
        else:
            owner = ROOTFILES
        try:
            txt = (ROOT / f).read_text(encoding="utf-8", errors="replace")
        except OSError:
            continue
        for line in txt.split("\n"):
            if not read_call.search(line):
                continue
            for marker, tree in markers.items():
                if marker in line and tree != owner:
                    matrix.setdefault(owner, {}).setdefault(tree, 0)
                    matrix[owner][tree] += 1
    return matrix


# ── 现打：`gate.sh` 里到底有几格判定 ─────────────────────────────────────────
# 只认**行首**的调用 —— `gate_selftest` 里那 10 条探针都写成 `probe="$(run_gate 自检… )"`，
# 缩进着、且套在命令替换里，本正则一条都不会认（**这一条是承重的**：把探针当成判定格，
# 格数会凭空多 10 个，`C5` 当场变成一条永远对不上的假判据）。
CELL_PATTERNS = [
    (re.compile(r"^run_gate_sum (\S+) ", re.M), lambda m: m.group(1)),
    (re.compile(r"^run_gate (\S+) ", re.M), lambda m: m.group(1)),
    (re.compile(r"^run_e2e (\S+) ", re.M), lambda m: "ccm e2e/" + m.group(1)),
]
# 这两格不走那三个函数，各自手写判定 ⇒ 单独按**逐字锚点**认。
HANDWRITTEN = {
    "generated": "git diff --quiet --exit-code -- src/generated/",
    "pb check": 'fails+=("pb check（没给 PB_WS',
}
ROLLCALL = re.compile(r"GATE: OK —— (\d+) 格全绿")


def found_cells(text):
    cells = []
    for pat, name_of in CELL_PATTERNS:
        for m in pat.finditer(text):
            cells.append(name_of(m))
    for name, anchor in HANDWRITTEN.items():
        if anchor in text:
            cells.append(name)
    return cells


def verdict_of(ent, tree):
    """取一格对一棵树的裁词；登记漏了就回 `?`（不崩，让 `C3` 那句话印得出来）。"""
    v = ent["cover"].get(tree)
    return v[0] if isinstance(v, tuple) and v else "?"


def main():
    text = GATE.read_text(encoding="utf-8")
    fails = []

    cells = found_cells(text)
    dup = [c for c in set(cells) if cells.count(c) > 1]
    if dup:
        fails.append(f"C1 `gate.sh` 里同名判定格出现不止一次：{sorted(dup)} —— 登记按名字索引，同名就点不准")
    got, want = set(cells), set(REGISTRY)
    if got - want:
        fails.append(f"C1 `gate.sh` 里有格**没登记**：{sorted(got - want)} —— "
                     f"加一格就回 `evidence/K-R80-gate-cell-coverage.py` 补一条")
    if want - got:
        fails.append(f"C1 登记里有格**盘上没有**：{sorted(want - got)} —— 登记陈了")

    for name, ent in sorted(REGISTRY.items()):
        n = text.count(ent["anchor"])
        if n != 1:
            fails.append(f"C2 `{name}` 的逐字锚点在 `gate.sh` 里命中 {n} 次（应当恰好 1 次）：{ent['anchor']!r}")
        miss = [t for t in TREES if t not in ent["cover"]]
        if miss:
            fails.append(f"C3 `{name}` 对这几棵树没有裁词：{miss}")
        extra = [t for t in ent["cover"] if t not in TREES]
        if extra:
            fails.append(f"C3 `{name}` 给了不在全集里的树表态：{extra}")
        for t, v in ent["cover"].items():
            if not (isinstance(v, tuple) and len(v) == 2):
                fails.append(f"C3 `{name}` / `{t}` 的裁词不是「(裁词, 理由)」两元组")
                continue
            verdict, why = v
            if verdict not in (FULL, PART, NONE):
                fails.append(f"C3 `{name}` / `{t}` 的裁词 {verdict!r} 不在闭集 {FULL}/{PART}/{NONE} 里")
            if not (why or "").strip():
                fails.append(f"C6 `{name}` / `{t}` 的裁词没带理由 —— 空白格不算点名")

    top = {p.split("/")[0] + "/" for p in ls_files() if "/" in p}
    registered_top = {t for t in TREES if t not in (VENDOR, ROOTFILES)}
    if top != registered_top:
        fails.append(f"C4 树的全集与 `git ls-files` 现打的顶层目录对不上："
                     f"盘上多出 {sorted(top - registered_top)} · 登记里多出 {sorted(registered_top - top)}")
    for t in TREES:
        if t in (ROOTFILES,):
            continue
        if not (ROOT / t).exists():
            fails.append(f"C4 登记里的树 `{t}` 盘上不存在")

    m = ROLLCALL.search(text)
    if not m:
        fails.append("C5 `gate.sh` 末尾找不到 `GATE: OK —— <n> 格全绿` 那一行 —— 那行点名没了，本条按红处理")
    elif int(m.group(1)) != len(cells):
        fails.append(f"C5 `GATE: OK` 那行自称 {m.group(1)} 格，而现打是 {len(cells)} 格 —— 加了格而点名没跟")

    print(f"# `K-R80` `KR80D3` 门禁分格覆盖登记 —— 量于 `{GATE}`")
    print()
    print(f"判定格 **{len(cells)}** 个 · 树的全集 **{len(TREES)}** 棵"
          f"（顶层目录 {len(registered_top)} + vendor 单拆 + 仓根文件）")
    print()
    print("| 格 | cwd | 命令 | 全 | 部 | 🔴 盖不到（逐棵点名） |")
    print("|---|---|---|---|---|---|")
    order = [c for c in ("fmt", "fmt-daemon", "winchk", "cargo", "generated", "daemon", "npm")
             if c in REGISTRY] + sorted(c for c in REGISTRY if c.startswith("ccm e2e/")) + ["pb check"]
    for name in order:
        ent = REGISTRY[name]
        # ⚠ 一律走 `verdict_of` —— 直接下标会在「登记漏了一棵树」那一形上 `KeyError` 崩掉，
        #   而崩掉虽然也是非零退出，`C3` 那句「哪一格漏了」就一个字都印不出来（红了但说不清）。
        full = [t for t in TREES if verdict_of(ent, t) == FULL]
        part = [t for t in TREES if verdict_of(ent, t) == PART]
        none = [t for t in TREES if verdict_of(ent, t) == NONE]
        print(f"| `{name}` | `{ent['cwd']}` | `{ent['cmd']}` | {'·'.join(full) or '—'} | "
              f"{'·'.join(part) or '—'} | **{len(none)}**：{'·'.join(none)} |")
    print()
    print("## 反过来看：**每一棵树被几格盖到**（同一份登记的转置，不是第二把尺子）")
    print()
    print("| 树 | 全 | 部 | 盖到它的格数 |")
    print("|---|---|---|---|")
    orphans = []
    for t in TREES:
        f = [c for c in order if verdict_of(REGISTRY[c], t) == FULL]
        p = [c for c in order if verdict_of(REGISTRY[c], t) == PART]
        if not f and not p:
            orphans.append(t)
        fs = "·".join("`%s`" % x for x in f) or "—"
        ps = "·".join("`%s`" % x for x in p) or "—"
        print(f"| `{t}` | {fs} | {ps} | {len(f) + len(p)} |")
    print()
    if orphans:
        print(f"🔴 **这 {len(orphans)} 棵树今天 12 格里一格都盖不到**："
              + " · ".join(f"`{t}`" for t in orphans))
        print("⚠ `K-R80` **只数不补**（件计划 `§0d` 逐字）—— 补是另一件。")
    else:
        print("每一棵树至少被一格盖到。")
    print()
    print("## 逐格的理由（`C6`：一格都不许空）")
    for name in order:
        ent = REGISTRY[name]
        print(f"\n### `{name}`")
        for t in TREES:
            v, why = ent["cover"].get(t, ("?", "**登记漏了这一棵** —— 见上面 C3"))
            print(f"- {v} `{t}` —— {why}")
    print()
    print("## 现打：跨树读点矩阵（不是判据，是让上面那些「部」的数可复算）")
    for owner, row in sorted(cross_tree_reads().items()):
        print(f"- `{owner}` 里的读点指向：" + " · ".join(f"`{t}` {n}" for t, n in sorted(row.items())))

    print()
    if fails:
        print(f"KR80D3: FAIL={len(fails)}")
        for f in fails:
            print(f"  ✗ {f}")
        return 1
    print("KR80D3: OK —— C1..C6 全过（⚠ 它只判「登记完整且指得到真东西」，不判裁词对不对）")
    return 0


if __name__ == "__main__":
    sys.exit(main())
