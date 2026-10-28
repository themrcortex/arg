use super::*;

pub(super) const WIDGETS: &[&str] = &["resources", "history", "operations", "mesh"];
const LEGACY_WIDGETS: &[&str] = &["resources", "history", "operations"];

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(super) struct Preferences {
    pub version: u32,
    pub preset: String,
    pub order: Vec<String>,
    pub hidden: Vec<String>,
}

impl Default for Preferences {
    fn default() -> Self {
        Self {
            version: 2,
            preset: "devops".into(),
            order: WIDGETS.iter().map(|value| (*value).to_owned()).collect(),
            hidden: Vec::new(),
        }
    }
}

pub(super) fn migrate(db: &Connection) -> rusqlite::Result<()> {
    db.execute_batch(
        "CREATE TABLE IF NOT EXISTS user_dashboard_preferences(
            user_id INTEGER PRIMARY KEY REFERENCES users(id) ON DELETE CASCADE,
            preferences_json TEXT NOT NULL,
            updated_at INTEGER NOT NULL
         );",
    )
}

fn valid(input: &Preferences) -> bool {
    if input.version != 2
        || !["minimal", "devops", "hosting", "fleet", "custom"].contains(&input.preset.as_str())
    {
        return false;
    }
    if input.order.len() != WIDGETS.len()
        || input
            .order
            .iter()
            .any(|widget| !WIDGETS.contains(&widget.as_str()))
    {
        return false;
    }
    let mut order = input.order.iter().map(String::as_str).collect::<Vec<_>>();
    order.sort_unstable();
    let mut expected = WIDGETS.to_vec();
    expected.sort_unstable();
    if order != expected {
        return false;
    }
    let mut hidden = input.hidden.iter().map(String::as_str).collect::<Vec<_>>();
    if hidden.iter().any(|widget| !WIDGETS.contains(widget)) {
        return false;
    }
    hidden.sort_unstable();
    hidden.dedup();
    hidden.len() == input.hidden.len()
}

fn normalize(input: Preferences) -> Option<Preferences> {
    if input.version == 1 {
        let mut legacy_order = input.order.iter().map(String::as_str).collect::<Vec<_>>();
        legacy_order.sort_unstable();
        let mut expected = LEGACY_WIDGETS.to_vec();
        expected.sort_unstable();
        let mut hidden = input.hidden.iter().map(String::as_str).collect::<Vec<_>>();
        hidden.sort_unstable();
        hidden.dedup();
        if legacy_order != expected
            || hidden.len() != input.hidden.len()
            || input.hidden.iter().any(|widget| !LEGACY_WIDGETS.contains(&widget.as_str()))
        {
            return None;
        }
        let mut upgraded = input;
        upgraded.version = 2;
        upgraded.order.push("mesh".into());
        return Some(upgraded);
    }
    Some(input)
}

pub(super) async fn read(State(state): State<AppState>, headers: HeaderMap) -> ApiResult {
    let db = state.lock().unwrap();
    let user = require_user(&db, &headers)?;
    let stored = db
        .query_row(
            "SELECT preferences_json,updated_at FROM user_dashboard_preferences WHERE user_id=?1",
            [user.id],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?)),
        )
        .optional()
        .map_err(db_error)?;
    let (preferences, updated_at) = match stored {
        Some((json, updated_at)) => {
            let parsed = serde_json::from_str::<Preferences>(&json)
                .ok()
                .and_then(normalize)
                .filter(valid)
                .unwrap_or_default();
            (parsed, Some(updated_at))
        }
        None => (Preferences::default(), None),
    };
    Ok(Json(
        json!({"preferences":preferences,"updated_at":updated_at,"available_widgets":WIDGETS}),
    ))
}

