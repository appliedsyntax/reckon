//! Turns a single line of text into tokens.
//!
//! The lexer is deliberately forgiving: anything it does not understand
//! becomes a `Sym` token and the parser decides whether it is noise.

use std::ops::Range;

#[derive(Debug, Clone, PartialEq)]
pub enum Tok {
    /// A number literal. `radix` records how it was written (2, 8, 10, 16).
    Num {
        value: f64,
        radix: u32,
    },
    /// A word: letters, digits and underscores, starting with a letter.
    Word(String),
    /// A single symbol character (`+`, `(`, `$`, `%`, ...).
    Sym(char),
    Shl,
    Shr,
    /// `14:30` or `2:30:15`.
    Time {
        h: u32,
        m: u32,
        s: u32,
    },
    /// `2026-03-05`.
    Date {
        y: i32,
        m: u32,
        d: u32,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct Token {
    pub tok: Tok,
    pub span: Range<usize>,
}

pub fn tokenize(src: &str) -> Vec<Token> {
    let chars: Vec<(usize, char)> = src.char_indices().collect();
    let end_of = |i: usize| -> usize { chars.get(i).map(|c| c.0).unwrap_or(src.len()) };
    let mut out = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let (start, c) = chars[i];
        if c.is_whitespace() {
            i += 1;
            continue;
        }
        if c.is_ascii_digit()
            || (c == '.' && chars.get(i + 1).is_some_and(|n| n.1.is_ascii_digit()))
        {
            let (tok, next) = lex_number(&chars, i);
            out.push(Token {
                tok,
                span: start..end_of(next),
            });
            i = next;
            continue;
        }
        if c == '°' {
            if let Some(&(_, n)) = chars.get(i + 1) {
                if matches!(n, 'C' | 'F' | 'K')
                    && !chars.get(i + 2).is_some_and(|x| x.1.is_alphanumeric())
                {
                    out.push(Token {
                        tok: Tok::Word(format!("°{n}")),
                        span: start..end_of(i + 2),
                    });
                    i += 2;
                    continue;
                }
            }
            out.push(Token {
                tok: Tok::Sym('°'),
                span: start..end_of(i + 1),
            });
            i += 1;
            continue;
        }
        if c.is_alphabetic() || c == '_' {
            let mut j = i;
            let mut w = String::new();
            while j < chars.len() && (chars[j].1.is_alphanumeric() || chars[j].1 == '_') {
                w.push(chars[j].1);
                j += 1;
            }
            out.push(Token {
                tok: Tok::Word(w),
                span: start..end_of(j),
            });
            i = j;
            continue;
        }
        let next = chars.get(i + 1).map(|c| c.1);
        let (tok, len) = match (c, next) {
            ('<', Some('<')) => (Tok::Shl, 2),
            ('>', Some('>')) => (Tok::Shr, 2),
            ('*', Some('*')) => (Tok::Sym('^'), 2),
            ('×', _) | ('·', _) => (Tok::Sym('*'), 1),
            ('÷', _) => (Tok::Sym('/'), 1),
            ('−', _) | ('–', _) => (Tok::Sym('-'), 1),
            _ => (Tok::Sym(c), 1),
        };
        out.push(Token {
            tok,
            span: start..end_of(i + len),
        });
        i += len;
    }
    out
}

fn digits_at(chars: &[(usize, char)], i: usize, radix: u32) -> usize {
    let mut j = i;
    while j < chars.len() && (chars[j].1.is_digit(radix) || chars[j].1 == '_') {
        j += 1;
    }
    j
}

fn exact_digits(chars: &[(usize, char)], i: usize, n: usize) -> bool {
    (0..n).all(|k| chars.get(i + k).is_some_and(|c| c.1.is_ascii_digit()))
        && !chars.get(i + n).is_some_and(|c| c.1.is_ascii_digit())
}

fn parse_u32(chars: &[(usize, char)], from: usize, to: usize) -> u32 {
    chars[from..to]
        .iter()
        .fold(0u32, |a, c| a * 10 + c.1.to_digit(10).unwrap_or(0))
}

fn lex_number(chars: &[(usize, char)], i: usize) -> (Tok, usize) {
    let c = chars[i].1;
    // Radix prefixes.
    if c == '0' {
        if let Some(&(_, p)) = chars.get(i + 1) {
            let radix = match p {
                'x' | 'X' => 16,
                'b' | 'B' => 2,
                'o' | 'O' => 8,
                _ => 0,
            };
            if radix != 0 && chars.get(i + 2).is_some_and(|d| d.1.is_digit(radix)) {
                let end = digits_at(chars, i + 2, radix);
                let s: String = chars[i + 2..end]
                    .iter()
                    .map(|c| c.1)
                    .filter(|c| *c != '_')
                    .collect();
                let v = u128::from_str_radix(&s, radix)
                    .map(|v| v as f64)
                    .unwrap_or(f64::NAN);
                return (Tok::Num { value: v, radix }, end);
            }
        }
    }
    let int_end = digits_at(chars, i, 10);
    let lead_len = int_end - i;

    // ISO date: YYYY-MM-DD
    if lead_len == 4
        && chars.get(int_end).is_some_and(|c| c.1 == '-')
        && exact_digits(chars, int_end + 1, 2)
        && chars.get(int_end + 3).is_some_and(|c| c.1 == '-')
        && exact_digits(chars, int_end + 4, 2)
    {
        let y = parse_u32(chars, i, int_end) as i32;
        let m = parse_u32(chars, int_end + 1, int_end + 3);
        let d = parse_u32(chars, int_end + 4, int_end + 6);
        return (Tok::Date { y, m, d }, int_end + 6);
    }
    // Time of day: H:MM or H:MM:SS
    if (1..=2).contains(&lead_len)
        && chars.get(int_end).is_some_and(|c| c.1 == ':')
        && exact_digits(chars, int_end + 1, 2)
    {
        let h = parse_u32(chars, i, int_end);
        let m = parse_u32(chars, int_end + 1, int_end + 3);
        let mut end = int_end + 3;
        let mut s = 0;
        if chars.get(end).is_some_and(|c| c.1 == ':') && exact_digits(chars, end + 1, 2) {
            s = parse_u32(chars, end + 1, end + 3);
            end += 3;
        }
        return (Tok::Time { h, m, s }, end);
    }

    // Decimal with optional digit grouping ("5 300", "1,000,000").
    let mut text: String = chars[i..int_end]
        .iter()
        .map(|c| c.1)
        .filter(|c| *c != '_')
        .collect();
    let mut j = int_end;
    if lead_len <= 3 {
        loop {
            let sep = chars.get(j).map(|c| c.1);
            if matches!(sep, Some(' ') | Some(',') | Some('\u{a0}'))
                && exact_digits(chars, j + 1, 3)
            {
                text.extend(chars[j + 1..j + 4].iter().map(|c| c.1));
                j += 4;
            } else {
                break;
            }
        }
    }
    if chars.get(j).is_some_and(|c| c.1 == '.')
        && chars.get(j + 1).is_some_and(|c| c.1.is_ascii_digit())
    {
        let fe = digits_at(chars, j + 1, 10);
        text.push('.');
        text.extend(chars[j + 1..fe].iter().map(|c| c.1).filter(|c| *c != '_'));
        j = fe;
    }
    // Exponent: 1.5e3, 2E-4 (only when digits follow).
    if chars.get(j).is_some_and(|c| c.1 == 'e' || c.1 == 'E') {
        let mut k = j + 1;
        if chars.get(k).is_some_and(|c| c.1 == '+' || c.1 == '-') {
            k += 1;
        }
        if chars.get(k).is_some_and(|c| c.1.is_ascii_digit()) {
            let ee = digits_at(chars, k, 10);
            text.push('e');
            text.extend(chars[j + 1..ee].iter().map(|c| c.1));
            j = ee;
        }
    }
    let value = text.parse::<f64>().unwrap_or(f64::NAN);
    (Tok::Num { value, radix: 10 }, j)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn toks(s: &str) -> Vec<Tok> {
        tokenize(s).into_iter().map(|t| t.tok).collect()
    }

    #[test]
    fn numbers() {
        assert_eq!(
            toks("5 300"),
            vec![Tok::Num {
                value: 5300.0,
                radix: 10
            }]
        );
        assert_eq!(
            toks("1,000,000"),
            vec![Tok::Num {
                value: 1e6,
                radix: 10
            }]
        );
        assert_eq!(
            toks("0x1F"),
            vec![Tok::Num {
                value: 31.0,
                radix: 16
            }]
        );
        assert_eq!(
            toks("1.5e3"),
            vec![Tok::Num {
                value: 1500.0,
                radix: 10
            }]
        );
        assert_eq!(toks("2:30"), vec![Tok::Time { h: 2, m: 30, s: 0 }]);
        assert_eq!(
            toks("2026-03-05"),
            vec![Tok::Date {
                y: 2026,
                m: 3,
                d: 5
            }]
        );
    }

    #[test]
    fn words_and_symbols() {
        assert_eq!(
            toks("$20 in °C"),
            vec![
                Tok::Sym('$'),
                Tok::Num {
                    value: 20.0,
                    radix: 10
                },
                Tok::Word("in".into()),
                Tok::Word("°C".into())
            ]
        );
        assert_eq!(
            toks("1 << 4"),
            vec![
                Tok::Num {
                    value: 1.0,
                    radix: 10
                },
                Tok::Shl,
                Tok::Num {
                    value: 4.0,
                    radix: 10
                }
            ]
        );
    }
}
