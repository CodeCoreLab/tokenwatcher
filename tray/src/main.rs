//! TokenWatcher 托盘宿主
//!
//! 职责：静默拉起 TokenWatcher.exe serve（无控制台窗口）、托盘图标、右键菜单、
//! 开机自启开关（HKCU Run，免管理员）、退出时收掉子进程。
//!
//! 设计约束：
//! - 零第三方 crate（只用 std + 手写 Win32 声明），保证离线可编译、产物体积小。
//! - `#![windows_subsystem = "windows"]` 让本进程无控制台窗口。
//! - 主程序仍走 CREATE_NO_WINDOW + 输出重定向到日志文件，而不是改 PE subsystem，
//!   这样 TokenWatcher.exe 保留 CLI 能力（`today` / `doctor` 等仍能看输出）。
#![windows_subsystem = "windows"]

use std::ffi::OsStr;
use std::fs::{create_dir_all, File, OpenOptions};
use std::net::TcpStream;
use std::os::windows::ffi::OsStrExt;
use std::os::windows::process::CommandExt;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::ptr::{null, null_mut};
use std::sync::atomic::{AtomicU16, Ordering};
use std::sync::Mutex;
use std::time::Duration;

/* ---------------------------------------------------------------- Win32 类型 */

type HWND = isize;
type HINSTANCE = isize;
type HANDLE = isize;
type HMENU = isize;
type HICON = isize;
type HKEY = isize;
type LRESULT = isize;
type WNDPROC = unsafe extern "system" fn(HWND, u32, usize, isize) -> LRESULT;

#[repr(C)]
struct POINT {
    x: i32,
    y: i32,
}

#[repr(C)]
struct MSG {
    hwnd: HWND,
    message: u32,
    w_param: usize,
    l_param: isize,
    time: u32,
    pt: POINT,
}

#[repr(C)]
struct WNDCLASSW {
    style: u32,
    lpfn_wnd_proc: Option<WNDPROC>,
    cb_cls_extra: i32,
    cb_wnd_extra: i32,
    h_instance: HINSTANCE,
    h_icon: HICON,
    h_cursor: HANDLE,
    hbr_background: HANDLE,
    lpsz_menu_name: *const u16,
    lpsz_class_name: *const u16,
}

#[repr(C)]
struct GUID {
    d1: u32,
    d2: u16,
    d3: u16,
    d4: [u8; 8],
}

/// NOTIFYICONDATAW（Vista+ 完整版）。cbSize 用 size_of 自适应，避免版本差异踩坑。
#[repr(C)]
struct NOTIFYICONDATAW {
    cb_size: u32,
    hwnd: HWND,
    uid: u32,
    u_flags: u32,
    u_callback_message: u32,
    h_icon: HICON,
    sz_tip: [u16; 128],
    dw_state: u32,
    dw_state_mask: u32,
    sz_info: [u16; 256],
    u_version: u32,
    sz_info_title: [u16; 64],
    dw_info_flags: u32,
    guid_item: GUID,
    h_balloon_icon: HICON,
}

/* ------------------------------------------------------------------ 常量 */

const WM_DESTROY: u32 = 0x0002;
const WM_COMMAND: u32 = 0x0111;
const WM_NULL: u32 = 0x0000;
const WM_CONTEXTMENU: u32 = 0x007B;
const WM_LBUTTONDBLCLK: u32 = 0x0203;
const WM_RBUTTONUP: u32 = 0x0205;

const NIM_ADD: u32 = 0x0000;
const NIM_MODIFY: u32 = 0x0001;
const NIM_DELETE: u32 = 0x0002;
const NIF_MESSAGE: u32 = 0x0001;
const NIF_ICON: u32 = 0x0002;
const NIF_TIP: u32 = 0x0004;
const NIF_INFO: u32 = 0x0010;

const MF_STRING: u32 = 0x0000;
const MF_SEPARATOR: u32 = 0x0800;
const MF_CHECKED: u32 = 0x0008;
const TPM_RIGHTBUTTON: u32 = 0x0002;
const TPM_RETURNCMD: u32 = 0x0100;

const CS_DBLCLKS: u32 = 0x0008;
const WS_EX_TOOLWINDOW: u32 = 0x0000_0080;
const WS_POPUP: u32 = 0x8000_0000;

const HKEY_CURRENT_USER: HKEY = 0x8000_0001u64 as HKEY;
const KEY_WRITE: u32 = 0x20006;
const KEY_READ: u32 = 0x20019;
const REG_SZ: u32 = 1;
const ERROR_ALREADY_EXISTS: u32 = 183;
const ERROR_FILE_NOT_FOUND: u32 = 2;

