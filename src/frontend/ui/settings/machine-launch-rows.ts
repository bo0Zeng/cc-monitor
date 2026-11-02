/**
 * 每台「这台上的 cc-monitor」里「怎么起会话」那几格（本机远端同一个组件）：恢复会话的启动程序 · 恢复时放在哪 ·
 * 子会话 · 分身会话默认进不进标签栏（［显示｜隐藏］分段按钮 ⇒ `prefs-set {childTab | bgTab}`；判哪条会话隐藏在那台后端，这里只改那台的偏好）。
 * 值住**那台**后端（`prefs.json`，经 `machine-prefs.ts` 的 `prefs-read` / `prefs-set`），界面不存副本：画之前问那台、改了交那台、画写完读回的那一份。
 *
 * - 启动程序：下拉（`ResumeSelect`；空 ＝ 那一家自己的默认启动器）⇒ `prefs-set {launchers: {<默认那一家>: 值}}`；用过的记进界面那份预设
 *   （只是输入历史）；越层启动器那条诊断摆在行下。
 * - 恢复时放在哪：［托管｜tmux｜照起时那一种］分段按钮 ⇒ `prefs-set {resumeWhere}`（读回之前灰着、一段不选）。
 * - 新会话的权限档：下拉（首项「默认」＝ 删那一条；选项是那台 `session-open-facts` 里默认那一家的权限档）⇒
 *   `prefs-set {permissionModes: {<默认那一家>: 值 | null}}`；那一家没有权限档 ⇒ 这一行不画。
 * - 读不懂（那台 `state: "unreadable"`）⇒ 两格禁用、行下摆那台写好的那一句（那台也拒写，不拿空的盖原文）；问不到 ⇒ 禁用、说读不到。
 * - 存成了 ⇒ 广播「设置改过了」（主窗口忘掉记着的「运行于」默认、下次重问）；存失败 ⇒ 退回读回的那份、行下一句。
 */
import { emit } from "@tauri-apps/api/event";
import { ccRow } from "./cc-row";
import { copyText } from "../copy-table";
import { rememberResumePreset, getBehavior } from "../behavior";
import { DEFAULT_AGENT } from "../agent-profile";
import { diagnoseRemoteLauncher } from "../launcher-diagnostics";
import { readPrefs, setPrefs, type PrefsReply } from "../machine-prefs";
import { askFacts } from "../new-session-reads";
import { select } from "../kit/select";
import type { Prefs } from "../generated/Prefs";
import { ResumeSelect } from "./resume-select";
import type { ResumeWhere } from "../generated/ResumeWhere";
import { segmented } from "../kit/tabs";
import { attachTooltip } from "../kit/tooltip";
import { setPlace } from "../session-hidden-cmds";
import { placesOf, type TabPlace, type TabPlaces } from "../session-hidden";
import { detailOf, sayFailure, sayWithDetail } from "../kit/detail";
import { SETTINGS_APPLIED_EVENT } from "./events";
import type { Machine } from "../ipc/origin";

function applied(): void {
  emit(SETTINGS_APPLIED_EVENT).catch((e: unknown) =>
    console.warn("[machine-launch] 通知不到主窗口：", e),
  );
}

/**
 * 子会话 · 分身会话那两行：名字短、说明在悬停；分段按钮读回那台之前灰着、一段都不选（缺省只住那台后端，这里不存一份），
 * 改了交那台、画写完读回的那一份，失败退回、行下一句。回两行与「读回了」那一下（读不懂 ⇒ 照读回的画、仍灰着：那台也拒写）。
 */
