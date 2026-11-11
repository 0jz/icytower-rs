// Icy Tower 1.5 rebuilt in Rust: logic in game.rs, real assets extracted by tools/extract.py.
mod app;
mod f80;
mod game;
mod hisc;
mod menu;
mod profile;

use game::{Game, STANDING};
use menu::Settings;
use macroquad::audio::{load_sound, play_sound, stop_sound, PlaySoundParams, Sound};
use macroquad::prelude::*;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

const W: f32 = 640.0;
const H: f32 = 480.0;
const CHARACTERS: [&str; 4] = ["harold", "dave", "jane", "wendy"]; // same order as menu::CHARACTER_NAMES
// data.dat floor sprites in datafile order: 11 styles x (left cap, middle, right cap)
const FLOORS: [&str; 33] = [
    "FLOOR01", "FLOOR02", "FLOOR03", "FLOOR04", "FLOOR05", "FLOOR06", "FLOOR07", "FLOOR08", "FLOOR09",
    "FLOOR10", "FLOOR11", "FLOOR12", "FLOOR12a", "FLOOR12b", "FLOOR12c", "FLOOR13", "FLOOR14", "FLOOR15",
    "FLOOR16", "FLOOR17", "FLOOR18", "FLOOR18a", "FLOOR18b", "FLOOR18c", "FLOOR19", "FLOOR20", "FLOOR21",
    "FLOOR22", "FLOOR23", "FLOOR24", "FLOOR25", "FLOOR26", "FLOOR27",
];
const SIGNS: [&str; 11] = [
    "SIGN01", "SIGN02", "SIGN03", "SIGN04", "SIGN04a", "SIGN05", "SIGN06", "SIGN06a", "SIGN07", "SIGN08", "SIGN09",
];
const REWARD_SOUNDS: [&str; 10] = [
    "S_GOOD", "S_SWEET", "S_GREAT", "S_SUPER", "S_WOW", "S_AMAZING", "S_EXTREME", "S_FANTASTIC", "S_SPLENDID",
    "S_UNBELIEVABLE",
];

fn asset_dir() -> PathBuf {
    let exe_dir = std::env::current_exe().ok().and_then(|p| p.parent().map(Path::to_path_buf));
    [exe_dir.map(|d| d.join("assets")), Some(PathBuf::from("assets")), Some(Path::new(env!("CARGO_MANIFEST_DIR")).join("assets"))]
        .into_iter()
        .flatten()
        .find(|p| p.join("data").is_dir())
        .expect("assets/ not found: run `python -I tools/extract.py .. assets` first")
}

/// HD graphics on/off (GFX Options), read by every sprite draw.
static HD: AtomicBool = AtomicBool::new(true);

pub fn set_hd(on: bool) {
    HD.store(on, Ordering::Relaxed);
}

/// A sprite: the original texture plus, when assets_hd/ has it, an upscaled copy.
/// Sizes are always the original's, so layout and positions don't change with HD on.
pub struct Tex {
    sd: Texture2D,
    hd: Option<Texture2D>,
}

impl Tex {
    pub fn width(&self) -> f32 {
        self.sd.width()
    }

    pub fn height(&self) -> f32 {
        self.sd.height()
    }

    fn pick(&self) -> &Texture2D {
        match &self.hd {
            Some(hd) if HD.load(Ordering::Relaxed) => hd,
            _ => &self.sd,
        }
    }
}

/// Shadows macroquad's: draws a `Tex` at its original size whichever texture is used.
pub fn draw_texture(t: &Tex, x: f32, y: f32, color: Color) {
    draw_texture_ex(t, x, y, color, DrawTextureParams::default());
}

/// Shadows macroquad's: `source` and the default size are in original pixels.
pub fn draw_texture_ex(t: &Tex, x: f32, y: f32, color: Color, mut params: DrawTextureParams) {
    let tex = t.pick();
    let k = tex.width() / t.width();
    params.dest_size = params.dest_size.or(Some(match params.source {
        Some(r) => vec2(r.w, r.h),
        None => vec2(t.width(), t.height()),
    }));
    params.source = params.source.map(|r| Rect::new(r.x * k, r.y * k, r.w * k, r.h * k));
    macroquad::prelude::draw_texture_ex(tex, x, y, color, params);
}

