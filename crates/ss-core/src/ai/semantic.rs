//! Semantic search arm: the query text's embedding (`embed.text`, cached per text) against every
//! `embedding` row of the same model, held in memory (6,260 x 768 floats is ~19 MB, Deferred decision
//! #17) and reloaded after the AI job writes embeddings. Brute-force cosine: the rows are unit
//! vectors, so cosine = dot product.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use std::time::Duration;

use crate::catalog::Catalog;
use crate::contract::AppError;
use crate::contract::sidecar::{EmbedTextParams, EmbedTextResult, methods};
use crate::filter::SemanticScores;
use crate::sidecar::config::SEMANTIC_TEXT_CACHE;
use crate::sidecar::vector::{self, decode_vector};
use crate::sidecar::{CallError, Sidecar};

/// Every embedding of one model, row-major.
struct Embeddings {
    model: String,
    dim: usize,
    ids: Vec<i64>,
    vecs: Vec<f32>,
}

/// Recent query-text embeddings: (model, text, unit vector), most recent last.
type TextCache = VecDeque<(String, String, Arc<Vec<f32>>)>;

#[derive(Default)]
pub struct SemanticIndex {
    cache: RwLock<Option<Arc<Embeddings>>>,
    /// Bumped by [`SemanticIndex::invalidate`]; a load that raced an invalidation is not cached.
    generation: AtomicU64,
    texts: Mutex<TextCache>,
}

impl SemanticIndex {
    /// Drops the in-memory embeddings (after the AI job wrote new ones).
    pub fn invalidate(&self) {
        self.generation.fetch_add(1, Ordering::SeqCst);
        *self.cache.write().expect("semantic cache") = None;
    }

    /// The query text's unit vector (cached by model + text). `timeout` bounds the sidecar call.
    pub fn embed_text(&self, sidecar: &Sidecar, model: &str, text: &str, timeout: Duration) -> Result<Arc<Vec<f32>>, CallError> {
        if let Some(v) = self.texts.lock().expect("text cache").iter().find(|(m, t, _)| m == model && t == text) {
            return Ok(v.2.clone());
        }
        let params = EmbedTextParams { texts: vec![text.to_owned()] };
        let r: EmbedTextResult = sidecar.call(methods::EMBED_TEXT, &params, timeout).typed()?;
        let first = r.vectors.first().ok_or_else(|| CallError::BadResult("embed.text returned no vector".into()))?;
        let mut v = decode_vector(first, usize::try_from(r.dim).unwrap_or(0)).map_err(CallError::BadResult)?;
        vector::normalize(&mut v);
        let v = Arc::new(v);
        let mut cache = self.texts.lock().expect("text cache");
        if cache.len() == SEMANTIC_TEXT_CACHE {
            cache.pop_front();
        }
        cache.push_back((r.model, text.to_owned(), v.clone()));
        Ok(v)
    }

    /// The `top_k` images most similar to `query` (unit vector of `model`), best first, keeping only
    /// scores >= `min_score` when given.
    pub fn top(&self, cat: &Catalog, model: &str, query: &[f32], top_k: usize, min_score: Option<f32>) -> Result<SemanticScores, AppError> {
        let e = self.embeddings(cat, model)?;
        if e.dim != query.len() || e.ids.is_empty() {
            return Ok(SemanticScores::default());
        }
        let mut scored: Vec<(i64, f32)> = e
            .ids
            .iter()
            .zip(e.vecs.chunks_exact(e.dim))
            .map(|(id, v)| (*id, vector::dot(v, query)))
            .filter(|(_, s)| min_score.is_none_or(|m| *s >= m))
            .collect();
        scored.sort_by(|a, b| b.1.total_cmp(&a.1).then(a.0.cmp(&b.0)));
        scored.truncate(top_k);
        Ok(SemanticScores(scored))
    }

    fn embeddings(&self, cat: &Catalog, model: &str) -> Result<Arc<Embeddings>, AppError> {
        if let Some(e) = self.cache.read().expect("semantic cache").as_ref().filter(|e| e.model == model) {
            return Ok(e.clone());
        }
        let generation = self.generation.load(Ordering::SeqCst);
        let model_owned = model.to_owned();
        let rows: Vec<(i64, i64, Vec<u8>)> = cat.read(move |conn| {
            let mut stmt = conn.prepare("SELECT image_id, dim, vec FROM embedding WHERE model = ?1 ORDER BY image_id")?;
            let rows = stmt.query_map([&model_owned], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?;
            Ok(rows.collect::<Result<_, _>>()?)
        })?;
        // One model has one dimension: the first row sets it; a row of another length is skipped.
        let dim = rows.first().map(|r| r.1).unwrap_or(0);
        let dim = usize::try_from(dim).unwrap_or(0);
        let mut ids = Vec::with_capacity(rows.len());
        let mut vecs = Vec::with_capacity(rows.len() * dim);
        for (id, _, blob) in rows {
            if let Some(mut v) = vector::floats_from_le(&blob, dim) {
                vector::normalize(&mut v);
                ids.push(id);
                vecs.extend(v);
            }
        }
        let e = Arc::new(Embeddings { model: model.to_owned(), dim, ids, vecs });
        if self.generation.load(Ordering::SeqCst) == generation {
            *self.cache.write().expect("semantic cache") = Some(e.clone());
        }
        Ok(e)
    }
}
