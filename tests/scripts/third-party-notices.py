#!/usr/bin/env python3
"""第三方许可声明的生成器：产物里随分发附上依赖的版权与许可全文。

产出两份（都进仓，都由本脚本生成，不手改）：
  · `THIRD-PARTY-NOTICES.txt` —— 随安装包（`tauri.sidecar.conf.json` 的 `bundle.resources`）与 Release 资产分发；
  · `tests/evidence/third-party-not-shipped.txt` —— 锁文件里有、但不进产物的包（只在别的平台 / 只开发期用）。
两份合起来 == 两份 `Cargo.lock` 的第三方包 ∪ `src/vendor/` 下的包；`package-lock.json` 的生产依赖全在声明里。
门禁 `release-gate`（`tests/evidence/K-R124-ruler.py` ⑰）两向核这两句；发版那一趟（`release.yml`）跑 `--check`。

来源：
  · Rust（后端 `src/backend` ＋ 壳与文件窗口 `src/frontend/shell` 整个工作区，含 `src/vendor/` 下放进仓的那几份）：
    cargo-about（版本钉在 `CARGO_ABOUT_VERSION`），离线读 cargo 注册表缓存里各包自带的许可文件；目标平台与收纳的许可住
    `tests/scripts/third-party-about.toml`；开发期依赖不收。
  · npm：`package-lock.json` 里不带 `dev` 的包，读 `node_modules/` 里各包自带的许可文件（要先 `npm ci`）。

用法（仓根下）：
    python3 tests/scripts/third-party-notices.py           # 重新生成两份
    python3 tests/scripts/third-party-notices.py --check   # 生成到临时处、与仓里那两份逐字比，不同 ⇒ 退出码 1
cargo-about 找法：PATH 上版本对的那个 ⇒ `.build/tools/bin/cargo-about` ⇒ 都没有就 `cargo install` 进 `.build/tools`（不碰家目录）。
"""

import json
import os
import re
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
CARGO_ABOUT_VERSION = "0.9.2"
CONFIG = ROOT / "tests/scripts/third-party-about.toml"
NOTICES = ROOT / "THIRD-PARTY-NOTICES.txt"
NOT_SHIPPED = ROOT / "tests/evidence/third-party-not-shipped.txt"
VENDOR = ROOT / "src/vendor"
#: (清单, 是不是整个工作区)
TREES = [("src/backend/Cargo.toml", False), ("src/frontend/shell/Cargo.toml", True)]
LOCKS = ["src/backend/Cargo.lock", "src/frontend/shell/Cargo.lock"]
LICENSE_FILE = re.compile(r"^(licen[cs]e|copying|notice)", re.I)

MIT_TEXT = """MIT License

Copyright (c) {holder}

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
"""


def die(msg):
    print("third-party-notices: " + msg, file=sys.stderr)
    sys.exit(2)


def cargo_about():
    def version_of(exe):
        try:
            out = subprocess.run([exe, "--version"], capture_output=True, text=True, check=True).stdout
        except (OSError, subprocess.CalledProcessError):
            return None
        return out.split()[-1] if out.strip() else None

    local = ROOT / ".build/tools/bin/cargo-about"
    for exe in (shutil.which("cargo-about"), str(local)):
        if exe and version_of(exe) == CARGO_ABOUT_VERSION:
            return exe
    print("third-party-notices: 装 cargo-about %s 进 .build/tools（只此一次）…" % CARGO_ABOUT_VERSION, file=sys.stderr)
    subprocess.run(["cargo", "install", "--locked", "--features", "cli", "--root", str(ROOT / ".build/tools"),
                    "cargo-about@" + CARGO_ABOUT_VERSION], check=True)
    if version_of(str(local)) != CARGO_ABOUT_VERSION:
        die("装完了还是找不到 cargo-about %s" % CARGO_ABOUT_VERSION)
    return str(local)


def is_ours(pkg):
    """本仓自己的包：路径来源、且不在 `src/vendor/` 下。"""
    if pkg.get("source") is not None:
        return False
    m = re.match(r"path\+file://(.+?)(#.*)?$", pkg.get("id", ""))
    path = Path(m.group(1)) if m else None
    return not (path and VENDOR.resolve() in path.resolve().parents)


def rust_groups():
    exe = cargo_about()
    groups = {}
    for manifest, workspace in TREES:
        subprocess.run(["cargo", "fetch", "--locked", "--manifest-path", str(ROOT / manifest)], check=True,
                       stdout=subprocess.DEVNULL)
        with tempfile.TemporaryDirectory() as td:
            out = Path(td) / "about.json"
            cmd = [exe, "generate", "--frozen", "--format", "json", "-c", str(CONFIG),
                   "-m", str(ROOT / manifest), "-o", str(out)]
            if workspace:
                cmd.append("--workspace")
            r = subprocess.run(cmd, capture_output=True, text=True)
            if r.returncode != 0:
                die("cargo-about 在 %s 上失败：\n%s" % (manifest, r.stderr[-4000:]))
            doc = json.loads(out.read_text(encoding="utf-8"))
        unknown = sorted("%s %s" % (c["package"]["name"], c["package"]["version"])
                         for c in doc["crates"] if c["license"] == "Unknown" and not is_ours(c["package"]))
        if unknown:
            die("这几个第三方包认不出许可（%s）：%s —— 在 %s 里给它们写 clarify" % (manifest, unknown, CONFIG.name))
        for lic in doc["licenses"]:
            users = {"%s %s" % (u["crate"]["name"], u["crate"]["version"])
                     for u in lic["used_by"] if not is_ours(u["crate"])}
            if users:
                groups.setdefault((lic["id"], lic["name"], lic["text"].strip("\n")), set()).update(users)
    return groups


