use crate::image::{ContentDigest, ImageVersion, Registry, Repository};
use std::{
    hash::Hash,
    ops::Range,
    path::{Path, PathBuf},
};

#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub enum StorageKey {
    Temp(u32, u64),
    Blob(ContentDigest),
    BlobPart(ContentDigest, Range<usize>),
    Manifest(Registry, Repository, ImageVersion),
}

impl StorageKey {
    pub fn temp() -> Self {
        StorageKey::Temp(std::process::id(), rand::random::<u64>())
    }

    pub fn range(self, sub_range: Range<usize>) -> Result<StorageKey, ()> {
        match self {
            StorageKey::Blob(content_digest) => Ok(StorageKey::BlobPart(content_digest, sub_range)),
            StorageKey::BlobPart(parent_digest, parent_range) => Ok(StorageKey::BlobPart(
                parent_digest,
                (sub_range.start + parent_range.start)..(sub_range.end + parent_range.start),
            )),
            _ => Err(()),
        }
    }

    pub fn to_path(&self, base_dir: &Path) -> PathBuf {
        match self {
            StorageKey::Temp(pid, random) => {
                let mut path = base_dir.to_path_buf();
                path.push("tmp");
                path.push(format!("{}-{}", pid, random));
                path.set_extension("tmp");
                path
            }
            StorageKey::Blob(content_digest) => {
                let mut path = base_dir.to_path_buf();
                path.push("blobs");
                path.push(path_encode(content_digest.as_str()));
                path.set_extension("blob");
                path
            }
            StorageKey::BlobPart(content_digest, range) => {
                let mut path = base_dir.to_path_buf();
                path.push("parts");
                path.push(path_encode(content_digest.as_str()));
                path.push(format!("{:x}-{:x}", range.start, range.end));
                path.set_extension("part");
                path
            }
            StorageKey::Manifest(registry, repository, version) => {
                let mut path = base_dir.to_path_buf();
                path.push("manifest");
                path.push(path_encode(registry.as_str()));
                path.push(path_encode(repository.as_str()));
                path.push(path_encode(version.as_str()));
                path.set_extension("json");
                path
            }
        }
    }
}

/// Encode any input string in a way which preserves uniqueness but only uses
/// lowercase alphanumeric characters and dashes.
fn path_encode(input: &str) -> String {
    let mut result = String::with_capacity(input.len() + 16);
    let mut changes = String::with_capacity(16);
    let mut in_replacement = false;
    for (idx, ch) in input.char_indices() {
        if ('a'..='z').contains(&ch) || ('0'..='9').contains(&ch) {
            // No change
            in_replacement = false;
            result.push(ch)
        } else if ('A'..='Z').contains(&ch) {
            // Record case conversion
            in_replacement = false;
            result.push(ch.to_ascii_lowercase());
            push_base18_varint(&mut changes, idx << 1);
        } else {
            // Character replacement
            if idx > 0 && !in_replacement {
                result.push('-');
            }
            in_replacement = true;
            push_base18_varint(&mut changes, (idx << 1) | 1);
            push_base18_varint(&mut changes, ch as usize);
        }
    }
    if result.is_empty() {
        // Empty string not allowed, encode the replacement like a NUL byte just after
        // the original end of the string.
        in_replacement = false;
        result.push('0');
        push_base18_varint(&mut changes, (input.len() << 1) | 1);
        push_base18_varint(&mut changes, 0);
    }
    if !changes.is_empty() {
        if !in_replacement {
            result.push('-');
        }
        result.push_str(&changes);
    }
    result
}

