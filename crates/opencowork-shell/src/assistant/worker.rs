use super::{
    analysis, auto_remaining, cases, context, current_preview, execution, now_ms, screen,
    Assistant, Data, Observation, AUTO_INTERVAL,
};
use base64::{engine::general_purpose::STANDARD, Engine};
use std::{
    path::Path,
    time::{Duration, Instant},
};
use uuid::Uuid;

pub(super) struct PendingSuggestion {
    latest: Observation,
    batch_id: Uuid,
    suggestion: analysis::Suggested,
    tokens: u64,
    image: bool,
    started: Instant,
    settings_revision: u64,
    learning_revision: u64,
    cases: Vec<Uuid>,
}

impl Assistant {
    pub(super) fn run_analysis(&self, cwd: &Path) {
        let mut last_run = None::<Instant>;
        let mut failures = 0u32;
        let mut budget = 8_000;
        let mut last_settings = 0;
        let mut input = None;
        let mut quiet_since = Instant::now();
        loop {
            let data = self.data();
            let (mut data, _) = self
                .shared
                .wake
                .wait_timeout(data, Duration::from_millis(500))
                .unwrap_or_else(|error| error.into_inner());
            let foreground = screen::foreground_input();
            if input != foreground {
                input = foreground;
                quiet_since = Instant::now();
            }
            if data.settings_revision != last_settings {
                last_settings = data.settings_revision;
                last_run = None;
                failures = 0;
                budget = 8_000;
            }
            if !data.enabled || !data.decision.analyze_automatically {
                data.pending = None;
                continue;
            }
            if let Some(pending) = &data.pending {
                if !current(
                    &data,
                    &pending.latest,
                    pending.settings_revision,
                    pending.learning_revision,
                ) {
                    data.pending = None;
                } else if quiet_since.elapsed() >= Duration::from_millis(1800)
                    && data.active_analysis.is_none()
                    && foreground.is_some_and(|(hwnd, pid, stamp)| {
                        hwnd == pending.latest.hwnd
                            && pid == pending.latest.pid
                            && Some(stamp) == pending.latest.input_stamp
                    })
                {
                    let pending = data.pending.take().expect("checked pending suggestion");
                    data.active_analysis = Some(pending.latest.id);
                    drop(data);
                    self.publish_suggestion(cwd, pending, foreground);
                    continue;
                }
            }
            if data.drafting.len() < 2 {
                let next_draft = data
                    .learning
                    .cases
                    .iter()
                    .find(|case| {
                        case.needs_draft(&data.decision)
                            && !data.drafting.contains(&case.id)
                            && data
                                .learning
                                .progress
                                .draft_attempts
                                .get(&case.id)
                                .is_none_or(|time| now_ms().saturating_sub(*time) >= 300_000)
                    })
                    .map(|case| case.id);
                if let Some(id) = next_draft {
                    drop(data);
                    self.schedule_draft(cwd, id);
                    continue;
                }
            }
            let delay = AUTO_INTERVAL.max(Duration::from_secs(5 * (1 << failures.min(5))));
            if data.active_analysis.is_some() || last_run.is_some_and(|last| last.elapsed() < delay)
            {
                continue;
            }
            let mut skips = Vec::new();
            for item in data
                .log
                .iter()
                .filter(|item| item.seq > data.learning.progress.cursor.seq)
            {
                if !item.assistant_action && item.allowed_for_upload(&data.decision) {
                    break;
                }
                skips.push(context::Gap {
                    seq: item.seq,
                    time_ms: item.time_ms,
                    reason: if item.assistant_action {
                        "assistantAction"
                    } else {
                        "uploadExcluded"
                    }
                    .into(),
                });
            }
            if let Some(last) = skips.last() {
                let mut next = data.learning.clone();
                next.progress.cursor = context::Cursor {
                    seq: last.seq,
                    offset: 0,
                };
                next.progress.gaps.extend(skips);
                if let Err(error) = self.commit(&mut data, next) {
                    data.analysis_error = Some(error.to_string());
                    continue;
                }
            }
            if auto_remaining(&data.learning, &data.decision) == 0 {
                continue;
            }
            if data
                .log
                .last()
                .is_none_or(|item| item.seq <= data.learning.progress.cursor.seq)
            {
                continue;
            }
            let mut batch_budget = super::worker_budget(cwd, &data.decision.text_provider, budget);
            let mut batch = context::Batch::build(
                &data.log,
                &data.learning,
                &data.decision,
                batch_budget,
                data.session,
            );
            let crop = batch.as_ref().and_then(|batch| {
                (batch.live
                    && data.decision.use_images
                    && batch.latest.for_upload().text.trim().chars().count() < 60
                    && data.decision.app_policy(&batch.latest.app)
                        == super::decision::AppPolicy::Allow)
                    .then(|| {
                        current_preview(&data)
                            .filter(|preview| preview.observation == batch.latest.id)
                            .and_then(|preview| preview.crop.clone())
                    })
                    .flatten()
            });
            if crop.is_some() {
                batch_budget = super::worker_budget(cwd, &data.decision.vision_provider, budget)
                    .saturating_sub(1024);
                batch = context::Batch::build(
                    &data.log,
                    &data.learning,
                    &data.decision,
                    batch_budget,
                    data.session,
                );
            }
            let Some(batch) = batch else { continue };
            let crop = crop.filter(|_| batch.live);
            let settings = data.decision.clone();
            let settings_revision = data.settings_revision;
            let revision = data.revision;
            let mut next = data.learning.clone();
            let now = now_ms();
            next.auto_calls
                .retain(|at| now.saturating_sub(*at) < 86_400_000);
            next.auto_calls.push(now);
            next.progress
                .attempts
                .retain(|item| now.saturating_sub(item.time_ms) <= super::RETENTION_MS);
            next.progress.attempts.push(context::Attempt {
                id: batch.id,
                start: batch.start,
                through: batch.through,
                time_ms: now,
                used_image: crop.is_some(),
                mode: if batch.input["follow_up"] == true {
                    "followUp"
                } else if batch.live {
                    "current"
                } else {
                    "history"
                }
                .into(),
            });
            if let Err(error) = self.commit(&mut data, next) {
                data.analysis_error = Some(error.to_string());
                last_run = Some(Instant::now());
                continue;
            }
            let learning_revision = data.learning_revision;
            data.active_analysis = Some(batch.latest.id);
            drop(data);

            let started = Instant::now();
            last_run = Some(started);
            let model = if crop.is_some() {
                &settings.vision_provider
            } else {
                &settings.text_provider
            };
            let images = crop
                .as_ref()
                .map(|png| format!("data:image/png;base64,{}", STANDARD.encode(png)))
                .into_iter()
                .collect();
            let instruction = format!(
                "{}\nFor a current-screen suggestion only, use this schema: {}",
                context::INSTRUCTION,
                analysis::INSTRUCTION
            );
            {
                let mut data = self.data();
                if data.settings_revision != settings_revision
                    || data.learning_revision != learning_revision
                    || data.revision != revision
                {
                    data.active_analysis = None;
                    continue;
                }
            }
            let transport = super::super::workflows::assistant_batch_json(
                cwd,
                &instruction,
                batch.input.clone(),
                images,
                model,
            );
            let result = transport.and_then(|(value, tokens)| {
                let response = serde_json::from_value::<context::Response>(value)
                    .map_err(|error| format!("Incomplete observation batch: {error}"))?;
                Ok((response, tokens))
            });

            let mut data = self.data();
            data.active_analysis = None;
            if data.revision != revision
                || data.settings_revision != settings_revision
                || data.learning_revision != learning_revision
            {
                continue;
            }
            let result = result.and_then(|(mut response, tokens)| {
                let mut next = data.learning.clone();
                let follow_up = batch.follow_up(&next, &response);
                if !follow_up.is_empty() {
                    next.progress.evidence_requests = follow_up;
                    next.progress.follow_up_through = Some(batch.through);
                    self.commit(&mut data, next)
                        .map_err(|error| error.to_string())?;
                    return Ok(Vec::new());
                }
                let changed = batch.apply(&mut next, &response, tokens)?;
                self.commit(&mut data, next)
                    .map_err(|error| error.to_string())?;
                if failures == 0
                    && !batch.input["follow_up"].as_bool().unwrap_or(false)
                    && batch.live
                    && data
                        .log
                        .last()
                        .is_some_and(|item| item.id == batch.latest.id)
                {
                    if let Some(suggestion) = response.suggestion.take() {
                        data.pending = Some(PendingSuggestion {
                            latest: batch.latest.for_upload(),
                            batch_id: batch.id,
                            suggestion,
                            tokens,
                            image: crop.is_some(),
                            started,
                            settings_revision,
                            learning_revision: data.learning_revision,
                            cases: changed.clone(),
                        });
                    }
                }
                Ok(changed)
            });
            match result {
                Ok(changed) => {
                    data.analysis_error = None;
                    failures = 0;
                    drop(data);
                    for id in changed {
                        self.schedule_draft(cwd, id);
                    }
                }
                Err(error) => {
                    failures = failures.saturating_add(1);
                    let lower = error.to_lowercase();
                    if lower.contains("context_length")
                        || lower.contains("too long")
                        || lower.contains("capacity")
                        || lower.contains("maximum context")
                    {
                        budget = (budget / 2).max(1000);
                    }
                    data.analysis_error = Some(format!(
                        "Observation {} is pending: {}",
                        batch.start.seq + 1,
                        cases::limit(&error, 300)
                    ));
                }
            }
        }
    }

