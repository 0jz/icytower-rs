// Profiles and replays, modelled on the original's profile.c / replay.c.
// Each profile lives in profiles/<name>/ with the original's human-readable <name>_stats.txt,
// a machine-readable profile.cfg, and a replays/ folder.
use crate::f80::{f, F80};
use crate::game::{Game, Rules, END, JUMP, LEFT, RIGHT};

use std::fs;
use std::path::{Path, PathBuf};

pub const RANK_NAMES: [&str; 12] = ["no rank", "F", "E", "D", "C", "B", "A", "*", "**", "***", "****", "*****"];
const RANK_FLOORS: [i32; 12] = [0, 50, 100, 150, 200, 300, 400, 500, 600, 750, 1000, 1500];
const RANK_COMBOS: [i32; 12] = [0, 0, 7, 15, 25, 35, 70, 120, 200, 300, 400, 650];
const RANK_NO_COMBO: [i32; 12] = [0, 0, 0, 0, 0, 0, 0, 400, 500, 600, 700, 1200];
const RANK_CC1: [i32; 12] = [0, 0, 55, 65, 75, 85, 95, 105, 115, 125, 135, 145];
pub const COMBO_NAMES: [&str; 10] =
    ["Good:", "Sweet:", "Great:", "Super:", "WOW:", "Amazing:", "Extreme:", "Fantastic:", "Splendid:", "No way!"];

pub fn base_dir() -> PathBuf {
    std::env::current_exe().ok().and_then(|p| p.parent().map(Path::to_path_buf)).unwrap_or_default()
}

fn profiles_dir() -> PathBuf {
    base_dir().join("profiles")
}

pub fn today() -> String {
    // days since 1970 -> y-m-d (civil-from-days), no date crate needed
    let days = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs() as i64 / 86400);
    let z = days + 719468;
    let era = z.div_euclid(146097);
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + (m <= 2) as i64;
    format!("{y}-{m:02}-{d:02}")
}

#[derive(Clone, Default)]
pub struct Profile {
    pub name: String,
    pub created: String,
    pub updated: String,
    pub games_played: i32,
    pub custom_games_played: i32,
    pub games_quit: i32,
    pub seconds: i32,
    pub total_floors: i32,
    pub total_score: i32,
    pub total_combos: i32,
    pub total_combo_floors: i32,
    pub best_floor: i32,
    pub best_combo: i32,
    pub best_score: i32,
    pub no_combo_top: i32,
    pub lost_combo: i32,
    pub ccc: [i32; 5],
    pub ccc_total: [i32; 5],
    pub ccc_num: [i32; 5],
    pub rewards: [i32; 10],
    pub total_jumps: i32,
}

/// What a finished game set as new personal bests (used to name auto-saved replays).
#[derive(Default)]
pub struct Bests {
    pub score: bool,
    pub floor: bool,
    pub combo: bool,
}

impl Profile {
    pub fn new(name: &str) -> Self {
        Profile { name: name.to_string(), created: today(), updated: today(), ..Default::default() }
    }

    pub fn dir(&self) -> PathBuf {
        profiles_dir().join(&self.name)
    }

    pub fn replay_dir(&self) -> PathBuf {
        self.dir().join("replays")
    }

    pub fn rank_id(&self) -> usize {
        (0..12)
            .rev()
            .find(|&i| {
                self.best_floor >= RANK_FLOORS[i]
                    && self.best_combo >= RANK_COMBOS[i]
                    && self.no_combo_top >= RANK_NO_COMBO[i]
                    && self.ccc[0] >= RANK_CC1[i]
            })
            .unwrap_or(0)
    }

    /// set_next_rank_message(): what's still missing for the next rank.
    pub fn next_rank_message(&self) -> String {
        let next = self.rank_id() + 1;
        if next >= 12 {
            return "\n  You have the highest rank!".into();
        }
        let mut s = String::new();
        for (have, need, what) in [
            (self.best_floor, RANK_FLOORS[next], "Highest floor"),
            (self.best_combo, RANK_COMBOS[next], "Best combo"),
            (self.no_combo_top, RANK_NO_COMBO[next], "Top floor, no combo"),
            (self.ccc[0], RANK_CC1[next], "Clock Challenge 1"),
        ] {
            if have < need {
                s += &format!("\n  {what}: {need}");
            }
        }
        s
    }

