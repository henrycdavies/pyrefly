/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

use std::fmt::Display;
use std::io::BufRead;
use std::ops::Deref;
use std::ops::DerefMut;
use std::path::Path;
use std::path::PathBuf;

use serde::Deserialize;
use serde::Serialize;

/// Used in serde's skip_serializing_if attribute to skip serializing a boolean field that defaults to true.
#[allow(clippy::trivially_copy_pass_by_ref)]
pub(crate) fn skip_default_true(v: &bool) -> bool {
    *v
}

/// Used in serde's skip_serializing_if attribute to skip serializing a boolean field that defaults to true.
#[allow(clippy::trivially_copy_pass_by_ref)]
pub(crate) fn skip_default_false(v: &bool) -> bool {
    !*v
}

/// Used in serde's skip_serializing_if attribute to skip serializing a boolean field that defaults to false.
pub(crate) fn none_or_empty<T>(v: &Option<Vec<T>>) -> bool {
    v.as_ref().is_none_or(|v| v.is_empty())
}

/// A helper struct for detailing the origin of a config option.
/// The `Serialize` functionality enables us to only serialize [`ConfigOrigin::ConfigFile`]
/// values, skipping serialization for values we might be able to figure out automatically,
/// and we deserialize directly into a `ConfigFile` variant, making it easy to
/// update with a different `Auto` or `CommandLine` variant when overriding it.
#[derive(Debug, PartialEq, Eq, Deserialize, Clone, Copy)]
#[serde(untagged)]
pub(crate) enum ConfigOrigin<T> {
    /// This value was explicitly provided from a CLI flag.
    #[serde(skip)]
    CommandLine(T),

    /// This value was automatically constructed by Pyrefly.
    #[serde(skip)]
    Auto(T),

    /// This value was explicitly provided by the IDE using "workspace.configuration" using LSP protocol.
    #[serde(skip)]
    Lsp(T),

    /// This value was provided by a [`ConfigFile`], either explicitly or implicitly.
    ConfigFile(T),
}

impl<T: Default> Default for ConfigOrigin<T> {
    fn default() -> Self {
        ConfigOrigin::ConfigFile(T::default())
    }
}

impl<T: Serialize> Serialize for ConfigOrigin<T> {
    /// Serialize this `ConfigOrigin`'s internal value, making the `ConfigOrigin`
    /// transparent. This will serialize ALL `ConfigOrigin` values,
    /// so you must use `skip_serializing_if` helpers below to better control
    /// when to output a value.
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        self.deref().serialize(serializer)
    }
}

impl<T: Display> Display for ConfigOrigin<T> {
    /// Display this `ConfigOrigin`'s internal value, making the `ConfigOrigin`
    /// transparent.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.deref().fmt(f)
    }
}

impl<T> Deref for ConfigOrigin<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        match self {
            Self::CommandLine(value)
            | Self::ConfigFile(value)
            | Self::Auto(value)
            | Self::Lsp(value) => value,
        }
    }
}

impl<T> DerefMut for ConfigOrigin<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        match self {
            Self::CommandLine(value)
            | Self::ConfigFile(value)
            | Self::Auto(value)
            | Self::Lsp(value) => value,
        }
    }
}

impl<T: Deref> ConfigOrigin<T> {
    pub(crate) fn as_deref(&self) -> ConfigOrigin<&<T as Deref>::Target> {
        self.as_ref().map(|t| t.deref())
    }
}

impl<T> ConfigOrigin<T> {
    /// Construct a new [`ConfigOrigin::CommandLine`] with the given value.
    pub(crate) fn cli(value: T) -> Self {
        Self::CommandLine(value)
    }

    /// Construct a new [`ConfigOrigin::ConfigFile`] with the given value.
    pub(crate) fn config(value: T) -> Self {
        Self::ConfigFile(value)
    }

