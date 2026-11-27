//! The Timer action's settings and in-memory countdowns. Time is measured
//! from a monotonic deadline, never by counting redraws or sleeps.

use deckly_device::Handle;
use serde_json::{Value, json};
use std::collections::HashMap;
use std::time::{Duration, Instant};

pub const MAX_SECONDS: u32 = 99 * 3600 + 59 * 60 + 59;
const MAX_TIMERS: usize = 128;
const FINISHED_FOR: Duration = Duration::from_secs(3);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Config {
    pub seconds: u32,
    pub chime: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            seconds: 60,
            chime: true,
        }
    }
}

impl Config {
    pub fn read(settings: &Value) -> Self {
        let default = Self::default();
        let seconds = settings
            .get("seconds")
            .and_then(Value::as_i64)
            .map_or(default.seconds, |n| {
                n.clamp(1, i64::from(MAX_SECONDS)) as u32
            });
        let chime = settings
            .get("chime")
            .and_then(Value::as_bool)
            .unwrap_or(default.chime);
        Self { seconds, chime }
    }

    pub fn settings(self) -> Value {
        json!({ "seconds": self.seconds, "chime": self.chime })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Display {
    Running(u32),
    Finished,
}

pub fn label(seconds: u32) -> String {
    let (h, m, s) = (seconds / 3600, seconds / 60 % 60, seconds % 60);
    if h == 0 {
        format!("{m:02}:{s:02}")
    } else {
        format!("{h:02}:{m:02}:{s:02}")
    }
}

struct Timer {
    config: Config,
    deadline: Instant,
    finished_until: Option<Instant>,
}

impl Timer {
    fn remaining(&self, now: Instant) -> u32 {
        let left = self.deadline.saturating_duration_since(now);
        (left.as_secs() + u64::from(left.subsec_nanos() != 0)) as u32
    }
}

/// A pinned key uses the same context on every page where it appears.
/// Different devices showing that context still have independent timers.
#[derive(Default)]
pub struct Timers(HashMap<Handle, HashMap<String, Timer>>);

impl Timers {
    pub fn press(
        &mut self,
        handle: Handle,
        context: &str,
        config: Config,
        now: Instant,
    ) -> Result<(), String> {
        if let Some(timers) = self.0.get_mut(&handle)
            && timers
                .get(context)
                .is_some_and(|t| t.config == config && t.deadline > now)
        {
            timers.remove(context);
            return Ok(());
        }
        if self.0.values().map(HashMap::len).sum::<usize>() >= MAX_TIMERS
            && !self
                .0
                .get(&handle)
                .is_some_and(|timers| timers.contains_key(context))
        {
            return Err("Timer: too many active countdowns".into());
        }
        self.0.entry(handle).or_default().insert(
            context.to_owned(),
            Timer {
                config,
                deadline: now + Duration::from_secs(u64::from(config.seconds)),
                finished_until: None,
            },
        );
        Ok(())
    }

    pub fn display(
        &self,
        handle: Handle,
        context: &str,
        config: Config,
        now: Instant,
    ) -> Option<Display> {
        let timer = self.0.get(&handle)?.get(context)?;
        if timer.config != config || timer.finished_until.is_some_and(|until| now >= until) {
            return None;
        }
        let left = timer.remaining(now);
        Some(if left == 0 {
            Display::Finished
        } else {
            Display::Running(left)
        })
    }

    pub fn remove_device(&mut self, handle: Handle) {
        self.0.remove(&handle);
    }

    pub fn retain(&mut self, mut valid: impl FnMut(&str, Config) -> bool) {
        self.0.retain(|_, timers| {
            timers.retain(|context, t| valid(context, t.config));
            !timers.is_empty()
        });
    }

    /// Mark each expiration once. The caller confirms the key still exists
    /// before using its chime setting.
    pub fn update(&mut self, now: Instant, mut expired: impl FnMut(&str, Config)) -> bool {
        let mut changed = false;
        self.0.retain(|_, timers| {
            timers.retain(|context, t| match t.finished_until {
                Some(until) if until <= now => {
                    changed = true;
                    false
                }
                None if t.deadline <= now => {
                    t.finished_until = Some(now + FINISHED_FOR);
                    expired(context, t.config);
                    changed = true;
                    true
                }
                _ => true,
            });
            !timers.is_empty()
        });
        changed
    }

    pub fn next_wake(&self, now: Instant) -> Option<Instant> {
        self.0
            .values()
            .flat_map(HashMap::values)
            .map(|t| {
                t.finished_until.unwrap_or_else(|| {
                    t.deadline - Duration::from_secs(u64::from(t.remaining(now).saturating_sub(1)))
                })
            })
            .min()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn untrusted_settings_are_bounded_and_round_trip() {
        assert_eq!(Config::read(&json!({"seconds": -500})).seconds, 1);
        assert_eq!(
            Config::read(&json!({"seconds": i64::MAX})).seconds,
            MAX_SECONDS
        );
        assert_eq!(
            Config::read(&json!({"seconds": "60", "chime": 1})),
            Config::default()
        );
        let config = Config {
            seconds: 3601,
            chime: false,
        };
        assert_eq!(Config::read(&config.settings()), config);
        assert_eq!(label(config.seconds), "01:00:01");
        assert_eq!(label(59), "00:59");
    }

    #[test]
    fn deadlines_do_not_drift_and_expiration_only_fires_once() {
        let mut timers = Timers::default();
        let now = Instant::now();
        let plus = |ms| now + Duration::from_millis(ms);
        let config = Config {
            seconds: 3,
            chime: true,
        };
        timers.press(1, "page.0", config, now).unwrap();
        assert_eq!(
            timers.display(1, "page.0", config, plus(999)),
            Some(Display::Running(3))
        );
        assert_eq!(
            timers.display(1, "page.0", config, plus(1000)),
            Some(Display::Running(2))
        );
        assert_eq!(timers.next_wake(plus(1400)), Some(plus(2000)));
        assert_eq!(timers.next_wake(plus(2500)), Some(plus(3000)));
        let mut expired = Vec::new();
        assert!(timers.update(plus(3400), |ctx, cfg| expired.push((ctx.to_owned(), cfg))));
        assert_eq!(expired, vec![("page.0".to_owned(), config)]);
        assert_eq!(
            timers.display(1, "page.0", config, plus(3500)),
            Some(Display::Finished)
        );
        assert!(!timers.update(plus(4000), |_, _| panic!("already expired")));
        assert!(timers.update(plus(6400), |_, _| panic!("already expired")));
        assert_eq!(timers.next_wake(plus(6400)), None);
    }

    #[test]
    fn a_second_press_cancels_only_that_devices_timer() {
        let mut timers = Timers::default();
        let now = Instant::now();
        let config = Config::default();
        timers.press(1, "pin.0", config, now).unwrap();
        timers.press(2, "pin.0", config, now).unwrap();
        timers.press(1, "pin.0", config, now).unwrap();
        assert_eq!(timers.display(1, "pin.0", config, now), None);
        assert_eq!(
            timers.display(2, "pin.0", config, now),
            Some(Display::Running(60))
        );
        timers.remove_device(2);
        assert_eq!(timers.next_wake(now), None);
    }
}
