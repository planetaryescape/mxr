//! `ask_user`: the assistant asks the user a structured question.
//!
//! Prose questions are fine for open conversation. This tool is for the
//! moment a choice decides what happens next (which account, which of
//! three matches, how wide a cleanup should go): the app shows the options
//! as a card and the turn waits for the answer, which comes back as the
//! tool result.

use mxr_llm::ToolSpec;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

pub const ASK_USER_TOOL: &str = "ask_user";
const MAX_QUESTIONS: usize = 4;
const MAX_OPTIONS: usize = 6;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Question {
    pub question: String,
    /// A short label for the card, at most a few words.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub header: Option<String>,
    #[serde(default)]
    pub options: Vec<QuestionOption>,
    #[serde(default)]
    pub multi_select: bool,
    /// Whether the user may type an answer instead of picking an option.
    /// Always true when there are no options.
    #[serde(default = "default_true")]
    pub allow_other: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QuestionOption {
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Answer {
    /// Labels of the chosen options, in the order the user chose them.
    #[serde(default)]
    pub selected: Vec<String>,
    /// What the user typed instead of, or as well as, choosing.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub other: Option<String>,
}

fn default_true() -> bool {
    true
}

pub fn spec() -> ToolSpec {
    ToolSpec {
        name: ASK_USER_TOOL.to_string(),
        description: "Ask the user to choose before you go on, shown as a card with options in the app. \
Use it when the request is ambiguous and the answer changes what you do: which account, which of several \
matching emails or senders, how wide a cleanup should go, which of two readings of the request they meant. \
Give 2 to 6 concrete options drawn from what you found, best first; the user can also type their own answer \
unless allow_other is false. Ask up to 4 related questions at once. Don't use it for yes/no confirmation of \
a change: calling the change itself already asks the user to approve it. The result is the user's answer."
            .to_string(),
        input_schema: json!({
            "type": "object",
            "properties": {
                "questions": {
                    "type": "array",
                    "minItems": 1,
                    "maxItems": MAX_QUESTIONS,
                    "items": {
                        "type": "object",
                        "properties": {
                            "question": {"type": "string", "description": "The full question, ending in a question mark."},
                            "header": {"type": "string", "description": "A label of at most three words, e.g. \"Account\"."},
                            "options": {
                                "type": "array",
                                "maxItems": MAX_OPTIONS,
                                "items": {
                                    "type": "object",
                                    "properties": {
                                        "label": {"type": "string", "description": "One to five words."},
                                        "description": {"type": "string", "description": "What choosing it means."}
                                    },
                                    "required": ["label"]
                                }
                            },
                            "multi_select": {"type": "boolean", "description": "Allow more than one option."},
                            "allow_other": {"type": "boolean", "description": "Let the user type their own answer. Defaults to true."}
                        },
                        "required": ["question"]
                    }
                }
            },
            "required": ["questions"]
        }),
    }
}

/// Read the model's questions, or say what is wrong with them so it can
/// ask again.
pub fn parse_questions(arguments: &Value) -> Result<Vec<Question>, String> {
    let questions: Vec<Question> = serde_json::from_value(
        arguments
            .get("questions")
            .cloned()
            .ok_or("ask_user needs a `questions` array")?,
    )
    .map_err(|error| format!("ask_user questions are malformed: {error}"))?;
    if questions.is_empty() || questions.len() > MAX_QUESTIONS {
        return Err(format!("ask_user takes 1 to {MAX_QUESTIONS} questions"));
    }
    for question in &questions {
        if question.question.trim().is_empty() {
            return Err("every ask_user question needs text".to_string());
        }
        if question.options.len() > MAX_OPTIONS {
            return Err(format!(
                "ask_user takes at most {MAX_OPTIONS} options per question"
            ));
        }
        let mut labels: Vec<&str> = question.options.iter().map(|o| o.label.trim()).collect();
        if labels.iter().any(|label| label.is_empty()) {
            return Err("every ask_user option needs a label".to_string());
        }
        labels.sort_unstable();
        labels.dedup();
        if labels.len() != question.options.len() {
            return Err("ask_user option labels must be unique within a question".to_string());
        }
    }
    Ok(questions)
}

/// Check the user's answers against the questions asked.
pub fn validate_answers(questions: &[Question], answers: &[Answer]) -> Result<(), String> {
    if answers.len() != questions.len() {
        return Err(format!(
            "expected {} answers, got {}",
            questions.len(),
            answers.len()
        ));
    }
    for (index, (question, answer)) in questions.iter().zip(answers).enumerate() {
        let number = index + 1;
        for label in &answer.selected {
            if !question.options.iter().any(|option| &option.label == label) {
                return Err(format!(
                    "answer {number}: '{label}' is not one of the options"
                ));
            }
        }
        if !question.multi_select && answer.selected.len() > 1 {
            return Err(format!("answer {number}: choose one option"));
        }
        let other = answer.other.as_deref().map(str::trim).unwrap_or_default();
        let typed_allowed = question.allow_other || question.options.is_empty();
        if !other.is_empty() && !typed_allowed {
            return Err(format!("answer {number}: pick one of the options"));
        }
        if answer.selected.is_empty() && other.is_empty() {
            return Err(format!("answer {number} is empty"));
        }
    }
    Ok(())
}

/// The tool result the model reads. The user wrote it, so it is not
/// fenced as untrusted mail.
pub fn result_content(questions: &[Question], answers: &[Answer]) -> String {
    let answered: Vec<Value> = questions
        .iter()
        .zip(answers)
        .map(|(question, answer)| {
            json!({
                "question": question.question,
                "selected": answer.selected,
                "other": answer.other.as_deref().map(str::trim).filter(|text| !text.is_empty()),
            })
        })
        .collect();
    json!({ "source": "user", "answers": answered }).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn question(options: &[&str], multi: bool, other: bool) -> Question {
        Question {
            question: "Which?".into(),
            header: None,
            options: options
                .iter()
                .map(|label| QuestionOption {
                    label: (*label).into(),
                    description: None,
                })
                .collect(),
            multi_select: multi,
            allow_other: other,
        }
    }

    fn answer(selected: &[&str], other: Option<&str>) -> Answer {
        Answer {
            selected: selected.iter().map(|s| (*s).into()).collect(),
            other: other.map(Into::into),
        }
    }

    #[test]
    fn questions_parse_with_defaults_and_reject_duplicate_labels() {
        let parsed = parse_questions(&json!({"questions": [
            {"question": "Which account?", "options": [{"label": "Work"}, {"label": "Home"}]}
        ]}))
        .unwrap();
        assert!(parsed[0].allow_other);
        assert!(!parsed[0].multi_select);
        assert!(parse_questions(&json!({"questions": [
            {"question": "x", "options": [{"label": "A"}, {"label": "A"}]}
        ]}))
        .is_err());
        assert!(parse_questions(&json!({"questions": []})).is_err());
        assert!(parse_questions(&json!({})).is_err());
    }

    #[test]
    fn answers_must_fit_their_question() {
        let single = [question(&["A", "B"], false, false)];
        assert!(validate_answers(&single, &[answer(&["A"], None)]).is_ok());
        assert!(validate_answers(&single, &[answer(&["A", "B"], None)]).is_err());
        assert!(validate_answers(&single, &[answer(&["C"], None)]).is_err());
        assert!(validate_answers(&single, &[answer(&[], Some("mine"))]).is_err());
        assert!(validate_answers(&single, &[answer(&[], None)]).is_err());
        assert!(validate_answers(&single, &[]).is_err());

        let open = [question(&[], false, false)];
        assert!(validate_answers(&open, &[answer(&[], Some("typed"))]).is_ok());

        let multi = [question(&["A", "B"], true, true)];
        assert!(validate_answers(&multi, &[answer(&["A", "B"], Some("and more"))]).is_ok());
    }

    #[test]
    fn the_result_names_the_user_as_its_source() {
        let questions = [question(&["A"], false, true)];
        let content: Value =
            serde_json::from_str(&result_content(&questions, &[answer(&["A"], Some("  "))]))
                .unwrap();
        assert_eq!(content["source"], "user");
        assert_eq!(content["answers"][0]["selected"], json!(["A"]));
        assert_eq!(content["answers"][0]["other"], Value::Null);
    }
}
