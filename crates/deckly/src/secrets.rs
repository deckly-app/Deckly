//! Secrets of plugin settings forms (passwords, tokens).
//!
//! A secret's value never goes into a profile, a profile export, a backup or
//! a plugin's global settings file. Those hold a reference instead,
//! `{"deckly_secret": "<id>"}`, and the value lives in the system's secret
//! store (`os::secrets`) under the plugin's id and that reference. On a
//! system without a usable store, the values go into one file in Deckly's
//! data folder that only the user can read, and the form says so. When a
//! store is found later, the file's secrets move into it and the file goes.
//!
//! Deckly plugins never see references: [`reveal`] puts the values back on
//! the way to the plugin, and [`keep`] takes them out of settings a plugin
//! sends back.
//!
//! The editor never waits for the store: [`Vault::peek`] answers from the
//! cache and reads a missing value on the secrets thread, and
//! [`Vault::set_later`] puts a new value into the cache at once and writes it
//! on that thread. A write that fails takes its value back out of the cache
//! and leaves a message for the form ([`Vault::take_write_error`]).

use crate::os::secrets::{self as system, Name};
use deckly_plugin_host::manifest::valid_uuid;
use deckly_plugin_host::proto::Form;
use serde_json::{Map, Value, json};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{SyncSender, sync_channel};
use std::sync::{Mutex, MutexGuard, OnceLock};
use std::time::{Duration, Instant};

/// The property of a reference object.
pub const MARK: &str = "deckly_secret";
/// The fallback file's name in the data folder.
const FILE: &str = "secrets.json";
/// A fallback file larger than this is not read.
const MAX_FILE_BYTES: u64 = 1024 * 1024;
/// A secret longer than this is refused.
const MAX_SECRET_BYTES: usize = 8 * 1024;
/// How long a store that failed is left alone before it is asked again, so
/// a locked keyring does not slow every frame down.
const RETRY_AFTER: Duration = Duration::from_secs(30);
/// Reads and writes waiting for the secrets thread; more are refused.
const MAX_JOBS: usize = 128;

/// A reference to a secret, as the settings keep it.
pub fn reference(id: &str) -> Value {
    json!({ MARK: id })
}

/// The id in a reference.
pub fn reference_id(v: &Value) -> Option<&str> {
    let o = v.as_object().filter(|o| o.len() == 1)?;
    o.get(MARK)?.as_str().filter(|id| valid_id(id))
}

/// Ids are Deckly's own (UUIDs, `global.<field>`); anything else in a
/// reference was not written by Deckly and is ignored.
fn valid_id(id: &str) -> bool {
    (1..=96).contains(&id.len())
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'))
}

/// A new id for a key's secret. Keys have no stable id of their own, so
/// the reference carries one; a copied key shares its secret until either
/// copy is given a new one.
pub fn new_id() -> String {
    uuid::Uuid::new_v4().to_string()
}

/// The id of a secret in a plugin's global settings.
pub fn global_id(field: &str) -> String {
    format!("global.{field}")
}

/// Where the values are kept.
enum Backend {
    System,
    File(PathBuf),
}

/// A secret as the editor can have it without waiting.
#[derive(Clone, Debug, PartialEq)]
pub enum Peek {
    /// The value; `None` when there is none.
    Ready(Option<String>),
    /// The store could not be read.
    Failed(String),
    /// Being read; ask again on a later frame.
    Loading,
}

type Slot = (String, String);

pub struct Vault {
    backend: Backend,
    /// Held while the store itself is read or written, so the file
    /// backend's read-modify-write never interleaves.
    io: Mutex<()>,
    /// Why the system's store is not used; shown under secret fields.
    fallback: Option<String>,
    /// Values read or written, by plugin and id; `None` is known missing.
    cache: Mutex<HashMap<(String, String), Option<String>>>,
    /// Reads that failed, with when and why.
    failed: Mutex<HashMap<(String, String), (Instant, String)>>,
    /// Slots being read or written on the secrets thread.
    busy: Mutex<HashMap<Slot, usize>>,
    /// Writes that failed, until a form shows them.
    write_errors: Mutex<HashMap<Slot, String>>,
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|p| p.into_inner())
}

