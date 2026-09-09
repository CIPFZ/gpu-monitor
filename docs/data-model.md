# 数据接口

[项目首页](../README.md) · [功能与使用](features.md) · [系统架构](architecture.md) · [开发指南](development.md)

Rust 是快照与运行时协议的定义来源。桌面端的 [wire.ts](../crates/gpu-monitor-gui/src-web/src/monitor/wire.ts) 和 [runtime-wire.ts](../crates/gpu-monitor-gui/src-web/src/monitor/runtime-wire.ts) 从公开 Rust 类型生成；应用层辅助逻辑使用 [models.ts](../crates/gpu-monitor-gui/src-web/src/monitor/models.ts)。

## 快照与版本

CLI JSON、GUI 快照 IPC 和录制文件使用 `MonitorSnapshot`：

```json
{
  "schema_version": 1,
  "sampled_at_ms": 1780000000000,
  "gpus": [],
  "failures": [],
  "error": null
}
```

| 字段 | 含义 |
| --- | --- |
| `schema_version` | 当前为整数 `1`；旧录制省略时按 `1` 读取 |
| `sampled_at_ms` | 本轮采样的 Unix 毫秒时间 |
| `gpus` | 身份可读取的 GPU，内部指标仍可能部分不可用 |
| `failures` | 必要身份或设备查询失败的设备 |
| `error` | 初始化、枚举、无设备等全局错误，成功时为 `null` |

`GpuInfo` 包含 `device`、`metrics`、`memory`、`processes`、`sampled_at_ms` 和 `issues`。每卡时间戳属于该条采样数据，不能仅凭外层时间将保留的旧数据当成新数据。

缓存重复读取可返回相同时间戳。系统时钟可能调整，因此录制按文件顺序表示采集顺序，不能按 Unix 时间重新排序。连续 JSON 和录制均为每行一个快照对象，单次 JSON 和桌面快照导出为一个格式化对象。

回放只支持 schema `1`。不认识的版本会被拒绝，不尝试按当前字段猜测其含义。消费者通过 `.gpus` 读取设备，并处理快照中的错误与 nullable 字段。

回放中的快照、GPU 采样时间和进程启动时间必须在 `0` 至 `8640000000000000` 毫秒之间，以保证可表示为 JavaScript Date。同帧内 GPU UUID 不得重复，GPU 采样时间不得晚于外层快照时间。

## 设备和指标

### DeviceInfo

| 字段 | 类型与单位 |
| --- | --- |
| `index` | 当前枚举索引，整数 |
| `name` | 设备名称 |
| `uuid` | 设备稳定标识，关联历史和筛选时使用 |
| `pci_bus_id` | PCI 地址字符串 |
| `driver_version` | NVIDIA 驱动版本字符串 |
| `cuda_version` | 驱动支持的 CUDA 版本字符串或 `null`，不是本机 Toolkit 版本 |
| `power_limit`、`power_limit_max` | 当前与最大可配置功耗上限，整数 W 或 `null` |

### GpuMetrics

| 字段 | 类型 / 单位 | 含义 |
| --- | --- | --- |
| `gpu_utilization` | 整数 % 或 `null` | GPU 忙碌率 |
| `memory_utilization` | 整数 % 或 `null` | 显存控制器忙碌率 |
| `encoder_utilization`、`decoder_utilization` | 整数 % 或 `null` | 编解码器忙碌率 |
| `temperature` | 整数 °C 或 `null` | 温度 |
| `power_usage` | 整数 mW 或 `null` | 功耗；除以 1000 转换为 W |
| `fan_speed` | 整数 % 或 `null` | NVML 风扇索引 0 的速度比例 |
| `clock_graphics`、`clock_memory`、`clock_sm` | 整数 MHz 或 `null` | 图形、显存、SM 时钟 |
| `performance_state` | 字符串或 `null` | 性能状态，例如 `P0`、`P2` |
| `throttle_reasons` | 字符串数组或 `null` | 稳定 snake_case 原因标识，例如 `sw_power_cap`、`gpu_idle`；空数组表示未报告原因 |
| `pcie_generation`、`pcie_width` | 整数或 `null` | 当前 PCIe 代数和通道宽度 |
| `pcie_rx_kb_per_second`、`pcie_tx_kb_per_second` | 整数 KB/s 或 `null` | NVML 报告的 PCIe 接收 / 发送吞吐；不按 KiB 字段处理 |

