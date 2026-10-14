import importlib.util
import json
import os
from pathlib import Path
import shutil
import stat
from types import SimpleNamespace
import zipfile

import pytest


@pytest.fixture
def release(monkeypatch):
    root = Path(__file__).parents[1]
    monkeypatch.syspath_prepend(str(root))
    spec = importlib.util.spec_from_file_location("verify_code_release", root / "verify_code_release.py")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


@pytest.mark.parametrize("name", ["../outside", "mash-cv-code/../outside", "/mash-cv-code/x",
                                  "mash-cv-code\\outside", "wrong-root/file", "mash-cv-code/C:/x",
                                  "mash-cv-code/file\0hidden"])
def test_extract_rejects_paths_outside_the_bundle(release, tmp_path, name):
    archive = tmp_path / "code.zip"
    with zipfile.ZipFile(archive, "w") as file:
        # ZipInfo normalizes separators on Windows and truncates NUL bytes.
        # Restore the raw filename so the archive really contains bad input.
        entry = zipfile.ZipInfo(name)
        entry.filename = name
        file.writestr(entry, "bad")
    with zipfile.ZipFile(archive) as file:
        assert [entry.orig_filename for entry in file.infolist()] == [name]
    with pytest.raises(ValueError, match="ZIP entry"):
        release.extract(archive, tmp_path / "extract", "mash-cv-code")


def test_extract_rejects_raw_backslashes_even_when_zipfile_normalizes_them(release, tmp_path, monkeypatch):
    archive = tmp_path / "code.zip"
    entry = zipfile.ZipInfo("mash-cv-code\\outside")
    entry.filename = entry.orig_filename
    with zipfile.ZipFile(archive, "w") as file:
        file.writestr(entry, "bad")

    class WindowsZipInfo(zipfile.ZipInfo):
        def __init__(self, *args, **kwargs):
            super().__init__(*args, **kwargs)
            self.filename = self.filename.replace("\\", "/")

    monkeypatch.setattr(zipfile, "ZipInfo", WindowsZipInfo)
    with pytest.raises(ValueError, match="ZIP entry"):
        release.extract(archive, tmp_path / "extract", "mash-cv-code")


@pytest.mark.skipif(os.name == "nt", reason="macOS runtime framework symlinks")
def test_extract_preserves_native_modes_and_safe_framework_links(release, tmp_path):
    archive = tmp_path / "runtime.zip"
    with zipfile.ZipFile(archive, "w") as file:
        executable = zipfile.ZipInfo("mash-cv-runtime/mash-cv")
        executable.external_attr = (stat.S_IFREG | 0o755) << 16
        file.writestr(executable, "executable")
        file.writestr("mash-cv-runtime/Python.framework/Versions/A/Python", "library")
        link = zipfile.ZipInfo("mash-cv-runtime/Python.framework/Versions/Current")
        link.external_attr = (stat.S_IFLNK | 0o777) << 16
        file.writestr(link, "A")
    root = release.extract(archive, tmp_path / "extract", "mash-cv-runtime")
    assert (root / "mash-cv").stat().st_mode & 0o111
    assert (root / "Python.framework/Versions/Current").is_symlink()
    assert (root / "Python.framework/Versions/Current/Python").read_text() == "library"


@pytest.mark.parametrize("target", ["../../outside", "/outside", "C:\\outside"])
def test_extract_refuses_escaping_symlink_targets(release, tmp_path, target):
    archive = tmp_path / "runtime.zip"
    with zipfile.ZipFile(archive, "w") as file:
        link = zipfile.ZipInfo("mash-cv-runtime/link")
        link.external_attr = (stat.S_IFLNK | 0o777) << 16
        file.writestr(link, target)
    with pytest.raises(ValueError, match="symlink"):
        release.extract(archive, tmp_path / "extract", "mash-cv-runtime")


def smoke_fixture(tmp_path):
    executable = tmp_path / "runtime" / "mash-cv"
    executable.parent.mkdir()
    executable.write_bytes(b"executable")
    code = tmp_path / "code"
    (code / "mash_cv").mkdir(parents=True)
    (code / "mash_cv" / "__init__.py").write_text("")
    models = tmp_path / "models"
    models.mkdir()
    return executable, code, models