/// Variable length integer encoding using only lowercase alphanumeric chars
fn push_base18_varint(buf: &mut String, mut value: usize) {
    loop {
        let base18_digit = value % 18;
        value /= 18;
        let continue_flag = if value == 0 {
            false
        } else {
            value -= 1;
            true
        };
        let base36_digit = if continue_flag {
            18 + base18_digit
        } else {
            base18_digit
        };
        buf.push(if base36_digit < 10 {
            ('0' as usize + base36_digit) as u8 as char
        } else {
            ('a' as usize + base36_digit - 10) as u8 as char
        });
        if !continue_flag {
            break;
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    fn base18(value: usize) -> String {
        let mut buffer = String::new();
        push_base18_varint(&mut buffer, value);
        buffer
    }

    #[test]
    fn encode_base18() {
        assert_eq!(base18(0), "0");
        assert_eq!(base18(1), "1");
        assert_eq!(base18(9), "9");
        assert_eq!(base18(10), "a");
        assert_eq!(base18(17), "h");
        assert_eq!(base18(18), "i0");
        assert_eq!(base18(35), "z0");
        assert_eq!(base18(36), "i1");
        assert_eq!(base18(99), "r4");
        assert_eq!(base18(999), "ri2");
        assert_eq!(base18(9999), "rwt0");
        assert_eq!(base18(18 * (18 + 1)), "ii0");
        assert_eq!(base18(18 * (18 + 1) - 1), "zh");
        assert_eq!(base18(18 * (18 * (18 + 1) + 1)), "iii0");
        assert_eq!(base18(18 * (18 * (18 + 1) + 1) - 1), "zzh");
    }

    #[test]
    fn encode_paths() {
        assert_eq!(path_encode("blah"), "blah");
        assert_eq!(path_encode("aaazzzz0909123248"), "aaazzzz0909123248");
        assert_eq!(path_encode("0"), "0");
        assert_eq!(path_encode("--bl----ah"), "bl-ah-1r13r19r1br1dr1fr1");
        assert_eq!(path_encode("bl----ah"), "bl-ah-5r17r19r1br1");
        assert_eq!(path_encode("blAh"), "blah-4");
        assert_eq!(path_encode("BLAH"), "blah-0246");
        assert_eq!(path_encode("b999lah"), "b999lah");
        assert_eq!(path_encode("foo-bar"), "foo-bar-7r1");
        assert_eq!(path_encode("foob-ar"), "foob-ar-9r1");
        assert_eq!(path_encode("foo::BAR!"), "foo-bar-7m29m2acehx0");
        assert_eq!(path_encode(".foo?"), "foo-1s19r2");
        assert_eq!(path_encode("blah-4"), "blah-4-9r1");
        assert_eq!(path_encode("blah-4-9r1"), "blah-4-9r1-9r1dr1");
        assert_eq!(
            path_encode("blah-4-9r1-9r1dr1"),
            "blah-4-9r1-9r1dr1-9r1dr1l0r1"
        );
        assert_eq!(path_encode(""), "0-10");
        assert_eq!(path_encode("\x00"), "0-1030");
        assert_eq!(path_encode("\x00\x00"), "0-103050");
        assert_eq!(path_encode("x\x00"), "x-30");
        assert_eq!(path_encode("X\x00"), "x-030");
        assert_eq!(path_encode("💀"), "0-1mpyk090");
        assert_eq!(path_encode("π"), "0-1oy150");
        assert_eq!(path_encode("oop💀"), "oop-7mpyk0");
        assert_eq!(path_encode("oopπ"), "oop-7oy1");
        assert_eq!(path_encode("0💀0"), "0-0-3mpyk0");
        assert_eq!(path_encode("0π0"), "0-0-3oy1");
        assert_eq!(path_encode("0💀💀0"), "0-0-3mpyk0bmpyk0");
        assert_eq!(path_encode("0ππ0"), "0-0-3oy17oy1");
    }

    #[test]
    fn storage_paths() {
        assert_eq!(
            StorageKey::Temp(1, 2)
                .to_path(Path::new("root"))
                .to_str()
                .unwrap(),
            "root/tmp/1-2.tmp"
        );
        assert_eq!(
            StorageKey::Temp(9999999, 4444444)
                .to_path(Path::new("root"))
                .to_str()
                .unwrap(),
            "root/tmp/9999999-4444444.tmp"
        );
        assert_eq!(
            StorageKey::Blob(
                "bla-a1-a2-a3:00112233445566778899aabbccddeeff"
                    .parse()
                    .unwrap()
            )
            .to_path(Path::new("root"))
            .to_str()
            .unwrap(),
            "root/blobs/bla-a1-a2-a3-00112233445566778899aabbccddeeff-7r1dr1j0r1p0m2.blob"
        );
        assert_eq!(
            StorageKey::Blob("sha256:00112233445566778899aabbccddeeff".parse().unwrap())
                .to_path(Path::new("root"))
                .to_str()
                .unwrap(),
            "root/blobs/sha256-00112233445566778899aabbccddeeff-dm2.blob"
        );
        assert_eq!(
            StorageKey::BlobPart(
                "bla-a1-a2-a3:00112233445566778899aabbccddeeff"
                    .parse()
                    .unwrap(),
                0x12345 .. 0xffff_ffff_ffff_ffff
            )
            .to_path(Path::new("root"))
            .to_str()
            .unwrap(),
            "root/parts/bla-a1-a2-a3-00112233445566778899aabbccddeeff-7r1dr1j0r1p0m2/12345-ffffffffffffffff.part"
        );
        assert_eq!(
            StorageKey::BlobPart(
                "bla-a1-a2-a3:00112233445566778899aabbccddeeff"
                    .parse()
                    .unwrap(),
                0..0
            )
            .to_path(Path::new("root"))
            .to_str()
            .unwrap(),
            "root/parts/bla-a1-a2-a3-00112233445566778899aabbccddeeff-7r1dr1j0r1p0m2/0-0.part"
        );
        assert_eq!(
            StorageKey::Manifest(
                "taco-extreme.example.org".parse().unwrap(),
                "foo/bar".parse().unwrap(),
                "latest".parse().unwrap(),
            )
            .to_path(Path::new("root"))
            .to_str()
            .unwrap(),
            "root/manifest/taco-extreme-example-org-9r1p0s1n1s1/foo-bar-7t1/latest.json",
        );
        assert_eq!(
            StorageKey::Manifest(
                "localhost:666".parse().unwrap(),
                "brrrr".parse().unwrap(),
                "sha256:00112233445566778899aabbccddeeff".parse().unwrap(),
            )
            .to_path(Path::new("root"))
            .to_str()
            .unwrap(),
            "root/manifest/localhost-666-j0m2/brrrr/sha256-00112233445566778899aabbccddeeff-dm2.json"
        );
        assert_eq!(
            StorageKey::Manifest(
                "gcr.io".parse().unwrap(),
                "library/emacs".parse().unwrap(),
                "taggy-mc-tagface.1".parse().unwrap(),
            )
            .to_path(Path::new("root"))
            .to_str()
            .unwrap(),
            "root/manifest/gcr-io-7s1/library-emacs-ft1/taggy-mc-tagface-1-br1hr1x0s1.json",
        );
        assert_eq!(
            StorageKey::Manifest(
                "registry-1.docker.io".parse().unwrap(),
                "library/busybox".parse().unwrap(),
                "1.2.400".parse().unwrap(),
            )
            .to_path(Path::new("root"))
            .to_str()
            .unwrap(),
            "root/manifest/registry-1-docker-io-hr1l0s1z0s1/library-busybox-ft1/1-2-400-3s17s1.json"
        );
    }
}
