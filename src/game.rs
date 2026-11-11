// Game logic rebuilt instruction-for-instruction from Icy Tower 1.5's exe (main.c play(),
// handle_player_input(), handle_player_collision_vector(), player.c, map.c), so that the
// original's .itr replays play back exactly. Coordinates are the original's 640x480 screen space;
// `off` (map.offset) is how far the tower has scrolled. One tick = 20ms (the original's 50Hz).
// Floating point goes through F80 where the original keeps values in x87 registers.
use crate::f80::{f, F80};

pub const TICK: f64 = 0.02;

const MAX_SPEED: f64 = 12.2; // max_speed[collision_type 2], the mode new_game() selects
const GRAVITY_MODS: [f64; 3] = [-0.2, 0.0, 0.2];
const FLOOR_SIZE_MODS: [i32; 5] = [2, 0, -2, -4, -6];
const START_SPEEDS: [i32; 6] = [5, 4, 3, 2, 1, 0];
const SPEEDUPS: [i32; 9] = [1500, 3000, 4500, 6000, 7500, 9000, 10500, 1800000, 9000000];

// control flags, as stored in .itr replays
pub const LEFT: u8 = 1;
pub const RIGHT: u8 = 2;
pub const JUMP: u8 = 0x10;
pub const END: u8 = 0x80;

/// The original's game options, stored in every replay. Classic mode is Rules::default().
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rules {
    pub floor_shrink: bool,
    pub floor_size: usize, // Wide, Normal, Shorter, Shortest, Tiny
    pub start_speed: usize, // index into start_speeds: 5 = Normal .. 0 = Insane
    pub speed_increase: bool,
    pub gravity: usize, // Helium, Normal, Heavy
    pub rejump: bool, // hold jump to keep jumping
    pub style_base: usize, // "Start floor" graphics (not part of the game)
    pub eye_candy: bool,
}

impl Default for Rules {
    fn default() -> Self {
        Rules { floor_shrink: true, floor_size: 1, start_speed: 5, speed_increase: true, gravity: 1, rejump: true, style_base: 0, eye_candy: true }
    }
}

impl Rules {
    /// is_custom_replay(): anything but the classic settings.
    pub fn is_custom(&self) -> bool {
        !(self.floor_shrink && self.floor_size == 1 && self.start_speed == 5 && self.speed_increase && self.gravity == 1)
    }
}

// Player status values used by the original
pub const STANDING: i32 = 0;
pub const RISING: i32 = 1;
const FALLING: i32 = 2;
const IN_AIR: i32 = 3; // walked off or passed a floor: lands on the next one

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Sfx {
    JumpLow,
    JumpMid,
    JumpHigh,
    Edge,
    Death,
    Hurry,
    Reward(usize),
    Milestone, // every 50 floors (S_AIGHT)
    Splat,     // hitting the bottom after a fall
}

#[derive(Clone, Copy, Debug)]
pub struct Floor {
    pub start: i32, // tile column (16px)
    pub end: i32,
}

#[derive(Clone, Copy, Default)]
struct Row {
    empty: bool,
    start: i32,
    end: i32,
    level: i32, // map row counter; floors are every 5th row
    style: usize,
}

/// particle.c Tparticle; positions and speeds are Allegro 16.16 fixed point.
pub struct Star {
    pub x: i32,
    pub y: i32,
    sx: i32,
    sy: i32,
    life: i32, // "intensity", 255 ticks
    pub kind: usize, // STAR01..08, re-rolled as it twinkles
}

impl Star {
    /// fixtoi(): round to nearest
    pub fn pos(&self) -> (i32, i32) {
        let r = |v: i32| (v >> 16) + ((v & 0x8000) >> 15);
        (r(self.x), r(self.y))
    }

    /// Position `lag` ticks back along this tick's motion, for drawing between ticks.
    pub fn pos_at(&self, lag: f32) -> (f32, f32) {
        let at = |v: i32, s: i32| (v as f32 - s as f32 * lag) / 65536.0;
        (at(self.x, self.sx), at(self.y, self.sy - 0x4ccd)) // sy already has this tick's gravity added
    }
}