const ID_OPEN: usize = 1;
const ID_SCAN: usize = 2;
const ID_AUTOSTART: usize = 3;
const ID_LOG: usize = 4;
const ID_DATA: usize = 5;
const ID_EXIT: usize = 6;
const ID_CONFIG: usize = 7;

const WM_TRAY: u32 = 0x8001; // WM_APP + 1
const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
const RUN_VALUE: &str = "TokenWatcher";
/// 端口来自 TokenWatcher.ini，启动时装载。单线程消息循环，Relaxed 足够。
static PORT: AtomicU16 = AtomicU16::new(8788);

fn port() -> u16 {
    PORT.load(Ordering::Relaxed)
}
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/* --------------------------------------------------------------- FFI 声明 */

#[link(name = "user32")]
extern "system" {
    fn RegisterClassW(c: *const WNDCLASSW) -> u16;
    fn CreateWindowExW(
        ex_style: u32,
        class: *const u16,
        name: *const u16,
        style: u32,
        x: i32,
        y: i32,
        w: i32,
        h: i32,
        parent: HWND,
        menu: HMENU,
        inst: HINSTANCE,
        param: *mut core::ffi::c_void,
    ) -> HWND;
    fn DefWindowProcW(hwnd: HWND, msg: u32, wp: usize, lp: isize) -> LRESULT;
    fn GetMessageW(msg: *mut MSG, hwnd: HWND, min: u32, max: u32) -> i32;
    fn TranslateMessage(msg: *const MSG) -> i32;
    fn DispatchMessageW(msg: *const MSG) -> LRESULT;
    fn PostQuitMessage(code: i32);
    fn SetForegroundWindow(hwnd: HWND) -> i32;
    fn CreatePopupMenu() -> HMENU;
    fn AppendMenuW(menu: HMENU, flags: u32, id: usize, text: *const u16) -> i32;
    fn TrackPopupMenu(
        menu: HMENU,
        flags: u32,
        x: i32,
        y: i32,
        reserved: i32,
        hwnd: HWND,
        rect: *const core::ffi::c_void,
    ) -> i32;
    fn DestroyMenu(menu: HMENU) -> i32;
    fn GetCursorPos(p: *mut POINT) -> i32;
    fn PostMessageW(hwnd: HWND, msg: u32, wp: usize, lp: isize) -> i32;
    fn LoadIconW(inst: HINSTANCE, name: *const u16) -> HICON;
    fn MessageBoxW(hwnd: HWND, text: *const u16, caption: *const u16, flags: u32) -> i32;
    fn CreateIconFromResourceEx(
        res: *const u8,
        size: u32,
        is_icon: i32,
        version: u32,
        cx: i32,
        cy: i32,
        flags: u32,
    ) -> HICON;
    fn ShellExecuteW(
        hwnd: HWND,
        op: *const u16,
        file: *const u16,
        params: *const u16,
        dir: *const u16,
        show: i32,
    ) -> HINSTANCE;
}

#[link(name = "shell32")]
extern "system" {
    fn Shell_NotifyIconW(msg: u32, data: *const NOTIFYICONDATAW) -> i32;
}

#[link(name = "kernel32")]
extern "system" {
    fn GetModuleHandleW(name: *const u16) -> HINSTANCE;
    fn CreateMutexW(attr: *const core::ffi::c_void, owner: i32, name: *const u16) -> HANDLE;
    fn GetLastError() -> u32;
    fn CloseHandle(h: HANDLE) -> i32;
}

#[link(name = "advapi32")]
extern "system" {
    fn RegCreateKeyExW(
        key: HKEY,
        sub: *const u16,
        reserved: u32,
        class: *const u16,
        options: u32,
        access: u32,
        attr: *const core::ffi::c_void,
        result: *mut HKEY,
        disposition: *mut u32,
    ) -> i32;
    fn RegSetValueExW(key: HKEY, name: *const u16, reserved: u32, ty: u32, data: *const u8, len: u32)
        -> i32;
    fn RegDeleteValueW(key: HKEY, name: *const u16) -> i32;
    fn RegQueryValueExW(
        key: HKEY,
        name: *const u16,
        reserved: *mut u32,
        ty: *mut u32,
        data: *mut u8,
        len: *mut u32,
    ) -> i32;
    fn RegCloseKey(key: HKEY) -> i32;
}

/* ---------------------------------------------------------------- 工具函数 */

