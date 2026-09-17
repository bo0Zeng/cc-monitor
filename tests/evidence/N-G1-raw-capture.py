#!/usr/bin/env python3
"""N-G1 的取样器：在沙箱里现打**原始**失败输出，喂给 `N-G1-diag-match.py`。

为什么要单独一份：`NG1D1` 的读数全部建立在「这几份是**原始** stdout+stderr」上。
PM 09-05 栽的那一跤逐字是「手上那几份日志**已经被门禁加过 `  | ` 前缀**」——
那不是原始输出。⇒ 取样这一步本身必须可复跑，否则下一轮谁也复不出那张表。

⚠ **它会临时改工作树里的文件**（往一个 vitest / tsx / Rust 文件尾巴上追加一段必红的判据），
  跑完 `git checkout --` 还原；每一形跑完立刻还原，`--keep` 才留着。
  🔴 宿主上一条 cargo test / vitest / e2e 都不跑 —— 全在 `ccmon-devbox:latest` 里。

用法（在工作树里跑）：
  python3 evidence/N-G1-raw-capture.py --out /tmp/ng1raw            # 全部七形
  python3 evidence/N-G1-raw-capture.py --out /tmp/ng1raw --only npm-vitest
  python3 evidence/N-G1-raw-capture.py --out /tmp/ng1raw --print-cmd  # 只把 docker 命令印出来

跑完接着量：
  python3 evidence/N-G1-diag-match.py --gate scripts/gate.sh /tmp/ng1raw/raw-*.txt
"""
from __future__ import annotations

import argparse
import pathlib
import shlex
import subprocess
import sys

PROJ = pathlib.Path("/home/zbl/文档/claudecode-frontend")
SKILL = pathlib.Path("/home/zbl/.claude-accts/z/skills/planned-build")
IMAGE = "ccmon-devbox:latest"

# 每一形：(名字, 往哪个文件追加什么, 跑什么命令)
#   追加的那一段一律是**新加的一个必红判据**，不改任何既有断言 ⇒ 还原只需 `git checkout --`。
VITEST_PROBE = '''
describe("NG1 探针（临时，跑完就 git checkout 掉）", () => {
  it("NG1-PROBE-VITEST 故意红的一条前端判据", () => {
    expect(accountColorSlot("ng1-probe")).toBe(-12345);
  });
});
'''
RUST_PROBE = '''
#[cfg(test)]
mod ng1_probe_tests {
    #[test]
    fn ng1_probe_deliberately_red() {
        assert_eq!(1, 2, "NG1-PROBE-CARGO");
    }
}
'''

SHAPES = {
    # 名字            改哪个文件（None = 一个字都不改）          追加什么        跑什么
    "npm-vitest": ("src/account-color.vitest.ts", VITEST_PROBE, "npm test"),
    "npm-tsx": ("src/format.test.ts", "__TSX_SED__", "npm test"),
    "cargo-workspace": ("src-tauri/src/lib.rs", RUST_PROBE,
                        "cd src-tauri && cargo test --workspace --exclude code-picture-core --lib"),
    "cargo-daemon": ("remote-daemon-proto/src/main.rs", RUST_PROBE,
                     "cd remote-daemon-proto && cargo test"),
    "cargo-compile-error": ("src-tauri/src/lib.rs", "\nfn ng1_probe_syntax_error( {\n",
                            "cd src-tauri && cargo test --workspace --exclude code-picture-core --lib"),
    "e2e-floor": (None, None, "bash e2e/assert-pass-floor.sh ccm-print-parity 999 exact"),
    "pbcheck": (None, None,
                "cp -a /home/zbl/文档/claudecode-frontend/.claude/planned-build /tmp/pb-copy && "
                "printf '\\n\\n### NG1PROBE 探针\\n> id: NG1PROBE | kind: dod | src: 自@09-05 |"
                " acceptor: 机检 | links: 不存在的节点XYZ\\n'"
                " >> /tmp/pb-copy/first-run/features/N-G1-门禁诊断吐得出失败测试的名字.md && "
                "python3 \"$HOME/.claude-accts/z/skills/planned-build/bin/pb.py\" check /tmp/pb-copy/first-run"),
}

# tsx 那一形是**改一个既有期望值**（追加没用：那份脚本是顺序执行的），单列。
TSX_SED = (
    "python3 - <<'PY'\n"
    "p='src/format.test.ts'\n"
    "s=open(p,encoding='utf-8').read()\n"
    "s=s.replace('eq(formatTimestampShort(\"not-a-date\"), \"not-a-date\");',"
    "'eq(formatTimestampShort(\"not-a-date\"), \"NG1-PROBE-TSX-EXPECTED\");',1)\n"
    "open(p,'w',encoding='utf-8').write(s)\n"
    "PY\n"
)


def build_script(shapes: list[str], keep: bool) -> str:
    lines = ["set -u", 'restore() { git checkout -- "$@" 2>/dev/null || true; }', ""]
    for name in shapes:
        path, patch, cmd = SHAPES[name]
        lines.append(f"# ---- {name} ----")
        if path is not None:
            if patch == "__TSX_SED__":
                lines.append(TSX_SED)
            else:
                lines.append(f"cat >> {shlex.quote(path)} <<'NG1EOF'{patch}NG1EOF")
        # 与 gate.sh 逐字同形：`out="$(cmd 2>&1)"` ⇒ 拿到的就是门禁看见的那一份
        lines.append(f'out="$(bash -c {shlex.quote(cmd)} 2>&1)"; rc=$?')
        lines.append(f"printf '%s\\n' \"$out\" > /ng1out/raw-{name}.txt")
        lines.append(f"echo '{name}: rc='\"$rc\"' 行='\"$(wc -l < /ng1out/raw-{name}.txt)\"")
        if path is not None and not keep:
            lines.append(f"restore {shlex.quote(path)}")
        lines.append("")
    lines.append('echo "== 收工时工作树 =="; git status --porcelain')
    return "\n".join(lines)


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--out", required=True, help="原始输出落在哪（宿主路径，会挂进容器 /ng1out）")
    ap.add_argument("--only", action="append", choices=sorted(SHAPES), help="只跑这几形")
    ap.add_argument("--keep", action="store_true", help="跑完不还原（默认还原）")
    ap.add_argument("--print-cmd", action="store_true", help="只印命令，不跑")
    args = ap.parse_args()

    wt = pathlib.Path.cwd()
    if not (wt / "scripts" / "gate.sh").exists():
        raise SystemExit(f"要在工作树根上跑（当前 {wt} 里没有 scripts/gate.sh）")
    out = pathlib.Path(args.out).resolve()
    out.mkdir(parents=True, exist_ok=True)
    script = build_script(args.only or list(SHAPES), args.keep)
    (out / "probe.sh").write_text(script, encoding="utf-8")

    cmd = [
        "docker", "run", "--rm", "--network", "none",
        "-v", f"{PROJ}:{PROJ}", "-v", f"{SKILL}:{SKILL}:ro",
        "-v", "ccmon-cargo-registry:/opt/rust/cargo/registry",
        "-v", f"{out}:/ng1out",
        "-e", f"CARGO_TARGET_DIR={PROJ}/.claude/pm-targets/{wt.name}",
        "-e", "HOME=/home/zbl", "-w", str(wt), IMAGE,
        "bash", "-c", 'mkdir -p "$HOME/.claude/projects" && bash /ng1out/probe.sh',
    ]
    print("$ " + " ".join(shlex.quote(c) for c in cmd))
    if args.print_cmd:
        return 0
    return subprocess.run(cmd).returncode


if __name__ == "__main__":
    sys.exit(main())