const MAX_STARS: usize = 512;

pub struct Player {
    pub x: f64,
    pub y: f64,
    pub sx: f64,
    pub sy: f64,
    pub status: i32,
    jump_key: i32,
    pub frame: i32,
    pub level: i32, // floor last stood on
    pub score: i32, // combo points
    pub best_combo: i32,
    pub in_combo: i32, // combo timer, 100 ticks
    pub acc_level: i32, // floors gained in this combo
    pub acc_jumps: i32,
    pub dead: i32,
    pub rotate: bool,
    pub angle: f64, // 0..256 per turn, like Allegro's fixed angles
    pub edge: i32, // 0 both feet on, 1 only left foot, 2 only right foot
    edge_drawn: i32,
    pub shake: i32, // screen shake ticks after the splat
    pub latest_combo: i32, // shown in the combo bubble while the reward is up
    // per-game stats the original keeps for profiles and replay headers
    pub ccc: [i32; 5], // floor reached at each clock speed-up ("Clock Challenge")
    pub jc_top: [i32; 5], // longest chain of n-floor jumps
    jc: [i32; 5],
    pub no_combo_top: i32,
    pub lost_combo: i32,
    pub jumps: i32,
    pub combos: i32,
    pub combo_floors: i32,
    pub rewards: [i32; 10],
    // drawing only
    pub facing_left: bool,
    pub walk_phase: f64,
}

pub struct Game {
    pub rules: Rules,
    pub p: Player,
    pub off: i32,
    rows: [Row; 32],
    holdrand: u32, // msvcrt rand() state; only add_floor() draws from it
    pub speed: i32, // -1 until the tower starts moving
    speedup_idx: usize,
    fall_count: i32,
    pub clock: i32, // clock_angle
    pub tick: i64,
    pub hurry_y: i32,
    pub reward_time: i32,
    pub reward_scale: f64,
    pub reward_idx: usize,
    last_diff: i32,
    combo_start: i32,
    milestone: i32,
    splat_count: i32, // ticks since death until the splat
    pub shake_y: i32,
    pub stars: Vec<Star>,
    pub bg_ids: [usize; 5],
    last_stripe: i32,
    vis: u32, // eye candy randomness, kept apart from the game's
    pub paused: bool,
}

/// line_intersect(): segment (x1,y1)-(x2,y2) against (x3,y3)-(x4,y4), x87 maths included.
fn line_intersect(x1: i32, y1: i32, x2: i32, y2: i32, x3: i32, y3: i32, x4: i32, y4: i32) -> Option<(i32, i32)> {
    let (dx, dy) = (x2.wrapping_sub(x1), y2.wrapping_sub(y1));
    let d = (x3 - x4).wrapping_mul(dy).wrapping_add((y4 - y3).wrapping_mul(dx));
    let n1 = (y3 - y4).wrapping_mul(x1 - x3).wrapping_add((x4 - x3).wrapping_mul(y1 - y3));
    let n2 = (y1 - y3).wrapping_mul(dx).wrapping_add((y1 - y2).wrapping_mul(x1 - x3));
    if d == 0 {
        // n/0 is +-inf (rejected) or NaN, which passes every test and converts to 0x80000000
        return (n1 == 0 && n2 == 0).then(|| (x1.wrapping_add(i32::MIN), y1.wrapping_add(i32::MIN)));
    }
    let (fd, one) = (F80::from_int(d as i64), F80::from_int(1));
    let t1 = F80::from_int(n1 as i64).div(fd);
    let t2 = F80::from_int(n2 as i64).div(fd);
    if F80::ZERO.gt(t1) || F80::ZERO.gt(t2) || t1.gt(one) || t2.gt(one) {
        return None;
    }
    let half = f(0.5);
    let ox = t1.mul(F80::from_int(dx as i64)).add(half).trunc() as i32;
    let oy = t1.mul(F80::from_int(dy as i64)).add(half).trunc() as i32;
    Some((ox.wrapping_add(x1), oy.wrapping_add(y1)))
}

