// A3：设置面板「账号」组（多账号 cc-acct-iso）。占用原「远端」空占位组。
//
// 展示某台远端的账号列表（名/邮箱/mode/登录态/configDir/默认）+ 设为默认 / 复制 configDir /
// 刷新。**只读 + 改本机默认账号**（写 config.json，不碰远端 manifest、不注入、不重启——A4/A5）。
// 部署引导（未启用时）留 A6 填；本组先给出"如何启用"的说明与 manifest 路径。
//
// 设置窗独立于主窗、拿不到活跃会话，故用远端选择器（多台时下拉）。改默认账号后
// emit(SETTINGS_APPLIED_EVENT) 让主窗状态栏 chip 同步。
import { emit } from "@tauri-apps/api/event";
import { invoke } from "@tauri-apps/api/core";
import {
  fetchAccounts,
  deriveUi,
  currentWorkingAccount,
  isSelectable,
  setDefaultName,
  invalidateAccountsCache,
  type AccountsState,
  type Account,
} from "../accounts";
import { pickPrimaryOrigin } from "../account-chip";
import { readRemoteConfig, type RemoteHostConfig } from "./remote-section";
import { showActionFailureToast } from "../error-toast";
import { SETTINGS_APPLIED_EVENT } from "./events";
import { buildAcctIsoCmd, validateAcctName, type AcctIsoStep } from "./acct-deploy";

export class AccountsSection {
  readonly element: HTMLElement;
  private body: HTMLElement;
  private originSelect: HTMLSelectElement;
  private hosts: RemoteHostConfig[] = [];
  private origin: string | null = null;

  constructor() {
    const root = document.createElement("div");
    root.className = "settings-group settings-accounts";

    // 顶部：远端选择 + 刷新
    const bar = document.createElement("div");
    bar.className = "accounts-bar";
    const originLabel = document.createElement("span");
    originLabel.className = "accounts-bar-label";
    originLabel.textContent = "远端：";
    bar.appendChild(originLabel);
    this.originSelect = document.createElement("select");
    this.originSelect.className = "accounts-origin-select";
    this.originSelect.addEventListener("change", () => {
      this.origin = this.originSelect.value || null;
      void this.reload(true);
    });
    bar.appendChild(this.originSelect);
    const refresh = document.createElement("button");
    refresh.type = "button";
    refresh.className = "accounts-refresh";
    refresh.textContent = "刷新";
    refresh.addEventListener("click", () => {
      if (this.origin) invalidateAccountsCache(this.origin);
      void this.reload(true);
    });
    bar.appendChild(refresh);
    root.appendChild(bar);

    this.body = document.createElement("div");
    this.body.className = "accounts-body";
    root.appendChild(this.body);

    this.element = root;
    void this.init();
  }

  private async init(): Promise<void> {
    try {
      const cfg = await readRemoteConfig();
      this.hosts = cfg.enabled ? cfg.hosts : [];
    } catch {
      this.hosts = [];
    }
    // 填远端下拉（含 daemonless，但标注）
    this.originSelect.innerHTML = "";
    for (const h of this.hosts) {
      const opt = document.createElement("option");
      const label = h.label || h.host;
      opt.value = label;
      opt.textContent = h.daemonless ? `${label}（daemonless）` : label;
      this.originSelect.appendChild(opt);
    }
    this.origin = pickPrimaryOrigin(this.hosts) ?? (this.hosts[0]?.label || this.hosts[0]?.host || null);
    if (this.origin) this.originSelect.value = this.origin;
    this.originSelect.style.display = this.hosts.length > 1 ? "" : "none";
    await this.reload(false);
  }

  private async reload(force: boolean): Promise<void> {
    this.body.innerHTML = "";
    if (!this.origin) {
      this.info("没有已配置的远端。账号功能在远端 Linux 上——先在「连接」组配一台远端。");
      return;
    }
    let state: AccountsState;
    try {
      state = await fetchAccounts(this.origin, force);
    } catch (e) {
      this.info(`拉取账号失败：${String(e)}`);
      return;
    }
    const ui = deriveUi(state);
    switch (ui.kind) {
      case "hidden":
        this.info("该远端配置为 daemonless，无法读取账号。");
        return;
      case "needs-update":
        this.info(`远端 daemon 需要更新才能用多账号：${ui.reason}`);
        return;
      case "not-enabled":
        this.renderNotEnabled(ui.manifestPath, ui.reason);
        return;
      case "ready":
        this.renderTable(state, ui.accounts);
        return;
    }
  }

