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
    types::{AppPaths, CreatePasteInput, FileMeta, IdempotencyRecord, PasteDraft, PasteMeta, UploadResponse},
};

const MAX_SLUG_LEN: usize = 80;

#[derive(Debug, serde::Serialize, serde::Deserialize)]
struct SlugRecord {
    slug: String,
    id: String,
    created_at: OffsetDateTime,
}

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

fn sanitize_slug_candidate(name: &str) -> AppResult<String> {
    let sanitized = sanitize_name(name)?;
    let stem = sanitized
        .rsplit_once('.')
        .map(|(s, _)| s)
        .unwrap_or(sanitized.as_str());
    let mut out = String::new();
    for ch in stem.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
        } else if matches!(ch, '-' | '_') {
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

fn slug_record_path(repo: &Path, slug: &str) -> PathBuf {
    repo.join("slugs").join(format!("{slug}.json"))
}

fn unique_slug(repo: &Path, base_slug: &str) -> String {
    if !slug_record_path(repo, base_slug).exists() {
        return base_slug.to_string();
    }
    for suffix in 2.. {
        let suffix_text = format!("-{suffix}");
        let max_base = MAX_SLUG_LEN.saturating_sub(suffix_text.len());
        let trimmed = if base_slug.len() > max_base {
            &base_slug[..max_base]
        } else {
            base_slug
        };
        let candidate = format!("{trimmed}{suffix_text}");
        if !slug_record_path(repo, &candidate).exists() {
            return candidate;
        }
    }
    base_slug.to_string()
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
    let slug = unique_slug(repo, &sanitize_slug_candidate(name)?);
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
        slug: Some(slug.clone()),
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
    fs::create_dir_all(repo.join("slugs")).map_err(|e| AppError::io("create slugs dir", e))?;
    fs::write(&abs_path, &input.bytes).map_err(|e| AppError::io("write paste", e))?;
    fs::write(
        &meta_path,
        serde_json::to_vec_pretty(&meta)
            .map_err(|e| AppError::internal(format!("serialize meta: {e}")))?,
    )
    .map_err(|e| AppError::io("write meta", e))?;
    let slug_rel_path = format!("slugs/{slug}.json");
    let slug_path = repo.join(&slug_rel_path);
    let slug_record = SlugRecord {
        slug: slug.clone(),
        id: id.clone(),
        created_at,
    };
    fs::write(
        &slug_path,
        serde_json::to_vec_pretty(&slug_record)
            .map_err(|e| AppError::internal(format!("serialize slug map: {e}")))?,
    )
    .map_err(|e| AppError::io("write slug map", e))?;

    let _ = cfg;
    Ok(PasteDraft {
        id,
        slug,
        rel_path,
        abs_path,
        meta_path,
        meta_rel_path,
        slug_rel_path,
        slug_path,
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

pub fn list_tags(repo: &Path) -> AppResult<Vec<(String, usize)>> {
    let meta_dir = repo.join("meta");
    if !meta_dir.exists() {
        return Ok(Vec::new());
    }
    let mut counts = std::collections::HashMap::<String, usize>::new();
    for entry in fs::read_dir(meta_dir).map_err(|e| AppError::io("read meta dir", e))? {
        let entry = entry.map_err(|e| AppError::io("read meta entry", e))?;
        let p = entry.path();
        if p.extension().and_then(|s| s.to_str()) != Some("json") {
            continue;
        }
        let data = fs::read(&p).map_err(|e| AppError::io("read meta file", e))?;
        if let Ok(meta) = serde_json::from_slice::<PasteMeta>(&data)
            && let Some(tag) = meta.tag
            && !tag.trim().is_empty()
        {
            *counts.entry(tag).or_insert(0) += 1;
        }
    }

    let mut out: Vec<(String, usize)> = counts.into_iter().collect();
    out.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    Ok(out)
}

pub fn read_paste(repo: &Path, meta: &PasteMeta) -> AppResult<Vec<u8>> {
    fs::read(repo.join(&meta.path)).map_err(|e| AppError::io("read paste", e))
}

fn detect_image_type(bytes: &[u8]) -> Option<(&'static str, &'static str)> {
    if bytes.starts_with(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]) {
        return Some(("png", "image/png"));
    }
    if bytes.len() >= 3 && bytes[0] == 0xFF && bytes[1] == 0xD8 && bytes[2] == 0xFF {
        return Some(("jpg", "image/jpeg"));
    }
    if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        return Some(("gif", "image/gif"));
    }
    if bytes.len() >= 12 && &bytes[0..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        return Some(("webp", "image/webp"));
    }
    None
}

fn content_type_for_ext(ext: &str) -> Option<&'static str> {
    match ext {
        "png" => Some("image/png"),
        "jpg" | "jpeg" => Some("image/jpeg"),
        "gif" => Some("image/gif"),
        "webp" => Some("image/webp"),
        _ => None,
    }
}

pub fn persist_upload(
    paths: &AppPaths,
    bytes: &[u8],
    name: Option<String>,
    tag: Option<String>,
) -> AppResult<UploadResponse> {
    if bytes.is_empty() {
        return Err(AppError::BadRequest("file is empty".to_string()));
    }

    let (ext, content_type) = detect_image_type(bytes).ok_or_else(|| {
        AppError::BadRequest("unsupported image type; allowed: png, jpg, webp, gif".to_string())
    })?;
    let dims = imagesize::blob_size(bytes)
        .map_err(|_| AppError::BadRequest("invalid image payload".to_string()))?;

    let mut hasher = Sha256::new();
    hasher.update(bytes);
    let id = hex::encode(hasher.finalize());
    let file_name = format!("{id}.{ext}");
    let file_path = paths.files.join(&file_name);
    let meta_path = paths.files_meta.join(format!("{id}.json"));

    fs::create_dir_all(&paths.files).map_err(|e| AppError::io("create files dir", e))?;
    fs::create_dir_all(&paths.files_meta).map_err(|e| AppError::io("create files meta dir", e))?;

    if !file_path.exists() {
        fs::write(&file_path, bytes).map_err(|e| AppError::io("write upload file", e))?;
    }

    let meta = if meta_path.exists() {
        let data = fs::read(&meta_path).map_err(|e| AppError::io("read upload meta", e))?;
        serde_json::from_slice::<FileMeta>(&data)
            .map_err(|e| AppError::internal(format!("parse upload meta: {e}")))?
    } else {
        let meta = FileMeta {
            id: id.clone(),
            ext: ext.to_string(),
            content_type: content_type.to_string(),
            bytes: bytes.len(),
            width: dims.width,
            height: dims.height,
            created_at: OffsetDateTime::now_utc(),
            name,
            tag,
        };
        let raw = serde_json::to_vec_pretty(&meta)
            .map_err(|e| AppError::internal(format!("serialize upload meta: {e}")))?;
        fs::write(&meta_path, raw).map_err(|e| AppError::io("write upload meta", e))?;
        meta
    };

    Ok(UploadResponse {
        id: meta.id,
        url: format!("/files/{file_name}"),
        content_type: meta.content_type,
        bytes: meta.bytes,
        width: meta.width,
        height: meta.height,
        created_at: meta.created_at,
    })
}

pub fn read_uploaded_file(paths: &AppPaths, file_name: &str) -> AppResult<(Vec<u8>, String)> {
    if file_name.is_empty()
        || file_name.contains('/')
        || file_name.contains('\\')
        || file_name.contains("..")
        || !file_name
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '.' | '_' | '-'))
    {
        return Err(AppError::BadRequest("invalid file path".to_string()));
    }

    let ext = file_name
        .rsplit_once('.')
        .map(|(_, ext)| ext.to_ascii_lowercase())
        .ok_or_else(|| AppError::NotFound("file not found".to_string()))?;
    let content_type = content_type_for_ext(&ext)
        .ok_or_else(|| AppError::NotFound("file not found".to_string()))?
        .to_string();

    let path = paths.files.join(file_name);
    if !path.exists() {
        return Err(AppError::NotFound("file not found".to_string()));
    }
    let bytes = fs::read(path).map_err(|e| AppError::io("read upload file", e))?;
    Ok((bytes, content_type))
}

