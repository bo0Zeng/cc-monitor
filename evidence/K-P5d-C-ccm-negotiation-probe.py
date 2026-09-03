#!/usr/bin/env python3
"""K-P5d 摸底量具：**旧 `ccm` 与新起会话方之间那条协商通道**的清账。

# 被测对象指向哪棵树（`5k` 要的那一栏）

默认被测树 = **本文件所在的那棵工作树的仓根**（`evidence/` 的父目录），
即 `.../worktrees/k-p5d`。`--tree <路径>` 可以改。
另外它会读**这台机器 PATH 上那个 `ccm`**（只读 + 只跑 `--ccm-probe` / `--print`）。

# 它答的三问，以及每一问的分母怎么切的

- `G1` **协商通道本身**：把 `shared/ccm` 的若干个历史版本各跑一次 `--ccm-probe`，
  把「它报出来的 `version=` / `capabilities=`」与「它**实际转发**几条变量」并排。
  分母 = `--revs` 给的那几个 git revision（默认 4 个，逐个写在下面）+ PATH 上那一份。
  **切法**：`capabilities=` 按**逗号**切（不是空格 —— PM 09-02 在这一格上栽过一次）。
  转发面按**行**认：形如 `payload="export <VAR>=$(sq "$<VAR>"); $payload"` 的行，
  只数容器路那个窗口（`payload=""` → `t="$(sq "=$tmux_name:")"`）之间。

- `G2` **端到端复现**：拿 Rust 渲染器今天真会产出的那条 argv
  （`ccm resume <sid> --tmux=<名> --ccm-sid=<sid> --base`，见 `ccm_invocation::render_ccm_invocation`），
  在**外侧带着** `CCM_LAUNCH_ID` 的环境里跑 `--print`，看它拼出来的 `send-keys` 载荷里
  有没有 `export CCM_LAUNCH_ID=`。**`--print` 不起 tmux、不 mkdir、不问 daemon**
  （`CCM_NO_DAEMON=1` / `CCM_NO_PRETRUST=1` + `env -i` + 夹具 HOME）。
  分母 = `--revs` 那几个版本 + PATH 上那一份，每份跑一次。

- `G3` **外侧那一串今天 `export` 了哪几个变量**（`KP5DD2` 的人群）：
  从 `history.rs` 拼装那一行（`let cmd = relay + &launch_identity_prefix(action) + &base;`）
  逆着数**它的两个前缀生产者**各渲出哪个变量名。
  🔴 **这是源码级读数，不是行为级** —— 行为级要走 `launch_sink()` 那条缝（判据能做，本量具不编 Rust）。
  分母写在输出里：**只数拼在 `base` 外面的那两截**，`base` 内部（旧路 `config_dir_prefix_posix`）
  **不在分母里**，理由是它那一支根本不走 ccm 容器。

用法：
    python3 evidence/K-P5d-C-ccm-negotiation-probe.py [--tree <仓根>] [--revs a,b,c]
"""

from __future__ import annotations

import argparse
import os
import re
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

# `shared/ccm` 的几个历史版本。选它们的理由逐个写下来（别人复跑要看得懂分母）：
#   b25acf1 2026-08-24 K-C1 —— `capabilities=` 最后一次变（加 `account-via-daemon`），
#                              `CCM_VERSION` 也是在这一笔从 2 跳到 3。**零条转发之后的基线**？不：
#                              它有 R08 那条，没有 K-H2b / K-P5c 那两条。
#   6006822 2026-08-28 K-H2b —— 加了 `ANTHROPIC_BASE_URL` 转发。**没动 capabilities、没动 version**。
#   473184d 2026-09-02 K-P5c —— 加了 `CCM_LAUNCH_ID` 转发。**同样没动 capabilities、没动 version**。
#   HEAD                     —— 本工作树此刻这一份。
DEFAULT_REVS = ["b25acf1", "6006822", "473184d", "HEAD"]

WIN_START = '\n  payload=""\n'
WIN_END = '\n  t="$(sq "=$tmux_name:")"'
FORWARD_RE = re.compile(r'payload="export (\w+)=\$\(sq "\$\w+"\); \$payload"')

# `ccm_invocation.rs::render_ccm_invocation` 今天对「本机 UI 起 + Base 账号 + 有 tmux 名」
# 这一格产出的 argv（`history.rs::render_local_ccm_with` 喂的 spec）。
# ⚠ 这几个 token 是**手抄**的 —— 抄错就是伪造一次读数 ⇒ `--check-argv` 会拿
#    `ccm_invocation.rs` 的生产段核一遍每个 flag 名确实还在。
RENDERED_ARGV = ["resume", "S1", "--tmux=s1abcdef-cc", "--ccm-sid=S1", "--base"]


def run(cmd: list[str], env: dict | None = None) -> tuple[int, str, str]:
    p = subprocess.run(cmd, capture_output=True, text=True, env=env, timeout=30)
    return p.returncode, p.stdout, p.stderr


