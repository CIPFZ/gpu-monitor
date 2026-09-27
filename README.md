# GPU Monitor

GPU Monitor 是面向 Linux NVIDIA GPU 的资源监控工具，提供 Rust 终端界面和 Tauri + React 桌面界面。它支持多 GPU、进程归属查询、时间历史、会话录制与回放、阈值告警及 JSON 输出。

采样结果保留真实零值、不可用指标和具体错误。单张 GPU 故障不阻止其他设备更新；驱动调用在后台执行，界面仍可响应导航和退出。

## 功能

- GPU 负载、显存容量、温度、功耗、风扇、时钟、性能状态、降频原因及 PCIe 链路与吞吐。
- 终端界面提供常驻设备侧栏与五个视图（仪表盘 / 进程 / 历史 / 诊断 / 告警），带阈值分级配色；桌面端支持跨 GPU 进程视图。
- 进程用户名 / UID、启动时间、运行时长、显存用量和可选完整参数；终端内可按名称 / PID / 用户过滤并切换排序列。
- 1 分钟、5 分钟、1 小时时间历史，可在运行中切换；负载、显存、温度、功耗四条曲线保留采样缺口和设备错误。
- JSON Lines 录制与按采集时间回放；桌面端支持暂停、速度调整、定位和快照导出。
- 温度、显存占用、设备和监控服务可用性告警；桌面通知需单独开启。

详细说明：[功能与使用](docs/features.md) · [数据接口](docs/data-model.md) · [系统架构](docs/architecture.md) · [开发指南](docs/development.md)

## 环境要求

- Linux；实时监控需要提供 NVML 的 NVIDIA 驱动。支持 `libnvidia-ml.so.1` 和未带版本号的库名。
- 当前稳定版 Rust。GUI 前端另需 Node.js 22+ 和 npm。
- Ubuntu / Debian 上构建 GUI 需要：

```sh
sudo apt-get update
sudo apt-get install -y build-essential libwebkit2gtk-4.1-dev libssl-dev libgtk-3-dev libayatana-appindicator3-dev librsvg2-dev
```

实时采样面向 NVIDIA 物理 GPU，不枚举 MIG 实例，不支持 AMD / Intel。录制回放和自动测试无需 NVIDIA 硬件。进程元数据受 `/proc` 权限影响；用户名仅从本地 `/etc/passwd` 解析，其他账户可使用数字 UID 筛选。

## 安装与运行

以下命令默认从仓库根目录执行。

### CLI

```sh
cargo install --locked --path crates/gpu-monitor-cli
gpu-monitor
```

不带子命令即进入终端界面。其余行为各有明确的子命令：

```sh
gpu-monitor watch --interval 500              # 终端界面
gpu-monitor snapshot                          # 单次文本报告
gpu-monitor snapshot --format json            # 单次 JSON 对象
gpu-monitor stream --format json              # 连续 JSON Lines
gpu-monitor processes --user alice             # 单次进程报告
gpu-monitor processes --stream --json --include-command
gpu-monitor diagnostics
gpu-monitor --gpu GPU-your-uuid --min-free-gib 8 --sort free-memory
gpu-monitor --history-seconds 3600 --alerts
```

`--format text|json`（`--json` 为简写）决定非交互输出格式。旧的 `--once` 与 `--watch` 仍被接受并映射到对应子命令，使用时会在标准错误提示替代写法。

界面按 `←` / `→` 切换 GPU，`Tab` 或 `1`–`5` 切换视图，`↑` / `↓` 滚动，`/` 过滤进程，`s` 切换排序，`c` 显示完整命令行，`w` 切换曲线窗口，`b` 收放设备侧栏，`r` 请求重试，`?` 查看完整按键表，`q` 或 Ctrl-C 退出。刷新间隔最小 100 毫秒。终端低于 48 列 × 12 行时提示调整尺寸；窄于 92 列时侧栏自动收起以保证详情区宽度。

### 录制、回放与告警

```sh
gpu-monitor record session.jsonl --duration 60
gpu-monitor replay session.jsonl --speed 2
gpu-monitor replay session.jsonl --json --speed 1000
gpu-monitor replay session.jsonl --first-frame --json --gpu GPU-your-uuid
gpu-monitor alerts --temperature 85 --temperature-recovery 80 --memory 95 --memory-recovery 90 --duration-seconds 10 --cooldown-seconds 60 --json
```

录制文件包含所有 GPU，视图筛选不改变录制内容。已有文件不会被覆盖。完整进程参数默认不写入 CLI 输出、录制或导出；需要时显式指定 `--include-command`，或勾选桌面端对应选项。

### 桌面应用

```sh
npm --prefix crates/gpu-monitor-gui/src-web ci
cargo install tauri-cli --version 2.11.3 --locked
(cd crates/gpu-monitor-gui && cargo tauri build --ci -- --locked)
```

Tauri 自动构建并嵌入前端。安装包位于仓库根目录的 `target/release/bundle/deb/` 和 `target/release/bundle/appimage/`。

```sh
sudo dpkg -i target/release/bundle/deb/*.deb
gpu-monitor-gui
```

开发时运行 `./crates/gpu-monitor-gui/dev.sh`。启动器管理自己的前端与 Tauri 子进程；端口 5173 已被占用时明确失败，不结束原有服务。

## 脚本接口

`snapshot --json` 输出一个快照对象，`stream --json` 和 JSON 回放逐行输出快照：

```json
{
  "schema_version": 1,
  "sampled_at_ms": 1780000000000,
  "gpus": [],
  "failures": [],
  "error": null
}
```

消费者应同时检查 `error`、`failures`、每卡 `issues` 和 nullable 指标。`memory_utilization` 表示显存控制器忙碌率；显存容量比例应使用 `memory.used / memory.total`。完整字段、单位、错误和退出码见[数据接口](docs/data-model.md)。

## 开发检查

```sh
cargo fmt --all -- --check
cargo test --locked -p gpu-monitor-core -p gpu-monitor-runtime -p gpu-monitor-cli
npm --prefix crates/gpu-monitor-gui/src-web ci
npm --prefix crates/gpu-monitor-gui/src-web test
npm --prefix crates/gpu-monitor-gui/src-web run build
python3 -m unittest discover -s tests -v
```

安装原生 GUI 依赖后可运行 `cargo test --workspace --locked`。类型生成、接口一致性检查和打包命令见[开发指南](docs/development.md)。

## License

MIT。
