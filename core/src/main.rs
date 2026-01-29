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

type CpuCache = HashMap<u32, f32>;

#[derive(Serialize, Clone)]
struct ProcessInfo {
    pid: u32,
    parent_pid: Option<u32>,
    name: String,
    ram_mb: u64,
    cpu_percent: f32,
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
struct SystemStats {
    ram: RamStats,
    cpu: CpuStats,
    processes: ProcessStats,
}

#[derive(Serialize)]
struct FluffyMessage {
    schema_version: &'static str,
    timestamp: u64,
    system: SystemStats,
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

fn collect_processes(system: &System, cpu_cache: &mut CpuCache) -> Vec<ProcessInfo> {
    system
        .processes()
        .iter()
        .map(|(pid, p)| {
            let pid_u32 = pid.as_u32();
            let cpu = p.cpu_usage();
            let smoothed = cpu_cache.insert(pid_u32, cpu).unwrap_or(cpu);

            ProcessInfo {
                pid: pid_u32,
                parent_pid: p.parent().map(|pp| pp.as_u32()),
                name: p.name().to_string_lossy().into_owned(),
                ram_mb: kib_to_mb(p.memory()),
                cpu_percent: smoothed,
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
    let mut cpu_cache = HashMap::new();

    while running.load(Ordering::SeqCst) {
        system.refresh_memory();
        system.refresh_cpu_all();
        system.refresh_processes(ProcessesToUpdate::All, false);

        let processes = collect_processes(&system, &mut cpu_cache);

        let total_mb = kib_to_mb(system.total_memory());
        let free_mb = kib_to_mb(system.available_memory());
        let used_mb = total_mb - free_mb;

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
                processes: ProcessStats { top_ram: processes },
            },
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
