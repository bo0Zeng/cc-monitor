/**
 * 〔FE1 · 第四波 4D〕**账号偏好**：本机 config.json 里 `accounts` 那一段 —— 「我这台 cc-monitor 起新会话默认用哪个号」
 * （`defaultName`）与「每个号默认用哪个模型」（`modelByAccount`）。本机、不跨机器同步。
 *
 * 从 `accounts.ts` 拆出来（审计 B §6 必须拆 4；守的要求 `设计/01 §5` D1）。⚠ 写口今天仍是「读整份 → 改一个键 → 整份写回」；
 * 按键补丁的单一写口归 CFG1（`config.json` 丢更新），本文件只挪住址、不改写法。
 */
import { loadConfig, saveConfig } from "./config";
import { isValidModelName } from "./shell-quote";

const CFG_KEY = "accounts";

/** 读本机默认账号名（config.json accounts.defaultName）。无则 null。 */
export async function getDefaultName(): Promise<string | null> {
  try {
    const cfg = (await loadConfig()) as Record<string, unknown>;
    const a = cfg[CFG_KEY];
    if (a && typeof a === "object") {
      const dn = (a as Record<string, unknown>).defaultName;
      if (typeof dn === "string" && dn) return dn;
    }
  } catch (e) {
    console.warn("getDefaultName failed:", e);
  }
  return null;
}

/** 写本机默认账号名。null = 清除（回退跟随 manifest）。枚举全字段写回，防静默丢失。 */
export async function setDefaultName(name: string | null): Promise<void> {
  const cfg = (await loadConfig()) as Record<string, unknown>;
  const prev =
    cfg[CFG_KEY] && typeof cfg[CFG_KEY] === "object"
      ? (cfg[CFG_KEY] as Record<string, unknown>)
      : {};
  cfg[CFG_KEY] = {
    ...prev,
    defaultName: name ?? undefined,
  };
  // undefined 键会被 serde_json 序列化时忽略——等效于删除
  if (name === null) delete (cfg[CFG_KEY] as Record<string, unknown>).defaultName;
  await saveConfig(cfg);
}

const MODEL_MAP_KEY = "modelByAccount";

/** F07：读某账号配置的默认模型偏好（config.json accounts.modelByAccount[name]）。无则 undefined。
 *  结构上是 `defaultName`（单值）的复数版——按账号名索引，同样是本机、不跨机器同步的偏好。 */
export async function getModelForAccount(name: string): Promise<string | undefined> {
  try {
    const cfg = (await loadConfig()) as Record<string, unknown>;
    const a = cfg[CFG_KEY];
    if (a && typeof a === "object") {
      const map = (a as Record<string, unknown>)[MODEL_MAP_KEY];
      if (map && typeof map === "object") {
        const v = (map as Record<string, unknown>)[name];
        if (typeof v === "string" && v) return v;
      }
    }
  } catch (e) {
    console.warn("getModelForAccount failed:", e);
  }
  return undefined;
}

/** 写某账号的模型偏好。`model === null` 清除该账号这一条（其余账号不受影响）。
 *
 *  Phase D 审计发现的阻塞项：校验必须在**写入点**做，不能只留给
 *  `MODEL_DIMENSION.apply()`（起会话时）——那样一个非法值一旦落盘，会让该账号**此后每一次**
 *  resume/新建/tmux resume 在 `buildLaunchPlan` 里统一 throw，用户只看到一堆"无法构造 resume
 *  命令"的 toast，且设置面板的输入框不会标出"当前值非法"，很难把两者联系起来。fail-closed：
 *  非法即 throw，调用方（UI）负责 catch 并提示，绝不静默落盘。 */
export async function setModelForAccount(name: string, model: string | null): Promise<void> {
  if (model && !isValidModelName(model)) {
    throw new Error(`非法模型名（拒绝保存）: ${JSON.stringify(model)}`);
  }
  const cfg = (await loadConfig()) as Record<string, unknown>;
  const prev =
    cfg[CFG_KEY] && typeof cfg[CFG_KEY] === "object"
      ? (cfg[CFG_KEY] as Record<string, unknown>)
      : {};
  const prevMap =
    prev[MODEL_MAP_KEY] && typeof prev[MODEL_MAP_KEY] === "object"
      ? (prev[MODEL_MAP_KEY] as Record<string, string>)
      : {};
  const nextMap = { ...prevMap };
  if (model) nextMap[name] = model;
  else delete nextMap[name];
  cfg[CFG_KEY] = { ...prev, [MODEL_MAP_KEY]: nextMap };
  await saveConfig(cfg);
}

// ------------------------------------------------------------ 带缓存的取数
