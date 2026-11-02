use ring::{
    aead::{self, Aad, LessSafeKey, Nonce, UnboundKey},
    rand::{SecureRandom, SystemRandom},
};
use std::{
    ffi::CString,
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    os::{
        fd::AsRawFd,
        unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt},
    },
    path::{Path, PathBuf},
};

const MAGIC: &[u8] = b"ARG-VAULT-V1\0";
const KEY_NAME: &str = ".master-key";
const MASTER_KEY_BYTES: usize = 32;
const MASTER_KEY_MODE: u32 = 0o600;
const MAX_SECRET_BYTES: usize = 4 * 1024 * 1024;

fn safe_reference(reference: &str) -> bool {
    (1..=64).contains(&reference.len())
        && reference
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() || byte == b'-')
}

fn ensure_root_directory(directory: &Path) -> Result<(), String> {
    fs::create_dir_all(directory).map_err(|_| "ساخت مخزن امن ناموفق بود.".to_string())?;
    let metadata =
        fs::symlink_metadata(directory).map_err(|_| "بررسی مخزن امن ناموفق بود.".to_string())?;
    if !metadata.file_type().is_dir() {
        return Err("مسیر مخزن امن باید یک پوشهٔ واقعی باشد.".into());
    }
    let path = CString::new(directory.as_os_str().as_encoded_bytes())
        .map_err(|_| "مسیر مخزن امن معتبر نیست.".to_string())?;
    if unsafe { libc::chown(path.as_ptr(), 0, 0) } != 0 {
        return Err("مالکیت root برای مخزن امن تنظیم نشد.".into());
    }
    fs::set_permissions(directory, fs::Permissions::from_mode(0o700))
        .map_err(|_| "تنظیم دسترسی مخزن امن ناموفق بود.".to_string())?;
    let metadata = fs::symlink_metadata(directory)
        .map_err(|_| "بررسی دسترسی مخزن امن ناموفق بود.".to_string())?;
    if !metadata.file_type().is_dir()
        || metadata.uid() != 0
        || metadata.permissions().mode() & 0o777 != 0o700
    {
        return Err("مالکیت یا دسترسی مخزن امن معتبر نیست.".into());
    }
    Ok(())
}

fn read_key_file_contents(file: &mut File, expected_uid: u32) -> Result<[u8; 32], String> {
    let metadata = file
        .metadata()
        .map_err(|_| "بررسی کلید اصلی مخزن امن ناموفق بود.".to_string())?;
    if !metadata.file_type().is_file() {
        return Err("کلید اصلی مخزن امن باید فایل معمولی باشد.".into());
    }
    if metadata.len() != MASTER_KEY_BYTES as u64 {
        return Err("کلید اصلی مخزن امن ناقص است و باید دقیقاً ۳۲ بایت باشد؛ آن را فقط از پشتیبان همین مخزن بازیابی کن. بدون کلید قبلی، اطلاعات اتصال رمز‌شده قابل بازیابی نیستند.".into());
    }
    if metadata.uid() != expected_uid {
        return Err("مالکیت کلید اصلی مخزن امن معتبر نیست؛ مالک مورد انتظار root است. کلید را از پشتیبان مورد اعتماد بازیابی کن.".into());
    }
    if metadata.permissions().mode() & 0o777 != MASTER_KEY_MODE
        && unsafe { libc::fchmod(file.as_raw_fd(), MASTER_KEY_MODE) } != 0
    {
        return Err("دسترسی کلید اصلی مخزن امن اصلاح نشد؛ تنظیمات فایل را بررسی کن.".into());
    }
    let secured = file
        .metadata()
        .map_err(|_| "بررسی دسترسی کلید اصلی مخزن امن ناموفق بود.".to_string())?;
    if secured.uid() != expected_uid || secured.permissions().mode() & 0o777 != MASTER_KEY_MODE {
        return Err("مالکیت یا دسترسی کلید اصلی مخزن امن معتبر نیست.".into());
    }
    let mut key = [0_u8; MASTER_KEY_BYTES];
    file.read_exact(&mut key)
        .map_err(|_| "خواندن کلید اصلی مخزن امن ناموفق بود.".to_string())?;
    Ok(key)
}

