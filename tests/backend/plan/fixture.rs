//! 计划读面的夹具：pb 0.3.8 对一份合成工作区真跑出来的 `pb dump`（形状 4）与 `pb dump --cell` · 一个假 pb 插件（入口脚本把夹具原样吐出来，并记下自己被怎么起的）。
//! 合成工作区：两片 `alpha`（软件，仓库登记在 `repos/alpha`）· `beta`（研究，只写文档、仓库还不在）；名字与正文全是合成的（「甲功能」之类），
//! 不引 pb 仓与任何真工作区的文件。工作区根在常量里写成占位 `/synthetic/ws`，[`dump`] 换成交进来的那个。
//!
//! alpha 那一片：顶层 A1 能力（子 A1-1 做完了 · A1-2 没签 · A1-3 测试 · A1-4 / A1-6 验收带没执行的 sets · A1-5 验收是收下 A1-1 那一笔的凭）·
//! A2 能力派出去成一块（子 A2-1，接手 SUB，执行那一步）· A3 规矩 covers A1 · A4 规矩 covers project · A5 写了「块:」还没立块；
//! A1-1 签过两笔（早先 `pb sign` 那一笔没有值 · 凭，后一笔凭 A1-5）；A1-2 指着一格不在的 A9（判据红）；`src/stray.txt` 无主；A2 拿着 `src/shared.txt` 没分下去。
//! beta 那一片：B1 问题 · B1-1 来源 · B1-2 事实（记了一笔值「正」，不算做完）；仓库不在 ⇒ `unowned.paths` 为 null、`why` 说为什么。

use serde_json::{json, Value};
use std::path::{Path, PathBuf};

/// 主会话 id · 子 agent id · 对不上的 id（造合成工作区时写进块的接手与签收的「由」）。
pub(crate) const MAIN: &str = "11111111-2222-3333-4444-555555555555";
pub(crate) const SUB: &str = "a0b1c2d3e4f5a6b7c";
pub(crate) const STRANGER: &str = "99999999-0000-0000-0000-000000000000";

/// 常量里工作区根的占位。
const WS: &str = "/synthetic/ws";

