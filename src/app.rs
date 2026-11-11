// Screens, menus, profiles and replays, laid out like the original (main.c / profile.c / replay.c).
use super::{blend, draw, draw_texture, draw_texture_ex, set_hd, shaken, sfx, switch_music, Assets, BmpFont, Character, Prev, Tex, H, W};
use crate::game::{self, Game, Sfx};
use crate::hisc::{self, Tables};
use crate::menu::{self, Action, MenuId, Settings};
use crate::game::{Rules, JUMP, LEFT, RIGHT};
use crate::profile::{self, Playback, Profile, Recorder, Replay};
use macroquad::audio::{set_sound_volume, Sound};
use macroquad::prelude::*;
use std::path::{Path, PathBuf};

const INK: Color = Color::new(30.0 / 255.0, 20.0 / 255.0, 10.0 / 255.0, 1.0); // draw_buffer() text colour
const GREETING: &str = "         Welcome to Icy Tower!     Help Harold the Homeboy to climb as high as possible!       Use arrow keys to move and spacebar to jump.      Good luck!";
const MENU_X: f32 = 355.0; // handle_menu(main_menu, ..., 355, 285)
const MENU_Y: f32 = 285.0;
const MENU_LINE: f32 = 24.0;
const WINDOW: (f32, f32) = (103.0, 25.0); // REPLAY_BG list windows, centred
const DIM_MENU: f32 = 0.6;
const CUSTOM_MSG: &str = "Custom mode is crazy fun but does not add to your profile. Play Classic Mode to compete in the highscore lists and climb in rank!";
const GUEST_MSG: &str = "You're playing in guest mode. Start a profile and record your progress!";
const HINTS: [&str; 45] = [
    "How much is left to reach the next rank?",
    "Check out your profile to see what you need to do to reach the next rank!",
    "In your profile you can see all your records! Check it out!",
    "How many times did you jump? Check out your profile!",
    "Tell your friends about Icy Tower. The more the merrier!",
    "Is that really your best?",
    "Was that really your best?",
    "You can do better than that!",
    "You can do better! One more time!",
    "Just one more time! Please?",
    "Make combo jumps for lots of score!",
    "Bounce on the walls to maintain your speed!",
    "AGAIN!",
    "Play again!",
    "Didn't you see that coming?",
    "Awww... Try again!",
    "That was close! You'll make it next time!",
    "Did you beat your high score yet?",
    "Calm down, it's not as hard as it seems.",
    "Come on, concentrate!",
    "Come on, focus!",
    "Better luck next time!",
    "Harold the Homeboy really likes cheese!",
    "There is always room for improvement! Once more!",
    "Icy Tower is also available for iPhone and iPod Touch! That's so exciting!",
    "Icy Tower is available on mobile phones! Have you tried it yet?",
    "Don't miss Icy Tower for iPhone and iPod Touch! Available now, yay!",
    "Now you can play Icy Tower on your iPhone phone as well, check it out now!",
    "Wanna play Icy Tower anywhere? Check out Icy Tower for mobile phones!",
    "Everyone is talking about Icy Tower for iPhone, you should get it too!",
    "Get Icy Tower for your phone and play anywhere, anytime!",
    "Awesome news: Icy Tower for mobile phones is out now!",
    "Challenge your friends in Icy Tower on Facebook!",
    "Now you can play anytime you want to! Icy Tower on your iPhone!",
    "Maybe you should try playing with a different character?",
    "Did you try playing with a different character?",
    "Icy Tower is a game from Free Lunch Design. More awesome gamea on our website!",
    "You are playing Icy Tower, the game that everyone loves!",
    "Everybody loves Icy Tower, you too!",
    "Play Icy Tower on Facebook! Compete with your friends!",
    "Have you tried Icy Tower on Facebook yet?",
    "Icy Tower is also available on Facebook. Try it out!",
    "Play Icy Tower on Facebook!",
    "Play Icy Tower with your friends on Facebook!",
    "Join all the Icy Tower fans on Facebook!",
];

/// After the fall: results slide up (play()'s ending), then the replay menu (do_replay_menu()).
#[derive(Clone, Copy, PartialEq)]
enum Post {
    Playing,
    Results,
    Unlock, // "A new start floor has been unlocked!"
    Menu,
}

/// play()'s "Enter your initials" (guest players only; profiles enter under their name).
/// Arrows pick from CHARSET and jump confirms, or just type; '^' is the backspace entry
/// (the original's '\xa4', which its ASCII font draws as Allegro's missing-glyph '^').
const CHARSET: &[u8; 29] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ .^";

struct Initials {
    slots: [u8; 3],
    cur: usize, // 3 = done
    sel: usize, // CHARSET index under the arrows
    wait: i32,  // arrow auto-repeat, 8 ticks
    done: i32,  // 20 while typing, then counts down from 19 after the last letter
    blink: u32,
}

impl Initials {
    fn new() -> Self {
        Initials { slots: *b"...", cur: 0, sel: 0, wait: 0, done: 20, blink: 0 }
    }

    /// One 50Hz tick; true once the name is in and the short pause is over.
    fn tick(&mut self, left: bool, right: bool) -> bool {
        self.blink += 1;
        if self.done != 20 {
            self.done -= 1;
            return self.done <= 0;
        }
        if self.wait > 0 {
            self.wait -= 1;
        } else if right {
            self.sel = (self.sel + 1) % CHARSET.len();
            self.wait = 8;
        } else if left {
            self.sel = (self.sel + CHARSET.len() - 1) % CHARSET.len();
            self.wait = 8;
        }
        if !left && !right {
            self.wait = 0;
        }
        if self.cur <= 2 {
            self.slots[self.cur] = CHARSET[self.sel];
        }
        false
    }

    fn fire(&mut self) {
        if self.done != 20 {
            return;
        }
        if CHARSET[self.sel] == b'^' {
            if self.cur > 0 {
                self.slots[self.cur] = b'.';
                self.cur -= 1;
            }
        } else if self.cur < 2 {
            self.cur += 1;
        } else {
            self.cur = 3;
            self.done = 19;
        }
    }

    /// Typed letters (any case) and '.' go straight in, like the original's readkey() path.
    fn typed(&mut self, c: char) {
        let c = c.to_ascii_uppercase() as u8;
        if self.done != 20 || self.cur > 2 {
            return;
        }
        if let Some(i) = CHARSET[..28].iter().position(|&x| x == c && x != b' ') {
            self.slots[self.cur] = c;
            self.cur += 1;
            self.sel = i;
            if self.cur == 3 {
                self.done = 19;
            }
        }
    }

    fn erase(&mut self) {
        if self.done != 20 {
            return;
        }
        if self.cur <= 2 {
            self.slots[self.cur] = b'.';
        }
        self.cur = self.cur.saturating_sub(1);
    }

    fn name(&self) -> String {
        self.slots.iter().map(|&c| c as char).collect()
    }
}

/// Menu/results animation values one menu tick ago, for drawing between ticks.
#[derive(Clone, Copy, Default)]
struct UiPrev {
    res_y: f32,
    rank_y: f32,
    summary_x: f32,
    hs_y: f32,
    hs_dim: f32,
}

/// view_scores(): the paper with all 15 tables slides up over the darkened menu.
#[derive(Clone, Copy)]
struct HsView {
    y: f32,
    target: f32,
    dim: f32,
    leaving: bool,
}

/// F1 in the replay list: Allegro's file_select_ex("Select a new folder and press OK.", "itr").
struct Browser {
    dir: PathBuf,
    entries: Vec<(String, bool)>, // name, is folder
    sel: usize,
}

impl Browser {
    fn open(dir: PathBuf) -> Self {
        let mut b = Browser { dir, entries: Vec::new(), sel: 0 };
        b.refresh();
        b
    }

    fn refresh(&mut self) {
        let (dirs, files) = folder_entries(&self.dir);
        self.entries = std::iter::once(("..".to_string(), true))
            .chain(dirs.into_iter().map(|d| (d, true)))
            .chain(files.into_iter().map(|f| (f, false)))
            .collect();
        self.sel = 0;
    }

    fn enter(&mut self, name: &str) {
        let next = if name == ".." { self.dir.parent().map(Path::to_path_buf) } else { Some(self.dir.join(name)) };
        if let Some(d) = next {
            self.dir = d;
            self.refresh();
        }
    }
}

