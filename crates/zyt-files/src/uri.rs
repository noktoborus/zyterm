//! Where the address of a link leads, and what kind of content a file holds.

use std::path::PathBuf;

/// What the clipboard of a desktop can be given.
///
/// A clipboard carries text and pictures and nothing else, so everything that is
/// neither is named by its media type and left to the caller to refuse.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Content {
    /// Text of this media type.
    Text(String),
    /// A picture of this media type.
    Image(String),
    /// Anything else, named by its media type.
    Other(String),
}

impl Content {
    /// Media type of the content, the string a clipboard would advertise.
    pub fn media_type(&self) -> &str {
        match self {
            Self::Text(media) | Self::Image(media) | Self::Other(media) => media,
        }
    }
}

/// Where the address of a link leads.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Place {
    /// A file or a directory of this machine, by its path.
    Here(PathBuf),
    /// A share of the network written as a path, `\\server\share`, on Windows.
    ///
    /// It is of the file system of this machine, as a path is, and is never
    /// touched to learn what it holds: a server that does not answer keeps the
    /// one who asked waiting for as long as Windows waits for it.
    Share,
    /// Anywhere else: a page, a mail, a file of another host.
    Away,
}

/// Where an address leads.
///
/// A `file://` address names this machine when it names no host, `localhost`
/// or the name of this machine, in full or short; its path has its percent
/// escapes resolved. On Windows a path of a drive, `C:\x` or `C:/x`, is a path
/// here as well, and `\\server\share` is a share.
pub fn place_of(uri: &str) -> Place {
    let machine = gethostname::gethostname();
    place_with(uri, &machine.to_string_lossy(), cfg!(windows))
}

/// [`place_of`], with the name of this machine and the kind of system given.
fn place_with(uri: &str, machine: &str, windows: bool) -> Place {
    if windows && uri.starts_with("\\\\") {
        return Place::Share;
    }
    if windows && is_drive_path(uri) {
        return Place::Here(PathBuf::from(uri));
    }
    let Some(rest) = uri
        .strip_prefix("file://")
        .or_else(|| uri.strip_prefix("FILE://"))
    else {
        return Place::Away;
    };
    let Some(slash) = rest.find('/') else {
        return Place::Away;
    };
    let (host, path) = rest.split_at(slash);
    if !is_this_machine(host, machine) {
        return Place::Away;
    }
    let path = decode(path);
    match path.is_empty() {
        true => Place::Away,
        false => Place::Here(PathBuf::from(path)),
    }
}

/// `C:\` or `C:/` at the head of the text.
fn is_drive_path(text: &str) -> bool {
    let bytes = text.as_bytes();
    bytes.len() >= 3
        && bytes[0].is_ascii_alphabetic()
        && bytes[1] == b':'
        && matches!(bytes[2], b'\\' | b'/')
}

/// Whether the host of a `file://` address is this machine.
///
/// The name is compared whole and by its first label either way, so
/// `pc.home.lan` and `pc` are one machine whichever of the two is known here.
fn is_this_machine(host: &str, machine: &str) -> bool {
    let short = |name: &str| name.split('.').next().unwrap_or(name).to_string();
    host.is_empty()
        || host.eq_ignore_ascii_case("localhost")
        || (!machine.is_empty()
            && (host.eq_ignore_ascii_case(machine)
                || host.eq_ignore_ascii_case(&short(machine))
                || short(host).eq_ignore_ascii_case(machine)))
}

/// What the name of a file says about its content.
///
/// The media type comes from `mime_guess`, which knows the list; what a
/// clipboard can do with it is decided here: text is text, a picture is a
/// picture, and everything else is named and left alone. A name that says
/// nothing is read as plain text, which is what a terminal deals in.
pub fn content_of(path: &std::path::Path) -> Content {
    let Some(media) = mime_guess::from_path(path).first() else {
        return Content::Text(mime_guess::mime::TEXT_PLAIN.to_string());
    };

    let name = media.essence_str().to_string();
    match media.type_().as_str() {
        "text" => Content::Text(name),
        "image" => Content::Image(name),
        "application" if is_text(media.subtype().as_str()) => Content::Text(name),
        _ => Content::Other(name),
    }
}

/// Media types below `application` that are text all the same.
fn is_text(subtype: &str) -> bool {
    subtype.ends_with("json")
        || subtype.ends_with("xml")
        || subtype.ends_with("yaml")
        || subtype.ends_with("toml")
        || matches!(
            subtype,
            "javascript" | "x-sh" | "x-shellscript" | "sql" | "x-yaml" | "x-toml"
        )
}

/// Resolves the `%XX` escapes of an address.
fn decode(text: &str) -> String {
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

#[cfg(test)]
mod tests {
    use super::*;

    fn here(path: &str) -> Place {
        Place::Here(PathBuf::from(path))
    }

    #[test]
    fn an_address_of_this_machine_is_a_path() {
        let place = |uri| place_with(uri, "pc.home.lan", false);
        assert_eq!(
            place("file:///srv/two%20words/notes.txt"),
            here("/srv/two words/notes.txt")
        );
        assert_eq!(place("file://localhost/etc/hosts"), here("/etc/hosts"));
        assert_eq!(place("file://PC.home.lan/etc/hosts"), here("/etc/hosts"));
        assert_eq!(
            place("file://pc/etc/hosts"),
            here("/etc/hosts"),
            "the short name"
        );
        assert_eq!(
            place_with("file://pc.home.lan/etc/hosts", "pc", false),
            here("/etc/hosts"),
            "the full name of a machine that knows its short one"
        );
    }

    #[test]
    fn an_address_of_something_else_is_away() {
        let place = |uri| place_with(uri, "pc", false);
        assert_eq!(place("https://example.com/a"), Place::Away);
        assert_eq!(place("file://other-host/etc/hosts"), Place::Away);
        assert_eq!(place("file://"), Place::Away);
        assert_eq!(
            place("C:\\notes.txt"),
            Place::Away,
            "not a path off Windows"
        );
        assert_eq!(place("\\\\server\\share"), Place::Away);
    }

    #[test]
    fn a_path_of_windows_is_a_path_there() {
        let place = |uri| place_with(uri, "pc", true);
        assert_eq!(place("C:\\notes.txt"), here("C:\\notes.txt"));
        assert_eq!(place("z:/notes.txt"), here("z:/notes.txt"));
        assert_eq!(place("\\\\server\\share\\a"), Place::Share);
        assert_eq!(place("https://example.com/a"), Place::Away);
    }

    #[test]
    fn the_name_says_how_the_clipboard_would_carry_it() {
        assert_eq!(
            content_of(std::path::Path::new("/tmp/a.txt")),
            Content::Text("text/plain".to_string())
        );
        assert_eq!(
            content_of(std::path::Path::new("/tmp/a.json")),
            Content::Text("application/json".to_string()),
            "json is text, whatever the list calls it"
        );
        assert_eq!(
            content_of(std::path::Path::new("/tmp/a.PNG")),
            Content::Image("image/png".to_string())
        );
        assert!(matches!(
            content_of(std::path::Path::new("/tmp/a.zip")),
            Content::Other(_)
        ));
        assert_eq!(
            content_of(std::path::Path::new("/tmp/readme")),
            Content::Text("text/plain".to_string()),
            "a name that says nothing is read as text"
        );
    }
}
