use serde::{Deserialize, Serialize};

mod catalogue;
pub use catalogue::{
    email_style_instructions, Detector, Pattern, PatternGroup, HUMANIZER_VERSION, PATTERNS,
};

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
    FigurativeLandscape,
    FigurativeKey,
    HighlightVerb,
    Profanity,
    FalseRange,
    ChallengesProspects,
}

/// Compatibility map from the original 24 categories into humanizer 3.1.
/// mxr-only categories have no pattern number.
pub fn pattern_for_category(category: HumanizerCategory) -> Option<u8> {
    Some(match category {
        HumanizerCategory::NegativeParallelism => 1,
        HumanizerCategory::GenericPositiveConclusion => 2,
        HumanizerCategory::FigurativeLandscape | HumanizerCategory::FigurativeKey => 3,
        HumanizerCategory::RuleOfThree => 6,
        HumanizerCategory::EmDashOveruse => 8,
        HumanizerCategory::HedgingStack => 9,
        HumanizerCategory::AiVocabulary => 12,
        HumanizerCategory::HighlightVerb | HumanizerCategory::ChallengesProspects => 13,
        HumanizerCategory::IngTailClause => 15,
        HumanizerCategory::PromotionalLanguage => 16,
        HumanizerCategory::CopulaAvoidance => 18,
        HumanizerCategory::ExcessiveBoldface | HumanizerCategory::InlineHeaderColonList => 19,
        HumanizerCategory::EmojiHeading | HumanizerCategory::TitleCaseHeading => 20,
        HumanizerCategory::CurlyQuotes => 21,
        HumanizerCategory::SycophanticOpener
        | HumanizerCategory::CollaborativeArtifact
        | HumanizerCategory::FillerPhrase => 22,
        HumanizerCategory::KnowledgeCutoffDisclaimer => 23,
        HumanizerCategory::Profanity | HumanizerCategory::FalseRange => return None,
    })
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

pub fn score(text: &str, opts: &HumanizerOpts) -> HumanizerReport {
    score_internal(text, opts, true)
}

fn score_internal(text: &str, _opts: &HumanizerOpts, suppress_weak_alone: bool) -> HumanizerReport {
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
        HumanizerCategory::FigurativeLandscape,
        &[
            "digital landscape",
            "business landscape",
            "evolving landscape",
        ],
        Some("Name the concrete setting."),
        &mut hits,
    );
    collect_dictionary_hits(
        text,
        HumanizerCategory::FigurativeKey,
        &["key driver", "key pillar", "key enabler", "key factor"],
        Some("Use specific importance instead of key."),
        &mut hits,
    );
    collect_dictionary_hits(
        text,
        HumanizerCategory::HighlightVerb,
        &["highlights the importance", "highlighting the importance"],
        Some("State the point directly."),
        &mut hits,
    );
    collect_dictionary_hits(
        text,
        HumanizerCategory::Profanity,
        &["damn", "shit", "fuck"],
        Some("Avoid profanity unless it is truly the sender's voice."),
        &mut hits,
    );
    collect_dictionary_hits(
        text,
        HumanizerCategory::FalseRange,
        &[
            "from x to y",
            "from ideation to execution",
            "from concept to reality",
        ],
        Some("Use the specific range or delete it."),
        &mut hits,
    );
    collect_dictionary_hits(
        text,
        HumanizerCategory::ChallengesProspects,
        &["challenges and prospects", "opportunities and challenges"],
        Some("Name the concrete trade-off."),
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

    // Formatting tells marked weak-alone by the source skill only count when
    // a separate tell appears in the same paragraph.
    let all_hits = hits.clone();
    hits.retain(|hit| {
        if !suppress_weak_alone {
            return true;
        }
        if !matches!(
            hit.category,
            HumanizerCategory::EmDashOveruse
                | HumanizerCategory::CurlyQuotes
                | HumanizerCategory::ExcessiveBoldface
                | HumanizerCategory::InlineHeaderColonList
                | HumanizerCategory::EmojiHeading
                | HumanizerCategory::TitleCaseHeading
        ) {
            return true;
        }
        let paragraph = paragraph_range(text, hit.span.start);
        all_hits.iter().any(|other| {
            other.category != hit.category && paragraph_range(text, other.span.start) == paragraph
        })
    });

    hits.sort_by_key(|hit| hit.span.start);
    let penalty: u32 = hits.iter().map(|hit| category_weight(hit.category)).sum();
    let score = 100_u32.saturating_sub(penalty).min(100) as u8;
    HumanizerReport {
        score,
        summary: summarize_hits(&hits),
        hits,
    }
}

/// Normalized rates from the writer's own recent sent mail. The values are
/// detections per thousand words, so a long sample does not dominate by size.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PatternRates {
    counts: std::collections::BTreeMap<u8, u64>,
    words: u64,
}

