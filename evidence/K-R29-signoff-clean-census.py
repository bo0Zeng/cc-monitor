#!/usr/bin/env python3
"""K-R29 量具：把「已量·未见写面」那一档的判决**现打一遍**。

被测对象 = **本量具所在的那棵工作树**（`__file__` 往上两级 = 仓根），不接受别处的树；
要量别的树就把这份 .py 拷过去跑，别改这里的默认值 —— 本仓 `5k`（同名量具被换成
指向另一棵树的版本，输出长得一模一样）。

它现打三样：
  ① 签字表 `SIGNED` 里判档 == `MEASURED_CLEAN` 的那几条（**现算，不手抄**），
     以及每一条在 `Cargo.toml` 里那行 `path = "…"`（= 它的源码住址）；
  ② 两把尺子各自在那几棵 `src` 上的命中：
       · **护栏自己那两张模式表 + 起进程点**（`FS_MUTATION_PATTERNS` ∪
         `WHITELIST_STILL_FORBIDDEN` ∪ `Command::new(`）—— 这把是 `KR29D2` 要用的那把，
         也是签字行逐字写的那把；
       · **PM 立件时那把粗尺子**（`§0a.4` 逐字），留着做口径对照；
  ③ 非空对照 `creds-core`（表里唯一那条 `MEASURED_WRITES`）—— 同一把尺子在它身上
     必须 > 0。零命中既可能是干净、也可能是尺子瞎了，这一栏把两者分开。

⚠ 本量具不判红，只出读数。判红的是 `readonly_guard.rs` 里的判据。
⚠ 覆盖面：只覆盖 `MEASURED_CLEAN` 那一档里**仓内**的那几条。`UNMEASURED` 那一档
   （源码不在树里）**不在本量具的分母里**，也不许拿本量具的绿去说它们。

用法：
    python3 evidence/K-R29-signoff-clean-census.py            # 两把尺子都量
    python3 evidence/K-R29-signoff-clean-census.py --strip    # 另加「剥掉 cfg(test) 之后」一栏
"""

import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
GUARD = ROOT / "remote-daemon-proto" / "src" / "readonly_guard.rs"
MANIFEST = ROOT / "remote-daemon-proto" / "Cargo.toml"

# `§0a.4` 逐字：PM 立件时那把粗尺子。留着做口径对照，**不是**判据要用的那把。
PM_RULER = [
    "Command::new",
    "fs::write",
    "fs::create",
    "fs::remove",
    "fs::rename",
    "File::create",
    "OpenOptions",
    "set_permissions",
    "create_dir",
]


def const_str_array(src: str, name: str) -> list[str]:
    """把 `const <name>: &[&str] = &[ "a", "b", ];` 里那几个字面量现算出来。"""
    i = src.find(f"const {name}:")
    if i < 0:
        raise SystemExit(f"读不到 const {name} —— 量具与被测对象漂开了")
    start = src.index("&[", src.index("= &[", i))
    depth, j = 0, start
    while j < len(src):
        if src[j] == "[":
            depth += 1
        elif src[j] == "]":
            depth -= 1
            if depth == 0:
                break
        j += 1
    body = src[start : j + 1]
    return re.findall(r'"((?:[^"\\]|\\.)*)"', body)


def signed_rows(src: str) -> list[tuple[str, str]]:
    """`SIGNED` 表里每条 `(crate 名, 判档常量名)` —— 现算，不手抄第二份。"""
    i = src.find("const SIGNED:")
    if i < 0:
        raise SystemExit("读不到 const SIGNED —— 量具与被测对象漂开了")
    start = src.index("= &[", i) + len("= &[") - 1
    depth, j = 0, start
    while j < len(src):
        if src[j] == "[":
            depth += 1
        elif src[j] == "]":
            depth -= 1
            if depth == 0:
                break
        j += 1
    body = src[start : j + 1]
    rows = []
    for m in re.finditer(
        r"\(\s*(\"[^\"]+\"|GATED_CRATE)\s*,\s*(DEPS|DEV_DEPS)\s*,\s*"
        r"(MEASURED_WRITES|MEASURED_CLEAN|UNMEASURED)\s*,",
        body,
    ):
        name = m.group(1).strip('"')
        if name == "GATED_CRATE":
            name = re.search(r'const GATED_CRATE: &str = "([^"]+)"', src).group(1)
        rows.append((name, m.group(3)))
    return rows


