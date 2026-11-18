//! The devices that asked to join, and what each may do. A device asks with
//! a name and gets a random token (as a cookie); the host lets it in to
//! watch or to operate, changes that, or removes it. Let-in devices are
//! kept (by the hash of their token, never the token itself) and known again
//! after a restart; requests not answered yet are forgotten.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tokio::sync::watch;

use super::protocol::Role;

/// At most this many join requests in [`JOIN_WINDOW`], and this many waiting
/// for an answer: a stray script on the network must not flood the host.
const MAX_JOINS: usize = 6;
const JOIN_WINDOW: Duration = Duration::from_secs(60);
const MAX_PENDING: usize = 8;
/// Longest device name, in characters.
pub const MAX_NAME: usize = 40;

/// A let-in device as it is kept on disk.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct StoredClient {
    pub id: String,
    pub name: String,
    /// SHA-256 of its token, hex.
    pub token_hash: String,
    /// `View` or `Control`.
    pub role: Role,
}

/// A device as the host's settings list it (the `remote` event).
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ClientInfo {
    pub id: String,
    pub name: String,
    pub role: Role,
    /// Open connections (a device may have the page open twice).
    pub connected: usize,
    /// Network address of its last connection.
    pub address: Option<String>,
    /// Milliseconds since 1970 of its last connection or request.
    pub last_seen: Option<u64>,
    /// A request with the name of a device let in before: probably that one
    /// again, with a new token (the host's address changed, cookies
    /// deleted).
    pub known_name: bool,
}

/// The server's state and the devices, for the host (`remote` event and
/// `get_remote`).
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct RemoteStatus {
    /// Port the server listens on; `None` = off.
    pub listening: Option<u16>,
    /// Why it is not listening although it should.
    pub error: Option<String>,
    /// Let-in devices first, then requests, each in the order they came.
    pub clients: Vec<ClientInfo>,
}

/// A connecting device, as its token says.
pub struct Lookup {
    pub id: String,
    pub name: String,
    /// Its role now and later; `None` once it is removed.
    pub role: watch::Receiver<Option<Role>>,
}

struct Entry {
    stored: StoredClient,
    pending: bool,
    connected: usize,
    address: Option<String>,
    last_seen: Option<u64>,
    role: watch::Sender<Option<Role>>,
}

impl Entry {
    fn new(stored: StoredClient, pending: bool) -> Self {
        let role = watch::Sender::new(Some(stored.role));
        Self { stored, pending, connected: 0, address: None, last_seen: None, role }
    }
}

#[derive(Default)]
struct Inner {
    entries: Vec<Entry>,
    joins: VecDeque<Instant>,
    listening: Option<u16>,
    error: Option<String>,
}

type Changed = Box<dyn Fn(&RemoteStatus) + Send + Sync>;
type Store = Box<dyn Fn(Vec<StoredClient>) + Send + Sync>;
type Joined = Box<dyn Fn(&str) + Send + Sync>;

/// Shared by the server's connections and the host's commands; locked only
/// briefly, never by the show thread on its way to a GO.
#[derive(Clone)]
pub struct Registry(Arc<Shared>);

struct Shared {
    inner: Mutex<Inner>,
    /// The host is told of every change.
    changed: Changed,
    /// The let-in devices are to be kept (see [`load`]).
    store: Store,
    /// A device asked to join (by name).
    joined: Joined,
}

impl Registry {
    pub fn new(
        stored: Vec<StoredClient>,
        changed: impl Fn(&RemoteStatus) + Send + Sync + 'static,
        store: impl Fn(Vec<StoredClient>) + Send + Sync + 'static,
        joined: impl Fn(&str) + Send + Sync + 'static,
    ) -> Self {
        let entries =
            stored.into_iter().filter(|c| c.role != Role::Pending).map(|c| Entry::new(c, false)).collect();
        let inner = Inner { entries, ..Inner::default() };
        Self(Arc::new(Shared {
            inner: Mutex::new(inner),
            changed: Box::new(changed),
            store: Box::new(store),
            joined: Box::new(joined),
        }))
    }

    /// A registry that keeps nothing and tells nobody (tests).
    #[cfg(test)]
    pub fn detached() -> Self {
        Self::new(Vec::new(), |_| {}, |_| {}, |_| {})
    }

    fn lock(&self) -> MutexGuard<'_, Inner> {
        // A panic while it was held left nothing half-done that matters more
        // than going on.
        self.0.inner.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Runs `change` and tells the host (and the store, if `store`) what
    /// came of it, outside the lock.
    fn update<T>(&self, store: bool, change: impl FnOnce(&mut Inner) -> T) -> T {
        let (result, status, stored) = {
            let mut inner = self.lock();
            let result = change(&mut inner);
            let stored = store.then(|| stored(&inner));
            (result, status(&inner), stored)
        };
        if let Some(stored) = stored {
            (self.0.store)(stored);
        }
        (self.0.changed)(&status);
        result
    }

    pub fn status(&self) -> RemoteStatus {
        status(&self.lock())
    }