static VAULT: OnceLock<Vault> = OnceLock::new();

/// The vault, set up on first use. That first use may wait for the
/// system's store to answer (a few seconds at most), so the app starts it
/// on a thread of its own early ([`init`]).
pub fn vault() -> &'static Vault {
    VAULT.get_or_init(Vault::open)
}

/// The vault if it is set up; `None` while the system's store is still
/// being probed. The editor uses this so it never waits for the probe.
pub fn ready() -> Option<&'static Vault> {
    VAULT.get()
}

/// A read or write for the secrets thread.
type Job = Box<dyn FnOnce() + Send>;

/// Runs jobs one after another on the secrets thread.
fn run_later(job: Job) -> Result<(), String> {
    static JOBS: OnceLock<Option<SyncSender<Job>>> = OnceLock::new();
    let jobs = JOBS.get_or_init(|| {
        let (tx, rx) = sync_channel::<Job>(MAX_JOBS);
        std::thread::Builder::new()
            .name("deckly-secret-jobs".into())
            .spawn(move || {
                while let Ok(job) = rx.recv() {
                    job();
                }
            })
            .ok()
            .map(|_| tx)
    });
    let jobs = jobs
        .as_ref()
        .ok_or("The secret store thread did not start.")?;
    jobs.try_send(job)
        .map_err(|_| "The secret store is busy. Try again.".to_owned())
}

/// Sets the vault up off the UI thread.
pub fn init() {
    let _ = std::thread::Builder::new()
        .name("deckly-secrets".into())
        .spawn(|| {
            vault();
        });
}

fn fallback_file() -> Option<PathBuf> {
    crate::store::data_dir().map(|d| d.join(FILE))
}

impl Vault {
    fn open() -> Self {
        let Some(file) = fallback_file() else {
            // No data folder: nothing can be kept but by the system.
            return Self::with(Backend::System, None);
        };
        // Tests never touch the user's keyring.
        if cfg!(test) {
            return Self::with(Backend::File(file), Some("tests".into()));
        }
        if std::env::var_os("DECKLY_SECRET_STORE").is_some_and(|v| v == "file") {
            let why = "DECKLY_SECRET_STORE is set to file".to_owned();
            return Self::with(Backend::File(file), Some(why));
        }
        match system::probe() {
            Ok(()) => match migrate(&file) {
                Ok(()) => Self::with(Backend::System, None),
                Err(e) => {
                    let why = format!("the secrets in {} could not be moved: {e}", file.display());
                    Self::with(Backend::File(file), Some(why))
                }
            },
            Err(e) => Self::with(Backend::File(file), Some(e)),
        }
    }

    fn with(backend: Backend, fallback: Option<String>) -> Self {
        Self {
            backend,
            io: Mutex::new(()),
            fallback,
            cache: Mutex::new(HashMap::new()),
            failed: Mutex::new(HashMap::new()),
            busy: Mutex::new(HashMap::new()),
            write_errors: Mutex::new(HashMap::new()),
        }
    }

    /// A vault that keeps its secrets in `file`, for tests.
    #[cfg(test)]
    pub fn in_file(file: PathBuf) -> Self {
        Self::with(Backend::File(file), Some("tests".into()))
    }

    /// Why secrets are kept in Deckly's own file rather than the system's
    /// store; `None` when the system's store is used.
    pub fn fallback(&self) -> Option<&str> {
        self.fallback.as_deref()
    }

