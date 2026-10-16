# Linux 版支持实施计划

目标：为 Mash 增加 Linux x86_64 首版支持，使用 AppImage 作为主分发产物，提供 DEB 辅助安装包，并接通独立的 mash-cv Linux runtime、ADB、CI 和 Tauri updater。

## 实施范围

- 支持平台：Linux x86_64；首版以 Ubuntu 22.04/24.04、Debian 12 为官方验证基线。
- 产物：AppImage、DEB；Tauri updater 使用 Linux AppImage updater artifact。
- CV：生成并发布 `linux-x86_64` runtime，继续复用平台无关的 code zip。
- ADB：随 Linux 包提供固定版本 ADB 资源，保留系统 PATH fallback。
- 测试：Linux Rust/前端/sidecar/build smoke tests，并保持 Windows/macOS 现有验证。

## 代码与配置改动

1. 增加 `linux-x86_64` runtime 版本和 manifest 条目；统一发布脚本的平台注册表。
2. 新增 `src-tauri/tauri.linux.conf.json`，配置 AppImage/DEB 和 Linux ADB 资源。
3. 增加 Linux CV 构建/发布入口，验证 PyInstaller runtime、权限、sidecar ping 和安装路径。
4. 扩展 updater artifact 选择、manifest 合并和相关测试，支持 Linux AppImage updater。
5. 扩展 GitHub Actions 的 Linux Rust、CV runtime、compatibility jobs。
6. 更新 README、分发文档、Linux 构建/安装文档和资源说明。

## 验证与交付

- 运行前端测试、脚本测试、Rust 测试和 sidecar pytest。
- 在可用的 Linux 工具链上构建 Linux Tauri 包；若环境缺少 Tauri/GTK 或 PyInstaller 依赖，保留代码级验证并明确缺口。
- 验证 runtime manifest、Linux runtime 安装、sidecar 启动、ADB 路径和 updater artifact 选择。
- 检查 diff，确保 Windows/macOS 平台和现有发布流程未被破坏。
