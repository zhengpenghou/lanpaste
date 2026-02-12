use std::{
    fs::{self, File, OpenOptions},
    path::Path,
    process::Command,
};

use fs2::FileExt;

use crate::{
    config::{PushMode, ServeCmd},
    errors::{AppError, AppResult},
    types::{GitCommitResult, PasteDraft},
};

pub struct FileLock {
    file: File,
}

impl FileLock {
    pub fn acquire(path: &Path) -> AppResult<Self> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| AppError::io("create lock parent", e))?;
        }
        let file = OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .truncate(false)
            .open(path)
            .map_err(|e| AppError::io("open lock", e))?;
        file.try_lock_exclusive()
            .map_err(|_| AppError::Conflict("already running".to_string()))?;
        Ok(Self { file })
    }
}

impl Drop for FileLock {
    fn drop(&mut self) {
        let _ = fs2::FileExt::unlock(&self.file);
    }
}

pub fn check_git_installed() -> AppResult<()> {
    let out = Command::new("git")
        .arg("--version")
        .output()
        .map_err(|_| {
            AppError::ServiceUnavailable(
                "git is required. Install with: Debian/Ubuntu `sudo apt-get install git`, Fedora `sudo dnf install git`, Arch `sudo pacman -S git`, macOS `xcode-select --install`".to_string(),
            )
        })?;
    if out.status.success() {
        Ok(())
    } else {
        Err(AppError::ServiceUnavailable("git is required".to_string()))
    }
}

pub fn run_git(repo: &Path, args: &[&str], cfg: &ServeCmd) -> AppResult<String> {
    let mut cmd = Command::new("git");
    cmd.args(args).current_dir(repo);
    cmd.env("GIT_AUTHOR_NAME", &cfg.git_author_name)
        .env("GIT_AUTHOR_EMAIL", &cfg.git_author_email)
        .env("GIT_COMMITTER_NAME", &cfg.git_author_name)
        .env("GIT_COMMITTER_EMAIL", &cfg.git_author_email);
    let out = cmd
        .output()
        .map_err(|e| AppError::internal(format!("git {:?} failed: {e}", args)))?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
    } else {
        Err(AppError::internal(format!(
            "git {:?}: {}",
            args,
            String::from_utf8_lossy(&out.stderr)
        )))
    }
}

pub fn is_git_repo(repo: &Path, cfg: &ServeCmd) -> bool {
    run_git(repo, &["rev-parse", "--is-inside-work-tree"], cfg)
        .map(|v| v == "true")
        .unwrap_or(false)
}

pub fn bootstrap_repo(repo: &Path, cfg: &ServeCmd) -> AppResult<()> {
    fs::create_dir_all(repo).map_err(|e| AppError::io("create repo", e))?;

    if !is_git_repo(repo, cfg) {
        run_git(repo, &["init"], cfg)?;
    }

    fs::create_dir_all(repo.join("pastes")).map_err(|e| AppError::io("create pastes", e))?;
    fs::create_dir_all(repo.join("meta")).map_err(|e| AppError::io("create meta", e))?;

    let readme = repo.join("README.md");
    if !readme.exists() {
        fs::write(&readme, "# LAN Paste\n\nGit-backed LAN paste store.\n")
            .map_err(|e| AppError::io("write readme", e))?;
    }

    let gitignore = repo.join(".gitignore");
    let required = [
        "# runtime / scratch (defensive)",
        "../run/",
        "../tmp/",
        "",
        "# common temp/intermediate",
        "*.tmp",
        "*.swp",
        "*.bak",
        "*.part",
        "*.lock",
        "*.log",
        "",
        "# OS/editor noise",
        ".DS_Store",
        "Thumbs.db",
        ".idea/",
        ".vscode/",
        "",
        "# Rust build artifacts",
        "target/",
    ];

    let mut content = if gitignore.exists() {
        fs::read_to_string(&gitignore).map_err(|e| AppError::io("read gitignore", e))?
    } else {
        String::new()
    };
    for line in required {
        if !content.contains(line) {
            content.push_str(line);
            content.push('\n');
        }
    }
    fs::write(&gitignore, content).map_err(|e| AppError::io("write gitignore", e))?;

    let has_commit = run_git(repo, &["rev-parse", "--verify", "HEAD"], cfg).is_ok();
    if !has_commit {
        run_git(repo, &["add", "README.md", ".gitignore", "pastes", "meta"], cfg)?;
        run_git(repo, &["commit", "-m", "init lanpaste repository"], cfg)?;
    }
    Ok(())
}

pub fn commit_paste(
    repo: &Path,
    cfg: &ServeCmd,
    draft: &PasteDraft,
    push_mode: PushMode,
    remote: &str,
) -> AppResult<GitCommitResult> {
    run_git(repo, &["add", &draft.rel_path, &draft.meta_rel_path], cfg)?;
    run_git(repo, &["commit", "-m", &draft.subject], cfg)?;
    let commit = run_git(repo, &["rev-parse", "--short=12", "HEAD"], cfg)?;

    match push_mode {
        PushMode::Off => Ok(GitCommitResult {
            commit,
            pushed: false,
            push_error: None,
        }),
        PushMode::BestEffort => {
            let push_res = run_git(repo, &["push", remote, "HEAD"], cfg);
            let push_error = push_res.err().map(|e| format!("{e:?}"));
            Ok(GitCommitResult {
                commit,
                pushed: push_error.is_none(),
                push_error,
            })
        }
        PushMode::Strict => {
            if let Err(push_err) = run_git(repo, &["push", remote, "HEAD"], cfg) {
                let _ = run_git(repo, &["reset", "--soft", "HEAD~1"], cfg);
                let _ = fs::remove_file(&draft.abs_path);
                let _ = fs::remove_file(&draft.meta_path);
                let _ = run_git(repo, &["reset"], cfg);
                return Err(AppError::Internal(format!("push failed in strict mode: {push_err:?}")));
            }
            Ok(GitCommitResult {
                commit,
                pushed: true,
                push_error: None,
            })
        }
    }
}

pub fn ready(repo: &Path, git_lock: &Path, cfg: &ServeCmd) -> AppResult<()> {
    if !is_git_repo(repo, cfg) {
        return Err(AppError::ServiceUnavailable("repo not ready".to_string()));
    }
    let _lock = FileLock::acquire(git_lock)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn push_mode_display() {
        assert_eq!(crate::types::push_mode_label(PushMode::BestEffort), "best_effort");
    }
}
