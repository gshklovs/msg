use anyhow::{anyhow, bail, Result};
use eframe::egui;
use global_hotkey::hotkey::{Code, HotKey, Modifiers};
use global_hotkey::{GlobalHotKeyEvent, GlobalHotKeyManager};
use std::sync::mpsc::{self, Receiver, Sender};

use crate::model::Person;
use crate::tui::{label, Picker};

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

struct App {
    picker: Picker,
    visible: bool,
    /// eframe on macOS shows the window after the first frame even when built
    /// with `with_visible(false)`, so the first frame hides it explicitly.
    first_frame: bool,
    focus_query: bool,
    /// Set whenever the selection moves, so the list scrolls to it once
    /// rather than every frame (which fought the user's own scrolling).
    scroll_to_cursor: bool,
    /// Whether the window has been seen focused since it was shown; a focus
    /// loss after that means the user clicked elsewhere, so hide.
    had_focus: bool,
    wake: Receiver<()>,
    log: fn(&str),
}

impl App {
    fn show(&mut self, ctx: &egui::Context) {
        (self.log)("hotkey: showing picker");
        // Pick up new conversations since the last popup.
        if let Ok(people) = crate::index::load() {
            self.picker.people = people;
        }
        self.picker.query.clear();
        self.picker.refilter();
        self.visible = true;
        self.focus_query = true;
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
}

impl eframe::App for App {
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

        // Click-off: hide when focus leaves after we have had it.
        let focused = ctx.input(|i| i.viewport().focused).unwrap_or(true);
        if focused {
            self.had_focus = true;
        } else if self.had_focus {
            self.hide(ctx);
            return;
        }

        let mut chosen: Option<Person> = None;
        let mut cancel = false;

        let panel = egui::CentralPanel::default().frame(
            egui::Frame::none()
                .fill(egui::Color32::from_rgb(0x14, 0x15, 0x1a))
                .inner_margin(egui::Margin::symmetric(18.0, 14.0)),
        );
        panel.show(ctx, |ui| {
            ctx.input(|i| {
                if i.key_pressed(egui::Key::Escape) {
                    cancel = true;
                }
                if i.key_pressed(egui::Key::ArrowDown) {
                    self.picker.move_cursor(1);
                    self.scroll_to_cursor = true;
                }
                if i.key_pressed(egui::Key::ArrowUp) {
                    self.picker.move_cursor(-1);
                    self.scroll_to_cursor = true;
                }
            });

            let before = self.picker.query.clone();
            let edit = ui.add(
                egui::TextEdit::singleline(&mut self.picker.query)
                    .hint_text("who?")
                    .desired_width(f32::INFINITY)
                    .frame(false)
                    .margin(egui::vec2(4.0, 8.0))
                    .font(egui::FontId::proportional(30.0)),
            );
            if self.focus_query {
                edit.request_focus();
                self.focus_query = false;
            }
            if self.picker.query != before {
                self.picker.refilter();
                self.scroll_to_cursor = true;
            }
            if edit.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                chosen = self.picker.selected().cloned();
            }

            ui.add_space(6.0);
            ui.separator();
            ui.add_space(6.0);
            egui::ScrollArea::vertical().show(ui, |ui| {
                let matches = self.picker.matches.clone();
                for (row, i) in matches.iter().enumerate().take(200) {
                    let selected = row == self.picker.cursor;
                    let person = &self.picker.people[*i];
                    let mut text = egui::RichText::new(label(person)).size(19.0);
                    if selected {
                        text = text.strong().color(egui::Color32::WHITE);
                    } else {
                        text = text.color(egui::Color32::from_gray(0xc8));
                    }
                    let (rect, resp) = ui.allocate_exact_size(
                        egui::vec2(ui.available_width(), 36.0),
                        egui::Sense::click(),
                    );
                    if selected {
                        ui.painter().rect_filled(rect, 8.0, egui::Color32::from_rgb(0x2f, 0x6f, 0xed));
                    } else if resp.hovered() {
                        ui.painter().rect_filled(rect, 8.0, egui::Color32::from_rgb(0x22, 0x24, 0x2c));
                    }
                    ui.allocate_new_ui(egui::UiBuilder::new().max_rect(rect.shrink2(egui::vec2(14.0, 0.0))), |ui| {
                        ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                            ui.label(text);
                        });
                    });
                    if selected && self.scroll_to_cursor {
                        ui.scroll_to_rect(rect, None);
                    }
                    if resp.clicked() {
                        chosen = Some(person.clone());
                    }
                }
            });
        });

        self.scroll_to_cursor = false;

        if cancel {
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
pub fn daemon(hotkey_spec: &str, log: fn(&str)) -> Result<()> {
    let hotkey = parse_hotkey(hotkey_spec)?;
    let people = crate::index::load()?;

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([WIDTH, HEIGHT])
            .with_decorations(false)
            .with_always_on_top()
            .with_resizable(false)
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

            let mut visuals = egui::Visuals::dark();
            visuals.selection.bg_fill = egui::Color32::from_rgb(0x2f, 0x6f, 0xed);
            visuals.selection.stroke = egui::Stroke::NONE;
            visuals.widgets.hovered.weak_bg_fill = egui::Color32::from_rgb(0x22, 0x24, 0x2c);
            visuals.widgets.active.weak_bg_fill = egui::Color32::from_rgb(0x2f, 0x6f, 0xed);
            visuals.widgets.inactive.rounding = egui::Rounding::same(8.0);
            visuals.widgets.hovered.rounding = egui::Rounding::same(8.0);
            visuals.widgets.active.rounding = egui::Rounding::same(8.0);
            visuals.extreme_bg_color = egui::Color32::from_rgb(0x14, 0x15, 0x1a);
            cc.egui_ctx.set_visuals(visuals);
            cc.egui_ctx.style_mut(|st| {
                st.spacing.item_spacing = egui::vec2(8.0, 4.0);
                st.spacing.button_padding = egui::vec2(12.0, 6.0);
            });

            Ok(Box::new(App {
                picker: Picker::new(people),
                visible: false,
                first_frame: true,
                focus_query: false,
                scroll_to_cursor: false,
                had_focus: false,
                wake: rx,
                log,
            }))
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
