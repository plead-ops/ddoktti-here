//! Monitor-local logical coordinates. No window APIs: deterministic and testable.
use crate::surfaces::{WindowRect, World};
#[derive(Clone, Debug, PartialEq)]
pub struct Ledge {
    pub id: String,
    pub left: f64,
    pub right: f64,
    pub y: f64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Motion {
    Grounded,
    Fall,
    Land,
    Hurt,
    Prepare,
    Jump,
    Grab,
    Climb,
    Pull,
    Lower,
    Descend,
    Wobble,
}
impl Motion {
    pub fn pose(self) -> &'static str {
        match self {
            Self::Grounded => "idle",
            Self::Fall => "fall",
            Self::Land => "land",
            Self::Hurt => "hurt",
            Self::Prepare => "prepare",
            Self::Jump => "travel-jump",
            Self::Grab => "grab",
            Self::Climb => "climb",
            Self::Pull => "pull",
            Self::Lower => "lower",
            Self::Descend => "descend",
            Self::Wobble => "wobble",
        }
    }
}
fn clamp(v: f64, a: f64, b: f64) -> f64 {
    v.clamp(a, b.max(a))
}
fn smooth(t: f64) -> f64 {
    let t = t.clamp(0., 1.);
    t * t * (3. - 2. * t)
}
pub fn climb_distance(age: f64, size: f64) -> f64 {
    let cycles = age.max(0.) / 0.8;
    size * 0.4 * (cycles.floor() + smooth((cycles.fract() - 0.25) / 0.5))
}
pub fn climb_frame(age: f64) -> usize {
    [1, 0, 2, 3][((age.max(0.) / 0.2).floor() as usize) % 4]
}
/// Rope descent replays the climb cycle backwards, hands lowering in the same beat.
pub fn descend_frame(age: f64) -> usize {
    [3, 2, 0, 1][((age.max(0.) / 0.2).floor() as usize) % 4]
}
pub fn ledges(w: &World) -> Vec<Ledge> {
    let mut out = Vec::new();
    let half = w.size * 0.07;
    let height = w.size * 0.83;
    for (i, win) in w.windows.iter().enumerate() {
        if win.y < height || win.y > w.height {
            continue;
        }
        let mut parts = vec![(win.x.max(0.), (win.x + win.width).min(w.width))];
        for front in &w.windows[..i] {
            if front.y < win.y + 1. && front.y + front.height > win.y - height {
                parts = parts
                    .into_iter()
                    .flat_map(|(a, b)| {
                        if front.x + front.width <= a || front.x >= b {
                            vec![(a, b)]
                        } else {
                            vec![(a, b.min(front.x)), (a.max(front.x + front.width), b)]
                                .into_iter()
                                .filter(|(l, r)| r > l)
                                .collect()
                        }
                    })
                    .collect();
            }
        }
        for (left, right) in parts {
            if right - left >= half * 2. {
                out.push(Ledge {
                    id: win.id.clone(),
                    left,
                    right,
                    y: win.y,
                });
            }
        }
    }
    out
}
/// What the surrounding windows just did to us, for a reaction on top of physics.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Nudge {
    None,
    /// Our support window was dragged while we stood on it.
    Ride,
    /// Another window was moved into or right past our body.
    Brush,
}
#[derive(Clone)]
struct Plan {
    x: f64,
    y: f64,
    vx: f64,
    vy: f64,
    /// Landing ledge; `None` hops down onto the desktop floor.
    id: Option<String>,
}
pub struct Physics {
    pub x: f64,
    pub y: f64,
    pub direction: f64,
    pub motion: Motion,
    pub age: f64,
    pub world: World,
    pub ledges: Vec<Ledge>,
    attached: Option<WindowRect>,
    support: Option<String>,
    side: f64,
    vx: f64,
    vy: f64,
    cooldown: f64,
    plan: Option<Plan>,
    approach: Option<(String, f64)>,
    /// Side of our own support whose corner we walk to before roping down.
    descent: Option<f64>,
    /// Seconds spent on windows since last standing on the desktop floor.
    pub perch: f64,
    pull_start: (f64, f64),
    seed: u64,
    fall_origin: f64,
    deliberate_jump: bool,
    wander_side: f64,
    wander_streak: u8,
    /// Just arrived by rope (pulled onto a window top, or rappelled down to a
    /// landing); the caller shows a proud moment instead of a braced landing.
    pub climbed: bool,
    /// Why the last autonomous choice went the way it did (field diagnosis only).
    pub last_choice: &'static str,
    /// Seconds spent restless on a window with no rope or hop down available.
    stuck: f64,
    /// Where we have been lately: -1 all left, +1 all right, averaged over
    /// roughly the last ten minutes. Strolls from the floor lean the other way.
    pub balance: f64,
    /// The last fall was caused by our support window closing or minimising.
    /// Stays set through the landing for the caller to consume; any other way
    /// back to the ground clears it. Being covered or resized away is silent.
    pub lost_support: bool,
}
impl Physics {
    pub fn new(world: World) -> Self {
        let mut p = Self {
            x: world.x,
            y: world.y,
            direction: if world.x > world.width / 2. { -1. } else { 1. },
            motion: Motion::Fall,
            age: 0.,
            ledges: ledges(&world),
            world,
            attached: None,
            support: None,
            side: 1.,
            vx: 0.,
            vy: 0.,
            cooldown: 4.,
            plan: None,
            approach: None,
            descent: None,
            perch: 0.,
            pull_start: (0., 0.),
            seed: 0x927fd,
            fall_origin: 0.,
            deliberate_jump: false,
            wander_side: 0.,
            wander_streak: 0,
            lost_support: false,
            climbed: false,
            last_choice: "",
            stuck: 0.,
            balance: 0.,
        };
        p.reset(p.x, p.y);
        p
    }
    fn random(&mut self) -> f64 {
        self.seed = self.seed.wrapping_mul(6364136223846793005).wrapping_add(1);
        (self.seed >> 11) as f64 / (1u64 << 53) as f64
    }
    /// Choose only at the start of an autonomous stroll, never mid-route.
    pub fn begin_wander(&mut self, sample: f64) {
        if self.motion != Motion::Grounded || self.approaching() {
            return;
        }
        let mut side = if sample < 0.5 { -1. } else { 1. };
        if self.wander_streak >= 2 && side == self.wander_side {
            side = -side;
        }
        // Windows worth climbing tend to sit on one side of the desk and keep
        // pulling us there; from the floor, three strolls in four head the other
        // way (the streak limit yields to this, the edge margin below does not).
        if self.support.is_none() && self.balance.abs() > 0.3 && (sample * 4.).fract() < 0.75 {
            side = -self.balance.signum();
        }
        let support = self.at(self.x, self.y, self.support.as_deref());
        let left = support
            .as_ref()
            .map_or(self.half(), |p| p.left + self.foot());
        let right = support
            .as_ref()
            .map_or(self.world.width - self.half(), |p| p.right - self.foot());
        let margin = (self.world.size * 0.5).min((right - left).max(0.) * 0.25);
        if self.x <= left + margin {
            side = 1.;
        } else if self.x >= right - margin {
            side = -1.;
        }
        self.wander_streak = if side == self.wander_side {
            self.wander_streak.saturating_add(1)
        } else {
            1
        };
        self.wander_side = side;
        self.direction = side;
    }
    pub fn half(&self) -> f64 {
        self.world.size * 0.46
    }
    pub fn foot(&self) -> f64 {
        self.world.size * 0.07
    }
    pub fn on_floor(&self) -> bool {
        self.support.is_none()
            && (self.y - self.world.height).abs() < 2.
            && self.motion == Motion::Grounded
    }
    pub fn height(&self) -> f64 {
        self.world.size * 0.83
    }
    pub fn busy(&self) -> bool {
        self.motion != Motion::Grounded
    }
    /// Direction of the desktop-floor edge the character stands at, or has just
    /// bumped into. Walking into the edge enters `Wobble` in the same tick and the
    /// wobble turns the character around, so only the Grounded case would never
    /// observe a display crossing opportunity.
    pub fn edge_departure(&self) -> Option<f64> {
        if self.support.is_some() || (self.y - self.world.height).abs() >= 2. {
            return None;
        }
        let settled = match self.motion {
            Motion::Grounded => true,
            Motion::Wobble => self.age < 0.65,
            _ => false,
        };
        let at_edge = if self.direction > 0. {
            self.x >= self.world.width - self.half() - 2.
        } else {
            self.x <= self.half() + 2.
        };
        (settled && at_edge).then_some(self.direction)
    }
    fn entering(&mut self, m: Motion) {
        self.motion = m;
        self.age = 0.;
    }
    fn at(&self, x: f64, y: f64, id: Option<&str>) -> Option<Ledge> {
        self.ledges
            .iter()
            .find(|p| {
                id.is_none_or(|id| p.id == id)
                    && (p.y - y).abs() < 3.
                    && x >= p.left + self.foot()
                    && x <= p.right - self.foot()
            })
            .cloned()
    }
    // Body overhang is allowed; both feet must remain on the exposed segment.
    fn edge_landing(&self, w: &WindowRect, side: f64) -> Option<f64> {
        self.ledges
            .iter()
            .find(|p| {
                p.id == w.id
                    && if side > 0. {
                        p.left <= w.x + 2.
                    } else {
                        p.right >= w.x + w.width - 2.
                    }
            })
            .map(|p| {
                let edge = if side > 0. { w.x } else { w.x + w.width };
                clamp(
                    edge + side * (self.foot() + 3.),
                    p.left + self.foot(),
                    p.right - self.foot(),
                )
            })
    }
    fn attach(&mut self, p: Option<Ledge>) {
        self.support = p.map(|p| p.id);
        self.attached = self
            .world
            .windows
            .iter()
            .find(|w| Some(&w.id) == self.support.as_ref())
            .cloned();
    }
    pub fn reset(&mut self, x: f64, y: f64) {
        self.fall_origin = y;
        self.deliberate_jump = false;
        self.lost_support = false;
        self.x = clamp(x, self.half(), self.world.width - self.half());
        self.y = clamp(y, self.height(), self.world.height);
        self.vx = 0.;
        self.vy = 0.;
        self.plan = None;
        self.approach = None;
        self.descent = None;
        self.attach(self.at(self.x, self.y, None));
        self.entering(
            if self.support.is_some() || self.y >= self.world.height - 1. {
                Motion::Grounded
            } else {
                Motion::Fall
            },
        );
    }
    fn falling(&mut self) {
        self.fall_origin = self.y;
        self.deliberate_jump = false;
        self.approach = None;
        self.descent = None;
        self.attach(None);
        self.plan = None;
        self.vx = 0.;
        self.vy = 0.;
        self.entering(Motion::Fall);
    }
    fn climbing(&self) -> bool {
        matches!(
            self.motion,
            Motion::Grab | Motion::Climb | Motion::Pull | Motion::Lower | Motion::Descend
        )
    }
    /// Restlessness on windows: grows with time away from the floor and with
    /// altitude, so a long perch high up ends with a deliberate way down.
    pub fn support_id(&self) -> Option<&str> {
        self.support.as_deref()
    }
    pub fn descent_urge(&self) -> f64 {
        if self.support.is_none() {
            return 0.;
        }
        let altitude = ((self.world.height - self.y) / self.world.height).clamp(0., 1.);
        let restless = ((self.perch - 25.) / 95.).clamp(0., 1.);
        restless * (0.5 + 0.5 * altitude)
    }
    pub fn rope_anchor(&self) -> Option<(f64, f64)> {
        if !self.climbing() {
            return None;
        }
        self.attached
            .as_ref()
            .map(|w| (if self.side > 0. { w.x } else { w.x + w.width }, w.y))
    }
    fn climb_visible(&self, w: &WindowRect) -> bool {
        self.wall_visible(w, self.side, w.y.min(self.y - self.height()), self.y)
    }
    /// The strip beside `w`'s edge between `top` and `bottom` is not covered by
    /// any window in front of it, and lies far enough inside the display.
    fn wall_visible(&self, w: &WindowRect, side: f64, top: f64, bottom: f64) -> bool {
        let edge = if side > 0. { w.x } else { w.x + w.width };
        if w.y < self.height()
            || edge < self.half() * 2.
            || edge > self.world.width - self.half() * 2.
        {
            return false;
        }
        let left = if side > 0. {
            edge - self.half() * 2.
        } else {
            edge
        };
        !self
            .world
            .windows
            .iter()
            .take_while(|v| v.id != w.id)
            .any(|v| {
                v.x < left + self.half() * 2.
                    && v.x + v.width > left
                    && v.y < bottom
                    && v.y + v.height > top
            })
    }
    /// A scripted route owns position until it finishes. Do not acquire a
    /// nearby window as support, clamp across the display seam, or keep a stale rope.
    pub fn guide(&mut self, world: World) {
        self.x = world.x;
        self.y = world.y;
        self.world = world;
        self.ledges = ledges(&self.world);
        self.attached = None;
        self.support = None;
        self.plan = None;
        self.approach = None;
        self.descent = None;
        self.vx = 0.;
        self.vy = 0.;
        self.motion = Motion::Fall;
        self.age = 0.;
    }
    pub fn update(&mut self, world: World) -> Nudge {
        if world.monitor != self.world.monitor || world.size != self.world.size {
            self.world = world;
            self.ledges = ledges(&self.world);
            self.attached = None;
            self.support = None;
            self.reset(self.world.x, self.world.y);
            return Nudge::None;
        }
        let brush = self.brushed(&world);
        let grounded = self.motion == Motion::Grounded;
        self.world = world;
        self.ledges = ledges(&self.world);
        let mut nudge = Nudge::None;
        if let Some(old) = self.attached.clone() {
            if let Some(next) = self.world.windows.iter().find(|w| w.id == old.id).cloned() {
                if self.climbing() {
                    let dx = if self.side > 0. {
                        next.x - old.x
                    } else {
                        next.x + next.width - old.x - old.width
                    };
                    let dy = next.y - old.y;
                    self.x += dx;
                    self.y += dy;
                    self.pull_start.0 += dx;
                    self.pull_start.1 += dy;
                } else {
                    self.x = next.x + (self.x - old.x) / old.width * next.width;
                    self.y += next.y - old.y;
                }
                self.attached = Some(next.clone());
                // A drag moves both edges together; an edge resize moves only one.
                let step = self.world.size * 0.15;
                let (dl, dr) = (next.x - old.x, next.x + next.width - old.x - old.width);
                let (dt, db) = (next.y - old.y, next.y + next.height - old.y - old.height);
                let dragged = (dl.abs() >= step && (dr - dl).abs() < 2.)
                    || (dt.abs() >= step && (db - dt).abs() < 2.);
                if (!self.climbing() && self.at(self.x, self.y, Some(&next.id)).is_none())
                    || (self.climbing() && !self.climb_visible(&next))
                    || self.x < self.half()
                    || self.x > self.world.width - self.half()
                    || self.y < self.height()
                    || self.y > self.world.height
                {
                    self.falling();
                } else if grounded && dragged {
                    nudge = Nudge::Ride;
                }
            } else {
                self.falling();
                self.lost_support = true;
            }
        }
        self.x = clamp(self.x, self.half(), self.world.width - self.half());
        self.y = clamp(self.y, self.height(), self.world.height);
        if nudge == Nudge::None && grounded && brush {
            Nudge::Brush
        } else {
            nudge
        }
    }
    // A window other than our support, in front of it, that moved a noticeable
    // step and now overlaps the space around our body.
    fn brushed(&self, world: &World) -> bool {
        let reach = self.world.size * 0.5;
        let (l, t) = (self.x - self.half() - reach, self.y - self.height() - reach);
        let (r, b) = (self.x + self.half() + reach, self.y + reach);
        let behind = self
            .support
            .as_ref()
            .and_then(|id| world.windows.iter().position(|w| &w.id == id))
            .unwrap_or(world.windows.len());
        world.windows.iter().take(behind).any(|w| {
            Some(&w.id) != self.support.as_ref()
                && self.world.windows.iter().any(|old| {
                    old.id == w.id && (w.x - old.x).abs() + (w.y - old.y).abs() >= self.world.size * 0.15
                })
                && w.x < r
                && w.x + w.width > l
                && w.y < b
                && w.y + w.height > t
        })
    }
    pub fn approaching(&self) -> bool {
        (self.approach.is_some() || self.descent.is_some()) && self.motion == Motion::Grounded
    }
    /// Highest exposed ledge under `x` at or below `y`, else the desktop floor.
    fn floor_below(&self, x: f64, y: f64) -> (Option<Ledge>, f64) {
        let p = self
            .ledges
            .iter()
            .filter(|p| p.y >= y && x >= p.left + self.foot() && x <= p.right - self.foot())
            .min_by(|a, b| a.y.total_cmp(&b.y))
            .cloned();
        let landing = p.as_ref().map_or(self.world.height, |p| p.y);
        (p, landing)
    }
    // Walk along our own support to its exposed corner, then rope down that side.
    // Returns the window, the standing x at the corner and the hanging x beside it.
    fn descent_target(&self, side: f64) -> Option<(WindowRect, f64, f64)> {
        let w = self.attached.clone()?;
        if self.support.as_deref() != Some(&w.id) {
            return None;
        }
        let stand = self.edge_landing(&w, side)?;
        let here = self.at(self.x, self.y, Some(&w.id))?;
        if stand < here.left + self.foot() || stand > here.right - self.foot() {
            return None;
        }
        let edge = if side > 0. { w.x } else { w.x + w.width };
        let hang = edge - side * self.half();
        if hang < self.half() || hang > self.world.width - self.half() {
            return None;
        }
        let (_, landing) = self.floor_below(hang, w.y + 1.);
        if landing - w.y < self.height() || !self.wall_visible(&w, side, w.y, landing) {
            return None;
        }
        Some((w, stand, hang))
    }
    fn seek_descent(&mut self) {
        let mut best: Option<(f64, f64)> = None;
        for side in [1., -1.] {
            if let Some((_, stand, _)) = self.descent_target(side) {
                let cost = (stand - self.x).abs();
                if best.is_none_or(|b| cost < b.1) {
                    best = Some((side, cost));
                }
            }
        }
        self.descent = best.map(|b| b.0);
    }
    fn approach_descent(&mut self, distance: f64) -> bool {
        let Some(side) = self.descent else {
            return false;
        };
        let Some((_, stand, _)) = self.descent_target(side) else {
            self.descent = None;
            self.cooldown = 2.;
            return false;
        };
        let dx = stand - self.x;
        if dx.abs() > 0.01 {
            self.direction = dx.signum();
        }
        self.x += dx.clamp(-distance, distance);
        if (stand - self.x).abs() < 0.01 {
            self.side = side;
            self.direction = side;
            self.descent = None;
            self.pull_start = (self.x, self.y);
            self.entering(Motion::Lower);
        }
        true
    }
    // Approach a visible edge on the current support, then climb the rope even
    // when the window's bottom is above our head. Never teleport across a gap.
    fn rope_target(&mut self, id: &str, side: f64) -> Option<(WindowRect, f64)> {
        let w = self.world.windows.iter().find(|w| w.id == id)?.clone();
        if self.support.as_deref() == Some(id) || w.y >= self.y - self.height() {
            return None;
        }
        self.side = side;
        let edge = if side > 0. { w.x } else { w.x + w.width };
        let x = edge - side * self.half();
        let landing = self.edge_landing(&w, side)?;
        if self.at(landing, w.y, Some(id)).is_none() || !self.climb_visible(&w) {
            return None;
        }
        if self.support.is_some() && self.at(x, self.y, self.support.as_deref()).is_none() {
            return None;
        }
        Some((w, x))
    }
    fn seek_rope(&mut self) {
        let ids: Vec<_> = self.world.windows.iter().map(|w| w.id.clone()).collect();
        let mut best: Option<(String, f64, f64)> = None;
        for id in ids {
            for side in [1., -1.] {
                if let Some((w, x)) = self.rope_target(&id, side) {
                    let cost = (x - self.x).abs() + (self.y - w.y) * 0.2;
                    if best.as_ref().is_none_or(|b| cost < b.2) {
                        best = Some((id.clone(), side, cost));
                    }
                }
            }
        }
        self.approach = best.map(|(id, side, _)| (id, side));
    }
    fn approach_rope(&mut self, distance: f64) -> bool {
        let Some((id, side)) = self.approach.clone() else {
            return false;
        };
        let Some((w, x)) = self.rope_target(&id, side) else {
            self.approach = None;
            self.cooldown = 2.;
            return false;
        };
        let dx = x - self.x;
        if dx.abs() > 0.01 {
            self.direction = dx.signum();
        }
        self.x += dx.clamp(-distance, distance);
        if (x - self.x).abs() < 0.01 {
            self.side = side;
            self.direction = side;
            self.support = Some(w.id.clone());
            self.attached = Some(w);
            self.approach = None;
            self.entering(Motion::Grab);
        }
        true
    }
    fn start_climb(&mut self, dx: f64) -> bool {
        let side = dx.signum();
        if side == 0. {
            return false;
        }
        for w in self.world.windows.clone() {
            let edge = if side > 0. { w.x } else { w.x + w.width };
            let hand = self.x + side * self.half();
            if self.y <= w.y + self.height() * 0.35
                || self.y - self.height() >= w.y + w.height
                || (edge - hand).abs() > dx.abs() + 5.
                || (edge - hand) * side < -3.
            {
                continue;
            }
            self.side = side;
            if !self.ledges.iter().any(|p| {
                p.id == w.id
                    && if side > 0. {
                        p.left <= w.x + 2.
                    } else {
                        p.right >= w.x + w.width - 2.
                    }
            }) || !self.climb_visible(&w)
            {
                continue;
            }
            self.direction = side;
            self.x = edge - side * self.half();
            self.support = Some(w.id.clone());
            self.attached = Some(w);
            self.entering(Motion::Grab);
            return true;
        }
        false
    }
    fn arc(&self, x: f64, y: f64, id: Option<String>) -> Option<Plan> {
        let dx = x - self.x;
        if dx.abs() < self.half() || dx.abs() > 300. {
            return None;
        }
        let rise = (self.y - y + 55.).max(70.);
        if self.y - rise < self.height() + 5. {
            return None;
        }
        let vy = -(1800. * rise).sqrt();
        let time = (-vy + (vy * vy + 1800. * (y - self.y)).sqrt()) / 900.;
        (time > 0. && (dx / time).abs() <= 350.).then(|| Plan {
            x,
            y,
            vx: dx / time,
            vy,
            id,
        })
    }
    /// Trapped on a window with no way down for this long, a far hop is allowed.
    const STUCK_AFTER: f64 = 45.;
    /// `far` lifts the 220px range for destinations below us: the escape route
    /// when every rope and short hop is blocked. Deliberate, so never a hurt.
    fn jump_plans(&self, far: bool) -> Vec<Plan> {
        let mut plans = Vec::new();
        for p in &self.ledges {
            let dy = p.y - self.y;
            if Some(&p.id) == self.support.as_ref() || dy <= -220. || (dy >= 220. && !far) {
                continue;
            }
            let margin = 8_f64.min(((p.right - p.left) / 2. - self.foot()).max(0.));
            let x = clamp(
                self.x,
                p.left + self.foot() + margin,
                p.right - self.foot() - margin,
            );
            plans.extend(self.arc(x, p.y, Some(p.id.clone())));
        }
        // A short hop from a corner of our window straight down to the floor.
        if self.support.is_some() && (self.world.height - self.y < 220. || far) {
            if let Some(p) = self.at(self.x, self.y, self.support.as_deref()) {
                for side in [1., -1.] {
                    let x = clamp(
                        if side > 0. {
                            p.right + self.half()
                        } else {
                            p.left - self.half()
                        },
                        self.half(),
                        self.world.width - self.half(),
                    );
                    if (x - self.x).abs() <= self.half() * 2.5 {
                        plans.extend(self.arc(x, self.world.height, None));
                    }
                }
            }
        }
        plans
    }
    fn jump_plan(&mut self) -> Option<Plan> {
        let plans = self.jump_plans(false);
        let i = (self.random() * plans.len() as f64) as usize;
        plans.get(i).cloned()
    }
    /// Only destinations below us: a hop beats a rope when the drop is short.
    fn drop_plan(&mut self) -> Option<Plan> {
        let plans: Vec<_> = self
            .jump_plans(self.stuck >= Self::STUCK_AFTER)
            .into_iter()
            .filter(|p| p.y > self.y + 1.)
            .collect();
        let i = (self.random() * plans.len() as f64) as usize;
        plans.get(i).cloned()
    }
    pub fn follow_step(&mut self, dt: f64, speed: f64) -> bool {
        if self.motion != Motion::Grounded || speed <= 0. || dt <= 0. {
            return false;
        }
        let p = self.at(self.x, self.y, self.support.as_deref());
        let left = p.as_ref().map_or(self.half(), |p| p.left + self.foot());
        let right = p
            .as_ref()
            .map_or(self.world.width - self.half(), |p| p.right - self.foot());
        let next = self.x + self.direction * speed * dt;
        self.x = clamp(next, left, right);
        if next < left || next > right {
            self.approach = None;
            self.descent = None;
            self.entering(Motion::Wobble);
            return true;
        }
        false
    }
    pub fn step(&mut self, seconds: f64, walk: f64, autonomous: bool, reduced: bool) {
        let dt = seconds.clamp(0., 0.1);
        let n = (dt / (1. / 60.)).ceil().max(1.) as usize;
        for _ in 0..n {
            self.tick(dt / n as f64, walk, autonomous, reduced);
        }
    }
    fn tick(&mut self, dt: f64, walk: f64, autonomous: bool, reduced: bool) {
        let old_age = self.age;
        self.age += dt;
        self.cooldown = (self.cooldown - dt).max(0.);
        if self.support.is_some() {
            self.perch += dt;
            if self.last_choice == "no-way-down" {
                self.stuck += dt;
            }
        } else if matches!(self.motion, Motion::Grounded | Motion::Land | Motion::Hurt) {
            self.perch = 0.;
            self.stuck = 0.;
        }
        let here = ((self.x - self.world.width / 2.) / (self.world.width / 2.)).clamp(-1., 1.);
        self.balance += (here - self.balance) * (dt / 300.).min(1.);
        if reduced {
            self.approach = None;
            self.descent = None;
            if self.busy() {
                let p = self
                    .ledges
                    .iter()
                    .filter(|p| {
                        p.y >= self.y - 3.
                            && self.x >= p.left + self.foot()
                            && self.x <= p.right - self.foot()
                    })
                    .min_by(|a, b| a.y.total_cmp(&b.y))
                    .cloned();
                self.y = p.as_ref().map_or(self.world.height, |p| p.y);
                self.attach(p);
                self.entering(Motion::Grounded);
                self.vx = 0.;
                self.vy = 0.;
                self.lost_support = false;
            }
            return;
        }
        match self.motion {
            Motion::Hurt => {
                if self.age >= 2.8 {
                    self.entering(Motion::Grounded);
                }
                return;
            }
            Motion::Land => {
                if self.age >= 0.45 {
                    self.entering(Motion::Grounded);
                }
                return;
            }
            Motion::Grab => {
                if self.age >= 0.35 {
                    self.entering(Motion::Climb);
                }
                return;
            }
            Motion::Climb => {
                if let Some(w) = &self.attached {
                    let target = w.y + self.height() * 0.6;
                    self.y = (self.y
                        - (climb_distance(self.age, self.world.size)
                            - climb_distance(old_age, self.world.size)))
                    .max(target);
                    if self.y <= target + 0.1 {
                        self.pull_start = (self.x, self.y);
                        self.entering(Motion::Pull);
                    }
                } else {
                    self.falling();
                }
                return;
            }
            Motion::Pull => {
                if let Some(w) = self.attached.clone() {
                    let Some(x) = self.edge_landing(&w, self.side) else {
                        self.falling();
                        return;
                    };
                    let t = smooth(self.age / 0.7);
                    self.x = self.pull_start.0 + (x - self.pull_start.0) * t;
                    self.y = self.pull_start.1 + (w.y - self.pull_start.1) * t;
                    if t >= 1. {
                        if let Some(p) = self.at(self.x, self.y, Some(&w.id)) {
                            // Arriving on top is an achievement, not a landing to brace for.
                            self.attach(Some(p));
                            self.entering(Motion::Grounded);
                            self.climbed = true;
                            self.cooldown = 6.;
                        } else {
                            self.falling();
                        }
                    }
                } else {
                    self.falling();
                }
                return;
            }
            Motion::Lower => {
                if let Some(w) = self.attached.clone() {
                    if self.edge_landing(&w, self.side).is_none() {
                        self.falling();
                        return;
                    }
                    let edge = if self.side > 0. { w.x } else { w.x + w.width };
                    let hang = edge - self.side * self.half();
                    let t = smooth(self.age / 0.7);
                    self.x = self.pull_start.0 + (hang - self.pull_start.0) * t;
                    self.y = self.pull_start.1 + (w.y + self.height() * 0.6 - self.pull_start.1) * t;
                    if t >= 1. {
                        self.entering(Motion::Descend);
                    }
                } else {
                    self.falling();
                }
                return;
            }
            Motion::Descend => {
                if self.attached.is_none() {
                    self.falling();
                    return;
                }
                let (p, landing) = self.floor_below(self.x, self.y - 1.);
                self.y = (self.y
                    + (climb_distance(self.age, self.world.size)
                        - climb_distance(old_age, self.world.size)))
                .min(landing);
                if self.y >= landing - 0.1 {
                    // Stepping off the rope is deliberate: no bracing, no relief.
                    self.y = landing;
                    self.attach(p);
                    self.vx = 0.;
                    self.vy = 0.;
                    self.entering(Motion::Grounded);
                    self.climbed = true;
                    self.cooldown = 5.;
                }
                return;
            }
            Motion::Prepare => {
                let valid = self.plan.as_ref().is_some_and(|p| {
                    p.id.as_ref().is_none_or(|id| {
                        self.ledges.iter().any(|l| {
                            l.id == *id
                                && (l.y - p.y).abs() < 3.
                                && p.x >= l.left + self.foot()
                                && p.x <= l.right - self.foot()
                        })
                    })
                });
                if !autonomous || !valid {
                    self.plan = None;
                    self.entering(Motion::Grounded);
                    return;
                }
                if self.age >= 0.28 {
                    let p = self.plan.clone().unwrap();
                    self.attach(None);
                    self.vx = p.vx;
                    self.vy = p.vy;
                    self.direction = self.vx.signum();
                    self.deliberate_jump = true;
                    self.lost_support = false;
                    self.entering(Motion::Jump);
                }
                return;
            }
            Motion::Wobble => {
                if self.age >= 0.65 {
                    if autonomous && self.support.is_some() && self.random() < 0.18 {
                        self.falling();
                        self.y += 3.;
                        self.vx = self.direction * 100.;
                    } else {
                        self.direction *= -1.;
                        self.entering(Motion::Grounded);
                    }
                    self.cooldown = 3.;
                }
                return;
            }
            Motion::Fall | Motion::Jump => {
                let (ox, oy) = (self.x, self.y);
                // Integrate acceleration exactly so narrow jump targets do not
                // shift with frame rate; include any time spent at terminal speed.
                let accelerating = ((700. - self.vy) / 900.).clamp(0., dt);
                let dy = self.vy * accelerating
                    + 450. * accelerating * accelerating
                    + 700. * (dt - accelerating);
                self.vy = (self.vy + 900. * dt).min(700.);
                self.x = clamp(
                    self.x + self.vx * dt,
                    self.half(),
                    self.world.width - self.half(),
                );
                self.y = (self.y + dy).max(self.height());
                if self.y <= self.height() && self.vy < 0. {
                    self.vy = 0.;
                }
                if self.vy >= 0. {
                    let p = self
                        .ledges
                        .iter()
                        .filter(|p| {
                            if p.y < oy - 2. || p.y > self.y {
                                return false;
                            }
                            let x = ox
                                + (self.x - ox)
                                    * ((p.y - oy) / (self.y - oy).max(0.0001)).clamp(0., 1.);
                            x >= p.left + self.foot() && x <= p.right - self.foot()
                        })
                        .min_by(|a, b| a.y.total_cmp(&b.y))
                        .cloned();
                    if p.is_some() || self.y >= self.world.height {
                        self.y = p.as_ref().map_or(self.world.height, |p| p.y);
                        if let Some(p) = &p {
                            self.x = clamp(self.x, p.left + self.foot(), p.right - self.foot());
                        }
                        self.attach(p);
                        self.vx = 0.;
                        self.vy = 0.;
                        self.plan = None;
                        let hurt = !self.deliberate_jump
                            && self.y - self.fall_origin > self.height() * 1.2;
                        self.entering(if hurt { Motion::Hurt } else { Motion::Land });
                        self.cooldown = 5.;
                    }
                }
                return;
            }
            Motion::Grounded => {}
        }
        if self.support.is_some() && self.at(self.x, self.y, self.support.as_deref()).is_none()
            || self.support.is_none() && self.y < self.world.height - 1.
        {
            self.falling();
            return;
        }
        if !autonomous {
            self.approach = None;
            self.descent = None;
            return;
        }
        if self.approaching()
            && (self.approach_rope(walk.abs() * dt) || self.approach_descent(walk.abs() * dt))
        {
            return;
        }
        if self.cooldown == 0. && walk > 0. {
            self.cooldown = 8. + self.random() * 8.;
            // Restless on a window: hop down when the drop is short, otherwise rope
            // down our own side. A settled character keeps looking for ropes up.
            if self.random() < self.descent_urge() {
                if let Some(p) = self.drop_plan() {
                    self.plan = Some(p);
                    self.last_choice = "hop-down";
                    self.stuck = 0.;
                    self.entering(Motion::Prepare);
                    return;
                }
                self.seek_descent();
                if self.approach_descent(walk.abs() * dt) {
                    self.last_choice = "rope-down";
                    self.stuck = 0.;
                    return;
                }
                self.last_choice = if self.descent_target(1.).is_none()
                    && self.descent_target(-1.).is_none()
                {
                    "no-way-down"
                } else {
                    "descent-blocked"
                };
            } else {
                self.seek_rope();
                if self.approach_rope(walk.abs() * dt) {
                    self.last_choice = "rope-up";
                    return;
                }
                self.last_choice = "no-rope";
            }
            if let Some(p) = self.jump_plan() {
                if self.random() < 0.55 {
                    self.plan = Some(p);
                    self.last_choice = "jump";
                    self.entering(Motion::Prepare);
                    return;
                }
            }
        }
        let dx = walk * self.direction * dt;
        if dx == 0. {
            return;
        }
        // Bumping into a wall starts a climb only while we are content up here;
        // a restless character would otherwise re-climb the window it just left.
        if self.descent_urge() < 0.5 && self.start_climb(dx) {
            return;
        }
        let p = self.at(self.x, self.y, self.support.as_deref());
        let left = p.as_ref().map_or(self.half(), |p| p.left + self.foot());
        let right = p
            .as_ref()
            .map_or(self.world.width - self.half(), |p| p.right - self.foot());
        let next = self.x + dx;
        if next < left || next > right {
            self.x = clamp(next, left, right);
            self.entering(Motion::Wobble);
        } else {
            self.x = next;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn rect(id: &str, x: f64, y: f64, width: f64, height: f64) -> WindowRect {
        WindowRect {
            id: id.into(),
            x,
            y,
            width,
            height,
        }
    }
    fn world(windows: Vec<WindowRect>, x: f64, y: f64) -> World {
        World {
            monitor: "main".into(),
            width: 1000.,
            height: 700.,
            size: 100.,
            x,
            y,
            windows,
        }
    }
    fn advance(p: &mut Physics, seconds: f64, walk: f64, auto: bool) {
        for _ in 0..(seconds * 60.).ceil() as usize {
            p.step(1. / 60., walk, auto, false);
        }
    }
    #[test]
    fn new_strolls_do_not_keep_one_direction_or_override_routes() {
        let mut p = Physics::new(world(vec![], 500., 700.));
        advance(&mut p, 1., 0., false);
        for sample in [0.9, 0.1] {
            let mut previous = 0.;
            let mut streak = 0;
            let mut sides = [0; 2];
            for _ in 0..60 {
                p.begin_wander(sample);
                sides[usize::from(p.direction > 0.)] += 1;
                streak = if p.direction == previous {
                    streak + 1
                } else {
                    1
                };
                assert!(streak <= 2);
                previous = p.direction;
            }
            assert!(sides.iter().all(|n| *n >= 20));
        }
        p.x = 950.;
        p.begin_wander(0.9);
        assert_eq!(p.direction, -1.);
        p.x = 50.;
        p.begin_wander(0.1);
        assert_eq!(p.direction, 1.);
        p.approach = Some(("window".into(), 1.));
        p.begin_wander(0.1);
        assert_eq!(p.direction, 1.);
        p.approach = None;
        p.motion = Motion::Fall;
        p.begin_wander(0.1);
        assert_eq!(p.direction, 1.);
    }
    #[test]
    fn strolls_from_the_floor_lean_away_from_where_we_have_been() {
        let mut p = Physics::new(world(vec![], 900., 700.));
        advance(&mut p, 1., 0., false);
        assert!(p.balance.abs() < 0.1, "fresh start is unbiased");
        // Ten minutes parked on the far right: the balance follows.
        advance(&mut p, 600., 0., false);
        assert!(p.balance > 0.6, "{}", p.balance);
        p.x = 500.;
        let mut left = 0;
        for i in 0..40 {
            p.begin_wander((i as f64 * 0.37) % 1.);
            left += usize::from(p.direction < 0.);
        }
        assert!(left >= 27, "{left} of 40 strolls head left");
        // On a window the bias does not apply (ropes and corners decide there).
        let mut w = Physics::new(world(vec![rect("a", 100., 400., 800., 300.)], 800., 400.));
        advance(&mut w, 2., 0., false);
        assert_eq!(w.support.as_deref(), Some("a"));
        w.balance = 0.9;
        w.x = 500.;
        w.begin_wander(0.9);
        assert_eq!(w.direction, 1.);
    }
    #[test]
    fn floating_window_is_approached_from_either_side_then_climbed() {
        for x in [70., 920.] {
            let mut p = Physics::new(world(
                vec![rect("floating", 300., 180., 350., 160.)],
                x,
                700.,
            ));
            p.cooldown = 0.;
            let mut climbed = false;
            for _ in 0..2400 {
                let old_x = p.x;
                let grounded = p.motion == Motion::Grounded;
                p.step(1. / 60., 80., true, false);
                if grounded {
                    assert!((p.x - old_x).abs() <= 80. / 60. + 0.01);
                }
                if p.motion == Motion::Climb {
                    climbed = true;
                    assert!(p.rope_anchor().is_some());
                }
                if p.motion == Motion::Grounded && p.support.as_deref() == Some("floating") {
                    break;
                }
            }
            assert!(climbed);
            assert_eq!(p.y, 180.);
            assert_eq!(p.motion, Motion::Grounded);
            assert!(std::mem::take(&mut p.climbed), "arrival is reported once");
            assert!(!p.climbed);
            assert!(p.rope_anchor().is_none());
        }
    }
    #[test]
    fn rope_approach_revalidates_moved_closed_and_occluded_windows() {
        let mut p = Physics::new(world(
            vec![rect("floating", 300., 180., 350., 160.)],
            80.,
            700.,
        ));
        p.cooldown = 0.;
        p.step(0.1, 80., true, false);
        assert!(p.approaching());
        p.update(world(
            vec![rect("floating", 500., 160., 350., 160.)],
            p.x,
            p.y,
        ));
        let before = p.x;
        p.step(0.1, 80., true, false);
        assert!((p.x - before - 8.).abs() < 0.01);
        assert!(p.approaching());
        p.update(world(vec![], p.x, p.y));
        p.step(0.1, 80., true, false);
        assert!(!p.approaching());
        assert_eq!(p.motion, Motion::Grounded);
        p.update(world(
            vec![
                rect("front", 180., 100., 700., 550.),
                rect("floating", 500., 160., 350., 160.),
            ],
            p.x,
            p.y,
        ));
        assert!(p.rope_target("floating", 1.).is_none());
        assert!(p.rope_target("floating", -1.).is_none());
    }
    #[test]
    fn rope_approach_respects_support_alerts_reduced_motion_and_drag() {
        let mut p = Physics::new(world(
            vec![
                rect("floating", 600., 180., 300., 150.),
                rect("support", 100., 500., 300., 200.),
            ],
            200.,
            500.,
        ));
        // Cannot walk off our current window to reach a distant rope.
        assert!(p.rope_target("floating", 1.).is_none());
        assert!(p.rope_target("floating", -1.).is_none());
        for action in 0..3 {
            let mut p = Physics::new(world(
                vec![rect("floating", 400., 180., 300., 150.)],
                80.,
                700.,
            ));
            p.cooldown = 0.;
            p.step(0.1, 80., true, false);
            assert!(p.approaching());
            match action {
                0 => p.step(0.1, 80., false, false),
                1 => p.step(0.1, 80., true, true),
                _ => p.reset(200., 700.),
            }
            assert!(!p.approaching());
            assert!(p.rope_anchor().is_none());
        }
    }
    #[test]
    fn pointer_follow_reacts_at_both_window_edges_then_can_retreat() {
        for direction in [-1., 1.] {
            let mut p = Physics::new(world(vec![rect("a", 200., 300., 400., 400.)], 400., 300.));
            p.direction = direction;
            let edge = if direction > 0. {
                600. - p.foot()
            } else {
                200. + p.foot()
            };
            p.x = edge - direction;
            assert!(!p.follow_step(1. / 60., 0.));
            assert!(p.follow_step(1. / 60., 180.));
            assert_eq!(p.motion, Motion::Wobble);
            assert!((p.x - edge).abs() < 1e-8);
            assert!(!p.follow_step(1. / 60., 180.));
            advance(&mut p, 0.7, 0., false);
            assert_eq!(p.motion, Motion::Grounded);
            assert_eq!(p.direction, -direction);
            let x = p.x;
            p.step(0.1, 80., true, false);
            assert!((p.x - x) * direction < 0.);
            assert_eq!(p.support.as_deref(), Some("a"));
        }
    }
    #[test]
    fn pointer_follow_screen_edge_never_falls_outside_the_desktop() {
        let mut p = Physics::new(world(vec![], 400., 700.));
        p.x = p.world.width - p.half();
        p.direction = 1.;
        assert!(p.follow_step(1. / 60., 180.));
        advance(&mut p, 0.7, 0., true);
        assert_eq!(p.motion, Motion::Grounded);
        assert_eq!(p.direction, -1.);
        assert_eq!(p.y, p.world.height);
    }
    #[test]
    fn occlusion_and_foot_support() {
        let w = world(
            vec![
                rect("front", 350., 100., 200., 300.),
                rect("back", 100., 300., 700., 400.),
                rect("top", 0., 20., 400., 400.),
            ],
            400.,
            100.,
        );
        let ls = ledges(&w);
        let back: Vec<_> = ls
            .iter()
            .filter(|p| p.id == "back")
            .map(|p| (p.left, p.right))
            .collect();
        assert_eq!(back, vec![(100., 350.), (550., 800.)]);
        assert!(!ls.iter().any(|p| p.id == "top"));
        let mut p = Physics::new(world(vec![rect("a", 250., 260., 350., 440.)], 260., 100.));
        advance(&mut p, 3., 0., false);
        assert_eq!(p.y, 260.);
        assert_eq!(p.support.as_deref(), Some("a"));
    }
    #[test]
    fn landing_follows_moving_resized_window_and_falls_when_closed() {
        let mut p = Physics::new(world(vec![rect("a", 200., 300., 400., 400.)], 400., 100.));
        advance(&mut p, 2., 0., false);
        assert_eq!(p.support.as_deref(), Some("a"));
        p.update(world(vec![rect("a", 300., 250., 500., 450.)], 400., 300.));
        assert_eq!((p.x, p.y), (550., 250.));
        p.update(world(vec![], 550., 250.));
        assert_eq!(p.motion, Motion::Fall);
        advance(&mut p, 3., 0., false);
        assert_eq!(p.y, 700.);
    }
    #[test]
    fn rope_climbs_both_sides_and_disappears_on_landing() {
        for (x, direction, edge) in [(190., 1., 250.), (760., -1., 700.)] {
            let mut p = Physics::new(world(vec![rect("a", 250., 250., 450., 450.)], x, 700.));
            p.direction = direction;
            let mut states = Vec::new();
            for _ in 0..900 {
                p.step(1. / 60., 70., true, false);
                states.push(p.motion);
                if p.motion == Motion::Climb {
                    assert_eq!(p.rope_anchor(), Some((edge, 250.)));
                }
                if p.motion == Motion::Grounded && p.support.is_some() {
                    break;
                }
            }
            for m in [
                Motion::Grab,
                Motion::Climb,
                Motion::Pull,
                Motion::Grounded,
            ] {
                assert!(states.contains(&m), "missing {m:?}");
            }
            assert!(!states.contains(&Motion::Land), "no bracing after a climb");
            assert!(p.climbed);
            assert_eq!(p.y, 250.);
            assert_eq!(p.rope_anchor(), None);
        }
    }
    #[test]
    fn rope_tracks_window_and_detaches_on_drag_close_or_reduce() {
        let mut p = Physics::new(world(vec![rect("a", 250., 250., 450., 450.)], 200., 700.));
        advance(&mut p, 0.2, 70., true);
        assert_eq!(p.motion, Motion::Grab);
        p.update(world(vec![rect("a", 280., 230., 450., 470.)], 200., 700.));
        assert_eq!(p.x, 234.);
        assert_eq!(p.rope_anchor(), Some((280., 230.)));
        p.reset(500., 400.);
        assert_eq!(p.rope_anchor(), None);
        for reduced in [true, false] {
            let mut p = Physics::new(world(vec![rect("a", 250., 250., 450., 450.)], 200., 700.));
            advance(&mut p, 0.5, 70., true);
            if reduced {
                p.step(0.1, 70., true, true);
            } else {
                p.update(world(vec![], 200., 700.));
            }
            assert_eq!(p.rope_anchor(), None);
        }
    }
    #[test]
    fn ascent_only_during_pull_and_independent_of_frame_rate() {
        assert_eq!(climb_distance(0.2, 100.), 0.);
        assert!((climb_distance(0.4, 100.) - 20.).abs() < 1e-8);
        assert_eq!(climb_distance(0.6, 100.), 40.);
        assert_eq!(climb_distance(0.79, 100.), 40.);
        assert_eq!([0., 0.21, 0.41, 0.61].map(climb_frame), [1, 0, 2, 3]);
        let run = |dt| {
            let mut p = Physics::new(world(vec![rect("a", 250., 150., 450., 550.)], 204., 700.));
            assert!(p.start_climb(1.));
            p.entering(Motion::Climb);
            for _ in 0..(2. / dt) as usize {
                p.step(dt, 0., false, false);
            }
            p.y
        };
        assert!((run(1. / 30.) - run(1. / 60.)).abs() < 0.01);
    }
    #[test]
    fn alerts_cancel_preparation_but_allow_airborne_landing() {
        let mut p = Physics::new(world(
            vec![
                rect("a", 100., 500., 300., 200.),
                rect("b", 480., 420., 350., 280.),
            ],
            350.,
            500.,
        ));
        p.plan = p.jump_plan();
        assert!(p.plan.is_some());
        p.entering(Motion::Prepare);
        p.step(0.1, 0., false, false);
        assert_eq!(p.motion, Motion::Grounded);
        p.plan = p.jump_plan();
        p.entering(Motion::Prepare);
        advance(&mut p, 0.3, 0., true);
        assert_eq!(p.motion, Motion::Jump);
        advance(&mut p, 3., 0., false);
        assert_eq!(p.support.as_deref(), Some("b"));
        assert_eq!(p.y, 420.);
    }
    #[test]
    fn monitor_change_and_delayed_frames_cannot_leave_pet_floating() {
        let mut p = Physics::new(world(vec![rect("a", 100., 550., 600., 150.)], 300., 100.));
        for _ in 0..25 {
            p.step(0.1, 0., false, false);
        }
        assert_eq!(p.y, 550.);
        let mut w = world(vec![], 40., 100.);
        w.monitor = "other".into();
        w.size = 180.;
        p.update(w);
        p.step(0.1, 100., true, true);
        assert_eq!(p.y, 700.);
        assert!(p.x >= p.half());
        assert!(p.support.is_none());
    }
    #[test]
    fn high_drop_hurts_low_drop_lands_and_intentional_jump_does_not_hurt() {
        for (y, expected) in [(100., Motion::Hurt), (650., Motion::Land)] {
            let mut p = Physics::new(world(vec![], 400., y));
            for _ in 0..180 {
                if p.motion != Motion::Fall {
                    break;
                }
                p.step(1. / 60., 0., false, false);
            }
            assert_eq!(p.motion, expected);
            assert_eq!(p.y, 700.);
            advance(&mut p, 3., 0., false);
            assert_eq!(p.motion, Motion::Grounded);
        }
        let mut p = Physics::new(world(
            vec![
                rect("source", 100., 350., 300., 350.),
                rect("target", 480., 540., 350., 160.),
            ],
            350.,
            350.,
        ));
        p.plan = p.jump_plan();
        assert!(p.plan.is_some());
        p.entering(Motion::Prepare);
        advance(&mut p, 0.3, 0., true);
        assert_eq!(p.motion, Motion::Jump);
        for _ in 0..180 {
            if p.motion != Motion::Jump {
                break;
            }
            p.step(1. / 60., 0., false, false);
        }
        assert_eq!(p.motion, Motion::Land);
        assert_eq!(p.support.as_deref(), Some("target"));
    }
    #[test]
    fn narrow_exposed_inactive_window_accepts_rope_and_small_front_window_accepts_drop() {
        let windows = vec![
            rect("front", 316., 100., 400., 400.),
            rect("inactive", 300., 250., 350., 150.),
        ];
        let mut p = Physics::new(world(windows.clone(), 100., 700.));
        assert_eq!(p.edge_landing(&windows[1], 1.), Some(309.));
        assert!(p.rope_target("inactive", 1.).is_some());
        p.approach = Some(("inactive".into(), 1.));
        for _ in 0..1600 {
            p.step(1. / 60., 80., true, false);
            if p.climbed {
                break;
            }
        }
        assert_eq!(p.support.as_deref(), Some("inactive"));
        assert_eq!((p.x, p.y), (309., 250.));
        assert_eq!(p.motion, Motion::Grounded);
        // Changing focus/Z order cannot detach an otherwise exposed support.
        p.update(world(windows.into_iter().rev().collect(), p.x, p.y));
        assert_eq!(p.support.as_deref(), Some("inactive"));
        assert_eq!(p.motion, Motion::Grounded);
        let mut p = Physics::new(world(
            vec![
                rect("small-front", 350., 200., 40., 100.),
                rect("back", 100., 360., 700., 340.),
            ],
            370.,
            100.,
        ));
        advance(&mut p, 2., 0., false);
        assert_eq!(p.support.as_deref(), Some("small-front"));
        assert_eq!(p.y, 200.);
    }
    #[test]
    fn narrow_jump_destination_uses_available_foot_clearance() {
        let mut p = Physics::new(world(
            vec![
                rect("source", 100., 500., 300., 200.),
                rect("narrow", 480., 420., 16., 280.),
            ],
            350.,
            500.,
        ));
        let plan = p.jump_plan().expect("narrow but supported landing");
        assert_eq!(plan.x, 488.);
        p.plan = Some(plan);
        p.entering(Motion::Prepare);
        advance(&mut p, 0.3, 0., true);
        advance(&mut p, 2., 0., false);
        assert_eq!(p.support.as_deref(), Some("narrow"));
        assert_eq!(p.y, 420.);
    }
    #[test]
    fn sixty_and_one_twenty_hz_keep_walking_falling_and_climbing_in_sync() {
        let run = |hz: usize, mode: Motion| {
            let mut p = Physics::new(world(
                vec![rect("wall", 700., 150., 200., 550.)],
                200.,
                700.,
            ));
            match mode {
                Motion::Fall => p.reset(200., 100.),
                Motion::Climb => {
                    p.x = 654.;
                    assert!(p.start_climb(1.));
                    p.entering(Motion::Climb);
                }
                _ => {}
            }
            for _ in 0..hz / 2 {
                p.step(1. / hz as f64, 80., mode == Motion::Grounded, false);
            }
            (p.x, p.y)
        };
        for mode in [Motion::Grounded, Motion::Fall, Motion::Climb] {
            let a = run(60, mode);
            let b = run(120, mode);
            assert!((a.0 - b.0).abs() < 0.01);
            assert!((a.1 - b.1).abs() < 0.01);
        }
    }
    #[test]
    fn restless_perch_ropes_down_its_own_window_and_lands_softly() {
        for (x, side, edge) in [(300., 1., 250.), (650., -1., 700.)] {
            let mut p = Physics::new(world(vec![rect("a", 250., 150., 450., 550.)], x, 150.));
            advance(&mut p, 1., 0., false);
            assert_eq!(p.support.as_deref(), Some("a"));
            assert_eq!(p.descent_urge(), 0., "a fresh perch is not restless");
            p.perch = 200.;
            assert!(p.descent_urge() > 0.85);
            p.cooldown = 0.;
            let mut states = Vec::new();
            for _ in 0..3600 {
                p.step(1. / 60., 80., true, false);
                states.push(p.motion);
                if p.motion == Motion::Descend {
                    assert_eq!(p.rope_anchor(), Some((edge, 150.)));
                    assert_eq!(p.side, side);
                    assert_eq!(p.x, edge - side * p.half());
                }
                if p.climbed {
                    break;
                }
            }
            for m in [Motion::Lower, Motion::Descend, Motion::Grounded] {
                assert!(states.contains(&m), "missing {m:?}");
            }
            assert!(!states.contains(&Motion::Land), "stepping off the rope is not a landing");
            assert!(!states.contains(&Motion::Hurt));
            assert!(!states.contains(&Motion::Fall));
            assert!(std::mem::take(&mut p.climbed));
            assert_eq!(p.y, 700.);
            assert!(p.support.is_none());
            assert_eq!(p.rope_anchor(), None);
            advance(&mut p, 1., 0., true);
            assert_eq!(p.motion, Motion::Grounded);
            assert_eq!(p.perch, 0., "standing on the floor resets restlessness");
        }
    }
    #[test]
    fn short_drop_hops_from_the_corner_instead_of_roping() {
        let mut p = Physics::new(world(vec![rect("a", 300., 560., 300., 140.)], 560., 560.));
        advance(&mut p, 1., 0., false);
        p.perch = 200.;
        p.cooldown = 0.;
        let mut states = Vec::new();
        for _ in 0..3600 {
            p.step(1. / 60., 80., true, false);
            states.push(p.motion);
            if matches!(p.motion, Motion::Land | Motion::Hurt) {
                break;
            }
        }
        assert!(states.contains(&Motion::Prepare));
        assert!(states.contains(&Motion::Jump));
        assert!(!states.contains(&Motion::Lower));
        assert_eq!(p.motion, Motion::Land, "a deliberate hop never hurts");
        assert_eq!(p.y, 700.);
        assert!(p.support.is_none());
        assert!(p.x > 600., "lands beside the window, not back on it");
    }
    #[test]
    fn descent_needs_an_exposed_corner_a_clear_wall_and_a_real_drop() {
        let mut p = Physics::new(
            world(vec![rect("a", 250., 150., 450., 550.)], 400., 150.),
        );
        advance(&mut p, 1., 0., false);
        assert!(p.descent_target(1.).is_some());
        assert!(p.descent_target(-1.).is_some());
        // A narrow window in front covering the left wall (but offering no ledge
        // under the rope) blocks that side only.
        p.update(world(
            vec![
                rect("front", 60., 250., 120., 300.),
                rect("a", 250., 150., 450., 550.),
            ],
            400.,
            150.,
        ));
        assert!(p.descent_target(1.).is_none());
        assert!(p.descent_target(-1.).is_some());
        // A wider front window exposes its own top under the rope: a shorter descent.
        p.update(world(
            vec![
                rect("front", 100., 300., 250., 300.),
                rect("a", 250., 150., 450., 550.),
            ],
            400.,
            150.,
        ));
        let (_, hang) = (p.descent_target(1.).map(|t| t.1), p.descent_target(1.).map(|t| t.2));
        assert_eq!(p.floor_below(hang.unwrap(), 151.).1, 300.);
        // A window in front covering the corner itself removes the standing spot.
        p.update(world(
            vec![
                rect("corner", 600., 100., 200., 200.),
                rect("a", 250., 150., 450., 550.),
            ],
            400.,
            150.,
        ));
        assert!(p.descent_target(-1.).is_none());
        // Too short a drop is a hop, never a rope; the floor has no descent at all.
        let mut p = Physics::new(world(vec![rect("low", 250., 650., 450., 50.)], 400., 650.));
        advance(&mut p, 1., 0., false);
        assert!(p.descent_target(1.).is_none());
        let mut p = Physics::new(world(vec![], 400., 700.));
        advance(&mut p, 1., 0., false);
        assert_eq!(p.descent_urge(), 0.);
        p.perch = 500.;
        assert_eq!(p.descent_urge(), 0.);
        assert!(p.descent_target(1.).is_none());
    }
    #[test]
    fn descent_follows_a_moving_window_and_falls_when_it_closes() {
        let mut p = Physics::new(world(vec![rect("a", 250., 150., 450., 550.)], 300., 150.));
        advance(&mut p, 1., 0., false);
        p.perch = 200.;
        p.cooldown = 0.;
        for _ in 0..3600 {
            p.step(1. / 60., 80., true, false);
            if p.motion == Motion::Descend && p.age > 0.5 {
                break;
            }
        }
        assert_eq!(p.motion, Motion::Descend);
        let (x, y) = (p.x, p.y);
        p.update(world(vec![rect("a", 280., 170., 450., 530.)], x, y));
        assert_eq!(p.motion, Motion::Descend);
        assert_eq!((p.x, p.y), (x + 30., y + 20.));
        assert_eq!(p.rope_anchor(), Some((280., 170.)));
        // Alerts cannot interrupt a rope descent; it still completes on its own.
        advance(&mut p, 1., 0., false);
        assert!(matches!(p.motion, Motion::Descend | Motion::Grounded));
        p.update(world(vec![], p.x, p.y));
        assert_eq!(p.motion, Motion::Fall);
        assert_eq!(p.rope_anchor(), None);
        // Reduced motion resolves a descent straight onto the floor.
        let mut p = Physics::new(world(vec![rect("a", 250., 150., 450., 550.)], 300., 150.));
        advance(&mut p, 1., 0., false);
        p.perch = 200.;
        p.cooldown = 0.;
        for _ in 0..3600 {
            p.step(1. / 60., 80., true, false);
            if p.motion == Motion::Descend {
                break;
            }
        }
        assert_eq!(p.motion, Motion::Descend);
        p.step(0.1, 80., true, true);
        assert_eq!(p.motion, Motion::Grounded);
        assert_eq!(p.y, 700.);
    }
    #[test]
    fn settled_perch_still_prefers_ropes_up_and_restlessness_accumulates() {
        let mut p = Physics::new(world(
            vec![
                rect("low", 100., 500., 300., 200.),
                rect("high", 500., 200., 300., 500.),
            ],
            250.,
            500.,
        ));
        advance(&mut p, 1., 0., false);
        assert_eq!(p.support.as_deref(), Some("low"));
        let perch = p.perch;
        advance(&mut p, 10., 0., false);
        assert!((p.perch - perch - 10.).abs() < 1e-6);
        assert_eq!(p.descent_urge(), 0., "urge stays zero for the first 25 seconds");
        p.perch = 120.;
        let urge = p.descent_urge();
        assert!(urge > 0.6 && urge < 0.7, "{urge}");
        p.y = 200.;
        assert!(p.descent_urge() > urge, "higher perches are more restless");
    }
    #[test]
    fn window_nudges_ride_brush_and_lost_are_reported_once_per_update() {
        let mut p = Physics::new(world(vec![rect("a", 200., 300., 400., 400.)], 400., 300.));
        advance(&mut p, 2., 0., false);
        assert_eq!(p.support.as_deref(), Some("a"));
        // Small jitter is not a ride; a drag step is; resizing one edge is not.
        assert_eq!(p.update(world(vec![rect("a", 205., 300., 400., 400.)], p.x, p.y)), Nudge::None);
        assert_eq!(p.update(world(vec![rect("a", 240., 290., 400., 400.)], p.x, p.y)), Nudge::Ride);
        assert_eq!(p.update(world(vec![rect("a", 200., 290., 440., 400.)], p.x, p.y)), Nudge::None);
        assert_eq!(p.update(world(vec![rect("a", 240., 290., 440., 400.)], p.x, p.y)), Nudge::Ride);
        assert_eq!(p.support.as_deref(), Some("a"));
        assert!(!p.lost_support);
        // Another window dragged into our space brushes us; far away it does not.
        let a = rect("a", 240., 290., 400., 400.);
        let w = p.update(world(vec![rect("b", 900., 100., 80., 80.), a.clone()], p.x, p.y));
        assert_eq!(w, Nudge::None, "appearing is not a brush");
        let w = p.update(world(vec![rect("b", 880., 120., 80., 80.), a.clone()], p.x, p.y));
        assert_eq!(w, Nudge::None, "moving far away is not a brush");
        let w = p.update(world(vec![rect("b", p.x + p.half() + 20., 150., 80., 80.), a.clone()], p.x, p.y));
        assert_eq!(w, Nudge::Brush);
        // The same movement by a window behind our support is invisible to us.
        let w = p.update(world(vec![a.clone(), rect("b", 880., 120., 80., 80.)], p.x, p.y));
        assert_eq!(w, Nudge::None);
        let w = p.update(world(vec![a.clone(), rect("b", p.x + p.half() + 20., 150., 80., 80.)], p.x, p.y));
        assert_eq!(w, Nudge::None);
        // Being covered by a front window drops us quietly: no sulking for alt-tab.
        let cover = rect("front", a.x, a.y - 60., a.width, 200.);
        assert_eq!(p.update(world(vec![cover, a.clone()], p.x, p.y)), Nudge::None);
        assert_eq!(p.motion, Motion::Fall);
        assert!(!p.lost_support);
        p.update(world(vec![a.clone()], p.x, p.y));
        p.reset(p.x, 290.);
        assert_eq!(p.support.as_deref(), Some("a"));
        // Closing the support is a loss, remembered until the caller clears it.
        assert_eq!(p.update(world(vec![], p.x, p.y)), Nudge::None);
        assert!(p.lost_support);
        assert_eq!(p.motion, Motion::Fall);
        advance(&mut p, 5., 0., false);
        assert_eq!(p.motion, Motion::Grounded);
        assert!(p.lost_support, "landing keeps the cause for the reaction");
        p.lost_support = false;
        // A deliberate reset (drag/drop) never reports a loss, and climbing is not a ride.
        p.reset(400., 700.);
        assert!(!p.lost_support);
        // Reduced motion snaps straight to the ground: no landing, so no stale loss.
        let mut q = Physics::new(world(vec![rect("a", 200., 300., 400., 400.)], 400., 300.));
        advance(&mut q, 2., 0., false);
        q.update(world(vec![], q.x, q.y));
        assert!(q.lost_support);
        q.step(0.1, 0., true, true);
        assert_eq!(q.motion, Motion::Grounded);
        assert!(!q.lost_support);
        let mut c = Physics::new(world(vec![rect("a", 250., 250., 450., 450.)], 200., 700.));
        advance(&mut c, 0.5, 70., true);
        assert!(matches!(c.motion, Motion::Grab | Motion::Climb));
        assert_eq!(c.update(world(vec![rect("a", 300., 250., 450., 450.)], c.x, c.y)), Nudge::None);
    }
    #[test]
    fn edge_departure_is_available_when_walking_bumps_the_display_edge() {
        let mut p = Physics::new(world(vec![], 120., 700.));
        advance(&mut p, 1., 0., false);
        p.direction = -1.;
        assert_eq!(p.edge_departure(), None, "not at the edge yet");
        let mut seen = false;
        for _ in 0..240 {
            // Autonomous walking; the post-landing cooldown rules out jump plans.
            p.step(1. / 60., 150., true, false);
            if p.edge_departure() == Some(-1.) {
                seen = true;
                assert!(p.x <= p.half() + 2.);
                break;
            }
        }
        assert!(seen, "reaching the floor edge must offer a departure");
        // Keep walking into the edge: the bump wobbles but still offers the departure.
        for _ in 0..30 {
            p.step(1. / 60., 150., true, false);
            if p.motion == Motion::Wobble {
                break;
            }
        }
        assert_eq!(p.motion, Motion::Wobble);
        assert_eq!(p.edge_departure(), Some(-1.));
        // The wobble then turns the character away; no departure in that direction.
        advance(&mut p, 1., 0., true);
        assert_eq!(p.motion, Motion::Grounded);
        assert_eq!(p.direction, 1.);
        assert_eq!(p.edge_departure(), None);
        // Standing still at the edge while facing it also counts.
        p.direction = -1.;
        assert_eq!(p.edge_departure(), Some(-1.));
        p.x = 500.;
        assert_eq!(p.edge_departure(), None);
    }
}
#[cfg(test)]
mod field_layout {
    use super::*;
    use crate::surfaces::{WindowRect, World};
    /// A real desktop snapshot: a big front window whose left corner ropes down
    /// onto a small window that has no way down of its own (left corner is off
    /// screen, right wall is behind the big window). The character used to rope
    /// down and immediately re-climb the big window, forever.
    #[test]
    fn restless_character_escapes_a_window_pocket_to_the_floor() {
        let spec = "289,194,2321,1522 728,286,2110,1580 1689,573,1693,1278 245,368,1427,904 1330,646,1716,1394 17,439,559,966 1998,177,1567,1023 423,950,1189,904 473,342,1919,1205 458,561,1716,1394 59,19,1665,1450 956,57,1716,1394 927,28,1716,1394 1986,404,1854,1112 1122,0,1496,1200 831,613,1716,1394 210,755,1496,1222";
        let windows: Vec<WindowRect> = spec
            .split(' ')
            .enumerate()
            .map(|(i, s)| {
                let v: Vec<f64> = s.split(',').map(|n| n.parse().unwrap()).collect();
                WindowRect { id: format!("w{i}"), x: v[0], y: v[1], width: v[2], height: v[3] }
            })
            .collect();
        let world = World { monitor: "m".into(), width: 3840., height: 2040., size: 204., x: 1500., y: 194., windows };
        let mut p = Physics::new(world);
        advance(&mut p, 2., 0., false);
        assert_eq!(p.support.as_deref(), Some("w0"));
        p.perch = 200.;
        p.cooldown = 0.;
        let mut climbs_after_descent = 0;
        let mut descended = false;
        let mut floor_at = None;
        for i in 0..(600 * 60) {
            p.step(1. / 60., 65., true, false);
            if p.motion == Motion::Descend {
                descended = true;
            }
            if descended && p.motion == Motion::Grab {
                climbs_after_descent += 1;
            }
            if p.on_floor() {
                floor_at = Some(i / 60);
                break;
            }
        }
        assert!(descended, "ropes down off the big window first");
        assert_eq!(climbs_after_descent, 0, "never re-climbs the window it just left");
        let t = floor_at.expect("reaches the floor within ten minutes");
        assert!(t < 300, "took {t}s");
        assert_ne!(p.motion, Motion::Hurt);
    }
    fn advance(p: &mut Physics, seconds: f64, walk: f64, auto: bool) {
        for _ in 0..(seconds * 60.).ceil() as usize {
            p.step(1. / 60., walk, auto, false);
        }
    }
}