/// `pb dump`（0.3.8 真跑的，工作区根换成占位）。
const DUMP: &str = r##"{
 "pb": "0.3.8",
 "shape": 4,
 "workspace": "/synthetic/ws",
 "repo": "alpha",
 "auto": false,
 "domains": [
  {
   "name": "小说",
   "kinds": [
    {
     "name": "纲",
     "edge_words": {
      "to": "连着",
      "with": "连着",
      "after": "连着",
      "replaces": "顶掉"
     },
     "evidence": false,
     "values": [
      "收下"
     ]
    },
    {
     "name": "设定",
     "edge_words": {
      "to": "指向",
      "with": "连着",
      "after": "排在后面",
      "replaces": "顶掉"
     },
     "evidence": false,
     "values": [
      "收下"
     ]
    },
    {
     "name": "线",
     "edge_words": {
      "to": "收在",
      "with": "收在",
      "after": "收在",
      "replaces": "顶掉"
     },
     "evidence": false,
     "values": [
      "收下",
      "埋",
      "推",
      "收"
     ]
    },
    {
     "name": "验收",
     "edge_words": {
      "to": "收",
      "with": "收",
      "after": "收",
      "replaces": "顶掉"
     },
     "evidence": true,
     "values": [
      "收下"
     ]
    },
    {
     "name": "规矩",
     "edge_words": {
      "to": "管",
      "with": "管",
      "after": "管",
      "replaces": "顶掉"
     },
     "evidence": false,
     "values": [
      "收下"
     ]
    }
   ],
   "phases": [
    {
     "name": "定纲",
     "does": "把这一块拆成下一级，每一格写清出口；再写排期：先后与分块，按这个仓库自己的设计与工作原则",
     "marks": []
    },
    {
     "name": "核",
     "does": "定纲之后审纲；写之后对着纲核这一段",
     "marks": []
    },
    {
     "name": "写",
     "does": "写这一段的正文",
     "marks": []
    },
    {
     "name": "看全局",
     "does": "通读这一级：衔接 · 文风 · 线；要改就加一格",
     "marks": [
      "看全局"
     ]
    }
   ]
  },
  {
   "name": "研究",
   "kinds": [
    {
     "name": "问题",
     "edge_words": {
      "to": "指向",
      "with": "连着",
      "after": "排在后面",
      "replaces": "顶掉"
     },
     "evidence": false,
     "values": [
      "收下"
     ]
    },
    {
     "name": "来源",
     "edge_words": {
      "to": "指向",
      "with": "连着",
      "after": "排在后面",
      "replaces": "顶掉"
     },
     "evidence": false,
     "values": [
      "收下",
      "正",
      "反"
     ]
    },
    {
     "name": "事实",
     "edge_words": {
      "to": "凭",
      "with": "凭",
      "after": "凭",
      "replaces": "顶掉"
     },
     "evidence": false,
     "values": [
      "收下",
      "正",
      "反"
     ]
    },
    {
     "name": "推断",
     "edge_words": {
      "to": "推自",
      "with": "推自",
      "after": "推自",
      "replaces": "顶掉"
     },
     "evidence": false,
     "values": [
      "收下",
      "正",
      "反"
     ]
    },
    {
     "name": "诠释",
     "edge_words": {
      "to": "读",
      "with": "读",
      "after": "读",
      "replaces": "顶掉"
     },
     "evidence": false,
     "values": [
      "收下"
     ]
    },
    {
     "name": "价值判断",
     "edge_words": {
      "to": "凭",
      "with": "凭",
      "after": "凭",
      "replaces": "顶掉"
     },
     "evidence": false,
     "values": [
      "收下"
     ]
    },
    {
     "name": "正文",
     "edge_words": {
      "to": "写到",
      "with": "写到",
      "after": "写到",
      "replaces": "顶掉"
     },
     "evidence": false,
     "values": [
      "收下"
     ]
    },
    {
     "name": "佐证",
     "edge_words": {
      "to": "指向",
      "with": "连着",
      "after": "排在后面",
      "replaces": "顶掉"
     },
     "evidence": true,
     "values": [
      "收下"
     ]
    },
    {
     "name": "反证",
     "edge_words": {
      "to": "指向",
      "with": "连着",
      "after": "排在后面",
      "replaces": "顶掉"
     },
     "evidence": true,
     "values": [
      "收下"
     ]
    },
    {
     "name": "规矩",
     "edge_words": {
      "to": "管",
      "with": "管",
      "after": "管",
      "replaces": "顶掉"
     },
     "evidence": false,
     "values": [
      "收下"
     ]
    }
   ],
   "phases": [
    {
     "name": "定题",
     "does": "把这一格的问题拆成子问题；报告分几节、各占哪个文件也在这一步定；再写排期：先后与分块，按这个仓库自己的设计与工作原则",
     "marks": []
    },
    {
     "name": "搜集",
     "does": "找来源、读：一份来源一格，读出的论断各立一格，with 到它凭的那几格",
     "marks": []
    },
    {
     "name": "核证",
     "does": "核来源与论断：摘录对得上原件，论断从它连着的格推得出",
     "marks": []
    },
    {
     "name": "写",
     "does": "按签过的论断写正文，每一节 with 到它写到的论断",
     "marks": []
    },
    {
     "name": "对比整合",
     "does": "横着比这一块底下：相同的并，冲突的并排写清，缺的和没走的路加格，问题本身站不站得住",
     "marks": [
      "看全局"
     ]
    }
   ]
  },
  {
   "name": "软件",
   "kinds": [
    {
     "name": "能力",
     "edge_words": {
      "to": "指向",
      "with": "连着",
      "after": "排在后面",
      "replaces": "顶掉"
     },
     "evidence": false,
     "values": [
      "收下"
     ]
    },
    {
     "name": "模块",
     "edge_words": {
      "to": "实现",
      "with": "实现",
      "after": "实现",
      "replaces": "顶掉"
     },
     "evidence": false,
     "values": [
      "收下"
     ]
    },
    {
     "name": "测试",
     "edge_words": {
      "to": "测",
      "with": "测",
      "after": "测",
      "replaces": "顶掉"
     },
     "evidence": true,
     "values": [
      "收下"
     ]
    },
    {
     "name": "验收",
     "edge_words": {
      "to": "收",
      "with": "收",
      "after": "收",
      "replaces": "顶掉"
     },
     "evidence": true,
     "values": [
      "收下"
     ]
    },
    {
     "name": "规矩",
     "edge_words": {
      "to": "管",
      "with": "管",
      "after": "管",
      "replaces": "顶掉"
     },
     "evidence": false,
     "values": [
      "收下"
     ]
    }
   ],
   "phases": [
    {
     "name": "定架构",
     "does": "先定结构：这一块拆成哪几格、各管什么、格之间怎么连（已有代码的仓库，结构可以先用 code-picture 看）；结构定了再写排期：先后与分块，按这个仓库自己的设计与工作原则",
     "marks": []
    },
    {
     "name": "审计",
     "does": "定架构之后审架构；执行之后从设计看到实现，跑测试与读数见 quality-gates，代码跟图上的边对不对得上（import · 线程 · 跨进程）用 code-picture 查",
     "marks": []
    },
    {
     "name": "执行",
     "does": "写代码 · 推远端 · 打 tag · 发 release；测试怎么写见 quality-gates",
     "marks": []
    },
    {
     "name": "回看",
     "does": "看全局，决定下一步",
     "marks": [
      "看全局"
     ]
    }
   ]
  }
 ],
 "slices": [
  {
   "name": "alpha",
   "domain": "软件",
   "path": "repos/alpha",
   "docs_only": false,
   "records": [
    {
     "n": 1,
     "summary": "读写分两半",
     "at": "2026-01-03T00:00:00Z",
     "by": "人",
     "quotes": [
      "「分开做」",
      "「先读后写」"
     ]
    }
   ],
   "kinds": [
    {
     "name": "能力",
     "edge_words": {
      "to": "指向",
      "with": "连着",
      "after": "排在后面",
      "replaces": "顶掉"
     },
     "evidence": false,
     "values": [
      "收下"
     ]
    },
    {
     "name": "模块",
     "edge_words": {
      "to": "实现",
      "with": "实现",
      "after": "实现",
      "replaces": "顶掉"
     },
     "evidence": false,
     "values": [
      "收下"
     ]
    },
    {
     "name": "测试",
     "edge_words": {
      "to": "测",
      "with": "测",
      "after": "测",
      "replaces": "顶掉"
     },
     "evidence": true,
     "values": [
      "收下"
     ]
    },
    {
     "name": "验收",
     "edge_words": {
      "to": "收",
      "with": "收",
      "after": "收",
      "replaces": "顶掉"
     },
     "evidence": true,
     "values": [
      "收下"
     ]
    },
    {
     "name": "规矩",
     "edge_words": {
      "to": "管",
      "with": "管",
      "after": "管",
      "replaces": "顶掉"
     },
     "evidence": false,
     "values": [
      "收下"
     ]
    }
   ],
   "phases": [
    {
     "name": "定架构",
     "does": "先定结构：这一块拆成哪几格、各管什么、格之间怎么连（已有代码的仓库，结构可以先用 code-picture 看）；结构定了再写排期：先后与分块，按这个仓库自己的设计与工作原则",
     "marks": []
    },
    {
     "name": "审计",
     "does": "定架构之后审架构；执行之后从设计看到实现，跑测试与读数见 quality-gates，代码跟图上的边对不对得上（import · 线程 · 跨进程）用 code-picture 查",
     "marks": []
    },
    {
     "name": "执行",
     "does": "写代码 · 推远端 · 打 tag · 发 release；测试怎么写见 quality-gates",
     "marks": []
    },
    {
     "name": "回看",
     "does": "看全局，决定下一步",
     "marks": [
      "看全局"
     ]
    }
   ],
   "done": false,
   "top": [
    "A1",
    "A2",
    "A3",
    "A4",
    "A5"
   ],
   "blocks": [
    {
     "id": "project",
     "cells": [
      "project"
     ],
     "dir": ".planned-build/alpha",
     "owner": "11111111-2222-3333-4444-555555555555",
     "phase": "回看",
     "at": null,
     "row": true
    },
    {
     "id": "A2",
     "cells": [
      "A2"
     ],
     "dir": ".planned-build/alpha/A2",
     "owner": "a0b1c2d3e4f5a6b7c",
     "phase": "执行",
     "at": "A2-1",
     "row": true
    }
   ],
   "unbuilt_blocks": [
    {
     "id": "A5",
     "cells": [
      "A5"
     ]
    }
   ],
   "unowned": {
    "paths": [
     "src/stray.txt"
    ],
    "why": null
   },
   "unsplit": [
    {
     "id": "A2",
     "path": "src/shared.txt"
    }
   ],
   "check": {
    "unreadable": [],
    "red": [
     {
      "rule": "悬空",
      "what": "A1-2 --to--> A9，A9 不在盘上",
      "block": "project",
      "fix": "边指的格不在盘上：改成一个在的编号，或先把那一格建出来；编号的每一段前缀都要是一个在的格",
      "refs": [
       {
        "start": 0,
        "end": 4,
        "ids": [
         "A1-2"
        ]
       }
      ]
     }
    ],
    "undecidable": []
   },
   "cells": [
    {
     "id": "A1",
     "title": "甲功能",
     "kind": "能力",
     "body": "照 A3 的公约；读写两半见 A1-1/2，`A2` 在代码里不算。",
     "parent": null,
     "children": [
      "A1-1",
      "A1-2",
      "A1-3",
      "A1-4",
      "A1-5",
      "A1-6"
     ],
     "edges": {
      "to": [],
      "with": [],
      "after": [],
      "replaces": []
     },
     "files": [],
     "evidence": false,
     "status": "没做完",
     "why": "里面 1/2 做完了",
     "signs": [],
     "counts": null,
     "sets": [],
     "block": "project",
     "owner": "11111111-2222-3333-4444-555555555555",
     "signer": null,
     "covers": [],
     "covered_by": [
      "A3",
      "A4"
     ],
     "refs": {
      "title": [],
      "body": [
       {
        "start": 2,
        "end": 4,
        "ids": [
         "A3"
        ]
       },
       {
        "start": 15,
        "end": 21,
        "ids": [
         "A1-1",
         "A1-2"
        ]
       }
      ]
     }
    },
    {
     "id": "A1-1",
     "title": "甲的读入",
     "kind": "模块",
     "body": "读入。",
     "parent": "A1",
     "children": [],
     "edges": {
      "to": [],
      "with": [
       "A1"
      ],
      "after": [],
      "replaces": []
     },
     "files": [
      {
       "path": "src/read.txt",
       "state": "在",
       "note": "3 字"
      }
     ],
     "evidence": false,
     "status": "做完了",
     "why": null,
     "signs": [
      {
       "at": "2026-01-01T00:00:00Z",
       "by": "a0b1c2d3e4f5a6b7c",
       "value": "收下",
       "via": null,
       "done": true,
       "reason": "第一次，照 A1 看过",
       "refs": [
        {
         "start": 6,
         "end": 8,
         "ids": [
          "A1"
         ]
        }
       ]
      },
      {
       "at": "2026-01-02T00:00:00Z",
       "by": "99999999-0000-0000-0000-000000000000",
       "value": "收下",
       "via": "A1-5",
       "done": true,
       "reason": "",
       "refs": []
      }
     ],
     "counts": null,
     "sets": [],
     "block": "project",
     "owner": "11111111-2222-3333-4444-555555555555",
     "signer": "11111111-2222-3333-4444-555555555555",
     "covers": [],
     "covered_by": [
      "A3",
      "A4"
     ],
     "refs": {
      "title": [],
      "body": []
     }
    },
    {
     "id": "A1-2",
     "title": "甲的写出",
     "kind": "模块",
     "body": "写出。",
     "parent": "A1",
     "children": [],
     "edges": {
      "to": [],
      "with": [
       "A1"
      ],
      "after": [
       "A1-1"
      ],
      "replaces": []
     },
     "files": [
      {
       "path": "src/write.txt",
       "state": "缺",
       "note": ""
      }
     ],
     "evidence": false,
     "status": "没做完",
     "why": "没签",
     "signs": [],
     "counts": null,
     "sets": [],
     "block": "project",
     "owner": "11111111-2222-3333-4444-555555555555",
     "signer": "11111111-2222-3333-4444-555555555555",
     "covers": [],
     "covered_by": [
      "A3",
      "A4"
     ],
     "refs": {
      "title": [],
      "body": []
     }
    },
    {
     "id": "A1-3",
     "title": "测读入",
     "kind": "测试",
     "body": "测读入的两形。",
     "parent": "A1",
     "children": [],
     "edges": {
      "to": [],
      "with": [
       "A1-1"
      ],
      "after": [],
      "replaces": []
     },
     "files": [
      {
       "path": "tests/read_test.txt",
       "state": "在",
       "note": "4 字"
      }
     ],
     "evidence": true,
     "status": "凭据",
     "why": null,
     "signs": [],
     "counts": null,
     "sets": [],
     "block": "project",
     "owner": "11111111-2222-3333-4444-555555555555",
     "signer": null,
     "covers": [],
     "covered_by": [
      "A3",
      "A4"
     ],
     "refs": {
      "title": [],
      "body": []
     }
    },
    {
     "id": "A1-4",
     "title": "收下甲的写出",
     "kind": "验收",
     "body": "看了 A1-2 的对账。",
     "parent": "A1",
     "children": [],
     "edges": {
      "to": [],
      "with": [],
      "after": [],
      "replaces": []
     },
     "files": [],
     "evidence": true,
     "status": "凭据",
     "why": null,
     "signs": [],
     "counts": null,
     "sets": [
      {
       "target": "A1-2",
       "value": "收下"
      }
     ],
     "block": "project",
     "owner": "11111111-2222-3333-4444-555555555555",
     "signer": null,
     "covers": [],
     "covered_by": [
      "A3",
      "A4"
     ],
     "refs": {
      "title": [],
      "body": [
       {
        "start": 3,
        "end": 7,
        "ids": [
         "A1-2"
        ]
       }
      ]
     }
    },
    {
     "id": "A1-5",
     "title": "收下甲的读入",
     "kind": "验收",
     "body": "跑了测读入，两形都过。",
     "parent": "A1",
     "children": [],
     "edges": {
      "to": [],
      "with": [],
      "after": [],
      "replaces": []
     },
     "files": [],
     "evidence": true,
     "status": "凭据",
     "why": null,
     "signs": [],
     "counts": null,
     "sets": [],
     "block": "project",
     "owner": "11111111-2222-3333-4444-555555555555",
     "signer": null,
     "covers": [],
     "covered_by": [
      "A3",
      "A4"
     ],
     "refs": {
      "title": [],
      "body": []
     }
    },
    {
     "id": "A1-6",
     "title": "收下乙的一半",
     "kind": "验收",
     "body": "越过块去收。",
     "parent": "A1",
     "children": [],
     "edges": {
      "to": [],
      "with": [],
      "after": [],
      "replaces": []
     },
     "files": [],
     "evidence": true,
     "status": "凭据",
     "why": null,
     "signs": [],
     "counts": null,
     "sets": [
      {
       "target": "A2-1",
       "value": "收下"
      }
     ],
     "block": "project",
     "owner": "11111111-2222-3333-4444-555555555555",
     "signer": null,
     "covers": [],
     "covered_by": [
      "A3",
      "A4"
     ],
     "refs": {
      "title": [],
      "body": []
     }
    },
    {
     "id": "A2",
     "title": "乙功能",
     "kind": "能力",
     "body": "派出去的一块。",
     "parent": null,
     "children": [
      "A2-1"
     ],
     "edges": {
      "to": [],
      "with": [],
      "after": [],
      "replaces": []
     },
     "files": [
      {
       "path": "src/shared.txt",
       "state": "在",
       "note": "6 字"
      }
     ],
     "evidence": false,
     "status": "没做完",
     "why": "里面 0/1 做完了",
     "signs": [],
     "counts": null,
     "sets": [],
     "block": "A2",
     "owner": "a0b1c2d3e4f5a6b7c",
     "signer": "11111111-2222-3333-4444-555555555555",
     "covers": [],
     "covered_by": [
      "A4"
     ],
     "refs": {
      "title": [],
      "body": []
     }
    },
    {
     "id": "A2-1",
     "title": "乙的一半",
     "kind": "模块",
     "body": "乙的一半，照 A1 走。",
     "parent": "A2",
     "children": [],
     "edges": {
      "to": [],
      "with": [
       "A2"
      ],
      "after": [],
      "replaces": []
     },
     "files": [],
     "evidence": false,
     "status": "没做完",
     "why": "没签",
     "signs": [],
     "counts": null,
     "sets": [],
     "block": "A2",
     "owner": "a0b1c2d3e4f5a6b7c",
     "signer": "a0b1c2d3e4f5a6b7c",
     "covers": [],
     "covered_by": [
      "A4"
     ],
     "refs": {
      "title": [],
      "body": [
       {
        "start": 7,
        "end": 9,
        "ids": [
         "A1"
        ]
       }
      ]
     }
    },
    {
     "id": "A3",
     "title": "公约",
     "kind": "规矩",
     "body": "守什么：读写同一份格式。",
     "parent": null,
     "children": [],
     "edges": {
      "to": [],
      "with": [
       "A1"
      ],
      "after": [],
      "replaces": []
     },
     "files": [],
     "evidence": false,
     "status": "没做完",
     "why": "没签",
     "signs": [],
     "counts": null,
     "sets": [],
     "block": "project",
     "owner": "11111111-2222-3333-4444-555555555555",
     "signer": "11111111-2222-3333-4444-555555555555",
     "covers": [
      "A1"
     ],
     "covered_by": [
      "A4"
     ],
     "refs": {
      "title": [],
      "body": []
     }
    },
    {
     "id": "A4",
     "title": "总则",
     "kind": "规矩",
     "body": "守什么：全片一样。",
     "parent": null,
     "children": [],
     "edges": {
      "to": [],
      "with": [
       "A1"
      ],
      "after": [],
      "replaces": []
     },
     "files": [],
     "evidence": false,
     "status": "没做完",
     "why": "没签",
     "signs": [],
     "counts": null,
     "sets": [],
     "block": "project",
     "owner": "11111111-2222-3333-4444-555555555555",
     "signer": "11111111-2222-3333-4444-555555555555",
     "covers": [
      "project"
     ],
     "covered_by": [],
     "refs": {
      "title": [],
      "body": []
     }
    },
    {
     "id": "A5",
     "title": "丙功能",
     "kind": "能力",
     "body": "还没立的一块。",
     "parent": null,
     "children": [],
     "edges": {
      "to": [],
      "with": [],
      "after": [],
      "replaces": []
     },
     "files": [],
     "evidence": false,
     "status": "没做完",
     "why": "没签",
     "signs": [],
     "counts": null,
     "sets": [],
     "block": "project",
     "owner": "11111111-2222-3333-4444-555555555555",
     "signer": "11111111-2222-3333-4444-555555555555",
     "covers": [],
     "covered_by": [
      "A4"
     ],
     "refs": {
      "title": [],
      "body": []
     }
    }
   ],
   "pending_sets": [
    {
     "source": "A1-4",
     "target": "A1-2",
     "value": "收下",
     "why_not": null
    },
    {
     "source": "A1-6",
     "target": "A2-1",
     "value": "收下",
     "why_not": "来处在 project 那一块，A2-1 的上一级格在 A2 那一块 —— 两头落进同一块，那一块的 update 才执行"
    }
   ],
   "archived": []
  },
  {
   "name": "beta",
   "domain": "研究",
   "path": "beta",
   "docs_only": true,
   "records": [],
   "kinds": [
    {
     "name": "问题",
     "edge_words": {
      "to": "指向",
      "with": "连着",
      "after": "排在后面",
      "replaces": "顶掉"
     },
     "evidence": false,
     "values": [
      "收下"
     ]
    },
    {
     "name": "来源",
     "edge_words": {
      "to": "指向",
      "with": "连着",
      "after": "排在后面",
      "replaces": "顶掉"
     },
     "evidence": false,
     "values": [
      "收下",
      "正",
      "反"
     ]
    },
    {
     "name": "事实",
     "edge_words": {
      "to": "凭",
      "with": "凭",
      "after": "凭",
      "replaces": "顶掉"
     },
     "evidence": false,
     "values": [
      "收下",
      "正",
      "反"
     ]
    },
    {
     "name": "推断",
     "edge_words": {
      "to": "推自",
      "with": "推自",
      "after": "推自",
      "replaces": "顶掉"
     },
     "evidence": false,
     "values": [
      "收下",
      "正",
      "反"
     ]
    },
    {
     "name": "诠释",
     "edge_words": {
      "to": "读",
      "with": "读",
      "after": "读",
      "replaces": "顶掉"
     },
     "evidence": false,
     "values": [
      "收下"
     ]
    },
    {
     "name": "价值判断",
     "edge_words": {
      "to": "凭",
      "with": "凭",
      "after": "凭",
      "replaces": "顶掉"
     },
     "evidence": false,
     "values": [
      "收下"
     ]
    },
    {
     "name": "正文",
     "edge_words": {
      "to": "写到",
      "with": "写到",
      "after": "写到",
      "replaces": "顶掉"
     },
     "evidence": false,
     "values": [
      "收下"
     ]
    },
    {
     "name": "佐证",
     "edge_words": {
      "to": "指向",
      "with": "连着",
      "after": "排在后面",
      "replaces": "顶掉"
     },
     "evidence": true,
     "values": [
      "收下"
     ]
    },
    {
     "name": "反证",
     "edge_words": {
      "to": "指向",
      "with": "连着",
      "after": "排在后面",
      "replaces": "顶掉"
     },
     "evidence": true,
     "values": [
      "收下"
     ]
    },
    {
     "name": "规矩",
     "edge_words": {
      "to": "管",
      "with": "管",
      "after": "管",
      "replaces": "顶掉"
     },
     "evidence": false,
     "values": [
      "收下"
     ]
    }
   ],
   "phases": [
    {
     "name": "定题",
     "does": "把这一格的问题拆成子问题；报告分几节、各占哪个文件也在这一步定；再写排期：先后与分块，按这个仓库自己的设计与工作原则",
     "marks": []
    },
    {
     "name": "搜集",
     "does": "找来源、读：一份来源一格，读出的论断各立一格，with 到它凭的那几格",
     "marks": []
    },
    {
     "name": "核证",
     "does": "核来源与论断：摘录对得上原件，论断从它连着的格推得出",
     "marks": []
    },
    {
     "name": "写",
     "does": "按签过的论断写正文，每一节 with 到它写到的论断",
     "marks": []
    },
    {
     "name": "对比整合",
     "does": "横着比这一块底下：相同的并，冲突的并排写清，缺的和没走的路加格，问题本身站不站得住",
     "marks": [
      "看全局"
     ]
    }
   ],
   "done": false,
   "top": [
    "B1"
   ],
   "blocks": [
    {
     "id": "project",
     "cells": [
      "project"
     ],
     "dir": ".planned-build/beta",
     "owner": "11111111-2222-3333-4444-555555555555",
     "phase": "回看",
     "at": null,
     "row": true
    }
   ],
   "unbuilt_blocks": [],
   "unowned": {
    "paths": null,
    "why": "fatal: cannot change to '/synthetic/ws/beta': 没有那个文件或目录"
   },
   "unsplit": [],
   "check": {
    "unreadable": [],
    "red": [
     {
      "rule": "只出不进",
      "what": "B1-1 --with--> B1，B1-1 是 来源（out: 无）",
      "block": "project",
      "fix": "写了 out: 无 的那一类不往外连：删掉那条边；要两头都印的，改由对面那一格写 with 指过来",
      "refs": [
       {
        "start": 0,
        "end": 4,
        "ids": [
         "B1-1"
        ]
       },
       {
        "start": 15,
        "end": 17,
        "ids": [
         "B1"
        ]
       },
       {
        "start": 18,
        "end": 22,
        "ids": [
         "B1-1"
        ]
       }
      ]
     }
    ],
    "undecidable": []
   },
   "cells": [
    {
     "id": "B1",
     "title": "问题",
     "kind": "问题",
     "body": "问一个合成的问题。",
     "parent": null,
     "children": [
      "B1-1",
      "B1-2"
     ],
     "edges": {
      "to": [],
      "with": [],
      "after": [],
      "replaces": []
     },
     "files": [],
     "evidence": false,
     "status": "没做完",
     "why": "里面 0/2 做完了",
     "signs": [],
     "counts": null,
     "sets": [],
     "block": "project",
     "owner": "11111111-2222-3333-4444-555555555555",
     "signer": null,
     "covers": [],
     "covered_by": [],
     "refs": {
      "title": [],
      "body": []
     }
    },
    {
     "id": "B1-1",
     "title": "一份来源",
     "kind": "来源",
     "body": "合成的来源。",
     "parent": "B1",
     "children": [],
     "edges": {
      "to": [],
      "with": [
       "B1"
      ],
      "after": [],
      "replaces": []
     },
     "files": [],
     "evidence": false,
     "status": "没做完",
     "why": "没签",
     "signs": [],
     "counts": null,
     "sets": [],
     "block": "project",
     "owner": "11111111-2222-3333-4444-555555555555",
     "signer": "11111111-2222-3333-4444-555555555555",
     "covers": [],
     "covered_by": [],
     "refs": {
      "title": [],
      "body": []
     }
    },
    {
     "id": "B1-2",
     "title": "一条事实",
     "kind": "事实",
     "body": "合成的事实。",
     "parent": "B1",
     "children": [],
     "edges": {
      "to": [],
      "with": [
       "B1-1"
      ],
      "after": [],
      "replaces": []
     },
     "files": [],
     "evidence": false,
     "status": "没做完",
     "why": "没签",
     "signs": [
      {
       "at": "2026-01-04T00:00:00Z",
       "by": "11111111-2222-3333-4444-555555555555",
       "value": "正",
       "via": "B1-1",
       "done": false,
       "reason": "",
       "refs": []
      }
     ],
     "counts": "正 1（B1-1）",
     "sets": [],
     "block": "project",
     "owner": "11111111-2222-3333-4444-555555555555",
     "signer": "11111111-2222-3333-4444-555555555555",
     "covers": [],
     "covered_by": [],
     "refs": {
      "title": [],
      "body": []
     }
    }
   ],
   "pending_sets": [],
   "archived": []
  }
 ]
}"##;

