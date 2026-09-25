//! Paths inside a `file://` address, and what kind of content they hold.

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

/// Path inside a `file://` address, with its percent escapes resolved.
///
/// An address of another scheme, or one naming a host that is not this machine,
/// is not a path here.
pub fn path_of(uri: &str) -> Option<PathBuf> {
    let rest = uri
        .strip_prefix("file://")
        .or_else(|| uri.strip_prefix("FILE://"))?;
    let (host, path) = rest.split_at(rest.find('/')?);
    if !host.is_empty() && !host.eq_ignore_ascii_case("localhost") {
        return None;
    }

    let path = decode(path);
    if path.is_empty() {
        None
    } else {
        Some(PathBuf::from(path))
    }
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

    #[test]
    fn an_address_of_this_machine_is_a_path() {
        assert_eq!(
            path_of("file:///srv/two%20words/notes.txt"),
            Some(PathBuf::from("/srv/two words/notes.txt"))
        );
        assert_eq!(
            path_of("file://localhost/etc/hosts"),
            Some(PathBuf::from("/etc/hosts"))
        );
    }

    #[test]
    fn an_address_of_something_else_is_not() {
        assert_eq!(path_of("https://example.com/a"), None);
        assert_eq!(path_of("file://other-host/etc/hosts"), None);
        assert_eq!(path_of("file://"), None);
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
