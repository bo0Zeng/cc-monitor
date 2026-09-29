/**
 * F58：本地端口转发(-L)管理台。overlay 面板(照 SFTP panel 范式,body-level fixed)——
 * 列当前转发 + 加转发表单(选主机/本地端口/远端 host:port)+ 启停 + 刷新。
 * 〔MIG-1 · `设计/99 §2.1 ⑬`〕转发账住本机常驻后端：起 / 停 / 列经通道直接问它（`../port-forward-reads.ts`），
 * 转发走本机后端池里到那台的 SSH 连接（复用连接大脑）。
 */
import { showActionFailureToast } from "../error-toast";
import { hostKey, readRemoteConfig, type RemoteHostConfig } from "../remote-config";

// `connCount`：累计连接数，按**累计连接数**量纲算 2^53-1 条（每秒 1000 连接要 28.5 万年）⇒ `number` 够用。
import { listForwards, startForward, stopForward, type ForwardStatus } from "../port-forward-reads";
import { copyText } from "../copy-table";

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
  private hosts: RemoteHostConfig[] = [];

  constructor() {
    this.el = document.createElement("div");
    this.el.className = "pf-overlay";
    this.el.style.display = "none";
    this.el.appendChild(this.buildChrome());
    document.body.appendChild(this.el);
    this.el.addEventListener("click", (e) => {
      if (e.target === this.el) this.close();
    });
    document.addEventListener("keydown", (e) => {
      if (e.key === "Escape" && this.el.style.display !== "none") this.close();
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
    header.appendChild(mkBtn(copyText("portForward.buildChrome.refresh"), () => void this.reload()));
    const close = mkBtn(copyText("portForward.buildChrome.close"), () => this.close());
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
    arrow.textContent = copyText("portForward.buildChrome.arrow");
    form.appendChild(arrow);
    this.rhostInput = mkInput(copyText("portForward.buildChrome.remoteHost"));
    this.rhostInput.value = "localhost";
    form.appendChild(this.rhostInput);
    this.rportInput = mkInput(copyText("portForward.buildChrome.remotePort"));
    form.appendChild(this.rportInput);
    const startBtn = mkBtn(copyText("portForward.buildChrome.start"), () => void this.onStart());
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
    this.originSel.innerHTML = "";
    try {
      const { hosts } = await readRemoteConfig();
      this.hosts = hosts;
      for (const h of hosts) {
        const origin = h.label.trim() || h.host;
        const opt = document.createElement("option");
        opt.value = origin;
        opt.textContent = origin;
        this.originSel.appendChild(opt);
      }
    } catch {
      /* 读配置失败 → 空下拉,用户仍可看列表 */
    }
    await this.reload();
  }

  close(): void {
    this.el.style.display = "none";
  }

  private async reload(): Promise<void> {
    let forwards: ForwardStatus[] = [];
    try {
      forwards = await listForwards();
    } catch (e) {
      showActionFailureToast(copyText("portForward.reload.listFailed"), String(e));
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
      row.appendChild(mkBtn(copyText("portForward.reload.stop"), () => void this.onStop(f.id)));
      this.listEl.appendChild(row);
    }
  }

  private async onStart(): Promise<void> {
    const origin = this.originSel.value;
    const localPort = Number.parseInt(this.localInput.value, 10);
    const remoteHost = this.rhostInput.value.trim();
    const remotePort = Number.parseInt(this.rportInput.value, 10);
    if (!origin) {
      showActionFailureToast(copyText("portForward.onStart.title"), copyText("portForward.onStart.noRemote"));
      return;
    }
    const validPort = (p: number): boolean => Number.isInteger(p) && p > 0 && p <= 65535;
    if (!validPort(localPort)) {
      showActionFailureToast(copyText("portForward.onStart.title"), copyText("portForward.onStart.badLocalPort"));
      return;
    }
    if (!remoteHost) {
      showActionFailureToast(copyText("portForward.onStart.title"), copyText("portForward.onStart.noHost"));
      return;
    }
    if (!validPort(remotePort)) {
      showActionFailureToast(copyText("portForward.onStart.title"), copyText("portForward.onStart.badRemotePort"));
      return;
    }
    try {
      // 〔MIG-1 续〕那台的配置（＋ 跳板那一台）一并交：它的流没起来时本机后端按配置自己拨，不拒。
      const machine = this.hosts.find((h) => hostKey(h) === origin) ?? null;
      const jumpName = machine?.jump.trim() ?? "";
      const jump = jumpName ? (this.hosts.find((h) => hostKey(h) === jumpName) ?? null) : null;
      await startForward({ origin, localPort, remoteHost, remotePort }, machine ? { machine, jump } : null);
      this.localInput.value = "";
      this.rportInput.value = "";
      await this.reload();
    } catch (e) {
      showActionFailureToast(copyText("portForward.onStart.failed"), String(e));
    }
  }

  private async onStop(id: string): Promise<void> {
    try {
      await stopForward(id);
      await this.reload();
    } catch (e) {
      showActionFailureToast(copyText("portForward.onStop.failed"), String(e));
    }
  }
}

let singleton: PortForwardPanel | null = null;
/** 打开端口转发管理台(单例 overlay)。 */
export function openPortForwardPanel(): void {
  singleton ??= new PortForwardPanel();
  void singleton.open();
}
