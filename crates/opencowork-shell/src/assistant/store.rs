use super::{Learning, Observation};
use rusqlite::{params, Connection, OptionalExtension, Transaction};
use serde::de::DeserializeOwned;
use std::{fs, io, path::Path, sync::Mutex, time::Duration};
use uuid::Uuid;

pub(super) struct Store {
    connection: Mutex<Connection>,
    // One live writer per configuration home. Windows releases this lock on exit,
    // including a crash. HTTP and model requests never hold a SQLite transaction.
    _writer: fs::File,
}

impl Store {
    pub fn open(root: &Path) -> anyhow::Result<(Self, Vec<Observation>, Learning)> {
        fs::create_dir_all(root)?;
        let writer = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(root.join("writer.lock"))?;
        writer.try_lock().map_err(|error| {
            anyhow::anyhow!("AS storage is in use or cannot be locked: {error}")
        })?;
        let mut connection = Connection::open(root.join("assistant.sqlite3"))?;
        connection.busy_timeout(Duration::from_secs(3))?;
        connection.execute_batch(
            "PRAGMA journal_mode = WAL;
             PRAGMA synchronous = FULL;
             CREATE TABLE IF NOT EXISTS observations (
                seq INTEGER PRIMARY KEY AUTOINCREMENT,
                id TEXT NOT NULL UNIQUE,
                time_ms INTEGER NOT NULL,
                body TEXT NOT NULL
             );
             CREATE INDEX IF NOT EXISTS observations_time ON observations(time_ms);
             CREATE TABLE IF NOT EXISTS learning (
                id INTEGER PRIMARY KEY CHECK (id = 1),
                body TEXT NOT NULL
             );",
        )?;
        let transaction = connection.transaction()?;
        let state: Option<String> = transaction
            .query_row("SELECT body FROM learning WHERE id = 1", [], |row| {
                row.get(0)
            })
            .optional()?;
        let learning = match state {
            Some(state) => serde_json::from_str(&state)?,
            None => {
                let log: Vec<Observation> = legacy(root, "observations.json")?;
                let mut learning: Learning = legacy(root, "learning.json")?;
                learning.progress.legacy_coverage_unknown =
                    !log.is_empty() || !learning.cases.is_empty();
                for observation in log {
                    insert(&transaction, &observation)?;
                }
                write_learning(&transaction, &learning)?;
                learning
            }
        };
        let log = {
            let mut query =
                transaction.prepare("SELECT seq, body FROM observations ORDER BY seq")?;
            let rows = query.query_map([], |row| {
                Ok((row.get::<_, u64>(0)?, row.get::<_, String>(1)?))
            })?;
            let mut log = Vec::new();
            for row in rows {
                let (seq, body) = row?;
                let mut item: Observation = serde_json::from_str(&body)?;
                item.seq = seq;
                log.push(item);
            }
            log
        };
        transaction.commit()?;
        Ok((
            Self {
                connection: Mutex::new(connection),
                _writer: writer,
            },
            log,
            learning,
        ))
    }

    pub fn append(&self, observation: &mut Observation) -> io::Result<()> {
        self.transaction(|tx| {
            observation.seq = insert(tx, observation)?;
            Ok(())
        })
    }

    pub fn save(&self, learning: &Learning, remove: &[Uuid]) -> io::Result<()> {
        self.transaction(|tx| {
            write_learning(tx, learning)?;
            if !remove.is_empty() {
                let mut delete = tx.prepare("DELETE FROM observations WHERE id = ?")?;
                for id in remove {
                    delete.execute([id.to_string()])?;
                }
            }
            Ok(())
        })
    }

