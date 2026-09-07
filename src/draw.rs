//! Painting the popup, in whichever theme is configured.
//!
//! Everything here is immediate-mode painting against fixed rectangles rather
//! than egui widgets, because the five looks are specified down to the pixel
//! and widgets would fight that. The shared shape of a frame is
//! [`window`] then [`header`] then [`list`] then [`footer`]; only the insides
//! of those four differ per theme.
//!
//! Nothing in here touches more rows than fit on screen. Fuzzy-match
//! highlighting in particular is computed per drawn row, never for the whole
//! list.

use eframe::egui;
use egui::text::LayoutJob;
use egui::{Color32, FontId, Pos2, Rect, Rounding, Sense, Shape, Stroke, TextFormat, Vec2};
use nucleo_matcher::pattern::{CaseMatching, Normalization, Pattern};
use nucleo_matcher::{Config, Matcher, Utf32Str};
use std::sync::Arc;

use crate::model::{Kind, Person};
use crate::recency;
use crate::theme::{avatar_index, initials, Metrics, Palette, Theme};

/// Which rows the rich theme's chips let through.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Filter {
    #[default]
    All,
    People,
    Groups,
}

impl Filter {
    pub const ORDER: [Filter; 3] = [Filter::All, Filter::People, Filter::Groups];

    pub fn label(self) -> &'static str {
        match self {
            Filter::All => "All",
            Filter::People => "People",
            Filter::Groups => "Groups",
        }
    }

    pub fn next(self) -> Filter {
        match self {
            Filter::All => Filter::People,
            Filter::People => Filter::Groups,
            Filter::Groups => Filter::All,
        }
    }

    pub fn accepts(self, p: &Person) -> bool {
        match self {
            Filter::All => true,
            Filter::People => p.kind == Kind::Person,
            Filter::Groups => p.kind == Kind::Group,
        }
    }
}

/// Which characters of a name the current query matched.
///
/// Holds the matcher and its scratch buffers so a frame allocates nothing.
pub struct Highlighter {
    matcher: Matcher,
    pattern: Option<Pattern>,
    utf32: Vec<char>,
    hits: Vec<u32>,
}

impl Default for Highlighter {
    fn default() -> Self {
        Self::new()
    }
}

impl Highlighter {
    pub fn new() -> Self {
        Self {
            matcher: Matcher::new(Config::DEFAULT),
            pattern: None,
            utf32: Vec::new(),
            hits: Vec::new(),
        }
    }

    /// Reparse the pattern. Cheap, but still only worth doing when the query
    /// actually changes.
    pub fn set_query(&mut self, query: &str) {
        let q = query.trim();
        self.pattern = if q.is_empty() {
            None
        } else {
            Some(Pattern::parse(q, CaseMatching::Ignore, Normalization::Smart))
        };
    }

    /// Character indices of `name` that the query matched, ascending.
    fn hits(&mut self, name: &str) -> &[u32] {
        let Self {
            matcher,
            pattern,
            utf32,
            hits,
        } = self;
        hits.clear();
        if let Some(pattern) = pattern {
            utf32.clear();
            let hay = Utf32Str::new(name, utf32);
            pattern.indices(hay, matcher, hits);
            hits.sort_unstable();
            hits.dedup();
        }
        hits
    }
}

/// One frame's worth of picker state, everything the painter needs to read.
pub struct Screen<'a> {
    pub query: &'a str,
    pub people: &'a [Person],
    /// Indices into `people`, best match first, already filtered.
    pub matches: &'a [usize],
    pub cursor: usize,
    /// First match drawn, so long lists do not lay out off-screen rows.
    pub top: usize,
    /// Size of the whole address book, for the "6 of 44" counters.
    pub total: usize,
    pub filter: Filter,
    /// Unix seconds; passed in so recency labels are computed once per frame.
    pub now: i64,
}

/// A painter bound to one theme.
pub struct View<'a> {
    ui: &'a egui::Ui,
    theme: Theme,
    pal: Palette,
    met: Metrics,
    hl: &'a mut Highlighter,
}

/// How many rows the list area of this theme can show at `height` pixels.
pub fn capacity(theme: Theme, size: Vec2) -> usize {
    let met = theme.metrics();
    let rect = list_rect(theme, &met, Rect::from_min_size(Pos2::ZERO, size));
    let step = met.row_h + met.row_gap;
    if step <= 0.0 {
        return 1;
    }
    (((rect.height() + met.row_gap) / step).floor() as usize).max(1)
}

fn list_rect(theme: Theme, met: &Metrics, window: Rect) -> Rect {
    let inner = window.shrink(met.border_width);
    let top = inner.top() + met.header_h + met.list_pad_y;
    let bottom = inner.bottom() - met.footer_h - met.list_pad_y;
    let (left, right) = match theme {
        // Brutalist rows are full-bleed: the rules between them run edge to edge.
        Theme::Brutalist => (inner.left(), inner.right()),
        _ => (inner.left() + met.list_pad_x, inner.right() - met.list_pad_x),
    };
    Rect::from_min_max(Pos2::new(left, top), Pos2::new(right, bottom.max(top)))
}

