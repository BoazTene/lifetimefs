use std::ffi::OsStr;
use std::fs::{self, OpenOptions};
use std::io::{ErrorKind, Read, Seek, SeekFrom, Write};
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::{MetadataExt, PermissionsExt, symlink};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use fuser::{FileAttr, FileType, INodeNo, RenameFlags};
use serde_json::Value;
use std::time::UNIX_EPOCH;

const METADATA_DB_FILE: &str = "metadata.db";
const DATA_DIR: &str = "data";
const HEAD_DIR: &str = "head";
const CONFIG_FILE: &str = "config.json";

#[derive(Debug, Clone)]
pub struct StorageInstance {
    root: PathBuf,
}

impl StorageInstance {
    pub fn new( root: PathBuf) -> Self {
        Self { root }
    }

    pub fn metadata_db_path(&self) -> PathBuf {
        self.root.join(METADATA_DB_FILE)
    }

    pub fn data_dir(&self) -> PathBuf {
        self.root.join(DATA_DIR)
    }

    pub fn head_dir(&self) -> PathBuf {
        self.root.join(HEAD_DIR)
    }

    pub fn config_path(&self) -> PathBuf {
        self.root.join(CONFIG_FILE)
    }

    pub fn exists(&self) -> Result<bool> {
        if !self.root.exists() {
            return Ok(false);
        }

        Ok(
            self.metadata_db_path().exists()
                && self.data_dir().exists()
                && self.head_dir().exists()
                && self.config_path().exists(),
        )
    }

    pub fn initialize(&self) -> Result<()> {
        fs::create_dir_all(&self.root)
            .with_context(|| format!("failed to create instance root at {}", self.root.display()))?;

        fs::create_dir_all(self.data_dir()).with_context(|| {
            format!(
                "failed to create data directory at {}",
                self.data_dir().display()
            )
        })?;

        fs::create_dir_all(self.head_dir()).with_context(|| {
            format!(
                "failed to create head directory at {}",
                self.head_dir().display()
            )
        })?;

        self.ensure_metadata_file()?;

        Ok(())
    }

    pub fn ensure_valid_layout(&self) -> Result<()> {
        if !self.root.exists() {
            bail!("storage instance root does not exist: {}", self.root.display());
        }

        if !self.metadata_db_path().exists() {
            bail!(
                "missing metadata db file for storage instance: {}",
                self.metadata_db_path().display()
            );
        }

        if !self.data_dir().is_dir() {
            bail!(
                "missing data directory for storage instance: {}",
                self.data_dir().display()
            );
        }

        if !self.head_dir().is_dir() {
            bail!(
                "missing head directory for storage instance: {}",
                self.head_dir().display()
            );
        }

        if !self.config_path().is_file() {
            bail!(
                "missing config file for storage instance: {}",
                self.config_path().display()
            );
        }

        Ok(())
    }

    pub fn load_config(&self) -> Result<Value> {
        let config_path = self.config_path();
        let content = fs::read_to_string(&config_path)
            .with_context(|| format!("failed to read config file {}", config_path.display()))?;

        let value = serde_json::from_str(&content)
            .with_context(|| format!("failed to parse config file {}", config_path.display()))?;

        Ok(value)
    }

    pub fn save_config(&self, config: &Value) -> Result<()> {
        let config_path = self.config_path();
        let mut file = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(&config_path)
            .with_context(|| format!("failed to open config file {}", config_path.display()))?;

        let payload = serde_json::to_vec_pretty(config)
            .context("failed to serialize storage instance config")?;
        file.write_all(&payload)
            .with_context(|| format!("failed to write config file {}", config_path.display()))?;
        file.write_all(b"\n")
            .with_context(|| format!("failed to finalize config file {}", config_path.display()))?;

        Ok(())
    }