  /**
   * A6：在远端终端里跑一个部署/维护步骤——构建命令（校验失败即提示不动手）→ danger 步二次确认 →
   * `launch_remote_terminal` 弹真实终端让用户看着跑（DESIGN §6，不经 daemon、不代跑）。
   */
  private async launchStep(
    step: AcctIsoStep,
    opts: { danger?: boolean; confirmExtra?: string } = {},
  ): Promise<void> {
    if (!this.origin) return;
    const built = buildAcctIsoCmd(step);
    if (!built.ok) {
      showActionFailureToast("命令无法生成", built.reason, { level: "error" });
      return;
    }
    if (opts.danger) {
      const msg =
        `将在远端「${this.origin}」的终端里运行：\n\n${built.cmd}\n\n` +
        (opts.confirmExtra ? `${opts.confirmExtra}\n\n` : "") +
        `命令在你看得见的终端里执行、需你亲手确认；工具自带备份，可 rollback。继续？`;
      if (!window.confirm(msg)) return;
    }
    try {
      await invoke("launch_remote_terminal", { origin: this.origin, remoteCmd: built.cmd });
      showActionFailureToast("已在终端里运行", "完成后点「刷新」更新账号列表。", {
        level: "info",
        durationMs: 5000,
      });
    } catch (e) {
      showActionFailureToast("拉起终端失败", String(e), { level: "error" });
    }
  }

  /** A6：未启用 → 内联「启用多账号」向导（无 modal）：填默认账号名 → 预览命令 → 分步弹终端。 */
  private renderNotEnabled(manifestPath: string | null, reason: string): void {
    const box = document.createElement("div");
    box.className = "accounts-not-enabled";

    const h = document.createElement("div");
    h.className = "accounts-ne-title";
    h.textContent = "该远端尚未启用多账号";
    box.appendChild(h);

    const p = document.createElement("div");
    p.className = "accounts-ne-body";
    p.innerHTML =
      `在远端跑 cc-acct-iso 迁移管线即可启用（各账号独立凭据、skills/记忆/历史/设置实时共享）。<br>` +
      `原因：${escapeHtml(reason)}<br>manifest 查找位置：<code>${escapeHtml(manifestPath ?? "—")}</code>`;
    box.appendChild(p);

    const wiz = document.createElement("div");
    wiz.className = "accounts-wizard";

    // 默认账号名输入 + 实时校验。
    const field = document.createElement("div");
    field.className = "accounts-wiz-field";
    const label = document.createElement("label");
    label.textContent = "默认账号名（迁移现有默认账号进来）：";
    const input = document.createElement("input");
    input.type = "text";
    input.className = "accounts-wiz-name";
    input.placeholder = "例如 z";
    field.appendChild(label);
    field.appendChild(input);
    const err = document.createElement("div");
    err.className = "accounts-wiz-err";
    field.appendChild(err);
    wiz.appendChild(field);

    // 命令预览（只读，可复制）。
    const preview = document.createElement("pre");
    preview.className = "accounts-wiz-preview";
    wiz.appendChild(preview);
    const copyRow = document.createElement("div");
    copyRow.className = "accounts-wiz-copyrow";
    const copyBtn = mkBtn("复制命令");
    copyBtn.addEventListener("click", () => {
      void navigator.clipboard?.writeText(preview.textContent ?? "").then(
        () => showActionFailureToast("已复制命令", "可粘到远端终端里跑。", { level: "info", durationMs: 2500 }),
        () => showActionFailureToast("复制失败", "剪贴板不可用", { level: "error" }),
      );
    });
    copyRow.appendChild(copyBtn);
    wiz.appendChild(copyRow);

    // 分步按钮。
    const btns = document.createElement("div");
    btns.className = "accounts-wiz-btns";
    const bPreview = mkBtn("① 预览计划（dry-run）");
    const bApply = mkBtn("② 执行迁移（--apply）");
    bApply.classList.add("danger");
    const bVerify = mkBtn("③ 自检 verify");
    const bShellinit = mkBtn("④ 打印 rc 片段");
    btns.append(bPreview, bApply, bVerify, bShellinit);
    wiz.appendChild(btns);

    const note = document.createElement("div");
    note.className = "accounts-wiz-note";
    note.innerHTML =
      "<small>顺序：① 看计划（什么都不动）→ ② 执行迁移 → ③ 自检 → ④ 把打印的片段贴进 " +
      "<code>~/.bashrc</code>（工具不改你的 rc）→ 回来点「刷新」。第二个账号可在启用后用「加账号」导入。</small>";
    wiz.appendChild(note);

    // —— 校验驱动的启用/禁用 + 预览 ——
    const sync = (): void => {
      const name = input.value.trim();
      const v = validateAcctName(name);
      const valid = v.ok;
      err.textContent = name && !v.ok ? v.reason : "";
      for (const b of [bPreview, bApply, bShellinit]) b.disabled = !valid;
      // verify 不依赖名字（自检当前状态），恒可点。
      const pv = valid ? buildAcctIsoCmd({ kind: "init-preview", name }) : null;
      const ap = valid ? buildAcctIsoCmd({ kind: "init-apply", name }) : null;
      preview.textContent =
        pv && pv.ok && ap && ap.ok
          ? `# 预览计划（零落盘）\n${pv.cmd}\n\n# 执行迁移\n${ap.cmd}\n\n# 打印 rc 片段\ncc-acct-iso shellinit`
          : "（填入合法账号名后显示将运行的命令）";
    };
    input.addEventListener("input", sync);
    bPreview.addEventListener("click", () =>
      void this.launchStep({ kind: "init-preview", name: input.value.trim() }),
    );
    bApply.addEventListener("click", () =>
      void this.launchStep(
        { kind: "init-apply", name: input.value.trim() },
        { danger: true, confirmExtra: "这会把现有默认账号的凭据与 .claude.json 搬进账号库、其余项建 symlink。" },
      ),
    );
    bVerify.addEventListener("click", () => void this.launchStep({ kind: "verify" }));
    bShellinit.addEventListener("click", () => void this.launchStep({ kind: "shellinit" }));
    sync();

    box.appendChild(wiz);
    this.body.appendChild(box);
  }

