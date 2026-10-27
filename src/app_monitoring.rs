use super::*;
use std::time::Duration;

const SAMPLE_INTERVAL: Duration = Duration::from_secs(60);
const SAMPLE_BATCH: i64 = 20;
const SAMPLE_RETENTION_SECONDS: i64 = 30 * 24 * 60 * 60;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct RuleInput {
    #[serde(default)]
    cpu_percent: Option<f64>,
    #[serde(default)]
    memory_bytes: Option<u64>,
    #[serde(default = "default_inactive_alert")]
    alert_when_inactive: bool,
    #[serde(default = "default_enabled")]
    enabled: bool,
}

fn default_inactive_alert() -> bool {
    true
}
fn default_enabled() -> bool {
    true
}

fn valid_rules(input: &RuleInput) -> bool {
    input
        .cpu_percent
        .is_none_or(|value| value.is_finite() && (1.0..=10_000.0).contains(&value))
        && input
            .memory_bytes
            .is_none_or(|value| value > 0 && value <= i64::MAX as u64)
}

pub(super) fn migrate(db: &Connection) -> rusqlite::Result<()> {
    db.execute_batch(
        "CREATE TABLE IF NOT EXISTS application_monitoring_samples(
            id INTEGER PRIMARY KEY,
            application_id TEXT NOT NULL REFERENCES applications(id) ON DELETE CASCADE,
            at INTEGER NOT NULL,
            systemd_unit TEXT NOT NULL,
            service_state TEXT NOT NULL,
            cpu_usage_ns INTEGER,
            cpu_percent REAL,
            memory_bytes INTEGER,
            volume_bytes INTEGER,
            volume_status TEXT NOT NULL DEFAULT 'unknown',
            error_code TEXT
         );
         CREATE INDEX IF NOT EXISTS app_monitoring_sample_time ON application_monitoring_samples(application_id,at DESC);
         CREATE TABLE IF NOT EXISTS application_monitoring_rules(
            application_id TEXT PRIMARY KEY REFERENCES applications(id) ON DELETE CASCADE,
            cpu_percent REAL,
            memory_bytes INTEGER,
            alert_when_inactive INTEGER NOT NULL DEFAULT 1,
            enabled INTEGER NOT NULL DEFAULT 1,
            updated_by TEXT NOT NULL,
            updated_at INTEGER NOT NULL
         );
         CREATE TABLE IF NOT EXISTS application_monitoring_alerts(
            id INTEGER PRIMARY KEY,
            application_id TEXT NOT NULL REFERENCES applications(id) ON DELETE CASCADE,
            metric TEXT NOT NULL,
            state TEXT NOT NULL,
            current_value REAL,
            threshold REAL,
            first_seen INTEGER NOT NULL,
            last_seen INTEGER NOT NULL,
            resolved_at INTEGER,
            UNIQUE(application_id,metric)
         );
         CREATE INDEX IF NOT EXISTS app_monitoring_open_alerts ON application_monitoring_alerts(application_id,state,last_seen DESC);",
    )
}

