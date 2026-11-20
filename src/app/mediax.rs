//! Run KANALI's installed converter in a child process so its codec DLLs are released after import.
use libloading::os::windows::{
    Library, LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR, LOAD_LIBRARY_SEARCH_SYSTEM32,
};
use std::{
    ffi::CString,
    os::windows::process::CommandExt,
    path::{Path, PathBuf},
    process::Command,
    thread,
    time::{Duration, Instant},
};

pub fn installed() -> Option<PathBuf> {
    ["ProgramFiles", "ProgramFiles(x86)", "LOCALAPPDATA"]
        .into_iter()
        .filter_map(std::env::var_os)
        .map(PathBuf::from)
        .flat_map(|root| {
            [
                root.join("KANALI/MediaX.dll"),
                root.join("Programs/KANALI/MediaX.dll"),
            ]
        })
        .find(|dll| dll.is_absolute() && dll.is_file())
}

pub fn encode(input: &Path, output: &Path) -> Result<(), String> {
    let mut child = Command::new(std::env::current_exe().map_err(|e| e.to_string())?)
        .arg("--encode-mediax")
        .arg(input)
        .arg(output)
        .creation_flags(0x08004000) // CREATE_NO_WINDOW | BELOW_NORMAL_PRIORITY_CLASS
        .spawn()
        .map_err(|e| e.to_string())?;
    let start = Instant::now();
    loop {
        if let Some(status) = child.try_wait().map_err(|e| e.to_string())? {
            return if status.success() {
                Ok(())
            } else {
                Err(format!("MediaX: {status}"))
            };
        }
        if start.elapsed() > Duration::from_secs(600) {
            let _ = child.kill();
            let _ = child.wait();
            return Err("MediaX quá thời gian chuyển video".into());
        }
        thread::sleep(Duration::from_millis(200));
    }
}

pub fn worker(input: &str, output: &str) -> Result<(), i32> {
    let dll = installed().ok_or(2)?;
    let input = CString::new(input).map_err(|_| 3)?;
    let output = CString::new(output).map_err(|_| 3)?;
    // These x64 exports and argument types are the ones used by KANALI's MediaX bridge.
    unsafe {
        let lib = Library::load_with_flags(
            dll,
            LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR | LOAD_LIBRARY_SEARCH_SYSTEM32,
        )
        .map_err(|_| 4)?;
        // MediaX owns asynchronous codec threads. The dedicated process releases it at exit.
        let lib = std::mem::ManuallyDrop::new(lib);
        let convert = lib
            .get::<unsafe extern "C" fn(*const i8, *const i8, i32, i32, i32) -> i64>(
                b"MX_ConvertToH264Raw\0",
            )
            .map_err(|_| 5)?;
        let status = lib
            .get::<unsafe extern "C" fn(i64, *mut i32) -> i64>(b"MX_SessionGetStatus\0")
            .map_err(|_| 5)?;
        let session = convert(input.as_ptr(), output.as_ptr(), 2240, 1080, 60);
        if session <= 0 {
            return Err(6);
        }
        loop {
            let mut state = 0;
            if status(session, &mut state) < 0 {
                return Err(7);
            }
            match state {
                3 => return Ok(()),
                4 | 5 => return Err(8),
                _ => thread::sleep(Duration::from_millis(200)),
            }
        }
    }
}
