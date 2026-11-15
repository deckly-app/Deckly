//! Keys protected against accidental presses: what a press, a release or
//! the passing of time means for a key that waits for a deliberate gesture.
//!
//! The decisions only, with time passed in, so every rule is tested without
//! a device. The runtime owns one [`Guard`] per device, feeds it the key
//! changes and runs what it returns through the one action path.

use deckly_core::Protection;
use std::collections::HashMap;
use std::time::{Duration, Instant};

/// How often a held key's progress ring moves on.
pub const HOLD_FRAME: Duration = Duration::from_millis(50);
/// How often an armed key's countdown moves on.
pub const ARMED_FRAME: Duration = Duration::from_millis(100);
/// How often the Back key's countdown on a confirmation screen moves on.
pub const CONFIRM_FRAME: Duration = Duration::from_millis(250);
/// How long a key shows that its gesture ran the action.
pub const DONE_TIME: Duration = Duration::from_millis(400);
/// A confirmation screen needs the key itself, Yes, No and Back.
pub const CONFIRM_MIN_KEYS: u8 = 4;
/// Last Yes positions kept per device; more start the memory over.
const MAX_REMEMBERED: usize = 256;

/// What one key under a gesture shows on the device.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Look {
    /// Held: how much of the hold time passed, in percent.
    Hold(u8),
    /// Armed: how much of the window is left, in percent.
    Armed(u8),
    /// The gesture ran the action.
    Done,
}

/// Where the answers of a confirmation screen are, by key index as the user
/// sees the keys.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Layout {
    pub yes: u8,
    pub no: u8,
    pub back: u8,
}

/// A confirmation screen as the device shows it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Screen {
    /// The protected key, shown as it is so it is clear what is asked.
    pub origin: u8,
    pub layout: Layout,
    /// How much of the time before it closes by itself is left, in percent.
    pub left: u8,
}

/// The keys of a device.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Board {
    pub keys: u8,
    pub columns: u8,
}

/// A key going down, as the guard needs to know it.
pub struct Press<P> {
    pub physical: u8,
    /// Key index as the user sees it.
    pub index: u8,
    /// The page and key it is on; a gesture belongs to one page.
    pub context: String,
    /// What the page looked like at the press. A gesture belongs to the
    /// page as it was: any edit to it ends the gesture.
    pub stamp: u64,
    /// The key's protection and what its action needs to run, or `None`
    /// for a key that runs on a plain press, or no key at all.
    pub protect: Option<(Protection, P)>,
}

/// What the runtime does with a key change.
#[derive(Debug, PartialEq, Eq)]
pub enum Verdict<P> {
    /// The key is not protected: its action hears the change as usual.
    Pass,
    /// The gesture took the change; the action hears nothing of it.
    Take,
    /// The gesture is complete: the action runs now, pressed and released.
    Fire(P),
}

enum Phase<P> {
    Holding {
        since: Instant,
        until: Instant,
        payload: P,
    },
    Armed {
        since: Instant,
        until: Instant,
    },
    Confirm {
        since: Instant,
        until: Instant,
        layout: Layout,
        /// The press that opened the screen is not over yet; until it is,
        /// presses do nothing, so a double tap cannot answer.
        origin_down: bool,
        payload: P,
    },
    Done {
        until: Instant,
    },
}

struct Pending<P> {
    physical: u8,
    index: u8,
    context: String,
    stamp: u64,
    phase: Phase<P>,
}

/// The gestures of one device: at most one at a time.
pub struct Guard<P> {
    board: Board,
    pending: Option<Pending<P>>,
    /// Keys whose press the guard took: their release is taken too.
    taken: Vec<u8>,
    /// Where Yes was the last time each key's screen opened, by context.
    /// Kept in memory only.
    last_yes: HashMap<String, u8>,
}

impl<P> Guard<P> {
    pub fn new(board: Board) -> Self {
        Self {
            board,
            pending: None,
            taken: Vec::new(),
            last_yes: HashMap::new(),
        }
    }

