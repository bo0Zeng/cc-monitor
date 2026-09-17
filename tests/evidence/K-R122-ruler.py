#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""`K-R122` `KR122D1` ③④ 的判据本体 —— **CI 里每一套硬门后端二进制的 e2e，前置得跟上。**

# 它守的性质是（一句，只许有一句）

`.github/workflows/ci.yml` 里每一条跑 e2e 的步骤，如果**它的被测对象是那个后端二进制**，
那么**同一个 job 里必须有一条真的 build，而且排在它前面**。

# 它扫的人群是（一句，只许有一句）

`ci.yml` 的 `steps:` 里每一条形如 `bash e2e/assert-pass-floor.sh <套件> <地板>` 的 `run:`
（现打 20 条），逐条按 `package.json` 的 `scripts["test:<套件>"]` 解析到那份 `.sh`，
再看那份 `.sh`（**连同它逐字点名的 `e2e/*.sh` 助手**）里有没有 `debug/cc-monitor-remote`。

# 🔴 它从哪来 —— 不是设想，是 `K-R119` 推 tag 那一趟云端实打出来的两条红

`K-R48` 第二拍（09-11）把 `ccm` 收成后端的原生命令、删掉 `shared/ccm`；
`K-R104`（09-13）把用量探针整条重写成「往真 daemon 的帧面写帧」。
**两次都换了被测对象，而 `ci.yml` 里那两处 job 的前置一次都没跟上** ⇒
`E2E real-machine (tmux, no Rust)` 与 `E2E real-machine (tmux + Rust builder emit)`
双双红在「找不到原生入口 / 需要先 build daemon」（读数住
`evidence/K-R119-发版读数.md` 的第六节）。

★ **形状**：这不是「测试坏了」，是**前置没跟上产品变化**。而它在本地
**一个字都看不见** —— `scripts/gate.sh` 自己在跑四套 ccm e2e 之前**有一步 build**，
CI 那两个 job 没有；两边的「绿」长得一模一样。

# ⚠ 它买不到什么（逐条，别读宽）

1. **它不跑任何 e2e**，只读三份盘上的文本（`ci.yml` · `package.json` · 那些 `.sh`）。
   「前置齐了」不等于「那一套会绿」。
2. **认「要不要后端二进制」靠一个字面量** `debug/cc-monitor-remote`。
   哪天有人换个写法拿到那个二进制（环境变量、别的路径），本条**看不见它**
   ⇒ 那时这条判据在那一套上静默。挡这一形的是下面的**地板**（人群非空 ＋ 命中非空）。
3. **认「有 build」靠 `cargo build`**。一条 `cargo build` 不一定编的就是那个 bin ——
   本条**不追**它编了什么（`--manifest-path` 指哪、`--bin` 是谁）。
   ⇒ 它买的是「**这个 job 里有人编过东西，而且排在前面**」，不是「编出来的正是那一份」。
4. 它只看 `steps:` 的**书写顺序**。`if:` / `continue-on-error` / 复合动作里藏的 build，一概不认。
5. 它不判 job 装没装 Rust 工具链 —— 但那一条是**蕴含**的：没有工具链，那条 `cargo build`
   自己会红，而它排在前面 ⇒ 后面那一套根本走不到。

# 跑法

    python3 evidence/K-R122-ruler.py            # 判本树
    K_R122_ROOT=<别的树> python3 evidence/K-R122-ruler.py