/// Sub-folders and .itr files in `dir`, each sorted by name.
fn folder_entries(dir: &Path) -> (Vec<String>, Vec<String>) {
    let (mut dirs, mut files) = (Vec::new(), Vec::new());
    for e in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        let name = e.file_name().to_string_lossy().into_owned();
        if e.path().is_dir() {
            dirs.push(name);
        } else if e.path().extension().map_or(false, |x| x.eq_ignore_ascii_case("itr")) {
            files.push(name);
        }
    }
    dirs.sort_by_key(|d| d.to_lowercase());
    files.sort_by_key(|f| f.to_lowercase());
    (dirs, files)
}

/// The 15 highscore values of a game, in hisc::NAMES order.
fn hisc_values(r: &Replay) -> [u32; 15] {
    let mut v = [r.score, r.combo, r.floor, r.lost_combo, r.no_combo, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
    v[5..10].copy_from_slice(&r.ccc);
    v[10..15].copy_from_slice(&r.jc);
    v.map(|x| x.max(0) as u32)
}

#[derive(Clone, Copy, PartialEq)]
enum Screen {
    Menu,
    Instructions,
    HighScores,
    ViewProfile,
    SelectProfile,
    NewProfile,
    SelectReplay,
    SaveReplay,
    Playing,
    Watching,
}

enum Confirm {
    DeleteProfile(String),
    DeleteReplay(PathBuf),
}

/// my_alert(): a small window with a title and message; Enter = yes/ok, Esc = no.
struct Alert {
    title: String,
    msg: String,
    confirm: Option<Confirm>,
}

struct Watch {
    rep: Replay,
    game: Game,
    input: Playback,
    paused: bool,
    back: Screen,
}

pub struct App {
    pub settings: Settings,
    profile: Profile,
    profiles: Vec<String>,
    screen: Screen,
    menu: MenuId,
    sel: usize,
    list_sel: usize,
    binding: Option<usize>,
    game: Game,
    seed: u32,
    custom: bool,
    replay: Replay,
    recorder: Recorder,
    post: Post,
    res_y: f64, // results panel slides from 480 towards 130
    rank_y: f64,
    rank_before: usize,
    bests: profile::Bests,
    summary: String,
    summary_x: f32,
    ui_acc: f64,
    face: usize,
    pending: Vec<Sfx>, // sounds from the 50Hz results loop
    last_replay: Option<Replay>,
    replays: Vec<Replay>,
    folders: Vec<String>, // shown above the replays; ".." first
    replay_dir: Option<PathBuf>, // another folder picked with F1 / Enter; None = the profile's
    browser: Option<Browser>,
    hisc: Tables,
    hisc_vals: [u32; 15],
    hisc_hits: Vec<usize>, // tables this game qualifies for
    initials: Option<Initials>,
    leave: i32, // results: 20 = waiting for a key, then counts down
    unlocked: bool,
    hs: HsView,
    thanks: Option<i32>, // show_credits() ticks while quitting
    fields: [String; 3],
    field: usize,
    alert: Option<Alert>,
    watch: Option<Watch>,
    music: Option<Sound>,
    acc: f64,
    prev: Prev, // state before the latest game tick, for smooth drawing
    ui_prev: UiPrev, // same for the menu/results animations
    t: u64, // title animation ticks
    view_back: Screen,
    pub quit: bool,
}

/// new_game(): srand(time(0)); seed = rand(), so 0..32767 like the original's
fn new_seed() -> u32 {
    (miniquad::date::now() * 1000.0) as u32 % 32768
}

impl App {
    pub fn new(a: &Assets) -> Self {
        macroquad::rand::srand(miniquad::date::now() as u64);
        let mut settings = Settings::load();
        let profiles = profile::list_profiles();
        if !profiles.contains(&settings.profile) {
            // first run: continue with a profile imported from the original game, if any
            // ponytail: takes the first one; read tower.cfg (packed) for the exact last-used one if people have several
            settings.profile = profiles.iter().find(|p| *p != "guest").cloned().unwrap_or_else(|| "guest".into());
        }
        let profile = profile::load_profile(&settings.profile);
        set_fullscreen(settings.fullscreen);
        set_hd(settings.hd);
        let mut app = App {
            game: Game::new(1, settings.rules(false)),
            settings,
            profile,
            profiles,
            screen: Screen::Menu,
            menu: MenuId::Main,
            sel: 0,
            list_sel: 0,
            binding: None,
            seed: 1,
            custom: false,
            replay: Replay::new("guest", 1, Rules::default(), 0),
            recorder: Recorder::new(),
            post: Post::Playing,
            res_y: 480.0,
            rank_y: 480.0,
            rank_before: 0,
            bests: Default::default(),
            summary: String::new(),
            summary_x: 0.0,
            ui_acc: 0.0,
            face: 0,
            pending: Vec::new(),
            last_replay: None,
            replays: Vec::new(),
            folders: Vec::new(),
            replay_dir: None,
            browser: None,
            hisc: Tables::load(),
            hisc_vals: [0; 15],
            hisc_hits: Vec::new(),
            initials: None,
            leave: 20,
            unlocked: false,
            hs: HsView { y: 500.0, target: 0.0, dim: 0.0, leaving: false },
            thanks: None,
            fields: Default::default(),
            field: 0,
            alert: None,
            watch: None,
            music: None,
            acc: 0.0,
            prev: Prev::default(),
            ui_prev: UiPrev::default(),
            t: 0,
            view_back: Screen::Menu,
            quit: false,
        };
        app.menu_music(a);
        app
    }

    fn menu_music(&mut self, a: &Assets) {
        switch_music(&mut self.music, a.menu_music.as_ref(), &self.settings);
    }

    fn to_main_menu(&mut self, a: &Assets) {
        self.screen = Screen::Menu;
        self.menu = MenuId::Main;
        self.sel = 0;
        self.menu_music(a);
    }

    fn info(&mut self, title: &str, msg: &str) {
        self.alert = Some(Alert { title: title.into(), msg: msg.into(), confirm: None });
    }

    fn start_game(&mut self, a: &Assets, chars: &[Character], custom: bool) {
        self.custom = custom;
        self.seed = new_seed();
        let rules = self.settings.rules(custom);
        self.game = Game::new(self.seed, rules);
        self.prev = Prev::of(&self.game);
        self.replay = Replay::new(&self.profile.name, self.seed, rules, self.settings.character);
        self.recorder = Recorder::new();
        self.post = Post::Playing;
        self.acc = 0.0;
        self.screen = Screen::Playing;
        let ch = &chars[self.settings.character];
        switch_music(&mut self.music, ch.sound(7).or(a.beat.as_ref()), &self.settings);
        sfx(ch.sound(3), &self.settings);
    }

    fn start_watch(&mut self, a: &Assets, chars: &[Character], rep: Replay, back: Screen) {
        let ch = &chars[rep.character.min(chars.len() - 1)];
        switch_music(&mut self.music, ch.sound(7).or(a.beat.as_ref()), &self.settings);
        let game = Game::new(rep.seed, rep.rules);
        self.prev = Prev::of(&game);
        self.watch = Some(Watch { game, input: rep.player(), rep, paused: false, back });
        self.acc = 0.0;
        self.screen = Screen::Watching;
    }

    /// Called once when the death animation ends: profile stats, high scores, replays.
    fn finish_game(&mut self, a: &Assets) {
        self.post = Post::Results;
        self.res_y = 480.0;
        self.rank_y = 580.0;
        self.leave = 20;
        self.rank_before = self.profile.rank_id();
        let g = &self.game;
        let bests = self.profile.add_game(g, self.custom);
        self.profile.save();
        let mut rep = self.replay.clone();
        rep.finish(g);
        // play(): custom games never reach the highscore lists or unlock start floors
        self.unlocked = false;
        self.hisc_hits.clear();
        self.initials = None;
        if !self.custom {
            self.unlocked = self.settings.reach_floor(g.p.level);
            self.settings.save();
            self.hisc_vals = hisc_values(&rep);
            self.hisc_hits = (0..15).filter(|&i| self.hisc.qualifies(i, self.hisc_vals[i])).collect();
        }
        let qualified = !self.hisc_hits.is_empty();
        if qualified {
            if self.profile.name == "guest" {
                self.initials = Some(Initials::new());
            } else {
                let name = self.profile.name.clone();
                self.enter_hisc(&name);
            }
        }
        profile::autosave_replays(&self.profile, &mut rep, &bests);
        self.last_replay = Some(rep);
        self.summary = if self.custom {
            CUSTOM_MSG.into()
        } else if self.profile.name == "guest" {
            GUEST_MSG.into()
        } else {
            HINTS[macroquad::rand::gen_range(0, HINTS.len())].into()
        };
        self.summary_x = W - 150.0;
        self.bests = bests;
        self.menu = MenuId::GameOver;
        self.sel = 0;
        // play(): a cheer only when the game made one of the 15 highscore lists
        sfx(if qualified { a.cheer.as_ref() } else { a.gameover_sfx.as_ref() }, &self.settings);
    }

    fn enter_hisc(&mut self, name: &str) {
        for &i in &self.hisc_hits {
            self.hisc.enter(i, self.hisc_vals[i], name);
        }
        self.hisc_hits.clear();
        self.hisc.save();
    }

    /// End of play()'s results loop: the unlock notice if any, then the replay menu.
    fn leave_results(&mut self) {
        self.sel = 0;
        if self.unlocked {
            self.post = Post::Unlock;
            self.pending.push(Sfx::Milestone); // S_AIGHT, like the original
        } else {
            self.post = Post::Menu;
        }
    }

    fn quit_game(&mut self) {
        self.settings.save();
        if self.thanks.is_none() {
            self.thanks = Some(0);
        }
    }

    /// The replay list: folders first, then .itr files in the chosen order (N/S/C/F).
    fn load_replays(&mut self) {
        let dir = self.replay_dir.clone().unwrap_or_else(|| self.profile.replay_dir());
        self.folders = folder_entries(&dir).0;
        if dir.parent().is_some() {
            self.folders.insert(0, "..".into());
        }
        self.replays = profile::list_replays(&dir);
        self.sort_replays();
        self.list_sel = 0;
    }

    /// my_strcmp(): file name order, or highest score / combo / floor first
    fn sort_replays(&mut self) {
        let key = |r: &Replay| r.file.file_name().map(|f| f.to_string_lossy().to_lowercase()).unwrap_or_default();
        match self.settings.replay_sort {
            2 => self.replays.sort_by(|a, b| b.score.cmp(&a.score)),
            3 => self.replays.sort_by(|a, b| b.combo.cmp(&a.combo)),
            4 => self.replays.sort_by(|a, b| b.floor.cmp(&a.floor)),
            _ => self.replays.sort_by_key(key),
        }
    }

    fn play_events(&mut self, ev: Vec<Sfx>, a: &Assets, ch: &Character) {
        for e in ev {
            let s = match e {
                Sfx::JumpLow => ch.sound(0),
                Sfx::JumpMid => ch.sound(1),
                Sfx::JumpHigh => ch.sound(2),
                Sfx::Death => {
                    switch_music(&mut self.music, None, &self.settings);
                    ch.sound(5)
                }
                Sfx::Edge => ch.sound(6),
                Sfx::Hurry => {
                    sfx(a.ring.as_ref(), &self.settings);
                    a.hurry_sfx.as_ref()
                }
                Sfx::Reward(i) => a.rewards_sfx[i].as_ref(),
                Sfx::Milestone => a.aight.as_ref(),
                Sfx::Splat => {
                    if let Some(s) = ch.sound(5) {
                        macroquad::audio::stop_sound(s); // the falling scream ends on impact
                    }
                    a.splat.as_ref()
                }
            };
            sfx(s, &self.settings);
        }
    }

    // ------------------------------------------------------------------ input

    /// Title and results animation run on the original's 50Hz menu timer, not the frame rate.
    fn ui_tick(&mut self) {
        self.ui_prev = UiPrev {
            res_y: self.res_y as f32,
            rank_y: self.rank_y as f32,
            summary_x: self.summary_x,
            hs_y: self.hs.y,
            hs_dim: self.hs.dim,
        };
        self.t += 1;
        // main_menu_callback(): new_rand() % 198 == 1 changes Harold's face
        if macroquad::rand::gen_range(0, 198) == 1 {
            self.face = (self.face + 1) % 3;
        }
        if self.post == Post::Results && self.res_y > 140.0 {
            self.res_y += (130.0 - self.res_y) * 0.1;
        }
        if self.post != Post::Playing {
            let ev = self.game.post_tick();
            self.pending.extend(ev);
            self.rank_y += (320.0 - self.rank_y) * 0.1;
            self.summary_x -= 2.0;
        }
        if self.post == Post::Results && self.screen == Screen::Playing {
            let k = self.settings.keys;
            let left = is_key_down(k[0]) || is_key_down(KeyCode::Left);
            let right = is_key_down(k[1]) || is_key_down(KeyCode::Right);
            if let Some(ini) = &mut self.initials {
                if ini.tick(left, right) {
                    let name = ini.name();
                    self.initials = None;
                    self.enter_hisc(&name);
                    self.leave_results();
                }
            } else if self.leave != 20 {
                self.leave -= 1;
                if self.leave <= 0 {
                    self.leave_results();
                }
            }
        }
        if self.screen == Screen::HighScores {
            // view_scores(): Up/Down scroll 16px a tick, the paper eases after the target,
            // the menu behind darkens to 158/255
            let hs = &mut self.hs;
            if !hs.leaving {
                if is_key_down(KeyCode::Up) {
                    hs.target = if hs.target < -15.0 { hs.target + 16.0 } else { 0.0 };
                }
                if is_key_down(KeyCode::Down) && hs.target - 15.0 > 470.0 - hs_paper_height() {
                    hs.target -= 16.0;
                }
            }
            let (target, dim) = if hs.leaving { (520.0, 0.0) } else { (hs.target, 158.0) };
            hs.y += ((target - hs.y) * 0.2).trunc();
            hs.dim += (dim - hs.dim) * 0.2;
            if hs.leaving && hs.y > 480.0 {
                self.screen = Screen::Menu;
            }
        }
        if let Some(t) = &mut self.thanks {
            // show_credits(): up to 3s while the music fades out, then a fade to black
            *t += 1;
            if let Some(m) = &self.music {
                set_sound_volume(m, self.settings.music as f32 / 10.0 * (1.0 - *t as f32 / THANKS_TICKS as f32).max(0.0));
            }
            if *t >= THANKS_TICKS + FADE_TICKS {
                self.quit = true;
            }
        }
    }

    pub fn update(&mut self, a: &Assets, chars: &[Character]) {
        self.ui_acc += get_frame_time().min(0.25) as f64;
        while self.ui_acc >= game::TICK {
            self.ui_acc -= game::TICK;
            self.ui_tick();
        }
        if !self.pending.is_empty() {
            let ev = std::mem::take(&mut self.pending);
            self.play_events(ev, a, &chars[self.settings.character]);
        }
        let typed: Vec<char> = std::iter::from_fn(get_char_pressed).collect();
        if is_quit_requested() {
            // the window's X: thanks screen first; a second click quits right away
            if self.thanks.is_some() {
                self.quit = true;
            }
            self.quit_game();
        }
        if let Some(t) = &mut self.thanks {
            if is_key_pressed(KeyCode::Escape) && *t < THANKS_TICKS {
                *t = THANKS_TICKS;
            }
            return;
        }
        let alt = is_key_down(KeyCode::LeftAlt) || is_key_down(KeyCode::RightAlt);
        if is_key_pressed(KeyCode::F11) || (alt && is_key_pressed(KeyCode::Enter)) {
            self.settings.fullscreen = !self.settings.fullscreen;
            set_fullscreen(self.settings.fullscreen);
            self.settings.save();
            return;
        }
        let pressed = |keys: &[KeyCode]| keys.iter().any(|k| is_key_pressed(*k));
        let confirm = pressed(&[KeyCode::Enter, KeyCode::KpEnter]);
        let back = pressed(&[KeyCode::Escape]);

        if let Some(al) = self.alert.take() {
            if confirm || pressed(&[KeyCode::Space]) {
                sfx(a.menu_select.as_ref(), &self.settings);
                match al.confirm {
                    Some(Confirm::DeleteProfile(name)) => {
                        profile::delete_profile(&name);
                        self.profiles = profile::list_profiles();
                        if self.settings.profile == name {
                            self.use_profile("guest");
                        }
                        self.list_sel = self.list_sel.min(self.profiles.len());
                    }
                    Some(Confirm::DeleteReplay(path)) => {
                        let _ = std::fs::remove_file(path);
                        let sel = self.list_sel;
                        self.load_replays();
                        self.list_sel = sel.min((self.folders.len() + self.replays.len()).saturating_sub(1));
                    }
                    None => {}
                }
            } else if !back {
                self.alert = Some(al);
            }
            return;
        }

        match self.screen {
            Screen::Menu => self.update_menu(a, chars, confirm, back),
            Screen::Instructions => {
                if confirm || back || pressed(&[KeyCode::Space]) {
                    self.screen = Screen::Menu;
                }
            }
            Screen::HighScores => {
                if confirm || back || pressed(&[KeyCode::Space, self.settings.keys[2]]) {
                    self.hs.leaving = true;
                }
            }
            Screen::ViewProfile => {
                if confirm || back || pressed(&[KeyCode::Space]) {
                    self.screen = self.view_back;
                }
            }
            Screen::SelectProfile => {
                let n = self.profiles.len() + 1; // row 0 is CREATE NEW PROFILE
                if pressed(&[KeyCode::Up]) {
                    self.list_sel = (self.list_sel + n - 1) % n;
                    sfx(a.menu_move.as_ref(), &self.settings);
                } else if pressed(&[KeyCode::Down]) {
                    self.list_sel = (self.list_sel + 1) % n;
                    sfx(a.menu_move.as_ref(), &self.settings);
                } else if back {
                    self.screen = Screen::Menu;
                } else if pressed(&[KeyCode::Delete]) && self.list_sel > 0 {
                    let name = self.profiles[self.list_sel - 1].clone();
                    if name != "guest" {
                        self.alert = Some(Alert {
                            title: format!("Really delete '{name}'?"),
                            msg: "WARNING: It will be gone forever.".into(),
                            confirm: Some(Confirm::DeleteProfile(name)),
                        });
                    }
                } else if confirm {
                    sfx(a.menu_select.as_ref(), &self.settings);
                    if self.list_sel == 0 {
                        self.fields[0].clear();
                        self.screen = Screen::NewProfile;
                    } else {
                        let name = self.profiles[self.list_sel - 1].clone();
                        self.use_profile(&name);
                        self.screen = Screen::Menu;
                        self.info("Profile Changed!", &format!("Now using profile '{name}'"));
                    }
                }
            }
            Screen::NewProfile => {
                edit(&mut self.fields[0], &typed, 32);
                if back {
                    self.screen = Screen::SelectProfile;
                } else if confirm {
                    match profile::create_profile(&self.fields[0]) {
                        Ok(p) => {
                            self.profiles = profile::list_profiles();
                            let name = p.name.clone();
                            self.use_profile(&name);
                            self.screen = Screen::Menu;
                            self.info("CREATE PROFILE", "Profile created!");
                        }
                        Err(e) => self.info("CREATE PROFILE", e),
                    }
                }
            }
            Screen::SelectReplay if self.browser.is_some() => {
                let b = self.browser.as_mut().unwrap();
                let n = b.entries.len().max(1);
                if pressed(&[KeyCode::Up]) {
                    b.sel = (b.sel + n - 1) % n;
                } else if pressed(&[KeyCode::Down]) {
                    b.sel = (b.sel + 1) % n;
                } else if pressed(&[KeyCode::Backspace]) {
                    b.enter("..");
                } else if back {
                    self.browser = None;
                } else if confirm {
                    let (name, folder) = b.entries[b.sel].clone();
                    if folder {
                        b.enter(&name);
                    } else {
                        // picking a replay picks its folder
                        let dir = b.dir.clone();
                        self.pick_folder(dir);
                    }
                } else if pressed(&[KeyCode::Space, KeyCode::F1]) {
                    let dir = b.dir.clone();
                    self.pick_folder(dir);
                }
            }
            Screen::SelectReplay => {
                let n = (self.folders.len() + self.replays.len()).max(1);
                let sort = [(KeyCode::N, 1), (KeyCode::S, 2), (KeyCode::C, 3), (KeyCode::F, 4)].into_iter().find(|(k, _)| is_key_pressed(*k));
                let on_folder = self.list_sel < self.folders.len();
                let rep_sel = self.list_sel.checked_sub(self.folders.len());
                if let Some((_, m)) = sort {
                    self.settings.replay_sort = m;
                    self.settings.save();
                    self.sort_replays();
                    sfx(a.menu_select.as_ref(), &self.settings);
                } else if pressed(&[KeyCode::F1]) {
                    sfx(a.menu_select.as_ref(), &self.settings);
                    let dir = self.replay_dir.clone().unwrap_or_else(|| self.profile.replay_dir());
                    self.browser = Some(Browser::open(dir));
                } else if pressed(&[KeyCode::Up]) {
                    self.list_sel = (self.list_sel + n - 1) % n;
                    sfx(a.menu_move.as_ref(), &self.settings);
                } else if pressed(&[KeyCode::Down]) {
                    self.list_sel = (self.list_sel + 1) % n;
                    sfx(a.menu_move.as_ref(), &self.settings);
                } else if back {
                    self.screen = Screen::Menu;
                } else if let (true, Some(i)) = (pressed(&[KeyCode::Delete]), rep_sel) {
                    self.alert = Some(Alert {
                        title: "Really delete replay?".into(),
                        msg: "WARNING: It will be gone forever.".into(),
                        confirm: Some(Confirm::DeleteReplay(self.replays[i].file.clone())),
                    });
                } else if (confirm || pressed(&[KeyCode::Space])) && on_folder {
                    sfx(a.menu_select.as_ref(), &self.settings);
                    let dir = self.replay_dir.clone().unwrap_or_else(|| self.profile.replay_dir());
                    let name = &self.folders[self.list_sel];
                    let next = if name == ".." { dir.parent().map(Path::to_path_buf).unwrap_or(dir) } else { dir.join(name) };
                    self.replay_dir = Some(next);
                    self.load_replays();
                } else if let (true, Some(i)) = (confirm || pressed(&[KeyCode::Space]), rep_sel) {
                    if let Some(rep) = self.replays.get(i).cloned() {
                        sfx(a.menu_select.as_ref(), &self.settings);
                        self.start_watch(a, chars, rep, Screen::SelectReplay);
                    }
                }
            }
            Screen::SaveReplay => {
                let max = [32, 64, 41][self.field];
                edit(&mut self.fields[self.field], &typed, max);
                if back {
                    self.screen = Screen::Playing;
                } else if confirm {
                    if self.field < 2 {
                        self.field += 1;
                    } else if let Some(rep) = &mut self.last_replay {
                        rep.name = self.fields[0].clone();
                        rep.comment = self.fields[2].clone();
                        let ok = rep.save_as(&self.profile.replay_dir(), &self.fields[1]);
                        self.screen = Screen::Playing;
                        self.info("SAVE REPLAY", if ok { "Replay saved." } else { "Could not save replay." });
                    }
                }
            }
            Screen::Playing => self.update_playing(a, chars, confirm, back, &typed),
            Screen::Watching => self.update_watching(a, chars, back),
        }
    }

    fn pick_folder(&mut self, dir: PathBuf) {
        self.browser = None;
        self.replay_dir = Some(dir);
        self.load_replays();
    }

    fn use_profile(&mut self, name: &str) {
        self.replay_dir = None;
        self.profile = profile::load_profile(name);
        self.settings.profile = name.to_string();
        self.settings.best_floor = self.settings.best_floor.max(self.profile.best_floor);
        self.settings.save();
    }

    fn update_menu(&mut self, a: &Assets, chars: &[Character], confirm: bool, back: bool) {
        let pressed = |keys: &[KeyCode]| keys.iter().any(|k| is_key_pressed(*k));
        let items = self.settings.items(self.menu);
        if let Some(i) = self.binding {
            if let Some(k) = get_keys_pressed().into_iter().next() {
                if k != KeyCode::Escape && menu::bindable(k) {
                    self.settings.keys[i] = k;
                    self.settings.save();
                }
                self.binding = None;
            }
            return;
        }
        if pressed(&[KeyCode::Up]) {
            self.sel = (self.sel + items.len() - 1) % items.len();
            sfx(a.menu_move.as_ref(), &self.settings);
            return;
        }
        if pressed(&[KeyCode::Down]) {
            self.sel = (self.sel + 1) % items.len();
            sfx(a.menu_move.as_ref(), &self.settings);
            return;
        }
        if back {
            if let Some(p) = menu::parent(self.menu) {
                self.menu = p;
                self.sel = 0;
                sfx(a.menu_select.as_ref(), &self.settings);
            }
            return;
        }
        let ok = confirm || pressed(&[KeyCode::Space]);
        let dir = if pressed(&[KeyCode::Left]) { -1 } else if pressed(&[KeyCode::Right]) || ok { 1 } else { 0 };
        match items[self.sel].action {
            Action::Change if dir != 0 => {
                self.settings.change(self.menu, self.sel, dir);
                set_fullscreen(self.settings.fullscreen);
                set_hd(self.settings.hd);
                if let Some(m) = &self.music {
                    set_sound_volume(m, self.settings.music as f32 / 10.0);
                }
                self.settings.save();
                sfx(a.menu_move.as_ref(), &self.settings);
            }
            action if ok => {
                sfx(a.menu_select.as_ref(), &self.settings);
                    match action {
                    Action::Open(m) => {
                        self.menu = m;
                        self.sel = 0;
                    }
                    Action::Back => {
                        self.menu = menu::parent(self.menu).unwrap_or(MenuId::Main);
                        self.sel = 0;
                    }
                    Action::Classic => self.start_game(a, chars, false),
                    Action::Custom => self.start_game(a, chars, true),
                    Action::Instructions => self.screen = Screen::Instructions,
                    Action::HighScores => {
                        self.hs = HsView { y: 500.0, target: 0.0, dim: 0.0, leaving: false };
                        self.screen = Screen::HighScores;
                    }
                    Action::Bind(i) => self.binding = Some(i),
                    Action::ViewProfile => {
                        self.view_back = Screen::Menu;
                        self.screen = Screen::ViewProfile;
                    }
                    Action::ChangeProfile => {
                        self.profiles = profile::list_profiles();
                        self.list_sel = 1 + self.profiles.iter().position(|p| *p == self.profile.name).unwrap_or(0);
                        self.screen = Screen::SelectProfile;
                    }
                    Action::LoadReplay => {
                        self.browser = None;
                        self.load_replays();
                        self.screen = Screen::SelectReplay;
                    }
                    Action::Exit => self.quit_game(),
                    Action::PlayAgain | Action::WatchReplay | Action::SaveReplay | Action::MainMenu | Action::Change => {}
                }
            }
            _ => {}
        }
    }

    fn update_playing(&mut self, a: &Assets, chars: &[Character], confirm: bool, back: bool, typed: &[char]) {
        let pressed = |keys: &[KeyCode]| keys.iter().any(|k| is_key_pressed(*k));
        let ch = &chars[self.settings.character];
        if self.post == Post::Results {
            let fire = pressed(&[self.settings.keys[2], KeyCode::Enter, KeyCode::KpEnter]);
            if let Some(ini) = &mut self.initials {
                for &c in typed {
                    ini.typed(c);
                }
                if pressed(&[KeyCode::Backspace, KeyCode::Delete]) {
                    ini.erase();
                }
                if fire {
                    ini.fire();
                }
            } else if self.leave == 20 && (get_last_key_pressed().is_some() || fire || back) {
                // any key: the results leave 14 ticks later
                self.leave = 14;
            }
            return;
        }
        if self.post == Post::Unlock {
            if pressed(&[KeyCode::Escape, KeyCode::Enter, KeyCode::KpEnter, KeyCode::Space]) {
                self.post = Post::Menu;
            }
            return;
        }
        if self.post == Post::Menu {
            // the original's replay_menu over the game-over screen
            let items = self.settings.items(MenuId::GameOver);
            if pressed(&[KeyCode::Up]) {
                self.sel = (self.sel + items.len() - 1) % items.len();
                sfx(a.menu_move.as_ref(), &self.settings);
            } else if pressed(&[KeyCode::Down]) {
                self.sel = (self.sel + 1) % items.len();
                sfx(a.menu_move.as_ref(), &self.settings);
            } else if back {
                self.to_main_menu(a);
            } else if confirm || pressed(&[KeyCode::Space]) {
                sfx(a.menu_select.as_ref(), &self.settings);
                match items[self.sel].action {
                    Action::PlayAgain => self.start_game(a, chars, self.custom),
                    Action::WatchReplay => {
                        if let Some(rep) = self.last_replay.clone() {
                            self.start_watch(a, chars, rep, Screen::Playing);
                        }
                    }
                    Action::SaveReplay => {
                        if let Some(rep) = &self.last_replay {
                            self.fields = [
                                self.profile.name.clone(),
                                format!("{}_{}_{}_{}", self.profile.name, rep.score, rep.floor, rep.combo),
                                String::new(),
                            ];
                            self.field = 0;
                            self.screen = Screen::SaveReplay;
                        }
                    }
                    Action::ViewProfile => {
                        self.view_back = Screen::Playing;
                        self.screen = Screen::ViewProfile;
                    }
                    _ => self.to_main_menu(a),
                }
            }
            return;
        }
        if !self.game.game_over() && (is_key_pressed(self.settings.keys[3]) || back) {
            // Esc pauses; Esc again while paused quits ("DO YOU REALLY WANT TO EXIT?")
            if self.game.paused && back {
                if self.game.p.dead == 0 {
                    self.profile.games_quit += 1;
                    self.profile.save();
                }
                self.to_main_menu(a);
                return;
            }
            self.game.paused = !self.game.paused;
            if self.game.paused {
                sfx(ch.sound(4), &self.settings);
            }
        }
        let [l, r, j, _] = self.settings.keys;
        let (left, right, jump) = (is_key_down(l), is_key_down(r), is_key_down(j));
        self.acc += get_frame_time().min(0.25) as f64;
        while self.acc >= game::TICK {
            self.acc -= game::TICK;
            if !self.game.paused && !self.game.game_over() {
                let flags = if left { LEFT } else { 0 } | if right { RIGHT } else { 0 } | if jump { JUMP } else { 0 };
                self.recorder.record(&mut self.replay, flags, self.game.p.dead != 0);
            }
            self.prev = self.prev.step(&self.game);
            let ev = self.game.tick(left, right, jump);
            self.play_events(ev, a, ch);
        }
        if self.game.game_over() && self.post == Post::Playing {
            self.finish_game(a);
        }
    }

    fn update_watching(&mut self, a: &Assets, chars: &[Character], back: bool) {
        let Some(w) = &mut self.watch else { return };
        if is_key_pressed(KeyCode::Space) {
            w.paused = !w.paused;
        }
        if back || w.game.game_over() {
            let to = w.back;
            self.watch = None;
            if to == Screen::Playing {
                self.screen = Screen::Playing;
                switch_music(&mut self.music, None, &self.settings);
            } else {
                self.screen = to;
                self.menu_music(a);
            }
            return;
        }
        // VCR: hold Right = fast forward, hold Up = very fast
        let speed = if is_key_down(KeyCode::Up) { 32 } else if is_key_down(KeyCode::Right) { 4 } else { 1 };
        let ch = &chars[w.rep.character.min(chars.len() - 1)];
        self.acc += get_frame_time().min(0.25) as f64;
        let mut events = Vec::new();
        while self.acc >= game::TICK {
            self.acc -= game::TICK;
            if w.paused {
                self.prev = Prev::of(&w.game);
                continue;
            }
            for _ in 0..speed {
                self.prev = self.prev.step(&w.game);
                let ev = w.game.tick_flags(w.input.next());
                if speed == 1 {
                    events.extend(ev);
                }
            }
        }
        self.play_events(events, a, ch);
    }

    // ------------------------------------------------------------------ drawing

    pub fn draw(&mut self, a: &Assets, chars: &[Character]) {
        match self.screen {
            Screen::Menu => self.draw_title(a),
            Screen::Instructions => draw_texture(&a.instructions, 0.0, 0.0, WHITE),
            Screen::HighScores => {
                self.draw_title(a);
                let hs = HsView {
                    y: self.ui_blend(self.ui_prev.hs_y, self.hs.y),
                    dim: self.ui_blend(self.ui_prev.hs_dim, self.hs.dim),
                    ..self.hs
                };
                draw_highscores(a, &self.hisc, &hs);
            }
            Screen::ViewProfile => {
                self.draw_back(a, chars);
                dim(0.5);
                draw_profile_page(a, &self.profile);
            }
            Screen::SelectProfile | Screen::NewProfile => {
                self.draw_title_back(a);
                dim(0.4);
                self.draw_profile_selector(a);
                if self.screen == Screen::NewProfile {
                    dim(0.6);
                    draw_texture(&a.replay_bg_sml, 100.0, 140.0, WHITE);
                    a.small.draw("Enter profile name:", 140.0, 149.0, INK);
                    input_box(a, &self.fields[0], 139.0, 191.0, 341.0, true);
                    let hint = "...and press enter.";
                    a.small.draw(hint, 480.0 - a.small.width(hint), 212.0, INK);
                }
            }
            Screen::SelectReplay => {
                self.draw_title_back(a);
                dim(0.4);
                self.draw_replay_selector(a);
                if let Some(b) = &self.browser {
                    draw_browser(a, b);
                }
            }
            Screen::SaveReplay => {
                draw(&self.game, a, &chars[self.settings.character], Prev::of(&self.game), 1.0);
                dim(0.6);
                draw_texture_ex(&a.replay_bg, 120.0, 140.0, WHITE, DrawTextureParams { dest_size: Some(vec2(380.0, 200.0)), ..Default::default() });
                a.black.draw_scaled("SAVE REPLAY", 140.0, 150.0, WHITE, 0.75);
                for (i, label) in ["Your name:", "Filename:", "Comment: (optional)"].iter().enumerate() {
                    let y = 210.0 + i as f32 * 40.0;
                    a.small.draw(label, 140.0, y - 18.0, INK);
                    input_box(a, &self.fields[i], 140.0, y, 340.0, i == self.field);
                }
                a.small.draw_centre("(enter to advance)", 320.0, 318.0, Color::from_rgba(80, 80, 80, 255));
            }
            Screen::Playing if self.post == Post::Unlock => draw_unlock(a),
            Screen::Playing => {
                draw(&self.game, a, &chars[self.settings.character], self.prev, (self.acc / game::TICK) as f32);
                if self.game.paused {
                    dim(0.5);
                    a.big.draw_centre("Game Paused", 320.0, 150.0, WHITE);
                    a.med.draw_centre("Press pause to resume", 320.0, 210.0, WHITE);
                    a.med.draw_centre("Press ESC to exit", 320.0, 240.0, WHITE);
                }
                if self.post != Post::Playing {
                    shaken(&self.game, || self.draw_game_over(a));
                }
            }
            Screen::Watching => {
                if let Some(w) = &self.watch {
                    let t = if w.paused { 1.0 } else { (self.acc / game::TICK) as f32 };
                    draw(&w.game, a, &chars[w.rep.character.min(chars.len() - 1)], self.prev, t);
                    draw_vcr(a, w);
                }
            }
        }
        if let Some(al) = &self.alert {
            draw_alert(a, al);
        }
        if let Some(t) = self.thanks {
            draw_thanks(a, t as f32 + self.ui_frac());
        }
    }

    /// Whatever screen sits behind the profile view.
    fn draw_back(&mut self, a: &Assets, chars: &[Character]) {
        if self.view_back == Screen::Playing {
            draw(&self.game, a, &chars[self.settings.character], Prev::of(&self.game), 1.0);
        } else {
            self.draw_title_back(a);
        }
    }

    fn draw_title_back(&self, a: &Assets) {
        draw_texture(&a.title_bg, 0.0, 0.0, WHITE);
        draw_texture(&a.title, 0.0, 0.0, WHITE);
        self.draw_face(a);
    }

    /// How far the 50Hz menu timer is into its next tick (0..1).
    fn ui_frac(&self) -> f32 {
        (self.ui_acc / game::TICK) as f32
    }

    /// A menu-timer value drawn between its last two ticks.
    fn ui_blend(&self, from: f32, to: f32) -> f32 {
        blend(from, to, 1.0 - self.ui_frac(), 128.0)
    }

    /// Menu timer ticks, fractional.
    fn ui_time(&self) -> f32 {
        self.t as f32 + self.ui_frac()
    }

    fn draw_face(&self, a: &Assets) {
        // main_menu_callback(): the head bobs 10px and tilts (Allegro angles, 256 per turn);
        // the shadow sits 2px right and 4px down at 110/255 opacity
        let deg = |n: f32| (n * 360.0 / 256.0).to_radians();
        let t = self.ui_time();
        let bob = deg(t * 5.0).sin() * 10.0;
        let rot = deg(t * 3.0).sin() * 5.0 * std::f32::consts::TAU / 256.0;
        let params = DrawTextureParams { rotation: rot, ..Default::default() };
        draw_texture_ex(&a.face_shadow, 402.0, 16.0 + bob, Color::new(1.0, 1.0, 1.0, 110.0 / 255.0), params.clone());
        draw_texture_ex(&a.faces[self.face], 400.0, 12.0 + bob, WHITE, params);
    }

    fn draw_title(&self, a: &Assets) {
        self.draw_title_back(a);
        draw_texture(&a.menu_bg, 330.0, 280.0, WHITE);
        draw_menu_items(a, &self.settings, self.menu, self.sel, self.binding, MENU_X, MENU_Y);

        // version and welcome, top corners
        a.small.draw("v1.5.1", 5.0, 3.0, Color::from_rgba(100, 21, 20, 255));
        a.small.draw("v1.5.1", 4.0, 2.0, Color::from_rgba(162, 90, 51, 255));
        let rank = self.profile.rank_id();
        let welcome = if rank > 0 {
            format!("Welcome, {} ({})!", self.profile.name, profile::RANK_NAMES[rank])
        } else {
            format!("Welcome, {}!", self.profile.name)
        };
        let wx = 636.0 - a.small.width(&welcome);
        a.small.draw(&welcome, wx + 2.0, 3.0, Color::from_rgba(50, 50, 50, 255));
        a.small.draw(&welcome, wx, 2.0, WHITE);

        // greeting scroller in a translucent bar along the bottom
        draw_rectangle(0.0, 462.0, W, 18.0, Color::new(0.0, 0.0, 0.0, 0.6));
        let len = a.small.width(GREETING);
        let x = W - (self.ui_time() % (len + W));
        a.small.draw(GREETING, x + 1.0, 462.0, Color::from_rgba(150, 150, 150, 255));
        a.small.draw(GREETING, x, 462.0, Color::from_rgba(200, 200, 200, 255));
    }

    fn draw_game_over(&self, a: &Assets) {
        // draw_results(): GAME OVER, then Score / Floor / Best Combo with personal-best icons
        let g = &self.game;
        let res_y = self.ui_blend(self.ui_prev.res_y, self.res_y as f32);
        let rank_y = self.ui_blend(self.ui_prev.rank_y, self.rank_y as f32);
        let y = res_y;
        draw_texture(&a.gameover, 320.0 - (a.gameover.width() / 2.0).floor(), y, WHITE);
        let rows = [("Score", g.final_score(), self.bests.score), ("Floor", g.p.level, self.bests.floor), ("Best Combo", g.p.best_combo, self.bests.combo)];
        let counts = !self.custom && self.profile.name != "guest";
        for (i, (name, value, best)) in rows.into_iter().enumerate() {
            let ry = y + a.gameover.height() + 3.0 + i as f32 * 30.0;
            a.med.draw(&format!("{name}:"), 200.0, ry, WHITE);
            let v = value.to_string();
            a.med.draw(&v, 440.0 - a.med.width(&v), ry, WHITE);
            if best && counts {
                draw_texture(&a.icon_pb, 476.0, ry + 10.0, WHITE);
            }
        }
        if let Some(ini) = &self.initials {
            // "Enter your initials" under the results; the current letter blinks
            let base = 2.0 * res_y;
            a.med.draw_centre("Enter your initials", 320.0, base + 80.0, WHITE);
            let blink = ini.blink & 4 != 0;
            for (i, &c) in ini.slots.iter().enumerate() {
                if i != ini.cur || blink {
                    a.med.draw_centre(&(c as char).to_string(), 300.0 + i as f32 * 20.0, base + 120.0, WHITE);
                }
            }
            if ini.cur == 3 && blink {
                a.med.draw_centre("%", 360.0, base + 120.0, WHITE);
            }
        }
        let rank = self.profile.rank_id();
        if counts && rank > self.rank_before {
            draw_texture(&a.ranks[rank], 20.0, rank_y, WHITE);
            a.med.draw("rank up!", 20.0, rank_y + 70.0, WHITE);
        }
        if self.post == Post::Menu {
            // replay_menu_callback() blacks out every other row and column; 1px lines turn into
            // moire once the window scales them, so dim evenly instead (DIM_MENU = darkness)
            dim(DIM_MENU);
            draw_texture(&a.replay_bg_med, 120.0, 140.0, WHITE);
            draw_menu_items(a, &self.settings, MenuId::GameOver, self.sel, None, 180.0, 160.0);
        }
        // summary scroller across a translucent bar at the top
        for h in [21.0, 19.0, 17.0] {
            draw_rectangle(0.0, 0.0, W, h, Color::new(0.0, 0.0, 0.0, 110.0 / 255.0));
        }
        let len = a.small.width(&self.summary);
        let sx = self.ui_blend(self.ui_prev.summary_x, self.summary_x);
        let x = if sx < -len { W - ((-len - sx) % (len + W)) } else { sx };
        a.small.draw(&self.summary, x + 1.0, 0.0, Color::from_rgba(150, 150, 150, 255));
        a.small.draw(&self.summary, x, 0.0, Color::from_rgba(200, 200, 200, 255));
    }

    fn draw_profile_selector(&self, a: &Assets) {
        let (wx, wy) = WINDOW;
        draw_texture(&a.replay_bg, wx, wy, WHITE);
        a.black.draw_scaled("SELECT PROFILE", wx + 30.0, wy + 18.0, WHITE, 0.75);
        let rows: Vec<String> = std::iter::once("CREATE NEW PROFILE".to_string())
            .chain(self.profiles.iter().map(|n| if *n == self.profile.name { format!("{n} (current)") } else { n.clone() }))
            .collect();
        list_rows(a, &rows, self.list_sel, wx + 30.0, wy + 70.0, 300.0, 16);
        draw_texture(&a.profile_buttons, wx + 320.0, wy + 230.0, WHITE);
    }

    fn draw_replay_selector(&self, a: &Assets) {
        let (wx, wy) = WINDOW;
        draw_texture(&a.replay_bg, wx, wy, WHITE);
        a.black.draw_scaled("SELECT REPLAY", wx + 30.0, wy + 18.0, WHITE, 0.75);
        let cols = [(wx + 40.0, "NAME", false), (wx + 210.0, "SCORE", true), (wx + 265.0, "FLOOR", true), (wx + 325.0, "COMBO", true), (wx + 340.0, "DATE", false)];
        for (x, t, right) in cols {
            a.small.draw(t, if right { x - a.small.width(t) } else { x }, wy + 62.0, INK);
        }
        let nf = self.folders.len();
        if self.replays.is_empty() && nf <= 1 {
            a.small.draw("No replays here. Play a game first!", wx + 40.0, wy + 84.0 + nf as f32 * 18.0, INK);
        }
        let top = self.list_sel.saturating_sub(13);
        for (i, f) in self.folders.iter().enumerate().skip(top).take(14) {
            let y = wy + 84.0 + (i - top) as f32 * 18.0;
            if i == self.list_sel {
                draw_rectangle(wx + 26.0, y, 300.0, 18.0, Color::new(0.0, 0.0, 0.0, 0.2));
                a.small.draw(">", wx + 30.0, y, INK);
            }
            let label = if f == ".." { ".. (parent directory)".to_string() } else { format!("[{f}]") };
            a.small.draw(&label, wx + 40.0, y, INK);
        }
        let first = top.max(nf);
        for (i, r) in self.replays.iter().enumerate().map(|(i, r)| (i + nf, r)).skip(first - nf).take((top + 14).saturating_sub(first)) {
            let y = wy + 84.0 + (i - top) as f32 * 18.0;
            if i == self.list_sel {
                draw_rectangle(wx + 26.0, y, 300.0, 18.0, Color::new(0.0, 0.0, 0.0, 0.2));
                a.small.draw(">", wx + 30.0, y, INK);
            }
            let stem = r.file.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
            let name: String = stem.chars().take(17).collect();
            a.small.draw(&name, wx + 40.0, y, INK);
            if r.custom {
                a.small.draw("C", wx + 140.0, y, Color::from_rgba(80, 20, 20, 255));
            }
            for (v, x) in [(r.score, wx + 210.0), (r.floor, wx + 265.0), (r.combo, wx + 325.0)] {
                let s = v.to_string();
                a.small.draw(&s, x - a.small.width(&s), y, INK);
            }
            a.small.draw(r.date.get(2..).unwrap_or(""), wx + 340.0, y, INK);
        }
        if self.list_sel < nf {
            a.small.draw("This is a folder. Press ENTER to open it.", wx + 30.0, wy + 352.0, INK);
        } else if let Some(r) = self.replays.get(self.list_sel - nf) {
            let mut info = format!("{}  ({})", r.name, profile_char_name(r.character));
            if r.custom {
                info += "  CUSTOM GAME";
            }
            a.small.draw(&info, wx + 30.0, wy + 352.0, INK);
            if !r.comment.is_empty() {
                a.small.draw(&r.comment, wx + 30.0, wy + 370.0, INK);
            }
        }
        // draw_replay_selector(): buttons and the sort panel (N/S/C/F) at the original offsets
        let (ox, oy) = (wx + 15.0, wy + 15.0);
        draw_texture(&a.replay_buttons, ox + 310.0, oy + 34.0, WHITE);
        draw_texture(&a.sort_buttons[0], ox + 311.0, oy + 260.0, WHITE);
        let m = self.settings.replay_sort.clamp(1, 4) as usize;
        let (sx, sy) = [(321.0, 274.0), (355.0, 274.0), (355.0, 286.0), (321.0, 286.0)][m - 1];
        draw_texture(&a.sort_buttons[m], ox + sx, oy + sy, WHITE);
        if let Some(d) = &self.replay_dir {
            let d = d.display().to_string();
            let d: String = d.chars().rev().take(48).collect::<Vec<_>>().into_iter().rev().collect();
            a.small.draw(&d, wx + 30.0, wy + 40.0, Color::from_rgba(80, 80, 80, 255));
        }
    }
}

fn profile_char_name(i: usize) -> &'static str {
    menu::CHARACTER_NAMES.get(i).copied().unwrap_or("?")
}

