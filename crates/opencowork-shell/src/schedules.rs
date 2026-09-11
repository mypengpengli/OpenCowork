use super::*;
use chrono::{TimeZone, Timelike};
use opencowork_runtime::{write_json_atomic, ExclusiveLease};
use std::time::SystemTime;
fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
fn root(state: &ShellState) -> PathBuf {
    project_memory_root(&state.config_home, &state.cwd).join(".schedules")
}
fn read(path: &Path) -> Value {
    fs::read(path)
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or(Value::Null)
}
fn next(v: &Value, after: u64) -> Result<u64, String> {
    if let Some(seconds) = v["intervalSeconds"]
        .as_u64()
        .filter(|s| *s >= 60 && *s <= 31536000)
    {
        return Ok(after.saturating_add(seconds));
    }
    let tz = v["timezone"]
        .as_str()
        .unwrap_or("Asia/Shanghai")
        .parse::<chrono_tz::Tz>()
        .map_err(|_| "Unknown IANA timezone")?;
    let time = chrono::NaiveTime::parse_from_str(v["dailyTime"].as_str().unwrap_or(""), "%H:%M")
        .map_err(|_| "Use HH:MM")?;
    let instant = chrono::DateTime::from_timestamp(after as i64, 0).ok_or("Invalid date")?;
    let mut date = instant.with_timezone(&tz).date_naive();
    for _ in 0..370 {
        // Gaps are skipped; a repeated local clock time runs at its first instant.
        if let Some(candidate) = tz
            .from_local_datetime(&date.and_hms_opt(time.hour(), time.minute(), 0).unwrap())
            .earliest()
        {
            if candidate.timestamp() > after as i64 {
                return Ok(candidate.timestamp() as u64);
            }
        }
        date = date.succ_opt().ok_or("Date overflow")?;
    }
    Err("No next occurrence".into())
}
pub(super) async fn list(State(state): State<ShellState>) -> Result<Json<Value>, ApiError> {
    let mut jobs = Vec::new();
    let mut inbox = Vec::new();
    for entry in fs::read_dir(root(&state)).into_iter().flatten().flatten() {
        if entry.path().extension().is_some_and(|x| x == "json") {
            jobs.push(read(&entry.path()));
        }
    }
    for entry in fs::read_dir(root(&state).join("inbox"))
        .into_iter()
        .flatten()
        .flatten()
    {
        if entry.path().extension().is_some_and(|x| x == "json") {
            inbox.push(read(&entry.path()));
        }
    }
    inbox.sort_by_key(|v| std::cmp::Reverse(v["at"].as_u64().unwrap_or(0)));
    inbox.truncate(100);
    Ok(Json(
        serde_json::json!({"jobs":jobs,"inbox":inbox,"hostMustBeRunning":true}),
    ))
}
pub(super) async fn change(
    State(state): State<ShellState>,
    Json(v): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    let id = v["id"]
        .as_str()
        .map(str::to_owned)
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
    if !workflows::valid(&id) {
        return Err(ApiError::bad_request("Invalid schedule ID"));
    }
    let dir = root(&state);
    let p = dir.join(format!("{id}.json"));
    // Cancellation is delivered immediately even while the job owns its disk lease.
    if v["action"] == "cancel" {
        let job = read(&p);
        if let Some(s) = job["sessionId"].as_str() {
            for t in state
                .turns
                .lock()
                .map_err(internal_error)?
                .values()
                .filter(|t| t.session_id == s)
            {
                let _ = t.cancel.send(true);
            }
        }
        return Ok(Json(serde_json::json!({"cancelRequested":true})));
    }
    let _lease = ExclusiveLease::acquire(&dir.join(format!("{id}.lock")))
        .map_err(|_| ApiError::bad_request("Job is running; use Cancel first"))?;
    let mut job = read(&p);
    match v["action"].as_str().unwrap_or("create") {
        "create" => {
            if !job.is_null() {
                return Err(ApiError::bad_request("Schedule already exists"));
            }
            let text = v["text"].as_str().unwrap_or("");
            if text.trim().is_empty() || text.len() > 16000 {
                return Err(ApiError::bad_request(
                    "Task text required, at most 16000 bytes",
                ));
            }
            let when = next(&v, now()).map_err(ApiError::bad_request)?;
            job = serde_json::json!({"id":id,"text":text,"intervalSeconds":v["intervalSeconds"],"dailyTime":v["dailyTime"],"timezone":v["timezone"].as_str().unwrap_or("Asia/Shanghai"),"missedPolicy":if v["missedPolicy"]=="once"{"once"}else{"skip"},"nextRunAt":when,"status":"enabled","budget":"execution settings"});
        }
        "pause" => job["status"] = "paused".into(),
        "resume" => {
            if job["text"].as_str().is_none() {
                return Err(ApiError::bad_request("Job missing"));
            }
            job["status"] = "enabled".into();
            job["nextRunAt"] = next(&job, now()).map_err(ApiError::bad_request)?.into();
        }
        _ => return Err(ApiError::bad_request("Unknown schedule action")),
    }
    write_json_atomic(&p, &job).map_err(internal_error)?;
    Ok(Json(job))
}
pub(super) fn start(state: ShellState) {
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(std::time::Duration::from_secs(10));
        loop {
            tick.tick().await;
            let dir = root(&state);
            let _ = fs::create_dir_all(&dir);
            for entry in fs::read_dir(&dir).into_iter().flatten().flatten() {
                let p = entry.path();
                if p.extension().is_none_or(|x| x != "json") {
                    continue;
                }
                let id = p.file_stem().unwrap().to_string_lossy().into_owned();
                if !workflows::valid(&id) {
                    continue;
                }
                let Ok(lease) = ExclusiveLease::acquire(&dir.join(format!("{id}.lock"))) else {
                    continue;
                };
                let mut job = read(&p);
                if job["status"] == "running" {
                    job["status"] = "paused".into();
                    job["error"]="Host stopped during run; outcome unknown. Inspect the recovered session before resuming.".into();
                    let _ = write_json_atomic(&p, &job);
                    let _ = write_json_atomic(
                        &dir.join("inbox").join(format!("recovered-{id}.json")),
                        &serde_json::json!({"jobId":id,"sessionId":job["sessionId"],"status":"unknown","at":now(),"error":job["error"]}),
                    );
                    continue;
                }
                let scheduled = job["nextRunAt"].as_u64().unwrap_or(u64::MAX);
                if job["status"] != "enabled" || scheduled > now() {
                    continue;
                }
                if now().saturating_sub(scheduled) > 60 && job["missedPolicy"] != "once" {
                    if let Ok(n) = next(&job, now()) {
                        job["nextRunAt"] = n.into();
                        let _ = write_json_atomic(&p, &job);
                    }
                    continue;
                }
                // One scheduled worker per host; interactive turns remain independent.
                let Ok(capacity) =
                    ExclusiveLease::acquire(&state.config_home.join("schedule-capacity.lock"))
                else {
                    continue;
                };
                let run = uuid::Uuid::new_v4().to_string();
                let session = format!("scheduled-{run}");
                let store = SessionStore::new(state.config_home.join("sessions"));
                let mut initial = Session::new().with_id(&session);
                initial.workspace = Some(state.cwd.to_string_lossy().into());
                if store.save_named(&session, &initial).is_err() {
                    continue;
                }
                job["status"] = "running".into();
                job["sessionId"] = session.clone().into();
                if write_json_atomic(&p, &job).is_err() {
                    continue;
                }
                let state = state.clone();
                let inbox = dir.join("inbox").join(format!("{run}.json"));
                tokio::spawn(async move {
                    let _lease = lease;
                    let _capacity = capacity;
                    let payload = ChatRequest {
                        turn_id: Some(run),
                        session_id: Some(session.clone()),
                        input: job["text"].as_str().unwrap_or("").into(),
                        references: vec![],
                        goal: None,
                        model: None,
                    };
                    let mut result = serde_json::json!({"jobId":id,"sessionId":session,"status":"failed","at":now()});
                    match chat::stream_turn(State(state), Json(payload)).await {
                        Ok(response) => {
                            match axum::body::to_bytes(response.into_body(), 64 * 1024 * 1024).await
                            {
                                Ok(bytes) => {
                                    for line in bytes.split(|b| *b == b'\n') {
                                        if let Ok(v) = serde_json::from_slice::<Value>(line) {
                                            if v["type"] == "complete" {
                                                result["status"] = v["response"]["status"].clone();
                                                result["error"] = v["response"]["error"].clone();
                                                result["iterations"] =
                                                    v["response"]["iterations"].clone();
                                            }
                                        }
                                    }
                                }
                                Err(_) => {
                                    result["status"] = "unknown".into();
                                    result["error"] =
                                        "Result exceeded inbox stream limit; inspect session"
                                            .into();
                                }
                            }
                        }
                        Err(e) => result["error"] = e.message.into(),
                    }
                    job["status"] = if result["status"] == "completed" {
                        "enabled"
                    } else {
                        "paused"
                    }
                    .into();
                    if let Ok(n) = next(&job, now()) {
                        job["nextRunAt"] = n.into();
                    }
                    let _ = write_json_atomic(&inbox, &result);
                    let _ = write_json_atomic(&p, &job);
                });
            }
        }
    });
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn timezone_and_dst() {
        let v = serde_json::json!({"dailyTime":"02:30","timezone":"America/New_York"});
        let before = chrono::DateTime::parse_from_rfc3339("2026-03-08T00:00:00Z")
            .unwrap()
            .timestamp() as u64;
        let n = next(&v, before).unwrap();
        assert_eq!(
            chrono::DateTime::from_timestamp(n as i64, 0)
                .unwrap()
                .to_rfc3339(),
            "2026-03-09T06:30:00+00:00"
        );
        assert!(next(
            &serde_json::json!({"timezone":"bad","dailyTime":"12:00"}),
            before
        )
        .is_err());
    }
}
