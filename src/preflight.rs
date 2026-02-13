use std::{fs, fs::OpenOptions, path::PathBuf, sync::Arc};

use fs2::FileExt;

use crate::{
    auth::ApiKeyStore,
    config::ServeCmd,
    errors::{AppError, AppResult},
    gitops,
    types::{AppPaths, AppState},
};

pub fn run_preflight(cfg: &ServeCmd) -> AppResult<()> {
    gitops::check_git_installed()?;
    let paths = AppPaths::from_base(cfg.dir.clone());
    fs::create_dir_all(&paths.run).map_err(|e| AppError::io("create run dir", e))?;
    fs::create_dir_all(&paths.idempotency)
        .map_err(|e| AppError::io("create idempotency dir", e))?;
    fs::create_dir_all(&paths.tmp).map_err(|e| AppError::io("create tmp dir", e))?;
    fs::create_dir_all(&paths.repo).map_err(|e| AppError::io("create repo dir", e))?;

    let write_test = paths.run.join(".write_test");
    fs::write(&write_test, b"ok").map_err(|e| AppError::io("write test file", e))?;
    fs::remove_file(&write_test).map_err(|e| AppError::io("cleanup write test", e))?;

    gitops::bootstrap_repo(&paths.repo, cfg)?;
    Ok(())
}

pub fn build_state(cfg: ServeCmd) -> AppResult<AppState> {
    let paths = AppPaths::from_base(cfg.dir.clone());
    let api_keys = ApiKeyStore::from_file(cfg.api_keys_file.as_deref())?;
    let lock_path = paths.run.join("daemon.lock");
    let daemon_lock = OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .truncate(false)
        .open(lock_path)
        .map_err(|e| AppError::io("open daemon lock", e))?;
    daemon_lock
        .try_lock_exclusive()
        .map_err(|_| AppError::Conflict("already running".to_string()))?;

    Ok(AppState {
        cfg,
        paths,
        _daemon_lock: Arc::new(daemon_lock),
        api_keys,
    })
}

pub fn _resolve_paths(base: PathBuf) -> AppPaths {
    AppPaths::from_base(base)
}
