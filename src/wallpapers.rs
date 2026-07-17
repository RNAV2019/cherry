use std::path::{Path, PathBuf};

const EXTENSIONS: &[&str] = &["png", "jpg", "jpeg"];

#[derive(Debug, Clone)]
pub struct Wallpaper {
    pub path: PathBuf,
    pub name: String,
}

pub fn default_dir() -> PathBuf {
    let home = std::env::var("HOME").expect("HOME not set");
    Path::new(&home).join("Pictures/backgrounds")
}

/// Scans `dir` (non-recursively, following symlinks) for png/jpg/jpeg files,
/// sorted by filename.
pub fn scan_wallpapers(dir: &Path) -> Vec<Wallpaper> {
    let mut out = Vec::new();
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            let ext = match path.extension().and_then(|e| e.to_str()) {
                Some(e) => e.to_lowercase(),
                None => continue,
            };
            if !EXTENSIONS.contains(&ext.as_str()) {
                continue;
            }
            let name = match path.file_name() {
                Some(n) => n.to_string_lossy().into_owned(),
                None => continue,
            };
            out.push(Wallpaper { path, name });
        }
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

pub fn fuzzy_match(query: &str, text: &str) -> bool {
    if query.is_empty() {
        return true;
    }
    let query = query.to_lowercase();
    let text = text.to_lowercase();
    let mut query_chars = query.chars();
    let mut current = match query_chars.next() {
        None => return true,
        Some(c) => c,
    };
    for ch in text.chars() {
        if ch == current {
            match query_chars.next() {
                None => return true,
                Some(c) => current = c,
            }
        }
    }
    false
}

/// Indices into `wallpapers` matching `query`, preserving the sorted order.
pub fn filtered(wallpapers: &[Wallpaper], query: &str) -> Vec<usize> {
    wallpapers
        .iter()
        .enumerate()
        .filter(|(_, w)| fuzzy_match(query, &w.name))
        .map(|(i, _)| i)
        .collect()
}

/// Diffs `old` against `fresh` by path, without assuming either is sorted.
/// Returns `(added, removed)`: wallpapers present in `fresh` but not `old`,
/// and the paths of wallpapers present in `old` but not `fresh`. Used to
/// rescan a directory without re-decoding images that haven't changed.
pub fn diff_scan(old: &[Wallpaper], fresh: &[Wallpaper]) -> (Vec<Wallpaper>, Vec<PathBuf>) {
    use std::collections::HashSet;

    let old_paths: HashSet<&PathBuf> = old.iter().map(|w| &w.path).collect();
    let fresh_paths: HashSet<&PathBuf> = fresh.iter().map(|w| &w.path).collect();

    let added = fresh
        .iter()
        .filter(|w| !old_paths.contains(&w.path))
        .cloned()
        .collect();
    let removed = old
        .iter()
        .filter(|w| !fresh_paths.contains(&w.path))
        .map(|w| w.path.clone())
        .collect();

    (added, removed)
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn scan_finds_only_images_sorted() {
        let dir = tempdir().unwrap();
        for name in ["b.png", "a.jpg", "c.jpeg", "notes.txt", "d.webp"] {
            fs::write(dir.path().join(name), b"x").unwrap();
        }
        fs::create_dir(dir.path().join("sub.png")).unwrap();
        let walls = scan_wallpapers(dir.path());
        let names: Vec<&str> = walls.iter().map(|w| w.name.as_str()).collect();
        assert_eq!(names, vec!["a.jpg", "b.png", "c.jpeg"]);
    }

    #[test]
    fn scan_follows_symlinks() {
        let dir = tempdir().unwrap();
        let target = dir.path().join("real.png");
        fs::write(&target, b"x").unwrap();
        let link_dir = tempdir().unwrap();
        std::os::unix::fs::symlink(&target, link_dir.path().join("link.png")).unwrap();
        let walls = scan_wallpapers(link_dir.path());
        assert_eq!(walls.len(), 1);
        assert_eq!(walls[0].name, "link.png");
    }

    #[test]
    fn scan_missing_dir_is_empty() {
        assert!(scan_wallpapers(Path::new("/nonexistent/backgrounds")).is_empty());
    }

    #[test]
    fn fuzzy_matches_subsequence_case_insensitive() {
        assert!(fuzzy_match("", "anything"));
        assert!(fuzzy_match("nbhd", "nbhd_v2.jpg"));
        assert!(fuzzy_match("NV2", "nbhd_v2.jpg"));
        assert!(!fuzzy_match("xyz", "nbhd_v2.jpg"));
    }

    #[test]
    fn filtered_returns_matching_indices() {
        let walls = vec![
            Wallpaper { path: "a".into(), name: "nasa.png".into() },
            Wallpaper { path: "b".into(), name: "nbhd_v2.jpg".into() },
            Wallpaper { path: "c".into(), name: "symbols.jpg".into() },
        ];
        assert_eq!(filtered(&walls, ""), vec![0, 1, 2]);
        assert_eq!(filtered(&walls, "nb"), vec![1]);
        assert_eq!(filtered(&walls, "s"), vec![0, 2]);
        assert_eq!(filtered(&walls, "zz"), Vec::<usize>::new());
    }

    #[test]
    fn diff_scan_detects_added_and_removed() {
        let old = vec![
            Wallpaper { path: PathBuf::from("a.png"), name: "a.png".into() },
            Wallpaper { path: PathBuf::from("b.png"), name: "b.png".into() },
        ];
        let fresh = vec![
            Wallpaper { path: PathBuf::from("b.png"), name: "b.png".into() },
            Wallpaper { path: PathBuf::from("c.png"), name: "c.png".into() },
        ];
        let (added, removed) = diff_scan(&old, &fresh);
        assert_eq!(added.len(), 1);
        assert_eq!(added[0].path, PathBuf::from("c.png"));
        assert_eq!(removed, vec![PathBuf::from("a.png")]);
    }

    #[test]
    fn diff_scan_no_changes_is_empty() {
        let list = vec![Wallpaper { path: PathBuf::from("a.png"), name: "a.png".into() }];
        let (added, removed) = diff_scan(&list, &list);
        assert!(added.is_empty());
        assert!(removed.is_empty());
    }
}
