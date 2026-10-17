//! MuMu SDK display lookup. The boot ID ties SDK instance selection to the
//! selected ADB transport, including aliases such as emulator-5554 / :5555.

use std::path::Path;
use std::sync::{Arc, Mutex};

/// SDK handles are process-global in MuMu. Serialize short connect/query/close
/// sequences through application-managed state rather than sharing raw handles.
#[derive(Default)]
pub(crate) struct MuMuLookupState(pub(crate) Arc<Mutex<()>>);

pub(crate) fn resolve_mumu_display(
    adb_path: &Path,
    serial: &str,
    package: &str,
) -> Result<Option<u32>, String> {
    imp::resolve(adb_path, serial, package)
}

#[cfg(not(windows))]
mod imp {
    use super::*;
    pub(super) fn resolve(_: &Path, _: &str, _: &str) -> Result<Option<u32>, String> {
        Ok(None)
    }
}

#[cfg(windows)]
mod imp {
    use super::*;
    use crate::platform::{external_command, output_with_timeout};
    use std::collections::BTreeSet;
    use std::ffi::CString;
    use std::os::windows::ffi::OsStrExt;
    use std::path::PathBuf;
    use std::time::Duration;

    struct Instance {
        root: PathBuf,
        index: i32,
    }

    #[link(name = "kernel32")]
    extern "system" {
        fn LoadLibraryExW(
            path: *const u16,
            file: *mut std::ffi::c_void,
            flags: u32,
        ) -> *mut std::ffi::c_void;
        fn GetProcAddress(module: *mut std::ffi::c_void, name: *const u8) -> *mut std::ffi::c_void;
        fn FreeLibrary(module: *mut std::ffi::c_void) -> i32;
    }

    struct Library(*mut std::ffi::c_void);

    impl Library {
        unsafe fn load(path: &Path) -> Result<Self, String> {
            let path: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
            // Resolve dependencies in the SDK directory and trusted host paths,
            // excluding the application's current working directory.
            let handle = LoadLibraryExW(path.as_ptr(), std::ptr::null_mut(), 0x1100);
            if handle.is_null() {
                Err(format!(
                    "加载 MuMu SDK 失败: {}",
                    std::io::Error::last_os_error()
                ))
            } else {
                Ok(Self(handle))
            }
        }

        unsafe fn symbol(&self, name: &[u8]) -> Result<*mut std::ffi::c_void, String> {
            let address = GetProcAddress(self.0, name.as_ptr());
            if address.is_null() {
                Err(format!(
                    "MuMu SDK 缺少接口 {}",
                    String::from_utf8_lossy(name)
                ))
            } else {
                Ok(address)
            }
        }
    }

    impl Drop for Library {
        fn drop(&mut self) {
            unsafe {
                FreeLibrary(self.0);
            }
        }
    }

