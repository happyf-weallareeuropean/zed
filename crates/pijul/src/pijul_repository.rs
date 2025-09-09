use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::Result;
use gpui::SharedString;

#[derive(Debug, Clone)]
pub struct Channel {
    pub name: SharedString,
}

pub trait PijulRepository: Send + Sync {
    fn list_channels(&self) -> Vec<Channel>;
}

pub struct RealPijulRepository {
    cwd: PathBuf,
}

impl RealPijulRepository {
    pub fn new(cwd: &Path) -> Result<Self> {
        Ok(Self {
            cwd: cwd.to_path_buf(),
        })
    }
}

impl PijulRepository for RealPijulRepository {
    fn list_channels(&self) -> Vec<Channel> {
        let output = Command::new("pijul")
            .arg("channel")
            .arg("list")
            .current_dir(&self.cwd)
            .output();

        match output {
            Ok(output) if output.status.success() => String::from_utf8_lossy(&output.stdout)
                .lines()
                .map(|line| Channel {
                    name: line.trim().to_string().into(),
                })
                .collect(),
            _ => Vec::new(),
        }
    }
}

pub struct FakePijulRepository {}

impl PijulRepository for FakePijulRepository {
    fn list_channels(&self) -> Vec<Channel> {
        Vec::new()
    }
}