/// `base/<folder>/<file>.png` and, if present, `base_hd/<folder>/<file>.png` (tools/upscale.py).
async fn tex(path: &Path) -> Tex {
    let load = |p: PathBuf| async move { load_texture(&p.to_string_lossy()).await };
    let sd = load(path.to_path_buf()).await.unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    sd.set_filter(FilterMode::Nearest);
    let hd_path = path.parent().and_then(|dir| {
        let base = dir.parent()?;
        let name = format!("{}_hd", base.file_name()?.to_string_lossy());
        Some(base.with_file_name(name).join(dir.file_name()?).join(path.file_name()?))
    });
    let hd = match hd_path {
        Some(p) if p.is_file() => load(p).await.ok(),
        _ => None,
    };
    if let Some(hd) = &hd {
        hd.set_filter(FilterMode::Linear); // shrunk to the window size, so smooth it
    }
    Tex { sd, hd }
}

async fn snd(path: &Path) -> Option<Sound> {
    load_sound(&path.to_string_lossy()).await.ok()
}

/// The game's own bitmap fonts: one glyph strip for chars 32..127 plus widths.
struct BmpFont {
    tex: Tex,
    x: Vec<f32>,
    w: Vec<f32>,
}

impl BmpFont {
    async fn load(base: &Path, name: &str) -> Self {
        let tex = tex(&base.join(format!("{name}.png"))).await;
        let w: Vec<f32> = std::fs::read_to_string(base.join(format!("{name}.widths")))
            .expect("font widths")
            .split_whitespace()
            .map(|s| s.parse().unwrap())
            .collect();
        let x = w.iter().scan(0.0, |acc, w| { let x = *acc; *acc += w; Some(x) }).collect();
        BmpFont { tex, x, w }
    }

    fn width(&self, s: &str) -> f32 {
        s.bytes().map(|c| self.w.get(c.wrapping_sub(32) as usize).copied().unwrap_or(0.0)).sum()
    }

    fn draw_scaled(&self, s: &str, mut x: f32, y: f32, color: Color, scale: f32) {
        for c in s.bytes() {
            let i = c.wrapping_sub(32) as usize;
            if i >= self.w.len() { continue; }
            draw_texture_ex(&self.tex, x, y, color, DrawTextureParams {
                source: Some(Rect::new(self.x[i], 0.0, self.w[i], self.tex.height())),
                dest_size: Some(vec2(self.w[i] * scale, self.tex.height() * scale)),
                ..Default::default()
            });
            x += self.w[i] * scale;
        }
    }

    fn draw(&self, s: &str, x: f32, y: f32, color: Color) {
        self.draw_scaled(s, x, y, color, 1.0);
    }

    fn draw_centre(&self, s: &str, x: f32, y: f32, color: Color) {
        self.draw(s, x - self.width(s) / 2.0, y, color);
    }
}

struct Character {
    frames: Vec<Tex>, // 15 frames, see characters.txt
    sounds: Vec<Option<Sound>>, // jump lo/mid/hi, greeting, pause, fall, edge, [music]
}

impl Character {
    async fn load(dir: &Path) -> Self {
        let mut files: Vec<PathBuf> = std::fs::read_dir(dir).expect("character dir").flatten().map(|e| e.path()).collect();
        files.sort();
        let mut frames = Vec::new();
        let mut sounds = Vec::new();
        for f in &files {
            match f.extension().and_then(|e| e.to_str()) {
                Some("png") => frames.push(tex(f).await),
                Some("ogg") => sounds.push(snd(f).await),
                _ => {}
            }
        }
        Character { frames, sounds }
    }

    fn sound(&self, i: usize) -> Option<&Sound> {
        self.sounds.get(i).and_then(Option::as_ref)
    }
}