    fn time_played(&self) -> String {
        let plural = |n: i32| if n == 1 { "" } else { "s" };
        let (d, h, m, s) = (self.seconds / 86400, self.seconds / 3600 % 24, self.seconds / 60 % 60, self.seconds % 60);
        match () {
            _ if self.seconds == 0 => "none".into(),
            _ if d > 0 => format!("{d} day{}, {h} hour{}, {m} minute{}, {s} second{}", plural(d), plural(h), plural(m), plural(s)),
            _ if h > 0 => format!("{h} hour{}, {m} minute{}, {s} second{}", plural(h), plural(m), plural(s)),
            _ if m > 0 => format!("{m} minute{}, {s} second{}", plural(m), plural(s)),
            _ => format!("{s} second{}", plural(s)),
        }
    }

    // profile_data_page_general / basic / advanced, using the original's format strings
    pub fn page_general(&self) -> String {
        format!(
            "Created:        {}\nTime played:    {}\nGames played:   {:<5}       Custom games played:  {:5}\nGames quit:     {}\n",
            self.created, self.time_played(), self.games_played, self.custom_games_played, self.games_quit
        )
    }

    pub fn page_basic(&self) -> String {
        let per = |a: i32, b: i32| if b > 0 { a / b } else { 0 };
        format!(
            "Best score ever:    {:7}\nAvg score per game: {:7}\nTotal score:        {:7}\n\n\
             Highest floor ever: {:7}\nAvg floors per game:{:7}\nFloors jumped:      {:7}\nTop Floor, No Combo:{:7}\n\n\
             Best combo ever:    {:7}\nAvg combos per game:{:7}\nAvg combo length:   {:7}\nCombos jumped:      {:7}\nLongest Lost Combo: {:7}\n",
            self.best_score, per(self.total_score, self.games_played), self.total_score,
            self.best_floor, per(self.total_floors, self.games_played), self.total_floors, self.no_combo_top,
            self.best_combo, per(self.total_combos, self.games_played), per(self.total_combo_floors, self.total_combos),
            self.total_combos, self.lost_combo
        )
    }

    pub fn page_advanced(&self) -> String {
        let mut s = String::new();
        for i in 0..5 {
            s += &format!("Clock Challenge {}:  {:7}\n", i + 1, self.ccc[i]);
        }
        s += "\n";
        for (name, n) in COMBO_NAMES.iter().zip(self.rewards) {
            s += &format!("{name:<12}        {n:7}\n");
        }
        s += &format!("\nTotal jumps:    {}\n", self.total_jumps);
        s
    }

    /// Fold a finished game into the profile. Custom games only count as played, like the original.
    pub fn add_game(&mut self, g: &Game, custom: bool) -> Bests {
        let p = &g.p;
        self.updated = today();
        self.total_jumps += p.jumps;
        self.seconds += (g.tick / 50) as i32;
        if custom {
            self.custom_games_played += 1;
            return Bests::default();
        }
        let score = g.final_score();
        let bests = Bests { score: score > self.best_score, floor: p.level > self.best_floor, combo: p.best_combo > self.best_combo };
        self.games_played += 1;
        self.total_floors += p.level;
        self.total_score += score;
        self.total_combos += p.combos;
        self.total_combo_floors += p.combo_floors;
        self.best_floor = self.best_floor.max(p.level);
        self.best_combo = self.best_combo.max(p.best_combo);
        self.best_score = self.best_score.max(score);
        self.no_combo_top = self.no_combo_top.max(p.no_combo_top);
        self.lost_combo = self.lost_combo.max(p.lost_combo);
        for i in 0..5 {
            if p.ccc[i] > 0 {
                self.ccc[i] = self.ccc[i].max(p.ccc[i]);
                self.ccc_total[i] += p.ccc[i];
                self.ccc_num[i] += 1;
            }
        }
        for i in 0..10 {
            self.rewards[i] += p.rewards[i];
        }
        bests
    }

