# 功能与使用

[项目首页](../README.md) · [系统架构](architecture.md) · [数据接口](data-model.md) · [开发指南](development.md)

## GPU 资源与设备诊断

两种界面共用 NVML 采样服务，显示 GPU 忙碌率、显存总量 / 已用 / 空闲、温度、功耗、风扇和图形 / SM / 显存时钟。设备详情还提供驱动支持的 CUDA 版本、UUID、PCI 地址、功耗上限、性能状态、降频原因、PCIe 代数 / 宽度及收发吞吐。

指标是否可用取决于设备、驱动和权限。不支持或读取失败的字段显示 `N/A`，并保留具体原因；有效的零读数仍显示为零。空降频原因列表表示没有已报告的原因，`null` 表示无法读取。

显存容量与活动程度是不同指标：`used / total` 判断容量余量，`memory_utilization` 表示显存控制器忙碌率。分配了大量显存的进程也可能暂时没有计算活动。

## 桌面视图

工具栏中的 View 可选择 Cards、GPU table 或 All processes。卡片和表格显示设备身份、利用率、显存容量及主要状态；View Details 打开任意设备的完整信息。只有一张卡时默认进入同一详情视图。

设备筛选支持名称、索引或 UUID 文本匹配。Minimum free GiB 只接受具有足够已知空闲显存的新鲜设备；未知和过期数据不能证明资源可用。GPU 可按索引升序，或按空闲显存、负载、温度降序排列，未知值置于末尾。

All processes 将同一进程在不同 GPU 上的分配列在一起，按 PID 和启动时间识别进程，避免 PID 重用造成错误合并。无法取得启动时间时保留每卡独立条目。各进程视图支持名称 / PID 搜索和精确用户名 / UID 筛选，并显示用户名、运行时长、每卡显存和状态。Show command 可展开参数向量。

View Processes 弹窗支持键盘操作，Escape 关闭并恢复打开前的焦点。查询不完整时显示已取得的进程及原因，空列表不会被误报为没有进程。

## CLI 筛选与输出

```sh
gpu-monitor --gpu GPU-first --gpu GPU-second --sort utilization
gpu-monitor --min-free-gib 16 --sort free-memory --json
gpu-monitor --user 1000 processes
gpu-monitor processes --watch --json --user alice
```

| 参数 | 行为 |
| --- | --- |
| `--gpu UUID` | 精确选择 UUID，可重复指定 |
| `--min-free-gib N` | 选择已知空闲显存至少为 N GiB 的设备；N 必须有限且非负 |
| `--user OWNER` | 按精确用户名或数字 UID 筛选进程，不删除其所属 GPU |
| `--sort index` | 按索引升序 |
| `--sort free-memory/utilization/temperature` | 按指定指标降序，未知值在末尾；相同值按索引、UUID 排序 |
| `--include-command` | 在输出和录制中保留完整参数；默认 `command: null` |
| `--history-seconds 60/300/3600` | 选择 TUI 历史窗口 |
| `--interval MS` | 采样周期，默认 1000 毫秒，最小 100 毫秒 |
| `--alerts` | 启用当前实时会话的默认告警规则 |

筛选不丢弃无法确认归属的设备失败，避免将未取得身份的故障静默隐藏。单次设备筛选无匹配时仍输出空快照，并返回非零退出码。只有进程筛选为空且设备可用时，查询仍成功。

`--once` 输出单次文本，`--json` 默认输出单次对象；`--watch --json` 连续输出 JSON Lines。`processes` 默认单次，配合 `--watch --json` 连续输出进程快照。`--once` 与 `--watch` 不能同时指定。`diagnostics` 输出设备身份、驱动、扩展指标和不可用原因，支持设备筛选和 JSON。

## TUI 导航

TUI 从后台缓存读取数据，不在输入循环中调用 NVML。驱动缓慢时仍可切换视图、查看最后一次成功数据或退出。

| 按键 | 操作 |
| --- | --- |
| Right、Tab、`]` | 下一张 GPU |
| Left、Shift+Tab、`[` | 上一张 GPU |
| Up / Down、`k` / `j` | 详情中滚动进程；总览中选择 GPU |
| PageUp / PageDown、Home / End | 浏览详情进程或诊断 / 事件文本 |
| `t` | 切换多卡紧凑总览 |
| Enter | 从总览进入当前 GPU 的详情 |
| `d` | 打开 / 关闭诊断 |
| `a` | 打开 / 关闭告警事件 |
| `r` | 请求重新初始化 |
| `q`、Escape、Ctrl-C | 退出 |

每张 GPU 保留自己的滚动位置。表格根据实际窗口高度调整滚动范围；较矮窗口先压缩曲线，低于 42 列 × 14 行时提示调整尺寸。总览会将选中的设备滚入可见区域。

## 时间历史与新鲜度

两种界面均支持 1 分钟、5 分钟和 1 小时窗口。后台记录每次实际采样，UI 读取频率和读数是否变化不会决定是否记录。历史以 UUID 关联，筛选和切换详情不会改变采样归属。

