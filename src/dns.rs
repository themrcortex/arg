use super::*;
use reqwest::{Client, Method};
use std::time::Duration;

const CF_API: &str = "https://api.cloudflare.com/client/v4";

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ConnectInput {
    name: String,
    zone: String,
    api_token: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct RecordInput {
    kind: String,
    name: String,
    content: String,
    #[serde(default = "default_ttl")]
    ttl: u32,
    #[serde(default)]
    proxied: bool,
    #[serde(default)]
    priority: Option<u16>,
    #[serde(default)]
    data: Option<Value>,
    #[serde(default)]
    confirmed: bool,
}
fn default_ttl() -> u32 {
    1
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct DeleteRecord {
    confirmed: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct DisconnectInput {
    confirmed: bool,
}

fn client() -> Result<Client, (StatusCode, Json<Value>)> {
    Client::builder()
        .timeout(Duration::from_secs(12))
        .connect_timeout(Duration::from_secs(5))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|_| {
            error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "کلاینت امن DNS ساخته نشد.",
            )
        })
}

fn valid_dns_name(value: &str) -> bool {
    let normalized = value.trim_end_matches('.');
    !normalized.is_empty()
        && normalized.len() <= 253
        && !normalized.contains(['/', '\\', ':', '@', '?', '#'])
        && normalized.split('.').all(|label| {
            !label.is_empty()
                && label.len() <= 63
                && label.as_bytes()[0].is_ascii_alphanumeric()
                && label.as_bytes()[label.len() - 1].is_ascii_alphanumeric()
                && label
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
        })
}
fn valid_zone(value: &str) -> bool {
    valid_dns_name(value) && !value.starts_with("*.")
}
fn valid_zone_id(value: &str) -> bool {
    (8..=64).contains(&value.len())
        && value
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() || byte == b'-')
}
fn token_valid(value: &str) -> bool {
    (20..=4096).contains(&value.len())
        && !value
            .bytes()
            .any(|byte| byte.is_ascii_whitespace() || byte.is_ascii_control())
}
fn secret_ref() -> String {
    format!("{:032x}", rand::random::<u128>())
}

async fn cf_request(
    method: Method,
    url: String,
    token: &str,
    body: Option<Value>,
) -> Result<Value, (StatusCode, Json<Value>)> {
    let client = client()?;
    let mut request = client
        .request(method, &url)
        .bearer_auth(token)
        .header(reqwest::header::ACCEPT, "application/json");
    if let Some(body) = body {
        request = request.json(&body);
    }
    let response = request.send().await.map_err(|problem| {
        let message = if problem.is_timeout() {
            "اتصال Cloudflare timeout شد؛ وضعیت zone تغییر نکرده یا پاسخ تأیید نشده است."
        } else {
            "Cloudflare API در دسترس نیست؛ DNS و دسترسی خروجی HTTPS را بررسی کن."
        };
        error(StatusCode::BAD_GATEWAY, message)
    })?;
    let status = response.status();
    let value = response
        .json::<Value>()
        .await
        .map_err(|_| error(StatusCode::BAD_GATEWAY, "پاسخ Cloudflare JSON معتبر نبود."))?;
    if !status.is_success() || value["success"] != true {
        let code = value["errors"]
            .as_array()
            .and_then(|items| items.first())
            .and_then(|item| item["code"].as_i64())
            .unwrap_or(status.as_u16() as i64);
        return Err(error(
            if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
                StatusCode::TOO_MANY_REQUESTS
            } else {
                StatusCode::BAD_GATEWAY
            },
            &format!("درخواست Cloudflare پذیرفته نشد (کد {code}). Token scope و zone را بررسی کن."),
        ));
    }
    Ok(value)
}

fn connection(
    db: &Connection,
    id: &str,
    owner: &str,
) -> Result<(String, String, String, String), (StatusCode, Json<Value>)> {
    let row=db.query_row("SELECT id,zone_id,zone_name,secret_ref FROM dns_connections WHERE id=?1 AND owner_server_uuid=?2 AND provider='cloudflare'",params![id,owner],|row|Ok((row.get::<_,String>(0)?,row.get::<_,String>(1)?,row.get::<_,String>(2)?,row.get::<_,String>(3)?))).optional().map_err(db_error)?;
    let Some(row) = row else {
        return Err(error(StatusCode::NOT_FOUND, "اتصال DNS پیدا نشد."));
    };
    if !valid_zone_id(&row.1) {
        return Err(error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "شناسهٔ zone ذخیره‌شده معتبر نیست.",
        ));
    }
    Ok(row)
}

