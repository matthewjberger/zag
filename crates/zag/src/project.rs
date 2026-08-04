//! Finds the Zig files a project is made of and asks the compiler about each
//! one.
//!
//! There are two ways in. Pointed at a `.zig` file, the crawl treats it as the
//! root and follows `@import` from there, which is the whole of module
//! discovery, because a Zig file is a struct and `@import` is how one names
//! another. Pointed at a directory or at a `build.zig`, it reads the build
//! script first and starts from every file the script says an artifact is
//! rooted at, so a project that builds several programs is read as several
//! programs over one set of modules rather than as whichever one it was
//! pointed at.
//!
//! The compiler's own modules, `std` and `builtin`, are not ported and are not
//! followed. An import that names neither a file nor a module the project
//! declares is recorded as unresolved rather than guessed at, because a program
//! read with a hole in it is a different program from the one on disk.
//!
//! Everything here is ordered. Modules come out sorted by the path they were
//! resolved to, so the same directory gives the same fact tables every time.

use std::path::{Path, PathBuf};
use zag_facts::tables::ArtifactKind;
use zag_frontend::project::{Artifact, Project, SourceModule};

/// How deep the crawl will follow imports before giving up. A cycle terminates
/// on the visited set rather than here, so this only bounds a pathological
/// chain of files.
const MAXIMUM_MODULES: usize = 4096;

/// What a call to `b.addExecutable` and its neighbours asks for. Matched on the
/// method name, so the builder can be called whatever the script called it.
const BUILDERS: [(&str, ArtifactKind); 5] = [
    ("addExecutable", ArtifactKind::Executable),
    ("addLibrary", ArtifactKind::Library),
    ("addStaticLibrary", ArtifactKind::Library),
    ("addSharedLibrary", ArtifactKind::Library),
    ("addTest", ArtifactKind::Test),
];

fn is_compiler_module(text: &str) -> bool {
    matches!(text, "std" | "builtin" | "root")
}

/// The name a module gets in the port, which is its path below the directory
/// every file of the project sits in, with the separators flattened. A file in
/// a subdirectory therefore keeps a name of its own rather than colliding with
/// one beside it.
fn module_name(base: &Path, path: &Path) -> String {
    let relative = path.strip_prefix(base).unwrap_or(path);
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

/// The run of text between the needle and the next quote. Every shape read out
/// of a build script is a quoted string just past something fixed, so one
/// helper covers all of them.
fn quoted_after<'a>(text: &'a str, needle: &str) -> Option<&'a str> {
    let start = text.find(needle)?.checked_add(needle.len())?;
    let rest = text.get(start..)?;
    let end = rest.find('"')?;
    rest.get(..end)
}

fn literal_field<'a>(text: &'a str, field: &str) -> Option<&'a str> {
    quoted_after(text, &format!(".{field} = \""))
}

/// Where the artifact's root source file is written, as the script spelled it.
///
/// Zig has changed how this is said more than once. Current code wraps it in
/// `b.path(...)`, older code assigned the string to `.root_source_file`
/// directly, and older still went through a `.{ .path = ... }`. All three are
/// read, and a script that computes the path rather than writing it matches
/// none of them and gives an artifact with no root, which is a hole the report
/// says out loud rather than a guess.
fn root_source(text: &str) -> Option<&str> {
    quoted_after(text, ".path(\"")
        .or_else(|| literal_field(text, "root_source_file"))
        .or_else(|| literal_field(text, "path"))
}

/// The first argument of every call the parser reported, paired with what was
/// called. An artifact is declared by one call taking one struct literal, so
/// the name and the root file are both in that one argument's source text.
fn first_arguments(extraction: &str) -> impl Iterator<Item = (&str, &str)> {
    extraction.lines().filter_map(|line| {
        let rest = line.strip_prefix("argument ")?;
        let (head, text) = rest.split_once(" text=")?;
        let mut parts = head.split('|');
        parts.next()?;
        let callee = parts.next()?;
        let index = parts.next()?;
        (index == "0").then_some((callee, text))
    })
}

struct Declared {
    name: String,
    root: Option<PathBuf>,
    kind: ArtifactKind,
}

/// What the build script asks the compiler to produce, read by spelling the
/// same way the rest of the frontend reads Zig. A script that builds its
/// artifact list in a loop says nothing this can see, and gets no artifacts
/// rather than wrong ones.
fn artifacts_in(extraction: &str, directory: &Path) -> Vec<Declared> {
    let mut found: Vec<Declared> = Vec::new();
    for (callee, text) in first_arguments(extraction) {
        let method = callee.rsplit('.').next().unwrap_or(callee);
        let Some((_, kind)) = BUILDERS.iter().find(|(name, _)| *name == method) else {
            continue;
        };
        let source = root_source(text);
        let root = source.map(|source| directory.join(source)).map(|path| {
            let mut normalised = PathBuf::new();
            for part in path.components() {
                match part {
                    std::path::Component::ParentDir => {
                        normalised.pop();
                    }
                    std::path::Component::CurDir => {}
                    other => normalised.push(other),
                }
            }
            normalised
        });
        // A test artifact usually goes unnamed, so it falls back to the file it
        // is rooted at, which is the only other thing that identifies it.
        let name = literal_field(text, "name")
            .map(str::to_string)
            .or_else(|| {
                source.map(|source| {
                    source
                        .rsplit(['/', '\\'])
                        .next()
                        .unwrap_or(source)
                        .trim_end_matches(".zig")
                        .to_string()
                })
            })
            .unwrap_or_else(|| format!("artifact{}", found.len()));
        found.push(Declared {
            name,
            root: root.filter(|path| path.is_file()),
            kind: *kind,
        });
    }
    found
}

