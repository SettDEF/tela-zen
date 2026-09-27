//! The extension host's loading layer.
//!
//! Unpacks XPI and CRX bundles, normalises their manifests, and parses
//! userscripts — everything that can be done without a browser attached, so it
//! stays testable in isolation.

pub mod manifest;
pub mod matcher;
pub mod userscript;

pub use manifest::{CompatIssue, Manifest, Severity};
pub use matcher::{MatchSet, Pattern};
pub use userscript::{RunAt, UserScript, GM_SHIM};

use std::io::{Cursor, Read};
use std::path::{Path, PathBuf};

#[derive(Debug)]
pub enum LoadError {
    Io(std::io::Error),
    Archive(String),
    NoManifest,
    BadManifest,
}

impl std::fmt::Display for LoadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LoadError::Io(error) => write!(f, "{error}"),
            LoadError::Archive(detail) => write!(f, "not a readable bundle: {detail}"),
            LoadError::NoManifest => write!(f, "bundle has no manifest.json"),
            LoadError::BadManifest => write!(f, "manifest.json is malformed"),
        }
    }
}

impl std::error::Error for LoadError {}

impl From<std::io::Error> for LoadError {
    fn from(error: std::io::Error) -> Self {
        LoadError::Io(error)
    }
}

/// CRX files are a zip with a signature header bolted on the front. Strip it so
/// the same zip reader handles both formats.
///
/// CRX2: magic, version, public-key length, signature length, then the zip.
/// CRX3: magic, version, header length, then the zip.
fn strip_crx_header(bytes: &[u8]) -> &[u8] {
    if bytes.len() < 16 || &bytes[0..4] != b"Cr24" {
        return bytes;
    }

    let word = |offset: usize| {
        u32::from_le_bytes([
            bytes[offset],
            bytes[offset + 1],
            bytes[offset + 2],
            bytes[offset + 3],
        ]) as usize
    };

    let version = word(4);
    let offset = match version {
        2 => 16 + word(8) + word(12),
        3 => 12 + word(8),
        _ => return bytes,
    };

    bytes.get(offset..).unwrap_or(bytes)
}

/// An unpacked extension on disk.
#[derive(Debug)]
pub struct Bundle {
    pub manifest: Manifest,
    pub root: PathBuf,
}

/// Unpack an XPI or CRX into `destination/<id>` and parse its manifest.
pub fn install_bundle(archive: &Path, destination: &Path) -> Result<Bundle, LoadError> {
    let bytes = std::fs::read(archive)?;
    let payload = strip_crx_header(&bytes);

    let mut zip = zip::ZipArchive::new(Cursor::new(payload))
        .map_err(|error| LoadError::Archive(error.to_string()))?;

    let manifest_source = {
        let mut file = zip
            .by_name("manifest.json")
            .map_err(|_| LoadError::NoManifest)?;
        let mut source = String::new();
        file.read_to_string(&mut source)
            .map_err(|_| LoadError::BadManifest)?;
        source
    };

    let mut manifest = Manifest::parse(&manifest_source).ok_or(LoadError::BadManifest)?;
    let root = destination.join(bundle_id(&manifest));
    std::fs::create_dir_all(&root)?;

    for index in 0..zip.len() {
        let mut entry = zip
            .by_index(index)
            .map_err(|error| LoadError::Archive(error.to_string()))?;

        // `enclosed_name` rejects absolute paths and `..` traversal — a bundle
        // must never be able to write outside its own directory.
        let Some(relative) = entry.enclosed_name() else {
            continue;
        };
        let target = root.join(relative);

        if entry.is_dir() {
            std::fs::create_dir_all(&target)?;
            continue;
        }
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let mut out = std::fs::File::create(&target)?;
        std::io::copy(&mut entry, &mut out)?;
    }

    // Locale files only exist once unpacked, so this runs after extraction —
    // and the bundle id deliberately stays based on the raw manifest name so it
    // does not change with the user's language.
    manifest.resolve_messages(&root);

    Ok(Bundle { manifest, root })
}

/// A filesystem-safe identity derived from the manifest.
pub fn bundle_id(manifest: &Manifest) -> String {
    let slug: String = manifest
        .name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect();
    let slug = slug.trim_matches('-').to_owned();
    if slug.is_empty() {
        "extension".to_owned()
    } else {
        slug
    }
}

/// Find `.user.js` files in a directory, newest first. Used to import whatever
/// the user has already downloaded.
pub fn discover_userscripts(directory: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(directory) else {
        return Vec::new();
    };

    let mut found: Vec<(std::time::SystemTime, PathBuf)> = entries
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let path = entry.path();
            let name = path.file_name()?.to_str()?.to_ascii_lowercase();
            if !name.ends_with(".user.js") {
                return None;
            }
            let modified = entry.metadata().ok()?.modified().ok()?;
            Some((modified, path))
        })
        .collect();

    found.sort_by(|a, b| b.0.cmp(&a.0));
    found.into_iter().map(|(_, path)| path).collect()
}

/// Copy userscripts into the config directory and parse them.
///
/// Files that fail to parse are skipped rather than aborting the import — one
/// malformed download should not block the rest.
pub fn import_userscripts(from: &Path, into: &Path) -> Vec<UserScript> {
    let mut imported = Vec::new();
    if std::fs::create_dir_all(into).is_err() {
        return imported;
    }

    for path in discover_userscripts(from) {
        let Ok(source) = std::fs::read_to_string(&path) else {
            continue;
        };
        let Some(script) = UserScript::parse(&source) else {
            continue;
        };
        if let Some(name) = path.file_name() {
            let _ = std::fs::write(into.join(name), &source);
        }
        imported.push(script);
    }

    imported
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn leaves_a_plain_zip_untouched() {
        let bytes = b"PK\x03\x04rest-of-a-zip-file-here";
        assert_eq!(strip_crx_header(bytes), bytes);
    }

    #[test]
    fn strips_a_crx3_header() {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"Cr24");
        bytes.extend_from_slice(&3u32.to_le_bytes()); // version
        bytes.extend_from_slice(&4u32.to_le_bytes()); // header length
        bytes.extend_from_slice(&[0xAA; 4]); // header
        bytes.extend_from_slice(b"PK\x03\x04zip");
        assert_eq!(strip_crx_header(&bytes), b"PK\x03\x04zip");
    }

    #[test]
    fn strips_a_crx2_header() {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"Cr24");
        bytes.extend_from_slice(&2u32.to_le_bytes()); // version
        bytes.extend_from_slice(&2u32.to_le_bytes()); // public key length
        bytes.extend_from_slice(&3u32.to_le_bytes()); // signature length
        bytes.extend_from_slice(&[0xBB; 5]); // key + signature
        bytes.extend_from_slice(b"PK\x03\x04zip");
        assert_eq!(strip_crx_header(&bytes), b"PK\x03\x04zip");
    }

    #[test]
    fn truncated_crx_does_not_panic() {
        let bytes = b"Cr24\x03\x00\x00\x00\xff\xff\xff\xff-short";
        // An absurd header length must fall back rather than slice out of range.
        let _ = strip_crx_header(bytes);
    }

    #[test]
    fn derives_a_filesystem_safe_id() {
        let manifest = Manifest::parse(
            r#"{"manifest_version":3,"name":"uBlock Origin!","version":"1"}"#,
        )
        .unwrap();
        assert_eq!(bundle_id(&manifest), "ublock-origin");
    }
}