    fn run(path: &Path, args: &[&str]) -> Result<String, String> {
        let mut command = external_command(path);
        command.args(args);
        let output = output_with_timeout(&mut command, Duration::from_secs(5))
            .map_err(|error| format!("MuMu 设备查询失败: {error}"))?;
        if !output.status.success() {
            return Err(format!(
                "MuMu 设备查询失败: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            ));
        }
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    }

    fn installations() -> Result<Vec<(PathBuf, PathBuf)>, String> {
        // Resolve the actual running installation, including custom locations
        // and device-only sessions. Never load a DLL from the working directory.
        let paths = run(Path::new("powershell.exe"), &["-NoProfile", "-NonInteractive", "-Command",
            "[Console]::OutputEncoding = [System.Text.UTF8Encoding]::new($false); Get-Process MuMuNxMain,MuMuNxDevice,MuMuPlayer -ErrorAction SilentlyContinue | ForEach-Object { $_.Path }; exit 0"])?;
        let mut roots = BTreeSet::new();
        for path in paths.lines().map(str::trim).filter(|path| !path.is_empty()) {
            for root in Path::new(path).ancestors().skip(1).take(5) {
                for relative in ["nx_main/MuMuManager.exe", "shell/MuMuManager.exe"] {
                    let manager = root.join(relative);
                    if manager.is_file() {
                        roots.insert((root.to_path_buf(), manager));
                    }
                }
            }
        }
        Ok(roots.into_iter().collect())
    }

    fn unrelated_device(adb_path: &Path, serial: &str) -> Result<Option<u32>, String> {
        let display = run(adb_path, &["-s", serial, "shell", "dumpsys", "display"])?;
        if display.to_ascii_lowercase().contains("mumuscreen") {
            Err("无法将所选 MuMu 设备匹配到运行中的模拟器实例，请确认 MuMu 管理工具与模拟器均已启动".into())
        } else {
            Ok(None)
        }
    }

    pub(super) fn resolve(
        adb_path: &Path,
        serial: &str,
        package: &str,
    ) -> Result<Option<u32>, String> {
        let installations = installations()?;
        if installations.is_empty() {
            return unrelated_device(adb_path, serial);
        }
        let identity = run(
            adb_path,
            &[
                "-s",
                serial,
                "shell",
                "cat",
                "/proc/sys/kernel/random/boot_id",
            ],
        )?;
        let identity = super::boot_id(&identity).ok_or("无法确定所选 ADB 设备的启动标识")?;
        let mut matches = Vec::new();
        for (root, manager) in installations {
            let indexes = run(&manager, &["info", "--vmindex", "all"])
                .and_then(|info| super::running_indexes(&info));
            let Ok(indexes) = indexes else {
                continue;
            };
            for index in indexes {
                let index_string = index.to_string();
                let candidate = run(
                    &manager,
                    &[
                        "adb",
                        "--vmindex",
                        &index_string,
                        "--cmd",
                        "shell cat /proc/sys/kernel/random/boot_id",
                    ],
                );
                if candidate
                    .as_deref()
                    .ok()
                    .and_then(super::boot_id)
                    .as_deref()
                    == Some(&identity)
                {
                    matches.push(Instance {
                        root: root.clone(),
                        index,
                    });
                }
            }
        }
        let instance = match matches.len() {
            0 => return unrelated_device(adb_path, serial),
            1 => matches.remove(0),
            _ => return Err("多个 MuMu 实例匹配同一设备，无法确定目标实例".into()),
        };
        // The SDK returns display 0 for any package when keep-alive is off.
        // Check the package is running before trusting that result.
        let process = run(adb_path, &["-s", serial, "shell", "pidof", package])
            .map_err(|_| format!("请先启动当前区服的 FGO（{package}）"))?;
        if process.trim().is_empty() {
            return Err(format!("请先启动当前区服的 FGO（{package}）"));
        }
        let id = sdk_display(&instance, package)?;
        eprintln!(
            "[mumu] selected serial={serial}, instance={}, package={package}, display={id}",
            instance.index
        );
        Ok(Some(id))
    }

    fn sdk_display(instance: &Instance, package: &str) -> Result<u32, String> {
        let path = [
            "nx_main/sdk/external_renderer_ipc.dll",
            "shell/sdk/external_renderer_ipc.dll",
        ]
        .iter()
        .map(|relative| instance.root.join(relative))
        .find(|path| path.is_file())
        .ok_or("MuMu SDK 不存在，请更新 MuMu 模拟器")?;
        let root: Vec<u16> = instance
            .root
            .as_os_str()
            .encode_wide()
            .chain(Some(0))
            .collect();
        let package_c = CString::new(package).map_err(|error| error.to_string())?;
        // SDK ABI: MaaXYZ/EmulatorExtras/Mumu/external_renderer_ipc.
        // All symbols and the handle stay within the loaded library's lifetime.
        unsafe {
            let library = Library::load(&path)?;
            let connect: unsafe extern "C" fn(*const u16, i32) -> i32 =
                std::mem::transmute(library.symbol(b"nemu_connect\0")?);
            let disconnect: unsafe extern "C" fn(i32) =
                std::mem::transmute(library.symbol(b"nemu_disconnect\0")?);
            let display: unsafe extern "C" fn(i32, *const std::ffi::c_char, i32) -> i32 =
                std::mem::transmute(library.symbol(b"nemu_get_display_id\0")?);
            let handle = connect(root.as_ptr(), instance.index);
            if handle <= 0 {
                return Err("连接 MuMu SDK 失败".into());
            }
            let id = display(handle, package_c.as_ptr(), 0);
            disconnect(handle);
            u32::try_from(id)
                .map_err(|_| format!("MuMu 找不到 FGO 的显示屏（{package}），请确认游戏正在运行"))
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        #[ignore = "read-only live probe; set MASH_LIVE_ADB_SERIAL and MASH_LIVE_GAME_PACKAGE"]
        fn live_package_display_lookup() {
            let serial = std::env::var("MASH_LIVE_ADB_SERIAL").expect("set selected ADB serial");
            let package = std::env::var("MASH_LIVE_GAME_PACKAGE").expect("set target package");
            let adb = Path::new(env!("CARGO_MANIFEST_DIR")).join("resources/adb/adb.exe");
            let display = resolve(&adb, &serial, &package)
                .expect("SDK lookup failed")
                .expect("not MuMu");
            println!("SDK resolved {package} on {serial} to display {display}");
            assert!(resolve(&adb, &serial, "com.mash.nonexistent.test.package").is_err());
        }
    }
}

#[cfg(any(windows, test))]
fn boot_id(output: &str) -> Option<String> {
    output
        .lines()
        .map(str::trim)
        .find(|line| {
            line.len() == 36
                && line.bytes().enumerate().all(|(index, byte)| {
                    if [8, 13, 18, 23].contains(&index) {
                        byte == b'-'
                    } else {
                        byte.is_ascii_hexdigit()
                    }
                })
        })
        .map(str::to_ascii_lowercase)
}

#[cfg(any(windows, test))]
fn running_indexes(info: &str) -> Result<Vec<i32>, String> {
    let info: serde_json::Value =
        serde_json::from_str(info).map_err(|error| format!("无法解析 MuMu 实例信息: {error}"))?;
    let instances = info.as_object().ok_or("MuMu 实例信息不是对象")?;
    Ok(instances
        .iter()
        .filter(|(_, value)| value["is_android_started"].as_bool() == Some(true))
        .filter_map(|(key, value)| {
            value["index"]
                .as_i64()
                .and_then(|index| i32::try_from(index).ok())
                .or_else(|| value["index"].as_str().unwrap_or(key).parse::<i32>().ok())
        })
        .filter(|index| *index >= 0)
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn boot_identity_ignores_cli_noise_and_cannot_match_empty_or_android_id() {
        assert_eq!(
            boot_id("connected\r\n79882AE6-EAE2-475A-856F-2E45913666B9\r\n"),
            Some("79882ae6-eae2-475a-856f-2e45913666b9".into())
        );
        for invalid in [
            "",
            "device offline",
            "79882ae6eae2475a",
            "xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx",
        ] {
            assert!(boot_id(invalid).is_none());
        }
    }

    #[test]
    fn instance_enumeration_excludes_stopped_and_invalid_instances() {
        let indexes = running_indexes(
            r#"{
            "0":{"index":"0","is_android_started":true},
            "2":{"index":"2","is_android_started":false},
            "4":{"is_android_started":true},
            "9":{"index":8,"is_android_started":true},
            "bad":{"index":"-1","is_android_started":true}
        }"#,
        )
        .unwrap();
        assert_eq!(indexes, [0, 4, 8]);
        assert!(running_indexes("not JSON").is_err());
    }
}
