/**
 * 通用组件总览（`tests/shots/kit.html`，产品真组件 ＋ 合成数据；那一页自己读 `?scene=` 画，不起假后端）。
 */
import type { Scene } from "./index";
import { defaultWorld } from "../fake/world";

function kit(id: string, title: string, desc: string, height: number): Scene {
  return { id, page: "tests/shots/kit", dir: "通用组件", title, desc, width: 1280, height, world: defaultWorld, act: async () => {} };
}

export const KIT_SCENES: Scene[] = [
  kit("kit-overview", "通用组件 · 各态", "按钮 · 输入 · 开关 · 分栏 · chip · 卡片 · 列表行 · 折叠 · 徽标 · 状态点 · 进度 · 计量 · 错误条 · 空态 · 区块七态", 2150),
  kit("kit-dialog", "会打断什么", "重启切换之前按族列「中断 / 保留」，取消是默认焦点、动作键红", 640),
  kit("kit-dialog-danger", "撤不回的一批", "清单限高可滚、超过 8 项只列前 8 ＋「另外 n 个」、数量写在按钮上", 640),
  kit("kit-dialog-text", "填值框", "错误写在框里、不关", 520),
  kit("kit-detail", "复制详情 · 各态", "［复制详情］默认 · 已复制 · 复制不了就地展开全选 · 窄只剩图标 · 没有详情不出；错误条 · 窄错误条 · 行内", 900),
  kit("kit-detail-dialog", "复制详情 · 对话框", "表单对话框提交没成：框不关，错误条带［复制详情］", 560),
  kit("kit-detail-toast", "复制详情 · toast", "带详情的出错 toast：动作排第二行（修法在前、复制详情在后）· 合流 ×N · 没有详情的（撤销提示条）版式不变", 640),
  kit("kit-detail-messages", "复制详情 · 「消息」", "带详情的那条：［详情］展开最近一段 ＋「另 n 段」· 同一颗［复制详情］复制出全部段", 820),
  kit("kit-float", "浮层", "弹出菜单（子菜单 · 不可选 · 危险项）· 悬停提示 · toast 四种 ＋ 合流 ×N ＋ 撤销 · 抽屉", 760),
];
