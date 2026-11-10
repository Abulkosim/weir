use std::{
    fs,
    io::{self, Write},
    path::Path,
};

/// Replaces `path` with `text` so a crash or power loss leaves either the old
/// file or the new one, never a partly written one.
pub fn replace(path: &Path, text: &str) -> io::Result<()> {
    let directory = path.parent();
    if let Some(directory) = directory {
        fs::create_dir_all(directory)?;
    }
    let temporary = path.with_extension("json.tmp");
    let mut file = fs::File::create(&temporary)?;
    file.write_all(text.as_bytes())?;
    file.sync_all()?;
    drop(file);
    fs::rename(&temporary, path)?;
    #[cfg(unix)]
    if let Some(directory) = directory {
        fs::File::open(directory)?.sync_all()?;
    }
    Ok(())
}
