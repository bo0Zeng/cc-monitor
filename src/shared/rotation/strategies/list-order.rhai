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

// 所有号同分：完全照号列表的先后（后端在分一样时按列表排）。
fn score(ctx) {
  #{ scores: #{} }
}