    /// Whether the server listens, and why not.
    pub fn set_server(&self, listening: Option<u16>, error: Option<String>) {
        self.update(false, |inner| {
            inner.listening = listening;
            inner.error = error;
        });
    }

    /// A device asks to join as `name`: a new request with a new token, or
    /// an error if there were too many lately.
    pub fn join(&self, name: &str, address: Option<String>) -> Result<(String, Lookup), String> {
        let name = clean_name(name)?;
        let now = Instant::now();
        let result = self.update(false, |inner| {
            while inner.joins.front().is_some_and(|t| now.duration_since(*t) > JOIN_WINDOW) {
                inner.joins.pop_front();
            }
            let pending = inner.entries.iter().filter(|e| e.pending).count();
            if inner.joins.len() >= MAX_JOINS || pending >= MAX_PENDING {
                return Err(jq_i18n::t!("remote.too_many_requests"));
            }
            inner.joins.push_back(now);
            let token = random_hex(32);
            let stored = StoredClient {
                id: random_hex(8),
                name: name.clone(),
                token_hash: hash(&token),
                role: Role::Pending,
            };
            let mut entry = Entry::new(stored, true);
            (entry.address, entry.last_seen) = (address, Some(now_ms()));
            let lookup =
                Lookup { id: entry.stored.id.clone(), name: name.clone(), role: entry.role.subscribe() };
            inner.entries.push(entry);
            Ok((token, lookup))
        });
        if result.is_ok() {
            (self.0.joined)(&name);
        }
        result
    }

    /// The device holding `token`, if it asked or was let in.
    pub fn lookup(&self, token: &str) -> Option<Lookup> {
        let hash = hash(token);
        let inner = self.lock();
        let entry = inner.entries.iter().find(|e| e.stored.token_hash == hash)?;
        Some(Lookup {
            id: entry.stored.id.clone(),
            name: entry.stored.name.clone(),
            role: entry.role.subscribe(),
        })
    }

    /// A connection of device `id` opened from `address`; closing it is
    /// counted when the guard is dropped.
    pub fn connect(&self, id: &str, address: String) -> Connection {
        self.update(false, |inner| {
            if let Some(entry) = inner.entries.iter_mut().find(|e| e.stored.id == id) {
                entry.connected += 1;
                (entry.address, entry.last_seen) = (Some(address), Some(now_ms()));
            }
        });
        Connection { registry: self.clone(), id: id.to_owned() }
    }

    /// Lets device `id` in as `role` (a request), or changes its role.
    pub fn set_role(&self, id: &str, role: Role) -> Result<(), String> {
        if role == Role::Pending {
            return Err(jq_i18n::t!("remote.bad_role"));
        }
        self.update(true, |inner| {
            let entry = inner
                .entries
                .iter_mut()
                .find(|e| e.stored.id == id)
                .ok_or_else(|| jq_i18n::t!("remote.unknown_client"))?;
            entry.stored.role = role;
            entry.pending = false;
            entry.role.send_replace(Some(role));
            Ok(())
        })
    }

    /// Removes device `id` (or turns down its request); its connections are
    /// closed and its token is worth nothing from now on.
    pub fn remove(&self, id: &str) -> Result<(), String> {
        let pending = self.lock().entries.iter().find(|e| e.stored.id == id).map(|e| e.pending);
        let Some(pending) = pending else { return Err(jq_i18n::t!("remote.unknown_client")) };
        self.update(!pending, |inner| {
            if let Some(at) = inner.entries.iter().position(|e| e.stored.id == id) {
                inner.entries.remove(at).role.send_replace(None);
            }
        });
        Ok(())
    }
}

/// Counts a connection of a device as open while it lives.
pub struct Connection {
    registry: Registry,
    id: String,
}

impl Drop for Connection {
    fn drop(&mut self) {
        self.registry.update(false, |inner| {
            if let Some(entry) = inner.entries.iter_mut().find(|e| e.stored.id == self.id) {
                entry.connected = entry.connected.saturating_sub(1);
                entry.last_seen = Some(now_ms());
            }
        });
    }
}

fn status(inner: &Inner) -> RemoteStatus {
    let known = |e: &Entry| {
        e.pending
            && inner.entries.iter().any(|other| {
                !other.pending && other.stored.name.to_lowercase() == e.stored.name.to_lowercase()
            })
    };
    let info = |e: &Entry| ClientInfo {
        id: e.stored.id.clone(),
        name: e.stored.name.clone(),
        role: if e.pending { Role::Pending } else { e.stored.role },
        connected: e.connected,
        address: e.address.clone(),
        last_seen: e.last_seen,
        known_name: known(e),
    };
    let (requests, let_in): (Vec<&Entry>, Vec<&Entry>) = inner.entries.iter().partition(|e| e.pending);
    RemoteStatus {
        listening: inner.listening,
        error: inner.error.clone(),
        clients: let_in.into_iter().chain(requests).map(info).collect(),
    }
}

fn stored(inner: &Inner) -> Vec<StoredClient> {
    inner.entries.iter().filter(|e| !e.pending).map(|e| e.stored.clone()).collect()
}