/// Typing into a text field: printable ASCII, backspace deletes.
fn edit(s: &mut String, typed: &[char], max: usize) {
    for &c in typed {
        if (' '..='~').contains(&c) && s.len() < max {
            s.push(c);
        }
    }
    if is_key_pressed(KeyCode::Backspace) {
        s.pop();
    }
}

fn dim(alpha: f32) {
    draw_rectangle(0.0, 0.0, W, H, Color::new(0.0, 0.0, 0.0, alpha));
}

fn input_box(a: &Assets, text: &str, x: f32, y: f32, w: f32, active: bool) {
    draw_rectangle(x, y, w, 19.0, WHITE);
    draw_rectangle_lines(x, y, w, 19.0, 1.0, Color::from_rgba(80, 80, 80, 255));
    let caret = if active && (get_time() * 2.0) as i64 % 2 == 0 { "_" } else { "" };
    a.small.draw(&format!("{text}{caret}"), x + 3.0, y, BLACK);
}

fn list_rows(a: &Assets, rows: &[String], sel: usize, x: f32, y: f32, w: f32, visible: usize) {
    let top = sel.saturating_sub(visible - 1);
    for (i, r) in rows.iter().enumerate().skip(top).take(visible) {
        let ry = y + (i - top) as f32 * 18.0;
        if i == sel {
            draw_rectangle(x - 4.0, ry, w, 18.0, Color::new(0.0, 0.0, 0.0, 0.2));
            a.small.draw(">", x, ry, INK);
        }
        a.small.draw(r, x + 14.0, ry, INK);
    }
}

