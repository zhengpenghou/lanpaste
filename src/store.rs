use std::{
    fs,
    path::{Path, PathBuf},
};

use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;
use time::OffsetDateTime;
use ulid::Ulid;

use crate::{
    config::ServeCmd,
    errors::{AppError, AppResult},
    gitops,
    types::{CreatePasteInput, PasteDraft, PasteMeta},
};

const MAX_SLUG_LEN: usize = 80;

pub fn verify_token(expected: Option<&str>, provided: Option<&str>) -> AppResult<()> {
    match expected {
        None => Ok(()),
        Some(exp) => {
            let got = provided.unwrap_or_default();
            if exp.as_bytes().ct_eq(got.as_bytes()).into() {
                Ok(())
            } else {
                Err(AppError::Unauthorized(
                    "missing or invalid token".to_string(),
                ))
            }
        }
    }
}

pub fn check_cidr(allow: &[ipnet::IpNet], ip: Option<std::net::IpAddr>) -> AppResult<()> {
    if allow.is_empty() {
        return Ok(());
    }
    let addr = ip.ok_or_else(|| AppError::Forbidden("client IP unavailable".to_string()))?;
    if allow.iter().any(|n| n.contains(&addr)) {
        Ok(())
    } else {
        Err(AppError::Forbidden(
            "client IP not in allowlist".to_string(),
        ))
    }
}

pub fn sanitize_name(name: &str) -> AppResult<String> {
    if name.contains('/') || name.contains('\\') || name.contains("..") {
        return Err(AppError::BadRequest("invalid name".to_string()));
    }
    if name.starts_with('.') {
        return Err(AppError::BadRequest("invalid name".to_string()));
    }
    let normalized = name.trim().replace(' ', "-");
    if normalized.is_empty() {
        return Ok("paste".to_string());
    }
    let mut out = String::new();
    for ch in normalized.chars() {
        if ch.is_ascii_alphanumeric() || matches!(ch, '.' | '_' | '-') {
            out.push(ch);
        } else {
            out.push('-');
        }
    }
    while out.contains("--") {
        out = out.replace("--", "-");
    }
    out = out.trim_matches('-').to_string();
    if out.is_empty() {
        out = "paste".to_string();
    }
    if out.len() > MAX_SLUG_LEN {
        out.truncate(MAX_SLUG_LEN);
    }
    Ok(out)
}

pub fn choose_ext(name: Option<&str>, content_type: Option<&str>) -> &'static str {
    let is_md_ct = content_type
        .map(|v| v.to_ascii_lowercase().contains("text/markdown"))
        .unwrap_or(false);
    let is_md_name = name
        .map(|n| n.to_ascii_lowercase().ends_with(".md"))
        .unwrap_or(false);
    if is_md_ct || is_md_name { "md" } else { "txt" }
}

pub fn build_paste_draft(
    repo: &Path,
    cfg: &ServeCmd,
    input: CreatePasteInput,
) -> AppResult<PasteDraft> {
    let id = Ulid::new().to_string();
    let created_at = OffsetDateTime::now_utc();
    let date_path = created_at
        .format(
            &time::format_description::parse("[year]/[month]/[day]")
                .map_err(|e| AppError::internal(format!("date format parse failed: {e}")))?,
        )
        .map_err(|e| AppError::internal(format!("date format failed: {e}")))?;

    let name = input.name.as_deref().unwrap_or("paste");
    let slug = sanitize_name(name)?;
    let ext = choose_ext(input.name.as_deref(), input.content_type.as_deref());
    let file_name = format!("{id}__{slug}.{ext}");
    let rel_path = format!("pastes/{date_path}/{file_name}");
    let abs_path = repo.join(&rel_path);

    let mut hasher = Sha256::new();
    hasher.update(&input.bytes);
    let sha256 = hex::encode(hasher.finalize());

    let content_type = if ext == "md" {
        "text/markdown; charset=utf-8".to_string()
    } else {
        input
            .content_type
            .unwrap_or_else(|| "text/plain; charset=utf-8".to_string())
    };

    let mut subject = format!("paste: {id} {slug}");
    if let Some(tag) = &input.tag {
        subject.push_str(&format!(" [tag:{tag}]"));
    }
    if let Some(msg) = input.msg {
        subject = msg;
    }

    let meta_rel_path = format!("meta/{id}.json");
    let meta_path = repo.join(&meta_rel_path);
    let meta = PasteMeta {
        id: id.clone(),
        created_at,
        path: rel_path.clone(),
        size: input.bytes.len(),
        content_type: content_type.clone(),
        commit: String::new(),
        sha256: sha256.clone(),
        tag: input.tag,
        client_ip: input.client_ip,
        user_agent: input.user_agent,
    };

    if let Some(parent) = abs_path.parent() {
        fs::create_dir_all(parent).map_err(|e| AppError::io("create paste parent", e))?;
    }
    fs::create_dir_all(repo.join("meta")).map_err(|e| AppError::io("create meta dir", e))?;
    fs::write(&abs_path, &input.bytes).map_err(|e| AppError::io("write paste", e))?;
    fs::write(
        &meta_path,
        serde_json::to_vec_pretty(&meta)
            .map_err(|e| AppError::internal(format!("serialize meta: {e}")))?,
    )
    .map_err(|e| AppError::io("write meta", e))?;

    let _ = cfg;
    Ok(PasteDraft {
        id,
        rel_path,
        abs_path,
        meta_path,
        meta_rel_path,
        content_type,
        size: input.bytes.len(),
        sha256,
        subject,
        meta,
    })
}