    pub fn save(&self) {
        let dir = self.dir();
        let _ = fs::create_dir_all(self.replay_dir());
        let list = |a: &[i32]| a.iter().map(|v| v.to_string()).collect::<Vec<_>>().join(",");
        let cfg = format!(
            "created={}\nupdated={}\ngames_played={}\ncustom_games_played={}\ngames_quit={}\nseconds={}\ntotal_floors={}\n\
             total_score={}\ntotal_combos={}\ntotal_combo_floors={}\nbest_floor={}\nbest_combo={}\nbest_score={}\n\
             no_combo_top={}\nlost_combo={}\nccc={}\nccc_total={}\nccc_num={}\nrewards={}\ntotal_jumps={}\n",
            self.created, self.updated, self.games_played, self.custom_games_played, self.games_quit, self.seconds,
            self.total_floors, self.total_score, self.total_combos, self.total_combo_floors, self.best_floor,
            self.best_combo, self.best_score, self.no_combo_top, self.lost_combo, list(&self.ccc),
            list(&self.ccc_total), list(&self.ccc_num), list(&self.rewards), self.total_jumps
        );
        let _ = fs::write(dir.join("profile.cfg"), cfg);
        let _ = fs::write(dir.join(format!("{}_stats.txt", self.name)), self.stats_txt());
    }

    /// The original's <name>_stats.txt layout.
    fn stats_txt(&self) -> String {
        let mut s = format!(
            "ICY TOWER 1.4 PROFILE\n**********************************************\nProfile name:          {}\n\
             Last updated:          {}\nRank:                  {}\nCreated:               {}\n\n\
             Time played:           {}\nGames played:          {:<5}             Custom games played:      {}\n\
             Games quit:            {}\n\nBest score ever:          {}\nTotal score:              {}\n\n\
             Highest floor ever:       {}\nFloors jumped:            {}\n\nBest combo ever:          {}\nCombos jumped:            {}\n\n",
            self.name, self.updated, RANK_NAMES[self.rank_id()], self.created, self.time_played(), self.games_played,
            self.custom_games_played, self.games_quit, self.best_score, self.total_score, self.best_floor,
            self.total_floors, self.best_combo, self.total_combos
        );
        for (name, n) in COMBO_NAMES.iter().zip(self.rewards) {
            if n > 0 {
                s += &format!("{name:<26}{n}\n");
            }
        }
        s += &format!("\nTotal jumps:    {}\n\n\n", self.total_jumps);
        s
    }

    fn load(name: &str) -> Option<Self> {
        let text = fs::read_to_string(profiles_dir().join(name).join("profile.cfg")).ok()?;
        let mut p = Profile::new(name);
        for line in text.lines() {
            let Some((k, v)) = line.split_once('=') else { continue };
            let n = v.parse().unwrap_or(0);
            let arr = |out: &mut [i32]| {
                for (o, x) in out.iter_mut().zip(v.split(',')) {
                    *o = x.parse().unwrap_or(0);
                }
            };
            match k {
                "created" => p.created = v.into(),
                "updated" => p.updated = v.into(),
                "games_played" => p.games_played = n,
                "custom_games_played" => p.custom_games_played = n,
                "games_quit" => p.games_quit = n,
                "seconds" => p.seconds = n,
                "total_floors" => p.total_floors = n,
                "total_score" => p.total_score = n,
                "total_combos" => p.total_combos = n,
                "total_combo_floors" => p.total_combo_floors = n,
                "best_floor" => p.best_floor = n,
                "best_combo" => p.best_combo = n,
                "best_score" => p.best_score = n,
                "no_combo_top" => p.no_combo_top = n,
                "lost_combo" => p.lost_combo = n,
                "ccc" => arr(&mut p.ccc),
                "ccc_total" => arr(&mut p.ccc_total),
                "ccc_num" => arr(&mut p.ccc_num),
                "rewards" => arr(&mut p.rewards),
                "total_jumps" => p.total_jumps = n,
                _ => {}
            }
        }
        Some(p)
    }