fn clean_name(name: &str) -> Result<String, String> {
    let name: String = name.trim().chars().filter(|c| !c.is_control()).take(MAX_NAME).collect();
    if name.is_empty() {
        return Err(jq_i18n::t!("remote.name_missing"));
    }
    Ok(name)
}

fn hash(token: &str) -> String {
    hex(&Sha256::digest(token.as_bytes()))
}

/// `bytes` random bytes as hex.
pub fn random_hex(bytes: usize) -> String {
    let mut buf = vec![0u8; bytes];
    // Without the system's randomness there is no safe token; the server is
    // the only thing that would fail, and it runs on a thread of its own.
    getrandom::fill(&mut buf).expect("system randomness");
    hex(&buf)
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn now_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_millis() as u64)
}

/// The let-in devices kept in `path` (`remote-clients.json`); none if it is
/// missing or unreadable.
pub fn load(path: &std::path::Path) -> Vec<StoredClient> {
    #[derive(Deserialize)]
    struct File {
        clients: Vec<StoredClient>,
    }
    std::fs::read_to_string(path)
        .ok()
        .and_then(|text| serde_json::from_str::<File>(&text).ok())
        .map(|file| file.clients)
        .unwrap_or_default()
}

/// Writes the let-in devices to `path`.
pub fn save(path: &std::path::Path, clients: &[StoredClient]) -> Result<(), String> {
    let text =
        serde_json::to_vec_pretty(&serde_json::json!({ "clients": clients })).map_err(|e| e.to_string())?;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    jq_show::write_atomic(path, &text).map_err(|e| format!("{}: {e}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_request_is_let_in_kept_and_known_again() {
        let kept = Arc::new(Mutex::new(Vec::new()));
        let keep = kept.clone();
        let registry = Registry::new(Vec::new(), |_| {}, move |c| *keep.lock().unwrap() = c, |_| {});
        let (token, lookup) = registry.join("  FOH Tablet ", None).unwrap();
        assert_eq!(lookup.name, "FOH Tablet");
        assert_eq!(*lookup.role.borrow(), Some(Role::Pending));
        assert!(kept.lock().unwrap().is_empty(), "requests are not kept");

        registry.set_role(&lookup.id, Role::Control).unwrap();
        assert_eq!(*lookup.role.borrow(), Some(Role::Control), "its connection hears of it");
        let stored = kept.lock().unwrap().clone();
        assert_eq!(stored.len(), 1);
        assert_ne!(stored[0].token_hash, token, "only the hash is kept");

        let again = Registry::new(stored, |_| {}, |_| {}, |_| {});
        let found = again.lookup(&token).unwrap();
        assert_eq!((found.id.as_str(), *found.role.borrow()), (lookup.id.as_str(), Some(Role::Control)));
        assert!(again.lookup("another token").is_none());
    }

    #[test]
    fn a_removed_device_is_cut_off() {
        let registry = Registry::detached();
        let (token, lookup) = registry.join("Phone", None).unwrap();
        registry.set_role(&lookup.id, Role::View).unwrap();
        registry.remove(&lookup.id).unwrap();
        assert_eq!(*lookup.role.borrow(), None);
        assert!(registry.lookup(&token).is_none());
        assert!(registry.remove(&lookup.id).is_err());
    }

    #[test]
    fn joins_are_limited() {
        let registry = Registry::detached();
        for n in 0..MAX_JOINS {
            registry.join(&format!("Device {n}"), None).unwrap();
        }
        assert!(registry.join("One more", None).is_err());
        assert!(Registry::detached().join("   ", None).is_err(), "a name is needed");
    }

    #[test]
    fn the_host_sees_connections_and_known_names() {
        let registry = Registry::detached();
        let (_, old) = registry.join("Tablet", None).unwrap();
        registry.set_role(&old.id, Role::View).unwrap();
        let (_, new) = registry.join("tablet", None).unwrap();
        let connection = registry.connect(&old.id, "10.0.0.7:5000".into());
        let clients = registry.status().clients;
        assert_eq!(clients[0].connected, 1);
        assert_eq!(clients[0].address.as_deref(), Some("10.0.0.7:5000"));
        assert!(!clients[0].known_name);
        assert_eq!(
            (clients[1].id.as_str(), clients[1].role, clients[1].known_name),
            (new.id.as_str(), Role::Pending, true)
        );
        drop(connection);
        assert_eq!(registry.status().clients[0].connected, 0);
    }

    #[test]
    fn kept_devices_survive_a_round_trip_on_disk() {
        let dir = std::env::temp_dir().join(format!("jq-remote-clients-{}", std::process::id()));
        let path = dir.join("remote-clients.json");
        let clients =
            vec![StoredClient { id: "a".into(), name: "A".into(), token_hash: "h".into(), role: Role::View }];
        save(&path, &clients).unwrap();
        assert_eq!(load(&path), clients);
        assert!(load(&dir.join("missing.json")).is_empty());
        let _ = std::fs::remove_dir_all(dir);
    }
}
