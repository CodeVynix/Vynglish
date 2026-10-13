//! Modules: the `.vyn` files a program can `use`.
//!
//! A module is just a Vynglish file. Its commands are added to the running
//! session's pattern table when it is loaded, which is why `use` has to happen
//! before anything is validated. Nothing is written back to disk, and using the
//! same module twice does nothing the second time.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

/// Where Vynglish looks for a module file, in order.
pub fn search_paths() -> Vec<PathBuf> {
    let mut roots: Vec<PathBuf> = Vec::new();
    if let Ok(current) = std::env::current_dir() {
        roots.push(current.clone());
        roots.push(current.join("stdlib"));
    }
    roots.push(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("src")
            .join("modules")
            .join("stdlib"),
    );
    roots
}

/// The file that holds a module, if there is one.
pub fn find(name: &str) -> Option<PathBuf> {
    for root in search_paths() {
        let candidate = root.join(format!("{name}.vyn"));
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

/// The module names that ship with Vynglish.
pub fn bundled() -> Vec<String> {
    let mut names: Vec<String> = Vec::new();
    for root in search_paths() {
        if !root.is_dir() {
            continue;
        }
        let Ok(entries) = std::fs::read_dir(&root) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|suffix| suffix.to_str()) != Some("vyn") {
                continue;
            }
            let Some(stem) = path.file_stem().and_then(|stem| stem.to_str()) else {
                continue;
            };
            if !names.contains(&stem.to_string()) {
                names.push(stem.to_string());
            }
        }
    }
    names.sort();
    names.dedup();
    names
}

/// Names used by tests to check that loading twice is harmless.
#[allow(dead_code)]
pub fn loaded_set(names: &[String]) -> HashSet<String> {
    names.iter().cloned().collect()
}
