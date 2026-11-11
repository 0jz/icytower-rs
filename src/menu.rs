// Menus and settings, laid out like the original's menu tables (strings from icytower15.exe).
use crate::game::Rules;
use macroquad::prelude::KeyCode;
use std::path::PathBuf;

pub const CHARACTER_NAMES: [&str; 4] = ["Harold", "Disco Dave", "Jungle Jane", "Wild Wendy"];
const SPEEDS: [&str; 6] = ["Normal", "Hasty", "Fast", "Faster", "Fastest", "Insane"];
const FLOOR_SIZES: [&str; 5] = ["Wide", "Normal", "Shorter", "Shortest", "Tiny"];
const GRAVITIES: [&str; 3] = ["Helium", "Normal", "Heavy"];
const EYE_CANDY: [&str; 3] = ["Lots", "Some", "None"];
const FLOOR_STYLES: usize = 10; // start floor 0..900

// keys that can be bound in Controls, saved by name
const BINDABLE: [KeyCode; 58] = {
    use KeyCode::*;
    [
        A, B, C, D, E, F, G, H, I, J, K, L, M, N, O, P, Q, R, S, T, U, V, W, X, Y, Z, Key0, Key1, Key2, Key3,
        Key4, Key5, Key6, Key7, Key8, Key9, Space, Up, Down, Left, Right, Enter, Tab, LeftShift, RightShift,
        LeftControl, RightControl, LeftAlt, RightAlt, Kp0, Kp1, Kp2, Kp4, Kp5, Kp6, Kp8, Backspace, Pause,
    ]
};

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum MenuId {
    Main,
    Play,
    Custom,
    Options,
    Gfx,
    Sound,
    Controls,
    Profile,
    GameOver, // the original's replay_menu shown after a game
}

/// What selecting an item does.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Action {
    Open(MenuId),
    Back,
    Classic,
    Custom,
    Instructions,
    HighScores,
    Exit,
    Change, // left/right or enter cycles a value
    Bind(usize),
    ViewProfile,
    ChangeProfile,
    LoadReplay,
    PlayAgain,
    WatchReplay,
    SaveReplay,
    MainMenu,
}

pub struct Item {
    pub label: String,
    pub action: Action,
}

pub struct Settings {
    pub character: usize,
    pub start_floor: usize,
    pub eye_candy: usize,
    pub fullscreen: bool,
    pub hd: bool, // upscaled sprites from assets_hd/ when present
    pub sound: i32, // 0..10
    pub music: i32,
    pub keys: [KeyCode; 4], // left, right, jump, pause
    pub rejump: bool,
    pub speed: usize,
    pub floors: usize,
    pub gravity: usize,
    pub best_floor: i32,
    pub replay_sort: u8, // replay list: 1 name, 2 score, 3 combo, 4 floor (N/S/C/F keys)
    pub profile: String,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            character: 0,
            start_floor: 0,
            eye_candy: 0,
            fullscreen: false,
            hd: true,
            sound: 8,
            music: 6,
            keys: [KeyCode::A, KeyCode::D, KeyCode::W, KeyCode::P],
            rejump: true,
            speed: 0,
            floors: 1,
            gravity: 1,
            best_floor: 0,
            replay_sort: 1,
            profile: String::new(), // empty until the first profile is chosen
        }
    }
}

fn key_name(k: KeyCode) -> String {
    format!("{k:?}")
}

fn key_from(name: &str) -> Option<KeyCode> {
    BINDABLE.iter().copied().find(|k| key_name(*k) == name)
}

pub fn bindable(k: KeyCode) -> bool {
    BINDABLE.contains(&k)
}

fn cycle(v: usize, n: usize, dir: i32) -> usize {
    (v as i32 + dir).rem_euclid(n as i32) as usize
}

impl Settings {
    fn path() -> PathBuf {
        let dir = std::env::current_exe().ok().and_then(|p| p.parent().map(|d| d.to_path_buf())).unwrap_or_default();
        dir.join("icytower.cfg")
    }

