//! Talks to the Quran.com API (which serves QuranicAudio URLs) and resolves reciters.

use crate::ipc::Track;

const UA: &str = concat!("murattal/", env!("CARGO_PKG_VERSION"));

/// Default reciter: Hani ar-Rifai.
pub const DEFAULT_RECITER: (u32, &str) = (5, "Hani ar-Rifai");

/// Well-known chapter-recitation reciter ids (Quran.com API v4).
pub const RECITERS: &[(u32, &str)] = &[
    (1, "AbdulBaset AbdulSamad (Mujawwad)"),
    (2, "AbdulBaset AbdulSamad (Murattal)"),
    (3, "Abdur-Rahman as-Sudais"),
    (4, "Abu Bakr al-Shatri"),
    (5, "Hani ar-Rifai"),
    (6, "Mahmoud Khalil Al-Husary"),
    (7, "Mishary Rashid Alafasy"),
    (9, "Mohamed Siddiq al-Minshawi"),
    (10, "Saud ash-Shuraim"),
];

fn normalize(s: &str) -> String {
    s.chars().filter(|c| c.is_alphanumeric()).flat_map(|c| c.to_lowercase()).collect()
}

/// Resolve a `--reciter` value (numeric id or fuzzy name) to `(id, name)`.
pub fn resolve_reciter(arg: &str) -> Result<(u32, String), String> {
    if let Ok(id) = arg.parse::<u32>() {
        let name = RECITERS
            .iter()
            .find(|(i, _)| *i == id)
            .map(|(_, n)| n.to_string())
            .unwrap_or_else(|| format!("Reciter #{id}"));
        return Ok((id, name));
    }
    let q = normalize(arg);
    RECITERS
        .iter()
        .find(|(_, n)| normalize(n).contains(&q))
        .map(|(i, n)| (*i, n.to_string()))
        .ok_or_else(|| format!("no reciter matches '{arg}' (try: quran reciters)"))
}

/// Fetch every surah's audio URL for a reciter, sorted 1..=114. Retries transient failures.
pub fn fetch_playlist(reciter_id: u32) -> Result<Vec<Track>, String> {
    let url = format!("https://api.quran.com/api/v4/chapter_recitations/{reciter_id}");
    let mut last = String::new();
    for attempt in 0..3 {
        match ureq::get(&url).set("User-Agent", UA).call() {
            Ok(resp) => {
                let v: serde_json::Value =
                    resp.into_json().map_err(|e| format!("bad API response: {e}"))?;
                let arr = v["audio_files"]
                    .as_array()
                    .ok_or("unexpected API response (no audio_files)")?;
                let mut tracks: Vec<Track> = arr
                    .iter()
                    .filter_map(|f| {
                        let surah = f["chapter_id"].as_u64()? as u32;
                        let url = f["audio_url"].as_str()?.to_string();
                        (1..=114).contains(&surah).then_some(Track { surah, url })
                    })
                    .collect();
                tracks.sort_by_key(|t| t.surah);
                if tracks.is_empty() {
                    return Err("API returned no audio files".into());
                }
                return Ok(tracks);
            }
            Err(e) => {
                last = e.to_string();
                if attempt < 2 {
                    std::thread::sleep(std::time::Duration::from_secs(2));
                }
            }
        }
    }
    Err(format!("could not reach api.quran.com ({last})"))
}