impl Game {
    pub fn new(seed: u32, rules: Rules) -> Self {
        let mut g = Game {
            rules,
            p: Player {
                x: 200.0,
                y: 431.0,
                sx: 0.001,
                sy: 0.0,
                status: STANDING,
                jump_key: 1,
                frame: 0,
                level: 0,
                score: 0,
                best_combo: 0,
                in_combo: 0,
                acc_level: 0,
                acc_jumps: 0,
                dead: 0,
                rotate: false,
                angle: 0.0,
                edge: 0,
                edge_drawn: 0,
                shake: 0,
                latest_combo: 0,
                ccc: [0; 5],
                jc_top: [0; 5],
                jc: [0; 5],
                no_combo_top: 0,
                lost_combo: 0,
                jumps: 0,
                combos: 0,
                combo_floors: 0,
                rewards: [0; 10],
                facing_left: false,
                walk_phase: 0.0,
            },
            off: 0,
            rows: [Row { empty: true, ..Row::default() }; 32],
            holdrand: seed,
            speed: -1,
            speedup_idx: 0,
            fall_count: 0,
            clock: 0,
            tick: 0,
            hurry_y: 480,
            reward_time: 0,
            reward_scale: 0.0,
            reward_idx: 0,
            last_diff: 0,
            combo_start: 0,
            milestone: 50,
            splat_count: 0,
            shake_y: 0,
            stars: Vec::new(),
            bg_ids: [0; 5],
            last_stripe: 0,
            vis: seed | 1,
            paused: false,
        };
        for _ in 0..30 {
            g.add_floor();
        }
        g
    }

    /// msvcrt rand()
    fn rand(&mut self) -> i32 {
        self.holdrand = self.holdrand.wrapping_mul(214013).wrapping_add(2531011);
        ((self.holdrand >> 16) & 0x7fff) as i32
    }

    fn vis_rand(&mut self) -> i32 {
        self.vis ^= self.vis << 13;
        self.vis ^= self.vis >> 17;
        self.vis ^= self.vis << 5;
        (self.vis >> 1) as i32
    }

    pub fn final_score(&self) -> i32 {
        self.p.level * 10 + self.p.score
    }

    /// Floors currently in the map window: (floor number, extent, screen y of the surface,
    /// first of the three FLOORxx pieces, SIGNxx index). draw_frame() offsets the style by the
    /// "start floor" option and caps it at FLOOR22-24; past row 4999 (floor 1000) come FLOOR25-27.
    pub fn visible_floors(&self) -> Vec<(i32, Floor, i32, usize, usize)> {
        (0..32)
            .filter(|&i| !self.rows[i].empty)
            .map(|i| {
                let r = &self.rows[i];
                let y = (29 - i as i32) * 16 + self.off % 16;
                let top = (r.level > 4999) as usize;
                let style = r.style + self.rules.style_base;
                ((r.level - 1) / 5, Floor { start: r.start, end: r.end }, y, (style * 3).min(27) + top * 3, style.min(9) + top)
            })
            .collect()
    }

    // map.c add_floor(): scroll the 32-row window and generate the new top row
    fn add_floor(&mut self) {
        self.rows.copy_within(1..32, 0);
        let lvl = self.rows[31].level;
        let mut row = Row { empty: true, start: 0, end: 0, level: lvl + 1, style: if lvl > 4999 { 10 } else { (lvl / 500) as usize } }; // new look every 100 floors
        if (lvl % 250 == 0 && lvl <= 5004) || lvl % 2500 == 0 {
            row = Row { empty: false, start: 0, end: 40, ..row };
        } else if lvl % 5 == 0 {
            let level = lvl + 1;
            let size = if self.rules.floor_shrink {
                if level <= 2999 {
                    let r = self.rand();
                    // (300 - level/5) / 300.0f * 10.0f, in extended precision
                    let room = F80::from_int((level / -5 + 300) as i64).div(F80::from_int(300)).mul(F80::from_int(10));
                    if F80::from_int(1).gt(room) { 6 } else { r % room.trunc() as i32 + 6 }
                } else if level <= 5004 {
                    6
                } else if level <= 7504 {
                    5
                } else if level <= 10004 {
                    4
                } else if level < 50005 {
                    3
                } else {
                    2
                }
            } else {
                self.rand() % 10 + 6
            };
            let size = size + FLOOR_SIZE_MODS[self.rules.floor_size.min(4)];
            let (range, w) = if size > 0 { (30 - size, size) } else { (29, 1) };
            let start = self.rand() % range + 5;
            row = Row { empty: false, start, end: start + w, ..row };
        }
        self.rows[31] = row;
    }