    /// A secret's value; `Ok(None)` when there is none.
    pub fn get(&self, plugin: &str, id: &str) -> Result<Option<String>, String> {
        if !valid_uuid(plugin) || !valid_id(id) {
            return Err("not a secret of a known plugin".into());
        }
        let slot = (plugin.to_owned(), id.to_owned());
        if let Some(v) = lock(&self.cache).get(&slot) {
            return Ok(v.clone());
        }
        if let Some((at, why)) = lock(&self.failed).get(&slot)
            && at.elapsed() < RETRY_AFTER
        {
            return Err(why.clone());
        }
        let name = Name { plugin, id };
        let read = {
            let _io = lock(&self.io);
            match &self.backend {
                Backend::System => system::read(name),
                Backend::File(file) => read_file(file).map(|all| file_value(&all, plugin, id)),
            }
        };
        match read {
            Ok(v) => {
                lock(&self.failed).remove(&slot);
                // A value set while this read waited is newer than the read.
                let v = lock(&self.cache).entry(slot).or_insert(v).clone();
                Ok(v)
            }
            Err(e) => {
                lock(&self.failed).insert(slot, (Instant::now(), e.clone()));
                Err(e)
            }
        }
    }

    /// Stores a value; an empty one removes the secret.
    pub fn set(&self, plugin: &str, id: &str, value: &str) -> Result<(), String> {
        if value.len() > MAX_SECRET_BYTES {
            return Err(format!("Use at most {MAX_SECRET_BYTES} bytes."));
        }
        if !valid_uuid(plugin) || !valid_id(id) {
            return Err("not a secret of a known plugin".into());
        }
        self.write(plugin, id, value)?;
        let slot = (plugin.to_owned(), id.to_owned());
        lock(&self.failed).remove(&slot);
        lock(&self.cache).insert(slot, (!value.is_empty()).then(|| value.to_owned()));
        Ok(())
    }

    /// Writes to the store itself, without touching the cache.
    fn write(&self, plugin: &str, id: &str, value: &str) -> Result<(), String> {
        let name = Name { plugin, id };
        let _io = lock(&self.io);
        match (&self.backend, value.is_empty()) {
            (Backend::System, false) => system::write(name, value),
            (Backend::System, true) => system::delete(name),
            (Backend::File(file), _) => update_file(file, plugin, id, value),
        }
    }

    /// A secret from the cache, never waiting: one not read yet is read on
    /// the secrets thread and is `Loading` until then.
    pub fn peek(&'static self, plugin: &str, id: &str) -> Peek {
        let slot = (plugin.to_owned(), id.to_owned());
        if let Some(v) = lock(&self.cache).get(&slot) {
            return Peek::Ready(v.clone());
        }
        if let Some((at, why)) = lock(&self.failed).get(&slot)
            && at.elapsed() < RETRY_AFTER
        {
            return Peek::Failed(why.clone());
        }
        if lock(&self.busy).contains_key(&slot) {
            return Peek::Loading;
        }
        let job_slot = slot.clone();
        self.start(slot.clone());
        let read = run_later(Box::new(move || {
            let _ = self.get(&job_slot.0, &job_slot.1);
            self.finish(&job_slot);
        }));
        match read {
            Ok(()) => Peek::Loading,
            Err(why) => {
                self.finish(&slot);
                Peek::Failed(why)
            }
        }
    }

