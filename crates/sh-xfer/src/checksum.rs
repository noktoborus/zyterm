//! Saying that a file arrived whole.
//!
//! The device runs a program and prints what it says; this machine counts the
//! same sum over its own copy, and the two are compared. Only the first field
//! of what the device wrote is taken, which is where `md5sum`, `sha1sum` and
//! `sha256sum` all put it.

use crate::error::{Result, ShXferError};
use md5::Digest as Summing;
use std::io::Read;
use std::path::Path;

/// How much of a file is read at a time while it is counted.
const CHUNK: usize = 64 * 1024;

/// Which sum the two sides compare.
///
/// Every one of them is counted here rather than run, so a machine without
/// `sha1sum` on it — which is every Windows one — still checks what it sent.
/// The device is asked for the program of the same name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Digest {
    /// What `md5sum` says.
    Md5,
    /// What `sha1sum` says.
    Sha1,
    /// What `sha256sum` says.
    Sha256,
}

impl Digest {
    /// The program the device is told to run, which is the name of the sum.
    pub fn program(self) -> &'static str {
        match self {
            Self::Md5 => "md5sum",
            Self::Sha1 => "sha1sum",
            Self::Sha256 => "sha256sum",
        }
    }

    /// The sums a device is asked for in turn when it is asked for any of
    /// them, strongest first.
    pub fn every() -> [Self; 3] {
        [Self::Sha256, Self::Sha1, Self::Md5]
    }

    /// The sum of a file of this machine, written as that program writes it.
    pub fn of_file(self, path: &Path) -> Result<String> {
        match self {
            Self::Md5 => count(path, md5::Md5::new()),
            Self::Sha1 => count(path, sha1::Sha1::new()),
            Self::Sha256 => count(path, sha2::Sha256::new()),
        }
    }

    /// Says whether a file of this machine is what the device says it is.
    ///
    /// The two sums are compared and nothing else: the program on the device
    /// may write what it likes after the first field, and often does.
    pub fn matches(self, path: &Path, theirs: &str) -> Result<()> {
        let ours = self.of_file(path)?;
        if ours == theirs {
            return Ok(());
        }
        Err(ShXferError::Mismatch {
            path: path.display().to_string(),
            ours,
            theirs: theirs.to_string(),
        })
    }
}

/// The sum of a file, counted here, in the lower case hexadecimal every one of
/// these programs writes.
fn count<D: Summing>(path: &Path, mut digest: D) -> Result<String> {
    let mut file = std::fs::File::open(path).map_err(|source| ShXferError::Io {
        path: path.to_path_buf(),
        source,
    })?;

    let mut buffer = vec![0_u8; CHUNK];
    loop {
        let count = file.read(&mut buffer).map_err(|source| ShXferError::Io {
            path: path.to_path_buf(),
            source,
        })?;
        if count == 0 {
            break;
        }
        digest.update(&buffer[..count]);
    }

    Ok(digest
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(case: &str, body: &[u8]) -> std::path::PathBuf {
        let path =
            std::env::temp_dir().join(format!("sh-xfer-sum-{case}-{}.bin", std::process::id()));
        std::fs::write(&path, body).expect("the file is written");
        path
    }

    #[test]
    fn every_sum_this_crate_offers_is_counted_here() {
        let path = file("known", b"abc");
        assert_eq!(
            Digest::Sha1.of_file(&path).expect("a sum"),
            "a9993e364706816aba3e25717850c26c9cd0d89d"
        );
        assert_eq!(
            Digest::Md5.of_file(&path).expect("a sum"),
            "900150983cd24fb0d6963f7d28e17f72"
        );
        assert_eq!(
            Digest::Sha256.of_file(&path).expect("a sum"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn a_file_of_no_bytes_has_a_sum_of_its_own() {
        let path = file("empty", b"");
        assert_eq!(
            Digest::Sha1.of_file(&path).expect("a sum"),
            "da39a3ee5e6b4b0d3255bfef95601890afd80709"
        );
    }

    #[test]
    fn every_sum_is_named_by_the_program_that_says_it() {
        assert_eq!(Digest::Md5.program(), "md5sum");
        assert_eq!(Digest::Sha1.program(), "sha1sum");
        assert_eq!(Digest::Sha256.program(), "sha256sum");
    }

    #[test]
    fn the_strongest_sum_is_the_one_asked_for_first() {
        assert_eq!(Digest::every()[0], Digest::Sha256);
    }

    #[test]
    fn a_file_that_is_not_there_says_so() {
        let path = std::env::temp_dir().join("sh-xfer-sum-no-such-file.bin");
        let _ = std::fs::remove_file(&path);
        assert!(matches!(
            Digest::Sha1.of_file(&path),
            Err(ShXferError::Io { .. })
        ));
    }

    #[test]
    fn a_file_that_is_what_the_device_says_it_is_passes() {
        let path = file("match", b"abc");
        Digest::Sha1
            .matches(&path, "a9993e364706816aba3e25717850c26c9cd0d89d")
            .expect("the same file matches itself");
    }

    #[test]
    fn a_file_that_is_not_says_what_both_sides_made_of_it() {
        let path = file("mismatch", b"abc");
        let Err(ShXferError::Mismatch { ours, theirs, .. }) =
            Digest::Sha1.matches(&path, "0000000000000000000000000000000000000000")
        else {
            panic!("a file that does not match is refused");
        };
        assert_eq!(ours, "a9993e364706816aba3e25717850c26c9cd0d89d");
        assert_eq!(theirs, "0000000000000000000000000000000000000000");
    }
}
