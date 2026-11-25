//! Accounts of a ChatGPT plan. What renews one is in the vault and nowhere else; what it is renewed
//! into lasts an hour and is in memory only (SI-3).

use std::{
    collections::HashMap,
    io,
    path::PathBuf,
    pin::Pin,
    sync::{Arc, Mutex, MutexGuard, PoisonError, atomic::AtomicBool, atomic::Ordering::Relaxed},
    time::{Duration, SystemTime},
};

use axiom_model::{
    Auth, Class, Credential, Endpoint, Error, Renew, Retry, Secret, SetupError,
    chatgpt::{self, Failed, Places, Tokens},
};
use tokio::{sync::OwnedMutexGuard, time::timeout};

use crate::{
    connections::{Text, Vault},
    log,
    paths::private_file,
};

/// How long before its end an access token is renewed.
const EARLY: Duration = Duration::from_secs(5 * 60);
/// How long a person who removes an account waits for it to be ended there.
const ENDING: Duration = Duration::from_secs(10);

/// What there is of an account while the host runs.
#[derive(Default)]
pub struct Live {
    access: Option<Access>,
    /// What renews it, if the vault did not take it.
    refresh: Option<Text>,
    gone: bool,
}
struct Access {
    credential: Arc<Credential>,
    renew_at: SystemTime,
    until: SystemTime,
}

#[derive(Default)]
pub struct Held {
    /// One renewal at a time: each makes what it was done with worthless.
    live: Arc<tokio::sync::Mutex<Live>>,
    /// Why nothing can be had of it any more.
    problem: Mutex<Option<String>>,
    unsaved: AtomicBool,
}

pub struct Plans {
    pub places: Places,
    vault: Arc<dyn Vault>,
    /// Where it is said what this installation is called.
    name: PathBuf,
    held: Mutex<HashMap<String, Arc<Held>>>,
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

impl Plans {
    pub fn new(places: Places, vault: Arc<dyn Vault>, name: PathBuf) -> Self {
        Self { places, vault, name, held: Mutex::default() }
    }

    /// Always the same for this installation, and nothing of the person or the machine.
    pub fn installation(&self) -> io::Result<String> {
        match std::fs::read_to_string(&self.name) {
            Ok(t) if t.starts_with("urn:uuid:") && t.len() == 45 => return Ok(t),
            Ok(_) => {
                return Err(io::Error::other(format!("{}: not a name", self.name.display())));
            }
            Err(e) if e.kind() == io::ErrorKind::NotFound => {}
            Err(e) => return Err(e),
        }
        let t = chatgpt::installation().map_err(io::Error::other)?;
        private_file(&self.name, &t)?;
        Ok(t)
    }

    fn held(&self, id: &str) -> Arc<Held> {
        lock(&self.held).entry(id.into()).or_default().clone()
    }

    pub fn problem(&self, id: &str) -> Option<String> {
        let held = lock(&self.held).get(id).cloned()?;
        lock(&held.problem).clone()
    }

    /// Whether what renews it is in the vault.
    pub fn saved(&self, id: &str) -> bool {
        lock(&self.held).get(id).is_none_or(|h| !h.unsaved.load(Relaxed))
    }

    /// Nothing is renewed while this is held.
    pub async fn hold(&self, id: &str) -> OwnedMutexGuard<Live> {
        let held = self.held(id);
        held.live.clone().lock_owned().await
    }

    /// What a sign-in gave; what renews it is in the vault already.
    pub fn begin(&self, id: &str, live: &mut Live, t: Tokens) -> Result<(), SetupError> {
        live.access = Some(self.access(t)?);
        live.refresh = None;
        let held = self.held(id);
        *lock(&held.problem) = None;
        held.unsaved.store(false, Relaxed);
        Ok(())
    }

    fn access(&self, t: Tokens) -> Result<Access, SetupError> {
        let origin = Endpoint::parse(&self.places.resource)?;
        let secret = Secret::new(t.access.to_string());
        let credential = Arc::new(Credential::new(origin.origin(), &Auth::Bearer, secret)?);
        let lasts = t.until.duration_since(SystemTime::now()).unwrap_or_default();
        Ok(Access { credential, renew_at: t.until - EARLY.min(lasts / 2), until: t.until })
    }