    /// map.c getFloorData(): the floor segment in the 16px row containing y.
    fn floor_data(&self, y: i32) -> Option<(i32, i32, i32)> {
        let r = (y + 1) >> 4;
        let row = 29 - r;
        if !(0..32).contains(&row) || self.rows[row as usize].empty {
            return None;
        }
        let rw = &self.rows[row as usize];
        Some((r * 16 + self.off % 16, rw.start * 16 - 2, rw.end * 16 + 17))
    }

    fn get_level(&self, y: i32) -> i32 {
        let row = 29 - ((y + 1) >> 4);
        if (0..32).contains(&row) { self.rows[row as usize].level } else { 0 }
    }

    // player.c jump_player()
    fn jump(&mut self, ev: &mut Vec<Sfx>) -> bool {
        let p = &mut self.p;
        if p.status != STANDING {
            return false;
        }
        p.status = RISING;
        let v = -2.0 * p.sx.abs();
        p.sy = if v < -MAX_SPEED { v } else { -MAX_SPEED };
        if p.sy < -22.0 {
            p.rotate = true;
        }
        p.angle = 0.0;
        p.jumps += 1;
        ev.push(if p.sy < -22.0 { Sfx::JumpHigh } else if p.sy < -15.0 { Sfx::JumpMid } else { Sfx::JumpLow });
        true
    }

    fn start_reward(&mut self, floors: i32, ev: &mut Vec<Sfx>) {
        let idx = match floors {
            ..=6 => 0,
            7..=14 => 1,
            15..=24 => 2,
            25..=34 => 3,
            35..=49 => 4,
            50..=69 => 5,
            70..=99 => 6,
            100..=139 => 7,
            140..=199 => 8,
            _ => 9,
        };
        self.reward_time = 80;
        self.reward_scale = 0.0;
        self.reward_idx = idx;
        self.p.rewards[idx] += 1;
        ev.push(Sfx::Reward(idx));
    }

    /// particle.c create_particle(); returns the slot, 0 when the pool is full (like the original)
    fn add_star(&mut self, x: i32, y: i32) -> usize {
        if self.stars.len() >= MAX_STARS {
            return 0;
        }
        let sx = ((self.vis_rand() % 50 - 25) << 16) / 10;
        let sy = ((self.vis_rand() % 50 - 25) << 16) / 50;
        let kind = (self.vis_rand() % 8) as usize;
        self.stars.push(Star { x: x << 16, y: y << 16, sx, sy, life: 255, kind });
        self.stars.len() - 1
    }

    /// particle.c update_particle() for every live star
    fn update_stars(&mut self) {
        for i in 0..self.stars.len() {
            let twinkle = self.vis_rand() % 5 == 1;
            let kind = if twinkle { Some((self.vis_rand() % 8) as usize) } else { None };
            let s = &mut self.stars[i];
            s.x += s.sx;
            s.y += s.sy;
            s.sy += 0x4ccd; // 0.3px/tick gravity
            s.life -= 1;
            if let Some(k) = kind {
                s.kind = k;
            }
        }
        self.stars.retain(|s| s.life > 0);
    }

