use serialport::SerialPort;
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager, State};
use serde::{Serialize, Deserialize};

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

struct PrinterHandle {
    port: Box<dyn SerialPort>,
    keep_reading: Arc<AtomicBool>,
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
fn connect_port(app: AppHandle, state: State<'_, PrinterState>, port_path: String) -> bool {
    let mut printer = state.0.lock().unwrap();
    
    // Close existing if open
    if let Some(existing) = printer.take() {
        existing.keep_reading.store(false, Ordering::SeqCst);
    }
    
    emit_log(&app, &format!("Connecting to {}...", port_path), "info");
    
    let port_builder = serialport::new(&port_path, 9600)
        .timeout(Duration::from_millis(50));
        
    match port_builder.open() {
        Ok(port) => {
            let keep_reading = Arc::new(AtomicBool::new(true));
            let keep_reading_clone = keep_reading.clone();
            
            // Try to clone the port for reading
            match port.try_clone() {
                Ok(mut read_port) => {
                    let app_clone = app.clone();
                    std::thread::spawn(move || {
                        let mut serial_buf: Vec<u8> = vec![0; 1000];
                        while keep_reading_clone.load(Ordering::SeqCst) {
                            match read_port.read(serial_buf.as_mut_slice()) {
                                Ok(t) => {
                                    if t > 0 {
                                        let bytes = &serial_buf[..t];
                                        let hex_string: Vec<String> = bytes.iter().map(|b| format!("{:02X}", b)).collect();
                                        let hex_str = hex_string.join(" ");
                                        emit_log(&app_clone, &format!("[RECV] {}", hex_str), "info");
                                    }
                                },
                                Err(ref e) if e.kind() == std::io::ErrorKind::TimedOut => (),
                                Err(_) => {
                                    break;
                                }
                            }
                            std::thread::sleep(Duration::from_millis(10));
                        }
                    });
                },
                Err(e) => {
                    emit_log(&app, &format!("Warning: Could not enable read support: {}", e), "error");
                }
            }
            
            *printer = Some(PrinterHandle {
                port,
                keep_reading,
            });
            emit_log(&app, &format!("Connected to {}", port_path), "success");
            true
        }
        Err(e) => {
            emit_log(&app, &format!("Failed to connect to {}: {}", port_path, e), "error");
            false
        }
    }
}

#[tauri::command]
fn disconnect_port(app: AppHandle, state: State<'_, PrinterState>) -> bool {
    let mut printer = state.0.lock().unwrap();
    if let Some(existing) = printer.take() {
        existing.keep_reading.store(false, Ordering::SeqCst);
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
        
        match handle.port.write_all(&bytes) {
            Ok(_) => {
                let _ = handle.port.flush();
                emit_log(&app, &format!("Sent bytes: {}", hex_str), "success");
                true
            }
            Err(e) => {
                emit_log(&app, &format!("Write error: {}", e), "error");
                false
            }
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
        .invoke_handler(tauri::generate_handler![get_ports, connect_port, disconnect_port, send_command])
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