pub fn draw_menu_items(a: &Assets, settings: &Settings, menu: MenuId, sel: usize, binding: Option<usize>, x: f32, y: f32) {
    for (i, item) in settings.items(menu).iter().enumerate() {
        let label = match (binding, item.action) {
            (Some(b), Action::Bind(k)) if b == k => format!("{}: press a key", item.label.split(':').next().unwrap_or("")),
            _ => item.label.clone(),
        };
        let iy = y + i as f32 * MENU_LINE;
        // long option labels shrink to stay on the paper
        let scale = ((W - x - 6.0) / a.black.width(&label)).min(1.0);
        a.black.draw_scaled(&label, x, iy + (1.0 - scale) * 12.0, WHITE, scale);
        if i == sel {
            draw_texture(&a.bullet, x - 42.0, iy + 4.0, WHITE);
        }
    }
}

fn draw_profile_page(a: &Assets, p: &Profile) {
    // view_profile(): REPLAY_BG with general info on top, two stat columns, then rank
    let (wx, wy) = WINDOW;
    draw_texture(&a.replay_bg, wx, wy, WHITE);
    a.black.draw_scaled(&format!("profile: {}", p.name), wx + 25.0, wy + 12.0, WHITE, 0.7);
    let mono = |text: &str, x: f32, y: f32| text_block(&a.mono, text, x, y);
    let y = mono(&p.page_general(), wx + 25.0, wy + 50.0);
    let sep = "-------------------------------------------------------------";
    a.mono.draw(sep, wx + 25.0, y, INK);
    let y1 = mono(&p.page_basic(), wx + 25.0, y + 16.0);
    let y2 = mono(&p.page_advanced(), wx + 230.0, y + 16.0);
    let y = y1.max(y2) - 4.0;
    let rank = p.rank_id();
    draw_texture_ex(&a.ranks[rank], wx + 25.0, y, WHITE, DrawTextureParams { dest_size: Some(vec2(72.0, 72.0)), ..Default::default() });
    let msg = format!("Rank: {}\nTo reach next rank:{}", profile::RANK_NAMES[rank], p.next_rank_message());
    mono(&msg, wx + 110.0, y + 6.0);
    a.small.draw_centre("Press ENTER to go back", 320.0, 462.0, WHITE);
}