    async fn kept(&self, id: &str, live: &Live) -> Result<Option<Text>, String> {
        if let Some(r) = &live.refresh {
            return Ok(Some(r.clone()));
        }
        let (vault, id) = (self.vault.clone(), id.to_owned());
        tokio::task::spawn_blocking(move || vault.get(&id)).await.map_err(|e| e.to_string())?
    }

    pub fn account(self: &Arc<Self>, id: String, client: String) -> Account {
        let held = self.held(&id);
        Account { plans: self.clone(), held, id, client }
    }

    async fn credential(
        &self,
        held: &Held,
        id: &str,
        client: &str,
    ) -> Result<Arc<Credential>, Error> {
        let auth = |words: &str| Error::new(Class::Auth, Retry::Never, words);
        let mut live = held.live.clone().lock_owned().await;
        if live.gone {
            return Err(auth("the account was removed"));
        }
        if let Some(p) = &*lock(&held.problem) {
            return Err(auth(p));
        }
        let now = SystemTime::now();
        if let Some(a) = live.access.as_ref().filter(|a| now < a.renew_at) {
            return Ok(a.credential.clone());
        }
        let over = |words: String| {
            *lock(&held.problem) = Some(words.clone());
            auth(&words)
        };
        let Some(refresh) = self.kept(id, &live).await.map_err(|e| auth(&e))? else {
            live.access = None;
            let words = "the sign-in is not in the system's store any more; sign in again";
            return Err(over(words.into()));
        };
        let t = match chatgpt::renew(&self.places, client, &refresh).await {
            Ok(t) => t,
            Err(e @ (Failed::Over(_) | Failed::NoPlan)) => {
                live.access = None;
                return Err(over(e.to_string()));
            }
            Err(e) => {
                // It is good a little longer, and then there is another try.
                if let Some(a) = live.access.as_ref().filter(|a| now < a.until) {
                    return Ok(a.credential.clone());
                }
                let retry = match e {
                    Failed::Later(_) => Retry::Safe,
                    _ => Retry::Never,
                };
                return Err(Error::new(Class::Network, retry, e.to_string()));
            }
        };
        // Kept before it is used: what was there is worthless from here on.
        let (vault, name, new) = (self.vault.clone(), id.to_owned(), t.refresh.clone());
        let put = tokio::task::spawn_blocking(move || vault.put(&name, &new)).await;
        let put = put.map_err(|e| e.to_string()).and_then(|r| r);
        if let Err(e) = &put {
            log::note(format!("the account {id} has to be signed in to again later: {e}"));
        }
        live.refresh = put.is_err().then(|| t.refresh.clone());
        held.unsaved.store(put.is_err(), Relaxed);
        let access = self.access(t).map_err(|e| auth(&e.to_string()))?;
        let credential = access.credential.clone();
        live.access = Some(access);
        Ok(credential)
    }

    /// Ends it at OpenAI. Whether that is certain.
    pub async fn end(&self, id: &str, live: &mut Live, client: &str) -> bool {
        match self.kept(id, live).await {
            Ok(Some(r)) => {
                timeout(ENDING, chatgpt::revoke(&self.places, client, &r)).await.unwrap_or_default()
            }
            _ => false,
        }
    }

    /// For good: who still has a client for it gets nothing more.
    pub fn forget(&self, id: &str, live: &mut Live) {
        *live = Live { gone: true, ..Live::default() };
        lock(&self.held).remove(id);
    }
}

/// Gives what is good now to a client of the account.
pub struct Account {
    plans: Arc<Plans>,
    held: Arc<Held>,
    id: String,
    client: String,
}
impl Renew for Account {
    fn now(&self) -> Pin<Box<dyn Future<Output = Result<Arc<Credential>, Error>> + Send + '_>> {
        let (plans, held) = (self.plans.clone(), self.held.clone());
        let (id, client) = (self.id.clone(), self.client.clone());
        Box::pin(async move {
            // To its end, also when whoever asked is stopped: what it renews by is gone as soon as
            // it is used.
            let task = tokio::spawn(async move { plans.credential(&held, &id, &client).await });
            task.await.unwrap_or_else(|e| {
                Err(Error::new(Class::Network, Retry::Never, format!("the host is stopping: {e}")))
            })
        })
    }
}