    /// A key went down. `seed` places shuffled answers.
    pub fn press(&mut self, press: Press<P>, seed: u64, now: Instant) -> Verdict<P> {
        let screen = match &self.pending {
            Some(Pending {
                phase:
                    Phase::Confirm {
                        layout,
                        origin_down,
                        ..
                    },
                ..
            }) => Some((*layout, *origin_down)),
            _ => None,
        };
        if let Some((layout, origin_down)) = screen {
            self.take(press.physical);
            if origin_down {
                return Verdict::Take;
            }
            if press.index == layout.yes {
                let done = Phase::Done {
                    until: now + DONE_TIME,
                };
                if let Some(pending) = self.pending.as_mut()
                    && let Phase::Confirm { payload, .. } =
                        std::mem::replace(&mut pending.phase, done)
                {
                    return Verdict::Fire(payload);
                }
            } else if press.index == layout.no || press.index == layout.back {
                self.pending = None;
            }
            // Every other key is blank and does nothing.
            return Verdict::Take;
        }
        let armed = self.pending.as_ref().and_then(|p| match p.phase {
            Phase::Armed { until, .. } if p.index == press.index && p.context == press.context => {
                Some(until)
            }
            _ => None,
        });
        // Any other press ends a gesture in progress.
        self.pending = None;
        let Some((mode, payload)) = press.protect else {
            return Verdict::Pass;
        };
        self.take(press.physical);
        let start = |phase| Pending {
            physical: press.physical,
            index: press.index,
            context: press.context.clone(),
            stamp: press.stamp,
            phase,
        };
        let hold = |ms: u32, payload| Phase::Holding {
            since: now,
            until: now + Duration::from_millis(u64::from(ms)),
            payload,
        };
        match mode.with_ms(mode.ms()) {
            Protection::Hold { ms } => {
                self.pending = Some(start(hold(ms, payload)));
                Verdict::Take
            }
            Protection::Twice { ms } => {
                if armed.is_some_and(|until| now < until) {
                    self.pending = Some(start(Phase::Done {
                        until: now + DONE_TIME,
                    }));
                    return Verdict::Fire(payload);
                }
                self.pending = Some(start(Phase::Armed {
                    since: now,
                    until: now + Duration::from_millis(u64::from(ms)),
                }));
                Verdict::Take
            }
            Protection::Confirm { ms, shuffle } => {
                let last = self.last_yes.get(&press.context).copied();
                let placed = place(self.board, press.index, shuffle, last, seed);
                let phase = match placed {
                    Some(layout) => {
                        if shuffle {
                            if self.last_yes.len() >= MAX_REMEMBERED {
                                self.last_yes.clear();
                            }
                            self.last_yes.insert(press.context.clone(), layout.yes);
                        }
                        Phase::Confirm {
                            since: now,
                            until: now + Duration::from_millis(u64::from(ms)),
                            layout,
                            origin_down: true,
                            payload,
                        }
                    }
                    // No room for a screen: holding the key confirms it.
                    None => hold(Protection::HOLD_DEFAULT_MS, payload),
                };
                self.pending = Some(start(phase));
                Verdict::Take
            }
        }
    }

    /// A key came up. A key held for its gesture that comes up too early
    /// runs nothing.
    pub fn release(&mut self, physical: u8) -> Verdict<P> {
        let mut early = false;
        if let Some(pending) = &mut self.pending
            && pending.physical == physical
        {
            match &mut pending.phase {
                Phase::Confirm { origin_down, .. } => *origin_down = false,
                Phase::Holding { .. } => early = true,
                _ => {}
            }
        }
        if early {
            self.pending = None;
        }
        match self.taken.iter().position(|&k| k == physical) {
            Some(i) => {
                self.taken.swap_remove(i);
                Verdict::Take
            }
            None => Verdict::Pass,
        }
    }