/// `pb dump --cell alpha A1-2`（同一份工作区）。
const CELL: &str = r##"{
 "pb": "0.3.8",
 "shape": 4,
 "workspace": "/synthetic/ws",
 "slice": "alpha",
 "id": "A1-2",
 "agent_view": "── A1-2 甲的写出（模块 · 没做完 · 元行住 .planned-build/alpha/图.md）\n> id: A1-2 | kind: 模块 | with: A1 | after: A1-1 | to: A9\n> 文件: src/write.txt\n\n写出。\n文件声明 1 个：src/write.txt\n\n父那一层\n── A1 甲功能（能力 · 没做完 · 元行住 .planned-build/alpha/图.md）\n> id: A1 | kind: 能力\n\n照 A3 的公约；读写两半见 A1-1/2，`A2` 在代码里不算。\nA1 这一层  1+0/2  没做完\n  A1-1  甲的读入  做完了 · 收下：凭 A1-5 · 99999999-0000-0000-0000-000000000000 @ 2026-01-02T00:00:00Z · 没排上\n  A1-2  甲的写出  没做完 · 收下：— · 没排上 · 待收 A1-4 收下：等 project 那一块的 update 执行\n  凭据 4 格（不算活，不进分母）：A1-3 测读入 · A1-4 收下甲的写出 · A1-5 收下甲的读入 · A1-6 收下乙的一半\nA1 那一层的排期 0 格，A1-2 还没排上\n  下一格 A3 公约\n\n这一格的边 3 条\n  A1-2 --to--> A9 · A9 不在盘上\n  A1-2 --with--> A1 · A1 住 .planned-build/alpha/图.md\n  A1-2 --after--> A1-1 · A1-1 住 .planned-build/alpha/图.md\n  被 A3 covers（范围 A1）\n  被 A4 covers（范围 project）\n\n全局 5 格\n  A1  甲功能\n  A2  乙功能\n  A3  公约\n  A4  总则\n  A5  丙功能\n待收：A1-4 sets 它 收下 —— 等 project 那一块的 update 执行\nA1-2 底下下一个空号：A1-2-1"
}"##;

