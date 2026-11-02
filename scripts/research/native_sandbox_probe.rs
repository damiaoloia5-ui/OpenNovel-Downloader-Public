//! Original fixture executable, never shipped as the content engine.
#![windows_subsystem = "windows"]
use std::{io::{self, BufRead, Write}, time::Duration};
fn main() {
    for line in io::stdin().lock().lines() {
        let line = line.unwrap();
        if let Some(path) = line.strip_prefix("read:") {
            println!("read_denied:{}", std::fs::read(path).is_err());
        } else if let Some(path) = line.strip_prefix("write:") {
            println!("write_denied:{}", std::fs::write(path, b"must not write").is_err());
        } else if let Some(address) = line.strip_prefix("connect:") {
            println!("network_denied:{}", std::net::TcpStream::connect_timeout(&address.parse().unwrap(), Duration::from_secs(2)).is_err());
        } else if line == "spawn" {
            println!("spawn_denied:{}", std::process::Command::new(std::env::current_exe().unwrap()).spawn().is_err());
        } else if line == "exit" { return; }
        else { println!("ready"); }
        io::stdout().flush().unwrap();
    }
}
