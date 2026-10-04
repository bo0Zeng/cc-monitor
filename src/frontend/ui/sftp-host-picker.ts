/**
 * 顶栏「远端文件」入口：按远端主机数分支 —— 0 台提示 / 1 台直开 / 多台先在一个小选单里选（终点是原生文件窗口，`file-window.ts`）。
 */
import { openFileWindow } from "./file-window";
import { readRemoteConfig, sftpEligibleHosts } from "./remote-config";
import { showActionFailureToast } from "./error-toast";
import { copyText } from "./copy-table";
import { armPopupDismiss } from "./popup-dismiss";

/** 多台远端时的选主机浮层（body-level，单例）与它的关法（`popup-dismiss.ts`）。 */
let sftpHostPicker: HTMLElement | null = null;
let disarm: (() => void) | null = null;

export function closeSftpHostPicker(): void {
  disarm?.();
  disarm = null;
  if (sftpHostPicker) {
    sftpHostPicker.remove();
    sftpHostPicker = null;
  }
}

/** 顶栏按钮点下去：选单开着 ⇒ 收起；没开 ⇒ 照台数分支。 */
export function toggleSftpFromTopbar(anchor: HTMLElement): Promise<void> {
  if (sftpHostPicker) {
    closeSftpHostPicker();
    return Promise.resolve();
  }
  return openSftpFromTopbar(anchor);
}

/** 顶栏 SFTP 入口点击：0 台提示 / 1 台直开 / 多台选单。 */
export async function openSftpFromTopbar(anchor: HTMLElement): Promise<void> {
  const cfg = await readRemoteConfig();
  const hosts = sftpEligibleHosts(cfg);
  if (hosts.length === 0) {
    // 这是引导提示不是失败 → info 级（非红色错误）。
    showActionFailureToast(
      copyText("main.sftp.noMachine"),
      copyText("main.sftp.noMachineHint"),
      { level: "info" },
    );
    return;
  }
  if (hosts.length === 1) {
    void openFileWindow(hosts[0]);
    return;
  }
  // ≥2 台：选主机浮层（body-level fixed；点外面 · 再点按钮 · Esc 关）。
  closeSftpHostPicker();
  const menu = document.createElement("div");
  menu.className = "sftp-host-picker";
  for (const h of hosts) {
    const item = document.createElement("button");
    item.type = "button";
    item.className = "sftp-host-picker-item";
    item.textContent = h.label || h.host;
    item.addEventListener("click", () => {
      closeSftpHostPicker();
      void openFileWindow(h);
    });
    menu.appendChild(item);
  }
  const r = anchor.getBoundingClientRect();
  menu.style.top = `${r.bottom + 4}px`;
  menu.style.right = `${Math.max(4, window.innerWidth - r.right)}px`;
  document.body.appendChild(menu);
  sftpHostPicker = menu;
  disarm = armPopupDismiss(menu, anchor, closeSftpHostPicker);
}
