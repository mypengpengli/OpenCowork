mod analysis;
mod cases;
mod context;
mod store;
#[cfg(windows)]
mod worker;
#[cfg(windows)]
use worker::PendingSuggestion;
pub(super) mod decision;
mod execution;
#[cfg(windows)]
mod reply;
#[cfg(windows)]
mod screen;
pub(super) mod tasks_api;

use super::{ApiError, ShellState};
use axum::{
    extract::{Path, Query},
    http::{header, HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    Extension, Json,
};
use serde::{Deserialize, Serialize};
use std::{
    fs, io,
    path::{Path as FsPath, PathBuf},
    sync::{Arc, Condvar, Mutex, MutexGuard},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use uuid::Uuid;

const RETENTION_MS: u64 = 30 * 24 * 60 * 60 * 1000;
const PREVIEW_TTL: Duration = Duration::from_secs(30);
const AUTO_INTERVAL: Duration = Duration::from_secs(20);

#[derive(Clone)]
pub(super) struct Assistant {
    shared: Arc<Shared>,
    path: PathBuf,
    desktop_pid: Option<u32>,
    store: Option<Arc<store::Store>>,
}

struct Shared {
    data: Mutex<Data>,
    wake: Condvar,
}

struct Data {
    enabled: bool,
    interval_ms: u64,
    revision: u64,
    phase: &'static str,
    language: String,
    error: Option<String>,
    preview: Option<Preview>,
    log: Vec<Observation>,
    decision: decision::Settings,
    learning: Learning,
    active_analysis: Option<Uuid>,
    settings_revision: u64,
    analysis_error: Option<String>,
    drafting: Vec<Uuid>,
    execution_worker: Option<Uuid>,
    learning_revision: u64,
    session: Option<Uuid>,
    #[cfg(windows)]
    pending: Option<PendingSuggestion>,
    #[cfg(windows)]
    capture_gap: Option<u64>,
}

struct Preview {
    time: Instant,
    observation: Uuid,
    png: Vec<u8>,
    crop: Option<Vec<u8>>,
}

#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(default)]
struct Learning {
    predictions: Vec<analysis::Prediction>,
    lessons: Vec<analysis::Outcome>,
    auto_calls: Vec<u64>,
    cases: Vec<cases::Case>,
    grants: Vec<execution::Grant>,
    progress: context::Progress,
}

impl Learning {
    fn permits_observation(&self, item: &Observation, settings: &decision::Settings) -> bool {
        item.allowed_for_upload(settings)
            && !self
                .progress
                .gaps
                .iter()
                .any(|gap| gap.seq == item.seq && gap.reason == "uploadExcluded")
    }
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Observation {
    id: Uuid,
    #[serde(default)]
    seq: u64,
    #[serde(default)]
    session: Option<Uuid>,
    #[serde(default)]
    local_at_capture: bool,
    #[serde(default)]
    input_stamp: Option<u32>,
    time_ms: u64,
    window: String,
    text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    window_text: Option<String>,
    #[serde(default)]
    capture_ms: u64,
    #[serde(default)]
    event: ObservationEvent,
    #[serde(default)]
    hwnd: u64,
    #[serde(default)]
    pid: u32,
    #[serde(default)]
    app: String,
    #[serde(default)]
    assistant_action: bool,
}

#[derive(Clone, Copy, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
enum ObservationEvent {
    #[default]
    ScreenChange,
    WindowSwitch,
    AfterInput,
    Assistant,
}

impl Observation {
    fn allowed_for_upload(&self, settings: &decision::Settings) -> bool {
        !self.local_at_capture
            && settings.permits_text(&self.app)
            && (self.window_text.is_some() || !settings.restricts_uploads())
    }

    fn for_upload(&self) -> Self {
        let mut scoped = self.clone();
        if let Some(text) = scoped.window_text.take() {
            scoped.text = text;
        }
        scoped
    }

    fn upload_source(&self) -> String {
        self.window_text
            .as_ref()
            .map(|_| self.app.clone())
            .unwrap_or_default()
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Overview {
    available: bool,
    enabled: bool,
    interval_ms: u64,
    phase: &'static str,
    language: String,
    error: Option<String>,
    has_preview: bool,
    image_observation: Option<Uuid>,
    observations: Vec<Observation>,
    decision: decision::Settings,
    predictions: Vec<analysis::Prediction>,
    lessons: Vec<analysis::Outcome>,
    analyzing: bool,
    analysis_error: Option<String>,
    auto_remaining: usize,
    progress: ProgressView,
    cases: Vec<cases::Case>,
    grants: Vec<execution::Grant>,
    drafting: Vec<Uuid>,
    execution_in_flight: bool,
    metrics: analysis::Metrics,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ProgressView {
    through: u64,
    through_time_ms: Option<u64>,
    pending: usize,
    oldest_pending_ms: Option<u64>,
    legacy_coverage_unknown: bool,
    gaps: usize,
    recent_gaps: Vec<context::Gap>,
    recent_batches: Vec<context::Receipt>,
    recent_attempts: Vec<context::Attempt>,
}

impl ProgressView {
    fn from_data(data: &Data) -> Self {
        let progress = &data.learning.progress;
        let through = progress.cursor.seq;
        let processed = data.log.iter().rev().find(|item| item.seq <= through);
        let pending = data.log.partition_point(|item| item.seq <= through);
        Self {
            through,
            through_time_ms: processed.map(|item| item.time_ms),
            pending: data.log.len() - pending,
            oldest_pending_ms: data.log.get(pending).map(|item| item.time_ms),
            legacy_coverage_unknown: progress.legacy_coverage_unknown,
            gaps: progress
                .gaps
                .iter()
                .filter(|gap| {
                    matches!(
                        gap.reason.as_str(),
                        "deleted" | "expired" | "captureInterrupted"
                    )
                })
                .count(),
            recent_gaps: progress.gaps.iter().rev().take(10).cloned().collect(),
            recent_batches: progress.batches.iter().rev().take(8).cloned().collect(),
            recent_attempts: progress.attempts.iter().rev().take(8).cloned().collect(),
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct Update {
    enabled: Option<bool>,
    interval_ms: Option<u64>,
}

impl Assistant {
    pub(super) fn new(home: &FsPath, cwd: &FsPath) -> Self {
        #[cfg(windows)]
        let desktop_pid = std::env::var("OPENCOWORK_DESKTOP_PID")
            .ok()
            .and_then(|value| value.parse().ok());
        #[cfg(not(windows))]
        let desktop_pid = None;
        let path = home.join("as-desktop");
        let (store, log, mut learning, mut error) = match store::Store::open(&path) {
            Ok((store, log, learning)) => (Some(Arc::new(store)), log, learning, None),
            Err(error) => (
                None,
                Vec::new(),
                Learning::default(),
                Some(error.to_string()),
            ),
        };
        let desktop_pid = desktop_pid.filter(|_| store.is_some());
        let (mut decision, settings_error) =
            load_or_report::<decision::Settings>(&path.join("decision.json"));
        if error.is_none() {
            error = settings_error;
        }
        if store.is_some() {
            match decision.migrate_legacy(cwd) {
                Ok(true) => {
                    if let Err(failure) = persist(&path.join("decision.json"), &decision) {
                        error = Some(format!(
                            "Could not save migrated decision settings: {failure}"
                        ));
                    }
                }
                Ok(false) => {}
                Err(failure) => {
                    error = Some(format!("Could not migrate decision Provider: {failure}"))
                }
            }
        }
        let mut recovered = false;
        for case in &mut learning.cases {
            if case.source_apps.is_empty() {
                case.source_apps.insert(String::new());
            }
            if let Some(run) = case.run.as_mut().filter(|run| run.is_running()) {
                run.status = if run.changed == 0 {
                    execution::RunStatus::Stopped
                } else {
                    execution::RunStatus::Unverified
                };
                run.detail =
                    "Desktop restarted during this task. Check the target before resuming.".into();
                run.ended_ms = Some(now_ms());
                recovered = true;
            }
        }
        learning.grants.retain(|grant| {
            grant.mode == execution::Mode::Always
                || grant.expires_ms.is_some_and(|ms| ms > now_ms())
        });
        let workspace = workspace_key(cwd);
        for grant in &mut learning.grants {
            if grant.workspace.is_empty() {
                grant.workspace = workspace.clone();
                recovered = true;
            }
        }
        if recovered {
            if let Some(store) = &store {
                if let Err(failure) = store.save(&learning, &[]) {
                    error = Some(format!("Could not record interrupted execution: {failure}"));
                }
            }
        }
        #[allow(unused_mut)]
        let mut assistant = Self {
            shared: Arc::new(Shared {
                data: Mutex::new(Data {
                    enabled: false,
                    interval_ms: 1000,
                    revision: 0,
                    phase: "paused",
                    language: String::new(),
                    error,
                    preview: None,
                    log,
                    decision,
                    learning,
                    active_analysis: None,
                    settings_revision: 0,
                    analysis_error: None,
                    drafting: Vec::new(),
                    execution_worker: None,
                    learning_revision: 0,
                    session: None,
                    #[cfg(windows)]
                    pending: None,
                    #[cfg(windows)]
                    capture_gap: None,
                }),
                wake: Condvar::new(),
            }),
            path,
            desktop_pid,
            store,
        };
        {
            let mut data = assistant.data();
            if assistant.store.is_some() {
                if let Err(failure) = assistant.expire(&mut data) {
                    data.error = Some(failure.to_string());
                }
            }
        }
        #[cfg(windows)]
        if let Some(pid) = desktop_pid {
            let worker = assistant.clone();
            if let Err(error) = std::thread::Builder::new()
                .name("as-desktop-observer".into())
                .spawn(move || worker.run(pid))
            {
                assistant.desktop_pid = None;
                assistant.data().error = Some(error.to_string());
            }
            let worker = assistant.clone();
            let cwd = cwd.to_owned();
            if let Err(error) = std::thread::Builder::new()
                .name("as-observation-analysis".into())
                .spawn(move || worker.run_analysis(&cwd))
            {
                assistant.data().analysis_error = Some(error.to_string());
            }
        }
        #[cfg(not(windows))]
        let _ = cwd;
        assistant
    }

    fn data(&self) -> MutexGuard<'_, Data> {
        self.shared
            .data
            .lock()
            .unwrap_or_else(|error| error.into_inner())
    }

    fn store(&self) -> io::Result<&store::Store> {
        self.store.as_deref().ok_or_else(|| {
            io::Error::other("AS storage is unavailable or owned by another process")
        })
    }

    fn commit(&self, data: &mut Data, learning: Learning) -> io::Result<()> {
        self.commit_with_removal(data, learning, &[])
    }

    fn commit_with_removal(
        &self,
        data: &mut Data,
        learning: Learning,
        remove: &[Uuid],
    ) -> io::Result<()> {
        self.store()?.save(&learning, remove)?;
        data.learning = learning;
        data.learning_revision += 1;
        Ok(())
    }

    fn expire(&self, data: &mut Data) -> io::Result<()> {
        let cutoff = now_ms().saturating_sub(RETENTION_MS);
        let expired = data
            .log
            .iter()
            .filter(|item| item.time_ms < cutoff)
            .collect::<Vec<_>>();
        if expired.is_empty() {
            return Ok(());
        }
        let mut learning = data.learning.clone();
        let remove = expired
            .iter()
            .map(|item| item.id)
            .collect::<std::collections::HashSet<_>>();
        if data
            .log
            .iter()
            .find(|item| item.seq > learning.progress.cursor.seq)
            .is_some_and(|item| remove.contains(&item.id))
        {
            learning.progress.cursor.offset = 0;
            learning.progress.evidence_requests.clear();
            learning.progress.follow_up_through = None;
        }
        for item in expired
            .into_iter()
            .filter(|item| item.seq > learning.progress.cursor.seq)
        {
            learning.progress.gaps.push(context::Gap {
                seq: item.seq,
                time_ms: item.time_ms,
                reason: "expired".into(),
            });
        }
        learning
            .predictions
            .retain(|item| !remove.contains(&item.observation_id));
        let ids = remove.iter().copied().collect::<Vec<_>>();
        self.commit_with_removal(data, learning, &ids)?;
        data.log.retain(|item| !remove.contains(&item.id));
        Ok(())
    }

    fn view(&self) -> Overview {
        let data = self.data();
        let cutoff = now_ms().saturating_sub(RETENTION_MS);
        let observations: Vec<_> = data
            .log
            .iter()
            .rev()
            .filter(|item| item.time_ms >= cutoff)
            .take(20)
            .cloned()
            .collect();
        let mut cases = data.learning.cases.iter().collect::<Vec<_>>();
        cases.sort_by_key(|case| (case.status.open(), case.updated_ms));
        Overview {
            available: self.desktop_pid.is_some(),
            enabled: data.enabled,
            interval_ms: data.interval_ms,
            phase: data.phase,
            language: data.language.clone(),
            error: data.error.clone(),
            has_preview: current_preview(&data).is_some(),
            image_observation: current_preview(&data)
                .filter(|item| item.crop.is_some() && data.decision.use_images)
                .map(|item| item.observation),
            predictions: data
                .learning
                .predictions
                .iter()
                .filter(|prediction| {
                    observations
                        .iter()
                        .any(|item| item.id == prediction.observation_id)
                })
                .cloned()
                .collect(),
            observations,
            decision: data.decision.clone(),
            lessons: data
                .learning
                .lessons
                .iter()
                .rev()
                .take(8)
                .cloned()
                .collect(),
            analyzing: data.active_analysis.is_some(),
            analysis_error: data.analysis_error.clone(),
            auto_remaining: auto_remaining(&data.learning, &data.decision),
            progress: ProgressView::from_data(&data),
            cases: cases.into_iter().rev().take(80).cloned().collect(),
            grants: data
                .learning
                .grants
                .iter()
                .filter(|grant| {
                    grant.mode == execution::Mode::Always
                        || grant.expires_ms.is_some_and(|ms| ms > now_ms())
                })
                .cloned()
                .collect(),
            drafting: data.drafting.clone(),
            execution_in_flight: data.execution_worker.is_some(),
            metrics: analysis::Metrics::recent(
                &data.log,
                &data.learning.predictions,
                &data.learning.lessons,
                now_ms(),
            ),
        }
    }

    fn update(&self, update: Update) -> Result<(), ApiError> {
        if update
            .interval_ms
            .is_some_and(|ms| !matches!(ms, 500 | 1000))
        {
            return Err(ApiError::bad_request(
                "Observation interval must be 500 or 1000 ms",
            ));
        }
        if update.enabled == Some(true) && self.desktop_pid.is_none() {
            return Err(ApiError::bad_request(
                "Start OpenCowork Desktop to observe your screen",
            ));
        }
        let mut data = self.data();
        if let Some(interval) = update.interval_ms {
            data.interval_ms = interval;
        }
        if let Some(enabled) = update.enabled.filter(|enabled| *enabled != data.enabled) {
            data.enabled = enabled;
            data.session = enabled.then(Uuid::new_v4);
            data.revision += 1;
            data.phase = if enabled { "starting" } else { "paused" };
            data.preview = None;
            data.error = None;
        }
        self.shared.wake.notify_all();
        Ok(())
    }

    #[cfg(windows)]
    fn run(&self, pid: u32) {
        loop {
            let data = self.data();
            let data = self
                .shared
                .wake
                .wait_while(data, |d| !d.enabled)
                .unwrap_or_else(|error| error.into_inner());
            let revision = data.revision;
            drop(data);
            let result = screen::in_apartment(|| self.observe(pid, revision));
            if let Err(error) = result {
                let mut data = self.data();
                if data.revision == revision {
                    data.enabled = false;
                    data.phase = "error";
                    data.preview = None;
                    data.error = Some(error.to_string());
                }
            }
        }
    }

    #[cfg(windows)]
    fn observe(&self, pid: u32, revision: u64) -> anyhow::Result<()> {
        let _lock = screen::ObserverLock::acquire()?;
        let mut screen = screen::Screen::new()?;
        let mut last_settings_revision = None;
        let mut last_retention = Instant::now();
        loop {
            anyhow::ensure!(
                screen::parent_is_running(pid),
                "Desktop host is no longer running"
            );
            let start = Instant::now();
            let data = self.data();
            if !data.enabled || data.revision != revision {
                return Ok(());
            }
            let settings = data.decision.clone();
            let settings_revision = data.settings_revision;
            drop(data);
            if last_settings_revision != Some(settings_revision) {
                screen.clear();
                last_settings_revision = Some(settings_revision);
            }
            let sample = screen.sample(pid, &settings);
            let mut data = self.data();
            if !data.enabled || data.revision != revision {
                return Ok(());
            }
            if data.settings_revision != settings_revision {
                screen.clear();
                continue;
            }
            data.language = screen.language().to_owned();
            data.phase = "observing";
            if data
                .preview
                .as_ref()
                .is_some_and(|preview| preview.time.elapsed() >= PREVIEW_TTL)
            {
                data.preview = None;
            }
            match sample {
                Ok(screen::Sample::Locked) => {
                    data.phase = "locked";
                    data.preview = None;
                }
                Ok(screen::Sample::Skipped) => data.phase = "waiting",
                Ok(screen::Sample::Excluded) => {
                    data.phase = "waiting";
                    data.preview = None;
                }
                Ok(screen::Sample::Unchanged) => {}
                Ok(screen::Sample::Observed {
                    window,
                    hwnd,
                    pid,
                    app,
                    text,
                    window_text,
                    visual_change,
                    event,
                    preview,
                    crop,
                }) => {
                    if (visual_change
                        && window_text
                            .as_ref()
                            .is_some_and(|text| text.trim().chars().count() < 60))
                        || data.log.last().is_none_or(|last| {
                            last.window != window
                                || last.text != text
                                || last.hwnd != hwnd
                                || last.pid != pid
                                || last.window_text != window_text
                        })
                    {
                        let assistant_action = data.learning.cases.iter().any(|case| {
                            case.run
                                .as_ref()
                                .is_some_and(|run| run.status == execution::RunStatus::Running)
                                && case.plan.as_ref().is_some_and(|plan| plan.hwnd == hwnd)
                        });
                        let mut record = Observation {
                            id: Uuid::new_v4(),
                            seq: 0,
                            session: data.session,
                            local_at_capture: !settings.permits_text(&app)
                                || settings.restricts_uploads() && window_text.is_none(),
                            input_stamp: screen::foreground_input()
                                .filter(|(h, p, _)| *h == hwnd && *p == pid)
                                .map(|(_, _, stamp)| stamp),
                            time_ms: now_ms(),
                            window,
                            text,
                            window_text,
                            capture_ms: start.elapsed().as_millis() as u64,
                            event: if assistant_action {
                                ObservationEvent::Assistant
                            } else {
                                event
                            },
                            hwnd,
                            pid,
                            app,
                            assistant_action,
                        };
                        if let Err(error) = self.store()?.append(&mut record) {
                            data.capture_gap.get_or_insert(record.time_ms);
                            data.phase = "captureError";
                            data.error = Some(format!("Could not save observation: {error}"));
                            data.preview = None;
                            screen.clear();
                            let _waiting =
                                self.shared.wake.wait_timeout(data, Duration::from_secs(1));
                            continue;
                        }
                        if let Some(at) = data.capture_gap {
                            let mut next = data.learning.clone();
                            next.progress.gaps.push(context::Gap {
                                seq: record.seq,
                                time_ms: at,
                                reason: "captureInterrupted".into(),
                            });
                            if self.commit(&mut data, next).is_ok() {
                                data.capture_gap = None;
                            }
                        }
                        data.log.push(record);
                        data.error = None;
                        self.shared.wake.notify_all();
                    }
                    let observation = data.log.last().expect("observation captured").id;
                    data.preview = Some(Preview {
                        time: Instant::now(),
                        observation,
                        png: preview,
                        crop: if data.decision.use_images { crop } else { None },
                    });
                }
                Err(error) => {
                    data.phase = "captureError";
                    data.preview = None;
                    data.error = Some(error.to_string());
                    data.capture_gap.get_or_insert_with(now_ms);
                    screen.clear();
                }
            }
            if last_retention.elapsed() >= Duration::from_secs(60) {
                last_retention = Instant::now();
                if let Err(error) = self.expire(&mut data) {
                    data.error = Some(error.to_string());
                }
            }
            let delay = Duration::from_millis(data.interval_ms).saturating_sub(start.elapsed());
            let _waiting = self.shared.wake.wait_timeout(data, delay);
        }
    }

    fn analyze(
        &self,
        cwd: &FsPath,
        id: Uuid,
        request_image: bool,
    ) -> Result<analysis::Prediction, String> {
        self.store().map_err(|error| error.to_string())?;
        let (
            settings,
            settings_revision,
            learning_revision,
            observations,
            lessons,
            open_cases,
            skills,
            image,
            sources,
        ) = {
            let mut data = self.data();
            if data.active_analysis.is_some() {
                return Err("An observation is already being analyzed".into());
            }
            let position = data
                .log
                .iter()
                .position(|item| {
                    item.id == id && now_ms().saturating_sub(item.time_ms) <= RETENTION_MS
                })
                .ok_or("Observation no longer exists")?;
            let latest = &data.log[position];
            if !data.learning.permits_observation(latest, &data.decision) {
                return Err("This observation is local only or has no isolated window text".into());
            }
            let published = latest.time_ms;
            let mut originals = data.log[..=position]
                .iter()
                .rev()
                .take_while(|item| published.saturating_sub(item.time_ms) <= 180_000)
                .filter(|item| !item.assistant_action || item.id == id)
                .filter(|item| data.learning.permits_observation(item, &data.decision))
                .take(12)
                .collect::<Vec<_>>();
            originals.reverse();
            let evidence = originals
                .iter()
                .map(|item| item.for_upload())
                .collect::<Vec<_>>();
            let mut sources: std::collections::BTreeSet<String> =
                originals.iter().map(|item| item.upload_source()).collect();
            let permitted_lessons = data
                .learning
                .lessons
                .iter()
                .filter(|lesson| {
                    !lesson.local_only
                        && data.decision.permits_text(&lesson.source_app)
                        && data.log[..=position]
                            .iter()
                            .any(|item| item.id == lesson.observation_id)
                })
                .cloned()
                .collect::<Vec<_>>();
            let latest = evidence.last().expect("latest allowed observation");
            let lessons = relevant_lessons(&permitted_lessons, latest);
            sources.extend(lessons.iter().map(|lesson| lesson.source_app.clone()));
            let eligible_cases = data
                .learning
                .cases
                .iter()
                .filter(|case| case.upload_allowed(&data.decision) && case.updated_ms <= published)
                .collect::<Vec<_>>();
            let open_cases = cases::references(eligible_cases.iter().copied());
            let skills = eligible_cases
                .iter()
                .rev()
                .filter_map(|case| case.draft.as_ref().map(|draft| (case, draft)))
                .filter(|(case, draft)| draft.installed.is_some() && !case.draft_stale)
                .take(8)
                .map(|(case, draft)| {
                    case.add_upload_sources(&mut sources);
                    draft.clone()
                })
                .collect::<Vec<_>>();
            for case in &eligible_cases {
                if open_cases.iter().any(|reference| reference.id == case.id) {
                    case.add_upload_sources(&mut sources);
                }
            }
            let image = (data.decision.use_images
                && data.decision.app_policy(&latest.app) == decision::AppPolicy::Allow
                && request_image)
                .then(|| {
                    current_preview(&data)
                        .filter(|preview| preview.observation == id)
                        .and_then(|preview| preview.crop.clone())
                })
                .flatten();
            if request_image && image.is_none() {
                return Err("No fresh image is available for this observation".into());
            }
            data.active_analysis = Some(id);
            data.analysis_error = None;
            (
                data.decision.clone(),
                data.settings_revision,
                data.learning_revision,
                evidence,
                lessons,
                open_cases,
                skills,
                image,
                sources,
            )
        };
        let started = Instant::now();
        let result = analysis::predict(
            cwd,
            &settings,
            &observations,
            &lessons,
            &open_cases,
            &skills,
            image.as_deref(),
            || {
                let data = self.data();
                settings_revision == data.settings_revision
                    && learning_revision == data.learning_revision
                    && data.log.iter().any(|item| item.id == id)
            },
        );
        let mut data = self.data();
        data.active_analysis = None;
        if settings_revision != data.settings_revision
            || learning_revision != data.learning_revision
            || !data.log.iter().any(|item| item.id == id)
        {
            return Err("Observation was cleared while the model was working".into());
        }
        match result {
            Ok((mut prediction, report, plan)) => {
                prediction.generated_ms = now_ms();
                prediction.elapsed_ms = started.elapsed().as_millis() as u64;
                prediction.evidence_delay_ms = observations
                    .last()
                    .map(|item| prediction.generated_ms.saturating_sub(item.time_ms));
                let mut history = data.learning.clone();
                if let (Some(report), Some(item)) = (
                    report,
                    observations.last().filter(|item| !item.assistant_action),
                ) {
                    let plan = plan.filter(|proposal| {
                        data.log.last().is_some_and(|current| current.id == id)
                            && (proposal.is_reply()
                                || prediction.missing.is_empty()
                                    && report
                                        .request
                                        .as_ref()
                                        .is_none_or(|request| request.missing.is_empty()))
                            && prediction.choice != "observe"
                    });
                    prediction.case_id = cases::record(&mut history.cases, &report, item, plan);
                    if let Some(case) = history
                        .cases
                        .iter_mut()
                        .find(|case| Some(case.id) == prediction.case_id)
                    {
                        case.record_sources(sources);
                    }
                }
                history.predictions.retain(|item| item.observation_id != id);
                history.predictions.push(prediction.clone());
                self.commit(&mut data, history)
                    .map_err(|error| error.to_string())?;
                drop(data);
                if let Some(case_id) = prediction.case_id {
                    self.schedule_draft(cwd, case_id);
                }
                Ok(prediction)
            }
            Err(error) => {
                data.analysis_error = Some(error.chars().take(400).collect());
                Err(error)
            }
        }
    }
}

fn auto_remaining(learning: &Learning, settings: &decision::Settings) -> usize {
    let now = now_ms();
    settings.daily_limit.saturating_sub(
        learning
            .auto_calls
            .iter()
            .filter(|at| now.saturating_sub(**at) < 86_400_000)
            .count(),
    )
}

fn relevant_lessons(
    lessons: &[analysis::Outcome],
    observation: &Observation,
) -> Vec<analysis::Outcome> {
    fn terms(text: &str) -> std::collections::HashSet<[char; 2]> {
        let text = text.to_lowercase().chars().take(6000).collect::<Vec<_>>();
        text.windows(2)
            .filter(|pair| pair.iter().all(|value| value.is_alphanumeric()))
            .map(|pair| [pair[0], pair[1]])
            .collect()
    }
    let query = terms(&format!("{} {}", observation.window, observation.text));
    let mut ranked = lessons
        .iter()
        .rev()
        .filter(|lesson| lesson.observation_id != observation.id)
        .filter_map(|lesson| {
            let score = terms(&format!("{} {}", lesson.summary, lesson.lesson))
                .intersection(&query)
                .count();
            (score > 0).then_some((score, lesson))
        })
        .collect::<Vec<_>>();
    ranked.sort_by_key(|(score, _)| std::cmp::Reverse(*score));
    ranked
        .into_iter()
        .take(5)
        .map(|(_, lesson)| lesson.clone())
        .collect()
}

fn workspace_key(cwd: &FsPath) -> String {
    cwd.canonicalize()
        .unwrap_or_else(|_| cwd.to_path_buf())
        .to_string_lossy()
        .into_owned()
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

fn current_preview(data: &Data) -> Option<&Preview> {
    data.preview
        .as_ref()
        .filter(|preview| preview.time.elapsed() < PREVIEW_TTL)
}

fn load_or_report<T: serde::de::DeserializeOwned + Default>(path: &FsPath) -> (T, Option<String>) {
    match fs::read(path) {
        Ok(bytes) => serde_json::from_slice(&bytes)
            .map(|value| (value, None))
            .unwrap_or_else(|error| {
                (
                    T::default(),
                    Some(format!("Could not load AS data: {error}")),
                )
            }),
        Err(error) if error.kind() == io::ErrorKind::NotFound => (T::default(), None),
        Err(error) => (
            T::default(),
            Some(format!("Could not load AS data: {error}")),
        ),
    }
}

fn persist(path: &FsPath, data: &impl Serialize) -> io::Result<()> {
    fs::create_dir_all(path.parent().expect("assistant file has a directory"))?;
    let temp = path.with_extension(format!("{}.tmp", Uuid::new_v4()));
    let result = fs::write(&temp, serde_json::to_vec(data)?).and_then(|()| fs::rename(&temp, path));
    if result.is_err() {
        let _ = fs::remove_file(&temp);
    }
    result
}

// These endpoints expose local observations. Cross-site browser requests must not reach them.
fn same_origin(headers: &HeaderMap) -> Result<(), ApiError> {
    let host = headers
        .get(header::HOST)
        .and_then(|value| value.to_str().ok());
    let is_loopback = host
        .and_then(|value| value.parse::<axum::http::uri::Authority>().ok())
        .is_some_and(|host| matches!(host.host(), "localhost" | "127.0.0.1" | "[::1]"));
    let is_cross_origin = !is_loopback
        || headers
            .get("sec-fetch-site")
            .is_some_and(|value| value != "same-origin" && value != "none")
        || headers.get(header::ORIGIN).is_some_and(|origin| {
            host.is_none_or(|host| origin.as_bytes() != format!("http://{host}").as_bytes())
        });
    if is_cross_origin {
        return Err(ApiError {
            status: StatusCode::FORBIDDEN,
            message: "Same-origin request required".into(),
        });
    }
    Ok(())
}

pub(super) async fn overview(
    Extension(assistant): Extension<Assistant>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, ApiError> {
    same_origin(&headers)?;
    Ok((
        [(header::CACHE_CONTROL, "no-store")],
        Json(assistant.view()),
    ))
}

#[derive(Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct ObservationQuery {
    page: Option<usize>,
    page_size: Option<usize>,
    anchor: Option<Uuid>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct ObservationPage {
    page: usize,
    page_size: usize,
    total: usize,
    pages: usize,
    anchor: Option<Uuid>,
    anchor_expired: bool,
    observations: Vec<Observation>,
    predictions: Vec<analysis::Prediction>,
    lessons: Vec<analysis::Outcome>,
}

impl Assistant {
    fn observation_page(&self, query: ObservationQuery) -> ObservationPage {
        let data = self.data();
        let cutoff = now_ms().saturating_sub(RETENTION_MS);
        let all = data
            .log
            .iter()
            .filter(|item| item.time_ms >= cutoff)
            .collect::<Vec<_>>();
        let anchor_position = query
            .anchor
            .and_then(|id| all.iter().position(|item| item.id == id));
        let anchor_expired = query.anchor.is_some() && anchor_position.is_none();
        let end = anchor_position.map_or(all.len(), |index| index + 1);
        let items = &all[..end];
        let page_size = query.page_size.unwrap_or(10).clamp(1, 50);
        let pages = items.len().div_ceil(page_size).max(1);
        let page = if anchor_expired {
            1
        } else {
            query.page.unwrap_or(1).clamp(1, pages)
        };
        let observations = items
            .iter()
            .rev()
            .skip((page - 1) * page_size)
            .take(page_size)
            .map(|item| (*item).clone())
            .collect::<Vec<_>>();
        let contains = |id| observations.iter().any(|item| item.id == id);
        ObservationPage {
            page,
            page_size,
            total: items.len(),
            pages,
            anchor: items.last().map(|item| item.id),
            anchor_expired,
            predictions: data
                .learning
                .predictions
                .iter()
                .filter(|item| contains(item.observation_id))
                .cloned()
                .collect(),
            lessons: data
                .learning
                .lessons
                .iter()
                .filter(|item| contains(item.observation_id))
                .cloned()
                .collect(),
            observations,
        }
    }
}

pub(super) async fn observations(
    Extension(assistant): Extension<Assistant>,
    headers: HeaderMap,
    Query(query): Query<ObservationQuery>,
) -> Result<impl IntoResponse, ApiError> {
    same_origin(&headers)?;
    Ok((
        [(header::CACHE_CONTROL, "no-store")],
        Json(assistant.observation_page(query)),
    ))
}

pub(super) async fn update(
    Extension(assistant): Extension<Assistant>,
    headers: HeaderMap,
    Json(update): Json<Update>,
) -> Result<Json<Overview>, ApiError> {
    same_origin(&headers)?;
    assistant.update(update)?;
    Ok(Json(assistant.view()))
}

pub(super) async fn preview(
    Extension(assistant): Extension<Assistant>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    same_origin(&headers)?;
    let data = assistant.data();
    Ok(match current_preview(&data) {
        Some(preview) => (
            [
                (header::CONTENT_TYPE, "image/png"),
                (header::CACHE_CONTROL, "no-store"),
            ],
            preview.png.clone(),
        )
            .into_response(),
        None => StatusCode::NO_CONTENT.into_response(),
    })
}

pub(super) async fn forget(
    Extension(assistant): Extension<Assistant>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<StatusCode, ApiError> {
    same_origin(&headers)?;
    let id = Uuid::parse_str(&id).map_err(|_| ApiError::bad_request("Invalid observation ID"))?;
    let mut data = assistant.data();
    let item = data
        .log
        .iter()
        .find(|item| item.id == id)
        .ok_or_else(|| ApiError::not_found("Observation not found"))?;
    let mut learning = data.learning.clone();
    learning.progress.gaps.push(context::Gap {
        seq: item.seq,
        time_ms: item.time_ms,
        reason: "deleted".into(),
    });
    if learning.progress.cursor.seq < item.seq
        && data
            .log
            .iter()
            .find(|old| old.seq > learning.progress.cursor.seq)
            .is_some_and(|old| old.id == id)
    {
        learning.progress.cursor.offset = 0;
    }
    learning
        .predictions
        .retain(|item| item.observation_id != id);
    learning.lessons.retain(|item| item.observation_id != id);
    let affected = learning
        .cases
        .iter()
        .filter(|case| {
            case.observations.contains(&id)
                || case.evidence.iter().any(|fact| fact.observation_id == id)
        })
        .map(|case| case.id)
        .collect::<std::collections::BTreeSet<_>>();
    if learning.cases.iter().any(|case| {
        affected.contains(&case.id)
            && case.run.as_ref().is_some_and(|run| {
                run.is_running() || run.status == execution::RunStatus::Unverified
            })
    }) {
        return Err(ApiError::bad_request(
            "Stop or verify the affected execution before deleting this observation",
        ));
    }
    for case in &mut learning.cases {
        case.remove_observation(id);
    }
    learning
        .grants
        .retain(|grant| !affected.contains(&grant.case_id));
    learning.predictions.retain(|prediction| {
        prediction
            .case_id
            .is_none_or(|case| !affected.contains(&case))
    });
    for batch in &mut learning.progress.batches {
        batch.associations.retain(|_, ids| {
            ids.retain(|id| !affected.contains(id));
            !ids.is_empty()
        });
    }
    assistant
        .commit_with_removal(&mut data, learning, &[id])
        .map_err(|error| ApiError::internal(error.to_string()))?;
    data.log.retain(|item| item.id != id);
    data.preview = None;
    Ok(StatusCode::NO_CONTENT)
}

pub(super) async fn save_decision(
    Extension(assistant): Extension<Assistant>,
    Extension(shell): Extension<ShellState>,
    headers: HeaderMap,
    Json(mut settings): Json<decision::Settings>,
) -> Result<Json<decision::Settings>, ApiError> {
    same_origin(&headers)?;
    settings
        .migrate_legacy(&shell.cwd)
        .map_err(ApiError::bad_request)?;
    settings.validate().map_err(ApiError::bad_request)?;
    if settings.use_for_analysis {
        super::saved_provider_profile(&shell.cwd, &settings.decision_provider)
            .map_err(ApiError::bad_request)?;
    }
    let mut data = assistant.data();
    assistant
        .store()
        .map_err(|error| ApiError::internal(error.to_string()))?;
    persist(&assistant.path.join("decision.json"), &settings)
        .map_err(|error| ApiError::internal(error.to_string()))?;
    data.decision = settings.clone();
    data.settings_revision += 1;
    if let Some(preview) = &mut data.preview {
        preview.crop = None;
    }
    assistant.shared.wake.notify_all();
    Ok(Json(settings))
}

#[derive(Deserialize)]
pub(super) struct AnalyzeQuery {
    #[serde(default)]
    image: bool,
}

pub(super) async fn analyze(
    Extension(assistant): Extension<Assistant>,
    Extension(shell): Extension<ShellState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    Query(query): Query<AnalyzeQuery>,
) -> Result<Json<analysis::Prediction>, ApiError> {
    same_origin(&headers)?;
    let result =
        tokio::task::spawn_blocking(move || assistant.analyze(&shell.cwd, id, query.image))
            .await
            .map_err(|error| ApiError::internal(error.to_string()))?
            .map_err(ApiError::bad_request)?;
    Ok(Json(result))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct OutcomeInput {
    summary: String,
    lesson: String,
    #[serde(default)]
    feedback: analysis::Feedback,
}

pub(super) async fn record_outcome(
    Extension(assistant): Extension<Assistant>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    Json(input): Json<OutcomeInput>,
) -> Result<Json<Overview>, ApiError> {
    same_origin(&headers)?;
    let summary = input.summary.trim();
    let lesson = input.lesson.trim();
    if summary.is_empty() || summary.chars().count() > 400 || lesson.chars().count() > 1200 {
        return Err(ApiError::bad_request(
            "Enter an outcome (up to 400 characters) and optional lesson (up to 1200 characters)",
        ));
    }
    let mut data = assistant.data();
    let observation = data
        .log
        .iter()
        .find(|item| item.id == id)
        .ok_or_else(|| ApiError::not_found("Observation no longer exists"))?;
    let source_app = observation.upload_source();
    let local_only = !data
        .learning
        .permits_observation(observation, &data.decision);
    let mut history = data.learning.clone();
    history.lessons.retain(|lesson| lesson.observation_id != id);
    history.lessons.push(analysis::Outcome {
        id: Uuid::new_v4(),
        observation_id: id,
        summary: summary.into(),
        lesson: lesson.into(),
        source_app,
        local_only,
        feedback: input.feedback,
    });
    history
        .lessons
        .drain(..history.lessons.len().saturating_sub(300));
    assistant
        .commit(&mut data, history)
        .map_err(|error| ApiError::internal(error.to_string()))?;
    drop(data);
    Ok(Json(assistant.view()))
}

pub(super) async fn forget_outcome(
    Extension(assistant): Extension<Assistant>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, ApiError> {
    same_origin(&headers)?;
    let mut data = assistant.data();
    let mut history = data.learning.clone();
    history.lessons.retain(|lesson| lesson.id != id);
    if history.lessons.len() == data.learning.lessons.len() {
        return Err(ApiError::not_found("Outcome no longer exists"));
    }
    assistant
        .commit(&mut data, history)
        .map_err(|error| ApiError::internal(error.to_string()))?;
    Ok(StatusCode::NO_CONTENT)
}

pub(super) async fn test_decision(
    Extension(assistant): Extension<Assistant>,
    Extension(shell): Extension<ShellState>,
    headers: HeaderMap,
) -> Result<Json<decision::ResultView>, ApiError> {
    same_origin(&headers)?;
    let settings = assistant.data().decision.clone();
    let result = tokio::task::spawn_blocking(move || decision::probe(&settings, &shell.cwd))
        .await
        .map_err(|error| ApiError::internal(error.to_string()))?
        .map_err(ApiError::bad_request)?;
    Ok(Json(result))
}

pub(super) fn worker_budget(cwd: &FsPath, provider_id: &str, budget: usize) -> usize {
    let window = if provider_id.is_empty() {
        super::provider_diagnostics::profile(cwd, true)
            .ok()
            .map(|(model, profile)| {
                profile.capabilities.context_window.unwrap_or_else(|| {
                    opencowork_runtime::get_context_window_for_model(&model, None)
                })
            })
    } else {
        super::saved_provider_profile(cwd, provider_id)
            .ok()
            .map(|profile| {
                profile
                    .model_capabilities
                    .context_window
                    .unwrap_or_else(|| {
                        opencowork_runtime::get_context_window_for_model(&profile.model, None)
                    })
            })
    };
    budget.min(window.unwrap_or(16_384).saturating_sub(5_500).max(1000))
}
