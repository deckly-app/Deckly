//! Plugin-wide state that every key reads and any thread may change: one
//! connection to a program and what it last reported. Changing it makes
//! the SDK ask every key's [`Action::face`](crate::Action::face) again.

use crate::Globals;
use std::any::{Any, TypeId};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|p| p.into_inner())
}

/// Asks the dispatcher to draw every key again.
pub(crate) struct Waker {
    /// A redraw is on its way; more changes before it need no other.
    pending: AtomicBool,
    wake: Box<dyn Fn() + Send + Sync>,
}

impl Waker {
    /// `wake` sends the redraw event; it does nothing once the run loop is
    /// gone.
    pub(crate) fn new(wake: impl Fn() + Send + Sync + 'static) -> Self {
        Self {
            pending: AtomicBool::new(false),
            wake: Box::new(wake),
        }
    }

    fn wake(&self) {
        // Once the run loop is gone nothing clears the flag, so later
        // changes do not even try.
        if !self.pending.swap(true, Ordering::AcqRel) {
            (self.wake)();
        }
    }

    /// The dispatcher is about to redraw: changes from here on need
    /// another redraw.
    pub(crate) fn take(&self) -> bool {
        self.pending.swap(false, Ordering::AcqRel)
    }
}

/// Every shared value of one running plugin, by type.
pub(crate) struct Store {
    values: Mutex<HashMap<TypeId, Box<dyn Any + Send + Sync>>>,
    waker: Arc<Waker>,
}

impl Store {
    pub(crate) fn new(waker: Arc<Waker>) -> Self {
        Self {
            values: Mutex::new(HashMap::new()),
            waker,
        }
    }

    pub(crate) fn get<T: Default + Send + 'static>(&self) -> Shared<T> {
        let mut values = lock(&self.values);
        let slot = values
            .entry(TypeId::of::<T>())
            .or_insert_with(|| Box::new(Arc::new(Mutex::new(T::default()))));
        let value = match slot.downcast_ref::<Arc<Mutex<T>>>() {
            Some(v) => v.clone(),
            None => {
                // Cannot happen: the entry is keyed by the type. Start over
                // rather than fail.
                let v = Arc::new(Mutex::new(T::default()));
                *slot = Box::new(v.clone());
                v
            }
        };
        Shared {
            value,
            waker: self.waker.clone(),
        }
    }
}

/// A value the whole plugin shares, one per type `T`: every key and every
/// thread that asks for a `T` gets the same one. It starts as
/// `T::default()`.
///
/// Get it with [`Key::shared`](crate::Key::shared) in a handler, or with
/// [`Running::shared`] in [`Plugin::on_start`](crate::Plugin::on_start).
/// Cheap to clone and `Send`: move it to the thread that listens to another
/// program, and call [`Shared::update`] there. After an update the SDK asks
/// every visible key's [`Action::face`](crate::Action::face) again and
/// sends only the faces that changed.
///
/// The value sits behind a lock. Keep the closures short, and do not call
/// [`Shared::update`] or [`Shared::read`] on the same `T` inside them: the
/// thread would wait for itself.
pub struct Shared<T> {
    value: Arc<Mutex<T>>,
    waker: Arc<Waker>,
}

impl<T> Clone for Shared<T> {
    fn clone(&self) -> Self {
        Self {
            value: self.value.clone(),
            waker: self.waker.clone(),
        }
    }
}

impl<T> Shared<T> {
    /// Reads the value.
    pub fn read<R>(&self, f: impl FnOnce(&T) -> R) -> R {
        f(&lock(&self.value))
    }

    /// Changes the value, then has every visible key drawn again. Many
    /// updates in a row lead to one redraw. Once the plugin has stopped,
    /// the value still changes but nothing is drawn.
    pub fn update<R>(&self, f: impl FnOnce(&mut T) -> R) -> R {
        let result = f(&mut lock(&self.value));
        self.waker.wake();
        result
    }

    /// Replaces the value; see [`Shared::update`].
    pub fn set(&self, value: T) {
        self.update(|v| *v = value);
    }
}

impl<T: Clone> Shared<T> {
    /// A copy of the value.
    pub fn get(&self) -> T {
        self.read(T::clone)
    }
}

/// The running plugin, as [`Plugin::on_start`](crate::Plugin::on_start)
/// sees it. Cheap to clone and `Send`.
#[derive(Clone)]
pub struct Running {
    pub(crate) store: Arc<Store>,
    pub(crate) globals: Globals,
}

impl Running {
    /// The plugin's shared `T`; see [`Shared`].
    pub fn shared<T: Default + Send + 'static>(&self) -> Shared<T> {
        self.store.get()
    }

    /// The plugin's global settings. Deckly sends them right after the
    /// start, so they may not be loaded yet; see
    /// [`Plugin::on_global_settings`](crate::Plugin::on_global_settings).
    pub fn globals(&self) -> &Globals {
        &self.globals
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicUsize;

    fn store() -> (Store, Arc<AtomicUsize>) {
        let wakes = Arc::new(AtomicUsize::new(0));
        let waker = Waker::new({
            let wakes = wakes.clone();
            move || {
                wakes.fetch_add(1, Ordering::Relaxed);
            }
        });
        (Store::new(Arc::new(waker)), wakes)
    }

    #[test]
    fn one_value_per_type() {
        let (store, _) = store();
        store.get::<u32>().set(7);
        assert_eq!(store.get::<u32>().get(), 7);
        assert_eq!(store.get::<String>().get(), "");
    }

    #[test]
    fn updates_before_a_redraw_wake_once() {
        let (store, wakes) = store();
        let n = store.get::<u32>();
        n.update(|n| *n += 1);
        n.update(|n| *n += 1);
        assert_eq!(wakes.load(Ordering::Relaxed), 1);
        assert!(store.waker.take());
        n.update(|n| *n += 1);
        assert_eq!(wakes.load(Ordering::Relaxed), 2);
        assert_eq!(n.get(), 3);
    }

    #[test]
    fn a_poisoned_value_is_still_usable() {
        let (store, _) = store();
        let n = store.get::<u32>();
        let m = n.clone();
        let _ = std::thread::spawn(move || m.update(|_| panic!("in the author's code"))).join();
        n.set(5);
        assert_eq!(n.get(), 5);
    }
}