function placeRows(origin: Machine): { rows: HTMLElement[]; fill: (prefs: Prefs, writable: boolean) => void } {
  const specs: { which: keyof TabPlaces; label: string; tip: string; role: string }[] = [
    { which: "childTab", label: copyText("settings.machine.childTab"), tip: copyText("settings.machine.childTabTip"), role: "child-tab" },
    { which: "bgTab", label: copyText("settings.machine.bgTab"), tip: copyText("settings.machine.bgTabTip"), role: "bg-tab" },
  ];
  let places: TabPlaces | null = null;
  let writable = false;
  const paints: (() => void)[] = [];
  const rows = specs.map((sp) => {
    const err = document.createElement("div");
    err.className = "settings-row-error";
    err.hidden = true;
    const help = document.createElement("div");
    help.append(err);
    const slot = document.createElement("div");
    const paint = (): void => {
      const seg = segmented<TabPlace>({
        items: [
          { key: "show", label: copyText("settings.machine.tabShow") },
          { key: "hide", label: copyText("settings.machine.tabHide") },
        ],
        current: places?.[sp.which] ?? null,
        label: sp.label,
        onChange: (to) => {
          if (!writable || places === null) return paint();
          err.hidden = true;
          setPlace(origin, sp.which, to).then(
            (p) => {
              places = p;
              paints.forEach((f) => f());
              applied();
            },
            (e: unknown) => {
              paint();
              sayWithDetail(err, copyText("settings.behavior.saveFailedLine", { why: String(e) }), detailOf(e));
              err.hidden = false;
            },
          );
        },
      });
      seg.setAttribute("aria-disabled", String(!writable));
      slot.replaceChildren(seg);
    };
    paints.push(paint);
    paint();
    const row = ccRow(sp.label, help, [slot]);
    row.dataset.role = sp.role;
    const label = row.querySelector<HTMLElement>(".machine-cc-label");
    if (label) attachTooltip(label, sp.tip);
    return row;
  });
  return {
    rows,
    fill: (prefs, w) => {
      places = placesOf(prefs);
      writable = w;
      paints.forEach((f) => f());
    },
  };
}