/// `pb dump --cell alpha A9`（图上没有的一格）。
const CELL_MISSING: &str = r##"{
 "pb": "0.3.8",
 "shape": 4,
 "workspace": "/synthetic/ws",
 "slice": "alpha",
 "id": "A9",
 "error": "A9 不在「alpha」的图上"
}"##;

fn at(text: &str, workspace: &str) -> Value {
    serde_json::from_str(&text.replace(WS, workspace)).unwrap()
}

/// 一份 dump，工作区根是 `workspace`。
pub(crate) fn dump(workspace: &str) -> Value {
    at(DUMP, workspace)
}

/// `pb dump --cell alpha A1-2` 那一份。
pub(crate) fn cell_view(workspace: &str) -> Value {
    at(CELL, workspace)
}

/// `pb dump --cell alpha A9` 那一份（带 `error`）。
pub(crate) fn cell_view_missing(workspace: &str) -> Value {
    at(CELL_MISSING, workspace)
}

/// 同一份，只是那一片的仓库登记在别处（`pb repo <路径>`：dump 每片带 `path`，相对工作区根）。
pub(crate) fn dump_at(workspace: &str, path: &str) -> Value {
    let mut d = dump(workspace);
    d["slices"][0]["path"] = json!(path);
    d
}

/// 同一份，只是 alpha 那一片这一刻读不成（pb 给 `error`，就没有下面那几样）。
pub(crate) fn dump_broken(workspace: &str) -> Value {
    let mut d = dump(workspace);
    d["slices"][0] = json!({"name": "alpha", "domain": "软件", "path": "repos/alpha", "docs_only": false, "error": "图.md 第 3 行：元行缺 id"});
    d
}

