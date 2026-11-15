#!/usr/bin/env bash
# 生成物有没有落在提交外：先照现在的代码把生成物全部重打一遍，再看仓里每个 `generated/` 目录下与 HEAD 不同（改了 · 删了）
#   或还没跟踪的文件，逐份印出。重打这一步与 `npm run gen:types` 是同一段（`--write`）：
#   - TS 线上类型（ts-rs 的 `export_bindings`）：后端与壳两棵 workspace 里凡是依赖 `ts-rs` 的包，名单照 `cargo metadata` 现算，不手写包表；
#   - 手机那两份 Kotlin（线上类型 · 命令表）照重打后的 TS 声明再生成（`gen-kotlin-wire.mjs`）。
# 用法：bash tests/scripts/generated-drift.sh           重打再比：退 0 = 一份都没有；1 = 有（印出来的就是）；2 = 判不了（不在 git 仓里 · 生成器跑不动）。
#       bash tests/scripts/generated-drift.sh --write   只重打、不比（`npm run gen:types` 调这一条）。
#   只在工作树里对、没提交的，合并以后才红 ⇒ quick-check 末尾与门禁 `generated` 格都调它。
#   生成物目录一律叫 `generated`（判据 ts_export_guard 钉着登记的落点都叫这个名字），这里按名字找，不另列目录。
set -u
root=$(git rev-parse --show-toplevel 2>/dev/null) || { echo "generated-drift: 不在 git 仓里 —— 判不了"; exit 2; }

# 一棵 workspace 里依赖 ts-rs 的包，一行一个 `-p <名>`（照 cargo metadata 现算）。
ts_packages() {
  (cd "$root/$1" && cargo metadata --no-deps --format-version 1 --offline 2>/dev/null) \
    | node -e 'let s="";process.stdin.on("data",d=>s+=d).on("end",()=>{const m=JSON.parse(s);for(const p of m.packages){if(p.dependencies.some(d=>d.name==="ts-rs"))console.log("-p\n"+p.name)}})'
}

regen() {
  local ws pkgs
  for ws in src/frontend/shell src/backend; do
    # 这棵树里没有那一棵 workspace（判据的夹具仓）⇒ 那一棵没有可重打的
    [ -f "$root/$ws/Cargo.toml" ] || continue
    pkgs=$(ts_packages "$ws") || return 1
    [ -n "$pkgs" ] || { echo "generated-drift: $ws 里认不出依赖 ts-rs 的包 —— 判不了"; return 1; }
    # shellcheck disable=SC2086
    (cd "$root/$ws" && cargo test --lib -q $pkgs export_bindings >/dev/null 2>&1) \
      || { echo "generated-drift: $ws 的线上类型导不出来 —— 判不了"; return 1; }
  done
  local gen="$root/tests/scripts/gen-kotlin-wire.mjs"
  if [ -f "$gen" ]; then node "$gen" >/dev/null 2>&1 || { echo "generated-drift: 手机线上类型生成不了 —— 判不了"; return 1; }; fi
}

regen || exit 2
[ "${1:-}" = "--write" ] && exit 0
kt=src/mobile/core-wire/src/main/kotlin/com/ccmonitor/mobile/core/wire/gen
out=$(git -C "$root" -c core.quotepath=false status --porcelain --untracked-files=all -- ':(glob)**/generated/**' "$kt/Generated.kt" "$kt/Commands.kt") \
  || { echo "generated-drift: git status 失败 —— 判不了"; exit 2; }
[ -z "$out" ] && exit 0
printf '%s\n' "$out"
exit 1