fn records_url(zone_id: &str) -> String {
    format!("{CF_API}/zones/{zone_id}/dns_records")
}

fn record_payload(input: &RecordInput, zone: &str) -> Result<Value, (StatusCode, Json<Value>)> {
    let kind = input.kind.to_ascii_uppercase();
    if !["A", "AAAA", "CNAME", "MX", "TXT", "CAA", "SRV"].contains(&kind.as_str()) {
        return Err(error(
            StatusCode::BAD_REQUEST,
            "نوع DNS رکورد پشتیبانی نمی‌شود.",
        ));
    }
    let name = if input.name.trim() == "@" {
        zone.to_owned()
    } else {
        input.name.trim().trim_end_matches('.').to_ascii_lowercase()
    };
    if !valid_dns_name(&name) || !(name == zone || name.ends_with(&format!(".{zone}"))) {
        return Err(error(
            StatusCode::BAD_REQUEST,
            "نام رکورد باید داخل zone انتخاب‌شده باشد.",
        ));
    }
    if input.ttl != 1 && !(60..=86_400).contains(&input.ttl) {
        return Err(error(
            StatusCode::BAD_REQUEST,
            "TTL باید Automatic یا بین ۶۰ تا ۸۶۴۰۰ ثانیه باشد.",
        ));
    }
    if input.content.len() > 4096 || input.content.chars().any(char::is_control) {
        return Err(error(
            StatusCode::BAD_REQUEST,
            "مقدار رکورد معتبر یا کوتاه نیست.",
        ));
    }
    if input.proxied && !matches!(kind.as_str(), "A" | "AAAA" | "CNAME") {
        return Err(error(
            StatusCode::BAD_REQUEST,
            "Proxy فقط برای رکوردهای A، AAAA و CNAME قابل استفاده است.",
        ));
    }
    let content = input.content.trim();
    let mut payload = json!({"type":kind,"name":name,"ttl":input.ttl,"proxied":input.proxied});
    match kind.as_str() {
        "A" => {
            if content.parse::<std::net::Ipv4Addr>().is_err() {
                return Err(error(
                    StatusCode::BAD_REQUEST,
                    "مقدار A باید IPv4 معتبر باشد.",
                ));
            }
            payload["content"] = json!(content);
        }
        "AAAA" => {
            if content.parse::<std::net::Ipv6Addr>().is_err() {
                return Err(error(
                    StatusCode::BAD_REQUEST,
                    "مقدار AAAA باید IPv6 معتبر باشد.",
                ));
            }
            payload["content"] = json!(content);
        }
        "CNAME" => {
            if !valid_dns_name(content) {
                return Err(error(
                    StatusCode::BAD_REQUEST,
                    "مقدار CNAME باید hostname معتبر باشد.",
                ));
            }
            payload["content"] = json!(content.trim_end_matches('.').to_ascii_lowercase());
        }
        "MX" => {
            if !valid_dns_name(content) || input.priority.is_none() {
                return Err(error(
                    StatusCode::BAD_REQUEST,
                    "رکورد MX به hostname مقصد و priority نیاز دارد.",
                ));
            }
            payload["content"] = json!(content.trim_end_matches('.').to_ascii_lowercase());
            payload["priority"] = json!(input.priority);
        }
        "TXT" => {
            if content.is_empty() {
                return Err(error(
                    StatusCode::BAD_REQUEST,
                    "رکورد TXT خالی نمی‌تواند باشد.",
                ));
            }
            payload["content"] = json!(content);
        }
        "CAA" => {
            let Some(data) = input.data.as_ref() else {
                return Err(error(
                    StatusCode::BAD_REQUEST,
                    "CAA به flags، tag و value نیاز دارد.",
                ));
            };
            let flags = data["flags"].as_u64().filter(|value| *value <= 255);
            let tag = data["tag"]
                .as_str()
                .filter(|value| ["issue", "issuewild", "iodef"].contains(value));
            let value = data["value"].as_str().filter(|value| {
                !value.is_empty() && value.len() <= 255 && !value.chars().any(char::is_control)
            });
            if flags.is_none() || tag.is_none() || value.is_none() {
                return Err(error(
                    StatusCode::BAD_REQUEST,
                    "پارامترهای CAA معتبر نیستند.",
                ));
            }
            payload["content"] = json!(format!(
                "{} {} \"{}\"",
                flags.unwrap_or_default(),
                tag.unwrap_or_default(),
                value.unwrap_or_default()
            ));
            payload["data"] = json!({"flags":flags,"tag":tag,"value":value});
        }
        "SRV" => {
            let Some(data) = input.data.as_ref() else {
                return Err(error(
                    StatusCode::BAD_REQUEST,
                    "SRV به priority، weight، port و target نیاز دارد.",
                ));
            };
            let p = data["priority"].as_u64().filter(|value| *value <= 65535);
            let w = data["weight"].as_u64().filter(|value| *value <= 65535);
            let port = data["port"]
                .as_u64()
                .filter(|value| (1..=65535).contains(value));
            let target = data["target"]
                .as_str()
                .filter(|value| valid_dns_name(value));
            if p.is_none() || w.is_none() || port.is_none() || target.is_none() {
                return Err(error(
                    StatusCode::BAD_REQUEST,
                    "پارامترهای SRV معتبر نیستند.",
                ));
            }
            payload["data"] = json!({"priority":p,"weight":w,"port":port,"target":target.unwrap_or_default().trim_end_matches('.').to_ascii_lowercase()});
            payload["content"] = json!(format!(
                "{} {} {} {}",
                p.unwrap_or_default(),
                w.unwrap_or_default(),
                port.unwrap_or_default(),
                target.unwrap_or_default()
            ));
        }
        _ => unreachable!(),
    }
    Ok(payload)
}