    /// Read an original Icy Tower <name>_stats.txt (the .itp binary is obfuscated).
    fn import(stats: &Path, name: &str) -> Self {
        let mut p = Profile::new(name);
        let Ok(text) = fs::read_to_string(stats) else { return p };
        for line in text.lines() {
            let Some((k, v)) = line.split_once(':') else { continue };
            let v = v.trim();
            let n = v.split_whitespace().next().and_then(|x| x.parse().ok()).unwrap_or(0);
            match k.trim() {
                "Created" => p.created = v.into(),
                "Last updated" => p.updated = v.into(),
                "Games played" => {
                    p.games_played = n;
                    p.custom_games_played = v.rsplit(':').next().and_then(|x| x.trim().parse().ok()).unwrap_or(0);
                }
                "Games quit" => p.games_quit = n,
                "Best score ever" => p.best_score = n,
                "Total score" => p.total_score = n,
                "Highest floor ever" => p.best_floor = n,
                "Floors jumped" => p.total_floors = n,
                "Best combo ever" => p.best_combo = n,
                "Combos jumped" => p.total_combos = n,
                "Total jumps" => p.total_jumps = n,
                other => {
                    if let Some(i) = COMBO_NAMES.iter().position(|c| c.trim_end_matches(':') == other) {
                        p.rewards[i] = n;
                    }
                }
            }
        }
        p
    }
}

/// All profile names, guest first. On first run, profiles from the original game are imported.
pub fn list_profiles() -> Vec<String> {
    let dir = profiles_dir();
    if !dir.is_dir() {
        let _ = fs::create_dir_all(&dir);
        import_original();
    }
    let mut names: Vec<String> = fs::read_dir(&dir)
        .map(|rd| rd.flatten().filter(|e| e.path().join("profile.cfg").is_file()).map(|e| e.file_name().to_string_lossy().into_owned()).collect())
        .unwrap_or_default();
    names.sort_by_key(|n| (n != "guest", n.to_lowercase()));
    if names.first().map(String::as_str) != Some("guest") {
        Profile::new("guest").save();
        names.insert(0, "guest".into());
    }
    names
}

fn import_original() {
    // the original game sits next to (or above) this folder: <game>/profiles/<name>/<name>_stats.txt
    let Some(orig) = base_dir().ancestors().map(|a| a.join("profiles")).find(|p| p.is_dir() && p.parent().map_or(false, |g| g.join("icytower15.exe").is_file())) else { return };
    for e in fs::read_dir(orig).into_iter().flatten().flatten() {
        let name = e.file_name().to_string_lossy().into_owned();
        let stats = e.path().join(format!("{name}_stats.txt"));
        if stats.is_file() {
            Profile::import(&stats, &name).save();
        }
    }
}

pub fn load_profile(name: &str) -> Profile {
    Profile::load(name).unwrap_or_else(|| {
        let p = Profile::new(name);
        p.save();
        p
    })
}

/// Profile names become folder names: keep them to safe characters, like replaceBadCharacters().
pub fn clean_name(s: &str) -> String {
    s.chars().map(|c| if c.is_ascii_alphanumeric() || c == '-' { c } else { '_' }).collect::<String>().trim_matches('_').to_string()
}

pub fn create_profile(name: &str) -> Result<Profile, &'static str> {
    let name = clean_name(name);
    if name.is_empty() {
        return Err("Failed to create profile.");
    }
    if profiles_dir().join(&name).join("profile.cfg").exists() {
        return Err("A profile with that name already exists.");
    }
    let p = Profile::new(&name);
    p.save();
    Ok(p)
}

pub fn delete_profile(name: &str) {
    if name != "guest" {
        let _ = fs::remove_dir_all(profiles_dir().join(name));
    }
}

// ---------------------------------------------------------------- replays

