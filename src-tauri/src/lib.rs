use serialport::SerialPort;
use std::sync::Mutex;
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

struct PrinterState(Mutex<Option<Box<dyn SerialPort>>>);

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
    *printer = None;
    
    emit_log(&app, &format!("Connecting to {}...", port_path), "info");
    
    let port_builder = serialport::new(&port_path, 9600)
        .timeout(Duration::from_millis(1000));
        
    match port_builder.open() {
        Ok(port) => {
            *printer = Some(port);
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
    if printer.is_some() {
        *printer = None; // This drops the port and closes it
        emit_log(&app, "Port closed", "info");
    }
    true
}

#[tauri::command]
fn send_command(app: AppHandle, state: State<'_, PrinterState>, bytes: Vec<u8>) -> bool {
    let mut printer_opt = state.0.lock().unwrap();
    
    if let Some(printer) = printer_opt.as_mut() {
        let hex_string: Vec<String> = bytes.iter().map(|b| format!("{:02X}", b)).collect();
        let hex_str = hex_string.join(" ");
        
        match printer.write_all(&bytes) {
            Ok(_) => {
                let _ = printer.flush();
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