fn wide(s: &str) -> Vec<u16> {
    OsStr::new(s).encode_wide().chain(std::iter::once(0)).collect()
}

fn copy_to_buf(dst: &mut [u16], src: &str) {
    let w = wide(src);
    let n = w.len().min(dst.len());
    dst[..n].copy_from_slice(&w[..n]);
    if n == dst.len() {
        dst[n - 1] = 0;
    }
}

fn exe_dir() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.to_path_buf()))
        .unwrap_or_else(|| PathBuf::from("."))
}

fn main_exe() -> PathBuf {
    exe_dir().join("TokenWatcher.exe")
}

fn local_appdata() -> PathBuf {
    std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
        .join("TokenWatcher")
}

fn log_path() -> PathBuf {
    // 按日切分：serve-YYYY-MM-DD.log。托盘拉起 serve 时按当天日期选文件，
    // 跨天由下一次启动自然轮转；长驻期间仍写同一文件，可接受。
    let (day, _) = local_day_stamp();
    let name = if day.is_empty() {
        "serve.log".to_string()
    } else {
        format!("serve-{day}.log")
    };
    local_appdata().join("logs").join(name)
}

fn data_dir() -> PathBuf {
    std::env::var_os("USERPROFILE")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".tokenmeter")
}

/// 轻量排障日志（托盘是 GUI 进程，没有 stdout，只能落文件）。
///
/// 每行带本地时间戳与级别前缀：`[YYYY-MM-DD HH:MM:SS] [INFO] msg`。
/// 文件名按日切分（`tray-YYYY-MM-DD.log`）：写新文件即可轮转，无需清理线程，
/// 旧文件按需手动删。时间取自系统时钟（零依赖，不引第三方 crate）。
fn trace(msg: &str) {
    trace_level("INFO", msg);
}

/// 带级别的日志底层：`[YYYY-MM-DD HH:MM:SS] [LEVEL] msg`。
fn trace_level(level: &str, msg: &str) {
    let (day, stamp) = local_day_stamp();
    // 时间不可得时回退到固定名、无前缀 —— 宁可少字段也不写成 `tray-.log`。
    let (name, prefix) = if day.is_empty() {
        ("tray.log".to_string(), String::new())
    } else {
        (format!("tray-{day}.log"), format!("[{stamp}] [{level}] "))
    };
    let lp = local_appdata().join("logs").join(name);
    if let Some(dir) = lp.parent() {
        let _ = create_dir_all(dir);
    }
    if let Ok(mut f) = OpenOptions::new().create(true).append(true).open(&lp) {
        use std::io::Write;
        let _ = writeln!(f, "{prefix}{msg}");
    }
}

/// (YYYY-MM-DD, HH:MM:SS) 取自本地系统时间。零依赖：用 DOS 日期 API 拿本地时间。
/// 返回 ("", "") 表示时间不可得（极少见），此时日志仍写入固定名文件、无时间戳。
fn local_day_stamp() -> (String, String) {
    #[repr(C)]
    struct SystemTime {
        year: u16,
        month: u16,
        day_of_week: u16,
        day: u16,
        hour: u16,
        minute: u16,
        second: u16,
        milliseconds: u16,
    }
    #[link(name = "kernel32")]
    extern "system" {
        fn GetLocalTime(t: *mut SystemTime);
    }
    let mut t = SystemTime {
        year: 0,
        month: 0,
        day_of_week: 0,
        day: 0,
        hour: 0,
        minute: 0,
        second: 0,
        milliseconds: 0,
    };
    // SAFETY: t 是正确尺寸的出参。
    unsafe { GetLocalTime(&mut t) };
    if t.year == 0 {
        return (String::new(), String::new());
    }
    (
        format!("{:04}-{:02}-{:02}", t.year, t.month, t.day),
        format!(
            "{:02}:{:02}:{:02}",
            t.hour, t.minute, t.second
        ),
    )
}

/* ---------------------------------------------------------------- 配置文件 */

/// 首次运行自动生成到 exe 同目录，改完重启托盘生效。
const DEFAULT_INI: &str = r#"; ===== TokenWatcher 配置 =====
; 存成 UTF-8；改完需要重启托盘（右键退出 → 重新双击 TokenWatcherTray.exe）

[server]
; 面板端口。若被别的程序占用，改成别的（如 8789）
port = 8788

[behavior]
; 手动双击启动时是否自动打开浏览器面板（true / false）
; 开机自启拉起时一律不弹窗，与本项无关（面板用托盘图标打开）
open_panel = true

