//! Menu-bar indicator selection. Pure so every combination is unit-tested.
//!
//! Rules (also shown in Settings):
//! - Only enabled providers can be displayed. A provider that is disabled or
//!   not configured is `None` here.
//! - If the chosen provider(s) are all inactive, the indicator falls back to the
//!   other active provider; with none active it shows "--".
//! - An enabled provider in error is never hidden: it shows "!" when selected,
//!   or a trailing "!" when it is active but not selected.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Display {
    /// Remaining percentage of the short window.
    Percent(u32),
    Pending,
    Error,
}

#[derive(Debug, PartialEq, Eq)]
pub struct Indicator {
    pub title: String,
    pub tooltip: String,
    /// Single value usable for a numeric tray icon (non-macOS).
    pub icon_value: Option<u32>,
    /// Two providers shown: (full name, value) lines stacked in the macOS menu bar.
    pub stack: Vec<(&'static str, String)>,
}

fn piece(d: Display) -> String {
    match d {
        Display::Percent(p) => format!("{p}%"),
        Display::Pending => "--".into(),
        Display::Error => "!".into(),
    }
}

pub fn compute(choice: &str, codex: Option<Display>, claude: Option<Display>) -> Indicator {
    let all = [("Codex", "CX", codex), ("Claude", "CL", claude)];
    let wanted = |name: &str| match choice {
        "claude" => name == "Claude",
        "both" => true,
        _ => name == "Codex",
    };
    let mut shown: Vec<_> = all
        .iter()
        .filter(|(n, _, d)| wanted(n) && d.is_some())
        .collect();
    let fell_back = shown.is_empty();
    if fell_back {
        shown = all.iter().filter(|(_, _, d)| d.is_some()).collect();
    }
    let hidden_error = all
        .iter()
        .any(|(n, _, d)| matches!(d, Some(Display::Error)) && !shown.iter().any(|s| s.0 == *n));

    let mut title = match shown.as_slice() {
        [] => "--".to_string(),
        [(_, _, Some(d))] => piece(*d),
        many => many
            .iter()
            .map(|(_, short, d)| format!("{short} {}", piece(d.unwrap_or(Display::Pending))))
            .collect::<Vec<_>>()
            .join(" · "),
    };
    if hidden_error {
        title.push_str(" !");
    }
    let mut parts: Vec<String> = all
        .iter()
        .map(|(name, _, d)| match d {
            None => format!("{name} : inactif"),
            Some(Display::Percent(p)) => format!("{name} : {p} % restants"),
            Some(Display::Pending) => format!("{name} : en attente"),
            Some(Display::Error) => format!("{name} : erreur"),
        })
        .collect();
    if fell_back && !shown.is_empty() {
        parts.push("Fournisseur choisi inactif : affichage de l’autre fournisseur.".into());
    }
    let icon_value = shown.iter().find_map(|(_, _, d)| match d {
        Some(Display::Percent(p)) => Some(*p),
        _ => None,
    });
    let stack = if shown.len() == 2 {
        shown
            .iter()
            .map(|(name, _, d)| (*name, piece(d.unwrap_or(Display::Pending))))
            .collect()
    } else {
        Vec::new()
    };
    Indicator {
        stack,
        title,
        tooltip: format!("Quota Monitor\n{}", parts.join("\n")),
        icon_value,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use Display::*;

    #[test]
    fn single_provider_choices() {
        assert_eq!(
            compute("codex", Some(Percent(62)), Some(Percent(40))).title,
            "62%"
        );
        assert_eq!(
            compute("claude", Some(Percent(62)), Some(Percent(40))).title,
            "40%"
        );
    }

    #[test]
    fn both_shows_labelled_values() {
        let i = compute("both", Some(Percent(62)), Some(Percent(0)));
        assert_eq!(i.title, "CX 62% · CL 0%");
        assert_eq!(
            i.stack,
            vec![("Codex", "62%".to_string()), ("Claude", "0%".to_string())]
        );
        assert!(compute("codex", Some(Percent(62)), Some(Percent(0)))
            .stack
            .is_empty());
        assert_eq!(i.icon_value, Some(62));
    }

    #[test]
    fn inactive_choice_falls_back_explicitly() {
        let i = compute("claude", Some(Percent(62)), None);
        assert_eq!(i.title, "62%");
        assert!(i.tooltip.contains("Claude : inactif") && i.tooltip.contains("autre fournisseur"));
        assert_eq!(compute("both", Some(Percent(10)), None).title, "10%");
        assert_eq!(compute("codex", None, None).title, "--");
    }

    #[test]
    fn errors_of_enabled_providers_stay_visible() {
        assert_eq!(compute("claude", Some(Percent(50)), Some(Error)).title, "!");
        assert_eq!(
            compute("codex", Some(Percent(50)), Some(Error)).title,
            "50% !"
        );
        assert_eq!(
            compute("both", Some(Error), Some(Percent(7))).title,
            "CX ! · CL 7%"
        );
        assert_eq!(compute("codex", Some(Pending), Some(Pending)).title, "--");
    }

    #[test]
    fn disabled_provider_error_is_not_shown() {
        assert_eq!(compute("codex", Some(Percent(50)), None).title, "50%");
    }
}
