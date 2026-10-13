/*--- rotation
{
  "format": 5,
  "name": "先用剩得多的",
  "line": "先用余量最大的号",
  "settings": {
    "caps": {},
    "capKind": "soft",
    "margin": 10,
    "accounts": [
      {
        "start": true
      }
    ]
  },
  "form": [
    {
      "key": "caps",
      "kind": "daySegments",
      "scope": "each",
      "inherit": true,
      "label": {
        "zh": "用量上限",
        "en": "Usage limit"
      },
      "help": {
        "zh": "一天里各时段用到多少就换下一个号；没另设的时段用满为止",
        "en": "How far to use each window in each part of the day before moving on"
      },
      "windows": [
        "5h",
        "7d"
      ],
      "min": 0,
      "max": 100,
      "unit": "percent",
      "default": 100
    },
    {
      "key": "capKind",
      "kind": "choice",
      "scope": "each",
      "inherit": true,
      "label": {
        "zh": "上限类型",
        "en": "Limit type"
      },
      "choices": [
        {
          "v": "soft",
          "label": {
            "zh": "软上限：所有号都到上限以后还能接着用",
            "en": "Soft: may go past it once every account is at its limit"
          }
        },
        {
          "v": "hard",
          "label": {
            "zh": "硬上限：任何时候都不超过",
            "en": "Hard: never past it"
          }
        }
      ],
      "default": "soft"
    },
    {
      "key": "delay",
      "kind": "minutes",
      "scope": "each",
      "label": {
        "zh": "启用延迟",
        "en": "Start delay"
      },
      "help": {
        "zh": "排在前面的号全部用不了满这么多分钟后，才换到这个号",
        "en": "Minutes every account ahead must stay unusable before this one is used"
      },
      "min": 0,
      "max": 1440,
      "unit": "minutes",
      "default": 0
    },
    {
      "key": "back",
      "kind": "percent",
      "scope": "each",
      "nullable": {
        "zh": "不回切",
        "en": "Never switch back"
      },
      "label": {
        "zh": "回切阈值",
        "en": "Switch-back level"
      },
      "help": {
        "zh": "这个号余量回到这个数以上时，从排在后面的号切回来",
        "en": "Switch back to this account once its headroom is back above this"
      },
      "min": 0,
      "max": 100,
      "unit": "percent",
      "default": null
    },
    {
      "key": "margin",
      "kind": "percent",
      "scope": "all",
      "label": {
        "zh": "多出几个点才换",
        "en": "Lead needed to switch"
      },
      "help": {
        "zh": "别的号余量要比在用的号多出这么多，才换过去",
        "en": "Another account needs this much more headroom before switching to it"
      },
      "min": 0,
      "max": 50,
      "unit": "percent",
      "default": 10
    }
  ]
}
---*/

// 分 ＝ 余量（两窗取小）。在用的号多算「多出几个点才换」（margin，默认 10）：别的号要比它多出这么多才排到它前面，
// 免得两个号余量相近时来回换。在用的号越用越少，次序会在它掉到「下一个号 − margin」那一读数时变：约好读数叫醒（不用另声明随时间变）。
fn decide(ctx) {
  let s = ctx.settings;
  let margin = if s.margin != () { s.margin.to_float() } else { 10.0 };
  let ids = lib::list(s, ctx);
  let scores = #{};
  let h_of = #{};
  for id in ids {
    let h = lib::left(s, ctx, id, false);
    h_of[id] = h;
    scores[id] = if id == ctx.current { h + margin } else { h };
  }
  let out = lib::decide_with(ctx, scores);
  let cur = h_of[ctx.current];
  if cur != () {
    // 下一个要越过它的号：余量不比它多的那些里最多的一个。
    let next = ();
    for id in ids {
      if id == ctx.current || !lib::usable(s, ctx, id, false) { continue; }
      let h = h_of[id];
      if h <= cur + margin && (next == () || h > next) { next = h; }
    }
    if next != () {
      // 在用号的余量掉到 next − margin 以下时那个号就排到前面了（同分照列表，所以要再多掉一点）：每一窗用到 上限 − (next − margin) 多一点的那一读数。
      for w in ["5h", "7d"] {
        out.wake.when.push(#{ acct: ctx.current, w: w, used_at_least: lib::cap(s, ctx, ctx.current, w, false) - (next - margin) + 0.01 });
      }
    }
  }
  out
}