; 是否开机自动启动（true / false）
; true  = 启动时确保写入 HKCU\Software\Microsoft\Windows\CurrentVersion\Run
; false = 启动时移除该项
; 托盘右键菜单勾选/取消会自动回写这里，保持两边一致
autostart = true
"#;

#[derive(Clone, Copy)]
struct Config {
    port: u16,
    open_panel: bool,
    autostart: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            port: 8788,
            open_panel: true,
            autostart: true,
        }
    }
}

fn config_path() -> PathBuf {
    exe_dir().join("TokenWatcher.ini")
}

fn parse_bool(v: &str) -> Option<bool> {
    match v.trim().to_ascii_lowercase().as_str() {
        "true" | "1" | "yes" | "on" | "是" => Some(true),
        "false" | "0" | "no" | "off" | "否" => Some(false),
        _ => None,
    }
}

/// 读配置；文件不存在则落一份默认值。任何解析失败都退回默认，不阻塞启动。
fn load_config() -> Config {
    let p = config_path();
    let mut cfg = Config::default();
    let raw = match std::fs::read(&p) {
        Ok(b) => String::from_utf8_lossy(&b).to_string(),
        Err(_) => {
            if std::fs::write(&p, DEFAULT_INI).is_ok() {
                trace("config: 首次运行，已生成默认 TokenWatcher.ini");
            } else {
                trace("config: 目录不可写，使用内置默认值");
            }
            return cfg;
        }
    };
    for line in raw.lines() {
        let l = line.trim();
        if l.is_empty() || l.starts_with(';') || l.starts_with('#') || l.starts_with('[') {
            continue;
        }
        let Some((k, v)) = l.split_once('=') else {
            continue;
        };
        match k.trim().to_ascii_lowercase().as_str() {
            "port" => {
                if let Ok(n) = v.trim().parse::<u16>() {
                    if n > 0 {
                        cfg.port = n;
                    }
                }
            }
            "open_panel" => {
                if let Some(b) = parse_bool(v) {
                    cfg.open_panel = b;
                }
            }
            "autostart" => {
                if let Some(b) = parse_bool(v) {
                    cfg.autostart = b;
                }
            }
            _ => {}
        }
    }
    cfg
}

/// 只改 autostart 那一行，其余内容（含注释）原样保留。
fn save_autostart_flag(v: bool) {
    let p = config_path();
    let text = match std::fs::read(&p) {
        Ok(b) => String::from_utf8_lossy(&b).to_string(),
        Err(_) => DEFAULT_INI.to_string(),
    };
    let mut out = String::new();
    let mut done = false;
    for line in text.lines() {
        let l = line.trim_start().to_ascii_lowercase();
        if l.starts_with("autostart") && l.contains('=') {
            out.push_str(&format!("autostart = {}\n", v));
            done = true;
        } else {
            out.push_str(line);
            out.push('\n');
        }
    }
    if !done {
        out.push_str(&format!("autostart = {}\n", v));
    }
    if std::fs::write(&p, out).is_err() {
        trace_level("ERROR", "config: 回写 autostart 失败（目录只读？）");
    }
}

/* -------------------------------------------------------------------- 状态 */

struct State {
    child: Option<Child>,
}

static STATE: Mutex<Option<State>> = Mutex::new(None);

fn with_state<R>(f: impl FnOnce(&mut State) -> R) -> Option<R> {
    let mut guard = STATE.lock().ok()?;
    guard.as_mut().map(f)
}

/* ---------------------------------------------------------------- 服务控制 */

fn port_alive() -> bool {
    TcpStream::connect_timeout(
        &format!("127.0.0.1:{}", port()).parse().unwrap(),
        Duration::from_millis(400),
    )
    .is_ok()
}

fn open_panel() {
    trace(&format!(
        "open_panel: port={} alive={}",
        port(),
        port_alive()
    ));
    if !port_alive() {
        start_server();
        for _ in 0..16 {
            std::thread::sleep(Duration::from_millis(500));
            if port_alive() {
                break;
            }
        }
    }
    let url = format!("http://127.0.0.1:{}/", port());
    unsafe {
        ShellExecuteW(0, wide("open").as_ptr(), wide(&url).as_ptr(), null(), null(), 1);
    }
}