pub(super) fn migrate(db: &Connection) -> rusqlite::Result<()> {
    db.execute_batch("CREATE TABLE IF NOT EXISTS dns_connections(id TEXT PRIMARY KEY,owner_server_uuid TEXT NOT NULL,provider TEXT NOT NULL,name TEXT NOT NULL,zone_id TEXT NOT NULL,zone_name TEXT NOT NULL,secret_ref TEXT NOT NULL,created_by TEXT NOT NULL,created_at INTEGER NOT NULL,updated_at INTEGER NOT NULL,UNIQUE(owner_server_uuid,provider,zone_id)); CREATE INDEX IF NOT EXISTS dns_connections_owner ON dns_connections(owner_server_uuid,created_at DESC);")
}

pub(super) async fn list_connections(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> ApiResult {
    let db = state.lock().unwrap();
    require_admin(&db, &headers)?;
    let owner = federation::local_server_uuid(&db).map_err(db_error)?;
    let mut statement=db.prepare("SELECT id,provider,name,zone_name,created_by,created_at,updated_at FROM dns_connections WHERE owner_server_uuid=?1 ORDER BY name COLLATE NOCASE").map_err(db_error)?;
    let rows=statement.query_map([owner],|row|Ok(json!({"id":row.get::<_,String>(0)?,"provider":row.get::<_,String>(1)?,"name":row.get::<_,String>(2)?,"zone_name":row.get::<_,String>(3)?,"created_by":row.get::<_,String>(4)?,"created_at":row.get::<_,i64>(5)?,"updated_at":row.get::<_,i64>(6)?,"credential_present":true}))).map_err(db_error)?.collect::<Result<Vec<_>,_>>().map_err(db_error)?;
    Ok(Json(
        json!({"connections":rows,"supported_providers":[{"id":"cloudflare","write_enabled":true}],"provider_billing":false}),
    ))
}

pub(super) async fn connect(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(input): Json<ConnectInput>,
) -> ApiResult {
    check_origin(&headers)?;
    let name = input.name.trim();
    let zone = input.zone.trim().trim_end_matches('.').to_ascii_lowercase();
    let token = input.api_token.trim();
    if name.is_empty()
        || name.len() > 80
        || name.chars().any(char::is_control)
        || !valid_zone(&zone)
        || !token_valid(token)
    {
        return Err(error(
            StatusCode::BAD_REQUEST,
            "نام اتصال، zone یا API Token معتبر نیست.",
        ));
    }
    {
        let db = state.lock().unwrap();
        require_admin(&db, &headers)?;
    }
    let request_url = reqwest::Url::parse(&format!("{CF_API}/zones")).map_err(|_| {
        error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Cloudflare API URL نامعتبر است.",
        )
    })?;
    let mut request_url = request_url;
    request_url
        .query_pairs_mut()
        .append_pair("name", &zone)
        .append_pair("per_page", "50");
    let response = cf_request(Method::GET, request_url.to_string(), token, None).await?;
    let matched = response["result"]
        .as_array()
        .and_then(|zones| {
            zones
                .iter()
                .find(|item| item["name"].as_str() == Some(&zone))
        })
        .cloned()
        .ok_or_else(|| {
            error(
                StatusCode::NOT_FOUND,
                "این zone در حساب Cloudflare پیدا نشد یا Token به آن دسترسی ندارد.",
            )
        })?;
    let zone_id = matched["id"].as_str().unwrap_or_default().to_owned();
    if !valid_zone_id(&zone_id) {
        return Err(error(
            StatusCode::BAD_GATEWAY,
            "Cloudflare شناسهٔ zone معتبر برنگرداند.",
        ));
    }
    let db = state.lock().unwrap();
    let actor = require_admin(&db, &headers)?.username;
    let owner = federation::local_server_uuid(&db).map_err(db_error)?;
    let id = secret_ref();
    dns_vault::store(&dns_vault::directory(), &id, token.as_bytes())
        .map_err(|problem| error(StatusCode::INTERNAL_SERVER_ERROR, &problem))?;
    let at = now();
    let insert=db.execute("INSERT INTO dns_connections(id,owner_server_uuid,provider,name,zone_id,zone_name,secret_ref,created_by,created_at,updated_at) VALUES(?1,?2,'cloudflare',?3,?4,?5,?1,?6,?7,?7)",params![id,owner,name,zone_id,zone,actor,at]);
    if let Err(problem) = insert {
        let _ = dns_vault::delete(&dns_vault::directory(), &id);
        return Err(
            if problem.sqlite_error_code() == Some(rusqlite::ErrorCode::ConstraintViolation) {
                error(StatusCode::CONFLICT, "این zone قبلاً به ARG وصل شده است.")
            } else {
                db_error(problem)
            },
        );
    }
    audit(
        &db,
        &actor,
        "dns.connection.create",
        &format!("zone={zone}; provider=cloudflare"),
    );
    Ok(Json(
        json!({"id":id,"provider":"cloudflare","name":name,"zone_name":zone,"credential_present":true,"tested":true,"secret_returned":false}),
    ))
}

