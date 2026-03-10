//! BM25 full-text search implementation.
//!
//! Compatible with the JS `search-index.json` format.

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

// ── Constants ────────────────────────────────────────────────────────

const DEFAULT_K1: f64 = 1.5;
const DEFAULT_B: f64 = 0.75;

const FIELD_WEIGHT_NAME: f64 = 3.0;
const FIELD_WEIGHT_TAGS: f64 = 2.0;
const FIELD_WEIGHT_DESC: f64 = 1.0;

static STOP_WORDS: &[&str] = &[
    "a", "an", "and", "are", "as", "at", "be", "but", "by", "for", "from", "has", "he", "in", "is",
    "it", "its", "of", "on", "or", "that", "the", "to", "was", "were", "will", "with", "this",
    "not", "no", "can", "do",
];

// ── Data structures (serde-compatible with JS index) ────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchIndex {
    pub version: String,
    pub algorithm: String,
    pub params: BM25Params,
    #[serde(rename = "totalDocs")]
    pub total_docs: usize,
    #[serde(rename = "avgFieldLengths")]
    pub avg_field_lengths: FieldLengths,
    pub idf: HashMap<String, f64>,
    pub documents: Vec<IndexDocument>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BM25Params {
    pub k1: f64,
    pub b: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FieldLengths {
    pub name: f64,
    pub description: f64,
    pub tags: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexDocument {
    pub id: String,
    pub tokens: FieldTokens,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FieldTokens {
    pub name: Vec<String>,
    pub description: Vec<String>,
    pub tags: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct SearchResult {
    pub id: String,
    pub score: f64,
}

// ── Tokenizer ───────────────────────────────────────────────────────

/// Tokenize text: lowercase, remove punctuation, filter stop words and 1-char tokens.
pub fn tokenize(text: &str) -> Vec<String> {
    let lower = text.to_lowercase();
    // Remove non-alphanumeric except spaces and hyphens
    let cleaned: String = lower
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == ' ' || c == '-' {
                c
            } else {
                ' '
            }
        })
        .collect();

    let stop: HashSet<&str> = STOP_WORDS.iter().copied().collect();

    cleaned
        .split([' ', '-'])
        .filter(|s| !s.is_empty() && s.len() > 1 && !stop.contains(s))
        .map(|s| s.to_string())
        .collect()
}

// ── Index building ──────────────────────────────────────────────────

/// Entry data needed for building an index.
pub struct IndexEntry {
    pub id: String,
    pub name: String,
    pub description: String,
    pub tags: Vec<String>,
}

/// Build a BM25 search index from a list of entries.
pub fn build_index(entries: &[IndexEntry]) -> SearchIndex {
    let total_docs = entries.len();
    let mut documents = Vec::with_capacity(total_docs);
    let mut df: HashMap<String, usize> = HashMap::new();
    let mut total_name_len = 0usize;
    let mut total_desc_len = 0usize;
    let mut total_tags_len = 0usize;

    for entry in entries {
        let name_tokens = tokenize(&entry.name);
        let desc_tokens = tokenize(&entry.description);
        let tags_tokens = tokenize(&entry.tags.join(" "));

        total_name_len += name_tokens.len();
        total_desc_len += desc_tokens.len();
        total_tags_len += tags_tokens.len();

        // Document frequency: count each term once per document (union across fields)
        let mut seen = HashSet::new();
        for t in name_tokens
            .iter()
            .chain(desc_tokens.iter())
            .chain(tags_tokens.iter())
        {
            seen.insert(t.clone());
        }
        for t in &seen {
            *df.entry(t.clone()).or_insert(0) += 1;
        }

        documents.push(IndexDocument {
            id: entry.id.clone(),
            tokens: FieldTokens {
                name: name_tokens,
                description: desc_tokens,
                tags: tags_tokens,
            },
        });
    }

    let n = total_docs as f64;
    let idf: HashMap<String, f64> = df
        .into_iter()
        .map(|(term, count)| {
            let df_val = count as f64;
            let score = ((n - df_val + 0.5) / (df_val + 0.5) + 1.0).ln();
            (term, score)
        })
        .collect();

    let avg_field_lengths = FieldLengths {
        name: if total_docs > 0 {
            total_name_len as f64 / n
        } else {
            0.0
        },
        description: if total_docs > 0 {
            total_desc_len as f64 / n
        } else {
            0.0
        },
        tags: if total_docs > 0 {
            total_tags_len as f64 / n
        } else {
            0.0
        },
    };

    SearchIndex {
        version: "1.0.0".to_string(),
        algorithm: "bm25".to_string(),
        params: BM25Params {
            k1: DEFAULT_K1,
            b: DEFAULT_B,
        },
        total_docs,
        avg_field_lengths,
        idf,
        documents,
    }
}

// ── Scoring ─────────────────────────────────────────────────────────

fn score_field(
    query_terms: &[String],
    field_tokens: &[String],
    idf: &HashMap<String, f64>,
    avg_field_len: f64,
    k1: f64,
    b: f64,
) -> f64 {
    let dl = field_tokens.len() as f64;
    let avg = if avg_field_len > 0.0 {
        avg_field_len
    } else {
        1.0
    };
    let mut score = 0.0;

    for term in query_terms {
        let term_idf = idf.get(term).copied().unwrap_or(0.0);
        let tf = field_tokens.iter().filter(|t| *t == term).count() as f64;
        if tf > 0.0 {
            score += term_idf * (tf * (k1 + 1.0)) / (tf + k1 * (1.0 - b + b * (dl / avg)));
        }
    }
    score
}

/// Search the index with a query string. Returns results sorted by score descending.
pub fn search(query: &str, index: &SearchIndex) -> Vec<SearchResult> {
    let query_terms = tokenize(query);
    if query_terms.is_empty() {
        return vec![];
    }

    let k1 = index.params.k1;
    let b = index.params.b;

    let mut results: Vec<SearchResult> = index
        .documents
        .iter()
        .filter_map(|doc| {
            let name_score = score_field(
                &query_terms,
                &doc.tokens.name,
                &index.idf,
                index.avg_field_lengths.name,
                k1,
                b,
            ) * FIELD_WEIGHT_NAME;

            let tags_score = score_field(
                &query_terms,
                &doc.tokens.tags,
                &index.idf,
                index.avg_field_lengths.tags,
                k1,
                b,
            ) * FIELD_WEIGHT_TAGS;

            let desc_score = score_field(
                &query_terms,
                &doc.tokens.description,
                &index.idf,
                index.avg_field_lengths.description,
                k1,
                b,
            ) * FIELD_WEIGHT_DESC;

            let total = name_score + tags_score + desc_score;
            if total > 0.0 {
                Some(SearchResult {
                    id: doc.id.clone(),
                    score: total,
                })
            } else {
                None
            }
        })
        .collect();

    results.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    results
}
