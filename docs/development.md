# 开发指南

[项目首页](../README.md) · [功能设计](features.md) · [系统架构](architecture.md) · [数据接口](data-model.md)

## 开发环境

- 当前稳定版 Rust，包含 Cargo 和 rustfmt。
- GUI 前端使用 Node.js 22+、npm；原生 GUI 构建依赖见[项目首页](../README.md)。
- GUI 开发命令使用 Tauri CLI 2.11.3，与流水线保持一致。
- 开发启动器需要 Bash 和 `setsid`（Linux util-linux）；启动器测试需要 Python 3。

`Cargo.lock` 和 `src-web/package-lock.json` 保存依赖解析结果。Rust 构建使用 `--locked`，前端安装使用 `npm ci`。

## 本地运行

以下命令从仓库根目录执行。

### CLI

```sh
cargo run --locked -p gpu-monitor-cli -- --watch
cargo run --locked -p gpu-monitor-cli -- --json
```

### GUI

```sh
cargo install tauri-cli --version 2.11.3 --locked
./crates/gpu-monitor-gui/dev.sh
```

启动器首次运行时安装缺失的前端依赖。它根据脚本自身位置解析路径，因此也可从其他目录使用脚本的绝对路径启动。

Tauri 的 `beforeDevCommand` 在 `src-web` 中启动 Vite，监听 `127.0.0.1:5173`。端口已被占用时启动失败，需要先自行处理冲突；启动器不会结束已有的监听进程。

依赖安装与 Tauri 运行都由启动器在独立进程组中执行。退出、Ctrl-C 或 SIGTERM 时只清理启动器创建的子树；先发送 TERM，等待最多约 1 秒，再以 KILL 清理仍在运行的进程。Tauri 负责前端就绪等待和正常启动流程，启动器负责整体进程生命周期。

只运行前端开发服务器也可使用：

```sh
npm --prefix crates/gpu-monitor-gui/src-web ci
npm --prefix crates/gpu-monitor-gui/src-web run dev
```

普通浏览器不提供 Tauri IPC，无法直接采集 GPU；组件测试通过模拟 IPC 提供数据。完整监控界面应通过桌面应用运行。

## 测试

### Rust

```sh
cargo fmt --all -- --check
cargo test --locked -p gpu-monitor-core -p gpu-monitor-cli
```

安装原生 GUI 依赖后，可测试整个 workspace：

```sh
cargo test --workspace --locked
```

### 前端与启动器

```sh
npm --prefix crates/gpu-monitor-gui/src-web ci
npm --prefix crates/gpu-monitor-gui/src-web test
npm --prefix crates/gpu-monitor-gui/src-web run build
python3 -m unittest discover -s tests -v
```

| 范围 | 测试方式 | 关注点 |
| --- | --- | --- |
| core | 可注入的后端、工厂与时钟 | 错误分层、未知值、进程合并、初始化和重试 |
| TUI | Ratatui TestBackend 与状态操作 | 多卡导航、独立滚动、窗口缩放、历史显示 |
| GUI Rust | 受控线程与通道 | 采样阻塞时缓存仍可读、重试合并、退出不等待驱动 |
| React | Vitest 与 Testing Library | 采样去重、时间窗口、错误恢复、详情、焦点及轮询生命周期 |
| 开发启动器 | 临时目录和替代子进程 | 路径与参数、退出码、安装中断、子树清理和无关进程存活 |

这些自动测试无需 NVIDIA GPU 或桌面显示服务。驱动兼容性、真实采集开销及平台渲染行为需要在相应硬件与桌面环境中验证。

## 打包与发布

构建发布包：

```sh
(cd crates/gpu-monitor-gui && cargo tauri build --ci -- --locked)
```

构建用于本地检查的 Debian 调试包：

```sh
(cd crates/gpu-monitor-gui && cargo tauri build --debug --bundles deb --ci -- --locked)
```

`beforeBuildCommand` 自动执行前端类型检查和 Vite 构建，再由 Tauri 嵌入前端资源。默认产物目录以 workspace 根目录为基准：

| 产物 | 路径 |
| --- | --- |
| CLI 发布程序 | `target/release/gpu-monitor` |
| GUI Debian 发布包 | `target/release/bundle/deb/*.deb` |
| GUI AppImage | `target/release/bundle/appimage/*.AppImage` |
| GUI Debian 调试包 | `target/debug/bundle/deb/*.deb` |

[CI](../.github/workflows/ci.yml) 执行启动器、前端、Rust 测试和格式检查，并构建 Debian 调试包以检查完整构建链路。

[发布工作流](../.github/workflows/release.yml) 分别测试和构建 CLI、GUI，要求预期安装包存在后上传产物。`v*` 标签触发 GitHub Release，附带 CLI、Debian 和 AppImage 文件；手动触发工作流用于构建产物。

## 扩展约定

新增指标时，在 NVML 适配器边界处理驱动错误，并在共享模型中表达值是否可用；不要将查询失败转换为有效零。同步更新 Rust 类型、前端类型、JSON 字段说明以及相关测试。

新增设备视图时复用现有快照与 UUID 状态。筛选、排序或详情组件不应自行初始化 NVML，也不应各自维护同一设备的独立采集历史。

修改采样调度时区分单调时钟和展示时间戳，保留可注入测试依赖。GUI IPC 不执行硬件采集，缓存锁不覆盖驱动调用；TUI 的同步采样模式与 GUI 的后台线程模式在[系统架构](architecture.md)中分别说明。
