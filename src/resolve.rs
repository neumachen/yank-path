//! Strict, all-or-nothing path resolver.
//!
//! Every operand must exist on disk. The first failure aborts the whole
//! call with [`YankError::NotFound`] so the App never produces partial
//! output or touches the clipboard for a bad input set.
//!
//! Order is preserved and duplicates are kept (v1 has no `--unique`).

use std::path::{Component, Path, PathBuf};

use crate::error::YankError;
use crate::fs::FileSystem;

/// Resolve each operand to an absolute path, verifying existence.
///
/// Returns the operands as absolute paths in input order on success. On the
/// first non-existent operand returns `Err(YankError::NotFound(operand))`
/// where `operand` is the *original* path the user supplied (so the error
/// message points at what the caller actually typed).
pub fn resolve_operands(
    operands: &[PathBuf],
    fs: &dyn FileSystem,
) -> Result<Vec<PathBuf>, YankError> {
    if operands.is_empty() {
        return Ok(Vec::new());
    }

    let cwd = fs.cwd()?;
    let mut resolved = Vec::with_capacity(operands.len());

    for operand in operands {
        let abs = absolutize(operand, &cwd);
        if !fs.exists(&abs) {
            return Err(YankError::NotFound(operand.clone()));
        }
        let final_path = match fs.canonicalize(&abs) {
            Ok(canon) => normalize_components(&canon),
            Err(_) => normalize_components(&abs),
        };
        resolved.push(final_path);
    }

    Ok(resolved)
}

/// Make `target` absolute by joining with `cwd` when relative.
fn absolutize(target: &Path, cwd: &Path) -> PathBuf {
    if target.is_absolute() {
        target.to_path_buf()
    } else {
        cwd.join(target)
    }
}

/// Lexically collapse `.` and `..` components — no disk access.
fn normalize_components(path: &Path) -> PathBuf {
    let mut out: Vec<Component> = Vec::new();
    for comp in path.components() {
        match comp {
            Component::CurDir => {}
            Component::ParentDir => {
                let pop = matches!(out.last(), Some(Component::Normal(_)));
                if pop {
                    out.pop();
                } else {
                    let on_root = matches!(
                        out.last(),
                        Some(Component::RootDir) | Some(Component::Prefix(_))
                    );
                    if !on_root {
                        out.push(comp);
                    }
                }
            }
            _ => out.push(comp),
        }
    }
    let mut buf = PathBuf::new();
    for c in out {
        buf.push(c.as_os_str());
    }
    if buf.as_os_str().is_empty() {
        PathBuf::from(".")
    } else {
        buf
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::collections::HashSet;

    /// Tiny in-memory `FileSystem` for resolver tests.
    struct MemFs {
        cwd: PathBuf,
        existing: RefCell<HashSet<PathBuf>>,
    }

    impl MemFs {
        fn new(cwd: impl Into<PathBuf>) -> Self {
            Self {
                cwd: cwd.into(),
                existing: RefCell::new(HashSet::new()),
            }
        }
        fn with_file(self, p: impl Into<PathBuf>) -> Self {
            self.existing.borrow_mut().insert(p.into());
            self
        }
    }

    impl FileSystem for MemFs {
        fn cwd(&self) -> Result<PathBuf, YankError> {
            Ok(self.cwd.clone())
        }
        fn home(&self) -> Option<PathBuf> {
            None
        }
        fn exists(&self, path: &Path) -> bool {
            self.existing.borrow().contains(path)
        }
        fn canonicalize(&self, path: &Path) -> Result<PathBuf, YankError> {
            if self.existing.borrow().contains(path) {
                Ok(path.to_path_buf())
            } else {
                Err(YankError::NotFound(path.to_path_buf()))
            }
        }
        fn is_dir(&self, _path: &Path) -> bool {
            false
        }
        fn is_file(&self, path: &Path) -> bool {
            self.existing.borrow().contains(path)
        }
    }

    #[test]
    fn all_existing_paths_resolved_in_order() {
        let fs = MemFs::new("/work")
            .with_file(PathBuf::from("/work/a.txt"))
            .with_file(PathBuf::from("/work/b.txt"));
        let inputs = vec![PathBuf::from("a.txt"), PathBuf::from("b.txt")];
        let resolved = resolve_operands(&inputs, &fs).unwrap();
        assert_eq!(
            resolved,
            vec![PathBuf::from("/work/a.txt"), PathBuf::from("/work/b.txt")]
        );
    }

    #[test]
    fn absolute_operand_passes_through() {
        let fs = MemFs::new("/work").with_file(PathBuf::from("/etc/hosts"));
        let inputs = vec![PathBuf::from("/etc/hosts")];
        let resolved = resolve_operands(&inputs, &fs).unwrap();
        assert_eq!(resolved, vec![PathBuf::from("/etc/hosts")]);
    }

    #[test]
    fn one_missing_aborts_whole_call() {
        let fs = MemFs::new("/work").with_file(PathBuf::from("/work/a.txt"));
        let inputs = vec![PathBuf::from("a.txt"), PathBuf::from("missing.txt")];
        let err = resolve_operands(&inputs, &fs).unwrap_err();
        match err {
            YankError::NotFound(p) => assert_eq!(p, PathBuf::from("missing.txt")),
            other => panic!("expected NotFound, got {other:?}"),
        }
    }

    #[test]
    fn empty_input_returns_empty() {
        let fs = MemFs::new("/work");
        let resolved = resolve_operands(&[], &fs).unwrap();
        assert!(resolved.is_empty());
    }

    #[test]
    fn duplicates_are_preserved() {
        let fs = MemFs::new("/work").with_file(PathBuf::from("/work/a.txt"));
        let inputs = vec![PathBuf::from("a.txt"), PathBuf::from("a.txt")];
        let resolved = resolve_operands(&inputs, &fs).unwrap();
        assert_eq!(
            resolved,
            vec![PathBuf::from("/work/a.txt"), PathBuf::from("/work/a.txt")]
        );
    }
}