    /// Stores a value without waiting: the cache has it at once, so the
    /// plugin and the form see it, and the store gets it on the secrets
    /// thread. An empty value removes the secret.
    pub fn set_later(&'static self, plugin: &str, id: &str, value: &str) -> Result<(), String> {
        if value.len() > MAX_SECRET_BYTES {
            return Err(format!("Use at most {MAX_SECRET_BYTES} bytes."));
        }
        if !valid_uuid(plugin) || !valid_id(id) {
            return Err("not a secret of a known plugin".into());
        }
        let slot: Slot = (plugin.to_owned(), id.to_owned());
        let stored = (!value.is_empty()).then(|| value.to_owned());
        // Into the cache before the job can run, so a failed write finds it.
        let previous = lock(&self.cache).insert(slot.clone(), stored.clone());
        self.start(slot.clone());
        let job_slot = slot.clone();
        let job_value = stored.clone();
        let queued = run_later(Box::new(move || {
            let (plugin, id) = (&job_slot.0, &job_slot.1);
            let written = self.write(plugin, id, job_value.as_deref().unwrap_or_default());
            match written {
                Ok(()) => {
                    lock(&self.write_errors).remove(&job_slot);
                }
                Err(e) => {
                    // Only this write's value leaves the cache; a later one
                    // stays until its own write ends.
                    let mut cache = lock(&self.cache);
                    if cache.get(&job_slot) == Some(&job_value) {
                        cache.remove(&job_slot);
                    }
                    drop(cache);
                    lock(&self.write_errors).insert(job_slot.clone(), e);
                }
            }
            self.finish(&job_slot);
        }));
        if let Err(why) = queued {
            self.finish(&slot);
            let mut cache = lock(&self.cache);
            match previous {
                Some(v) => cache.insert(slot, v),
                None => cache.remove(&slot),
            };
            return Err(why);
        }
        lock(&self.failed).remove(&slot);
        Ok(())
    }

    /// Why the last write of a secret failed, once.
    pub fn take_write_error(&self, plugin: &str, id: &str) -> Option<String> {
        lock(&self.write_errors).remove(&(plugin.to_owned(), id.to_owned()))
    }

    /// Whether a read or write is still running on the secrets thread.
    pub fn busy(&self) -> bool {
        !lock(&self.busy).is_empty()
    }

    fn start(&self, slot: Slot) {
        *lock(&self.busy).entry(slot).or_default() += 1;
    }

    fn finish(&self, slot: &Slot) {
        let mut busy = lock(&self.busy);
        if let Some(n) = busy.get_mut(slot) {
            *n = n.saturating_sub(1);
            if *n == 0 {
                busy.remove(slot);
            }
        }
    }
}

/// The fallback file: `{"plugin id": {"secret id": "value"}}`.
fn read_file(file: &Path) -> Result<Map<String, Value>, String> {
    let len = match std::fs::metadata(file) {
        Ok(m) => m.len(),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Map::new()),
        Err(e) => return Err(format!("cannot read {}: {e}", file.display())),
    };
    if len > MAX_FILE_BYTES {
        return Err(format!("{} is too large", file.display()));
    }
    // The file may grow after the size check.
    use std::io::Read;
    let mut text = String::new();
    std::fs::File::open(file)
        .and_then(|f| f.take(MAX_FILE_BYTES + 1).read_to_string(&mut text))
        .map_err(|e| format!("cannot read {}: {e}", file.display()))?;
    if text.len() as u64 > MAX_FILE_BYTES {
        return Err(format!("{} is too large", file.display()));
    }
    match serde_json::from_str::<Value>(&text) {
        Ok(Value::Object(all)) => Ok(all),
        _ => Err(format!("{} is not valid", file.display())),
    }
}

fn file_value(all: &Map<String, Value>, plugin: &str, id: &str) -> Option<String> {
    all.get(plugin)?.get(id)?.as_str().map(str::to_owned)
}

fn update_file(file: &Path, plugin: &str, id: &str, value: &str) -> Result<(), String> {
    let mut all = read_file(file)?;
    let entry = all
        .entry(plugin.to_owned())
        .or_insert_with(|| Value::Object(Map::new()));
    if !entry.is_object() {
        *entry = Value::Object(Map::new());
    }
    if let Some(secrets) = entry.as_object_mut() {
        if value.is_empty() {
            secrets.remove(id);
        } else {
            secrets.insert(id.to_owned(), Value::String(value.to_owned()));
        }
    }
    all.retain(|_, v| v.as_object().is_some_and(|o| !o.is_empty()));
    write_private(file, &Value::Object(all).to_string())
}

