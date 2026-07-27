# F5 — cc-acct-iso 一键部署 + 存在性检测

## DoD（可验证）
- [ ] 全新远端（无 cc-acct-iso）点一次「部署」→ 远端 `cc-acct-iso` 命令可用（`command -v` 命中）。
- [ ] 已部署且 vendor_id 相同 → 再点报「已是最新，无需重装」（skip-if-current）。
- [ ] 面板未启用态先探测：无 cc-acct-iso → 显「一键部署」；有但无 manifest → 走现有 init 向导；有且有 manifest → 正常。
- [ ] 部署**不碰 rc**（install.sh 只软链 ~/.local/bin + 打印手动步骤）。
- [ ] 部署路径过安全守卫（绝对/无 `..`/非根/含 cc-acct-iso 或 .cc-monitor）。
- [ ] tsc 0 / vitest 全绿（新增 IPC 桩 + 面板分支用例）/ `cargo test`（守卫纯函数 + 部署决策）/ prod build。
- **不做**：不动 cc-acct-iso 迁移逻辑；不代跑 init/add（仍走终端）；不改 rc。

## 与主计划对接（共享面）
- 新 IPC `deploy_remote_acct_iso` / `check_remote_acct_iso`（`src-tauri`）——账本「新 IPC」项最终形态。
- `accounts-section.ts` 未启用态分支——账本面板项，本功能只加「检测→部署」前置，不动 init 向导本体（F3 再重构面板主体）。

## 接口/契约
```
// Rust
#[tauri::command] deploy_remote_acct_iso(cfg: RemoteConfig, dest_dir: String) -> Result<String,String>
#[tauri::command] check_remote_acct_iso(cfg: RemoteConfig) -> Result<AcctIsoStatus,String>
  // AcctIsoStatus { installed: bool, version: Option<String>, path: Option<String> }
// 默认 dest_dir = ~/.cc-monitor/cc-acct-iso（与 daemon ~/.cc-monitor/bin 同根；SFTP 不展开 ~ → 前端传绝对路径）
```
- vendor 指纹：`src-tauri/vendor/cc-acct-iso/.vendor_id`（内容哈希，已生成 20be7f2a）→ 远端 marker `<dest>/.vendor_id`，deploy_decision 复用 daemon 的比对语义。

## 逐条实现步骤
1. **vendor 脚本进仓** ✓ 已做：`src-tauri/vendor/cc-acct-iso/`（scripts/{cc-acct-iso,lib.sh,cc-acct-iso-install.sh,test/run-tests.sh}, SKILL.md, examples/config, .vendor_id）。
2. **build.rs freshness 检查**：仿 `check_vendor_freshness`（code-picture-core）加 cc-acct-iso vendor 过期软警告（上游 `~/.claude/skills/cc-acct-iso` 存在则比指纹）。`rerun-if-changed` 挂 vendor 目录。
3. **新模块 `src-tauri/src/acct_iso_deploy.rs`**：
   - `include_bytes!` 四个脚本 + `include_str!(".vendor_id")`（runtime trim）。
   - `is_safe_remote_acct_iso_dir(path)`（纯函数，可单测）：绝对/无 `..`/非根/含 `cc-acct-iso` 或 `.cc-monitor`。
   - `check_remote_acct_iso`：exec `command -v cc-acct-iso && cc-acct-iso config 2>/dev/null`（或 `--version`）→ AcctIsoStatus。
   - `deploy_remote_acct_iso`：connect_sftp（复用 sftp.rs）→ 读 `<dest>/.vendor_id` marker → deploy_decision → mkdir -p dest/scripts/test + dest/examples → upload_atomic 各文件（cc-acct-iso/lib.sh/install.sh/run-tests.sh 0o755，SKILL.md/config 0o644）→ 跑 install.sh（exec `bash <dest>/scripts/cc-acct-iso-install.sh` 建软链，CC_ACCT_ISO_BIN_DIR 默认 ~/.local/bin）→ 写 marker。返回人读结果。
4. **注册命令**：`lib.rs` invoke_handler 加两个（旁 `deploy_remote_daemon`）。
5. **前端桩 + 面板分支**：`src/settings/accounts-section.ts` renderNotEnabled 前加 `check_remote_acct_iso`；未装 → 「一键部署 cc-acct-iso」按钮 invoke deploy → 成功后 reload。vitest 覆盖三分支（未装/装了无 manifest/正常）。
6. **门禁**：tsc/vitest/cargo test/build，回盘核实（pipefail）。