  private renderTable(state: AccountsState, accounts: Account[]): void {
    const def = currentWorkingAccount(state);
    const meta = state.meta;
    if (meta) {
      const info = document.createElement("div");
      info.className = "accounts-meta";
      info.textContent = `已启用 · ${accounts.length} 个账号 · manifest ${meta.manifestPath}${meta.updatedAt ? ` · 更新于 ${meta.updatedAt}` : ""}`;
      this.body.appendChild(info);
    }
    const table = document.createElement("div");
    table.className = "accounts-table";
    for (const a of accounts) {
      table.appendChild(this.accountRow(a, def?.name === a.name));
    }
    this.body.appendChild(table);

    const hint = document.createElement("div");
    hint.className = "accounts-hint";
    hint.textContent =
      "点某账号「设为当前账号」= 以后新会话、以及没指定过账号的 resume 都用它；正在跑的会话不受影响、不动远端、不碰凭据。未登录的账号可点它那行的「去登录」在终端里 /login。";
    this.body.appendChild(hint);

    this.body.appendChild(this.renderMaintenance());
  }

  /** A6：已启用态的「维护」区——加账号 / 自检 / 补链，均弹终端。 */
  private renderMaintenance(): HTMLElement {
    const box = document.createElement("div");
    box.className = "accounts-maint";
    const title = document.createElement("div");
    title.className = "accounts-maint-title";
    title.textContent = "维护";
    box.appendChild(title);

    // 加账号：内联小表单（名 + 可选凭据快照路径）→ 弹终端 add --apply。
    const addForm = document.createElement("div");
    addForm.className = "accounts-maint-add";
    const nameIn = document.createElement("input");
    nameIn.type = "text";
    nameIn.placeholder = "新账号名（如 b）";
    nameIn.className = "accounts-maint-name";
    const credIn = document.createElement("input");
    credIn.type = "text";
    credIn.placeholder = "可选：旧凭据快照路径（免重登）";
    credIn.className = "accounts-maint-cred";
    const addBtn = mkBtn("加账号…");
    addBtn.classList.add("danger");
    const addErr = document.createElement("span");
    addErr.className = "accounts-maint-err";
    const syncAdd = (): void => {
      const v = validateAcctName(nameIn.value.trim());
      addBtn.disabled = !v.ok;
      addErr.textContent = nameIn.value.trim() && !v.ok ? v.reason : "";
    };
    nameIn.addEventListener("input", syncAdd);
    addBtn.addEventListener("click", () => {
      const name = nameIn.value.trim();
      const credFile = credIn.value.trim() || undefined;
      void this.launchStep(
        { kind: "add-apply", name, credFile },
        {
          danger: true,
          confirmExtra: credFile
            ? "将新建该账号 config-dir 并从指定快照导入凭据（免重登）。"
            : "将新建该账号 config-dir；随后在弹出的终端里用它 /login。",
        },
      );
    });
    addForm.append(nameIn, credIn, addBtn, addErr);
    box.appendChild(addForm);
    syncAdd();

    // 自检 / 补链。
    const ops = document.createElement("div");
    ops.className = "accounts-maint-ops";
    const verifyBtn = mkBtn("自检 verify");
    verifyBtn.addEventListener("click", () => void this.launchStep({ kind: "verify" }));
    const syncBtn = mkBtn("补链 sync");
    syncBtn.addEventListener("click", () =>
      void this.launchStep(
        { kind: "sync-apply" },
        { danger: true, confirmExtra: "补齐/修复共享库软链、修权限、刷新 manifest 邮箱（幂等）。" },
      ),
    );
    ops.append(verifyBtn, syncBtn);
    box.appendChild(ops);
    return box;
  }

