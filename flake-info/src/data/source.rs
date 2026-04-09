use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::{
    ffi::OsStr,
    fs::File,
    io::{self, Read},
    path::Path,
};

pub type Hash = String;
pub type FlakeRef = String;

/// Information about the flake origin
/// Supports (local/raw) Git, GitHub, SourceHut and Gitlab repos
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum Source {
    Github {
        owner: String,
        repo: String,
        description: Option<String>,
        #[serde(rename(deserialize = "hash"))]
        git_ref: Option<Hash>,
    },
    Gitlab {
        owner: String,
        repo: String,
        git_ref: Option<Hash>,
    },
    SourceHut {
        owner: String,
        repo: String,
        git_ref: Option<Hash>,
    },
    Git {
        url: String,
        #[serde(rename = "ref", default)]
        git_ref: Option<String>,
    },
    Nixpkgs(Nixpkgs),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct TomlDocument {
    sources: Vec<Source>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
struct RegistryEntry {
    to: Source,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
struct RegistryDocument {
    version: u32,
    flakes: Vec<RegistryEntry>,
}

impl Source {
    pub fn to_flake_ref(&self) -> FlakeRef {
        match self {
            Source::Github {
                owner,
                repo,
                git_ref,
                ..
            } => format!(
                "github:{}/{}{}",
                owner,
                repo,
                git_ref
                    .as_ref()
                    .map_or("".to_string(), |f| format!("?ref={}", f))
            ),
            Source::Gitlab {
                owner,
                repo,
                git_ref,
            } => format!(
                "gitlab:{}/{}{}",
                owner,
                repo,
                git_ref
                    .as_ref()
                    .map_or("".to_string(), |f| format!("?ref={}", f))
            ),
            Source::SourceHut {
                owner,
                repo,
                git_ref,
            } => format!(
                "sourcehut:{}/{}{}",
                owner,
                repo,
                git_ref
                    .as_ref()
                    .map_or("".to_string(), |f| format!("?ref={}", f))
            ),
            Source::Git { url, git_ref } => {
                // Registry JSON stores URLs without the git+ scheme prefix.
                // Ensure the URL has the git+ prefix required by nix.
                let base = if url.starts_with("git+") {
                    url.to_string()
                } else {
                    format!("git+{}", url)
                };
                // Append ?ref=... if a ref was specified separately and the URL
                // does not already carry one.
                match git_ref {
                    Some(r) if !base.contains("?ref=") => format!("{}?ref={}", base, r),
                    _ => base,
                }
            }
            Source::Nixpkgs(Nixpkgs { git_ref, .. }) => format!(
                "https://api.github.com/repos/NixOS/nixpkgs/tarball/{}",
                git_ref
            ),
        }
    }

    pub fn read_sources_file(path: &Path) -> io::Result<Vec<Source>> {
        let mut file = File::open(path)?;

        let mut buf = String::new();
        file.read_to_string(&mut buf)?;

        if path.extension() == Some(OsStr::new("toml")) {
            let document: TomlDocument = toml::from_str(&buf)?;
            Ok(document.sources)
        } else {
            // Detect whether this is a standardized flake registry
            // ({"version": 2, "flakes": [{"from": ..., "to": ...}, ...]})
            // or a legacy flat array of sources.
            let value: serde_json::Value = serde_json::from_str(&buf)?;
            if value.get("version").is_some() && value.get("flakes").is_some() {
                let doc: RegistryDocument = serde_json::from_value(value)?;
                Ok(doc.flakes.into_iter().map(|e| e.to).collect())
            } else {
                Ok(serde_json::from_str(&buf)?)
            }
        }
    }

    pub async fn nixpkgs(channel: String) -> Result<Nixpkgs> {
        #[derive(Deserialize, Debug)]
        struct ApiResult {
            commit: Commit,
        }

        #[derive(Deserialize, Debug)]
        struct Commit {
            sha: String,
        }

        let request = reqwest::Client::builder()
            .user_agent("nixos-search")
            .build()?
            .get(format!(
                "https://api.github.com/repos/nixos/nixpkgs/branches/nixos-{}",
                channel
            ));

        let request = match std::env::var("GITHUB_TOKEN") {
            Ok(token) => request.bearer_auth(token),
            _ => request,
        };

        let response = request.send().await?;

        if !response.status().is_success() {
            Err(anyhow::anyhow!(
                "GitHub returned {:?} {}",
                response.status(),
                response.text().await?
            ))
        } else {
            let git_ref = response.json::<ApiResult>().await?.commit.sha;
            let nixpkgs = Nixpkgs { channel, git_ref };
            Ok(nixpkgs)
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Nixpkgs {
    pub channel: String,

    pub git_ref: String,
}