/// Draws multi-line text, returns the y below it.
fn text_block(font: &BmpFont, text: &str, x: f32, mut y: f32) -> f32 {
    for line in text.lines() {
        font.draw(line, x, y, INK);
        y += 12.0;
    }
    y
}

fn draw_vcr(a: &Assets, w: &Watch) {
    let (x, y) = (251.0, 412.0);
    draw_texture(&a.vcr, x, y, WHITE);
    let lit = |dx: f32, t: &Tex| draw_texture(t, x + dx, y + 44.0, WHITE);
    if w.paused {
        lit(39.0, &a.vcr_up);
    } else if is_key_down(KeyCode::Up) {
        lit(106.0, &a.vcr_up);
    } else if is_key_down(KeyCode::Right) {
        lit(83.0, &a.vcr_right);
    } else {
        lit(62.0, &a.vcr_right);
    }
    a.small.draw_centre(&format!("replay: {}", w.rep.name), 320.0, 2.0, WHITE);
}

fn draw_alert(a: &Assets, al: &Alert) {
    dim(0.4);
    let (x, y) = (130.0, 155.0);
    draw_texture(&a.replay_bg_med, x, y, WHITE);
    let scale = (340.0 / a.black.width(&al.title)).min(0.75);
    a.black.draw_scaled(&al.title, 320.0 - a.black.width(&al.title) * scale / 2.0, y + 20.0, WHITE, scale);
    let mut ly = y + 70.0;
    for line in al.msg.lines() {
        a.small.draw_centre(line, 320.0, ly, INK);
        ly += 18.0;
    }
    if al.confirm.is_some() {
        draw_texture(&a.bt_yes, 250.0, y + 115.0, WHITE);
        draw_texture(&a.bt_no, 350.0, y + 115.0, WHITE);
        a.small.draw("Enter", 288.0, y + 123.0, INK);
        a.small.draw("Esc", 393.0, y + 123.0, INK);
    } else {
        draw_texture(&a.bt_yes_2, 303.0, y + 115.0, WHITE);
    }
}