`0` 是真实读数，`null` 表示不可用。数值不可用的驱动原因保存在 `issues`，不使用零值代替失败。

`memory` 为 `{ total, used, free }` 或 `null`，三个数值均为字节。容量占用应在对象可用且 `total > 0` 时计算 `used / total × 100`。`MiB = 1024²` 字节，`GiB = 1024³` 字节；容量比例与 `memory_utilization` 不能互换。

## 进程字段

| 字段 | 类型与含义 |
| --- | --- |
| `pid` | 进程 ID，整数 |
| `name` | 进程名，无法读取时为 `unknown` |
| `gpu_memory` | 此进程在外层 GPU 上占用的字节数或 `null` |
| `process_type` | `Graphics`、`Compute`、`Mixed` 或 `Unknown` |
| `user` | 本地 `/etc/passwd` 对应的账户名或 `null` |
| `uid` | 实 UID，整数或 `null` |
| `command` | 保留参数边界的字符串数组或 `null` |
| `started_at_ms` | 进程启动的 Unix 毫秒时间或 `null` |
| `elapsed_seconds` | 采样时的运行秒数或 `null`，使用系统 uptime 计算 |

同一卡内的计算 / 图形查询按 PID 合并，同时存在时类型为 `Mixed`，显存取两个可用值中的较大者，避免重复相加。同一 PID 在不同 GPU 上可能各有分配；跨 GPU 聚合还需要启动时间，以区分 PID 重用。

操作系统元数据读取失败不会使 GPU 查询失败。权限受限、读取期间进程退出 / PID 重用、文件过大等情况保留未知值。每次读取上限为 1 MiB，过大的命令行不伪装成完整参数。账户名不经过 LDAP / NSS 远程查询，名称未知时仍可保留和使用 UID。

完整参数可包含应用传入的敏感信息。CLI 默认输出 `command: null`，`--include-command` 才保留；录制及桌面导出也默认清除此字段。桌面实时详情中可主动展开已取得的参数。

`gpu-monitor --json processes` 使用同样的五个顶层字段，但 GPU 条目简化为：

- `device`：`index`、`uuid`、`name`。
- `sampled_at_ms`、`issues`。
- `processes`：上述进程字段，另附 `gpu_memory_mib`，按整 MiB 向下取整，未知时仍为 `null`。

## 错误分层

`SampleError` 为 `{ kind, message }`。程序逻辑使用 `kind`，界面展示 `message`。

| `kind` | 含义 |
| --- | --- |
| `not_supported` | 硬件、驱动或接口不支持 |
| `permission_denied` | 查询权限不足 |
| `device_lost` | 设备失联或需要重置 |
| `uninitialized` | 驱动 / NVML 会话或动态库不可用 |
| `no_devices` | 未发现 NVIDIA 设备 |
| `unknown` | 未归入上述类别的错误 |

`DeviceFailure` 为 `{ index, uuid, error }`，身份尚未知时 `uuid: null`。`MetricIssue` 为 `{ metric, error }`，`metric` 通常为不可用字段名，例如 `temperature`、`memory`、`power_limit`。进程查询使用 `processes_compute` / `processes_graphics`，未报告的进程显存使用 `processes.<pid>.gpu_memory`。

`error: null` 仅表示没有全局错误，仍需检查设备失败、指标问题和空值。某类进程查询失败时，可继续使用另一类的成功结果，但不能把空数组解释为没有 GPU 进程。

## 历史、告警与录制状态

