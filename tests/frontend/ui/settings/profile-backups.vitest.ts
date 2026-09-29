/**
 * 住址：`设计/99 §1` V156（shell 方言知识只住后端 OS 适配层）＋ 主会话 09-28 裁 OSA 子步 4「拆 —— 数据位置页里 `$PROFILE` 备份那一格
 * 由界面经通道直接问本机后端」。`$PROFILE` 在哪由本机后端的方言答（`aliases-read`），目录里有没有备份问同一台的 `files-ls`；
 * 本机不说 PowerShell ⇒ 一发都不问。
 */
import { describe, it, expect, vi, beforeEach } from "vitest";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
const os = vi.hoisted(() => ({
  shell: "powershell" as "powershell" | "posix",
}));
vi.mock("../../../../src/frontend/ui/settings/machine-aliases", () => ({
  localShell: () => os.shell,
}));

import { invoke } from "@tauri-apps/api/core";
import GOLDEN from "../../../__fixtures__/aliases.golden.json";
import { findProfileBackupDirs } from "../../../../src/frontend/ui/settings/profile-backups";
import {
  chanArgsJson,
  chanReply,
  type ChanCallArgs,
} from "../../../test-support/chan-fake";

const invokeMock = invoke as unknown as ReturnType<typeof vi.fn>;
const W = "C:\\Users\\u\\Documents";
/** 那台后端 `aliases-read`（PowerShell）答的成品：形状照跨语言金样，候选换成四份 `$PROFILE`（两个目录各两份）。 */
function listing(): unknown {
  const one = GOLDEN.readReply.rcCandidates[0];
  const paths = [
    `${W}\\WindowsPowerShell\\Microsoft.PowerShell_profile.ps1`,
    `${W}\\WindowsPowerShell\\profile.ps1`,
    `${W}\\PowerShell\\Microsoft.PowerShell_profile.ps1`,
    `${W}\\PowerShell\\profile.ps1`,
  ];
  return {
    ...GOLDEN.readReply,
    rcCandidates: paths.map((path) => ({ ...one, path })),
  };
}

beforeEach(() => {
  invokeMock.mockReset();
  os.shell = "powershell";
});

describe("$PROFILE 备份：问本机后端", () => {
  it("候选由 aliases-read 答；每个目录问一次 files-ls（大小写不敏感去重）；只列有 `.ccm-backup-` 的", async () => {
    invokeMock.mockImplementation((_cmd: string, a: ChanCallArgs) => {
      if (a.op === "aliases-read") return Promise.resolve(chanReply(listing()));
      const dir = (chanArgsJson(a) as { path: string }).path;
      const entries = dir.endsWith("\\WindowsPowerShell")
        ? [
            { path: `${dir}\\profile.ps1`, kind: "file" },
            { path: `${dir}\\profile.ps1.ccm-backup-1-0`, kind: "file" },
          ]
        : [{ path: `${dir}\\profile.ps1`, kind: "file" }];
      return Promise.resolve(chanReply({ entries, truncated: false }));
    });
    expect(await findProfileBackupDirs()).toEqual([`${W}\\WindowsPowerShell`]);
    const calls = invokeMock.mock.calls.map((c) => c[1] as ChanCallArgs);
    expect(calls.map((a) => [a.origin, a.op, chanArgsJson(a)])).toEqual([
      ["<local>", "aliases-read", { shell: "powershell", rcPath: null }],
      ["<local>", "files-ls", { path: `${W}\\WindowsPowerShell` }],
      ["<local>", "files-ls", { path: `${W}\\PowerShell` }],
    ]);
  });

  it("目录列不出来 ⇒ 当没有备份（不抛）；应答缺 entries ⇒ 抛（契约对不上）", async () => {
    invokeMock.mockImplementation((_cmd: string, a: ChanCallArgs) =>
      a.op === "aliases-read"
        ? Promise.resolve(chanReply(listing()))
        : Promise.reject({ err: { Peer: { why: "refused" } }, body: [] }),
    );
    expect(await findProfileBackupDirs()).toEqual([]);
    invokeMock.mockImplementation((_cmd: string, a: ChanCallArgs) =>
      Promise.resolve(
        chanReply(a.op === "aliases-read" ? listing() : { truncated: false }),
      ),
    );
    await expect(findProfileBackupDirs()).rejects.toThrow();
  });

  it("本机不说 PowerShell ⇒ 一发都不问、没有这一格", async () => {
    os.shell = "posix";
    expect(await findProfileBackupDirs()).toEqual([]);
    expect(invokeMock).not.toHaveBeenCalled();
  });
});