const THANKS_TICKS: i32 = 150; // show_credits() stays up to 3s (ESC skips)
const FADE_TICKS: i32 = 8;

/// show_credits(): the title, "Thanks for playing!" and the credits, faded in and out.
fn draw_thanks(a: &Assets, t: f32) {
    draw_texture(&a.title_bg, 0.0, 0.0, WHITE);
    draw_texture(&a.title, 320.0 - (a.title.width() / 2.0).floor(), 10.0, WHITE);
    a.big.draw_centre("Thanks for playing!", 320.0, 280.0, WHITE);
    a.med.draw_centre("DESIGN & CODING: Johan Peitz", 320.0, 360.0, WHITE);
    a.med.draw_centre("GRAPHICS: Emanuel Garnheim", 320.0, 390.0, WHITE);
    let (fade, thanks) = (FADE_TICKS as f32, THANKS_TICKS as f32);
    let black = if t < fade { 1.0 - t / fade } else { ((t - thanks) / fade).clamp(0.0, 1.0) };
    dim(black);
}

/// play(): "A new start floor has been unlocked!" over the darkened title background.
fn draw_unlock(a: &Assets) {
    draw_texture(&a.title_bg, 0.0, 0.0, WHITE);
    dim(158.0 / 255.0);
    draw_texture(&a.faces[0], 320.0 - (a.faces[0].width() / 2.0).floor(), 20.0, WHITE);
    a.med.draw_centre("A new start floor", 320.0, 300.0, WHITE);
    a.med.draw_centre("has been unlocked!", 320.0, 350.0, WHITE);
    a.med.draw_centre("(Get it in the options menu)", 320.0, 440.0, WHITE);
}

