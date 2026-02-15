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

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
struct FlakeRefAttrs {
    #[serde(rename = "ref", alias = "git_ref")]
    r#ref: Option<String>,
    #[serde(alias = "hash")]
    rev: Option<Hash>,
    dir: Option<String>,
    #[serde(rename = "narHash")]
    nar_hash: Option<Hash>,
    #[serde(rename = "revCount")]
    rev_count: Option<u64>,
    #[serde(rename = "lastModified")]
    last_modified: Option<u64>,
}

impl FlakeRefAttrs {
    fn query_string(&self) -> String {
        let mut params = Vec::new();

        if let Some(git_ref) = &self.r#ref {
            params.push(format!("ref={}", git_ref));
        }

        if let Some(rev) = &self.rev {
            params.push(format!("rev={}", rev));
        }

        if let Some(dir) = &self.dir {
            params.push(format!("dir={}", dir));
        }

        if let Some(nar_hash) = &self.nar_hash {
            params.push(format!("narHash={}", nar_hash));
        }

        if let Some(rev_count) = &self.rev_count {
            params.push(format!("revCount={}", rev_count));
        }

        if let Some(last_modified) = &self.last_modified {
            params.push(format!("lastModified={}", last_modified));
        }

        if params.is_empty() {
            String::new()
        } else {
            format!("?{}", params.join("&"))
        }
    }

    fn append_to(&self, base: &str) -> String {
        let query = self.query_string();

        if query.is_empty() {
            base.to_string()
        } else if base.contains('?') {
            format!("{}&{}", base, &query[1..])
        } else {
            format!("{}{}", base, query)
        }
    }
}

/// Information about the flake origin
/// Supports (local/raw) Git, GitHub, SourceHut and Gitlab repos
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum Source {
    Github {
        owner: String,
        repo: String,
        description: Option<String>,
        #[serde(flatten)]
        attrs: FlakeRefAttrs,
    },
    Gitlab {
        owner: String,
        repo: String,
        #[serde(flatten)]
        attrs: FlakeRefAttrs,
    },
    SourceHut {
        owner: String,
        repo: String,
        #[serde(flatten)]
        attrs: FlakeRefAttrs,
    },
    Git {
        url: String,
        #[serde(flatten)]
        attrs: FlakeRefAttrs,
    },
    Nixpkgs(Nixpkgs),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct TomlDocument {
    sources: Vec<Source>,
}

impl Source {
    pub fn to_flake_ref(&self) -> FlakeRef {
        match self {
            Source::Github {
                owner,
                repo,
                attrs,
                ..
            } => attrs.append_to(&format!("github:{}/{}", owner, repo)),
            Source::Gitlab { owner, repo, attrs } => {
                attrs.append_to(&format!("gitlab:{}/{}", owner, repo))
            }
            Source::SourceHut { owner, repo, attrs } => {
                attrs.append_to(&format!("sourcehut:{}/{}", owner, repo))
            }
            Source::Git { url, attrs } => attrs.append_to(url),
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
            Ok(serde_json::from_str(&buf)?)
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
