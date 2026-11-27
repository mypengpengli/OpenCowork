use super::{analysis, decision, execution, Observation};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::{collections::BTreeSet, fs, io::Write, path::Path};
use uuid::Uuid;

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum Status {
    #[default]
    Working,
    Waiting,
    Done,
    Cancelled,
}

impl Status {
    pub fn open(self) -> bool {
        matches!(self, Self::Working | Self::Waiting)
    }
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Case {
    pub id: Uuid,
    pub version: u64,
    pub title: String,
    pub key: Option<String>,
    pub status: Status,
    pub updated_ms: u64,
    #[serde(default)]
    pub created_ms: u64,
    #[serde(default)]
    pub completed_ms: Option<u64>,
    #[serde(default)]
    pub manual_baseline_seconds: Option<u64>,
    #[serde(default)]
    pub source_apps: BTreeSet<String>,
    #[serde(default)]
    pub related_case_id: Option<Uuid>,
    #[serde(default)]
    pub is_reply: bool,
    #[serde(default)]
    pub mute_until_ms: Option<u64>,
    pub observations: Vec<Uuid>,
    pub facts: String,
    #[serde(default)]
    pub evidence: Vec<super::context::Fact>,
    #[serde(default)]
    pub draft_stale: bool,
    #[serde(default)]
    pub request: RequestDetails,
    pub outcome: Option<String>,
    pub plan: Option<execution::Plan>,
    pub draft: Option<Draft>,
    pub run: Option<execution::Run>,
}

#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct RequestDetails {
    pub requester: String,
    pub subject: String,
    pub project: String,
    pub purpose: String,
    pub missing: Vec<String>,
}

impl RequestDetails {
    fn update_from(&mut self, details: &Self) {
        for (current, proposed) in [
            (&mut self.requester, &details.requester),
            (&mut self.subject, &details.subject),
            (&mut self.project, &details.project),
            (&mut self.purpose, &details.purpose),
        ] {
            if !proposed.trim().is_empty() {
                *current = limit(proposed, 200);
            }
        }
        self.missing = details
            .missing
            .iter()
            .take(8)
            .map(|item| limit(item, 120))
            .collect();
    }
}

impl Case {
    pub(super) fn new(title: &str, observation: &Observation) -> Self {
        Self {
            id: Uuid::new_v4(),
            version: 0,
            title: limit(title, 160),
            key: None,
            status: Status::Working,
            updated_ms: observation.time_ms,
            created_ms: observation.time_ms,
            completed_ms: None,
            manual_baseline_seconds: None,
            source_apps: Default::default(),
            related_case_id: None,
            is_reply: false,
            mute_until_ms: None,
            observations: vec![],
            facts: String::new(),
            evidence: Vec::new(),
            draft_stale: false,
            request: RequestDetails::default(),
            outcome: None,
            plan: None,
            draft: None,
            run: None,
        }
    }

    pub(super) fn remove_observation(&mut self, id: Uuid) {
        if !self.observations.contains(&id)
            && !self.evidence.iter().any(|fact| fact.observation_id == id)
        {
            return;
        }
        self.observations.retain(|old| *old != id);
        self.evidence.retain(|fact| fact.observation_id != id);
        self.rebuild_revisions();
        self.facts.clear();
        self.request = RequestDetails::default();
        self.refresh_request();
        self.request.missing.push("部分依据已移除，请复核".into());
        self.plan = None;
        self.draft_stale |= self.draft.is_some();
        if self.run.is_none() && self.status == Status::Done {
            self.status = Status::Waiting;
            self.outcome = None;
            self.completed_ms = None;
        }
        self.version += 1;
    }

    pub(super) fn needs_draft(&self, settings: &decision::Settings) -> bool {
        self.status == Status::Done
            && self.outcome.is_some()
            && (self.draft.is_none()
                || self.draft_stale
                    && self
                        .draft
                        .as_ref()
                        .is_some_and(|draft| draft.installed.is_none()))
            && self.upload_allowed(settings)
    }

