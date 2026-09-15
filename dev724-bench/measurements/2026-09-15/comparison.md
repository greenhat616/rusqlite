# Benchmark comparison

All mutation values are milliseconds per entire workload; query/statistics values are microseconds per call.

| Case | Baseline writes | Workaround writes | Patched writes | Patched vs baseline | Workaround vs patched |
|---|---:|---:|---:|---:|---:|
| short_wal | 2320.19 | 2426.67 | 2287.43 | -1.41% | +6.09% |
| short_memory | 1777.77 | 1886.01 | 1770.88 | -0.39% | +6.50% |
| short_single_wal | 837.12 | 928.56 | 838.43 | +0.16% | +10.75% |
| medium_wal | 6975.80 | 7398.47 | 6833.79 | -2.04% | +8.26% |
| long_wal | 14402.81 | 14254.67 | 14134.94 | -1.86% | +0.85% |

## Per-operation medians and ranges

| Case | Phase | Baseline µs | Workaround µs | Patched µs | Patched min–max µs |
|---|---|---:|---:|---:|---:|
| short_wal | insert | 18.897 | 20.115 | 18.797 | 17.567–19.121 |
| short_wal | replace_same | 39.254 | 41.070 | 38.904 | 36.430–39.722 |
| short_wal | replace_changed | 44.440 | 46.157 | 43.827 | 41.606–44.799 |
| short_wal | delete_half | 26.835 | 26.750 | 26.226 | 24.961–27.261 |
| short_wal | stats_read | 547.683 | 2.026 | 3.381 | 3.346–3.769 |
| short_wal | query_needle | 673.445 | 102.021 | 100.279 | 99.186–110.178 |
| short_wal | query_common | 6658.815 | 6031.475 | 5849.385 | 5692.600–6167.780 |
| short_wal | recovery_scan | 0.260 | 0.247 | 0.244 | 0.243–0.270 |
| short_memory | insert | 16.568 | 17.540 | 16.187 | 15.629–16.793 |
| short_memory | replace_same | 29.142 | 31.004 | 29.284 | 28.247–29.683 |
| short_memory | replace_changed | 33.915 | 35.626 | 33.755 | 32.848–34.487 |
| short_memory | delete_half | 19.079 | 20.108 | 19.258 | 19.026–20.019 |
| short_memory | stats_read | 589.640 | 1.380 | 2.896 | 2.588–2.990 |
| short_memory | query_needle | 692.043 | 110.880 | 111.233 | 109.658–112.002 |
| short_memory | query_common | 6874.115 | 6374.880 | 6274.315 | 6204.710–6529.005 |
| short_memory | recovery_scan | 0.262 | 0.261 | 0.264 | 0.250–0.275 |
| short_single_wal | insert | 106.532 | 123.022 | 107.389 | 101.150–119.813 |
| short_single_wal | replace_same | 135.909 | 139.552 | 137.966 | 129.211–143.836 |
| short_single_wal | replace_changed | 141.900 | 156.509 | 139.166 | 136.344–144.757 |
| short_single_wal | delete_half | 68.933 | 82.720 | 67.525 | 66.495–69.492 |
| short_single_wal | stats_read | 64.724 | 1.924 | 3.339 | 3.296–5.392 |
| short_single_wal | query_needle | 107.447 | 42.679 | 44.458 | 41.825–45.807 |
| short_single_wal | query_common | 719.215 | 649.020 | 639.990 | 633.740–651.420 |
| short_single_wal | recovery_scan | 0.254 | 0.236 | 0.263 | 0.239–0.295 |
| medium_wal | insert | 115.037 | 124.335 | 122.343 | 115.687–122.935 |
| medium_wal | replace_same | 224.797 | 238.174 | 221.571 | 219.859–243.352 |
| medium_wal | replace_changed | 278.798 | 291.867 | 269.911 | 267.762–294.086 |
| medium_wal | delete_half | 164.543 | 153.609 | 141.453 | 139.108–148.841 |
| medium_wal | stats_read | 280.582 | 2.120 | 3.557 | 3.438–3.606 |
| medium_wal | query_needle | 363.646 | 74.103 | 67.772 | 67.006–79.224 |
| medium_wal | query_common | 3274.090 | 3019.035 | 2719.195 | 2717.025–3044.970 |
| medium_wal | recovery_scan | 0.265 | 0.264 | 0.256 | 0.250–0.329 |
| long_wal | insert | 698.143 | 702.853 | 702.232 | 683.117–708.568 |
| long_wal | replace_same | 1641.612 | 1566.138 | 1585.820 | 1576.436–1625.566 |
| long_wal | replace_changed | 1940.765 | 1994.632 | 1975.151 | 1906.806–1986.442 |
| long_wal | delete_half | 1025.761 | 1000.432 | 989.919 | 953.882–1117.536 |
| long_wal | stats_read | 94.428 | 2.007 | 3.437 | 3.409–3.556 |
| long_wal | query_needle | 137.896 | 39.995 | 42.180 | 41.471–44.362 |
| long_wal | query_common | 1035.745 | 945.880 | 879.010 | 874.100–936.725 |
| long_wal | recovery_scan | 0.262 | 0.264 | 0.264 | 0.256–1.950 |
