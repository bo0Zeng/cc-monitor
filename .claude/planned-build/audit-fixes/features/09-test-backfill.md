# F09 — 测试补齐 / test backfill

> 账本 I8/G3。低风险、纯补齐（新增测试 + CI 步骤 + 一处去重到已测 helper）。**不重构脊柱**（留 F12/F13）。

## 背景（摸底）
- **code-picture-core**（`src-tauri/vendor/code-picture-core/`）：path 依赖、**非** workspace 成员 → src-tauri `cargo test --all` = 只测 `monitor` 包，**其 25 个测试从不在 CI 跑**。`cargo test -p code-picture-core` 可跑（本地 25 passed）。红线：**别误伤 vendor crate**，只加 CI 步骤、不动其源码/Cargo.toml。
- **e2e**（`e2e/f40-suite.sh` + `gen-fork-session.py`）：需 Xvfb + `tauri dev` 全量 app + xdotool 的重集成套件 → **真 e2e 进 CI = 大 GUI-runner 投入**（且 app 生产仅 Windows，CI frontend 跑 windows-latest）→ 本轮不做真 e2e，改**脚本健康冒烟**（shellcheck + py_compile，防脚本腐烂）。aya 有 shellcheck 0.11 + python3。
- **main.ts**（1067 行）：绝大多数是 async/DOM 引导胶水、非纯函数、难单测。唯一清晰盲区=`:989` 内联 basename（`cwd.replace(...).split(...).pop()`），与 `sftp/paths.ts` 的**已导出+已测** `basename`（paths.vitest:63-68 覆盖含尾斜杠 cwd 用例）行为等价。sftp/paths.ts 是纯 leaf（无 import），可安全引入。

## DoD（分步）
- [x] **步骤 1（code-picture-core 进 CI）**：ci.yml rust job 加 `cargo test -p code-picture-core`（vendor 25 测）。未动 vendor 源码/Cargo.toml。本地 25 passed。
- [x] **步骤 2（e2e 脚本健康冒烟进 CI）**：新增 `e2e-smoke` ubuntu job：`shellcheck --severity=error e2e/*.sh`（只挡 error 级、忽略 f40 有意的 ls/eval 等 info/style 噪音——真机验：severity=error 本地 exit 0）+ `python3 -m py_compile e2e/*.py`。真 e2e 记档待 v2/真机。本地两命令 exit 0。
- [x] **步骤 3（main.ts basename 盲区去重到已测 helper）**：`main.ts:989` 内联 → `import { basename } from "./sftp/paths"`。行为等价（`if(base)` 守卫下 ""/undefined 同效；paths.vitest:63-68 已覆盖含尾斜杠 cwd）。未动 panorama basename（留 F12）。tsc 0 / npm 595 不变。
- [x] **验证**：code-picture-core 25 / src-tauri 365 / daemon 全绿；tsc 0 / npm test 595 / build 0；shellcheck --severity=error 0 / py_compile 0；ci.yml YAML 合法。

## 不做什么（防蔓延）
- **不动 vendor code-picture-core crate 源码/Cargo.toml**（红线）——只加 CI 跑其测。
- **不做真 e2e 进 CI**（Xvfb + tauri dev + xdotool = 大 runner 投入；app 生产仅 Windows）——只脚本健康冒烟；真 e2e 记档待 v2/真机。
- **不重构 main.ts 胶水、不动 panorama basename、不把 basename 迁到新通用 util**（3 文件 churn=F12/F13 territory）。
- 不 push/发版/bump。

## 与主计划对接（共享面）
- `.github/workflows/ci.yml`（账本 CI 终态行）：F09 补「vendor-crate 测 + e2e-smoke」两项（F08 已落 lint/coverage + 双写点/只读护栏）。朝账本终态实现。
- main.ts:989 去重到 `sftp/paths.basename`：消一处重复（panorama 的第三份留 F12 统一）。

## 审计结果
- **代码审计(D)（低风险主线程自审——纯补测 + CI 步骤 + 一处去重到已测 helper）**：
  - *正确性*：code-picture-core CI 步骤只跑既有 25 测、不动 vendor；e2e smoke `--severity=error` 是真破坏守护（忽略 style）；main.ts→sftp/paths.basename 行为等价已核（sftp/paths 纯 leaf 无环、`if(base)` 守卫下 ""/undefined 同效）。
  - *计划符合度*：三步全做；「不做什么」守住（未动 vendor 源码、未跑真 e2e、未动 panorama basename、未重构 main.ts 胶水）。
  - *架构/红线*：daemon 零改（code-picture-core 是 src-tauri vendor 非 daemon；e2e smoke 不碰 daemon）；无 TMUX_LS_FMT/bashrc/cc-sid8/发版/轮询/孤儿；无 emoji。
- **工程审计(E)**：F09 落成账本 CI 终态的 vendor-crate 测 + e2e-smoke 两项。main.ts basename 去重后仅剩 panorama 一份私有 basename（留 F12 统一，已记账本）。覆盖率地板未变（F09 补的是 Rust 测 + 路由到已覆盖代码，vitest 覆盖不涨）→ 收紧 F08b 地板仍待更多 TS 侧补测。主计划自洽。

## 签收
- [x] **F09 过 D+E+F**（低风险主线程自审）：code-picture-core 25 测进 CI（不动 vendor）+ e2e 脚本健康冒烟（shellcheck --severity=error + py_compile，真 e2e 待 v2/真机）+ main.ts basename 盲区去重到已测 sftp/paths.basename。tsc 0 / npm 595 / code-picture-core 25 / shellcheck+py 0。