fn tf(font: FontId, color: Color32) -> TextFormat {
    TextFormat {
        font_id: font,
        color,
        ..Default::default()
    }
}

fn tf_tracked(font: FontId, color: Color32, tracking: f32) -> TextFormat {
    TextFormat {
        font_id: font,
        color,
        extra_letter_spacing: tracking,
        ..Default::default()
    }
}

/// One line of text, ellipsized rather than wrapped.
fn one_line(job: &mut LayoutJob, max_width: f32) {
    job.wrap.max_width = max_width;
    job.wrap.max_rows = 1;
    job.wrap.break_anywhere = true;
    job.wrap.overflow_character = Some('…');
}

/// A name with its matched characters in a second format. One job, so the row
/// is a single galley however many runs it has.
fn name_job(name: &str, hits: &[u32], base: &TextFormat, hit: &TextFormat) -> LayoutJob {
    let mut job = LayoutJob::default();
    if name.is_empty() {
        return job;
    }
    let mut start = 0usize;
    let mut in_hit = hits.first() == Some(&0);
    for (ci, (bi, _)) in name.char_indices().enumerate() {
        let is_hit = hits.binary_search(&(ci as u32)).is_ok();
        if is_hit != in_hit {
            job.append(&name[start..bi], 0.0, if in_hit { hit.clone() } else { base.clone() });
            start = bi;
            in_hit = is_hit;
        }
    }
    job.append(&name[start..], 0.0, if in_hit { hit.clone() } else { base.clone() });
    job
}

impl<'a> View<'a> {
    pub fn new(ui: &'a egui::Ui, theme: Theme, hl: &'a mut Highlighter) -> Self {
        Self {
            ui,
            theme,
            pal: theme.palette(),
            met: theme.metrics(),
            hl,
        }
    }

    fn galley(&self, job: LayoutJob) -> Arc<egui::Galley> {
        self.ui.fonts(|f| f.layout_job(job))
    }

    fn text(&self, s: &str, format: TextFormat, max_width: f32) -> Arc<egui::Galley> {
        let mut job = LayoutJob::default();
        job.append(s, 0.0, format);
        one_line(&mut job, max_width);
        self.galley(job)
    }

    /// Paint a galley with its left edge at `x`, centered on `cy`. Returns the
    /// x just past it.
    fn put_left(&self, g: &Arc<egui::Galley>, x: f32, cy: f32) -> f32 {
        let pos = Pos2::new(x, cy - g.size().y * 0.5);
        self.ui.painter().galley(pos, g.clone(), self.pal.text);
        x + g.size().x
    }

    /// Paint a galley with its right edge at `x`, centered on `cy`. Returns the
    /// x just before it.
    fn put_right(&self, g: &Arc<egui::Galley>, x: f32, cy: f32) -> f32 {
        let pos = Pos2::new(x - g.size().x, cy - g.size().y * 0.5);
        self.ui.painter().galley(pos, g.clone(), self.pal.text);
        x - g.size().x
    }

    fn rect(&self, rect: Rect, radius: f32, fill: Color32) {
        self.ui
            .painter()
            .rect_filled(rect, Rounding::same(radius), fill);
    }

    fn hairline(&self, y: f32, x0: f32, x1: f32, color: Color32, width: f32) {
        self.ui.painter().rect_filled(
            Rect::from_min_max(Pos2::new(x0, y), Pos2::new(x1, y + width)),
            Rounding::ZERO,
            color,
        );
    }

    /// The window itself: background, rounded corners, border.
    pub fn window(&self, rect: Rect) {
        let r = Rounding::same(self.met.window_radius);
        self.ui.painter().rect_filled(rect, r, self.pal.window_bg);
        if self.met.border_width > 0.0 && self.pal.window_border != Color32::TRANSPARENT {
            self.ui.painter().rect_stroke(
                rect.shrink(self.met.border_width * 0.5),
                r,
                Stroke::new(self.met.border_width, self.pal.window_border),
            );
        }
    }

    // ---------------------------------------------------------------- icons

    /// A magnifier: a circle and a stub handle.
    fn search_icon(&self, center: Pos2, size: f32, color: Color32) {
        let s = Stroke::new((size / 10.0).max(1.2), color);
        let r = size * 0.29;
        let c = center - Vec2::splat(size * 0.06);
        self.ui.painter().circle_stroke(c, r, s);
        let d = Vec2::splat(std::f32::consts::FRAC_1_SQRT_2) * r;
        self.ui
            .painter()
            .line_segment([c + d, c + d + Vec2::splat(size * 0.19)], s);
    }

