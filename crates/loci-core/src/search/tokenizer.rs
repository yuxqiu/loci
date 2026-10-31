//! The `loci` tokenizer (m0 §9.3): NFC, script runs, CJK word segmentation, lowercase
//! alphanumeric words elsewhere.

use icu_segmenter::WordSegmenter;
use icu_segmenter::options::WordBreakInvariantOptions;
use tantivy::tokenizer::{Token, TokenStream, Tokenizer};
use unicode_normalization::UnicodeNormalization;

/// The `loci` tokenizer, registered under that name in the search index.
#[derive(Clone, Default)]
pub struct LociTokenizer;

impl LociTokenizer {
    pub fn new() -> Self {
        Self
    }

    /// Tokenizes `text`. Offsets refer to the NFC form of `text`.
    fn tokens(&self, text: &str) -> Vec<Token> {
        let text: String = text.nfc().collect();
        let mut out = Vec::new();
        let mut push = |from: usize, to: usize, text: String| {
            let position = out.len();
            out.push(Token { offset_from: from, offset_to: to, position, text, position_length: 1 });
        };
        let mut i = 0;
        while let Some(c) = text[i..].chars().next() {
            // A run of one script class: Hangul, Han and kana, or other letters and digits.
            let class: Option<fn(char) -> bool> = if is_hangul(c) {
                Some(is_hangul)
            } else if is_han(c) || is_kana(c) {
                Some(|ch| is_han(ch) || is_kana(ch))
            } else if c.is_alphanumeric() {
                Some(|ch: char| ch.is_alphanumeric() && !is_cjk(ch))
            } else {
                None
            };
            let Some(in_run) = class else {
                i += c.len_utf8();
                continue;
            };
            let len: usize = text[i..].chars().take_while(|&ch| in_run(ch)).map(char::len_utf8).sum();
            let run = &text[i..i + len];
            if is_hangul(c) {
                for (s, e) in hangul_bigrams(run) {
                    push(i + s, i + e, run[s..e].to_string());
                }
            } else if is_cjk(c) {
                for (s, e) in chinese_japanese_words(run) {
                    push(i + s, i + e, run[s..e].to_string());
                }
            } else {
                push(i, i + len, run.to_lowercase());
            }
            i += len;
        }
        out
    }
}

/// Tokenizes `text` with the `loci` tokenizer and returns the token texts.
pub fn tokenize(text: &str) -> Vec<String> {
    LociTokenizer::new().tokens(text).into_iter().map(|t| t.text).collect()
}

/// Owned token stream: CJK segmenters return tokens borrowing their input, so the
/// tokenizer collects owned tokens first.
pub struct LociTokenStream {
    tokens: Vec<Token>,
    next: usize,
}

impl TokenStream for LociTokenStream {
    fn advance(&mut self) -> bool {
        self.next += 1;
        self.next <= self.tokens.len()
    }
    fn token(&self) -> &Token {
        &self.tokens[self.next - 1]
    }
    fn token_mut(&mut self) -> &mut Token {
        &mut self.tokens[self.next - 1]
    }
}

impl Tokenizer for LociTokenizer {
    type TokenStream<'a> = LociTokenStream;
    fn token_stream<'a>(&'a mut self, text: &'a str) -> LociTokenStream {
        LociTokenStream { tokens: self.tokens(text), next: 0 }
    }
}

fn is_hangul(c: char) -> bool {
    matches!(c,
        '\u{1100}'..='\u{11FF}' | '\u{3130}'..='\u{318F}' | '\u{A960}'..='\u{A97F}'
        | '\u{AC00}'..='\u{D7A3}' | '\u{D7B0}'..='\u{D7FF}' | '\u{FFA0}'..='\u{FFDC}')
}

/// Hiragana and Katakana, including the prolonged sound mark U+30FC.
fn is_kana(c: char) -> bool {
    matches!(c,
        '\u{3040}'..='\u{309F}' | '\u{30A0}'..='\u{30FF}' | '\u{31F0}'..='\u{31FF}'
        | '\u{FF66}'..='\u{FF9F}' | '\u{1B000}'..='\u{1B16F}')
}

