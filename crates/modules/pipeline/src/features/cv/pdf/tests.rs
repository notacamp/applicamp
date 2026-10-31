//! The PDF: both templates, the person's text as data, the length, and the
//! read-back. All data is invented.

use super::{
    Length, Refusal, Template, data, make,
    readback::{self, Extracted},
};
use crate::{
    features::cv::model::{
        Contact, Content, EducationItem, Frame, LanguageItem, Line, Role, SkillItem,
    },
    ports::{CvPosition, CvProfile},
};

fn line(id: &str, text: &str) -> Line {
    Line {
        id: id.into(),
        text: text.into(),
        sources: vec!["hl".into()],
        edited: false,
    }
}

fn frame() -> Frame {
    Frame::build(
        &CvProfile {
            positions: vec![
                CvPosition {
                    index: 0,
                    title: Some("Senior Engineer".into()),
                    company: Some("Example Org".into()),
                    location: Some("Porto".into()),
                    start_date: Some("2021-03".into()),
                    ..CvPosition::default()
                },
                CvPosition {
                    index: 1,
                    title: Some("Engineer".into()),
                    company: Some("Sample Labs".into()),
                    start_date: Some("2017-06".into()),
                    end_date: Some("2021-02".into()),
                    ..CvPosition::default()
                },
            ],
            ..CvProfile::default()
        },
        "Example Co",
        &[],
    )
}

fn content(bullets: &[&str]) -> Content {
    Content {
        contact: Contact {
            name: Some("Alex Demo".into()),
            email: Some("alex.demo@example.com".into()),
            location: Some("Porto, Portugal".into()),
            url: Some("https://alex.example.com".into()),
            links: vec![(Some("GitHub".into()), "https://github.com/alex-demo".into())],
        },
        headline: Some(line("l1", "Backend engineer who ships billing systems")),
        summary: Some(line(
            "l2",
            "Eight years of backend work on billing platforms.",
        )),
        experience: vec![
            Role {
                position: 0,
                bullets: bullets
                    .iter()
                    .enumerate()
                    .map(|(i, b)| line(&format!("b{i}"), b))
                    .collect(),
            },
            Role {
                position: 1,
                bullets: vec![line("c1", "Built ingestion for 3 partner feeds")],
            },
        ],
        skills: vec![
            SkillItem {
                id: "k1".into(),
                name: "Rust".into(),
            },
            SkillItem {
                id: "k2".into(),
                name: "PostgreSQL".into(),
            },
        ],
        education: vec![EducationItem {
            institution: "Example University".into(),
            area: Some("Computer science".into()),
            study_type: Some("BSc".into()),
            start_date: None,
            end_date: None,
        }],
        languages: vec![LanguageItem {
            language: "English".into(),
            fluency: Some("Fluent".into()),
        }],
        next_line: 10,
    }
}

const BULLETS: [&str; 2] = [
    "Rebuilt the billing pipeline in Rust, cutting invoice runs from 6 hours to 40 minutes",
    "Led a team of 4 engineers",
];

async fn made(
    content: &Content,
    language: &str,
    template: Template,
    length: Length,
) -> Result<super::Made, Refusal> {
    make(content, &frame(), language, template, length, Vec::new()).await
}

#[tokio::test]
async fn both_templates_render_and_the_read_back_finds_every_section_and_line() {
    for template in [Template::Clean, Template::Compact] {
        let made = made(&content(&BULLETS), "en", template, Length::One)
            .await
            .unwrap_or_else(|r| panic!("{template:?}: {r:?}"));
        assert!(made.bytes.starts_with(b"%PDF-"));
        assert_eq!(made.pages, 1);
        let checks = &made.checks;
        assert_eq!(
            checks.sections.expected,
            ["Summary", "Experience", "Skills", "Education", "Languages"]
        );
        assert!(checks.sections.missing.is_empty(), "{checks:?}");
        assert!(checks.lines.missing.is_empty(), "{checks:?}");
        assert!(checks.lines.total >= 10, "{}", checks.lines.total);
        assert!(checks.text_layer);
        assert!(checks.contact.name && checks.contact.email && checks.contact.links);
        assert!(checks.passed());
        // The name and the links are text a person can select, and links are links.
        let read = readback::extract(&made.bytes).expect("readable");
        assert!(read.text.contains("alex.demo@example.com"), "{}", read.text);
        assert!(read.text.contains("github.com/alex-demo"));
        assert_eq!(read.link_targets, 3);
        assert_eq!(read.images, 0);
    }
}