fn read_key_file(path: &Path) -> Result<[u8; MASTER_KEY_BYTES], String> {
    let directory = path
        .parent()
        .ok_or_else(|| "مسیر مخزن کلید اصلی معتبر نیست.".to_string())?;
    ensure_root_directory(directory)?;
    let mut file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC | libc::O_NONBLOCK)
        .open(path)
        .map_err(|_| "کلید اصلی مخزن امن در دسترس نیست.".to_string())?;
    read_key_file_contents(&mut file, 0)
}

/// Publish a fully-written key without ever exposing a partially-written final file.
/// A hard link provides an atomic create-if-absent operation on the same filesystem.
fn create_key_file_atomically(
    directory: &Path,
    path: &Path,
    key: &[u8; MASTER_KEY_BYTES],
) -> Result<bool, String> {
    let temporary = directory.join(format!(
        ".{KEY_NAME}.tmp-{}-{}",
        std::process::id(),
        rand::random::<u64>()
    ));
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(MASTER_KEY_MODE)
        .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open(&temporary)
        .map_err(|_| "فایل موقت کلید اصلی مخزن امن ساخته نشد.".to_string())?;
    file.set_permissions(fs::Permissions::from_mode(MASTER_KEY_MODE))
        .map_err(|_| {
            let _ = fs::remove_file(&temporary);
            "تنظیم دسترسی کلید اصلی مخزن امن ناموفق بود.".to_string()
        })?;
    if file.write_all(key).and_then(|_| file.sync_all()).is_err() {
        drop(file);
        let _ = fs::remove_file(&temporary);
        return Err("ذخیرهٔ موقت کلید اصلی مخزن امن ناموفق بود.".into());
    }
    drop(file);

    let created = match fs::hard_link(&temporary, path) {
        Ok(()) => true,
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => false,
        Err(_) => {
            let _ = fs::remove_file(&temporary);
            return Err("انتشار اتمی کلید اصلی مخزن امن ناموفق بود.".into());
        }
    };
    fs::remove_file(&temporary)
        .map_err(|_| "پاک‌سازی فایل موقت کلید اصلی مخزن امن ناموفق بود.".to_string())?;
    File::open(directory)
        .and_then(|directory| directory.sync_all())
        .map_err(|_| "پایدارکردن کلید اصلی مخزن امن ناموفق بود.".to_string())?;
    Ok(created)
}

fn get_or_create_key(directory: &Path) -> Result<[u8; MASTER_KEY_BYTES], String> {
    ensure_root_directory(directory)?;
    let path = directory.join(KEY_NAME);
    match fs::symlink_metadata(&path) {
        Ok(_) => return read_key_file(&path),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(_) => return Err("بررسی کلید اصلی مخزن امن ناموفق بود.".into()),
    }
    let mut key = [0_u8; MASTER_KEY_BYTES];
    SystemRandom::new()
        .fill(&mut key)
        .map_err(|_| "تولید کلید اصلی مخزن امن ناموفق بود.".to_string())?;
    if create_key_file_atomically(directory, &path, &key)? {
        Ok(key)
    } else {
        read_key_file(&path)
    }
}

fn aad(reference: &str) -> Vec<u8> {
    format!("ARG secret vault v1:{reference}").into_bytes()
}

fn key_from_bytes(key: &[u8; 32]) -> Result<LessSafeKey, String> {
    UnboundKey::new(&aead::AES_256_GCM, key)
        .map(LessSafeKey::new)
        .map_err(|_| "ساخت کلید رمزنگاری مخزن ناموفق بود.".into())
}

