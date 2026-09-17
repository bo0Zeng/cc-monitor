#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""`K-R124` 的判据本体 —— **发版那条流水线的两件事：闸没被偷偷改过 · 正文是我们写的那份。**

# 它守的性质是（两句，只许有两句）

1. 〔`KR114D1`，`K-R114` 09-14 立〕`release.yml` 里「发不发布」那个闸
   （`env.PUBLISH` 的**字面** ＋ 每一处「往 Release 上写」与 CI 门那一步的 `if:`）
   **与钉在本文件里的那一份逐字相同**。
2. 〔`KR124D2`，本件 09-15 立〕**每一处「往 Release 上写」的步骤都带一个正文来源**，
   而且那份正文**同一个 job 里真的有人生成它**、生成器盘上真的在、生成出来的段真的非空。

# 🔴 它为什么住在这里，而不是住在 `ci.yml` 的 `run:` 块里

`K-R114`（`d1a0552`）把整段判据写进 `ci.yml` 的 `run: |` 里，而 **runner 会把 `run:` 块里的
`${{ … }}` 先求值再交给 shell** ⇒ 上面第 1 条要比的那个字面 `CANON_ENV` 在渲染之后变成
`"false"`（`push` / tag 上是 `"true"`），**与盘上那串模板在三个触发器上都必不相等**
⇒ **这条守卫从加进去那天起就不可能过**。云端实打读数住
`evidence/K-R123-发版读数.md § 1.3`（run `34928839471`，日志里逐字 `CANON_ENV = "false"`）。

⇒ 修法选的是单子给的第一条路 —— **把字面从 `run:` 块里挪出去**，而且挪得比「经 `env:` 传」更远：
**整段判据搬进这份 `.py`**。`.py` 不经 GitHub 的表达式渲染器，`${{` 在这里就是四个字符。
选它不选转义（`${{` → `${{ '${{' }}` 之类）的理由写在交回里，一句话：
**转义只治「这一个字面」，搬出去同时治了「这段判据在本地跑不起来」** —— 而后者才是
它坏了一个月没人看见的原因（`KR124D3`）。

# 🔴 刻意不用 PyYAML（`KR124D3` 选的乙）

沙箱镜像 `ccmon-devbox:latest` 里 `python3 -c 'import yaml'` 是
`ModuleNotFoundError: No module named 'yaml'`（**2026-09-15 现打**，命令
`docker run --rm ccmon-devbox:latest python3 -c 'import yaml'`）。
原版判据用 `yaml.safe_load` 写 ⇒ 它**在本地从来跑不起来** ⇒ 只能在云端切刀，
而云端每切一刀要一趟 CI。下面那个 `parse_workflow()` 是本文件自带的 **YAML 子集切块器**，
口径与 `evidence/K-R122-ruler.py` 的 `ci_steps()` 同源（那一份只切 `steps:`，这一份要
`on:` / `env:` / `jobs:` / 每一步的 `with:`，所以写全了）。

# ⚠ 它买不到什么（逐条，别读宽）

1. **它不执行 GitHub 的表达式求值器。**「这个表达式在那个事件下真的是 false 吗」这一格靠的是
   「字面与钉住的那一份逐字相同」，不是求值。等价改写（`== 'true'` 写成 `!= 'false'`）会被
   判红 —— 假阳，但方向是安全的那一侧。
2. **「往 Release 上写」只认两种形状**：`uses:` 是 `softprops/action-gh-release`，
   或 `run:` 里出现 `gh release` / `gh api` 打 `/releases`。
   **换第三种路子上传（手写 `curl` 打 `uploads.github.com`）它看不见。**
3. 它读的是**盘上的 `release.yml`**，不是某一次 run 真正跑的那一份。
4. **它不证明「云端真会绿」。** 它证明的是「盘上这份文本满足这几条」。
   本地绿 ＋ 云端红这一形，`K-R119` 那趟已经实打过一次（读数住 `evidence/K-R119-发版读数.md`）。
5. 第 2 条性质里「正文真的是我们写的那份」，机器认的是
   **「有 `body_path` · 不是 `generate_release_notes` · 那个路径同 job 里有人生成 · 生成器跑得出非空的段」**。
   **正文写得对不对、好不好，它一个字都不判**（那要读语义）。
6. **切块器是手写的 YAML 子集**，不是 YAML 实现。锚点、多文档、流式映射（`{a: 1}`）、
   复杂 key 一概不支持 —— 挡这一形的是下面那几条**地板**（`on` 解析得出来 · job 数 ·
   step 总数 · 「往 Release 上写」至少两处 · CI 门恰好一处）：切块器坏了地板先红，
   而不是静默地「零违例」。

