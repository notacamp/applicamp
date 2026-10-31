//! The CV as the template is given it: plain strings in a JSON document,
//! and what the PDF must say, which the read-back looks for.
//!
//! Nothing the person wrote is ever put into Typst markup. The template is
//! fixed and embedded; this document is the only thing that varies, and the
//! template shows its strings as text.

use serde::Serialize;

use crate::features::cv::model::{Content, Frame};

/// The most the document may weigh: a CV is a page or two.
const MAX_JSON: usize = 256 * 1024;

/// The section names, by language.
#[derive(Debug, Clone, Copy, Serialize)]
pub struct Labels {
    pub summary: &'static str,
    pub experience: &'static str,
    pub skills: &'static str,
    pub education: &'static str,
    pub languages: &'static str,
}

/// The language a PDF is set in: `en`, `pt`, `es` or `fr`.
pub fn lang_of(tag: &str) -> &'static str {
    match tag
        .split(['-', '_'])
        .next()
        .unwrap_or_default()
        .to_lowercase()
        .as_str()
    {
        "pt" => "pt",
        "es" => "es",
        "fr" => "fr",
        _ => "en",
    }
}

/// The standard headings in `lang` (an ATS looks for exactly these).
pub fn labels(lang: &str) -> Labels {
    match lang {
        "pt" => Labels {
            summary: "Resumo",
            experience: "Experiência",
            skills: "Competências",
            education: "Formação",
            languages: "Línguas",
        },
        "es" => Labels {
            summary: "Resumen",
            experience: "Experiencia",
            skills: "Competencias",
            education: "Formación",
            languages: "Idiomas",
        },
        "fr" => Labels {
            summary: "Résumé",
            experience: "Expérience",
            skills: "Compétences",
            education: "Formation",
            languages: "Langues",
        },
        _ => Labels {
            summary: "Summary",
            experience: "Experience",
            skills: "Skills",
            education: "Education",
            languages: "Languages",
        },
    }
}

fn present(lang: &str) -> &'static str {
    match lang {
        "pt" | "es" => "Presente",
        "fr" => "Présent",
        _ => "Present",
    }
}

/// `text` as the PDF shows it: control characters and soft hyphens gone,
/// every run of white space one space.
pub fn clean(text: &str) -> String {
    text.chars()
        .filter(|c| {
            (!c.is_control() || c.is_whitespace())
                && !matches!(c, '\u{00ad}' | '\u{200b}' | '\u{feff}')
        })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn opt(text: Option<&str>) -> String {
    text.map(clean).unwrap_or_default()
}

/// `2021-03` as `03/2021`; anything else as it is.
fn month(date: &str) -> String {
    match date.split_once('-') {
        Some((year, month))
            if year.len() == 4
                && month.len() == 2
                && year.chars().all(|c| c.is_ascii_digit())
                && month.chars().all(|c| c.is_ascii_digit()) =>
        {
            format!("{month}/{year}")
        }
        _ => date.to_owned(),
    }
}

fn period(start: Option<&str>, end: Option<&str>, lang: &str) -> String {
    let start = opt(start);
    let end = opt(end);
    match (start.is_empty(), end.is_empty()) {
        (true, true) => String::new(),
        (false, true) => format!("{} – {}", month(&start), present(lang)),
        (true, false) => month(&end),
        (false, false) => format!("{} – {}", month(&start), month(&end)),
    }
}

/// A web address as text: no scheme, no trailing slash.
fn shown(url: &str) -> String {
    let rest = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))
        .unwrap_or(url);
    rest.trim_end_matches('/').to_owned()
}

/// `url` when it is a web address the PDF may link to.
fn web(url: &str) -> Option<String> {
    let url = clean(url);
    let lower = url.to_lowercase();
    ((lower.starts_with("https://") || lower.starts_with("http://")) && !url.contains(' '))
        .then_some(url)
}

#[derive(Serialize)]
struct ContactItem {
    text: String,
    url: String,
}

#[derive(Serialize)]
struct RoleOut {
    title: String,
    org: String,
    place: String,
    when: String,
    bullets: Vec<String>,
}

