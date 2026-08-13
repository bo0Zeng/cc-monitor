# `scripts/` 目录

| 脚本 | 作用 |
|---|---|
| [`run.ps1`](run.ps1) | 自动注入 MSVC dev shell 环境后跑 tauri 命令 |
| [`gate.sh`](gate.sh) | 出货前的**唯一闸门**：cargo（monitor + daemon）· npm · `pb check` 跑一遍，末尾只吐一行 `GATE: OK` / `GATE: FAIL …`。★ 它解决的是**过程**问题——门禁散成三条命令时，很容易写成「跑门禁 && git commit」一条龙，而长输出里那行 `1 failed` 会滚过去（08-13 实测发生过一次，红着出了货）。⇒ **先跑它、看见 OK，再单独敲 commit**；它**故意不提供 `--commit` 开关** |
| [`verify-committed-state.sh`](verify-committed-state.sh) | 从**提交状态**（不是工作树）编一次。★ **本仓不 push ⇒ CI 见不到这些 commit，这道门只能在本机跑**；理由与那次「约二十轮编不过」的事故见它自己的头注 |
| [`assert-coverage-floors.mjs`](assert-coverage-floors.mjs) | 逐文件覆盖率地板 + 0% 文件递减棘轮（聚合阈值看不见单模块归零）。跑法与登记见它自己的头注 |

> ⚠ **这张表由判据钉住**：`doc_claim_registry.rs::every_script_in_the_directory_is_listed_in_its_readme`
> —— 往 `scripts/` 放新文件而不登记就会红。
> 它是补出来的：08-06 之前这张表**只有 `run.ps1`**，于是照 README 找不到
> 「唯一量提交状态、且必须本机跑」的那道门。

另有一份 PowerShell 模板存在 `src-tauri/scripts/`（编译时 `include_str!` 进 Rust 二进制，不在本目录）：

| 模板 | 作用 |
|---|---|
| `../src-tauri/scripts/cc.ps1.tpl` | cc 集成 PowerShell 块模板。设置面板装 cc 集成时把这段（含 `__ccm_bind` helper + 可选 `function cc`）写入用户 profile 的 `# === cc-monitor BEGIN === ... # === cc-monitor END ===` 块内 |

---

## run.ps1

### 用途

Tauri 在 Windows 上要靠 MSVC link.exe / cl.exe 编译 Rust 后端。如果当前 PowerShell 没注入 vcvars，会出现各种诡异错误（最常见：链接到 Git Bash 的 GNU coreutils 假冒 link，崩在链接阶段）。

`run.ps1` 通过 `vswhere.exe` 自动定位 MSVC 安装位置，调 `Launch-VsDevShell.ps1` 注入 PATH/LIB/INCLUDE，然后跑 tauri 命令。

### 用法

```powershell
powershell -NoProfile -File scripts\run.ps1 [dev|build|check|clean]
```

| 子命令 | 等价于 |
|---|---|
| `dev` | `npx tauri dev`（弹 1100x800 窗口，HMR） |
| `build` | `npx tauri build`（产 msi + nsis + exe） |
| `check` | `cargo check`（不编 release） |
| `clean` | `cargo clean`（清 `src-tauri/target/`） |

### 前置依赖

- **Visual Studio Build Tools 2022** + **VCTools workload**（提供 link.exe / cl.exe / Windows SDK）
- `vswhere.exe` 默认在 `C:\Program Files (x86)\Microsoft Visual Studio\Installer\vswhere.exe`（VS 安装时自动放）

脚本会在缺失依赖时抛错并提示安装。

### 工作原理

```powershell
1. vswhere.exe -latest -requires Microsoft.VisualCpp.Tools.HostX64.TargetX64
   → 找到 MSVC 安装路径，比如 D:\Microsoft Visual Studio\2022\BuildTools

2. & "<install>\Common7\Tools\Launch-VsDevShell.ps1" -SkipAutomaticLocation -Arch amd64
   → 注入 PATH / LIB / INCLUDE / INCLUDE 等环境变量到当前 PS session

3. Set-Location <repo root>
4. npx tauri $Cmd
```

详 [`../doc/DEVELOPMENT.md`](../doc/DEVELOPMENT.md)。