fn is_han(c: char) -> bool {
    matches!(c,
        '\u{3005}' | '\u{3007}' | '\u{3021}'..='\u{3029}' | '\u{3400}'..='\u{4DBF}'
        | '\u{4E00}'..='\u{9FFF}' | '\u{F900}'..='\u{FAFF}' | '\u{20000}'..='\u{323AF}')
}

fn is_cjk(c: char) -> bool {
    is_han(c) || is_kana(c) || is_hangul(c)
}

/// Words of a run of Han and kana, as byte ranges in `run`: ICU4X's dictionary
/// segmenter, whose one word list covers Chinese and Japanese, so a query and a
/// document are split alike whatever their language.
fn chinese_japanese_words(run: &str) -> Vec<(usize, usize)> {
    // Compiled data: constructing the segmenter only borrows static tables.
    let breaks: Vec<usize> = WordSegmenter::new_auto(WordBreakInvariantOptions::default()).segment_str(run).collect();
    breaks.windows(2).map(|w| (w[0], w[1])).collect()
}

/// Overlapping two-syllable tokens of a Hangul run, as byte ranges in `run`; a
/// one-syllable run is one token. Korean attaches particles and endings to words, so
/// bigrams let `공부` match `공부했습니다` without a dictionary.
fn hangul_bigrams(run: &str) -> Vec<(usize, usize)> {
    let mut bounds: Vec<usize> = run.char_indices().map(|(b, _)| b).collect();
    bounds.push(run.len());
    if bounds.len() <= 3 {
        return vec![(0, run.len())];
    }
    bounds.windows(3).map(|w| (w[0], w[2])).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn latin_lowercased_and_split() {
        assert_eq!(tokenize("Graph-Search_v2.md, BM25!"), ["graph", "search", "v2", "md", "bm25"]);
    }

    #[test]
    fn nfc_applied() {
        // "e" + combining acute → "é".
        assert_eq!(tokenize("Cafe\u{301}"), ["café"]);
    }

    #[test]
    fn positions_increase_per_token() {
        let mut t = LociTokenizer::new();
        let mut stream = t.token_stream("alpha 学习 공부했다 beta");
        let mut positions = Vec::new();
        while stream.advance() {
            positions.push(stream.token().position);
        }
        assert_eq!(positions, (0..positions.len()).collect::<Vec<_>>());
    }

    #[test]
    fn mixed_runs_split_by_script() {
        let t = tokenize("Rust和tantivy");
        assert_eq!(t.first().map(String::as_str), Some("rust"));
        assert_eq!(t.last().map(String::as_str), Some("tantivy"));
        assert!(t.contains(&"和".to_string()));
        // Hangul next to Han is its own run.
        assert!(tokenize("韓國語한국어").ends_with(&["한국".to_string(), "국어".to_string()]));
    }

    #[test]
    fn chinese_and_japanese_split_alike_in_queries_and_text() {
        let t = tokenize("東京大学でアルゴリズムを勉強しています");
        assert!(t.contains(&"勉強".to_string()) && t.contains(&"アルゴリズム".to_string()), "{t:?}");
        // A kanji-only query is split the same way as inside Japanese text.
        assert_eq!(tokenize("勉強"), ["勉強"]);
        let t = tokenize("机器学习中的梯度下降算法");
        assert!(t.contains(&"梯度".to_string()) && t.contains(&"下降".to_string()), "{t:?}");
        assert_eq!(tokenize("梯度下降"), ["梯度", "下降"]);
    }

    #[test]
    fn hangul_bigrams() {
        assert_eq!(tokenize("공부했습니다"), ["공부", "부했", "했습", "습니", "니다"]);
        assert_eq!(tokenize("공부"), ["공부"]);
        assert_eq!(tokenize("책 을"), ["책", "을"]);
    }
}
