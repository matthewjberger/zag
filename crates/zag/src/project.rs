//! Finds the Zig files a program is made of, starting from its root, and asks
//! the compiler about each one.
//!
//! A Zig file is a struct and `@import` is how one names another, so following
//! imports from the root is the whole of module discovery. The compiler's own
//! modules, `std` and `builtin`, are not ported and are not followed. An import
//! that names neither a file nor a module the project declares is recorded as
//! unresolved rather than guessed at, because a program read with a hole in it
//! is a different program from the one on disk.
//!
//! Everything here is ordered. Modules come out with the root first and the
//! rest sorted by the path they were resolved to, so the same directory gives
//! the same fact tables every time.

use std::path::{Path, PathBuf};
use zag_frontend::project::SourceModule;

/// How deep the crawl will follow imports before giving up. A cycle terminates
/// on the visited set rather than here, so this only bounds a pathological
/// chain of files.
const MAXIMUM_MODULES: usize = 4096;

fn is_compiler_module(text: &str) -> bool {
    matches!(text, "std" | "builtin" | "root")
}

/// The name a module gets in the port, which is its path below the root file's
/// directory with the separators flattened. A file in a subdirectory therefore
/// keeps a name of its own rather than colliding with one beside the root.
fn module_name(root_directory: &Path, path: &Path) -> String {
    let relative = path.strip_prefix(root_directory).unwrap_or(path);
    let mut name = String::new();
    for part in relative.components() {
        let part = part.as_os_str().to_string_lossy();
        if !name.is_empty() {
            name.push('_');
        }
        name.push_str(part.trim_end_matches(".zig"));
    }
    name.chars()
        .map(|letter| {
            if letter.is_ascii_alphanumeric() {
                letter
            } else {
                '_'
            }
        })
        .collect()
}

/// Where an import points, or nothing when it points outside what the crawl
/// can open. A path import resolves against the directory of the file that
/// wrote it, which is what Zig does.
fn resolve_import(from: &Path, text: &str) -> Option<PathBuf> {
    if !text.ends_with(".zig") {
        return None;
    }
    let directory = from.parent()?;
    let joined = directory.join(text);
    // The path is normalised so that two spellings of one file do not become
    // two modules, which would duplicate every declaration in it.
    let mut normalised = PathBuf::new();
    for part in joined.components() {
        match part {
            std::path::Component::ParentDir => {
                normalised.pop();
            }
            std::path::Component::CurDir => {}
            other => normalised.push(other),
        }
    }
    normalised.is_file().then_some(normalised)
}

fn imports_of(text: &str) -> Vec<(String, String)> {
    text.lines()
        .filter_map(|line| {
            let rest = line.strip_prefix("import ")?;
            let (alias, path) = rest.split_once(" path=")?;
            Some((alias.trim().to_string(), path.trim().to_string()))
        })
        .collect()
}

struct Crawled {
    path: PathBuf,
    extraction: String,
    reflection: String,
    imports: Vec<(String, String)>,
}

/// Reads every file the root reaches. A file already read is not read again,
/// which is what makes a cycle terminate, and a file that cannot be read is an
/// error the caller sees rather than a panic.
fn crawl(root: &Path) -> Result<Vec<Crawled>, String> {
    let root = crate::normalise(root);
    let mut pending = vec![root];
    let mut found: Vec<Crawled> = Vec::new();
    while let Some(path) = pending.pop() {
        if found.len() >= MAXIMUM_MODULES {
            return Err(format!(
                "the crawl reached {MAXIMUM_MODULES} files, which is more than a program should need"
            ));
        }
        if found.iter().any(|entry| entry.path == path) {
            continue;
        }
        let (extraction, reflection) = crate::ask_zig(&path)?;
        let imports = imports_of(&extraction);
        for (_, text) in &imports {
            if is_compiler_module(text) {
                continue;
            }
            if let Some(resolved) = resolve_import(&path, text)
                && !found.iter().any(|entry| entry.path == resolved)
            {
                pending.push(resolved);
            }
        }
        found.push(Crawled {
            path,
            extraction,
            reflection,
            imports,
        });
    }
    Ok(found)
}

/// Every module of the program, the root first and the rest sorted by path.
/// The root keeps the top level because in Zig the root file is the program's
/// own namespace, so a program that is one file ports exactly as it did before
/// there were modules at all.
pub fn read_project(root: &Path) -> Result<Vec<SourceModule>, String> {
    let crawled = crawl(root)?;
    let root_path = crate::normalise(root);
    let root_directory = root_path
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_default();

    let mut ordered: Vec<&Crawled> = crawled.iter().collect();
    ordered.sort_by(|left, right| {
        let left_is_root = left.path == root_path;
        let right_is_root = right.path == root_path;
        right_is_root
            .cmp(&left_is_root)
            .then_with(|| left.path.cmp(&right.path))
    });

    let named: Vec<(PathBuf, String)> = ordered
        .iter()
        .map(|entry| {
            let name = if entry.path == root_path {
                String::new()
            } else {
                module_name(&root_directory, &entry.path)
            };
            (entry.path.clone(), name)
        })
        .collect();

    Ok(ordered
        .iter()
        .zip(named.iter())
        .map(|(entry, (_, name))| {
            let mut imports = Vec::new();
            let mut unresolved = Vec::new();
            for (alias, text) in &entry.imports {
                if is_compiler_module(text) {
                    continue;
                }
                match resolve_import(&entry.path, text)
                    .and_then(|resolved| named.iter().find(|(path, _)| *path == resolved))
                {
                    Some((_, target)) => imports.push((alias.clone(), target.clone())),
                    None => unresolved.push(text.clone()),
                }
            }
            SourceModule {
                name: name.clone(),
                // Relative to the root file, so the fact tables say the same
                // thing wherever the directory happens to sit.
                path: entry
                    .path
                    .strip_prefix(&root_directory)
                    .unwrap_or(&entry.path)
                    .display()
                    .to_string()
                    .replace('\\', "/"),
                program: zag_frontend::program::parse(&entry.extraction, &entry.reflection),
                imports,
                unresolved,
            }
        })
        .collect())
}
