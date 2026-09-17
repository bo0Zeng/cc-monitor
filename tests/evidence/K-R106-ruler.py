#!/usr/bin/env python3
"""`K-R106` 的尺子。**只读**，不改任何文件。

住址：本文件住在 `<本工作树>/evidence/`；`ROOT` = **它所在的那棵树的仓根**
（`brief` 第 12 条：量具住址要唯一定位到那一份被测对象 —— 不许拿 `cwd` 猜）。

四把尺子，各自的分母都在自己的输出里写清楚：

  A `creation`   —— `daemon_kill.rs::CREATION_PATHS` 的**登记条数**与**遍历实得**
                     （遍历口径逐字照抄 `creation_detect::creates_a_session`）。
  B `seat`       —— `SESSION_BACKEND` / `renderFallback` 在 **TS 生产段**里的逐文件处数
                     （尺子 A：标识符出现几次；剥注释口径照抄 `launch_wire::production_ts`）。
  C `reach`      —— 尺子 B：那几个消费者文件**有没有生产调用方**（本文件之外被引过没有）。
  D `localps`    —— `LocalPsAction` 在 `history.rs` **生产段**里的匹配点逐处。
  E `docsentence`—— 「过渡期回落」那句话在耐久文档人群（`doc/**/*.md`）里的逐处命中。

用法：`python3 evidence/K-R106-ruler.py [creation|seat|reach|localps|docsentence|all]`
"""
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent


# ── 剥注释：Rust 生产段（近似 `guard_core::production_code` 的整行 + 行尾两步）──
def rust_prod(src: str) -> str:
    # 去掉 `#[cfg(test)] mod tests { ... }` 到文件尾（本仓测试段一律在尾部单模块）
    m = re.search(r"^#\[cfg\(test\)\]\nmod tests \{", src, re.M)
    if m:
        src = src[: m.start()]
    src = re.sub(r"/\*.*?\*/", "", src, flags=re.S)
    out = []
    for line in src.splitlines():
        if line.lstrip().startswith("//"):
            continue
        i = line.find("//")
        out.append(line[:i] if i >= 0 else line)
    return "\n".join(out)


def ts_prod(src: str) -> str:
    out = []
    for line in src.splitlines():
        t = line.lstrip()
        if t.startswith("//") or t.startswith("*") or t.startswith("/*"):
            continue
        i = line.find("//")
        out.append(line[:i] if i >= 0 else line)
    return "\n".join(out)


def creation() -> None:
    kill = (ROOT / "src-tauri/src/backend/control/daemon_kill.rs").read_text(encoding="utf8")
    block = kill.split("const CREATION_PATHS: &[(&str, CreationVerdict, &str)] = &[", 1)[1]
    block = block.split("\n    ];", 1)[0]
    # 只取每个元组的**第一个**字符串（路径）—— 理由那一列也是 12 空格缩进的字符串，
    # 用「缩进 + 引号」当锚点会把理由一起数进来（第一版就是这么数出 6 条的）。
    registered = [
        m.group(1)
        for m in re.finditer(r'\n        \(\n\s*(?://[^\n]*\n\s*)*"([^"]+)",', block)
    ]
    print(f"【A creation】登记条数 = {len(registered)}（分母 = `CREATION_PATHS` 表里的行）")
    for r in registered:
        print(f"   · {r}")

    verb = "new-" + "session"
    wide = f"tmux {verb}"
    argv = f'"{verb}", "-d"'
    found, scanned = [], 0
    for d in ("src-tauri/src", "remote-daemon-proto/src", "src", "shared"):
        for p in sorted((ROOT / d).rglob("*")):
            if not p.is_file():
                continue
            name = p.name
            if ".test." in name or ".vitest." in name:
                continue
            ext = p.suffix.lstrip(".")
            if ext not in ("rs", "ts", "sh", ""):
                continue
            try:
                raw = p.read_text(encoding="utf8")
            except Exception:
                continue
            scanned += 1
            body = rust_prod(raw) if ext == "rs" else raw
            hit = any(
                (not l.lstrip().startswith(("//", "#", "*"))) and (wide in l or argv in l)
                for l in body.splitlines()
            )
            if hit:
                found.append(str(p.relative_to(ROOT)))
    found.sort()
    print(f"【A creation】遍历实得 = {len(found)}（分母 = 四个根下扫到的 {scanned} 个文件，"
          f"口径 = `tmux {verb}` 或 argv 形态 `{argv}`，跳过 `.test.`/`.vitest.`）")
    for f in found:
        print(f"   · {f}")
    print(f"【A creation】登记 == 实得 ? {sorted(registered) == found}")


SEAT_SYMS = ("SESSION_BACKEND", "renderFallback")