    // handle_player_input()
    fn input(&mut self, flags: u8, ev: &mut Vec<Sfx>) {
        let p = &mut self.p;
        if flags & LEFT != 0 {
            let mut s = f(p.sx);
            if p.sx > 0.0 {
                s = s.mul(f(0.7));
                p.sx = s.to_f64();
            }
            p.sx = s.sub(f(0.3)).to_f64();
        } else if flags & RIGHT != 0 {
            let mut s = f(p.sx);
            if 0.0 > p.sx {
                s = s.mul(f(0.7));
                p.sx = s.to_f64();
            }
            p.sx = s.add(f(0.3)).to_f64();
        } else {
            p.sx = f(p.sx).mul(f(0.9)).to_f64();
        }
        let fire = flags & JUMP != 0;
        if self.rules.rejump {
            if fire {
                self.jump(ev);
            }
        } else {
            if fire && self.p.jump_key == 0 && self.jump(ev) {
                self.p.jump_key = -1;
            }
            if !fire {
                self.p.jump_key = 0;
            }
        }
    }

    // player.c update_player()
    fn update_player(&mut self) {
        let p = &mut self.p;
        p.sy = p.sy.max(-100.0).min(MAX_SPEED);
        p.sx = p.sx.max(-MAX_SPEED).min(MAX_SPEED);
        let x = f(p.x).add(f(p.sx));
        p.x = x.to_f64();
        let y = f(p.y).add(f(p.sy));
        p.y = y.to_f64();
        if y.gt(f(1000.0)) {
            p.y = 1000.0;
        }
        if x.gt(f(555.0)) {
            p.x = 555.0;
            p.sx = f(-0.9).mul(f(p.sx)).to_f64();
        } else if f(85.0).gt(x) {
            p.x = 85.0;
            p.sx = f(-0.9).mul(f(p.sx)).to_f64();
        }
        if p.status != STANDING {
            let sy = f(0.8).add(f(GRAVITY_MODS[self.rules.gravity.min(2)])).add(f(p.sy));
            p.sy = sy.to_f64();
            if p.status == RISING && sy.gt(F80::ZERO) {
                p.status = FALLING;
            }
        }
        if p.sx.abs() > 0.2 {
            p.facing_left = p.sx < 0.0;
        }
        p.walk_phase += 0.05 + p.sx.abs() * 0.02;
    }

    // handle_player_collision_vector(): sweep both feet from last tick's position
    fn collide(&mut self, old_x: i32, old_y: i32) {
        let yi = self.p.y as i32;
        let Some((fy, x1, x2)) = self.floor_data(yi).or_else(|| self.floor_data(old_y)) else {
            if self.p.status == FALLING || self.p.status == STANDING {
                self.p.status = IN_AIR;
            }
            return;
        };
        let xi = self.p.x as i32;
        let a = line_intersect(x1, fy, x2, fy, xi - 11, yi + 1, old_x - 11, old_y);
        let b = line_intersect(x1, fy, x2, fy, xi + 11, yi + 1, old_x + 11, old_y);
        let p = &mut self.p;
        if a.is_none() && b.is_none() && (p.status == FALLING || p.status == STANDING) {
            p.status = IN_AIR;
        }
        p.edge = match (a.is_some(), b.is_some()) {
            (true, true) | (false, false) => 0,
            (true, false) => 1,
            (false, true) => 2,
        };
        if (a.is_none() && b.is_none()) || !(p.status == FALLING || p.status == IN_AIR) {
            return;
        }
        let x = match (a, b) {
            (Some((ax, _)), Some((bx, _))) => {
                if !(ax >= -10000 && bx >= -10000 && ax <= 10000 && bx <= 10000) {
                    return;
                }
                ax + 11
            }
            (Some((ax, _)), None) => ax + 11,
            (None, Some((bx, _))) => bx - 11,
            (None, None) => unreachable!(),
        };
        p.status = STANDING;
        p.sy = 0.0;
        p.y = (fy - 1) as f64;
        p.x = x as f64;
        p.rotate = false;
    }

