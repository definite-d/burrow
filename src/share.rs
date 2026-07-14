use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum ShareMode {
    Download,
    Upload,
    Both,
}

impl Default for ShareMode {
    fn default() -> Self {
        ShareMode::Download
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Share {
    pub id: String,
    pub token: String,
    pub mode: ShareMode,
    pub path: PathBuf,
    pub expires: Option<DateTime<Utc>>,
    #[serde(default)]
    pub allow_archive: bool,
    #[serde(default = "default_max_upload_size")]
    pub max_upload_size: u64,
}

fn default_max_upload_size() -> u64 {
    100 * 1024 * 1024
}

impl Share {
    pub fn is_expired(&self) -> bool {
        self.expires
            .map(|exp| Utc::now() > exp)
            .unwrap_or(false)
    }

    pub fn allows_download(&self) -> bool {
        self.mode == ShareMode::Download || self.mode == ShareMode::Both
    }

    pub fn allows_upload(&self) -> bool {
        self.mode == ShareMode::Upload || self.mode == ShareMode::Both
    }
}

pub struct ShareStore {
    shares: Arc<RwLock<HashMap<String, Share>>>,
}

impl ShareStore {
    pub fn new() -> Self {
        Self {
            shares: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn create(
        &self,
        id: String,
        path: PathBuf,
        mode: ShareMode,
        expires: Option<DateTime<Utc>>,
        allow_archive: bool,
        max_upload_size: u64,
    ) -> Result<Share, String> {
        if !path.exists() {
            return Err(format!("path does not exist: {}", path.display()));
        }
        if !path.is_dir() {
            return Err(format!("path is not a directory: {}", path.display()));
        }

        let token = nanoid::nanoid!(21);
        let share = Share {
            id: id.clone(),
            token: token.clone(),
            mode,
            path,
            expires,
            allow_archive,
            max_upload_size,
        };

        let mut shares = self.shares.write().await;
        if shares.contains_key(&id) {
            return Err(format!("share with id '{id}' already exists"));
        }
        shares.insert(id, share.clone());
        Ok(share)
    }

    pub async fn get(&self, token: &str) -> Option<Share> {
        let shares = self.shares.read().await;
        shares.values().find(|s| s.token == token).cloned()
    }

    pub async fn get_by_id(&self, id: &str) -> Option<Share> {
        let shares = self.shares.read().await;
        shares.get(id).cloned()
    }

    pub async fn list(&self) -> Vec<Share> {
        let shares = self.shares.read().await;
        shares.values().cloned().collect()
    }

    pub async fn replace(&self, id: &str, share: Share) {
        let mut shares = self.shares.write().await;
        shares.insert(id.to_string(), share);
    }

    pub async fn delete(&self, id: &str) -> bool {
        let mut shares = self.shares.write().await;
        shares.remove(id).is_some()
    }

    pub async fn evict_expired(&self) {
        let mut shares = self.shares.write().await;
        shares.retain(|_, share| !share.is_expired());
    }

    pub async fn load_from_config(
        &self,
        config_shares: &[ConfigShare],
    ) -> Result<Vec<Share>, String> {
        let mut loaded = Vec::new();
        for cs in config_shares {
            let path = PathBuf::from(&cs.path);
            let path = if path.is_relative() {
                std::env::current_dir()
                    .map(|cwd| cwd.join(&path))
                    .unwrap_or(path)
            } else {
                path
            };
            let path = match path.canonicalize() {
                Ok(p) => p,
                Err(e) => {
                    tracing::warn!("config share '{}' path error: {e}", cs.id);
                    continue;
                }
            };

            let mode = cs.mode.clone().unwrap_or_default();
            let expires = cs.expires.as_deref().and_then(parse_duration);
            let max_upload_size = cs
                .max_size
                .as_deref()
                .and_then(|s| crate::filter::parse_size(s).ok())
                .unwrap_or(100 * 1024 * 1024);

            match self
                .create(
                    cs.id.clone(),
                    path,
                    mode,
                    expires,
                    cs.allow_archive.unwrap_or(false),
                    max_upload_size,
                )
                .await
            {
                Ok(share) => loaded.push(share),
                Err(e) => tracing::warn!("failed to load config share '{}': {e}", cs.id),
            }
        }
        Ok(loaded)
    }
}

#[derive(Debug, Deserialize)]
pub struct ConfigShare {
    pub id: String,
    pub path: String,
    #[serde(default)]
    pub mode: Option<ShareMode>,
    pub token: Option<String>,
    pub expires: Option<String>,
    pub max_size: Option<String>,
    #[serde(default)]
    pub allowed_types: Option<Vec<String>>,
    pub allow_archive: Option<bool>,
}

fn parse_duration(s: &str) -> Option<DateTime<Utc>> {
    let s = s.trim().to_lowercase();
    let seconds: i64 = if let Some(v) = s.strip_suffix("s") {
        v.trim().parse().ok()?
    } else if let Some(v) = s.strip_suffix("m") {
        v.trim().parse::<i64>().ok()? * 60
    } else if let Some(v) = s.strip_suffix("h") {
        v.trim().parse::<i64>().ok()? * 3600
    } else if let Some(v) = s.strip_suffix("d") {
        v.trim().parse::<i64>().ok()? * 86400
    } else {
        return None;
    };
    Some(Utc::now() + chrono::Duration::seconds(seconds))
}
