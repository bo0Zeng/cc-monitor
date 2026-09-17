#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""`K-R118` `KR118D2` 的量具 —— 交给 PM 的那两样读数，**现算，不抄**。

## 它答两问

    --places    「版本号那六处到底是哪六处」——🔴 **从判据里读，不自己数**：
                解析 `src-tauri/src/doc_claim_registry.rs` 里
                `the_release_version_is_the_same_in_all_six_places` 的**函数体**，
                把它每一处 `pick(who, &文件, 锚点)` 抠出来，再照它自己的锚点去盘上取值。
    --commits   「`v3.7.0..HEAD` 那一批提交怎么分档」—— 按**动到的路径**机械分三档，
                分母与每一档的判别式逐条印出来。

## ⚠ 它买得到什么、买不到什么（写死，别读宽）

**买得到**
  · `--places` 的清单**跟着判据走**：那条判据哪天多一处 / 少一处 / 换个锚点，
    本量具的输出当场跟着变 —— 它不复述一份「六处是哪六处」的手抄表。
  · `--places` 复用判据自己的**命中数纪律**（锚点必须恰好 1 次），
    命中 0 次或多次一律当场报出来，不静默取第一个。
  · `--commits` 每一档的判别式是**路径**，可复算；每一条都带 sha 与主题。

**买不到**
  · 它**不是判据**，不进门禁、不 `exit 1` 报违例（除非自己解析不动）。
    「六处一致」那条闸是 `doc_claim_registry` 那个 `#[test]`，本文件只是它的**读数口**。
  · `--places` **只认 `pick(...)` 这一种写法**。判据哪天改成别的取值方式，
    本量具会当场说「解析不动」，**不会**给一个自信的错答案。
  · `--commits` 分的是**动到哪棵树**，不是「用户感不感知」——
    「产品面」这一档里混着大量内部重构（现打的数就说明了这件事）。
    ⇒ 它给的是**筛子**，最后哪几条进 CHANGELOG 要人读一遍。
  · ⚠ **`v3.7.0..HEAD` 里的 `HEAD` 是每轮都变的量** ⇒ 输出里逐字带上量于哪个提交。
