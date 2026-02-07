mod ipc;
mod permissions;

use ipc::protocol::IpcMessage;
use ipc::receiver::start_command_server;
use ipc::server::IpcServer;

use serde::Serialize;
use std::collections::HashMap;
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use sysinfo::{ProcessesToUpdate, System};

type CpuHistory = HashMap<u32, f32>;

#[derive(Serialize, Clone)]
struct ProcessInfo {
    pid: u32,
    parent_pid: Option<u32>,
    name: String,
    exe_path: String,
    ram_mb: u64,
    cpu_percent: f32,
    disk_read_kb: u64,
    disk_written_kb: u64,
}

#[derive(Serialize)]
struct ProcessStats {
    top_ram: Vec<ProcessInfo>,
}

#[derive(Serialize)]
struct RamStats {
    total_mb: u64,
    used_mb: u64,
    free_mb: u64,
}

#[derive(Serialize)]
struct CpuStats {
    usage_percent: f32,
}

#[derive(Serialize)]
struct NetworkStats {
    received_kb: u64,
    transmitted_kb: u64,
}

#[derive(Serialize)]
struct SystemStats {
    ram: RamStats,
    cpu: CpuStats,
    network: NetworkStats,
    processes: ProcessStats,
}

#[derive(Serialize)]
struct FluffyMessage {
    schema_version: &'static str,
    timestamp: u64,
    system: SystemStats,
    persistence: Vec<String>,
}

fn kib_to_mb(kib: u64) -> u64 {
    kib / 1024 / 1024
}

fn unix_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

#[cfg(target_os = "windows")]
fn get_startup_entries() -> Vec<String> {
    use windows_sys::Win32::System::Registry::{
        RegCloseKey, RegEnumValueW, RegOpenKeyExW, HKEY_CURRENT_USER, KEY_READ, LSTATUS,
    };
    use std::ptr;

    let mut entries = Vec::new();
    let subkey = encode_wide("Software\\Microsoft\\Windows\\CurrentVersion\\Run");
    let mut hkey = 0;

    unsafe {
        if RegOpenKeyExW(HKEY_CURRENT_USER, subkey.as_ptr(), 0, KEY_READ, &mut hkey) == 0 {
            let mut index = 0;
            loop {
                let mut name = [0u16; 260];
                let mut name_len = name.len() as u32;
                let mut data = [0u8; 1024];
                let mut data_len = data.len() as u32;

                let res: LSTATUS = RegEnumValueW(
                    hkey,
                    index,
                    name.as_mut_ptr(),
                    &mut name_len,
                    ptr::null_mut(),
                    ptr::null_mut(),
                    data.as_mut_ptr(),
                    &mut data_len,
                );

                if res != 0 {
                    break;
                }

                let path_str = String::from_utf16_lossy(&name[..name_len as usize]);
                let val_str = String::from_utf16_lossy(
                    &data.chunks_exact(2)
                        .map(|c| u16::from_ne_bytes([c[0], c[1]]))
                        .collect::<Vec<u16>>()[..(data_len as usize / 2)],
                );
                
                entries.push(val_str.trim_matches(char::from(0)).to_string());
                index += 1;
            }
            RegCloseKey(hkey);
        }
    }
    entries
}

fn encode_wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

#[cfg(not(target_os = "windows"))]
fn get_startup_entries() -> Vec<String> {
    Vec::new()
}

fn collect_processes(system: &System, cpu_history: &mut CpuHistory) -> Vec<ProcessInfo> {
    system
        .processes()
        .iter()
        .map(|(pid, p)| {
            let pid_u32 = pid.as_u32();
            let cpu = p.cpu_usage();
            
            // Explicit smoothing: 70% new, 30% old
            let old_cpu = cpu_history.get(&pid_u32).cloned().unwrap_or(cpu);
            let smoothed = (cpu * 0.7) + (old_cpu * 0.3);
            cpu_history.insert(pid_u32, smoothed);

            let disk = p.disk_usage();

            ProcessInfo {
                pid: pid_u32,
                parent_pid: p.parent().map(|pp| pp.as_u32()),
                name: p.name().to_string_lossy().into_owned(),
                exe_path: p.exe().map(|path| path.to_string_lossy().into_owned()).unwrap_or_default(),
                ram_mb: kib_to_mb(p.memory()),
                cpu_percent: smoothed,
                disk_read_kb: disk.read_bytes / 1024,
                disk_written_kb: disk.written_bytes / 1024,
            }
        })
        .collect()
}

fn main() {
    let ipc = IpcServer::start(9001);
    start_command_server(9002);

    let running = Arc::new(AtomicBool::new(true));
    let r = running.clone();

    ctrlc::set_handler(move || {
        eprintln!("\n[Fluffy Core] Shutdown signal received");
        r.store(false, Ordering::SeqCst);
    })
    .expect("Failed to set Ctrl+C handler");

    let mut system = System::new_all();
    let mut cpu_history = CpuHistory::new();

    while running.load(Ordering::SeqCst) {
        system.refresh_memory();
        system.refresh_cpu_all();
        system.refresh_processes_with_spec(ProcessesToUpdate::All, true);
        system.refresh_networks_all();

        let mut processes = collect_processes(&system, &mut cpu_history);
        processes.sort_by(|a, b| b.ram_mb.cmp(&a.ram_mb));

        let total_mb = kib_to_mb(system.total_memory());
        let free_mb = kib_to_mb(system.available_memory());
        let used_mb = total_mb - free_mb;

        let mut total_received = 0;
        let mut total_transmitted = 0;
        for (_, data) in system.networks() {
            total_received += data.received();
            total_transmitted += data.transmitted();
        }

        let message = FluffyMessage {
            schema_version: "1.0",
            timestamp: unix_timestamp(),
            system: SystemStats {
                ram: RamStats {
                    total_mb,
                    used_mb,
                    free_mb,
                },
                cpu: CpuStats {
                    usage_percent: system.global_cpu_usage(),
                },
                network: NetworkStats {
                    received_kb: total_received / 1024,
                    transmitted_kb: total_transmitted / 1024,
                },
                processes: ProcessStats { top_ram: processes },
            },
            persistence: get_startup_entries(),
        };

        let payload = serde_json::to_value(&message).unwrap();

        ipc.broadcast(&IpcMessage {
            schema_version: "1.0".to_string(),
            payload,
        });

        // 👇 IMPORTANT: sleep in small chunks so shutdown is responsive
        for _ in 0..20 {
            if !running.load(Ordering::SeqCst) {
                break;
            }
            thread::sleep(Duration::from_millis(100));
        }
    }

    // 👇 clean exit point
    eprintln!("[Fluffy Core] Clean shutdown complete");
}
