#!/usr/bin/env python3
"""K-R126 量具 —— 在**沙箱里**反复跑 `relay::server::tests` 那一族，数红。

住址：`<本仓>/evidence/K-R126-ruler.py`（跟着工作树走）。
被测对象：**本文件所在的那棵工作树**（由 `__file__` 现算，见 `WT`）——
刻意不接受「树」参数，免得同一个住址先后指向两棵树、跑出一张静默的假读数表
（`brief` 硬规则 12 逐字点名的那一形）。

🔴 K31：一切 cargo 动作都在 `ccmon-devbox:latest` 容器里跑，宿主上一条都不跑。
   挂载与环境**逐项抄自** `.claude/devbox/gate`（PROJ / cargo registry 卷 / HOME / CARGO_TARGET_DIR）。
   与 `gate` 的差别只有一处、且是刻意的：它跑 `scripts/gate.sh` 全量门禁，
   本量具跑**指名的那几条测试**并循环 N 趟 —— `KR126D2` 刀③ 要「正常一趟跑很多遍」，
   全量门禁跑不了那个次数。收官那一趟仍然跑真 `gate`。

用法：
  K-R126-ruler.py build                      # 只编，不跑
  K-R126-ruler.py list                       # 整族的**权威名单**（问测试二进制要，不数源码）
  K-R126-ruler.py run  --runs=N [--threads=K] [--filter=F ...] [--cpuset=C]
  K-R126-ruler.py family --runs=N            # 整族（relay::server::tests::）
  K-R126-ruler.py census                     # 整族**逐条单跑**，数每条撞上几次「下游先走」
退出码：0 = 全绿；1 = 有红；3 = 环境不对。

⚠ `census` 要先落 `K-R126-cut.py apply probe-connection-outcomes` 那把量具刀，
  否则一条 `[KR126CENSUS]` 都数不到 —— 它会**当场报出来**，不许把「没落刀」读成「没人撞上」。
"""
import argparse
import json
import os
import pathlib
import re
import subprocess
import sys

WT = pathlib.Path(__file__).resolve().parent.parent          # 被测对象 = 这棵树
PROJ = pathlib.Path("/home/zbl/文档/claudecode-frontend")
TARGETS = PROJ / ".claude" / "pm-targets"
TAG = WT.name
CACHE_VOL = "ccmon-cargo-registry"
IMAGE = "ccmon-devbox:latest"
CRATE_DIR = WT / "remote-daemon-proto"

# 那一族的前缀。**只有一个住址**，下面所有命令都取它。
FAMILY = "relay::server::tests::"


def _docker(inner: str, cpuset: str | None = None, timeout: int = 3600):
    cmd = ["docker", "run", "--rm", "--network", "none"]
    if cpuset:
        cmd += ["--cpuset-cpus", cpuset]
    cmd += [
        "-v", f"{PROJ}:{PROJ}",
        "-v", f"{CACHE_VOL}:/opt/rust/cargo/registry",
        "-e", f"CARGO_TARGET_DIR={TARGETS / TAG}",
        "-e", "HOME=/home/zbl",
        "-w", str(CRATE_DIR),
        IMAGE,
        "bash", "-o", "pipefail", "-c", inner,
    ]
    return subprocess.run(cmd, capture_output=True, text=True, timeout=timeout)


def preflight():
    if not (WT / "src-tauri").is_dir():
        sys.exit(f"❌ 不像工作树（没有 src-tauri/）：{WT}")
    if subprocess.run(["docker", "image", "inspect", IMAGE],
                      capture_output=True).returncode != 0:
        sys.exit(f"❌ 镜像 {IMAGE} 不在 —— 不许退回宿主跑（K31）")
    (TARGETS / TAG).mkdir(parents=True, exist_ok=True)


def build() -> tuple[str, bool]:
    """编出测试二进制，返回 (路径, 这一趟重编了几个 crate)。

    ⚠ 第二个返回值不是装饰 —— `cargo` 按 **mtime** 判新旧，
    而「把源码还原回去」这个动作很容易还原出一个**比上一趟编译还老**的 mtime
    ⇒ 它跳过重编，你下一趟量的是**上一刀那个二进制**，输出长得一模一样。
    调用方要把「重编了没有」印出来，让读的人自己对得上账。
    """
    # ⚠ 「重编了没有」要在 `tail` **之前**数 —— 本仓的编译输出里警告几十行，
    #   `Compiling` 那一行早就被 `tail -40` 切掉了，照着 tail 后的文本判会恒答「否」。
    r = _docker(
        "cargo test --no-run 2>&1 | tee /tmp/kr126.build.log | tail -40; "
        "echo \"__COMPILED__=$(grep -c '^ *Compiling' /tmp/kr126.build.log)\""
    )
    m = re.search(r"Executable unittests \S+ \((\S+)\)", r.stdout)
    if not m:
        print(r.stdout[-4000:], file=sys.stderr)
        sys.exit("❌ 没能从 `cargo test --no-run` 的输出里认出测试二进制")
    c = re.search(r"__COMPILED__=(\d+)", r.stdout)
    if not c:
        sys.exit("❌ 数不出「重编了几个 crate」—— 按 CRASH 记，不许猜")
    return m.group(1), int(c.group(1))


