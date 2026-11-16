//! `{calc:Qty*Price|2}`: the small arithmetic of a template, and the number fields it reads.
//!
//! An expression has numbers, names of `{number:Name}` prompts (`Qty`, or `"Unit price"` in
//! quotes), `+ - * /`, unary minus and parentheses. Evaluation cannot fail the render: a
//! division by zero shows [`DIV_ZERO`].

use std::fmt::Write as _;

/// What a division by zero (or an overflow) renders as.
pub const DIV_ZERO: &str = "#DIV/0!";

/// The deepest nesting of parentheses and unary signs.
const MAX_DEPTH: usize = 24;

/// The longest expression, in characters.
pub const MAX_EXPR_CHARS: usize = 200;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Op {
    Add,
    Sub,
    Mul,
    Div,
}

/// A parsed expression. Names start as [`Expr::Name`] and become [`Expr::Var`] (an index
/// into the template's prompts) once the whole template is known.
#[derive(Debug, Clone, PartialEq)]
pub(super) enum Expr {
    Num(f64),
    Name(String),
    Var(usize),
    Neg(Box<Expr>),
    Bin(Box<Expr>, Op, Box<Expr>),
}

impl Expr {
    /// The prompt indexes of the resolved variables, in order of appearance.
    pub(super) fn vars(&self, out: &mut Vec<usize>) {
        match self {
            Self::Var(i) => out.push(*i),
            Self::Neg(inner) => inner.vars(out),
            Self::Bin(a, _, b) => {
                a.vars(out);
                b.vars(out);
            }
            Self::Num(_) | Self::Name(_) => {}
        }
    }

    /// Replaces every name with the index `resolve` gives it; the first name it cannot give
    /// one for is returned.
    pub(super) fn resolve(
        &mut self,
        resolve: &dyn Fn(&str) -> Option<usize>,
    ) -> Result<(), String> {
        match self {
            Self::Name(name) => {
                let index = resolve(name).ok_or_else(|| name.clone())?;
                *self = Self::Var(index);
                Ok(())
            }
            Self::Neg(inner) => inner.resolve(resolve),
            Self::Bin(a, _, b) => {
                a.resolve(resolve)?;
                b.resolve(resolve)
            }
            Self::Num(_) | Self::Var(_) => Ok(()),
        }
    }

    /// The value, with `var` giving the number of each prompt; `None` for a division by zero
    /// or a result that is not finite.
    pub(super) fn eval(&self, var: &dyn Fn(usize) -> f64) -> Option<f64> {
        let value = match self {
            Self::Num(n) => *n,
            Self::Name(_) => 0.0,
            Self::Var(i) => var(*i),
            Self::Neg(inner) => -inner.eval(var)?,
            Self::Bin(a, op, b) => {
                let (a, b) = (a.eval(var)?, b.eval(var)?);
                match op {
                    Op::Add => a + b,
                    Op::Sub => a - b,
                    Op::Mul => a * b,
                    Op::Div if b == 0.0 => return None,
                    Op::Div => a / b,
                }
            }
        };
        value.is_finite().then_some(value)
    }
}

struct Reader<'a> {
    chars: std::iter::Peekable<std::str::Chars<'a>>,
}

/// Parses an expression, or `None` if it is not one.
pub(super) fn parse(src: &str) -> Option<Expr> {
    if src.chars().count() > MAX_EXPR_CHARS {
        return None;
    }
    let mut reader = Reader {
        chars: src.chars().peekable(),
    };
    let expr = reader.sum(0)?;
    reader.skip_space();
    reader.chars.peek().is_none().then_some(expr)
}