    /// Time passed. Returns the action to run when a hold is complete.
    pub fn tick(&mut self, now: Instant) -> Option<P> {
        let pending = self.pending.as_mut()?;
        match pending.phase {
            Phase::Holding { until, .. } if now >= until => {
                let done = Phase::Done {
                    until: now + DONE_TIME,
                };
                match std::mem::replace(&mut pending.phase, done) {
                    Phase::Holding { payload, .. } => Some(payload),
                    _ => None,
                }
            }
            Phase::Armed { until, .. } | Phase::Confirm { until, .. } | Phase::Done { until }
                if now >= until =>
            {
                self.pending = None;
                None
            }
            _ => None,
        }
    }

    /// Ends the gesture in progress without running anything. Releases of
    /// keys it took stay taken. Returns whether there was one.
    pub fn cancel(&mut self) -> bool {
        self.pending.take().is_some()
    }

    /// The key, the page and the page's stamp of the gesture in progress.
    pub fn pending_at(&self) -> Option<(u8, &str, u64)> {
        self.pending
            .as_ref()
            .map(|p| (p.index, p.context.as_str(), p.stamp))
    }

    /// When the guard needs [`Self::tick`] next: the next frame or the end
    /// of its time. `None` while nothing is in progress.
    pub fn deadline(&self, now: Instant) -> Option<Instant> {
        Some(match self.pending.as_ref()?.phase {
            Phase::Holding { until, .. } => until.min(now + HOLD_FRAME),
            Phase::Armed { until, .. } => until.min(now + ARMED_FRAME),
            Phase::Confirm { until, .. } => until.min(now + CONFIRM_FRAME),
            Phase::Done { until } => until,
        })
    }

    /// The key that shows a gesture, and how. A confirmation screen is
    /// [`Self::screen`] instead.
    pub fn look(&self, now: Instant) -> Option<(u8, Look)> {
        let pending = self.pending.as_ref()?;
        let look = match pending.phase {
            Phase::Holding { since, until, .. } => {
                Look::Hold(100 - percent_left(since, until, now))
            }
            Phase::Armed { since, until } => Look::Armed(percent_left(since, until, now)),
            Phase::Done { .. } => Look::Done,
            Phase::Confirm { .. } => return None,
        };
        Some((pending.index, look))
    }

    /// The confirmation screen the device shows, if one is open.
    pub fn screen(&self, now: Instant) -> Option<Screen> {
        let pending = self.pending.as_ref()?;
        match pending.phase {
            Phase::Confirm {
                since,
                until,
                layout,
                ..
            } => Some(Screen {
                origin: pending.index,
                layout,
                left: percent_left(since, until, now),
            }),
            _ => None,
        }
    }

    fn take(&mut self, physical: u8) {
        if !self.taken.contains(&physical) {
            self.taken.push(physical);
        }
    }
}

/// How much of the time from `since` to `until` is left at `now`, in percent.
fn percent_left(since: Instant, until: Instant, now: Instant) -> u8 {
    let total = until.saturating_duration_since(since).as_millis();
    let left = until.saturating_duration_since(now).as_millis();
    match (left * 100).checked_div(total) {
        // At most 100.
        Some(p) => p.min(100) as u8,
        None => 0,
    }
}