fn seal(reference: &str, plaintext: &[u8], key: &[u8; 32]) -> Result<Vec<u8>, String> {
    if plaintext.is_empty() || plaintext.len() > MAX_SECRET_BYTES || !safe_reference(reference) {
        return Err("شناسه یا اندازهٔ مقدار محرمانه معتبر نیست.".into());
    }
    let mut nonce_bytes = [0_u8; 12];
    SystemRandom::new()
        .fill(&mut nonce_bytes)
        .map_err(|_| "تولید nonce مخزن امن ناموفق بود.".to_string())?;
    let nonce = Nonce::assume_unique_for_key(nonce_bytes);
    let mut payload = plaintext.to_vec();
    key_from_bytes(key)?
        .seal_in_place_append_tag(nonce, Aad::from(aad(reference)), &mut payload)
        .map_err(|_| "رمزگذاری مقدار محرمانه ناموفق بود.".to_string())?;
    let mut result = Vec::with_capacity(MAGIC.len() + nonce_bytes.len() + payload.len());
    result.extend_from_slice(MAGIC);
    result.extend_from_slice(&nonce_bytes);
    result.extend_from_slice(&payload);
    Ok(result)
}

fn open(reference: &str, ciphertext: &[u8], key: &[u8; 32]) -> Result<Vec<u8>, String> {
    if !safe_reference(reference)
        || !ciphertext.starts_with(MAGIC)
        || ciphertext.len() < MAGIC.len() + 12 + aead::AES_256_GCM.tag_len()
        || ciphertext.len() > MAX_SECRET_BYTES + MAGIC.len() + 12 + aead::AES_256_GCM.tag_len()
    {
        return Err("قالب مقدار محرمانهٔ رمز‌شده معتبر نیست.".into());
    }
    let nonce_start = MAGIC.len();
    let nonce_end = nonce_start + 12;
    let nonce_bytes: [u8; 12] = ciphertext[nonce_start..nonce_end]
        .try_into()
        .map_err(|_| "nonce مخزن امن معتبر نیست.".to_string())?;
    let nonce = Nonce::assume_unique_for_key(nonce_bytes);
    let mut payload = ciphertext[nonce_end..].to_vec();
    let plaintext = key_from_bytes(key)?
        .open_in_place(nonce, Aad::from(aad(reference)), &mut payload)
        .map_err(|_| "تأیید صحت مقدار محرمانه ناموفق بود.".to_string())?;
    Ok(plaintext.to_vec())
}

fn secret_path(directory: &Path, reference: &str) -> Result<PathBuf, String> {
    if !safe_reference(reference) {
        return Err("ارجاع مخزن امن معتبر نیست.".into());
    }
    Ok(directory.join(format!("{reference}.bin")))
}

pub(crate) fn contains(directory: &Path, reference: &str) -> Result<bool, String> {
    let path = secret_path(directory, reference)?;
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_file() => Ok(true),
        Ok(_) => Err("مسیر مقدار محرمانه فایل معمولی نیست.".into()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(_) => Err("بررسی مقدار محرمانه ناموفق بود.".into()),
    }
}

pub(crate) fn store(directory: &Path, reference: &str, plaintext: &[u8]) -> Result<(), String> {
    ensure_root_directory(directory)?;
    let key = get_or_create_key(directory)?;
    let encoded = seal(reference, plaintext, &key)?;
    let path = secret_path(directory, reference)?;
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
        .map_err(|_| "فایل موقت مخزن امن ساخته نشد.".to_string())?;
    if file
        .write_all(&encoded)
        .and_then(|_| file.sync_all())
        .is_err()
    {
        let _ = fs::remove_file(&temp);
        return Err("ذخیرهٔ رمز‌شدهٔ مقدار محرمانه ناموفق بود.".into());
    }
    fs::rename(&temp, &path).map_err(|_| {
        let _ = fs::remove_file(&temp);
        "جایگزینی اتمی مقدار محرمانه ناموفق بود.".to_string()
    })?;
    fs::set_permissions(&path, fs::Permissions::from_mode(0o600))
        .map_err(|_| "تنظیم دسترسی مقدار محرمانه ناموفق بود.".to_string())?;
    File::open(directory)
        .and_then(|directory| directory.sync_all())
        .map_err(|_| "پایدارکردن فایل محرمانه ناموفق بود.".to_string())
}