pub(super) async fn read(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(app_id): Path<String>,
) -> ApiResult {
    let db = state.lock().unwrap();
    require_admin(&db, &headers)?;
    let owner = federation::local_server_uuid(&db).map_err(db_error)?;
    let exists: bool = db.query_row(
        "SELECT EXISTS(SELECT 1 FROM applications WHERE id=?1 AND owner_server_uuid=?2 AND deleted_at IS NULL)",
        params![app_id, owner],
        |row| row.get(0),
    ).map_err(db_error)?;
    if !exists {
        return Err(error(StatusCode::NOT_FOUND, "اپلیکیشن پیدا نشد."));
    }

    let latest = db.query_row(
        "SELECT at,systemd_unit,service_state,cpu_percent,memory_bytes,volume_bytes,volume_status,error_code FROM application_monitoring_samples WHERE application_id=?1 ORDER BY at DESC,id DESC LIMIT 1",
        [&app_id],
        |row| Ok(json!({"measured_at":row.get::<_,i64>(0)?,"systemd_unit":row.get::<_,String>(1)?,"service_state":row.get::<_,String>(2)?,"cpu_percent":row.get::<_,Option<f64>>(3)?,"memory_bytes":row.get::<_,Option<i64>>(4)?,"volume_bytes":row.get::<_,Option<i64>>(5)?,"volume_status":row.get::<_,String>(6)?,"error_code":row.get::<_,Option<String>>(7)?})),
    ).optional().map_err(db_error)?;
    let mut statement = db.prepare(
        "SELECT at,service_state,cpu_percent,memory_bytes,volume_bytes,volume_status FROM application_monitoring_samples WHERE application_id=?1 AND at>=?2 ORDER BY at DESC LIMIT 240",
    ).map_err(db_error)?;
    let samples = statement.query_map(params![app_id, now() - 24 * 60 * 60], |row| {
        Ok(json!({"at":row.get::<_,i64>(0)?,"service_state":row.get::<_,String>(1)?,"cpu_percent":row.get::<_,Option<f64>>(2)?,"memory_bytes":row.get::<_,Option<i64>>(3)?,"volume_bytes":row.get::<_,Option<i64>>(4)?,"volume_status":row.get::<_,String>(5)?}))
    }).map_err(db_error)?.filter_map(Result::ok).collect::<Vec<_>>();
    let rules = read_rules(&db, &app_id).map_err(db_error)?;
    let mut statement = db.prepare(
        "SELECT metric,state,current_value,threshold,first_seen,last_seen,resolved_at FROM application_monitoring_alerts WHERE application_id=?1 ORDER BY last_seen DESC LIMIT 50",
    ).map_err(db_error)?;
    let alerts = statement.query_map([app_id], |row| {
        Ok(json!({"metric":row.get::<_,String>(0)?,"state":row.get::<_,String>(1)?,"current_value":row.get::<_,Option<f64>>(2)?,"threshold":row.get::<_,Option<f64>>(3)?,"first_seen":row.get::<_,i64>(4)?,"last_seen":row.get::<_,i64>(5)?,"resolved_at":row.get::<_,Option<i64>>(6)?}))
    }).map_err(db_error)?.filter_map(Result::ok).collect::<Vec<_>>();
    Ok(Json(
        json!({"latest":latest,"samples":samples,"rules":rules,"alerts":alerts,"sampling":{"interval_seconds":60,"retention_days":30,"scope":"managed-systemd-services","network_bytes":null,"http_latency_ms":null}}),
    ))
}

pub(super) async fn save_rules(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(app_id): Path<String>,
    Json(input): Json<RuleInput>,
) -> ApiResult {
    check_origin(&headers)?;
    if !valid_rules(&input) {
        return Err(error(
            StatusCode::BAD_REQUEST,
            "آستانهٔ CPU یا حافظه معتبر نیست.",
        ));
    }
    let db = state.lock().unwrap();
    let actor = require_admin(&db, &headers)?.username;
    let owner = federation::local_server_uuid(&db).map_err(db_error)?;
    let exists: bool = db.query_row(
        "SELECT EXISTS(SELECT 1 FROM applications WHERE id=?1 AND owner_server_uuid=?2 AND deleted_at IS NULL)",
        params![app_id, owner],
        |row| row.get(0),
    ).map_err(db_error)?;
    if !exists {
        return Err(error(StatusCode::NOT_FOUND, "اپلیکیشن پیدا نشد."));
    }
    db.execute(
        "INSERT INTO application_monitoring_rules(application_id,cpu_percent,memory_bytes,alert_when_inactive,enabled,updated_by,updated_at) VALUES(?1,?2,?3,?4,?5,?6,?7) ON CONFLICT(application_id) DO UPDATE SET cpu_percent=excluded.cpu_percent,memory_bytes=excluded.memory_bytes,alert_when_inactive=excluded.alert_when_inactive,enabled=excluded.enabled,updated_by=excluded.updated_by,updated_at=excluded.updated_at",
        params![app_id,input.cpu_percent,input.memory_bytes.map(|v|v as i64),input.alert_when_inactive,input.enabled,actor,now()],
    ).map_err(db_error)?;
    audit(
        &db,
        &actor,
        "application.monitoring.rules.update",
        &format!("app={app_id}"),
    );
    Ok(Json(
        json!({"rules":rules_value(input.cpu_percent,input.memory_bytes.map(|v|v as i64),input.alert_when_inactive,input.enabled)}),
    ))
}