struct Assets {
    bg: Vec<Tex>,
    floors: Vec<Tex>,
    signs: Vec<Tex>,
    side: Tex,
    stars: Vec<Tex>,
    rewards: Vec<Tex>,
    clock: Tex,
    clock_hand: Tex,
    meter: Tex,
    liquid: Tex,
    count: Tex,
    hurry: Tex,
    gameover: Tex,
    icon_pb: Tex,
    title_bg: Tex,
    title: Tex,
    menu_bg: Tex,
    bullet: Tex,
    instructions: Tex,
    #[allow(dead_code)] // in data.dat but never drawn by the original
    highscore: Tex,
    hisc: [Tex; 3],
    black: BmpFont,
    big: BmpFont,
    med: BmpFont,
    small: BmpFont,
    rewards_sfx: Vec<Option<Sound>>,
    ring: Option<Sound>,
    hurry_sfx: Option<Sound>,
    gameover_sfx: Option<Sound>,
    cheer: Option<Sound>,
    aight: Option<Sound>,
    splat: Option<Sound>,
    beat: Option<Sound>,
    menu_music: Option<Sound>,
    menu_move: Option<Sound>,
    menu_select: Option<Sound>,
    mono: BmpFont,
    faces: Vec<Tex>,
    face_shadow: Tex,
    replay_bg: Tex,
    replay_bg_med: Tex,
    replay_bg_sml: Tex,
    profile_buttons: Tex,
    replay_buttons: Tex,
    vcr: Tex,
    vcr_right: Tex,
    vcr_up: Tex,
    ranks: Vec<Tex>,
    bt_yes: Tex,
    bt_yes_2: Tex,
    bt_no: Tex,
    bt_esc: Tex,
    bt_up_dn: Tex,
    sort_buttons: [Tex; 5], // ALL, then the highlight for name, score, combo, floor
}

impl Assets {
    async fn load(base: &Path) -> Self {
        let d = base.join("data");
        let s = base.join("sfx15");
        let v = |names: Vec<String>| {
            let d = d.clone();
            async move {
                let mut out = Vec::new();
                for n in names { out.push(tex(&d.join(format!("{n}.png"))).await); }
                out
            }
        };
        let bg = v((1..=5).map(|i| if i == 1 { "BGTILE".into() } else { format!("BGTILE{i}") }).collect()).await;
        let floors = v(FLOORS.iter().map(|s| s.to_string()).collect()).await;
        let signs = v(SIGNS.iter().map(|s| s.to_string()).collect()).await;
        let stars = v((1..=8).map(|i| format!("STAR0{i}")).collect()).await;
        let rewards = v((0..10).map(|i| format!("REWARD00{i}")).collect()).await;
        let faces = v((0..3).map(|i| format!("HEROFACE00{i}")).collect()).await;
        let ranks = v((0..12).map(|i| format!("RANK_{i:02}")).collect()).await;
        let mut rewards_sfx = Vec::new();
        for n in REWARD_SOUNDS { rewards_sfx.push(snd(&s.join(format!("{n}.ogg"))).await); }
        Assets {
            bg,
            floors,
            signs,
            stars,
            rewards,
            side: tex(&d.join("SIDEBLOCK.png")).await,
            clock: tex(&d.join("CLOCK.png")).await,
            clock_hand: tex(&d.join("CLOCK_HAND.png")).await,
            meter: tex(&d.join("COMBO_METER.png")).await,
            liquid: tex(&d.join("COMBO_LIQUID.png")).await,
            count: tex(&d.join("COMBO_COUNT.png")).await,
            hurry: tex(&d.join("HURRYUP.png")).await,
            gameover: tex(&d.join("GAMEOVER.png")).await,
            icon_pb: tex(&d.join("ICON_PB.png")).await,
            title_bg: tex(&d.join("TITLE_BG.png")).await,
            title: tex(&d.join("TITLE.png")).await,
            menu_bg: tex(&d.join("MENU_BG.png")).await,
            bullet: tex(&d.join("MENU_BULLET.png")).await,
            instructions: tex(&d.join("INSTRUCTIONS.png")).await,
            highscore: tex(&d.join("HIGHSCORE.png")).await,
            hisc: [tex(&d.join("HISC_TOP.png")).await, tex(&d.join("HISC_MID.png")).await, tex(&d.join("HISC_BTM.png")).await],
            black: BmpFont::load(&d, "FONT_MED_BLACK").await,
            big: BmpFont::load(&d, "FONT_BIG_WHITE").await,
            med: BmpFont::load(&d, "FONT_MED_WHITE").await,
            small: BmpFont::load(&d, "FONT_SMALL").await,
            rewards_sfx,
            ring: snd(&s.join("S_RING.ogg")).await,
            hurry_sfx: snd(&s.join("S_HURRYUP.ogg")).await,
            gameover_sfx: snd(&s.join("S_GAMEOVER.ogg")).await,
            cheer: snd(&s.join("S_CHEER.ogg")).await,
            aight: snd(&s.join("S_AIGHT.ogg")).await,
            splat: snd(&s.join("S_SPLAT.ogg")).await,
            beat: snd(&s.join("S_BG_BEAT.ogg")).await,
            menu_music: snd(&s.join("S_BG_MENU.ogg")).await,
            menu_move: snd(&s.join("S_MENU_CHANGE.ogg")).await,
            menu_select: snd(&s.join("S_MENU_CHOOSE.ogg")).await,
            mono: BmpFont::load(&d, "FONT_MONO").await,
            faces,
            face_shadow: tex(&d.join("HEROFACESHADOW.png")).await,
            replay_bg: tex(&d.join("REPLAY_BG.png")).await,
            replay_bg_med: tex(&d.join("REPLAY_BG_MED.png")).await,
            replay_bg_sml: tex(&d.join("REPLAY_BG_SML.png")).await,
            profile_buttons: tex(&d.join("PROFILE_BUTTONS.png")).await,
            replay_buttons: tex(&d.join("REPLAY_BUTTONS.png")).await,
            vcr: tex(&d.join("VCR.png")).await,
            vcr_right: tex(&d.join("VCR_RIGHT.png")).await,
            vcr_up: tex(&d.join("VCR_UP.png")).await,
            ranks,
            bt_yes: tex(&d.join("BT_YES.png")).await,
            bt_yes_2: tex(&d.join("BT_YES_2.png")).await,
            bt_no: tex(&d.join("BT_NO.png")).await,
            bt_esc: tex(&d.join("BT_ESC.png")).await,
            bt_up_dn: tex(&d.join("BT_UP_DN.png")).await,
            sort_buttons: [
                tex(&d.join("SORT_BUTTON_ALL.png")).await,
                tex(&d.join("SORT_BUTTON_NAME.png")).await,
                tex(&d.join("SORT_BUTTON_SCORE.png")).await,
                tex(&d.join("SORT_BUTTON_COMBO.png")).await,
                tex(&d.join("SORT_BUTTON_FLOOR.png")).await,
            ],
        }
    }
}