pub(crate) fn read(directory: &Path, reference: &str) -> Result<Vec<u8>, String> {
    ensure_root_directory(directory)?;
    let path = secret_path(directory, reference)?;
    let mut file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC | libc::O_NONBLOCK)
        .open(path)
        .map_err(|_| "مقدار محرمانه در مخزن پیدا نشد.".to_string())?;
    let length = validate_secret_file(&mut file, 0)?;
    let mut ciphertext = Vec::with_capacity(length as usize);
    file.read_to_end(&mut ciphertext)
        .map_err(|_| "خواندن مقدار محرمانه ناموفق بود.".to_string())?;
    let key = read_key_file(&directory.join(KEY_NAME))?;
    open(reference, &ciphertext, &key)
}

fn validate_secret_file(file: &mut File, expected_uid: u32) -> Result<u64, String> {
    let metadata = file
        .metadata()
        .map_err(|_| "بررسی فایل محرمانه ناموفق بود.".to_string())?;
    if !metadata.file_type().is_file()
        || metadata.uid() != expected_uid
        || metadata.len()
            > (MAX_SECRET_BYTES + MAGIC.len() + 12 + aead::AES_256_GCM.tag_len()) as u64
    {
        return Err("مالکیت، نوع یا اندازهٔ فایل رمز‌شدهٔ اعتبارنامه معتبر نیست.".into());
    }
    if metadata.permissions().mode() & 0o777 != MASTER_KEY_MODE
        && unsafe { libc::fchmod(file.as_raw_fd(), MASTER_KEY_MODE) } != 0
    {
        return Err("دسترسی فایل رمز‌شدهٔ اعتبارنامه اصلاح نشد؛ تنظیمات فایل را بررسی کن.".into());
    }
    let secured = file
        .metadata()
        .map_err(|_| "بررسی دسترسی فایل محرمانه ناموفق بود.".to_string())?;
    if secured.uid() != expected_uid || secured.permissions().mode() & 0o777 != MASTER_KEY_MODE {
        return Err("مالکیت یا دسترسی فایل رمز‌شدهٔ اعتبارنامه معتبر نیست.".into());
    }
    Ok(secured.len())
}