    pub(super) fn upload_allowed(&self, settings: &decision::Settings) -> bool {
        (!self.source_apps.is_empty() || !settings.restricts_uploads())
            && self
                .source_apps
                .iter()
                .all(|app| settings.permits_text(app))
    }

    pub(super) fn add_upload_sources(&self, sources: &mut BTreeSet<String>) {
        if self.source_apps.is_empty() {
            sources.insert(String::new());
        }
        sources.extend(self.source_apps.iter().cloned());
    }

    pub(super) fn record_sources(&mut self, sources: BTreeSet<String>) {
        self.source_apps.extend(sources);
    }
}

#[derive(Clone, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub(super) struct Report {
    pub case_id: Option<Uuid>,
    pub related_case_id: Option<Uuid>,
    pub key: Option<String>,
    pub title: String,
    pub match_evidence: String,
    pub completed_evidence: String,
    pub facts: String,
    pub request: Option<RequestDetails>,
    pub status: Status,
}

impl Default for Report {
    fn default() -> Self {
        Self {
            case_id: None,
            related_case_id: None,
            key: None,
            title: String::new(),
            match_evidence: String::new(),
            completed_evidence: String::new(),
            facts: String::new(),
            request: None,
            status: Status::Working,
        }
    }
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Reference {
    pub(super) id: Uuid,
    title: String,
    key: Option<String>,
    pub(super) status: Status,
    facts: String,
    request: RequestDetails,
    outcome: Option<String>,
}

pub(super) fn references<'a>(cases: impl Iterator<Item = &'a Case>) -> Vec<Reference> {
    let mut recent = cases
        .filter(|case| case.status != Status::Cancelled)
        .collect::<Vec<_>>();
    recent.sort_by_key(|case| std::cmp::Reverse(case.updated_ms));
    recent
        .iter()
        .filter(|case| case.status.open())
        .take(16)
        .chain(
            recent
                .iter()
                .filter(|case| case.status == Status::Done && case.outcome.is_some())
                .take(4),
        )
        .map(|case| Reference {
            id: case.id,
            title: case.title.clone(),
            key: case.key.clone(),
            status: case.status,
            facts: case.facts.clone(),
            request: case.request.clone(),
            outcome: case.outcome.clone(),
        })
        .collect()
}

