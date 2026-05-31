use serialport::SerialPort;
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager, State};
use serde::{Serialize, Deserialize};
use std::net::TcpStream;
use std::io::Write;
use std::io::Read;

#[cfg(windows)]
use windows::Win32::Graphics::Printing::{
    EnumPrintersW, OpenPrinterW, WritePrinter, StartDocPrinterW, StartPagePrinter,
    EndPagePrinter, EndDocPrinter, ClosePrinter, PRINTER_ENUM_LOCAL, PRINTER_ENUM_CONNECTIONS,
    PRINTER_INFO_4W, DOC_INFO_1W,
};
#[cfg(windows)]
use windows::core::{PWSTR, PCWSTR};

#[derive(Serialize)]
struct PortInfo {
    path: String,
    manufacturer: String,
}

#[derive(Serialize, Clone)]
struct LogPayload {
    message: String,
    #[serde(rename = "type")]
    log_type: String,
}

enum ConnectionMode {
    Serial(Box<dyn SerialPort>),
    Tcp(TcpStream),
    #[cfg(windows)]
    WindowsSpooler(windows::Win32::Graphics::Printing::HANDLE),
}

struct PrinterHandle {
    mode: ConnectionMode,
    keep_reading: Option<Arc<AtomicBool>>,
}

struct PrinterState(Mutex<Option<PrinterHandle>>);

fn emit_log(app: &AppHandle, message: &str, log_type: &str) {
    let payload = LogPayload {
        message: message.to_string(),
        log_type: log_type.to_string(),
    };
    let _ = app.emit("printer-log", payload);
}

#[tauri::command]
fn get_ports(app: AppHandle) -> Vec<PortInfo> {
    match serialport::available_ports() {
        Ok(ports) => {
            ports.into_iter().map(|p| {
                let manufacturer = match p.port_type {
                    serialport::SerialPortType::UsbPort(info) => info.manufacturer.unwrap_or_default(),
                    _ => String::new(),
                };
                PortInfo {
                    path: p.port_name,
                    manufacturer,
                }
            }).collect()
        }
        Err(e) => {
            emit_log(&app, &format!("Error listing ports: {}", e), "error");
            vec![]
        }
    }
}

#[tauri::command]
fn get_windows_printers(app: AppHandle) -> Vec<String> {
    #[cfg(windows)]
    {
        unsafe {
            let flags = PRINTER_ENUM_LOCAL | PRINTER_ENUM_CONNECTIONS;
            let mut cb_needed: u32 = 0;
            let mut c_returned: u32 = 0;
            
            // First call to get size needed
            let _ = EnumPrintersW(
                flags,
                PCWSTR::null(),
                4,
                None,
                &mut cb_needed,
                &mut c_returned,
            );
            
            if cb_needed == 0 {
                return vec![];
            }
            
            let mut buffer: Vec<u8> = vec![0; cb_needed as usize];
            let success = EnumPrintersW(
                flags,
                PCWSTR::null(),
                4,
                Some(buffer.as_mut_ptr()),
                &mut cb_needed,
                &mut c_returned,
            );
            
            if success.is_ok() {
                let mut printers = Vec::new();
                let info_array: *const PRINTER_INFO_4W = buffer.as_ptr() as *const PRINTER_INFO_4W;
                
                for i in 0..c_returned {
                    let info = &*info_array.offset(i as isize);
                    if !info.pPrinterName.is_null() {
                        if let Ok(name) = info.pPrinterName.to_string() {
                            printers.push(name);
                        }
                    }
                }
                return printers;
            } else {
                emit_log(&app, "Error enumerating Windows printers", "error");
            }
        }
    }
    vec![]
}

