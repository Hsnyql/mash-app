# Mash

**Mash** 是一款基于 Tauri 构建的开源跨平台《Fate/Grand Order》（FGO）自动化工具。

通过 ADB 连接 Android 模拟器，结合图像识别与自动化操作，实现队伍配置、助战选择、自动战斗等功能。

## 功能特性

- **自动战斗** — 根据预设策略自动执行战斗操作，包括技能释放、宝具使用和指令卡选择。
- **队伍配置** — 管理队伍、从者及相关战斗配置。
- **助战选择** — 根据指定条件自动识别并选择助战从者。
- **战斗策略** — 支持基于回合、战斗阶段及其他条件的自定义战斗规则。
- **搓丸子** — 自动执行概念礼装强化流程。

## 下载与安装

前往 [官网]([../../releases](https://docs.mash.xiaotongx.com/start/install/)) 下载适用于当前操作系统的最新版本。

### 系统要求

- Windows、macOS (M 系列芯片) 或 Linux x86_64（首版验证 Ubuntu 22.04/24.04、Debian 12）
- Android 模拟器
- ADB（Android Debug Bridge）

Mash 通过 ADB 与游戏设备通信，因此需要确保模拟器已开启 ADB 连接。

### 首次使用

1. 下载并安装 Mash。
2. 启动 Android 设备或模拟器，并运行 FGO。
3. 在 Mash 中连接设备并完成相关配置。
4. 配置自动化任务并开始运行。

Mash 的图像识别功能依赖独立的 CV  运行时。首次运行时，如果缺少所需组件，应用会提示安装。

## 技术架构

Mash 采用前后端分离的桌面应用架构，主要由以下部分组成：

| 模块 | 技术 | 职责 |
|------|------|------|
| 桌面应用 | Tauri 2 / Rust | 应用生命周期、设备通信与自动化执行 |
| 用户界面 | React / TypeScript / Vite | 配置管理与运行状态展示 |
| UI 组件 | Radix UI Themes | 基础界面组件 |
| 图像识别 | Python / OpenCV | 游戏画面识别与目标定位 |
| 视频采集 | scrcpy / PyAV | 实时获取并解码设备画面 |
| 设备控制 | ADB | 执行点击、滑动等操作 |

### 工作原理

Mash 通过以下流程完成自动化操作：

1. **设备连接**：使用 ADB 连接 Android 设备或模拟器。
2. **画面采集**：通过 scrcpy 获取实时画面，并使用 PyAV 解码。
3. **图像识别**：使用 OpenCV 模板匹配及 OCR 识别当前游戏状态。
4. **决策执行**：Rust 自动化引擎根据当前状态及用户配置决定下一步操作。
5. **设备操作**：通过 ADB 执行点击、滑动等操作。
6. **状态反馈**：将执行状态同步至前端界面。

自动化逻辑由 Rust 状态机驱动，图像识别则由独立的 Python 进程完成。

### CV 运行时

为了减小应用安装包及后续更新的体积，Mash 将图像识别运行时与主程序分开分发。

CV 组件由两部分组成：

- **Runtime**：包含 Python 运行环境、原生依赖、OCR 模型等。
- **Code**：包含图像识别相关的 Python 代码。

两者独立进行版本管理，使 Python 代码更新不必重新分发完整的运行环境。

详细信息请参阅 [分发与更新文档](docs/distribution.md)。

## 开发指南

### 环境要求

开发 Mash 需要以下工具：

- [Node.js](https://nodejs.org/)（LTS）
- [pnpm](https://pnpm.io/)
- [Rust](https://www.rust-lang.org/tools/install)（stable）
- [ADB](https://developer.android.com/tools/adb)

如果需要开发或构建图像识别组件，还需要：

- Python 3
- [Poetry](https://python-poetry.org/)

### 本地开发

克隆仓库：

```bash
git clone <repository-url>
cd mash
```

安装依赖：

```bash
pnpm install
```

启动 Tauri 开发环境：

```bash
pnpm tauri dev
```

仅启动前端开发服务器：

```bash
pnpm dev
```

### 构建应用

```bash
pnpm tauri build
```

图像识别组件需要单独构建。

macOS：

```bash
bash scripts/macos/build-cv.sh
```

Windows：

```powershell
./scripts/windows/build-cv.ps1
```

Linux x86_64：

```bash
bash scripts/linux/build-cv.sh
pnpm build:linux
```

有关 Windows 构建环境、安装包及签名的详细说明，请参阅 [Windows 构建文档](docs/windows.md)。
Linux 构建环境、ADB 和安装包说明请参阅 [Linux 构建文档](docs/linux.md)。

### 测试

前端代码检查：

```bash
pnpm lint
```

运行 Python 图像识别测试：

```bash
cd sidecar/mash_cv
poetry install
poetry run pytest
```

## 项目结构

```text
mash/
├── src/                # React 前端
│   ├── components/     # 通用 UI 组件
│   ├── features/       # 功能模块
│   ├── styles/         # 样式
│   └── types/          # 类型定义
│
├── src-tauri/          # Tauri / Rust 后端
│   ├── src/
│   │   ├── commands/   # Tauri 命令
│   │   ├── runner/     # 自动战斗引擎
│   │   ├── screen/     # 图像识别通信
│   │   ├── touch/      # 设备输入控制
│   │   └── adb.rs      # ADB 通信
│   └── resources/      # 应用资源与运行时配置
│
├── sidecar/            # Python 图像识别组件
├── scripts/            # 构建与发布脚本
├── docs/               # 项目文档
└── versions.toml       # 组件版本配置
```

模块划分与代码组织规范请参阅 [模块结构文档](docs/module-structure.md)。

## 贡献

欢迎通过 Issue 和 Pull Request 参与 Mash 的开发。

如果你发现了 Bug、希望提出功能建议，或者有意改进代码与文档，都可以通过 GitHub 参与讨论。

提交 Pull Request 前，建议先阅读项目文档，了解现有模块划分与代码组织方式。

对于较大规模的功能改动，建议先创建 Issue 讨论实现方案。

## 许可证

Mash 的原创源代码及文档采用 [MIT License](LICENSE) 开源。

项目使用的第三方软件、OCR 模型、依赖项及游戏相关资源可能适用不同的许可证或使用条款。

详情请参阅 [THIRD_PARTY_NOTICES](THIRD_PARTY_NOTICES)。

## 免责声明

Mash 是一个非官方的开源项目，与《Fate/Grand Order》的开发商、发行商及相关权利方不存在关联或合作关系。

本项目仅用于技术研究、学习与交流。

使用者应自行了解并遵守游戏服务条款。使用自动化工具可能存在账号限制或其他风险，相关责任由使用者自行承担。
