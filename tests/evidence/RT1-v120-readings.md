# RT1 · V120 中转并发读数原件（Win11 虚拟机，2026-09-25）

## rt1-bench（N 路同时起跑；每条 40 事件 × 50 ms；/t/claude-code/0/<sid> → 本机常驻后端的中转 127.0.0.1:8788 → 假上游 127.0.0.1:18080；direct = 不经中转）
```
SUMMARY {"tag":"direct1","n":1,"events":40,"ok":1,"bytes":7450,"first_byte_p50":2.2,"first_byte_max":2.2,"max_gap_p50":305.8,"max_gap_max":305.8,"total_p50":2027.1,"total_max":2027.1}
SUMMARY {"tag":"direct10","n":10,"events":40,"ok":10,"bytes":74500,"first_byte_p50":3.5,"first_byte_max":4.0,"max_gap_p50":51.1,"max_gap_max":315.0,"total_p50":2042.5,"total_max":2043.7}
SUMMARY {"tag":"direct30","n":30,"events":40,"ok":30,"bytes":223500,"first_byte_p50":6.7,"first_byte_max":8.2,"max_gap_p50":51.0,"max_gap_max":51.6,"total_p50":2038.0,"total_max":2046.0}
SUMMARY {"tag":"relay1","n":1,"events":40,"ok":1,"bytes":7450,"first_byte_p50":4.6,"first_byte_max":4.6,"max_gap_p50":50.9,"max_gap_max":50.9,"total_p50":2029.1,"total_max":2029.1}
SUMMARY {"tag":"relay10","n":10,"events":40,"ok":10,"bytes":74500,"first_byte_p50":6.7,"first_byte_max":7.3,"max_gap_p50":51.4,"max_gap_max":51.9,"total_p50":2041.2,"total_max":2044.0}
SUMMARY {"tag":"relay30","n":30,"events":40,"ok":30,"bytes":223500,"first_byte_p50":12.0,"first_byte_max":16.0,"max_gap_p50":54.3,"max_gap_max":58.1,"total_p50":2052.4,"total_max":2063.4}
SUMMARY {"tag":"relay60","n":60,"events":40,"ok":60,"bytes":447000,"first_byte_p50":24.3,"first_byte_max":29.8,"max_gap_p50":52.3,"max_gap_max":55.7,"total_p50":2078.1,"total_max":2100.3}
SUMMARY {"tag":"relay100","n":100,"events":40,"ok":100,"bytes":745000,"first_byte_p50":30.7,"first_byte_max":42.8,"max_gap_p50":51.0,"max_gap_max":54.2,"total_p50":2073.8,"total_max":2106.0}
```

## 40 个 claude 替身进程同时起（每个 5 条流，顺序发）
### 第一趟（经中转）
```
wall_s=14.6439263
exitcodes=
streams=200
same_true=200
first_byte p50=13.1 p95=304.9 max=566.4
max_gap p50=84.5 p95=262.2 max=309.9
1790356688950 stream 9e3750d5-a6d9-488c-8c8f-e10749391c8f#0: status=200 bytes=7450 want=7450 same=true first_byte_ms=240.1 max_gap_ms=101.7 total_ms=2760.0
1790356691547 stream 9e3750d5-a6d9-488c-8c8f-e10749391c8f#1: status=200 bytes=7450 want=7450 same=true first_byte_ms=243.9 max_gap_ms=76.9 total_ms=2562.1
1790356691166 stream 9e37541d-a6d9-4572-80f2-d1ae6615c0f2#0: status=200 bytes=7450 want=7450 same=true first_byte_ms=304.9 max_gap_ms=181.4 total_ms=3628.8
1790356688903 stream 9e375589-a6d9-486c-806a-bba77df9506a#0: status=200 bytes=7450 want=7450 same=true first_byte_ms=224.4 max_gap_ms=88.7 total_ms=2713.1
1790356691402 stream 9e375589-a6d9-486c-806a-bba77df9506a#1: status=200 bytes=7450 want=7450 same=true first_byte_ms=209.6 max_gap_ms=80.5 total_ms=2474.7
pidfiles left=0
```
### 对照（同样 40 个，直连假上游）
```
wall_s=10.9655856
streams=200
same_true=200
first_byte p50=1.2 p95=48.9 max=100.6
max_gap p50=51.1 p95=169.8 max=314.2
```
### 第二趟（经中转，带 CPU 账）
```
wall_s=11.0814194
backend_cpu_ms=3218.75 monitor_cpu_ms=828.125
streams=200 same_true=200
first_byte p50=3 p95=100.7 max=199.8
max_gap p50=51.6 p95=108.1 max=166.1
```
