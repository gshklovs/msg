use anyhow::{anyhow, bail, Result};
use eframe::egui;
use global_hotkey::hotkey::{Code, HotKey, Modifiers};
use global_hotkey::{GlobalHotKeyEvent, GlobalHotKeyManager};
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::Instant;

use crate::draw::{self, Filter, Highlighter, Screen, View};
use crate::model::Person;
use crate::theme::Theme;
use crate::tui::Picker;

/// Parse a config string like `cmd+shift+m` into a hotkey.
///
/// `cmd`/`super`/`win` are the Command key, `opt` is Option, and the last
/// segment is the key itself.
pub fn parse_hotkey(spec: &str) -> Result<HotKey> {
    let parts: Vec<&str> = spec
        .split('+')
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .collect();
    let Some((key, mods)) = parts.split_last() else {
        bail!("empty hotkey");
    };
    let mut modifiers = Modifiers::empty();
    for m in mods {
        modifiers |= match m.to_ascii_lowercase().as_str() {
            "cmd" | "command" | "super" | "win" | "meta" => Modifiers::SUPER,
            "ctrl" | "control" => Modifiers::CONTROL,
            "alt" | "opt" | "option" => Modifiers::ALT,
            "shift" => Modifiers::SHIFT,
            other => bail!("unknown modifier `{other}` in hotkey `{spec}`"),
        };
    }
    Ok(HotKey::new(Some(modifiers), parse_code(key)?))
}

fn parse_code(key: &str) -> Result<Code> {
    let k = key.to_ascii_lowercase();
    if k.len() == 1 {
        let c = k.chars().next().unwrap();
        if c.is_ascii_alphabetic() {
            return Ok(match c {
                'a' => Code::KeyA, 'b' => Code::KeyB, 'c' => Code::KeyC, 'd' => Code::KeyD,
                'e' => Code::KeyE, 'f' => Code::KeyF, 'g' => Code::KeyG, 'h' => Code::KeyH,
                'i' => Code::KeyI, 'j' => Code::KeyJ, 'k' => Code::KeyK, 'l' => Code::KeyL,
                'm' => Code::KeyM, 'n' => Code::KeyN, 'o' => Code::KeyO, 'p' => Code::KeyP,
                'q' => Code::KeyQ, 'r' => Code::KeyR, 's' => Code::KeyS, 't' => Code::KeyT,
                'u' => Code::KeyU, 'v' => Code::KeyV, 'w' => Code::KeyW, 'x' => Code::KeyX,
                'y' => Code::KeyY, _ => Code::KeyZ,
            });
        }
        if c.is_ascii_digit() {
            return Ok(match c {
                '0' => Code::Digit0, '1' => Code::Digit1, '2' => Code::Digit2,
                '3' => Code::Digit3, '4' => Code::Digit4, '5' => Code::Digit5,
                '6' => Code::Digit6, '7' => Code::Digit7, '8' => Code::Digit8,
                _ => Code::Digit9,
            });
        }
    }
    Ok(match k.as_str() {
        "space" => Code::Space,
        "enter" | "return" => Code::Enter,
        "tab" => Code::Tab,
        "escape" | "esc" => Code::Escape,
        "f1" => Code::F1, "f2" => Code::F2, "f3" => Code::F3, "f4" => Code::F4,
        "f5" => Code::F5, "f6" => Code::F6, "f7" => Code::F7, "f8" => Code::F8,
        "f9" => Code::F9, "f10" => Code::F10, "f11" => Code::F11, "f12" => Code::F12,
        other => return Err(anyhow!("unknown key `{other}`")),
    })
}

const WIDTH: f32 = 560.0;
const HEIGHT: f32 = 420.0;

/// What one keypress asked the picker to do.
enum Act {
    None,
    Cancel,
    Accept,
}