# 跑法

    python3 evidence/K-R124-ruler.py                      # 判本树
    K_R124_ROOT=<别的树> python3 evidence/K-R124-ruler.py   # 死值验：对着变异过的副本跑
    RELEASE_WORKFLOW=<某份 release.yml> python3 …          # 只换被测的那一份

退出码 0 = 过；1 = 有违例 / 地板没过 / 切块器坏了。
最后一行恒印 `release-gate: <N> passed（…）`，那个 `N` = 上面逐行印出来的 `PASS` 条数。
"""
import os
import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(os.environ.get("K_R124_ROOT") or Path(__file__).resolve().parent.parent)
TARGET = Path(os.environ.get("RELEASE_WORKFLOW") or (ROOT / ".github" / "workflows" / "release.yml"))

#: 🔴 **闭集只有这一个住址**：`release.yml` 里「发不发布」那个闸的字面，钉在这两个常量上。
CANON_ENV = "${{ github.event_name == 'push' || inputs.publish == true }}"
CANON_IF = "env.PUBLISH == 'true'"

#: 正文生成器的住址（相对仓根）。`release.yml` 两处发布步骤的 `body_path` 都由它产出。
RENDERER = "scripts/release-notes.mjs"


# ── YAML 子集切块器 ──────────────────────────────────────────────────────────
_BLOCK_SCALAR = {"|", ">", "|-", ">-", "|+", ">+"}
_KEY_RE = re.compile(r"^([^:]+):(?:\s+(.*))?$")


def _strip_comment(line):
    """剥掉**引号外**的行内注释。块标量的正文不走这条路（`files: |` 里 `#` 不是注释）。"""
    out = []
    quote = None
    for i, ch in enumerate(line):
        if quote:
            out.append(ch)
            if ch == quote:
                quote = None
            continue
        if ch in "\"'":
            quote = ch
            out.append(ch)
            continue
        if ch == "#" and (i == 0 or line[i - 1] in " \t"):
            break
        out.append(ch)
    return "".join(out).rstrip()


def _scalar(raw):
    raw = raw.strip()
    if len(raw) >= 2 and raw[0] == raw[-1] and raw[0] in "\"'":
        return raw[1:-1]
    if raw == "true":
        return True
    if raw == "false":
        return False
    if raw in ("", "null", "~"):
        return None
    if re.fullmatch(r"-?\d+", raw):
        return int(raw)
    return raw


def _indent_of(s):
    return len(s) - len(s.lstrip())


def _read_block_scalar(lines, i, parent_indent):
    body, block_indent = [], None
    while i < len(lines):
        raw = lines[i]
        if raw.strip() == "":
            body.append("")
            i += 1
            continue
        ind = _indent_of(raw)
        if ind <= parent_indent:
            break
        if block_indent is None:
            block_indent = ind
        body.append(raw[block_indent:] if len(raw) >= block_indent else raw.lstrip())
        i += 1
    while body and body[-1] == "":
        body.pop()
    return ("\n".join(body) + "\n") if body else "", i


def _first_content(lines, i, indent):
    while i < len(lines):
        s = _strip_comment(lines[i])
        if not s.strip():
            i += 1
            continue
        if _indent_of(s) < indent:
            return None, i
        return s, i
    return None, i


def _parse_block(lines, i, indent):
    s, i = _first_content(lines, i, indent)
    if s is None:
        return None, i
    if s.lstrip().startswith("- "):
        return _parse_seq(lines, i, _indent_of(s))
    return _parse_map(lines, i, _indent_of(s))


def _parse_map(lines, i, indent):
    out = {}
    while i < len(lines):
        s = _strip_comment(lines[i])
        if not s.strip():
            i += 1
            continue
        ind = _indent_of(s)
        if ind < indent:
            break
        if ind > indent:          # 子块已被递归吃掉，剩下的只可能是切块器够不着的形状
            i += 1
            continue
        body = s.strip()
        if body.startswith("- "):
            break
        m = _KEY_RE.match(body)
        if not m:
            i += 1
            continue
        key = _scalar(m.group(1))
        rest = (m.group(2) or "").strip()
        i += 1
        if rest in _BLOCK_SCALAR:
            val, i = _read_block_scalar(lines, i, indent)
        elif rest == "":
            val, i = _parse_block(lines, i, indent + 1)
        else:
            val = _scalar(rest)
        out[key] = val
    return out, i


def _parse_seq(lines, i, indent):
    out = []
    while i < len(lines):
        s = _strip_comment(lines[i])
        if not s.strip():
            i += 1
            continue
        ind = _indent_of(s)
        if ind < indent:
            break
        if ind > indent:
            i += 1
            continue
        body = s.strip()
        if not body.startswith("- "):
            break
        inner = body[2:]
        if _KEY_RE.match(inner):
            # `- key: v` ⇒ 把那个短横换成两个空格，整项当一份缩进 +2 的映射解析。
            patched = list(lines)
            patched[i] = lines[i][:ind] + "  " + lines[i][ind + 2:]
            item, i = _parse_map(patched, i, ind + 2)
            out.append(item)
        else:
            out.append(_scalar(inner))
            i += 1
    return out, i


def parse_workflow(text):
    doc, _ = _parse_block(text.split("\n"), 0, 0)
    return doc if isinstance(doc, dict) else {}


# ── 判据 ────────────────────────────────────────────────────────────────────
def run_checks(emit):
    fails = []
    passes = [0]

    def check(ok, label, detail):
        emit(("PASS  " if ok else "FAIL  ") + label + " :: " + detail)
        if ok:
            passes[0] += 1
        else:
            fails.append(label)

    if not TARGET.exists():
        emit("::error::读不到 %s —— 判不了，按红记" % TARGET)
        return 1, 0, ["地板·读得到 release.yml"]
    doc = parse_workflow(TARGET.read_text(encoding="utf-8"))
    on = doc.get("on")
    jobs = doc.get("jobs") or {}

    # ── 地板：先证够得到，再问有没有违例（否则「零违例」是空真）──────────────
    check(isinstance(on, dict) and len(on) > 0, "地板·触发器解析得出来",
          "on 的键 = %r" % (sorted(map(str, on)) if isinstance(on, dict) else on,))
    check(len(jobs) >= 4, "地板·job 数",
          "%d 个 job（分母 = release.yml 顶层 jobs 的键数）" % len(jobs))
    steps = []
    for jname, j in (jobs.items() if isinstance(jobs, dict) else []):
        for idx, st in enumerate(((j or {}).get("steps") or [])):
            if isinstance(st, dict):
                steps.append((jname, idx, st))
    check(len(steps) >= 20, "地板·step 总数",
          "%d 步（分母 = 全部 job 的 steps 逐条）—— 切块器坏了这个数会塌" % len(steps))
    if fails:
        emit("::error::地板没过 —— 后面的判据一律不算数")
        return 1, passes[0], fails

    # ── ① 触发得了吗 ────────────────────────────────────────────────────────
    wd = on.get("workflow_dispatch", "<缺>")
    check(wd != "<缺>", "①触发得了（有 workflow_dispatch）", "on.workflow_dispatch = %r" % (wd,))

    # ── ② 手工触发默认不发布 ─────────────────────────────────────────────────
    inputs = (wd or {}).get("inputs", {}) if isinstance(wd, dict) else {}
    pub = inputs.get("publish") if isinstance(inputs, dict) else None
    check(isinstance(pub, dict), "②有 publish 这个输入",
          "inputs 的键 = %r" % (sorted(inputs) if isinstance(inputs, dict) else inputs,))
    if isinstance(pub, dict):
        check(pub.get("type") == "boolean", "②publish 是 boolean", "type=%r" % (pub.get("type"),))
        check(pub.get("default") is False, "②publish 默认 false",
              "default=%r（这一格红 = 随手点一下就发一个版）" % (pub.get("default"),))

    # ── ③ 那个量只有一个住址，且字面就是钉住的那一份 ───────────────────────────
    env_pub = (doc.get("env") or {}).get("PUBLISH")
    check(env_pub == CANON_ENV, "③env.PUBLISH 与钉住的字面逐字相同", "盘上=%r" % (env_pub,))

    # ── ④ 每一处「往 Release 上写」都挂着那个闸 ─────────────────────────────
    pubsteps = []
    for jname, idx, st in steps:
        uses = str(st.get("uses") or "")
        run = str(st.get("run") or "")
        writes = uses.startswith("softprops/action-gh-release") \
            or "gh release" in run \
            or ("gh api" in run and "/releases" in run)
        if writes:
            pubsteps.append((jname, idx, st))
    check(len(pubsteps) >= 2, "④地板·找得到「往 Release 上写」的步骤",
          "%d 处（分母 = 全部 job 的全部 step 共 %d 个）" % (len(pubsteps), len(steps)))
    for jname, idx, st in pubsteps:
        sname = st.get("name") or st.get("uses")
        check(st.get("if") == CANON_IF, "④闸·%s / %s" % (jname, sname), "if=%r" % (st.get("if"),))

    # ── ⑤ CI 门那一步：只在要发布时拦（拦法与闸同一个住址）───────────────────
    gate = [st for _, _, st in steps if "green CI run" in str(st.get("name") or "")]
    check(len(gate) == 1, "⑤地板·找得到 CI 门那一步", "%d 处" % len(gate))
    if len(gate) == 1:
        check(gate[0].get("if") == CANON_IF, "⑤CI 门挂在同一个闸上",
              "if=%r（这一格红 = 真发版那条路上 CI 门可能被跳过）" % (gate[0].get("if"),))

    # ── ⑥ 每一处发布步骤都带正文来源（`KR124D2`）─────────────────────────────
    #   失效方向逐字：只给 Windows 那处加 ⇒ Linux 那处照样发 GitHub 自动生成的。
    #   ⇒ 这一条对 `pubsteps` **逐条**判，不是「至少有一处带」。
    paths = []
    for jname, idx, st in pubsteps:
        sname = st.get("name") or st.get("uses")
        with_ = st.get("with") or {}
        bp = with_.get("body_path")
        inline = with_.get("body")
        gen = with_.get("generate_release_notes")
        check(bool(bp) or bool(inline), "⑥正文来源·%s / %s" % (jname, sname),
              "body_path=%r · body=%r（这一格红 ⇒ 真发出去的正文是 GitHub 自动生成那份）"
              % (bp, inline))
        check(gen is not True, "⑥不回落自动生成·%s / %s" % (jname, sname),
              "generate_release_notes=%r" % (gen,))
        if bp:
            paths.append((jname, idx, str(bp)))
    check(len(paths) == len(pubsteps), "⑥地板·每一处发布步骤都有 body_path",
          "%d/%d 处（分母 = 上面数出来的发布步骤）" % (len(paths), len(pubsteps)))
    check(len({p for _, _, p in paths}) <= 1, "⑥正文只有一个住址",
          "body_path 的取值集合 = %r" % (sorted({p for _, _, p in paths}),))

    # ── ⑦ 那份正文**同一个 job 里真的有人生成它**，而且排在发布步骤前面 ───────
    #   口径与 `K-R122` 的「build 排在 e2e 前面」同源：认字面量 + 认书写顺序。
    for jname, idx, bp in paths:
        producers = [i for jn, i, st in steps
                     if jn == jname and RENDERER in str(st.get("run") or "")
                     and bp in str(st.get("run") or "") and i < idx]
        check(bool(producers), "⑦生成器排在发布步骤前面·%s" % jname,
              "`%s` 在本 job 第 %s 步之前%s" % (RENDERER, idx,
                                              ("被第 %d 步调用，且那一行点名了 `%s`" % (min(producers), bp))
                                              if producers else
                                              "**没有任何一步既调它、又点名 `%s`** —— "
                                              "那个 body_path 没人产出，真发版时 action 读不到它" % bp))

    # ── ⑧ 生成器盘上真的在，而且真的吐得出非空的正文 ──────────────────────────
    #   🔴 这一条是本条的**死值**那一半：不是「不报错」，是**有数**。
    rp = ROOT / RENDERER
    check(rp.exists(), "⑧地板·生成器在盘上", "%s" % rp)
    if rp.exists():
        try:
            proc = subprocess.run(["node", str(rp), "--check"], cwd=str(ROOT),
                                  capture_output=True, text=True, timeout=60)
            line = (proc.stdout or proc.stderr).strip().splitlines()
            check(proc.returncode == 0, "⑧生成器吐得出本版正文",
                  (line[-1] if line else "（无输出）") + "（rc=%d）" % proc.returncode)
        except FileNotFoundError:
            check(False, "⑧生成器吐得出本版正文",
                  "这台机器上没有 `node` —— **判不了，按红记**（不许退化成静默跳过）")
        except subprocess.TimeoutExpired:
            check(False, "⑧生成器吐得出本版正文", "`node` 跑超时 60s —— 判不了，按红记")

    return (1 if fails else 0), passes[0], fails


def main():
    out = []
    rc, n, fails = run_checks(out.append)
    print("\n".join(out))
    if fails:
        print("---- 红 %d 条（分母 = 上面逐行 PASS/FAIL 打出来的那几条）----" % len(fails))
        print("::error::release.yml 发版守卫红了：" + " / ".join(fails))
        return rc
    print("release-gate: %d passed（分母 = 上面逐行印出来的 PASS 条数；被测对象 `%s`。"
          "⚠ 它不执行 GitHub 的表达式求值器、只认两种「往 Release 上写」的形状、"
          "不判正文写得对不对 —— 逐条射程写在本文件头注）" % (n, TARGET))
    return rc


if __name__ == "__main__":
    sys.exit(main())
