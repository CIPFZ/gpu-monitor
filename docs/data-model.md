# 数据接口

[项目首页](../README.md) · [功能设计](features.md) · [系统架构](architecture.md) · [开发指南](development.md)

CLI 的 JSON 输出和 GUI 的快照 IPC 使用相同的 Rust 数据模型。定义分别位于 [core 类型](../crates/gpu-monitor-core/src/lib.rs)与[前端类型](../crates/gpu-monitor-gui/src-web/src/monitor/models.ts)。

## 快照结构

`gpu-monitor --json` 输出一个 `MonitorSnapshot` 对象。`gpu-monitor --watch --json` 每行输出一个相同结构的对象，可按 JSON Lines 逐行解析。

| 字段 | 类型 | 含义 |
| --- | --- | --- |
| `sampled_at_ms` | 整数 | 本轮采样开始的 Unix 时间，单位毫秒 |
| `gpus` | `GpuInfo[]` | 身份信息可读的设备，内部指标仍可能部分不可用 |
| `failures` | `DeviceFailure[]` | 无法查询必要身份信息的设备 |
| `error` | `SampleError` 或 `null` | 初始化、枚举或未发现设备等全局错误 |

以下示例表示 GPU 0 可读取，但风扇指标不支持；GPU 1 不可访问：

```json
{
  "sampled_at_ms": 1780000000000,
  "gpus": [
    {
      "device": {
        "index": 0,
        "name": "NVIDIA GPU",
        "uuid": "GPU-example-0",
        "pci_bus_id": "0000:01:00.0",
        "driver_version": "570.0",
        "cuda_version": "12.8",
        "power_limit": 300,
        "power_limit_max": 350
      },
      "metrics": {
        "gpu_utilization": 50,
        "memory_utilization": 20,
        "encoder_utilization": 0,
        "decoder_utilization": 0,
        "temperature": 60,
        "power_usage": 120000,
        "fan_speed": null,
        "clock_graphics": 1400,
        "clock_memory": 1200,
        "clock_sm": 1400
      },
      "memory": { "total": 8589934592, "used": 2147483648, "free": 6442450944 },
      "processes": [],
      "sampled_at_ms": 1780000000000,
      "issues": [
        {
          "metric": "fan_speed",
          "error": { "kind": "not_supported", "message": "Fan speed is not supported" }
        }
      ]
    }
  ],
  "failures": [
    {
      "index": 1,
      "uuid": null,
      "error": { "kind": "device_lost", "message": "Device is inaccessible" }
    }
  ],
  "error": null
}
```

`error: null` 仅表示没有全局错误。消费者仍需检查 `failures`、每卡 `issues` 和字段是否为 `null`。GUI 重复读取同一缓存快照时，时间戳保持不变。

## 设备与指标

`GpuInfo` 包含 `device`、`metrics`、`memory`、`processes`、`sampled_at_ms` 和 `issues`。

### DeviceInfo

| 字段 | 类型 | 含义 |
| --- | --- | --- |
| `index` | 整数 | 当前枚举索引，从 0 开始 |
| `name` | 字符串 | 设备名称 |
| `uuid` | 字符串 | 跨采样关联设备的标识 |
| `pci_bus_id` | 字符串 | PCI 地址 |
| `driver_version` | 字符串 | NVIDIA 驱动版本 |
| `cuda_version` | 字符串或 `null` | 驱动支持的 CUDA 版本，不用于判断已安装的 CUDA Toolkit 版本 |
| `power_limit` | 整数或 `null` | 当前功耗上限，W |
| `power_limit_max` | 整数或 `null` | 可配置功耗上限的最大值，W |

### GpuMetrics

下列字段均为整数或 `null`。数值 `0` 表示有效读数，`null` 表示无法取得数值。

| 字段 | 单位 | 含义 |
| --- | --- | --- |
| `gpu_utilization` | % | GPU 忙碌率 |
| `memory_utilization` | % | 显存读写忙碌率 |
| `encoder_utilization` | % | 编码器忙碌率 |
| `decoder_utilization` | % | 解码器忙碌率 |
| `temperature` | °C | GPU 温度 |
| `power_usage` | mW | 当前功耗；换算为 W 时除以 1000 |
| `fan_speed` | % | NVML 风扇索引 0 的速度比例 |
| `clock_graphics` | MHz | 图形时钟 |
| `clock_memory` | MHz | 显存时钟 |
| `clock_sm` | MHz | SM 时钟 |