fn start_server() {
    if port_alive() {
        return; // 已有实例在跑（可能是手动启的），不重复拉
    }
    let exe = main_exe();
    if !exe.exists() {
        unsafe {
            MessageBoxW(
                0,
                wide("找不到 TokenWatcher.exe，请确认它和托盘程序在同一目录。").as_ptr(),
                wide("TokenWatcher").as_ptr(),
                0x10,
            );
        }
        return;
    }
    let lp = log_path();
    if let Some(dir) = lp.parent() {
        let _ = create_dir_all(dir);
    }
    let out = OpenOptions::new().create(true).append(true).open(&lp).ok();
    let err = out.as_ref().and_then(|f| f.try_clone().ok());

    let mut cmd = Command::new(&exe);
    cmd.arg("serve")
        .arg("--port")
        .arg(port().to_string())
        .arg("--no-open");
    match out {
        Some(f) => cmd.stdout(Stdio::from(f)),
        None => cmd.stdout(Stdio::null()),
    };
    match err {
        Some(f) => cmd.stderr(Stdio::from(f)),
        None => cmd.stderr(Stdio::null()),
    };
    cmd.stdin(Stdio::null());
    cmd.creation_flags(CREATE_NO_WINDOW);

    match cmd.spawn() {
        Ok(child) => {
            with_state(|s| s.child = Some(child));
        }
        Err(e) => {
            trace_level("ERROR", &format!("启动 TokenWatcher.exe 失败：{e}"));
            unsafe {
                MessageBoxW(
                    0,
                    wide(&format!("启动 TokenWatcher.exe 失败：{}", e)).as_ptr(),
                    wide("TokenWatcher").as_ptr(),
                    0x10,
                );
            }
        }
    }
}

fn stop_server() {
    with_state(|s| {
        if let Some(mut c) = s.child.take() {
            let _ = c.kill();
            let _ = c.wait();
        }
    });
}

fn run_scan() {
    let exe = main_exe();
    if !exe.exists() {
        return;
    }
    let mut cmd = Command::new(&exe);
    cmd.arg("scan").creation_flags(CREATE_NO_WINDOW);
    cmd.stdout(Stdio::null())
        .stderr(Stdio::null())
        .stdin(Stdio::null());
    let _ = cmd.spawn();
}

/* ---------------------------------------------------------------- 开机自启 */

fn read_autostart() -> Option<String> {
    unsafe {
        let mut key: HKEY = 0;
        if RegCreateKeyExW(
            HKEY_CURRENT_USER,
            wide(RUN_KEY).as_ptr(),
            0,
            null(),
            0,
            KEY_READ,
            null(),
            &mut key,
            null_mut(),
        ) != 0
        {
            return None;
        }
        let mut buf = vec![0u8; 2048];
        let mut len = buf.len() as u32;
        let mut ty = 0u32;
        let rc = RegQueryValueExW(
            key,
            wide(RUN_VALUE).as_ptr(),
            null_mut(),
            &mut ty,
            buf.as_mut_ptr(),
            &mut len,
        );
        RegCloseKey(key);
        if rc != 0 || ty != REG_SZ {
            return None;
        }
        let units: Vec<u16> = buf[..len as usize]
            .chunks_exact(2)
            .map(|c| u16::from_le_bytes([c[0], c[1]]))
            .take_while(|&c| c != 0)
            .collect();
        Some(String::from_utf16_lossy(&units))
    }
}

fn autostart_enabled() -> bool {
    match read_autostart() {
        Some(v) => {
            let me = std::env::current_exe().unwrap_or_default();
            let me_str = me.to_string_lossy().to_lowercase();
            let v = v.to_lowercase();
            v.contains(me_str.trim_matches('"'))
        }
        None => false,
    }
}

/// 开机自启注册表里应该写的完整命令行。
/// 带 `--autostart` 标记是刚需：托盘靠它识别「我是被开机拉起的」，从而不弹浏览器。
fn autostart_expected_value() -> String {
    let exe = std::env::current_exe().unwrap_or_default();
    format!("\"{}\" --autostart", exe.display())
}

/// 注册表值是否已是最新形态（含 --autostart）。老版本写的是不带参数的值，需要升级。
fn autostart_up_to_date() -> bool {
    match read_autostart() {
        Some(v) => v.trim().eq_ignore_ascii_case(&autostart_expected_value()),
        None => false,
    }
}