"""

import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
REG = ROOT / "src-tauri/src/doc_claim_registry.rs"
FN = "fn the_release_version_is_the_same_in_all_six_places()"


def die(msg: str) -> "int":
    print(f"🔴 解析不动：{msg}")
    return 3


def fn_body(text: str) -> str:
    """抠出那个 `#[test] fn` 的函数体 —— 收尾判据是**列 4 的 `}`**（它住在 `mod tests` 里）。"""
    at = text.find(FN)
    if at < 0:
        raise SystemExit(f"🔴 `{REG.name}` 里找不到 `{FN}` —— 判据改名了，先修本量具再谈读数")
    lines = text[at:].split("\n")
    end = next((i for i, l in enumerate(lines[1:], 1) if l == "    }"), None)
    if end is None:
        raise SystemExit("🔴 找不到列 4 的收尾 `}` —— 段界读法坏了")
    return "\n".join(lines[: end + 1])


def places():
    text = REG.read_text(encoding="utf-8")
    body = fn_body(text)

    # ① `let (a, b, ...) = (rd("路径"), ...)` —— 变量名 ↔ 文件路径
    m = re.search(r"let \(([^)]*)\) = \(\s*(.*?)\s*\);", body, re.S)
    if not m:
        return die("抠不到那条 `let (...) = (rd(...), ...)` 绑定")
    names = [x.strip() for x in m.group(1).split(",") if x.strip()]
    files = re.findall(r'rd\("([^"]+)"\)', m.group(2))
    if len(names) != len(files):
        return die(f"变量 {len(names)} 个 vs `rd()` {len(files)} 个，对不上")
    var2file = dict(zip(names, files))

    # ② 每一处 `pick("who", &var, "needle")`
    calls = re.findall(r'pick\(\s*"((?:[^"\\]|\\.)*)"\s*,\s*&(\w+)\s*,\s*"((?:[^"\\]|\\.)*)"\s*\)', body)
    if not calls:
        return die("一处 `pick(...)` 都抠不到")

    # ③ 权威源：`let authority = pick(...)` 那一处
    auth = re.search(r'let authority = pick\(\s*"((?:[^"\\]|\\.)*)"', body)
    auth_who = auth.group(1) if auth else None

    rows = []
    for who, var, needle in calls:
        f = var2file.get(var)
        if f is None:
            return die(f"`pick` 里的变量 `{var}` 不在那条 `let` 绑定里")
        raw = needle.encode().decode("unicode_escape") if "\\" in needle else needle
        hay = (ROOT / f).read_text(encoding="utf-8")
        n = hay.count(raw)
        at = hay.find(raw)
        lineno = hay.count("\n", 0, at) + 1 if at >= 0 else None
        val = None
        if at >= 0:
            rest = hay[at + len(raw):]
            mm = re.match(r"[0-9.]+", rest)
            val = mm.group(0) if mm else None
        rows.append({"who": who, "file": f, "needle": raw, "hits": n,
                     "line": lineno, "value": val,
                     "authority": who == auth_who})

    print(f"# 版本号那几处 —— 现算于 `{REG.relative_to(ROOT)}::"
          f"the_release_version_is_the_same_in_all_six_places` 的函数体")
    print(f"# 判据点名 **{len(rows)} 处**（这个数就是判据里 `pick()` 的调用点数，不是手数的）")
    print()
    print("| # | 判据里的名字 | 文件 | 逐字锚点 | 行 | 现值 | 命中 |")
    print("|---|---|---|---|---|---|---|")
    for i, r in enumerate(rows, 1):
        tag = " 🔴权威源" if r["authority"] else ""
        # 锚点里带换行（`\n  "version": "` 这一族）⇒ 印之前转义，别把表格撑断。
        shown = r["needle"].replace("\n", "\\n").replace("|", "\\|")
        print(f"| {i} | {r['who']}{tag} | `{r['file']}` | `{shown}` | "
              f"{r['line']} | {r['value']} | {r['hits']} |")
    print()
    bad = [r for r in rows if r["hits"] != 1]
    if bad:
        print("🔴 锚点命中数不是 1 的：" + " · ".join(f"{r['who']}（{r['hits']} 次）" for r in bad))
    vals = {r["value"] for r in rows}
    print(f"现打：{len(rows)} 处里出现 {len(vals)} 个不同的值 ⇒ {sorted(v for v in vals if v)}"
          + ("（一致）" if len(vals) == 1 else " 🔴 **不一致**"))
    print()
    print("⚠ **判据够不着、而发版路上真的会拦你的另外两处**（现打，逐处给住址）：")
    print("  · `src-tauri/Cargo.lock` 里 `name = \"monitor\"` 紧跟的 `version` ——"
          " 守它的是 `.github/workflows/release.yml` 的 `Verify version consistency with tag`"
          "（那一步比的是**四处**：`package.json` · `tauri.conf.json` · `Cargo.toml` · `Cargo.lock`），"
          "而上面那条 `#[test]` 的分母里**没有它**。")
    print("  · `remote-daemon-proto/Cargo.toml` 的 `version` **刻意恒是 `0.0.0`**，"
          "同一步 guard 盯着它别被人顺手改 —— **它不参加这次 bump**。")
    lock = ROOT / "src-tauri/Cargo.lock"
    mm = re.search(r'(?m)^name = "monitor"\nversion = "([^"]+)"', lock.read_text(encoding="utf-8"))
    print(f"  ⇒ 现打 `src-tauri/Cargo.lock` 的 monitor version = {mm.group(1) if mm else '<抠不到>'}")
    dv = re.search(r'(?m)^version = "([^"]+)"',
                   (ROOT / "remote-daemon-proto/Cargo.toml").read_text(encoding="utf-8"))
    print(f"  ⇒ 现打 `remote-daemon-proto/Cargo.toml` version = {dv.group(1) if dv else '<抠不到>'}")
    return 0


DOC_SUFFIX = (".md",)


def classify(paths):
    """三档的判别式**只看路径**，逐条写死在这里（分母 = 这一段代码）。"""
    if not paths:
        return "空提交"
    if all(p.endswith(DOC_SUFFIX) or p.startswith("doc/") or p == "LICENSE" for p in paths):
        return "纯文档"
    inner = all(
        p.startswith(("evidence/", "scripts/", "e2e/", ".github/", "hooks/", "doc/"))
        or p.endswith(DOC_SUFFIX)
        or ".test." in p or ".vitest." in p or p.endswith("vitest.config.ts")
        for p in paths
    )
    if inner:
        return "内部判据与量具"
    return "产品面"


def area(paths):
    """产品面那一档再按**动到哪一层**打标（给人读 CHANGELOG 时当筛子用）。"""
    f = any(p.startswith("src/") and ".test." not in p and ".vitest." not in p
            and not p.startswith("src/generated/") for p in paths)
    g = any(p.startswith("src/generated/") for p in paths)
    b = any(p.startswith("src-tauri/src/") for p in paths)
    d = any(p.startswith("remote-daemon-proto/src") for p in paths)
    s = any(p.startswith("shared/") for p in paths)
    return ("F" if f else "-") + ("G" if g else "-") + ("B" if b else "-") \
        + ("D" if d else "-") + ("S" if s else "-")


def git(*a):
    return subprocess.run(["git", "-C", str(ROOT), *a],
                          capture_output=True, text=True, encoding="utf-8",
                          errors="surrogateescape").stdout


def commits(base="v3.7.0"):
    head = git("rev-parse", "HEAD").strip()
    total = len(git("rev-list", f"{base}..HEAD").split())
    shas = git("rev-list", "--no-merges", "--reverse", f"{base}..HEAD").split()
    print(f"# `{base}..HEAD` 分档 —— 量于 **{head[:8]}**（`HEAD` 每轮都变，这个 sha 就是本次的分母）")
    print(f"# 分母：`git rev-list {base}..HEAD` 现打 **{total}** 条；"
          f"去掉合并提交剩 **{len(shas)}** 条 —— 下面全部按这 {len(shas)} 条算")
    print(f"# `{base}` 那个 tag 指着 {git('rev-parse', base).strip()[:8]}"
          f"（{git('log', '-1', '--format=%ci', base).strip()[:10]}）")
    print()
    rows = []
    for c in shas:
        ps = [p for p in git("show", "--name-only", "--format=", "-z", c).split("\0") if p]
        subj = git("log", "-1", "--format=%s", c).strip()
        rows.append((classify(ps), area(ps), c[:8], subj, len(ps)))
    for k in ("产品面", "内部判据与量具", "纯文档", "空提交"):
        n = sum(1 for r in rows if r[0] == k)
        if n:
            print(f"- **{k}**：{n} 条")
    print()
    print("判别式（逐条，就是本文件 `classify()` 的字面）：")
    print("  · **纯文档** = 动到的每一份都是 `*.md` / `doc/` / `LICENSE`")
    print("  · **内部判据与量具** = 每一份都落在 `evidence/` `scripts/` `e2e/` `.github/` "
          "`hooks/` `doc/` 或 `*.test.*` / `*.vitest.*` 里")
    print("  · **产品面** = 其余（动到了会进构建的代码）")
    print("  ⚠ 「产品面」**不等于**「用户可见」：这一档里混着大量内部重构，"
          "所以下面对它再按层打一个标 —— F=`src/` 前端 · G=`src/generated/` · "
          "B=`src-tauri/src/` · D=daemon · S=`shared/`")
    print()
    for k in ("产品面", "内部判据与量具", "纯文档", "空提交"):
        sel = [r for r in rows if r[0] == k]
        if not sel:
            continue
        print(f"## {k}（{len(sel)} 条）")
        for _, a, c, s, nf in sel:
            print(f"{a} {c} ({nf:3d}f) {s}")
        print()
    return 0


def main() -> int:
    a = sys.argv[1] if len(sys.argv) > 1 else ""
    if a == "--places":
        return places()
    if a == "--commits":
        return commits(sys.argv[2] if len(sys.argv) > 2 else "v3.7.0")
    print(__doc__)
    return 2


if __name__ == "__main__":
    sys.exit(main())