/// A replay in the original's .itr format (replay.c save_replay/load_replay): the game options,
/// the random seed, and the control flags as (flags, extra ticks) runs, which play back exactly
/// because the game logic is the original's.
#[derive(Clone)]
pub struct Replay {
    pub name: String,
    pub date: String,
    pub comment: String,
    pub score: i32,
    pub floor: i32,
    pub combo: i32,
    pub no_combo: i32,
    pub lost_combo: i32,
    pub ccc: [i32; 5],
    pub jc: [i32; 5],
    pub seed: u32,
    pub rules: Rules,
    pub custom: bool,
    pub character: usize, // not in the file: the original plays replays with the current character
    raw: [Vec<u8>; 3], // name, date, comment as stored: bytes after the NUL count in the checksum
    tc: Vec<u8>, // timing statistics (tc_posts + 100 posts of 5 floats), kept as-is
    pub records: Vec<(u8, i32)>,
    pub file: PathBuf,
}

const ITR_HEADER: &[u8; 6] = b"ITR140";

fn read_i32(b: &[u8], at: &mut usize) -> Option<i32> {
    let v = i32::from_le_bytes(b.get(*at..*at + 4)?.try_into().ok()?);
    *at += 4;
    Some(v)
}

fn read_str(b: &[u8], at: &mut usize, n: usize) -> Option<(String, Vec<u8>)> {
    let s = b.get(*at..*at + n)?;
    *at += n;
    Some((c_str(s), s.to_vec()))
}

fn c_str(s: &[u8]) -> String {
    let end = s.iter().position(|&c| c == 0).unwrap_or(s.len());
    s[..end].iter().map(|&c| c as char).collect()
}

fn fixed(s: &str, n: usize) -> Vec<u8> {
    let mut v: Vec<u8> = s.chars().map(|c| if (c as u32) < 256 { c as u8 } else { b'?' }).take(n - 1).collect();
    v.resize(n, 0);
    v
}

/// The original's date format, e.g. " 9 Oct 2026".
pub fn itr_date() -> String {
    const MONTHS: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
    let t = today();
    let (y, m, d) = (&t[0..4], t[5..7].parse::<usize>().unwrap_or(1), t[8..10].parse::<u32>().unwrap_or(1));
    format!("{d:2} {} {y}", MONTHS[m - 1])
}

impl Replay {
    pub fn new(name: &str, seed: u32, rules: Rules, character: usize) -> Replay {
        Replay {
            name: name.into(),
            date: itr_date(),
            comment: String::new(),
            score: 0,
            floor: 0,
            combo: 0,
            no_combo: 0,
            lost_combo: 0,
            ccc: [0; 5],
            jc: [0; 5],
            seed,
            rules,
            custom: rules.is_custom(),
            character,
            raw: Default::default(),
            tc: vec![0; 4 + 2000],
            records: vec![(0, 0)],
            file: PathBuf::new(),
        }
    }

    /// Fill the header from a finished game (main.c, end of play()).
    pub fn finish(&mut self, g: &Game) {
        let p = &g.p;
        self.score = g.final_score();
        self.floor = p.level;
        self.combo = p.best_combo;
        self.no_combo = p.no_combo_top;
        self.lost_combo = p.lost_combo;
        self.ccc = p.ccc;
        self.jc = p.jc_top;
    }