/// 夹具的身份表：MAIN 是一个活着、在等你批准的会话；SUB 是 MAIN 底下的子 agent；别的都对不上。
pub(crate) fn who(id: &str) -> crate::plan::Whose {
    use crate::plan::{Live, Whose};
    let live = Some(Live {
        activity: Some(crate::agents::SessionActivity::NeedsYou),
        needs: Some(crate::observe::recognize::facts::NeedsKind::Approve),
    });
    match id {
        MAIN => Whose::Session {
            sid: MAIN.into(),
            live,
        },
        SUB => Whose::Subagent {
            parent: MAIN.into(),
            live,
        },
        _ => Whose::Unknown,
    }
}

/// 一次性目录。
pub(crate) fn scratch(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("plan-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

/// 在 `root` 底下造一个假 pb 插件：清单名字是 pb，入口脚本
/// ① 把自己的 argv · 当前目录 · 环境里有没有 `PB_ID` 记进 `<root>/ran.json`（最近那一次），argv 另追加一行进 `<root>/runs.log`（每一次）；
/// ② 当前目录（往上找）有 `.planned-build/` ⇒ `record --slice` 说一句记下了；`dump --cell` 印 `<那个目录>/cell.json`、别的印 `<那个目录>/dump.json`，退 0；没有 ⇒ stderr 一句、退 3。
/// 它认的 `--` 旗标就是真 pb 那边后端会发的那几个（`protocol_doc_guard` 的子进程旗标对拍读这份）。
/// `rc` 给了就不管上面，直接以它退出（造「没有 dump」「别的失败」）。
pub(crate) fn fake_pb(root: &Path, name: &str, rc: Option<i32>) -> PathBuf {
    let tools = root.join("skills/planned-build/tools");
    std::fs::create_dir_all(&tools).unwrap();
    std::fs::create_dir_all(root.join(".claude-plugin")).unwrap();
    std::fs::write(
        root.join(".claude-plugin/plugin.json"),
        json!({"name": name, "version": "0.0.0"}).to_string(),
    )
    .unwrap();
    let forced = rc.map_or("None".to_string(), |c| c.to_string());
    let ran = root.join("ran.json");
    std::fs::write(
        tools.join("entry.py"),
        format!(
            r#"import json, os, sys
open({ran:?}, "w").write(json.dumps({{"argv": sys.argv[1:], "cwd": os.getcwd(), "pb_id": "PB_ID" in os.environ}}))
open({runs:?}, "a").write(json.dumps(sys.argv[1:]) + "\n")
forced = {forced}
if forced is not None:
    sys.stderr.write("forced\n"); sys.exit(forced)
d = os.getcwd()
while True:
    if os.path.isdir(os.path.join(d, ".planned-build")):
        if sys.argv[1:3] == ["record", "--slice"]:
            print("记下了 原话 1"); sys.exit(0)
        f = "cell.json" if sys.argv[1:3] == ["dump", "--cell"] else "dump.json"
        sys.stdout.write(open(os.path.join(d, f)).read()); sys.exit(0)
    up = os.path.dirname(d)
    if up == d:
        sys.stderr.write("读不成：这里不是一个 pb 工作区\n"); sys.exit(3)
    d = up
"#,
            ran = ran.to_string_lossy(),
            runs = root.join("runs.log").to_string_lossy(),
        ),
    )
    .unwrap();
    tools.join("entry.py")
}

/// 造一个工作区：`.planned-build/alpha/` ＋ 工作区根上的 `.env` ＋ 一份 `dump.json` 与 `cell.json`（假 pb 吐它们）。
/// 仓库目录照 dump 里那一片的 `path`（没有 ⇒ 片名）。
pub(crate) fn workspace(root: &Path, doc: &Value) -> PathBuf {
    let repo = doc["slices"][0]["path"].as_str().unwrap_or("alpha");
    std::fs::create_dir_all(root.join(".planned-build/alpha")).unwrap();
    std::fs::create_dir_all(root.join(repo).join("src")).unwrap();
    std::fs::write(root.join(".env"), r#"{"仓库": "alpha"}"#).unwrap();
    std::fs::write(root.join("dump.json"), doc.to_string()).unwrap();
    let ws = doc["workspace"].as_str().unwrap_or(WS);
    std::fs::write(root.join("cell.json"), cell_view(ws).to_string()).unwrap();
    root.to_path_buf()
}

/// 假 pb 一次一次被起时带的参数（`runs.log`，没起过 ⇒ 空）。
pub(crate) fn runs(root: &Path) -> Vec<Vec<String>> {
    std::fs::read_to_string(root.join("runs.log"))
        .unwrap_or_default()
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect()
}
