//! Geometry of the scrollbar, kept apart from drawing so it can be tested.

/// Smallest thumb that is still easy to grab.
const MIN_THUMB: f32 = 16.0;

/// Position and size of the thumb inside the track.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Metrics {
    /// Distance from the top of the track.
    pub top: f32,
    /// Height of the thumb.
    pub height: f32,
}

/// Thumb of a viewport of `rows` lines that sits `offset` lines above the
/// newest line of a buffer holding `history` lines of scrollback.
pub(crate) fn metrics(track_height: f32, rows: usize, history: usize, offset: usize) -> Metrics {
    let rows = rows.max(1);
    let total = (history + rows) as f32;
    let visible = (rows as f32 / total).clamp(0.0, 1.0);
    let height = (track_height * visible).clamp(MIN_THUMB.min(track_height), track_height);
    let travel = (track_height - height).max(0.0);

    let scrolled = if history == 0 {
        1.0
    } else {
        (history.saturating_sub(offset)) as f32 / history as f32
    };
    Metrics {
        top: travel * scrolled.clamp(0.0, 1.0),
        height,
    }
}

/// Scrollback offset the thumb centre lands on when the pointer sits `local_y`
/// below the top of the track.
pub(crate) fn offset_at(
    track_height: f32,
    thumb_height: f32,
    history: usize,
    local_y: f32,
) -> usize {
    let travel = track_height - thumb_height;
    if travel <= 0.0 || history == 0 {
        return 0;
    }
    let scrolled = ((local_y - thumb_height / 2.0) / travel).clamp(0.0, 1.0);
    let from_top = (scrolled * history as f32).round() as usize;
    history.saturating_sub(from_top.min(history))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn thumb_reaches_both_ends() {
        let track = 200.0;
        let bottom = metrics(track, 25, 75, 0);
        assert_eq!(bottom.top + bottom.height, track);

        let top = metrics(track, 25, 75, 75);
        assert_eq!(top.top, 0.0);

        let middle = metrics(track, 25, 75, 37);
        assert!(middle.top > top.top && middle.top < bottom.top);
    }

    #[test]
    fn thumb_size_follows_the_visible_share() {
        let track = 200.0;
        assert_eq!(metrics(track, 25, 75, 0).height, 50.0);
        assert_eq!(metrics(track, 25, 0, 0).height, track);
        assert!(metrics(track, 1, 100_000, 0).height >= MIN_THUMB);
    }

    #[test]
    fn dragging_maps_the_whole_range() {
        let track = 200.0;
        let thumb = metrics(track, 25, 75, 0).height;

        assert_eq!(offset_at(track, thumb, 75, 0.0), 75);
        assert_eq!(offset_at(track, thumb, 75, track), 0);
        assert_eq!(offset_at(track, thumb, 75, track / 2.0), 37);
        assert_eq!(offset_at(track, thumb, 75, -50.0), 75);
        assert_eq!(offset_at(track, thumb, 0, 10.0), 0);
    }

    #[test]
    fn a_short_track_never_produces_a_negative_travel() {
        let metrics = metrics(10.0, 25, 1000, 500);
        assert_eq!(metrics.height, 10.0);
        assert_eq!(metrics.top, 0.0);
        assert_eq!(offset_at(10.0, metrics.height, 1000, 5.0), 0);
    }
}