/// Allegro's file selector in the default GUI colours (black on white).
fn draw_browser(a: &Assets, b: &Browser) {
    let (x, y, w, h) = (120.0, 40.0, 400.0, 400.0);
    draw_rectangle(x, y, w, h, WHITE);
    draw_rectangle_lines(x, y, w, h, 2.0, BLACK);
    a.small.draw_centre("Select a new folder and press OK.", x + w / 2.0, y + 8.0, BLACK);
    let path = b.dir.display().to_string();
    let path: String = path.chars().rev().take(52).collect::<Vec<_>>().into_iter().rev().collect();
    draw_rectangle_lines(x + 16.0, y + 32.0, w - 32.0, 20.0, 1.0, BLACK);
    a.small.draw(&path, x + 20.0, y + 33.0, BLACK);
    let (lx, ly, lh) = (x + 16.0, y + 62.0, 18.0);
    draw_rectangle_lines(lx, ly, w - 32.0, 15.0 * lh + 4.0, 1.0, BLACK);
    let top = b.sel.saturating_sub(14);
    for (i, (name, folder)) in b.entries.iter().enumerate().skip(top).take(15) {
        let ry = ly + 2.0 + (i - top) as f32 * lh;
        let label = if *folder && name != ".." { format!("{name}/") } else { name.clone() };
        let label: String = label.chars().take(50).collect();
        if i == b.sel {
            draw_rectangle(lx + 2.0, ry, w - 36.0, lh, BLACK);
            a.small.draw(&label, lx + 4.0, ry, WHITE);
        } else {
            a.small.draw(&label, lx + 4.0, ry, BLACK);
        }
    }
    for (bx, label) in [(x + 80.0, "OK (space)"), (x + 220.0, "Cancel (esc)")] {
        draw_rectangle_lines(bx, y + h - 40.0, 100.0, 24.0, 1.0, BLACK);
        a.small.draw_centre(label, bx + 50.0, y + h - 37.0, BLACK);
    }
}

