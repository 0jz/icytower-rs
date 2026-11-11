// hisc.c: 15 highscore tables of 5 posts each. The original keeps them in tower.cfg (an Allegro
// packfile: options, then 15*5 posts of name[32] + value + checksum); we keep a text copy next to
// the exe and import the original's tables on first run.
use std::path::PathBuf;

pub const NAMES: [&str; 15] = [
    "Best Scores",
    "Best Combos",
    "Highest Floors",
    "Biggest Lost Combos",
    "Top Floors, No Combos",
    "Clock Challenge 1",
    "Clock Challenge 2",
    "Clock Challenge 3",
    "Clock Challenge 4",
    "Clock Challenge 5",
    "Single Jump Sequence",
    "Double Jump Sequence",
    "Triple Jump Sequence",
    "Quadruple Jump Sequence",
    "Quintuple Jump Sequence",
];

pub type Post = (String, u32);

pub struct Tables(pub Vec<[Post; 5]>);

impl Tables {
    /// reset_hisc_table(t, "Harold", ...): every post is Harold with 0
    fn fresh() -> Self {
        Tables(vec![std::array::from_fn(|_| ("Harold".to_string(), 0)); 15])
    }

    fn path() -> PathBuf {
        crate::profile::base_dir().join("highscores.txt")
    }

    pub fn load() -> Self {
        if let Ok(text) = std::fs::read_to_string(Self::path()) {
            let mut t = Self::fresh();
            for line in text.lines() {
                let f: Vec<&str> = line.splitn(4, '\t').collect();
                if let [i, j, v, name] = f[..] {
                    if let (Ok(i), Ok(j), Ok(v)) = (i.parse::<usize>(), j.parse::<usize>(), v.parse()) {
                        if i < 15 && j < 5 {
                            t.0[i][j] = (name.to_string(), v);
                        }
                    }
                }
            }
            return t;
        }
        let t = crate::profile::base_dir()
            .ancestors()
            .map(|d| d.join("tower.cfg"))
            .find(|p| p.is_file())
            .and_then(|p| Self::from_tower_cfg(&std::fs::read(p).ok()?))
            .unwrap_or_else(Self::fresh);
        t.save();
        t
    }

    pub fn save(&self) {
        let mut out = String::new();
        for (i, t) in self.0.iter().enumerate() {
            for (j, (name, v)) in t.iter().enumerate() {
                out += &format!("{i}\t{j}\t{v}\t{name}\n");
            }
        }
        let _ = std::fs::write(Self::path(), out);
    }

    /// The tables sit at the end of the unpacked tower.cfg; a post whose checksum fails resets
    /// its whole table, like load_hisc_table().
    fn from_tower_cfg(raw: &[u8]) -> Option<Self> {
        let data = unpack(raw)?;
        let base = data.len().checked_sub(15 * 5 * 40)?;
        let mut t = Self::fresh();
        for i in 0..15 {
            let mut table = t.0[i].clone();
            let mut ok = true;
            for (j, post) in table.iter_mut().enumerate() {
                let o = base + (i * 5 + j) * 40;
                let name = &data[o..o + 32];
                let name = &name[..name.iter().position(|&c| c == 0).unwrap_or(32)];
                let v = u32::from_le_bytes(data[o + 32..o + 36].try_into().ok()?);
                let sum = u32::from_le_bytes(data[o + 36..o + 40].try_into().ok()?);
                ok &= checksum(name, v) == sum;
                *post = (name.iter().map(|&c| c as char).collect(), v);
            }
            if ok {
                t.0[i] = table;
            }
        }
        Some(t)
    }

    /// qualify_hisc_table(): a non-zero value that beats any post
    pub fn qualifies(&self, i: usize, v: u32) -> bool {
        v != 0 && self.0[i].iter().any(|p| p.1 < v)
    }

    /// enter_hisc_table() + sort_hisc_table(): replace the first lowest post, keep the order stable
    pub fn enter(&mut self, i: usize, v: u32, name: &str) {
        let t = &mut self.0[i];
        let mut low = None;
        let mut min = 10_000_000;
        for (j, p) in t.iter().enumerate() {
            if p.1 < min {
                min = p.1;
                low = Some(j);
            }
        }
        if let Some(j) = low {
            t[j] = (name.to_string(), v);
        }
        for j in 1..5 {
            let mut k = j;
            while k > 0 && t[k - 1].1 < t[k].1 {
                t.swap(k - 1, k);
                k -= 1;
            }
        }
    }
}

/// generate_checksum(): value, then *140 + each (signed) name char
fn checksum(name: &[u8], v: u32) -> u32 {
    name.iter().fold(v, |h, &c| h.wrapping_mul(140).wrapping_add(c as i8 as u32))
}

/// Allegro packfile: "slh!" + LZSS, or "slh." stored as is.
fn unpack(raw: &[u8]) -> Option<Vec<u8>> {
    match raw.get(..4)? {
        b"slh." => Some(raw[4..].to_vec()),
        b"slh!" => Some(lzss(&raw[4..])),
        _ => None,
    }
}

fn lzss(src: &[u8]) -> Vec<u8> {
    const N: usize = 4096;
    const F: usize = 18;
    let (mut text, mut r, mut out) = ([0u8; N], N - F, Vec::new());
    let (mut flags, mut i) = (0u32, 0);
    while i < src.len() {
        flags >>= 1;
        if flags & 0x100 == 0 {
            flags = src[i] as u32 | 0xff00;
            i += 1;
            if i >= src.len() {
                break;
            }
        }
        if flags & 1 != 0 {
            let c = src[i];
            i += 1;
            out.push(c);
            text[r] = c;
            r = (r + 1) & (N - 1);
        } else {
            if i + 1 >= src.len() {
                break;
            }
            let (a, b) = (src[i] as usize, src[i + 1] as usize);
            i += 2;
            let (pos, len) = (a | ((b & 0xf0) << 4), (b & 0x0f) + 2);
            for k in 0..=len {
                let c = text[(pos + k) & (N - 1)];
                out.push(c);
                text[r] = c;
                r = (r + 1) & (N - 1);
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn qualify_and_enter_like_the_original() {
        let mut t = Tables::fresh();
        assert!(!t.qualifies(0, 0));
        for v in [50, 10, 30, 20, 40] {
            assert!(t.qualifies(0, v));
            t.enter(0, v, "abc");
        }
        assert_eq!(t.0[0].iter().map(|p| p.1).collect::<Vec<_>>(), [50, 40, 30, 20, 10]);
        assert!(!t.qualifies(0, 10) && t.qualifies(0, 11));
        t.enter(0, 35, "new");
        assert_eq!(t.0[0].iter().map(|p| p.1).collect::<Vec<_>>(), [50, 40, 35, 30, 20]);
    }

    #[test]
    fn reads_the_original_tower_cfg() {
        let Some(p) = std::path::Path::new("..").ancestors().map(|d| d.join("tower.cfg")).find(|p| p.is_file()) else { return };
        let t = Tables::from_tower_cfg(&std::fs::read(p).unwrap()).unwrap();
        // the default name for a post nobody has beaten yet
        assert!(t.0.iter().flatten().all(|(n, _)| !n.is_empty()));
        assert!(t.0[0][0].1 >= t.0[0][4].1);
    }
}