fn sprite(t: &Tex, x: f32, y: f32, flip: bool) {
    draw_texture_ex(t, x, y, WHITE, DrawTextureParams { flip_x: flip, ..Default::default() });
}

// draw_frame(): back to front, same positions as the original
/// The splat shakes the whole screen down by 0-7px (the original blits the screen onto itself).
fn shaken(g: &Game, f: impl FnOnce()) {
    let shake = g.p.shake != 0;
    if shake {
        unsafe { get_internal_gl() }.quad_gl.push_model_matrix(Mat4::from_translation(vec3(0.0, g.shake_y as f32, 0.0)));
    }
    f();
    if shake {
        unsafe { get_internal_gl() }.quad_gl.pop_model_matrix();
    }
}

/// The scroll and player position one tick ago. Logic stays at 50Hz (replays depend on it);
/// drawing blends from here toward the current tick so motion is smooth at any refresh rate.
#[derive(Clone, Copy, Default)]
pub struct Prev {
    off: i32,
    x: f64,
    y: f64,
    clock: i32,
    angle: f64,
    in_combo: i32,
    reward_scale: f64,
}

impl Prev {
    pub fn of(g: &Game) -> Self {
        Prev { off: g.off, x: g.p.x, y: g.p.y, clock: g.clock, angle: g.p.angle, in_combo: g.p.in_combo, reward_scale: g.reward_scale }
    }
}

/// `to` moved back `lag` of the way toward `from`; big jumps (resets) aren't blended.
pub fn blend(from: f32, to: f32, lag: f32, max: f32) -> f32 {
    if (to - from).abs() <= max { to - (to - from) * lag } else { to }
}

/// `t` = how far we are from `prev` (0) to `g` (1).
fn draw(g: &Game, a: &Assets, ch: &Character, prev: Prev, t: f32) {
    shaken(g, || draw_frame(g, a, ch, prev, t.clamp(0.0, 1.0)));
}

