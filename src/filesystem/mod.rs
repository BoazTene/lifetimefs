use anyhow::Result;
use fuser::{
    BackgroundSession, BsdFileFlags, Config, Errno, FileHandle, FileType, Filesystem, FopenFlags,
    Generation, INodeNo, LockOwner, OpenFlags, RenameFlags, ReplyAttr, ReplyData, ReplyDirectory,
    ReplyEmpty, ReplyEntry, ReplyOpen, Request, TimeOrNow,
};
use std::{
    ffi::{OsStr, OsString},
    io::ErrorKind,
    path::{Path, PathBuf},
    process::Command,
    time::{Duration, SystemTime},
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

    fn lookup(&self, _req: &Request, parent: INodeNo, name: &OsStr, reply: ReplyEntry) {
        match self.storage.lookup_attr(parent, name) {
            Ok(Some(attr)) => reply.entry(&Duration::from_secs(1), &attr, Generation(0)),
            Ok(None) => reply.error(Errno::ENOENT),
            Err(_) => reply.error(Errno::EIO),
        }
    }

    fn getattr(&self, _req: &Request, ino: INodeNo, _fh: Option<FileHandle>, reply: ReplyAttr) {
        match self.storage.getattr(ino) {
            Ok(Some(attr)) => reply.attr(&Duration::from_secs(1), &attr),
            Ok(None) => reply.error(Errno::ENOENT),
            Err(_) => reply.error(Errno::EIO),
        }
    }

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
        _fh: Option<FileHandle>,
        _crtime: Option<SystemTime>,
        _chgtime: Option<SystemTime>,
        _bkuptime: Option<SystemTime>,
        _flags: Option<BsdFileFlags>,
        reply: ReplyAttr,
    ) {
        match self.storage.setattr(ino, mode, uid, gid, size) {
            Ok(Some(attr)) => reply.attr(&Duration::from_secs(1), &attr),
            Ok(None) => reply.error(Errno::ENOENT),
            Err(_) => reply.error(Errno::EIO),
        }
    }

    fn readlink(&self, _req: &Request, ino: INodeNo, reply: ReplyData) {
        match self.storage.readlink(ino) {
            Ok(Some(target)) => reply.data(&target),
            Ok(None) => reply.error(Errno::ENOENT),
            Err(_) => reply.error(Errno::EIO),
        }
    }

    /// Create file node.
    /// Create a regular file, character device, block device, fifo or socket node.
    fn mknod(
        &self,
        _req: &Request,
        parent: INodeNo,
        name: &OsStr,
        mode: u32,
        _umask: u32,
        _rdev: u32,
        reply: ReplyEntry,
    ) {
        match self.storage.mknod(parent, name, mode) {
            Ok(Some(attr)) => reply.entry(&Duration::from_secs(1), &attr, Generation(0)),
            Ok(None) => reply.error(Errno::ENOENT),
            Err(_) => reply.error(Errno::EIO),
        }
    }

    /// Create a directory.
    fn mkdir(
        &self,
        _req: &Request,
        parent: INodeNo,
        name: &OsStr,
        mode: u32,
        _umask: u32,
        reply: ReplyEntry,
    ) {
        match self.storage.mkdir(parent, name, mode) {
            Ok(Some(attr)) => reply.entry(&Duration::from_secs(1), &attr, Generation(0)),
            Ok(None) => reply.error(Errno::ENOENT),
            Err(_) => reply.error(Errno::EIO),
        }
    }

    /// Remove a file.
    fn unlink(&self, _req: &Request, parent: INodeNo, name: &OsStr, reply: ReplyEmpty) {
        match self.storage.unlink(parent, name) {
            Ok(true) => reply.ok(),
            Ok(false) => reply.error(Errno::ENOENT),
            Err(_) => reply.error(Errno::EIO),
        }
    }

    /// Remove a directory.
    fn rmdir(&self, _req: &Request, parent: INodeNo, name: &OsStr, reply: ReplyEmpty) {
        match self.storage.rmdir(parent, name) {
            Ok(true) => reply.ok(),
            Ok(false) => reply.error(Errno::ENOENT),
            Err(_) => reply.error(Errno::EIO),
        }
    }

    /// Create a symbolic link.
    fn symlink(
        &self,
        _req: &Request,
        parent: INodeNo,
        link_name: &OsStr,
        target: &Path,
        reply: ReplyEntry,
    ) {
        match self.storage.symlink(parent, link_name, target) {
            Ok(Some(attr)) => reply.entry(&Duration::from_secs(1), &attr, Generation(0)),
            Ok(None) => reply.error(Errno::ENOENT),
            Err(_) => reply.error(Errno::EIO),
        }
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
        match self.storage.rename(parent, name, newparent, newname, flags) {
            Ok(true) => reply.ok(),
            Ok(false) => reply.error(Errno::ENOENT),
            Err(_) => reply.error(Errno::EIO),
        }
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
        match self.storage.link(ino, newparent, newname) {
            Ok(Some(attr)) => reply.entry(&Duration::from_secs(1), &attr, Generation(0)),
            Ok(None) => reply.error(Errno::ENOENT),
            Err(_) => reply.error(Errno::EIO),
        }
    }

    fn opendir(&self, _req: &Request, ino: INodeNo, _flags: OpenFlags, reply: ReplyOpen) {
        match self.storage.getattr(ino) {
            Ok(Some(attr)) if attr.kind == FileType::Directory => {
                reply.opened(FileHandle(0), FopenFlags::empty());
            }
            Ok(Some(_)) => reply.error(Errno::ENOTDIR),
            Ok(None) => reply.error(Errno::ENOENT),
            Err(_) => reply.error(Errno::EIO),
        }
    }

    fn readdir(
        &self,
        _req: &Request,
        ino: INodeNo,
        _fh: FileHandle,
        offset: u64,
        mut reply: ReplyDirectory,
    ) {
        let entries = match self.storage.readdir(ino) {
            Ok(Some(entries)) => entries,
            Ok(None) => {
                reply.error(Errno::ENOENT);
                return;
            }
            Err(_) => {
                reply.error(Errno::EIO);
                return;
            }
        };

        let mut all_entries: Vec<(INodeNo, FileType, OsString)> =
            Vec::with_capacity(entries.len() + 2);
        all_entries.push((ino, FileType::Directory, OsString::from(".")));
        let parent_ino = match self.storage.parent_ino(ino) {
            Ok(Some(parent)) => parent,
            Ok(None) => INodeNo::ROOT,
            Err(_) => {
                reply.error(Errno::EIO);
                return;
            }
        };
        all_entries.push((parent_ino, FileType::Directory, OsString::from("..")));
        all_entries.extend(entries);

        for (i, (entry_ino, kind, name)) in all_entries.into_iter().enumerate().skip(offset as usize) {
            let next_offset = (i + 1) as u64;
            if reply.add(entry_ino, next_offset, kind, name) {
                break;
            }
        }

        reply.ok();
    }

    fn releasedir(
        &self,
        _req: &Request,
        _ino: INodeNo,
        _fh: FileHandle,
        _flags: OpenFlags,
        reply: ReplyEmpty,
    ) {
        reply.ok();
    }

    fn read(
        &self,
        _req: &Request,
        ino: INodeNo,
        _fh: FileHandle,
        offset: u64,
        size: u32,
        _flags: OpenFlags,
        _lock_owner: Option<LockOwner>,
        reply: ReplyData,
    ) {
        match self.storage.read(ino, offset, size) {
            Ok(Some(data)) => reply.data(&data),
            Ok(None) => reply.error(Errno::ENOENT),
            Err(_) => reply.error(Errno::EIO),
        }
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
