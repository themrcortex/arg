export function emptyNodeServiceLogsMessage({ configured, state } = {}) {
  if (configured === false || state === 'not-configured') {
    return 'سرویس systemd موجود نیست یا پس از شروع ناموفق پاک‌سازی شده؛ جزئیات Job استقرار را بررسی کن و بعد دوباره تلاش کن.'
  }
  if (state && state !== 'active') {
    return `فعلاً لاگی ثبت نشده است؛ وضعیت واقعی سرویس: ${state}. جزئیات Job استقرار را هم بررسی کن.`
  }
  return 'هنوز خروجی‌ای در journald ثبت نشده است؛ پس از اولین اجرا یا درخواست سایت دوباره تازه‌سازی کن.'
}
