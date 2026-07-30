/**
 * F58：本地端口转发(-L)管理台。overlay 面板(照 SFTP panel 范式,body-level fixed)——
 * 列当前转发 + 加转发表单(选主机/本地端口/远端 host:port)+ 启停 + 刷新。消费后端
 * start_forward/stop_forward/list_forwards;转发经 cc-monitor 已有 SSH 连接隧道(复用连接大脑)。
 */
import { commands } from "../ipc/commands";
import { showActionFailureToast } from "../error-toast";
import { readRemoteConfig } from "../remote-config";

/** 后端 ForwardStatus（camelCase）。 */
// C04d 批 3：改用生成物（源 `port_forward.rs`）。手写版与它**逐字等价**
// ——本批次这一处零漂移，价值是防将来漂，不是抓到了 bug。
// `connCount` 走 C03 大整数策略：Rust 是 `u64`，按**累计连接数**量纲算
// 2^53-1 条（每秒 1000 连接要 28.5 万年）⇒ `number` 够用。
import type { ForwardStatus } from "../generated/ForwardStatus";

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
    title.textContent = "端口转发（-L 本地转发）";
    header.appendChild(title);
    header.appendChild(mkBtn("刷新", () => void this.reload()));
    const close = mkBtn("关闭", () => this.close());
    close.classList.add("pf-close");
    header.appendChild(close);
    panel.appendChild(header);

    const form = document.createElement("div");
    form.className = "pf-form";
    this.originSel = document.createElement("select");
    this.originSel.className = "pf-input";
    form.appendChild(this.originSel);
    this.localInput = mkInput("本地端口 (如 15432)");
    form.appendChild(this.localInput);
    const arrow = document.createElement("span");
    arrow.className = "pf-arrow";
    arrow.textContent = "→";
    form.appendChild(arrow);
    this.rhostInput = mkInput("远端 host (如 localhost)");
    this.rhostInput.value = "localhost";
    form.appendChild(this.rhostInput);
    this.rportInput = mkInput("远端端口 (如 5432)");
    form.appendChild(this.rportInput);
    const startBtn = mkBtn("启动", () => void this.onStart());
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
      forwards = await commands.list_forwards();
    } catch (e) {
      showActionFailureToast("列转发失败", String(e));
    }
    this.listEl.innerHTML = "";
    if (forwards.length === 0) {
      const empty = document.createElement("div");
      empty.className = "pf-empty";
      empty.textContent = "(暂无转发。填上方表单启动一条。)";
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
      desc.textContent = `[${f.origin}] 127.0.0.1:${f.localPort} → ${f.remoteHost}:${f.remotePort} · ${f.connCount} 连接`;
      row.appendChild(desc);
      row.appendChild(mkBtn("停止", () => void this.onStop(f.id)));
      this.listEl.appendChild(row);
    }
  }

  private async onStart(): Promise<void> {
    const origin = this.originSel.value;
    const localPort = Number.parseInt(this.localInput.value, 10);
    const remoteHost = this.rhostInput.value.trim();
    const remotePort = Number.parseInt(this.rportInput.value, 10);
    if (!origin) {
      showActionFailureToast("启动转发", "请先在设置里配置一台远端主机。");
      return;
    }
    const validPort = (p: number): boolean => Number.isInteger(p) && p > 0 && p <= 65535;
    if (!validPort(localPort)) {
      showActionFailureToast("启动转发", "本地端口非法（1–65535）。");
      return;
    }
    if (!remoteHost) {
      showActionFailureToast("启动转发", "远端 host 不能为空。");
      return;
    }
    if (!validPort(remotePort)) {
      showActionFailureToast("启动转发", "远端端口非法（1–65535）。");
      return;
    }
    try {
      await commands.start_forward({ spec: { origin, localPort, remoteHost, remotePort } });
      this.localInput.value = "";
      this.rportInput.value = "";
      await this.reload();
    } catch (e) {
      showActionFailureToast("启动转发失败", String(e));
    }
  }

  private async onStop(id: string): Promise<void> {
    try {
      await commands.stop_forward({ id });
      await this.reload();
    } catch (e) {
      showActionFailureToast("停止转发失败", String(e));
    }
  }
}

let singleton: PortForwardPanel | null = null;
/** 打开端口转发管理台(单例 overlay)。 */
export function openPortForwardPanel(): void {
  singleton ??= new PortForwardPanel();
  void singleton.open();
}
