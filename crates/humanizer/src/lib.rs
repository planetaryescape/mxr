use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HumanizerCategory {
    AiVocabulary,
    EmDashOveruse,
    CurlyQuotes,
    NegativeParallelism,
    IngTailClause,
    CopulaAvoidance,
    FillerPhrase,
    HedgingStack,
    SycophanticOpener,
    KnowledgeCutoffDisclaimer,
    CollaborativeArtifact,
    PromotionalLanguage,
    EmojiHeading,
    TitleCaseHeading,
    InlineHeaderColonList,
    RuleOfThree,
    ExcessiveBoldface,
    GenericPositiveConclusion,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TextRange {
    pub start: usize,
    pub end: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HumanizerHit {
    pub category: HumanizerCategory,
    pub span: TextRange,
    pub matched: String,
    pub suggestion: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct HumanizerSummary {
    pub ai_vocabulary: u32,
    pub style_artifacts: u32,
    pub formatting_artifacts: u32,
    pub prompt_artifacts: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HumanizerReport {
    pub score: u8,
    pub hits: Vec<HumanizerHit>,
    pub summary: HumanizerSummary,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HumanizerOpts {
    pub score_threshold: u8,
}

impl Default for HumanizerOpts {
    fn default() -> Self {
        Self {
            score_threshold: 70,
        }
    }
}

pub fn score(text: &str, _opts: &HumanizerOpts) -> HumanizerReport {
    let mut hits = Vec::new();
    collect_dictionary_hits(
        text,
        HumanizerCategory::AiVocabulary,
        &[
            "delve",
            "tapestry",
            "testament",
            "underscore",
            "moreover",
            "additionally",
            "intricate",
            "intricacies",
            "pivotal",
            "vibrant",
            "showcase",
            "garner",
            "align with",
            "crucial",
            "enduring",
            "enhance",
            "fostering",
            "interplay",
            "valuable",
        ],
        None,
        &mut hits,
    );
    collect_dictionary_hits(
        text,
        HumanizerCategory::PromotionalLanguage,
        &[
            "nestled",
            "breathtaking",
            "must-visit",
            "stunning",
            "boasts",
            "renowned",
            "groundbreaking",
        ],
        None,
        &mut hits,
    );
    collect_dictionary_hits(
        text,
        HumanizerCategory::FillerPhrase,
        &[
            "in order to",
            "due to the fact that",
            "at this point in time",
            "in the event that",
            "the ability to",
            "it is important to note that",
            "it should be noted that",
        ],
        Some("Use shorter, direct wording."),
        &mut hits,
    );
    collect_dictionary_hits(
        text,
        HumanizerCategory::CopulaAvoidance,
        &["serves as", "stands as", "boasts"],
        Some("Prefer is/has."),
        &mut hits,
    );
    collect_dictionary_hits(
        text,
        HumanizerCategory::HedgingStack,
        &[
            "could potentially possibly",
            "might potentially",
            "may possibly perhaps",
        ],
        Some("Use one hedge or none."),
        &mut hits,
    );
    collect_dictionary_hits(
        text,
        HumanizerCategory::SycophanticOpener,
        &[
            "great question",
            "absolutely right",
            "certainly!",
            "of course!",
            "you're absolutely right",
        ],
        Some("Open with the actual reply."),
        &mut hits,
    );
    collect_dictionary_hits(
        text,
        HumanizerCategory::KnowledgeCutoffDisclaimer,
        &[
            "as of my last training",
            "based on available information",
            "while specific details are",
        ],
        None,
        &mut hits,
    );
    collect_dictionary_hits(
        text,
        HumanizerCategory::CollaborativeArtifact,
        &[
            "hope this helps",
            "would you like me to",
            "let me know if you'd like",
        ],
        None,
        &mut hits,
    );
    collect_dictionary_hits(
        text,
        HumanizerCategory::GenericPositiveConclusion,
        &[
            "exciting times ahead",
            "future looks bright",
            "step in the right direction",
        ],
        None,
        &mut hits,
    );
    collect_dictionary_hits(
        text,
        HumanizerCategory::NegativeParallelism,
        &["not just", "not only", "not merely"],
        Some("Avoid stock contrast framing."),
        &mut hits,
    );
    collect_dictionary_hits(
        text,
        HumanizerCategory::IngTailClause,
        &[
            ", highlighting",
            ", underscoring",
            ", emphasizing",
            ", ensuring",
            ", reflecting",
            ", symbolizing",
            ", contributing to",
            ", cultivating",
            ", fostering",
            ", encompassing",
            ", showcasing",
        ],
        Some("Make it a direct sentence or delete it."),
        &mut hits,
    );

    collect_char_hits(
        text,
        HumanizerCategory::CurlyQuotes,
        &['“', '”', '‘', '’'],
        &mut hits,
    );
    collect_em_dash_hits(text, &mut hits);
    collect_line_hits(text, &mut hits);
    collect_boldface_hit(text, &mut hits);
    collect_rule_of_three_hits(text, &mut hits);

    hits.sort_by_key(|hit| hit.span.start);
    let penalty: u32 = hits.iter().map(|hit| category_weight(hit.category)).sum();
    let score = 100_u32.saturating_sub(penalty).min(100) as u8;
    HumanizerReport {
        score,
        summary: summarize_hits(&hits),
        hits,
    }
}

pub fn writing_constraints() -> &'static str {
    "Avoid AI-writing patterns: no delve/tapestry/testament/underscore/moreover/additionally; no em dashes; no not-just-but framing; no comma + -ing tail clauses; no serves as/stands as/boasts; no filler phrases; no sycophantic openers; no knowledge-cutoff disclaimers; no curly quotes, emoji headings, or generic positive conclusions. Vary sentence rhythm. Use I when natural. Don't sound like a press release."
}

fn collect_dictionary_hits(
    text: &str,
    category: HumanizerCategory,
    terms: &[&str],
    suggestion: Option<&str>,
    hits: &mut Vec<HumanizerHit>,
) {
    let lower = text.to_ascii_lowercase();
    for term in terms {
        let mut offset = 0;
        while let Some(index) = lower[offset..].find(term) {
            let start = offset + index;
            let end = start + term.len();
            hits.push(HumanizerHit {
                category,
                span: TextRange { start, end },
                matched: text[start..end].to_string(),
                suggestion: suggestion.map(str::to_string),
            });
            offset = end;
        }
    }
}

fn collect_char_hits(
    text: &str,
    category: HumanizerCategory,
    chars: &[char],
    hits: &mut Vec<HumanizerHit>,
) {
    for (start, ch) in text.char_indices() {
        if chars.contains(&ch) {
            hits.push(HumanizerHit {
                category,
                span: TextRange {
                    start,
                    end: start + ch.len_utf8(),
                },
                matched: ch.to_string(),
                suggestion: Some("Use straight quotes.".to_string()),
            });
        }
    }
}

fn collect_em_dash_hits(text: &str, hits: &mut Vec<HumanizerHit>) {
    let sentence_count = text.matches(['.', '!', '?']).count().max(1);
    let em_dash_count = text.matches('—').count();
    if em_dash_count == 0 || (em_dash_count as f32 / sentence_count as f32) <= 0.15 {
        return;
    }
    collect_char_hits(text, HumanizerCategory::EmDashOveruse, &['—'], hits);
}

fn collect_line_hits(text: &str, hits: &mut Vec<HumanizerHit>) {
    let mut offset = 0;
    for line in text.lines() {
        let trimmed = line.trim_start();
        let start = offset + line.len().saturating_sub(trimmed.len());
        if trimmed.starts_with("- **") && trimmed.contains("**:") {
            hits.push(line_hit(
                HumanizerCategory::InlineHeaderColonList,
                start,
                trimmed,
            ));
        }
        if trimmed.ends_with(':') && trimmed.chars().next().is_some_and(|ch| !ch.is_ascii()) {
            hits.push(line_hit(HumanizerCategory::EmojiHeading, start, trimmed));
        }
        if trimmed.starts_with('#') {
            let words = trimmed.split_whitespace().skip(1).collect::<Vec<_>>();
            if words.len() >= 4
                && words
                    .iter()
                    .filter(|word| word.chars().next().is_some_and(char::is_uppercase))
                    .count()
                    >= 4
            {
                hits.push(line_hit(
                    HumanizerCategory::TitleCaseHeading,
                    start,
                    trimmed,
                ));
            }
        }
        offset += line.len() + 1;
    }
}

fn line_hit(category: HumanizerCategory, start: usize, line: &str) -> HumanizerHit {
    HumanizerHit {
        category,
        span: TextRange {
            start,
            end: start + line.len(),
        },
        matched: line.to_string(),
        suggestion: None,
    }
}

fn collect_boldface_hit(text: &str, hits: &mut Vec<HumanizerHit>) {
    let bold_chars = text.matches("**").count().saturating_mul(2);
    if text.len() > 80 && bold_chars as f32 / text.len() as f32 > 0.05 {
        hits.push(HumanizerHit {
            category: HumanizerCategory::ExcessiveBoldface,
            span: TextRange {
                start: 0,
                end: text.len(),
            },
            matched: "excessive boldface".to_string(),
            suggestion: Some("Use plain text.".to_string()),
        });
    }
}

fn collect_rule_of_three_hits(text: &str, hits: &mut Vec<HumanizerHit>) {
    for sentence in text.split(['.', '!', '?']) {
        if sentence.matches(',').count() >= 2 && sentence.split(',').count() >= 3 {
            if let Some(start) = text.find(sentence) {
                hits.push(HumanizerHit {
                    category: HumanizerCategory::RuleOfThree,
                    span: TextRange {
                        start,
                        end: start + sentence.len(),
                    },
                    matched: sentence.trim().to_string(),
                    suggestion: Some("Prefer the exact number of points needed.".to_string()),
                });
            }
        }
    }
}

fn category_weight(category: HumanizerCategory) -> u32 {
    match category {
        HumanizerCategory::SycophanticOpener => 10,
        HumanizerCategory::NegativeParallelism => 8,
        HumanizerCategory::IngTailClause => 6,
        HumanizerCategory::EmDashOveruse | HumanizerCategory::CopulaAvoidance => 5,
        HumanizerCategory::AiVocabulary | HumanizerCategory::PromotionalLanguage => 4,
        HumanizerCategory::KnowledgeCutoffDisclaimer => 10,
        HumanizerCategory::CollaborativeArtifact => 6,
        _ => 3,
    }
}

fn summarize_hits(hits: &[HumanizerHit]) -> HumanizerSummary {
    let mut summary = HumanizerSummary::default();
    for hit in hits {
        match hit.category {
            HumanizerCategory::AiVocabulary | HumanizerCategory::PromotionalLanguage => {
                summary.ai_vocabulary += 1;
            }
            HumanizerCategory::EmojiHeading
            | HumanizerCategory::TitleCaseHeading
            | HumanizerCategory::InlineHeaderColonList
            | HumanizerCategory::ExcessiveBoldface
            | HumanizerCategory::CurlyQuotes => summary.formatting_artifacts += 1,
            HumanizerCategory::KnowledgeCutoffDisclaimer
            | HumanizerCategory::CollaborativeArtifact
            | HumanizerCategory::SycophanticOpener => summary.prompt_artifacts += 1,
            _ => summary.style_artifacts += 1,
        }
    }
    summary
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ai_smell_text_scores_low_and_reports_hits() {
        let report = score(
            "Great question. Additionally, this serves as a testament — not just showcasing our commitment to innovation, but fostering a vibrant tapestry of ideas.",
            &HumanizerOpts::default(),
        );
        assert!(report.score < 70);
        assert!(report
            .hits
            .iter()
            .any(|hit| hit.category == HumanizerCategory::AiVocabulary));
        assert!(report
            .hits
            .iter()
            .any(|hit| hit.category == HumanizerCategory::CopulaAvoidance));
        assert!(report
            .hits
            .iter()
            .any(|hit| hit.category == HumanizerCategory::SycophanticOpener));
    }

    #[test]
    fn plain_email_scores_high() {
        let report = score(
            "Thanks for sending this over. I can review it tomorrow morning and send notes by lunch.",
            &HumanizerOpts::default(),
        );
        assert!(report.score >= 90);
        assert!(report.hits.is_empty());
    }
}
