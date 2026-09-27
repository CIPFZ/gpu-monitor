//! The key map is data, and the help text is generated from that same data.
//!
//! The previous footer was a hand-written string that drifted from the handler:
//! documented keys were missing and working keys were undocumented. Here every
//! binding is declared once, `resolve` interprets it, and tests prove that each
//! advertised key actually produces an action.

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

use crate::{
    action::{Action, Scroll, SearchEdit},
    view::View,
};

/// Chart windows offered at runtime, in milliseconds.
pub const HISTORY_WINDOWS_MS: [u64; 3] = [60_000, 300_000, 3_600_000];

/// A declared key, kept structured so it can be both rendered and replayed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Key {
    Char(char),
    Ctrl(char),
    /// A named key together with the glyph shown to the user.
    Named(KeyCode, &'static str),
}

impl Key {
    pub fn label(self) -> String {
        match self {
            Self::Char(' ') => "Space".to_owned(),
            Self::Char(character) => character.to_string(),
            Self::Ctrl(character) => format!("Ctrl-{}", character.to_ascii_uppercase()),
            Self::Named(_, label) => label.to_owned(),
        }
    }

    /// The event a user would generate by pressing this key. Used to prove that
    /// every advertised binding is handled.
    #[cfg(test)]
    pub fn event(self) -> KeyEvent {
        match self {
            Self::Char(character) => KeyEvent::new(KeyCode::Char(character), KeyModifiers::NONE),
            Self::Ctrl(character) => KeyEvent::new(KeyCode::Char(character), KeyModifiers::CONTROL),
            Self::Named(code, _) => KeyEvent::new(code, KeyModifiers::NONE),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Group {
    Devices,
    Views,
    Reading,
    Session,
}

impl Group {
    pub const ALL: [Self; 4] = [Self::Devices, Self::Views, Self::Reading, Self::Session];

    pub fn title(self) -> &'static str {
        match self {
            Self::Devices => "Devices",
            Self::Views => "Views",
            Self::Reading => "Reading data",
            Self::Session => "Session",
        }
    }
}

pub struct Binding {
    pub keys: &'static [Key],
    pub description: &'static str,
    pub group: Group,
}

impl Binding {
    pub fn keys_label(&self) -> String {
        self.keys
            .iter()
            .map(|key| key.label())
            .collect::<Vec<_>>()
            .join(" / ")
    }
}

pub const BINDINGS: &[Binding] = &[
    Binding {
        keys: &[
            Key::Named(KeyCode::Left, "←"),
            Key::Char('h'),
            Key::Char('['),
        ],
        description: "Previous device",
        group: Group::Devices,
    },
    Binding {
        keys: &[
            Key::Named(KeyCode::Right, "→"),
            Key::Char('l'),
            Key::Char(']'),
        ],
        description: "Next device",
        group: Group::Devices,
    },
    Binding {
        keys: &[Key::Char('b')],
        description: "Show or hide the device sidebar",
        group: Group::Devices,
    },
    Binding {
        keys: &[Key::Named(KeyCode::Tab, "Tab")],
        description: "Next view",
        group: Group::Views,
    },
    Binding {
        keys: &[Key::Named(KeyCode::BackTab, "Shift-Tab")],
        description: "Previous view",
        group: Group::Views,
    },
    Binding {
        keys: &[
            Key::Char('1'),
            Key::Char('2'),
            Key::Char('3'),
            Key::Char('4'),
            Key::Char('5'),
        ],
        description: "Select a view by number",
        group: Group::Views,
    },
    Binding {
        keys: &[
            Key::Named(KeyCode::Up, "↑"),
            Key::Char('k'),
            Key::Named(KeyCode::Down, "↓"),
            Key::Char('j'),
        ],
        description: "Scroll one row",
        group: Group::Reading,
    },
    Binding {
        keys: &[
            Key::Named(KeyCode::PageUp, "PageUp"),
            Key::Named(KeyCode::PageDown, "PageDown"),
        ],
        description: "Scroll one page",
        group: Group::Reading,
    },
    Binding {
        keys: &[
            Key::Named(KeyCode::Home, "Home"),
            Key::Named(KeyCode::End, "End"),
        ],
        description: "Jump to the first or last row",
        group: Group::Reading,
    },
    Binding {
        keys: &[Key::Char('/')],
        description: "Filter processes by name, PID or owner",
        group: Group::Reading,
    },
    Binding {
        keys: &[Key::Char('s')],
        description: "Change the process sort order",
        group: Group::Reading,
    },
    Binding {
        keys: &[Key::Char('c')],
        description: "Show full process command lines",
        group: Group::Reading,
    },
    Binding {
        keys: &[Key::Char('w')],
        description: "Change the chart window: 1m, 5m or 1h",
        group: Group::Reading,
    },
    Binding {
        keys: &[Key::Char('r')],
        description: "Retry driver initialisation now",
        group: Group::Session,
    },
    Binding {
        keys: &[Key::Char('?')],
        description: "Show or hide this help",
        group: Group::Session,
    },
    Binding {
        keys: &[
            Key::Char('q'),
            Key::Named(KeyCode::Esc, "Esc"),
            Key::Ctrl('c'),
        ],
        description: "Quit",
        group: Group::Session,
    },
];

/// The status bar has room for a noun, so it advertises representative keys only.
/// Each one is still a real binding, which the tests enforce.
const FOOTER: &[(&[Key], &str)] = &[
    (
        &[
            Key::Named(KeyCode::Left, "←"),
            Key::Named(KeyCode::Right, "→"),
        ],
        "device",
    ),
    (&[Key::Named(KeyCode::Tab, "Tab")], "view"),
    (
        &[Key::Named(KeyCode::Up, "↑"), Key::Named(KeyCode::Down, "↓")],
        "scroll",
    ),
    (&[Key::Char('/')], "filter"),
    (&[Key::Char('s')], "sort"),
    (&[Key::Char('w')], "window"),
    (&[Key::Char('?')], "help"),
    (&[Key::Char('q')], "quit"),
];

/// Maps a key press to an intent. `editing_search` routes printable characters
/// into the query instead of triggering commands.
pub fn resolve(key: KeyEvent, editing_search: bool) -> Option<Action> {
    if key.kind == KeyEventKind::Release {
        return None;
    }
    if key.modifiers.contains(KeyModifiers::CONTROL) {
        // An interrupt must never be swallowed by a text field.
        return match key.code {
            KeyCode::Char('c') => Some(Action::Quit),
            _ => None,
        };
    }
    if editing_search {
        return match key.code {
            KeyCode::Esc => Some(Action::Search(SearchEdit::Cancel)),
            KeyCode::Enter => Some(Action::Search(SearchEdit::Commit)),
            KeyCode::Backspace => Some(Action::Search(SearchEdit::Pop)),
            KeyCode::Char(character) if !character.is_control() => {
                Some(Action::Search(SearchEdit::Push(character)))
            }
            _ => None,
        };
    }
    match key.code {
        KeyCode::Char('q') | KeyCode::Esc => Some(Action::Quit),
        KeyCode::Char('r') => Some(Action::Retry),
        KeyCode::Char('?') => Some(Action::ToggleHelp),
        KeyCode::Char('b') => Some(Action::ToggleSidebar),
        KeyCode::Char('c') => Some(Action::ToggleCommandColumn),
        KeyCode::Char('s') => Some(Action::CycleProcessSort),
        KeyCode::Char('w') => Some(Action::CycleHistoryWindow),
        KeyCode::Char('/') => Some(Action::Search(SearchEdit::Begin)),
        KeyCode::Left | KeyCode::Char('h') | KeyCode::Char('[') => Some(Action::PreviousDevice),
        KeyCode::Right | KeyCode::Char('l') | KeyCode::Char(']') => Some(Action::NextDevice),
        KeyCode::Tab => Some(Action::CycleView(true)),
        KeyCode::BackTab => Some(Action::CycleView(false)),
        KeyCode::Char(digit) if View::from_hotkey(digit).is_some() => {
            View::from_hotkey(digit).map(Action::GoToView)
        }
        KeyCode::Up | KeyCode::Char('k') => Some(Action::Scroll(Scroll::LineUp)),
        KeyCode::Down | KeyCode::Char('j') => Some(Action::Scroll(Scroll::LineDown)),
        KeyCode::PageUp => Some(Action::Scroll(Scroll::PageUp)),
        KeyCode::PageDown => Some(Action::Scroll(Scroll::PageDown)),
        KeyCode::Home => Some(Action::Scroll(Scroll::Top)),
        KeyCode::End => Some(Action::Scroll(Scroll::Bottom)),
        _ => None,
    }
}

/// Compact hints for the status bar, generated from the declarations above.
pub fn footer_hint() -> String {
    FOOTER
        .iter()
        .map(|(keys, noun)| {
            let keys = keys
                .iter()
                .map(|key| key.label())
                .collect::<Vec<_>>()
                .join("/");
            format!("{keys} {noun}")
        })
        .collect::<Vec<_>>()
        .join("  ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_documented_key_is_actually_handled() {
        for binding in BINDINGS {
            for key in binding.keys {
                assert!(
                    resolve(key.event(), false).is_some(),
                    "{} is documented as \"{}\" but produces no action",
                    key.label(),
                    binding.description
                );
            }
        }
    }

    #[test]
    fn the_footer_only_advertises_real_documented_bindings() {
        for (keys, noun) in FOOTER {
            for key in *keys {
                assert!(
                    resolve(key.event(), false).is_some(),
                    "footer hint \"{noun}\" uses an unhandled key"
                );
                assert!(
                    BINDINGS.iter().any(|binding| binding.keys.contains(key)),
                    "footer hint \"{noun}\" uses a key missing from the help"
                );
            }
        }
        let hint = footer_hint();
        assert!(hint.contains("←/→ device"));
        assert!(hint.contains("Tab view"));
        assert!(hint.contains("q quit"));
    }

    #[test]
    fn documented_keys_are_not_claimed_twice() {
        let mut declared = BINDINGS
            .iter()
            .flat_map(|binding| binding.keys.iter().copied())
            .collect::<Vec<_>>();
        let total = declared.len();
        declared.sort_by_key(|key| key.label());
        declared.dedup_by_key(|key| key.label());
        assert_eq!(declared.len(), total, "a key is documented in two groups");
    }

    #[test]
    fn released_keys_and_unknown_combinations_do_nothing() {
        let mut release = Key::Char('q').event();
        release.kind = KeyEventKind::Release;
        assert_eq!(resolve(release, false), None);
        assert_eq!(
            resolve(
                KeyEvent::new(KeyCode::Char('z'), KeyModifiers::CONTROL),
                false
            ),
            None
        );
        assert_eq!(resolve(Key::Char('z').event(), false), None);
    }

    #[test]
    fn typing_a_query_cannot_trigger_commands_but_interrupts_still_work() {
        for character in ['q', 'r', 'j', '/', '1'] {
            assert_eq!(
                resolve(Key::Char(character).event(), true),
                Some(Action::Search(SearchEdit::Push(character))),
                "{character} must be text while the query is being edited"
            );
        }
        assert_eq!(resolve(Key::Ctrl('c').event(), true), Some(Action::Quit));
        assert_eq!(
            resolve(Key::Named(KeyCode::Esc, "Esc").event(), true),
            Some(Action::Search(SearchEdit::Cancel))
        );
        assert_eq!(
            resolve(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE), true),
            Some(Action::Search(SearchEdit::Commit))
        );
    }

    #[test]
    fn digits_select_views_and_windows_are_ordered() {
        assert_eq!(
            resolve(Key::Char('3').event(), false),
            Some(Action::GoToView(View::History))
        );
        assert!(HISTORY_WINDOWS_MS.windows(2).all(|pair| pair[0] < pair[1]));
        assert_eq!(
            resolve(Key::Char('w').event(), false),
            Some(Action::CycleHistoryWindow)
        );
    }
}