fn lookup_commit(repo: &Path, cfg: &ServeCmd, rel_path: &str) -> AppResult<String> {
    let full = gitops::run_git(
        repo,
        &["log", "-n", "1", "--format=%H", "--", rel_path],
        cfg,
    )?;
    Ok(full.chars().take(12).collect())
}

fn hydrate_commit(repo: &Path, cfg: &ServeCmd, mut meta: PasteMeta) -> AppResult<PasteMeta> {
    if meta.commit.is_empty() {
        meta.commit = lookup_commit(repo, cfg, &meta.path)?;
    }
    Ok(meta)
}

pub fn read_meta(repo: &Path, cfg: &ServeCmd, id: &str) -> AppResult<PasteMeta> {
    let path = repo.join("meta").join(format!("{id}.json"));
    if !path.exists() {
        return Err(AppError::NotFound("paste not found".to_string()));
    }
    let data = fs::read(&path).map_err(|e| AppError::io("read meta", e))?;
    let meta = serde_json::from_slice(&data)
        .map_err(|e| AppError::internal(format!("parse meta: {e}")))?;
    hydrate_commit(repo, cfg, meta)
}

pub fn read_recent(
    repo: &Path,
    cfg: &ServeCmd,
    n: usize,
    tag: Option<&str>,
) -> AppResult<Vec<PasteMeta>> {
    let meta_dir = repo.join("meta");
    if !meta_dir.exists() {
        return Ok(Vec::new());
    }
    let mut metas = Vec::new();
    for entry in fs::read_dir(meta_dir).map_err(|e| AppError::io("read meta dir", e))? {
        let entry = entry.map_err(|e| AppError::io("read meta entry", e))?;
        let p = entry.path();
        if p.extension().and_then(|s| s.to_str()) != Some("json") {
            continue;
        }
        let data = fs::read(&p).map_err(|e| AppError::io("read meta file", e))?;
        if let Ok(meta) = serde_json::from_slice::<PasteMeta>(&data) {
            if let Some(expected) = tag
                && meta.tag.as_deref() != Some(expected)
            {
                continue;
            }
            metas.push(hydrate_commit(repo, cfg, meta)?);
        }
    }
    metas.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    metas.truncate(n);
    Ok(metas)
}

pub fn read_paste(repo: &Path, meta: &PasteMeta) -> AppResult<Vec<u8>> {
    fs::read(repo.join(&meta.path)).map_err(|e| AppError::io("read paste", e))
}

pub fn remove_files(paths: &[PathBuf]) {
    for path in paths {
        let _ = fs::remove_file(path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{PushMode, ServeCmd};

    #[test]
    fn sanitize_ok() {
        assert_eq!(sanitize_name("my note.md").expect("sanitize"), "my-note.md");
    }

    #[test]
    fn sanitize_rejects_path() {
        assert!(sanitize_name("../a").is_err());
    }

    #[test]
    fn ext_selection() {
        assert_eq!(choose_ext(Some("a.md"), None), "md");
        assert_eq!(choose_ext(None, Some("text/markdown")), "md");
        assert_eq!(choose_ext(Some("a.txt"), Some("text/plain")), "txt");
    }

    #[test]
    fn token_cmp() {
        assert!(verify_token(Some("abc"), Some("abc")).is_ok());
        assert!(verify_token(Some("abc"), Some("abd")).is_err());
    }

    #[test]
    fn cidr_match() {
        let allow: Vec<ipnet::IpNet> = vec!["192.168.1.0/24".parse().expect("parse")];
        assert!(check_cidr(&allow, Some("192.168.1.8".parse().expect("parse"))).is_ok());
        assert!(check_cidr(&allow, Some("10.0.0.1".parse().expect("parse"))).is_err());
    }

    #[test]
    fn build_draft_layout() {
        let td = tempfile::tempdir().expect("tempdir");
        let repo = td.path().join("repo");
        std::fs::create_dir_all(&repo).expect("mkdir");
        let cfg = ServeCmd {
            dir: td.path().to_path_buf(),
            bind: "127.0.0.1:0".parse().expect("bind"),
            token: None,
            max_bytes: 1024,
            push: PushMode::Off,
            remote: "origin".to_string(),
            allow_cidr: vec![],
            git_author_name: "LAN Paste".to_string(),
            git_author_email: "paste@lan".to_string(),
        };
        let draft = build_paste_draft(
            &repo,
            &cfg,
            CreatePasteInput {
                name: Some("n.md".to_string()),
                msg: None,
                tag: Some("t".to_string()),
                content_type: Some("text/markdown".to_string()),
                bytes: b"hello".to_vec(),
                client_ip: None,
                user_agent: None,
            },
        )
        .expect("draft");
        assert!(draft.rel_path.starts_with("pastes/"));
        assert!(draft.rel_path.ends_with(".md"));
        assert!(draft.meta_rel_path.starts_with("meta/"));
        assert!(draft.abs_path.exists());
        assert!(draft.meta_path.exists());
    }
}
