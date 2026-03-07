use fs_err as fs;
use std::path::Path;
use url::Url;

pub struct Repository(git2::Repository);

impl Repository {
    /// Tries to open a bare repository from a local directory.
    pub fn open_bare<P: AsRef<Path>>(path: P) -> Result<Self, git2::Error> {
        Ok(Self(git2::Repository::open_bare(path)?))
    }

    /// Tries to clone a bare repository from a source URL
    /// into a local directory.
    pub fn clone_bare(url: &Url, path: &Path) -> Result<Self, git2::Error> {
        CloneOptions::new().clone_bare(url, path)
    }

    /// Checks if the repository has a Git reference.
    /// It returns true if it does, false if it doesn't or an error
    /// occurred while querying the repository.
    pub fn has_ref(&self, git_ref: &str) -> bool {
        self.0.find_reference(git_ref).is_ok()
    }

    pub fn reference(&'_ self, git_ref: &str) -> Result<Reference<'_>, git2::Error> {
        Ok(Reference(self.0.find_reference(git_ref)?))
    }

    /// Fetches new references from upstream, ensuring
    /// all `git_ref`s are available.
    pub fn update(&self, git_refs: &[&str]) -> Result<(), git2::Error> {
        let mut remote = self.0.find_remote("origin")?;
        remote.fetch(git_refs, None, None)
    }

    /// Adds a new worktree.
    pub fn add_worktree(&self, path: &Path, reference: &Reference<'_>) -> Result<Worktree, git2::Error> {
        let mut options = git2::WorktreeAddOptions::new();
        options.reference(Some(&reference.0));

        let name = path
            .file_name()
            .ok_or(git2::Error::new(
                git2::ErrorCode::Invalid,
                git2::ErrorClass::Reference,
                "invalid branch name \"\"",
            ))?
            .to_string_lossy();

        Ok(Worktree(self.0.worktree(&name, path, Some(&options))?))
    }
}

pub struct Worktree(git2::Worktree);

impl Worktree {
    pub fn remove(self) -> Result<(), git2::Error> {
        let mut force_prune = git2::WorktreePruneOptions::new();
        force_prune.locked(true).valid(true);

        self.0.prune(Some(&mut force_prune))?;
        fs::remove_dir_all(self.0.path())
            .map_err(|err| git2::Error::new(git2::ErrorCode::Directory, git2::ErrorClass::Os, err.to_string()))
    }
}

pub struct Reference<'a>(git2::Reference<'a>);

impl<'a> Reference<'a> {
    pub fn hash(&'a self) -> Result<String, git2::Error> {
        let reff = self.0.resolve()?;
        // The unwrap won't panic, because `resolve` ensures
        // the reference is direct. There's no way `target` returns
        // None.
        Ok(reff.target().unwrap().to_string())
    }
}

#[derive(Default)]
pub struct CloneOptions<'a> {
    progress_callback: Option<Box<git2::IndexerProgress<'a>>>,
}

impl<'a> CloneOptions<'a> {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn progress_callback<F>(&mut self, cb: F) -> &mut Self
    where
        F: FnMut(git2::Progress<'_>) -> bool + 'a,
    {
        self.progress_callback = Some(Box::new(cb));
        self
    }

    pub fn clone_bare(self, url: &Url, path: &Path) -> Result<Repository, git2::Error> {
        let mut builder = git2::build::RepoBuilder::new();
        builder.bare(true);

        if let Some(cb) = self.progress_callback {
            let mut remote_callbacks = git2::RemoteCallbacks::new();
            remote_callbacks.transfer_progress(cb);

            let mut fetch_options = git2::FetchOptions::new();
            fetch_options.remote_callbacks(remote_callbacks);

            builder.fetch_options(fetch_options);
        }

        Ok(Repository(builder.clone(url.as_str(), path)?))
    }
}