    fn ensure_metadata_file(&self) -> Result<()> {
        let metadata_path = self.metadata_db_path();

        OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(false)
            .open(&metadata_path)
            .with_context(|| {
                format!(
                    "failed to create or open metadata db file {}",
                    metadata_path.display()
                )
            })?;

        Ok(())
    }

    pub fn lookup(&self, parent: INodeNo, name: &OsStr) -> Result<Option<PathBuf>> {
        let Some(candidate) = self.child_path(parent, name)? else {
            return Ok(None);
        };

        match fs::symlink_metadata(&candidate) {
            Ok(_) => Ok(Some(candidate)),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(error)
                .with_context(|| format!("failed to lookup head entry {}", candidate.display())),
        }
    }

    pub fn lookup_attr(&self, parent: INodeNo, name: &OsStr) -> Result<Option<FileAttr>> {
        match self.lookup(parent, name)? {
            Some(path) => self.path_to_attr(&path, None).map(Some),
            None => Ok(None),
        }
    }

    pub fn getattr(&self, ino: INodeNo) -> Result<Option<FileAttr>> {
        if ino == INodeNo::ROOT {
            return self.path_to_attr(&self.head_dir(), Some(INodeNo::ROOT)).map(Some);
        }

        match self.path_for_ino(ino)? {
            Some(path) => self.path_to_attr(&path, None).map(Some),
            None => Ok(None),
        }
    }

    pub fn setattr(
        &self,
        ino: INodeNo,
        mode: Option<u32>,
        _uid: Option<u32>,
        _gid: Option<u32>,
        size: Option<u64>,
    ) -> Result<Option<FileAttr>> {
        let path = if ino == INodeNo::ROOT {
            self.head_dir()
        } else if let Some(path) = self.path_for_ino(ino)? {
            path
        } else {
            return Ok(None);
        };

        if let Some(mode) = mode {
            let permissions = fs::Permissions::from_mode(mode & 0o7777);
            fs::set_permissions(&path, permissions).with_context(|| {
                format!("failed to set permissions for {}", path.display())
            })?;
        }

        if let Some(size) = size {
            let file = OpenOptions::new()
                .write(true)
                .open(&path)
                .with_context(|| format!("failed to open {} for truncate", path.display()))?;
            file.set_len(size)
                .with_context(|| format!("failed to set size for {}", path.display()))?;
        }

        self.path_to_attr(&path, if ino == INodeNo::ROOT { Some(INodeNo::ROOT) } else { None })
            .map(Some)
    }

    pub fn readlink(&self, ino: INodeNo) -> Result<Option<Vec<u8>>> {
        let Some(path) = self.path_for_ino(ino)? else {
            return Ok(None);
        };

        let target = fs::read_link(&path)
            .with_context(|| format!("failed to read symlink {}", path.display()))?;
        Ok(Some(target.as_os_str().as_bytes().to_vec()))
    }

    pub fn read(&self, ino: INodeNo, offset: u64, size: u32) -> Result<Option<Vec<u8>>> {
        let Some(path) = self.path_for_ino(ino)? else {
            return Ok(None);
        };

        let metadata = fs::symlink_metadata(&path)
            .with_context(|| format!("failed to stat {}", path.display()))?;
        if !metadata.is_file() {
            return Ok(None);
        }

        let file_len = metadata.len();
        if offset >= file_len || size == 0 {
            return Ok(Some(Vec::new()));
        }

        let read_len = (file_len - offset).min(size as u64) as usize;
        let mut buf = vec![0u8; read_len];
        let mut file = OpenOptions::new()
            .read(true)
            .open(&path)
            .with_context(|| format!("failed to open {} for read", path.display()))?;
        file.seek(SeekFrom::Start(offset))
            .with_context(|| format!("failed to seek {}", path.display()))?;
        file.read_exact(&mut buf)
            .with_context(|| format!("failed to read {}", path.display()))?;

        Ok(Some(buf))
    }