    pub fn load() -> Self {
        let mut s = Settings::default();
        let Ok(text) = std::fs::read_to_string(Self::path()) else { return s };
        for line in text.lines() {
            let Some((k, v)) = line.split_once('=') else { continue };
            let n = v.parse::<i32>().unwrap_or(0);
            match k {
                "character" => s.character = (n as usize).min(CHARACTER_NAMES.len() - 1),
                "start_floor" => s.start_floor = (n as usize).min(FLOOR_STYLES - 1),
                "eye_candy" => s.eye_candy = (n as usize).min(2),
                "fullscreen" => s.fullscreen = n != 0,
                "hd" => s.hd = n != 0,
                "sound" => s.sound = n.clamp(0, 10),
                "music" => s.music = n.clamp(0, 10),
                "rejump" => s.rejump = n != 0,
                "speed" => s.speed = (n as usize).min(SPEEDS.len() - 1),
                "floors" => s.floors = (n as usize).min(FLOOR_SIZES.len() - 1),
                "gravity" => s.gravity = (n as usize).min(GRAVITIES.len() - 1),
                "best_floor" => s.best_floor = n.max(0),
                "replay_sort" => s.replay_sort = n.clamp(1, 4) as u8,
                "key_left" | "key_right" | "key_jump" | "key_pause" => {
                    let i = ["key_left", "key_right", "key_jump", "key_pause"].iter().position(|x| *x == k).unwrap();
                    if let Some(key) = key_from(v) { s.keys[i] = key; }
                }
                "profile" => s.profile = v.to_string(),
                _ => {}
            }
        }
        s
    }

    pub fn save(&self) {
        let mut out = format!(
            "character={}\nstart_floor={}\neye_candy={}\nfullscreen={}\nsound={}\nmusic={}\nrejump={}\nspeed={}\nfloors={}\ngravity={}\nbest_floor={}\n",
            self.character, self.start_floor, self.eye_candy, self.fullscreen as i32, self.sound, self.music,
            self.rejump as i32, self.speed, self.floors, self.gravity, self.best_floor
        );
        for (name, k) in ["key_left", "key_right", "key_jump", "key_pause"].iter().zip(self.keys) {
            out += &format!("{name}={}\n", key_name(k));
        }
        out += &format!("hd={}\nreplay_sort={}\nprofile={}\n", self.hd as i32, self.replay_sort, self.profile);
        let _ = std::fs::write(Self::path(), out); // settings are a convenience; ignore a read-only folder
    }

    /// Floor graphics unlock every 100 floors you've reached, like the original's "Start floor".
    pub fn unlocked_styles(&self) -> usize {
        ((self.best_floor / 100) as usize + 1).min(FLOOR_STYLES)
    }

    pub fn rules(&self, custom: bool) -> Rules {
        let base = Rules {
            rejump: self.rejump,
            style_base: self.start_floor,
            eye_candy: self.eye_candy < 2,
            ..Rules::default()
        };
        if !custom {
            return base;
        }
        // the original stores option indices; its speed list runs Insane..Normal
        Rules { start_speed: 5 - self.speed.min(5), floor_size: self.floors, gravity: self.gravity, ..base }
    }

    /// A classic game reached `floor`: true when that unlocks a new start floor (100..900),
    /// which play() announces with "A new start floor has been unlocked!".
    pub fn reach_floor(&mut self, floor: i32) -> bool {
        let (old, new) = (self.best_floor / 100, floor / 100);
        self.best_floor = self.best_floor.max(floor);
        new > old && new <= 9
    }

