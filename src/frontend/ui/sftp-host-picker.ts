/**
 * 顶栏「远端文件」入口：按远端主机数分支 —— 0 台提示 / 1 台直开 / 多台先在一个小选单里选（终点是原生文件窗口，`file-window.ts`）。
 */
import { openFileWindow } from "./file-window";
import { readRemoteConfig, sftpEligibleHosts } from "./remote-config";
import { toast } from "./kit/toast";
import { copyText } from "./copy-table";
import { closeMenu, menuAnchoredOn, openMenu } from "./kit/menu";

/** 选主机那个菜单开着时再点顶栏按钮 ⇒ 收起；没开 ⇒ 照台数分支。 */
export function toggleSftpFromTopbar(anchor: HTMLElement): Promise<void> {
  if (menuAnchoredOn(anchor)) {
    closeMenu();
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
    toast(
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
  // ≥2 台：选主机菜单（全产品那一个弹出菜单）。
  openMenu(
    { el: anchor, align: "end" },
    hosts.map((h) => ({ label: h.label || h.host, onClick: () => void openFileWindow(h) })),
    { label: copyText("main.sftp.pickHost") },
  );
}