impl PatternRates {
    pub fn from_samples<'a>(samples: impl IntoIterator<Item = &'a str>) -> Self {
        let mut counts = std::collections::BTreeMap::<u8, u64>::new();
        let mut words = 0_u64;
        for text in samples {
            words += text.split_whitespace().count() as u64;
            // Learn raw habits even when weak-alone detections are suppressed
            // during draft scoring.
            for hit in score_internal(text, &HumanizerOpts::default(), false).hits {
                if let Some(pattern) = pattern_for_category(hit.category) {
                    *counts.entry(pattern).or_default() += 1;
                }
            }
        }
        Self { counts, words }
    }

    pub fn rate(&self, pattern: u8) -> f32 {
        if self.words == 0 {
            0.0
        } else {
            self.counts.get(&pattern).copied().unwrap_or(0) as f32 * 1000.0 / self.words as f32
        }
    }
}

/// Score only the detections that exceed the writer's normalized rate.
pub fn score_with_rates(text: &str, opts: &HumanizerOpts, rates: &PatternRates) -> HumanizerReport {
    let mut report = score(text, opts);
    let words = text.split_whitespace().count() as u64;
    let mut observed = std::collections::BTreeMap::<u8, u64>::new();
    report.hits.retain(|hit| {
        let Some(pattern) = pattern_for_category(hit.category) else {
            return true;
        };
        let count = observed.entry(pattern).or_default();
        *count += 1;
        // Compare cross-products against the unrounded historical rate. This
        // avoids granting a whole occurrence to a short draft for a tiny rate.
        let historical = rates.counts.get(&pattern).copied().unwrap_or(0);
        rates.words == 0 || count.saturating_mul(rates.words) > historical.saturating_mul(words)
    });
    let penalty: u32 = report
        .hits
        .iter()
        .map(|hit| category_weight(hit.category))
        .sum();
    report.score = 100_u32.saturating_sub(penalty).min(100) as u8;
    report.summary = summarize_hits(&report.hits);
    report
}

fn paragraph_range(text: &str, offset: usize) -> (usize, usize) {
    let before = text[..offset].rfind("\n\n").map_or(0, |start| start + 2);
    let after = text[offset..]
        .find("\n\n")
        .map_or(text.len(), |end| offset + end);
    (before, after)
}

pub fn writing_constraints() -> &'static str {
    // Compatibility for legacy callers. Shared compose paths use the
    // catalogue-derived email_writing_constraints below.
    "Avoid AI-writing patterns: no delve/tapestry/testament/underscore/moreover/additionally; no em dashes; no not-just-but framing; no comma + -ing tail clauses; no serves as/stands as/boasts; no filler phrases; no sycophantic openers; no knowledge-cutoff disclaimers; no curly quotes, emoji headings, or generic positive conclusions. Vary sentence rhythm. Use I when natural. Don't sound like a press release."
}

/// Catalogue-derived constraints for the shared compose-assist path.
pub fn email_writing_constraints() -> String {
    format!(
        "Use the writer's own voice and preserve supported facts. Apply these email style rules:\n{}",
        email_style_instructions()
    )
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
        HumanizerCategory::AiVocabulary
        | HumanizerCategory::PromotionalLanguage
        | HumanizerCategory::FigurativeLandscape
        | HumanizerCategory::FigurativeKey
        | HumanizerCategory::HighlightVerb
        | HumanizerCategory::ChallengesProspects => 4,
        HumanizerCategory::KnowledgeCutoffDisclaimer => 10,
        HumanizerCategory::CollaborativeArtifact => 6,
        _ => 3,
    }
}