#[tokio::test]
async fn the_headings_are_the_standard_ones_in_the_language_of_the_cv() {
    for (tag, headings) in [
        (
            "pt-PT",
            [
                "Resumo",
                "Experiência",
                "Competências",
                "Formação",
                "Línguas",
            ],
        ),
        (
            "es-ES",
            [
                "Resumen",
                "Experiencia",
                "Competencias",
                "Formación",
                "Idiomas",
            ],
        ),
        (
            "fr",
            [
                "Résumé",
                "Expérience",
                "Compétences",
                "Formation",
                "Langues",
            ],
        ),
    ] {
        let made = made(&content(&BULLETS), tag, Template::Clean, Length::One)
            .await
            .unwrap_or_else(|r| panic!("{tag}: {r:?}"));
        assert_eq!(made.checks.sections.expected, headings, "{tag}");
        assert!(
            made.checks.sections.missing.is_empty(),
            "{tag}: {:?}",
            made.checks
        );
    }
}

/// What a person could type that Typst would read as something else.
const HOSTILE: [&str; 9] = [
    "#import \"@preview/x:0.1.0\": *",
    "a ] closing bracket and a [ opening one",
    "Costs $5 and $x^2$ here",
    "*not bold* and _not italic_",
    "<x> and <label> are not labels",
    "see @ref and @fig-1",
    "#sys.inputs.cv leaks nothing",
    "#read(\"/etc/passwd\")",
    "= not a heading, - not a list, + not numbered",
];

#[tokio::test]
async fn what_the_person_types_is_text_never_typst() {
    let mut cv = content(&HOSTILE);
    cv.contact.name = Some("Alex #Demo *Star* _U_ $m$".into());
    cv.summary = Some(line(
        "l2",
        "#set text(size: 40pt) and `raw` and \\ backslash",
    ));
    for template in [Template::Clean, Template::Compact] {
        let made = made(&cv, "en", template, Length::Two)
            .await
            .unwrap_or_else(|r| panic!("{template:?}: {r:?}"));
        let read = readback::extract(&made.bytes).expect("readable");
        let flat = readback::flat(&read.text);
        for text in HOSTILE {
            assert!(
                flat.contains(&readback::flat(text)),
                "{text:?} did not come out literally of {template:?}: {}",
                read.text
            );
        }
        assert!(flat.contains(&readback::flat("Alex #Demo *Star* _U_ $m$")));
        assert!(flat.contains(&readback::flat(
            "#set text(size: 40pt) and `raw` and \\ backslash"
        )));
        assert!(made.checks.passed(), "{:?}", made.checks);
        // Nothing of the compiler's own world leaked in.
        assert!(!read.text.contains("passwd:"));
    }
}

#[tokio::test]
async fn a_cv_that_does_not_fit_one_page_is_not_made_but_two_pages_take_it() {
    let many: Vec<String> = (0..70)
        .map(|i| format!("Delivered project number {i} for a partner, on time and within budget"))
        .collect();
    let many: Vec<&str> = many.iter().map(String::as_str).collect();
    let cv = content(&many);
    match made(&cv, "en", Template::Clean, Length::One).await {
        Err(Refusal::DoesntFit { pages, length }) => {
            assert_eq!(length, 1);
            assert!(pages >= 2);
        }
        other => panic!("expected the CV not to fit, got {other:?}"),
    }
    let two = made(&cv, "en", Template::Clean, Length::Two)
        .await
        .unwrap_or_else(|r| panic!("{r:?}"));
    assert_eq!(two.pages, 2);
    assert!(two.checks.passed(), "{:?}", two.checks);
}