#[tauri::command]
fn connect_port(app: AppHandle, state: State<'_, PrinterState>, port_path: String, mode: String) -> bool {
    let mut printer = state.0.lock().unwrap();
    
    // Close existing if open
    if let Some(existing) = printer.take() {
        if let Some(kr) = existing.keep_reading {
            kr.store(false, Ordering::SeqCst);
        }
        #[cfg(windows)]
        if let ConnectionMode::WindowsSpooler(handle) = existing.mode {
            unsafe { let _ = ClosePrinter(handle); }
        }
    }
    
    emit_log(&app, &format!("Connecting to {} ({})", port_path, mode), "info");
    
    if mode == "serial" {
        let port_builder = serialport::new(&port_path, 9600)
            .timeout(Duration::from_millis(50));
            
        match port_builder.open() {
            Ok(port) => {
                let keep_reading = Arc::new(AtomicBool::new(true));
                let keep_reading_clone = keep_reading.clone();
                
                match port.try_clone() {
                    Ok(mut read_port) => {
                        let app_clone = app.clone();
                        std::thread::spawn(move || {
                            let mut serial_buf: Vec<u8> = vec![0; 1000];
                            while keep_reading_clone.load(Ordering::SeqCst) {
                                match read_port.read(serial_buf.as_mut_slice()) {
                                    Ok(t) if t > 0 => {
                                        let bytes = &serial_buf[..t];
                                        let hex_string: Vec<String> = bytes.iter().map(|b| format!("{:02X}", b)).collect();
                                        emit_log(&app_clone, &format!("[RECV] {}", hex_string.join(" ")), "info");
                                    },
                                    Err(ref e) if e.kind() == std::io::ErrorKind::TimedOut => (),
                                    Err(_) => break,
                                    _ => (),
                                }
                                std::thread::sleep(Duration::from_millis(10));
                            }
                        });
                    },
                    Err(_) => (),
                }
                
                *printer = Some(PrinterHandle {
                    mode: ConnectionMode::Serial(port),
                    keep_reading: Some(keep_reading),
                });
                emit_log(&app, &format!("Connected to {}", port_path), "success");
                return true;
            }
            Err(e) => {
                emit_log(&app, &format!("Failed to connect: {}", e), "error");
                return false;
            }
        }
    } else if mode == "tcp" {
        match TcpStream::connect_timeout(&port_path.parse().unwrap_or_else(|_| "127.0.0.1:9100".parse().unwrap()), Duration::from_secs(3)) {
            Ok(stream) => {
                let _ = stream.set_read_timeout(Some(Duration::from_millis(50)));
                let keep_reading = Arc::new(AtomicBool::new(true));
                let keep_reading_clone = keep_reading.clone();
                
                if let Ok(mut read_stream) = stream.try_clone() {
                    let app_clone = app.clone();
                    std::thread::spawn(move || {
                        let mut buf: Vec<u8> = vec![0; 1000];
                        while keep_reading_clone.load(Ordering::SeqCst) {
                            match read_stream.read(buf.as_mut_slice()) {
                                Ok(t) if t > 0 => {
                                    let hex_string: Vec<String> = buf[..t].iter().map(|b| format!("{:02X}", b)).collect();
                                    emit_log(&app_clone, &format!("[RECV] {}", hex_string.join(" ")), "info");
                                },
                                Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock || e.kind() == std::io::ErrorKind::TimedOut => (),
                                Err(_) => break,
                                _ => (),
                            }
                            std::thread::sleep(Duration::from_millis(10));
                        }
                    });
                }
                
                *printer = Some(PrinterHandle {
                    mode: ConnectionMode::Tcp(stream),
                    keep_reading: Some(keep_reading),
                });
                emit_log(&app, &format!("Connected to TCP {}", port_path), "success");
                return true;
            }
            Err(e) => {
                emit_log(&app, &format!("TCP Connect error: {}", e), "error");
                return false;
            }
        }
    } else if mode == "windows" {
        #[cfg(windows)]
        {
            let mut handle: windows::Win32::Graphics::Printing::HANDLE = windows::Win32::Graphics::Printing::HANDLE::default();
            let mut port_utf16: Vec<u16> = port_path.encode_utf16().chain(std::iter::once(0)).collect();
            unsafe {
                let success = OpenPrinterW(PCWSTR(port_utf16.as_ptr()), &mut handle, None);
                if success.is_ok() {
                    *printer = Some(PrinterHandle {
                        mode: ConnectionMode::WindowsSpooler(handle),
                        keep_reading: None,
                    });
                    emit_log(&app, &format!("Connected to Windows Printer {}", port_path), "success");
                    return true;
                } else {
                    emit_log(&app, "Failed to open Windows Printer", "error");
                    return false;
                }
            }
        }
        #[cfg(not(windows))]
        {
            emit_log(&app, "Windows Spooler not supported on this OS", "error");
            return false;
        }
    }
    
    false
}