fn read_rules(db: &Connection, app_id: &str) -> rusqlite::Result<Value> {
    let row = db.query_row(
        "SELECT cpu_percent,memory_bytes,alert_when_inactive,enabled FROM application_monitoring_rules WHERE application_id=?1",
        [app_id],
        |row| Ok((row.get::<_,Option<f64>>(0)?,row.get::<_,Option<i64>>(1)?,row.get::<_,bool>(2)?,row.get::<_,bool>(3)?)),
    ).optional()?;
    Ok(match row {
        Some((cpu, memory, inactive, enabled)) => rules_value(cpu, memory, inactive, enabled),
        None => rules_value(None, None, true, true),
    })
}

fn rules_value(cpu: Option<f64>, memory: Option<i64>, inactive: bool, enabled: bool) -> Value {
    json!({"cpu_percent":cpu,"memory_bytes":memory,"alert_when_inactive":inactive,"enabled":enabled})
}

pub(super) async fn run(state: AppState) {
    loop {
        tokio::time::sleep(SAMPLE_INTERVAL).await;
        collect_batch(&state).await;
    }
}

async fn collect_batch(state: &AppState) {
    let candidates = {
        let db = state.lock().unwrap();
        db.prepare("SELECT a.id,s.systemd_unit,s.desired_state FROM applications a JOIN application_services s ON s.application_id=a.id WHERE a.deleted_at IS NULL AND s.systemd_unit IS NOT NULL AND s.service_key IN ('node-web','laravel-web') ORDER BY COALESCE((SELECT MAX(at) FROM application_monitoring_samples m WHERE m.application_id=a.id),0) ASC LIMIT ?1")
            .and_then(|mut stmt| stmt.query_map([SAMPLE_BATCH], |row| Ok((row.get::<_,String>(0)?,row.get::<_,String>(1)?,row.get::<_,String>(2)?)))?.collect::<Result<Vec<_>,_>>())
            .unwrap_or_default()
    };
    for (app_id, unit, desired_state) in candidates {
        let response =
            privileged_request_async(json!({"action":"service-resources","unit":unit})).await;
        let data = match response {
            Ok(value) if value["ok"] == true => value["data"].clone(),
            _ => json!({"error":"SERVICE_METRICS_UNAVAILABLE"}),
        };
        let at = now();
        let cpu_ns = data["cpu_usage_ns"]
            .as_u64()
            .map(|value| value.min(i64::MAX as u64) as i64);
        let memory = data["memory_current_bytes"]
            .as_u64()
            .map(|value| value.min(i64::MAX as u64) as i64);
        let service_state = data["active_state"]
            .as_str()
            .unwrap_or("unknown")
            .to_owned();
        let (previous_at, previous_cpu) = {
            let db = state.lock().unwrap();
            db.query_row("SELECT at,cpu_usage_ns FROM application_monitoring_samples WHERE application_id=?1 ORDER BY at DESC,id DESC LIMIT 1",[&app_id],|row|Ok((row.get::<_,i64>(0)?,row.get::<_,Option<i64>>(1)?))).optional().unwrap_or(None).unwrap_or((0,None))
        };
        let cpu_percent = cpu_percent_delta(previous_cpu, cpu_ns, at - previous_at);
        let (volume_bytes, volume_status) = volume_usage(state, &app_id, at);
        let error_code = data["error_code"]
            .as_str()
            .or(data["measurement_error"].as_str())
            .map(str::to_owned);
        {
            let db = state.lock().unwrap();
            let _ = db.execute("INSERT INTO application_monitoring_samples(application_id,at,systemd_unit,service_state,cpu_usage_ns,cpu_percent,memory_bytes,volume_bytes,volume_status,error_code) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)",params![app_id,at,unit,service_state,cpu_ns,cpu_percent,memory,volume_bytes,volume_status,error_code]);
            let _ = db.execute("UPDATE applications SET observed_state=?1,observed_at=?2 WHERE id=?3 AND deleted_at IS NULL",params![if service_state=="active" {"running"} else if service_state=="inactive" || service_state=="failed" {"stopped"} else {"unknown"},at,app_id]);
            update_alerts(
                &db,
                &app_id,
                &desired_state,
                &service_state,
                cpu_percent,
                memory,
                at,
            );
            if at % 86_400 < SAMPLE_INTERVAL.as_secs() as i64 {
                let _ = db.execute(
                    "DELETE FROM application_monitoring_samples WHERE at<?1",
                    [at - SAMPLE_RETENTION_SECONDS],
                );
            }
        }
    }
}