/// Where Yes, No and Back go on a confirmation screen opened by the key at
/// `origin`. The origin keeps showing the protected key, so Yes is never
/// where the opening press was. Without shuffling the answers have fixed
/// places: Yes at the bottom right, No at the bottom left, Back at the top
/// left, each moved on to the next free key when taken. Shuffled, they go
/// anywhere else, and Yes also avoids `last_yes` when the keys allow it.
/// `None` when the device has too few keys for a screen.
pub fn place(
    board: Board,
    origin: u8,
    shuffle: bool,
    last_yes: Option<u8>,
    seed: u64,
) -> Option<Layout> {
    let keys = board.keys;
    if keys < CONFIRM_MIN_KEYS || origin >= keys {
        return None;
    }
    if !shuffle {
        let columns = board.columns.clamp(1, keys);
        let mut used = vec![origin];
        let mut take = |preferred: u8| {
            let free = (0..keys)
                .map(|step| ((u16::from(preferred) + u16::from(step)) % u16::from(keys)) as u8)
                .find(|k| !used.contains(k))?;
            used.push(free);
            Some(free)
        };
        let yes = take(keys - 1)?;
        let no = take(keys - columns)?;
        let back = take(0)?;
        return Some(Layout { yes, no, back });
    }
    let mut rng = SplitMix(seed);
    let mut free: Vec<u8> = (0..keys).filter(|&k| k != origin).collect();
    let fresh: Vec<u8> = free
        .iter()
        .copied()
        .filter(|&k| Some(k) != last_yes)
        .collect();
    // Enough keys remain for No and Back after Yes either way; the last
    // place is the rule that gives way when there is no other choice.
    let candidates = if fresh.is_empty() { &free } else { &fresh };
    let yes = *candidates.get(rng.below(candidates.len()))?;
    free.retain(|&k| k != yes);
    let no = free.swap_remove(rng.below(free.len()));
    let back = free.swap_remove(rng.below(free.len()));
    Some(Layout { yes, no, back })
}

/// A small generator for placing answers; not for anything secret.
struct SplitMix(u64);

impl SplitMix {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// A number below `n`, or 0 for an empty range.
    fn below(&mut self, n: usize) -> usize {
        match u64::try_from(n) {
            Ok(n) if n > 0 => usize::try_from(self.next() % n).unwrap_or(0),
            _ => 0,
        }
    }
}

/// A seed for shuffling, from the clock.
pub fn seed() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos() as u64)
}

#[cfg(test)]
mod tests {
    use super::*;

    const MK2: Board = Board {
        keys: 15,
        columns: 5,
    };
    const MINI: Board = Board {
        keys: 6,
        columns: 3,
    };

    fn ms(n: u64) -> Duration {
        Duration::from_millis(n)
    }