    pub fn parse(b: &[u8]) -> Option<Replay> {
        if b.get(0..6)? != ITR_HEADER {
            return None;
        }
        let mut at = 6;
        let size = read_i32(b, &mut at)?;
        let (name, raw_name) = read_str(b, &mut at, 32)?;
        let (date, raw_date) = read_str(b, &mut at, 32)?;
        let mut v = [0i32; 5];
        for x in &mut v {
            *x = read_i32(b, &mut at)?;
        }
        let [score, floor, combo, no_combo, lost_combo] = v;
        let (mut ccc, mut jc) = ([0; 5], [0; 5]);
        for x in ccc.iter_mut().chain(jc.iter_mut()) {
            *x = read_i32(b, &mut at)?;
        }
        let mut o = [0i32; 7];
        for x in &mut o {
            *x = read_i32(b, &mut at)?;
        }
        let (comment, raw_comment) = read_str(b, &mut at, 42)?;
        let _checksum = read_i32(b, &mut at)?;
        let tc = b.get(at..at + 2004)?.to_vec();
        at += 2004;
        let mut records = Vec::new();
        for _ in 0..size.max(0) {
            let count = read_i32(b, &mut at)?;
            let flags = *b.get(at)?;
            at += 1;
            records.push((flags, count));
        }
        let rules = Rules {
            floor_shrink: o[0] != 0,
            floor_size: o[1].clamp(0, 4) as usize,
            start_speed: o[2].clamp(0, 5) as usize,
            speed_increase: o[3] != 0,
            gravity: o[4].clamp(0, 2) as usize,
            rejump: o[5] != 0,
            ..Rules::default()
        };
        Some(Replay {
            name,
            date,
            comment,
            score,
            floor,
            combo,
            no_combo,
            lost_combo,
            ccc,
            jc,
            seed: o[6] as u32,
            custom: rules.is_custom(),
            rules,
            character: 0,
            raw: [raw_name, raw_date, raw_comment],
            tc,
            records,
            file: PathBuf::new(),
        })
    }

    pub fn load(path: &Path) -> Option<Replay> {
        let mut r = Replay::parse(&fs::read(path).ok()?)?;
        r.file = path.to_path_buf();
        Some(r)
    }

    /// name, date and comment as fixed-size fields, keeping loaded bytes when unchanged
    fn fields(&self) -> [Vec<u8>; 3] {
        let keep = |i: usize, s: &str, n: usize| {
            if self.raw[i].len() == n && c_str(&self.raw[i]) == s { self.raw[i].clone() } else { fixed(s, n) }
        };
        [keep(0, &self.name, 32), keep(1, &self.date, 32), keep(2, &self.comment, 42)]
    }

    fn header_bytes(&self, checksum: i32) -> Vec<u8> {
        let r = &self.rules;
        let mut b = ITR_HEADER.to_vec();
        b.extend((self.records.len() as i32).to_le_bytes());
        let [name, date, comment] = self.fields();
        b.extend(name);
        b.extend(date);
        let ints = [self.score, self.floor, self.combo, self.no_combo, self.lost_combo];
        for v in ints.iter().chain(&self.ccc).chain(&self.jc) {
            b.extend(v.to_le_bytes());
        }
        let opts = [r.floor_shrink as i32, r.floor_size as i32, r.start_speed as i32, r.speed_increase as i32, r.gravity as i32, r.rejump as i32, self.seed as i32];
        for v in opts {
            b.extend(v.to_le_bytes());
        }
        b.extend(comment);
        b.extend(checksum.to_le_bytes());
        b
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let mut b = self.header_bytes(self.checksum() as i32);
        b.extend(&self.tc);
        for &(flags, count) in &self.records {
            b.extend(count.to_le_bytes());
            b.push(flags);
        }
        b
    }

