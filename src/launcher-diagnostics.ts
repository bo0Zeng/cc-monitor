/**
 * F08（unify-launch）：越层启动器诊断 ＋ 用户级 PATH 那一格。
 *
 * MASTERPLAN 设计原则#7：越层启动器只诊断 + 引导迁移，绝不自动降级、**绝不在用户没要求时
 * 改他的配置**（「用户按了一个写明写到哪、写什么的按钮」不在禁令内，`K-R49` 那一次收窄的原话仍成立）。
 *
 * 〔AL1 · 2026-09-24〕**别名那两块搬走了**：「按账号生成命令」与「生成自定义别名」合成一类
 * （`设计/71`：别名 ＝ 名字 ＋ 一组 ccm 参数），住机器页「本机 → 工具 → 别名」，
 * 代码在 `src/settings/machine-aliases.ts`；shell 文本改由后端渲染（`account_aliases::render`），
 * 本文件从前那个 TS 生成器与「该调哪一份 ccm」的三档判词一起退役
 * —— 别名里只写裸 `ccm`（`01 §6.7b`），「你 PATH 上那个 `ccm` 是旧的」那句话照样在别名那一块上屏。
 */
import { commands } from "./ipc/commands"; // `K-R135`：用户级 PATH 那三条命令
import { showActionFailureToast } from "./error-toast"; // `K-R135`：那一格的失败要出声
// `K-R93`：**盘上有几个 agent、默认是哪个，都是后端的事实** —— 本文件从前把
// `claude` / `codex` 写死了两处（下拉清单 + 越层诊断的豁免名单）。
import { listAgents, lookupAgentProfile } from "./agent-profile";

/**
 * 后端认得的 agent 的**默认拉起二进制名**（`claude` / `codex` / …）。
 *
 * 🔴 `K-R93`：刻意**只收 `defaultLauncher`，不收 `launcherAlias`**。
 * 别名（claude 的 `cc`）是用户自己那层 shell wrapper —— 它与 `cct` / `oot` 是同一类东西，
 * 照样绕开 `ccm`，**不该被豁免**。这两个值住在同一行画像里，但它们回答的不是同一个问题。
 */
function baseLaunchers(): string[] {
  const out: string[] = [];
  for (const agent of listAgents()) {
    const got = lookupAgentProfile(agent);
    // 问不到就跳过、不猜 —— 少豁免一个只是多提示一句，编一个出来才是错的。
    if (got.known) out.push(got.facts.defaultLauncher);
  }
  return out;
}

/** 该远端命令看起来是不是绕开了 `ccm`（越层启动器）——启发式：非空、不含 "ccm"、且不是
 *  某个 agent 的裸基座命令（显式写 `claude` / `codex` 是有意选择基座行为，不算"看起来像
 *  旧式包装"）。命中不代表一定错——用户可能就是要一个完全自定义的命令——只是账号/模型偏好
 *  不会随它生效，值得提醒。
 *
 *  🔴 `K-R93`（09-12）：这里从前只豁免 `claude` 一个字面量 —— 那是「前端只认 claude」
 *  那一格漏的**第二处**（第一处是 `AGENT_PROFILE`）。填 `codex` 的人从前会收到一句
 *  「你绕开了 ccm」，而他做的与填 `claude` 是同一件事。今天名单由后端那张表给。 */
export function diagnoseRemoteLauncher(cmd: string): string | null {
  const trimmed = cmd.trim();
  if (!trimmed) return null; // 空 = 走默认（后端 `ACTIVE_AGENT` 那一份），不算绕过
  if (baseLaunchers().includes(trimmed)) return null; // 显式基座，不是旧式包装
  if (/ccm/.test(trimmed)) return null; // 命令本身含 ccm 子串（可能是包了一层的自定义命令）
  return "这条命令似乎绕开了 ccm——账号/模型偏好不会随它生效。想要这些好处的话，改填 ccm（或含 ccm 的自定义命令），或在机器页「本机 → 工具 → 别名」里拼一条。";
}