fn summarize_hits(hits: &[HumanizerHit]) -> HumanizerSummary {
    let mut summary = HumanizerSummary::default();
    for hit in hits {
        match hit.category {
            HumanizerCategory::AiVocabulary
            | HumanizerCategory::PromotionalLanguage
            | HumanizerCategory::FigurativeLandscape
            | HumanizerCategory::FigurativeKey
            | HumanizerCategory::HighlightVerb
            | HumanizerCategory::ChallengesProspects => {
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
    fn catalogue_names_and_numbers_match_the_vendored_skill() {
        let skill = include_str!("../fixtures/humanizer-3.1.0-bk.1.md");
        let names: Vec<(u8, &str)> = skill
            .lines()
            .filter_map(|line| {
                let rest = line.strip_prefix("### ")?;
                let (number, name) = rest.split_once('.')?;
                Some((number.parse().ok()?, name.trim()))
            })
            .collect();
        assert_eq!(names.len(), 26);
        for (pattern, (number, name)) in PATTERNS.iter().zip(names) {
            assert_eq!(pattern.number, number);
            assert_eq!(pattern.name, name);
        }
        assert_eq!(HUMANIZER_VERSION, "3.1.0+mxr.1");
        assert!(!PATTERNS[19].applies_to_email);
        assert!(!PATTERNS[23].applies_to_email);
    }

    #[test]
    fn email_prompt_uses_catalogue_instructions_and_omits_non_email_patterns() {
        let prompt = email_style_instructions();
        assert!(prompt.contains("§1 Say the point directly"));
        assert!(!prompt.contains("§20 "));
        assert!(!prompt.contains("§24 "));
    }

    #[test]
    fn legacy_categories_map_to_catalogue_patterns_and_mxr_categories_stay_separate() {
        assert_eq!(
            pattern_for_category(HumanizerCategory::NegativeParallelism),
            Some(1)
        );
        assert_eq!(
            pattern_for_category(HumanizerCategory::IngTailClause),
            Some(15)
        );
        assert_eq!(pattern_for_category(HumanizerCategory::FalseRange), None);
        assert_eq!(pattern_for_category(HumanizerCategory::Profanity), None);
    }

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

    #[test]
    fn no_history_has_zero_allowance_and_writer_dash_rate_is_normalized() {
        let draft = "One — two. Three — four. Five. Six. Seven. Eight. Moreover, this is clear.";
        let no_history = PatternRates::from_samples(std::iter::empty());
        assert_eq!(
            score_with_rates(draft, &HumanizerOpts::default(), &no_history)
                .hits
                .len(),
            score(draft, &HumanizerOpts::default()).hits.len()
        );

        let sample =
            "One — two. Three — four. Five. Six. Seven. Eight. Moreover, this is clear.".repeat(20);
        let writer = PatternRates::from_samples([sample.as_str()]);
        let allowed = score_with_rates(draft, &HumanizerOpts::default(), &writer);
        assert!(!allowed
            .hits
            .iter()
            .any(|hit| hit.category == HumanizerCategory::EmDashOveruse));
        let excess = format!("{draft} Nine — ten. Eleven — twelve. Thirteen — fourteen.");
        assert!(
            score_with_rates(&excess, &HumanizerOpts::default(), &writer)
                .hits
                .iter()
                .any(|hit| hit.category == HumanizerCategory::EmDashOveruse)
        );
    }

    #[test]
    fn rates_compare_normalized_counts_without_rounding_or_weak_hit_loss() {
        let ten_words = "One — two three four five six seven eight nine ten.";
        let equal = PatternRates::from_samples([ten_words]);
        assert!(
            !score_with_rates(ten_words, &HumanizerOpts::default(), &equal)
                .hits
                .iter()
                .any(|hit| hit.category == HumanizerCategory::EmDashOveruse)
        );

        let long_history = format!("One — two. {}", "word ".repeat(990));
        let tiny_rate = PatternRates::from_samples([long_history.as_str()]);
        let short_draft =
            "One — two three four five six seven eight nine. Moreover, this is specific.";
        assert!(
            score_with_rates(short_draft, &HumanizerOpts::default(), &tiny_rate)
                .hits
                .iter()
                .any(|hit| hit.category == HumanizerCategory::EmDashOveruse)
        );

        let habitual_weak_alone = "A — B C D E. ".repeat(200);
        let habit = PatternRates::from_samples([habitual_weak_alone.as_str()]);
        let accompanied = "A — B. Additionally, another point.";
        assert!(score(accompanied, &HumanizerOpts::default())
            .hits
            .iter()
            .any(|hit| hit.category == HumanizerCategory::EmDashOveruse));
        assert!(
            !score_with_rates(accompanied, &HumanizerOpts::default(), &habit)
                .hits
                .iter()
                .any(|hit| hit.category == HumanizerCategory::EmDashOveruse)
        );

        let above_rate = "A — B. C — D. E — F. G — H. I — J. Moreover.";
        assert!(
            score_with_rates(above_rate, &HumanizerOpts::default(), &habit)
                .hits
                .iter()
                .any(|hit| hit.category == HumanizerCategory::EmDashOveruse)
        );
    }

    #[test]
    fn weak_heading_tell_needs_a_second_tell_in_the_same_paragraph() {
        let alone = score(
            "🚀 Summary:\nThe meeting is at noon.",
            &HumanizerOpts::default(),
        );
        assert!(!alone
            .hits
            .iter()
            .any(|hit| hit.category == HumanizerCategory::EmojiHeading));
        let paired = score(
            "🚀 Summary:\ndelve into the details.",
            &HumanizerOpts::default(),
        );
        assert!(paired
            .hits
            .iter()
            .any(|hit| hit.category == HumanizerCategory::EmojiHeading));
    }

    #[test]
    fn weak_formatting_tell_counts_only_beside_a_separate_tell_in_its_paragraph() {
        let alone = score("He said “ok”.", &HumanizerOpts::default());
        assert!(!alone
            .hits
            .iter()
            .any(|hit| hit.category == HumanizerCategory::CurlyQuotes));
        let with_tell = score("He said “ok”. Great question.", &HumanizerOpts::default());
        assert!(with_tell
            .hits
            .iter()
            .any(|hit| hit.category == HumanizerCategory::CurlyQuotes));
    }

    #[test]
    fn detects_generic_landscape_and_challenges_language() {
        let report = score(
            "This key driver shapes the evolving landscape and creates opportunities and challenges.",
            &HumanizerOpts::default(),
        );
        assert!(report
            .hits
            .iter()
            .any(|hit| hit.category == HumanizerCategory::FigurativeKey));
        assert!(report
            .hits
            .iter()
            .any(|hit| hit.category == HumanizerCategory::FigurativeLandscape));
        assert!(report
            .hits
            .iter()
            .any(|hit| hit.category == HumanizerCategory::ChallengesProspects));
    }
}