def dep_path(manifest: str, name: str) -> str | None:
    """清单里那条依赖的 `path = "…"`；没有 path ⇒ 不是仓内 crate ⇒ 本量具扫不了它。"""
    for line in manifest.splitlines():
        s = line.strip()
        if s.startswith("#") or "=" not in s:
            continue
        key = s.split("=", 1)[0].strip()
        if key != name:
            continue
        m = re.search(r'path\s*=\s*"([^"]+)"', s)
        return m.group(1) if m else None
    return None


def strip_cfg_test(src: str) -> str:
    """与 `readonly_guard::tests::strip_cfg_test` 同款：行首锚点 + 括号配平。"""
    ATTR = "#[cfg(test)]"
    out, rest, first = [], src, True
    while True:
        if first and rest.startswith(ATTR):
            pos = 0
        else:
            k = rest.find("\n" + ATTR)
            if k < 0:
                break
            pos = k + 1
        first = False
        out.append(rest[:pos])
        after = rest[pos:]
        b, s = after.find("{"), after.find(";")
        is_block = (b >= 0) and (s < 0 or s > b)
        if not is_block:
            end = (s + 1) if s >= 0 else len(after)
        else:
            depth, end = 0, b
            while end < len(after):
                if after[end] == "{":
                    depth += 1
                elif after[end] == "}":
                    depth -= 1
                    if depth == 0:
                        end += 1
                        break
                end += 1
        rest = after[end:]
    out.append(rest)
    return "".join(out)


def census(src_dir: Path, needles: list[str], strip: bool) -> list[str]:
    """逐处命中：`相对路径:行号: 那一行原文`（**不截断**，调用方自己决定印几条）。"""
    hits = []
    if not src_dir.is_dir():
        return [f"<{src_dir} 不存在>"]
    for p in sorted(src_dir.rglob("*.rs")):
        text = p.read_text(encoding="utf-8", errors="replace")
        if strip:
            text = strip_cfg_test(text)
        for n, line in enumerate(text.splitlines(), 1):
            for pat in needles:
                if pat in line:
                    hits.append(f"{p.relative_to(src_dir.parent.parent)}:{n}: {line.strip()}")
                    break
    return hits


def main() -> int:
    strip_too = "--strip" in sys.argv
    guard = GUARD.read_text(encoding="utf-8")
    manifest = MANIFEST.read_text(encoding="utf-8")

    guard_ruler = sorted(
        set(const_str_array(guard, "FS_MUTATION_PATTERNS"))
        | set(const_str_array(guard, "WHITELIST_STILL_FORBIDDEN"))
        | {"Command::new("}
    )
    rows = signed_rows(guard)
    clean = [n for n, v in rows if v == "MEASURED_CLEAN"]
    writes = [n for n, v in rows if v == "MEASURED_WRITES"]

    print(f"# 被测对象：{ROOT}")
    print(f"# 签字表 SIGNED 现算 {len(rows)} 条 · MEASURED_CLEAN {len(clean)} 条 · "
          f"MEASURED_WRITES {len(writes)} 条")
    print(f"# 护栏尺子（两张模式表 ∪ 起进程点）现算 {len(guard_ruler)} 项：{guard_ruler}")
    print(f"# PM 粗尺子（§0a.4 逐字）{len(PM_RULER)} 项：{PM_RULER}")
    print()

    for title, names in (("MEASURED_CLEAN（本件覆盖）", clean),
                         ("MEASURED_WRITES（非空对照）", writes)):
        print(f"== {title} ==")
        for name in names:
            rel = dep_path(manifest, name)
            if rel is None:
                print(f"{name:18s} 清单那一行没有 path= ⇒ **不是仓内 crate，本量具扫不了它**")
                continue
            src_dir = (MANIFEST.parent / rel / "src").resolve()
            g = census(src_dir, guard_ruler, strip=False)
            p = census(src_dir, PM_RULER, strip=False)
            extra = ""
            if strip_too:
                gs = census(src_dir, guard_ruler, strip=True)
                extra = f" · 护栏尺子(剥 cfg(test) 后)={len(gs)}"
            n_files = len(list(src_dir.rglob("*.rs"))) if src_dir.is_dir() else 0
            print(f"{name:18s} path={rel}  .rs={n_files:3d}  "
                  f"护栏尺子={len(g):3d}  PM 粗尺子={len(p):3d}{extra}")
            for h in g[:40]:
                print(f"    护栏尺子 ▸ {h}")
            if len(g) > 40:
                print(f"    …（还有 {len(g) - 40} 处，本行是量具自己截的，不是读数）")
        print()
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