fn cpu_percent_delta(
    previous: Option<i64>,
    current: Option<i64>,
    elapsed_seconds: i64,
) -> Option<f64> {
    match (previous, current) {
        (Some(previous), Some(current))
            if previous >= 0 && current >= previous && (1..=600).contains(&elapsed_seconds) =>
        {
            Some((current - previous) as f64 / (elapsed_seconds as f64 * 1_000_000_000.0) * 100.0)
        }
        _ => None,
    }
}

fn volume_usage(state: &AppState, app_id: &str, at: i64) -> (Option<i64>, &'static str) {
    let db = state.lock().unwrap();
    let result: rusqlite::Result<(i64,i64,i64)> = db.query_row(
        "SELECT COUNT(*),COUNT(usage_bytes),COALESCE(SUM(usage_bytes),0) FROM application_volumes WHERE application_id=?1",
        [app_id],
        |row| Ok((row.get(0)?,row.get(1)?,row.get(2)?)),
    );
    let Ok((total, measured, bytes)) = result else {
        return (None, "unknown");
    };
    if total == 0 {
        return (Some(0), "measured");
    }
    let fresh: i64 = db.query_row("SELECT COUNT(*) FROM application_volumes WHERE application_id=?1 AND usage_bytes IS NOT NULL AND measured_at>=?2",params![app_id,at-7*86400],|row|row.get(0)).unwrap_or(0);
    if measured == total && fresh == total {
        (Some(bytes), "fresh")
    } else if measured > 0 {
        (Some(bytes), "stale")
    } else {
        (None, "not_measured")
    }
}