    /// One 20ms step with the original's control flags (LEFT | RIGHT | JUMP).
    pub fn tick_flags(&mut self, flags: u8) -> Vec<Sfx> {
        let mut ev = Vec::new();
        if self.paused {
            return ev;
        }
        self.tick += 1;
        self.fall_count += 1;
        let old_x = self.p.x as i32;
        let mut old_y = self.p.y as i32;
        self.input(flags, &mut ev);
        self.update_player();

        // spinning through a combo leaves a trail of stars
        if self.p.rotate && self.p.in_combo != 0 && self.rules.eye_candy {
            let (x, y) = (self.p.x as i32, self.p.y as i32 - 16);
            self.add_star(x, y);
        }
        self.update_stars();

        // camera follows the player above y=160, faster the higher they are
        let old_off = self.off;
        let mut scrolled = 0;
        if 160.0 > self.p.y {
            let y = self.p.y;
            let mut k = if 140.0 > y { 2 } else { 1 };
            for (lim, add) in [(120.0, 1), (100.0, 1), (80.0, 1), (60.0, 1), (40.0, 2), (20.0, 2), (0.0, 3)] {
                if lim > y {
                    k += add;
                }
            }
            self.off += k;
            self.p.y = f(self.p.y).add(F80::from_int(k as i64)).to_f64();
            old_y += k;
            scrolled = k;
        }
        if self.p.dead == 0 {
            self.clock += 1;
        }
        // the tower starts moving once you've climbed 100px
        if self.off > 100 && self.p.dead == 0 {
            if self.speed == -1 {
                self.speed = START_SPEEDS[self.rules.start_speed.min(5)];
            }
            let k = if self.speed == 0 { (self.tick & 1) as i32 } else { self.speed };
            self.off += k;
            scrolled += k;
            self.p.y = f(self.p.y).add(F80::from_int(k as i64)).to_f64();
            old_y += k;
        } else if self.off <= 100 && self.p.dead == 0 {
            self.clock = 0;
            self.fall_count = 0;
        }
        if ((self.hurry_y + 99) as u32) <= 578 {
            self.hurry_y -= 2;
        }
        if self.rules.speed_increase
            && self.p.dead == 0
            && SPEEDUPS[self.speedup_idx.min(8)] < self.fall_count
            && self.speed <= 4
        {
            if self.speedup_idx < 5 {
                self.p.ccc[self.speedup_idx] = self.p.level;
            }
            self.speedup_idx += 1;
            self.speed += 1;
            self.hurry_y = 479;
            ev.push(Sfx::Hurry);
        }
        if self.speed == 5 {
            self.fall_count -= 45;
            if self.p.dead == 0 {
                self.clock -= 45;
            }
        }
        if old_off % 16 > self.off % 16 || scrolled > 15 {
            self.add_floor();
        }
        self.collide(old_x, old_y);

        let p = &mut self.p;
        if p.rotate {
            p.angle = (p.angle + 8.0) % 256.0;
        }
        // combo timer runs out: pay floors^2
        if p.in_combo != 0 {
            p.in_combo -= 1;
            if p.in_combo == 0 && p.acc_jumps > 1 {
                p.score += p.acc_level * p.acc_level;
                p.latest_combo = p.acc_level;
                let floors = p.acc_level;
                self.start_reward(floors, &mut ev);
                let p = &mut self.p;
                p.combo_floors += floors;
                p.combos += 1;
                p.best_combo = p.best_combo.max(floors);
            }
        }

        // landing: jumps of 2+ floors build the combo, anything else ends it
        let level_here = self.get_level(self.p.y as i32);
        let p = &mut self.p;
        if p.status == STANDING {
            let floor = (level_here - 1) / 5;
            if floor >= p.level {
                let diff = floor - p.level;
                if diff != self.last_diff && diff != 0 {
                    for i in 0..5 {
                        p.jc_top[i] = p.jc_top[i].max(p.jc[i]);
                        p.jc[i] = 0;
                    }
                    self.last_diff = 0;
                }
                if diff >= 2 {
                    if diff <= 5 {
                        p.jc[diff as usize - 1] += 1;
                    }
                    if p.in_combo != 0 {
                        p.acc_level += diff;
                        p.acc_jumps += 1;
                    } else {
                        p.acc_level = diff;
                        p.acc_jumps = 1;
                    }
                    p.in_combo = 100;
                    self.last_diff = diff;
                } else {
                    if diff == 1 {
                        p.jc[0] += 1;
                        self.last_diff = 1;
                    }
                    if diff == 1 && p.in_combo != 0 {
                        p.in_combo = 1;
                    }
                }
                if p.in_combo == 0 {
                    self.combo_start = floor;
                }
            } else {
                if p.in_combo != 0 {
                    p.in_combo = 1;
                }
                for i in 0..5 {
                    p.jc_top[i] = p.jc_top[i].max(p.jc[i]);
                    p.jc[i] = 0;
                }
                self.last_diff = 0;
            }
            p.level = floor;
            if p.combos == 0 && p.no_combo_top < p.level {
                p.no_combo_top = self.combo_start;
            }
        }

        if p.y > 540.0 && p.dead == 0 {
            if p.in_combo != 0 && p.acc_jumps > 1 {
                p.lost_combo = p.acc_level;
            }
            p.in_combo = 0;
            p.dead = 1;
            self.splat_count = 1;
            ev.push(Sfx::Death);
            for i in 0..5 {
                p.jc_top[i] = p.jc_top[i].max(p.jc[i]);
                p.jc[i] = 0;
            }
            if p.combos == 0 && p.no_combo_top < p.level {
                p.no_combo_top = p.level;
            }
        }
        self.splat(&mut ev);
        let p = &mut self.p;
        // every 50 floors (500 past 1000): a fountain of stars from the bottom of the screen
        if self.milestone <= p.level {
            ev.push(Sfx::Milestone);
            if self.rules.eye_candy {
                for _ in 0..self.milestone / 2 {
                    let x = self.vis_rand() % 600 + 20;
                    let i = self.add_star(x, 480);
                    let sy = -((self.vis_rand() % 200) << 16) / 10;
                    if let Some(s) = self.stars.get_mut(i) {
                        s.sy = sy;
                    }
                }
            }
            self.milestone += if self.milestone > 999 { 500 } else { 50 };
        }
        self.shake();
        let p = &mut self.p;
        if p.edge == 0 {
            p.edge_drawn = 0;
        }
        if p.edge_drawn == 11 && p.status == STANDING {
            ev.push(Sfx::Edge);
        }
        if p.edge_drawn == 50 {
            p.edge_drawn = 0;
        }

        self.update_frame();

        // background stripes change every 256px of scrolling
        if self.off >> 8 > self.last_stripe {
            self.last_stripe += 1;
            self.bg_ids.copy_within(0..4, 1);
            let kinds = match self.p.level {
                ..=200 => 2,
                201..=350 => 3,
                351..=600 => 4,
                _ => 5,
            };
            self.bg_ids[0] = loop {
                if self.vis_rand() % 100 > 40 {
                    break 0;
                }
                let id = (self.vis_rand() % kinds) as usize;
                if id != self.bg_ids[1] && id != self.bg_ids[2] {
                    break id;
                }
            };
        }
        ev
    }

