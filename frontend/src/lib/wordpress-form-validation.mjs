const usernamePattern = /^[A-Za-z0-9_.-]{1,60}$/
const controlPattern = /[\u0000-\u001F\u007F-\u009F]/u

function utf8Length(value) {
  return new TextEncoder().encode(value).length
}

function validEmail(value) {
  if (utf8Length(value) > 254) return false
  const separator = value.indexOf('@')
  if (separator < 1 || separator !== value.lastIndexOf('@')) return false
  const local = value.slice(0, separator)
  let domain = value.slice(separator + 1)
  if (utf8Length(local) > 64 || /\s/u.test(local)) return false
  if (domain.endsWith('.')) domain = domain.slice(0, -1)
  if (!domain || utf8Length(domain) > 253) return false
  return domain.split('.').every((label) =>
    label.length > 0 && label.length <= 63 && /^[A-Za-z0-9](?:[A-Za-z0-9-]*[A-Za-z0-9])?$/u.test(label),
  )
}

function validHost(value) {
  const host = value.replace(/\.+$/u, '')
  if (!host || host.length > 253) return false
  if (host.includes(':')) {
    try {
      return new URL(`http://[${host}]/`).hostname.length > 0
    } catch {
      return false
    }
  }
  if (/^[0-9.]+$/u.test(host)) {
    const parts = host.split('.')
    return parts.length === 4 && parts.every((part) => /^\d{1,3}$/u.test(part) && Number(part) <= 255)
  }
  return host.split('.').every((label) =>
    label.length > 0 && label.length <= 63 && /^[A-Za-z0-9](?:[A-Za-z0-9-]*[A-Za-z0-9])?$/u.test(label),
  )
}

export function validateWordPressCreation({ name, domain, version, siteTitle, username, email, password }) {
  if (!name.trim() || Array.from(name.trim()).length > 80 || controlPattern.test(name)) {
    return 'نام اپلیکیشن باید بین ۱ تا ۸۰ نویسه باشد.'
  }
  const host = domain.trim()
  if (/[\s*/@\\]/u.test(host) || !validHost(host)) {
    return 'دامنهٔ اصلی را به‌شکل hostname وارد کن؛ پروتکل و مسیر را ننویس.'
  }
  if (!/^(?:0|[1-9][0-9]{0,2})\.(?:0|[1-9][0-9]{0,2})\.(?:0|[1-9][0-9]{0,2})$/u.test(version.trim())) {
    return 'نسخهٔ هسته باید سه بخش عددی داشته باشد؛ مانند 6.8.3.'
  }
  const title = siteTitle.trim()
  if (!title || Array.from(title).length > 100 || controlPattern.test(title)) {
    return 'عنوان سایت را وارد کن؛ حداکثر ۱۰۰ نویسه مجاز است.'
  }
  if (!usernamePattern.test(username.trim())) {
    return 'نام کاربری مدیر فقط می‌تواند ۱ تا ۶۰ حرف انگلیسی، عدد، نقطه، خط تیره یا زیرخط باشد.'
  }
  if (!validEmail(email.trim())) {
    return 'ایمیل کامل و معتبر مدیر را وارد کن؛ مانند admin@example.com.'
  }
  // HTML maxlength/minlength use UTF-16 code units; Rust uses encode_utf16().count().
  if (password.length < 12 || password.length > 128 || controlPattern.test(password)) {
    return 'گذرواژه باید ۱۲ تا ۱۲۸ نویسه داشته باشد و نویسهٔ کنترلی نداشته باشد.'
  }
  return ''
}