    /// Construct a new [`ConfigOrigin::Auto`] with the given value.
    pub(crate) fn auto(value: T) -> Self {
        Self::Auto(value)
    }

    /// Construct a new [`ConfigOrigin::Lsp`] with the given value.
    pub(crate) fn lsp(value: T) -> Self {
        Self::Lsp(value)
    }

    /// Consume the [`ConfigOrigin`], mapping the internal value with the
    /// given function, and inserting it into a new [`ConfigOrigin`] of the same
    /// variant.
    pub(crate) fn map<U, F>(self, f: F) -> ConfigOrigin<U>
    where
        F: FnOnce(T) -> U,
    {
        match self {
            ConfigOrigin::ConfigFile(value) => ConfigOrigin::ConfigFile(f(value)),
            ConfigOrigin::CommandLine(value) => ConfigOrigin::CommandLine(f(value)),
            ConfigOrigin::Auto(value) => ConfigOrigin::Auto(f(value)),
            ConfigOrigin::Lsp(value) => ConfigOrigin::Lsp(f(value)),
        }
    }

    pub fn as_ref(&self) -> ConfigOrigin<&T> {
        match *self {
            ConfigOrigin::ConfigFile(ref value) => ConfigOrigin::ConfigFile(value),
            ConfigOrigin::CommandLine(ref value) => ConfigOrigin::CommandLine(value),
            ConfigOrigin::Auto(ref value) => ConfigOrigin::Auto(value),
            ConfigOrigin::Lsp(ref value) => ConfigOrigin::Lsp(value),
        }
    }

    /// Determine if this [`Option<ConfigOrigin>`] should be output when serializing.
    /// We only serialize if the value is `Some(ConfigFile)`. All other
    /// [`Option`] and [`ConfigOrigin`] variants are not serialized.
    pub(crate) fn should_skip_serializing_option(origin: &Option<Self>) -> bool {
        !matches!(origin, Some(Self::ConfigFile(_)))
    }
}

impl<T, E> ConfigOrigin<Result<T, E>> {
    /// Swap a ConfigOrigin<Result<T, E>>'s [`ConfigOrigin`] and [`Result`] types,
    /// turning it into a `Result<ConfigOrigin<T>, E>`.
    pub(crate) fn transpose_err(self) -> Result<ConfigOrigin<T>, E> {
        match self {
            ConfigOrigin::ConfigFile(Ok(value)) => Ok(ConfigOrigin::ConfigFile(value)),
            ConfigOrigin::CommandLine(Ok(value)) => Ok(ConfigOrigin::CommandLine(value)),
            ConfigOrigin::Auto(Ok(value)) => Ok(ConfigOrigin::Auto(value)),
            ConfigOrigin::Lsp(Ok(value)) => Ok(ConfigOrigin::Lsp(value)),
            ConfigOrigin::ConfigFile(Err(err))
            | ConfigOrigin::CommandLine(Err(err))
            | ConfigOrigin::Auto(Err(err))
            |             ConfigOrigin::Lsp(Err(err)) => Err(err),
        }
    }
}

