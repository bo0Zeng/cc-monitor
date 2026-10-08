/**
 * 把这一版换到那台（远端后端部署）的那一处：机器卡问题行［更新］与主窗口 ↗ 浮层［更新］都调它。
 * 判：先问会打断什么，有才弹框，取消 ⇒ 不部署、回 `null`；确认 / 没什么会断 ⇒ 部署、回那句人读结果；
 * 按 origin 找那台的配置（设置里没有这台 ⇒ 抛，不拿空配置去部署）。
 */
import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("../../../src/frontend/ui/settings/interrupts", () => ({
  askInterrupts: vi.fn(),
  interruptRows: vi.fn(),
}));
vi.mock("../../../src/frontend/ui/kit/dialog", () => ({ confirmDialog: vi.fn() }));
vi.mock("../../../src/frontend/ui/ipc/commands", () => ({ commands: { deploy_remote_backend: vi.fn() } }));
vi.mock("../../../src/frontend/ui/remote-config", async (orig) => {
  const real = await orig<typeof import("../../../src/frontend/ui/remote-config")>();
  return { ...real, resolveRemoteConfigByOrigin: vi.fn() };
});

import { updateBackend, updateBackendOf } from "../../../src/frontend/ui/backend-deploy";
import { askInterrupts, interruptRows } from "../../../src/frontend/ui/settings/interrupts";
import { confirmDialog } from "../../../src/frontend/ui/kit/dialog";
import { commands } from "../../../src/frontend/ui/ipc/commands";
import { resolveRemoteConfigByOrigin, type RemoteHostConfig } from "../../../src/frontend/ui/remote-config";

const CFG = { host: "192.0.2.9", user: "u", label: "devbox" } as unknown as RemoteHostConfig;
const ROW = { text: "1 个会话会断" } as never;

describe("backend-deploy：把这一版换到那台", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.mocked(askInterrupts).mockResolvedValue({} as never);
    vi.mocked(commands.deploy_remote_backend).mockResolvedValue("已部署");
  });

  it("什么都不会断 ⇒ 不弹框、直接部署，回那句结果；开工前叫 onStart", async () => {
    vi.mocked(interruptRows).mockReturnValue([]);
    const started = vi.fn();
    expect(await updateBackend(CFG, "devbox", "devbox", started)).toBe("已部署");
    expect(vi.mocked(askInterrupts).mock.calls).toEqual([["devbox"]]);
    expect(confirmDialog).not.toHaveBeenCalled();
    expect(vi.mocked(commands.deploy_remote_backend).mock.calls).toEqual([[{ cfg: CFG }]]);
    expect(started).toHaveBeenCalledTimes(1);
  });

  it("有会断的 ⇒ 先弹框；取消 ⇒ 不部署、不叫 onStart，回 null；确认 ⇒ 部署", async () => {
    vi.mocked(interruptRows).mockReturnValue([ROW]);
    vi.mocked(confirmDialog).mockResolvedValueOnce(false);
    const started = vi.fn();
    expect(await updateBackend(CFG, "devbox", "devbox", started)).toBeNull();
    expect(commands.deploy_remote_backend).not.toHaveBeenCalled();
    expect(started).not.toHaveBeenCalled();
    vi.mocked(confirmDialog).mockResolvedValueOnce(true);
    expect(await updateBackend(CFG, "devbox", "devbox")).toBe("已部署");
    expect(vi.mocked(confirmDialog).mock.calls[1][0].rows).toEqual([ROW]);
  });

  it("部署失败 ⇒ 原样抛给调用方（各自在原处说）", async () => {
    vi.mocked(interruptRows).mockReturnValue([]);
    vi.mocked(commands.deploy_remote_backend).mockRejectedValue(new Error("上传失败"));
    await expect(updateBackend(CFG, "devbox", "devbox")).rejects.toThrow("上传失败");
  });

  it("按 origin：找得到 ⇒ 用那台的配置部署；设置里没有这台 ⇒ 抛，不部署", async () => {
    vi.mocked(interruptRows).mockReturnValue([]);
    vi.mocked(resolveRemoteConfigByOrigin).mockResolvedValueOnce(CFG);
    expect(await updateBackendOf("devbox", "devbox")).toBe("已部署");
    expect(vi.mocked(commands.deploy_remote_backend).mock.calls).toEqual([[{ cfg: CFG }]]);
    vi.mocked(resolveRemoteConfigByOrigin).mockResolvedValueOnce(null);
    await expect(updateBackendOf("gone", "gone")).rejects.toThrow();
    expect(commands.deploy_remote_backend).toHaveBeenCalledTimes(1);
  });
});