/**
 * 🔴 `K-R135`（`R85`）：**用户级 PATH 那一格** —— 现在状态 · 一个按钮加 · 一个按钮撤。
 *
 * 用户逐字：「只把那个目录塞进进程内 `$env:PATH` 这是怎么做的，**删除能清干净吗？
 * 应该让用户手动点击加，也能管理删除。就像是 log 数据管理一样。**」
 * ⇒ 形状照他点名的那个范式 `src/settings/diagnostics-section.ts`
 * （路径 ＋ 现在的读数 ＋ 几个按钮，`refresh()` 在构造与点按钮时跑）。
 *
 * # 🔴 三条纪律，都是这一格特有的
 *
 * 1. **现算，不缓存** —— 每次 `refresh()` 后端真跑一趟 `powershell.exe`（几百 ms）。
 *    ⇒ 只在**构造**与**点按钮**时调，**不轮询**。用户改完 PATH 回来点一下刷新就对了。
 * 2. **探不动 ≠ 不在 PATH 上。** 后端回 `error` 时这一格显示那句原话，
 *    **不许静默成「未安装」** —— 静默的后果是用户去点「加」，那一下同样会失败，
 *    而两次失败之间他学不到任何东西（同本文件 `refreshRc` 那条「扫不动不许静默」）。
 * 3. **两个按钮互斥地禁用**：已经在了就不让点「加」（点了也只是幂等空转，但按钮亮着
 *    等于在说「还没加」）；不在就不让点「撤」。`error` / 不支持时两个都禁。
 *
 * ⚠ **非 Windows 上不隐藏，而是显示一句「这一档只有 Windows 有」** ——
 * 隐藏会让 Linux 上的人以为「我这儿没这个问题」，而事实是**他那边走的是另一条路**
 * （rc 里那个别名块，住「本机 → 工具 → 别名」）。把两条路的关系说出来，比藏起来强。
 */