struct Crawled {
    path: PathBuf,
    extraction: String,
    reflection: String,
    imports: Vec<(String, String)>,
}

/// Reads every file the roots reach. A file already read is not read again,
/// which is what makes a cycle terminate and what lets two artifacts share a
/// module, and a file that cannot be read is an error the caller sees rather
/// than a panic.
fn crawl(roots: &[PathBuf]) -> Result<Vec<Crawled>, String> {
    let mut pending: Vec<PathBuf> = roots.to_vec();
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

/// The deepest directory holding every file the crawl read, which is what
/// module names are relative to. A project whose sources all sit under `src`
/// gets names like `store` rather than `src_store`.
fn common_directory(paths: &[PathBuf]) -> PathBuf {
    let mut shared: Option<PathBuf> = None;
    for path in paths {
        let directory = path.parent().unwrap_or(Path::new("")).to_path_buf();
        shared = Some(match shared {
            None => directory,
            Some(current) => {
                let mut prefix = PathBuf::new();
                for (left, right) in current.components().zip(directory.components()) {
                    if left != right {
                        break;
                    }
                    prefix.push(left);
                }
                prefix
            }
        });
    }
    shared.unwrap_or_default()
}

/// Turns what the crawl read into modules, given the name each file is to take.
/// Imports are matched against the same names, so a file the crawl reached
/// becomes a module path and one it did not stays an unresolved import.
fn assemble(crawled: &[&Crawled], named: &[(PathBuf, String)], base: &Path) -> Vec<SourceModule> {
    crawled
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
                // Relative to the directory the project sits in, so the fact
                // tables say the same thing wherever that directory happens to
                // be.
                path: entry
                    .path
                    .strip_prefix(base)
                    .unwrap_or(&entry.path)
                    .display()
                    .to_string()
                    .replace('\\', "/"),
                program: zag_frontend::program::parse(&entry.extraction, &entry.reflection),
                imports,
                unresolved,
            }
        })
        .collect()
}

/// A project read from its root file. The root keeps the top level, because in
/// Zig the root file is the program's own namespace, so a program of one file
/// ports exactly as it did before there were modules at all.
fn read_from_root(root: &Path) -> Result<Project, String> {
    let crawled = crawl(&[root.to_path_buf()])?;
    let base = common_directory(
        &crawled
            .iter()
            .map(|entry| entry.path.clone())
            .collect::<Vec<_>>(),
    );

    let mut ordered: Vec<&Crawled> = crawled.iter().collect();
    ordered.sort_by(|left, right| {
        let left_is_root = left.path.as_path() == root;
        let right_is_root = right.path.as_path() == root;
        right_is_root
            .cmp(&left_is_root)
            .then_with(|| left.path.cmp(&right.path))
    });

    let named: Vec<(PathBuf, String)> = ordered
        .iter()
        .map(|entry| {
            let name = if entry.path.as_path() == root {
                String::new()
            } else {
                module_name(&base, &entry.path)
            };
            (entry.path.clone(), name)
        })
        .collect();

    Ok(Project {
        modules: assemble(&ordered, &named, &base),
        artifacts: Vec::new(),
    })
}

/// A project read from its build script. There is no root file, because the
/// script may name several, so the top level namespace is empty and every file
/// is a named module. That is also what the port needs: a library of modules
/// with a binary per artifact rather than one program with several entry
/// points.
fn read_from_build_script(script: &Path) -> Result<Project, String> {
    let directory = script.parent().unwrap_or(Path::new(""));
    let declared = artifacts_in(&crate::parse_only(script)?, directory);
    if declared.is_empty() {
        return Err(format!(
            "{} declares no artifact this can read, so there is no build graph to follow. \
             Point at the root .zig file instead",
            script.display()
        ));
    }

    let mut roots: Vec<PathBuf> = declared.iter().filter_map(|one| one.root.clone()).collect();
    roots.sort();
    roots.dedup();
    if roots.is_empty() {
        return Err(format!(
            "{} names no root source file this can open, so nothing could be read",
            script.display()
        ));
    }

    let crawled = crawl(&roots)?;
    let base = common_directory(
        &crawled
            .iter()
            .map(|entry| entry.path.clone())
            .collect::<Vec<_>>(),
    );
    let mut ordered: Vec<&Crawled> = crawled.iter().collect();
    ordered.sort_by(|left, right| left.path.cmp(&right.path));
    let named: Vec<(PathBuf, String)> = ordered
        .iter()
        .map(|entry| (entry.path.clone(), module_name(&base, &entry.path)))
        .collect();

    // The empty module the rest hang off. Nothing in the project is written at
    // the top level, because no one file is the project.
    let mut modules = vec![SourceModule::default()];
    modules.extend(assemble(&ordered, &named, &base));

    let artifacts = declared
        .iter()
        .map(|one| Artifact {
            name: one.name.clone(),
            root: one.root.as_ref().and_then(|path| {
                named
                    .iter()
                    .find(|(candidate, _)| candidate == path)
                    .map(|(_, name)| name.clone())
            }),
            kind: one.kind,
        })
        .collect();

    Ok(Project { modules, artifacts })
}

/// Everything the project is made of, and what it builds.
///
/// The path is a root `.zig` file, a `build.zig`, or the directory one sits in.
pub fn read_project(root: &Path) -> Result<Project, String> {
    let root = crate::normalise(root);
    if root.is_dir() {
        let script = root.join("build.zig");
        if !script.is_file() {
            return Err(format!(
                "{} holds no build.zig, so there is no build graph to read. \
                 Point at the root .zig file instead",
                root.display()
            ));
        }
        return read_from_build_script(&script);
    }
    if root.file_name().is_some_and(|name| name == "build.zig") {
        return read_from_build_script(&root);
    }
    read_from_root(&root)
}