    /// calc_replay_checksum(), so the original game accepts our replays too.
    pub fn checksum(&self) -> u32 {
        let r = &self.rules;
        let w = |a: i32, k: i32| a.wrapping_mul(k);
        let c1 = w(self.lost_combo, 17).wrapping_add(17).wrapping_add(w(self.no_combo, 127));
        let mut s = 0xe76i32
            .wrapping_add(w(r.floor_size as i32, 17))
            .wrapping_add(w(r.floor_shrink as i32, 102))
            .wrapping_add(w(r.start_speed as i32, 163))
            .wrapping_add(w(r.speed_increase as i32, 23))
            .wrapping_add(w(r.gravity as i32, 88))
            .wrapping_add(w(c1, 2));
        let tc_posts = i32::from_le_bytes(self.tc[0..4].try_into().unwrap());
        s = s
            .wrapping_add(w(self.seed as i32, 329))
            .wrapping_add(w(tc_posts, 127))
            .wrapping_add(w(r.rejump as i32, 13))
            .wrapping_add(w(self.score, 17).wrapping_add(17))
            .wrapping_add(w(self.combo.wrapping_add(1), 73))
            .wrapping_add(w(self.floor.wrapping_add(1), 113));
        for i in 0..5 {
            s = s.wrapping_add(w(self.ccc[i], 39 + 3 * i as i32)).wrapping_add(w(self.jc[i], 27 + 3 * i as i32));
        }
        // the timing floats: (u64)s + tc * k, truncated, three times per post (x87 maths)
        let tcf = |arr: usize, i: usize| f32::from_le_bytes(self.tc[4 + i * 20 + arr * 4..][..4].try_into().unwrap()) as f64;
        let mut s = s as u32;
        for i in 1..=100usize {
            for (arr, k) in [(0, i % 13), (1, (i + 6) % 17), (2, (i + 8) % 23)] {
                let v = F80::from_int(s as i64).add(f(tcf(arr, i - 1)).mul(F80::from_int(k as i64)));
                s = v.trunc() as u32;
            }
        }
        let mut s = s as i32;
        let [name, date, comment] = self.fields();
        for i in 0..32 {
            let a = (date[i] as i8 as i32).wrapping_add(i as i32);
            let b = (name[i] as i8 as i32).wrapping_add(i as i32);
            s = s.wrapping_add(b.wrapping_mul(a).wrapping_mul(17 * (i as i32 + 1)));
        }
        for i in 0..42 {
            let a = (comment[i] as i8 as i32).wrapping_add(i as i32);
            s = s.wrapping_add(a.wrapping_mul(a).wrapping_mul(-3 + 3 * i as i32));
        }
        for (i, &(flags, count)) in self.records.iter().enumerate() {
            let i = i as i32;
            s = s.wrapping_add((flags as i32 * 3).wrapping_mul(i % 193 + 1)).wrapping_add(count.wrapping_mul(7).wrapping_mul(i % 167 + 1));
        }
        // hash()
        let mut x = s as u32;
        x = (x ^ 61) ^ (x >> 16);
        x = x.wrapping_mul(9);
        x ^= x >> 4;
        x = x.wrapping_mul(0x27d4eb2d);
        x ^ (x >> 15)
    }

    pub fn save_as(&mut self, dir: &Path, file_name: &str) -> bool {
        let _ = fs::create_dir_all(dir);
        let stem = clean_name(file_name.trim_end_matches(".itr"));
        if stem.is_empty() {
            return false;
        }
        self.file = dir.join(format!("{stem}.itr"));
        fs::write(&self.file, self.to_bytes()).is_ok()
    }

    pub fn player(&self) -> Playback {
        Playback { records: self.records.clone(), pos: 0, flags: 0 }
    }
}

/// handle_player_input() while recording: runs of identical flags, then an END marker.
pub struct Recorder {
    pos: usize,
}

impl Recorder {
    pub fn new() -> Recorder {
        Recorder { pos: 0 }
    }

    pub fn record(&mut self, rep: &mut Replay, flags: u8, dead: bool) {
        let flags = flags & (LEFT | RIGHT | JUMP);
        let recs = &mut rep.records;
        if !dead && recs[self.pos].0 & END == 0 {
            if recs[self.pos].0 == flags {
                recs[self.pos].1 += 1;
            } else {
                self.pos += 1;
                recs.truncate(self.pos);
                recs.push((flags, 0));
            }
        } else {
            recs.truncate(self.pos + 1);
            recs.push((END, 0));
        }
    }
}

/// handle_player_input() during playback.
pub struct Playback {
    records: Vec<(u8, i32)>,
    pos: usize,
    flags: u8,
}

impl Playback {
    pub fn next(&mut self) -> u8 {
        if self.pos == 0 {
            self.pos = 1;
        } else if let Some(r) = self.records.get_mut(self.pos - 1) {
            self.flags = r.0;
            if r.1 > 0 {
                r.1 -= 1;
            } else {
                self.pos += 1;
            }
        } else {
            self.flags = 0;
        }
        self.flags
    }
}

