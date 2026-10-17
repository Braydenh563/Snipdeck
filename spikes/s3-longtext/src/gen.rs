//! Deterministic synthetic data.

pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Self {
        Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1)
    }
    pub fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }
    pub fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
    pub fn range(&mut self, lo: usize, hi: usize) -> usize {
        lo + self.below(hi - lo + 1)
    }
}

#[rustfmt::skip]
const WORDS: &[&str] = &[
    "the", "snippet", "clipboard", "paste", "deck", "panel", "lorem", "ipsum", "dolor", "sit",
    "amet", "consectetur", "adipiscing", "elit", "sed", "do", "eiusmod", "tempor", "incididunt",
    "ut", "labore", "et", "dolore", "magna", "aliqua", "fn", "let", "mut", "return", "impl",
    "struct", "match", "Some", "None", "Ok", "Err", "self", "async", "await", "const", "http",
    "https://example.com/path", "json", "yaml", "select", "from", "where", "order", "by",
    "group", "insert", "update", "delete", "customer_id", "invoice", "total", "amount",
    "address", "street", "city", "postcode", "phone", "email", "regards", "thanks", "meeting",
    "tomorrow", "schedule", "report", "quarterly", "budget", "release", "notes", "version",
    "build", "deploy", "error", "warning", "info", "debug", "trace", "{name}", "{date}",
    "template", "hello", "world", "foo", "bar", "baz", "qux", "alpha", "beta", "gamma", "delta",
    "epsilon", "omega", "kubernetes", "docker", "terraform", "postgres", "redis", "nginx",
    "1234", "5678", "0xdeadbeef", "user@example.org", "TODO", "FIXME", "ok", "yes", "no",
];

pub fn word(rng: &mut Rng) -> &'static str {
    WORDS[rng.below(WORDS.len())]
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Shape {
    /// Many short lines (avg ~55 chars), like prose / code / logs.
    Short,
    /// Long lines of 8-16 KB with spaces (word-wrappable), like minified-ish data.
    Long,
    /// One single line, no newline at all (worst case: minified JSON on one line).
    Single,
}

impl Shape {
    pub fn parse(s: &str) -> Shape {
        match s {
            "short" => Shape::Short,
            "long" => Shape::Long,
            "single" => Shape::Single,
            _ => panic!("unknown shape {s}"),
        }
    }
}

/// Generates ASCII text of exactly `size` bytes.
pub fn text(size: usize, shape: Shape, seed: u64) -> String {
    let mut rng = Rng::new(seed);
    let mut s = String::with_capacity(size + 64);
    let mut line_len = 0usize;
    let mut target = next_target(&mut rng, shape);
    while s.len() < size {
        if line_len > 0 && line_len >= target {
            s.push('\n');
            line_len = 0;
            target = next_target(&mut rng, shape);
            continue;
        }
        if line_len > 0 {
            s.push(' ');
            line_len += 1;
        }
        let w = word(&mut rng);
        s.push_str(w);
        line_len += w.len();
    }
    s.truncate(size);
    s
}

fn next_target(rng: &mut Rng, shape: Shape) -> usize {
    match shape {
        Shape::Short => rng.range(10, 100),
        Shape::Long => rng.range(8 * 1024, 16 * 1024),
        Shape::Single => usize::MAX,
    }
}

pub fn parse_size(s: &str) -> usize {
    let s = s.to_ascii_lowercase();
    if let Some(n) = s.strip_suffix("kb") {
        n.parse::<usize>().unwrap() * 1024
    } else if let Some(n) = s.strip_suffix("mb") {
        n.parse::<usize>().unwrap() * 1024 * 1024
    } else {
        s.parse().unwrap()
    }
}

/// One clipboard item for list/search benchmarks: title + ~200 char body.
pub struct Item {
    pub title: String,
    pub body: String,
    pub list: &'static str,
}

pub fn items(n: usize, seed: u64) -> Vec<Item> {
    let lists = ["Work", "Personal", "Code", "Replies", "Links", "Addresses"];
    let mut rng = Rng::new(seed);
    (0..n)
        .map(|_| {
            let mut title = String::new();
            for i in 0..rng.range(2, 6) {
                if i > 0 {
                    title.push(' ');
                }
                title.push_str(word(&mut rng));
            }
            let target = rng.range(160, 240);
            let mut body = String::new();
            while body.len() < target {
                if !body.is_empty() {
                    body.push(' ');
                }
                body.push_str(word(&mut rng));
            }
            Item {
                title,
                body,
                list: lists[rng.below(lists.len())],
            }
        })
        .collect()
}