    /// Two heads and two shoulders: the group marker.
    fn group_icon(&self, center: Pos2, size: f32, color: Color32) {
        let s = Stroke::new((size / 11.0).max(1.1), color);
        let p = self.ui.painter();
        let head = size * 0.15;
        let front = center + Vec2::new(-size * 0.12, -size * 0.16);
        let back = center + Vec2::new(size * 0.21, -size * 0.13);
        p.circle_stroke(front, head, s);
        p.circle_stroke(back, head * 0.72, s);
        p.add(Shape::line(
            arc(front + Vec2::new(0.0, size * 0.46), size * 0.25, 180.0, 360.0),
            s,
        ));
        p.add(Shape::line(
            arc(back + Vec2::new(0.0, size * 0.44), size * 0.21, 200.0, 340.0),
            s,
        ));
    }

    /// The return arrow used in the footer hint.
    fn return_icon(&self, center: Pos2, size: f32, color: Color32) {
        let s = Stroke::new(1.3_f32, color);
        let p = self.ui.painter();
        let right = center + Vec2::new(size * 0.42, -size * 0.35);
        let corner = center + Vec2::new(size * 0.42, size * 0.2);
        let left = center + Vec2::new(-size * 0.42, size * 0.2);
        p.line_segment([right, corner], s);
        p.line_segment([corner, left], s);
        p.line_segment([left, left + Vec2::new(size * 0.24, -size * 0.22)], s);
        p.line_segment([left, left + Vec2::new(size * 0.24, size * 0.22)], s);
    }

    /// A stacked up/down arrow pair for the footer hint.
    fn updown_icon(&self, center: Pos2, size: f32, color: Color32) {
        let s = Stroke::new(1.3_f32, color);
        let p = self.ui.painter();
        for (dx, dir) in [(-size * 0.22, -1.0), (size * 0.22, 1.0)] {
            let top = center + Vec2::new(dx, -size * 0.38 * dir);
            let bot = center + Vec2::new(dx, size * 0.38 * dir);
            p.line_segment([top, bot], s);
            p.line_segment([top, top + Vec2::new(-size * 0.16, size * 0.2 * dir)], s);
            p.line_segment([top, top + Vec2::new(size * 0.16, size * 0.2 * dir)], s);
        }
    }

    /// The initials disc (editorial) or rounded square (rich).
    fn avatar(&self, rect: Rect, p: &Person) {
        let bg = self.pal.avatars[avatar_index(&p.name, self.pal.avatars.len())];
        let radius = match self.theme {
            Theme::Editorial => rect.width() * 0.5,
            _ => 10.0,
        };
        self.rect(rect, radius, bg);
        // A group is better identified by how many people are in it than by
        // the first letters of a name that is a list of names.
        let text = match (self.theme, p.kind, p.members.len()) {
            (Theme::Rich, Kind::Group, n) if n > 0 => n.to_string(),
            _ => initials(&p.name),
        };
        let size = if self.theme == Theme::Editorial { 12.0 } else { 13.0 };
        let g = self.text(
            &text,
            tf(
                FontId::new(size, self.theme.family_meta_strong()),
                self.pal.avatar_fg,
            ),
            rect.width(),
        );
        self.ui.painter().galley(
            rect.center() - g.size() * 0.5,
            g,
            self.pal.avatar_fg,
        );
    }

    // --------------------------------------------------------------- header

    pub fn header(&mut self, window: Rect, s: &Screen) {
        let met = &self.met;
        let inner = window.shrink(met.border_width);
        let rect = Rect::from_min_size(inner.min, Vec2::new(inner.width(), met.header_h));
        if let Some(bg) = self.pal.header_bg {
            self.rect(rect, 0.0, bg);
        }
        match self.theme {
            Theme::Spotlight => self.header_spotlight(rect, s),
            // The terminal's prompt lives at the bottom, with the footer.
            Theme::Terminal => {}
            Theme::Editorial => self.header_editorial(rect, s),
            Theme::Brutalist => self.header_brutalist(rect, s),
            Theme::Rich => self.header_rich(rect, s),
        }
        if matches!(self.theme, Theme::Spotlight | Theme::Rich) {
            self.hairline(rect.bottom(), rect.left(), rect.right(), self.pal.rule, 1.0);
        }
    }

    /// The caret: a solid bar in the accent color, sized by the theme.
    fn caret(&self, x: f32, cy: f32) {
        let (w, h) = (self.met.caret_w, self.met.caret_h);
        self.rect(
            Rect::from_min_size(Pos2::new(x, cy - h * 0.5), Vec2::new(w, h)),
            0.0,
            self.pal.accent,
        );
    }

