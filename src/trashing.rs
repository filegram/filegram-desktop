//! Moving entries to the trash.

use std::io;
use std::path::Path;

/// Trashes `path`. On Linux files go through the desktop portal first: a
/// sandboxed build (snap, Flatpak) sees its own `XDG_DATA_HOME`, so the
/// fallback would bury the file in a trash directory nobody opens. The portal
/// wants a writable descriptor, which rules out directories.
pub fn delete(path: &Path) -> io::Result<()> {
    #[cfg(target_os = "linux")]
    if path.is_file() {
        match portal::trash_file(path) {
            Ok(()) => return Ok(()),
            Err(error) => {
                eprintln!("filegram: the trash portal declined, falling back: {error}");
            }
        }
    }
    trash::delete(path).map_err(io::Error::other)
}

#[cfg(target_os = "linux")]
mod portal {
    use std::fs::OpenOptions;
    use std::io;
    use std::os::fd::AsFd;
    use std::path::Path;

    use zbus::blocking::Connection;
    use zbus::zvariant::Fd;

    pub fn trash_file(path: &Path) -> io::Result<()> {
        // The portal refuses a descriptor that is not open for writing.
        let file = OpenOptions::new().read(true).write(true).open(path)?;
        let connection = Connection::session().map_err(io::Error::other)?;
        let reply = connection
            .call_method(
                Some("org.freedesktop.portal.Desktop"),
                "/org/freedesktop/portal/desktop",
                Some("org.freedesktop.portal.Trash"),
                "TrashFile",
                &(Fd::from(file.as_fd()),),
            )
            .map_err(io::Error::other)?;
        let result: u32 = reply.body().deserialize().map_err(io::Error::other)?;
        // 1 is the only success value the interface defines.
        if result == 1 {
            Ok(())
        } else {
            Err(io::Error::other("the portal did not trash the file"))
        }
    }
}