/// Writes a file only the user can read, through a temporary file so a
/// crash leaves the old one whole.
fn write_private(file: &Path, text: &str) -> Result<(), String> {
    use std::io::Write;
    let fail = |e: std::io::Error| format!("cannot write {}: {e}", file.display());
    if let Some(dir) = file.parent() {
        std::fs::create_dir_all(dir).map_err(fail)?;
    }
    let dir = file.parent().ok_or("the secret file has no folder")?;
    // An unpredictable name, opened exclusively, 0600 on Unix; on Windows
    // the data folder is inside the user's own profile, which other users
    // cannot read. Persisting replaces the old file in one step.
    let mut tmp = tempfile::NamedTempFile::new_in(dir).map_err(fail)?;
    tmp.write_all(text.as_bytes())
        .and_then(|()| tmp.as_file().sync_all())
        .map_err(fail)?;
    tmp.persist(file).map_err(|e| fail(e.error))?;
    Ok(())
}

/// Moves the fallback file's secrets into the system's store and removes
/// the file. Leaves the file alone if any secret cannot be moved.
fn migrate(file: &Path) -> Result<(), String> {
    if !file.exists() {
        return Ok(());
    }
    let all = read_file(file)?;
    // Nothing moves unless every entry is one Deckly could have written.
    for (plugin, values) in &all {
        let values = values
            .as_object()
            .filter(|_| valid_uuid(plugin))
            .ok_or("it holds an entry Deckly did not write")?;
        for (id, value) in values {
            if !valid_id(id) || value.as_str().is_none_or(|v| v.len() > MAX_SECRET_BYTES) {
                return Err("it holds an entry Deckly did not write".into());
            }
        }
    }
    for (plugin, secrets) in &all {
        let Some(secrets) = secrets.as_object() else {
            continue;
        };
        for (id, value) in secrets {
            if let Some(value) = value.as_str().filter(|v| !v.is_empty()) {
                system::write(Name { plugin, id }, value)?;
            }
        }
    }
    std::fs::remove_file(file).map_err(|e| format!("cannot remove {}: {e}", file.display()))
}

/// Puts secret values in place of the references in a Deckly plugin's
/// settings, on their way to the plugin. A secret that cannot be read
/// reaches the plugin as an empty string.
pub fn reveal(vault: &Vault, plugin: &str, settings: &mut Value) {
    let Some(map) = settings.as_object_mut() else {
        return;
    };
    for value in map.values_mut() {
        if let Some(id) = reference_id(value) {
            let secret = vault.get(plugin, id).ok().flatten().unwrap_or_default();
            *value = Value::String(secret);
        }
    }
}

/// Takes the values of the form's secret fields out of settings a plugin
/// sent (`new`), stores them, and leaves references in their place. `old`
/// is what Deckly held before, so a key keeps its reference. A value equal
/// to the stored one, or one sent while the store could not be read, does
/// not overwrite the store. `make_id` names a secret that has no reference
/// yet.
pub fn keep(
    vault: &Vault,
    plugin: &str,
    form: &Form,
    old: &Value,
    new: &mut Value,
    make_id: &dyn Fn(&str) -> String,
) -> Result<(), String> {
    let Some(map) = new.as_object_mut() else {
        return Ok(());
    };
    let mut result = Ok(());
    for key in form.secrets() {
        let id = old.get(key).and_then(reference_id).map(str::to_owned);
        match map.get(key) {
            Some(Value::String(text)) => {
                let text = text.clone();
                let id = match id {
                    Some(id) => id,
                    None if text.is_empty() => {
                        map.remove(key);
                        continue;
                    }
                    None => make_id(key),
                };
                match vault.get(plugin, &id) {
                    Ok(stored) if stored.as_deref().unwrap_or_default() == text => {}
                    Ok(_) => {
                        if let Err(e) = vault.set(plugin, &id, &text) {
                            result = Err(e);
                        }
                    }
                    // What the plugin holds may be the empty stand-in.
                    Err(_) => {}
                }
                map.insert(key.to_owned(), reference(&id));
            }
            // A reference stays as long as it is the key's own.
            Some(v) if reference_id(v).is_some() && reference_id(v) == id.as_deref() => {}
            _ => match id {
                Some(id) => {
                    map.insert(key.to_owned(), reference(&id));
                }
                None => {
                    map.remove(key);
                }
            },
        }
    }
    result
}

