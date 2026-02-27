use std::path::PathBuf;

use thiserror::Error;

#[derive(Error, Debug)]
pub enum MountError {
    #[error("the mount point \"{0}\" does not exist")]
    PathNotExists(PathBuf),
}