export function buildUserPathBlock(): HTMLElement {
  const wrap = document.createElement("div");
  wrap.className = "settings-group ccm-user-path-block";

  const heading = document.createElement("div");
  heading.className = "settings-group-title";
  heading.textContent = "这台机器的 ccm 命令（用户级 PATH）";
  wrap.appendChild(heading);

  const hint = document.createElement("div");
  hint.className = "settings-hint";
  hint.textContent =
    "把 cc-monitor 放 ccm 的那个目录加到你的用户级 PATH 上，三种终端（PowerShell / cmd / Git Bash）就都敲得到 ccm。" +
    "只改你自己的用户级 PATH：不需要管理员、不碰系统 PATH、不碰别的用户。改完要重开终端才生效。";
  wrap.appendChild(hint);

  const statusRow = document.createElement("div");
  statusRow.className = "settings-row";
  const statusLabel = document.createElement("span");
  statusLabel.className = "settings-label";
  statusLabel.textContent = "现在状态";
  statusRow.appendChild(statusLabel);
  const statusValue = document.createElement("span");
  statusValue.className = "settings-cc-stat-value ccm-user-path-status";
  statusValue.textContent = "—";
  statusRow.appendChild(statusValue);
  wrap.appendChild(statusRow);

  const dirRow = document.createElement("div");
  dirRow.className = "settings-row settings-row-stack";
  const dirLabel = document.createElement("span");
  dirLabel.className = "settings-label";
  dirLabel.textContent = "那个目录";
  dirRow.appendChild(dirLabel);
  const dirValue = document.createElement("span");
  dirValue.className = "settings-cc-autolaunch-path-value ccm-user-path-dir";
  dirValue.style.fontFamily = "var(--font-mono, monospace)";
  dirValue.style.fontSize = "11px";
  dirValue.style.wordBreak = "break-all";
  dirValue.textContent = "—";
  dirRow.appendChild(dirValue);
  wrap.appendChild(dirRow);

  const btnRow = document.createElement("div");
  btnRow.className = "settings-cc-profile-buttons";
  const addBtn = document.createElement("button");
  addBtn.type = "button";
  addBtn.className = "settings-btn settings-btn-primary ccm-user-path-add";
  addBtn.textContent = "加到用户级 PATH";
  const delBtn = document.createElement("button");
  delBtn.type = "button";
  delBtn.className = "settings-btn settings-btn-secondary ccm-user-path-remove";
  delBtn.textContent = "从用户级 PATH 撤掉";
  delBtn.title = "只摘掉我们自己那一格，你 PATH 上别的东西一个字节都不动。";
  const refreshBtn = document.createElement("button");
  refreshBtn.type = "button";
  refreshBtn.className = "settings-btn settings-btn-secondary ccm-user-path-refresh";
  refreshBtn.textContent = "刷新";
  refreshBtn.title = "重新读一次你的用户级 PATH（现算，不缓存）";
  btnRow.append(addBtn, delBtn, refreshBtn);
  wrap.appendChild(btnRow);

  // 那两条命令的逐字文本 —— **给不想点按钮的人复制**。
  // 🔴 它与按钮跑的是**同一份字节**（后端同一个 render 函数），所以这里敢这么说。
  const cmdNote = document.createElement("div");
  cmdNote.className = "settings-hint ccm-user-path-cmd-note";
  cmdNote.textContent = "不想点按钮？下面这段就是按钮会跑的那一段，复制到 PowerShell 里自己跑一次也一样：";
  cmdNote.hidden = true;
  wrap.appendChild(cmdNote);
  const cmdPre = document.createElement("pre");
  cmdPre.className = "ccm-user-path-cmd";
  cmdPre.hidden = true;
  wrap.appendChild(cmdPre);

  const setBusy = (busy: boolean): void => {
    addBtn.disabled = busy;
    delBtn.disabled = busy;
    refreshBtn.disabled = busy;
  };

  const refresh = async (): Promise<void> => {
    setBusy(true);
    try {
      const st = await commands.ccm_user_path_status();
      dirValue.textContent = st.dir ?? "—";
      if (!st.supported) {
        // 不隐藏，说清它与别名块那一格的关系（见本函数头注第 3 条下面那一段）。
        statusValue.textContent =
          "这一档只有 Windows 有。这台机器上让终端找到 ccm 的，是「本机 → 工具 → 别名」里那个别名块。";
        addBtn.hidden = true;
        delBtn.hidden = true;
        cmdNote.hidden = true;
        cmdPre.hidden = true;
        return;
      }
      if (st.error) {
        // 🔴 探不动 ≠ 不在 PATH 上。原话上屏，两个按钮都不给点。
        statusValue.textContent = `读不出来：${st.error}`;
        addBtn.disabled = true;
        delBtn.disabled = true;
        return;
      }
      statusValue.textContent = st.onUserPath
        ? "✓ 已经在你的用户级 PATH 上"
        : "✗ 还不在你的用户级 PATH 上（三种终端里都敲不到 ccm）";
      addBtn.disabled = st.onUserPath;
      delBtn.disabled = !st.onUserPath;
      const text = st.onUserPath ? st.removeCommand : st.addCommand;
      cmdPre.textContent = text ?? "";
      cmdNote.hidden = !text;
      cmdPre.hidden = !text;
    } catch (e) {
      statusValue.textContent = `读不出来：${String(e)}`;
      addBtn.disabled = true;
      delBtn.disabled = true;
    } finally {
      refreshBtn.disabled = false;
    }
  };

  const act = async (what: "add" | "remove"): Promise<void> => {
    setBusy(true);
    try {
      if (what === "add") await commands.ccm_user_path_add();
      else await commands.ccm_user_path_remove();
    } catch (e) {
      showActionFailureToast(
        what === "add" ? "加到用户级 PATH 失败" : "从用户级 PATH 撤掉失败",
        String(e),
      );
    }
    // 成功失败都重扫：**盘上现在是什么样，就显示什么样**（不拿我们以为的结果去写界面）。
    await refresh();
  };

  addBtn.addEventListener("click", () => void act("add"));
  delBtn.addEventListener("click", () => void act("remove"));
  refreshBtn.addEventListener("click", () => void refresh());
  void refresh();
  return wrap;
}