/// HISC_TOP + HISC_MID * n + HISC_BTM, tall enough for the 15 tables (93px each from y=80).
fn hs_paper_mids() -> usize {
    ((80.0 + 15.0 * 93.0 - 164.0 - 98.0) / 101.0f32).ceil().max(2.0) as usize
}

fn hs_paper_height() -> f32 {
    164.0 + hs_paper_mids() as f32 * 101.0 + 98.0
}

/// view_scores() / draw_table(): table names in the black font, posts in the mono font.
fn draw_highscores(a: &Assets, t: &Tables, hs: &HsView) {
    dim(hs.dim / 255.0);
    let (x, mut y) = (160.0, hs.y);
    draw_texture(&a.hisc[0], x, y, WHITE);
    y += a.hisc[0].height();
    for _ in 0..hs_paper_mids() {
        draw_texture(&a.hisc[1], x, y, WHITE);
        y += a.hisc[1].height();
    }
    draw_texture(&a.hisc[2], x, y, WHITE);
    for (i, table) in t.0.iter().enumerate() {
        let ty = hs.y + 80.0 + i as f32 * 93.0;
        if !(-100.0..=500.0).contains(&ty) {
            continue;
        }
        a.black.draw(hisc::NAMES[i], x + 40.0, ty - 20.0, WHITE);
        for (k, (name, v)) in table.iter().enumerate() {
            let ry = ty + 15.0 + k as f32 * 12.0;
            let rank = format!("{}.", k + 1);
            a.mono.draw(&rank, x + 60.0 - a.mono.width(&rank), ry, INK);
            a.mono.draw(name, x + 65.0, ry, INK);
            let v = v.to_string();
            a.mono.draw(&v, x + 260.0 - a.mono.width(&v), ry, INK);
        }
    }
    draw_texture(&a.bt_esc, 658.0 - hs.dim, 40.0, WHITE);
    draw_texture(&a.bt_up_dn, 658.0 - hs.dim, 380.0, WHITE);
}