#[derive(Serialize)]
struct SchoolOut {
    title: String,
    org: String,
    when: String,
}

#[derive(Serialize)]
struct Document {
    lang: &'static str,
    labels: Labels,
    name: String,
    headline: String,
    contact: Vec<ContactItem>,
    summary: String,
    roles: Vec<RoleOut>,
    skills: Vec<String>,
    education: Vec<SchoolOut>,
    languages: Vec<String>,
}

/// One piece of text the PDF must give back.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Expected {
    /// The line it is (its id), or what it is (`role:Senior Engineer`).
    pub id: String,
    pub text: String,
}

/// What a PDF made from a document must say.
#[derive(Debug, Clone, Default)]
pub struct Expect {
    /// The section headings the CV has.
    pub headings: Vec<String>,
    pub name: Option<String>,
    pub email: Option<String>,
    /// The links as they are shown (`github.com/alex-demo`).
    pub links: Vec<String>,
    /// How many places the PDF links to (an email counts).
    pub link_targets: usize,
    /// Every line of the CV, and the heads of its roles and schools.
    pub lines: Vec<Expected>,
}

/// The document for a template, and what it must say.
#[derive(Debug, Clone)]
pub struct Prepared {
    pub json: String,
    pub expect: Expect,
}

/// Why a document could not be made.
#[derive(Debug, PartialEq, Eq)]
pub struct TooLarge;

