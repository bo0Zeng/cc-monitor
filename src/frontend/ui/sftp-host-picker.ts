/**
 * 「远端文件」入口（栏顶「更多去处 ⋯」那一项 · 命令面板）：按远端主机数分支 —— 0 台提示 / 1 台直开 / 多台先在一个小选单里选（终点是原生文件窗口，`file-window.ts`）。
 */
import { openFileWindow } from "./file-window";
import { readRemoteConfig, sftpEligibleHosts } from "./remote-config";
import { toast } from "./kit/toast";
import { copyText } from "./copy-table";
import { openMenu } from "./kit/menu";

/** 远端文件入口：0 台提示 / 1 台直开 / 多台选单（锚在 `anchor` 上）。 */
export async function openSftpFromTopbar(anchor: HTMLElement): Promise<void> {
  const cfg = await readRemoteConfig();
  const hosts = sftpEligibleHosts(cfg);
  if (hosts.length === 0) {
    // 这是引导提示不是失败 → info 级（非红色错误）。
    toast(
      copyText("main.sftp.noMachine"),
      copyText("main.sftp.noMachine"),
      { level: "info" },
    );
    return;
  }
  if (hosts.length === 1) {
    void openFileWindow(hosts[0].name);
    return;
  }
  // ≥2 台：选主机菜单（全产品那一个弹出菜单）。
  openMenu(
    { el: anchor, align: "end" },
    hosts.map((h) => ({ label: h.name, onClick: () => void openFileWindow(h.name) })),
    { label: copyText("detail.label.machine") },
  );
}