    fn header_spotlight(&mut self, rect: Rect, s: &Screen) {
        let cy = rect.center().y;
        let mut x = rect.left() + self.met.side_pad;
        self.search_icon(Pos2::new(x + 10.0, cy), 20.0, self.pal.dim);
        x += 20.0 + 12.0;

        // The "esc" chip is measured first so the query knows its room.
        let esc = self.text(
            "esc",
            tf(self.theme.font(12.0), self.pal.chip_fg),
            f32::INFINITY,
        );
        let chip_w = esc.size().x + 14.0;
        let chip = Rect::from_center_size(
            Pos2::new(rect.right() - self.met.side_pad - chip_w * 0.5, cy),
            Vec2::new(chip_w, 20.0),
        );
        self.ui.painter().rect_stroke(
            chip,
            Rounding::same(5.0),
            Stroke::new(1.0_f32, Color32::from_rgba_unmultiplied(0xff, 0xff, 0xff, 31)),
        );
        self.ui.painter().galley(
            Pos2::new(chip.left() + 7.0, cy - esc.size().y * 0.5),
            esc,
            self.pal.chip_fg,
        );

        let room = chip.left() - 12.0 - x - self.met.caret_w;
        let g = self.text(
            s.query,
            tf(
                FontId::new(self.met.query_size, self.theme.family_query()),
                self.pal.text,
            ),
            room,
        );
        let end = self.put_left(&g, x, cy);
        self.caret(end + 1.0, cy);
    }

    fn header_editorial(&mut self, rect: Rect, s: &Screen) {
        let cy = rect.top() + 22.0 + self.met.query_size * 0.5;
        let count = if s.matches.len() == 1 {
            "1 PERSON".to_string()
        } else {
            format!("{} PEOPLE", s.matches.len())
        };
        let cg = self.text(
            &count,
            tf_tracked(
                FontId::new(11.0, self.theme.family_meta()),
                self.pal.dim,
                1.54,
            ),
            f32::INFINITY,
        );
        let count_left = rect.right() - self.met.side_pad - cg.size().x;
        self.ui.painter().galley(
            Pos2::new(count_left, cy - cg.size().y * 0.5 + 4.0),
            cg,
            self.pal.dim,
        );

        let x = rect.left() + self.met.side_pad;
        let g = self.text(
            s.query,
            tf(
                FontId::new(self.met.query_size, self.theme.family_query()),
                self.pal.text,
            ),
            count_left - 12.0 - x - self.met.caret_w,
        );
        let end = self.put_left(&g, x, cy);
        self.caret(end + 1.0, cy);
        self.hairline(
            rect.bottom() - 1.0,
            rect.left() + self.met.side_pad,
            rect.right() - self.met.side_pad,
            self.pal.rule,
            1.0,
        );
    }

    fn header_brutalist(&mut self, rect: Rect, s: &Screen) {
        let cy = rect.center().y;
        let count = format!("{} / {}", s.matches.len(), s.total);
        let cg = self.text(
            &count,
            tf_tracked(
                FontId::new(11.0, self.theme.family_strong()),
                self.pal.accent,
                1.54,
            ),
            f32::INFINITY,
        );
        let count_left = self.put_right(&cg, rect.right() - self.met.side_pad, cy);

        let x = rect.left() + self.met.side_pad;
        let g = self.text(
            &s.query.to_uppercase(),
            tf_tracked(
                FontId::new(self.met.query_size, self.theme.family_query()),
                self.pal.text,
                -1.76,
            ),
            count_left - 12.0 - x - self.met.caret_w - 4.0,
        );
        let end = self.put_left(&g, x, cy);
        self.caret(end + 4.0, cy);
        self.hairline(
            rect.bottom() - self.met.border_width,
            rect.left(),
            rect.right(),
            self.pal.rule,
            self.met.border_width,
        );
    }

    fn header_rich(&mut self, rect: Rect, s: &Screen) {
        let cy = rect.center().y;
        let mut x = rect.left() + self.met.side_pad;
        self.search_icon(Pos2::new(x + 9.0, cy), 18.0, self.pal.dim);
        x += 18.0 + 10.0;

        // Chips are laid out right to left so the query keeps whatever is left.
        let mut chips: Vec<(Filter, Arc<egui::Galley>)> = Vec::with_capacity(3);
        let mut chips_w = 0.0;
        for f in Filter::ORDER {
            let active = f == s.filter;
            let fg = if active { Color32::WHITE } else { self.pal.chip_fg };
            let g = self.text(f.label(), tf(self.theme.font(11.0), fg), f32::INFINITY);
            chips_w += g.size().x + 16.0 + 6.0;
            chips.push((f, g));
        }
        let mut cx = rect.right() - self.met.side_pad - chips_w + 6.0;
        let chips_left = cx;
        for (f, g) in chips {
            let active = f == s.filter;
            let w = g.size().x + 16.0;
            let r = Rect::from_center_size(Pos2::new(cx + w * 0.5, cy), Vec2::new(w, 20.0));
            self.rect(
                r,
                6.0,
                if active { self.pal.accent } else { self.pal.chip_bg },
            );
            let fg = if active { Color32::WHITE } else { self.pal.chip_fg };
            self.ui
                .painter()
                .galley(Pos2::new(cx + 8.0, cy - g.size().y * 0.5), g, fg);
            cx += w + 6.0;
        }

        let g = self.text(
            s.query,
            tf(
                FontId::new(self.met.query_size, self.theme.family_query()),
                self.pal.text,
            ),
            chips_left - 12.0 - x - self.met.caret_w,
        );
        let end = self.put_left(&g, x, cy);
        self.caret(end + 1.0, cy);
    }

