/**
 * 把这一版换到那台（远端后端部署）的那一处：机器卡问题行［更新］与主窗口 ↗ 浮层［更新］都调它。
 * 判：先问会打断什么，有才弹框，取消 ⇒ 不部署、回 `null`；确认 / 没什么会断 ⇒ 部署、回那句人读结果；
 * 按 origin 找那台的配置（设置里没有这台 ⇒ 抛，不拿空配置去部署）。
 * 本机：口上是上一版那一形的［更新］＝ 同一个框问过之后先停（壳拿这一版的落点停口上那一位）再起这一版，不部署、不查远端配置。
 */
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { Host } from "../../../src/frontend/ui/generated/Host";

vi.mock("../../../src/frontend/ui/settings/interrupts", () => ({
  askInterrupts: vi.fn(),
  interruptRows: vi.fn(),
}));
vi.mock("../../../src/frontend/ui/kit/dialog", () => ({ confirmDialog: vi.fn() }));
vi.mock("../../../src/frontend/ui/ipc/commands", () => ({
  commands: { deploy_remote_backend: vi.fn(), backend_stop: vi.fn(), backend_start: vi.fn() },
}));
vi.mock("../../../src/frontend/ui/remote-config", async (orig) => {
  const real = await orig<typeof import("../../../src/frontend/ui/remote-config")>();
  return { ...real, resolveRemoteConfigByOrigin: vi.fn() };
});

import { updateBackend, updateBackendOf } from "../../../src/frontend/ui/backend-deploy";
import { askInterrupts, interruptRows } from "../../../src/frontend/ui/settings/interrupts";
import { confirmDialog } from "../../../src/frontend/ui/kit/dialog";
import { commands } from "../../../src/frontend/ui/ipc/commands";
import { resolveRemoteConfigByOrigin } from "../../../src/frontend/ui/remote-config";
import { LOCAL } from "../../../src/frontend/ui/ipc/origin";

const CFG = { host: "192.0.2.9", user: "u", label: "devbox" } as unknown as Host;
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
    expect(await updateBackend("devbox", "devbox", started)).toBe("已部署");
    expect(vi.mocked(askInterrupts).mock.calls).toEqual([["devbox"]]);
    expect(confirmDialog).not.toHaveBeenCalled();
    expect(vi.mocked(commands.deploy_remote_backend).mock.calls).toEqual([[{ origin: "devbox" }]]);
    expect(started).toHaveBeenCalledTimes(1);
  });

  it("有会断的 ⇒ 先弹框；取消 ⇒ 不部署、不叫 onStart，回 null；确认 ⇒ 部署", async () => {
    vi.mocked(interruptRows).mockReturnValue([ROW]);
    vi.mocked(confirmDialog).mockResolvedValueOnce(false);
    const started = vi.fn();
    expect(await updateBackend("devbox", "devbox", started)).toBeNull();
    expect(commands.deploy_remote_backend).not.toHaveBeenCalled();
    expect(started).not.toHaveBeenCalled();
    vi.mocked(confirmDialog).mockResolvedValueOnce(true);
    expect(await updateBackend("devbox", "devbox")).toBe("已部署");
    expect(vi.mocked(confirmDialog).mock.calls[1][0].rows).toEqual([ROW]);
  });

  it("部署失败 ⇒ 原样抛给调用方（各自在原处说）", async () => {
    vi.mocked(interruptRows).mockReturnValue([]);
    vi.mocked(commands.deploy_remote_backend).mockRejectedValue(new Error("上传失败"));
    await expect(updateBackend("devbox", "devbox")).rejects.toThrow("上传失败");
  });

  it("按 origin：找得到 ⇒ 用那台的配置部署；设置里没有这台 ⇒ 抛，不部署", async () => {
    vi.mocked(interruptRows).mockReturnValue([]);
    vi.mocked(resolveRemoteConfigByOrigin).mockResolvedValueOnce({ name: "devbox", host: CFG });
    expect(await updateBackendOf("devbox", "devbox")).toBe("已部署");
    expect(vi.mocked(commands.deploy_remote_backend).mock.calls).toEqual([[{ origin: "devbox" }]]);
    vi.mocked(resolveRemoteConfigByOrigin).mockResolvedValueOnce(null);
    await expect(updateBackendOf("gone", "gone")).rejects.toThrow();
    expect(commands.deploy_remote_backend).toHaveBeenCalledTimes(1);
  });
});

describe("backend-deploy：本机口上是上一版 ⇒ 停掉它、起这一版", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.mocked(askInterrupts).mockResolvedValue(null as never);
    vi.mocked(commands.backend_stop).mockResolvedValue({ stopped: "graceful", pid: 4242 });
    vi.mocked(commands.backend_start).mockResolvedValue(undefined as never);
  });

  it("先问会打断什么（本机通道断着 ⇒ 数量不明那几行），确认后先停再起，不走远端部署、不查远端配置", async () => {
    vi.mocked(interruptRows).mockReturnValue([ROW]);
    vi.mocked(confirmDialog).mockResolvedValueOnce(true);
    const order: string[] = [];
    vi.mocked(commands.backend_stop).mockImplementation(async (a) => {
      order.push(`stop ${a.origin}`);
      return { stopped: "graceful", pid: 4242 };
    });
    vi.mocked(commands.backend_start).mockImplementation(async (a) => {
      order.push(`start ${a.origin}`);
    });
    const started = vi.fn();
    expect(await updateBackendOf(LOCAL, "this-box", started)).toBe("");
    expect(vi.mocked(askInterrupts).mock.calls).toEqual([[LOCAL]]);
    expect(vi.mocked(interruptRows).mock.calls).toEqual([[null, "this-box", "update"]]);
    expect(vi.mocked(confirmDialog).mock.calls[0][0].rows).toEqual([ROW]);
    expect(order).toEqual([`stop ${LOCAL}`, `start ${LOCAL}`]);
    expect(started).toHaveBeenCalledTimes(1);
    expect(commands.deploy_remote_backend).not.toHaveBeenCalled();
    expect(resolveRemoteConfigByOrigin).not.toHaveBeenCalled();
  });

  it("取消 ⇒ 不停不起、回 null；停没停成 ⇒ 原样抛、不去起", async () => {
    vi.mocked(interruptRows).mockReturnValue([ROW]);
    vi.mocked(confirmDialog).mockResolvedValueOnce(false);
    expect(await updateBackend(LOCAL, "this-box")).toBeNull();
    expect(commands.backend_stop).not.toHaveBeenCalled();
    expect(commands.backend_start).not.toHaveBeenCalled();
    vi.mocked(confirmDialog).mockResolvedValueOnce(true);
    vi.mocked(commands.backend_stop).mockRejectedValueOnce(new Error("没停掉"));
    await expect(updateBackend(LOCAL, "this-box")).rejects.toThrow("没停掉");
    expect(commands.backend_start).not.toHaveBeenCalled();
  });
});
