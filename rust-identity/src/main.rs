use std::fs;
use tiny_keccak::{Hasher, Keccak};

// Read a text file safely, returning empty string if it fails
fn read_sys_string(path: &str) -> String {
    fs::read_to_string(path).unwrap_or_default().trim().to_string()
}

fn get_cpu_serial() -> String {
    let cpuinfo = read_sys_string("/proc/cpuinfo");
    for line in cpuinfo.lines() {
        if line.starts_with("Serial") {
            return line.split(':').last().unwrap_or("0000").trim().to_string();
        }
    }
    "DEV_ENV_NO_SERIAL".to_string()
}

fn get_mac_address() -> String {
    let wlan = read_sys_string("/sys/class/net/wlan0/address");
    if !wlan.is_empty() { return wlan; }
    
    let eth = read_sys_string("/sys/class/net/eth0/address");
    if !eth.is_empty() { return eth; }
    
    "00:00:00:00:00:00".to_string()
}

// Read the binary salt safely as raw bytes
fn get_salt() -> Vec<u8> {
    fs::read("/boot/.device_salt").unwrap_or_else(|_| {
        eprintln!("[SYS_WARN] Could not read binary salt, using fallback");
        vec![0u8; 32]
    })
}

fn main() {
    // 1. Gather all hardware identifiers matching the Python architecture's footprint
    let camera_id = "cam_01"; // Mocked for CLI PoC
    let serial = get_cpu_serial();
    let mac = get_mac_address();
    let machine_id = read_sys_string("/etc/machine-id");
    
    // 2. Read the raw binary device salt
    let salt = get_salt();

    // 3. Hash them together using Keccak256 (Intentional EVM-native upgrade)
    let mut keccak = Keccak::v256();
    let mut output = [0u8; 32];
    
    keccak.update(camera_id.as_bytes());
    keccak.update(b"|");
    keccak.update(serial.as_bytes());
    keccak.update(b"|");
    keccak.update(mac.as_bytes());
    keccak.update(b"|");
    keccak.update(machine_id.as_bytes());
    keccak.update(b"|");
    keccak.update(&salt); // Passed as raw bytes
    keccak.finalize(&mut output);

    let hardware_id_hash = hex::encode(output);
    
    println!(
        r#"{{"status": "success", "hardware_fingerprint_hash": "0x{}", "identifiers_used": ["camera_id", "cpu_serial", "mac", "machine_id", "binary_salt"]}}"#,
        hardware_id_hash
    );
}