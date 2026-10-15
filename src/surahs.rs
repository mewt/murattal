//! The 114 surah names (index == surah number; index 0 is unused).

pub const NAMES: [&str; 115] = [
    "",
    "Al-Fatihah", "Al-Baqarah", "Ali 'Imran", "An-Nisa", "Al-Ma'idah", "Al-An'am",
    "Al-A'raf", "Al-Anfal", "At-Tawbah", "Yunus", "Hud", "Yusuf", "Ar-Ra'd", "Ibrahim",
    "Al-Hijr", "An-Nahl", "Al-Isra", "Al-Kahf", "Maryam", "Ta-Ha", "Al-Anbiya", "Al-Hajj",
    "Al-Mu'minun", "An-Nur", "Al-Furqan", "Ash-Shu'ara", "An-Naml", "Al-Qasas",
    "Al-'Ankabut", "Ar-Rum", "Luqman", "As-Sajdah", "Al-Ahzab", "Saba", "Fatir", "Ya-Sin",
    "As-Saffat", "Sad", "Az-Zumar", "Ghafir", "Fussilat", "Ash-Shura", "Az-Zukhruf",
    "Ad-Dukhan", "Al-Jathiyah", "Al-Ahqaf", "Muhammad", "Al-Fath", "Al-Hujurat", "Qaf",
    "Adh-Dhariyat", "At-Tur", "An-Najm", "Al-Qamar", "Ar-Rahman", "Al-Waqi'ah",
    "Al-Hadid", "Al-Mujadila", "Al-Hashr", "Al-Mumtahanah", "As-Saff", "Al-Jumu'ah",
    "Al-Munafiqun", "At-Taghabun", "At-Talaq", "At-Tahrim", "Al-Mulk", "Al-Qalam",
    "Al-Haqqah", "Al-Ma'arij", "Nuh", "Al-Jinn", "Al-Muzzammil", "Al-Muddaththir",
    "Al-Qiyamah", "Al-Insan", "Al-Mursalat", "An-Naba", "An-Nazi'at", "'Abasa",
    "At-Takwir", "Al-Infitar", "Al-Mutaffifin", "Al-Inshiqaq", "Al-Buruj", "At-Tariq",
    "Al-A'la", "Al-Ghashiyah", "Al-Fajr", "Al-Balad", "Ash-Shams", "Al-Layl", "Ad-Duha",
    "Ash-Sharh", "At-Tin", "Al-'Alaq", "Al-Qadr", "Al-Bayyinah", "Az-Zalzalah",
    "Al-'Adiyat", "Al-Qari'ah", "At-Takathur", "Al-'Asr", "Al-Humazah", "Al-Fil",
    "Quraysh", "Al-Ma'un", "Al-Kawthar", "Al-Kafirun", "An-Nasr", "Al-Masad",
    "Al-Ikhlas", "Al-Falaq", "An-Nas",
];

/// Juz 'Amma short surahs, in a common memorisation order (not numeric).
/// Al-Ikhlas, Al-Falaq, An-Nas, Al-Kawthar, Al-'Asr, An-Nasr, Al-Kafirun,
/// Al-Masad (Lahab), Al-Ma'un, Quraysh.
pub const JUZ_AMMA: [u32; 10] = [112, 113, 114, 108, 103, 110, 109, 111, 107, 106];

/// Name for a surah number, or `""` if out of range.
pub fn name(n: u32) -> &'static str {
    NAMES.get(n as usize).copied().unwrap_or("")
}

fn normalize(s: &str) -> String {
    s.chars().filter(|c| c.is_alphanumeric()).flat_map(|c| c.to_lowercase()).collect()
}

/// Resolve a surah argument — a number `1..=114` or a (fuzzy) name — to its number.
pub fn resolve(arg: &str) -> Result<u32, String> {
    if let Ok(n) = arg.parse::<u32>() {
        if (1..=114).contains(&n) {
            return Ok(n);
        }
        return Err(format!("surah must be 1..114 (got {n})"));
    }
    let q = normalize(arg);
    if q.is_empty() {
        return Err("empty surah".into());
    }
    for (i, nm) in NAMES.iter().enumerate().skip(1) {
        if normalize(nm).contains(&q) {
            return Ok(i as u32);
        }
    }
    Err(format!("no surah matches '{arg}' (try: quran list)"))
}