fn set_autostart(enable: bool) -> bool {
    unsafe {
        let mut key: HKEY = 0;
        if RegCreateKeyExW(
            HKEY_CURRENT_USER,
            wide(RUN_KEY).as_ptr(),
            0,
            null(),
            0,
            KEY_WRITE,
            null(),
            &mut key,
            null_mut(),
        ) != 0
        {
            return false;
        }
        let ok = if enable {
            let cmd = autostart_expected_value();
            let data = wide(&cmd);
            RegSetValueExW(
                key,
                wide(RUN_VALUE).as_ptr(),
                0,
                REG_SZ,
                data.as_ptr() as *const u8,
                (data.len() * 2) as u32,
            ) == 0
        } else {
            let rc = RegDeleteValueW(key, wide(RUN_VALUE).as_ptr());
            rc == 0 || GetLastError() == ERROR_FILE_NOT_FOUND
        };
        RegCloseKey(key);
        ok
    }
}

/* -------------------------------------------------------------------- 托盘 */

/// 内嵌的项目图标（由 web/apple-touch-icon.png 转出的多尺寸 ico）。
/// 不用 TokenWatcher.exe 的图标：那是 Node 运行时自带的绿圆，跟产品无关。
const ICO: &[u8] = include_bytes!("../icon.ico");

/// 从内存 ico 里挑最接近 32px 的那张，创建 HICON。
/// 系统会按 DPI 自行缩放，所以不需要在 ico 里追求每个尺寸都完美。
unsafe fn embedded_icon() -> HICON {
    let b = ICO;
    if b.len() < 6 {
        return 0;
    }
    let count = u16::from_le_bytes([b[4], b[5]]) as usize;
    let mut best: (usize, usize, i32) = (0, 0, i32::MAX);
    for i in 0..count {
        let off = 6 + i * 16;
        if off + 16 > b.len() {
            break;
        }
        let w = if b[off] == 0 { 256 } else { b[off] as i32 };
        let res = u32::from_le_bytes([b[off + 8], b[off + 9], b[off + 10], b[off + 11]]) as usize;
        let img = u32::from_le_bytes([b[off + 12], b[off + 13], b[off + 14], b[off + 15]]) as usize;
        if img + res > b.len() {
            continue;
        }
        let d = (w - 32).abs();
        if d < best.2 {
            best = (res, img, d);
        }
    }
    if best.0 == 0 {
        return 0;
    }
    CreateIconFromResourceEx(
        b.as_ptr().add(best.1),
        best.0 as u32,
        1,
        0x0003_0000,
        0,
        0,
        0,
    )
}

fn build_nid(hwnd: HWND) -> NOTIFYICONDATAW {
    let mut nid: NOTIFYICONDATAW = unsafe { std::mem::zeroed() };
    nid.cb_size = std::mem::size_of::<NOTIFYICONDATAW>() as u32;
    nid.hwnd = hwnd;
    nid.uid = 1;
    nid.u_flags = NIF_MESSAGE | NIF_ICON | NIF_TIP;
    nid.u_callback_message = WM_TRAY;
    let icon = unsafe {
        let h = embedded_icon();
        if h != 0 {
            h
        } else {
            // 兜底：内嵌图标解析失败时用系统默认图标，至少别让托盘空着
            LoadIconW(0, 32512 as *const u16)
        }
    };
    nid.h_icon = icon;
    nid
}

fn rebuild_tip(hwnd: HWND) {
    let alive = port_alive();
    let auto = autostart_enabled();
    let tip = format!(
        "TokenWatcher · {} · 自启{}",
        if alive { "运行中" } else { "未运行" },
        if auto { "开" } else { "关" }
    );
    let mut nid = build_nid(hwnd);
    copy_to_buf(&mut nid.sz_tip, &tip);
    unsafe {
        Shell_NotifyIconW(NIM_MODIFY, &nid);
    }
}

/// 首次运行弹一次气泡：Win11 默认把新图标收进 ^ 溢出区，不提示用户会找不到。
fn show_first_run_balloon(hwnd: HWND) {
    let marker = local_appdata().join(".balloon-shown");
    if marker.exists() {
        return;
    }
    let mut nid = build_nid(hwnd);
    copy_to_buf(&mut nid.sz_tip, "TokenWatcher");
    copy_to_buf(&mut nid.sz_info_title, "TokenWatcher 已在后台运行");
    copy_to_buf(
        &mut nid.sz_info,
        "图标可能收在任务栏右下角的 ^ 里，可拖到任务栏固定。双击图标打开面板，右键可设置开机自启。",
    );
    nid.u_flags = NIF_INFO | NIF_ICON | NIF_MESSAGE | NIF_TIP;
    nid.dw_info_flags = 0x1; // NIIF_INFO
    unsafe {
        Shell_NotifyIconW(NIM_MODIFY, &nid);
    }
    if let Some(dir) = marker.parent() {
        let _ = create_dir_all(dir);
    }
    let _ = File::create(&marker);
}