fn draw_frame(g: &Game, a: &Assets, ch: &Character, prev: Prev, t: f32) {
    let off = g.off;
    let lag = 1.0 - t;
    // everything tied to the scroll sits this many px higher between ticks (skip on resets)
    let k = off - prev.off;
    let ds = if (0..=32).contains(&k) { -(k as f32) * lag } else { 0.0 };

    // background stripes scroll at half speed
    for (i, id) in g.bg_ids.iter().enumerate() {
        let y = (i as i32 - 1) * 128 + (off & 255) / 2;
        draw_texture(&a.bg[*id], 37.0, y as f32 + ds / 2.0, WHITE);
    }
    if (0..=578).contains(&(g.hurry_y + 99)) {
        sprite(&a.hurry, 320.0 - a.hurry.width() / 2.0, g.hurry_y as f32 + 2.0 * lag, false);
    }

    // floors and floor-number signs
    for (n, f, y, s, sign) in g.visible_floors() {
        if !(-48..=496).contains(&y) { continue; }
        let fy = (y - 6) as f32 + ds;
        sprite(&a.floors[s], (f.start * 16 - 5) as f32, fy, false);
        for t in f.start + 1..f.end {
            sprite(&a.floors[s + 1], (t * 16) as f32, fy, false);
        }
        sprite(&a.floors[s + 2], (f.end * 16) as f32, fy, false);
        if n > 0 && n % 10 == 0 {
            let sign = &a.signs[sign];
            let cx = ((f.start + (f.end - f.start) / 2) * 16 + 8) as f32;
            let sy = (y + 10) as f32 + ds;
            sprite(sign, cx - sign.width() / 2.0, sy, false);
            let txt = n.to_string();
            for (dx, dy) in [(-1.0, 0.0), (1.0, 0.0), (0.0, -1.0), (0.0, 1.0)] {
                a.small.draw_centre(&txt, cx + dx, sy + 7.0 + dy, Color::from_rgba(55, 55, 55, 255));
            }
            a.small.draw_centre(&txt, cx, sy + 7.0, WHITE);
        }
    }

    for s in &g.stars {
        let (x, y) = s.pos_at(lag);
        sprite(&a.stars[s.kind], x, y, false);
    }

    // player: frames 1-3 idle (1,10,11), 2-5 walk, 6-8 jump, 9 straight jump, 12 chock, 13 spin, 14-15 edge
    let p = &g.p;
    let f = |i: usize| &ch.frames[(i - 1).min(ch.frames.len() - 1)];
    let (t, flip) = if p.rotate {
        (f(13), p.facing_left)
    } else if p.status == STANDING {
        if p.edge != 0 && p.sx.abs() < 0.2 {
            (f(if g.tick & 8 != 0 { 15 } else { 14 }), p.edge == 2)
        } else if p.sx.abs() < 0.2 {
            (f([1, 10, 1, 11][(p.frame & 3) as usize]), false)
        } else {
            (f(2 + (p.walk_phase as usize & 3)), p.facing_left)
        }
    } else if p.y > 420.0 {
        (f(12), p.facing_left)
    } else if p.sx.abs() < 2.0 {
        (f(9), p.facing_left)
    } else if p.sy < -3.0 {
        (f(6), p.facing_left)
    } else if p.sy <= 3.0 {
        (f(7), p.facing_left)
    } else {
        (f(8), p.facing_left)
    };
    let (x, y) = (blend(prev.x as f32, p.x as f32, lag, 64.0), blend(prev.y as f32, p.y as f32, lag, 64.0));
    let (px, py) = (x - (t.width() / 2.0).floor(), y - t.height() + 1.0);
    let from = if prev.angle > p.angle { prev.angle - 256.0 } else { prev.angle }; // wraps at 256
    let angle = blend(from as f32, p.angle as f32, lag, 8.0);
    let spin = angle / 256.0 * std::f32::consts::TAU * if p.facing_left { -1.0 } else { 1.0 };
    draw_texture_ex(t, px, py, WHITE, DrawTextureParams { flip_x: flip, rotation: if p.rotate { spin } else { 0.0 }, ..Default::default() });

    // walls: 1.476x scroll speed, left one mirrored
    let wy = ((off % 84) as f64 * 1.476) as i32;
    for j in 0..5 {
        let y = (j * 124 - 124 + wy) as f32 + ds * 1.476;
        sprite(&a.side, -57.0, y, true);
        sprite(&a.side, 565.0, y, false);
    }

    // clock: one turn of the hand per 1500 ticks (30s); it rattles while HURRY UP is on screen
    let lc = g.tick as i32;
    let jiggle = |a: i32, b: i32| ((a % 3 - 1) as f32, (b % 3 - 1) as f32);
    let (mut dx, mut dy) = (0.0, 0.0);
    if (251..=479).contains(&g.hurry_y) {
        (dx, dy) = jiggle(lc, lc + 1);
    }
    sprite(&a.clock, 6.0 + dx, 10.0 + dy, false);
    if (201..=479).contains(&g.hurry_y) {
        (dx, dy) = jiggle(lc + 2, lc + 3);
    }
    let clock = blend(prev.clock as f32, g.clock as f32, lag, 1.0);
    let turn = (clock % 1500.0) / 1500.0; // C remainder: negative while the clock runs backwards
    draw_texture_ex(&a.clock_hand, 34.0 + dx, 28.0 + dy, WHITE, DrawTextureParams {
        rotation: turn * std::f32::consts::TAU,
        ..Default::default()
    });

    // combo meter drains over the combo's 100 ticks
    sprite(&a.meter, 22.0, 100.0, false);
    let fill = blend(prev.in_combo as f32, p.in_combo as f32, lag, 1.0).clamp(0.0, 100.0);
    let bubble = if p.in_combo != 0 {
        draw_texture_ex(&a.liquid, 33.0, 219.0 - fill, WHITE, DrawTextureParams {
            source: Some(Rect::new(0.0, 100.0 - fill, 16.0, fill)),
            ..Default::default()
        });
        Some(p.acc_level)
    } else {
        (g.reward_time != 0).then_some(p.latest_combo)
    };
    if let Some(n) = bubble {
        sprite(&a.count, -8.0, 210.0, false);
        a.big.draw_centre(&n.to_string(), 42.0, 210.0, WHITE);
    }

    if g.reward_time > 0 && g.reward_scale > 0.0 {
        let r = &a.rewards[g.reward_idx];
        let s = blend(prev.reward_scale as f32, g.reward_scale as f32, lag, 0.5).min(1.0);
        let (w, h) = (r.width() * s, r.height() * s);
        draw_texture_ex(r, 320.0 - w / 2.0, 200.0 - h / 2.0, WHITE, DrawTextureParams { dest_size: Some(vec2(w, h)), ..Default::default() });
    }

    a.med.draw(&format!("Score: {}", g.final_score()), 8.0, 440.0, WHITE);

}