## 测试策略
- Rust：`is_safe_remote_acct_iso_dir` 边界（相对/`..`/根/不含关键词 → false）；`deploy_decision` 复用现成测试语义。
- 前端：check→分支渲染纯逻辑单测（mock invoke）。
- 真机冒烟（Phase C 末，可选）：对一台远端跑一次部署 + `command -v`。

## 实现状态（Phase C 完）
- ✓ vendor 脚本进仓 + VENDOR.md + .vendor_id
- ✓ build.rs `check_acct_iso_vendor_freshness`（上游存在则比对，软警告）
- ✓ `acct_iso_deploy.rs`：include_bytes 内嵌 + `deploy_remote_acct_iso`/`check_remote_acct_iso` IPC + `is_safe_remote_acct_iso_dir` 守卫 + exec_collect + 3 单测
- ✓ sftp.rs read_optional/ensure_dir_all → pub(crate)（复用）
- ✓ lib.rs 注册 mod + 两命令
- ✓ acct-deploy.ts `deriveAcctIsoDir` 纯函数 + 4 单测
- ✓ accounts-section.ts renderNotEnabledFlow（探测）+ renderNeedsDeploy（一键部署）+ 2 分支单测
- **门禁**：tsc 0 / vitest 600/0 / cargo check 0 / cargo test 368/0。prod build 留 Phase G。

## 审计结果（Phase D 完）
2 并行 agent（正确性+安全 / 计划+架构），**无阻塞**。两 agent 独立同报 I1（强信号）→ 已修：
- **I1（必修，已修）**：install 失败被 `|| true` + `unwrap_or_default()` 吞、marker 无条件写 → 假成功 + Skip 死锁。修：install 追加 `__CCM_ACCT_ISO_INSTALL_OK__` sentinel，无 sentinel = 未成功 → 不写 marker + 返回 Err（可重试）+ tracing::warn 输出；path_hint 归因修正。
- **S1 无超时（已修）**：`exec_collect` 包 45s `tokio::time::timeout`，防 install 挂住令按钮永久「部署中…」。
- **S2 check 白握手 + 偏离契约（已修）**：`check_remote_acct_iso` 去掉 dest_dir 参数与 SFTP marker 读，纯一次 `command -v` exec；AcctIsoStatus 去掉 version 字段。签名回归计划契约 `check(cfg)`。
- **S5 dest=null 留 command-not-found 残角（已修）**：check 不再依赖 dest → 任何配置都探测；dest 推不出时 renderNeedsDeploy 退文字指引（引导填 user/daemonPath），不留死角。
- **S1/S5 指纹只覆盖 3/6 文件（已修）**：`.vendor_id` 重算覆盖全部 6 部署文件（固定顺序）；build.rs 加自洽校验（vendored sha vs .vendor_id）+ 过期检查扩到 6 文件；VENDOR.md 菜谱同步。
- **S4 尾斜杠不一致（随 S2 消解）**：check 不再用 dest。
- 不修（登记）：S2 守卫子串非锚定（非可利用，dest 本地推导 + sq 转义，同 daemon 守卫取舍）；S3 参数 symlink 父目录（同信任边界，同 daemon 残留）；S3 IPC 集成测试盲区（需活 SSH，守卫纯函数已测）；S6 vendored install.sh 里 ✔（上游原文，红线约束本仓自写，豁免）。
- **复验门禁**：tsc 0 / vitest 600/0 / cargo check 0（无过期/不自洽告警）/ cargo test 368/0。

## 工程审计结果（Phase E）
主线程对账 MASTERPLAN §3 账本：新 IPC（deploy/check）落到账本最终形态；F5 只作用于**未启用态**，与 F3 要重构的 **ready 态卡片**正交（架构 agent 确认「未装→一键部署」正是账本为 accounts-section 定的既定分支、非补丁，F3 重构不打架）。未引入拖累 F1/F3/F4/F7 的耦合债。红线全守（不碰 rc / 无新增轮询 / 无 emoji / daemon·remote-launch 零改）。

## 签收
- [x] 过代码审计(D)（无阻塞；I1+S1/S2/S4/S5 已修并复验）
- [x] 过工程审计(E)（与 F3 正交，账本最终形态，无耦合债）
- [x] 主计划已更新(F)