GUI 按采样时间绘图，缺失值或超出正常采样间隔的间断会打断连线。TUI 将固定时间窗口映射为等时长的列，聚合时保留峰值，包含缺测的桶显示 `×`。宽窗口按采样周期覆盖已知值，避免将同一采样周期内没有额外采样的位置误标成缺测。

历史最多保留一小时，同时受帧数和总 GPU 数据点上限约束；高设备数量或高采样频率可能使可见历史短于一小时。运行时使用单调时钟限制保留时间。系统时钟回拨时继续接受新采样，展示历史从新时间段开始，避免把回拨前后的点连成错误曲线。

Connecting 表示首次数据仍在等待，Live 表示当前快照新鲜，Stale 表示保留值已经过期或设备 / 服务失败，Offline 表示没有可展示的成功数据。设备卡片提供最后采样时间；Live 不意味着设备空闲或所有指标均受支持。

持续监控会在初始化和枚举失败后自动退避重试。Retry 或 `r` 可请求提前重试，但不能强制中断已经阻塞的 NVML 调用。

## 录制、回放与快照导出

```sh
gpu-monitor --interval 500 record session.jsonl --duration 120
gpu-monitor record session-with-arguments.jsonl --include-command
gpu-monitor replay session.jsonl --speed 2
gpu-monitor replay session.jsonl --json --speed 1000 --gpu GPU-first
gpu-monitor replay session.jsonl --once --json
```

录制包含完整设备会话，不受 GPU、显存或进程显示筛选影响。若开始时已有缓存，先写入该快照，随后记录实际采样；每个文件只保存 JSON Lines，不保存界面状态。未指定 `--duration` 时持续录制，Ctrl-C 或 SIGTERM 请求停止并排空写入队列。停止后若写入超时、失败、丢帧或没有任何样本，CLI 返回非零退出码。

桌面端在 Recording, replay and export 中输入路径，使用 Start recording / Stop recording 控制文件写入。Finishing 表示写入队列尚未排空，此时不能开始下一段录制。关闭应用时最多等待五秒处理录制收尾，写入不完整会写入标准错误并以失败状态退出，不等待阻塞中的驱动调用。

文件以新建模式打开，不覆盖已有路径，在 Linux 上使用仅当前用户读写的权限。单文件上限为 64 MiB、24 小时或 86,400 帧，单帧上限为 4 MiB；后台写入队列满时记录丢帧数量，不阻塞硬件采样。到达限制或写入失败会在录制状态中显示原因。

CLI 回放无需 NVML，按文件中的采集顺序播放，`--speed` 调整时间间隔。系统时间回拨的相邻帧立即衔接，不丢弃该帧。TUI 播放结束后保留最后一帧供检查，`q` 退出；JSON 回放输出所有帧后结束。`--once` 在验证整个文件后只输出第一帧。

桌面端 Open replay 打开同样的录制文件，支持播放 / 暂停、0.5× 至 8× 速度和按帧定位。Return to live 返回当前实时状态。回放期间实时后台会话仍存在，已有实时录制继续进行；回放不会把历史数据送入实时告警评估。

Export snapshot 导出当前数据状态的 JSON 对象。默认删除完整进程参数，勾选对应选项才保留。导出与录制包含数据状态而非当前筛选后的屏幕列表，保留故障信息。单对象快照导出与逐行快照录制的文件格式不同。

## 告警与通知

共享规则的默认值为：温度达到 85°C、显存容量占用达到 95%，持续十秒后触发；温度降至 80°C 或显存占用降至 90% 后恢复，同一阈值重复触发的冷却时间为六十秒。设备失联和监控不可用分别记录触发 / 恢复事件。

CLI 普通实时会话使用 `--alerts` 启用规则，`a` 查看事件。`alerts` 子命令始终启用告警，可调整温度、显存、恢复阈值、持续时间和冷却时间，并以文本或 JSON Lines 输出事件：

```sh
gpu-monitor alerts --temperature 90 --temperature-recovery 85 --duration-seconds 15 --json
```

GUI 默认启用监控告警；在 Alerts and events 中更改规则并 Apply alert rules，或取消启用。恢复阈值必须低于触发阈值，输入必须有限；温度上限 200°C，显存比例上限 100%，持续时间最长一小时，冷却最长一天。

缺测或超过三倍采样周期的中断会重新计算尚未触发的持续条件，不会把“无数据”当成已经恢复。当前保留最近 500 条事件。规则与历史属于各自应用进程，不跨 CLI / GUI 共享，也不写入配置文件。

CLI 的 GPU UUID 筛选同时作用于事件，无法识别 UUID 的设备故障和全局事件继续保留；进程所有者和空闲显存筛选只作用于数据视图。GUI 事件面板展示当前实时会话事件。

Desktop notifications 默认关闭，开启后只发送新事件，依赖桌面会话的通知服务。通知失败显示具体错误；慢通知服务不会阻塞采样，队列有界，跳过的通知仍可从事件列表中查看。退出或切换到回放视图后停止发送后续通知；已经提交给系统的通知调用无法撤回。