    fn press(index: u8, protect: Option<Protection>) -> Press<&'static str> {
        Press {
            physical: index,
            index,
            context: format!("page.{index}"),
            stamp: 0,
            protect: protect.map(|p| (p, "run")),
        }
    }

    fn guard() -> Guard<&'static str> {
        Guard::new(MK2)
    }

    #[test]
    fn unprotected_keys_pass_through() {
        let mut g = guard();
        let t = Instant::now();
        assert_eq!(g.press(press(1, None), 0, t), Verdict::Pass);
        assert_eq!(g.release(1), Verdict::Pass);
        assert_eq!(g.deadline(t), None);
    }

    #[test]
    fn a_hold_released_early_runs_nothing() {
        let mut g = guard();
        let t = Instant::now();
        let hold = Some(Protection::hold());
        assert_eq!(g.press(press(2, hold), 0, t), Verdict::Take);
        assert_eq!(g.tick(t + ms(500)), None);
        assert_eq!(g.look(t + ms(500)), Some((2, Look::Hold(50))));
        assert_eq!(g.release(2), Verdict::Take);
        assert_eq!(g.look(t + ms(600)), None);
        assert_eq!(g.tick(t + ms(5_000)), None);
        assert_eq!(g.deadline(t), None);
    }

    #[test]
    fn a_hold_past_its_time_runs_once_while_still_held() {
        let mut g = guard();
        let t = Instant::now();
        let hold = Some(Protection::Hold { ms: 800 });
        g.press(press(2, hold), 0, t);
        // Frames come at a modest rate, and never later than the end.
        assert_eq!(g.deadline(t), Some(t + HOLD_FRAME));
        assert_eq!(g.deadline(t + ms(790)), Some(t + ms(800)));
        assert_eq!(g.tick(t + ms(799)), None);
        assert_eq!(g.tick(t + ms(800)), Some("run"));
        assert_eq!(g.look(t + ms(800)), Some((2, Look::Done)));
        assert_eq!(g.tick(t + ms(900)), None);
        // The release that follows is taken and runs nothing more.
        assert_eq!(g.release(2), Verdict::Take);
        assert_eq!(g.tick(t + ms(800) + DONE_TIME), None);
        assert_eq!(g.look(t + ms(800) + DONE_TIME), None);
        assert_eq!(g.deadline(t), None);
    }

    #[test]
    fn a_stored_time_out_of_bounds_is_clamped() {
        let mut g = guard();
        let t = Instant::now();
        let hold = Some(Protection::Hold { ms: 0 });
        g.press(press(2, hold), 0, t);
        assert_eq!(g.tick(t + ms(10)), None);
        let min = u64::from(Protection::HOLD_MIN_MS);
        assert_eq!(g.tick(t + ms(min)), Some("run"));
    }

    #[test]
    fn a_second_press_within_the_window_runs_once() {
        let mut g = guard();
        let t = Instant::now();
        let twice = Some(Protection::twice());
        assert_eq!(g.press(press(3, twice), 0, t), Verdict::Take);
        assert_eq!(g.release(3), Verdict::Take);
        // Releasing does not disarm.
        assert_eq!(g.look(t + ms(1_500)), Some((3, Look::Armed(50))));
        assert_eq!(g.tick(t + ms(1_500)), None);
        assert_eq!(
            g.press(press(3, twice), 0, t + ms(2_000)),
            Verdict::Fire("run")
        );
        assert_eq!(g.release(3), Verdict::Take);
        assert_eq!(g.look(t + ms(2_000)), Some((3, Look::Done)));
        // A third press arms again rather than running.
        assert_eq!(g.press(press(3, twice), 0, t + ms(2_100)), Verdict::Take);
    }

    #[test]
    fn a_second_press_after_the_window_only_arms_again() {
        let mut g = guard();
        let t = Instant::now();
        let twice = Some(Protection::twice());
        g.press(press(3, twice), 0, t);
        g.release(3);
        // Even before a tick noticed the window ended.
        assert_eq!(g.press(press(3, twice), 0, t + ms(3_000)), Verdict::Take);
        g.release(3);
        assert_eq!(g.tick(t + ms(6_000)), None);
        assert_eq!(g.look(t + ms(6_000)), None);
        assert_eq!(g.press(press(3, twice), 0, t + ms(6_100)), Verdict::Take);
    }

    #[test]
    fn another_key_disarms() {
        let mut g = guard();
        let t = Instant::now();
        let twice = Some(Protection::twice());
        g.press(press(3, twice), 0, t);
        g.release(3);
        assert_eq!(g.press(press(4, None), 0, t + ms(100)), Verdict::Pass);
        assert_eq!(g.release(4), Verdict::Pass);
        assert_eq!(g.look(t + ms(100)), None);
        assert_eq!(g.press(press(3, twice), 0, t + ms(200)), Verdict::Take);
    }

    #[test]
    fn the_same_key_on_another_page_is_another_key() {
        let mut g = guard();
        let t = Instant::now();
        let twice = Some(Protection::twice());
        g.press(press(3, twice), 0, t);
        g.release(3);
        let mut moved = press(3, twice);
        moved.context = "other-page.3".into();
        assert_eq!(g.press(moved, 0, t + ms(100)), Verdict::Take);
    }

    #[test]
    fn cancelling_runs_nothing_and_keeps_releases_taken() {
        let t = Instant::now();
        for protect in [
            Protection::hold(),
            Protection::twice(),
            Protection::confirm(),
        ] {
            let mut g = guard();
            g.press(press(5, Some(protect)), 0, t);
            assert_eq!(g.pending_at(), Some((5, "page.5", 0)));
            assert!(g.cancel());
            assert!(!g.cancel());
            assert_eq!(g.pending_at(), None);
            assert_eq!(g.tick(t + ms(60_000)), None);
            assert_eq!(g.release(5), Verdict::Take);
            assert_eq!(g.deadline(t), None);
        }
    }

    fn open_screen(g: &mut Guard<&'static str>, origin: u8, t: Instant) -> Screen {
        assert_eq!(
            g.press(press(origin, Some(Protection::confirm())), 7, t),
            Verdict::Take
        );
        let screen = g.screen(t).unwrap();
        assert_eq!(screen.origin, origin);
        assert_eq!(screen.left, 100);
        assert_eq!(g.look(t), None);
        assert_eq!(g.release(origin), Verdict::Take);
        screen
    }

    #[test]
    fn yes_runs_once_and_closes_the_screen() {
        let mut g = guard();
        let t = Instant::now();
        let layout = open_screen(&mut g, 6, t).layout;
        let yes = press(layout.yes, None);
        assert_eq!(g.press(yes, 0, t + ms(500)), Verdict::Fire("run"));
        assert_eq!(g.screen(t + ms(500)), None);
        assert_eq!(g.look(t + ms(500)), Some((6, Look::Done)));
        assert_eq!(g.release(layout.yes), Verdict::Take);
        assert_eq!(g.tick(t + ms(60_000)), None);
    }

    #[test]
    fn no_back_and_the_timeout_run_nothing() {
        let t = Instant::now();
        for answer in [|l: Layout| l.no, |l: Layout| l.back] {
            let mut g = guard();
            let layout = open_screen(&mut g, 6, t).layout;
            let key = answer(layout);
            assert_eq!(g.press(press(key, None), 0, t + ms(100)), Verdict::Take);
            assert_eq!(g.release(key), Verdict::Take);
            assert_eq!(g.screen(t + ms(100)), None);
            assert_eq!(g.tick(t + ms(60_000)), None);
            // Yes after the screen closed is just that key again.
            assert_eq!(
                g.press(press(layout.yes, None), 0, t + ms(200)),
                Verdict::Pass
            );
        }
        let mut g = guard();
        open_screen(&mut g, 6, t);
        assert_eq!(g.screen(t + ms(5_000)).map(|s| s.left), Some(50));
        assert_eq!(g.tick(t + ms(9_999)), None);
        assert!(g.screen(t + ms(9_999)).is_some());
        assert_eq!(g.tick(t + ms(10_000)), None);
        assert_eq!(g.screen(t + ms(10_000)), None);
        assert_eq!(g.deadline(t), None);
    }

    #[test]
    fn blank_keys_on_the_screen_do_nothing() {
        let mut g = guard();
        let t = Instant::now();
        let layout = open_screen(&mut g, 6, t).layout;
        let blank = (0..15)
            .find(|k| ![layout.yes, layout.no, layout.back].contains(k))
            .unwrap();
        // Neither a blank key, nor a protected key under the screen, nor
        // the origin itself answers.
        for key in [blank, 6] {
            let p = press(key, Some(Protection::twice()));
            assert_eq!(g.press(p, 0, t + ms(100)), Verdict::Take);
            assert_eq!(g.release(key), Verdict::Take);
        }
        assert!(g.screen(t + ms(100)).is_some());
    }

    #[test]
    fn presses_wait_for_the_opening_press_to_end() {
        let mut g = guard();
        let t = Instant::now();
        g.press(press(6, Some(Protection::confirm())), 0, t);
        let layout = g.screen(t).unwrap().layout;
        // Another finger on Yes while the key is still down does nothing.
        assert_eq!(
            g.press(press(layout.yes, None), 0, t + ms(50)),
            Verdict::Take
        );
        assert_eq!(g.release(layout.yes), Verdict::Take);
        assert!(g.screen(t + ms(50)).is_some());
        assert_eq!(g.release(6), Verdict::Take);
        assert_eq!(
            g.press(press(layout.yes, None), 0, t + ms(100)),
            Verdict::Fire("run")
        );
    }

    #[test]
    fn a_device_without_room_for_a_screen_asks_for_a_hold() {
        let mut g = Guard::new(Board {
            keys: 3,
            columns: 3,
        });
        let t = Instant::now();
        g.press(press(1, Some(Protection::confirm())), 0, t);
        assert_eq!(g.screen(t), None);
        assert_eq!(g.look(t), Some((1, Look::Hold(0))));
        let hold = u64::from(Protection::HOLD_DEFAULT_MS);
        assert_eq!(g.tick(t + ms(hold)), Some("run"));
    }

    fn check(layout: Layout, board: Board, origin: u8) {
        let all = [origin, layout.yes, layout.no, layout.back];
        for (i, a) in all.iter().enumerate() {
            assert!(*a < board.keys, "{layout:?}");
            assert!(!all[i + 1..].contains(a), "{layout:?} origin {origin}");
        }
    }

    #[test]
    fn fixed_places_are_distinct_and_never_put_yes_on_the_origin() {
        for board in [
            MINI,
            MK2,
            Board {
                keys: 32,
                columns: 8,
            },
            Board {
                keys: 4,
                columns: 4,
            },
        ] {
            for origin in 0..board.keys {
                let layout = place(board, origin, false, None, 0).unwrap();
                check(layout, board, origin);
                // Fixed means fixed: the seed changes nothing.
                assert_eq!(place(board, origin, false, None, 99), Some(layout));
            }
        }
        // A middle key on an MK.2: Yes bottom right, No bottom left, Back top left.
        assert_eq!(
            place(MK2, 7, false, None, 0),
            Some(Layout {
                yes: 14,
                no: 10,
                back: 0
            })
        );
        // The bottom right key moves Yes on.
        assert_eq!(place(MINI, 5, false, None, 0).map(|l| l.yes), Some(0));
        assert_eq!(
            place(
                Board {
                    keys: 3,
                    columns: 3
                },
                0,
                false,
                None,
                0
            ),
            None
        );
        assert_eq!(place(MINI, 6, false, None, 0), None);
    }

    #[test]
    fn shuffled_places_are_distinct_vary_and_avoid_the_last_yes() {
        for board in [
            MINI,
            MK2,
            Board {
                keys: 4,
                columns: 2,
            },
        ] {
            for origin in 0..board.keys {
                let mut seen = std::collections::HashSet::new();
                let mut last = None;
                for seed in 0..200 {
                    let layout = place(board, origin, true, last, seed).unwrap();
                    check(layout, board, origin);
                    assert_ne!(Some(layout.yes), last, "seed {seed}");
                    seen.insert(layout.yes);
                    last = Some(layout.yes);
                }
                // Yes goes to every other key over time.
                assert_eq!(seen.len(), usize::from(board.keys - 1));
            }
        }
    }

    #[test]
    fn the_smallest_screen_keeps_both_rules_for_yes() {
        let tiny = Board {
            keys: 4,
            columns: 4,
        };
        for seed in 0..50 {
            let layout = place(tiny, 0, true, Some(1), seed).unwrap();
            check(layout, tiny, 0);
            assert_ne!(layout.yes, 1);
        }
        // A remembered Yes on what is now the origin excludes nothing new.
        for seed in 0..50 {
            let layout = place(tiny, 2, true, Some(2), seed).unwrap();
            check(layout, tiny, 2);
        }
    }

    #[test]
    fn a_shuffled_screen_does_not_repeat_its_yes() {
        let mut g = guard();
        let t = Instant::now();
        let shuffled = Some(Protection::Confirm {
            ms: 10_000,
            shuffle: true,
        });
        let mut last = None;
        for seed in 0..50 {
            g.press(press(6, shuffled), seed, t);
            let yes = g.screen(t).unwrap().layout.yes;
            assert_ne!(Some(yes), last);
            assert_ne!(yes, 6);
            last = Some(yes);
            g.release(6);
            g.press(press(g.screen(t).unwrap().layout.back, None), 0, t);
        }
    }
}
