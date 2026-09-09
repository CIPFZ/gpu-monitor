# GPU Monitor

面向 Linux 的 NVIDIA GPU 资源监控工具，支持多张物理 GPU。提供 Rust 终端界面、Tauri + React 桌面界面，以及供脚本使用的 JSON 输出。

## 功能概览

| 功能 | 用途 |
| --- | --- |
| 实时指标 | 查看 GPU 负载、显存容量、温度、功耗、风扇、时钟和编解码器忙碌率 |
| 多 GPU 监控 | 桌面端总览与逐卡详情，终端端逐卡切换 |
| 历史曲线 | 观察负载和显存变化，区分有效数据与缺测 |
| 进程查询 | 查看进程显存、PID 和类型；桌面端支持按名称或 PID 搜索 |
| 异常状态与恢复 | 保留健康设备的数据，显示不可用指标，并自动重试初始化 |
| JSON 输出 | 获取单次快照或连续 JSON Lines 数据流 |

功能的使用方式、设计和适用场景见[功能设计](docs/features.md)。

## 运行与构建要求

- 实时采集需要 Linux、NVIDIA GPU 和提供 NVML 的 NVIDIA 驱动。
- 从源码构建需要当前稳定版 Rust 工具链。
- 构建桌面端另需 Node.js 22+、npm 和 Linux 原生 GUI 依赖；CLI 无需 Node.js。
- 当前监控本机物理 GPU，未实现 AMD/Intel、MIG 实例枚举、远程监控或历史持久化。

## 快速开始

以下命令从仓库根目录执行。

### 终端端

```sh
cargo install --locked --path crates/gpu-monitor-cli
gpu-monitor --watch
```

使用左右方向键或 Tab 切换 GPU，上下方向键滚动进程，`q` 退出。

```sh
gpu-monitor --once                    # 单次可读快照
gpu-monitor --json                    # 单次 JSON 快照
gpu-monitor --watch --json            # 连续 JSON Lines
gpu-monitor --watch --interval 500    # 刷新间隔，单位毫秒
gpu-monitor processes                # 单次进程列表
gpu-monitor --json processes          # 带设备归属的 JSON 进程列表
```

### 桌面端

Ubuntu/Debian 原生构建依赖：

```sh
sudo apt-get update
sudo apt-get install -y build-essential libwebkit2gtk-4.1-dev libssl-dev libgtk-3-dev libayatana-appindicator3-dev librsvg2-dev
```

安装工具并构建：

```sh
npm --prefix crates/gpu-monitor-gui/src-web ci
cargo install tauri-cli --version 2.11.3 --locked
(cd crates/gpu-monitor-gui && cargo tauri build --ci -- --locked)
```

Tauri 自动构建前端。默认产物位于仓库根目录的 `target/release/bundle/`：

- Debian 安装包：`deb/*.deb`
- AppImage：`appimage/*.AppImage`

```sh
sudo dpkg -i target/release/bundle/deb/*.deb
gpu-monitor-gui
```

安装后也可从应用菜单启动。开发启动、测试和发布配置见[开发指南](docs/development.md)。

## 文档

| 文档 | 内容 |
| --- | --- |
| [功能设计](docs/features.md) | 各功能解决的问题、界面行为与设计方式 |
| [系统架构](docs/architecture.md) | 模块职责、数据流、采样生命周期和并发模型 |
| [数据接口](docs/data-model.md) | 快照结构、指标单位、错误语义和 JSON 使用方式 |
| [开发指南](docs/development.md) | 本地开发、测试、打包与扩展约定 |

## 许可证

MIT。
