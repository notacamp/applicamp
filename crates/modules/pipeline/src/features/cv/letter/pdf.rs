//! The cover letter as a PDF: the CV's Typst stack (the compiler's file-less
//! world, the embedded fonts, the bounds and the read-back) with a letter
//! template in the same style, on one page.
//!
//! The letter is data, never markup: the paragraphs go to the fixed, embedded
//! template as JSON in `sys.inputs` and are shown as text. The read-back
//! checks that the name, the email, the greeting and every paragraph came out
//! of the PDF as text, and that it is one page. The answers to the job's
//! questions are not in it.

use serde::Serialize;

use super::{
    super::pdf::{
        self, Length, Made, Refusal,
        data::{Expect, Expected, Prepared, TooLarge, clean, lang_of, shown, web},
    },
    model::Content,
};

/// The document may not weigh more than this: a letter is a page.
const MAX_JSON: usize = 64 * 1024;

const TEMPLATE: &str = include_str!("letter.typ");

/// The greeting line, in `lang`, for the company.
pub fn greeting(lang: &str, company: &str) -> String {
    let company = clean(company);
    match (lang, company.is_empty()) {
        ("pt", false) => format!("Caros responsáveis pelo recrutamento na {company},"),
        ("pt", true) => "Caros responsáveis pelo recrutamento,".to_owned(),
        ("es", false) => format!("Estimado equipo de selección de {company},"),
        ("es", true) => "Estimado equipo de selección,".to_owned(),
        ("fr", false) => format!("Bonjour à l'équipe de recrutement de {company},"),
        ("fr", true) => "Bonjour à l'équipe de recrutement,".to_owned(),
        (_, false) => format!("Dear hiring team at {company},"),
        (_, true) => "Dear hiring team,".to_owned(),
    }
}

/// The sign-off, in `lang`.
pub fn closing(lang: &str) -> &'static str {
    match lang {
        "pt" => "Com os melhores cumprimentos,",
        "es" => "Atentamente,",
        "fr" => "Cordialement,",
        _ => "Sincerely,",
    }
}

fn title(lang: &str) -> &'static str {
    match lang {
        "pt" => "Carta de apresentação",
        "es" => "Carta de presentación",
        "fr" => "Lettre de motivation",
        _ => "Cover letter",
    }
}

#[derive(Serialize)]
struct ContactItem {
    text: String,
    url: String,
}

#[derive(Serialize)]
struct Document {
    lang: &'static str,
    title: &'static str,
    name: String,
    contact: Vec<ContactItem>,
    greeting: String,
    paragraphs: Vec<String>,
    closing: &'static str,
}

/// The document for the template, and what the PDF must say. `content`'s
/// contact and paragraphs, set in `language`, addressed to `company`.
pub fn prepare(content: &Content, company: &str, language: &str) -> Result<Prepared, TooLarge> {
    let lang = lang_of(language);
    let mut expect = Expect::default();
    let contact = &content.contact;
    let opt = |text: Option<&str>| text.map(clean).unwrap_or_default();

    let name = opt(contact.name.as_deref());
    if !name.is_empty() {
        expect.name = Some(name.clone());
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
    for address in contact
        .url
        .iter()
        .chain(contact.links.iter().map(|(_, url)| url))
    {
        if let Some(url) = web(address) {
            let text = shown(&url);
            expect.links.push(text.clone());
            expect.link_targets += 1;
            items.push(ContactItem { text, url });
        }
    }

    let greeting = greeting(lang, company);
    expect.lines.push(Expected {
        id: "greeting".to_owned(),
        text: greeting.clone(),
    });
    let mut paragraphs = Vec::new();
    for paragraph in &content.paragraphs {
        let text = clean(&paragraph.text());
        if text.is_empty() {
            continue;
        }
        expect.lines.push(Expected {
            id: paragraph.id.clone(),
            text: text.clone(),
        });
        paragraphs.push(text);
    }
    let closing = closing(lang);
    expect.lines.push(Expected {
        id: "closing".to_owned(),
        text: closing.to_owned(),
    });

    let document = Document {
        lang,
        title: title(lang),
        name,
        contact: items,
        greeting,
        paragraphs,
        closing,
    };
    let json = serde_json::to_string(&document).map_err(|_| TooLarge)?;
    if json.len() > MAX_JSON {
        return Err(TooLarge);
    }
    Ok(Prepared { json, expect })
}

/// The letter as a one-page PDF that has been read back and found to say what
/// the letter says.
pub async fn make(content: &Content, company: &str, language: &str) -> Result<Made, Refusal> {
    let prepared = prepare(content, company, language).map_err(|_| Refusal::Unavailable)?;
    pdf::make_prepared(TEMPLATE, prepared, Length::One, Vec::new()).await
}