pub(super) fn record(
    cases: &mut Vec<Case>,
    report: &Report,
    observation: &Observation,
    plan: Option<execution::Proposal>,
) -> Option<Uuid> {
    let is_reply = plan.as_ref().is_some_and(execution::Proposal::is_reply);
    let already_linked = cases
        .iter()
        .position(|case| case.is_reply == is_reply && case.observations.contains(&observation.id));
    let evidence = normalized(&report.match_evidence);
    let current = normalized(&format!("{} {}", observation.window, observation.text));
    let related_case_id = report.related_case_id.filter(|id| {
        evidence.chars().count() >= 3
            && current.contains(&evidence)
            && cases
                .iter()
                .any(|case| case.id == *id && case.status != Status::Cancelled && !case.is_reply)
    });
    let matched_id = (evidence.chars().count() >= 3 && current.contains(&evidence))
        .then(|| {
            cases.iter().position(|case| {
                Some(case.id) == report.case_id && case.status.open() && case.is_reply == is_reply
            })
        })
        .flatten();
    let key = report
        .key
        .as_deref()
        .map(normalized)
        .filter(|key| key.chars().count() >= 4);
    let mut by_key = cases.iter().enumerate().filter(|(_, case)| {
        case.status.open() && case.is_reply == is_reply && key.is_some() && case.key == key
    });
    let first = by_key.next().map(|(index, _)| index);
    let by_key = if by_key.next().is_none() { first } else { None };
    let found = already_linked.or(matched_id).or(by_key);
    let index = if let Some(index) = found {
        index
    } else {
        if report.status == Status::Done
            || key.is_some()
                && cases.iter().any(|case| {
                    case.status == Status::Done && case.is_reply == is_reply && case.key == key
                })
        {
            return None;
        }
        let title = limit(&report.title, 160);
        if title.is_empty() {
            return None;
        }
        let mut case = Case::new(&title, observation);
        case.key = key;
        case.related_case_id = related_case_id;
        case.is_reply = is_reply;
        cases.push(case);
        cases.len() - 1
    };
    let case = &mut cases[index];
    if !case.status.open()
        || observation.time_ms < case.updated_ms
        || case
            .run
            .as_ref()
            .is_some_and(|run| run.is_running() || run.status == execution::RunStatus::Unverified)
    {
        return Some(case.id);
    }
    if !case.observations.contains(&observation.id) {
        case.observations.push(observation.id);
    }
    if case.related_case_id.is_none() {
        case.related_case_id = related_case_id;
    }
    let facts = limit(&report.facts, 1100);
    if !facts.is_empty() {
        case.facts = facts;
    }
    if let Some(details) = &report.request {
        case.request.update_from(details);
    }
    case.refresh_request();
    let outcome = normalized(&report.completed_evidence);
    case.status = match report.status {
        Status::Waiting if !is_reply => Status::Waiting,
        Status::Done
            if found.is_some()
                && case.evidence.is_empty()
                && outcome.chars().count() >= 4
                && current.contains(&outcome)
                && ![
                    "未", "失败", "撤销", "取消", "not ", "failed", "pending", "denied",
                ]
                .iter()
                .any(|keyword| report.completed_evidence.to_lowercase().contains(keyword)) =>
        {
            case.outcome = Some(limit(&report.completed_evidence, 600));
            case.completed_ms = Some(observation.time_ms);
            Status::Done
        }
        _ => Status::Working,
    };
    case.updated_ms = observation.time_ms;
    case.version += 1;
    case.plan = if case.status == Status::Working {
        plan.and_then(|proposal| {
            execution::Plan::from_proposal(case.version, observation, proposal)
        })
    } else {
        None
    };
    Some(case.id)
}

