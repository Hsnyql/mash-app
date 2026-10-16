# Linux 构建、验证与安装

Linux 首版支持 x86_64，官方验证基线为 Ubuntu 22.04/24.04 和 Debian 12。主分发产物是 AppImage，另外提供 DEB 包。Tauri updater 使用 AppImage updater artifact；DEB 适合手动安装和系统集成。

## 系统依赖

在 Ubuntu 22.04 上安装构建依赖：

```bash
sudo apt-get update
sudo apt-get install -y \
  build-essential file \
  libayatana-appindicator3-dev \
  libgtk-3-dev libssl-dev \
  libwebkit2gtk-4.1-dev \
  libxdo-dev librsvg2-dev patchelf
```

Ubuntu 22.04/24.04 均使用 WebKitGTK 4.1 开发包。运行已打包应用时，系统仍需提供对应的 GTK/WebKitGTK 运行库。

## 构建

准备并校验固定版本的 Linux ADB：

```bash
scripts/linux/prepare-linux-adb.sh
```

构建 CV runtime 和 code package：

```bash
scripts/linux/build-cv.sh
```

构建 Tauri 应用：

```bash
pnpm build:linux
```

本地和 CI 构建默认生成未签名的 AppImage/DEB。发布 updater 时使用带签名构建：

```bash
scripts/linux/build-linux.sh --signed
```

产物位于：

```text
src-tauri/target/release/bundle/appimage/
src-tauri/target/release/bundle/deb/
```

`mash-cv` runtime 不会嵌入 Tauri 应用，而是作为独立的 Linux runtime ZIP 分发。首次运行前，需要在应用的资源管理页面安装与 `src-tauri/resources/runtime-manifest.json` 匹配的 runtime 和 code 包。

## 测试

```bash
pnpm lint
pnpm test
pnpm test:scripts
cargo test --manifest-path src-tauri/Cargo.toml
cd sidecar/mash_cv
poetry install
poetry run pytest
```

本地 runtime 构建完成后，可运行冻结 runtime smoke test：

```bash
poetry run pytest tests/test_linux_runtime.py
```

## 发布

Linux runtime 发布入口：

```bash
R2_ENDPOINT=... R2_BUCKET=... RELEASE_BASE_URL=... \
  scripts/linux/release-cv-runtime.sh 0.1.0
```

Linux updater 发布入口：

```bash
R2_ENDPOINT=... R2_BUCKET=... RELEASE_BASE_URL=... \
  scripts/linux/release-tauri-updater.sh
```

发布前应确认工作区干净、当前提交是 `vX.Y.Z` tag，并已生成带签名的 AppImage 及其 `.sig` 文件。Linux 平台键固定为 `linux-x86_64`。