fn update_alerts(
    db: &Connection,
    app_id: &str,
    desired: &str,
    service: &str,
    cpu: Option<f64>,
    memory: Option<i64>,
    at: i64,
) {
    let rule: Option<(Option<f64>,Option<i64>,bool,bool)> = db.query_row("SELECT cpu_percent,memory_bytes,alert_when_inactive,enabled FROM application_monitoring_rules WHERE application_id=?1",[app_id],|row|Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get(3)?))).optional().ok().flatten();
    let (cpu_limit, memory_limit, inactive, enabled) = rule.unwrap_or((None, None, true, true));
    let service_inactive = if !enabled || !inactive || desired != "running" {
        Some(false)
    } else if matches!(service, "active" | "inactive" | "failed") {
        Some(service != "active")
    } else {
        None
    };
    let cpu_high = if !enabled || cpu_limit.is_none() {
        Some(false)
    } else {
        cpu.zip(cpu_limit).map(|(value, limit)| value >= limit)
    };
    let memory_high = if !enabled || memory_limit.is_none() {
        Some(false)
    } else {
        memory
            .zip(memory_limit)
            .map(|(value, limit)| value >= limit)
    };
    let checks = [
        ("service_inactive", service_inactive, None, None),
        ("cpu_high", cpu_high, cpu, cpu_limit),
        (
            "memory_high",
            memory_high,
            memory.map(|value| value as f64),
            memory_limit.map(|value| value as f64),
        ),
    ];
    for (metric, condition, value, threshold) in checks {
        let Some(open) = condition else { continue };
        if !open {
            let existing_open: bool = db.query_row(
                "SELECT EXISTS(SELECT 1 FROM application_monitoring_alerts WHERE application_id=?1 AND metric=?2 AND state='open')",
                params![app_id,metric],
                |row| row.get(0),
            ).unwrap_or(false);
            if !existing_open {
                continue;
            }
        }
        let state = if open { "open" } else { "resolved" };
        let resolved_at = if open { None } else { Some(at) };
        let _ = db.execute("INSERT INTO application_monitoring_alerts(application_id,metric,state,current_value,threshold,first_seen,last_seen,resolved_at) VALUES(?1,?2,?3,?4,?5,?6,?6,?7) ON CONFLICT(application_id,metric) DO UPDATE SET state=excluded.state,current_value=excluded.current_value,threshold=excluded.threshold,last_seen=excluded.last_seen,resolved_at=CASE WHEN excluded.state='open' THEN NULL ELSE COALESCE(application_monitoring_alerts.resolved_at,excluded.resolved_at) END,first_seen=CASE WHEN excluded.state='open' AND application_monitoring_alerts.state<>'open' THEN excluded.first_seen ELSE application_monitoring_alerts.first_seen END",params![app_id,metric,state,value,threshold,at,resolved_at]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rule_thresholds_are_bounded_and_finite() {
        assert!(valid_rules(&RuleInput {
            cpu_percent: Some(200.0),
            memory_bytes: Some(1024),
            alert_when_inactive: true,
            enabled: true
        }));
        assert!(!valid_rules(&RuleInput {
            cpu_percent: Some(f64::NAN),
            memory_bytes: None,
            alert_when_inactive: true,
            enabled: true
        }));
        assert!(!valid_rules(&RuleInput {
            cpu_percent: Some(0.0),
            memory_bytes: None,
            alert_when_inactive: true,
            enabled: true
        }));
    }

    #[test]
    fn migration_is_idempotent_and_preserves_rows() {
        let db = Connection::open_in_memory().unwrap();
        db.execute_batch("CREATE TABLE applications(id TEXT PRIMARY KEY); INSERT INTO applications(id) VALUES('app');").unwrap();
        migrate(&db).unwrap();
        migrate(&db).unwrap();
        db.execute("INSERT INTO application_monitoring_samples(application_id,at,systemd_unit,service_state) VALUES('app',1,'arg-node-app.service','inactive')",[]).unwrap();
        let found: i64 = db
            .query_row(
                "SELECT COUNT(*) FROM application_monitoring_samples WHERE application_id='app'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(found, 1);
    }

    #[test]
    fn cpu_counter_requires_a_forward_reliable_interval() {
        assert_eq!(
            cpu_percent_delta(Some(1_000_000_000), Some(6_000_000_000), 10),
            Some(50.0)
        );
        assert_eq!(
            cpu_percent_delta(Some(6_000_000_000), Some(1_000_000_000), 10),
            None
        );
        assert_eq!(cpu_percent_delta(Some(1), Some(2), 0), None);
        assert_eq!(cpu_percent_delta(Some(1), Some(2), 601), None);
        assert_eq!(cpu_percent_delta(None, Some(2), 10), None);
    }

    #[test]
    fn threshold_alerts_are_deduplicated_and_resolved() {
        let db = Connection::open_in_memory().unwrap();
        db.execute_batch("CREATE TABLE applications(id TEXT PRIMARY KEY); INSERT INTO applications(id) VALUES('app');").unwrap();
        migrate(&db).unwrap();
        db.execute("INSERT INTO application_monitoring_rules(application_id,cpu_percent,memory_bytes,alert_when_inactive,enabled,updated_by,updated_at) VALUES('app',50,1024,1,1,'admin',1)",[]).unwrap();
        update_alerts(&db, "app", "running", "active", Some(60.0), Some(2048), 10);
        update_alerts(&db, "app", "running", "active", Some(70.0), Some(2048), 20);
        let (count, first_seen, last_seen, state): (i64, i64, i64, String) = db.query_row(
            "SELECT COUNT(*),MIN(first_seen),MAX(last_seen),MAX(CASE WHEN metric='cpu_high' THEN state END) FROM application_monitoring_alerts WHERE application_id='app' AND metric='cpu_high'",
            [], |row| Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get(3)?)),
        ).unwrap();
        assert_eq!(
            (count, first_seen, last_seen, state.as_str()),
            (1, 10, 20, "open")
        );

        update_alerts(&db, "app", "running", "unknown", None, None, 25);
        let (state, last_seen): (String, i64) = db
            .query_row(
                "SELECT state,last_seen FROM application_monitoring_alerts WHERE application_id='app' AND metric='cpu_high'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(
            state, "open",
            "a missing sample must not resolve an active alert"
        );
        assert_eq!(last_seen, 20);

        update_alerts(&db, "app", "running", "active", Some(20.0), Some(512), 30);
        let (state, resolved_at): (String, Option<i64>) = db.query_row(
            "SELECT state,resolved_at FROM application_monitoring_alerts WHERE application_id='app' AND metric='cpu_high'",
            [], |row| Ok((row.get(0)?,row.get(1)?)),
        ).unwrap();
        assert_eq!(state, "resolved");
        assert_eq!(resolved_at, Some(30));
    }
}