#[tauri::command]
fn disconnect_port(app: AppHandle, state: State<'_, PrinterState>) -> bool {
    let mut printer = state.0.lock().unwrap();
    if let Some(existing) = printer.take() {
        if let Some(kr) = existing.keep_reading {
            kr.store(false, Ordering::SeqCst);
        }
        #[cfg(windows)]
        if let ConnectionMode::WindowsSpooler(handle) = existing.mode {
            unsafe { let _ = ClosePrinter(handle); }
        }
        emit_log(&app, "Port closed", "info");
    }
    true
}

#[tauri::command]
fn send_command(app: AppHandle, state: State<'_, PrinterState>, bytes: Vec<u8>) -> bool {
    let mut printer_opt = state.0.lock().unwrap();
    
    if let Some(handle) = printer_opt.as_mut() {
        let hex_string: Vec<String> = bytes.iter().map(|b| format!("{:02X}", b)).collect();
        let hex_str = hex_string.join(" ");
        
        let success = match &mut handle.mode {
            ConnectionMode::Serial(port) => {
                if port.write_all(&bytes).is_ok() {
                    let _ = port.flush();
                    true
                } else { false }
            },
            ConnectionMode::Tcp(stream) => {
                if stream.write_all(&bytes).is_ok() {
                    let _ = stream.flush();
                    true
                } else { false }
            },
            #[cfg(windows)]
            ConnectionMode::WindowsSpooler(win_handle) => {
                unsafe {
                    let mut doc_name: Vec<u16> = "LX350 Panel Print\0".encode_utf16().collect();
                    let mut datatype: Vec<u16> = "RAW\0".encode_utf16().collect();
                    
                    let doc_info = DOC_INFO_1W {
                        pDocName: PWSTR(doc_name.as_mut_ptr()),
                        pOutputFile: PWSTR::null(),
                        pDatatype: PWSTR(datatype.as_mut_ptr()),
                    };
                    
                    if StartDocPrinterW(*win_handle, 1, &doc_info as *const _ as *const u8).is_ok() {
                        if StartPagePrinter(*win_handle).is_ok() {
                            let mut bytes_written = 0;
                            let res = WritePrinter(*win_handle, bytes.as_ptr() as *const _, bytes.len() as u32, &mut bytes_written);
                            let _ = EndPagePrinter(*win_handle);
                            let _ = EndDocPrinter(*win_handle);
                            res.is_ok()
                        } else {
                            let _ = EndDocPrinter(*win_handle);
                            false
                        }
                    } else { false }
                }
            }
        };
        
        if success {
            emit_log(&app, &format!("Sent bytes: {}", hex_str), "success");
            true
        } else {
            emit_log(&app, "Write error", "error");
            false
        }
    } else {
        emit_log(&app, "Cannot write: Port is not open", "error");
        false
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(PrinterState(Mutex::new(None)))
        .invoke_handler(tauri::generate_handler![get_ports, get_windows_printers, connect_port, disconnect_port, send_command])
        .setup(|app| {
            if cfg!(debug_assertions) {
                app.handle().plugin(
                    tauri_plugin_log::Builder::default()
                        .level(log::LevelFilter::Info)
                        .build(),
                )?;
            }
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
