# 开发指南

[项目首页](../README.md) · [功能与使用](features.md) · [系统架构](architecture.md) · [数据接口](data-model.md)

## 环境与依赖

使用当前稳定版 Rust、rustfmt；GUI 另需 Node.js 22+ 和 npm。原生 GTK / WebKit 依赖见项目首页。Tauri CLI 固定为 `2.11.3`，与流水线一致。

开发启动器需要 Bash 和 Linux util-linux 提供的 `setsid`，启动器测试使用 Python 3。NVIDIA 硬件仅用于实时采集；模拟采样、回放、协议和组件测试无需真实 GPU。

仓库提交 `Cargo.lock` 和前端 `package-lock.json`。使用 `cargo ... --locked` 和 `npm ci` 复现依赖；增加依赖后同步更新对应锁文件。

## 本地运行

以下命令从仓库根目录执行：

```sh
cargo run --locked -p gpu-monitor-cli -- --watch --alerts
cargo run --locked -p gpu-monitor-cli -- --json
cargo run --locked -p gpu-monitor-cli -- replay session.jsonl --json --speed 1000
```

GUI 开发：

```sh
cargo install tauri-cli --version 2.11.3 --locked
./crates/gpu-monitor-gui/dev.sh
```

启动器根据自身路径定位 GUI crate，支持从仓库之外调用。缺少 `node_modules` 时先执行 `npm ci`，之后启动 Tauri。Tauri 的 `beforeDevCommand` 在 `src-web` 中启动 Vite，地址为 `127.0.0.1:5173`；端口冲突立即失败，不结束原监听者。

安装和 Tauri 命令各在独立进程组内运行。正常退出、Ctrl-C 或 SIGTERM 时，启动器对自己创建的组先发送 TERM，最多等待约一秒，再结束未退出的子进程。

单独启动前端：

```sh
npm --prefix crates/gpu-monitor-gui/src-web ci
npm --prefix crates/gpu-monitor-gui/src-web run dev
```

普通浏览器没有 Tauri IPC，不能直接进行实时硬件采样。前端测试在 IPC 边界提供模拟数据；验证文件访问、桌面通知和真实原生窗口时使用 Tauri。

## 自动检查

无需原生 GUI 构建依赖的 Rust 检查：

```sh
cargo fmt --all -- --check
cargo test --locked -p gpu-monitor-core -p gpu-monitor-runtime -p gpu-monitor-cli
```

安装 GTK / WebKit 依赖后检查整个 workspace：

```sh
cargo test --workspace --locked
```

前端与开发启动器：

```sh
npm --prefix crates/gpu-monitor-gui/src-web ci
npm --prefix crates/gpu-monitor-gui/src-web test
npm --prefix crates/gpu-monitor-gui/src-web run build
python3 -m unittest discover -s tests -v
```

| 范围 | 测试入口与重点 |
| --- | --- |
| core | 可注入采集后端、工厂和时钟；驱动错误、空值、元数据边界、PID 重用与退避 |
| runtime | 受控线程和通道；缓存响应、时间历史、队列、录制限额、回放验证、告警状态 |
| CLI | Ratatui TestBackend、状态测试和实际可执行文件；筛选、排序、时间桶、回放、命令隐私、退出语义 |
| React | Vitest 与 Testing Library；轮询、历史、排序、跨卡进程、回放定位、告警和录制控件 |
| 启动器 | 临时目录及替代子进程；安装中断、路径参数、组清理与无关进程存活 |

驱动兼容性、实际采集成本、原生 WebKit 渲染和桌面通知还应在目标机器上验证。录制错误或丢帧是可观察状态，测试时应断言结果，不仅检查是否创建了文件。

## 生成 TypeScript 协议

共享数据模型使用 `ts-rs` 生成前端 wire 类型：

```sh
cargo run --locked -p gpu-monitor-core --features typescript --example generate_types
cargo run --locked -p gpu-monitor-runtime --features typescript --example generate_runtime_types
```

产物分别为：

- `crates/gpu-monitor-gui/src-web/src/monitor/wire.ts`：快照、设备、指标、进程和错误。
- `crates/gpu-monitor-gui/src-web/src/monitor/runtime-wire.ts`：历史、告警、录制状态。

不直接编辑生成文件。在 Rust 中修改字段后运行生成器，再更新选择器、输出、展示和对应文档。只检查是否同步：

```sh
cargo run --locked -p gpu-monitor-core --features typescript --example generate_types -- --check
cargo run --locked -p gpu-monitor-runtime --features typescript --example generate_runtime_types -- --check
```

CI 使用同样的 `--check`，接口漂移会直接失败。新增字段要考虑 serde 默认值、旧录制读取、nullable 语义、单位和 schema 兼容性；无法兼容的协议变更应使用明确版本。

## 构建和产物

CLI：

```sh
cargo build --locked --release --bin gpu-monitor
```

GUI 发布包与本地 Debian 调试包：

```sh
(cd crates/gpu-monitor-gui && cargo tauri build --ci -- --locked)
(cd crates/gpu-monitor-gui && cargo tauri build --debug --bundles deb --ci -- --locked)
```

也可不安装 Cargo 版 Tauri CLI，使用流水线相同的固定 npm 包：

```sh
(cd crates/gpu-monitor-gui && npx --yes --package @tauri-apps/cli@2.11.3 tauri build --debug --bundles deb --ci -- --locked)
```

`beforeBuildCommand` 自动运行 TypeScript 检查和 Vite 构建。默认产物位于 workspace 根目录：

| 产物 | 路径 |
| --- | --- |
| CLI 发布程序 | `target/release/gpu-monitor` |
| GUI Debian 包 | `target/release/bundle/deb/*.deb` |
| GUI AppImage | `target/release/bundle/appimage/*.AppImage` |
| GUI Debian 调试包 | `target/debug/bundle/deb/*.deb` |

[CI](../.github/workflows/ci.yml) 检查格式、协议、Rust、前端、启动器，并通过真实 Tauri 构建前置命令生成 Debian 调试包。预期产物缺失会使工作流失败。

[发布流程](../.github/workflows/release.yml) 构建 CLI、Debian 和 AppImage；`v*` 标签触发发布，手动运行用于构建产物。发布包的 Linux 运行依赖由 Tauri 打包器依据构建环境处理。

## 扩展约定

新增硬件指标应在 NVML 适配边界保留失败原因，通过共享 nullable 字段表达不可用状态。不要使用真实零值代表查询失败，也不要把进程元数据错误升级为整张卡失效。

新的展示或筛选逻辑复用快照与 UUID 状态。硬件读取只属于采样器，录制文件写入只属于录制器；视图不自行创建 NVML 会话或独立采集历史。

时间逻辑区分墙上时间与单调计时。缓存读取必须保持有界，不持锁调用驱动、解析不受限文件或等待外部通知服务。对队列和文件使用明确限额，并通过状态告知丢帧、失败及收尾状态。

终端文本输出需清除录制和进程字符串中的控制字符，JSON 保留原始结构并依赖序列化转义。完整进程参数的公开输出或磁盘写入需要显式选项，默认路径应保持脱敏。
