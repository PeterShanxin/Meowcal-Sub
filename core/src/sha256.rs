use std::io::Read;
use std::path::Path;

const BUFFER_SIZE: usize = 4 * 1024 * 1024;

pub fn digest_file(path: &Path) -> Result<[u8; 32], String> {
    let mut file = std::fs::File::open(path).map_err(|error| format!("HASH_OPEN: {error}"))?;
    digest_reader(&mut file)
}

pub fn digest_file_hex(path: &Path) -> Result<String, String> {
    digest_file(path).map(|digest| encode_hex(&digest))
}

pub fn digest_reader(reader: &mut impl Read) -> Result<[u8; 32], String> {
    platform::digest_reader(reader)
}

pub fn encode_hex(digest: &[u8; 32]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut encoded = String::with_capacity(64);
    for byte in digest {
        encoded.push(HEX[(byte >> 4) as usize] as char);
        encoded.push(HEX[(byte & 0x0f) as usize] as char);
    }
    encoded
}

#[cfg(target_os = "windows")]
mod platform {
    use super::{Read, BUFFER_SIZE};
    use windows::Security::Cryptography::Core::{HashAlgorithmNames, HashAlgorithmProvider};
    use windows::Security::Cryptography::CryptographicBuffer;

    pub fn digest_reader(reader: &mut impl Read) -> Result<[u8; 32], String> {
        let algorithm = HashAlgorithmNames::Sha256()
            .map_err(|error| format!("HASH_NATIVE_ALGORITHM: {error}"))?;
        let provider = HashAlgorithmProvider::OpenAlgorithm(&algorithm)
            .map_err(|error| format!("HASH_NATIVE_PROVIDER: {error}"))?;
        let hash = provider
            .CreateHash()
            .map_err(|error| format!("HASH_NATIVE_CREATE: {error}"))?;
        let mut bytes = vec![0_u8; BUFFER_SIZE];
        loop {
            let count = reader
                .read(&mut bytes)
                .map_err(|error| format!("HASH_READ: {error}"))?;
            if count == 0 {
                break;
            }
            let buffer = CryptographicBuffer::CreateFromByteArray(&bytes[..count])
                .map_err(|error| format!("HASH_NATIVE_BUFFER: {error}"))?;
            hash.Append(&buffer)
                .map_err(|error| format!("HASH_NATIVE_APPEND: {error}"))?;
        }
        let buffer = hash
            .GetValueAndReset()
            .map_err(|error| format!("HASH_NATIVE_FINALIZE: {error}"))?;
        let mut digest = windows::core::Array::<u8>::new();
        CryptographicBuffer::CopyToByteArray(&buffer, &mut digest)
            .map_err(|error| format!("HASH_NATIVE_RESULT: {error}"))?;
        digest.as_slice().try_into().map_err(|_| {
            format!(
                "HASH_NATIVE_LENGTH: expected 32 bytes, found {}",
                digest.len()
            )
        })
    }
}

#[cfg(not(target_os = "windows"))]
mod platform {
    use super::{Read, BUFFER_SIZE};
    use sha2::{Digest, Sha256};

    pub fn digest_reader(reader: &mut impl Read) -> Result<[u8; 32], String> {
        let mut hasher = Sha256::new();
        let mut bytes = vec![0_u8; BUFFER_SIZE];
        loop {
            let count = reader
                .read(&mut bytes)
                .map_err(|error| format!("HASH_READ: {error}"))?;
            if count == 0 {
                break;
            }
            hasher.update(&bytes[..count]);
        }
        Ok(hasher.finalize().into())
    }
}

#[cfg(test)]
fn software_digest_reader(reader: &mut impl Read) -> Result<[u8; 32], String> {
    use sha2::{Digest, Sha256};

    let mut hasher = Sha256::new();
    let mut bytes = vec![0_u8; BUFFER_SIZE];
    loop {
        let count = reader
            .read(&mut bytes)
            .map_err(|error| format!("HASH_READ: {error}"))?;
        if count == 0 {
            break;
        }
        hasher.update(&bytes[..count]);
    }
    Ok(hasher.finalize().into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;
    use std::time::Instant;

    #[test]
    fn native_and_software_hash_known_vectors() {
        for (input, expected) in [
            (
                b"".as_slice(),
                "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
            ),
            (
                b"abc".as_slice(),
                "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
            ),
        ] {
            let native = digest_reader(&mut Cursor::new(input)).unwrap();
            let software = software_digest_reader(&mut Cursor::new(input)).unwrap();
            assert_eq!(encode_hex(&native), expected);
            assert_eq!(native, software);
        }
    }

    #[test]
    fn incremental_hash_crosses_the_read_buffer_boundary() {
        let input = vec![0x5a; BUFFER_SIZE + 17];
        let native = digest_reader(&mut Cursor::new(&input)).unwrap();
        let software = software_digest_reader(&mut Cursor::new(&input)).unwrap();
        assert_eq!(native, software);
    }

    #[test]
    #[ignore = "set MEOWCAL_HASH_BENCHMARK_FILE to compare the real model file"]
    fn benchmark_native_and_software_on_same_file() {
        let path = std::env::var_os("MEOWCAL_HASH_BENCHMARK_FILE")
            .map(std::path::PathBuf::from)
            .expect("set MEOWCAL_HASH_BENCHMARK_FILE");
        let expected = crate::engine_manifest::EngineManifest::shipped()
            .unwrap()
            .model
            .artifact
            .sha256;

        let measure_native = || {
            let started = Instant::now();
            let digest = digest_file(&path).unwrap();
            (digest, started.elapsed())
        };
        let measure_software = || {
            let mut file = std::fs::File::open(&path).unwrap();
            let started = Instant::now();
            let digest = software_digest_reader(&mut file).unwrap();
            (digest, started.elapsed())
        };

        let (native_one, native_one_elapsed) = measure_native();
        let (software_one, software_one_elapsed) = measure_software();
        let (software_two, software_two_elapsed) = measure_software();
        let (native_two, native_two_elapsed) = measure_native();
        for digest in [native_one, software_one, software_two, native_two] {
            assert_eq!(encode_hex(&digest), expected);
        }
        eprintln!(
            "bytes={} digest={expected} native=[{native_one_elapsed:?}, {native_two_elapsed:?}] software=[{software_one_elapsed:?}, {software_two_elapsed:?}]",
            std::fs::metadata(&path).unwrap().len()
        );
    }
}