    // ----------------------------------------------------------------- list

    /// Draw the visible window of rows. Returns the match index clicked, if any.
    pub fn list(&mut self, window: Rect, s: &Screen) -> Option<usize> {
        let rect = list_rect(self.theme, &self.met, window);
        let step = self.met.row_h + self.met.row_gap;
        let cap = capacity(self.theme, window.size());
        let mut clicked = None;
        for slot in 0..cap {
            let Some(&person) = s.matches.get(s.top + slot) else {
                break;
            };
            // The terminal stacks upward from the prompt: best match at the
            // bottom, nearest the thing you are typing into.
            let top = match self.theme {
                Theme::Terminal => rect.bottom() - (slot as f32 + 1.0) * step,
                _ => rect.top() + slot as f32 * step,
            };
            if top < rect.top() - 0.5 || top + self.met.row_h > rect.bottom() + 0.5 {
                continue;
            }
            let row = Rect::from_min_size(
                Pos2::new(rect.left(), top),
                Vec2::new(rect.width(), self.met.row_h),
            );
            let id = self.ui.id().with(("msg-row", slot));
            let resp = self.ui.interact(row, id, Sense::click());
            let selected = s.top + slot == s.cursor;
            self.row(row, &s.people[person], selected, resp.hovered(), s.now);
            if resp.clicked() {
                clicked = Some(s.top + slot);
            }
        }
        clicked
    }

    fn row_background(&self, rect: Rect, selected: bool, hovered: bool) {
        let r = self.met.row_radius;
        if selected {
            self.rect(rect, r, self.pal.sel_bg);
            if let Some(outline) = self.pal.sel_outline {
                self.ui.painter().rect_stroke(
                    rect.shrink(0.5),
                    Rounding::same(r),
                    Stroke::new(1.0_f32, outline),
                );
            }
        } else if hovered {
            self.rect(rect, r, self.pal.row_hover);
        }
        if self.theme == Theme::Brutalist {
            // Every row carries its own bottom rule, selected included.
            self.hairline(
                rect.bottom() - 1.0,
                rect.left(),
                rect.right(),
                Color32::from_rgb(0x26, 0x26, 0x26),
                1.0,
            );
        }
    }

    fn row(&mut self, rect: Rect, p: &Person, selected: bool, hovered: bool, now: i64) {
        self.row_background(rect, selected, hovered);
        match self.theme {
            Theme::Spotlight => self.row_spotlight(rect, p, selected, now),
            Theme::Terminal => self.row_terminal(rect, p, selected),
            Theme::Editorial => self.row_editorial(rect, p, selected, now),
            Theme::Brutalist => self.row_brutalist(rect, p, selected, now),
            Theme::Rich => self.row_rich(rect, p, selected, now),
        }
    }

    /// The two text formats a name is drawn with in this theme.
    fn name_formats(&self, selected: bool, size: f32, tracking: f32) -> (TextFormat, TextFormat) {
        let (fg, hit_fg) = if selected {
            (self.pal.sel_fg, self.pal.sel_match_fg)
        } else {
            (self.pal.text, self.pal.match_fg)
        };
        let base = tf_tracked(FontId::new(size, self.theme.family()), fg, tracking);
        let mut hit = tf_tracked(FontId::new(size, self.theme.family_strong()), hit_fg, tracking);
        // The brutalist selected row is already inverted, so the match is
        // marked by a fat underline rather than another color.
        if self.theme == Theme::Brutalist && selected {
            hit.underline = Stroke::new(3.0_f32, self.pal.sel_fg);
        }
        (base, hit)
    }

    fn row_spotlight(&mut self, rect: Rect, p: &Person, selected: bool, now: i64) {
        let cy = rect.center().y;
        let dim = if selected { self.pal.sel_dim } else { self.pal.dim };
        let when = recency::short(p.last_message_ts, now);
        let wg = self.text(&when, tf(self.theme.font(self.met.meta_size), dim), 90.0);
        let mut right = self.put_right(&wg, rect.right() - self.met.row_pad_x, cy);
        if p.kind == Kind::Group {
            right -= 10.0;
            self.group_icon(Pos2::new(right - 8.0, cy), 16.0, dim);
            right -= 16.0;
        }
        let x = rect.left() + self.met.row_pad_x;
        let (base, hit) = self.name_formats(selected, self.met.name_size, 0.0);
        let hits = self.hl.hits(&p.name);
        let mut job = name_job(&p.name, hits, &base, &hit);
        one_line(&mut job, (right - 10.0 - x).max(20.0));
        let g = self.galley(job);
        self.put_left(&g, x, cy);
    }