/// Stores a secret the user typed into a form and puts its reference into
/// the settings. An empty value removes the secret but keeps the reference.
pub fn store_typed(
    vault: &'static Vault,
    plugin: &str,
    settings: &mut Value,
    key: &str,
    value: &str,
    make_id: &dyn Fn(&str) -> String,
) -> Result<(), String> {
    let id = settings
        .get(key)
        .and_then(reference_id)
        .map_or_else(|| make_id(key), str::to_owned);
    vault.set_later(plugin, &id, value)?;
    if !settings.is_object() {
        *settings = Value::Object(Map::new());
    }
    if let Some(map) = settings.as_object_mut() {
        map.insert(key.to_owned(), reference(&id));
    }
    Ok(())
}

/// The value of a secret field as the form shows it, without waiting;
/// `Loading` while the vault itself is being set up.
pub fn typed(vault: Option<&'static Vault>, plugin: &str, settings: &Value, key: &str) -> Peek {
    match (settings.get(key).and_then(reference_id), vault) {
        (None, _) => Peek::Ready(None),
        (Some(_), None) => Peek::Loading,
        (Some(id), Some(vault)) => vault.peek(plugin, id),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PLUGIN: &str = "com.example.obs";

    fn form() -> Form {
        serde_json::from_value(json!({"fields": [
            {"kind": "text", "key": "address"},
            {"kind": "secret", "key": "password"}
        ]}))
        .unwrap()
    }

    /// Leaked, as the app's vault lives for the whole run.
    fn temp_vault(name: &str) -> (&'static Vault, PathBuf) {
        let dir = crate::store::data_dir()
            .unwrap()
            .join(format!("secrets-{name}"));
        let _ = std::fs::remove_dir_all(&dir);
        let file = dir.join(FILE);
        (Box::leak(Box::new(Vault::in_file(file.clone()))), file)
    }

    #[test]
    fn the_global_vault_in_tests_never_uses_the_system_store() {
        let v = vault();
        assert_eq!(v.fallback(), Some("tests"));
        assert!(
            matches!(&v.backend, Backend::File(f) if f.starts_with(crate::store::data_dir().unwrap()))
        );
    }

    #[test]
    fn the_fallback_file_keeps_secrets_for_the_user_only() {
        let (vault, file) = temp_vault("file");
        assert_eq!(vault.get(PLUGIN, "a").unwrap(), None);
        vault.set(PLUGIN, "a", "hunter2").unwrap();
        vault.set(PLUGIN, "b", "other").unwrap();
        // A fresh vault reads the same file.
        let again = Vault::in_file(file.clone());
        assert_eq!(again.get(PLUGIN, "a").unwrap().as_deref(), Some("hunter2"));
        again.set(PLUGIN, "a", "").unwrap();
        assert_eq!(Vault::in_file(file.clone()).get(PLUGIN, "a").unwrap(), None);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&file).unwrap().permissions().mode();
            assert_eq!(mode & 0o777, 0o600);
        }
        // Bad names and huge values are refused.
        assert!(vault.set("../x", "a", "v").is_err());
        assert!(vault.set(PLUGIN, "../a", "v").is_err());
        assert!(
            vault
                .set(PLUGIN, "a", &"x".repeat(MAX_SECRET_BYTES + 1))
                .is_err()
        );
        // A broken file is an error, not a crash, and is not overwritten.
        std::fs::write(&file, "not json").unwrap();
        let broken = Vault::in_file(file.clone());
        assert!(broken.get(PLUGIN, "z").is_err());
        assert!(broken.set(PLUGIN, "z", "v").is_err());
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "not json");
    }

    #[test]
    fn references_hide_values_from_settings() {
        let (vault, _) = temp_vault("refs");
        let ids = |_: &str| "id-1".to_owned();
        let mut settings = json!({"address": "localhost"});
        store_typed(vault, PLUGIN, &mut settings, "password", "pw", &ids).unwrap();
        assert_eq!(
            settings,
            json!({"address": "localhost", "password": {MARK: "id-1"}})
        );
        assert!(!settings.to_string().contains("pw\""));
        assert_eq!(
            typed(Some(vault), PLUGIN, &settings, "password"),
            Peek::Ready(Some("pw".into()))
        );
        // The plugin sees the value...
        let mut shown = settings.clone();
        reveal(vault, PLUGIN, &mut shown);
        assert_eq!(shown, json!({"address": "localhost", "password": "pw"}));
        // ...and only the plugin it belongs to.
        let mut other = settings.clone();
        reveal(vault, "com.example.other", &mut other);
        assert_eq!(other["password"], "");
        // What the plugin sends back loses the value again.
        let mut back = json!({"address": "remote", "password": "pw"});
        keep(vault, PLUGIN, &form(), &settings, &mut back, &ids).unwrap();
        assert_eq!(
            back,
            json!({"address": "remote", "password": {MARK: "id-1"}})
        );
        // A new value from the plugin is stored under the same reference.
        let mut changed = json!({"password": "new"});
        keep(vault, PLUGIN, &form(), &settings, &mut changed, &ids).unwrap();
        assert_eq!(changed["password"], json!({MARK: "id-1"}));
        assert_eq!(vault.get(PLUGIN, "id-1").unwrap().as_deref(), Some("new"));
        // Settings that leave the secret out keep the reference.
        let mut without = json!({"address": "x"});
        keep(vault, PLUGIN, &form(), &settings, &mut without, &ids).unwrap();
        assert_eq!(without["password"], json!({MARK: "id-1"}));
        // A secret first set by the plugin gets a reference of its own.
        let fresh = |_: &str| "id-2".to_owned();
        let mut first = json!({"password": "from plugin"});
        keep(vault, PLUGIN, &form(), &json!({}), &mut first, &fresh).unwrap();
        assert_eq!(first["password"], json!({MARK: "id-2"}));
        // Something that is not text never lands in the settings.
        let mut odd = json!({"password": 12});
        keep(vault, PLUGIN, &form(), &json!({}), &mut odd, &fresh).unwrap();
        assert_eq!(odd, json!({}));
    }

    #[test]
    fn a_store_that_cannot_be_read_is_not_overwritten_by_the_stand_in() {
        let (vault, file) = temp_vault("unreadable");
        vault.set(PLUGIN, "id-1", "real").unwrap();
        std::fs::write(&file, "broken").unwrap();
        let broken = Vault::in_file(file.clone());
        let settings = json!({"password": {MARK: "id-1"}});
        let mut shown = settings.clone();
        reveal(&broken, PLUGIN, &mut shown);
        assert_eq!(shown["password"], "");
        // The plugin echoes the empty stand-in back.
        let mut echoed = shown.clone();
        keep(&broken, PLUGIN, &form(), &settings, &mut echoed, &|_| {
            "x".into()
        })
        .unwrap();
        assert_eq!(echoed, settings);
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "broken");
    }

    /// Waits for the secrets thread to finish this vault's jobs.
    fn settle(vault: &Vault) {
        let start = Instant::now();
        while vault.busy() {
            assert!(start.elapsed() < Duration::from_secs(10), "jobs hang");
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    #[test]
    fn the_editor_reads_without_waiting() {
        let (vault, file) = temp_vault("peek");
        vault.set(PLUGIN, "a", "hunter2").unwrap();
        // A fresh vault has nothing cached: the first look starts a read.
        let fresh: &'static Vault = Box::leak(Box::new(Vault::in_file(file)));
        assert_eq!(fresh.peek(PLUGIN, "a"), Peek::Loading);
        settle(fresh);
        assert_eq!(fresh.peek(PLUGIN, "a"), Peek::Ready(Some("hunter2".into())));
        assert_eq!(fresh.peek(PLUGIN, "none"), Peek::Loading);
        settle(fresh);
        assert_eq!(fresh.peek(PLUGIN, "none"), Peek::Ready(None));
        // No vault yet: loading, not a wait.
        let settings = json!({"password": {MARK: "a"}});
        assert_eq!(typed(None, PLUGIN, &settings, "password"), Peek::Loading);
        assert_eq!(
            typed(None, PLUGIN, &json!({}), "password"),
            Peek::Ready(None)
        );
    }

    #[test]
    fn a_secret_set_later_is_seen_at_once_and_stored_after() {
        let (vault, file) = temp_vault("later");
        vault.set_later(PLUGIN, "a", "new").unwrap();
        // The plugin's settings filter sees the new value right away.
        assert_eq!(vault.get(PLUGIN, "a").unwrap().as_deref(), Some("new"));
        assert_eq!(vault.peek(PLUGIN, "a"), Peek::Ready(Some("new".into())));
        settle(vault);
        let again = Vault::in_file(file);
        assert_eq!(again.get(PLUGIN, "a").unwrap().as_deref(), Some("new"));
        assert_eq!(vault.take_write_error(PLUGIN, "a"), None);
        // Bad names and huge values are refused before anything is queued.
        assert!(vault.set_later("../x", "a", "v").is_err());
        assert!(
            vault
                .set_later(PLUGIN, "a", &"x".repeat(MAX_SECRET_BYTES + 1))
                .is_err()
        );
    }

    #[test]
    fn a_failed_write_takes_its_value_back_and_says_why() {
        let (vault, file) = temp_vault("failed-write");
        vault.set(PLUGIN, "a", "old").unwrap();
        std::fs::write(&file, "broken").unwrap();
        vault.set_later(PLUGIN, "a", "new").unwrap();
        settle(vault);
        assert!(vault.take_write_error(PLUGIN, "a").is_some());
        // Shown once, then gone.
        assert_eq!(vault.take_write_error(PLUGIN, "a"), None);
        // The value that was not stored is not shown or sent as if it were.
        assert_ne!(vault.peek(PLUGIN, "a"), Peek::Ready(Some("new".into())));
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "broken");
    }

    #[test]
    fn the_fallback_file_is_bounded_and_moves_only_when_every_entry_is_deckly_s() {
        let (_, file) = temp_vault("bounds");
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        let big = format!(
            "{{\"{PLUGIN}\": {{\"a\": \"{}\"}}}}",
            "x".repeat(MAX_FILE_BYTES as usize)
        );
        std::fs::write(&file, big).unwrap();
        assert!(read_file(&file).is_err());
        // Checked before anything is written to the system's store.
        for bad in [
            json!({"../evil": {"a": "v"}}),
            json!({PLUGIN: {"../a": "v"}}),
            json!({PLUGIN: {"a": 5}}),
            json!({PLUGIN: "v"}),
        ] {
            std::fs::write(&file, bad.to_string()).unwrap();
            assert!(migrate(&file).is_err(), "{bad}");
            assert!(file.exists());
        }
    }

    #[test]
    fn references_that_deckly_did_not_write_are_left_alone() {
        assert_eq!(reference_id(&json!({MARK: "a/b"})), None);
        assert_eq!(reference_id(&json!({MARK: "ok", "more": 1})), None);
        assert_eq!(reference_id(&json!({MARK: 5})), None);
        assert_eq!(
            reference_id(&json!({MARK: global_id("password")})),
            Some("global.password")
        );
    }
}