def _ts_files():
    for p in sorted((ROOT / "src").rglob("*.ts")):
        if ".test." in p.name or ".vitest." in p.name:
            continue
        yield p


def seat() -> None:
    print("【B seat（尺子A：标识符在 TS 生产段里出现几处）】"
          "分母 = `src/**/*.ts` 去掉 `*.test.ts` / `*.vitest.ts` 之后的全部文件")
    for sym in SEAT_SYMS:
        rows = []
        for p in _ts_files():
            n = ts_prod(p.read_text(encoding="utf8")).count(sym)
            if n:
                rows.append((str(p.relative_to(ROOT)), n))
        total = sum(n for _, n in rows)
        print(f"  {sym}: {len(rows)} 个文件 / {total} 处")
        for f, n in rows:
            note = "  ← 定义本身" if f.endswith("session-backend.ts") and sym == "SESSION_BACKEND" else ""
            note = note or ("  ← 定义本身" if f.endswith("launch-render-fallback.ts") and sym == "renderFallback" else "")
            print(f"     · {f}: {n}{note}")


def reach() -> None:
    """尺子B：某个消费者文件导出的符号，在**别的** TS 生产段里有没有被引。"""
    print("【C reach（尺子B：这个消费者文件今天还站不站在生产路上）】"
          "分母 = 该文件的导出符号在 `src/**` 别处生产段里的引用处数")
    targets = {
        "src/launch-render-fallback.ts": ["renderFallback"],
        "src/remote-launch-run.ts": [
            "renderLaunchCommand", "runLocalResumeIntoExistingTmux",
            "runRemoteResumeTmux", "runRemoteLauncher", "runNewSessionRemote",
            "runRemoteResumeDirect", "runRemoteResumeIntoExistingTmux", "runAttachRemote",
        ],
        "src/remote-launch.ts": [
            "buildResumeDirectCmd", "buildResumeTmuxCmd", "buildSendIntoCmd",
            "buildLauncherCmd", "buildAttachCmd",
        ],
        "src/launch-payload-golden.ts": ["GOLDEN_CASES", "renderGolden"],
        "src/session-backend.ts": ["SESSION_BACKEND", "TMUX_BACKEND"],
    }
    for owner, syms in targets.items():
        hits = []
        for p in _ts_files():
            rel = str(p.relative_to(ROOT))
            if rel == owner:
                continue
            body = ts_prod(p.read_text(encoding="utf8"))
            for s in syms:
                n = len(re.findall(rf"\b{re.escape(s)}\b", body))
                if n:
                    hits.append((rel, s, n))
        verdict = "OnProductionPath" if hits else "OffProductionPath"
        print(f"  {owner}: {verdict}（{len(hits)} 处）")
        for rel, s, n in hits:
            print(f"     · {rel}: {s} × {n}")


def localps() -> None:
    src = (ROOT / "src-tauri/src/history.rs").read_text(encoding="utf8")
    prod = rust_prod(src)
    print("【D localps】`LocalPsAction` 在 `history.rs` **生产段**里的逐处"
          "（分母 = 剥掉整行/行尾注释与 `#[cfg(test)] mod tests` 之后的全文）")
    total = 0
    for i, line in enumerate(prod.splitlines(), 1):
        if "LocalPsAction" in line:
            total += line.count("LocalPsAction")
            print(f"   · 生产段第 {i} 行 ×{line.count('LocalPsAction')}: {line.strip()[:110]}")
    print(f"【D localps】生产段合计 {total} 处")
    whole = src.count("LocalPsAction")
    print(f"【D localps】整份文件（含测试段与注释）{whole} 处 —— 两个数分母不同，别混")


def docsentence() -> None:
    needle = "过渡期" + "回落"
    docs = sorted((ROOT / "doc").rglob("*.md"))
    said = [str(p.relative_to(ROOT)) for p in docs if needle in p.read_text(encoding="utf8")]
    print(f"【E docsentence】人群 = `doc/**/*.md` 共 {len(docs)} 份；"
          f"今天还写着那句话的 = {len(said)} 份")
    for f in said:
        print(f"   · {f}")
    others = []
    for p in sorted(ROOT.rglob("*.md")):
        if "node_modules" in p.parts:
            continue
        rel = str(p.relative_to(ROOT))
        if rel.startswith("doc/"):
            continue
        if needle in p.read_text(encoding="utf8"):
            others.append(rel)
    print(f"【E docsentence】⚠ **人群之外**还命中 {len(others)} 处（登记，不是漏）：{others}")


if __name__ == "__main__":
    which = sys.argv[1] if len(sys.argv) > 1 else "all"
    for name, fn in (("creation", creation), ("seat", seat), ("reach", reach),
                     ("localps", localps), ("docsentence", docsentence)):
        if which in ("all", name):
            fn()
            print()