    fn row_terminal(&mut self, rect: Rect, p: &Person, selected: bool) {
        let cy = rect.center().y;
        let x = rect.left() + self.met.row_pad_x;
        if selected {
            // The pointer occupies a column on every row; only the selected
            // one is inked, so names stay aligned.
            self.rect(
                Rect::from_center_size(Pos2::new(x + 4.0, cy), Vec2::new(3.0, 15.0)),
                0.0,
                Color32::from_rgb(0xff, 0x7b, 0x72),
            );
        }
        let name_x = x + 17.0;
        let (base, hit) = self.name_formats(selected, self.met.name_size, 0.0);
        let hits = self.hl.hits(&p.name);
        let mut job = name_job(&p.name, hits, &base, &hit);
        if p.kind == Kind::Group {
            job.append(
                "  [group]",
                0.0,
                tf(self.theme.font(self.met.name_size), self.pal.dim),
            );
        }
        one_line(&mut job, rect.right() - self.met.row_pad_x - name_x);
        let g = self.galley(job);
        self.put_left(&g, name_x, cy);
    }

    fn row_editorial(&mut self, rect: Rect, p: &Person, _selected: bool, now: i64) {
        let x = rect.left() + self.met.row_pad_x;
        let disc = Rect::from_center_size(
            Pos2::new(x + self.met.avatar * 0.5, rect.center().y),
            Vec2::splat(self.met.avatar),
        );
        self.avatar(disc, p);
        let text_x = disc.right() + 14.0;
        let room = rect.right() - self.met.row_pad_x - text_x;

        // Serif has no bold cut here, so a match is gold, not heavier.
        let base = tf(
            FontId::new(self.met.name_size, self.theme.family()),
            self.pal.text,
        );
        let hit = tf(
            FontId::new(self.met.name_size, self.theme.family()),
            self.pal.match_fg,
        );
        let hits = self.hl.hits(&p.name);
        let mut job = name_job(&p.name, hits, &base, &hit);
        one_line(&mut job, room);
        let name = self.galley(job);

        let sub = match (p.kind, p.members.len()) {
            (Kind::Group, n) if n > 0 => format!(
                "Group of {n}, {}",
                recency::trailing(p.last_message_ts, now).to_lowercase()
            ),
            (Kind::Group, _) => format!("Group, {}", recency::trailing(p.last_message_ts, now)),
            _ => recency::long(p.last_message_ts, now),
        };
        let sg = self.text(
            &sub,
            tf(
                FontId::new(self.met.meta_size, self.theme.family_meta()),
                self.pal.dim,
            ),
            room,
        );

        let total = name.size().y + 1.0 + sg.size().y;
        let top = rect.center().y - total * 0.5;
        self.ui
            .painter()
            .galley(Pos2::new(text_x, top), name.clone(), self.pal.text);
        self.ui.painter().galley(
            Pos2::new(text_x, top + name.size().y + 1.0),
            sg,
            self.pal.dim,
        );
    }

    fn row_brutalist(&mut self, rect: Rect, p: &Person, selected: bool, now: i64) {
        let cy = rect.center().y;
        let tag = if selected {
            "enter".to_string()
        } else if p.kind == Kind::Group {
            "group".to_string()
        } else {
            recency::short(p.last_message_ts, now).to_lowercase()
        };
        let tag_fg = if selected { self.pal.sel_fg } else { self.pal.dim };
        let tg = self.text(
            &tag.to_uppercase(),
            tf_tracked(
                FontId::new(self.met.meta_size, self.theme.family_strong()),
                tag_fg,
                1.32,
            ),
            120.0,
        );
        let right = self.put_right(&tg, rect.right() - self.met.side_pad, cy);

        let x = rect.left() + self.met.side_pad;
        let (base, hit) = self.name_formats(selected, self.met.name_size, -0.44);
        let hits = self.hl.hits(&p.name);
        let mut job = name_job(&p.name, hits, &base, &hit);
        one_line(&mut job, (right - 12.0 - x).max(20.0));
        let g = self.galley(job);
        self.put_left(&g, x, cy);
    }

