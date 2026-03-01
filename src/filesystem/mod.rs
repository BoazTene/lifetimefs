use anyhow::Result;
use fuser::{
    BackgroundSession, BsdFileFlags, Config, Errno, FileAttr, FileHandle, Filesystem, INodeNo,
    RenameFlags, ReplyAttr, ReplyData, ReplyEmpty, ReplyEntry, Request, TimeOrNow,
};
use std::{
    collections::HashMap,
    ffi::{OsStr, OsString},
    io::ErrorKind,
    path::{Path, PathBuf},
    process::Command,
    time::SystemTime,
};

mod errors;
pub mod storage;
pub mod storage_instance;

use storage_instance::StorageInstance;

use crate::filesystem::{errors::MountError, storage::Storage};

type OnUnmount = Box<dyn Fn() + Send + Sync + 'static>;

pub struct Lifetimefs {
    pub mountpoint: PathBuf,
    on_unmount: OnUnmount,
    storage: StorageInstance,
}

impl Filesystem for Lifetimefs {
    fn destroy(&mut self) {
        // Handles service cleanup.
        (self.on_unmount)()
    }

    fn lookup(&self, _req: &Request, parent: INodeNo, name: &OsStr, reply: ReplyEntry) {}

    fn getattr(&self, _req: &Request, ino: INodeNo, fh: Option<FileHandle>, reply: ReplyAttr) {}

    fn setattr(
        &self,
        _req: &Request,
        ino: INodeNo,
        mode: Option<u32>,
        uid: Option<u32>,
        gid: Option<u32>,
        size: Option<u64>,
        _atime: Option<TimeOrNow>,
        _mtime: Option<TimeOrNow>,
        _ctime: Option<SystemTime>,
        fh: Option<FileHandle>,
        _crtime: Option<SystemTime>,
        _chgtime: Option<SystemTime>,
        _bkuptime: Option<SystemTime>,
        flags: Option<BsdFileFlags>,
        reply: ReplyAttr,
    ) {
    }

    fn readlink(&self, _req: &Request, ino: INodeNo, reply: ReplyData) {}

    /// Create file node.
    /// Create a regular file, character device, block device, fifo or socket node.
    fn mknod(
        &self,
        _req: &Request,
        parent: INodeNo,
        name: &OsStr,
        mode: u32,
        umask: u32,
        rdev: u32,
        reply: ReplyEntry,
    ) {
    }

    /// Create a directory.
    fn mkdir(
        &self,
        _req: &Request,
        parent: INodeNo,
        name: &OsStr,
        mode: u32,
        umask: u32,
        reply: ReplyEntry,
    ) {
    }

    /// Remove a file.
    fn unlink(&self, _req: &Request, parent: INodeNo, name: &OsStr, reply: ReplyEmpty) {}

    /// Remove a directory.
    fn rmdir(&self, _req: &Request, parent: INodeNo, name: &OsStr, reply: ReplyEmpty) {}

    /// Create a symbolic link.
    fn symlink(
        &self,
        _req: &Request,
        parent: INodeNo,
        link_name: &OsStr,
        target: &Path,
        reply: ReplyEntry,
    ) {
    }

    /// Rename a file.
    fn rename(
        &self,
        _req: &Request,
        parent: INodeNo,
        name: &OsStr,
        newparent: INodeNo,
        newname: &OsStr,
        flags: RenameFlags,
        reply: ReplyEmpty,
    ) {
    }

    /// Create a hard link.
    fn link(
        &self,
        _req: &Request,
        ino: INodeNo,
        newparent: INodeNo,
        newname: &OsStr,
        reply: ReplyEntry,
    ) {
    }
}

impl Lifetimefs {
    pub fn new(on_unmount: OnUnmount, mountpoint: &PathBuf) -> Result<Lifetimefs> {
        Self::ensure_mountpoint_ready(mountpoint)?;

        let storage = Storage::default()?;
        let storage_instance =
            storage.get_or_create_instance_for_mountpoint(mountpoint.as_path())?;

        Ok(Lifetimefs {
            storage: storage_instance,
            on_unmount,
            mountpoint: mountpoint.to_path_buf(),
        })
    }

    pub fn mount(self) -> Result<BackgroundSession> {
        let options = Config::default();
        let mountpoint = &self.mountpoint.clone();

        let session = fuser::spawn_mount2(self, mountpoint, &options)?;

        Ok(session)
    }

    fn recover_stale_mountpoint(mountpoint: &Path) -> Result<()> {
        match std::fs::read_dir(mountpoint) {
            Ok(_) => Ok(()),
            Err(error) if Self::is_not_connected_error(&error) => {
                eprintln!(
                    "Detected stale mount at {}, attempting lazy unmount",
                    mountpoint.display()
                );
                Self::lazy_unmount(mountpoint)?;
                Ok(())
            }
            Err(error) => Err(error.into()),
        }
    }

    fn ensure_mountpoint_ready(mountpoint: &Path) -> Result<()> {
        match std::fs::read_dir(mountpoint) {
            Ok(_) => Ok(()),
            Err(error) if Self::is_not_connected_error(&error) => {
                Self::recover_stale_mountpoint(mountpoint)?;

                match std::fs::read_dir(mountpoint) {
                    Ok(_) => Ok(()),
                    Err(error) if error.kind() == ErrorKind::NotFound => {
                        Err(MountError::PathNotExists(mountpoint.to_path_buf()).into())
                    }
                    Err(error) => Err(error.into()),
                }
            }
            Err(error) if error.kind() == ErrorKind::NotFound => {
                Err(MountError::PathNotExists(mountpoint.to_path_buf()).into())
            }
            Err(error) => Err(error.into()),
        }
    }

    fn is_not_connected_error(error: &std::io::Error) -> bool {
        error.kind() == ErrorKind::NotConnected || error.raw_os_error() == Some(107)
    }

    fn lazy_unmount(mountpoint: &Path) -> Result<()> {
        let mut attempts: Vec<(&str, Vec<&str>)> = vec![
            ("fusermount3", vec!["-u", "-z"]),
            ("fusermount", vec!["-u", "-z"]),
            ("umount", vec!["-l"]),
        ];

        let mountpoint_string = mountpoint.to_string_lossy().to_string();
        let mut errors = Vec::new();

        for (binary, mut args) in attempts.drain(..) {
            args.push(&mountpoint_string);

            match Command::new(binary).args(args).output() {
                Ok(output) if output.status.success() => {
                    return Ok(());
                }
                Ok(output) => {
                    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
                    errors.push(format!("{binary} failed: {stderr}"));
                }
                Err(error) => {
                    errors.push(format!("{binary} failed to start: {error}"));
                }
            }
        }

        anyhow::bail!(
            "failed to unmount stale mountpoint {}: {}",
            mountpoint.display(),
            errors.join("; ")
        );
    }
}