pub fn slug_from_rel_path(rel_path: &str) -> Option<String> {
    let file_name = Path::new(rel_path).file_name()?.to_str()?;
    let stem = file_name
        .rsplit_once('.')
        .map(|(s, _)| s)
        .unwrap_or(file_name);
    let (_, slug) = stem.split_once("__")?;
    if slug.is_empty() {
        None
    } else {
        Some(slug.to_string())
    }
}

pub fn resolve_slug_id(repo: &Path, slug: &str) -> AppResult<Option<String>> {
    if slug.is_empty()
        || !slug
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_'))
    {
        return Ok(None);
    }
    let path = slug_record_path(repo, slug);
    if !path.exists() {
        return Ok(None);
    }
    let data = fs::read(path).map_err(|e| AppError::io("read slug map", e))?;
    let record = serde_json::from_slice::<SlugRecord>(&data)
        .map_err(|e| AppError::internal(format!("parse slug map: {e}")))?;
    Ok(Some(record.id))
}

pub fn remove_files(paths: &[PathBuf]) {
    for path in paths {
        let _ = fs::remove_file(path);
    }
}

fn idempotency_record_path(idempotency_dir: &Path, key: &str) -> PathBuf {
    let mut hasher = Sha256::new();
    hasher.update(key.as_bytes());
    let file = format!("{}.json", hex::encode(hasher.finalize()));
    idempotency_dir.join(file)
}

