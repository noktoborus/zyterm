//! Asking the desktop which program should open a link.

use std::io::{Error, Result};
use std::path::PathBuf;

/// Opens the chooser of the desktop for one address.
///
/// Linux asks the desktop portal, which shows the list of applications;
/// Windows asks the shell for its "open with" dialog. Where neither answers,
/// the link is opened with the default program instead of failing.
pub fn open_with(uri: &str) -> Result<()> {
    let uri = absolute(uri);
    let local = local_path(&uri);

    #[cfg(target_os = "linux")]
    {
        let asked = match &local {
            Some(path) => portal_open_file(path),
            None => portal_open_uri(&uri),
        };
        match asked {
            Ok(()) => return Ok(()),
            Err(error) => log::debug!("no chooser from the desktop portal: {error}"),
        }
    }

    #[cfg(windows)]
    if let Some(path) = &local {
        return std::process::Command::new("rundll32.exe")
            .arg("shell32.dll,OpenAs_RunDLL")
            .arg(path)
            .spawn()
            .map(|_| ());
    }

    let _ = &local;
    open::that_detached(&uri)
}

/// The portal wants an address with a scheme, so a bare path becomes a file
/// address before it is handed over.
fn absolute(uri: &str) -> String {
    if uri.contains("://") || uri.starts_with("mailto:") {
        return uri.to_string();
    }
    match std::fs::canonicalize(uri) {
        Ok(path) => format!("file://{}", path.to_string_lossy()),
        Err(_) => uri.to_string(),
    }
}

/// Path of a `file://` address on this machine, if it is one.
pub fn local_path(uri: &str) -> Option<PathBuf> {
    let rest = uri.strip_prefix("file://")?;
    let path = &rest[rest.find('/')?..];
    Some(PathBuf::from(percent_decode(path)))
}

/// Resolves the `%XX` escapes of an address.
fn percent_decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;

    while index < bytes.len() {
        if bytes[index] == b'%' && index + 2 < bytes.len() {
            let digits = std::str::from_utf8(&bytes[index + 1..index + 3]).unwrap_or("");
            if let Ok(byte) = u8::from_str_radix(digits, 16) {
                out.push(byte);
                index += 3;
                continue;
            }
        }
        out.push(bytes[index]);
        index += 1;
    }

    String::from_utf8_lossy(&out).into_owned()
}

/// Options every call of the portal carries.
#[cfg(target_os = "linux")]
fn ask_options() -> std::collections::HashMap<&'static str, zbus::zvariant::Value<'static>> {
    use zbus::zvariant::Value;

    let mut options = std::collections::HashMap::new();
    options.insert("ask", Value::Bool(true));
    options
}

/// Asks the portal to show its chooser for an address.
///
/// `OpenURI(parent_window: s, uri: s, options: a{sv}) -> o`; an empty parent
/// window is allowed and `ask` asks for the chooser instead of the default
/// application.
#[cfg(target_os = "linux")]
fn portal_open_uri(uri: &str) -> Result<()> {
    let connection =
        zbus::blocking::Connection::session().map_err(|error| Error::other(error.to_string()))?;

    connection
        .call_method(
            Some("org.freedesktop.portal.Desktop"),
            "/org/freedesktop/portal/desktop",
            Some("org.freedesktop.portal.OpenURI"),
            "OpenURI",
            &("", uri, ask_options()),
        )
        .map(|_| ())
        .map_err(|error| Error::other(error.to_string()))
}

/// Asks the portal to show its chooser for a file on this machine.
///
/// `OpenFile(parent_window: s, fd: h, options: a{sv}) -> o`. The portal refuses
/// `file://` addresses in `OpenURI` on purpose, so a local file is handed over
/// as an open file descriptor instead.
#[cfg(target_os = "linux")]
fn portal_open_file(path: &std::path::Path) -> Result<()> {
    use std::os::fd::AsFd;

    let file = std::fs::File::open(path)?;
    let connection =
        zbus::blocking::Connection::session().map_err(|error| Error::other(error.to_string()))?;

    connection
        .call_method(
            Some("org.freedesktop.portal.Desktop"),
            "/org/freedesktop/portal/desktop",
            Some("org.freedesktop.portal.OpenURI"),
            "OpenFile",
            &("", zbus::zvariant::Fd::from(file.as_fd()), ask_options()),
        )
        .map(|_| ())
        .map_err(|error| Error::other(error.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_local_address_becomes_a_path() {
        assert_eq!(
            local_path("file:///tmp/my%20file.txt"),
            Some(PathBuf::from("/tmp/my file.txt"))
        );
        assert_eq!(
            local_path("file://host/srv/data"),
            Some(PathBuf::from("/srv/data"))
        );
        assert_eq!(local_path("https://example.org/a"), None);
        assert_eq!(local_path("mailto:someone@example.org"), None);
    }

    #[test]
    fn a_bare_path_gets_a_scheme() {
        let directory = std::env::temp_dir();
        let uri = absolute(&directory.to_string_lossy());
        assert!(uri.starts_with("file://"), "{uri}");

        assert_eq!(absolute("https://example.org"), "https://example.org");
        assert_eq!(
            absolute("mailto:someone@example.org"),
            "mailto:someone@example.org"
        );
    }
}