    fn publish_suggestion(
        &self,
        cwd: &Path,
        pending: PendingSuggestion,
        input: Option<(u64, u32, u32)>,
    ) {
        let settings = self.data().decision.clone();
        let still_current = || {
            let data = self.data();
            current(
                &data,
                &pending.latest,
                pending.settings_revision,
                pending.learning_revision,
            ) && screen::foreground_input() == input
        };
        let selection = analysis::finish_suggestion(
            cwd,
            &settings,
            &pending.latest,
            pending.suggestion,
            pending.tokens,
            pending.image,
            still_current,
        );
        let mut data = self.data();
        data.active_analysis = None;
        if !current(
            &data,
            &pending.latest,
            pending.settings_revision,
            pending.learning_revision,
        ) || screen::foreground_input() != input
        {
            return;
        }
        let Ok((mut prediction, report, proposal)) = selection else {
            return;
        };
        let mut next = data.learning.clone();
        let proposal = proposal.filter(|plan| prediction.missing.is_empty() || plan.is_reply());
        let mut execution_case = None;
        let (proposal, reply_proposal) = match proposal {
            Some(plan) if plan.is_reply() => (None, Some(plan)),
            ordinary => (ordinary, None),
        };
        if let Some(report) = report {
            let mut identity = pending
                .cases
                .iter()
                .copied()
                .filter(|id| Some(*id) == report.case_id);
            let target = identity.next().or_else(|| {
                (pending.cases.len() == 1 && report.case_id.is_none()).then(|| pending.cases[0])
            });
            if let Some(case) = next.cases.iter_mut().find(|case| {
                Some(case.id) == target
                    && case.status.open()
                    && case.observations.contains(&pending.latest.id)
            }) {
                prediction.case_id = Some(case.id);
                if prediction.choice != "observe" && case.request.missing.is_empty() {
                    case.plan = proposal.and_then(|plan| {
                        execution::Plan::from_proposal(case.version, &pending.latest, plan)
                    });
                    if case.plan.is_some() {
                        execution_case = Some(case.id);
                    }
                }
            }
            // A reply has its own authorization and case. Anchor it to the
            // already identified request; never create it while catching up.
            if let Some(plan) = reply_proposal {
                if target.is_some()
                    && report.related_case_id == target
                    && prediction.choice != "observe"
                {
                    let mut reply = report;
                    reply.case_id = None;
                    reply.status = cases::Status::Working;
                    if let Some(id) =
                        cases::record(&mut next.cases, &reply, &pending.latest, Some(plan))
                    {
                        let source_apps = next
                            .cases
                            .iter()
                            .find(|case| Some(case.id) == target)
                            .map(|case| case.source_apps.clone())
                            .unwrap_or_default();
                        if let Some(case) = next.cases.iter_mut().find(|case| case.id == id) {
                            case.record_sources(source_apps);
                        }
                        prediction.case_id = Some(id);
                        execution_case = Some(id);
                    }
                }
            }
        }
        prediction.batch_id = Some(pending.batch_id);
        prediction.live = true;
        prediction.generated_ms = now_ms();
        prediction.elapsed_ms = pending.started.elapsed().as_millis() as u64;
        prediction.evidence_delay_ms = Some(
            prediction
                .generated_ms
                .saturating_sub(pending.latest.time_ms),
        );
        next.predictions
            .retain(|old| old.observation_id != pending.latest.id);
        next.predictions.push(prediction);
        if let Err(error) = self.commit(&mut data, next) {
            data.analysis_error = Some(error.to_string());
            return;
        }
        drop(data);
        if let Some(id) = execution_case {
            self.try_auto_execute(cwd, id);
        }
    }
}

fn current(
    data: &Data,
    item: &Observation,
    settings_revision: u64,
    learning_revision: u64,
) -> bool {
    data.enabled
        && data.phase == "observing"
        && data.decision.analyze_automatically
        && data.settings_revision == settings_revision
        && data.learning_revision == learning_revision
        && data.session == item.session
        && item.session.is_some()
        && now_ms().saturating_sub(item.time_ms) <= 30_000
        && data.log.last().is_some_and(|latest| latest.id == item.id)
}
