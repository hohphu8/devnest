use crate::error::AppError;
use std::fs;
use std::path::Path;

pub fn copy_dir_recursive(source: &Path, destination: &Path) -> Result<(), AppError> {
    if !source.exists() || !source.is_dir() {
        return Err(AppError::new_validation(
            "INVALID_RUNTIME_SOURCE",
            "The selected runtime folder does not exist or is not a directory.",
        ));
    }

    fs::create_dir_all(destination)?;

    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let entry_path = entry.path();
        let destination_path = destination.join(entry.file_name());
        let file_type = entry.file_type()?;

        if file_type.is_dir() {
            copy_dir_recursive(&entry_path, &destination_path)?;
            continue;
        }

        if file_type.is_file() {
            fs::copy(&entry_path, &destination_path)?;
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::copy_dir_recursive;
    use std::fs;
    use std::path::PathBuf;
    use uuid::Uuid;

    fn temp_dir(prefix: &str) -> PathBuf {
        std::env::temp_dir().join(format!("{prefix}-{}", Uuid::new_v4()))
    }

    #[test]
    fn copies_nested_directory_tree() {
        let source = temp_dir("devnest-copy-source");
        let destination = temp_dir("devnest-copy-destination");

        fs::create_dir_all(source.join("bin")).expect("source bin dir should be created");
        fs::create_dir_all(source.join("conf")).expect("source conf dir should be created");
        fs::write(source.join("bin").join("runtime.exe"), "binary")
            .expect("runtime binary should be written");
        fs::write(source.join("conf").join("runtime.conf"), "config")
            .expect("runtime config should be written");

        copy_dir_recursive(&source, &destination).expect("copy should succeed");

        assert!(destination.join("bin").join("runtime.exe").exists());
        assert!(destination.join("conf").join("runtime.conf").exists());

        fs::remove_dir_all(source).ok();
        fs::remove_dir_all(destination).ok();
    }
}