pub fn idempotency_fingerprint(input: &CreatePasteInput) -> String {
    let mut hasher = Sha256::new();
    hasher.update(input.name.as_deref().unwrap_or_default().as_bytes());
    hasher.update(b"\0");
    hasher.update(input.msg.as_deref().unwrap_or_default().as_bytes());
    hasher.update(b"\0");
    hasher.update(input.tag.as_deref().unwrap_or_default().as_bytes());
    hasher.update(b"\0");
    hasher.update(input.content_type.as_deref().unwrap_or_default().as_bytes());
    hasher.update(b"\0");
    hasher.update(&input.bytes);
    hex::encode(hasher.finalize())
}

pub fn read_idempotency_record(
    idempotency_dir: &Path,
    key: &str,
) -> AppResult<Option<IdempotencyRecord>> {
    let path = idempotency_record_path(idempotency_dir, key);
    if !path.exists() {
        return Ok(None);
    }
    let data = fs::read(path).map_err(|e| AppError::io("read idempotency record", e))?;
    let record = serde_json::from_slice::<IdempotencyRecord>(&data)
        .map_err(|e| AppError::internal(format!("parse idempotency record: {e}")))?;
    Ok(Some(record))
}

pub fn write_idempotency_record(
    idempotency_dir: &Path,
    key: &str,
    record: &IdempotencyRecord,
) -> AppResult<()> {
    fs::create_dir_all(idempotency_dir).map_err(|e| AppError::io("create idempotency dir", e))?;
    let path = idempotency_record_path(idempotency_dir, key);
    let bytes = serde_json::to_vec_pretty(record)
        .map_err(|e| AppError::internal(format!("serialize idempotency record: {e}")))?;
    fs::write(path, bytes).map_err(|e| AppError::io("write idempotency record", e))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{PushMode, ServeCmd};
    use crate::types::AppPaths;

    const ONE_PX_PNG: &[u8] = &[
        0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48,
        0x44, 0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x04, 0x00, 0x00,
        0x00, 0xB5, 0x1C, 0x0C, 0x02, 0x00, 0x00, 0x00, 0x0B, 0x49, 0x44, 0x41, 0x54, 0x78,
        0xDA, 0x63, 0xFC, 0x5F, 0x0F, 0x00, 0x02, 0x7F, 0x01, 0xF5, 0x90, 0xA1, 0x8D, 0xA5,
        0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4E, 0x44, 0xAE, 0x42, 0x60, 0x82,
    ];

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
            api_keys_file: None,
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
        assert!(draft.slug_rel_path.starts_with("slugs/"));
        assert!(draft.abs_path.exists());
        assert!(draft.meta_path.exists());
        assert!(draft.slug_path.exists());
        assert_eq!(draft.slug, "n");
    }

    #[test]
    fn slug_from_rel_path_works() {
        let slug = slug_from_rel_path("pastes/2026/02/13/01ABC__note.md.md").expect("slug");
        assert_eq!(slug, "note.md");
    }

    #[test]
    fn persist_upload_dedupes_and_serves() {
        let td = tempfile::tempdir().expect("tempdir");
        let paths = AppPaths::from_base(td.path().to_path_buf());
        std::fs::create_dir_all(&paths.files).expect("mkdir files");
        std::fs::create_dir_all(&paths.files_meta).expect("mkdir files meta");

        let first = persist_upload(
            &paths,
            ONE_PX_PNG,
            Some("chart.png".to_string()),
            Some("demo".to_string()),
        )
        .expect("upload");
        let second = persist_upload(
            &paths,
            ONE_PX_PNG,
            Some("chart-duplicate.png".to_string()),
            Some("demo".to_string()),
        )
        .expect("upload");

        assert_eq!(first.id, second.id);
        assert_eq!(first.url, second.url);
        assert_eq!(first.content_type, "image/png");
        assert_eq!(first.width, 1);
        assert_eq!(first.height, 1);

        let file_name = first.url.trim_start_matches("/files/");
        let (bytes, mime) = read_uploaded_file(&paths, file_name).expect("read file");
        assert_eq!(mime, "image/png");
        assert_eq!(bytes, ONE_PX_PNG);
    }

    #[test]
    fn slug_collision_gets_numeric_suffix() {
        let td = tempfile::tempdir().expect("tempdir");
        let repo = td.path().join("repo");
        std::fs::create_dir_all(&repo).expect("mkdir");
        let cfg = ServeCmd {
            dir: td.path().to_path_buf(),
            bind: "127.0.0.1:0".parse().expect("bind"),
            token: None,
            api_keys_file: None,
            max_bytes: 1024,
            push: PushMode::Off,
            remote: "origin".to_string(),
            allow_cidr: vec![],
            git_author_name: "LAN Paste".to_string(),
            git_author_email: "paste@lan".to_string(),
        };

        let first = build_paste_draft(
            &repo,
            &cfg,
            CreatePasteInput {
                name: Some("brief-2026-03-03.md".to_string()),
                msg: None,
                tag: None,
                content_type: Some("text/markdown".to_string()),
                bytes: b"one".to_vec(),
                client_ip: None,
                user_agent: None,
            },
        )
        .expect("first");
        let second = build_paste_draft(
            &repo,
            &cfg,
            CreatePasteInput {
                name: Some("brief-2026-03-03.md".to_string()),
                msg: None,
                tag: None,
                content_type: Some("text/markdown".to_string()),
                bytes: b"two".to_vec(),
                client_ip: None,
                user_agent: None,
            },
        )
        .expect("second");

        assert_eq!(first.slug, "brief-2026-03-03");
        assert_eq!(second.slug, "brief-2026-03-03-2");
        let mapped =
            resolve_slug_id(&repo, "brief-2026-03-03-2").expect("resolve").expect("id");
        assert_eq!(mapped, second.id);
    }
}