/** 那台那几格（`name` ＝ 那台在界面上的名字，说明里用）。建好不问；`load()` 才问那台（机器页第一次可见时宿主调）。 */
export function machineLaunchRows(
  origin: Machine,
  name: string,
): { rows: HTMLElement[]; load: () => void } {
  // ── 启动程序 ──
  const help = document.createElement("div");
  help.textContent = copyText("machineCard.field.resumeCmdScope", {
    machine: name,
  });
  const warn = document.createElement("div");
  warn.className = "settings-launcher-warning";
  warn.hidden = true;
  const err = document.createElement("div");
  err.className = "settings-row-error";
  err.hidden = true;
  help.append(warn, err);
  let saved = "";
  let presets: string[] = [];
  const paintWarn = (cmd: string): void => {
    const msg = diagnoseRemoteLauncher(cmd);
    warn.textContent = msg ?? "";
    warn.hidden = msg === null;
  };
  const sel = new ResumeSelect({
    onChange: (next) => {
      err.hidden = true;
      paintWarn(next);
      setPrefs(origin, { launchers: { [DEFAULT_AGENT]: next } }).then(
        (r) => {
          saved = r.prefs.launchers[DEFAULT_AGENT] ?? "";
          applied();
          if (next === "") return;
          return rememberResumePreset(next).then(
            (p) => {
              presets = p;
              sel.set(saved, presets);
            },
            (e: unknown) =>
              console.warn("[machine-launch] 用过的那几条没记上：", e),
          );
        },
        (e: unknown) => {
          sel.set(saved, presets);
          paintWarn(saved);
          sayWithDetail(
            err,
            copyText("settings.behavior.saveFailedLine", { why: String(e) }),
            detailOf(e),
          );
          err.hidden = false;
        },
      );
    },
  });
  sel.disabled = true;
  sel.element.dataset.role = "machine-resume";
  const launchRow = ccRow(copyText("machineCard.field.resumeCmd"), help, [
    sel.element,
  ]);
  launchRow.dataset.col = "resume";

  // ── 恢复时放在哪（三段；读回之前灰着、一段不选）──
  const whereErr = document.createElement("div");
  whereErr.className = "settings-row-error";
  whereErr.hidden = true;
  const whereHelp = document.createElement("div");
  whereHelp.append(whereErr);
  const whereSlot = document.createElement("div");
  let where: ResumeWhere | null = null;
  let whereWritable = false;
  const paintWhere = (): void => {
    const seg = segmented<ResumeWhere>({
      items: [
        { key: "managed", label: copyText("settings.machine.resumeManaged") },
        { key: "tmux", label: copyText("settings.machine.resumeTmux") },
        { key: "as_started", label: copyText("settings.machine.resumeAsStarted") },
      ],
      current: where,
      label: copyText("settings.machine.resumeWhere"),
      onChange: (to) => {
        if (!whereWritable || where === null) return paintWhere();
        whereErr.hidden = true;
        setPrefs(origin, { resumeWhere: to }).then(
          (r) => {
            where = r.prefs.resumeWhere;
            paintWhere();
            applied();
          },
          (e: unknown) => {
            paintWhere();
            sayWithDetail(whereErr, copyText("settings.behavior.saveFailedLine", { why: String(e) }), detailOf(e));
            whereErr.hidden = false;
          },
        );
      },
    });
    seg.setAttribute("aria-disabled", String(!whereWritable));
    whereSlot.replaceChildren(seg);
  };
  paintWhere();
  const whereRow = ccRow(copyText("settings.machine.resumeWhere"), whereHelp, [whereSlot]);
  whereRow.dataset.role = "resume-where";
  const whereLabel = whereRow.querySelector<HTMLElement>(".machine-cc-label");
  if (whereLabel) attachTooltip(whereLabel, copyText("settings.machine.resumeWhereTip"));
  const fillWhere = (r: PrefsReply): void => {
    where = r.prefs.resumeWhere;
    whereWritable = r.state !== "unreadable";
    paintWhere();
  };

  // ── 新会话的权限档（默认那一家）──
  const permErr = document.createElement("div");
  permErr.className = "settings-row-error";
  permErr.hidden = true;
  const permHelp = document.createElement("div");
  permHelp.textContent = copyText("settings.machine.permissionModeHelp");
  permHelp.append(permErr);
  let permSaved = "";
  const permSel = select({
    label: copyText("settings.machine.permissionMode"),
    options: [],
    onChange: (v) => {
      permErr.hidden = true;
      setPrefs(origin, { permissionModes: { [DEFAULT_AGENT]: v === "" ? null : v } }).then(
        (r) => {
          permSaved = r.prefs.permissionModes[DEFAULT_AGENT] ?? "";
          permSel.setValue(permSaved);
          applied();
        },
        (e: unknown) => {
          permSel.setValue(permSaved);
          sayWithDetail(permErr, copyText("settings.behavior.saveFailedLine", { why: String(e) }), detailOf(e));
          permErr.hidden = false;
        },
      );
    },
  });
  permSel.setDisabled(true);
  const permRow = ccRow(copyText("settings.machine.permissionMode"), permHelp, [permSel.el]);
  permRow.dataset.role = "permission-mode";
  permRow.hidden = true;
  const fillPerm = (r: PrefsReply, modes: { value: string; text: string }[]): void => {
    permRow.hidden = modes.length === 0;
    permSaved = r.prefs.permissionModes[DEFAULT_AGENT] ?? "";
    permSel.setOptions(
      [{ value: "", label: copyText("resumeSelect.option.defaultBare") }, ...modes.map((m) => ({ value: m.value, label: m.text }))],
      permSaved,
    );
    permSel.setDisabled(r.state === "unreadable");
  };

  // ── 子会话 · 分身会话进不进标签栏 ──
  const place = placeRows(origin);

  // ── 问那台 ──
  const fill = (r: PrefsReply, p: string[]): void => {
    place.fill(r.prefs, r.state !== "unreadable");
    presets = p;
    saved = r.prefs.launchers[DEFAULT_AGENT] ?? "";
    sel.set(saved, presets);
    paintWarn(saved);
    fillWhere(r);
    if (r.state === "unreadable") {
      sayWithDetail(
        err,
        r.reason ?? copyText("machinePrefs.read.failed"),
        r.detail ?? "",
      );
      err.hidden = false;
      return;
    }
    sel.disabled = false;
  };
  const load = (): void => {
    // 一条链里问：前一样同步抛了，后一样不会被白白发出去、没人接。
    void Promise.resolve()
      .then(() => {
        const p = getBehavior().then(
          (b) => b.resumeCommandPresets,
          () => [] as string[],
        );
        // 权限档那一行的选项：那台能起的默认那一家声明的档（问不到 ⇒ 这一行不画）。
        const modes = askFacts(origin, null).then(
          (f) => f.agents.find((a) => a.kind === DEFAULT_AGENT)?.permissionModes ?? [],
          () => [],
        );
        return Promise.all([readPrefs(origin), p, modes]);
      })
      .then(
        ([r, p, modes]) => {
          fill(r, p);
          fillPerm(r, modes);
        },
        (e: unknown) => {
          sayFailure(err, copyText("machinePrefs.read.failed"), e);
          err.hidden = false;
        },
      );
  };
  return { rows: [launchRow, whereRow, permRow, ...place.rows], load };
}
