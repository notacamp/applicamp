//! The job's own questions, read from the posting's text without a model.
//!
//! Simple and deterministic on purpose: the lines that end in `?` under an
//! "Application questions" heading (or its translation). Nothing is scraped
//! from an application form; that is a later issue.

/// The most questions one letter answers.
pub const MAX_QUESTIONS: usize = 5;
/// The longest question kept, in characters.
const MAX_QUESTION: usize = 240;
/// The shortest, so a stray "Why?" is not a question of the job.
const MIN_QUESTION: usize = 8;

/// What a heading of the section may read, lower case.
const HEADINGS: &[&str] = &[
    "application questions",
    "questions for applicants",
    "questions to answer",
    "screening questions",
    "perguntas da candidatura",
    "perguntas de candidatura",
    "perguntas para candidatos",
    "preguntas de la solicitud",
    "preguntas de la candidatura",
    "preguntas para candidatos",
    "questions de candidature",
    "questions de la candidature",
    "questions pour les candidats",
];

fn unmarked(line: &str) -> &str {
    let line = line.trim();
    let line = line
        .trim_start_matches(['-', '*', '•', '·', '#'])
        .trim_start();
    // "1. " or "2) "
    let digits = line.chars().take_while(char::is_ascii_digit).count();
    if digits > 0
        && let Some(rest) = line[digits..]
            .strip_prefix('.')
            .or_else(|| line[digits..].strip_prefix(')'))
    {
        return rest.trim_start();
    }
    line
}

fn is_heading(line: &str) -> bool {
    let line = unmarked(line)
        .trim_end_matches([':', '.'])
        .trim()
        .to_lowercase();
    line.chars().count() <= 60 && HEADINGS.contains(&line.as_str())
}

/// The questions the posting asks of its applicants, in its order.
pub fn parse(posting: &str) -> Vec<String> {
    let mut found: Vec<String> = Vec::new();
    let mut lines = posting.lines();
    while let Some(line) = lines.next() {
        if !is_heading(line) {
            continue;
        }
        let mut intro = false;
        for line in lines.by_ref() {
            let line = unmarked(line);
            if line.is_empty() {
                continue;
            }
            if line.ends_with(['?', '？']) {
                let count = line.chars().count();
                if (MIN_QUESTION..=MAX_QUESTION).contains(&count)
                    && !found.iter().any(|q| q.eq_ignore_ascii_case(line))
                {
                    found.push(line.to_owned());
                }
                continue;
            }
            // One line of introduction ("Please answer:") before the first one.
            if found.is_empty() && !intro && line.ends_with(':') {
                intro = true;
                continue;
            }
            break;
        }
        if !found.is_empty() {
            break;
        }
    }
    found.truncate(MAX_QUESTIONS);
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_questions_under_the_heading_are_read_in_order() {
        let posting = "Staff Engineer\nYou will own billing.\n\nApplication questions\n\
                       - Why do you want to work at Example Co?\n- What's your notice period?\n\
                       Benefits\n- Is there a gym?\n";
        assert_eq!(
            parse(posting),
            [
                "Why do you want to work at Example Co?",
                "What's your notice period?"
            ]
        );
    }

    #[test]
    fn a_question_outside_the_section_is_not_the_jobs() {
        assert_eq!(
            parse("Can you start soon?\nRust required.\n"),
            Vec::<String>::new()
        );
        assert_eq!(
            parse("Requirements\n- Do you know Rust?\n"),
            Vec::<String>::new()
        );
    }

    #[test]
    fn numbers_an_introduction_other_languages_and_the_cap_are_handled() {
        let posting = "Perguntas da candidatura:\nResponda:\n1. Porque quer trabalhar connosco?\n\
                       2) Qual é o seu pré-aviso?\n3. Q three here?\n4. Q four here?\n\
                       5. Q five here?\n6. Q six here?\n";
        let found = parse(posting);
        assert_eq!(found.len(), MAX_QUESTIONS);
        assert_eq!(found[0], "Porque quer trabalhar connosco?");
        assert_eq!(found[1], "Qual é o seu pré-aviso?");
    }
}