/// Expands .pth files found in the given directories, returning all paths
/// referenced by those .pth files that exist and are valid.
///
/// .pth files contain one path per line. Lines starting with 'import' are
/// ignored as they represent import hooks (which Pyrefly cannot execute).
/// Empty lines, lines with only whitespace, and comment lines are also ignored.
///
/// This mimics Python's .pth file processing behavior for site-packages directories,
/// allowing Pyrefly to handle custom site-package directories
/// that contain .pth files but aren't in Python's standard site-packages locations.
pub fn expand_pth_files(site_package_dirs: &[PathBuf]) -> Vec<PathBuf> {
    let mut expanded_paths = Vec::new();

    for dir in site_package_dirs {
        if !dir.is_dir() {
            continue;
        }

        let Ok(entries) = std::fs::read_dir(dir) else {
            continue;
        };

        for entry in entries.flatten() {
            let path = entry.path();

            // Only process .pth files
            if path.extension() != Some(std::ffi::OsStr::new("pth")) {
                continue;
            }

            let Ok(file) = std::fs::File::open(&path) else {
                continue;
            };

            let reader = std::io::BufReader::new(file);

            for line in reader.lines().flatten() {
                let trimmed = line.trim();

                // Skip empty lines, comments, and import hooks
                // Import hooks start with "import " or "import\t"
                if trimmed.is_empty()
                    || trimmed.starts_with('#')
                    || trimmed.starts_with("import ")
                    || trimmed.starts_with("import\t")
                {
                    continue;
                }

                // Path can be absolute or relative to the .pth file's directory
                let pth_path = if Path::new(trimmed).is_absolute() {
                    PathBuf::from(trimmed)
                } else {
                    dir.join(trimmed)
                };

                // Only add if the path exists
                if pth_path.exists() {
                    expanded_paths.push(pth_path);
                }
            }
        }
    }

    expanded_paths
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_expand_pth_files_empty_dir() {
        let result = expand_pth_files(&[]);
        assert!(result.is_empty());
    }

    #[test]
    fn test_expand_pth_files_nonexistent_dir() {
        let result = expand_pth_files(&[PathBuf::from("/nonexistent/path")]);
        assert!(result.is_empty());
    }

    #[test]
    fn test_expand_pth_files_with_content() {
        use std::fs;
        use tempfile::tempdir;

        let temp = tempdir().unwrap();
        let site_packages = temp.path().join("site-packages");
        fs::create_dir(&site_packages).unwrap();

        // Create a .pth file with various types of content
        let pth_content = format!(
            "# This is a comment\n\
             relative/path\n\
             /absolute/path\n\
             import some_module\n\
             \n\
             \t\n\
             another/relative/path\n"
        );
        fs::write(site_packages.join("test.pth"), pth_content).unwrap();

        // Create the relative paths so they exist
        let relative1 = site_packages.join("relative/path");
        let relative2 = site_packages.join("another/relative/path");
        fs::create_dir_all(&relative1).unwrap();
        fs::create_dir_all(&relative2).unwrap();

        let expanded = expand_pth_files(&[site_packages]);

        // Should only include the existing relative paths
        // (absolute doesn't exist, import line ignored, comments and empty lines ignored)
        assert_eq!(expanded.len(), 2);
        assert!(expanded.contains(&relative1));
        assert!(expanded.contains(&relative2));
    }

    #[test]
    fn test_expand_pth_files_multiple_pth_files() {
        use std::fs;
        use tempfile::tempdir;

        let temp = tempdir().unwrap();
        let site_packages = temp.path().join("site-packages");
        fs::create_dir(&site_packages).unwrap();

        // Create multiple .pth files
        let path1 = site_packages.join("path1");
        let path2 = site_packages.join("path2");
        fs::create_dir(&path1).unwrap();
        fs::create_dir(&path2).unwrap();

        fs::write(site_packages.join("first.pth"), "path1\n").unwrap();
        fs::write(site_packages.join("second.pth"), "path2\n").unwrap();

        let expanded = expand_pth_files(&[site_packages]);

        assert_eq!(expanded.len(), 2);
        assert!(expanded.contains(&path1));
        assert!(expanded.contains(&path2));
    }

    #[test]
    fn test_expand_pth_files_ignores_non_pth_files() {
        use std::fs;
        use tempfile::tempdir;

        let temp = tempdir().unwrap();
        let site_packages = temp.path().join("site-packages");
        fs::create_dir(&site_packages).unwrap();

        // Create a non-.pth file
        fs::write(site_packages.join("not_a_pth.txt"), "some/path\n").unwrap();

        let expanded = expand_pth_files(&[site_packages]);

        // Should be empty since we only process .pth files
        assert!(expanded.is_empty());
    }
}