def npm_entries():
    lock = json.loads((ROOT / "package-lock.json").read_text(encoding="utf-8"))
    out = []
    for key, meta in sorted(lock["packages"].items()):
        if not key or meta.get("dev"):
            continue
        name = key.rsplit("node_modules/", 1)[-1]
        pdir = ROOT / key
        if not (pdir / "package.json").is_file():
            die("%s 不在盘上 —— 先 npm ci" % key)
        pj = json.loads((pdir / "package.json").read_text(encoding="utf-8"))
        spdx = pj.get("license") or meta.get("license") or "?"
        files = sorted(p for p in pdir.iterdir() if p.is_file() and LICENSE_FILE.match(p.name))
        texts = [p.read_text(encoding="utf-8", errors="replace").strip("\n") for p in files
                 if not p.name.lower().endswith(".spdx")]
        if not texts:
            holder = None
            for p in files:
                m = re.search(r"^PackageCopyrightText:\s*(.+)$", p.read_text(encoding="utf-8"), re.M)
                holder = holder or (m and m.group(1).strip())
            authors = pj.get("authors") or ([pj["author"]] if pj.get("author") else [])
            holder = holder or ", ".join(a if isinstance(a, str) else a.get("name", "") for a in authors)
            if "MIT" not in re.split(r"[\s()]+", spdx) or not holder:
                die("%s 没带许可全文（许可 %s）—— 补一条做法再生成" % (name, spdx))
            texts = [MIT_TEXT.format(holder=holder).strip("\n")]
        out.append((name, meta["version"], spdx, texts))
    return out


def lock_crates():
    got = set()
    for lock in LOCKS:
        text = (ROOT / lock).read_text(encoding="utf-8")
        for name, ver, src in re.findall(
                r'\[\[package\]\]\nname = "([^"]+)"\nversion = "([^"]+)"\n(?:source = "([^"]+)")?', text):
            if src:
                got.add("%s %s" % (name, ver))
    for manifest in sorted(VENDOR.glob("*/Cargo.toml")):
        text = manifest.read_text(encoding="utf-8")
        name = re.search(r'^name\s*=\s*"([^"]+)"', text, re.M).group(1)
        ver = re.search(r'^version\s*=\s*"([^"]+)"', text, re.M).group(1)
        got.add("%s %s" % (name, ver))
    return got


def render():
    groups = rust_groups()
    npm = npm_entries()
    lines = [
        "cc-monitor — 第三方许可声明 / Third-party notices",
        "",
        "本程序包含下列第三方组件。各组件的版权归其作者所有，按各自的许可分发；下面逐组附上许可全文。",
        "This program includes the third-party components listed below, each distributed under its own",
        "license. The full license texts follow.",
        "",
        "（本文件由 tests/scripts/third-party-notices.py 生成，不手改。）",
        "",
        "",
        "================================ Rust crates ================================",
    ]
    declared = set()
    for (lid, lname, text), users in sorted(groups.items(), key=lambda kv: (kv[0][0], sorted(kv[1])[0])):
        lines += ["", "-------- %s (%s) --------" % (lname, lid), "Used by:"]
        lines += ["    crate: %s" % u for u in sorted(users)]
        lines += ["", text, ""]
        declared |= users
    lines += ["", "================================ npm packages ================================"]
    for name, ver, spdx, texts in npm:
        lines += ["", "-------- %s %s (%s) --------" % (name, ver, spdx), "    npm: %s %s" % (name, ver)]
        for t in texts:
            lines += ["", t, ""]
    notices = "\n".join(lines).replace("\r\n", "\n").replace("\r", "\n").rstrip("\n") + "\n"

    lock = lock_crates()
    stray = sorted(declared - lock)
    if stray:
        die("声明里有、锁文件里没有：%s" % stray)
    rest = sorted(lock - declared)
    not_shipped = "\n".join([
        "# 锁文件里有、但不进产物的第三方包（只在别的平台 / 只开发期 / 没开的特性用）。",
        "# 由 tests/scripts/third-party-notices.py 生成（== 两份 Cargo.lock 的第三方包 ∪ src/vendor − THIRD-PARTY-NOTICES.txt 里的），不手改。",
    ] + ["crate: %s" % c for c in rest]) + "\n"
    return notices, not_shipped


def main():
    check = "--check" in sys.argv[1:]
    notices, not_shipped = render()
    if not check:
        NOTICES.write_text(notices, encoding="utf-8", newline="\n")
        NOT_SHIPPED.write_text(not_shipped, encoding="utf-8", newline="\n")
        print("third-party-notices: 写好 %s（%d 字节）与 %s" % (NOTICES.name, len(notices.encode()), NOT_SHIPPED.name))
        return 0
    bad = [p.relative_to(ROOT).as_posix() for p, want in ((NOTICES, notices), (NOT_SHIPPED, not_shipped))
           if not p.is_file() or p.read_bytes() != want.encode("utf-8")]
    if bad:
        print("::error::第三方许可声明与现打的依赖对不上：%s —— 跑 python3 tests/scripts/third-party-notices.py 重新生成再提交"
              % ", ".join(bad))
        return 1
    print("third-party-notices: 仓里那两份与现打的依赖逐字一致")
    return 0


if __name__ == "__main__":
    os.chdir(ROOT)
    sys.exit(main())