`HistoryResponse` 包含 `window_ms`、`interval_ms` 和 `frames`。每个 `HistoryFrame` 包含 `sampled_at_ms` 和 `gpus`，每个历史 GPU 只保留 `uuid`、`index`、`gpu_utilization`、`memory_percent`、`temperature`、`power_watts`。这里的功耗单位是 W，和原始快照的 mW 不同。GPU 缺失或 nullable 指标表示数据缺口；空帧也保留。

`AlertConfig` 字段为 `enabled`、`temperature_threshold`、`temperature_recovery`、`memory_threshold`、`memory_recovery`、`duration_ms`、`cooldown_ms`。温度用 °C，显存用容量百分比，持续 / 冷却用毫秒。

`AlertEvent` 包含 `id`、`at_ms`、`gpu_uuid`、`kind`、`state`、`message`、`value`。种类为 `temperature`、`memory`、`device_unavailable`、`monitor_unavailable`，状态为 `firing` 或 `recovered`。全局事件的 `gpu_uuid` 为 `null`，身份未确定的设备事件可使用临时 `index:N` 标识。`id` 在当前运行时会话中递增。

`RecordingStatus` 包含：

| 字段 | 含义 |
| --- | --- |
| `active` | 是否接受新的采样帧 |
| `finishing` | 停止后是否仍在排空写入队列 |
| `path` | 当前 / 最近录制路径或 `null` |
| `samples_written`、`bytes_written` | 已成功写入的帧数和字节数 |
| `dropped_samples` | 未能完整写入的帧数，包括队列满、提交繁忙或写入失败时丢弃的帧 |
| `error` | 写入失败或限制原因，正常时为 `null` |

停止请求返回不等于文件已经完成，应等待 `finishing: false` 并检查 `error`、`dropped_samples`。文件及内存限制见[系统架构](architecture.md)。

## GUI IPC

| 命令 | 参数 | 返回 / 行为 |
| --- | --- | --- |
| `get_gpu_info` | 无 | 最近完成的 `MonitorSnapshot`，无缓存时拒绝 |
| `retry_gpu_monitor` | 无 | 提交并合并重试请求，成功不表示已经恢复 |
| `get_gpu_count` | 无 | 缓存中成功设备和失败设备数量；全局错误时拒绝 |
| `is_gpu_available` | 无 | 缓存中是否存在可读 GPU，不执行硬件探测 |
| `get_monitor_tools` | `windowMs` | `{ history, events, alert_config, recording }` |
| `configure_alerts` | `config` | 验证并应用当前会话规则 |
| `start_recording` | `path`、`includeCommands` | 新建录制并返回状态 |
| `stop_recording` | 无 | 请求停止，返回可能仍为 finishing 的状态 |
| `load_recording` | `path` | 验证并返回录制快照数组 |
| `notify_event` | `eventId` | 将仍保留的事件发送到桌面通知服务 |

JavaScript 参数名使用上表的 camelCase，数据字段保持协议中的 snake_case。命令失败的拒绝值为 `{ "message": "具体原因" }`；这与成功返回的快照内包含采样错误是两种情况。

## CLI 退出语义

| 情况 | 行为 |
| --- | --- |
| 单次查询有匹配设备，可能存在局部指标 / 部分设备失败 | 输出结果，退出码 0 |
| 全局错误、全部选中设备失败或设备筛选无匹配 | 输出可取得的快照，再返回非零退出码 |
| 只有进程筛选结果为空，设备正常 | 输出空进程列表，退出码 0 |
| 参数非法 | 参数解析失败，非零退出码 |
| 录制无法创建、写入失败、丢帧、零帧或收尾超时 | 非零退出码，保留已有文件用于检查 |
| 回放格式非法、版本不支持或文件超过限制 | 拒绝回放，非零退出码 |

JSON 标准输出仅包含数据，诊断信息写入标准错误。连续采样遇到驱动错误时继续输出错误快照并重试。回放文件按采集顺序处理，JSON 回放不会重新计算告警或触发桌面通知。
