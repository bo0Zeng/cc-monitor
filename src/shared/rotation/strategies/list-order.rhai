/*--- rotation
{
  "format": 5,
  "name": "照列表先后",
  "line": "照号列表的先后，前一个用不了才换下一个",
  "settings": {
    "caps": {},
    "capKind": "soft",
    "accounts": [
      {"start": true}
    ]
  }
}
---*/

// 所有号同分：照号列表的先后。上限 · 软硬 · 启用延迟 · 回切照设置（积木库里那一判）。
fn decide(ctx) {
  lib::decide_with(ctx, #{})
}