def run(binpath: str, runs: int, threads: int, filters: list[str],
        cpuset: str | None, keep_red: pathlib.Path | None):
    """跑 runs 趟，返回 (red, total, 第一趟红的全文)。"""
    filt = " ".join(f"'{f}'" for f in filters)
    inner = (
        f'red=0; first=""; '
        f'for i in $(seq 1 {runs}); do '
        f'  if ! {binpath} --test-threads={threads} {filt} > /tmp/kr126.$$.out 2>&1; then '
        f'    red=$((red+1)); '
        f'    if [ -z "$first" ]; then first=1; cp /tmp/kr126.$$.out /tmp/kr126.firstred; fi; '
        f'  fi; '
        f'  grep -h "^test result:" /tmp/kr126.$$.out || true; '
        f'done; '
        f'echo "__RED__=$red"; '
        f'if [ -f /tmp/kr126.firstred ]; then echo "__FIRSTRED__"; cat /tmp/kr126.firstred; fi'
    )
    r = _docker(inner, cpuset=cpuset)
    out = r.stdout
    m = re.search(r"__RED__=(\d+)", out)
    if not m:
        print(out[-4000:], file=sys.stderr)
        print(r.stderr[-2000:], file=sys.stderr)
        sys.exit("❌ 量具自己没跑成 —— 按 CRASH 记，不许读成「新红 0」")
    red = int(m.group(1))
    body = out.split("__FIRSTRED__", 1)[1] if "__FIRSTRED__" in out else ""
    if keep_red and body:
        keep_red.write_text(body, encoding="utf-8")
    # ★ 分母：**最后一趟**那行 `test result:` 原样带回来。
    #   「跑了 0 条」和「跑了 35 条全过」在 `RED=0` 上一模一样（`brief` 12「差集为空要附非空对照」）。
    tail = re.findall(r"^test result:.*$", out, re.M)
    return red, runs, body, (tail[-1] if tail else "")


def family_names(binpath: str) -> list[str]:
    """整族的权威名单 —— **问测试二进制要**（`brief` 硬规则 5：量真实输出，不量源码）。

    ⚠ 数源码里的 `#[test]` 与问二进制要，是两把**作用域不同**的尺子：
    前者数不出 `cfg` 关掉的、也数不出别的文件里同族的。读数要写明用的是哪一把。
    """
    r = _docker(f"{binpath} --list '{FAMILY}'")
    names = re.findall(r"^(\S+): test$", r.stdout, re.M)
    return [n for n in names if n.startswith(FAMILY)]


def census(binpath: str):
    """整族**逐条单跑**，数每条撞上几次「下游在响应写完之前走了」。

    一条一个进程 ⇒ `[KR126CENSUS]` 那几行的归属**不靠并发下的交错去猜**。
    要先落 `probe-connection-outcomes` 那把量具刀。
    """
    names = family_names(binpath)
    inner_names = "\n".join(names)
    inner = (
        f"cat > /tmp/kr126.names <<'EOF'\n{inner_names}\nEOF\n"
        f'while read -r t; do '
        f'  echo "@@@TEST $t"; '
        f'  out=$({binpath} --test-threads=1 --nocapture --exact "$t" 2>&1); rc=$?; '
        f'  echo "@@@RC $rc"; '
        f'  echo "$out" | grep -E "\\[KR126CENSUS\\]" || true; '
        f'done < /tmp/kr126.names'
    )
    r = _docker(inner)
    if "@@@TEST" not in r.stdout:
        print(r.stdout[-3000:], file=sys.stderr)
        print(r.stderr[-2000:], file=sys.stderr)
        sys.exit("❌ 普查自己没跑成 —— 按 CRASH 记")
    rows = []
    cur = None
    for ln in r.stdout.splitlines():
        if ln.startswith("@@@TEST "):
            cur = {"name": ln[8:].strip(), "rc": None, "seq": []}
            rows.append(cur)
        elif ln.startswith("@@@RC ") and cur is not None:
            cur["rc"] = int(ln[6:].strip())
        elif "[KR126CENSUS]" in ln and cur is not None:
            cur["seq"].append("abort" if "abort" in ln else "ok")
    total_probe = sum(len(x["seq"]) for x in rows)
    if total_probe == 0:
        sys.exit("❌ 一行 `[KR126CENSUS]` 都没数到 —— 量具刀没落地，"
                 "这一张表是空真，不许读成「没人撞上」")
    print(f"· 名单来自测试二进制 `--list '{FAMILY}'` · 共 {len(rows)} 条")
    print(f"· 探针总行数 {total_probe}（非零 ⇒ 采集面真的接上了）")
    print(f"{'rc':>3} {'连接':>4} {'abort':>5} {'abort后还接':>10}  名字")
    hazard = []
    for x in rows:
        seq = x["seq"]
        ab = seq.count("abort")
        after = 0
        if "abort" in seq:
            after = len(seq) - (seq.index("abort") + 1)
        if ab:
            hazard.append((x["name"], ab, after))
        print(f"{x['rc']:>3} {len(seq):>4} {ab:>5} {after:>10}  "
              f"{x['name'][len(FAMILY):]}")
    print()
    print(f"撞上「下游先走」的：{len(hazard)} / {len(rows)}")
    for n, ab, after in hazard:
        print(f"  · {n[len(FAMILY):]} — abort {ab} 次，其后还要 {after} 条连接")
    return 0