def fixture_env(home: Path, **extra: str) -> dict:
    """`env -i` 那一档：只留跑得起来所必需的几个，别把这台机器的配置带进读数。"""
    e = {
        "HOME": str(home),
        "PATH": "/usr/bin:/bin",
        "CCM_NO_DAEMON": "1",  # 红线：不许起真 daemon
        "CCM_NO_PRETRUST": "1",
    }
    e.update(extra)
    return e


def probe(path: Path, home: Path) -> dict:
    rc, out, err = run(["bash", str(path), "--ccm-probe"], env=fixture_env(home))
    if rc != 0:
        return {"rc": rc, "err": err.strip()[:200]}
    d: dict = {"rc": 0, "version": None, "caps": []}
    for line in out.splitlines():
        if line.startswith("version="):
            d["version"] = line[len("version="):]
        elif line.startswith("capabilities="):
            # 🔴 **逗号**切，不是空格切。
            d["caps"] = [t for t in line[len("capabilities="):].split(",") if t]
    return d


def forwards(text: str) -> list[str] | str:
    """容器路窗口里那几条转发。窗口取不到就如实回一句话，不许回空列表冒充「零条」。"""
    if text.count(WIN_START) != 1 or text.count(WIN_END) != 1:
        return f"窗口锚点不是各恰好一处（start={text.count(WIN_START)} end={text.count(WIN_END)}）⇒ 判不了"
    s, e = text.find(WIN_START), text.find(WIN_END)
    if s >= e:
        return "两个锚点先后反了 ⇒ 判不了"
    return FORWARD_RE.findall(text[s:e])


def payload_of(path: Path, home: Path, launch_id: str) -> str | None:
    """跑一次 `--print`，把 `send-keys` 那一段载荷抠出来。抠不到回 None。"""
    env = fixture_env(home, CCM_LAUNCH_ID=launch_id)
    rc, out, err = run(["bash", str(path), *RENDERED_ARGV, "--print"], env=env)
    if rc != 0:
        return None
    m = re.search(r"send-keys -t '[^']*' (.*) Enter", out, re.S)
    return m.group(1) if m else None


def check_argv_still_matches(tree: Path) -> list[str]:
    """核一遍手抄的那几个 flag 名在渲染器生产段里还在。核不上就点名。"""
    src = (tree / "src-tauri/src/backend/control/ccm_invocation.rs").read_text(encoding="utf-8")
    bad = []
    for needle in ['"resume"', '"--tmux={name}"', '"--ccm-sid={}"', '"--base"']:
        probe_s = needle.replace('{name}', '').replace('{}', '')
        if probe_s.strip('"') not in src:
            bad.append(needle)
    return bad


def _fn_body(src: str, sig: str) -> str:
    """按花括号配平切一个函数体。切不出来回空串（调用方按「抽取器坏了」处理）。"""
    i = src.find(sig)
    if i < 0:
        return ""
    j = src.find("{", i)
    if j < 0:
        return ""
    depth, k = 0, j
    while k < len(src):
        if src[k] == "{":
            depth += 1
        elif src[k] == "}":
            depth -= 1
            if depth == 0:
                return src[j:k + 1]
        k += 1
    return ""


