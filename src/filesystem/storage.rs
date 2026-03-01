use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use serde_json::json;
use sha2::{Digest, Sha256};

use crate::filesystem::storage_instance::StorageInstance;

const INSTANCES_DIR: &str = "instances";

#[derive(Debug, Clone)]
pub struct Storage {
    root: PathBuf,
}

impl Storage {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn default() -> Result<Self> {
        let home: String = std::env::var("HOME").context("HOME environment variable is not set")?;
        Ok(Self::new(PathBuf::from(home).join(".lifetimefs")))
    }

    pub fn instances_root(&self) -> PathBuf {
        self.root.join(INSTANCES_DIR)
    }

    pub fn initialize(&self) -> Result<()> {
        fs::create_dir_all(self.instances_root()).with_context(|| {
            format!(
                "failed to create storage instances root at {}",
                self.instances_root().display()
            )
        })?;

        Ok(())
    }

    fn get_or_create_instance(&self, name: &str) -> Result<StorageInstance> {
        Self::validate_instance_name(name)?;
        self.initialize()?;

        let instance = StorageInstance::new(self.instances_root().join(name));
        if !instance.exists()? {
            instance.initialize()?;
        }

        instance.ensure_valid_layout()?;

        Ok(instance)
    }

    pub fn get_or_create_instance_for_mountpoint(
        &self,
        mountpoint: &Path,
    ) -> Result<StorageInstance> {
        let canonical_mountpoint = mountpoint.canonicalize().with_context(|| {
            format!(
                "failed to canonicalize mountpoint path {}",
                mountpoint.display()
            )
        })?;

        let instance_id = self.instance_id_for_canonical_mountpoint(&canonical_mountpoint);
        let instance = self.get_or_create_instance(&instance_id)?;

        let mut config = instance
            .load_config()
            .unwrap_or_else(|_| json!({ "version": 1 }));
        config["id"] = json!(instance_id);
        config["mountpoint"] = json!(mountpoint.to_string_lossy().to_string());
        config["canonical_mountpoint"] =
            json!(canonical_mountpoint.to_string_lossy().to_string());
        instance.save_config(&config)?;

        Ok(instance)
    }

    pub fn list_instances(&self) -> Result<Vec<String>> {
        if !self.instances_root().exists() {
            return Ok(Vec::new());
        }

        let mut names = Vec::new();

        for entry in fs::read_dir(self.instances_root())
            .with_context(|| format!("failed to read {}", self.instances_root().display()))?
        {
            let entry = entry?;
            if !entry.file_type()?.is_dir() {
                continue;
            }

            if let Some(name) = entry.file_name().to_str() {
                let instance = StorageInstance::new(entry.path());
                if let Ok(config) = instance.load_config() {
                    if let Some(canonical_mountpoint) = config
                        .get("canonical_mountpoint")
                        .and_then(|v| v.as_str())
                    {
                        names.push(canonical_mountpoint.to_string());
                        continue;
                    }
                }

                // Fallback for old instances created before mountpoint metadata existed.
                names.push(name.to_string());
            }
        }

        names.sort();
        Ok(names)
    }

    fn instance_id_for_canonical_mountpoint(&self, canonical_mountpoint: &Path) -> String {
        let mut hasher = Sha256::new();
        hasher.update(canonical_mountpoint.to_string_lossy().as_bytes());
        let digest = hasher.finalize();
        format!("{digest:x}")
    }

    fn validate_instance_name(name: &str) -> Result<()> {
        if name.is_empty() {
            bail!("storage instance name must not be empty");
        }

        if name == "." || name == ".." {
            bail!("invalid storage instance name: {name}");
        }

        if name.contains('/') || name.contains('\\') {
            bail!("invalid storage instance name '{name}': path separators are not allowed");
        }

        Ok(())
    }
}