struct App {
    picker: Picker,
    /// Indices into `picker.people` after the theme's chip filter, best first.
    rows: Vec<usize>,
    cursor: usize,
    /// First row drawn, so a long list never lays out off-screen text.
    top: usize,
    theme: Theme,
    filter: Filter,
    hl: Highlighter,
    visible: bool,
    /// eframe on macOS shows the window after the first frame even when built
    /// with `with_visible(false)`, so the first frame hides it explicitly.
    first_frame: bool,
    /// Set whenever the selection moves, so the list scrolls to it once
    /// rather than every frame (which fought the user's own scrolling).
    scroll_to_cursor: bool,
    /// Whether the window has been seen focused since it was shown; a focus
    /// loss after that means the user clicked elsewhere, so hide.
    had_focus: bool,
    wake: Receiver<()>,
    log: fn(&str),
    /// `MSG_PERF=1` prints one timing line per popup and per query change.
    perf: bool,
    perf_show: Option<Instant>,
    perf_key: Option<Instant>,
}

impl App {
    /// Re-rank, then apply the chip filter. Called on every query change.
    fn refilter(&mut self) {
        self.picker.refilter();
        self.hl.set_query(&self.picker.query);
        self.apply_filter();
    }

    fn apply_filter(&mut self) {
        self.rows.clear();
        self.rows.extend(
            self.picker
                .matches
                .iter()
                .copied()
                .filter(|i| self.filter.accepts(&self.picker.people[*i])),
        );
        self.cursor = 0;
        self.top = 0;
    }

    fn selected(&self) -> Option<&Person> {
        self.rows.get(self.cursor).map(|i| &self.picker.people[*i])
    }

    fn move_cursor(&mut self, delta: isize) {
        if self.rows.is_empty() {
            return;
        }
        let last = self.rows.len() as isize - 1;
        self.cursor = (self.cursor as isize + delta).clamp(0, last) as usize;
        self.scroll_to_cursor = true;
    }

    fn show(&mut self, ctx: &egui::Context) {
        (self.log)("hotkey: showing picker");
        self.perf_show = self.perf.then(Instant::now);
        // Pick up new conversations since the last popup.
        if let Ok(people) = crate::index::load() {
            self.picker.people = people;
        }
        self.picker.query.clear();
        self.filter = Filter::All;
        self.refilter();
        self.visible = true;
        self.scroll_to_cursor = true;
        self.had_focus = false;
        if let Some(monitor) = ctx.input(|i| i.viewport().monitor_size) {
            let pos = ((monitor - egui::vec2(WIDTH, HEIGHT)) * 0.5).to_pos2();
            ctx.send_viewport_cmd(egui::ViewportCommand::OuterPosition(pos));
        }
        ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
        ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
    }

    fn hide(&mut self, ctx: &egui::Context) {
        if !self.visible {
            return;
        }
        self.visible = false;
        ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
    }

    /// Drain this frame's keyboard input into the query and the cursor.
    ///
    /// The query is edited by hand rather than through a `TextEdit` because
    /// every theme draws its own prompt: an italic serif line, a fat uppercase
    /// slab, a monospace `>` at the bottom of the window.
    fn keys(&mut self, ctx: &egui::Context) -> Act {
        let mut act = Act::None;
        let mut changed = false;
        let mut scroll = 0.0;
        ctx.input(|i| {
            scroll = i.raw_scroll_delta.y;
            for event in &i.events {
                match event {
                    egui::Event::Text(text) => {
                        self.picker.query.push_str(text);
                        changed = true;
                    }
                    egui::Event::Key {
                        key,
                        pressed: true,
                        modifiers,
                        ..
                    } => {
                        let ctrl = modifiers.ctrl || modifiers.mac_cmd;
                        match key {
                            egui::Key::Escape => act = Act::Cancel,
                            egui::Key::Enter => act = Act::Accept,
                            egui::Key::ArrowDown => self.move_cursor(1),
                            egui::Key::ArrowUp => self.move_cursor(-1),
                            egui::Key::N if ctrl => self.move_cursor(1),
                            egui::Key::P if ctrl => self.move_cursor(-1),
                            egui::Key::U if ctrl => {
                                self.picker.query.clear();
                                changed = true;
                            }
                            egui::Key::Backspace => {
                                if ctrl {
                                    self.picker.query.clear();
                                } else {
                                    self.picker.query.pop();
                                }
                                changed = true;
                            }
                            egui::Key::Tab => {
                                self.filter = self.filter.next();
                                self.apply_filter();
                                self.scroll_to_cursor = true;
                            }
                            _ => {}
                        }
                    }
                    _ => {}
                }
            }
        });
        if changed {
            self.perf_key = self.perf.then(Instant::now);
            self.refilter();
            self.scroll_to_cursor = true;
        }
        if scroll != 0.0 {
            let step = (scroll / 24.0).round() as isize;
            self.top = (self.top as isize - step).max(0) as usize;
        }
        act
    }
}

