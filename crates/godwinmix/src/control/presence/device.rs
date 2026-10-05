//! A device a person recognises, from the User-Agent its browser sent.

/// A short guess at the device from a User-Agent: "iPhone Safari", "Android
/// Chrome", "Windows Edge", "gmx CLI". Never exact, and only ever shown to a
/// person, so a guess that reads well beats a string nobody can read.
pub fn device_of(agent: &str) -> String {
    let platforms = [
        ("iPhone", "iPhone"),
        ("iPad", "iPad"),
        ("Android", "Android"),
        ("Windows", "Windows"),
        ("Macintosh", "Mac"),
        ("CrOS", "Chromebook"),
        ("Linux", "Linux"),
    ];
    // Order matters: Edge and Opera say Chrome, and Chrome says Safari.
    let browsers = [
        ("Edg", "Edge"),
        ("OPR", "Opera"),
        ("Firefox", "Firefox"),
        ("FxiOS", "Firefox"),
        ("CriOS", "Chrome"),
        ("Chrome", "Chrome"),
        ("Safari", "Safari"),
    ];
    let platform = platforms.iter().find(|(k, _)| agent.contains(k)).map(|(_, v)| *v);
    let browser = browsers.iter().find(|(k, _)| agent.contains(k)).map(|(_, v)| *v);
    match (platform, browser) {
        (Some(p), Some(b)) => format!("{p} {b}"),
        (Some(p), None) => p.to_string(),
        (None, Some(b)) => b.to_string(),
        (None, None) if agent.starts_with("gmx") => "gmx CLI".into(),
        (None, None) => agent.split('/').next().unwrap_or("").trim().chars().take(32).collect(),
    }
}