    fn row_rich(&mut self, rect: Rect, p: &Person, selected: bool, now: i64) {
        let x = rect.left() + self.met.row_pad_x;
        let sq = Rect::from_center_size(
            Pos2::new(x + self.met.avatar * 0.5, rect.center().y),
            Vec2::splat(self.met.avatar),
        );
        self.avatar(sq, p);
        let text_x = sq.right() + 12.0;

        let when = recency::short(p.last_message_ts, now);
        let wg = self.text(
            &when,
            tf(self.theme.font(11.0), self.pal.dim),
            90.0,
        );
        let right = self.put_right(&wg, rect.right() - self.met.row_pad_x, rect.center().y);

        // Chips sit after the name, so they are measured before it is laid out.
        let mut chips: Vec<String> = Vec::new();
        if p.kind == Kind::Group {
            chips.push("group".into());
            if !p.members.is_empty() {
                chips.push(p.members.len().to_string());
            }
        }
        let chip_w: f32 = chips
            .iter()
            .map(|c| {
                self.text(c, tf(self.theme.font(10.0), self.pal.chip_fg), f32::INFINITY)
                    .size()
                    .x
                    + 12.0
                    + 4.0
            })
            .sum();

        let base = tf(
            FontId::new(self.met.name_size, self.theme.family_strong()),
            if selected { self.pal.sel_fg } else { self.pal.text },
        );
        let hit = tf(
            FontId::new(self.met.name_size, self.theme.family_strong()),
            self.pal.match_fg,
        );
        let hits = self.hl.hits(&p.name);
        let mut job = name_job(&p.name, hits, &base, &hit);
        let room = (right - 10.0 - text_x - chip_w - if chips.is_empty() { 0.0 } else { 8.0 })
            .max(40.0);
        one_line(&mut job, room);
        let name = self.galley(job);

        let sub = recency::long(p.last_message_ts, now);
        let sg = self.text(
            &sub,
            tf(FontId::new(self.met.meta_size, self.theme.family()), self.pal.dim),
            right - 10.0 - text_x,
        );

        let total = name.size().y + 2.0 + sg.size().y;
        let top = rect.center().y - total * 0.5;
        let name_h = name.size().y;
        let name_w = name.size().x;
        self.ui
            .painter()
            .galley(Pos2::new(text_x, top), name, self.pal.text);
        let mut cx = text_x + name_w + 8.0;
        for c in &chips {
            cx = self.chip_small(cx, top + name_h * 0.5, c) + 4.0;
        }
        self.ui
            .painter()
            .galley(Pos2::new(text_x, top + name_h + 2.0), sg, self.pal.dim);
    }

    /// The tiny "group" / "3" pills next to a rich-theme name.
    fn chip_small(&self, x: f32, cy: f32, label: &str) -> f32 {
        let g = self.text(label, tf(self.theme.font(10.0), self.pal.chip_fg), f32::INFINITY);
        let w = g.size().x + 12.0;
        let h = g.size().y + 2.0;
        self.rect(
            Rect::from_center_size(Pos2::new(x + w * 0.5, cy), Vec2::new(w, h)),
            4.0,
            self.pal.chip_bg,
        );
        self.ui
            .painter()
            .galley(Pos2::new(x + 6.0, cy - g.size().y * 0.5), g, self.pal.chip_fg);
        x + w
    }

    // --------------------------------------------------------------- footer

    pub fn footer(&mut self, window: Rect, s: &Screen) {
        let inner = window.shrink(self.met.border_width);
        if self.met.footer_h <= 0.0 {
            return;
        }
        let rect = Rect::from_min_max(
            Pos2::new(inner.left(), inner.bottom() - self.met.footer_h),
            inner.max,
        );
        match self.theme {
            Theme::Spotlight => self.footer_spotlight(rect, s),
            Theme::Terminal => self.footer_terminal(rect, s),
            _ => {}
        }
    }

    fn footer_spotlight(&mut self, rect: Rect, s: &Screen) {
        self.hairline(rect.top(), rect.left(), rect.right(), self.pal.rule, 1.0);
        let cy = rect.center().y;
        let mut x = rect.left() + self.met.side_pad;

        for (icon, word) in [(0, "move"), (1, "open")] {
            let g = self.text("  ", tf(self.theme.font(11.0), self.pal.chip_fg), 40.0);
            let chip_h = g.size().y + 6.0;
            let chip_w = 22.0;
            let chip = Rect::from_center_size(
                Pos2::new(x + chip_w * 0.5, cy),
                Vec2::new(chip_w, chip_h),
            );
            self.ui.painter().rect_stroke(
                chip,
                Rounding::same(4.0),
                Stroke::new(1.0_f32, Color32::from_rgba_unmultiplied(0xff, 0xff, 0xff, 36)),
            );
            if icon == 0 {
                self.updown_icon(chip.center(), 11.0, self.pal.chip_fg);
            } else {
                self.return_icon(chip.center(), 11.0, self.pal.chip_fg);
            }
            x += chip_w + 6.0;
            let wg = self.text(word, tf(self.theme.font(11.0), self.pal.chip_fg), 60.0);
            x = self.put_left(&wg, x, cy) + 14.0;
        }

        let count = format!("{} of {}", s.matches.len(), s.total);
        let cg = self.text(&count, tf(self.theme.font(11.0), self.pal.chip_fg), 120.0);
        self.put_right(&cg, rect.right() - self.met.side_pad, cy);
    }