impl eframe::App for App {
    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        if self.theme.metrics().transparent {
            [0.0, 0.0, 0.0, 0.0]
        } else {
            let c = self.theme.palette().window_bg;
            let [r, g, b, _] = c.to_normalized_gamma_f32();
            [r, g, b, 1.0]
        }
    }

    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if self.first_frame {
            self.first_frame = false;
            ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
        }
        while self.wake.try_recv().is_ok() {
            if self.visible {
                self.hide(ctx);
            } else {
                self.show(ctx);
            }
        }
        if !self.visible {
            // Nothing to draw. Sit still until the wake thread repaints us;
            // repainting here instead would spin the CPU behind a hidden window.
            return;
        }
        let started = self.perf.then(Instant::now);

        // Click-off: hide when focus leaves after we have had it.
        let focused = ctx.input(|i| i.viewport().focused).unwrap_or(true);
        if focused {
            self.had_focus = true;
        } else if self.had_focus {
            self.hide(ctx);
            return;
        }

        let act = self.keys(ctx);
        let mut chosen: Option<Person> = match act {
            Act::Accept => self.selected().cloned(),
            _ => None,
        };

        let panel = egui::CentralPanel::default().frame(egui::Frame::none());
        panel.show(ctx, |ui| {
            let window = ui.max_rect();
            let cap = draw::capacity(self.theme, window.size());
            // Keep the cursor on screen, but only when it just moved, so that
            // scrolling with the wheel is not fought by the selection.
            if self.scroll_to_cursor {
                if self.cursor < self.top {
                    self.top = self.cursor;
                } else if self.cursor >= self.top + cap {
                    self.top = self.cursor + 1 - cap;
                }
            }
            self.top = self.top.min(self.rows.len().saturating_sub(cap));

            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs() as i64)
                .unwrap_or(0);
            let screen = Screen {
                query: &self.picker.query,
                people: &self.picker.people,
                matches: &self.rows,
                cursor: self.cursor,
                top: self.top,
                total: self.picker.people.len(),
                filter: self.filter,
                now,
            };
            let mut view = View::new(ui, self.theme, &mut self.hl);
            view.window(window);
            view.header(window, &screen);
            let clicked = view.list(window, &screen);
            view.footer(window, &screen);
            if let Some(row) = clicked {
                chosen = self.rows.get(row).map(|i| self.picker.people[*i].clone());
            }
        });

        self.scroll_to_cursor = false;
        if let (Some(started), true) = (started, self.perf) {
            let ms = started.elapsed().as_secs_f64() * 1000.0;
            if let Some(t0) = self.perf_key.take() {
                (self.log)(&format!(
                    "perf: keystroke to painted frame {:.2} ms (update body {ms:.2} ms, {} matches)",
                    t0.elapsed().as_secs_f64() * 1000.0,
                    self.rows.len()
                ));
            }
            if let Some(t0) = self.perf_show.take() {
                (self.log)(&format!(
                    "perf: hotkey to first painted frame {:.2} ms",
                    t0.elapsed().as_secs_f64() * 1000.0
                ));
            }
        }

        if matches!(act, Act::Cancel) {
            self.hide(ctx);
        }
        if let Some(person) = chosen {
            if let Err(e) = crate::action::open(&person) {
                (self.log)(&format!("open failed: {e:#}"));
            }
            self.hide(ctx);
        }
    }
}

