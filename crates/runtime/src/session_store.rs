use crate::{Session, SessionError};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionDescriptor {
    pub id: String,
    pub path: PathBuf,
    pub message_count: usize,
    pub updated_at_unix_ms: u128,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionStore {
    root: PathBuf,
}

impl SessionStore {
    #[must_use]
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn save(&self, session: &Session) -> Result<SessionDescriptor, SessionError> {
        let id = format!("session-{}", now_ms());
        self.save_named(&id, session)
    }

    pub fn save_named(
        &self,
        id: &str,
        session: &Session,
    ) -> Result<SessionDescriptor, SessionError> {
        fs::create_dir_all(&self.root)?;
        let path = self.root.join(format!("{id}.json"));
        session.with_id(id).save_to_path(&path)?;
        Ok(SessionDescriptor {
            id: id.to_string(),
            path,
            message_count: session.messages.len(),
            updated_at_unix_ms: now_ms(),
        })
    }

    pub fn load(&self, id: &str) -> Result<Session, SessionError> {
        Session::load_from_path(self.root.join(format!("{id}.json")))
    }

    pub fn list(&self) -> Result<Vec<SessionDescriptor>, SessionError> {
        self.map_sessions(|descriptor, _session| descriptor)
    }

    /// Project each session while it is loaded, retaining only the projected values.
    /// Results have the same newest-first order as `list`.
    pub fn map_sessions<T>(
        &self,
        mut project: impl FnMut(SessionDescriptor, Session) -> T,
    ) -> Result<Vec<T>, SessionError> {
        if !self.root.exists() {
            return Ok(Vec::new());
        }
        let mut descriptors = Vec::new();
        for entry in fs::read_dir(&self.root)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().and_then(|value| value.to_str()) != Some("json") {
                continue;
            }
            let metadata = entry.metadata()?;
            let session = Session::load_from_path(&path)?;
            let id = path
                .file_stem()
                .and_then(|value| value.to_str())
                .unwrap_or("unknown")
                .to_string();
            let updated_at_unix_ms = metadata
                .modified()
                .ok()
                .and_then(|value| value.duration_since(UNIX_EPOCH).ok())
                .map_or(0, |value| value.as_millis());
            let descriptor = SessionDescriptor {
                id,
                path,
                message_count: session.messages.len(),
                updated_at_unix_ms,
            };
            descriptors.push((updated_at_unix_ms, project(descriptor, session)));
        }
        descriptors.sort_by(|left, right| right.0.cmp(&left.0));
        Ok(descriptors.into_iter().map(|(_, value)| value).collect())
    }
}

fn now_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time")
        .as_millis()
}

#[cfg(test)]
mod tests {
    use super::SessionStore;
    use crate::{ConversationMessage, Session};
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_dir() -> std::path::PathBuf {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time")
            .as_nanos();
        std::env::temp_dir().join(format!("opencowork-sessions-{stamp}"))
    }

    #[test]
    fn saves_lists_and_loads_sessions() {
        let root = temp_dir();
        let store = SessionStore::new(&root);
        let session = Session::from_messages(vec![ConversationMessage::user("hello")]);

        let saved = store.save_named("demo", &session).expect("save");
        assert_eq!(saved.id, "demo");
        assert_eq!(store.list().expect("list").len(), 1);
        assert_eq!(store.load("demo").expect("load"), session.with_id("demo"));

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn projects_sessions_with_metadata_in_newest_first_order() {
        let root = temp_dir();
        let store = SessionStore::new(&root);
        assert!(store
            .map_sessions(|descriptor, _| descriptor.id)
            .unwrap()
            .is_empty());

        for (id, seconds) in [("older", 1_700_000_000), ("newer", 1_700_000_010)] {
            let session = Session::from_messages(vec![ConversationMessage::user(id)]);
            let saved = store.save_named(id, &session).unwrap();
            fs::File::options()
                .write(true)
                .open(saved.path)
                .unwrap()
                .set_times(
                    fs::FileTimes::new()
                        .set_modified(UNIX_EPOCH + std::time::Duration::from_secs(seconds)),
                )
                .unwrap();
        }
        fs::write(root.join("ignore.txt"), "not a session").unwrap();
        let projected = store
            .map_sessions(|descriptor, session| {
                assert_eq!(descriptor.message_count, session.messages.len());
                assert_eq!(session, store.load(&descriptor.id).unwrap());
                descriptor.id
            })
            .unwrap();
        assert_eq!(projected, vec!["newer", "older"]);
        assert_eq!(store.list().unwrap()[0].id, "newer");
        fs::remove_dir_all(root).unwrap();
    }
}
