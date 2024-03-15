use std::net::Ipv4Addr;
use std::process::Stdio;
use std::sync::mpsc;
use std::time::Duration;

use mdns_sd::ServiceDaemon;
use mdns_sd::ServiceEvent;
use mdns_sd::ServiceInfo;

use rand::distributions;
use rand::Rng;

type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;

fn main() {
    if let Err(e) = run() {
        eprintln!("{e}");
    }
}

fn run() -> Result<()> {
    let mut rng = rand::thread_rng();
    let mdns = ServiceDaemon::new()?;
    let service_name = "_adb-tls-pairing._tcp.local.";
    let (tx, rx) = mpsc::channel();
    let receiver = mdns.browse(service_name).expect("failed to browse service");
    let password: String = (0..7)
        .map(|_| rng.sample(distributions::Alphanumeric) as char)
        .collect();

    let name = format!(
        "ADB_WIFI_{}",
        (0..7)
            .map(|_| rng.sample(distributions::Alphanumeric) as char)
            .collect::<String>()
    );
    let adb_url = format!("WIFI:T:ADB;S:{};P:{};;", name, password);
    println!("\n\n");
    qr2term::print_qr(adb_url)?;
    println!("\n\n");

    let discovery = std::thread::spawn(move || {
        while let Ok(event) = receiver.recv() {
            if let ServiceEvent::ServiceResolved(service) = event {
                tx.send(Device::new(service))
                    .expect("could not send device through channel receiever")
            }
        }
    });

    if let Ok(device) = rx.recv_timeout(Duration::from_secs(30)) {
        let pair_status = device.pair(&password)?;
        mdns.shutdown().expect("failed to shutdown mdns");
        discovery.join().expect("failed to join thread: discovery");
        if pair_status.success() {
            println!("device at {} is paired is paired successfully", device);
        } else {
            eprintln!("failed to pair device {}", device);
        }
    } else {
        eprintln!("connection timed out.");
    }

    Ok(())
}

#[derive(Debug, Copy, Clone)]
struct Device {
    ip: Ipv4Addr,
    port: u16,
}

impl Device {
    fn new(service_info: ServiceInfo) -> Self {
        Self {
            // this is not a good way to get IP
            // however, we can ignore this as `ServiceEvent::ServiceResolved`
            // takes care of this already
            ip: **service_info
                // regular get_addresses will pollute with
                // IPv6 addresses which adb can't use
                .get_addresses_v4()
                .iter()
                .next()
                .expect("could not find ip address"),
            port: service_info.get_port(),
        }
    }
    fn pair(&self, password: &str) -> Result<std::process::ExitStatus> {
        let adb = std::process::Command::new("adb")
            .args(["pair", &self.to_string(), password])
            .stdout(Stdio::piped())
            .spawn()?
            .wait()?;
        Ok(adb)
    }
}

impl std::fmt::Display for Device {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}:{}", self.ip, self.port)
    }
}