def _split_fns(src: str) -> dict[str, str]:
    """把一份 `.rs` 按 `fn` 切成「名字 -> 那一整段逐字文本」。

    ⚠ 这不是 Rust 的 `ast`，是**括号配平**：它不认宏展开、也不认字符串里的花括号
    （本文件里没有那两形，切之前用「切出来的段数 ＋ 首尾行」自检过）。
    同名的 `fn` 会带上序号，免得两段互相覆盖、把「改了一处」读成「没改」。
    """
    import re as _re
    out: dict[str, str] = {}
    for m in _re.finditer(r"^[ \t]*(?:pub(?:\([^)]*\))? )?(?:async )?fn ([A-Za-z0-9_]+)",
                          src, _re.M):
        name = m.group(1)
        i = src.find("{", m.end())
        if i < 0:
            continue
        depth, j = 0, i
        while j < len(src):
            if src[j] == "{":
                depth += 1
            elif src[j] == "}":
                depth -= 1
                if depth == 0:
                    break
            j += 1
        key, n = name, 1
        while key in out:
            n += 1
            key = f"{name}#{n}"
        out[key] = src[m.start():j + 1]
    return out


def fnmd5(base: str):
    """逐函数 md5 对账：`<base>` 那一版 vs 现在工作树这一版。"""
    import hashlib
    import subprocess as sp
    rel = "remote-daemon-proto/src/relay/server.rs"
    pre_src = sp.run(["git", "-C", str(WT), "show", f"{base}:{rel}"],
                     capture_output=True, text=True, check=True).stdout
    post_src = (WT / rel).read_text(encoding="utf-8")
    pre, post = _split_fns(pre_src), _split_fns(post_src)
    h = lambda t: hashlib.md5(t.encode()).hexdigest()[:12]
    added = [k for k in post if k not in pre]
    removed = [k for k in pre if k not in post]
    changed = [k for k in post if k in pre and h(pre[k]) != h(post[k])]
    same = [k for k in post if k in pre and h(pre[k]) == h(post[k])]
    print(f"· 分母：`{rel}` 里切得出的 `fn` —— {base} 版 {len(pre)} 个 · 现在 {len(post)} 个")
    print(f"· 整份文件 md5：{base}={h(pre_src)} · 现在={h(post_src)}")
    print(f"· 没动的 {len(same)} 个 · 改了的 {len(changed)} 个 · 新增 {len(added)} 个 · 删掉 {len(removed)} 个")
    for k in changed:
        print(f"  [改] {k}  {h(pre[k])} -> {h(post[k])}")
    for k in added:
        print(f"  [新] {k}  {h(post[k])}")
    for k in removed:
        print(f"  [删] {k}  {h(pre[k])}")
    return 0


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("mode", choices=["build", "list", "run", "family", "census", "fnmd5"])
    ap.add_argument("--runs", type=int, default=1)
    ap.add_argument("--threads", type=int, default=1)
    ap.add_argument("--filter", action="append", default=[])
    ap.add_argument("--cpuset", default=None)
    ap.add_argument("--keep-red", default=None)
    ap.add_argument("--base", default="078a987",
                    help="逐函数 md5 对账的基准提交（默认 main 上那一笔）")
    a = ap.parse_args()

    if a.mode == "fnmd5":
        return fnmd5(a.base)
    preflight()
    binpath, recompiled = build()
    print(f"· 被测树：{WT}")
    print(f"· 测试二进制：{binpath}")
    print(f"· 这一趟重编了几个 crate：{recompiled}"
          f"{'（0 ⇒ 源码没动过才该是 0，刚切过刀就该是非 0）' if recompiled == 0 else ''}")
    if a.mode == "build":
        return 0
    if a.mode == "list":
        names = family_names(binpath)
        for n in names:
            print(n)
        print(f"整族条数（`--list '{FAMILY}'` 现算）= {len(names)}")
        return 0
    if a.mode == "census":
        return census(binpath)

    filters = a.filter or ([FAMILY] if a.mode == "family" else [])
    if not filters:
        sys.exit("❌ run 模式要给 --filter（给不出就用 family）")
    keep = pathlib.Path(a.keep_red).resolve() if a.keep_red else None
    red, total, body, result_line = run(
        binpath, a.runs, a.threads, filters, a.cpuset, keep
    )
    print(f"· 筛子：{filters} · --test-threads={a.threads} · cpuset={a.cpuset}")
    print(f"· 末趟分母（原样）：{result_line or '（一行都没收到 —— 按 CRASH 记）'}")
    print(f"RED={red} / {total}")
    if body:
        print("---- 头一趟红的全文（截断到 120 行）----")
        print("\n".join(body.splitlines()[:120]))
    return 1 if red else 0


if __name__ == "__main__":
    sys.exit(main())
