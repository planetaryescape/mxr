//! Per-mode index recipes (blueprint 22, "Each mode indexes the part of the
//! email it cares about"). A recipe picks which parts of a message become
//! chunks and what context prefix each carries. Recipes are a typed,
//! versioned table in code, not user config (D117); a version bump marks
//! messages indexed under the old recipe as stale.
//!
//! Only Messages ships here (phase 3). The indexer keeps today's chunking
//! until the retrieval eval on real mail shows a recipe beats it, as the
//! blueprint requires before switching.

use mxr_reader::{new_text, EarlierMessage};

/// The modes that have a recipe.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RecipeMode {
    Messages,
}

/// One recipe and its version.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IndexRecipe {
    pub mode: RecipeMode,
    pub version: u32,
    /// Words per window and overlap, for text longer than one window.
    pub window_words: usize,
    pub overlap_words: usize,
}

/// Messages: each message's new text, prefixed with the person and topic,
/// plus the gist. Quoted history is not re-indexed: it already belongs to
/// the earlier message it came from.
pub const MESSAGES_RECIPE: IndexRecipe = IndexRecipe {
    mode: RecipeMode::Messages,
    version: 1,
    window_words: 120,
    overlap_words: 30,
};

/// Every recipe, one per mode.
pub const RECIPES: &[IndexRecipe] = &[MESSAGES_RECIPE];

pub fn recipe(mode: RecipeMode) -> &'static IndexRecipe {
    RECIPES
        .iter()
        .find(|recipe| recipe.mode == mode)
        .unwrap_or(&MESSAGES_RECIPE)
}

/// What the Messages recipe needs about one message.
pub struct MessagesRecipeInput<'a> {
    /// "Samir Patel", or a group's first names.
    pub person: &'a str,
    /// The topic: the subject without "Re:".
    pub topic: &'a str,
    pub text_plain: Option<&'a str>,
    pub text_html: Option<&'a str>,
    /// Earlier messages of the thread, as plain text.
    pub earlier: &'a [EarlierMessage<'a>],
    /// The conversation's gist, when one is cached.
    pub gist: Option<&'a str>,
}

/// The chunk texts the Messages recipe makes for one message: windows of
/// its new text, each prefixed "Samir Patel · Contract renewal: ", and the
/// gist with the same prefix. Contextual retrieval: the prefix carries who
/// and what, which a 120-word window loses.
pub fn messages_chunks(input: &MessagesRecipeInput<'_>) -> Vec<String> {
    let recipe = recipe(RecipeMode::Messages);
    let prefix = format!("{} · {}: ", input.person.trim(), input.topic.trim());
    let new = new_text(input.text_plain, input.text_html, input.earlier);
    let mut chunks: Vec<String> = windows(&new.text, recipe.window_words, recipe.overlap_words)
        .into_iter()
        .map(|window| format!("{prefix}{window}"))
        .collect();
    if let Some(gist) = input.gist.map(str::trim).filter(|g| !g.is_empty()) {
        chunks.push(format!("{prefix}{gist}"));
    }
    chunks
}

fn windows(text: &str, size: usize, overlap: usize) -> Vec<String> {
    let words: Vec<&str> = text.split_whitespace().collect();
    if words.is_empty() {
        return Vec::new();
    }
    let step = size.saturating_sub(overlap).max(1);
    let mut out = Vec::new();
    let mut start = 0;
    loop {
        let end = (start + size).min(words.len());
        out.push(words[start..end].join(" "));
        if end == words.len() {
            break;
        }
        start += step;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn messages_chunks_are_new_text_with_person_and_topic_and_the_gist() {
        let earlier = "Should we keep the canary at 5% until the dashboard is quiet?";
        let chunks = messages_chunks(&MessagesRecipeInput {
            person: "Samir Patel",
            topic: "Rollout",
            text_plain: Some(
                "Yes, keep it at 5%.\n\nOn Mon, Alex wrote:\n> Should we keep the canary at 5% until the dashboard is quiet?",
            ),
            text_html: None,
            earlier: &[EarlierMessage {
                text: earlier,
                same_author: false,
            }],
            gist: Some("Samir agreed to keep the canary at 5%."),
        });
        assert_eq!(
            chunks,
            vec![
                "Samir Patel · Rollout: Yes, keep it at 5%.".to_string(),
                "Samir Patel · Rollout: Samir agreed to keep the canary at 5%.".to_string(),
            ]
        );
    }

    #[test]
    fn long_text_is_windowed_with_overlap() {
        let text = (0..250).map(|i| format!("w{i}")).collect::<Vec<_>>().join(" ");
        let got = windows(&text, 120, 30);
        assert_eq!(got.len(), 3);
        assert!(got[1].starts_with("w90 "));
        assert!(got[2].ends_with("w249"));
    }

    #[test]
    fn every_mode_has_one_recipe() {
        assert_eq!(RECIPES.len(), 1);
        assert_eq!(recipe(RecipeMode::Messages).version, 1);
    }
}
