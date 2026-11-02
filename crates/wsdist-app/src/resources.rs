use std::ffi::OsStr;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use wsdist_core::{CalcError, Catalog};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppPaths {
    pub root: PathBuf,
    pub data: PathBuf,
    pub defaults: PathBuf,
    pub item_list: PathBuf,
    pub icons: PathBuf,
}

#[derive(Debug, thiserror::Error)]
pub enum ResourceError {
    #[error("Unable to locate the executable for application resources: {source}")]
    Executable {
        #[source]
        source: io::Error,
    },
    #[error("Unable to resolve relative WSDIST_DATA_DIR {path:?}: {source}")]
    CurrentDirectory {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("Cannot locate application resources ({path:?}): {message}")]
    InvalidRoot {
        path: PathBuf,
        message: &'static str,
    },
    #[error(transparent)]
    Catalog(#[from] CalcError),
}

pub struct Resources {
    pub paths: AppPaths,
    pub catalog: Arc<Catalog>,
}

impl AppPaths {
    pub fn discover() -> Result<Self, ResourceError> {
        match std::env::var_os("WSDIST_DATA_DIR") {
            Some(value) => {
                let path = PathBuf::from(value);
                if path.is_absolute() || path.as_os_str().is_empty() {
                    Self::from_override(&path, Path::new(""))
                } else {
                    let current_dir = std::env::current_dir().map_err(|source| {
                        ResourceError::CurrentDirectory {
                            path: path.clone(),
                            source,
                        }
                    })?;
                    Self::from_override(&path, &current_dir)
                }
            }
            None => {
                let executable = std::env::current_exe()
                    .map_err(|source| ResourceError::Executable { source })?;
                Self::from_executable(&executable)
            }
        }
    }

    /// Resolves the same inputs as `discover` without reading process-global state.
    /// `current_dir` is used only for a relative environment override; it is never
    /// a fallback resource root. No filesystem canonicalization is performed.
    pub fn from_parts(
        data_dir: Option<&OsStr>,
        executable: &Path,
        current_dir: &Path,
    ) -> Result<Self, ResourceError> {
        match data_dir {
            Some(value) => Self::from_override(Path::new(value), current_dir),
            None => Self::from_executable(executable),
        }
    }

    fn from_override(path: &Path, current_dir: &Path) -> Result<Self, ResourceError> {
        if path.as_os_str().is_empty() {
            return Err(ResourceError::InvalidRoot {
                path: path.to_path_buf(),
                message: "WSDIST_DATA_DIR is set but empty; specify the application resource directory",
            });
        }
        if path.is_absolute() {
            return Ok(Self::from_root(path.to_path_buf()));
        }
        if !current_dir.is_absolute() {
            return Err(ResourceError::InvalidRoot {
                path: current_dir.to_path_buf(),
                message: "the current directory used to resolve WSDIST_DATA_DIR must be absolute",
            });
        }

        let root = resolve_relative_override(path, current_dir)?;
        Ok(Self::from_root(root))
    }

    fn from_executable(executable: &Path) -> Result<Self, ResourceError> {
        if !executable.is_absolute() {
            return Err(ResourceError::InvalidRoot {
                path: executable.to_path_buf(),
                message: "the executable path must be absolute; the working directory is not a resource fallback",
            });
        }
        let root = executable.parent().ok_or_else(|| ResourceError::InvalidRoot {
            path: executable.to_path_buf(),
            message: "the executable path has no parent directory",
        })?;
        Ok(Self::from_root(root.to_path_buf()))
    }

    fn from_root(root: PathBuf) -> Self {
        Self {
            data: root.join("data"),
            defaults: root.join("defaults.json"),
            item_list: root.join("item_list.csv"),
            icons: root.join("icons32"),
            root,
        }
    }
}

fn resolve_relative_override(path: &Path, current_dir: &Path) -> Result<PathBuf, ResourceError> {
    #[cfg(windows)]
    {
        use std::path::Component;

        let mut components = path.components();
        if let Some(Component::Prefix(prefix)) = components.next() {
            let same_drive = matches!(
                current_dir.components().next(),
                Some(Component::Prefix(current)) if current.kind() == prefix.kind()
            );
            if !same_drive {
                return Err(ResourceError::InvalidRoot {
                    path: path.to_path_buf(),
                    message: "a drive-relative WSDIST_DATA_DIR must use the current drive; use an absolute path for another drive",
                });
            }
            return Ok(current_dir.join(components.as_path()));
        }
    }

    Ok(current_dir.join(path))
}

pub fn load_resources() -> Result<Resources, ResourceError> {
    load_resources_at(AppPaths::discover()?)
}

pub fn load_resources_at(paths: AppPaths) -> Result<Resources, ResourceError> {
    let catalog = Arc::new(Catalog::load(&paths.root)?);
    Ok(Resources { paths, catalog })
}
