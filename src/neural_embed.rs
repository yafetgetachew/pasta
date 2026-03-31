use std::sync::Mutex;

use fastembed::{EmbeddingModel, InitOptions, TextEmbedding};

const NEURAL_VECTOR_DIM: usize = 384;

pub(crate) struct NeuralEmbedder {
    model: Mutex<TextEmbedding>,
}

impl NeuralEmbedder {
    pub(crate) fn try_new() -> anyhow::Result<Self> {
        let model = TextEmbedding::try_new(
            InitOptions::new(EmbeddingModel::AllMiniLML6V2).with_show_download_progress(true),
        )?;
        Ok(Self {
            model: Mutex::new(model),
        })
    }

    pub(crate) fn embed(&self, content: &str, seed_terms: &[String]) -> Vec<f32> {
        let text = if seed_terms.is_empty() {
            content.to_owned()
        } else {
            format!("{content} {}", seed_terms.join(" "))
        };

        let text = text.trim().to_owned();
        if text.is_empty() {
            return vec![0.0; NEURAL_VECTOR_DIM];
        }

        let model = self
            .model
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());

        match model.embed(vec![text], None) {
            Ok(embeddings) if !embeddings.is_empty() => embeddings.into_iter().next().unwrap(),
            Ok(_) => vec![0.0; NEURAL_VECTOR_DIM],
            Err(err) => {
                eprintln!("warning: neural embedding failed: {err}");
                vec![0.0; NEURAL_VECTOR_DIM]
            }
        }
    }

    pub(crate) fn zero_vector() -> Vec<f32> {
        vec![0.0; NEURAL_VECTOR_DIM]
    }
}