    fn footer_terminal(&mut self, rect: Rect, s: &Screen) {
        let x = rect.left() + self.met.row_pad_x;
        // Count line, then the prompt under it.
        let count_cy = rect.top() + 13.0;
        let count = format!("  {}/{}", s.matches.len(), s.total);
        let cg = self.text(
            &count,
            tf(
                self.theme.font(self.met.name_size),
                Color32::from_rgb(0x3f, 0xb9, 0x50),
            ),
            200.0,
        );
        let after = self.put_left(&cg, x, count_cy);
        let rule_w = rect.right() - self.met.row_pad_x - after;
        // The rule is drawn, not typed, so it lands on the pixel rather than
        // on a multiple of the character cell.
        if rule_w > 0.0 {
            self.hairline(
                count_cy.round(),
                after,
                after + rule_w,
                self.pal.rule,
                1.0,
            );
        }

        let prompt_cy = rect.top() + 26.0 + 15.0;
        let pg = self.text(
            ">",
            tf(self.theme.font(self.met.query_size), self.pal.accent),
            40.0,
        );
        let mut px = self.put_left(&pg, x, prompt_cy) + 8.0;
        let qg = self.text(
            s.query,
            tf(self.theme.font(self.met.query_size), self.pal.sel_fg),
            rect.width() - 80.0,
        );
        px = self.put_left(&qg, px, prompt_cy);
        self.rect(
            Rect::from_min_size(
                Pos2::new(px + 1.0, prompt_cy - self.met.caret_h * 0.5),
                Vec2::new(self.met.caret_w, self.met.caret_h),
            ),
            0.0,
            self.pal.text,
        );
    }
}

/// Points along a circular arc, in degrees clockwise from three o'clock.
fn arc(center: Pos2, r: f32, from: f32, to: f32) -> Vec<Pos2> {
    let n = 10;
    (0..=n)
        .map(|i| {
            let t = (from + (to - from) * i as f32 / n as f32).to_radians();
            Pos2::new(center.x + r * t.cos(), center.y + r * t.sin())
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Kind;

    fn person(name: &str, kind: Kind) -> Person {
        Person {
            name: name.into(),
            handles: vec![],
            last_message_ts: 1,
            kind,
            guid: None,
            best_handle: None,
            members: vec![],
            named: false,
        }
    }

    #[test]
    fn highlighter_marks_the_matched_characters() {
        let mut hl = Highlighter::new();
        hl.set_query("gra");
        assert_eq!(hl.hits("Grace Hopper"), &[0, 1, 2]);
        assert_eq!(hl.hits("Paul Graham"), &[5, 6, 7]);
        assert!(hl.hits("Edsger Dijkstra").len() >= 3);
        hl.set_query("   ");
        assert!(hl.hits("Grace Hopper").is_empty());
    }

    #[test]
    fn name_job_splits_into_matched_and_unmatched_runs() {
        let base = tf(FontId::proportional(12.0), Color32::WHITE);
        let hit = tf(FontId::proportional(12.0), Color32::RED);
        let job = name_job("Paul Graham", &[5, 6, 7], &base, &hit);
        let text: String = job.sections.iter().map(|s| &job.text[s.byte_range.clone()]).collect();
        assert_eq!(text, "Paul Graham");
        assert_eq!(job.sections.len(), 3);
        assert_eq!(&job.text[job.sections[1].byte_range.clone()], "Gra");
        assert_eq!(job.sections[1].format.color, Color32::RED);
    }

    #[test]
    fn a_match_at_the_start_or_the_whole_name_still_works() {
        let base = tf(FontId::proportional(12.0), Color32::WHITE);
        let hit = tf(FontId::proportional(12.0), Color32::RED);
        assert_eq!(name_job("Ada", &[0, 1, 2], &base, &hit).sections.len(), 1);
        assert_eq!(name_job("Ada", &[], &base, &hit).sections.len(), 1);
        assert!(name_job("", &[], &base, &hit).sections.is_empty());
        // Multi-byte names index by character, not by byte.
        let job = name_job("Émile Borel", &[0], &base, &hit);
        assert_eq!(&job.text[job.sections[0].byte_range.clone()], "É");
    }

    #[test]
    fn every_theme_fits_at_least_a_few_rows() {
        for name in crate::theme::THEME_NAMES {
            let t: Theme = name.parse().unwrap();
            let cap = capacity(t, Vec2::new(560.0, 420.0));
            assert!((4..=20).contains(&cap), "{name} shows {cap} rows");
        }
    }

    #[test]
    fn the_filter_cycles_and_selects() {
        assert_eq!(Filter::All.next().next().next(), Filter::All);
        assert!(Filter::All.accepts(&person("Ada", Kind::Person)));
        assert!(!Filter::Groups.accepts(&person("Ada", Kind::Person)));
        assert!(Filter::Groups.accepts(&person("Trip", Kind::Group)));
        assert!(!Filter::People.accepts(&person("Trip", Kind::Group)));
    }
}