#[tokio::test]
async fn compact_holds_more_lines_to_a_page_than_clean() {
    // The first count of lines that Clean cannot hold on one page: Compact can.
    for count in (30..120).step_by(2) {
        let many: Vec<String> = (0..count)
            .map(|i| format!("Delivered project number {i} for a partner, on time and on budget"))
            .collect();
        let many: Vec<&str> = many.iter().map(String::as_str).collect();
        let cv = content(&many);
        if let Err(Refusal::DoesntFit { .. }) = made(&cv, "en", Template::Clean, Length::One).await
        {
            let compact = made(&cv, "en", Template::Compact, Length::One).await;
            assert!(compact.is_ok(), "{count} lines: {compact:?}");
            return;
        }
    }
    panic!("Clean held 119 lines on one page");
}

#[test]
fn a_heading_split_by_letter_spacing_is_not_found() {
    let prepared = data::prepare(&content(&BULLETS), &frame(), "en").expect("small");
    let text = "Alex Demo\nSummary\nEX PERIENCE\nSkills\nEducation\nLanguages\n";
    let checks = readback::check(
        &prepared.expect,
        &[],
        &Extracted {
            text: text.into(),
            pages: 1,
            link_targets: 3,
            images: 0,
        },
        1,
        1000,
    );
    assert_eq!(checks.sections.missing, ["Experience"]);
    assert!(!checks.passed());
    assert!(
        !checks.lines.missing.is_empty(),
        "the lines are not there either"
    );
    assert!(
        checks
            .reasons()
            .iter()
            .any(|r| r.contains("heading") && r.contains("Experience"))
    );
}

#[test]
fn a_line_survives_a_break_at_a_hyphen_and_a_ligature_but_not_a_loss() {
    let prepared = data::prepare(&content(&BULLETS), &frame(), "en").expect("small");
    let all: String = prepared
        .expect
        .lines
        .iter()
        .map(|l| format!("{}\n", l.text))
        .collect();
    let broken = all
        .replace("pipeline", "pipe-\nline")
        .replace("fi", "\u{fb01}");
    let checks = readback::check(
        &prepared.expect,
        &[],
        &Extracted {
            text: format!("Summary\nExperience\nSkills\nEducation\nLanguages\n{broken}"),
            pages: 1,
            link_targets: 3,
            images: 0,
        },
        1,
        1000,
    );
    assert!(
        checks.lines.missing.is_empty(),
        "{:?}",
        checks.lines.missing
    );
    let lost = readback::check(
        &prepared.expect,
        &[],
        &Extracted {
            text: format!(
                "Summary\nExperience\nSkills\nEducation\nLanguages\n{}",
                all.replace("Led a team of 4 engineers", "")
            ),
            pages: 1,
            link_targets: 3,
            images: 0,
        },
        1,
        1000,
    );
    assert_eq!(lost.lines.missing.len(), 1);
    assert_eq!(lost.lines.missing[0].text, "Led a team of 4 engineers");
}

#[tokio::test]
async fn the_keywords_the_review_showed_as_on_the_cv_must_survive() {
    let ok = make(
        &content(&BULLETS),
        &frame(),
        "en",
        Template::Clean,
        Length::One,
        vec!["Rust".into(), "billing".into()],
    )
    .await;
    let made = ok.unwrap_or_else(|r| panic!("{r:?}"));
    assert_eq!(made.checks.keywords.total, 2);
    assert_eq!(made.checks.keywords.found, 2);

    let lost = make(
        &content(&BULLETS),
        &frame(),
        "en",
        Template::Clean,
        Length::One,
        vec!["Kubernetes".into()],
    )
    .await;
    match lost {
        Err(Refusal::Checks(checks)) => {
            assert_eq!(checks.keywords.missing, ["Kubernetes"]);
            assert!(checks.reasons().iter().any(|r| r.contains("Kubernetes")));
        }
        other => panic!("expected the keyword to be missing, got {other:?}"),
    }
}