/// Replays in a profile's folder, newest first.
pub fn list_replays(dir: &Path) -> Vec<Replay> {
    let mut v: Vec<(std::time::SystemTime, Replay)> = fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.path().extension().map_or(false, |x| x.eq_ignore_ascii_case("itr")))
        .filter_map(|e| Some((e.metadata().ok()?.modified().ok()?, Replay::load(&e.path())?)))
        .collect();
    v.sort_by(|a, b| b.0.cmp(&a.0));
    v.into_iter().map(|x| x.1).collect()
}

/// After every game: last_game.itr, plus <name>_best_*_N.itr for new personal bests (old one replaced).
pub fn autosave_replays(profile: &Profile, rep: &mut Replay, bests: &Bests) {
    let dir = profile.replay_dir();
    rep.save_as(&dir, "last_game");
    for (hit, kind, value) in [(bests.floor, "floor", rep.floor), (bests.combo, "combo", rep.combo), (bests.score, "score", rep.score)] {
        if !hit {
            continue;
        }
        let prefix = format!("{}_best_{kind}_", profile.name);
        for e in fs::read_dir(&dir).into_iter().flatten().flatten() {
            if e.file_name().to_string_lossy().starts_with(&prefix) {
                let _ = fs::remove_file(e.path());
            }
        }
        rep.save_as(&dir, &format!("{prefix}{value}"));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn play(rep: &Replay) -> Game {
        let mut g = Game::new(rep.seed, rep.rules);
        let mut pb = rep.player();
        while g.p.dead == 0 && g.tick < 200_000 {
            g.tick_flags(pb.next());
        }
        g
    }

    #[test]
    fn recorded_replay_round_trips_through_itr_bytes() {
        let rules = Rules { floor_size: 3, ..Rules::default() };
        let mut g = Game::new(42, rules);
        let mut rep = Replay::new("t", 42, rules, 0);
        let mut rec = Recorder::new();
        let mut t = 0u32;
        while g.p.dead == 0 {
            let flags = if t % 200 > 120 { LEFT } else if t % 200 < 100 { RIGHT } else { 0 } | if t % 7 < 3 { JUMP } else { 0 };
            rec.record(&mut rep, flags, g.p.dead != 0);
            g.tick_flags(flags);
            t += 1;
        }
        rec.record(&mut rep, 0, true);
        rep.finish(&g);
        let back = Replay::parse(&rep.to_bytes()).unwrap();
        assert_eq!(back.records, rep.records);
        assert_eq!(back.rules, rules);
        let g2 = play(&back);
        assert_eq!((g2.final_score(), g2.p.level, g2.p.best_combo), (back.score, back.floor, back.combo));
    }

    /// The original game's own replays (from ../profiles), if present: checksums and results must match.
    #[test]
    fn original_replays_play_back_exactly() {
        let Some(dir) = base_dir().ancestors().map(|a| a.join("profiles")).find(|p| p.join("neske").is_dir()) else { return };
        let mut n = 0;
        for e in fs::read_dir(dir.join("neske/replays")).into_iter().flatten().flatten() {
            let bytes = fs::read(e.path()).unwrap();
            let rep = Replay::parse(&bytes).expect("parse");
            let stored = i32::from_le_bytes(bytes[204..208].try_into().unwrap()) as u32;
            let g = play(&rep);
            let got = (g.final_score(), g.p.level, g.p.best_combo, g.p.no_combo_top, g.p.lost_combo, g.p.ccc, g.p.jc_top);
            let want = (rep.score, rep.floor, rep.combo, rep.no_combo, rep.lost_combo, rep.ccc, rep.jc);
            assert_eq!(got, want, "{:?}", e.path());
            assert_eq!(rep.checksum(), stored, "checksum of {:?}", e.path());
            assert!(rep.to_bytes() == bytes, "re-encoding {:?}", e.path());
            n += 1;
        }
        eprintln!("{n} original replays verified");
    }

    #[test]
    fn rank_follows_original_tables() {
        let mut p = Profile::new("x");
        assert_eq!(RANK_NAMES[p.rank_id()], "no rank");
        p.best_floor = 120;
        p.best_combo = 10;
        p.ccc[0] = 60;
        assert_eq!(RANK_NAMES[p.rank_id()], "E");
    }
}