  private accountRow(a: Account, isCurrent: boolean): HTMLElement {
    const row = document.createElement("div");
    row.className = "accounts-row";
    if (isCurrent) row.classList.add("current");

    const mark = document.createElement("span");
    mark.className = "accounts-row-mark";
    mark.textContent = isCurrent ? "★" : "";
    mark.title = isCurrent ? "当前工作账号" : "";
    row.appendChild(mark);

    const name = document.createElement("span");
    name.className = "accounts-row-name";
    name.textContent = a.name;
    row.appendChild(name);

    const email = document.createElement("span");
    email.className = "accounts-row-email";
    email.textContent = a.email || "—";
    row.appendChild(email);

    const badge = document.createElement("span");
    badge.className = "accounts-row-badge";
    if (a.mode === "in-place") {
      badge.textContent = "逃生口";
      badge.classList.add("warn");
      badge.title = "in-place 模式：cc-monitor 不支持对它按会话切号";
    } else if (!a.loggedIn) {
      badge.textContent = "未登录";
      badge.classList.add("warn");
    } else {
      badge.textContent = "已登录";
    }
    row.appendChild(badge);

    const dir = document.createElement("span");
    dir.className = "accounts-row-dir";
    dir.textContent = a.configDir;
    dir.title = a.configDir;
    row.appendChild(dir);

    const actions = document.createElement("span");
    actions.className = "accounts-row-actions";
    if (isSelectable(a) && !isCurrent) {
      const setDef = document.createElement("button");
      setDef.type = "button";
      setDef.textContent = "设为当前账号";
      setDef.addEventListener("click", () => void this.selectDefault(a));
      actions.appendChild(setDef);
    }
    const copy = document.createElement("button");
    copy.type = "button";
    copy.textContent = "复制路径";
    copy.addEventListener("click", () => {
      void navigator.clipboard?.writeText(a.configDir).then(
        () => showActionFailureToast("已复制", a.configDir, { level: "info", durationMs: 2500 }),
        () => showActionFailureToast("复制失败", "剪贴板不可用", { level: "error" }),
      );
    });
    actions.appendChild(copy);
    // A6：打开该账号的终端（去 /login / 修复登录）——对 in-place 逃生口不给（不支持切号）。
    if (a.mode !== "in-place") {
      const login = document.createElement("button");
      login.type = "button";
      login.textContent = a.loggedIn ? "登录终端" : "去登录";
      login.title = "用该账号打开一个远端终端（在里面 /login）";
      login.addEventListener("click", () => void this.launchStep({ kind: "login", name: a.name }));
      actions.appendChild(login);
    }
    row.appendChild(actions);
    return row;
  }

  private async selectDefault(a: Account): Promise<void> {
    try {
      await setDefaultName(a.name);
      if (this.origin) invalidateAccountsCache(this.origin);
      await this.reload(true);
      void emit(SETTINGS_APPLIED_EVENT); // 让主窗状态栏 chip 同步
      showActionFailureToast(
        "已设为当前工作账号",
        `以后新会话 / resume 默认用 ${a.name}；正在跑的会话不受影响。`,
        { level: "info", durationMs: 4000 },
      );
    } catch (e) {
      showActionFailureToast("设为当前账号失败", String(e), { level: "error" });
    }
  }

  private info(text: string): void {
    const p = document.createElement("div");
    p.className = "accounts-info";
    p.textContent = text;
    this.body.appendChild(p);
  }
}

function escapeHtml(s: string): string {
  return s.replace(/[&<>"]/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;" })[c] ?? c);
}

/** A6 向导用的按钮工厂（type=button，避免 form 默认提交）。 */
function mkBtn(text: string): HTMLButtonElement {
  const b = document.createElement("button");
  b.type = "button";
  b.textContent = text;
  return b;
}
