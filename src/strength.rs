//! Password strength estimation (zxcvbn), used for the master password gate and
//! the strength meter in the entry editor.

/// zxcvbn score required for a master password (0 = terrible .. 4 = great).
pub const MIN_MASTER_SCORE: u8 = 3;
pub const MIN_MASTER_LENGTH: usize = 10;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Strength {
    pub score: u8,
    /// Human-readable estimate against an offline attacker doing 10k guesses/s.
    pub crack_time: String,
    pub warning: Option<String>,
    pub suggestions: Vec<String>,
}

pub fn assess(password: &str, context: &[&str]) -> Strength {
    if password.is_empty() {
        return Strength { score: 0, crack_time: "instantly".into(), warning: None, suggestions: vec![] };
    }
    let est = zxcvbn::zxcvbn(password, context);
    let (warning, suggestions) = match est.feedback() {
        Some(f) => (f.warning().map(|w| w.to_string()), f.suggestions().iter().map(|s| s.to_string()).collect()),
        None => (None, vec![]),
    };
    Strength {
        score: u8::from(est.score()),
        crack_time: est.crack_times().offline_slow_hashing_1e4_per_second().to_string(),
        warning,
        suggestions,
    }
}

/// Why a proposed master password is rejected, or `None` if it is acceptable.
pub fn master_password_problem(password: &str) -> Option<String> {
    if password.chars().count() < MIN_MASTER_LENGTH {
        return Some(format!("Use at least {MIN_MASTER_LENGTH} characters."));
    }
    let s = assess(password, &["seepass", "password", "manager"]);
    if s.score < MIN_MASTER_SCORE {
        let hint = s.warning.or_else(|| s.suggestions.first().cloned()).unwrap_or_default();
        return Some(format!("Too easy to guess ({}). {hint}", label(s.score)).trim_end().to_string());
    }
    None
}

pub fn label(score: u8) -> &'static str {
    match score {
        0 => "very weak",
        1 => "weak",
        2 => "fair",
        3 => "strong",
        _ => "very strong",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn weak_master_passwords_rejected() {
        // The old 7.x rules ("12+ chars, upper, lower, digit") accepted this one.
        assert!(master_password_problem("Password1234").is_some());
        assert!(master_password_problem("short").is_some());
        assert!(master_password_problem("").is_some());
    }

    #[test]
    fn strong_master_password_accepted() {
        assert!(master_password_problem("velvet-otter-cactus-lighthouse").is_none());
    }

    #[test]
    fn scores_increase() {
        assert!(assess("aaaa", &[]).score < assess("k8#Qz!v2Lp@9wR", &[]).score);
    }
}
