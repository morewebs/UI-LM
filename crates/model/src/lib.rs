//! The model: a screen's text in, the page that describes it out.
//!
//! It answers one question per row — is this ordinary text, a tool call, a meter, a reasoning
//! line — and hands that answer to the page builder, which knows how each component is
//! shaped. Everything a person sees is learned from recorded screens; nothing here reads the
//! recording, the events, or the previous frame.
//!
//! # Why this size
//!
//! The weights are a softmax regression over seventeen features chosen from what recorded
//! screens look like — a glyph at the start of a row, a number against a limit, a border, a
//! list of keys. With a corpus of a hundred screens and a few thousand rows, a larger model
//! would fit the corpus and say nothing about a screen it had not seen. This one can be read
//! by a person and checked by hand.
//!
//! # Where the numbers come from
//!
//! `scripts/train_model.py` trains it and writes the weights as JSON; this reads them. The
//! features are spelled identically on both sides on purpose — a drift between the two would
//! make the model silently worse rather than fail.

pub mod features;

use std::path::Path;

use ui_lm_pages::Envelope;

/// A trained model, ready to describe screens.
pub struct Model {
    /// The kinds, in the order the weight columns use.
    kinds: Vec<String>,
    /// One row per feature, one column per kind.
    weights: Vec<Vec<f64>>,
    /// One value per kind.
    bias: Vec<f64>,
    /// How many screens it was trained on, for reporting.
    pub trained_on: usize,
    /// A short digest of the weights, stamped onto every page this model assembles.
    ///
    /// A page built by the reader and a page built by this model were indistinguishable in the record, and rounds
    /// 109–112 compared them repeatedly without being able to say which was which. The digest answers that the way
    /// the build stamp answers it for a binary: the artefact says what produced it.
    digest: String,
}

impl Model {
    /// Reads a model from the JSON `scripts/train_model.py` writes.
    pub fn load(path: &Path) -> anyhow::Result<Self> {
        let text = std::fs::read_to_string(path)
            .map_err(|error| anyhow::anyhow!("cannot read {}: {error}", path.display()))?;
        let json: serde_json::Value = serde_json::from_str(&text)
            .map_err(|error| anyhow::anyhow!("{} is not a model: {error}", path.display()))?;

        let kinds = json["kinds"]
            .as_array()
            .ok_or_else(|| anyhow::anyhow!("the model names no kinds"))?
            .iter()
            .map(|kind| kind.as_str().unwrap_or_default().to_owned())
            .collect::<Vec<_>>();
        let weights = json["weights"]
            .as_array()
            .ok_or_else(|| anyhow::anyhow!("the model has no weights"))?
            .iter()
            .map(|row| {
                row.as_array()
                    .map(|columns| {
                        columns
                            .iter()
                            .map(|value| value.as_f64().unwrap_or(0.0))
                            .collect()
                    })
                    .unwrap_or_default()
            })
            .collect::<Vec<Vec<f64>>>();
        let bias = json["bias"]
            .as_array()
            .ok_or_else(|| anyhow::anyhow!("the model has no bias"))?
            .iter()
            .map(|value| value.as_f64().unwrap_or(0.0))
            .collect::<Vec<_>>();

        if weights.len() != features::COUNT {
            return Err(anyhow::anyhow!(
                "the model has {} feature rows, this build knows {}",
                weights.len(),
                features::COUNT
            ));
        }
        if weights.iter().any(|row| row.len() != kinds.len()) || bias.len() != kinds.len() {
            return Err(anyhow::anyhow!(
                "the model's shape does not agree with its kinds"
            ));
        }
        let trained_on = usize::try_from(json["trainedOn"].as_u64().unwrap_or(0)).unwrap_or(0);
        // The same FNV-1a the build stamp uses, over every weight as written: no dependency, and a change of one
        // weight in the fifth decimal changes the digest.
        let mut hash: u64 = 0xcbf2_9ce4_8422_2325_u64;
        for byte in text.bytes() {
            hash ^= u64::from(byte);
            hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
        }
        Ok(Self {
            kinds,
            weights,
            bias,
            trained_on,
            digest: format!("{hash:016x}"),
        })
    }

    /// The digest stamped onto every page this model assembles.
    pub fn digest(&self) -> &str {
        &self.digest
    }

    /// What each row of `lines` is, as a kind name.
    pub fn labels(&self, lines: &[String]) -> Vec<String> {
        let count = lines.len();
        (0..count)
            .map(|row| {
                let values = features::of(&lines[row], row, count);
                let mut best = 0usize;
                let mut best_logit = f64::NEG_INFINITY;
                for (column, _) in self.kinds.iter().enumerate() {
                    let mut logit = self.bias[column];
                    for (index, value) in values.iter().enumerate() {
                        logit += value * self.weights[index][column];
                    }
                    if logit > best_logit {
                        best_logit = logit;
                        best = column;
                    }
                }
                self.kinds[best].clone()
            })
            .collect()
    }

    /// The page describing `text`, as this model sees it.
    pub fn page(&self, text: &str, surface: &str) -> Envelope {
        // `split('\n')`, not `.lines()`: a screen whose last row is blank joins to text ending in a
        // newline, and `.lines()` drops that trailing blank — one row fewer than the reader, which
        // counts every row of the grid. One row is enough to change which column looks like a
        // divider, and that is exactly what made a frame split in the reader and not here.
        let lines: Vec<String> = text.split('\n').map(str::to_owned).collect();
        let labels = self.labels(&lines);
        let mut page = ui_lm_pages::page_from_labels(&lines, &labels, surface);
        page.create_surface.model = Some(self.digest.clone());
        // ADR 0008's shaping rules apply to every page, whoever built it: the model decides what each row *is*, and a
        // table is decided afterwards from the rows. Doing it here means no caller can forget (round 21).
        ui_lm_pages::promote_tables(&mut page.create_surface.components);
        page
    }
}

#[cfg(test)]
mod stamp_tests {
    use super::Model;
    use std::path::PathBuf;

    #[test]
    fn a_page_from_the_model_says_which_model_built_it() {
        // Rounds 109-112 compared pages from the reader and pages from the label path repeatedly, and the two were
        // indistinguishable in the record. The digest is the answer, so it gets a test: present on a model page,
        // equal to the model's own, and absent from a page the reader built.
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../models/page.json");
        let model = Model::load(&path).expect("the model loads");
        let page = model.page("alpha\nbeta\n", "test");
        assert_eq!(
            page.create_surface.model.as_deref(),
            Some(model.digest()),
            "the page names the model that built it"
        );
        let reader = ui_lm_pages::page_from_text("alpha\nbeta\n", "test");
        assert_eq!(
            reader.create_surface.model, None,
            "a reader page names no model, because no model built it"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::Model;
    use std::path::PathBuf;

    fn model_path() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../models/page.json")
    }

    #[test]
    fn the_checked_in_model_loads_and_fits_together() {
        let model = Model::load(&model_path()).expect("the model must load");
        assert!(
            model.trained_on > 0,
            "the model must record what it learned from"
        );
    }

    #[test]
    fn a_meter_row_is_recognised_by_the_model() {
        // The one component the do-nothing converter never finds. A model that cannot find
        // it has not learned anything worth having.
        let model = Model::load(&model_path()).expect("the model must load");
        let page = model.page("1.2K / 500K | [Dashboard]", "t");
        assert!(
            page.create_surface
                .components
                .iter()
                .any(|component| component.component == "Meter"),
            "the meter must be found: {page:?}"
        );
    }
}