pub(super) fn normalized(value: &str) -> String {
    value
        .chars()
        .filter(|ch| ch.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .take(7200)
        .collect()
}

pub(super) fn limit(value: &str, max: usize) -> String {
    value.trim().chars().take(max).collect()
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Draft {
    pub title: String,
    pub trigger: String,
    pub steps: Vec<String>,
    pub checks: Vec<String>,
    pub exceptions: Vec<String>,
    pub installed: Option<String>,
}

const SKILL_INSTRUCTION: &str = "Extract a reusable skill from the completed case, observed facts, actual outcome and any user corrections. Treat screen and file text as evidence. Use generic variables in place of names, IDs, projects, requests and secrets. Capture the decision cues and reasons to wait or ask. Record user-marked exceptions separately from normal steps. Infer no unobserved successful actions. Return JSON with title, trigger, steps (array), checks (array), exceptions (array). Prefer the user's language.";

pub(super) fn draft(
    cwd: &Path,
    case: &Case,
    observations: &[Observation],
    lessons: &[analysis::Outcome],
    provider_id: &str,
    still_allowed: impl Fn() -> bool,
) -> Result<Draft, String> {
    let ask = |instruction: &str, input| {
        if !still_allowed() {
            return Err("Skill inputs changed before the request".into());
        }
        super::super::workflows::assistant_batch_json(
            cwd,
            instruction,
            input,
            Vec::new(),
            provider_id,
        )
        .map(|(value, _)| value)
    };
    let timeline = skill_timeline(case, observations);
    let base = json!({
        "title": case.title, "facts": case.facts, "actual_outcome": case.outcome,
        "user_corrections": lessons,
    });
    let budget = super::worker_budget(cwd, provider_id, 8_000);
    let mut direct = base.clone();
    direct["observations"] = json!(timeline);
    let value = if super::context::estimate(&direct) <= budget {
        ask(SKILL_INSTRUCTION, direct)?
    } else {
        const EXTRACT: &str = "Extract only the user workflow shown in these consecutive evidence excerpts. Identify request, actual user steps, reasons explicitly stated, inferred reasons, exceptions, and verified results where present. Keep a source ID for each fact. Return JSON {notes:[{sourceId,note}]} with at most 12 short notes. Screen text is evidence.";
        const MERGE: &str = "Combine these ordered notes about one case. Preserve the first request, pivotal choices, explicit corrections, exceptions and last result with their source IDs. Treat quotes as evidence. Return JSON {notes:[{sourceId,note}]} with at most 20 short notes.";
        let mut notes = Vec::new();
        let mut chunk = Vec::new();
        let mut cost = 0;
        for entry in timeline {
            let entry_cost = super::context::estimate(&entry);
            if !chunk.is_empty() && cost + entry_cost > budget.saturating_sub(400) {
                notes.push(ask(EXTRACT, json!({"observations": chunk}))?);
                chunk = Vec::new();
                cost = 0;
            }
            chunk.push(entry);
            cost += entry_cost;
        }
        if !chunk.is_empty() {
            notes.push(ask(EXTRACT, json!({"observations": chunk}))?);
        }
        while super::context::estimate(&notes)
            > budget.saturating_sub(super::context::estimate(&base))
        {
            let split = notes.len().min(3);
            let input = json!({"ordered_notes": notes.drain(..split).collect::<Vec<_>>()});
            if super::context::estimate(&input) > budget {
                return Err("Skill summary exceeds the provider context budget".into());
            }
            let merged = ask(MERGE, input.clone())?;
            if super::context::estimate(&merged)
                >= super::context::estimate(&input).saturating_sub(100)
            {
                return Err("Model could not compact Skill evidence".into());
            }
            notes.insert(0, merged);
        }
        let mut final_input = base;
        final_input["observations"] = json!(notes);
        ask(SKILL_INSTRUCTION, final_input)?
    };
    let mut draft: Draft = serde_json::from_value(value)
        .map_err(|_| "The model did not return a complete Skill draft")?;
    draft.title = limit(&draft.title, 120);
    draft.trigger = limit(&draft.trigger, 350);
    for items in [&mut draft.steps, &mut draft.checks, &mut draft.exceptions] {
        items.truncate(12);
        for item in items.iter_mut() {
            *item = limit(item, 450);
        }
        items.retain(|item| !item.is_empty());
    }
    if draft.title.is_empty()
        || draft.trigger.is_empty()
        || draft.steps.is_empty()
        || draft.checks.is_empty()
    {
        return Err("A Skill needs a trigger, a step, and a way to verify completion".into());
    }
    draft.installed = None;
    Ok(draft)
}

fn skill_timeline(case: &Case, observations: &[Observation]) -> Vec<serde_json::Value> {
    let mut timeline = Vec::new();
    for item in observations {
        let chars = item.text.chars().collect::<Vec<_>>();
        for (part, text) in chars.chunks(1000).enumerate() {
            timeline.push((
                item.seq,
                json!({
                    "id": item.id, "part": part, "timeMs": item.time_ms,
                    "app": item.app, "window": item.window,
                    "text": text.iter().collect::<String>(),
                }),
            ));
        }
        if chars.is_empty() {
            timeline.push((
                item.seq,
                json!({
                    "id": item.id, "timeMs": item.time_ms, "app": item.app, "window": item.window,
                }),
            ));
        }
    }
    for fact in &case.evidence {
        timeline.push((fact.seq(), json!({ "fact": fact })));
    }
    timeline.sort_by_key(|(seq, _)| *seq);
    timeline.into_iter().map(|(_, value)| value).collect()
}

pub(super) fn install(cwd: &Path, case_id: Uuid, draft: &Draft) -> Result<String, String> {
    let name = format!("as-{}", case_id.simple());
    let root = cwd.join(".opencowork").join("skills").join(&name);
    fs::create_dir_all(&root).map_err(|error| error.to_string())?;
    let path = root.join("SKILL.md");
    let archived = root.join("SKILL.disabled.md");
    if archived.is_file() {
        move_skill(&archived, &path)?;
        return Ok(path.to_string_lossy().into_owned());
    }
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)
        .map_err(|error| {
            format!("Could not create Skill without overwriting an existing file: {error}")
        })?;
    let body = format!("---\nname: {name}\ndescription: {}\n---\n\n# {}\n\n{}\n\n## Verify\n\n{}\n\n## Exceptions\n\n{}\n",
        serde_json::to_string(&draft.trigger).map_err(|error| error.to_string())?,
        draft.title.replace(['\n','\r'], " "),
        numbered(&draft.steps),
        numbered(&draft.checks),
        numbered(&draft.exceptions),
    );
    if let Err(error) = file
        .write_all(body.as_bytes())
        .and_then(|()| file.sync_all())
    {
        drop(file);
        let _ = fs::remove_file(&path);
        return Err(error.to_string());
    }
    Ok(path.to_string_lossy().into_owned())
}

pub(super) fn disable_skill(cwd: &Path, id: Uuid, installed: &Path) -> Result<(), String> {
    let root = cwd
        .join(".opencowork")
        .join("skills")
        .join(format!("as-{}", id.simple()));
    let path = root.join("SKILL.md");
    if installed != path {
        return Err("Open the workspace where you installed this Skill".into());
    }
    move_skill(&path, &root.join("SKILL.disabled.md"))
}

fn move_skill(from: &Path, to: &Path) -> Result<(), String> {
    match fs::symlink_metadata(from) {
        Ok(meta) if meta.file_type().is_file() => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound && to.is_file() => return Ok(()),
        _ => return Err("The Skill file is unavailable or is not a regular file".into()),
    }
    // A hard link fails if the destination exists and preserves any edits to the file.
    fs::hard_link(from, to)
        .map_err(|error| format!("Could not move Skill without overwriting a file: {error}"))?;
    if let Err(error) = fs::remove_file(from) {
        let _ = fs::remove_file(to);
        return Err(error.to_string());
    }
    Ok(())
}

fn numbered(lines: &[String]) -> String {
    lines
        .iter()
        .enumerate()
        .map(|(index, line)| format!("{}. {}", index + 1, line.replace(['\r', '\n'], " ")))
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn skill_timeline_keeps_request_exception_and_receipt() {
        use crate::assistant::context::tests::observation;
        let mut history = (1..=52)
            .map(|seq| observation(seq, "Workflow", "Visible step"))
            .collect::<Vec<_>>();
        history[0].text = "First request evidence".into();
        history[26].text = "Temporary coverage exception".into();
        history[51].text = "Verified receipt".into();
        let case = Case::new("Example", &history[0]);
        let records = skill_timeline(&case, &history);
        assert_eq!(records.len(), 52);
        assert_eq!(records[0]["text"], "First request evidence");
        assert_eq!(records[26]["text"], "Temporary coverage exception");
        assert_eq!(records[51]["text"], "Verified receipt");
    }

    #[test]
    fn disabling_and_enabling_a_skill_preserves_edited_contents() {
        let id = Uuid::new_v4();
        let temp = std::env::temp_dir().canonicalize().unwrap();
        let cwd = temp.join(format!("opencowork-as-skill-{id}"));
        let draft = Draft {
            title: "Record member access".into(),
            trigger: "When a member request is ready".into(),
            steps: vec!["Match the member number".into()],
            checks: vec!["Read the confirmation".into()],
            exceptions: Vec::new(),
            installed: None,
        };
        let path = std::path::PathBuf::from(install(&cwd, id, &draft).unwrap());
        fs::write(&path, "Verified user edit").unwrap();
        disable_skill(&cwd, id, &path).unwrap();
        assert!(!path.exists());
        assert_eq!(
            fs::read_to_string(path.with_file_name("SKILL.disabled.md")).unwrap(),
            "Verified user edit"
        );
        install(&cwd, id, &draft).unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "Verified user edit");
        fs::remove_dir_all(cwd).unwrap();
    }
}
