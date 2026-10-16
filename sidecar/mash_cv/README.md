# mash-cv

OpenCV / OCR sidecar for Mash, including scrcpy video decoding through PyAV. It runs as a persistent process and exchanges one JSON object per line with the Rust backend over stdin/stdout.

## Setup

Requires Python 3.11–3.14 and Poetry. CI uses Python 3.12. Run these commands from `sidecar/mash_cv/`:

```bash
poetry install
poetry run python -m mash_cv
```

The Tauri development build reads Python source from this directory, using the installed CV runtime and OCR models.

## Layout

| Path | Purpose |
| --- | --- |
| `mash_cv/cv.py` | Command loop and recognition |
| `mash_cv/stream.py` | scrcpy / PyAV video pipeline |
| `mash_cv/host/` | OS-specific I/O and process handling |
| `mash_cv/region_tool.py` | Template-region CLI |
| `tests/` | Offline tests and fixtures |
| `build_sidecar.py` | Shared runtime and code ZIP builder |
| `training/` | Digit classifier training and labeling tools |

## Protocol

Requests include a `cmd` and may include an `id`, which is echoed in responses. The Rust client uses IDs to discard stale replies. Keep stdout for protocol output and write diagnostics to stderr.

Example request and response:

```text
→ {"id":1,"cmd":"ping"}
← {"id":1,"ok":true,"recognitionDiagnosticsVersion":1}
```

Common commands:

| Command | Purpose |
| --- | --- |
| `load_templates` | Load PNGs from `dir`; `append` preserves existing templates and `keyPrefix` adds a namespace |
| `load_config` | Read `cv.json` from `path`; `merge` deep-merges it into the current config |
| `set_server` | Select the game server |
| `start_stream` / `stop_stream` | Manage the device video stream |
| `detect` | Classify the current frame or `imagePath` |
| `find_element` | Match `templateKey` in a normalized `region` |
| `find_element_by_name` | Look up an element by `screen` and `element` |
| `find_region` | Match a template loaded from `templatePath` |
| `read_battle_scene` | Read the battle counter; `debug: true` adds diagnostics |
| `find_noble_phantasms` | Read NP readiness |
| `quit` | Stop the process |

Regions use normalized `x`, `y`, `w`, `h` coordinates in `[0, 1]`. Template keys preserve the relative path without `.png`, for example `battle/button_attack` or `shared/screen_team_party`.

Command handling lives in [cv.py](mash_cv/cv.py), with Rust request types and wrappers in [screen/](../../src-tauri/src/screen/). See [CV resources](../../src-tauri/resources/README.md), [battle-scene detection](../../docs/battle-scene-detection.md), and [NP readiness](../../docs/np-readiness-detection.md) for configuration and detection details.

## Region CLI

```bash
poetry run region --screenshot /path/to/screenshot.png --template /path/to/template.png --padding 0.02 --threshold 0.8
```

Outputs JSON with `found`, `score`, the normalized center `x` / `y`, matched `region` / `originalRegion`, and expanded `paddedRoi`. Use `--padding-x` and `--padding-y` to override padding per axis.

## Tests

```bash
poetry run pytest
```

For focused runs, pass a test path such as `tests/test_cv.py`. Keep fixtures under `tests/test_data/`; routine tests need no ADB device or running stream. Tests of packaged runtimes additionally require the built artifacts.

New template probes should include a 1920×1080 downscaled fixture to check resolution independence. See [sidecar conventions](AGENTS.md).

## Build

Run the shared builder in the native host environment:

```bash
poetry run python build_sidecar.py
poetry run python build_sidecar.py --code-only
poetry run python build_sidecar.py --runtime-only
```

Platform wrappers also install Poetry dependencies:

```bash
# macOS, from sidecar/mash_cv/
bash ../../scripts/macos/build-cv.sh
```

```powershell
# Windows, from sidecar/mash_cv/
../../scripts/windows/build-cv.ps1
# Or select one package
../../scripts/windows/build-cv.ps1 -Mode code
```

Windows accepts `-Mode runtime` and `-Python <path>` for an existing environment. Outputs:

```text
dist/mash-cv-runtime-<platform>-v<runtimeVersion>.zip
dist/mash-cv-code-v<codeVersion>.zip
```

The builder reads `versions.toml`: code has one shared version, while runtime versions are keyed by platform. One-off overrides use `MASH_CV_CODE_VERSION` and `MASH_CV_RUNTIME_VERSION`.

The runtime ZIP contains the PyInstaller launcher, native dependencies and OCR models under `mash-cv-runtime/`; the code ZIP contains `mash_cv/` under `mash-cv-code/`. Python-only changes need only a code package update. Publishing and manifest updates are covered in [distribution](../../docs/distribution.md).
