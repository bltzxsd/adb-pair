use std::io::Write;
use std::net::Ipv4Addr;
use std::process::ExitStatus;
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
        eprintln!("{}", e);
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
                tx.send(Device::new(service)).expect("mpsc channel failure")
            }
        }
    });
    let device = rx.recv_timeout(Duration::from_secs(30));

    match device {
        Ok(device) => {
            let pair_status = device.pair(&password)?;
            mdns.shutdown().expect("failed to shutdown mdns");
            discovery.join().expect("failed to join thread: discovery");
            if pair_status.success() {
                println!("device at {} is paired is paired successfully", device);
            } else {
                return Err(format!("failed to pair device: {}", device).into());
            }
        }
        Err(a) => return Err(a.into()),
    }

    let mut input = String::new();
    print!("Please enter the port for your device: ");
    let _ = std::io::stdout().flush();
    std::io::stdin().read_line(&mut input)?;
    let port = input.trim_end().parse::<u16>();
    match port {
        Ok(port) => {
            let connect_status = device?.connect(port)?;
            if connect_status.success() {
                println!("device connected successfully");
                return Ok(());
            }
            return Err("failed to connect to device. please use adb connect".into());
        }
        Err(_) => return Err("failed to parse port. please use with adb connect".into()),
    }
}

#[derive(Debug, Copy, Clone)]
struct Device {
    ip: Ipv4Addr,
    port: u16,
}

impl Device {
    fn new(service_info: ServiceInfo) -> Self {
        Self {
            // this is not a good way to get anything from a HashSet
            // however, this is okay as `ServiceEvent::ServiceResolved`
            // has atleast one IPv4 present after discovery resolution
            ip: **service_info
                .get_addresses_v4()
                .iter()
                .next()
                .expect("could not find ip address"),
            port: service_info.get_port(),
        }
    }
    fn pair(&self, password: &str) -> Result<ExitStatus> {
        let adb = std::process::Command::new("adb")
            .args(["pair", &self.to_string(), password])
            .stdout(Stdio::piped())
            .spawn()?
            .wait()?;
        Ok(adb)
    }

    fn connect(&self, port: u16) -> Result<ExitStatus> {
        let adb = std::process::Command::new("adb")
            .args(["connect", format!("{}:{}", self.ip, port).as_str()])
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
