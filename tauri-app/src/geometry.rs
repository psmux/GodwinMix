//! Keeping the main window on its screen when it opens.
//!
//! The window's place and size are saved when it closes and put back when
//! it opens. What comes back can be bigger than the screen has room for: a
//! state saved on another monitor, or by 0.3.16, which on a 2880 by 1800
//! laptop saved 2880 by 1720 for a window that then opened 1819 pixels tall,
//! under the taskbar and past the bottom of the screen. So a window put back
//! that does not lie inside its monitor's work area (the screen less its
//! taskbar) is moved and shrunk until it does.
//!
//! Nothing here calls the runtime, so it is tested on its own.

/// A rectangle: x, y, width, height, in physical pixels.
pub type Rect = (i32, i32, u32, u32);

/// Where a window's outer frame, `window`, should go to lie inside `work`,
/// or `None` when it already does. `slack` is how far the frame may reach
/// past an edge and still count as inside: on Windows the outer frame holds
/// resize borders that cannot be seen, about 7 pixels on the left, the right
/// and the bottom, so a window that visibly fits overhangs by that much.
pub fn fit(window: Rect, work: Rect, slack: u32) -> Option<Rect> {
    let (x, y, w, h) = window;
    let (wx, wy, ww, wh) = work;
    let s = slack as i64;
    let inside = x as i64 >= wx as i64 - s
        && y as i64 >= wy as i64 - s
        && x as i64 + w as i64 <= wx as i64 + ww as i64 + s
        && y as i64 + h as i64 <= wy as i64 + wh as i64 + s;
    if inside {
        return None;
    }
    let width = w.min(ww);
    let height = h.min(wh);
    let nx = (x as i64).clamp(wx as i64, wx as i64 + (ww - width) as i64);
    let ny = (y as i64).clamp(wy as i64, wy as i64 + (wh - height) as i64);
    Some((nx as i32, ny as i32, width, height))
}

#[cfg(test)]
mod tests {
    use super::fit;

    const WORK: (i32, i32, u32, u32) = (0, 0, 2880, 1704);

    #[test]
    fn a_window_taller_than_the_work_area_is_shrunk_to_it() {
        // The frame seen on the test laptop: 11,0 to 2895,1819, with the
        // invisible borders around it.
        let got = fit((4, 0, 2898, 1826), WORK, 8).expect("it reaches under the taskbar");
        assert_eq!(got, (0, 0, 2880, 1704));
    }

    #[test]
    fn a_window_that_fits_stays_where_it_is() {
        assert_eq!(fit((200, 100, 1440, 900), WORK, 8), None);
        // A window dragged to fill the work area by hand, borders and all.
        assert_eq!(fit((-7, 0, 2894, 1711), WORK, 8), None);
    }

    #[test]
    fn a_window_partly_off_the_screen_is_moved_back_and_keeps_its_size() {
        assert_eq!(fit((2000, 1200, 1440, 900), WORK, 8), Some((1440, 804, 1440, 900)));
        assert_eq!(fit((-600, -50, 1440, 900), WORK, 8), Some((0, 0, 1440, 900)));
    }

    #[test]
    fn a_work_area_that_does_not_start_at_the_origin_is_honoured() {
        // A second monitor to the left, taskbar at its top.
        let work = (-1920, 40, 1920, 1040);
        assert_eq!(fit((-1920, 0, 1920, 1080), work, 8), Some((-1920, 40, 1920, 1040)));
    }
}
