/**
 * F03b（devbench）：**收件箱编辑 overlay** —— skill 接入面的「结构化注入」入口。
 *
 * # 它为什么只是一个文本框
 *
 * 定框 C2〔用 08-10〕：「**最好做得轻一点，因为 skill 形式可能改，而只有 inbox 这种
 * 「结构化注入」的核心哲学不容易变**」。⇒ 做进 UI 的只许是「人往一个文件里写意图、
 * agent 下一轮读它」这个**机制**；skill 的阶段名、命令名、schema、判据清单一律不进 UI。
 *
 * 所以这里**没有**：工作区切换器（实例只做展示上下文）· 阶段视图 · 判据面板 · 进度条。
 * 有的只是：这个 skill 在不在（带身份）· 有哪些实例 · 那个文件的内容 · 一个保存按钮。
 *
 * # 入口走命令面板，不加第 8 个顶栏按钮
 *
 * `main.ts:516` 逐字有先例「**刻意不加第 7 个顶栏图标** —— F84 加命令面板时就为
 * 「顶栏已拥挤」立过先例」。而命令面板自己的纪律（`command-bar.ts:4-6`）逐字是
 * 「**首刀只列只读命令**（**开 overlay** / 窗口操作 / 导航）；resume/new-session/attach/
 * kill/delete 等**写/驱动动作首刀排除**」——「**开 overlay**」明确在允许列表里。
 * ⇒ 走命令面板**不撞那条纪律**：写动作发生在本 overlay 内部的「保存」上，
 * 那是 F47 口径里的「面板内一次直接用户手势」（`doc/INVARIANTS.md:23`）。
 *
 * # 安全判断**不在这里**
 *
 * 前端不做任何路径判断。写面围栏的真相源只有一处：Rust 侧 `skill_host::resolve_editable`
 * 的三道（路径 `canonicalize` **之后**做集合判定 · 过 Claude 数据保护守卫 · 目标必须已存在），
 * 之后再经 `verified_write` 读回逐字节比对。⇒ 本文件只负责**把后端算好的路径原样送回去**。
 *
 * ⚠ `missingReason` 非 null 时**原样显示**（定框 C6）：它是带身份的缺席原因
 * （哪个 skill 的哪条前提没满足）。不许退化成「不可用」——那正是 C6 禁的形状。
 *
 * 全 textContent，无 innerHTML（同 `command-bar.ts` 那条纪律）。
 */
import { commands, type SkillView } from "../ipc/commands";
import { dispatcher, type OverlayHandle } from "../keybindings/registry";
import { showActionFailureToast } from "../error-toast";

type CwdGetter = () => { cwd: string; origin: string | null } | null;

export class InboxView implements OverlayHandle {
  private root: HTMLDivElement | null = null;
  private statusEl: HTMLDivElement | null = null;
  private pathEl: HTMLDivElement | null = null;
  private textarea: HTMLTextAreaElement | null = null;
  private saveBtn: HTMLButtonElement | null = null;
  /** 当前编辑对象：`null` = 没有可编辑的文件（skill 不在场 / 该 skill 没声明 editable）。 */
  private target: { skillId: string; path: string } | null = null;
  private cwd = "";

  constructor(private getRepo: CwdGetter) {}

  isVisible(): boolean {
    return this.root !== null && this.root.style.display !== "none";
  }

  handleEsc(): boolean {
    this.close();
    return true;
  }

  async open(): Promise<void> {
    const info = this.getRepo();
    if (!info || info.origin !== null) {
      // 远端会话：计划目录在远端机上，本地读不到。
      // ⚠ 与 panorama 同一条诚实降级（它对远端也是「不索引 + 说清为什么」）。
      // 远端项目的收件箱要不要能编辑，`parity_ledger` 里记着 `Undecided`——没人裁定过。
      showActionFailureToast(
        "收件箱仅支持本地会话",
        info?.origin
          ? `当前会话来自远端机 [${info.origin}]，计划目录在那台机器上，本地读不到。切到一个本地会话再打开。`
          : "当前 tab 没有工作目录。",
      );
      return;
    }
    this.cwd = info.cwd;
    this.ensureDom();
    if (this.root) this.root.style.display = "flex";
    dispatcher.pushOverlay(this);
    await this.reload();
  }

