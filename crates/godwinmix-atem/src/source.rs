//! Source numbers, which the switcher uses for everything a bus can carry.
//!
//! Inputs are 1 and up. The internal sources sit in fixed ranges that every
//! model shares; a model has only the ones its topology counts.

/// Black.
pub const BLACK: u16 = 0;
/// Colour bars.
pub const BARS: u16 = 1000;
/// Colour generator 1. Generator 2 is 2002.
pub const COLOR_1: u16 = 2001;
/// Media player 1's fill. Player n is 3000 + 10n, its key one more.
pub const MEDIA_PLAYER_1: u16 = 3010;
/// SuperSource.
pub const SUPER_SOURCE: u16 = 6000;
/// Clean feed 1. Clean feed 2 is 7002.
pub const CLEAN_FEED_1: u16 = 7001;
/// M/E 1 programme. M/E n is 10000 + 10n, its preview one more.
pub const ME_1_PROGRAM: u16 = 10010;

/// A source number in words, for a log line or an input the switcher has
/// not named.
pub fn describe(source: u16) -> String {
    match source {
        BLACK => "Black".into(),
        1..=999 => format!("Input {source}"),
        BARS => "Bars".into(),
        2001..=2009 => format!("Colour {}", source - 2000),
        3010..=3999 if source % 10 == 0 => format!("Media player {}", (source - 3000) / 10),
        3010..=3999 if source % 10 == 1 => format!("Media player {} key", (source - 3000) / 10),
        SUPER_SOURCE => "SuperSource".into(),
        7001..=7009 => format!("Clean feed {}", source - 7000),
        8001..=8099 => format!("Aux {}", source - 8000),
        10010..=10999 if source % 10 == 0 => format!("M/E {} programme", (source - 10000) / 10),
        10010..=10999 if source % 10 == 1 => format!("M/E {} preview", (source - 10000) / 10),
        other => format!("Source {other}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixed_sources_have_names() {
        assert_eq!(describe(0), "Black");
        assert_eq!(describe(4), "Input 4");
        assert_eq!(describe(3020), "Media player 2");
        assert_eq!(describe(3011), "Media player 1 key");
        assert_eq!(describe(10011), "M/E 1 preview");
        assert_eq!(describe(4321), "Source 4321");
    }
}