    pub fn tick(&mut self, left: bool, right: bool, jump: bool) -> Vec<Sfx> {
        self.tick_flags(if left { LEFT } else { 0 } | if right { RIGHT } else { 0 } | if jump { JUMP } else { 0 })
    }

    /// The splat comes level*5 ticks after falling off (at most 5s), usually behind the results.
    fn splat(&mut self, ev: &mut Vec<Sfx>) {
        if self.splat_count != 0 {
            self.splat_count += 1;
            if self.splat_count > self.p.level * 5 || self.splat_count > 250 {
                ev.push(Sfx::Splat);
                self.p.shake = 24;
                self.splat_count = 0;
            }
        }
    }

    fn shake(&mut self) {
        if self.p.shake != 0 {
            self.p.shake -= 1;
            self.shake_y = self.vis_rand() % 8;
        }
    }

    fn update_frame(&mut self) {
        if self.reward_time != 0 {
            if self.reward_time > 60 {
                self.reward_scale += 0.05;
            } else if self.reward_time <= 9 {
                self.reward_scale -= 0.1;
            }
            self.reward_time -= 1;
        }
        let p = &mut self.p;
        if p.dead != 0 && p.dead <= 299 {
            p.dead += 8;
        }
        if p.edge != 0 {
            p.edge_drawn += 1;
        }
        if self.tick % 10 == 0 {
            p.frame += 1;
        }
    }