  close(): void {
    if (this.root) this.root.style.display = "none";
    dispatcher.popOverlay(this);
  }

  /** 拉一次 skill 列表并把「第一个有可编辑文件的」装进编辑区。 */
  private async reload(): Promise<void> {
    if (!this.statusEl || !this.textarea || !this.pathEl || !this.saveBtn) return;
    this.statusEl.textContent = "读取中…";
    this.textarea.value = "";
    this.textarea.disabled = true;
    this.saveBtn.disabled = true;
    this.target = null;

    let skills: SkillView[];
    try {
      skills = await commands.list_skills({ cwd: this.cwd });
    } catch (e) {
      this.statusEl.textContent = `读不到 skill 列表：${String(e)}`;
      return;
    }

    // 状态行：每个 skill 一句。★ 缺席的**原样显示后端给的带身份原因**（C6）。
    this.statusEl.textContent = skills
      .map((s) => {
        const where = s.missing_reason ?? `在场（${s.instances.length} 个实例）`;
        return `${s.label}：${where}`;
      })
      .join(" ｜ ");

    const editable = skills.find((s) => s.missing_reason === null && s.editable.length > 0);
    if (!editable) {
      this.pathEl.textContent = "";
      this.statusEl.textContent += " ｜ 没有可编辑的收件箱";
      return;
    }
    const path = editable.editable[0];
    this.pathEl.textContent = path;
    try {
      this.textarea.value = await commands.read_skill_file({
        cwd: this.cwd,
        skillId: editable.id,
        path,
      });
    } catch (e) {
      this.statusEl.textContent += ` ｜ 读文件失败：${String(e)}`;
      return;
    }
    this.target = { skillId: editable.id, path };
    this.textarea.disabled = false;
    this.saveBtn.disabled = false;
  }

  private async save(): Promise<void> {
    if (!this.target || !this.textarea || !this.statusEl || !this.saveBtn) return;
    const { skillId, path } = this.target;
    this.saveBtn.disabled = true;
    try {
      await commands.write_skill_file({
        cwd: this.cwd,
        skillId,
        path,
        content: this.textarea.value,
      });
      this.statusEl.textContent = "已保存（后端已读回逐字节比对）";
    } catch (e) {
      // ⚠ 写面围栏拒绝时后端给的是**带理由的**错误（哪条围栏、为什么）——原样显示。
      showActionFailureToast("保存失败", String(e));
    } finally {
      this.saveBtn.disabled = false;
    }
  }

  private ensureDom(): void {
    if (this.root) return;
    const root = document.createElement("div");
    root.className = "inbox-overlay";

    const panel = document.createElement("div");
    panel.className = "inbox-panel";

    const head = document.createElement("div");
    head.className = "inbox-head";
    const title = document.createElement("span");
    title.textContent = "收件箱 — 写给下一轮的 agent";
    const closeBtn = document.createElement("button");
    closeBtn.type = "button";
    closeBtn.className = "inbox-close";
    closeBtn.setAttribute("aria-label", "关闭");
    closeBtn.textContent = "关闭";
    closeBtn.addEventListener("click", () => this.close());
    head.append(title, closeBtn);

    const status = document.createElement("div");
    status.className = "inbox-status";
    const pathLine = document.createElement("div");
    pathLine.className = "inbox-path";

    const ta = document.createElement("textarea");
    ta.className = "inbox-text";
    ta.spellcheck = false;
    ta.placeholder = "一行一条：新需求 / 改进 / 纠正。下一轮 agent 会读它、处置它、清空它。";

    const foot = document.createElement("div");
    foot.className = "inbox-foot";
    const save = document.createElement("button");
    save.type = "button";
    save.className = "inbox-save";
    save.textContent = "保存";
    save.addEventListener("click", () => void this.save());
    foot.append(save);

    panel.append(head, status, pathLine, ta, foot);
    root.append(panel);
    root.addEventListener("click", (e) => {
      if (e.target === root) this.close();
    });
    document.body.appendChild(root);

    this.root = root;
    this.statusEl = status;
    this.pathEl = pathLine;
    this.textarea = ta;
    this.saveBtn = save;
  }
}