    pub fn items(&self, menu: MenuId) -> Vec<Item> {
        let it = |label: &str, action| Item { label: label.to_string(), action };
        let val = |label: &str, v: &str| Item { label: format!("{label}: {v}"), action: Action::Change };
        let key = |i: usize, label: &str| Item { label: format!("{label}: {}", key_name(self.keys[i])), action: Action::Bind(i) };
        let onoff = |b: bool| if b { "On" } else { "Off" };
        match menu {
            MenuId::Main => vec![
                it("Play Game", Action::Open(MenuId::Play)),
                it("Instructions", Action::Instructions),
                it("Profile", Action::Open(MenuId::Profile)),
                it("High Scores", Action::HighScores),
                it("Load Replay", Action::LoadReplay),
                it("Options", Action::Open(MenuId::Options)),
                it("Exit", Action::Exit),
            ],
            MenuId::Profile => vec![
                it("View Profile", Action::ViewProfile),
                it("Change Profile", Action::ChangeProfile),
                it("Back", Action::Back),
            ],
            MenuId::GameOver => vec![
                it("Play Again", Action::PlayAgain),
                it("Watch Replay", Action::WatchReplay),
                it("Save Replay", Action::SaveReplay),
                it("View Profile", Action::ViewProfile),
                it("Main Menu", Action::MainMenu),
            ],
            MenuId::Play => vec![
                it("Classic Game", Action::Classic),
                it("Custom Game", Action::Open(MenuId::Custom)),
                it("Back", Action::Back),
            ],
            MenuId::Custom => vec![
                it("Start Game", Action::Custom),
                val("Speed", SPEEDS[self.speed]),
                val("Floors", FLOOR_SIZES[self.floors]),
                val("Gravity", GRAVITIES[self.gravity]),
                it("Back", Action::Back),
            ],
            MenuId::Options => vec![
                it("GFX Options", Action::Open(MenuId::Gfx)),
                it("Sound Options", Action::Open(MenuId::Sound)),
                it("Controls", Action::Open(MenuId::Controls)),
                it("Back", Action::Back),
            ],
            MenuId::Gfx => vec![
                val("Character", CHARACTER_NAMES[self.character]),
                val("Start floor", &(self.start_floor * 100).to_string()),
                val("Eye Candy", EYE_CANDY[self.eye_candy]),
                val("Fullscreen", onoff(self.fullscreen)),
                val("HD Graphics", onoff(self.hd)),
                it("Back", Action::Back),
            ],
            MenuId::Sound => vec![
                val("Sound", &bar(self.sound)),
                val("Music", &bar(self.music)),
                it("Back", Action::Back),
            ],
            MenuId::Controls => vec![
                key(0, "Left"),
                key(1, "Right"),
                key(2, "Jump"),
                key(3, "Pause"),
                val("ReJump", onoff(self.rejump)),
                it("Back", Action::Back),
            ],
        }
    }

    /// Change the value of item `index` in `menu` by `dir` (-1 / +1).
    pub fn change(&mut self, menu: MenuId, index: usize, dir: i32) {
        match (menu, index) {
            (MenuId::Custom, 1) => self.speed = cycle(self.speed, SPEEDS.len(), dir),
            (MenuId::Custom, 2) => self.floors = cycle(self.floors, FLOOR_SIZES.len(), dir),
            (MenuId::Custom, 3) => self.gravity = cycle(self.gravity, GRAVITIES.len(), dir),
            (MenuId::Gfx, 0) => self.character = cycle(self.character, CHARACTER_NAMES.len(), dir),
            (MenuId::Gfx, 1) => self.start_floor = cycle(self.start_floor, self.unlocked_styles(), dir),
            (MenuId::Gfx, 2) => self.eye_candy = cycle(self.eye_candy, EYE_CANDY.len(), dir),
            (MenuId::Gfx, 3) => self.fullscreen = !self.fullscreen,
            (MenuId::Gfx, 4) => self.hd = !self.hd,
            (MenuId::Sound, 0) => self.sound = (self.sound + dir).clamp(0, 10),
            (MenuId::Sound, 1) => self.music = (self.music + dir).clamp(0, 10),
            (MenuId::Controls, 4) => self.rejump = !self.rejump,
            _ => {}
        }
    }
}

fn bar(v: i32) -> String {
    format!("{}{}", "|".repeat(v as usize), ".".repeat(10 - v as usize))
}

pub fn parent(menu: MenuId) -> Option<MenuId> {
    match menu {
        MenuId::Main | MenuId::GameOver => None,
        MenuId::Play | MenuId::Options | MenuId::Profile => Some(MenuId::Main),
        MenuId::Custom => Some(MenuId::Play),
        MenuId::Gfx | MenuId::Sound | MenuId::Controls => Some(MenuId::Options),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_round_trip_and_start_floors_unlock() {
        assert_eq!(key_from(&key_name(KeyCode::W)), Some(KeyCode::W));
        let mut s = Settings::default();
        assert!(!s.reach_floor(99));
        assert!(s.reach_floor(150));
        assert!(!s.reach_floor(199));
        assert!(s.reach_floor(901));
        assert!(!s.reach_floor(1200), "nothing past 900");
        assert_eq!(s.unlocked_styles(), 10);
    }
}
