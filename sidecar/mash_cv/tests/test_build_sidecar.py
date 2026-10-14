import importlib.util
import os
from pathlib import Path
import stat
import sys
import zipfile

import pytest


spec = importlib.util.spec_from_file_location("build_sidecar", Path(__file__).parents[1] / "build_sidecar.py")
builder = importlib.util.module_from_spec(spec)
spec.loader.exec_module(builder)


@pytest.mark.parametrize("system,machine,version", [
    ("Darwin", "arm64", "2026.05.08-runtime1"),
    ("Windows", "AMD64", "0.1.0"),
])
def test_main_builds_the_host_runtime_version(tmp_path, monkeypatch, system, machine, version):
    root = tmp_path / "sidecar" / "mash_cv"
    root.mkdir(parents=True)
    (tmp_path / "versions.toml").write_text(
        '[mash_cv]\ncode = "0.4.26"\n[mash_cv.runtime]\n'
        'darwin-aarch64 = "2026.05.08-runtime1"\nwindows-x86_64 = "0.1.0"\n', encoding="utf-8",
    )
    monkeypatch.setattr(builder, "ROOT", root)
    monkeypatch.setattr(builder.platform, "system", lambda: system)
    monkeypatch.setattr(builder.platform, "machine", lambda: machine)
    monkeypatch.setattr(sys, "argv", ["build_sidecar.py", "--runtime-only"])
    monkeypatch.delenv("MASH_CV_RUNTIME_VERSION", raising=False)
    calls = []
    monkeypatch.setattr(builder, "build_runtime", lambda v, target: (calls.append((v, target)) or (root / "runtime.zip", "sha")))
    builder.main()
    assert calls == [(version, builder.platform_key(system, machine))]


def test_code_only_build_does_not_require_a_host_runtime_version(tmp_path, monkeypatch):
    root = tmp_path / "sidecar" / "mash_cv"
    root.mkdir(parents=True)
    (tmp_path / "versions.toml").write_text('[mash_cv]\ncode = "0.4.26"\n[mash_cv.runtime]\n', encoding="utf-8")
    monkeypatch.setattr(builder, "ROOT", root)
    monkeypatch.setattr(sys, "argv", ["build_sidecar.py", "--code-only"])
    monkeypatch.delenv("MASH_CV_CODE_VERSION", raising=False)
    calls = []
    monkeypatch.setattr(builder, "build_code", lambda v: (calls.append(v) or (root / "code.zip", "sha")))
    builder.main()
    assert calls == ["0.4.26"]
    monkeypatch.setattr(sys, "argv", ["build_sidecar.py", "--runtime-only"])
    monkeypatch.delenv("MASH_CV_RUNTIME_VERSION", raising=False)
    with pytest.raises(SystemExit, match="2"):
        builder.main()


def test_runtime_version_override_supports_a_new_platform(tmp_path, monkeypatch):
    root = tmp_path / "sidecar" / "mash_cv"
    root.mkdir(parents=True)
    (tmp_path / "versions.toml").write_text('[mash_cv]\ncode = "0.4.26"\n[mash_cv.runtime]\n', encoding="utf-8")
    monkeypatch.setattr(builder, "ROOT", root)
    monkeypatch.setattr(sys, "argv", ["build_sidecar.py", "--runtime-only"])
    monkeypatch.setenv("MASH_CV_RUNTIME_VERSION", "0.2.0")
    calls = []
    monkeypatch.setattr(builder, "build_runtime", lambda v, target: (calls.append(v) or (root / "runtime.zip", "sha")))
    builder.main()
    assert calls == ["0.2.0"]


@pytest.mark.parametrize("system,machine,expected", [
    ("Windows", "AMD64", "windows-x86_64"),
    ("Windows", "ARM64", "windows-aarch64"),
    ("Darwin", "arm64", "darwin-aarch64"),
    ("Linux", "x86_64", "linux-x86_64"),
])
def test_platform_key(system, machine, expected):
    assert builder.platform_key(system, machine) == expected


def test_archive_preserves_bundle_root_and_binary_contents(tmp_path):
    root = tmp_path / "mash-cv-runtime"
    (root / "_internal").mkdir(parents=True)
    (root / "mash-cv.exe").write_bytes(b"MZ executable")
    (root / "_internal" / "library.dll").write_bytes(b"dll")
    archive_path = tmp_path / "runtime.zip"
    digest = builder.archive_directory(root, archive_path)
    assert len(digest) == 64
    with zipfile.ZipFile(archive_path) as archive:
        assert archive.read("mash-cv-runtime/mash-cv.exe") == b"MZ executable"
        assert archive.read("mash-cv-runtime/_internal/library.dll") == b"dll"


@pytest.mark.skipif(os.name == "nt", reason="Unix framework symlink archive")
def test_archive_preserves_framework_symlinks(tmp_path):
    root = tmp_path / "mash-cv-runtime"
    root.mkdir()
    (root / "library").write_bytes(b"library")
    (root / "link").symlink_to("library")
    archive_path = tmp_path / "runtime.zip"
    builder.archive_directory(root, archive_path)
    with zipfile.ZipFile(archive_path) as archive:
        entry = archive.getinfo("mash-cv-runtime/link")
        assert stat.S_ISLNK(entry.external_attr >> 16)
        assert archive.read(entry) == b"library"


def test_code_archive_omits_models_and_python_caches(tmp_path, monkeypatch):
    source = tmp_path / "mash_cv"
    (source / "models").mkdir(parents=True)
    (source / "__pycache__").mkdir()
    (source / "__init__.py").write_text("", encoding="utf-8")
    (source / "models" / "heavy.onnx").write_bytes(b"model")
    (source / "__pycache__" / "cache.pyc").write_bytes(b"cache")
    (tmp_path / "dist").mkdir()
    monkeypatch.setattr(builder, "ROOT", tmp_path)
    path, digest = builder.build_code("0.4.26")
    with zipfile.ZipFile(path) as archive:
        assert archive.namelist() == ["mash-cv-code/mash_cv/__init__.py"]
    assert len(digest) == 64


def test_code_archive_digest_is_independent_of_checkout_timestamps(tmp_path, monkeypatch):
    source = tmp_path / "mash_cv"
    source.mkdir()
    path = source / "__init__.py"
    path.write_bytes(b"code")
    (tmp_path / "dist").mkdir()
    monkeypatch.setattr(builder, "ROOT", tmp_path)
    _, first = builder.build_code("0.4.26")
    os.utime(path, (1700000000, 1700000000))
    _, second = builder.build_code("0.4.26")
    assert first == second


def test_code_archive_normalizes_windows_checkout_line_endings(tmp_path, monkeypatch):
    source = tmp_path / "mash_cv"
    source.mkdir()
    path = source / "__init__.py"
    path.write_bytes(b"line\nline\n")
    (tmp_path / "dist").mkdir()
    monkeypatch.setattr(builder, "ROOT", tmp_path)
    _, first = builder.build_code("0.4.26")
    path.write_bytes(b"line\r\nline\r\n")
    archive_path, second = builder.build_code("0.4.26")
    assert first == second
    with zipfile.ZipFile(archive_path) as archive:
        assert archive.read("mash-cv-code/mash_cv/__init__.py") == b"line\nline\n"
