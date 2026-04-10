use anyhow::Result;
use serde::{Deserialize, Serialize};
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
    },
    Nixpkgs(Nixpkgs),
}

#[derive(Debug, Deserialize)]
struct Registry {
    flakes: Vec<RegistryEntry>,
}

#[derive(Debug, Deserialize)]
struct RegistryEntry {
    to: RegistryFlakeRef,
}

#[derive(Debug, Deserialize)]
struct RegistryFlakeRef {
    #[serde(rename = "type")]
    flake_type: String,
    owner: Option<String>,
    repo: Option<String>,
    #[serde(rename = "ref")]
    git_ref: Option<String>,
    url: Option<String>,
}

impl RegistryFlakeRef {
    fn into_source(self) -> Option<Source> {
        match self.flake_type.as_str() {
            "github" => Some(Source::Github {
                owner: self.owner?,
                repo: self.repo?,
                git_ref: self.git_ref,
                description: None,
            }),
            "gitlab" => Some(Source::Gitlab {
                owner: self.owner?,
                repo: self.repo?,
                git_ref: self.git_ref,
            }),
            "sourcehut" => Some(Source::SourceHut {
                owner: self.owner?,
                repo: self.repo?,
                git_ref: self.git_ref,
            }),
            "git" => {
                let url = self.url?;
                let full_url = match self.git_ref {
                    Some(r) => format!("git+{}?ref={}", url, r),
                    None => format!("git+{}", url),
                };
                Some(Source::Git { url: full_url })
            }
            _ => None,
        }
    }
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
            Source::Git { url } => url.to_string(),
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

        let registry: Registry = serde_json::from_str(&buf)?;
        Ok(registry
            .flakes
            .into_iter()
            .filter_map(|entry| entry.to.into_source())
            .collect())
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
