use anyhow::Result;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
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
        #[serde(default, alias = "hash", alias = "ref", alias = "rev")]
        git_ref: Option<Hash>,
    },
    Gitlab {
        owner: String,
        repo: String,
        #[serde(default, alias = "hash", alias = "ref", alias = "rev")]
        git_ref: Option<Hash>,
    },
    SourceHut {
        owner: String,
        repo: String,
        #[serde(default, alias = "hash", alias = "ref", alias = "rev")]
        git_ref: Option<Hash>,
    },
    Git {
        url: String,
        #[serde(default, alias = "hash", alias = "ref", alias = "rev")]
        git_ref: Option<Hash>,
    },
    Nixpkgs(Nixpkgs),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct LegacyJsonDocument {
    sources: Vec<Source>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct RegistryDocument {
    flakes: Vec<RegistryEntry>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct RegistryEntry {
    to: Value,
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
                let url = if url.starts_with("git+") {
                    url.to_string()
                } else if url.starts_with("http://")
                    || url.starts_with("https://")
                    || url.starts_with("ssh://")
                {
                    format!("git+{url}")
                } else {
                    url.to_string()
                };

                if let Some(git_ref) = git_ref {
                    let separator = if url.contains('?') { "&" } else { "?" };
                    format!("{url}{separator}ref={git_ref}")
                } else {
                    url
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

        if path
            .extension()
            .is_some_and(|extension| extension == "toml")
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "TOML group inputs are deprecated; use a JSON file in standard nix flake registry format",
            ));
        }

        let value: Value = serde_json::from_str(&buf)?;

        if value.is_array() {
            return Ok(serde_json::from_value(value)?);
        }

        if value.get("sources").is_some() {
            let document: LegacyJsonDocument = serde_json::from_value(value)?;
            return Ok(document.sources);
        }

        if value.get("flakes").is_some() {
            let document: RegistryDocument = serde_json::from_value(value)?;
            let sources = document
                .flakes
                .into_iter()
                .map(|entry| serde_json::from_value(entry.to))
                .collect::<std::result::Result<Vec<Source>, _>>()?;
            return Ok(sources);
        }

        Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Unsupported group source JSON format",
        ))
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