def test_smoke_uses_packaged_code_and_validates_unicode_io_and_ocr(release, tmp_path, monkeypatch):
    executable, code, models = smoke_fixture(tmp_path)
    monkeypatch.setenv("PYTHONPATH", "ambient-source")

    def run(args, **kwargs):
        assert args == [str(executable)]
        assert kwargs["env"]["MASH_CV_CODE_DIR"] == str(code)
        assert "PYTHONPATH" not in kwargs["env"]
        commands = [json.loads(line) for line in kwargs["input"].decode("utf-8").splitlines()]
        assert "中文" in commands[1]["dir"]
        assert Path(commands[2]["imagePath"]).read_bytes().startswith(b"\x89PNG")
        return SimpleNamespace(returncode=0, stderr=b"", stdout=b'{"id":1,"ok":true}\n{"id":2,"count":1}\n{"id":3,"fragments":[]}\n')

    monkeypatch.setattr(release.subprocess, "run", run)
    release.smoke(executable, code, models, tmp_path)


@pytest.mark.parametrize("result", [
    SimpleNamespace(returncode=1, stderr=b"missing dependency", stdout=b""),
    SimpleNamespace(returncode=0, stderr=b"", stdout=b'{"id":1,"ok":true}\n{"id":2,"count":1}\n'),
    SimpleNamespace(returncode=0, stderr=b"", stdout=b'{"id":1,"ok":true}\n{"id":2,"count":1}\n{"id":3,"error":"OCR failed"}\n'),
])
def test_smoke_fails_on_process_errors_or_missing_ocr_reply(release, tmp_path, monkeypatch, result):
    executable, code, models = smoke_fixture(tmp_path)
    monkeypatch.setattr(release.subprocess, "run", lambda *args, **kwargs: result)
    with pytest.raises(RuntimeError, match="Frozen CV"):
        release.smoke(executable, code, models, tmp_path)


def test_download_uses_release_user_agent_and_verifies_runtime_hash(release, tmp_path, monkeypatch):
    source = tmp_path / "source.zip"
    source.write_bytes(b"published runtime")

    def urlopen(request, **kwargs):
        assert request.full_url == "https://example.invalid/runtime.zip"
        assert request.get_header("User-agent") == "mash-release/1.0"
        assert kwargs["timeout"] == 300
        return source.open("rb")

    monkeypatch.setattr(release.urllib.request, "urlopen", urlopen)
    output = tmp_path / "download.zip"
    release.download("https://example.invalid/runtime.zip", output, release.digest(source))
    assert output.read_bytes() == source.read_bytes()


def test_download_rejects_incorrect_runtime_hash(release, tmp_path, monkeypatch):
    source = tmp_path / "source.zip"
    source.write_bytes(b"wrong runtime")
    monkeypatch.setattr(release.urllib.request, "urlopen", lambda *args, **kwargs: source.open("rb"))
    output = tmp_path / "download.zip"
    with pytest.raises(ValueError, match="SHA-256"):
        release.download("https://example.invalid/runtime.zip", output, "0" * 64)
    assert not output.exists()


def test_verify_only_reports_success_after_the_exact_zip_pair_runs(release, tmp_path, monkeypatch):
    candidate = tmp_path / "mash-cv-code-v1.1.0.zip"
    with zipfile.ZipFile(candidate, "w") as file:
        file.writestr("mash-cv-code/mash_cv/__init__.py", "# candidate")
    published = tmp_path / "published.zip"
    with zipfile.ZipFile(published, "w") as file:
        file.writestr("mash-cv-runtime/mash-cv.exe", "executable")
        file.writestr("mash-cv-runtime/_internal/mash_cv/models/model.onnx", "model")
    receipt = {"version": "1.1.0", "sourceSha": "a" * 40, "artifact": candidate.name,
               "sha256": release.digest(candidate), "runtimes": {"windows-x86_64": {
                   "runtimeVersion": "0.1.0", "runtimeUrl": "https://example.invalid/runtime.zip",
                   "runtimeSha256": release.digest(published)}}}
    record = tmp_path / "release.json"
    record.write_text(json.dumps(receipt))
    monkeypatch.setattr(release, "platform_key", lambda *args: "windows-x86_64")

    def download(url, output, expected):
        assert expected == release.digest(published)
        shutil.copyfile(published, output)

    calls = []
    monkeypatch.setattr(release, "download", download)
    monkeypatch.setattr(release, "smoke", lambda exe, code, models, work: calls.append((exe, code, models)))
    report = tmp_path / "reports" / "windows-x86_64.json"
    release.verify(record, tmp_path / "dist", report)
    assert len(calls) == 1
    assert json.loads(report.read_text())["codeSha256"] == receipt["sha256"]
    assert (tmp_path / "dist" / "mash-cv-runtime-windows-x86_64-v0.1.0.zip").is_file()
    assert (tmp_path / "dist" / candidate.name).read_bytes() == candidate.read_bytes()
    report.unlink()
    candidate.write_bytes(b"modified after testing")
    with pytest.raises(ValueError, match="code SHA-256"):
        release.verify(record, tmp_path / "dist", report)
    assert not report.exists()
