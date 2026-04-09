use anyhow::Result;
use log::warn;
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
    },
    Nixpkgs(Nixpkgs),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct TomlDocument {
    sources: Vec<Source>,
}

#[derive(Debug, Clone, Deserialize)]
struct RegistryDocument {
    flakes: Vec<RegistryEntry>,
}

#[derive(Debug, Clone, Deserialize)]
struct RegistryEntry {
    #[serde(default)]
    _from: Option<RegistryInput>,
    to: RegistryInput,
}

#[derive(Debug, Clone, Deserialize)]
struct RegistryInput {
    #[serde(rename = "type")]
    kind: String,
    owner: Option<String>,
    repo: Option<String>,
    url: Option<String>,
    #[serde(rename = "ref")]
    ref_field: Option<String>,
    rev: Option<String>,
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

        if path.extension() == Some(OsStr::new("toml")) {
            warn!(
                "TOML flake lists are deprecated. Please use a nix flake registry JSON file instead: {}",
                path.display()
            );
            let document: TomlDocument = toml::from_str(&buf)
                .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))?;
            Ok(document.sources)
        } else {
            if let Ok(registry) = serde_json::from_str::<RegistryDocument>(&buf) {
                registry.into_sources()
            } else {
                serde_json::from_str(&buf)
                    .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))
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

impl RegistryDocument {
    fn into_sources(self) -> io::Result<Vec<Source>> {
        self.flakes
            .into_iter()
            .map(|entry| entry.to.into_source())
            .collect()
    }
}

impl RegistryInput {
    fn into_source(self) -> io::Result<Source> {
        match self.kind.as_str() {
            "github" => Ok(Source::Github {
                owner: self.owner.ok_or_else(|| {
                    io::Error::new(
                        io::ErrorKind::InvalidData,
                        "registry entry missing GitHub owner",
                    )
                })?,
                repo: self.repo.ok_or_else(|| {
                    io::Error::new(
                        io::ErrorKind::InvalidData,
                        "registry entry missing GitHub repo",
                    )
                })?,
                description: None,
                git_ref: self.ref_field.or(self.rev),
            }),
            "gitlab" => Ok(Source::Gitlab {
                owner: self.owner.ok_or_else(|| {
                    io::Error::new(
                        io::ErrorKind::InvalidData,
                        "registry entry missing GitLab owner",
                    )
                })?,
                repo: self.repo.ok_or_else(|| {
                    io::Error::new(
                        io::ErrorKind::InvalidData,
                        "registry entry missing GitLab repo",
                    )
                })?,
                git_ref: self.ref_field.or(self.rev),
            }),
            "sourcehut" => Ok(Source::SourceHut {
                owner: self.owner.ok_or_else(|| {
                    io::Error::new(
                        io::ErrorKind::InvalidData,
                        "registry entry missing SourceHut owner",
                    )
                })?,
                repo: self.repo.ok_or_else(|| {
                    io::Error::new(
                        io::ErrorKind::InvalidData,
                        "registry entry missing SourceHut repo",
                    )
                })?,
                git_ref: self.ref_field.or(self.rev),
            }),
            "git" => {
                let mut url = self.url.ok_or_else(|| {
                    io::Error::new(io::ErrorKind::InvalidData, "registry entry missing git url")
                })?;

                if !url.starts_with("git+") {
                    url = format!("git+{}", url);
                }

                if let Some(rev) = self.rev {
                    let joiner = if url.contains('?') { "&" } else { "?" };
                    url = format!("{url}{joiner}rev={rev}");
                } else if let Some(git_ref) = self.ref_field {
                    let joiner = if url.contains('?') { "&" } else { "?" };
                    url = format!("{url}{joiner}ref={git_ref}");
                }

                Ok(Source::Git { url })
            }
            other => Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("unsupported registry entry type '{}'", other),
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{fs, process};

    fn write_temp(name: &str, contents: &str) -> std::path::PathBuf {
        let path = std::env::temp_dir().join(format!("flake-info-{name}-{}", process::id()));
        fs::write(&path, contents).unwrap();
        path
    }

    #[test]
    fn reads_registry_format() {
        let registry = r#"
{
  "flakes": [
    {
      "from": { "type": "indirect", "id": "example" },
      "to": { "type": "github", "owner": "demo", "repo": "pkg", "ref": "main" }
    },
    {
      "from": { "type": "indirect", "id": "git-example" },
      "to": { "type": "git", "url": "https://example.invalid/thing.git", "rev": "abc123" }
    }
  ]
}
        "#;
        let path = write_temp("registry", registry);
        let sources = Source::read_sources_file(&path).unwrap();
        assert_eq!(
            sources,
            vec![
                Source::Github {
                    owner: "demo".to_string(),
                    repo: "pkg".to_string(),
                    description: None,
                    git_ref: Some("main".to_string())
                },
                Source::Git {
                    url: "git+https://example.invalid/thing.git?rev=abc123".to_string()
                }
            ]
        );
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn reads_legacy_json_array() {
        let legacy = r#"[{"type": "git", "url": "git+https://example.invalid/repo?ref=main"}]"#;
        let path = write_temp("legacy", legacy);
        let sources = Source::read_sources_file(&path).unwrap();
        assert_eq!(
            sources,
            vec![Source::Git {
                url: "git+https://example.invalid/repo?ref=main".to_string()
            }]
        );
        fs::remove_file(path).unwrap();
    }
}