pub(super) async fn save(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(input): Json<Preferences>,
) -> ApiResult {
    check_origin(&headers)?;
    let input = normalize(input).ok_or_else(|| {
        error(
            StatusCode::BAD_REQUEST,
            "چیدمان داشبورد نامعتبر است؛ ترتیب باید شامل هر ویجت پشتیبانی‌شده دقیقاً یک‌بار باشد.",
        )
    })?;
    if !valid(&input) {
        return Err(error(
            StatusCode::BAD_REQUEST,
            "چیدمان داشبورد نامعتبر است؛ ترتیب باید شامل هر ویجت پشتیبانی‌شده دقیقاً یک‌بار باشد.",
        ));
    }
    let db = state.lock().unwrap();
    let user = require_user(&db, &headers)?;
    let serialized = serde_json::to_string(&input)
        .map_err(|_| error(StatusCode::BAD_REQUEST, "تنظیمات داشبورد قابل ذخیره نیست."))?;
    let updated_at = now();
    db.execute("INSERT INTO user_dashboard_preferences(user_id,preferences_json,updated_at) VALUES(?1,?2,?3) ON CONFLICT(user_id) DO UPDATE SET preferences_json=excluded.preferences_json,updated_at=excluded.updated_at", params![user.id, serialized, updated_at]).map_err(db_error)?;
    audit(
        &db,
        &user.username,
        "dashboard.preferences.update",
        &format!("preset={}; hidden={}", input.preset, input.hidden.join(",")),
    );
    Ok(Json(json!({"preferences":input,"updated_at":updated_at})))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_valid_and_contain_each_supported_widget_once() {
        let preferences = Preferences::default();
        assert!(valid(&preferences));
        assert_eq!(preferences.order.len(), WIDGETS.len());
    }

    #[test]
    fn rejects_missing_duplicate_unknown_and_duplicate_hidden_widgets() {
        let mut preferences = Preferences::default();
        preferences.order.pop();
        assert!(!valid(&preferences));
        preferences = Preferences::default();
        preferences.order[1] = "resources".into();
        assert!(!valid(&preferences));
        preferences = Preferences::default();
        preferences.order[1] = "custom-script".into();
        assert!(!valid(&preferences));
        preferences = Preferences::default();
        preferences.hidden = vec!["history".into(), "history".into()];
        assert!(!valid(&preferences));
    }

    #[test]
    fn permits_all_current_presets_and_only_known_visibility_choices() {
        for preset in ["minimal", "devops", "hosting", "fleet", "custom"] {
            let mut preferences = Preferences::default();
            preferences.preset = preset.into();
            preferences.hidden = vec!["history".into()];
            assert!(valid(&preferences));
        }
        let mut preferences = Preferences::default();
        preferences.hidden = vec!["unknown".into()];
        assert!(!valid(&preferences));
    }

    #[test]
    fn upgrades_saved_v1_preferences_without_losing_order_or_visibility() {
        let legacy = Preferences {
            version: 1,
            preset: "custom".into(),
            order: LEGACY_WIDGETS.iter().map(|widget| (*widget).to_owned()).collect(),
            hidden: vec!["operations".into()],
        };
        let upgraded = normalize(legacy).unwrap();
        assert_eq!(upgraded.version, 2);
        assert_eq!(upgraded.order.iter().map(String::as_str).collect::<Vec<_>>(), vec!["resources", "history", "operations", "mesh"]);
        assert_eq!(upgraded.hidden.iter().map(String::as_str).collect::<Vec<_>>(), vec!["operations"]);
        assert!(valid(&upgraded));
    }

    #[test]
    fn migration_is_idempotent_and_preferences_are_keyed_to_each_user() {
        let db = Connection::open_in_memory().unwrap();
        db.execute_batch("CREATE TABLE users(id INTEGER PRIMARY KEY,username TEXT NOT NULL); INSERT INTO users VALUES(1,'one'),(2,'two');").unwrap();
        migrate(&db).unwrap();
        let preferences = serde_json::to_string(&Preferences::default()).unwrap();
        db.execute("INSERT INTO user_dashboard_preferences(user_id,preferences_json,updated_at) VALUES(1,?1,10)", [&preferences]).unwrap();
        migrate(&db).unwrap();
        let first: String = db
            .query_row(
                "SELECT preferences_json FROM user_dashboard_preferences WHERE user_id=1",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let second: Option<String> = db
            .query_row(
                "SELECT preferences_json FROM user_dashboard_preferences WHERE user_id=2",
                [],
                |row| row.get(0),
            )
            .optional()
            .unwrap();
        assert_eq!(first, preferences);
        assert_eq!(second, None);
        assert_eq!(
            db.query_row(
                "SELECT count(*) FROM user_dashboard_preferences",
                [],
                |row| row.get::<_, i64>(0)
            )
            .unwrap(),
            1
        );
    }
}