pub(crate) fn delete(directory: &Path, reference: &str) -> Result<(), String> {
    let path = secret_path(directory, reference)?;
    let metadata =
        fs::symlink_metadata(&path).map_err(|_| "مقدار محرمانه در مخزن پیدا نشد.".to_string())?;
    if !metadata.file_type().is_file()
        || metadata.uid() != 0
        || metadata.permissions().mode() & 0o777 != 0o600
    {
        return Err("مالکیت یا دسترسی فایل محرمانه معتبر نیست.".into());
    }
    fs::remove_file(path).map_err(|_| "حذف مقدار محرمانه ناموفق بود.".to_string())?;
    File::open(directory)
        .and_then(|directory| directory.sync_all())
        .map_err(|_| "پایدارکردن حذف فایل محرمانه ناموفق بود.".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_directory() -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "arg-secret-vault-test-{}-{}",
            std::process::id(),
            rand::random::<u64>()
        ));
        fs::create_dir(&path).unwrap();
        path
    }

    #[test]
    fn encrypted_payload_is_bound_to_reference_and_detects_tampering() {
        let key = [42_u8; 32];
        let plaintext = br#"{"token":"never-plain"}"#;
        let mut encoded = seal("a1b2", plaintext, &key).unwrap();
        assert!(!encoded.windows(11).any(|part| part == b"never-plain"));
        assert_eq!(open("a1b2", &encoded, &key).unwrap(), plaintext);
        assert!(open("a1b3", &encoded, &key).is_err());
        let last = encoded.len() - 1;
        encoded[last] ^= 0x80;
        assert!(open("a1b2", &encoded, &key).is_err());
    }

    #[test]
    fn references_and_plaintext_sizes_are_bounded() {
        let key = [7_u8; 32];
        assert!(seal("../escape", b"secret", &key).is_err());
        assert!(seal("abcd", b"", &key).is_err());
        assert!(seal("abcd", &vec![0_u8; MAX_SECRET_BYTES + 1], &key).is_err());
    }

    #[test]
    fn concurrent_master_key_creation_never_publishes_partial_or_replaces_a_key() {
        let directory = test_directory();
        let path = directory.join(KEY_NAME);
        let keys = (1_u8..=8)
            .map(|value| [value; MASTER_KEY_BYTES])
            .collect::<Vec<_>>();
        let barrier = std::sync::Arc::new(std::sync::Barrier::new(keys.len()));
        let workers = keys
            .iter()
            .copied()
            .map(|key| {
                let directory = directory.clone();
                let path = path.clone();
                let barrier = barrier.clone();
                std::thread::spawn(move || {
                    barrier.wait();
                    create_key_file_atomically(&directory, &path, &key).unwrap()
                })
            })
            .collect::<Vec<_>>();
        let created = workers
            .into_iter()
            .map(|worker| worker.join().unwrap())
            .filter(|created| *created)
            .count();

        assert_eq!(created, 1);
        let published = fs::read(&path).unwrap();
        assert_eq!(published.len(), MASTER_KEY_BYTES);
        assert_eq!(
            fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            MASTER_KEY_MODE
        );
        assert!(keys.iter().any(|key| published == key));
        assert_eq!(
            fs::read_dir(&directory).unwrap().count(),
            1,
            "temporary key files must be removed after publication"
        );
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn valid_master_key_with_restrictive_mode_is_repaired_without_changing_bytes() {
        let directory = test_directory();
        let path = directory.join(KEY_NAME);
        let original = [0x5a; MASTER_KEY_BYTES];
        fs::write(&path, original).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o400)).unwrap();
        let mut file = File::open(&path).unwrap();

        let key = read_key_file_contents(&mut file, fs::metadata(&path).unwrap().uid()).unwrap();

        assert_eq!(key, original);
        assert_eq!(
            fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn invalid_master_key_length_is_actionable_and_never_replaced() {
        let directory = test_directory();
        let path = directory.join(KEY_NAME);
        let original = [0x3c; MASTER_KEY_BYTES - 1];
        fs::write(&path, original).unwrap();
        let mut file = File::open(&path).unwrap();

        let error =
            read_key_file_contents(&mut file, fs::metadata(&path).unwrap().uid()).unwrap_err();

        assert!(error.contains("پشتیبان همین مخزن"));
        assert_eq!(fs::read(&path).unwrap(), original);
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn master_key_from_unexpected_owner_is_rejected_without_repair() {
        let directory = test_directory();
        let path = directory.join(KEY_NAME);
        let original = [0x7b; MASTER_KEY_BYTES];
        fs::write(&path, original).unwrap();
        let mut file = File::open(&path).unwrap();
        let wrong_owner = fs::metadata(&path).unwrap().uid().saturating_add(1);

        let error = read_key_file_contents(&mut file, wrong_owner).unwrap_err();

        assert!(error.contains("مالکیت کلید اصلی"));
        assert_eq!(fs::read(&path).unwrap(), original);
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn restrictive_encrypted_secret_permissions_are_repaired_without_changing_contents() {
        let directory = test_directory();
        let path = directory.join("abcdef.bin");
        let original = b"ciphertext fixture";
        fs::write(&path, original).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o400)).unwrap();
        let mut file = File::open(&path).unwrap();

        let length = validate_secret_file(&mut file, fs::metadata(&path).unwrap().uid()).unwrap();

        assert_eq!(length, original.len() as u64);
        assert_eq!(fs::read(&path).unwrap(), original);
        assert_eq!(
            fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            MASTER_KEY_MODE
        );
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn secret_file_size_and_owner_are_rejected_before_permissions_change() {
        let directory = test_directory();
        let path = directory.join("abcdef.bin");
        let original = b"ciphertext fixture";
        fs::write(&path, original).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o400)).unwrap();
        let mut file = File::open(&path).unwrap();

        let wrong_owner = fs::metadata(&path).unwrap().uid().saturating_add(1);
        assert!(validate_secret_file(&mut file, wrong_owner).is_err());
        assert_eq!(
            fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o400,
            "untrusted file ownership must not trigger a permission repair"
        );
        fs::remove_dir_all(directory).unwrap();
    }
}
