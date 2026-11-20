//! File I/O commands. Every write is atomic: the data goes to a temp file in the same
//! directory, is fsynced, and is then renamed over the target. A crash or power loss
//! leaves either the old file or the new one, never a truncated mix.

use serde::Serialize;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use crate::app::AppContext;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReadResult {
    pub text: String,
    pub mtime_ms: f64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WriteResult {
    pub mtime_ms: f64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StatResult {
    pub mtime_ms: f64,
    pub size: u64,
    pub is_dir: bool,
}

fn mtime_ms(meta: &fs::Metadata) -> f64 {
    meta.modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_secs_f64() * 1000.0)
        .unwrap_or(0.0)
}

pub fn describe(err: &io::Error) -> String {
    match err.kind() {
        io::ErrorKind::NotFound => "The file no longer exists.".into(),
        io::ErrorKind::PermissionDenied => "Permission denied.".into(),
        _ => err.to_string(),
    }
}

pub fn read_text_file(path: String) -> Result<ReadResult, String> {
    let path = Path::new(&path);
    let bytes = fs::read(path).map_err(|e| describe(&e))?;
    let text = String::from_utf8(bytes).map_err(|_| "This file isn't UTF-8 text.".to_string())?;
    let meta = fs::metadata(path).map_err(|e| describe(&e))?;
    Ok(ReadResult { text, mtime_ms: mtime_ms(&meta) })
}

pub fn write_text_file(path: String, text: String) -> Result<WriteResult, String> {
    let target = resolve_symlink(Path::new(&path));
    write_atomic(&target, text.as_bytes()).map_err(|e| describe(&e))?;
    let meta = fs::metadata(&target).map_err(|e| describe(&e))?;
    Ok(WriteResult { mtime_ms: mtime_ms(&meta) })
}

pub fn stat_path(path: String) -> Option<StatResult> {
    fs::metadata(&path).ok().map(|m| StatResult {
        mtime_ms: mtime_ms(&m),
        size: m.len(),
        is_dir: m.is_dir(),
    })
}

/// Write through symlinks instead of replacing the link with a regular file.
fn resolve_symlink(path: &Path) -> PathBuf {
    match fs::symlink_metadata(path) {
        Ok(m) if m.file_type().is_symlink() => fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf()),
        _ => path.to_path_buf(),
    }
}

pub fn write_atomic(path: &Path, contents: &[u8]) -> io::Result<()> {
    let dir = match path.parent() {
        Some(p) if !p.as_os_str().is_empty() => p,
        _ => Path::new("."),
    };
    let name = path
        .file_name()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "path has no file name"))?;
    let nonce = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
    let tmp = dir.join(format!(".{}.{}-{}.calmmd-tmp", name.to_string_lossy(), std::process::id(), nonce));

    let result = (|| -> io::Result<()> {
        let mut f = fs::OpenOptions::new().write(true).create_new(true).open(&tmp)?;
        f.write_all(contents)?;
        f.sync_all()?;
        drop(f);
        #[cfg(unix)]
        if let Ok(meta) = fs::metadata(path) {
            fs::set_permissions(&tmp, meta.permissions())?;
        }
        replace(&tmp, path)?;
        #[cfg(unix)]
        if let Ok(d) = fs::File::open(dir) {
            let _ = d.sync_all(); // persist the rename itself
        }
        Ok(())
    })();

    if result.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    result
}

/// `rename` replaces the target atomically on all platforms. On Windows it can briefly fail
/// while another process (an indexer or antivirus) has the file open, so retry a few times.
fn replace(from: &Path, to: &Path) -> io::Result<()> {
    let mut attempt = 0;
    loop {
        match fs::rename(from, to) {
            Ok(()) => return Ok(()),
            Err(e) if cfg!(windows) && attempt < 5 && e.kind() == io::ErrorKind::PermissionDenied => {
                attempt += 1;
                std::thread::sleep(std::time::Duration::from_millis(40 * attempt));
            }
            Err(e) => return Err(e),
        }
    }
}

// ---------- Images and other binary assets ----------