fn show_menu(hwnd: HWND) {
    unsafe {
        let menu = CreatePopupMenu();
        let auto = autostart_enabled();
        AppendMenuW(menu, MF_STRING, ID_OPEN, wide("打开面板").as_ptr());
        AppendMenuW(menu, MF_STRING, ID_SCAN, wide("立即扫描一次").as_ptr());
        AppendMenuW(menu, MF_SEPARATOR, 0, null());
        AppendMenuW(
            menu,
            MF_STRING | if auto { MF_CHECKED } else { 0 },
            ID_AUTOSTART,
            wide("开机自动启动").as_ptr(),
        );
        AppendMenuW(menu, MF_STRING, ID_LOG, wide("打开日志").as_ptr());
        AppendMenuW(menu, MF_STRING, ID_DATA, wide("打开数据目录").as_ptr());
        AppendMenuW(menu, MF_STRING, ID_CONFIG, wide("编辑配置 (TokenWatcher.ini)").as_ptr());
        AppendMenuW(menu, MF_SEPARATOR, 0, null());
        AppendMenuW(menu, MF_STRING, ID_EXIT, wide("退出（停止服务）").as_ptr());

        let mut pt = POINT { x: 0, y: 0 };
        GetCursorPos(&mut pt);
        // 必须先抢前台，否则点别处菜单不消失（Win32 经典坑）
        SetForegroundWindow(hwnd);
        let cmd = TrackPopupMenu(
            menu,
            TPM_RIGHTBUTTON | TPM_RETURNCMD,
            pt.x,
            pt.y,
            0,
            hwnd,
            null(),
        );
        PostMessageW(hwnd, WM_NULL, 0, 0);
        DestroyMenu(menu);

        match cmd as usize {
            ID_OPEN => open_panel(),
            ID_SCAN => run_scan(),
            ID_AUTOSTART => {
                let next = !auto;
                let _ = set_autostart(next);
                save_autostart_flag(next); // 配置文件是真源，菜单操作同步回写
                rebuild_tip(hwnd);
            }
            ID_CONFIG => {
                let p = config_path();
                if !p.exists() {
                    let _ = std::fs::write(&p, DEFAULT_INI);
                }
                ShellExecuteW(
                    0,
                    wide("open").as_ptr(),
                    wide(&p.to_string_lossy()).as_ptr(),
                    null(),
                    null(),
                    1,
                );
            }
            ID_LOG => {
                let lp = log_path();
                if let Some(dir) = lp.parent() {
                    let _ = create_dir_all(dir);
                }
                let _ = File::create(&lp);
                ShellExecuteW(
                    0,
                    wide("open").as_ptr(),
                    wide(&lp.to_string_lossy()).as_ptr(),
                    null(),
                    null(),
                    1,
                );
            }
            ID_DATA => {
                let d = data_dir();
                let _ = create_dir_all(&d);
                ShellExecuteW(
                    0,
                    wide("open").as_ptr(),
                    wide(&d.to_string_lossy()).as_ptr(),
                    null(),
                    null(),
                    1,
                );
            }
            ID_EXIT => {
                stop_server();
                let nid = build_nid(hwnd);
                Shell_NotifyIconW(NIM_DELETE, &nid);
                PostQuitMessage(0);
            }
            _ => {}
        }
    }
}

