use super::{cases, execution, now_ms, same_origin, ApiError, Assistant, Overview, ShellState};
use axum::{
    extract::Path,
    http::{HeaderMap, StatusCode},
    Extension, Json,
};
use execution::{Grant, Mode, Plan, Run, RunStatus};
use serde::{Deserialize, Serialize};
use std::path::{Path as FsPath, PathBuf};
use uuid::Uuid;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Edit {
    version: u64,
    title: Option<String>,
    status: Option<cases::Status>,
    outcome: Option<String>,
    merge_from: Option<Uuid>,
    observation: Option<Uuid>,
    manual_baseline_seconds: Option<u64>,
    mute_for_minutes: Option<u64>,
    correction: Option<Correction>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Correction {
    field: super::context::Field,
    value: String,
}

pub async fn edit(
    Extension(assistant): Extension<Assistant>,
    Extension(shell): Extension<ShellState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    Json(edit): Json<Edit>,
) -> Result<Json<Overview>, ApiError> {
    same_origin(&headers)?;
    let mut data = assistant.data();
    let mut history = data.learning.clone();
    let index = history
        .cases
        .iter()
        .position(|case| case.id == id)
        .ok_or_else(|| ApiError::not_found("Case no longer exists"))?;
    let current = &history.cases[index];
    if current.version != edit.version || current.run.as_ref().is_some_and(Run::is_running) {
        return Err(ApiError::bad_request(
            "Case changed. Refresh or stop the current run first",
        ));
    }
    if let Some(seconds) = edit.manual_baseline_seconds {
        if seconds > 86_400 {
            return Err(ApiError::bad_request(
                "Enter a baseline between 0 and 86400 seconds",
            ));
        }
        history.cases[index].manual_baseline_seconds = (seconds > 0).then_some(seconds);
        assistant.commit(&mut data, history).map_err(io_error)?;
        drop(data);
        return Ok(Json(assistant.view()));
    }
    if let Some(minutes) = edit.mute_for_minutes {
        if minutes > 1440 {
            return Err(ApiError::bad_request(
                "Case notifications can be paused for up to 24 hours",
            ));
        }
        history.cases[index].mute_until_ms = (minutes > 0).then(|| now_ms() + minutes * 60_000);
        assistant.commit(&mut data, history).map_err(io_error)?;
        drop(data);
        return Ok(Json(assistant.view()));
    }
    let manually_resolved = current
        .run
        .as_ref()
        .is_some_and(|run| run.status == RunStatus::Unverified)
        && edit
            .status
            .is_some_and(|status| matches!(status, cases::Status::Done | cases::Status::Cancelled))
        && edit
            .outcome
            .as_ref()
            .is_some_and(|outcome| !outcome.trim().is_empty());
    if current
        .run
        .as_ref()
        .is_some_and(|run| run.status == RunStatus::Unverified)
        && !manually_resolved
    {
        return Err(ApiError::bad_request(
            "Verify the last execution before editing this case",
        ));
    }
    let mut appended = Vec::new();
    let mut sources = std::collections::BTreeSet::new();
    let mut merged_facts = None;
    let mut merged_evidence = Vec::new();
    if let Some(source_id) = edit.merge_from {
        if source_id == id {
            return Err(ApiError::bad_request("Choose a different case to merge"));
        }
        let source = history
            .cases
            .iter_mut()
            .find(|case| case.id == source_id)
            .ok_or_else(|| ApiError::not_found("Source case no longer exists"))?;
        if !source.status.open()
            || source
                .run
                .as_ref()
                .is_some_and(|run| run.is_running() || run.status == RunStatus::Unverified)
        {
            return Err(ApiError::bad_request("Source case must be open and idle"));
        }
        appended.append(&mut source.observations);
        source.add_upload_sources(&mut sources);
        merged_facts = Some(source.facts.clone());
        merged_evidence.append(&mut source.evidence);
        source.status = cases::Status::Cancelled;
        source.plan = None;
        source.version += 1;
    }
    if let Some(observation) = edit.observation {
        let source = data
            .log
            .iter()
            .find(|item| item.id == observation)
            .ok_or_else(|| ApiError::not_found("Observation no longer exists"))?;
        sources.insert(source.upload_source());
        for source in &mut history.cases {
            if source.id != id && source.observations.contains(&observation) {
                if source
                    .run
                    .as_ref()
                    .is_some_and(|run| run.is_running() || run.status == RunStatus::Unverified)
                {
                    return Err(ApiError::bad_request(
                        "Stop the source case before moving its observation",
                    ));
                }
                source.remove_observation(observation);
            }
        }
        appended.push(observation);
    }
    let current = &mut history.cases[index];
    current.record_sources(sources);
    current.evidence.extend(merged_evidence);
    current.rebuild_revisions();
    current.refresh_request();
    if let Some(correction) = edit.correction {
        current
            .user_correction(
                correction.field,
                &correction.value,
                data.log.last().map_or(0, |item| item.seq),
            )
            .map_err(ApiError::bad_request)?;
    }
    if let Some(title) = edit.title {
        let title = cases::limit(&title, 160);
        if title.is_empty() {
            return Err(ApiError::bad_request("Enter a case name"));
        }
        current.title = title;
    }
    for item in appended {
        if !current.observations.contains(&item) {
            current.observations.push(item);
        }
    }
    if let Some(facts) = merged_facts {
        current.facts = cases::limit(&format!("{}\n{}", current.facts, facts), 1100);
    }
    if let Some(status) = edit.status {
        current.status = status;
        current.completed_ms = (status == cases::Status::Done).then(now_ms);
    }
    if let Some(outcome) = edit.outcome {
        let outcome = cases::limit(&outcome, 850);
        current.outcome = (!outcome.is_empty()).then_some(outcome);
    }
    if manually_resolved {
        if let Some(run) = &mut current.run {
            run.status = if current.status == cases::Status::Done {
                RunStatus::Done
            } else {
                RunStatus::Stopped
            };
            run.detail = format!(
                "User verified the result: {}",
                current.outcome.as_deref().unwrap_or_default()
            );
        }
    }
    if current.status == cases::Status::Done && current.outcome.is_none() {
        return Err(ApiError::bad_request(
            "Record the actual result before marking this case done",
        ));
    }
    current.version += 1;
    current.draft_stale |= current.draft.is_some();
    current.updated_ms = now_ms();
    current.plan = None;
    history
        .grants
        .retain(|grant| grant.case_id != id && edit.merge_from != Some(grant.case_id));
    let attached = &current.observations;
    for prediction in &mut history.predictions {
        if attached.contains(&prediction.observation_id) {
            prediction.case_id = Some(id);
        }
    }
    assistant.commit(&mut data, history).map_err(io_error)?;
    drop(data);
    assistant.schedule_draft(&shell.cwd, id);
    Ok(Json(assistant.view()))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Approval {
    plan_id: Uuid,
    mode: Mode,
}

pub async fn delete_case(
    Extension(assistant): Extension<Assistant>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, ApiError> {
    same_origin(&headers)?;
    let mut data = assistant.data();
    let mut next = data.learning.clone();
    let index = next
        .cases
        .iter()
        .position(|case| case.id == id)
        .ok_or_else(|| ApiError::not_found("Case no longer exists"))?;
    if data.active_analysis.is_some()
        || next.cases[index]
            .run
            .as_ref()
            .is_some_and(|run| run.is_running() || run.status == RunStatus::Unverified)
    {
        return Err(ApiError::bad_request(
            "Wait for analysis, stop execution, or resolve an unverified result before deleting this case",
        ));
    }
    next.cases.remove(index);
    next.progress.draft_attempts.remove(&id);
    next.predictions
        .retain(|prediction| prediction.case_id != Some(id));
    next.grants.retain(|grant| grant.case_id != id);
    for case in &mut next.cases {
        if case.related_case_id == Some(id) {
            case.related_case_id = None;
        }
    }
    assistant.commit(&mut data, next).map_err(io_error)?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn approve(
    Extension(assistant): Extension<Assistant>,
    Extension(shell): Extension<ShellState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    Json(approval): Json<Approval>,
) -> Result<(StatusCode, Json<Run>), ApiError> {
    same_origin(&headers)?;
    let run = assistant
        .begin_execution(&shell.cwd, id, approval.plan_id, Some(approval.mode))
        .map_err(ApiError::bad_request)?;
    Ok((StatusCode::ACCEPTED, Json(run)))
}

pub async fn stop(
    Extension(assistant): Extension<Assistant>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, ApiError> {
    same_origin(&headers)?;
    let mut data = assistant.data();
    let mut history = data.learning.clone();
    let case = history
        .cases
        .iter_mut()
        .find(|case| case.id == id)
        .ok_or_else(|| ApiError::not_found("Case no longer exists"))?;
    if let Some(run) = case.run.as_mut().filter(|run| run.is_running()) {
        let plan = case.plan.as_ref();
        run.status = if run.changed == 0 {
            RunStatus::Stopped
        } else {
            RunStatus::Unverified
        };
        run.detail = "User stopped the task. Check any steps that were already dispatched.".into();
        run.ended_ms = Some(now_ms());
        history
            .grants
            .retain(|grant| plan.is_none_or(|plan| !grant.plan.same_recipe(plan)));
        assistant.commit(&mut data, history).map_err(io_error)?;
    }
    Ok(StatusCode::NO_CONTENT)
}

pub async fn revoke(
    Extension(assistant): Extension<Assistant>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, ApiError> {
    same_origin(&headers)?;
    let mut data = assistant.data();
    let mut history = data.learning.clone();
    history.grants.retain(|grant| grant.id != id);
    assistant.commit(&mut data, history).map_err(io_error)?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn verify(
    Extension(assistant): Extension<Assistant>,
    Extension(shell): Extension<ShellState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Result<Json<Overview>, ApiError> {
    same_origin(&headers)?;
    if assistant.desktop_pid.is_none() {
        return Err(ApiError::bad_request(
            "Open the desktop app to check a receipt",
        ));
    }
    let (plan, version, run_id, worker_id) = {
        let mut data = assistant.data();
        if data.execution_worker.is_some() {
            return Err(ApiError::bad_request(
                "Wait for the current desktop operation to finish",
            ));
        }
        let case = data
            .learning
            .cases
            .iter()
            .find(|case| case.id == id)
            .ok_or_else(|| ApiError::not_found("Case no longer exists"))?;
        let run = case
            .run
            .as_ref()
            .filter(|run| run.status == RunStatus::Unverified)
            .ok_or_else(|| ApiError::bad_request("This case has no unverified run"))?;
        let plan = case
            .plan
            .as_ref()
            .filter(|plan| plan.id == run.plan_id)
            .cloned()
            .ok_or_else(|| {
                ApiError::bad_request("The original verification target is unavailable")
            })?;
        if plan.reply.is_some() && run.changed < 2 {
            return Err(ApiError::bad_request(
                "No send was dispatched. Review the draft in the chat.",
            ));
        }
        let (version, run_id, worker_id) = (case.version, run.id, Uuid::new_v4());
        data.execution_worker = Some(worker_id);
        (plan, version, run_id, worker_id)
    };
    let result = tokio::task::spawn_blocking(move || execution::verify_receipt(&plan)).await;
    let mut data = assistant.data();
    if data.execution_worker == Some(worker_id) {
        data.execution_worker = None;
    }
    let receipt = result
        .map_err(|error| ApiError::internal(error.to_string()))?
        .map_err(ApiError::bad_request)?;
    let mut next = data.learning.clone();
    let case = next
        .cases
        .iter_mut()
        .find(|case| case.id == id && case.version == version)
        .ok_or_else(|| ApiError::bad_request("Case changed while checking the receipt"))?;
    let run = case
        .run
        .as_mut()
        .filter(|run| run.id == run_id && run.status == RunStatus::Unverified)
        .ok_or_else(|| ApiError::bad_request("Run changed while checking the receipt"))?;
    run.status = RunStatus::Done;
    run.detail = receipt.clone();
    case.status = cases::Status::Done;
    case.outcome = Some(receipt);
    case.plan = None;
    case.version += 1;
    case.updated_ms = now_ms();
    case.completed_ms = Some(case.updated_ms);
    assistant.commit(&mut data, next).map_err(io_error)?;
    drop(data);
    assistant.schedule_draft(&shell.cwd, id);
    Ok(Json(assistant.view()))
}

pub async fn draft(
    Extension(assistant): Extension<Assistant>,
    Extension(shell): Extension<ShellState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, ApiError> {
    same_origin(&headers)?;
    if !assistant.schedule_draft(&shell.cwd, id) {
        return Err(ApiError::bad_request(
            "A Skill needs a completed case that the assistant may send to its model",
        ));
    }
    Ok(StatusCode::ACCEPTED)
}

pub async fn install(
    Extension(assistant): Extension<Assistant>,
    Extension(shell): Extension<ShellState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Result<Json<Overview>, ApiError> {
    same_origin(&headers)?;
    let mut data = assistant.data();
    let mut history = data.learning.clone();
    let draft = history
        .cases
        .iter_mut()
        .find(|case| case.id == id && case.status == cases::Status::Done)
        .and_then(|case| case.draft.as_mut())
        .ok_or_else(|| ApiError::not_found("Skill draft does not exist"))?;
    if draft.installed.is_none() {
        draft.installed =
            Some(cases::install(&shell.cwd, id, draft).map_err(ApiError::bad_request)?);
        assistant.commit(&mut data, history).map_err(io_error)?;
    }
    drop(data);
    Ok(Json(assistant.view()))
}

pub async fn disable_skill(
    Extension(assistant): Extension<Assistant>,
    Extension(shell): Extension<ShellState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Result<Json<Overview>, ApiError> {
    same_origin(&headers)?;
    let mut data = assistant.data();
    let mut history = data.learning.clone();
    let draft = history
        .cases
        .iter_mut()
        .find(|case| case.id == id)
        .and_then(|case| case.draft.as_mut())
        .ok_or_else(|| ApiError::not_found("Skill draft does not exist"))?;
    if let Some(installed) = draft.installed.as_deref() {
        cases::disable_skill(&shell.cwd, id, FsPath::new(installed))
            .map_err(ApiError::bad_request)?;
        draft.installed = None;
        assistant.commit(&mut data, history).map_err(io_error)?;
    }
    drop(data);
    Ok(Json(assistant.view()))
}

fn io_error(error: std::io::Error) -> ApiError {
    ApiError::internal(error.to_string())
}

#[derive(Serialize)]
pub struct Notice {
    id: String,
    title: String,
    text: String,
}

pub async fn notice(
    Extension(assistant): Extension<Assistant>,
    headers: HeaderMap,
) -> Result<Json<Option<Notice>>, ApiError> {
    same_origin(&headers)?;
    let data = assistant.data();
    if !data.decision.show_notifications {
        return Ok(Json(None));
    }
    let now = now_ms();
    let recent_result = data
        .learning
        .cases
        .iter()
        .filter(|case| now.saturating_sub(case.updated_ms) <= 45_000)
        .filter_map(|case| {
            case.run
                .as_ref()
                .filter(|run| !run.is_running())
                .map(|run| (case, run))
        })
        .max_by_key(|(case, _)| case.updated_ms);
    if let Some((case, run)) = recent_result {
        return Ok(Json(Some(Notice {
            id: run.id.to_string(),
            title: case.title.clone(),
            text: run.detail.clone(),
        })));
    }
    if !data.enabled || data.phase != "observing" || !data.decision.analyze_automatically {
        return Ok(Json(None));
    }
    let latest = data.log.last().filter(|item| {
        now.saturating_sub(item.time_ms) <= 30_000
            && item.session == data.session
            && data.learning.permits_observation(item, &data.decision)
    });
    let prediction = latest.and_then(|item| {
        data.learning.predictions.iter().find(|prediction| {
            prediction.live
                && prediction.observation_id == item.id
                && prediction.choice != "observe"
                && prediction.case_id.is_some()
        })
    });
    let result = prediction.and_then(|prediction| {
        let case = data.learning.cases.iter().find(|case| {
            Some(case.id) == prediction.case_id
                && case.status.open()
                && case.upload_allowed(&data.decision)
                && case.mute_until_ms.is_none_or(|until| until <= now)
                && case.run.as_ref().is_none_or(|run| !run.is_running())
        })?;
        Some(Notice {
            id: {
                use std::hash::{DefaultHasher, Hash, Hasher};
                let mut hash = DefaultHasher::new();
                prediction.choice.hash(&mut hash);
                prediction.choices.get(&prediction.choice).hash(&mut hash);
                if let Some(plan) = &case.plan {
                    plan.before.hash(&mut hash);
                    plan.verify.hash(&mut hash);
                }
                format!("{}:{:x}", case.id, hash.finish())
            },
            title: case.title.clone(),
            text: if case.plan.is_some() {
                "已准备执行方案，点此查看并授权。".into()
            } else {
                prediction.choices.get(&prediction.choice)?.clone()
            },
        })
    });
    Ok(Json(result))
}

impl Assistant {
    pub(super) fn schedule_draft(&self, cwd: &FsPath, id: Uuid) -> bool {
        let (case, history, lessons, provider, settings_revision) = {
            let mut data = self.data();
            let Some(case) = data
                .learning
                .cases
                .iter()
                .find(|case| case.id == id && case.needs_draft(&data.decision))
                .cloned()
            else {
                return false;
            };
            if data.drafting.contains(&id) {
                return true;
            }
            if data.drafting.len() >= 2 {
                return false;
            }
            let mut next = data.learning.clone();
            next.progress.draft_attempts.insert(id, now_ms());
            if let Err(error) = self.commit(&mut data, next) {
                data.analysis_error = Some(error.to_string());
                return false;
            }
            data.drafting.push(id);
            let history = data
                .log
                .iter()
                .filter(|item| {
                    !item.assistant_action
                        && case.observations.contains(&item.id)
                        && data.learning.permits_observation(item, &data.decision)
                })
                .map(super::Observation::for_upload)
                .collect::<Vec<_>>();
            let lessons = data
                .learning
                .lessons
                .iter()
                .rev()
                .filter(|lesson| {
                    case.observations.contains(&lesson.observation_id)
                        && !lesson.local_only
                        && data.decision.permits_text(&lesson.source_app)
                })
                .take(8)
                .cloned()
                .collect::<Vec<_>>();
            (
                case,
                history,
                lessons,
                data.decision.skill_provider.clone(),
                data.settings_revision,
            )
        };
        let worker = self.clone();
        let cwd = cwd.to_owned();
        let started = std::thread::Builder::new()
            .name("as-completed-skill".into())
            .spawn(move || {
                let allowed = || {
                    let data = worker.data();
                    data.settings_revision == settings_revision
                        && data
                            .learning
                            .cases
                            .iter()
                            .any(|current| current.id == id && current.version == case.version)
                        && lessons.iter().all(|lesson| {
                            data.learning
                                .lessons
                                .iter()
                                .any(|current| current.id == lesson.id)
                        })
                };
                let result = cases::draft(&cwd, &case, &history, &lessons, &provider, allowed);
                let mut data = worker.data();
                data.drafting.retain(|candidate| *candidate != id);
                if data.settings_revision != settings_revision
                    || !lessons.iter().all(|lesson| {
                        data.learning
                            .lessons
                            .iter()
                            .any(|current| current.id == lesson.id)
                    })
                {
                    return;
                }
                let mut next = data.learning.clone();
                let Some(current) = next.cases.iter_mut().find(|value| {
                    value.id == id
                        && value.version == case.version
                        && (value.draft.is_none()
                            || value.draft_stale
                                && value
                                    .draft
                                    .as_ref()
                                    .is_some_and(|draft| draft.installed.is_none()))
                }) else {
                    return;
                };
                match result {
                    Ok(draft) => {
                        current.draft = Some(draft);
                        current.draft_stale = false;
                        if let Err(error) = worker.commit(&mut data, next) {
                            data.analysis_error = Some(error.to_string());
                        }
                    }
                    Err(error) => data.analysis_error = Some(cases::limit(&error, 400)),
                }
            });
        if let Err(error) = started {
            let mut data = self.data();
            data.drafting.retain(|candidate| *candidate != id);
            data.analysis_error = Some(error.to_string());
            return false;
        }
        true
    }

    pub(super) fn try_auto_execute(&self, cwd: &FsPath, case_id: Uuid) {
        let id = {
            let data = self.data();
            data.learning
                .cases
                .iter()
                .find(|case| case.id == case_id && case.run.is_none())
                .and_then(|case| case.plan.as_ref())
                .map(|plan| plan.id)
        };
        if let Some(id) = id {
            let _ = self.begin_execution(cwd, case_id, id, None);
        }
    }

    fn begin_execution(
        &self,
        cwd: &FsPath,
        case_id: Uuid,
        plan_id: Uuid,
        mode: Option<Mode>,
    ) -> Result<Run, String> {
        let (plan, run, grant_id, revision) = {
            let mut data = self.data();
            if self.desktop_pid.is_none() || !data.enabled {
                return Err("Start desktop observation before approving the plan".into());
            }
            let config = opencowork_runtime::ConfigLoader::default_for(cwd)
                .load()
                .map_err(|error| error.to_string())?;
            if config
                .merged()
                .pointer("/computer/enabled")
                .and_then(serde_json::Value::as_bool)
                == Some(false)
            {
                return Err("Computer control is disabled for this workspace".into());
            }
            if data
                .learning
                .cases
                .iter()
                .any(|case| case.run.as_ref().is_some_and(Run::is_running))
                || data.execution_worker.is_some()
            {
                return Err("Another case is already using the desktop".into());
            }
            let mut next = data.learning.clone();
            let case = next
                .cases
                .iter_mut()
                .find(|case| case.id == case_id && case.status == cases::Status::Working)
                .ok_or("Case is no longer ready")?;
            if case
                .run
                .as_ref()
                .is_some_and(|run| run.changed > 0 || run.status == RunStatus::Done)
            {
                return Err("Check the previous result before making another change".into());
            }
            if !case.upload_allowed(&data.decision) {
                return Err(
                    "This case includes evidence blocked by the current application policy".into(),
                );
            }
            if mode.is_none() && (!data.decision.analyze_automatically || data.phase != "observing")
            {
                return Err("Automatic suggestions are paused".into());
            }
            let plan = case
                .plan
                .as_ref()
                .filter(|plan| plan.id == plan_id && plan.version == case.version)
                .cloned()
                .ok_or("Proposal changed; review the latest plan")?;
            if data.log.last().is_none_or(|item| {
                item.id != plan.observation_id || now_ms().saturating_sub(item.time_ms) > 180_000
            }) {
                return Err(
                    "The desktop has changed. Observe the target again before proceeding".into(),
                );
            }
            let run = Run {
                id: Uuid::new_v4(),
                plan_id,
                status: RunStatus::WaitingTarget,
                changed: 0,
                detail: "Waiting for the approved target window to be active and idle.".into(),
                started_ms: None,
                ended_ms: None,
            };
            next.grants.retain(|grant| valid(grant));
            let workspace = super::workspace_key(cwd);
            let previous_grant = next
                .grants
                .iter()
                .find(|grant| {
                    grant.workspace == workspace
                        && grant.mode != Mode::Once
                        && valid(grant)
                        && grant.plan.same_recipe(&plan)
                })
                .map(|grant| grant.id);
            let automatic = mode.is_none()
                && previous_grant.is_none()
                && if plan.reply.is_some() {
                    data.decision.auto_run_replies
                } else {
                    data.decision.auto_run_ready
                };
            let grant_id = if let Some(mode) = mode.or(automatic.then_some(Mode::Once)) {
                if next.grants.len() >= 200 {
                    return Err("Revoke an old grant before adding another".into());
                }
                let grant = Grant {
                    id: Uuid::new_v4(),
                    case_id,
                    workspace,
                    mode,
                    plan: plan.clone(),
                    from_global_setting: automatic,
                    expires_ms: match mode {
                        Mode::Once => Some(now_ms() + 120_000),
                        Mode::Timed => Some(now_ms() + 300_000),
                        Mode::Always => None,
                    },
                };
                let id = grant.id;
                next.grants.push(grant);
                id
            } else {
                previous_grant.ok_or("This plan has no standing authorization")?
            };
            case.run = Some(run.clone());
            self.commit(&mut data, next)
                .map_err(|error| error.to_string())?;
            data.execution_worker = Some(run.id);
            (plan, run, grant_id, data.revision)
        };
        let worker = self.clone();
        let view = run.clone();
        let worker_cwd = cwd.to_owned();
        let started = std::thread::Builder::new()
            .name("as-case-execution".into())
            .spawn(move || {
                worker.execute_case(worker_cwd, case_id, plan, run.id, grant_id, revision);
            });
        if let Err(error) = started {
            self.finish_run(cwd, case_id, view.id, grant_id, Err(error.to_string()));
            return Err(error.to_string());
        }
        Ok(view)
    }

    fn execute_case(
        &self,
        cwd: PathBuf,
        case_id: Uuid,
        plan: Plan,
        run_id: Uuid,
        grant_id: Uuid,
        revision: u64,
    ) {
        let still_authorized = |data: &super::Data| -> Result<(), String> {
            if !data.enabled
                || data.revision != revision
                || !data.learning.grants.iter().any(|grant| {
                    grant.id == grant_id
                        && valid(grant)
                        && (!grant.from_global_setting
                            || if plan.reply.is_some() {
                                data.decision.auto_run_replies
                            } else {
                                data.decision.auto_run_ready
                            })
                })
                || !data.learning.cases.iter().any(|case| {
                    case.id == case_id
                        && case.upload_allowed(&data.decision)
                        && case.version == plan.version
                        && case.status == cases::Status::Working
                        && case
                            .run
                            .as_ref()
                            .is_some_and(|run| run.id == run_id && run.is_running())
                })
            {
                return Err(
                    "Authorization ended, observation was paused, or the task changed".into(),
                );
            }
            Ok(())
        };
        let check = || still_authorized(&self.data());
        let progress = |changed| {
            let mut data = self.data();
            still_authorized(&data)?;
            let mut next = data.learning.clone();
            let run = next
                .cases
                .iter_mut()
                .find(|case| case.id == case_id)
                .and_then(|case| case.run.as_mut())
                .ok_or("Run no longer exists")?;
            run.status = RunStatus::Running;
            run.started_ms.get_or_insert_with(now_ms);
            run.changed = changed;
            run.detail = if plan.reply.is_some() {
                if changed == 1 {
                    "Preparing approved message".into()
                } else {
                    "Sending approved message".into()
                }
            } else {
                format!("Dispatching step {changed} of {}", plan.steps.len())
            };
            self.commit(&mut data, next)
                .map_err(|error| error.to_string())?;
            Ok(())
        };
        let result = execution::run(&plan, check, progress);
        self.finish_run(&cwd, case_id, run_id, grant_id, result);
    }

    fn finish_run(
        &self,
        cwd: &FsPath,
        case_id: Uuid,
        run_id: Uuid,
        grant_id: Uuid,
        result: Result<String, String>,
    ) {
        let mut data = self.data();
        if data.execution_worker == Some(run_id) {
            data.execution_worker = None;
        }
        let mut next = data.learning.clone();
        let Some(case) = next.cases.iter_mut().find(|case| case.id == case_id) else {
            return;
        };
        let Some(run) = case.run.as_mut().filter(|run| run.id == run_id) else {
            return;
        };
        let succeeded = run.is_running() && result.is_ok();
        if run.is_running() {
            run.ended_ms = Some(now_ms());
            match result {
                Ok(detail) => {
                    run.status = RunStatus::Done;
                    run.detail = detail.clone();
                    case.outcome = Some(detail);
                    case.status = cases::Status::Done;
                    case.plan = None;
                    case.version += 1;
                    case.updated_ms = now_ms();
                    case.completed_ms = Some(case.updated_ms);
                }
                Err(error) => {
                    run.status = if run.changed == 0 {
                        RunStatus::Stopped
                    } else {
                        RunStatus::Unverified
                    };
                    run.detail = cases::limit(&error, 600);
                    case.updated_ms = now_ms();
                }
            }
        }
        next.grants
            .retain(|grant| grant.id != grant_id || grant.mode != Mode::Once);
        if let Err(error) = self.commit(&mut data, next) {
            data.analysis_error = Some(error.to_string());
            return;
        }
        drop(data);
        if succeeded {
            self.schedule_draft(cwd, case_id);
        }
    }
}

fn valid(grant: &Grant) -> bool {
    grant.mode == Mode::Always || grant.expires_ms.is_some_and(|at| at > now_ms())
}