/// Run the resident daemon: a hidden window that the hotkey toggles.
pub fn daemon(hotkey_spec: &str, theme: Theme, log: fn(&str)) -> Result<()> {
    let hotkey = parse_hotkey(hotkey_spec)?;
    let people = crate::index::load()?;
    let met = theme.metrics();

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([WIDTH, HEIGHT])
            .with_decorations(false)
            .with_always_on_top()
            .with_resizable(false)
            // Rounded corners need the pixels outside them to be see-through.
            .with_transparent(met.transparent)
            .with_visible(false),
        ..Default::default()
    };

    let (tx, rx): (Sender<()>, Receiver<()>) = mpsc::channel();
    eframe::run_native(
        "msg",
        options,
        Box::new(move |cc| {
            // The manager must outlive the app and be built on the thread that
            // owns the event loop, so it is leaked into the callback.
            let manager = Box::leak(Box::new(GlobalHotKeyManager::new().map_err(
                |e| format!("cannot create the global hotkey manager: {e}"),
            )?));
            manager
                .register(hotkey)
                .map_err(|e| format!("cannot register hotkey `{hotkey_spec}`: {e}"))?;
            log(&format!("registered hotkey id {}", hotkey.id()));

            // Hotkey events arrive on a channel; wake the UI thread from here so
            // the popup appears even while the window is hidden.
            let ctx = cc.egui_ctx.clone();
            std::thread::spawn(move || {
                let rx = GlobalHotKeyEvent::receiver();
                while let Ok(event) = rx.recv() {
                    log(&format!("hotkey event {event:?}"));
                    if event.state == global_hotkey::HotKeyState::Pressed
                        && tx.send(()).is_ok()
                    {
                        ctx.request_repaint();
                    }
                }
            });

            // One font atlas for the whole run; rebuilding it per frame would
            // re-rasterize every glyph.
            crate::theme::install_fonts(&cc.egui_ctx);
            let mut visuals = egui::Visuals::dark();
            visuals.panel_fill = egui::Color32::TRANSPARENT;
            visuals.window_fill = egui::Color32::TRANSPARENT;
            cc.egui_ctx.set_visuals(visuals);
            cc.egui_ctx.style_mut(|st| {
                st.spacing.item_spacing = egui::vec2(0.0, 0.0);
                st.interaction.selectable_labels = false;
            });

            let mut app = App {
                picker: Picker::new(people),
                rows: Vec::new(),
                cursor: 0,
                top: 0,
                theme,
                filter: Filter::All,
                hl: Highlighter::new(),
                visible: false,
                first_frame: true,
                scroll_to_cursor: false,
                had_focus: false,
                wake: rx,
                log,
                perf: std::env::var_os("MSG_PERF").is_some(),
                perf_show: None,
                perf_key: None,
            };
            app.apply_filter();
            Ok(Box::new(app) as Box<dyn eframe::App>)
        }),
    )
    .map_err(|e| anyhow!("{e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_the_default_hotkey() {
        let hk = parse_hotkey("cmd+shift+m").unwrap();
        assert_eq!(hk.mods, Modifiers::SUPER | Modifiers::SHIFT);
        assert_eq!(hk.key, Code::KeyM);
    }

    #[test]
    fn accepts_aliases_and_odd_spacing() {
        let hk = parse_hotkey(" Control + Option + Space ").unwrap();
        assert_eq!(hk.mods, Modifiers::CONTROL | Modifiers::ALT);
        assert_eq!(hk.key, Code::Space);
        assert_eq!(parse_hotkey("super+f5").unwrap().key, Code::F5);
        assert_eq!(parse_hotkey("cmd+1").unwrap().key, Code::Digit1);
    }

    #[test]
    fn rejects_nonsense() {
        assert!(parse_hotkey("").is_err());
        assert!(parse_hotkey("hyper+m").is_err());
        assert!(parse_hotkey("cmd+banana").is_err());
    }
}
