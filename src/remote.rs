use lazy_static::lazy_static;
use regex::Regex;
use std::process::{Command, Stdio};

#[derive(Debug)]
pub struct RemoteConfig {
    edid: Vec<u8>,
    area: u32,
}

impl RemoteConfig {
    pub fn area(&self) -> u32 {
        self.area
    }

    pub fn edid(&self) -> &[u8] {
        &self.edid
    }

    pub fn new(edid: Vec<u8>, area: u32) -> RemoteConfig {
        RemoteConfig { edid, area }
    }

    pub fn get() -> RemoteConfig {
        Self::new(Self::get_edid(), Self::get_area())
    }

    fn get_edid() -> Vec<u8> {
        let cmd = Command::new("get-edid")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();

        let out = cmd.wait_with_output().unwrap();
        if !out.status.success() {
            panic!(
                "get-edid failed:\n\n{}",
                String::from_utf8_lossy(&out.stderr)
            )
        }

        out.stdout
    }

    fn get_area() -> u32 {
        let cmd = Command::new("xrandr")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();

        let out = cmd.wait_with_output().unwrap();
        if !out.status.success() {
            panic!("xrandr failed:\n\n{}", String::from_utf8_lossy(&out.stderr))
        }

        let lines: Vec<String> = String::from_utf8(out.stdout)
            .unwrap()
            .lines()
            .filter(|line| line.contains(" connected"))
            .map(|line| line.to_string())
            .collect();

        if lines.len() != 1 {
            panic!("Invalid number of connected devices {:?}", lines.len())
        }

        lazy_static! {
            static ref RE: Regex =
                Regex::new(r" (?P<w>[0-9]+)x(?P<h>[0-9]+)\+[0-9]+\+[0-9]+ ").unwrap();
        }

        let captures = RE.captures(&lines[0]).unwrap();
        let w: u32 = captures.name("w").unwrap().as_str().parse().unwrap();
        let h: u32 = captures.name("h").unwrap().as_str().parse().unwrap();

        w * h
    }
}