pub(super) async fn disconnect(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(input): Json<DisconnectInput>,
) -> ApiResult {
    check_origin(&headers)?;
    if !input.confirmed {
        return Err(error(
            StatusCode::BAD_REQUEST,
            "قطع اتصال DNS نیازمند تأیید است.",
        ));
    }
    let db = state.lock().unwrap();
    let actor = require_admin(&db, &headers)?.username;
    let owner = federation::local_server_uuid(&db).map_err(db_error)?;
    let row = connection(&db, &id, &owner)?;
    dns_vault::delete(&dns_vault::directory(), &row.3)
        .map_err(|problem| error(StatusCode::INTERNAL_SERVER_ERROR, &problem))?;
    db.execute(
        "DELETE FROM dns_connections WHERE id=?1 AND owner_server_uuid=?2",
        params![id, owner],
    )
    .map_err(db_error)?;
    audit(
        &db,
        &actor,
        "dns.connection.disconnect",
        &format!("zone={}", row.2),
    );
    Ok(Json(
        json!({"disconnected":true,"zone":row.2,"provider_records_untouched":true}),
    ))
}

async fn connection_token(
    state: &AppState,
    headers: &HeaderMap,
    id: &str,
) -> Result<(String, String, String), (StatusCode, Json<Value>)> {
    let db_row = {
        let db = state.lock().unwrap();
        require_admin(&db, headers)?;
        let owner = federation::local_server_uuid(&db).map_err(db_error)?;
        connection(&db, id, &owner)?
    };
    let token = dns_vault::read(&dns_vault::directory(), &db_row.3).map_err(|_| {
        error(
            StatusCode::SERVICE_UNAVAILABLE,
            "Credential رمز‌شدهٔ این اتصال در دسترس نیست؛ اتصال را دوباره ثبت کن.",
        )
    })?;
    let token = String::from_utf8(token).map_err(|_| {
        error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Credential ذخیره‌شده UTF-8 نیست.",
        )
    })?;
    Ok((db_row.1, db_row.2, token))
}

