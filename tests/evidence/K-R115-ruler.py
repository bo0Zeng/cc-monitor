#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""K-R115 `KR115D1`：**量具还原那一跳有没有把旧 mtime 搬回被测树** —— 的机检。

## 它治的是什么

纪律 ㉒ 立于 `K-R75`（09-12）：变异台 `restore` 用 `shutil.copy2` 把**旧 mtime** 一起搬回
⇒ `cargo` 判「源码没变」⇒ 复用上一刀的产物 ⇒ **那一趟读到的是上一刀的回声**。
第二天 `K-R102`（09-13）在另一把量具上**同形复发**（`M6-final` 印 `GATE: OK · daemon 755`，
而那条判据还在盘上一趟没跑）。

⇒ 处置当时写的是一句纪律，**而纪律这一档已经被证伪过**（立了一天，第二天在另一把量具里又长出来）。
本尺子是那句纪律的**机器面**。

## 🔴 它判的不是「源码里有没有 `copy2` 这个词」

`shutil.copy2` 有正当用途 —— 造夹具、拷读数文件、把二进制搬进临时目录，
那些 `copy2` 一个都不该红。**判的是「还原被测源码那一跳用了它」**，而那一跳的可判形状是：

    这一次复制的**目的地**，落在**被 git 跟踪着的工作树内容**上。

把一份文件写回 git 管着的路径 = 你在还原被测源码；写进临时目录 / 写进一个 git 里
一个文件都没有的暂存目录 = 你在造夹具或备份。**方向与落点，不是词。**

## 人群 · 分母

分母 = `evidence/*.py` 里 `shutil` **保元数据复制族**的**调用点**（`ast` 枚举，不是 `grep` 词）：

  · `copy2`    —— 连 mtime 一起搬
  · `copytree` —— 它的默认 `copy_function` **就是** `copy2`
  · `copystat` —— 单把 mtime/权限搬过去

`grep -c copy2 evidence/*.py` 数的是**行**（注释行也算）；本尺子数的是**调用点**，
两个数不一样是对的 —— 差在哪逐处印在下面那张表里。

## 每一处的裁词（闭集，现算见 `VERDICTS`）

  · `树外`        目的地解析得出，且不在被测树里（临时目录 / 别的绝对路径）⇒ 不红
  · `树内·零跟踪`  目的地在树内，但那个前缀下 `git ls-files` **一份都没有**
                  （备份目录 / 暂存目录）⇒ 不红，但**点名**
  · `树内·有跟踪`  🔴 **红** —— 这就是还原被测源码那一跳
  · `判不了`      目的地的表达式解析不出来 ⇒ 🔴 **红**（fail-closed；
                  「判不了」与「不违规」在输出上一模一样，这一档不许静默）

## ⚠ 诚实边界（写死，别把绿读宽）

1. **只看 `evidence/*.py`**。别处的量具（`.sh` / `.mjs` / 别的目录）本尺子一眼都没看。
2. **只看 `shutil` 那三个名字**。`subprocess` 里的 `cp -a` / `cp -p`、`tarfile.extractall`
   （它也还原 mtime）、手写 `os.utime(dst, 旧时间)` —— **一概看不见**。
   现打过：`evidence/*.py` 里字面 `cp -a` 出现在容器/临时目录的 shell 串里，
   目的地是容器路径，本尺子解析不了那种串。⇒ 这是**已知的漏**，不是「没有」。
3. **判的是落点，不是意图**。把备份目录建成一个 git 跟踪着的目录 ⇒ 误红；
   还原进一个**尚未被跟踪**的新文件 ⇒ 漏。两侧都写在这里。
4. **解析器是静态的**：模块级/函数内赋值、形参按同文件内调用点回溯、
   `Path()/os.path.join()/ '/' 拼接 / .parent / .parents[n]`、`tempfile.*`、`__file__`、
   `git rev-parse --show-toplevel` 这几形认得。别的一律落 `判不了` ⇒ 红。
