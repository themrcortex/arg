use ring::{
    aead::{self, Aad, LessSafeKey, Nonce, UnboundKey},
    rand::{SecureRandom, SystemRandom},
};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt},
    path::{Path, PathBuf},
};

const MAGIC: &[u8] = b"ARG-DNS-V1\0";
const KEY_FILE: &str = ".master-key";
const MAX_CREDENTIAL: usize = 16 * 1024;

pub(super) fn directory() -> PathBuf {
    std::env::var_os("ARG_DNS_SECRET_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/var/lib/arg/dns-secrets"))
}

fn valid_ref(reference: &str) -> bool {
    (1..=64).contains(&reference.len())
        && reference
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() || byte == b'-')
}

fn ensure_directory(directory: &Path) -> Result<(), String> {
    fs::create_dir_all(directory).map_err(|_| "ساخت پوشهٔ دسترسی DNS ناموفق بود.".to_string())?;
    let before =
        fs::symlink_metadata(directory).map_err(|_| "بررسی پوشهٔ DNS ناموفق بود.".to_string())?;
    if !before.file_type().is_dir() || before.uid() != unsafe { libc::geteuid() } {
        return Err("پوشهٔ DNS باید پوشهٔ واقعی و متعلق به کاربر ARG باشد.".into());
    }
    fs::set_permissions(directory, fs::Permissions::from_mode(0o700))
        .map_err(|_| "محدودکردن دسترسی پوشهٔ DNS ناموفق بود.".to_string())?;
    let metadata =
        fs::symlink_metadata(directory).map_err(|_| "بررسی پوشهٔ DNS ناموفق بود.".to_string())?;
    if !metadata.file_type().is_dir()
        || metadata.uid() != unsafe { libc::geteuid() }
        || metadata.permissions().mode() & 0o777 != 0o700
    {
        return Err("مالکیت یا مجوز پوشهٔ DNS معتبر نیست.".into());
    }
    Ok(())
}

fn read_key(path: &Path) -> Result<[u8; 32], String> {
    let mut file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open(path)
        .map_err(|_| "کلید رمزنگاری DNS در دسترس نیست.".to_string())?;
    let metadata = file
        .metadata()
        .map_err(|_| "بررسی کلید DNS ناموفق بود.".to_string())?;
    if !metadata.file_type().is_file()
        || metadata.uid() != unsafe { libc::geteuid() }
        || metadata.permissions().mode() & 0o777 != 0o600
        || metadata.len() != 32
    {
        return Err("مالکیت، مجوز یا اندازهٔ کلید DNS معتبر نیست.".into());
    }
    let mut key = [0u8; 32];
    file.read_exact(&mut key)
        .map_err(|_| "خواندن کلید DNS ناموفق بود.".to_string())?;
    Ok(key)
}

fn key_for(directory: &Path) -> Result<[u8; 32], String> {
    ensure_directory(directory)?;
    let path = directory.join(KEY_FILE);
    if path.exists() {
        return read_key(&path);
    }
    let mut key = [0u8; 32];
    SystemRandom::new()
        .fill(&mut key)
        .map_err(|_| "تولید کلید DNS ناموفق بود.".to_string())?;
    let mut file = match OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open(&path)
    {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => return read_key(&path),
        Err(_) => return Err("ساخت کلید DNS ناموفق بود.".into()),
    };
    file.write_all(&key)
        .and_then(|_| file.sync_all())
        .map_err(|_| "ثبت کلید DNS ناموفق بود.".to_string())?;
    File::open(directory)
        .and_then(|dir| dir.sync_all())
        .map_err(|_| "پایدارکردن کلید DNS ناموفق بود.".to_string())?;
    Ok(key)
}

fn key(key: &[u8; 32]) -> Result<LessSafeKey, String> {
    UnboundKey::new(&aead::AES_256_GCM, key)
        .map(LessSafeKey::new)
        .map_err(|_| "کلید رمزنگاری DNS معتبر نیست.".into())
}
fn aad(reference: &str) -> Vec<u8> {
    format!("ARG DNS credential v1:{reference}").into_bytes()
}
fn seal(reference: &str, plain: &[u8], key_bytes: &[u8; 32]) -> Result<Vec<u8>, String> {
    if !valid_ref(reference) || plain.is_empty() || plain.len() > MAX_CREDENTIAL {
        return Err("ارجاع یا اندازهٔ Credential DNS معتبر نیست.".into());
    }
    let mut nonce_bytes = [0u8; 12];
    SystemRandom::new()
        .fill(&mut nonce_bytes)
        .map_err(|_| "ساخت nonce DNS ناموفق بود.".to_string())?;
    let nonce = Nonce::assume_unique_for_key(nonce_bytes);
    let mut data = plain.to_vec();
    key(key_bytes)?
        .seal_in_place_append_tag(nonce, Aad::from(aad(reference)), &mut data)
        .map_err(|_| "رمزگذاری Credential DNS ناموفق بود.".to_string())?;
    let mut encoded = Vec::with_capacity(MAGIC.len() + nonce_bytes.len() + data.len());
    encoded.extend_from_slice(MAGIC);
    encoded.extend_from_slice(&nonce_bytes);
    encoded.extend_from_slice(&data);
    Ok(encoded)
}
fn open(reference: &str, encoded: &[u8], key_bytes: &[u8; 32]) -> Result<Vec<u8>, String> {
    let nonce_start = MAGIC.len();
    let nonce_end = nonce_start + 12;
    if !valid_ref(reference)
        || encoded.len() < nonce_end + aead::AES_256_GCM.tag_len()
        || !encoded.starts_with(MAGIC)
    {
        return Err("فرمت Credential DNS معتبر نیست.".into());
    }
    let nonce = Nonce::try_assume_unique_for_key(&encoded[nonce_start..nonce_end])
        .map_err(|_| "nonce Credential DNS معتبر نیست.".to_string())?;
    let mut data = encoded[nonce_end..].to_vec();
    let result = key(key_bytes)?
        .open_in_place(nonce, Aad::from(aad(reference)), &mut data)
        .map_err(|_| "صحت Credential DNS تأیید نشد.".to_string())?;
    Ok(result.to_vec())
}
fn file_path(directory: &Path, reference: &str) -> Result<PathBuf, String> {
    if !valid_ref(reference) {
        return Err("ارجاع Credential DNS معتبر نیست.".into());
    }
    Ok(directory.join(format!("{reference}.bin")))
}