def outer_prefix_population(tree: Path) -> dict:
    """`G3`：拼在 ccm 调用**外面**那两截各 export 了哪个变量名。

    🔴 **本函数第一版是错的，错法值得留在这里当活体**：它拿
    `"export (\\w+)=\\{\\}; "` 扫**整份 `payload.rs`**，数出 4 个
    （多出 `CLAUDE_CONFIG_DIR` 与 `ANTHROPIC_MODEL`）——
    而那两个住在 `render_env_ops`，是 **`L3 · daemon_send_into`** 那条路的载荷渲染器，
    它送进的是**已经存在**的 tmux（载荷本来就在边界内侧），**根本不经过本件说的那个边界**。
    ⇒ 尺子的作用域（一份文件）对不上它要守的性质（拼在 ccm 外面的那两截）。
    **这正是 `K-R18` 那一族**，而它长在给 `K-R18` 供语料的这次测量上。
    现在按**函数体**切，并把「同形但不在人群里」的那几处单列出来给人看。
    """
    hist = (tree / "src-tauri/src/history.rs").read_text(encoding="utf-8")
    pay = (tree / "src-tauri/src/backend/control/payload.rs").read_text(encoding="utf-8")
    join = "let cmd = relay + &launch_identity_prefix(action) + &base;"
    out: dict = {"拼装行在不在": join in hist, "拼装行处数": hist.count(join),
                 "变量": [], "同形但不在人群里": []}
    # ① 中转那一截：`payload::relay_env_prefix_posix`（**只切它的函数体**）
    body = _fn_body(pay, "pub fn relay_env_prefix_posix(")
    if not body:
        out["变量"].append(("(抽取器坏了：切不出 relay_env_prefix_posix 的体)", "-"))
    else:
        for m in re.finditer(r'"export (\w+)=\{\}; "', body):
            out["变量"].append((m.group(1), "payload.rs::relay_env_prefix_posix"))
    # ② 身份那一截：`history.rs::launch_identity_env_prefix`（变量名走常量，不是字面量）
    m = re.search(r'LAUNCH_ID_VAR: &str = "(\w+)"', hist)
    body2 = _fn_body(hist, "fn launch_identity_env_prefix(")
    if m and body2 and '"export {LAUNCH_ID_VAR}={}; "' in body2:
        out["变量"].append((m.group(1), "history.rs::launch_identity_env_prefix"))
    # 同形但**不在人群里**的：整份 `payload.rs` 里那几处减去上面切到的。
    mine = {v for v, _ in out["变量"]}
    for m2 in re.finditer(r'"export (\w+)=\{\}; "', pay):
        if m2.group(1) not in mine:
            out["同形但不在人群里"].append((m2.group(1), "payload.rs::render_env_ops（L3 送进已存在的 tmux）"))
    return out


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--tree", default=str(Path(__file__).resolve().parent.parent))
    ap.add_argument("--revs", default=",".join(DEFAULT_REVS))
    args = ap.parse_args()
    tree = Path(args.tree).resolve()
    revs = [r for r in args.revs.split(",") if r]

    print(f"# 被测树：{tree}")
    rc, head, _ = run(["git", "-C", str(tree), "rev-parse", "HEAD"])
    print(f"# 被测树尖：{head.strip()}")
    print(f"# 量于：{subprocess.run(['date', '-Is'], capture_output=True, text=True).stdout.strip()}")

    bad = check_argv_still_matches(tree)
    print(f"# 手抄 argv 自检：{'全部核上' if not bad else '❌ 核不上 ' + str(bad)}")

    tmp = Path(tempfile.mkdtemp(prefix="k-p5d-"))
    home = tmp / "home"
    home.mkdir()
    try:
        rows = []
        for rev in revs:
            rc, text, err = run(["git", "-C", str(tree), "show", f"{rev}:shared/ccm"])
            if rc != 0:
                rows.append((rev, "取不到该版本", "-", "-", "-"))
                continue
            f = tmp / f"ccm-{rev}"
            f.write_text(text, encoding="utf-8")
            p = probe(f, home)
            fw = forwards(text)
            pl = payload_of(f, home, "tok-1")
            rows.append((
                rev,
                p.get("version"),
                len(p.get("caps", [])),
                fw if isinstance(fw, str) else ",".join(fw) or "（零条）",
                "有" if (pl and "export CCM_LAUNCH_ID=" in pl) else ("无" if pl is not None else "跑不出"),
            ))

        # PATH 上那一份 —— 这台机器**真会被 `probe_local_ccm()` 探到**的那一个。
        which = shutil.which("ccm") or str(Path.home() / ".local/bin/ccm")
        if Path(which).exists():
            text = Path(which).read_text(encoding="utf-8", errors="replace")
            p = probe(Path(which), home)
            fw = forwards(text)
            pl = payload_of(Path(which), home, "tok-1")
            st = os.stat(which)
            rows.append((
                f"PATH:{which} (mtime={subprocess.run(['date','-d','@%d' % int(st.st_mtime),'+%F'],capture_output=True,text=True).stdout.strip()})",
                p.get("version"),
                len(p.get("caps", [])),
                fw if isinstance(fw, str) else ",".join(fw) or "（零条）",
                "有" if (pl and "export CCM_LAUNCH_ID=" in pl) else ("无" if pl is not None else "跑不出"),
            ))
        else:
            rows.append((f"PATH:{which}", "不存在", "-", "-", "-"))

        print("\n## G1+G2 · 报出来的（version/caps）↔ 真做得到的（转发/载荷）")
        print(f"{'版本':<58} {'ver':<5} {'caps':<5} {'转发面':<52} 载荷里有身份?")
        for r in rows:
            print(f"{str(r[0]):<58} {str(r[1]):<5} {str(r[2]):<5} {str(r[3]):<52} {r[4]}")

        print("\n## G3 · 外侧那一串今天 export 了哪几个变量（源码级；分母 = 拼在 `base` 外面的两截）")
        pop = outer_prefix_population(tree)
        print(f"拼装行「{'在' if pop['拼装行在不在'] else '不在'}」，处数 = {pop['拼装行处数']}")
        for name, where in pop["变量"]:
            print(f"  - {name}   ← {where}")
        print(f"  人群大小 = {len(pop['变量'])}")
        print("  ── 同形但**不在人群里**（尺子作用域一放宽就会混进来的那几个）：")
        for name, where in pop["同形但不在人群里"]:
            print(f"  - {name}   ← {where}")
        print("  ⚠ 不在这个分母里的还有：`base` 内部（旧路 `config_dir_prefix_posix` 的 CLAUDE_CONFIG_DIR）"
              "—— 那一支不走 ccm 容器；以及 ccm 自己从环境**继承**读的那些（R08 那一族）。")
    finally:
        shutil.rmtree(tmp, ignore_errors=True)
    return 0


if __name__ == "__main__":
    sys.exit(main())
