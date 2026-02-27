use std::path::PathBuf;
use fuser::{BackgroundSession, Config, Filesystem};
use anyhow::Result;

use crate::filesystem::errors::MountError;

mod errors;

type OnUnmount = Box<dyn Fn() + Send + Sync + 'static>;

pub struct Lifetimefs {
    pub mountpoint: PathBuf,
    on_unmount: OnUnmount,
}

impl Filesystem for Lifetimefs {
    fn destroy(&mut self) {
        (self.on_unmount)()
    }
}

impl Lifetimefs {
    pub fn new(on_unmount: OnUnmount, mountpoint: &PathBuf) -> Result<Lifetimefs, MountError> {
        if mountpoint.exists() {
            Ok(Lifetimefs { on_unmount, mountpoint: mountpoint.to_path_buf() })
        } else {
            Err(MountError::PathNotExists(mountpoint.to_path_buf()))
        }
    }

    pub fn mount(self) -> Result<BackgroundSession> {
        let options = Config::default();
        let mountpoint = &self.mountpoint.clone();
        
        let session = fuser::spawn_mount2(self, mountpoint, &options)?;
        
        Ok(session)
    }
}