### MemoryInfo

`memory` 为 `{ total, used, free }` 或 `null`，三个字段的单位都是字节。显存容量占用比例为 `used / total × 100`；计算前应检查对象可用且 `total > 0`。

显示单位使用二进制换算：`1 MiB = 1024²` 字节，`1 GiB = 1024³` 字节。

## 进程数据

`GpuProcess` 归属于外层 GPU，包含：

| 字段 | 类型 | 含义 |
| --- | --- | --- |
| `pid` | 整数 | 进程 ID |
| `name` | 字符串 | `/proc/<pid>/comm` 中的名称，无法读取时为 `unknown` |
| `gpu_memory` | 整数或 `null` | 此进程在该 GPU 上的显存用量，字节 |
| `process_type` | 字符串 | `Graphics`、`Compute`、`Mixed` 或 `Unknown` |

进程在同一张 GPU 内按 PID 合并；同一 PID 可同时出现在多张 GPU 上。不要跨 GPU 将它们视为重复条目。

`gpu-monitor --json processes` 保留快照顶层的四个字段，但 `gpus` 内每个对象精简为：

- `device`：仅包含 `index`、`uuid`、`name`。
- `sampled_at_ms`：该 GPU 的采样时间。
- `processes`：上述进程字段，以及额外的 `gpu_memory_mib`，按整 MiB 向下取整；未知时为 `null`。
- `issues`：该 GPU 的采样问题，包括进程查询问题。

## 错误数据

`SampleError` 为 `{ kind, message }`，其中 `kind` 用于程序判断，`message` 用于展示具体原因。

| `kind` | 含义 |
| --- | --- |
| `not_supported` | 硬件、驱动或接口不支持该查询 |
| `permission_denied` | 无法获得查询所需权限 |
| `device_lost` | 设备失联或需要重置 |
| `uninitialized` | NVML 会话不可用、驱动未加载或库加载失败等 |
| `no_devices` | 未发现 NVIDIA 设备 |
| `unknown` | 无法归入上述类别的错误 |

`DeviceFailure` 为 `{ index, uuid, error }`。UUID 尚无法取得时为 `null`，索引仍用于定位当次查询的设备。

`MetricIssue` 为 `{ metric, error }`，常见 `metric`：

| 值 | 对应问题 |
| --- | --- |
| 指标字段名，例如 `temperature`、`power_limit` | 对应字段不可用 |
| `memory` | 整体显存信息不可用 |
| `processes_compute` / `processes_graphics` | 计算 / 图形进程查询失败，列表可能不完整 |
| `processes.<pid>.gpu_memory` | 驱动未报告该进程的显存用量 |

若计算和图形查询都成功且返回空列表，才可将其理解为未发现 GPU 进程。某一查询失败时，空列表无法证明没有进程。

## CLI 输出与退出码

单次 JSON 命令输出格式化对象；连续模式每次输出一行紧凑 JSON。程序日志和错误诊断写入标准错误，不混入 JSON 标准输出。

| 情况 | 单次命令行为 |
| --- | --- |
| 有可查询设备，包括局部指标或部分设备失败 | 输出快照，退出码 0 |
| 全局采样错误或全部设备身份查询失败 | 先输出包含错误的快照，再以非零退出码退出 |
| 参数不合法 | 参数解析阶段以非零退出码退出 |

`--watch --json` 在采集错误后继续采样并输出错误快照。`processes` 子命令始终执行一次查询；`--once` 与 `--watch` 同时出现时使用单次模式。

## GUI IPC

| 命令 | 返回值 | 行为 |
| --- | --- | --- |
| `get_gpu_info` | `MonitorSnapshot` | 返回最近完成的采样；首次尚无结果时返回错误 |
| `retry_gpu_monitor` | 空成功值 | 提交重新初始化请求；相邻请求合并，返回成功不表示已经恢复 |
| `get_gpu_count` | 整数 | 根据缓存中 `gpus` 和 `failures` 的数量计算；缓存包含全局错误时返回错误 |
| `is_gpu_available` | 布尔值 | 缓存中是否至少有一个 `gpus` 条目，不触发硬件探测 |

上述 Rust 命令返回 `CommandError` 时，拒绝值为 `{ "message": "具体原因" }`。应区分命令错误与成功返回的快照内的采样错误。判断实时状态时使用快照时间戳及错误字段，不能只依赖设备数量或 `is_gpu_available`。