/* ---------------------------------------------------------------- 窗口过程 */

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wp: usize, lp: isize) -> LRESULT {
    match msg {
        WM_TRAY => {
            match lp as u32 {
                WM_RBUTTONUP | WM_CONTEXTMENU => show_menu(hwnd),
                WM_LBUTTONDBLCLK => open_panel(),
                _ => {}
            }
            0
        }
        WM_COMMAND => {
            match wp & 0xFFFF {
                ID_OPEN => open_panel(),
                ID_SCAN => run_scan(),
                ID_EXIT => {
                    stop_server();
                    let nid = build_nid(hwnd);
                    Shell_NotifyIconW(NIM_DELETE, &nid);
                    PostQuitMessage(0);
                }
                _ => {}
            }
            0
        }
        WM_DESTROY => {
            PostQuitMessage(0);
            0
        }
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}

/* -------------------------------------------------------------------- 入口 */

fn main() {
    unsafe {
        // 脚本/自动化入口：TokenWatcherTray.exe --set-autostart on|off
        let args: Vec<String> = std::env::args().collect();
        if let Some(i) = args.iter().position(|a| a == "--set-autostart") {
            let on = args.get(i + 1).map(|s| s == "on").unwrap_or(false);
            let ok = set_autostart(on);
            save_autostart_flag(on);
            trace(&format!("cli set-autostart {} -> {}", on, ok));
            std::process::exit(if ok { 0 } else { 1 });
        }
        if args.iter().any(|a| a == "--show-config") {
            trace(&format!(
                "cli show-config -> path={} exists={}",
                config_path().display(),
                config_path().exists()
            ));
            std::process::exit(0);
        }
        if args.iter().any(|a| a == "--query-autostart") {
            let on = autostart_enabled();
            trace(&format!("cli query-autostart -> {}", on));
            std::process::exit(if on { 0 } else { 1 });
        }

        // 被开机自启拉起时带 --autostart：此时不弹浏览器，只静默驻留托盘
        let from_boot = args.iter().any(|a| a == "--autostart");

        // 配置文件驱动：端口 / 是否自动开面板 / 是否开机自启
        let cfg = load_config();
        PORT.store(cfg.port, Ordering::Relaxed);
        trace(&format!(
            "config: port={} open_panel={} autostart={} from_boot={} path={}",
            cfg.port,
            cfg.open_panel,
            cfg.autostart,
            from_boot,
            config_path().display()
        ));

        // 单实例：已有托盘在跑就只把面板打开，不重复拉服务
        let mutex_name = wide("Local\\TokenWatcherTray.SingleInstance");
        let mutex = CreateMutexW(null(), 0, mutex_name.as_ptr());
        if GetLastError() == ERROR_ALREADY_EXISTS {
            let url = format!("http://127.0.0.1:{}/", port());
            ShellExecuteW(0, wide("open").as_ptr(), wide(&url).as_ptr(), null(), null(), 1);
            return;
        }

        *STATE.lock().unwrap() = Some(State { child: None });

        // 按配置同步开机自启：ini 是唯一真源，注册表跟着它走。
        // 用 up_to_date 而非 enabled 判定，可顺带把老版本写的「不带 --autostart」的值升级掉。
        if cfg.autostart {
            if !autostart_up_to_date() {
                let ok = set_autostart(true);
                trace(&format!("config: 写入/升级开机自启 (ok={})", ok));
            }
        } else if autostart_enabled() {
            let ok = set_autostart(false);
            trace(&format!("config: 按配置移除开机自启 (ok={})", ok));
        }

        let inst = GetModuleHandleW(null());
        let class_name = wide("TokenWatcherTrayWindow");
        let wc = WNDCLASSW {
            style: CS_DBLCLKS,
            lpfn_wnd_proc: Some(wndproc),
            cb_cls_extra: 0,
            cb_wnd_extra: 0,
            h_instance: inst,
            h_icon: 0,
            h_cursor: 0,
            hbr_background: 0,
            lpsz_menu_name: null(),
            lpsz_class_name: class_name.as_ptr(),
        };
        RegisterClassW(&wc);

        let hwnd = CreateWindowExW(
            WS_EX_TOOLWINDOW,
            class_name.as_ptr(),
            wide("TokenWatcher").as_ptr(),
            WS_POPUP,
            0,
            0,
            0,
            0,
            0,
            0,
            inst,
            null_mut(),
        );

        let mut nid = build_nid(hwnd);
        copy_to_buf(&mut nid.sz_tip, "TokenWatcher 启动中…");
        let add_rc = Shell_NotifyIconW(NIM_ADD, &nid);
        trace(&format!(
            "NIM_ADD rc={} hwnd={} hicon={} ico_len={} cbSize={}",
            add_rc,
            hwnd,
            nid.h_icon,
            ICO.len(),
            nid.cb_size
        ));

        start_server();
        std::thread::sleep(Duration::from_millis(1200));
        rebuild_tip(hwnd);
        show_first_run_balloon(hwnd);

        // 配置 open_panel = true 时自动开浏览器；开机自启拉起时一律跳过，避免每次开机弹窗
        if cfg.open_panel {
            if from_boot {
                trace("open_panel: 开机自启拉起，跳过弹窗（面板仍可用托盘图标打开）");
            } else {
                open_panel();
            }
        }

        let mut msg: MSG = std::mem::zeroed();
        while GetMessageW(&mut msg, 0, 0, 0) > 0 {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }

        // 正常退出路径：收掉子进程
        let nid2 = build_nid(hwnd);
        Shell_NotifyIconW(NIM_DELETE, &nid2);
        stop_server();
        CloseHandle(mutex);
    }
}