5. 它**不证明**「这把量具的还原真的让 cargo 重编了」——那要跑一趟。它只证明
   **没人再用「连 mtime 一起搬回去」那一跳**。

## 跑法

    python3 evidence/K-R115-ruler.py            # 判：有红 ⇒ exit 1
    python3 evidence/K-R115-ruler.py --census   # 只印人群表，恒 exit 0

环境变量 `K_R115_ROOT` 只为**死值验**存在（把本文件拷去别处跑时指回真树），日常不带。
"""

import ast
import os
import subprocess
import sys
from pathlib import Path

ROOT = Path(os.environ.get("K_R115_ROOT") or Path(__file__).resolve().parents[2])
EVIDENCE = ROOT / "evidence"

# `shutil` 里会把源文件的 mtime 一起搬到目的地的那几个名字。
# 🔴 闭集只许有一个住址 —— 下面凡是要报「有几个」的地方一律 `len(FAMILY)`，不写字面量。
FAMILY = ("copy2", "copytree", "copystat")

OUT_OF_TREE = "树外"
IN_TREE_UNTRACKED = "树内·零跟踪"
IN_TREE_TRACKED = "树内·有跟踪"
UNRESOLVED = "判不了"
VERDICTS = (OUT_OF_TREE, IN_TREE_UNTRACKED, IN_TREE_TRACKED, UNRESOLVED)
RED_VERDICTS = (IN_TREE_TRACKED, UNRESOLVED)

# 计数自检（要件 3）：扫到的调用点少于这个数 ⇒ **遍历坏了**，不是「大家都改好了」。
#   〔量于 09-14，本工作树 `track/k-r115`，`--census` 现打〕**11 处**。
#   ⚠ 本件之前是 **12** 处 —— `kg3-c1-cuts.py::cmd_oldgate` 那一处还原跳改成
#     `copyfile` ＋ `os.utime` 之后，`copy2` 少了一处 ⇒ 地板同拍从 12 拧到 11。
#   ⚠ 真删掉一处要来改这个数，**并在这里写清删的是哪一处** —— 不写就没人分得开
#     「有人修好了一处」与「遍历少扫了一份文件」。
SITE_FLOOR = 11

# ── 静态路径解析 ────────────────────────────────────────────────────────────
# 三态：("temp", None) · ("path", 绝对路径字符串) · ("unknown", 为什么)
TEMP_CALLS = {"mkdtemp", "TemporaryDirectory", "gettempdir", "mkstemp", "NamedTemporaryFile"}
# 「这是仓根」的两个可认形状。别的形状一律落 `判不了`（第 4 条边界）。
TOPLEVEL_MARK = "--show-toplevel"


def _fname(node):
    """取一个 Call 的函数名（末段）。"""
    f = node.func
    if isinstance(f, ast.Attribute):
        return f.attr
    if isinstance(f, ast.Name):
        return f.id
    return None


class Scope:
    """一个作用域（模块 或 一个函数）。**按作用域解析，不按全文件解析** ——
    全文件union会把另一个函数里同名的局部变量拌进来，答案当场变成「判不了」。"""

    def __init__(self, node, name, parent):
        self.node = node
        self.name = name
        self.parent = parent          # 词法父作用域（模块）
        self.assigns: dict[str, list] = {}
        self.params: list[str] = []
        if isinstance(node, (ast.FunctionDef, ast.AsyncFunctionDef)):
            self.params = [a.arg for a in node.args.args] + [a.arg for a in node.args.kwonlyargs]


def _own_stmts(fn):
    """一个函数体里、**不进嵌套函数**的全部节点。"""
    out = []
    stack = list(fn.body) if isinstance(fn, (ast.FunctionDef, ast.AsyncFunctionDef)) else list(fn.body)
    while stack:
        n = stack.pop()
        out.append(n)
        for ch in ast.iter_child_nodes(n):
            if isinstance(ch, (ast.FunctionDef, ast.AsyncFunctionDef, ast.ClassDef)):
                continue
            stack.append(ch)
    return out


DYN = "<dyn>"
PROP_ATTRS = {"name", "stem", "suffix", "parts"}


class Resolver:
    """一份 `.py` 的静态路径解析器。**只解析得了下面写明的那几形**（见头注第 4 条边界）。"""

    def __init__(self, path: Path, tree: ast.Module):
        self.path = path
        self.tree = tree
        self.module = Scope(tree, "<模块级>", None)
        self.scopes: dict[str, Scope] = {}
        self.fn_of_line: dict[int, str] = {}
        for node in ast.walk(tree):
            if isinstance(node, (ast.FunctionDef, ast.AsyncFunctionDef)):
                self.scopes[node.name] = Scope(node, node.name, self.module)
        # 模块级赋值：不进任何函数
        self._collect(self.module, [n for n in tree.body])
        for name, sc in self.scopes.items():
            self._collect(sc, _own_stmts(sc.node))
        # 行号 → 最内层函数名
        for name, sc in self.scopes.items():
            end = getattr(sc.node, "end_lineno", sc.node.lineno)
            for ln in range(sc.node.lineno, end + 1):
                self.fn_of_line[ln] = name
        # 函数名 → 同文件内每个调用点（连它落在哪个作用域一起记）
        self.callsites: dict[str, list] = {}
        for node in ast.walk(tree):
            if isinstance(node, ast.Call):
                n = _fname(node)
                if n in self.scopes:
                    self.callsites.setdefault(n, []).append(node)

    def _collect(self, scope, stmts):
        for n in stmts:
            if isinstance(n, (ast.FunctionDef, ast.AsyncFunctionDef, ast.ClassDef)) and scope is self.module:
                continue
            for sub in ([n] if scope is not self.module else ast.walk(n)) if False else [n]:
                pass
            if isinstance(n, ast.Assign):
                for t in n.targets:
                    if isinstance(t, ast.Name):
                        scope.assigns.setdefault(t.id, []).append(n.value)
                    elif isinstance(t, (ast.Tuple, ast.List)):
                        # `a, b = x, y` —— **逐位配对**。不配对就只能记 `None`，
                        # 而 `None` 会一路走成「判不了」⇒ 一条真话被读成判不了。
                        vals = n.value.elts if isinstance(n.value, (ast.Tuple, ast.List)) \
                            and len(n.value.elts) == len(t.elts) else None
                        for k, el in enumerate(t.elts):
                            if isinstance(el, ast.Name):
                                scope.assigns.setdefault(el.id, []).append(
                                    vals[k] if vals is not None else ("DYN",))
            elif isinstance(n, ast.AnnAssign) and isinstance(n.target, ast.Name) and n.value is not None:
                scope.assigns.setdefault(n.target.id, []).append(n.value)
            elif isinstance(n, (ast.For, ast.AsyncFor)):
                for t in ast.walk(n.target):
                    if isinstance(t, ast.Name):
                        scope.assigns.setdefault(t.id, []).append(("DYN",))
            elif isinstance(n, ast.With):
                for it in n.items:
                    if it.optional_vars is not None:
                        for t in ast.walk(it.optional_vars):
                            if isinstance(t, ast.Name):
                                scope.assigns.setdefault(t.id, []).append(it.context_expr)
            # 模块级还要往下钻一层（`if __name__` 之类里的赋值）
            if scope is self.module and isinstance(n, (ast.If, ast.Try, ast.For, ast.While, ast.With)):
                body = []
                for f in ("body", "orelse", "finalbody"):
                    body += getattr(n, f, []) or []
                self._collect(scope, body)

    def scope_at(self, lineno):
        return self.scopes.get(self.fn_of_line.get(lineno), self.module)

    def resolve(self, node, scope=None, depth=0, seen=None):
        scope = scope or self.module
        seen = seen if seen is not None else set()
        if depth > 40:
            return {("unknown", "解析层数超过 40 层")}
        if node is None:
            return {("unknown", "没有这个实参")}
        if isinstance(node, tuple) and node and node[0] == "DYN":
            return {("path", DYN)}
        key = (id(node), scope.name)
        if key in seen:
            return set()
        seen = seen | {key}

        if isinstance(node, ast.Constant):
            if isinstance(node.value, str):
                return {("path", node.value)}
            return {("unknown", f"字面量不是字符串：{node.value!r}")}

        if isinstance(node, ast.JoinedStr):
            return {("path", DYN)}

        if isinstance(node, ast.Name):
            if node.id == "__file__":
                return {("path", str(self.path))}
            out = set()
            if node.id in scope.assigns:
                for v in scope.assigns[node.id]:
                    out |= self.resolve(v, scope, depth + 1, seen)
            elif node.id in scope.params:
                sites = self.callsites.get(scope.name, [])
                if not sites:
                    return {("unknown",
                             f"`{scope.name}()` 的形参 `{node.id}` —— 本文件里一个调用点都没有，回溯不了")}
                i = scope.params.index(node.id)
                for call in sites:
                    caller = self.scope_at(call.lineno)
                    actual = call.args[i] if i < len(call.args) else None
                    if actual is None:
                        for kw in call.keywords:
                            if kw.arg == node.id:
                                actual = kw.value
                    if actual is None:
                        out |= {("unknown", f"`{scope.name}()` 有个调用点没给第 {i + 1} 个实参")}
                    else:
                        out |= self.resolve(actual, caller, depth + 1, seen)
            elif node.id in self.module.assigns:
                for v in self.module.assigns[node.id]:
                    out |= self.resolve(v, self.module, depth + 1, seen)
            else:
                return {("unknown", f"名字 `{node.id}` 在 `{scope.name}` 与模块级都找不到")}
            return out or {("unknown", f"名字 `{node.id}` 解析出空集")}

        if isinstance(node, ast.BinOp) and isinstance(node.op, ast.Div):
            return self._join(self.resolve(node.left, scope, depth + 1, seen),
                              self.resolve(node.right, scope, depth + 1, seen))

        if isinstance(node, ast.Attribute):
            if node.attr == "parent":
                return self._up(self.resolve(node.value, scope, depth + 1, seen), 1)
            if node.attr in PROP_ATTRS:
                return {("path", DYN)}
            return self.resolve(node.value, scope, depth + 1, seen)

        if isinstance(node, ast.Subscript):
            v = node.value
            if isinstance(v, ast.Attribute) and v.attr == "parents":
                idx = node.slice
                if isinstance(idx, ast.Constant) and isinstance(idx.value, int):
                    return self._up(self.resolve(v.value, scope, depth + 1, seen), idx.value)
            return {("unknown", f"下标形状认不出：{ast.unparse(node)[:60]}")}

        if isinstance(node, ast.Call):
            name = _fname(node)
            if name in TEMP_CALLS:
                return {("temp", None)}
            if name in ("Path", "PurePath", "str", "abspath", "realpath", "expanduser",
                        "resolve", "absolute"):
                if node.args:
                    return self.resolve(node.args[0], scope, depth + 1, seen)
                if isinstance(node.func, ast.Attribute):
                    return self.resolve(node.func.value, scope, depth + 1, seen)
                return {("unknown", f"`{name}()` 没有可解析的实参")}
            if name == "join" and node.args:
                out = self.resolve(node.args[0], scope, depth + 1, seen)
                for a in node.args[1:]:
                    out = self._join(out, self.resolve(a, scope, depth + 1, seen))
                return out
            if name == "dirname" and node.args:
                return self._up(self.resolve(node.args[0], scope, depth + 1, seen), 1)
            if name in ("replace", "format", "strip", "lower", "upper"):
                return {("path", DYN)}
            for a in node.args:
                if isinstance(a, ast.Constant) and a.value == TOPLEVEL_MARK:
                    return {("path", str(ROOT))}
            if name in self.scopes:
                fn = self.scopes[name]
                out = set()
                for st in _own_stmts(fn.node):
                    if isinstance(st, ast.Return) and st.value is not None:
                        out |= self.resolve(st.value, fn, depth + 1, seen)
                if out:
                    return out
            return {("unknown", f"调用形状认不出：{ast.unparse(node)[:60]}")}

        return {("unknown", f"表达式形状认不出：{ast.unparse(node)[:60]}")}

    @staticmethod
    def _join(left, right):
        out = set()
        for lk, lv in left:
            if lk != "path":
                out.add((lk, lv))
                continue
            for rk, rv in right:
                if rk == "temp":
                    out.add(("temp", None))
                elif rk == "unknown":
                    out.add((rk, rv))
                elif rv.startswith("/"):
                    out.add(("path", rv))
                else:
                    out.add(("path", str(Path(lv) / rv)))
        return out or {("unknown", "拼接的左侧是空的")}

    @staticmethod
    def _up(origins, n):
        out = set()
        for k, v in origins:
            if k != "path":
                out.add((k, v))
                continue
            p = Path(v)
            for _ in range(n):
                p = p.parent
            out.add(("path", str(p)))
        return out


# ── git 现打：这个前缀下有没有被跟踪的文件 ──────────────────────────────────
_tracked_cache: dict[str, bool] = {}


def tracked_under(abs_path: str) -> bool:
    """`abs_path`（或它最长的静态前缀）下面，`git ls-files` 现打有没有东西。"""
    p = Path(abs_path)
    parts = []
    for seg in p.parts:
        if seg == "<dyn>":
            break
        parts.append(seg)
    static = Path(*parts) if parts else p
    try:
        rel = static.relative_to(ROOT)
    except ValueError:
        return False
    key = str(rel)
    if key in _tracked_cache:
        return _tracked_cache[key]
    out = subprocess.run(["git", "-C", str(ROOT), "ls-files", "--", key],
                         capture_output=True, text=True)
    ans = bool(out.stdout.strip())
    _tracked_cache[key] = ans
    return ans


def inside_tree(abs_path: str) -> bool:
    try:
        Path(abs_path).relative_to(ROOT)
        return True
    except ValueError:
        return False


def census():
    rows = []
    files = sorted(EVIDENCE.glob("*.py"))
    broken = []
    for f in files:
        try:
            tree = ast.parse(f.read_text(encoding="utf-8"))
        except SyntaxError as e:
            broken.append(f"{f.name}: {e}")
            continue
        rv = Resolver(f, tree)
        for node in ast.walk(tree):
            if not isinstance(node, ast.Call):
                continue
            name = _fname(node)
            if name not in FAMILY:
                continue
            dst = node.args[1] if len(node.args) > 1 else None
            origins = rv.resolve(dst, rv.scope_at(node.lineno))
            verdict, why = judge(origins)
            rows.append({
                "file": f.name,
                "line": node.lineno,
                "func": rv.fn_of_line.get(node.lineno, "<模块级>"),
                "call": name,
                "dst": ast.unparse(dst) if dst is not None else "<没有第二个实参>",
                "verdict": verdict,
                "why": why,
            })
    rows.sort(key=lambda r: (r["file"], r["line"]))
    return rows, files, broken


def judge(origins):
    if any(k == "path" and inside_tree(v) and tracked_under(v) for k, v in origins):
        hit = [v for k, v in origins if k == "path" and inside_tree(v) and tracked_under(v)]
        return IN_TREE_TRACKED, f"目的地落在被 git 跟踪的树内内容上：{sorted(hit)}"
    if any(k == "unknown" for k, v in origins):
        why = "；".join(sorted(v for k, v in origins if k == "unknown"))
        return UNRESOLVED, why
    if any(k == "path" and inside_tree(v) for k, v in origins):
        hit = [v for k, v in origins if k == "path" and inside_tree(v)]
        return IN_TREE_UNTRACKED, f"树内，但那个前缀下 `git ls-files` 一份都没有：{sorted(hit)}"
    return OUT_OF_TREE, "临时目录 / 树外绝对路径"


def main() -> int:
    only_census = "--census" in sys.argv[1:]
    rows, files, broken = census()

    print("=" * 96)
    print("K-R115 `KR115D1` —— 量具还原那一跳有没有把旧 mtime 搬回被测树")
    print("=" * 96)
    print(f"· 被测树   ：{ROOT}")
    print(f"· 分母     ：`evidence/*.py` 现打 {len(files)} 份；"
          f"`shutil` 保元数据复制族 {len(FAMILY)} 个名字（{' · '.join(FAMILY)}）的**调用点** {len(rows)} 处")
    print(f"· 裁词闭集 ：{len(VERDICTS)} 档 —— {' / '.join(VERDICTS)}；其中判红的 "
          f"{len(RED_VERDICTS)} 档：{' / '.join(RED_VERDICTS)}")
    print("-" * 96)
    for r in rows:
        print(f"  {r['verdict']:<8} {r['file']}:{r['line']}  {r['call']}  "
              f"[{r['func']}]  dst = {r['dst']}")
        print(f"           {r['why']}")
    print("-" * 96)
    tally = {v: sum(1 for r in rows if r["verdict"] == v) for v in VERDICTS}
    print("· 逐档计数 ：" + " · ".join(f"{v} {tally[v]}" for v in VERDICTS))

    if only_census:
        print("（`--census`：只印人群表，不判）")
        return 0

    fails = []
    if broken:
        fails.append(f"有 {len(broken)} 份 `evidence/*.py` `ast` 解析不了 —— 遍历坏了：{broken}")
    if len(rows) < SITE_FLOOR:
        fails.append(
            f"只扫到 {len(rows)} 处调用点，地板是 {SITE_FLOOR} —— **遍历坏了**，"
            f"不是「大家都改好了」。真删掉一处要来改 `SITE_FLOOR` 并写清删的是哪一处。")
    for r in rows:
        if r["verdict"] == IN_TREE_TRACKED:
            fails.append(
                f"{r['file']}:{r['line']} `shutil.{r['call']}` 把文件写回**被 git 跟踪的**"
                f"`{r['dst']}` —— 那是「还原被测源码」那一跳，而 `{r['call']}` 连 mtime 一起搬回去，"
                f"cargo 会判「源码没变」直接复用上一刀的产物，下一趟读到的是**上一刀的回声**。"
                f"改成 `shutil.copyfile(...)` ＋ `os.utime(目的地, None)`。")
        elif r["verdict"] == UNRESOLVED:
            fails.append(
                f"{r['file']}:{r['line']} `shutil.{r['call']}` 的目的地**解析不出来**"
                f"（{r['why']}）—— 「判不了」不许当成「不违规」。"
                f"把目的地写成解析得动的形状，或来扩这把尺子的解析器。")

    if fails:
        print()
        for f in fails:
            print(f"  🔴 {f}")
        print()
        print(f"KR115D1: FAIL —— {len(fails)} 条")
        return 1
    print()
    print(f"KR115D1: OK —— {len(rows)} 处调用点，0 处把旧 mtime 搬回被跟踪的树内内容")
    # 🔴 门禁那一格的读数行：`run_gate` 靠「N passed」认这一格跑没跑
    #   （`0 passed 不是绿` 是它的另一条判定）。这个 N **就是判过的调用点数**，
    #   不是一个凑出来的 1 —— 遍历塌了它会先撞上 `SITE_FLOOR`。
    print(f"copy2: {len(rows)} passed")
    return 0


if __name__ == "__main__":
    sys.exit(main())
