use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use serde_json::Value;

const METADATA_DB_FILE: &str = "metadata.db";
const DATA_DIR: &str = "data";
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
}