/// Writes binary data (a pasted image) atomically, creating missing folders. The bytes come as
/// the raw IPC body, with the target path URL-encoded in the `path` header: JSON would turn a
/// few megabytes of image into a slow array of numbers.
pub fn write_binary(path: &Path, bytes: &[u8]) -> io::Result<()> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    write_atomic(path, bytes)
}

/// Copies a file, creating missing folders. Never overwrites: the caller picks a free name.
pub fn copy_file(from: String, to: String) -> Result<(), String> {
    copy_new(Path::new(&from), Path::new(&to)).map_err(|e| describe(&e))
}

pub fn copy_new(from: &Path, to: &Path) -> io::Result<()> {
    if to.exists() {
        return Err(io::Error::new(io::ErrorKind::AlreadyExists, "A file with that name already exists."));
    }
    if let Some(dir) = to.parent() {
        fs::create_dir_all(dir)?;
    }
    fs::copy(from, to).map(|_| ())
}

/// Images pasted into untitled documents wait here until the document is saved.
pub fn pending_assets_dir(app: &AppContext) -> Result<String, String> {
    let dir = app.data_dir().join("pending-assets");
    fs::create_dir_all(&dir).map_err(|e| describe(&e))?;
    Ok(dir.to_string_lossy().into_owned())
}

pub fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let Ok(b) = u8::from_str_radix(&s[i + 1..i + 3], 16) {
                out.push(b);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_percent_encoded_paths() {
        assert_eq!(percent_decode("C%3A%5CUsers%5Cme%5Cmy%20pic.png"), r"C:\Users\me\my pic.png");
        assert_eq!(percent_decode("caf%C3%A9.png"), "café.png");
        assert_eq!(percent_decode("100%"), "100%");
    }

    #[test]
    fn binary_writes_create_folders_and_copies_never_overwrite() {
        let dir = tempfile::tempdir().unwrap();
        let img = dir.path().join("a").join("assets").join("x.png");
        write_binary(&img, &[1, 2, 3]).unwrap();
        assert_eq!(fs::read(&img).unwrap(), vec![1, 2, 3]);
        let copy = dir.path().join("b").join("assets").join("x.png");
        copy_new(&img, &copy).unwrap();
        assert_eq!(fs::read(&copy).unwrap(), vec![1, 2, 3]);
        assert_eq!(copy_new(&img, &copy).unwrap_err().kind(), io::ErrorKind::AlreadyExists);
    }

    fn leftovers(dir: &Path) -> Vec<String> {
        fs::read_dir(dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|n| n.ends_with(".calmmd-tmp"))
            .collect()
    }

    #[test]
    fn creates_a_new_file() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("new.md");
        write_atomic(&p, "héllo ✍️".as_bytes()).unwrap();
        assert_eq!(fs::read_to_string(&p).unwrap(), "héllo ✍️");
        assert!(leftovers(dir.path()).is_empty());
    }

    #[test]
    fn replaces_an_existing_file() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("a.md");
        fs::write(&p, "old content that is longer").unwrap();
        write_atomic(&p, b"new").unwrap();
        assert_eq!(fs::read_to_string(&p).unwrap(), "new");
        assert!(leftovers(dir.path()).is_empty());
    }

    #[test]
    fn fails_cleanly_when_the_directory_is_missing() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("missing").join("a.md");
        assert!(write_atomic(&p, b"x").is_err());
    }

    #[test]
    fn many_sequential_writes_leave_no_temp_files() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("a.md");
        for i in 0..50 {
            write_atomic(&p, format!("v{i}").as_bytes()).unwrap();
        }
        assert_eq!(fs::read_to_string(&p).unwrap(), "v49");
        assert!(leftovers(dir.path()).is_empty());
    }

    #[cfg(unix)]
    #[test]
    fn writes_through_symlinks() {
        let dir = tempfile::tempdir().unwrap();
        let real = dir.path().join("real.md");
        let link = dir.path().join("link.md");
        fs::write(&real, "a").unwrap();
        std::os::unix::fs::symlink(&real, &link).unwrap();
        write_atomic(&resolve_symlink(&link), b"b").unwrap();
        assert!(fs::symlink_metadata(&link).unwrap().file_type().is_symlink());
        assert_eq!(fs::read_to_string(&real).unwrap(), "b");
    }
}