    pub fn mknod(&self, parent: INodeNo, name: &OsStr, mode: u32) -> Result<Option<FileAttr>> {
        let Some(path) = self.child_path(parent, name)? else {
            return Ok(None);
        };

        OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&path)
            .with_context(|| format!("failed to create node {}", path.display()))?;

        fs::set_permissions(&path, fs::Permissions::from_mode(mode & 0o7777))
            .with_context(|| format!("failed to set mode for {}", path.display()))?;

        self.path_to_attr(&path, None).map(Some)
    }

    pub fn mkdir(&self, parent: INodeNo, name: &OsStr, mode: u32) -> Result<Option<FileAttr>> {
        let Some(path) = self.child_path(parent, name)? else {
            return Ok(None);
        };

        fs::create_dir(&path).with_context(|| format!("failed to create dir {}", path.display()))?;
        fs::set_permissions(&path, fs::Permissions::from_mode(mode & 0o7777))
            .with_context(|| format!("failed to set mode for {}", path.display()))?;

        self.path_to_attr(&path, None).map(Some)
    }

    pub fn unlink(&self, parent: INodeNo, name: &OsStr) -> Result<bool> {
        let Some(path) = self.child_path(parent, name)? else {
            return Ok(false);
        };

        match fs::remove_file(&path) {
            Ok(()) => Ok(true),
            Err(error) if error.kind() == ErrorKind::NotFound => Ok(false),
            Err(error) => Err(error).with_context(|| format!("failed to unlink {}", path.display())),
        }
    }

    pub fn rmdir(&self, parent: INodeNo, name: &OsStr) -> Result<bool> {
        let Some(path) = self.child_path(parent, name)? else {
            return Ok(false);
        };

        match fs::remove_dir(&path) {
            Ok(()) => Ok(true),
            Err(error) if error.kind() == ErrorKind::NotFound => Ok(false),
            Err(error) => Err(error).with_context(|| format!("failed to remove dir {}", path.display())),
        }
    }

    pub fn symlink(
        &self,
        parent: INodeNo,
        link_name: &OsStr,
        target: &std::path::Path,
    ) -> Result<Option<FileAttr>> {
        let Some(path) = self.child_path(parent, link_name)? else {
            return Ok(None);
        };

        symlink(target, &path)
            .with_context(|| format!("failed to create symlink {}", path.display()))?;
        self.path_to_attr(&path, None).map(Some)
    }

    pub fn rename(
        &self,
        parent: INodeNo,
        name: &OsStr,
        newparent: INodeNo,
        newname: &OsStr,
        flags: RenameFlags,
    ) -> Result<bool> {
        if !flags.is_empty() {
            return Ok(false);
        }

        let Some(old_path) = self.child_path(parent, name)? else {
            return Ok(false);
        };
        let Some(new_path) = self.child_path(newparent, newname)? else {
            return Ok(false);
        };

        match fs::rename(&old_path, &new_path) {
            Ok(()) => Ok(true),
            Err(error) if error.kind() == ErrorKind::NotFound => Ok(false),
            Err(error) => Err(error).with_context(|| {
                format!(
                    "failed to rename {} to {}",
                    old_path.display(),
                    new_path.display()
                )
            }),
        }
    }

    pub fn link(&self, ino: INodeNo, newparent: INodeNo, newname: &OsStr) -> Result<Option<FileAttr>> {
        let Some(source_path) = self.path_for_ino(ino)? else {
            return Ok(None);
        };
        let Some(dest_path) = self.child_path(newparent, newname)? else {
            return Ok(None);
        };

        fs::hard_link(&source_path, &dest_path).with_context(|| {
            format!(
                "failed to hard-link {} to {}",
                source_path.display(),
                dest_path.display()
            )
        })?;
        self.path_to_attr(&dest_path, None).map(Some)
    }

    pub fn readdir(
        &self,
        ino: INodeNo,
    ) -> Result<Option<Vec<(INodeNo, FileType, std::ffi::OsString)>>> {
        let dir_path = if ino == INodeNo::ROOT {
            self.head_dir()
        } else if let Some(path) = self.path_for_ino(ino)? {
            path
        } else {
            return Ok(None);
        };

        if !dir_path.is_dir() {
            return Ok(None);
        }

        let mut entries = Vec::new();
        for entry in fs::read_dir(&dir_path)
            .with_context(|| format!("failed to read directory {}", dir_path.display()))?
        {
            let entry = entry?;
            let metadata = fs::symlink_metadata(entry.path())?;
            let kind = FileType::from_std(metadata.file_type()).unwrap_or(FileType::RegularFile);
            entries.push((INodeNo(metadata.ino()), kind, entry.file_name()));
        }

        Ok(Some(entries))
    }

    pub fn parent_ino(&self, ino: INodeNo) -> Result<Option<INodeNo>> {
        if ino == INodeNo::ROOT {
            return Ok(Some(INodeNo::ROOT));
        }

        let Some(path) = self.path_for_ino(ino)? else {
            return Ok(None);
        };
        let Some(parent_path) = path.parent() else {
            return Ok(Some(INodeNo::ROOT));
        };
        if parent_path == self.head_dir() {
            return Ok(Some(INodeNo::ROOT));
        }

        let metadata = fs::symlink_metadata(parent_path)
            .with_context(|| format!("failed to stat {}", parent_path.display()))?;
        Ok(Some(INodeNo(metadata.ino())))
    }

    fn valid_child_name(&self, name: &OsStr) -> bool {
        if name.is_empty() || name == OsStr::new(".") || name == OsStr::new("..") {
            return false;
        }
        true
    }

    fn child_path(&self, parent: INodeNo, name: &OsStr) -> Result<Option<PathBuf>> {
        if !self.valid_child_name(name) {
            return Ok(None);
        }

        let dir = if parent == INodeNo::ROOT {
            self.head_dir()
        } else if let Some(path) = self.path_for_ino(parent)? {
            let metadata = fs::symlink_metadata(&path)?;
            if !metadata.is_dir() {
                return Ok(None);
            }
            path
        } else {
            return Ok(None);
        };

        Ok(Some(dir.join(name)))
    }

    fn path_for_ino(&self, ino: INodeNo) -> Result<Option<PathBuf>> {
        let mut stack = vec![self.head_dir()];

        while let Some(dir) = stack.pop() {
            for entry in fs::read_dir(&dir).with_context(|| format!("failed to read {}", dir.display()))? {
                let entry = entry?;
                let path = entry.path();
                let metadata = fs::symlink_metadata(&path)?;
                if metadata.ino() == ino.0 {
                    return Ok(Some(path));
                }

                if metadata.is_dir() {
                    stack.push(path);
                }
            }
        }

        Ok(None)
    }

    fn path_to_attr(&self, path: &Path, inode_override: Option<INodeNo>) -> Result<FileAttr> {
        let metadata = fs::symlink_metadata(path)
            .with_context(|| format!("failed to stat {}", path.display()))?;

        Ok(FileAttr {
            ino: inode_override.unwrap_or(INodeNo(metadata.ino())),
            size: metadata.size(),
            blocks: metadata.blocks(),
            atime: metadata.accessed().unwrap_or(UNIX_EPOCH),
            mtime: metadata.modified().unwrap_or(UNIX_EPOCH),
            ctime: metadata.modified().unwrap_or(UNIX_EPOCH),
            crtime: metadata.created().unwrap_or(UNIX_EPOCH),
            kind: FileType::from_std(metadata.file_type()).unwrap_or(FileType::RegularFile),
            perm: (metadata.mode() & 0o7777) as u16,
            nlink: metadata.nlink() as u32,
            uid: metadata.uid(),
            gid: metadata.gid(),
            rdev: metadata.rdev() as u32,
            blksize: metadata.blksize() as u32,
            flags: 0,
        })
    }
}