impl Reader<'_> {
    fn skip_space(&mut self) {
        while self.chars.peek().is_some_and(|c| c.is_whitespace()) {
            self.chars.next();
        }
    }

    fn sum(&mut self, depth: usize) -> Option<Expr> {
        let mut left = self.product(depth)?;
        loop {
            self.skip_space();
            let op = match self.chars.peek() {
                Some('+') => Op::Add,
                Some('-') => Op::Sub,
                _ => return Some(left),
            };
            self.chars.next();
            let right = self.product(depth)?;
            left = Expr::Bin(Box::new(left), op, Box::new(right));
        }
    }

    fn product(&mut self, depth: usize) -> Option<Expr> {
        let mut left = self.unary(depth)?;
        loop {
            self.skip_space();
            let op = match self.chars.peek() {
                Some('*' | '×') => Op::Mul,
                Some('/' | '÷') => Op::Div,
                _ => return Some(left),
            };
            self.chars.next();
            let right = self.unary(depth)?;
            left = Expr::Bin(Box::new(left), op, Box::new(right));
        }
    }

    fn unary(&mut self, depth: usize) -> Option<Expr> {
        if depth > MAX_DEPTH {
            return None;
        }
        self.skip_space();
        match self.chars.peek().copied()? {
            '-' => {
                self.chars.next();
                Some(Expr::Neg(Box::new(self.unary(depth + 1)?)))
            }
            '+' => {
                self.chars.next();
                self.unary(depth + 1)
            }
            '(' => {
                self.chars.next();
                let inner = self.sum(depth + 1)?;
                self.skip_space();
                (self.chars.next()? == ')').then_some(inner)
            }
            '"' => {
                self.chars.next();
                let mut name = String::new();
                loop {
                    match self.chars.next()? {
                        '"' => break,
                        c => name.push(c),
                    }
                }
                let name = name.trim().to_owned();
                (!name.is_empty()).then_some(Expr::Name(name))
            }
            c if c.is_ascii_digit() || c == '.' => {
                let mut text = String::new();
                while let Some(&c) = self.chars.peek() {
                    if c.is_ascii_digit() || c == '.' {
                        text.push(c);
                        self.chars.next();
                    } else {
                        break;
                    }
                }
                text.parse().ok().map(Expr::Num)
            }
            c if c.is_alphabetic() || c == '_' => {
                let mut name = String::new();
                while let Some(&c) = self.chars.peek() {
                    if c.is_alphanumeric() || c == '_' {
                        name.push(c);
                        self.chars.next();
                    } else {
                        break;
                    }
                }
                Some(Expr::Name(name))
            }
            _ => None,
        }
    }
}

/// The number a person typed: an optional sign, digits and at most one decimal mark (`.` or
/// `,`). `None` for anything else, including an empty text.
#[must_use]
pub fn parse_number(text: &str) -> Option<f64> {
    let text = text.trim();
    let digits = text.strip_prefix(['-', '+']).unwrap_or(text);
    let marks = digits.chars().filter(|c| matches!(c, '.' | ',')).count();
    if text.len() > 32
        || digits.is_empty()
        || marks > 1
        || !digits.chars().any(|c| c.is_ascii_digit())
        || digits
            .chars()
            .any(|c| !c.is_ascii_digit() && !matches!(c, '.' | ','))
    {
        return None;
    }
    text.replace(',', ".").parse().ok()
}

/// `value` with `decimals` places (at most 10), or, with none given, up to 6 places with the
/// trailing zeros removed; `decimal` is the mark to write.
#[must_use]
pub fn format_number(value: f64, decimals: Option<usize>, decimal: char) -> String {
    let places = decimals.map_or(6, |d| d.min(10));
    let mut text = String::new();
    // Writing to a `String` cannot fail.
    let _ = write!(text, "{value:.places$}");
    if decimals.is_none() && text.contains('.') {
        text.truncate(text.trim_end_matches('0').trim_end_matches('.').len());
    }
    if text == "-0" || text.starts_with("-0.") && text.chars().all(|c| matches!(c, '-' | '0' | '.'))
    {
        text.remove(0);
    }
    if decimal != '.' {
        text = text.replace('.', &decimal.to_string());
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(src: &str) -> Option<f64> {
        parse(src)?.eval(&|_| 0.0)
    }

    #[test]
    fn arithmetic_follows_the_usual_precedence() {
        assert_eq!(run("1+2*3"), Some(7.0));
        assert_eq!(run("(1+2)*3"), Some(9.0));
        assert_eq!(run("-2*-3"), Some(6.0));
        assert_eq!(run("10/4"), Some(2.5));
        assert_eq!(run("2 * 3 - 1"), Some(5.0));
    }

    #[test]
    fn a_division_by_zero_or_bad_syntax_is_not_a_number() {
        assert_eq!(run("1/0"), None);
        assert!(parse("1+").is_none());
        assert!(parse("(1").is_none());
        assert!(parse("1 2").is_none());
        assert!(parse("").is_none());
        assert!(parse(&"(".repeat(200)).is_none());
    }

    #[test]
    fn typed_numbers_take_either_decimal_mark() {
        assert_eq!(parse_number(" 3,5 "), Some(3.5));
        assert_eq!(parse_number("-2.25"), Some(-2.25));
        assert_eq!(parse_number("12"), Some(12.0));
        for bad in ["", "abc", "1.2.3", "1,000.5", "--1", "1e5", "."] {
            assert_eq!(parse_number(bad), None, "{bad:?}");
        }
    }

    #[test]
    fn numbers_are_written_with_the_asked_places_and_mark() {
        assert_eq!(format_number(2.5, Some(2), '.'), "2.50");
        assert_eq!(format_number(2.5, None, ','), "2,5");
        assert_eq!(format_number(3.0, None, '.'), "3");
        assert_eq!(format_number(-0.0, Some(0), '.'), "0");
        assert_eq!(format_number(1.0 / 3.0, Some(3), '.'), "0.333");
    }
}