    fn transaction<T>(
        &self,
        operation: impl FnOnce(&Transaction<'_>) -> anyhow::Result<T>,
    ) -> io::Result<T> {
        let mut connection = self
            .connection
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        let apply = || -> anyhow::Result<T> {
            let tx = connection.transaction()?;
            let value = operation(&tx)?;
            tx.commit()?;
            Ok(value)
        };
        apply().map_err(|error| io::Error::other(error.to_string()))
    }
}

fn legacy<T: DeserializeOwned + Default>(root: &Path, filename: &str) -> anyhow::Result<T> {
    let path = root.join(filename);
    match fs::read(&path) {
        Ok(bytes) => serde_json::from_slice(&bytes)
            .map_err(|error| anyhow::anyhow!("Could not migrate {filename}: {error}")),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(T::default()),
        Err(error) => Err(error.into()),
    }
}

fn insert(tx: &Transaction<'_>, item: &Observation) -> anyhow::Result<u64> {
    tx.execute(
        "INSERT INTO observations (id, time_ms, body) VALUES (?, ?, ?)",
        params![
            item.id.to_string(),
            item.time_ms,
            serde_json::to_string(item)?
        ],
    )?;
    Ok(tx.last_insert_rowid().try_into()?)
}

fn write_learning(tx: &Transaction<'_>, learning: &Learning) -> anyhow::Result<()> {
    tx.execute(
        "INSERT INTO learning (id, body) VALUES (1, ?)
         ON CONFLICT (id) DO UPDATE SET body = excluded.body",
        [serde_json::to_string(learning)?],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assistant::{context, now_ms};

    fn root() -> std::path::PathBuf {
        std::env::temp_dir().join(format!("opencowork-as-store-{}", Uuid::new_v4()))
    }

    #[test]
    fn migration_is_once_and_checkpoint_and_deletion_are_atomic() {
        let path = root();
        fs::create_dir_all(&path).unwrap();
        let mut first = context::tests::observation(0, "Chat", "Original evidence");
        first.time_ms = now_ms();
        fs::write(
            path.join("observations.json"),
            serde_json::to_vec(&[&first]).unwrap(),
        )
        .unwrap();
        let (store, log, mut learning) = Store::open(&path).unwrap();
        assert_eq!(log[0].id, first.id);
        assert!(learning.progress.legacy_coverage_unknown);
        assert!(Store::open(&path).is_err());
        let mut second = context::tests::observation(0, "Attachment", "Key A71");
        // A clock correction must not change the database ordering.
        second.time_ms = first.time_ms - 1000;
        store.append(&mut second).unwrap();
        assert!(second.seq > log[0].seq);
        {
            let db = store.connection.lock().unwrap();
            db.execute_batch(
                "CREATE TRIGGER fail_delete BEFORE DELETE ON observations
                BEGIN SELECT RAISE(ABORT, 'injected failure'); END;",
            )
            .unwrap();
        }
        learning.progress.cursor.seq = second.seq;
        assert!(store.save(&learning, &[first.id]).is_err());
        drop(store);
        let (store, log, persisted) = Store::open(&path).unwrap();
        assert_eq!(persisted.progress.cursor.seq, 0);
        assert_eq!(
            log.iter().map(|item| item.id).collect::<Vec<_>>(),
            [first.id, second.id]
        );
        store
            .connection
            .lock()
            .unwrap()
            .execute_batch("DROP TRIGGER fail_delete")
            .unwrap();
        store.save(&learning, &[first.id]).unwrap();
        drop(store);
        let (store, log, persisted) = Store::open(&path).unwrap();
        assert_eq!(persisted.progress.cursor.seq, second.seq);
        assert_eq!(log.len(), 1);
        assert_eq!(log[0].id, second.id);
        drop(store);
        fs::remove_dir_all(&path).unwrap();
    }

    #[test]
    fn malformed_snapshot_can_be_fixed_without_losing_the_original() {
        let path = root();
        fs::create_dir_all(&path).unwrap();
        let legacy = path.join("observations.json");
        fs::write(&legacy, b"{broken").unwrap();
        assert!(Store::open(&path).is_err());
        assert_eq!(fs::read(&legacy).unwrap(), b"{broken");
        fs::write(&legacy, b"[]").unwrap();
        let (store, log, _) = Store::open(&path).unwrap();
        assert!(log.is_empty());
        drop(store);
        fs::remove_dir_all(&path).unwrap();
    }
}
