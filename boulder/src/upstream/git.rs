// SPDX-FileCopyrightText: Copyright © 2026 Serpent OS Developers
//
// SPDX-License-Identifier: MPL-2.0

use std::{
    io,
    path::{Path, PathBuf},
    string,
};

use moss::util;
use thiserror::Error;
use tui::{ProgressBar, ProgressStyle};
use url::Url;

#[derive(Clone, Debug)]
pub struct Git {
    pub url: Url,
    pub ref_id: String,
}

impl Git {
    pub async fn fetch_new(url: &Url, container_dir: &Path) -> Result<Self, Error> {
        Self::fetch_new_progress(url, container_dir, &ProgressBar::hidden()).await
    }

    pub async fn fetch_new_progress(url: &Url, container_dir: &Path, pb: &ProgressBar) -> Result<Self, Error> {
        todo!()
    }

    pub fn name(&self) -> &str {
        util::uri_file_name(&self.url)
    }

    pub async fn store(&self, storage_dir: &Path, pb: &ProgressBar) -> Result<StoredGit, Error> {
        tokio::fs::create_dir_all(storage_dir).await?;

        let dir = storage_dir.join(self.directory_name());
        let mut cached = true;
        let repo = match git2wrap::Repository::open_bare(&dir) {
            Ok(repo) => {
                if !repo.has_ref(&self.ref_id) {
                    cached = false;
                    repo.update(&[&self.ref_id])?
                }
                Ok(repo)
            }
            Err(err) if matches!(err.code(), git2::ErrorCode::NotFound) => {
                cached = false;
                let repo = clone_bare(&self.url, &dir, pb);
                if matches!(repo, Err(_)) {
                    tokio::fs::remove_dir_all(&dir).await;
                }
                repo
            }
            Err(err) => Err(err),
        }?;

        Ok(StoredGit {
            name: self.name().to_owned(),
            was_cached: cached,
            repo,
            reference,
        })
    }

    pub async fn remove(&self, storage_dir: &Path) -> Result<(), Error> {
        let result = tokio::fs::remove_dir_all(storage_dir.join(self.directory_name())).await;
        if let Err(err) = result
            && err.kind() != io::ErrorKind::NotFound
        {
            Err(Error::from(err))
        } else {
            Ok(())
        }
    }

    /// Returns the name of the directory that should contain
    /// the Git repository.
    /// It is a composition of the hostname and the repository name
    /// so that it's unique.
    fn directory_name(&self) -> PathBuf {
        let host = self.url.host_str();
        let path = self.url.path();

        let mut name = String::with_capacity(host.unwrap_or("").len() + 1 + path.len());
        if let Some(host) = host {
            name.push_str(host);
            name.push('.');
        }
        name.push_str(&path.replace('/', "."));
        name.into()
    }
}

pub struct StoredGit {
    pub name: String,
    pub was_cached: bool,
    repo: git2wrap::Repository,
    reference: String,
}

impl StoredGit {
    pub async fn share(&self, dest_dir: &Path) -> Result<SharedGit, Error> {
        Ok(SharedGit(
            self.repo
                .add_worktree(dest_dir, &self.repo.reference(&self.reference)?)?,
        ))
    }
}

struct SharedGit(git2wrap::Worktree);

impl SharedGit {
    pub fn remove(self) -> Result<(), Error> {
        self.0.remove().map_err(Error::from)
    }
}

#[derive(Debug, Error)]
pub enum Error {
    #[error("{0}")]
    Git(#[from] git2::Error),
    #[error("{0}")]
    Io(#[from] io::Error),
}

fn clone_bare(url: &Url, path: &Path, pb: &ProgressBar) -> Result<git2wrap::Repository, git2::Error> {
    pb.set_style(
        ProgressStyle::with_template(" {spinner} {wide_msg} ")
            .unwrap()
            .tick_chars("--=≡■≡=--"),
    );

    let mut previous_total = 0;

    let mut options = git2wrap::CloneOptions::new();
    options.progress_callback(move |progress| {
        let delta = progress.received_bytes() - previous_total;
        pb.inc(delta as u64);
        previous_total = progress.received_bytes();
        true
    });
    options.clone_bare(url, path)
}

#[derive(Debug, Error)]
pub enum GitError {
    #[error("ref '{ref_id}' did not resolve to a valid commit hash for {uri}")]
    UnresolvedRef { ref_id: String, uri: Url },
    #[error(transparent)]
    Io(#[from] io::Error),
    #[error(transparent)]
    Utf8(#[from] string::FromUtf8Error),
}
