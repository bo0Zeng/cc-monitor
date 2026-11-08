/**
 * 本地端口转发（-L）管理台：浮层面板（body 级 fixed），列当前转发 ＋ 加转发表单（选主机 / 本地端口 / 远端 host:port）＋ 启停 ＋ 刷新。
 * 转发账住本机常驻后端：起 / 停 / 列经通道直接问它（`../port-forward-reads.ts`），
 * 转发走本机后端池里到那台的 SSH 连接（复用连接大脑）。
 */
import { failToast } from "../kit/toast";
import { saidOf } from "../kit/detail";
import { readRemoteNames } from "../machine-feed";

// `connCount`：累计连接数，按**累计连接数**量纲算 2^53-1 条（每秒 1000 连接要 28.5 万年）⇒ `number` 够用。
import { listForwards, startForward, stopForward, type ForwardStatus } from "../port-forward-reads";
import { copyText } from "../copy-table";
import { dispatcher, type OverlayHandle } from "../keybindings/registry";

function mkBtn(label: string, onClick: () => void): HTMLButtonElement {
  const b = document.createElement("button");
  b.type = "button";
  b.className = "pf-btn";
  b.textContent = label;
  b.addEventListener("click", onClick);
  return b;
}

function mkInput(placeholder: string): HTMLInputElement {
  const i = document.createElement("input");
  i.type = "text";
  i.className = "pf-input";
  i.placeholder = placeholder;
  i.spellcheck = false;
  return i;
}

class PortForwardPanel {
  private el: HTMLElement;
  private listEl!: HTMLElement;
  private originSel!: HTMLSelectElement;
  private localInput!: HTMLInputElement;
  private rhostInput!: HTMLInputElement;
  private rportInput!: HTMLInputElement;
  /** 打开时读到的那几台（起转发时一并交它的配置：那台的流没起来时本机后端按它自己拨）。 */
  /** Esc 栈里的这一层：Esc 只关这个面板，不连带关下面那层（设置窗）。 */
  private readonly layer: OverlayHandle = { handleEsc: () => (this.close(), true) };

  constructor() {
    this.el = document.createElement("div");
    this.el.className = "pf-overlay";
    this.el.style.display = "none";
    this.el.appendChild(this.buildChrome());
    document.body.appendChild(this.el);
    this.el.addEventListener("click", (e) => {
      if (e.target === this.el) this.close();
    });
  }

  private buildChrome(): HTMLElement {
    const panel = document.createElement("div");
    panel.className = "pf-panel";

    const header = document.createElement("div");
    header.className = "pf-header";
    const title = document.createElement("span");
    title.className = "pf-title";
    title.textContent = copyText("portForward.buildChrome.title");
    header.appendChild(title);
    header.appendChild(mkBtn(copyText("accountChip.menu.refresh"), () => void this.reload()));
    const close = mkBtn(copyText("kit.drawer.close"), () => this.close());
    close.classList.add("pf-close");
    header.appendChild(close);
    panel.appendChild(header);

    const form = document.createElement("div");
    form.className = "pf-form";
    this.originSel = document.createElement("select");
    this.originSel.className = "pf-input";
    form.appendChild(this.originSel);
    this.localInput = mkInput(copyText("portForward.buildChrome.localPort"));
    form.appendChild(this.localInput);
    const arrow = document.createElement("span");
    arrow.className = "pf-arrow";
    arrow.textContent = copyText("registry.prettyChord.right");
    form.appendChild(arrow);
    this.rhostInput = mkInput(copyText("portForward.buildChrome.remoteHost"));
    this.rhostInput.value = "localhost";
    form.appendChild(this.rhostInput);
    this.rportInput = mkInput(copyText("portForward.buildChrome.remotePort"));
    form.appendChild(this.rportInput);
    const startBtn = mkBtn(copyText("backend.buildCells.start"), () => void this.onStart());
    startBtn.classList.add("pf-start");
    form.appendChild(startBtn);
    panel.appendChild(form);

    this.listEl = document.createElement("div");
    this.listEl.className = "pf-list";
    panel.appendChild(this.listEl);

    return panel;
  }

  async open(): Promise<void> {
    this.el.style.display = "flex";
    dispatcher.pushOverlay(this.layer);
    this.originSel.innerHTML = "";
    try {
      for (const origin of await readRemoteNames()) {
        const opt = document.createElement("option");
        opt.value = origin;
        opt.textContent = origin;
        this.originSel.appendChild(opt);
      }
    } catch {
      /* 本机后端问不到 → 空下拉,用户仍可看列表 */
    }
    await this.reload();
  }

  close(): void {
    dispatcher.popOverlay(this.layer);
    this.el.style.display = "none";
  }

  private async reload(): Promise<void> {
    let forwards: ForwardStatus[] = [];
    try {
      forwards = await listForwards();
    } catch (e) {
      failToast(copyText("portForward.reload.listFailed"), e);
    }
    this.listEl.innerHTML = "";
    if (forwards.length === 0) {
      const empty = document.createElement("div");
      empty.className = "pf-empty";
      empty.textContent = copyText("portForward.reload.empty");
      this.listEl.appendChild(empty);
      return;
    }
    for (const f of forwards) {
      const row = document.createElement("div");
      row.className = "pf-row";
      const dot = document.createElement("span");
      dot.className = `pf-dot pf-dot-${f.state === "running" ? "ok" : "err"}`;
      dot.title = f.state;
      row.appendChild(dot);
      const desc = document.createElement("span");
      desc.className = "pf-desc";
      desc.textContent = copyText("portForward.reload.row", { origin: f.origin, localPort: f.localPort, remoteHost: f.remoteHost, remotePort: f.remotePort, connCount: f.connCount });
      row.appendChild(desc);
      row.appendChild(mkBtn(copyText("composer.input.stop"), () => void this.onStop(f.id)));
      this.listEl.appendChild(row);
    }
  }

  private async onStart(): Promise<void> {
    try {
      // 只交名字与敲进去的原文：没选机器 · 端口不对 · 主机空都由本机后端判、写好那一句；怎么拨照它的机器表（流没起来也照拨，不拒）。
      await startForward({ origin: this.originSel.value, localPort: this.localInput.value, remoteHost: this.rhostInput.value, remotePort: this.rportInput.value });
      this.localInput.value = "";
      this.rportInput.value = "";
      await this.reload();
    } catch (e) {
      failToast(saidOf(e), e);
    }
  }

  private async onStop(id: string): Promise<void> {
    try {
      await stopForward(id);
      await this.reload();
    } catch (e) {
      failToast(copyText("portForward.onStop.failed"), e);
    }
  }
}

let singleton: PortForwardPanel | null = null;
/** 打开端口转发管理台(单例 overlay)。 */
export function openPortForwardPanel(): void {
  singleton ??= new PortForwardPanel();
  void singleton.open();
}