退出码 0 = 过；1 = 有违例 / 抽取器坏了。最后一行恒印 `ci-e2e-prereq: <N> passed（…）`，
那个 `N` 就是**判过的步骤条数** —— 抽取器坏了它是 0，而门禁那条
「`0 passed` 不是绿」会当场把这一格判红（fail-closed 的那一半靠它）。
"""
import json
import os
import re
import sys
from pathlib import Path

ROOT = Path(os.environ.get("K_R122_ROOT") or Path(__file__).resolve().parent.parent)
CI = ROOT / ".github" / "workflows" / "ci.yml"
PKG = ROOT / "package.json"

#: 「这一套的被测对象是后端二进制」的**唯一**认法。射程写在头注第 2 条。
BINARY_NEEDLE = "debug/cc-monitor-remote"
#: 「这一步真编了东西」的认法。射程写在头注第 3 条。
BUILD_NEEDLE = "cargo build"
#: e2e 调用行的形状。
CALL_RE = re.compile(r"bash\s+e2e/assert-pass-floor\.sh\s+(\S+)\s+(\d+)")
#: 顶层 job 键：恰好两个空格 + 名字 + 冒号（与 `shared_crate_registry` 那把切法同口径）。
JOB_RE = re.compile(r"^  ([A-Za-z0-9_-]+):\s*$")
#: 一条步骤的起点。
STEP_RE = re.compile(r"^      - ")
#: 脚本里逐字点名的 `e2e/*.sh` 助手（只跟一层，够不着的写在头注里）。
HELPER_RE = re.compile(r"e2e/([A-Za-z0-9._-]+\.sh)")


def ci_steps(text):
    """切出 `(job, 序号, 步骤名, 这一步的 run 文本)`。

    **刻意不用 PyYAML** —— 现打（09-14）沙箱镜像 `ccmon-devbox:latest` 里
    `import yaml` 是 `ModuleNotFoundError`，而本判据要在门禁里跑。
    ⚠ 注释行（`^\\s*#`）一律剔掉：`ci.yml` 的注释里逐字抄过调用行，不剔会数出假的。
    """
    job = None
    out = []
    idx = -1
    step = None
    collecting = False
    base_indent = 0

    def flush():
        if step is not None:
            out.append(step)

    for raw in text.split("\n"):
        if raw.strip().startswith("#"):
            continue
        m = JOB_RE.match(raw)
        if m and not raw.startswith("      "):
            flush()
            step = None
            collecting = False
            job = m.group(1)
            idx = -1
            continue
        if job is None:
            continue
        if STEP_RE.match(raw):
            flush()
            idx += 1
            step = {"job": job, "i": idx, "name": None, "run": ""}
            collecting = False
        if step is None:
            continue
        stripped = raw.strip()
        if collecting:
            # 块标量：缩进比 `run:` 那一行深就还在块里。
            if raw.strip() == "" or (len(raw) - len(raw.lstrip())) > base_indent:
                step["run"] += raw.strip() + "\n"
                continue
            collecting = False
        if stripped.startswith("- name:"):
            step["name"] = stripped[len("- name:"):].strip()
        elif stripped.startswith("name:"):
            step["name"] = stripped[len("name:"):].strip()
        elif stripped.startswith("- run:") or stripped.startswith("run:"):
            body = stripped.split("run:", 1)[1].strip()
            base_indent = len(raw) - len(raw.lstrip())
            if body in ("|", ">", "|-", ">-"):
                collecting = True
            else:
                step["run"] += body + "\n"
    flush()
    return out


def suite_script(scripts, suite):
    """`test:<套件>` 那条 npm 脚本指向的 `.sh`（拿不到 ⇒ None）。"""
    cmd = scripts.get(f"test:{suite}")
    if not cmd:
        return None
    m = re.search(r"(e2e/[A-Za-z0-9._-]+)", cmd)
    return m.group(1) if m else None


def needs_binary(rel):
    """那份 `.sh`（含它逐字点名的 `e2e/*.sh` 助手）里有没有那个字面量。"""
    p = ROOT / rel
    if not p.exists():
        return None, f"`{rel}` 盘上不存在"
    text = p.read_text(encoding="utf-8", errors="replace")
    blob = text
    for h in sorted(set(HELPER_RE.findall(text))):
        hp = ROOT / "e2e" / h
        if hp.exists():
            blob += hp.read_text(encoding="utf-8", errors="replace")
    return (BINARY_NEEDLE in blob), None


def main():
    fails = []
    if not CI.exists():
        print(f"ci-e2e-prereq: 读不到 {CI} —— 判不了")
        return 1
    steps = ci_steps(CI.read_text(encoding="utf-8"))
    scripts = json.loads(PKG.read_text(encoding="utf-8")).get("scripts", {})

    calls = []
    builds = {}
    for st in steps:
        if BUILD_NEEDLE in st["run"]:
            builds.setdefault(st["job"], []).append(st["i"])
        m = CALL_RE.search(st["run"])
        if m:
            calls.append((st, m.group(1), int(m.group(2))))

    # 抽取器自检①：人群非空。切块坏了它就是 0，而 0 违例看起来和全过一模一样。
    if not calls:
        print("ci-e2e-prereq: 从 ci.yml 一条 e2e 调用行都没抽到 —— 抽取器坏了，判不了")
        return 1

    rows = []
    need_n = 0
    for st, suite, floor in calls:
        rel = suite_script(scripts, suite)
        if rel is None:
            fails.append(f"套件 `{suite}`（{st['job']} 第 {st['i']} 步）在 `package.json` 里"
                         f"找不到 `test:{suite}` 或它没指向一份 `e2e/*.sh` —— 判不了，按红记")
            rows.append((st["job"], st["i"], suite, "?", "?", "找不到脚本"))
            continue
        need, why = needs_binary(rel)
        if need is None:
            fails.append(f"套件 `{suite}`：{why}")
            rows.append((st["job"], st["i"], suite, rel, "?", why))
            continue
        if not need:
            rows.append((st["job"], st["i"], suite, rel, "否", "—"))
            continue
        need_n += 1
        before = [i for i in builds.get(st["job"], []) if i < st["i"]]
        if before:
            rows.append((st["job"], st["i"], suite, rel, "是", f"前置在第 {min(before)} 步"))
        else:
            has_any = builds.get(st["job"], [])
            tail = (f"同 job 里那条 build 在第 {min(has_any)} 步 —— **排在它后面**"
                    if has_any else "同 job 里**一条 build 都没有**")
            rows.append((st["job"], st["i"], suite, rel, "是", "🔴 " + tail))
            fails.append(
                f"`{st['job']}` 第 {st['i']} 步（{st['name']}）跑 `{suite}`，"
                f"而 `{rel}` 硬门 `{BINARY_NEEDLE}`，{tail}。\n"
                f"    ⇒ 这一步在云端**必红**，而红的不是被测对象，是前置。"
                f"处置只有两条：把 build 挪到它前面，或者把这一步搬到有 build 的那个 job 去。"
            )

    # 抽取器自检②：**命中非空**。一条命中 0 的闸是空真（闸死了 `[] == []` 照样绿）。
    if need_n == 0:
        print(f"ci-e2e-prereq: 判了 {len(calls)} 条调用行，而「要后端二进制」的**一条都没有** —— "
              f"那个字面量 `{BINARY_NEEDLE}` 全不命中，本条此刻是空真，判不了")
        return 1

    print(f"# `K-R122` `KR122D1` ③④ —— CI 的 e2e 前置对账（量于 `{CI}`）")
    print()
    print(f"调用行 **{len(calls)}** 条 · 其中要后端二进制的 **{need_n}** 条")
    print()
    print("| job | 步 | 套件 | 脚本 | 要二进制 | 前置 |")
    print("|---|---|---|---|---|---|")
    for r in rows:
        print("| `%s` | %s | `%s` | `%s` | %s | %s |" % r)
    print()
    if fails:
        print(f"ci-e2e-prereq: FAIL={len(fails)}")
        for f in fails:
            print(f"  ✗ {f}")
        return 1
    print(f"ci-e2e-prereq: {len(calls)} passed（判过的 e2e 调用行数；其中 {need_n} 条要后端二进制，"
          f"逐条都有一条 `{BUILD_NEEDLE}` 排在它前面、同 job。"
          f"⚠ 本条不跑任何 e2e，只读盘上的文本；射程逐条写在本文件头注）")
    return 0


if __name__ == "__main__":
    sys.exit(main())