pub(super) async fn list_records(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> ApiResult {
    let (zone_id, zone, token) = connection_token(&state, &headers, &id).await?;
    let response = cf_request(
        Method::GET,
        format!("{}?per_page=100&page=1", records_url(&zone_id)),
        &token,
        None,
    )
    .await?;
    let rows = response["result"].as_array().cloned().unwrap_or_default();
    Ok(Json(
        json!({"records":rows,"zone":zone,"pagination":{"page":1,"per_page":100,"total_pages":response["result_info"]["total_pages"]},"provider":"cloudflare"}),
    ))
}

pub(super) async fn create_record(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(input): Json<RecordInput>,
) -> ApiResult {
    check_origin(&headers)?;
    if !input.confirmed {
        return Err(error(
            StatusCode::BAD_REQUEST,
            "پیش از ایجاد رکورد تغییر را تأیید کن.",
        ));
    }
    let (zone_id, zone, token) = connection_token(&state, &headers, &id).await?;
    let payload = record_payload(&input, &zone)?;
    let response = cf_request(Method::POST, records_url(&zone_id), &token, Some(payload)).await?;
    let record = response["result"].clone();
    let db = state.lock().unwrap();
    let actor = require_admin(&db, &headers)?.username;
    audit(
        &db,
        &actor,
        "dns.record.create",
        &format!(
            "connection={id}; type={}; name={}",
            input.kind.to_ascii_uppercase(),
            input.name
        ),
    );
    Ok(Json(json!({"record":record,"zone":zone,"created":true})))
}

pub(super) async fn update_record(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((id, record_id)): Path<(String, String)>,
    Json(input): Json<RecordInput>,
) -> ApiResult {
    check_origin(&headers)?;
    if !input.confirmed || !valid_zone_id(&record_id) {
        return Err(error(
            StatusCode::BAD_REQUEST,
            "تأیید تغییر و شناسهٔ رکورد معتبر لازم است.",
        ));
    }
    let (zone_id, zone, token) = connection_token(&state, &headers, &id).await?;
    let payload = record_payload(&input, &zone)?;
    let response = cf_request(
        Method::PUT,
        format!("{}/{record_id}", records_url(&zone_id)),
        &token,
        Some(payload),
    )
    .await?;
    let record = response["result"].clone();
    let db = state.lock().unwrap();
    let actor = require_admin(&db, &headers)?.username;
    audit(
        &db,
        &actor,
        "dns.record.update",
        &format!("connection={id}; record={record_id}"),
    );
    Ok(Json(json!({"record":record,"updated":true})))
}

pub(super) async fn delete_record(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((id, record_id)): Path<(String, String)>,
    Json(input): Json<DeleteRecord>,
) -> ApiResult {
    check_origin(&headers)?;
    if !input.confirmed || !valid_zone_id(&record_id) {
        return Err(error(
            StatusCode::BAD_REQUEST,
            "حذف DNS نیازمند تأیید و شناسهٔ معتبر رکورد است.",
        ));
    }
    let (zone_id, zone, token) = connection_token(&state, &headers, &id).await?;
    let _ = cf_request(
        Method::DELETE,
        format!("{}/{record_id}", records_url(&zone_id)),
        &token,
        None,
    )
    .await?;
    let db = state.lock().unwrap();
    let actor = require_admin(&db, &headers)?.username;
    audit(
        &db,
        &actor,
        "dns.record.delete",
        &format!("connection={id}; zone={zone}; record={record_id}"),
    );
    Ok(Json(
        json!({"deleted":true,"zone":zone,"provider_data_removed":true}),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn input(kind: &str, name: &str, content: &str) -> RecordInput {
        RecordInput {
            kind: kind.into(),
            name: name.into(),
            content: content.into(),
            ttl: 1,
            proxied: false,
            priority: None,
            data: None,
            confirmed: true,
        }
    }
    #[test]
    fn validates_zone_scope_and_addresses() {
        assert!(valid_zone("example.com"));
        assert!(!valid_zone("https://example.com"));
        assert!(record_payload(&input("A", "@", "192.0.2.5"), "example.com").is_ok());
        assert!(record_payload(&input("A", "other.invalid", "192.0.2.5"), "example.com").is_err());
        assert!(record_payload(&input("AAAA", "@", "192.0.2.5"), "example.com").is_err());
        assert!(record_payload(&input("MX", "@", "mail.example.com"), "example.com").is_err());
    }
    #[test]
    fn refuses_proxy_on_mail_and_text_records() {
        let mut record = input("TXT", "@", "hello");
        record.proxied = true;
        assert!(record_payload(&record, "example.com").is_err());
    }
}