    /// One 50Hz step of play()'s results loop: the tower is frozen but the eye candy, the
    /// HURRY UP sign and the splat keep going.
    pub fn post_tick(&mut self) -> Vec<Sfx> {
        let mut ev = Vec::new();
        self.update_frame();
        self.update_stars();
        if ((self.hurry_y + 99) as u32) <= 578 {
            self.hurry_y -= 2;
        }
        self.splat(&mut ev);
        self.shake();
        ev
    }

    /// The original stops the game this long after the fall (dead > 100).
    pub fn game_over(&self) -> bool {
        self.p.dead > 100
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    const FLOOR_GAP: i32 = 80; // 5 map rows of 16px

    fn run(g: &mut Game, ticks: usize, left: bool, right: bool, jump: bool) {
        for _ in 0..ticks {
            g.tick(left, right, jump);
        }
    }

    #[test]
    fn falling_off_splats_then_shakes_and_stars_fall() {
        let mut g = Game::new(1, Rules::default());
        g.p.status = IN_AIR;
        g.p.y = 600.0;
        // floor 0: splat_count > level*5 straight away
        assert!(g.tick_flags(0).contains(&Sfx::Splat));
        assert_eq!(g.p.shake, 23);
        g.add_star(320, 200);
        let y0 = g.stars[0].pos().1;
        for _ in 0..30 {
            g.post_tick();
        }
        assert!(g.stars[0].pos().1 > y0 && g.p.shake == 0);
    }

    #[test]
    fn star_pos_at_steps_back_one_tick() {
        let mut g = Game::new(1, Rules::default());
        g.add_star(320, 200);
        g.update_stars();
        let before = (g.stars[0].x as f32 / 65536.0, g.stars[0].y as f32 / 65536.0);
        g.update_stars();
        let (x, y) = g.stars[0].pos_at(1.0);
        assert!((x - before.0).abs() < 1e-3 && (y - before.1).abs() < 1e-3);
    }

    #[test]
    fn starts_standing_on_ground() {
        let mut g = Game::new(1, Rules::default());
        run(&mut g, 30, false, false, false);
        assert_eq!(g.p.status, STANDING);
        assert_eq!(g.p.y, 431.0);
    }

    #[test]
    fn running_jump_is_higher_and_spins() {
        let mut g = Game::new(3, Rules::default());
        g.p.sx = 11.5;
        g.tick(false, true, true);
        assert!(g.p.sy < -22.0 + 0.8 && g.p.rotate);
    }

    #[test]
    fn floors_follow_original_layout() {
        let g = Game::new(9, Rules::default());
        let floors = g.visible_floors();
        let ground = floors.iter().find(|f| f.0 == 0).unwrap();
        assert_eq!((ground.1.start, ground.1.end, ground.2), (0, 40, 432));
        for (n, fl, y, _, _) in &floors {
            assert_eq!(*y, 432 - n * FLOOR_GAP);
            if *n > 0 {
                let w = fl.end - fl.start;
                assert!((6..=15).contains(&w) && fl.start >= 5 && fl.end <= 34, "{}..{}", fl.start, fl.end);
            }
        }
    }

    #[test]
    fn msvcrt_rand_sequence() {
        let mut g = Game::new(1, Rules::default());
        g.holdrand = 1;
        assert_eq!([g.rand(), g.rand(), g.rand()], [41, 18467, 6334]);
    }
}