fn conf() -> Conf {
    Conf {
        window_title: "Icy Tower".into(),
        window_width: 960,
        window_height: 720,
        window_resizable: true,
        ..Default::default()
    }
}

fn sfx(s: Option<&Sound>, settings: &Settings) {
    if let Some(s) = s {
        play_sound(s, PlaySoundParams { looped: false, volume: settings.sound as f32 / 10.0 });
    }
}

/// Stop whatever is playing and loop `next`.
fn switch_music(current: &mut Option<Sound>, next: Option<&Sound>, settings: &Settings) {
    if let Some(m) = current.take() {
        stop_sound(&m);
    }
    if let Some(m) = next {
        play_sound(m, PlaySoundParams { looped: true, volume: settings.music as f32 / 10.0 });
        *current = Some(m.clone());
    }
}

#[macroquad::main(conf)]
async fn main() {
    let base = asset_dir();
    let assets = Assets::load(&base).await;
    let mut chars = Vec::new();
    for c in CHARACTERS {
        chars.push(Character::load(&base.join(c)).await);
    }
    let mut app = app::App::new(&assets);
    prevent_quit(); // the window's X shows the "Thanks for playing!" screen first, like the original
    while !app.quit {
        app.update(&assets, &chars);

        // letterbox the original 640x480 screen into the window
        clear_background(BLACK);
        let s = (screen_width() / W).min(screen_height() / H);
        let (vw, vh) = (W * s, H * s);
        let mut cam = Camera2D::from_display_rect(Rect::new(0.0, 0.0, W, H));
        cam.zoom.y = -cam.zoom.y; // from_display_rect is y-up here; the game is y-down
        cam.viewport = Some((((screen_width() - vw) / 2.0) as i32, ((screen_height() - vh) / 2.0) as i32, vw as i32, vh as i32));
        set_camera(&cam);
        app.draw(&assets, &chars);
        set_default_camera();
        next_frame().await
    }
}