pub(super) fn store(directory: &Path, reference: &str, plain: &[u8]) -> Result<(), String> {
    let key_bytes = key_for(directory)?;
    let encoded = seal(reference, plain, &key_bytes)?;
    let path = file_path(directory, reference)?;
    let temp = directory.join(format!(
        ".{reference}.tmp-{}-{}",
        std::process::id(),
        rand::random::<u64>()
    ));
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open(&temp)
        .map_err(|_| "فایل موقت Credential DNS ساخته نشد.".to_string())?;
    if file
        .write_all(&encoded)
        .and_then(|_| file.sync_all())
        .is_err()
    {
        drop(file);
        let _ = fs::remove_file(temp);
        return Err("ذخیرهٔ رمز‌شدهٔ Credential DNS ناموفق بود.".into());
    }
    fs::rename(&temp, &path).map_err(|_| {
        let _ = fs::remove_file(&temp);
        "جایگزینی Credential DNS ناموفق بود.".to_string()
    })?;
    File::open(directory)
        .and_then(|dir| dir.sync_all())
        .map_err(|_| "پایدارکردن Credential DNS ناموفق بود.".to_string())
}
pub(super) fn read(directory: &Path, reference: &str) -> Result<Vec<u8>, String> {
    let path = file_path(directory, reference)?;
    let mut file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open(path)
        .map_err(|_| "Credential DNS پیدا نشد.".to_string())?;
    let meta = file
        .metadata()
        .map_err(|_| "بررسی Credential DNS ناموفق بود.".to_string())?;
    if !meta.file_type().is_file()
        || meta.uid() != unsafe { libc::geteuid() }
        || meta.permissions().mode() & 0o777 != 0o600
        || meta.len() as usize > MAX_CREDENTIAL + MAGIC.len() + 12 + aead::AES_256_GCM.tag_len()
    {
        return Err("مالکیت، مجوز یا اندازهٔ Credential DNS معتبر نیست.".into());
    }
    let mut encoded = Vec::new();
    file.read_to_end(&mut encoded)
        .map_err(|_| "خواندن Credential DNS ناموفق بود.".to_string())?;
    open(reference, &encoded, &read_key(&directory.join(KEY_FILE))?)
}
pub(super) fn delete(directory: &Path, reference: &str) -> Result<(), String> {
    let path = file_path(directory, reference)?;
    let meta = fs::symlink_metadata(&path).map_err(|_| "Credential DNS پیدا نشد.".to_string())?;
    if !meta.file_type().is_file()
        || meta.uid() != unsafe { libc::geteuid() }
        || meta.permissions().mode() & 0o777 != 0o600
    {
        return Err("مالکیت یا مجوز Credential DNS معتبر نیست.".into());
    }
    fs::remove_file(path).map_err(|_| "حذف Credential DNS ناموفق بود.".to_string())?;
    File::open(directory)
        .and_then(|dir| dir.sync_all())
        .map_err(|_| "پایدارکردن حذف Credential DNS ناموفق بود.".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn encrypts_and_binds_secret_to_reference() {
        let key = [9u8; 32];
        let mut encoded = seal("ab12", b"token-value", &key).unwrap();
        assert_eq!(open("ab12", &encoded, &key).unwrap(), b"token-value");
        assert!(open("ab13", &encoded, &key).is_err());
        let index = encoded.len() - 1;
        encoded[index] ^= 1;
        assert!(open("ab12", &encoded, &key).is_err());
    }
    #[test]
    fn safe_refs_only() {
        assert!(valid_ref("1234-abcd"));
        assert!(!valid_ref("../../tmp"));
        assert!(!valid_ref(""));
    }
    #[test]
    fn file_store_round_trip_uses_private_permissions() {
        let dir = std::env::temp_dir().join(format!("arg-dns-vault-{}", rand::random::<u64>()));
        store(&dir, "a1b2", b"test-token").unwrap();
        assert_eq!(read(&dir, "a1b2").unwrap(), b"test-token");
        assert_eq!(
            fs::metadata(&dir).unwrap().permissions().mode() & 0o777,
            0o700
        );
        assert_eq!(
            fs::metadata(dir.join("a1b2.bin"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
        delete(&dir, "a1b2").unwrap();
        let _ = fs::remove_dir_all(dir);
    }
}
