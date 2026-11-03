//! Cryptographically secure password generation.

use rand::rngs::OsRng;
use rand::seq::SliceRandom;
use rand::Rng;
use zeroize::Zeroizing;

const LOWER: &str = "abcdefghijklmnopqrstuvwxyz";
const UPPER: &str = "ABCDEFGHIJKLMNOPQRSTUVWXYZ";
const DIGITS: &str = "0123456789";
const SYMBOLS: &str = "!@#$%^&*()-_=+[]{};:,.<>/?~";
const AMBIGUOUS: &str = "Il1O0o|`'\"";

pub const MIN_LENGTH: usize = 4;
pub const MAX_LENGTH: usize = 128;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GeneratorOptions {
    pub length: usize,
    pub lowercase: bool,
    pub uppercase: bool,
    pub digits: bool,
    pub symbols: bool,
    pub avoid_ambiguous: bool,
}

impl Default for GeneratorOptions {
    fn default() -> Self {
        Self { length: 20, lowercase: true, uppercase: true, digits: true, symbols: true, avoid_ambiguous: true }
    }
}

impl GeneratorOptions {
    fn classes(&self) -> Vec<Vec<char>> {
        let mut classes = Vec::new();
        for (enabled, set) in
            [(self.lowercase, LOWER), (self.uppercase, UPPER), (self.digits, DIGITS), (self.symbols, SYMBOLS)]
        {
            if enabled {
                let chars: Vec<char> =
                    set.chars().filter(|c| !(self.avoid_ambiguous && AMBIGUOUS.contains(*c))).collect();
                classes.push(chars);
            }
        }
        classes
    }

    /// Entropy of a password produced with these options, in bits.
    pub fn entropy_bits(&self) -> f64 {
        let pool: usize = self.classes().iter().map(Vec::len).sum();
        if pool == 0 {
            return 0.0;
        }
        self.length.clamp(MIN_LENGTH, MAX_LENGTH) as f64 * (pool as f64).log2()
    }
}

/// Generate a password containing at least one character from every enabled
/// class. Returns `None` if no class is enabled.
pub fn generate(opts: &GeneratorOptions) -> Option<Zeroizing<String>> {
    let classes = opts.classes();
    if classes.is_empty() {
        return None;
    }
    let length = opts.length.clamp(MIN_LENGTH.max(classes.len()), MAX_LENGTH);
    let pool: Vec<char> = classes.iter().flatten().copied().collect();
    let mut rng = OsRng;

    let mut chars: Zeroizing<Vec<char>> = Zeroizing::new(Vec::with_capacity(length));
    for class in &classes {
        chars.push(class[rng.gen_range(0..class.len())]);
    }
    while chars.len() < length {
        chars.push(pool[rng.gen_range(0..pool.len())]);
    }
    chars.shuffle(&mut rng);
    Some(Zeroizing::new(chars.iter().collect()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn respects_length_and_classes() {
        let opts = GeneratorOptions::default();
        for _ in 0..200 {
            let pw = generate(&opts).unwrap();
            assert_eq!(pw.chars().count(), 20);
            assert!(pw.chars().any(|c| c.is_ascii_lowercase()));
            assert!(pw.chars().any(|c| c.is_ascii_uppercase()));
            assert!(pw.chars().any(|c| c.is_ascii_digit()));
            assert!(pw.chars().any(|c| SYMBOLS.contains(c)));
            assert!(!pw.chars().any(|c| AMBIGUOUS.contains(c)));
        }
    }

    #[test]
    fn digits_only() {
        let opts =
            GeneratorOptions { length: 8, lowercase: false, uppercase: false, symbols: false, ..Default::default() };
        let pw = generate(&opts).unwrap();
        assert!(pw.chars().all(|c| c.is_ascii_digit()));
    }

    #[test]
    fn nothing_enabled() {
        let opts = GeneratorOptions {
            lowercase: false,
            uppercase: false,
            digits: false,
            symbols: false,
            ..Default::default()
        };
        assert!(generate(&opts).is_none());
        assert_eq!(opts.entropy_bits(), 0.0);
    }

    #[test]
    fn not_repeating() {
        let opts = GeneratorOptions::default();
        assert_ne!(*generate(&opts).unwrap(), *generate(&opts).unwrap());
    }

    #[test]
    fn entropy() {
        let opts = GeneratorOptions { length: 10, avoid_ambiguous: false, ..Default::default() };
        let pool = (26 + 26 + 10 + SYMBOLS.len()) as f64;
        assert!((opts.entropy_bits() - 10.0 * pool.log2()).abs() < 1e-9);
    }
}