/// The CV `content` (the positions it heads from `frame`), set in `language`.
pub fn prepare(content: &Content, frame: &Frame, language: &str) -> Result<Prepared, TooLarge> {
    let lang = lang_of(language);
    let labels = labels(lang);
    let mut expect = Expect::default();
    let contact = &content.contact;

    let name = opt(contact.name.as_deref());
    let headline = content
        .headline
        .as_ref()
        .map(|l| clean(&l.text))
        .unwrap_or_default();
    if !name.is_empty() {
        expect.name = Some(name.clone());
    }
    if !headline.is_empty()
        && let Some(line) = &content.headline
    {
        expect.lines.push(Expected {
            id: line.id.clone(),
            text: headline.clone(),
        });
    }

    let mut items: Vec<ContactItem> = Vec::new();
    let email = opt(contact.email.as_deref());
    if !email.is_empty() && !email.contains(' ') {
        expect.email = Some(email.clone());
        expect.link_targets += 1;
        items.push(ContactItem {
            url: format!("mailto:{email}"),
            text: email,
        });
    }
    let place = opt(contact.location.as_deref());
    if !place.is_empty() {
        items.push(ContactItem {
            text: place,
            url: String::new(),
        });
    }
    let addresses = contact
        .url
        .iter()
        .chain(contact.links.iter().map(|(_, url)| url));
    for address in addresses {
        if let Some(url) = web(address) {
            let text = shown(&url);
            expect.links.push(text.clone());
            expect.link_targets += 1;
            items.push(ContactItem { text, url });
        }
    }

    let summary = content
        .summary
        .as_ref()
        .map(|l| clean(&l.text))
        .unwrap_or_default();
    if !summary.is_empty()
        && let Some(line) = &content.summary
    {
        expect.headings.push(labels.summary.to_owned());
        expect.lines.push(Expected {
            id: line.id.clone(),
            text: summary.clone(),
        });
    }

    let mut roles = Vec::new();
    for role in &content.experience {
        let Some(info) = frame.position(role.position) else {
            continue;
        };
        let title = opt(info.title.as_deref());
        let org = opt(info.company.as_deref());
        let bullets: Vec<(String, String)> = role
            .bullets
            .iter()
            .map(|b| (b.id.clone(), clean(&b.text)))
            .filter(|(_, text)| !text.is_empty())
            .collect();
        if title.is_empty() && org.is_empty() && bullets.is_empty() {
            continue;
        }
        for (what, text) in [("title", &title), ("org", &org)] {
            if !text.is_empty() {
                expect.lines.push(Expected {
                    id: format!("role{}.{what}", role.position),
                    text: text.clone(),
                });
            }
        }
        for (id, text) in &bullets {
            expect.lines.push(Expected {
                id: id.clone(),
                text: text.clone(),
            });
        }
        roles.push(RoleOut {
            title,
            org,
            place: opt(info.location.as_deref()),
            when: period(info.start_date.as_deref(), info.end_date.as_deref(), lang),
            bullets: bullets.into_iter().map(|(_, text)| text).collect(),
        });
    }
    if !roles.is_empty() {
        expect.headings.push(labels.experience.to_owned());
    }

    let skills: Vec<String> = content
        .skills
        .iter()
        .map(|s| clean(&s.name))
        .filter(|s| !s.is_empty())
        .collect();
    if !skills.is_empty() {
        expect.headings.push(labels.skills.to_owned());
        for (item, name) in content.skills.iter().zip(&skills) {
            expect.lines.push(Expected {
                id: item.id.clone(),
                text: name.clone(),
            });
        }
    }

    let mut education = Vec::new();
    for (i, school) in content.education.iter().enumerate() {
        let org = clean(&school.institution);
        if org.is_empty() {
            continue;
        }
        let title = [school.study_type.as_deref(), school.area.as_deref()]
            .into_iter()
            .flatten()
            .map(clean)
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join(", ");
        expect.lines.push(Expected {
            id: format!("school{i}"),
            text: org.clone(),
        });
        education.push(SchoolOut {
            title,
            org,
            when: period(
                school.start_date.as_deref(),
                school.end_date.as_deref(),
                lang,
            ),
        });
    }
    if !education.is_empty() {
        expect.headings.push(labels.education.to_owned());
    }

    let languages: Vec<String> = content
        .languages
        .iter()
        .map(|l| match l.fluency.as_deref().map(clean) {
            Some(fluency) if !fluency.is_empty() => format!("{} ({fluency})", clean(&l.language)),
            _ => clean(&l.language),
        })
        .filter(|s| !s.is_empty())
        .collect();
    if !languages.is_empty() {
        expect.headings.push(labels.languages.to_owned());
        for (i, line) in languages.iter().enumerate() {
            expect.lines.push(Expected {
                id: format!("language{i}"),
                text: line.clone(),
            });
        }
    }

    let document = Document {
        lang,
        labels,
        name,
        headline,
        contact: items,
        summary,
        roles,
        skills,
        education,
        languages,
    };
    let json = serde_json::to_string(&document).map_err(|_| TooLarge)?;
    if json.len() > MAX_JSON {
        return Err(TooLarge);
    }
    Ok(Prepared { json, expect })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn white_space_and_control_characters_are_squeezed_out() {
        assert_eq!(clean("  a\n\tb \u{0007}c\u{00ad} "), "a b c");
    }

    #[test]
    fn dates_are_months_and_an_open_end_is_present_in_the_cv_language() {
        assert_eq!(period(Some("2021-03"), None, "pt"), "03/2021 – Presente");
        assert_eq!(
            period(Some("2017-06"), Some("2021-02"), "en"),
            "06/2017 – 02/2021"
        );
        assert_eq!(period(Some("2015"), Some("2017"), "fr"), "2015 – 2017");
        assert_eq!(period(None, None, "en"), "");
    }

    #[test]
    fn only_web_addresses_become_links() {
        assert_eq!(web("javascript:alert(1)"), None);
        assert_eq!(web("https://a b"), None);
        assert_eq!(
            web("https://github.com/alex-demo/").as_deref(),
            Some("https://github.com/alex-demo/")
        );
        assert_eq!(
            shown("https://github.com/alex-demo/"),
            "github.com/alex-demo"
        );
    }

    #[test]
    fn the_headings_are_the_standard_ones_in_each_language() {
        assert_eq!(labels(lang_of("pt-PT")).experience, "Experiência");
        assert_eq!(labels(lang_of("es-ES")).languages, "Idiomas");
        assert_eq!(labels(lang_of("fr")).summary, "Résumé");
        assert_eq!(labels(lang_of("de")).skills, "Skills");
    }
}
