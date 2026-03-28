use std::fs;
use tiny_keccak::{Hasher, Keccak};

/// Missing sudo privileges or running this on a Mac during dev shouldn't crash the app.
fn get_hardware_entropy() -> Result<String, std::io::Error> {
    let cpuinfo_result = fs::read_to_string("/proc/cpuinfo");
    
    match cpuinfo_result {
        Ok(cpuinfo) => {
            for line in cpuinfo.lines() {
                if line.starts_with("Serial") {
                    let serial = line.split(':').last().unwrap_or("0000").trim();
                    return Ok(serial.to_string());
                }
            }
            // Handled fallback if /proc/cpuinfo exists but lacks a serial
            Ok("DEV_ENV_NO_HARDWARE_SERIAL".to_string())
        }
        Err(e) => {
            // If the file doesn't exist (e.g., running on Windows/Mac during dev)
            eprintln!("[SYS_WARN] Could not read /proc/cpuinfo: {}", e);
            Ok("DEV_ENV_FALLBACK_ENTROPY".to_string())
        }
    }
}

fn main() {
    // 1. Gathers hardware entropy gracefully
    let serial = match get_hardware_entropy() {
        Ok(s) => s,
        Err(_) => "CRITICAL_FAILURE_FALLBACK".to_string(), 
    };

    // Note: MAC address is mocked for this PoC. 
    // In Phase 2, we can pull the real wlan0 physical address.
    let mac = "00:00:00:00:00:00"; 

    // 2. Hash using Keccak256 (EVM Standard)
    let mut keccak = Keccak::v256();
    let mut output = [0u8; 32];
    
    keccak.update(serial.as_bytes());
    keccak.update(b"-"); // Delimiter
    keccak.update(mac.as_bytes());
    keccak.finalize(&mut output);

    let hardware_key = hex::encode(output);
    
    // 3. Output strict JSON to stdout so the Python subprocess can parse it seamlessly
    println!(
        r#"{{"status": "success", "hardware_key": "0x{}", "entropy_source": "proc_cpuinfo"}}"#,
        hardware_key
    );
}